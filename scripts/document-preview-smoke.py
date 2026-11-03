#!/usr/bin/env python3
"""Exercise paginated documents in an isolated macOS app, never the user's app."""
import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import time
import uuid
import zlib

parser=argparse.ArgumentParser()
parser.add_argument('--home',type=Path,required=True)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--session',default='docqa')
parser.add_argument('--case',choices=['all','symlink','history-single','history-collection','reload'],default='all')
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
home=args.home.resolve()
assert root.parent==Path('/private/tmp') and root.name.startswith('herdr-qa-'), 'Run a staged QA fixture under /private/tmp'
assert home==root/'.browser-test/app' and args.session!='default' and len(args.session)<=8
home.mkdir(parents=True,exist_ok=True)
artifacts=root/'document-artifacts';artifacts.mkdir(exist_ok=True)
results=[]

def png(path,rgb):
    path.parent.mkdir(parents=True,exist_ok=True)
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',32,16,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes(rgb)*32)*16))+chunk(b'IEND',b''))

def pdf(path,count):
    objects=['<< /Type /Catalog /Pages 2 0 R >>',f'<< /Type /Pages /Count {count} /Kids ['+' '.join(f'{3+2*i} 0 R' for i in range(count))+'] >>']
    for i in range(count):
        objects.append(f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 180] /Contents {4+i*2} 0 R >>')
        content=f'{(i+1)/count} 0 0 rg 0 0 320 180 re f'
        objects.append(f'<< /Length {len(content)} >>\nstream\n{content}\nendstream')
    data=b'%PDF-1.4\n';offsets=[0]
    for i,obj in enumerate(objects):offsets.append(len(data));data+=f'{i+1} 0 obj\n{obj}\nendobj\n'.encode()
    xref=len(data);data+=f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode()
    data+=b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets[1:])
    data+=f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode()
    path.write_bytes(data)

def call(payload):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(8);client.connect(str(home/'herdr-app.sock'))
        client.sendall((json.dumps(payload)+'\n').encode());data=b''
        while b'\n' not in data:
            chunk=client.recv(65536)
            if not chunk:break
            data+=chunk
    return json.loads(data)

def do(name,**kw):
    value=call({'do':name,'args':kw})
    if value.get('error'):raise AssertionError(value)
    return value

def wait(check,label,timeout=25):
    deadline=time.monotonic()+timeout
    while time.monotonic()<deadline:
        result=check()
        if result:return result
        time.sleep(.08)
    raise AssertionError(label+'; status='+json.dumps(call({'status':True})))

def ready(page,pages,target):
    def check():
        v=call({'status':True}).get('preview') or {}
        if v.get('error'):raise AssertionError(v)
        return v if v.get('target')==str(target) and v.get('page')==page and v.get('pages')==pages and not v.get('loading') else None
    return wait(check,f'page {page}/{pages} did not load',60)

def probe(expression):
    token=uuid.uuid4().hex
    script='JSON.stringify({token:'+json.dumps(token)+',value:('+expression+')})'
    def check():
        call({'_preview_test':script})
        try:
            value=json.loads((home/'preview-test-result.json').read_text())
            if isinstance(value,str):value=json.loads(value)
            if value.get('token')==token:return value
        except (OSError,ValueError,AttributeError):pass
    return wait(check,'native document DOM probe failed')['value']

def pixel():
    return probe("(()=>{const i=document.querySelector('img');if(!i?.complete)return null;const c=document.createElement('canvas');c.width=1;c.height=1;c.getContext('2d').drawImage(i,0,0);return [...c.getContext('2d').getImageData(0,0,1,1).data].slice(0,3)})()")

def passed(name):
    results.append(name);print('PASS',name,flush=True)

