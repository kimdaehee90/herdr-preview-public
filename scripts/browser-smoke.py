#!/usr/bin/env python3
"""Native Browser checks on an isolated app/profile, using only loopback fixtures."""
import argparse
import http.server
import json
import os
import socket
import struct
import subprocess
import threading
import time
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--home',type=Path,required=True)
parser.add_argument('--binary',type=Path,required=True)
parser.add_argument('--session',default='bv')
args=parser.parse_args()
assert args.session and args.session != "default" and len(args.session) <= 8, "Use a short isolated core session"
home=args.home.resolve(); binary=args.binary.resolve()
root=Path(__file__).resolve().parent.parent
assert home==root/'.browser-test'/'app', 'Only the dedicated .browser-test app may be restarted'
home.mkdir(parents=True,exist_ok=True)
settings={'zoom':1,'font':'D2Coding','ui_font':'Pretendard','core':str(Path.home()/'.local/bin/herdr-core'),'telemetry':False,'sounds':{'enabled':False}}
(home/'settings.json').write_text(json.dumps(settings))
if not (home/'shared.json').exists():
 (home/'shared.json').write_text(json.dumps({'layout':'main','frame.welcomed':'finished','workspace.panels':['files','changes','preview','browser']}))

PAGE='''<!doctype html><html lang="ko"><meta charset="utf-8"><title>Browser 검증</title>
<style>body{font:16px -apple-system;margin:30px;color:#17442b;background:#f4f7f4}label,input,textarea{display:block;margin:8px 0}input,textarea,[contenteditable]{width:90%;padding:10px;border:1px solid #adc3af;border-radius:6px}[contenteditable]{min-height:70px;background:white}button{padding:10px 18px;background:#205e3b;color:white;border:0;border-radius:6px}iframe{width:95%;height:120px;border:1px solid #adc3af}#result{padding:12px;background:#e1eee3}</style>
<h1>브라우저 입력 테스트</h1><label>제목<input id="title" aria-label="제목"></label>
<div id="body" role="textbox" aria-label="본문" contenteditable="true"></div>
<p id="lazy-title" onmousedown="document.getElementById('lazy-editor').hidden=false">편집 활성화</p>
<div id="lazy-editor" aria-label="지연 편집기" contenteditable="plaintext-only" hidden></div>
<p id="model-editor" onpaste="event.preventDefault();this.textContent=event.clipboardData.getData('text/plain')">모델 편집기</p>
<p id="deferred-editor" onpaste="event.preventDefault();const text=event.clipboardData.getData('text/plain');setTimeout(()=>this.textContent=text,300)">지연 모델</p>
<div contenteditable="true" aria-label="거절 편집기" onpaste="event.preventDefault()"></div>
<iframe style="position:absolute;left:-10000px;width:20px;height:20px" src="/bridge"></iframe><p id="bridge-result">가상 편집기 모델</p>
<input id="photo-upload" type="file" accept="image/*" aria-label="사진 파일" hidden onchange="document.getElementById('upload-result').textContent='파일 확인: '+this.files[0].name">
<div id="image-paste" contenteditable="true" aria-label="사진 편집기" onpaste="if(event.clipboardData.files.length){event.preventDefault();const img=document.createElement('img');img.src=window.URL.createObjectURL(event.clipboardData.files[0]);img.alt='붙여넣은 사진';this.appendChild(img)}"></div><p id="upload-result"></p>
<label>비밀번호<input type="password" aria-label="비밀번호" value="fixture-password"></label>
<label>인증번호<input autocomplete="one-time-code" aria-label="인증번호" value="123456"></label>
<input autocomplete="section-login one-time-code" aria-label="복합 인증 필드" value="fixture-compound-secret">
<input type="text" autocomplete="current-password" aria-label="현재 암호" value="fixture-current-secret">
<input type="text" autocomplete="new-password" aria-label="새 암호" value="fixture-new-secret">
<input type="text" aria-label="인증번호 일반" value="fixture-plain-secret">
<input id="reveal" type="password" value="fixture-reveal-secret"><button onclick="document.getElementById('reveal').type='text'">비밀번호 보기</button>
<button id="save">초안 확인</button><p id="result">아직 입력 전</p>
<iframe src="/frame"></iframe><iframe src="CROSS_URL"></iframe>
<p id="persistent"></p><div style="height:1000px">스크롤 테스트</div>
<script>document.getElementById('persistent').textContent='저장 상태: '+localStorage.getItem('qa-persist')+' / '+document.cookie;
document.getElementById('save').onclick=()=>{document.getElementById('result').textContent='저장 확인: '+document.getElementById('title').value+' | '+document.getElementById('body').innerText;localStorage.setItem('qa-persist','survived');document.cookie='qa-cookie=survived; max-age=3600; path=/';};</script>'''

