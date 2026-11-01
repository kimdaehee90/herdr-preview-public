#!/usr/bin/env python3
"""Create the public package only from a path-remapped, validated native build."""
import gzip
import hashlib
import importlib.util
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import tarfile
import tempfile

ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('privacy',Path(__file__).with_name('audit-public.py'))
privacy=importlib.util.module_from_spec(spec);spec.loader.exec_module(privacy)


def main():
    proof=json.loads((ROOT/'.public-build/validated.json').read_text());source=ROOT/'target/release/herdr-app'
    if not proof.get('native_ui_checks_passed') or not proof.get('paths_remapped') or hashlib.sha256(source.read_bytes()).hexdigest()!=proof['binary_sha256']:
        raise RuntimeError('A path-remapped, native-tested binary is required')
    privacy.scan(source.read_bytes(),'release executable',privacy.private_terms())
    info=json.loads(subprocess.check_output([str(source),'--personal-build-info']))
    if not info.get('protected') or info['bundle_id']!='dev.herdr.preview':raise RuntimeError('Unexpected build identity')
    if subprocess.check_output(['lipo','-archs',str(source)],text=True).strip()!='arm64':raise RuntimeError('Apple Silicon required')
    out=ROOT/'packages';out.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='public-package-') as temporary:
        app=Path(temporary)/'Herdr Preview.app';macos=app/'Contents/MacOS';resources=app/'Contents/Resources'
        macos.mkdir(parents=True);resources.mkdir();shutil.copy2(source,macos/'HerdrPreview')
        shutil.copy2(ROOT/'assets/icons/Herdr.icns',resources/'Herdr.icns')
        for name in ['LICENSE','THIRD-PARTY-NOTICES']:shutil.copy2(ROOT/name,resources/name)
        record={'schema':1,'source_head':proof['head'],'identity':info,'portable':True,'native_ui_checks_passed':True,'paths_remapped':True}
        (resources/'PersonalBuild.json').write_text(json.dumps(record,indent=2)+'\n')
        plist={'CFBundleExecutable':'HerdrPreview','CFBundleIdentifier':'dev.herdr.preview','CFBundleName':'Herdr Preview','CFBundleDisplayName':'Herdr Preview','CFBundleIconFile':'Herdr','CFBundlePackageType':'APPL','CFBundleShortVersionString':info['version'],'CFBundleVersion':info['version'],'NSPrincipalClass':'NSApplication','NSHighResolutionCapable':True,'NSAppTransportSecurity':{'NSAllowsLocalNetworking':True,'NSAllowsArbitraryLoadsInWebContent':True},'LSEnvironment':{'HERDR_APP_ID':'herdr-preview','HERDR_PREVIEW_TEST_CONTROL':'0'}}
        (app/'Contents/Info.plist').write_bytes(plistlib.dumps(plist))
        subprocess.run(['codesign','--force','--sign','-',str(app)],check=True)
        subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True)
        def normalize(item):
            if not (item.isfile() or item.isdir()):raise RuntimeError('Unexpected package link or special file')
            item.uid=item.gid=0;item.uname=item.gname='';item.mtime=0;item.pax_headers={}
            return item
        archive=out/'herdr-preview-macos-arm64.tar.gz'
        with archive.open('wb') as output, gzip.GzipFile(filename='',mode='wb',fileobj=output,mtime=0) as compressed:
            with tarfile.open(fileobj=compressed,mode='w',format=tarfile.USTAR_FORMAT) as target:target.add(app,arcname=app.name,filter=normalize)
    privacy.archive(archive.read_bytes(),archive.name,privacy.private_terms())
    if archive.stat().st_size>=90*1024*1024:raise RuntimeError('Package is too large')
    manifest={'schema':1,'platform':'macos-arm64','minimum_macos':14,'source_head':proof['head'],'native_ui_checks_passed':True,'paths_remapped':True,'archive_metadata_normalized':True,'core_min':'0.1.11','core_generation':1,'archive':archive.name,'bytes':archive.stat().st_size,'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'profile_files':{str(p.relative_to(ROOT/'personal-profile')):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((ROOT/'personal-profile').rglob('*')) if p.is_file()}}
    (out/'personal-release.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(json.dumps({'package_bytes':manifest['bytes'],'sha256':manifest['sha256'],'public_source':proof['head']}))


if __name__=='__main__':main()
