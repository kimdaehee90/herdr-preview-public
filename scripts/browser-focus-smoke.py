#!/usr/bin/env python3
"""Native first-responder regression on an isolated app, with no desktop input."""
import json
import argparse
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--session', default='bv')
parser.add_argument('--expect-bug', action='store_true')
args = parser.parse_args()
assert args.session and args.session != "default" and len(args.session) <= 8, "Use a short isolated core session"
home = root / '.browser-test/app'
assert home.parent.name == '.browser-test'

def call(payload):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(8)
        client.connect(str(home / 'herdr-app.sock'))
        client.sendall((json.dumps(payload) + '\n').encode())
        data = b''
        while b'\n' not in data:
            data += client.recv(65536)
    result = json.loads(data)
    assert 'error' not in result, result
    return result

env = os.environ.copy()
env.update(HERDR_APP_HOME=str(home), HC_SESSION=args.session, HERDR_APP_ID='herdr-browser-test', HERDR_PREVIEW_TEST_CONTROL='1')
with (home.parent / 'focus-app.log').open('ab') as log:
    app = subprocess.Popen([str(root / 'target/debug/herdr-app')], env=env, stdout=log, stderr=log)
space = None
try:
    for _ in range(100):
        try:
            call({'status': True})
            break
        except (OSError, ValueError):
            time.sleep(.1)
    for attempt in range(100):
        try:
            space = call({'do':'space.new','args':{'name':'Browser focus QA','folder':str(root)}})['space']
            break
        except AssertionError:
            if attempt == 99:
                raise
            time.sleep(.1)
    # A local Preview server supplies the Browser page; no account or remote site.
    call({'do':'preview.open','args':{'target':str(root / 'examples/preview/index.html')}})
    url = call({'status': True})['preview']['url']
    call({'do':'browser.open','args':{'url':url}})
    for _ in range(100):
        if not call({'status':True})['browser']['loading']:
            break
        time.sleep(.1)
    for _ in range(3):
        before = call({'_browser_focus_test':'browser'})
        assert not before['parent'], before
        call({'_browser_focus_test':'parent-click'})
        time.sleep(.15)
        after = call({'_browser_focus_test':'status'})
        print(json.dumps({'before':before,'after':after}), flush=True)
        if '--expect-bug' in sys.argv:
            assert not after['parent'], after
            print('REPRODUCED: native Browser retains keyboard focus after terminal click', flush=True)
            break
        assert after['parent'], 'Keyboard remained in the native browser after parent click'
        call({'_browser_focus_test':'type'})
        time.sleep(.15)
        world = call({'ls':True})
        current = next(s for s in world['spaces'] if s['id'] == space)
        terminal = current['tabs'][0]['terminals'][0]['terminal']
        screen = subprocess.check_output([str(Path.home()/'.local/bin/hc'), 'read', terminal, '--lines', '12'], env=env, text=True)
        assert 'focus_verify' in screen, 'Native keyboard input did not reach the terminal: ' + screen
    else:
        print('PASS native Browser → terminal → Browser focus switching and native text input, repeated 3 times', flush=True)
finally:
    if space:
        call({'do':'space.close','args':{'id':space,'confirmed':True}})
    call({'do':'app.quit'})
    app.wait(timeout=10)