class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  if self.path.startswith('/secret-frame'):
   data='<html><body><input type="password" value="frame-private-value"></body></html>'
  elif self.path.startswith('/secrets'):
   data='''<html><meta charset="utf-8"><title>민감 입력 검증</title><body>
<input id="s0" aria-label="암호" value="ko-alt-value">
<input id="s1" name="passwd" aria-label="pw" value="passwd-value">
<input id="s2" name="pwd" value="pwd-value">
<input id="s3" autocomplete="section-pay cc-csc" value="csc-value">
<input id="s4" autocomplete="cc-number" value="card-value">
<input id="s5" name="pw" value="short-pw-value">
<input id="s6" type="password" value="ab"><input id="s7" type="password" value="abc">
<p>code abc end</p>
<input id="s8" type="password" value="replace-me">
<button onclick="const old=document.getElementById('s8');const el=old.cloneNode();el.type='text';el.id='shown';el.setAttribute('aria-label','보이는 값');old.replaceWith(el)">교체</button>
<input id="s9" type="password" value="a.*[x]$"><p>special a.*[x]$ end</p>
<input id="public" name="power" aria-label="일반 제목" value="ordinary-value">
<iframe src="/secret-frame"></iframe><p>frame-private-value</p>
</body></html>'''
  elif self.path.startswith('/frame'):
   data='<html lang="ko"><meta charset="utf-8"><button onclick="this.textContent=\'프레임 확인 완료\'">프레임 버튼</button></html>'
  elif self.path.startswith('/cross'):
   data='<html><body>cross-origin private frame</body></html>'
  elif self.path.startswith('/bridge'):
   data='<html><body><div contenteditable="true" aria-label="가상 입력 프레임" style="min-width:10px;min-height:10px" onpaste="event.preventDefault();parent.document.getElementById(\'bridge-result\').textContent=event.clipboardData.getData(\'text/plain\')"></div></body></html>'
  elif self.path.startswith('/hang'):
   time.sleep(18);return
  elif self.path.startswith('/wide-frame'):
   data='<html><style>body{margin:0}iframe{width:100%;height:200px;border:0}</style><iframe src="/wide-child"></iframe></html>'
  elif self.path.startswith('/wide-inner'):
   data='<html><style>body{margin:0}.scroll{width:100%;overflow-x:auto}.wide{width:1400px}</style><div class="scroll"><div class="wide">Internal overflow</div></div></html>'
  elif self.path.startswith('/wide-child'):
   data='<html><style>body{margin:0;min-width:1400px}</style><div>1400px iframe document</div></html>'
  elif self.path.startswith('/wide'):
   data='<html><meta charset="utf-8"><title>Wide layout</title><style>body{margin:0;min-width:1280px;background:#edf4f0}main{width:1280px;padding:30px;box-sizing:border-box}</style><main><h1>1280px 고정 폭 페이지</h1><p>패널 폭에 맞게 전체 가로 내용이 표시되어야 합니다.</p></main></html>'
  else:data=PAGE.replace('CROSS_URL',cross_url)
  encoded=data.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(encoded)));self.end_headers();self.wfile.write(encoded)
 def log_message(self,*args):pass

cross=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
cross_url=f'http://127.0.0.1:{cross.server_port}/cross'
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
url=f'http://127.0.0.1:{server.server_port}/'
for http in (cross,server):threading.Thread(target=http.serve_forever,daemon=True).start()
process=None
checks=[]

def call(payload):
 with socket.socket(socket.AF_UNIX) as client:
  client.settimeout(32);client.connect(str(home/'herdr-app.sock'))
  client.sendall((json.dumps(payload,ensure_ascii=False)+'\n').encode());reply=b''
  while b'\n' not in reply:
   chunk=client.recv(65536)
   if not chunk:break
   reply+=chunk
 return json.loads(reply)
def do(command,**kwargs):return call({'do':command,'args':kwargs})
def wait(predicate,label,timeout=20):
 deadline=time.monotonic()+timeout
 while time.monotonic()<deadline:
  try:
   value=predicate()
   if value:return value
  except (OSError,ValueError,KeyError):pass
  time.sleep(.1)
 raise AssertionError(label)
def passed(label):checks.append(label);print('PASS',label,flush=True)
def status():return call({'status':True})
def page():
 value=do('browser.read');assert 'error' not in value,value;return value
def read_ready():
 return wait(lambda:do('browser.read') if not (status().get('browser') or {}).get('loading',True) else None,'Browser did not finish loading')
