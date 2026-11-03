#!/usr/bin/env python3
"""Native temporary keychains only; never uses the person's signing identity."""
import importlib.util
import json
import os
from pathlib import Path
import platform
import plistlib
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('signing',Path(__file__).resolve().parent.parent/'distribution/macos-signing.py')
signing=importlib.util.module_from_spec(spec);spec.loader.exec_module(signing)

@unittest.skipUnless(platform.system()=='Darwin','macOS code signing only')
class NativeSigning(unittest.TestCase):
    def check_fresh_and_reused(self, system_tools=False):
        with tempfile.TemporaryDirectory(prefix='hsg-',dir='/private/tmp') as temp:
            home=Path(temp)
            app=home/'Test.app';binary=app/'Contents/MacOS/Test';binary.parent.mkdir(parents=True)
            shutil.copyfile('/usr/bin/true',binary);binary.chmod(0o755)
            (app/'Contents/Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'dev.herdr.preview','CFBundleExecutable':'Test','CFBundlePackageType':'APPL'}))
            chain=signing.signing_home(home)/'identity.keychain-db'
            task_env=dict(os.environ)
            if system_tools:task_env['PATH']='/usr/bin:/bin:/usr/sbin:/sbin'
            before_search=subprocess.check_output(['security','list-keychains','-d','user'])
            before_trust=subprocess.run(['security','dump-trust-settings'],capture_output=True).stdout
            try:
                with mock.patch.dict(os.environ,task_env,clear=True):
                    first=signing.sign(app,home)
                    record=(signing.signing_home(home)/'identity.json').read_bytes()
                    second=signing.sign(app,home)
                    self.assertEqual(first['identity'],second['identity'])
                    self.assertEqual((signing.signing_home(home)/'identity.json').read_bytes(),record)
                    self.assertNotIn('cdhash ',second['requirement'])
                    self.assertIn('certificate leaf',second['requirement'])
                    subprocess.run(['codesign','--verify','--deep','--strict',str(app)],check=True,capture_output=True)
                    listed=subprocess.check_output(['security','list-keychains','-d','user'],text=True)
                    self.assertNotIn(str(chain),listed)
                    self.assertEqual(subprocess.check_output(['security','list-keychains','-d','user']),before_search)
                    self.assertEqual(subprocess.run(['security','dump-trust-settings'],capture_output=True).stdout,before_trust)
            finally:
                if chain.exists():subprocess.run(['security','delete-keychain',str(chain)],capture_output=True)

    def test_fresh_install_and_reuse_with_current_openssl(self):
        self.check_fresh_and_reused()

    def test_fresh_install_and_reuse_with_apple_system_tools(self):
        self.check_fresh_and_reused(system_tools=True)

if __name__=='__main__':unittest.main()
