#!/usr/bin/env python3
"""Restore the personal app/profile from a Git checkout. Never restarts a core."""
import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import plistlib
import re
import shutil
import socket
import subprocess
import tarfile
import tempfile
import uuid

ROOT = Path(__file__).resolve().parent.parent
MAX_EXPANDED = 300*1024*1024
SETTINGS = {'font','ui_font','font_size','zoom','minimum_contrast','sounds','theme'}
SHARED = {'layout','workspace.panels','frame.welcomed','sidebar.width','sidebar.collapsed'}
PROFILE_JSON = {'settings.json','keys.json','shared.json','browser.json',
                'layouts/personal-main.json','layouts/.state/personal-main.json'}


def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_profile(root, manifest):
    expected = manifest['profile_files']
    actual = {str(p.relative_to(root)) for p in root.rglob('*') if p.is_file()}
    if actual != set(expected): raise RuntimeError('Profile inventory changed; rebuild the package manifest')
    for name, digest in expected.items():
        path = PurePosixPath(name)
        allowed = name in PROFILE_JSON or (len(path.parts)==2 and path.parts[0]=='fonts' and path.suffix in ['.ttf','.otf','.txt'])
        if not allowed or path.is_absolute() or '..' in path.parts or (root/name).is_symlink():
            raise RuntimeError('Unexpected profile file: '+name)
        if sha(root/name) != digest: raise RuntimeError('Profile checksum mismatch: '+name)
        if path.suffix=='.json':
            data = json.loads((root/name).read_text())
            if name=='settings.json' and set(data)-SETTINGS: raise RuntimeError('Machine-specific settings are not portable')
            if name=='shared.json' and set(data)-SHARED: raise RuntimeError('Session state is not portable')
            text=json.dumps(data)
            if re.search(r'/Users/|/home/|BEGIN (?:RSA |OPENSSH )?PRIVATE KEY|(?:sk-proj-|github_pat_|ghp_)[A-Za-z0-9_]{10,}',text):
                raise RuntimeError('Local paths or credentials found in portable profile: '+name)
    if not PROFILE_JSON.issubset(expected): raise RuntimeError('Incomplete portable profile')


def unpack(archive, destination):
    # Do not let a bad archive follow links, write outside the staging directory,
    # or create devices. This works on the system Python 3.9 too.
    with tarfile.open(archive,'r:gz') as source:
        members=source.getmembers()
        if sum(m.size for m in members)>MAX_EXPANDED: raise RuntimeError('Package is too large')
        for member in members:
            path=PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or '\\' in member.name or not path.parts or path.parts[0]!='Herdr Preview.app' or not (member.isfile() or member.isdir()):
                raise RuntimeError('Unsafe package entry')
            target=destination.joinpath(*path.parts)
            if member.isdir(): target.mkdir(parents=True,exist_ok=True)
            else:
                target.parent.mkdir(parents=True,exist_ok=True)
                with source.extractfile(member) as data, target.open('xb') as output: shutil.copyfileobj(data,output)
                target.chmod(0o755 if member.mode & 0o111 else 0o644)


def core_version(path):
    try:
        result=subprocess.run([str(path),'--version'],capture_output=True,text=True,timeout=5)
        match=re.fullmatch(r'hc (\d+)\.(\d+)\.(\d+)',result.stdout.strip())
        if result.returncode==0 and match:return tuple(map(int,match.groups()))
    except (OSError,subprocess.TimeoutExpired): pass


def find_core(home, requested=None):
    candidates=[requested] if requested else []
    for config in [home/'.config/herdr-preview/settings.json',home/'.config/herdr-app/settings.json']:
        try:
            value=json.loads(config.read_text()).get('core')
            if isinstance(value,str): candidates.append(Path(value.replace('~/',str(home)+'/',1)))
        except (OSError,ValueError):pass
    candidates += [home/'.local/bin/herdr-core',home/'.local/bin/hc']
    candidates += [Path(p) for name in ['herdr-core','hc'] if (p:=shutil.which(name))]
    for path in candidates:
        if path and (version:=core_version(path)):
            if version[:2] == (0,1) and version >= (0,1,11):return path.resolve(),version
            if requested:raise RuntimeError('This package requires generation 1 herdr-core >= 0.1.11; no core was changed')
    raise RuntimeError('Install the official Herdr app/core first, or pass --core /path/to/herdr-core (generation 1, >= 0.1.11)')


