use super::macos::Preview;
use serde_json::{Value, json};
use std::sync::mpsc::Sender;

pub(super) const MOBILE_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1";

pub(super) fn set_mobile_agent(view: &wry::WebView, mobile: bool) {
    use wry::WebViewExtMacOS;
    let agent = mobile.then(|| objc2_foundation::NSString::from_str(MOBILE_AGENT));
    unsafe {
        view.webview().setCustomUserAgent(agent.as_deref());
    }
}

pub(super) fn device_url(value: &str, mobile: bool) -> String {
    let Ok(url) = url::Url::parse(value) else {
        return value.to_owned();
    };
    let host = url.host_str().unwrap_or("");
    let home = matches!(
        url.path(),
        "/" | "/BlogHome.naver"
            | "/Recommendation.naver"
            | "/FeedList.naver"
            | "/SectionMain.naver"
    );
    if mobile && home && matches!(host, "section.blog.naver.com" | "blog.naver.com") {
        return "https://m.blog.naver.com/".into();
    }
    if !mobile && home && host == "m.blog.naver.com" {
        return "https://section.blog.naver.com/BlogHome.naver".into();
    }
    value.to_owned()
}

pub(super) fn browser_address(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("웹사이트 주소나 검색어를 입력하세요.".into());
    }
    if value.starts_with('/') || value.starts_with("~/") {
        return Err("로컬 파일은 Preview에서 열어주세요.".into());
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return Ok(value.into());
    }
    if value.contains("://")
        || value.starts_with("javascript:")
        || value.starts_with("data:")
        || value.starts_with("file:")
    {
        return Err("Browser에서는 http·https 웹사이트를 열 수 있습니다.".into());
    }
    if value.starts_with("localhost:")
        || value.starts_with("127.0.0.1:")
        || value.starts_with("[::1]:")
    {
        return Ok(format!("http://{value}"));
    }
    if !value.contains(char::is_whitespace) && value.contains('.') && !value.starts_with('/') {
        return Ok(format!("https://{value}"));
    }
    let mut url = url::Url::parse("https://search.naver.com/search.naver").unwrap();
    url.query_pairs_mut().append_pair("query", value);
    Ok(url.into())
}

