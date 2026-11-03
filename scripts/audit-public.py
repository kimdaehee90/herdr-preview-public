#!/usr/bin/env python3
"""Fail publication on private build data; never print matched secret values."""
import argparse
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import socket
import subprocess
import tarfile

PATTERNS = {
    'GitHub/OpenAI token': re.compile(rb'(?<![A-Za-z0-9_-])(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|sk-(?:proj-|svcacct-)?[A-Za-z0-9_-]{35,})(?![A-Za-z0-9_-])'),
    'private key': re.compile(rb'-----BEGIN (?:RSA |OPENSSH |EC )?PRIVATE KEY-----'),
    'AWS access key': re.compile(rb'AKIA[A-Z0-9]{16}'),
}
PRIVATE_DIRS = {'.git','.public-build','.personal-update','.browser-test','.preview-test','WebKit','.ssh','.aws','reviews','output'}
# This repository owner is already a public identifier. A local account name
# can be its substring; that must not reject the published GitHub URL itself.
PUBLIC_OWNER = b'kim'+b'dae'+b'hee90'
PUBLIC_OWNER_CONTEXTS = (
    rb'https://github\.com/'+re.escape(PUBLIC_OWNER)+rb'(?=[/?#\s)]|$)',
    rb'(?<![A-Za-z0-9._%+-])(?:[0-9]+\+)?'+re.escape(PUBLIC_OWNER)+rb'@users\.noreply\.github\.com(?![A-Za-z0-9.-])',
)


def private_terms():
    home=Path.home()
    values=[str(home),socket.gethostname()]
    if len(home.name)>=5 and home.name not in {'runner','builder','public','admin','administrator'}:values.append(home.name)
    values += json.loads(os.environ.get('PUBLIC_PRIVACY_TERMS','[]'))
    return [v.encode() for v in values if isinstance(v,str) and len(v)>=4]


def scan(data, name, terms):
    for value in terms:
        candidate=data
        if value.lower() in PUBLIC_OWNER.lower():
            for context in PUBLIC_OWNER_CONTEXTS:
                candidate=re.sub(context,b'PUBLIC_GITHUB_IDENTITY',candidate,flags=re.I)
        if value.lower() in candidate.lower():
            raise RuntimeError('Private producer identifier in '+name)
    for kind, pattern in PATTERNS.items():
        if pattern.search(data):raise RuntimeError(kind+' pattern in '+name)


def archive(data, name, terms):
    scan(data,name,terms)
    # gzip header must not encode producer time or filename.
    if data[:2]!=b'\x1f\x8b' or data[3] or data[4:8]!=b'\0\0\0\0':
        raise RuntimeError('Non-normalized gzip metadata in '+name)
    with tarfile.open(fileobj=io.BytesIO(data),mode='r:gz') as source:
        for item in source.getmembers():
            path=PurePosixPath(item.name)
            if path.is_absolute() or '..' in path.parts or not (item.isfile() or item.isdir()):
                raise RuntimeError('Unsafe archive member in '+name)
            if item.uid or item.gid or item.uname or item.gname or item.mtime or item.pax_headers:
                raise RuntimeError('Producer metadata in '+name)
            scan(item.name.encode(),name+' member name',terms)
            if item.isfile():scan(source.extractfile(item).read(),name+'!'+item.name,terms)


def check_path(name):
    if any(part in PRIVATE_DIRS for part in PurePosixPath(name).parts):
        raise RuntimeError('Private state path in publication: '+name)


def audit(root, history=True):
    terms=private_terms();count=0;archives=0
    files=subprocess.check_output(['git','ls-files','-z'],cwd=root).decode().split('\0')
    for name in filter(None,files):
        check_path(name);path=root/name
        if path.is_symlink():scan(os.readlink(path).encode(),name,terms)
        else:
            data=path.read_bytes()
            if name.endswith('.tar.gz'):archive(data,name,terms);archives+=1
            else:scan(data,name,terms)
        count+=1
    commits=0;historic_blobs=0
    if history:
        meta=subprocess.check_output(['git','log','--all','--format=%H %an <%ae> %cn <%ce>%n%B'],cwd=root)
        scan(meta,'commit metadata',terms)
        commits=int(subprocess.check_output(['git','rev-list','--all','--count'],cwd=root))
        lines=subprocess.check_output(['git','rev-list','--objects','--all'],cwd=root,text=True).splitlines()
        objects=[]
        for line in lines:
            fields=line.split(' ',1)
            if len(fields)==2:objects.append(fields)
        # Batch all objects through Git once, rather than a subprocess per file.
        batch=subprocess.Popen(['git','cat-file','--batch'],cwd=root,stdin=subprocess.PIPE,stdout=subprocess.PIPE)
        try:
            for oid,name in objects:
                batch.stdin.write((oid+'\n').encode());batch.stdin.flush()
                header=batch.stdout.readline().split()
                if len(header)!=3:raise RuntimeError('Missing history object')
                size=int(header[2]);data=batch.stdout.read(size);batch.stdout.read(1)
                if header[1]!=b'blob':continue
                check_path(name)
                if name.endswith('.tar.gz'):archive(data,'history:'+name,terms)
                else:scan(data,'history:'+name,terms)
                historic_blobs+=1
        finally:
            batch.stdin.close();batch.stdout.close();batch.wait()
    return {'files_scanned':count,'archives_scanned':archives,'commits_scanned':commits,'historic_blobs_scanned':historic_blobs,'passed':True}


def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--staged-only',action='store_true');args=parser.parse_args()
    print(json.dumps(audit(Path(__file__).resolve().parent.parent,history=not args.staged_only),indent=2))


if __name__=='__main__':main()
