#!/usr/bin/env python3
"""Disposable installs: preserve other apps, core sessions, and browser profiles."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import plistlib
import socket
import tarfile
import tempfile
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('restore',Path(__file__).resolve().parent.parent/'distribution/restore-personal.py')
restore=importlib.util.module_from_spec(spec);spec.loader.exec_module(restore)


class Restore(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='hpr-',dir='/private/tmp');self.addCleanup(self.temp.cleanup)
        self.base=Path(self.temp.name);self.root=self.base/'repo';self.home=self.base/'home'
        self.root.mkdir();self.home.mkdir();(self.root/'packages').mkdir()
        profile=self.root/'personal-profile'
        for name in restore.PROFILE_JSON:
            p=profile/name;p.parent.mkdir(parents=True,exist_ok=True)
            p.write_text(json.dumps({'font':'D2Coding'} if name=='settings.json' else {'layout':'personal-main'} if name=='shared.json' else {}))
        self.core=self.home/'.local/bin/herdr-core';self.core.parent.mkdir(parents=True)
        self.core.write_text('#!/bin/sh\necho "hc 0.1.16"\n');self.core.chmod(0o755)
        self.package=self.root/'packages/test.tar.gz'
        with tarfile.open(self.package,'w:gz') as output:
            for name,data in [('Contents/Info.plist',plistlib.dumps({'CFBundleIdentifier':'dev.herdr.preview'})),('Contents/MacOS/HerdrPreview',b'test binary')]:
                member=tarfile.TarInfo('Herdr Preview.app/'+name);member.size=len(data);output.addfile(member,io.BytesIO(data))
        self.manifest={'schema':1,'platform':'macos-arm64','native_ui_checks_passed':True,'archive':self.package.name,'bytes':self.package.stat().st_size,'sha256':restore.sha(self.package),'source_head':'validated-test',
            'profile_files':{str(p.relative_to(profile)):restore.sha(p) for p in profile.rglob('*') if p.is_file()}}
        self.save_manifest()

    def save_manifest(self):
        (self.root/'packages/personal-release.json').write_text(json.dumps(self.manifest))

    def install(self,**kwargs):return restore.install(self.root,self.home,native=False,**kwargs)

    def test_restore_keeps_other_app_core_cookies_and_unrelated_settings(self):
        config=self.home/'.config/herdr-preview';config.mkdir(parents=True)
        (config/'settings.json').write_text('{"theme":"moss","font":"old"}')
        (config/'private-data').write_text('keep local data')
        sentinels=['Applications/Herdr.app/keep','.local/state/herdr-core/default/docs.json','Library/WebKit/dev.herdr.preview/cookie','Applications/Herdr Preview.app/keep']
        for name in sentinels:
            p=self.home/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text('untouched')
        info=self.home/'Applications/Herdr Preview.app/Contents/Info.plist';info.parent.mkdir();info.write_bytes(plistlib.dumps({'CFBundleIdentifier':'dev.herdr.preview'}))
        before_core=self.core.read_bytes();result=self.install()
        settings=json.loads((config/'settings.json').read_text())
        self.assertEqual(settings['theme'],'moss');self.assertEqual(settings['font'],'D2Coding')
        self.assertEqual(settings['core'],str(self.core))
        self.assertEqual(self.core.read_bytes(),before_core)
        for name in sentinels[:-1]:self.assertEqual((self.home/name).read_text(),'untouched')
        self.assertEqual((Path(result['backup'])/'Herdr Preview.app/keep').read_text(),'untouched')
        self.assertEqual((config/'private-data').read_text(),'keep local data')
        self.assertTrue((Path(result['backup'])/'previous-settings.tar.gz').is_file())
        self.assertIn('$HOME',(self.home/'.local/bin/herdr-preview').read_text())

    def test_corrupt_app_and_profile_refused_before_mutation(self):
        self.package.write_bytes(b'corrupt')
        with self.assertRaisesRegex(RuntimeError,'checksum'):self.install()
        self.assertFalse((self.home/'Applications').exists())
        self.manifest['bytes']=self.package.stat().st_size;self.manifest['sha256']=restore.sha(self.package);self.save_manifest()
        (self.root/'personal-profile/settings.json').write_text('{}')
        with self.assertRaisesRegex(RuntimeError,'checksum'):self.install()

    def test_portable_profile_rejects_private_paths_and_extra_files(self):
        p=self.root/'personal-profile/settings.json';p.write_text('{"core":"/Users/someone/secret"}')
        self.manifest['profile_files']['settings.json']=restore.sha(p);self.save_manifest()
        with self.assertRaisesRegex(RuntimeError,'Machine-specific'):self.install()
        p.write_text('{"font":"/Users/someone/private"}');self.manifest['profile_files']['settings.json']=restore.sha(p);self.save_manifest()
        with self.assertRaisesRegex(RuntimeError,'Local paths'):self.install()
        (self.root/'personal-profile/cookies').write_text('never publish')
        with self.assertRaisesRegex(RuntimeError,'inventory'):self.install()

    def test_active_personal_gui_refuses_restore_without_stopping_it(self):
        config=self.home/'.config/herdr-preview';config.mkdir(parents=True)
        with socket.socket(socket.AF_UNIX) as server:
            server.bind(str(config/'herdr-app.sock'));server.listen()
            with self.assertRaisesRegex(RuntimeError,'Quit only Herdr Preview'):self.install()
            self.assertFalse((self.home/'Applications').exists())

    def test_archive_traversal_and_links_are_refused(self):
        for name,kind in [('../outside',tarfile.REGTYPE),('Herdr Preview.app/link',tarfile.SYMTYPE),('/absolute',tarfile.REGTYPE)]:
            with tarfile.open(self.package,'w:gz') as target:
                m=tarfile.TarInfo(name);m.type=kind;m.linkname='/outside';target.addfile(m)
            dest=self.base/'unpack';dest.mkdir(exist_ok=True)
            with self.assertRaisesRegex(RuntimeError,'Unsafe package'):restore.unpack(self.package,dest)

    def test_restore_failure_rolls_back_settings_and_app(self):
        config=self.home/'.config/herdr-preview';config.mkdir(parents=True)
        (config/'settings.json').write_text('{"font":"original"}')
        with mock.patch.object(restore.shutil,'copy2',side_effect=OSError('simulated write failure')):
            with self.assertRaisesRegex(OSError,'simulated'):self.install()
        self.assertEqual(json.loads((config/'settings.json').read_text()),{'font':'original'})
        self.assertFalse((self.home/'Applications/Herdr Preview.app').exists())

    def test_symlink_destination_does_not_write_outside_config(self):
        config=self.home/'.config/herdr-preview';config.mkdir(parents=True)
        outside=self.base/'outside';outside.mkdir();(outside/'keep').write_text('untouched')
        (config/'layouts').symlink_to(outside,target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError,'symlinked profile destination'):self.install()
        self.assertEqual((outside/'keep').read_text(),'untouched')
        self.assertFalse((outside/'personal-main.json').exists())

    def test_failure_after_app_swap_restores_previous_app_and_cli(self):
        app=self.home/'Applications/Herdr Preview.app';(app/'Contents').mkdir(parents=True)
        (app/'Contents/Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'dev.herdr.preview'}))
        (app/'keep').write_text('previous app')
        cli=self.home/'.local/bin/herdr-preview';cli.write_text('previous HerdrPreview CLI')
        original=Path.replace
        def failing_replace(path,target):
            if Path(target)==cli:raise OSError('simulated final CLI write failure')
            return original(path,target)
        with mock.patch.object(Path,'replace',failing_replace):
            with self.assertRaisesRegex(OSError,'simulated final CLI'):self.install()
        self.assertEqual((app/'keep').read_text(),'previous app')
        self.assertEqual(cli.read_text(),'previous HerdrPreview CLI')
        self.assertFalse((self.home/'.config/herdr-preview').exists())
        self.assertFalse(list(cli.parent.glob('.herdr-preview-*')))


if __name__=='__main__':unittest.main()