impl Preview {
    // Isolated debug harness only; delivers events directly to this app's view,
    // never to the desktop or any other application.
    #[cfg(debug_assertions)]
    pub fn test_native_focus(&self, action: &str) -> Value {
        use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
        use objc2_foundation::NSPoint;
        use wry::WebViewExtMacOS;
        let browser = self.browser.borrow();
        let Some(view) = &browser.view else {
            return json!({"error":"No browser page"});
        };
        unsafe {
            let web = view.webview();
            let parent = web.superview().unwrap();
            let window = web.window().unwrap();
            match action {
                "browser" => {
                    let _ = view.focus();
                }
                "parent-click" => {
                    let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
                        NSEventType::LeftMouseDown, NSPoint::new(250., 200.), NSEventModifierFlags::empty(), 0., window.windowNumber(), None, 1, 1, 1.);
                    let _: () = objc2::msg_send![&parent, performSelector: objc2::sel!(mouseDown:), withObject: event.as_deref().unwrap(), afterDelay: 0.0_f64];
                }
                "type" => {
                    let text = objc2_foundation::NSString::from_str("focus_verify");
                    let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                        NSEventType::KeyDown, NSPoint::new(250., 200.), NSEventModifierFlags::empty(), 0., window.windowNumber(), None, &text, &text, false, 0);
                    let _: () = objc2::msg_send![&window, performSelector: objc2::sel!(sendEvent:), withObject: event.as_deref().unwrap(), afterDelay: 0.0_f64];
                }
                _ => {}
            }
            let responder = window.firstResponder();
            json!({"parent":responder.as_ref().is_some_and(|r| std::ptr::eq(r.as_ref() as *const objc2::runtime::AnyObject, parent.as_ref() as *const objc2::runtime::AnyObject)),"responder":responder.map(|r| format!("{r:?}"))})
        }
    }

    pub fn automate(
        &self,
        command: &str,
        args: &Value,
        reply: Sender<Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.is_browser {
            let _ = reply.send(json!({"error":"Use the Browser panel."}));
            return;
        }
        if self.loading {
            let _ = reply.send(
                json!({"error":"Browser is loading. Check status and retry after it finishes."}),
            );
            return;
        }
        if let Some(error) = &self.error {
            let _ = reply.send(json!({"error":error,"kind":"navigation_failed"}));
            return;
        }
        if command == "browser.screenshot" {
            self.screenshot(args, reply);
            return;
        }
        let op = command.strip_prefix("browser.").unwrap_or("");
        if !matches!(op, "read" | "click" | "fill" | "upload" | "scroll") {
            let _ = reply.send(json!({"error":"Unsupported Browser operation"}));
            return;
        }
        let mut action = args.as_object().cloned().unwrap_or_default();
        if op == "upload" {
            match image_upload(args) {
                Ok(file) => {
                    action.insert("file".into(), file);
                }
                Err(error) => {
                    let _ = reply.send(json!({"error":error}));
                    return;
                }
            }
        }
        action.insert("op".into(), json!(op));
        let generation = self.generation;
        let epoch = self.automation_epoch;
        let guarded = matches!(op, "read" | "fill" | "upload");
        cx.spawn(async move |this, cx| {
            let mut action = Value::Object(action);
            let mut token = None;
            for attempt in 0..=25 {
                if attempt > 0 {
                    cx.background_executor().timer(std::time::Duration::from_millis(200)).await;
                }
                let (sender, receiver) = async_channel::bounded(1);
                let timeout_sender = sender.clone();
                let timeout = cx.background_executor().timer(std::time::Duration::from_secs(3));
                cx.background_executor().spawn(async move {
                    timeout.await;
                    let _ = timeout_sender.try_send(json!({"error":"WebKit did not answer; inspect the page before retrying."}));
                }).detach();
                let started = this.update(cx, |page, _| -> Result<(), String> {
                    if page.generation != generation || page.automation_epoch != epoch || page.loading { return Err("Browser page or tab changed; read it again before acting.".into()); }
                    if let Some(error) = &page.error { return Err(error.clone()); }
                    let browser = page.browser.borrow();
                    let view = browser.view.as_ref().ok_or("Open Browser first.")?;
                    let expected = view.url().map_err(|e| e.to_string())?;
                    let script = browser_script(&action, &expected);
                    view.evaluate_script_with_callback(&script, move |raw| {
                        let _ = sender.try_send(parse_response(&raw));
                    }).map_err(|e| e.to_string())
                });
                if !matches!(started, Ok(Ok(()))) {
                    let error = match started { Ok(Err(e)) => e, _ => "Browser view closed.".into() };
                    let _ = reply.send(json!({"error":error,"accepted":token.is_some(),"applied":false}));
                    return;
                }
                let value = receiver.recv().await.unwrap_or_else(|_| json!({"error":"Browser callback closed"}));
                if guarded {
                    let valid = this.update(cx, |page, _| page.generation == generation && page.automation_epoch == epoch && !page.loading && page.error.is_none()).unwrap_or(false);
                    if !valid { let _ = reply.send(json!({"error":"Browser navigated while verifying the result.","accepted":token.is_some(),"applied":false})); return; }
                }
                if let Some(pending) = value["pending"].as_str() {
                    token = Some(pending.to_owned());
                    action = json!({"op":"verify","token":pending});
                } else {
                    let _ = reply.send(value);
                    return;
                }
            }
            if let Some(token) = token {
                let _ = this.update(cx, |page, _| {
                    if page.generation == generation && page.automation_epoch == epoch {
                        if let Some(view) = &page.browser.borrow().view {
                            if let Ok(url) = view.url() { let _ = view.evaluate_script(&browser_script(&json!({"op":"cancel","token":token}), &url)); }
                        }
                    }
                });
            }
            let _ = reply.send(json!({"error":"입력은 접수됐지만 실제 문서 반영을 확인하지 못했습니다. 중복 입력하지 말고 browser.read로 확인하세요.","kind":"unverified","accepted":true,"applied":false}));
        }).detach();
    }

    fn screenshot(&self, args: &Value, reply: Sender<Value>) {
        use objc2::AllocAnyThread;
        use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
        use objc2_foundation::{NSDictionary, NSError};
        use wry::WebViewExtMacOS;
        let path = args["path"].as_str().map(std::path::PathBuf::from);
        let Some(path) = path.filter(|p| {
            p.is_absolute() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png"))
        }) else {
            let _ = reply.send(json!({"error":"Use an absolute .png output path."}));
            return;
        };
        let browser = self.browser.borrow();
        let Some(view) = &browser.view else {
            let _ = reply.send(json!({"error":"Open Browser first."}));
            return;
        };
        if !browser.shown {
            let _ =
                reply.send(json!({"error":"Show the Browser panel before taking a screenshot."}));
            return;
        }
        let callback = block2::RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let result = (|| -> Result<Value, String> {
                if !error.is_null() {
                    return Err(unsafe { &*error }.localizedDescription().to_string());
                }
                let image = unsafe { image.as_ref() }.ok_or("WebKit returned no image")?;
                let tiff = image
                    .TIFFRepresentation()
                    .ok_or("Cannot encode browser image")?;
                let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
                    .ok_or("Cannot decode browser image")?;
                // Empty properties have the expected dictionary value types.
                let png = unsafe {
                    bitmap.representationUsingType_properties(
                        NSBitmapImageFileType::PNG,
                        &NSDictionary::new(),
                    )
                }
                .ok_or("Cannot encode PNG")?;
                crate::persist::write_atomic(&path, png.to_vec()).map_err(|e| e.to_string())?;
                Ok(json!({"path":path,"captured":"browser-viewport"}))
            })();
            let _ = reply.send(result.unwrap_or_else(|error| json!({"error":error})));
        });
        // The callback is retained by WebKit and runs on its main-thread view.
        unsafe {
            view.webview()
                .takeSnapshotWithConfiguration_completionHandler(None, &callback);
        }
    }
}