def review_case(case,red,blue):
    if case=='symlink':
        linked=artifacts/'linked.png'
        if not linked.exists():linked.symlink_to(red)
        folder=artifacts/'linked-parent'
        if not folder.exists():folder.symlink_to(red.parent,target_is_directory=True)
        for kw in [{'target':str(linked)},{'target':str(folder/red.name)},
                   {'targets':[str(linked),str(blue)]},{'targets':[str(folder/red.name),str(blue)]}]:
            result=call({'do':'preview.open','args':kw})
            assert result.get('error'),f'public open accepted a linked selection: {kw}'
        passed('Public single and multi-file opens reject symbolic files and parent directories')
    elif case=='history-single':
        source=artifacts/'fresh-history.pdf';pdf(source,3)
        do('preview.tab.new',target=str(source));ready(1,3,source)
        do('preview.view',page=2);ready(2,3,source);before=pixel()
        call({'_preview_history_test':False})
        deadline=time.monotonic()+1
        while time.monotonic()<deadline:
            assert pixel()==before,'History Back changed document pixels without changing the page indicator'
            assert call({'status':True})['preview']['page']==2
        passed('History Back in a fresh document cannot desynchronize pixels and page count')
    elif case=='history-collection':
        first=artifacts/'history-first.pdf';last=artifacts/'history-last.pdf';pdf(first,3);pdf(last,2)
        do('preview.tab.new',target=str(first));ready(1,3,first)
        do('preview.open',targets=[str(red),str(blue)]);ready(1,2,red)
        do('preview.open',target=str(last));ready(1,2,last)
        call({'_preview_history_test':False})
        state=wait(lambda:(lambda v:v if v and not v.get('loading') else None)(call({'status':True}).get('preview')),'History did not settle')
        assert state['target']==str(red) and state['files']==[str(red),str(blue)],state
        assert pixel()==[255,0,0]
        passed('Artifact history restores the entire explicitly selected PNG collection')
    elif case=='reload':
        deck=root/'tests/fixtures/document-preview/three-slides.pptx'
        do('preview.open',target=str(deck));ready(1,3,deck)
        do('preview.view',page=3);ready(3,3,deck)
        do('preview.reload');do('preview.reload')
        state=wait(lambda:(lambda v:v if v and v.get('pages')==3 and not v.get('loading') else None)(call({'status':True}).get('preview')),'Repeated reload did not settle',60)
        assert state['page']==3,state
        passed('Repeated reload during office preparation preserves the requested page')

