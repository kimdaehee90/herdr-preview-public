function measureHerdrPageWidth() {
  let content = innerWidth, budget = 12000;
  function visit(doc, depth, offset) {
    if (depth > 8 || !doc.documentElement || budget <= 0) return;
    const win = doc.defaultView;
    content = Math.max(content, offset + doc.documentElement.scrollWidth, offset + (doc.body?.scrollWidth || 0));
    for (const el of doc.querySelectorAll('*')) {
      if (--budget <= 0) break;
      if (el.clientWidth > 1 && el.scrollWidth > el.clientWidth + 2) {
        const style = win.getComputedStyle(el), rect = el.getBoundingClientRect();
        if (style.display !== 'none' && style.visibility !== 'hidden' && rect.height > 0 && /auto|scroll|hidden|clip/.test(style.overflowX))
          content = Math.max(content, offset + Math.max(0, rect.left) + el.scrollWidth);
      }
      if (el.tagName === 'IFRAME' || el.tagName === 'FRAME') {
        try {
          const rect = el.getBoundingClientRect();
          if (rect.width > 1 && rect.height > 1 && el.contentDocument) visit(el.contentDocument, depth + 1, offset + Math.max(0, rect.left));
        } catch { /* Cross-origin frames remain isolated. */ }
      }
    }
  }
  visit(document, 0, 0);
  return JSON.stringify({available: innerWidth, content});
}
measureHerdrPageWidth();