fn parse_response(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| match v {
            Value::String(s) => serde_json::from_str(&s).ok(),
            Value::Object(_) => Some(v),
            _ => None,
        })
        .unwrap_or_else(
            || json!({"error":"The page changed while reading it. Run browser.read again."}),
        )
}

fn browser_script(action: &Value, expected: &str) -> String {
    let action = serde_json::to_string(action).unwrap();
    let expected = serde_json::to_string(expected).unwrap();
    format!(
        "{}\n(()=>{{try{{const actual=new URL(location.href),expected=new URL({expected});actual.hash='';expected.hash='';if(actual.href!==expected.href)throw new Error('Browser document does not match the current page.');return JSON.stringify(performBrowserAction({action}))}}catch(e){{return JSON.stringify({{error:String(e.message||e)}})}}}})()",
        include_str!("browser.js")
    )
}

fn image_upload(args: &Value) -> Result<Value, String> {
    use base64::Engine as _;
    let path = std::path::Path::new(
        args["path"]
            .as_str()
            .ok_or("Provide an absolute image path")?,
    );
    if !path.is_absolute() {
        return Err("Provide an absolute image path".into());
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > 20 * 1024 * 1024 {
        return Err("Select an image file up to 20 MiB".into());
    }
    use std::io::Read as _;
    let mut bytes = Vec::new();
    file.take(20 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("Select an image file up to 20 MiB".into());
    }
    let mime = match image::guess_format(&bytes).map_err(|_| "Select a PNG, JPEG or WebP image")? {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err("Select a PNG, JPEG or WebP image".into()),
    };
    Ok(
        json!({"name":path.file_name().unwrap().to_string_lossy(),"mime":mime,"data":base64::engine::general_purpose::STANDARD.encode(bytes)}),
    )
}

#[cfg(test)]
mod tests {
    use super::browser_address;
    #[test]
    fn normalizes_web_addresses_and_search_without_host_schemes() {
        assert_eq!(
            browser_address("blog.naver.com").unwrap(),
            "https://blog.naver.com"
        );
        assert_eq!(
            browser_address("localhost:3000").unwrap(),
            "http://localhost:3000"
        );
        assert!(
            browser_address("네이버 블로그")
                .unwrap()
                .contains("search.naver.com")
        );
        for value in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hello",
            "herdr://command",
        ] {
            assert!(browser_address(value).is_err());
        }
    }
}
