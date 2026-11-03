"""Run GUI fixtures under /private/tmp, away from macOS protected folders."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile


def run(root, binary, session, full=True, logs=None):
    assert session and session!='default' and len(session)<=8
    logs=logs or root/'.personal-update';logs.mkdir(exist_ok=True)
    env=os.environ.copy()
    for key in ['HC_SOCKET','HC_TERMINAL_ID','HERDR_APP_HOME','HERDR_APP_ID']:env.pop(key,None)
    env['HC_SESSION']=session
    hc=str(Path.home()/'.local/bin/herdr-core')
    with tempfile.TemporaryDirectory(prefix='herdr-qa-',dir='/private/tmp') as temp:
        qa=Path(temp);(qa/'scripts').mkdir();(qa/'target/debug').mkdir(parents=True)
        for name in ['browser-smoke.py','preview-smoke.py','browser-focus-smoke.py','document-preview-smoke.py']:
            shutil.copyfile(root/'scripts'/name,qa/'scripts'/name)
        shutil.copytree(root/'examples/preview',qa/'examples/preview')
        shutil.copytree(root/'tests/fixtures/document-preview',qa/'tests/fixtures/document-preview')
        staged=qa/'target/debug/herdr-app';shutil.copyfile(binary,staged);staged.chmod(0o755)
        home=qa/'.browser-test/app'
        commands=[['python3','scripts/browser-smoke.py','--home',str(home),'--binary',str(staged),'--session',session]]
        if full:commands += [['python3','scripts/preview-smoke.py','--home',str(home),'--binary',str(staged),'--session',session],['python3','scripts/browser-focus-smoke.py','--session',session],['python3','scripts/document-preview-smoke.py','--home',str(home),'--binary',str(staged),'--session',session]]
        try:
            for index,argv in enumerate(commands):
                with (logs/f'native-{index}.log').open('w') as output:
                    subprocess.run(argv,cwd=qa,env=env,stdout=output,stderr=subprocess.STDOUT,check=True)
        finally:
            # All programs in this uniquely named QA core are ours.
            result=subprocess.run([hc,'ls','--json'],env=env,capture_output=True,text=True)
            try:
                for terminal in json.loads(result.stdout).get('terminals',[]):
                    if '/' not in terminal['address']:subprocess.run([hc,'kill',terminal['address']],env=env,capture_output=True)
            except (ValueError,KeyError):pass
            try:
                with socket.socket(socket.AF_UNIX) as client:
                    client.settimeout(3);client.connect(str(home/'herdr-app.sock'));client.sendall(b'{"do":"app.quit"}\n');client.recv(1024)
            except OSError:pass
            subprocess.run([hc,'server','stop'],env=env,capture_output=True)
            if home.parent.exists():shutil.copytree(home.parent,logs/'native-artifacts',dirs_exist_ok=True,ignore=shutil.ignore_patterns('*.sock'))
        return {'native_ui_checks_passed':True,'protected_folders_used':False,'commands':commands,'session':session}
