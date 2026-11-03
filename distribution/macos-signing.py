"""Device-local code identity; never changes trust settings or exports signing keys."""
import fcntl
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import tempfile


def command(argv, *, env=None):
    result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=30)
    if result.returncode:
        # argv can contain an ephemeral keychain password: never log it.
        raise RuntimeError('Local signing command failed: '+argv[0]+' '+argv[1]+': '+result.stderr.strip()[:300])
    return result.stdout


def signing_home(home):
    return home/'Library/Application Support/Herdr Preview/Signing'


def remove_from_search_list(chain):
    # A dedicated keychain need not appear in the user's global search list.
    listed=command(['security','list-keychains','-d','user'])
    paths=re.findall(r'"([^"\n]+)"',listed)
    kept=[p for p in paths if Path(p).resolve()!=chain.resolve()]
    if len(kept)!=len(paths):command(['security','list-keychains','-d','user','-s',*kept])


def ensure_identity(directory):
    record=directory/'identity.json';chain=directory/'identity.keychain-db';password_file=directory/'keychain-password'
    if record.exists():
        data=json.loads(record.read_text())
        if not chain.is_file() or not password_file.is_file() or not re.fullmatch('[A-F0-9]{40}',data.get('identity','')):
            raise RuntimeError('Local signing identity is incomplete; restore its private backup rather than silently rotating it')
        return data,chain,password_file.read_text()
    if chain.exists() or password_file.exists():
        raise RuntimeError('Incomplete signing setup; inspect it before creating another identity')
    password=secrets.token_urlsafe(48)
    with tempfile.TemporaryDirectory(prefix='.identity-',dir=directory) as temporary:
        stage=Path(temporary);stage.chmod(0o700)
        config=stage/'certificate.cnf'
        config.write_text('[req]\ndistinguished_name=dn\nx509_extensions=ext\nprompt=no\n[dn]\nCN=Herdr Preview Local\n[ext]\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=codeSigning\n')
        created=False
        try:
            command(['openssl','req','-new','-x509','-newkey','rsa:3072','-nodes','-days','3650','-config',str(config),'-keyout',str(stage/'key.pem'),'-out',str(stage/'certificate.pem')])
            (stage/'key.pem').chmod(0o600)
            command(['security','create-keychain','-p',password,str(chain)]);created=True
            remove_from_search_list(chain)
            command(['security','unlock-keychain','-p',password,str(chain)])
            # Native PEM import avoids OpenSSL-version-dependent PKCS#12
            # encryption/MAC defaults. Keys remain non-extractable after import.
            command(['security','import',str(stage/'key.pem'),'-k',str(chain),'-x','-T','/usr/bin/codesign'])
            command(['security','import',str(stage/'certificate.pem'),'-k',str(chain)])
            command(['security','set-key-partition-list','-S','apple-tool:,apple:','-s','-k',password,str(chain)])
            identities=command(['security','find-identity','-p','codesigning',str(chain)])
            match=re.search(r'\b([A-F0-9]{40})\b',identities)
            if not match:raise RuntimeError('No local code signing identity was imported')
            data={'schema':1,'identity':match.group(1),'purpose':'Herdr Preview local updates','system_trust_changed':False}
            (directory/'certificate.pem').write_bytes((stage/'certificate.pem').read_bytes())
            password_file.write_text(password);password_file.chmod(0o600)
            record.write_text(json.dumps(data,indent=2)+'\n');record.chmod(0o600)
            return data,chain,password
        except BaseException:
            if created:command(['security','delete-keychain',str(chain)])
            password_file.unlink(missing_ok=True);record.unlink(missing_ok=True)
            raise
        finally:
            if chain.exists():command(['security','lock-keychain',str(chain)])


def sign(app, home, identifier='dev.herdr.preview'):
    directory=signing_home(home)
    # Keys must never be redirected into a repository or shared directory.
    current=directory
    while current!=home:
        if current.is_symlink():raise RuntimeError('Refusing a symlinked local signing directory')
        current=current.parent
    directory.mkdir(parents=True,exist_ok=True,mode=0o700);directory.chmod(0o700)
    # All installs by this OS user share one search-list lock, including
    # isolated verification homes. No key material is stored in this lock.
    search_lock=Path(tempfile.gettempdir())/'herdr-preview-signing-search.lock'
    fd=os.open(search_lock,os.O_WRONLY|os.O_CREAT|os.O_NOFOLLOW|os.O_CLOEXEC,0o600)
    with os.fdopen(fd,'a') as shared_lock, (directory/'signing.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        fcntl.flock(shared_lock,fcntl.LOCK_EX)
        data,chain,password=ensure_identity(directory)
        try:
            command(['security','unlock-keychain','-p',password,str(chain)])
            listed=command(['security','list-keychains','-d','user'])
            paths=re.findall(r'"([^"\n]+)"',listed)
            added=not any(Path(p).resolve()==chain.resolve() for p in paths)
            try:
                # codesign needs certificate-chain discovery even with
                # --keychain. Searchability grants no trust and lasts only
                # for this signing call; preserve preexisting list entries.
                if added:command(['security','list-keychains','-d','user','-s',*paths,str(chain)])
                command(['codesign','--force','--sign',data['identity'],'--keychain',str(chain),'--identifier',identifier,'--timestamp=none',str(app)])
            finally:
                if added:remove_from_search_list(chain)
            command(['codesign','--verify','--deep','--strict',str(app)])
            requirement=command(['codesign','-d','-r-',str(app)]).strip()
            expected='certificate leaf = H"'+data['identity'].lower()+'"'
            if expected not in requirement or 'cdhash ' in requirement:
                raise RuntimeError('Signing did not produce the stable certificate-bound requirement')
            return {'kind':'device-local-certificate','identity':data['identity'],'requirement':requirement}
        finally:
            command(['security','lock-keychain',str(chain)])