env=os.environ.copy()
for name in ['HC_SOCKET','HC_TERMINAL_ID','HERDR_APP_HOME','HERDR_APP_ID']:env.pop(name,None)
env.update(HERDR_APP_HOME=str(home),HERDR_APP_ID='herdr-document-test',HC_SESSION=args.session,HERDR_PREVIEW_TEST_CONTROL='1')
(home/'settings.json').write_text(json.dumps({'core':str(Path.home()/'.local/bin/herdr-core'),'telemetry':False,'sounds':{'enabled':False}}))
(home/'shared.json').write_text(json.dumps({'layout':'main','frame.welcomed':'finished','workspace.panels':['preview','files']}))
log=(home.parent/'document-app.log').open('ab')
app=subprocess.Popen([str(args.binary)],env=env,stdout=log,stderr=log)
space=None
try:
    def connected():
        try:return call({'status':True})
        except (OSError,ValueError):return None
    wait(connected,'isolated app did not start')
    def new_space():
        value=call({'do':'space.new','args':{'name':'문서 Preview 검증','folder':str(artifacts)}})
        return value.get('space')
    space=wait(new_space,'isolated core did not connect')
    red=artifacts/'a/same.png';blue=artifacts/'b/same.png'
    png(red,[255,0,0]);png(blue,[0,0,255]);png(artifacts/'unselected.png',[0,255,0])
    if args.case!='all':
        review_case(args.case,red,blue)
        sys.exit(0)
    do('preview.open',targets=[str(blue),str(red)])
    state=ready(1,2,blue);assert state['files']==[str(blue),str(red)]
    assert pixel()==[0,0,255]
    do('preview.view',page=2);ready(2,2,blue);assert pixel()==[255,0,0]
    passed('Explicit multi-PNG selection keeps order and duplicate basenames distinct')
    for page in [0,3,-1,2.5]:
        result=call({'do':'preview.view','args':{'page':page}})
        assert result.get('error'),result
        assert call({'status':True})['preview']['page']==2
    passed('Invalid page requests retain the last verified page')
    png_tab=state['active_tab']
    source=artifacts/'pages.pdf';pdf(source,3)
    pdf_tab=do('preview.tab.new',target=str(source))['tab'];ready(1,3,source)
    do('preview.view',page=3);ready(3,3,source);assert pixel()[0]>240
    do('preview.tab.select',id=png_tab);ready(2,2,blue);assert pixel()==[255,0,0]
    do('preview.tab.select',id=pdf_tab);ready(3,3,source)
    passed('PPT/PDF-style page controls and independent artifact tab positions')
    pdf(source,1);do('preview.reload');ready(1,1,source)
    passed('Reload clamps a shortened PDF to its remaining page')
    for suffix in ['pptx','ppt']:
        deck=root/'tests/fixtures/document-preview'/f'three-slides.{suffix}'
        do('preview.open',target=str(deck));ready(1,3,deck)
        do('preview.view',page=3);ready(3,3,deck);assert pixel()[2]>200
    passed('PPT and PPTX open as static slides and jump directly to slide 3')
    deck=root/'tests/fixtures/document-preview/three-slides.pptx'
    do('preview.open',target=str(deck));do('preview.open',targets=[str(red),str(blue)]);ready(1,2,red)
    deadline=time.monotonic()+1.5
    while time.monotonic()<deadline:
        assert call({'status':True})['preview']['target']==str(red);time.sleep(.08)
    assert pixel()==[255,0,0]
    passed('Switching away during office conversion cannot restore stale slides')
    broken=artifacts/'broken.pdf';broken.write_bytes(b'not a PDF')
    do('preview.open',target=str(broken))
    failed=wait(lambda:(lambda v:v if v.get('error') and not v.get('loading') else None)(call({'status':True})['preview']),'Malformed document did not report failure')
    assert failed['page']==0 and failed['target']==str(broken) and not failed['visible']
    passed('Failed document preparation hides the old image and reports the new file error')
    do('preview.open',targets=[str(red),str(blue)]);ready(1,2,red)
    do('preview.view',page=2);ready(2,2,red)
    # Keep a representative window frame for visual QA, plus a native-image probe.
    shot=subprocess.run([str(args.binary),'shot',str(home.parent/'document-controls.png')],env=env,capture_output=True,text=True)
    assert shot.returncode==0,shot.stderr
    do('app.quit');app.wait(timeout=10)
    app=subprocess.Popen([str(args.binary)],env=env,stdout=log,stderr=log)
    wait(connected,'isolated app did not restart')
    do('layout.select',stack='panel-body',child='preview')
    restored=ready(2,2,red)
    assert restored['files']==[str(red),str(blue)] and pixel()==[0,0,255]
    passed('Explicit PNG collection and selected page restore after an app restart')
    for case in ['symlink','history-single','history-collection','reload']:review_case(case,red,blue)
    (home.parent/'document-smoke-results.json').write_text(json.dumps({'passed':len(results),'checks':results},ensure_ascii=False,indent=2))
    print(json.dumps({'passed':len(results),'report':str(home.parent/'document-smoke-results.json')}),flush=True)
finally:
    try:
        if space:do('space.close',id=space,confirmed=True)
        do('app.quit');app.wait(timeout=10)
    finally:
        if app.poll() is None:app.terminate();app.wait(timeout=5)
        subprocess.run([str(Path.home()/'.local/bin/herdr-core'),'server','stop'],env=env,capture_output=True)