def active(config):
    try:
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(1);client.connect(str(config/'herdr-app.sock'))
        return True
    except OSError:return False


def write_json(path, value):
    path.parent.mkdir(parents=True,exist_ok=True)
    temporary=path.with_name('.'+path.name+'-'+uuid.uuid4().hex[:8])
    temporary.write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n')
    temporary.replace(path)


def snapshot(config, destination):
    if not config.exists():return
    def regular(member):return member if member.isfile() or member.isdir() or member.issym() or member.islnk() else None
    with tarfile.open(destination,'w:gz',compresslevel=1) as target:target.add(config,arcname='herdr-preview',filter=regular)


def install(root, home, core=None, dry_run=False, native=True, register=True):
    manifest=json.loads((root/'packages/personal-release.json').read_text())
    if manifest.get('schema')!=1 or manifest.get('platform')!='macos-arm64' or not manifest.get('native_ui_checks_passed'):
        raise RuntimeError('Unvalidated or unsupported personal package')
    archive=root/'packages'/manifest['archive']
    if Path(manifest['archive']).name!=manifest['archive'] or archive.stat().st_size!=manifest['bytes'] or sha(archive)!=manifest['sha256']:
        raise RuntimeError('App package checksum/size mismatch')
    profile=root/'personal-profile';validate_profile(profile,manifest)
    config=home/'.config/herdr-preview';app=home/'Applications/Herdr Preview.app'
    cli=home/'.local/bin/herdr-preview'
    def redirected(target):
        while target != home:
            if target.is_symlink():return True
            target=target.parent
        return False
    if any(redirected(p) for p in [config,app,cli]):raise RuntimeError('Refusing symlinked install destinations')
    if active(config):raise RuntimeError('Quit only Herdr Preview before restoring settings. Keep herdr-core running; your terminals remain available.')
    if config.is_symlink() or app.is_symlink():raise RuntimeError('Refusing symlinked app/config destinations')
    if app.exists() and plistlib.loads((app/'Contents/Info.plist').read_bytes()).get('CFBundleIdentifier')!='dev.herdr.preview':
        raise RuntimeError('Refusing to replace an unrelated app')
    if cli.exists() and (cli.is_symlink() or 'HerdrPreview' not in cli.read_text()):raise RuntimeError('Unrelated herdr-preview CLI exists')
    core,version=find_core(home,core)
    result={'config_home':str(config),'app':str(app),'core_version':'.'.join(map(str,version)),'source_head':manifest['source_head']}
    if dry_run:return {**result,'dry_run':True,'changed':False}
    # The old config may contain private values. Keep its archive private and
    # never copy it into the repository or overwrite WebKit/core session stores.
    backup=home/'Library/Application Support/Herdr Personal/backups'/(datetime.now(timezone.utc).strftime('%Y%m%dT%H%M%SZ')+'-'+uuid.uuid4().hex[:6])
    backup.mkdir(parents=True,mode=0o700);backup.chmod(0o700)
    app.parent.mkdir(parents=True,exist_ok=True)
    with (app.parent/'.herdr-preview-install.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        if active(config):raise RuntimeError('Herdr Preview opened during preparation; close it and retry')
        # Reject redirected paths before making any changes inside a live config.
        for name in manifest['profile_files']:
            target=config/name
            if redirected(target):raise RuntimeError('Refusing symlinked profile destination: '+name)
        snapshot(config,backup/'previous-settings.tar.gz')
        if cli.exists():shutil.copy2(cli,backup/'previous-herdr-preview-cli')
        with tempfile.TemporaryDirectory(prefix='.HerdrPersonal-stage-',dir=app.parent) as temporary:
            stage=Path(temporary);unpack(archive,stage);candidate=stage/app.name
            binary=candidate/'Contents/MacOS/HerdrPreview'
            if native:
                subprocess.run(['codesign','--verify','--deep','--strict',str(candidate)],check=True)
                info=json.loads(subprocess.check_output([str(binary),'--personal-build-info']))
                if not info.get('protected') or info.get('bundle_id')!='dev.herdr.preview':raise RuntimeError('Package identity mismatch')
                policy=json.loads((root/'personal-build.json').read_text())
                names={c['name'] for c in json.loads(subprocess.check_output([str(binary),'commands']))}
                if set(policy['required_commands'])-names:raise RuntimeError('Personal Browser/Preview commands missing')
            plist=plistlib.loads((candidate/'Contents/Info.plist').read_bytes())
            plist['LSEnvironment']={'HERDR_APP_HOME':str(config),'HERDR_APP_ID':'herdr-preview','HERDR_PREVIEW_TEST_CONTROL':'0',
                'PATH':str(home/'.local/bin')+':/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin'}
            (candidate/'Contents/Info.plist').write_bytes(plistlib.dumps(plist))
            if native:
                subprocess.run(['codesign','--force','--sign','-',str(candidate)],check=True)
                subprocess.run(['codesign','--verify','--deep','--strict',str(candidate)],check=True)
            config.mkdir(parents=True,exist_ok=True)
            previous=backup/app.name
            had_config=(backup/'previous-settings.tar.gz').exists()
            installed_app=False
            wrapper=None
            try:
                for name in manifest['profile_files']:
                    source,target=profile/name,config/name
                    target.parent.mkdir(parents=True,exist_ok=True)
                    if name in ['settings.json','shared.json']:
                        original=json.loads(target.read_text()) if target.exists() else {}
                        original.update(json.loads(source.read_text()))
                        if name=='settings.json':original['core']=str(core)
                        write_json(target,original)
                    else:
                        temporary_file=target.with_name('.'+target.name+'-'+uuid.uuid4().hex[:8])
                        shutil.copy2(source,temporary_file);temporary_file.replace(target)
                if app.exists():app.rename(previous)
                candidate.rename(app)
                installed_app=True
                cli.parent.mkdir(parents=True,exist_ok=True)
                wrapper=cli.with_name('.herdr-preview-'+uuid.uuid4().hex[:8])
                # No original-machine paths; resolve this user's home at invocation.
                wrapper.write_text('#!/bin/sh\nexport HERDR_APP_HOME="$HOME/.config/herdr-preview"\nexport HERDR_APP_ID=herdr-preview\nexport HERDR_PREVIEW_TEST_CONTROL=0\nexec "$HOME/Applications/Herdr Preview.app/Contents/MacOS/HerdrPreview" "$@"\n')
                wrapper.chmod(0o755);wrapper.replace(cli)
            except BaseException:
                # Restoring settings is explicit rollback while the personal GUI
                # is closed. Core state and the WebKit profile are never touched.
                if installed_app and app.exists():shutil.rmtree(app)
                if previous.exists():previous.rename(app)
                shutil.rmtree(config)
                if had_config:
                    with tarfile.open(backup/'previous-settings.tar.gz') as saved:saved.extractall(config.parent)
                if (backup/'previous-herdr-preview-cli').exists():shutil.copy2(backup/'previous-herdr-preview-cli',cli)
                elif cli.exists():cli.unlink()
                if wrapper and wrapper.exists():wrapper.unlink()
                raise
    write_json(backup/'restore-record.json',{**result,'previous_bundle_retained':(backup/app.name).exists()})
    if native and register:
        subprocess.run(['/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister','-f',str(app)],check=True)
    return {**result,'backup':str(backup),'installed':True}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--home',type=Path,default=Path.home(),help='Isolated install home for verification')
    parser.add_argument('--core',type=Path)
    parser.add_argument('--dry-run',action='store_true')
    args=parser.parse_args()
    if platform.system()!='Darwin' or platform.machine()!='arm64':parser.error('Apple Silicon macOS is required')
    if int(platform.mac_ver()[0].split('.')[0])<14:parser.error('macOS 14 or newer is required')
    result=install(ROOT,args.home.resolve(),args.core,args.dry_run)
    print(json.dumps(result,ensure_ascii=False,indent=2))
    if not args.dry_run: print('Installed. Open ~/Applications/Herdr Preview.app. Log in to websites again on this Mac.')


if __name__=='__main__':main()
