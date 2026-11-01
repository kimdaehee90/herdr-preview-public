#!/usr/bin/env python3
"""Exercise only an isolated Preview debug app; no OS mouse/keyboard input."""
import argparse
import base64
import json
import shutil
import socket
import time
import uuid
import os
import subprocess
import urllib.request
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--home', type=Path, required=True)
parser.add_argument('--binary', type=Path)
parser.add_argument('--session', default='pv')
args = parser.parse_args()
assert args.session and args.session != "default" and len(args.session) <= 8, "Use a short isolated core session"
home = args.home.resolve()
root = Path(__file__).resolve().parent.parent
assert home in {root/'.preview-test/app',root/'.browser-test/app'}, 'Use an isolated QA home only'
artifacts = home.parent / 'artifacts'
shutil.copytree(root / 'examples' / 'preview', artifacts, dirs_exist_ok=True)
(artifacts / 'sample.png').write_bytes(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j3ioAAAAASUVORK5CYII='))
results = []

def call(command):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(8)
        client.connect(str(home / 'herdr-app.sock'))
        client.sendall((json.dumps(command) + '\n').encode())
        reply = b''
        while b'\n' not in reply:
            chunk = client.recv(65536)
            if not chunk: break
            reply += chunk
    data = json.loads(reply)
    if 'error' in data: raise RuntimeError(data['error'])
    return data

def do(command_name, **kwargs):
    return call({'do': command_name, 'args': kwargs})

def wait(predicate, label, timeout=12):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value: return value
        time.sleep(.08)
    raise AssertionError(label)

def probe(expression):
    token = uuid.uuid4().hex
    script = '(()=>{const token=' + json.dumps(token) + ';window.__previewTests??={};if(!(token in window.__previewTests))window.__previewTests[token]=(' + expression + ');return JSON.stringify({token,value:window.__previewTests[token]})})()'
    last_sent = [0.]
    def read():
        try:
            data = json.loads((home / 'preview-test-result.json').read_text())
            if isinstance(data, str): data = json.loads(data)
            if data.get('token') == token: return data
        except (OSError, ValueError): pass
        # Wry queues scripts during a navigation but drops their callbacks.
        # Retry a cached, at-most-once expression after the page has loaded.
        if time.monotonic() - last_sent[0] > .3 and not (call({'status': True}).get('preview') or {}).get('loading'):
            call({'_preview_test': script}); last_sent[0] = time.monotonic()
        return None
    return wait(read, 'WebKit evaluation timed out')['value']

def visible(want):
    call({'raise': True})
    return wait(lambda: (call({'status': True}).get('preview') or {}).get('visible') == want,
                f'Preview visible != {want}')

def passed(name, detail=None):
    results.append({'check': name, 'passed': True, 'detail': detail})
    print('PASS', name, flush=True)

owned_process = None
try:
    try:
        call({'status':True})
    except OSError:
        home.mkdir(parents=True,exist_ok=True)
        (home/'settings.json').write_text(json.dumps({'core':str(Path.home()/'.local/bin/herdr-core'),'telemetry':False,'sounds':{'enabled':False}}))
        (home/'shared.json').write_text(json.dumps({'layout':'main','frame.welcomed':'finished','workspace.panels':['files','changes','preview','browser']}))
        env={**os.environ,'HERDR_APP_HOME':str(home),'HERDR_APP_ID':'herdr-preview-test','HC_SESSION':args.session,'HERDR_PREVIEW_TEST_CONTROL':'1'}
        log=(home.parent/'standalone-app.log').open('ab')
        owned_process=subprocess.Popen([str((args.binary or root/'target/debug/herdr-app').resolve())],env=env,stdout=log,stderr=log)
        def ready():
            try:return call({'status':True})
            except OSError:return None
        wait(ready,'Standalone test app did not start')
    call({'raise': True})
    def create_space():
        try:return do('space.new',name='Preview 검증',folder=str(artifacts)).get('space')
        except RuntimeError as error:
            if 'layout is unavailable' not in str(error):raise
            return None
    space=wait(create_space,'Test core did not connect')
    do('preview.open', target=str(artifacts / 'index.html'))
    visible(True)
    wait(lambda: probe('document.readyState') == 'complete', 'HTML did not load')
    loaded = probe("({title:document.title,background:getComputedStyle(document.documentElement).backgroundColor,image:document.querySelector('img').naturalWidth,width:innerWidth})")
    assert loaded['background'] == 'rgb(243, 246, 243)' and loaded['image'] > 0 and loaded['width'] > 200, loaded
    passed('HTML, relative CSS/SVG and native viewport', loaded)

    value = probe("(()=>{const el=document.getElementById('name');el.value='대한민국';el.dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('counter').click();document.getElementById('counter').click();return {greeting:document.getElementById('greeting').textContent,count:document.getElementById('count').textContent}})()")
    assert '대한민국님' in value['greeting'] and value['count'] == '2', value
    passed('Korean DOM input and real page click handlers', value)

    do('layout.select', stack='panel-body', child='files'); visible(False)
    do('layout.select', stack='panel-body', child='preview'); visible(True)
    assert probe("document.getElementById('count').textContent") == '2'
    passed('Files/Preview switch preserves page state')

    first_tab=call({'status':True})['preview']['active_tab']
    do('preview.view',zoom=.75)
    image_tab=do('preview.tab.new',target=str(artifacts/'icon.svg'))['tab'];visible(True)
    wait(lambda:probe('document.documentElement.tagName.toLowerCase()')=='svg','Image tab did not load')
    image_url=call({'status':True})['preview']['url']
    do('preview.tab.select',id=first_tab);visible(True)
    assert probe("document.getElementById('count').textContent")=='2'
    assert call({'status':True})['preview']['page_zoom']==.75
    assert sum(t['visible'] for t in call({'status':True})['preview']['tabs'])==1
    passed('Preview artifact tabs retain independent live documents and zoom when switching')
    empty_tab=do('preview.tab.new')['tab']
    wait(lambda:not any(t['visible'] for t in call({'status':True})['preview']['tabs']),'Old Preview remained visible behind a fresh tab')
    do('preview.tab.close',id=empty_tab)
    do('preview.tab.close',id=image_tab)
    def image_server_closed():
        try:
            with urllib.request.urlopen(image_url,timeout=.3) as response:return response.status!=200
        except OSError:return True
    wait(image_server_closed,'Closed artifact tab kept its folder server alive')
    assert probe("document.getElementById('count').textContent")=='2'
    passed('Fresh Preview tabs hide old pages; closing a tab releases its native view and file server')
    do('preview.view',fit=True)

    do('app.settings', section='look'); visible(False)
    do('overlay.close'); visible(True)
    assert probe("document.getElementById('name').value") == '대한민국'
    passed('Settings overlay hides native child and restores form state')

    do('layout.resize', id='panel', px=680)
    wait(lambda: probe('innerWidth') >= 675, 'WebView did not follow panel resize')
    do('layout.resize', id='panel', px=560)
    passed('Native child follows panel resize')

    css = artifacts / 'style.css'
    original = css.read_text()
    css.write_text(original.replace('#f3f6f3', '#e6f0ff'))
    wait(lambda: probe('getComputedStyle(document.documentElement).backgroundColor') == 'rgb(230, 240, 255)', 'CSS save did not reload WebView')
    css.write_text(original)
    wait(lambda: probe('getComputedStyle(document.documentElement).backgroundColor') == 'rgb(243, 246, 243)', 'CSS restore did not reload WebView')
    passed('Saved file refreshes HTML and linked CSS automatically')

    do('preview.open', target=str(artifacts / 'icon.svg'))
    wait(lambda: probe('document.documentElement.tagName.toLowerCase()') == 'svg', 'SVG image did not load')
    passed('Image artifacts open in the same panel')

    do('preview.open', target=str(artifacts / 'sample.png'))
    wait(lambda: probe("document.contentType==='image/png' && document.querySelector('img')?.naturalWidth===1"), 'PNG image did not load')
    passed('PNG result opens directly inside Preview')

    do('preview.open', target=str(artifacts / 'index.html'))
    wait(lambda: probe("!!document.querySelector('#name')"), 'HTML did not return')
    local_url = call({'status': True})['preview']['url']
    do('preview.open', target=local_url)
    wait(lambda: probe("!!document.querySelector('#counter')"), 'HTTP target did not load')
    passed('HTTP development URL opens as an interactive page')

    probe("(()=>{document.getElementById('counter').click();return true})()")
    other = do('space.new', name='Preview 두 번째 공간', folder=str(artifacts))['space']
    do('preview.open', target=str(artifacts / 'index.html')); visible(True)
    wait(lambda: probe("!!document.querySelector('#count')"), 'Second space did not load')
    assert probe("document.getElementById('count').textContent") == '0'
    do('space.select', id=space); visible(True)
    assert probe("document.getElementById('count').textContent") == '1'
    passed('Each space retains its own live page state')
    do('space.close', id=other, confirmed=True)
    previous_url=call({'status':True})['preview']['url']
    next_folder=artifacts/'next-folder';next_folder.mkdir(exist_ok=True)
    (next_folder/'index.html').write_text('<html><body>SECOND-FOLDER</body></html>')
    do('preview.open',target=str(next_folder/'index.html'))
    wait(lambda:probe('document.body.innerText')=='SECOND-FOLDER','Second folder did not load')
    def previous_closed():
        try:
            with urllib.request.urlopen(previous_url,timeout=.3) as response:return response.status!=200
        except OSError:return True
    wait(previous_closed,'Previous preview server still serves files')
    call({'_preview_history_test':False})
    wait(lambda:probe("!!document.querySelector('#counter')"),'Local history did not reopen the previous artifact')
    passed('Folder changes close old servers; artifact Back reopens with a fresh scoped server')
    do('preview.open', target=str(root / 'examples' / 'preview' / 'index.html'))
    visible(True)
    report = {'checks': results, 'count': len(results), 'all_passed': True,
              'note': 'DOM input/click events in a native WebView; OS keyboard and IME composition are not synthesized.'}
    (home.parent / 'smoke-results.json').write_text(json.dumps(report, ensure_ascii=False, indent=2))
    print(json.dumps({'passed': len(results), 'report': str(home.parent / 'smoke-results.json')}, ensure_ascii=False))
finally:
    if owned_process:
        try:
            if 'space' in globals():do('space.close',id=space,confirmed=True)
            do('app.quit')
            owned_process.wait(timeout=10)
        finally:
            subprocess.run([str(Path.home()/'.local/bin/hc'),'server','stop'],env=env,capture_output=True)
