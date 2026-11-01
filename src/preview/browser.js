// Local host -> page only. Page contents never get a Herdr/native bridge.
function performBrowserAction(action) {
  const visible = el => {
    const style = el.ownerDocument.defaultView.getComputedStyle(el);
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0 && style.display !== 'none' && style.visibility !== 'hidden' && style.opacity !== '0';
  };
  const seenSecrets = window.__herdrBrowserSecrets ||= new WeakSet();
  // Kept only inside this document's JS world, never persisted. A reveal widget
  // may replace the input entirely, so node identity alone is insufficient.
  const secretValues = window.__herdrBrowserSecretValues ||= new Set();
  const accessibleName = el => [el.getAttribute('aria-label'), el.getAttribute('placeholder'), el.title, el.name, el.id,
    ...Array.from(el.labels || [], label => label.innerText),
    ...(el.getAttribute('aria-labelledby') || '').split(/\s+/).map(id => el.ownerDocument.getElementById(id)?.innerText)].filter(Boolean).join(' ');
  const secret = el => {
    if (!el.matches('input,textarea,[contenteditable],[role="textbox"]')) return false;
    const value = el.value || el.innerText || '';
    const tokens = (el.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
    const sensitive = seenSecrets.has(el) || (value && secretValues.has(value)) || el.type === 'password'
      || tokens.some(t => ['one-time-code','current-password','new-password','cc-csc','cc-number'].includes(t))
      || /비밀\s*번호|암호|인증\s*번호|보안\s*코드|(?:^|[^a-z0-9])(?:otp|password|passwd|pwd|pw|passcode|pin)(?:$|[^a-z0-9])|verification[\s_-]*code|one[\s_-]*time[\s_-]*(?:code|password)/i.test(accessibleName(el));
    if (sensitive) {
      seenSecrets.add(el);
      if (value) secretValues.add(value);
    }
    return sensitive;
  };
  const pending = window.__herdrBrowserPending ||= new Map();
  if (action.op === 'verify' || action.op === 'cancel') {
    const item = pending.get(action.token);
    if (!item) throw new Error('페이지가 바뀌어 입력 반영을 확인할 수 없습니다.');
    if (action.op === 'cancel') { pending.delete(action.token); return {cancelled:true}; }
    if (!item.el.isConnected || secret(item.el)) { pending.delete(action.token); throw new Error('입력 대상이 바뀌었습니다.'); }
    if (!item.check()) return {pending:action.token,accepted:true,applied:false};
    pending.delete(action.token);
    return {...item.result,ok:true,accepted:true,applied:true,persisted:false};
  }
  if (action.op === 'read') {
    const token = 'r' + Math.random().toString(36).slice(2);
    const refs = new Map(), frames = [], elements = [];
    const limit = Math.max(1000, Math.min(Number(action.limit) || 16000, 40000));
    let count = 0, textLeft = limit;
    // Discover secrets in every accessible frame before emitting any text;
    // child-frame secrets can also be echoed in the parent document.
    const docs = [];
    function collect(doc, depth) {
      if (depth > 8 || docs.length >= 30 || docs.includes(doc)) return;
      docs.push(doc);
      for (const el of doc.querySelectorAll('input,textarea,[contenteditable],[role="textbox"]')) secret(el);
      for (const frame of doc.querySelectorAll('iframe,frame')) {
        try { if (frame.contentDocument) collect(frame.contentDocument, depth+1); } catch {}
      }
    }
    collect(document, 0);
    const values = [...secretValues].sort((a,b) => b.length-a.length);
    const pattern = values.length ? new RegExp(values.map(value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|'), 'g') : null;
    // One replacement pass prevents overlapping values from leaking suffixes
    // and prevents the replacement marker itself from being redacted again.
    const redact = value => pattern ? String(value).replace(pattern, () => '[수동 입력]') : String(value);
    function visit(doc, depth) {
      if (depth > 8 || frames.length >= 30) return;
      const text = redact(doc.body?.innerText || '').slice(0, textLeft);
      textLeft -= text.length;
      frames.push({url: doc.URL, title: doc.title, text});
      for (const el of doc.querySelectorAll('a[href],button,input,textarea,select,[contenteditable],[role="button"],[role="textbox"],[data-placeholder],p,img')) {
        if (elements.length >= 500 || (!visible(el) && el.type !== 'file')) continue;
        const ref = `${token}:${++count}`;
        refs.set(ref, el);
        const manual = secret(el);
        const label = el.getAttribute('aria-label') || el.labels?.[0]?.innerText || el.getAttribute('placeholder') || (!manual && el.innerText) || el.title || el.name || '';
        const item = {ref, tag: el.tagName.toLowerCase(), role: el.getAttribute('role'), type: el.type || '', label: redact(label).trim().slice(0,180), frame: doc.URL, disabled: !!el.disabled};
        if (el.id) item.id = el.id;
        if (typeof el.className === 'string' && el.className) item.classes = el.className.slice(0,250);
        if (el.href) item.href = el.href.slice(0,2048);
        if (el.tagName === 'IMG') {
          const rect = el.getBoundingClientRect();
          item.image = {src:el.currentSrc||el.src,alt:el.alt,width:el.naturalWidth,height:el.naturalHeight,displayWidth:rect.width,displayHeight:rect.height};
        }
        if (el.type === 'file') { item.accept = el.accept; item.hidden = !visible(el); }
        if (manual) item.manual = true;
        else if ('value' in el) item.value = redact(el.value).slice(0,1000);
        if (el.isContentEditable) item.editable = true;
        if (el.isContentEditable || el.tagName === 'P') {
          item.context = [];
          for (let parent = el.parentElement; parent && item.context.length < 4; parent = parent.parentElement) item.context.push({tag:parent.tagName.toLowerCase(),id:parent.id,classes:typeof parent.className === 'string' ? parent.className.slice(0,180) : ''});
        }
        elements.push(item);
      }
      for (const frame of doc.querySelectorAll('iframe,frame')) {
        if (frames.length >= 30) break;
        try {
          if (frame.contentDocument) visit(frame.contentDocument, depth+1);
          else frames.push({url: frame.src, inaccessible: true});
        } catch { frames.push({url: frame.src, inaccessible: true}); }
      }
    }
    visit(document, 0);
    window.__herdrBrowserRefs = refs;
    return {source: 'browser-page', untrusted: true, url: location.href, title: document.title, ready: document.readyState, viewport: {width: innerWidth,height: innerHeight,documentWidth:Math.max(document.documentElement.scrollWidth,document.body?.scrollWidth||0),devicePixelRatio},userAgent:navigator.userAgent, frames, elements, truncated: textLeft <= 0 || elements.length >= 500};
  }
  let el = null;
  if (action.ref) {
    el = window.__herdrBrowserRefs?.get(action.ref);
    if (!el || !el.isConnected) throw new Error('화면이 바뀌었습니다. browser.read로 다시 확인하세요.');
    if (secret(el)) throw new Error('비밀번호와 인증번호는 Browser에서 직접 입력하세요.');
    if (el.disabled || el.readOnly) throw new Error('입력하거나 클릭할 수 없는 요소입니다.');
  }
  if (action.op === 'scroll') {
    const x = Number(action.x) || 0, y = Number(action.y) || 0;
    (el || window).scrollBy({left:x,top:y,behavior:'instant'});
    return {ok:true, url:location.href, x:scrollX,y:scrollY};
  }
  if (!el) throw new Error('browser.read에서 받은 ref가 필요합니다.');
  el.scrollIntoView({block:'center',inline:'nearest'});
  el.focus();
  const normal = value => String(value).replace(/\s+/g,' ').trim();
  const documents = [];
  for (let win = el.ownerDocument.defaultView; win && documents.length < 8;) {
    try { documents.push(win.document); if (win === win.parent) break; win = win.parent; }
    catch { break; }
  }
  const visibleInput = (() => {
    let node = el;
    while (node) {
      const win = node.ownerDocument.defaultView, style = win.getComputedStyle(node), rect = node.getBoundingClientRect();
      if (style.display === 'none' || style.visibility === 'hidden' || style.opacity === '0' || rect.width < 1 || rect.height < 1 || rect.right <= 0 || rect.bottom <= 0 || rect.left >= win.innerWidth || rect.top >= win.innerHeight) return false;
      node = node.parentElement || win.frameElement;
    }
    return true;
  })();
  const observe = (result, check) => {
    for (const [key,item] of pending) if (Date.now()-item.created > 30000) pending.delete(key);
    if (pending.size >= 20) throw new Error('대기 중인 입력 확인이 너무 많습니다.');
    const token = 'p'+Math.random().toString(36).slice(2);
    pending.set(token,{el,result,check,created:Date.now()});
    return {pending:token,accepted:true,applied:false};
  };
  if (action.op === 'click') {
    const rect = el.getBoundingClientRect(), win = el.ownerDocument.defaultView;
    const pointer = {bubbles:true,cancelable:true,view:win,clientX:rect.left+Math.min(rect.width/2,20),clientY:rect.top+rect.height/2,button:0};
    el.dispatchEvent(new win.MouseEvent('mousedown',{...pointer,buttons:1}));
    el.dispatchEvent(new win.MouseEvent('mouseup',pointer));
    el.click();
    return {ok:true, ref:action.ref, action:'click'};
  }
  if (action.op === 'upload') {
    const before = new Map(documents.flatMap(doc => [...doc.images].map(img => [img,img.currentSrc||img.src])));
    const file = action.file, win = el.ownerDocument.defaultView;
    const bytes = Uint8Array.from(atob(file.data), c => c.charCodeAt(0));
    const transfer = new win.DataTransfer();
    transfer.items.add(new win.File([bytes],file.name,{type:file.mime}));
    if (el.tagName === 'INPUT' && el.type === 'file') {
      el.files = transfer.files;
      el.dispatchEvent(new win.Event('input',{bubbles:true}));
      el.dispatchEvent(new win.Event('change',{bubbles:true}));
    } else if (el.dispatchEvent(new win.ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:transfer}))) {
      throw new Error('이 편집기는 이미지 붙여넣기를 처리하지 않습니다. 파일 입력란을 선택하세요.');
    }
    return observe({action:'upload',name:file.name,bytes:bytes.length,verification:el.type==='file'?'file-selection':'editor-image'}, () =>
      el.type === 'file' ? [...el.files].some(f => f.name===file.name && f.size===bytes.length && f.type===file.mime)
      : documents.some(doc => [...doc.images].some(img => img.isConnected && img.naturalWidth>0 && before.get(img)!==(img.currentSrc||img.src))));
  }
  if (action.op === 'fill') {
    const text = String(action.text ?? '');
    if (text.length > 200000) throw new Error('본문은 한 번에 200,000자 이내로 입력하세요.');
    const win = el.ownerDocument.defaultView;
    const wanted = normal(text);
    const occurrences = doc => wanted ? normal(doc.body?.innerText||'').split(wanted).length-1 : 0;
    const before = new Map(documents.map(doc => [doc,occurrences(doc)]));
    const paste = () => {
      const transfer = new win.DataTransfer(); transfer.setData('text/plain', text);
      return !el.dispatchEvent(new win.ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:transfer}));
    };
    if (!el.isContentEditable && el.tagName !== 'INPUT' && el.tagName !== 'TEXTAREA') {
      if (!paste()) throw new Error('이 요소는 붙여넣기를 처리하는 편집기가 아닙니다.');
    } else if (el.isContentEditable) {
      const doc = el.ownerDocument, selection = doc.defaultView.getSelection(), range = doc.createRange();
      range.selectNodeContents(el); selection.removeAllRanges(); selection.addRange(range);
      if (!paste() && !doc.execCommand('insertText', false, text)) throw new Error('이 편집기는 일반 텍스트 입력을 지원하지 않습니다.');
    } else if (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA') {
      const win = el.ownerDocument.defaultView;
      const prototype = el.tagName === 'INPUT' ? win.HTMLInputElement.prototype : win.HTMLTextAreaElement.prototype;
      Object.getOwnPropertyDescriptor(prototype,'value').set.call(el,text);
      el.dispatchEvent(new win.InputEvent('input',{bubbles:true,inputType:'insertText',data:text}));
      el.dispatchEvent(new win.Event('change',{bubbles:true}));
    } else { throw new Error('입력창이나 본문 편집기를 선택하세요.'); }
    return observe({ref:action.ref,action:'fill',characters:text.length,verification:'document-text'}, () =>
      (el.tagName==='INPUT'||el.tagName==='TEXTAREA') ? el.value===text
      : (visibleInput && normal(el.innerText||'')===wanted) || documents.some(doc => (doc!==el.ownerDocument || visibleInput) && occurrences(doc)>before.get(doc)));
  }
  throw new Error('지원하지 않는 Browser 작업입니다.');
}