def ref(data,label):return next(el['ref'] for el in data['elements'] if el['label']==label)
def start():
 global process
 env=os.environ.copy();env.update({'HERDR_APP_HOME':str(home),'HC_SESSION':args.session,'HERDR_APP_ID':'herdr-browser-test','HERDR_PREVIEW_TEST_CONTROL':'1','NO_COLOR':'1','PATH':str(Path.home()/'.local/bin')+':/opt/homebrew/bin:'+env['PATH']})
 log=(home.parent/'app.log').open('ab')
 process=subprocess.Popen([str(binary)],env=env,stdout=log,stderr=log)
 (home.parent/'app.pid').write_text(str(process.pid))
 wait(lambda:status(),'App did not open');call({'raise':True})

start()
try:
 space=wait(lambda:do('space.new',name='Browser 검증',folder=str(root/'examples/preview')).get('space'),'Test core layout did not connect')
 probe=do('terminal.open',argv=['/bin/sh','-c','if [ -n "${NO_COLOR:-}" ]; then printf "HERDR_COLOR_SUPPRESSED\\n"; else printf "HERDR_COLOR_ENABLED\\n"; fi'],place='tab',name='Color inheritance')
 def color_ready():
  output=subprocess.check_output([str(Path.home()/'.local/bin/hc'),'read',probe['terminal'],'--lines','8'],env={**os.environ,'HC_SESSION':args.session},text=True)
  return 'HERDR_COLOR_ENABLED' in output and 'HERDR_COLOR_SUPPRESSED' not in output
 wait(color_ready,'GUI terminal inherited NO_COLOR from an agent-started core')
 passed('New GUI terminals restore ANSI colors even when their core inherited NO_COLOR=1')
 do('browser.open',url=url+'wide')
 do('browser.view',zoom=1.0,fit=True,mobile=False)
 def fitted():
  current=status().get('browser') or {}
  if current.get('loading') or current.get('page_zoom',1)>=.95:return None
  data=do('browser.read')
  viewport=data.get('viewport') or {}
  return data if viewport.get('documentWidth',100000)<=viewport.get('width',0)+3 else None
 wait(fitted,'Fixed-width page did not fit')
 before=status()['browser']['page_zoom']
 do('layout.resize',id='panel',px=698);call({'raise':True})
 wait(lambda:status()['browser']['page_zoom']>before+.02 and fitted(),'Fit did not follow panel resize')
 passed('Fixed 1280px page fits narrow panel and follows divider width changes')
 do('browser.view',zoom=.75)
 assert status()['browser']['page_zoom']==.75 and not status()['browser']['fit_width']
 do('browser.view',zoom=1.0)
 assert status()['browser']['page_zoom']==1.0
 passed('Manual page zoom disables fitting and resets to 100%')
 do('layout.resize',id='panel',px=560)
 for nested in ['wide-frame','wide-inner']:
  do('browser.open',url=url+nested);do('browser.view',fit=True)
  def nested_fitted():
   if status()['browser']['loading']:return None
   data=do('browser.read');return data if data.get('viewport',{}).get('width',0)>=1390 and status()['browser']['page_zoom']<.5 else None
  wait(nested_fitted,'Nested content did not fit: '+nested)
 passed('Width-fit includes same-origin iframe documents and internal overflow')
 do('browser.open',url=url)
 do('browser.view',mobile=True,fit=True)
 data=read_ready();assert 'iPhone' in data['userAgent'] and status()['browser']['mobile']
 do('browser.view',mobile=False)
 data=read_ready();assert 'iPhone' not in data['userAgent'] and not status()['browser']['mobile']
 passed('Mobile and desktop site modes switch the actual WebKit user agent')
 do('layout.resize',id='panel',px=560)
 do('browser.open',url=url)
 data=wait(lambda: read_ready() if (status().get('browser') or {}).get('visible') else None,'Browser is not visible')
 assert data['title']=='Browser 검증' and data['source']=='browser-page'
 passed('Browser opens an HTTP page and returns readable text/refs')
 assert any(f.get('inaccessible') for f in data['frames'])
 assert any('프레임 버튼' in f.get('text','') for f in data['frames'])
 passed('Same-origin frames work; cross-origin frames stay isolated')
 secrets=[e for e in data['elements'] if e.get('manual')]
 assert len(secrets)==7 and all('value' not in e for e in secrets)
 assert 'error' in do('browser.fill',ref=ref(data,'비밀번호'),text='not-used')
 assert 'error' in do('browser.fill',ref=ref(data,'인증번호'),text='not-used')
 passed('Password and OTP values are redacted and automatic fill is refused')
 for label in ['복합 인증 필드','현재 암호','새 암호','인증번호 일반']:
  assert 'error' in do('browser.fill',ref=ref(data,label),text='must-not-be-entered')
 assert do('browser.click',ref=ref(data,'비밀번호 보기'))['ok']
 data=page();revealed=next(e for e in data['elements'] if e.get('id')=='reveal')
 assert revealed.get('manual') and 'value' not in revealed
 assert 'error' in do('browser.fill',ref=revealed['ref'],text='must-not-be-entered')
 assert 'fixture-reveal-secret' not in json.dumps(data)
 passed('Compound OTP, password autocomplete, labelled OTP and revealed password remain manual')
 do('browser.open',url=url+'secrets')
 data=read_ready()
 for ident in ['s'+str(i) for i in range(10)]:
  field=next(e for e in data['elements'] if e.get('id')==ident)
  assert field.get('manual') and 'value' not in field,field
  for command in ['browser.fill','browser.click']:
   assert 'error' in do(command,ref=field['ref'],**({'text':'must-not-enter'} if command=='browser.fill' else {}))
 public=next(e for e in data['elements'] if e.get('id')=='public')
 assert not public.get('manual') and public['value']=='ordinary-value'
 passed('Korean password aliases, passwd/pwd/pw and payment autocomplete are manual; ordinary names stay editable')
 text='\n'.join(f.get('text','') for f in data['frames'])
 assert 'code [수동 입력] end' in text and 'code [수동 입력]c end' not in text,text
 assert 'special [수동 입력] end' in text and 'frame-private-value' not in text,text
 passed('Overlapping and regex-special secrets redact in one pass across accessible frames')
 assert do('browser.click',ref=ref(data,'교체'))['ok']
 data=page();field=next(e for e in data['elements'] if e.get('id')=='shown')
 assert field.get('manual') and 'value' not in field
 assert 'replace-me' not in json.dumps(data)
 assert 'error' in do('browser.fill',ref=field['ref'],text='must-not-enter')
 assert 'error' in do('browser.click',ref=field['ref'])
 passed('Replacement-node password reveal retains page-lifetime protection')
 do('browser.open',url=url)
 data=read_ready()
 assert next(e for e in data['elements'] if e.get('id')=='title').get('manual') is not True
 assert do('browser.click',ref=ref(data,'편집 활성화'))['ok']
 data=page()
 assert do('browser.fill',ref=ref(data,'지연 편집기'),text='문단 클릭으로 활성화한 편집기')['ok']
 assert any('문단 클릭으로 활성화한 편집기' in f.get('text','') for f in page()['frames'])
 passed('Paragraph mouse events activate a rich editor and plaintext-only contenteditable accepts text')
 data=page()
 assert do('browser.fill',ref=ref(data,'모델 편집기'),text='모델에 반영된 붙여넣기')['ok']
 assert any('모델에 반영된 붙여넣기' in f.get('text','') for f in page()['frames'])
 passed('Custom paragraph editor receives plain-text paste through its document model')
 data=page()
 delayed=do('browser.fill',ref=ref(data,'지연 모델'),text='지연 반영 완료')
 assert delayed.get('applied') and delayed.get('persisted') is False
 data=page();assert do('browser.fill',ref=ref(data,'가상 입력 프레임'),text='상위 문서에 반영된 가상 편집기')['applied']
 data=page();rejected=do('browser.fill',ref=ref(data,'거절 편집기'),text='MUST-NOT-STICK')
 assert rejected.get('kind')=='unverified' and not rejected.get('applied') and 'ok' not in rejected,rejected
 passed('Delayed/virtual editor model changes are verified; cancelled text paste is not success')
 data=page()
 upload=home.parent/'upload-fixture.png'
 import zlib
 def png_chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
 upload.write_bytes(b'\x89PNG\r\n\x1a\n'+png_chunk(b'IHDR',struct.pack('>IIBBBBB',12,12,8,2,0,0,0))+png_chunk(b'IDAT',zlib.compress((b'\0'+b'\x55\x99\x66'*12)*12))+png_chunk(b'IEND',b''))
 assert do('browser.upload',ref=ref(data,'사진 파일'),path=str(upload))['ok']
 assert do('browser.upload',ref=ref(data,'사진 편집기'),path=str(upload))['ok']
 data=wait(lambda:page() if any(e.get('image',{}).get('width')==12 for e in page()['elements']) else None,'Uploaded image did not load')
 assert any('파일 확인: upload-fixture.png' in f.get('text','') for f in data['frames'])
 assert 'error' in do('browser.upload',ref=ref(data,'사진 파일'),path=str(home/'settings.json'))
 passed('Explicit image files work through hidden file inputs and editor paste; non-images are rejected')
 data=page()
 rejected=do('browser.upload',ref=ref(data,'거절 편집기'),path=str(upload))
 assert rejected.get('kind')=='unverified' and not rejected.get('applied') and 'ok' not in rejected,rejected
 passed('Cancelled image paste is not reported as an applied upload')
 data=page()
 title='한글 제목 <script>literal</script> "인용"'
 body='한글 본문 첫째 줄\n둘째 줄 — 클릭과 입력 검증'
 assert do('browser.fill',ref=ref(data,'제목'),text='temporary')['ok']
 assert do('browser.fill',ref=ref(data,'제목'),text='')['ok']
 assert do('browser.fill',ref=ref(data,'제목'),text=title)['ok']
 assert do('browser.fill',ref=ref(data,'본문'),text=body)['ok']
 assert do('browser.click',ref=ref(data,'초안 확인'))['ok']
 data=page();text='\n'.join(f.get('text','') for f in data['frames'])
 assert title in text and '한글 본문 첫째 줄' in text and '둘째 줄' in text,text
 passed('Korean title and contenteditable body are filled and click handlers run')
 stale=ref(data,'초안 확인');page()
 assert 'error' in do('browser.click',ref=stale)
 passed('Stale element refs fail instead of clicking a different element')
 data=page();assert do('browser.click',ref=ref(data,'프레임 버튼'))['ok']
 assert any('프레임 확인 완료' in f.get('text','') for f in page()['frames'])
 passed('Same-origin iframe buttons can be operated')
 image=home.parent/'browser-native.png'
 captured=do('browser.screenshot',path=str(image));assert captured.get('path')==str(image),captured
 raw=image.read_bytes();assert raw[:8]==b'\x89PNG\r\n\x1a\n'
 width,height=struct.unpack('>II',raw[16:24]);assert width>200 and height>200
 passed(f'Native WebKit screenshot includes the page ({width}x{height})')
 do('preview.open',target=str(root/'examples/preview/index.html'))
 wait(lambda:not status()['browser']['visible'] and status()['preview']['visible'],'Browser and Preview overlap')
 do('browser.open')
 wait(lambda:status()['browser']['visible'],'Browser did not return')
 assert any(title in f.get('text','') for f in page()['frames'])
 passed('Preview and Browser remain independent and preserve editing state')
 do('browser.scroll',y=500)
 assert do('browser.scroll',y=0)['y']>0
 do('browser.scroll',y=-500)
 passed('Browser scroll commands operate the page')
 do('browser.reload');data=read_ready()
 assert any('저장 상태: survived / qa-cookie=survived' in f.get('text','') for f in data['frames'])
 passed('Browser cookies and local storage survive reload')
 do('browser.view',zoom=.9,mobile=True)
 read_ready()
 do('app.quit');process.wait(timeout=10)
 start();do('browser.open');data=read_ready()
 assert any('저장 상태: survived / qa-cookie=survived' in f.get('text','') for f in data['frames'])
 assert status()['browser']['mobile'] and status()['browser']['page_zoom']==.9 and not status()['browser']['fit_width']
 passed('Persistent Browser profile survives app restart')
 passed('Page zoom, fit choice and mobile mode survive app restart')
 do('browser.view',mobile=False,fit=True)
 for s in call({'ls':True})['spaces']:
  if s['name']=='Browser 검증':do('space.close',id=s['id'],confirmed=True)
 do('browser.open',url=url);data=read_ready()
 old_ref=ref(data,'제목')
 do('browser.open',url=url+'hang')
 wait(lambda:(status().get('browser') or {}).get('error'),'Navigation timeout was not reported',timeout=22)
 assert do('browser.read').get('kind')=='navigation_failed'
 assert do('browser.fill',ref=old_ref,text='MUST-NOT-EDIT-OLD-PAGE').get('kind')=='navigation_failed'
 do('browser.open',url=url);read_ready()
 passed('Timed-out navigation blocks stale reads and writes until successful reload')
 assert not status().get('errors'),status().get('errors')
 report={'all_passed':True,'count':len(checks),'checks':checks,'note':'Tests only fill and click local fixtures. Naver login/editor/publishing are not exercised.'}
 (home.parent/'smoke-results.json').write_text(json.dumps(report,ensure_ascii=False,indent=2))
 print(json.dumps({'passed':len(checks),'report':str(home.parent/'smoke-results.json')},ensure_ascii=False))
finally:
 for http in (cross,server):http.shutdown()
