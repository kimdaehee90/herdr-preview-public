#!/usr/bin/env python3
import gzip
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest import mock

spec=importlib.util.spec_from_file_location('audit',Path(__file__).with_name('audit-public.py'))
audit=importlib.util.module_from_spec(spec);spec.loader.exec_module(audit)


def archive(owner='',mtime=0):
    output=io.BytesIO()
    with gzip.GzipFile(filename='',mode='wb',fileobj=output,mtime=0) as compressed:
        with tarfile.open(fileobj=compressed,mode='w',format=tarfile.USTAR_FORMAT) as target:
            member=tarfile.TarInfo('Herdr Preview.app/file');member.size=4;member.uname=owner;member.mtime=mtime
            target.addfile(member,io.BytesIO(b'safe'))
    return output.getvalue()


class Privacy(unittest.TestCase):
    def test_public_repository_handle_does_not_hide_private_paths_or_email(self):
        handle=b'kim'+b'dae'+b'hee90'
        local_name=handle[3:9]
        audit.scan(b'https://github.com/'+handle+b'/repo '+handle+b'@users.noreply.github.com','public repo',[local_name])
        for value in [b'/Users/'+local_name+b'/project',local_name+b'@example.invalid',b'host-'+local_name+b'-private',
                      b'/Users/'+handle+b'/private',handle+b'@example.invalid',handle+b'.local']:
            with self.assertRaisesRegex(RuntimeError,'Private producer'):
                audit.scan(value,'private content',[b'/Users/'+local_name,local_name])

    def test_private_identifiers_and_credentials_are_rejected_without_echoing(self):
        with self.assertRaisesRegex(RuntimeError,'Private producer'):
            audit.scan(b'private-path/example','binary',[b'private-path'])
        for value in [b'ghp_'+b'A'*40,b'sk-proj-'+b'x'*60,b'-----BEGIN '+b'OPENSSH PRIVATE KEY-----']:
            with self.assertRaisesRegex(RuntimeError,'pattern') as result:audit.scan(value,'file',[])
            self.assertNotIn(value.decode(),str(result.exception))

    def test_css_mask_border_is_not_a_token(self):
        audit.scan(b'mask-border'+b'x'*100,'CSS',[])

    def test_package_owner_and_time_metadata_are_rejected(self):
        audit.archive(archive(),'safe.tar.gz',[])
        for data in [archive(owner='private-user'),archive(mtime=1234)]:
            with self.assertRaisesRegex(RuntimeError,'metadata'):audit.archive(data,'bad.tar.gz',[])

    def test_hidden_sensitive_value_inside_archive_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError,'Private producer'):
            audit.archive(archive(),'package',[b'safe'])

    def test_deleted_file_is_still_scanned_in_history(self):
        with tempfile.TemporaryDirectory(prefix='pub-audit-',dir='/private/tmp') as temp:
            root=Path(temp)
            def git(*args):return subprocess.run(['git',*args],cwd=root,check=True,capture_output=True)
            git('init','-b','main');git('config','user.name','Distribution');git('config','user.email','noreply@example.invalid')
            (root/'old.txt').write_text('fixture-private-address');git('add','.');git('commit','-m','first snapshot')
            git('rm','old.txt');(root/'README').write_text('safe');git('add','.');git('commit','-m','remove old file')
            with mock.patch.dict(os.environ,{'PUBLIC_PRIVACY_TERMS':json.dumps(['fixture-private-address'])}):
                with self.assertRaisesRegex(RuntimeError,'history:old.txt'):audit.audit(root)


if __name__=='__main__':unittest.main()
