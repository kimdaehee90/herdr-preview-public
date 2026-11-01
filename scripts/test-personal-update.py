#!/usr/bin/env python3
"""Exercise preservation and conflicts in disposable Git repos, never real apps."""
import importlib.util
import json
from pathlib import Path
import socket
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('personal_update', Path(__file__).with_name('personal-update.py'))
updates = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updates)


class Preservation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='hu-test-',dir='/private/tmp')
        self.work = Path(self.temp.name)
        self.root = self.work/'repo'; self.root.mkdir()
        updates.git(self.root,'init','-b','personal')
        updates.git(self.root,'config','user.name','Update Test')
        updates.git(self.root,'config','user.email','test@example.invalid')
        (self.root/'base').write_text('base\n')
        updates.git(self.root,'add','.'); updates.git(self.root,'commit','-m','base')
        self.base = updates.git(self.root,'rev-parse','HEAD')

    def tearDown(self):
        self.temp.cleanup()

    def commit(self, name, content):
        (self.root/name).write_text(content)
        updates.git(self.root,'add',name); updates.git(self.root,'commit','-m',name)

    def test_merge_preserves_personal_features_and_live_checkout(self):
        self.commit('personal-feature','keep native browser\n')
        head = updates.git(self.root,'rev-parse','HEAD')
        updates.git(self.root,'checkout','-b','upstream',self.base)
        self.commit('upstream-fix','new official behavior\n')
        updates.git(self.root,'checkout','personal')
        result = updates.prepare(self.root,'upstream',self.work/'backups')
        self.assertEqual(result['state'],'merged_needs_validation')
        candidate = Path(result['worktree'])
        self.assertEqual((candidate/'personal-feature').read_text(),'keep native browser\n')
        self.assertTrue((candidate/'upstream-fix').exists())
        self.assertEqual(updates.git(self.root,'rev-parse','HEAD'),head)
        self.assertFalse((self.root/'upstream-fix').exists())
        self.assertFalse(result['installed'])

    def test_conflict_does_not_replace_personal_source(self):
        self.commit('base','personal edit\n'); head=updates.git(self.root,'rev-parse','HEAD')
        updates.git(self.root,'checkout','-b','upstream',self.base)
        self.commit('base','conflicting official edit\n')
        updates.git(self.root,'checkout','personal')
        result=updates.prepare(self.root,'upstream',self.work/'backups')
        self.assertEqual(result['state'],'conflicts')
        self.assertEqual(result['conflicts'],['base'])
        self.assertEqual((self.root/'base').read_text(),'personal edit\n')
        self.assertEqual(updates.git(self.root,'rev-parse','HEAD'),head)
        self.assertFalse(result['installed'])

    def test_dirty_source_is_not_silently_overwritten(self):
        (self.root/'base').write_text('unsaved work\n')
        with self.assertRaisesRegex(RuntimeError,'Commit personal'):
            updates.prepare(self.root,self.base,self.work/'backups')
        self.assertEqual((self.root/'base').read_text(),'unsaved work\n')

    def test_missing_and_empty_required_backups_are_refused(self):
        for source in [self.work/'missing', self.work/'empty']:
            if source.name == 'empty': source.mkdir()
            with self.assertRaisesRegex(RuntimeError,'Required backup resource'):
                updates.archive(self.work/'required.tar.gz',[(source,'state')],required=True)
        result=updates.archive(self.work/'optional.tar.gz',[(self.work/'absent','optional')])
        self.assertEqual(result['status'],'missing')
        self.assertIsNone(result['archive'])

    def test_empty_or_stubbed_personal_feature_refuses_certification(self):
        (self.root/'personal-build.json').write_text(json.dumps({'required_sources':['feature.js']}))
        for text in ['', '// removed', 'export default {};']:
            (self.root/'feature.js').write_text(text)
            with self.assertRaisesRegex(RuntimeError,'missing or incomplete'):
                updates.verify_sources(self.root)

    def test_backup_preserves_config_app_and_uncommitted_source(self):
        config=self.work/'config';config.mkdir()
        (config/'settings.json').write_text('{"font":"custom","zoom":0.9}')
        app=self.work/'Personal.app';app.mkdir();(app/'binary').write_bytes(b'previous app')
        profile=self.work/'profile';profile.mkdir();(profile/'test-state').write_text('profile state')
        state=self.work/'state';(state/'default').mkdir(parents=True)
        (state/'default/docs.json').write_text('{"layout":"live"}')
        (state/'default/terminals.json').write_text('{"terminals":[]}')
        core=self.work/'herdr-core';core.write_bytes(b'core binary')
        (self.root/'base').write_text('new work\n\n')
        (self.root/'new-feature').write_text('not committed yet\n')
        sock=socket.socket(socket.AF_UNIX);sock.bind(str(state/'default/core.sock'))
        try:
            folder=updates.backup(self.root,self.work/'backups',config,app,profile,core,self.work/'missing-core-config',state,installed=True)
        finally:
            sock.close()
        self.assertEqual(folder.stat().st_mode & 0o777,0o700)
        updates.verify_backup(folder)
        manifest=json.loads((folder/'manifest.json').read_text())
        self.assertEqual(manifest['resources']['core_config']['status'],'missing')
        self.assertTrue(manifest['resources']['core_state']['required'])
        with tarfile.open(folder/'core-state-live-copy.tar.gz') as archive:
            self.assertIn('herdr-core-state/default/docs.json',archive.getnames())
            self.assertIn('herdr-core-state/default/terminals.json',archive.getnames())
            self.assertNotIn('herdr-core-state/default/core.sock',archive.getnames())
        with tarfile.open(folder/'personal-settings.tar.gz') as archive:
            self.assertIn('herdr-preview/settings.json',archive.getnames())
            self.assertNotIn('herdr-preview/app.sock',archive.getnames())
        with tarfile.open(folder/'untracked-source.tar.gz') as archive:
            self.assertIn('new-feature',archive.getnames())
        patch=folder/'working-tree.patch'
        updates.run(self.root,'git','apply','--reverse','--check',str(patch))
        self.assertEqual((config/'settings.json').read_text(),'{"font":"custom","zoom":0.9}')
        patch.write_text('tampered')
        with self.assertRaisesRegex(RuntimeError,'checksum'):
            updates.verify_backup(folder)


if __name__=='__main__':
    unittest.main()
