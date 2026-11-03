use super::server::Server;
use super::document::Document;
#[path = "document_ui.rs"]
mod document_ui;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use crate::{mods, settings};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_base::input::{Input, InputEvent, InputState};
use notify::Watcher as _;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

enum Event {
    Loading(String),
    Loaded(String),
    Changed,
    Popup(String),
    Title(String),
    Resized,
    DocumentReady(u64, Result<(Arc<Document>, usize, PathBuf), String>),
    PageReady(u64, Result<(usize, PathBuf), String>),
    FitMeasured(u64, f64, f64, u8),
}

pub(super) struct Browser {
    pub(super) view: Option<wry::WebView>,
    allowed: bool,
    pub(super) shown: bool,
    bounds: Option<Bounds<Pixels>>,
    events: async_channel::Sender<Event>,
}

impl Browser {
    fn hide(&mut self) {
        if self.shown {
            if let Some(view) = &self.view {
                let _ = view.focus_parent();
                let _ = view.set_visible(false);
            }
            self.shown = false;
        }
    }

    fn paint(&mut self, bounds: Bounds<Pixels>) {
        if !self.allowed || bounds.size.width <= px(1.) || bounds.size.height <= px(1.) {
            self.hide();
            return;
        }
        let Some(view) = &self.view else {
            return;
        };
        if self.bounds != Some(bounds) {
            let width_changed = self.bounds.is_none_or(|b| {
                (f32::from(b.size.width) - f32::from(bounds.size.width)).abs() > 1.0
            });
            let rect = wry::Rect {
                position: wry::dpi::LogicalPosition::new(
                    f32::from(bounds.origin.x) as f64,
                    f32::from(bounds.origin.y) as f64,
                )
                .into(),
                size: wry::dpi::LogicalSize::new(
                    f32::from(bounds.size.width) as f64,
                    f32::from(bounds.size.height) as f64,
                )
                .into(),
            };
            if let Err(e) = view.set_bounds(rect) {
                tracing::warn!("preview bounds: {e}");
                return;
            }
            self.bounds = Some(bounds);
            if width_changed {
                let _ = self.events.try_send(Event::Resized);
            }
        }
        if !self.shown {
            if let Err(e) = view.set_visible(true) {
                tracing::warn!("preview show: {e}");
                return;
            }
            self.shown = true;
        }
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.hide();
    }
}

#[derive(Clone, PartialEq, Eq)]
struct ArtifactLocation { target: String, sources: Vec<String> }

pub struct Preview {
    pub(super) is_browser: bool,
    space: String,
    cwd: String,
    input: Entity<InputState>,
    page_input: Entity<InputState>,
    document: Option<Arc<Document>>,
    document_sources: Vec<PathBuf>,
    document_page: usize,
    document_requested_page: usize,
    document_epoch: u64,
    document_pending: bool,
    document_cancel: Arc<AtomicBool>,
    _page_subscription: Subscription,
    pub(super) browser: Rc<RefCell<Browser>>,
    events: async_channel::Sender<Event>,
    server: Option<Server>,
    history: Vec<ArtifactLocation>,
    history_at: usize,
    watcher: Option<notify::RecommendedWatcher>,
    target: String,
    url: String,
    pub(super) loading: bool,
    pub(super) error: Option<String>,
    auto: bool,
    dirty: bool,
    allowed: bool,
    pub(super) generation: u64,
    pub(super) automation_epoch: u64,
    tabbed: bool,
    popup: Option<String>,
    title: String,
    restored: bool,
    page_zoom: f64,
    fit_width: bool,
    mobile: bool,
    fit_generation: u64,
    _subscription: Subscription,
    _events: Task<()>,
}

fn color(name: &str) -> Hsla {
    settings::color(name)
}

pub(super) fn network_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|u| {
        matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
    })
}

pub(super) fn target(value: &str, cwd: &str) -> Result<String, String> {
    let value = value.trim();
    let value = if value.len() >= 2
        && ((value.starts_with('\'') && value.ends_with('\''))
            || (value.starts_with('"') && value.ends_with('"')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    };
    if value.is_empty() {
        return Err("HTML·이미지 파일 경로나 http://localhost:3000 같은 주소를 입력하세요.".into());
    }
    let value = if value.starts_with("localhost:")
        || value.starts_with("127.0.0.1:")
        || value.starts_with("[::1]:")
    {
        format!("http://{value}")
    } else {
        value.to_owned()
    };
    if network_url(&value) {
        return Ok(value);
    }
    if value.contains("://") {
        return Err("http·https 주소 또는 로컬 파일을 열 수 있습니다.".into());
    }
    let path = value
        .strip_prefix("~/")
        .map(|s| mods::home().join(s))
        .unwrap_or_else(|| {
            let p = PathBuf::from(&value);
            if p.is_absolute() {
                p
            } else {
                Path::new(cwd).join(p)
            }
        });
    if super::document::is_document(&path) {
        return super::document::checked_source_path(&path).map(|p|p.to_string_lossy().into_owned());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "파일을 찾을 수 없습니다. 경로를 확인하세요.")?;
    if !path.is_file() {
        return Err("미리볼 HTML 또는 이미지 파일을 선택하세요.".into());
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "html"
            | "htm"
            | "svg"
            | "png"
            | "jpg"
            | "jpeg"
            | "webp"
            | "avif"
            | "gif"
            | "pdf"
            | "ppt"
            | "pptx"
            | "txt"
            | "md"
    ) {
        return Err("PPT·PPTX·PDF·이미지·HTML·텍스트 파일을 미리볼 수 있습니다.".into());
    }
    Ok(path.to_string_lossy().into_owned())
}

impl Preview {
    pub fn new(space: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::new_with_mode(space, false, window, cx)
    }

    pub fn new_browser_tab(id: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut page = Self::new_with_mode(id, true, window, cx);
        page.tabbed = true;
        page
    }

    pub fn take_popup(&mut self) -> Option<String> { self.popup.take() }

    pub fn invalidate_automation(&mut self) {
        self.automation_epoch += 1;
        if let Some(view) = &self.browser.borrow().view {
            let _ = view.evaluate_script("window.__herdrBrowserRefs?.clear(); window.__herdrBrowserPending?.clear();");
        }
    }

    fn new_with_mode(
        space: &str,
        is_browser: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(if is_browser {
                "검색어 또는 웹사이트 주소"
            } else {
                "PPT·PDF·이미지 경로 또는 http://localhost:3000"
            })
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.navigate_input(window, cx);
            }
        });
        let page_input = cx.new(|cx| InputState::new(window, cx).placeholder("1"));
        let page_subscription = cx.subscribe_in(&page_input, window, |this, _, event, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) { this.jump_document_input(cx); }
        });
        let (tx, rx) = async_channel::unbounded();
        let events = cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = rx.recv().await {
                if matches!(event, Event::Changed) {
                    // Editors may replace several artifacts in one save.
                    cx.background_executor()
                        .timer(Duration::from_millis(250))
                        .await;
                }
                if this
                    .update_in(cx, |this: &mut Self, window, cx| {
                        match event {
                            Event::DocumentReady(epoch, result) => {
                                if epoch == this.document_epoch { this.finish_document(result, window, cx); }
                            }
                            Event::PageReady(epoch, result) => {
                                if epoch == this.document_epoch {
                                    match result {
                                        Ok((page,image)) => if let Err(error)=this.show_document_image(page,&image,window,cx) {this.error=Some(error);this.loading=false;},
                                        Err(error) => {this.error=Some(error);this.loading=false;}
                                    }
                                }
                            }
                            Event::Loading(url) => {
                                if !this.document_sources.is_empty() && (this.document_pending || url != this.url) { return; }
                                this.url = url;
                                this.start_loading(cx);
                            }
                            Event::Loaded(url) => {
                                if !this.document_sources.is_empty() && (this.document_pending || url != this.url) { return; }
                                this.loading = false;
                                this.url = url.clone();
                                this.error = None;
                                if this.is_browser {
                                    this.target = url.clone();
                                    this.input.update(cx, |input, cx| {
                                        input.set_value(url.clone(), window, cx)
                                    });
                                }
                                this.save_view();
                                this.schedule_fit(cx);
                            }
                            Event::Changed => {
                                if this.server.is_some() {
                                    this.dirty = true;
                                    if this.allowed && this.auto {
                                        this.reload(cx);
                                    }
                                }
                            }
                            Event::Popup(url) => {
                                if this.tabbed {
                                    this.popup = Some(url);
                                } else {
                                    let cwd = this.cwd.clone();
                                    let _ = this.open(&url, &cwd, window, cx);
                                }
                            }
                            Event::Title(title) => {
                                if !this.document_sources.is_empty() { return; }
                                this.title = title.chars().take(200).collect();
                                this.save_view();
                            }
                            Event::Resized => this.schedule_fit(cx),
                            Event::FitMeasured(generation, available, content, pass) => {
                                if this.fit_width
                                    && !this.loading
                                    && generation == this.fit_generation
                                    && available > 0.0
                                    && content > 0.0
                                {
                                    let zoom =
                                        (this.page_zoom * available / content).clamp(0.25, 1.0);
                                    this.apply_zoom(zoom);
                                    this.save_view();
                                    if pass == 0 {
                                        this.remeasure_fit(generation, cx);
                                    }
                                }
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            is_browser,
            space: space.into(),
            cwd: mods::home().to_string_lossy().into_owned(),
            input,
            page_input,
            document: None,
            document_sources: Vec::new(),
            document_page: 0,
            document_requested_page: 1,
            document_epoch: 0,
            document_pending: false,
            document_cancel: Arc::new(AtomicBool::new(false)),
            _page_subscription: page_subscription,
            browser: Rc::new(RefCell::new(Browser {
                view: None,
                allowed: false,
                shown: false,
                bounds: None,
                events: tx.clone(),
            })),
            events: tx,
            server: None,
            history: Vec::new(),
            history_at: 0,
            watcher: None,
            target: String::new(),
            url: String::new(),
            loading: false,
            error: None,
            auto: true,
            dirty: false,
            allowed: false,
            generation: 0,
            automation_epoch: 0,
            tabbed: false,
            popup: None,
            title: String::new(),
            restored: false,
            page_zoom: 1.0,
            fit_width: true,
            mobile: false,
            fit_generation: 0,
            _subscription: subscription,
            _events: events,
        }
    }

    pub fn context_initial(&mut self, cwd: &str, target: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_browser && !self.restored {
            let saved: Value=std::fs::read_to_string(self.state_path()).ok().and_then(|s|serde_json::from_str(&s).ok()).unwrap_or(json!({}));
            if target.is_some() && saved[&self.space].as_str()==target {self.context(cwd,window,cx);return;}
        }
        if let Some(target) = target.filter(|_| !self.restored) {
            self.restored = true;
            self.cwd = cwd.into();
            let _ = self.open(target, cwd, window, cx);
        } else { self.context(cwd, window, cx); }
    }

    pub fn context(&mut self, cwd: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.cwd = cwd.into();
        if !self.restored {
            self.restored = true;
            let saved: Value = std::fs::read_to_string(self.state_path())
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(json!({}));
            let view = &saved["_view"][&self.space];
            self.title = view["title"].as_str().unwrap_or_default().chars().take(200).collect();
            self.page_zoom = view["zoom"]
                .as_f64()
                .filter(|z| z.is_finite())
                .unwrap_or(1.0)
                .clamp(0.25, 2.0);
            self.fit_width = view["fit"].as_bool().unwrap_or(true);
            self.mobile = self.is_browser && view["mobile"].as_bool().unwrap_or(false);
            if let Some(last) = saved[&self.space].as_str() {
                let files:Vec<PathBuf>=view["files"].as_array().into_iter().flatten().filter_map(Value::as_str).map(PathBuf::from).collect();
                if !self.is_browser && !files.is_empty() && files[0].to_str()==Some(last) && files.iter().all(|p|p.is_absolute() && super::document::is_document(p)) && self.ensure_browser(window).is_ok() {
                    self.target=last.into();
                    self.input.update(cx,|i,cx|i.set_value(last.to_owned(),window,cx));
                    self.begin_document(files,view["page"].as_u64().unwrap_or(1) as usize,cx);
                } else {let _ = self.open(last, cwd, window, cx);}
            } else if self.is_browser {
                let _ = self.open("https://blog.naver.com/", cwd, window, cx);
            }
        }
    }

    fn state_path(&self) -> PathBuf {
        mods::app_dir().join(if self.is_browser {
            "browser.json"
        } else {
            "preview.json"
        })
    }

    fn save_view(&self) {
        let path = self.state_path();
        let mut saved: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(Value::is_object)
            .unwrap_or(json!({}));
        if !self.target.is_empty() {
            saved[&self.space] = json!(self.target);
        }
        if !saved["_view"].is_object() {
            saved["_view"] = json!({});
        }
        saved["_view"][&self.space] =
            json!({"zoom":self.page_zoom,"fit":self.fit_width,"mobile":self.mobile,"title":self.title,"files":self.document_sources,"page":self.document_requested_page.max(1)});
        let _ = crate::persist::write_atomic(&path, serde_json::to_string_pretty(&saved).unwrap());
    }

    fn apply_zoom(&mut self, zoom: f64) {
        let zoom = zoom.clamp(0.25, 2.0);
        if let Some(view) = &self.browser.borrow().view {
            let _ = view.zoom(zoom);
        }
        self.page_zoom = zoom;
    }

    fn schedule_fit(&mut self, cx: &mut Context<Self>) {
        if !self.fit_width || self.loading {
            return;
        }
        self.fit_generation += 1;
        let generation = self.fit_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(160))
                .await;
            let ready = this
                .update(cx, |this, _| {
                    if this.fit_width && this.fit_generation == generation && !this.loading {
                        this.apply_zoom(1.0);
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if !ready {
                return;
            }
            // Let WebKit apply 100% before measuring the page's natural width.
            cx.background_executor()
                .timer(Duration::from_millis(35))
                .await;
            let _ = this.update(cx, |this, _| {
                if !this.fit_width || this.fit_generation != generation || this.loading {
                    return;
                }
                let events = this.events.clone();
                if let Some(view) = &this.browser.borrow().view {
                    let _ = view.evaluate_script_with_callback(
                        include_str!("page-width.js"),
                        move |raw| {
                            let parsed =
                                serde_json::from_str::<Value>(&raw)
                                    .ok()
                                    .and_then(|v| match v {
                                        Value::String(s) => serde_json::from_str(&s).ok(),
                                        v => Some(v),
                                    });
                            if let Some(data) = parsed {
                                let _ = events.try_send(Event::FitMeasured(
                                    generation,
                                    data["available"].as_f64().unwrap_or(0.0),
                                    data["content"].as_f64().unwrap_or(0.0),
                                    0,
                                ));
                            }
                        },
                    );
                }
            });
        })
        .detach();
    }

    fn remeasure_fit(&self, generation: u64, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(80))
                .await;
            let _ = this.update(cx, |this, _| {
                if !this.fit_width || this.loading || this.fit_generation != generation {
                    return;
                }
                let events = this.events.clone();
                if let Some(view) = &this.browser.borrow().view {
                    let _ = view.evaluate_script_with_callback(
                        include_str!("page-width.js"),
                        move |raw| {
                            let data =
                                serde_json::from_str::<Value>(&raw)
                                    .ok()
                                    .and_then(|v| match v {
                                        Value::String(s) => serde_json::from_str::<Value>(&s).ok(),
                                        v => Some(v),
                                    });
                            if let Some(data) = data {
                                let _ = events.try_send(Event::FitMeasured(
                                    generation,
                                    data["available"].as_f64().unwrap_or(0.0),
                                    data["content"].as_f64().unwrap_or(0.0),
                                    1,
                                ));
                            }
                        },
                    );
                }
            });
        })
        .detach();
    }

    pub fn configure_view(
        &mut self,
        args: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(page)=args.get("page") {
            if self.is_browser {return Err("페이지 이동은 Preview 문서에서만 사용할 수 있습니다.".into());}
            let page=page.as_u64().and_then(|p|usize::try_from(p).ok()).ok_or("정수 페이지 번호를 입력하세요.")?;
            self.request_document_page(page,cx)?;
        }
        if let Some(zoom) = args.get("zoom") {
            let zoom = zoom
                .as_f64()
                .filter(|z| z.is_finite() && (0.25..=2.0).contains(z))
                .ok_or("zoom must be between 0.25 and 2.0")?;
            self.fit_generation += 1;
            self.fit_width = false;
            self.apply_zoom(zoom);
        }
        if let Some(fit) = args.get("fit") {
            self.fit_width = fit.as_bool().ok_or("fit must be true or false")?;
            self.fit_generation += 1;
            if self.fit_width {
                self.schedule_fit(cx);
            }
        }
        if let Some(mobile) = args.get("mobile") {
            let mobile = mobile.as_bool().ok_or("mobile must be true or false")?;
            if mobile != self.mobile {
                self.mobile = mobile;
                if let Some(view) = &self.browser.borrow().view {
                    super::automation::set_mobile_agent(view, mobile);
                }
                if !self.target.is_empty() {
                    let target = super::automation::device_url(&self.url, mobile);
                    let cwd = self.cwd.clone();
                    self.open(&target, &cwd, window, cx)?;
                }
            }
        }
        self.save_view();
        cx.notify();
        Ok(())
    }

    pub fn allow(&mut self, allowed: bool) {
        self.allowed = allowed;
        let mut browser = self.browser.borrow_mut();
        browser.allowed = allowed;
        if !allowed {
            browser.hide();
        }
    }

    fn ensure_browser(&mut self, window: &mut Window) -> Result<(), String> {
        if self.browser.borrow().view.is_some() {
            return Ok(());
        }
        let loads = self.events.clone();
        let popups = self.events.clone();
        let titles = self.events.clone();
        let mut builder = wry::WebViewBuilder::new()
            .with_url("about:blank")
            .with_visible(false)
            .with_focused(false)
            .with_incognito(!self.is_browser)
            .with_navigation_handler(|url| url == "about:blank" || network_url(&url))
            .with_download_started_handler(|_, _| false)
            .with_document_title_changed_handler(move |title| { let _ = titles.try_send(Event::Title(title)); })
            .with_new_window_req_handler(move |url, _| {
                if network_url(&url) {
                    let _ = popups.try_send(Event::Popup(url));
                }
                wry::NewWindowResponse::Deny
            })
            .with_on_page_load_handler(move |event, url| {
                if url == "about:blank" {
                    return;
                }
                let _ = loads.try_send(match event {
                    wry::PageLoadEvent::Started => Event::Loading(url),
                    wry::PageLoadEvent::Finished => Event::Loaded(url),
                });
            });
        if self.mobile {
            builder = builder.with_user_agent(super::automation::MOBILE_AGENT);
        }
        if self.is_browser {
            use sha2::{Digest, Sha256};
            use wry::WebViewBuilderExtDarwin;
            // Stable per app home: test profiles never share the user's login.
            let hash = Sha256::digest(mods::app_dir().to_string_lossy().as_bytes());
            let mut identifier = [0_u8; 16];
            identifier.copy_from_slice(&hash[..16]);
            identifier[6] = (identifier[6] & 15) | 64;
            identifier[8] = (identifier[8] & 63) | 128;
            builder = builder.with_data_store_identifier(identifier);
        }
        let view = builder
            .build_as_child(&*window)
            .map_err(|e| format!("웹뷰를 열지 못했습니다: {e}"))?;
        self.browser.borrow_mut().view = Some(view);
        self.apply_zoom(self.page_zoom);
        Ok(())
    }

    pub fn open(
        &mut self,
        value: &str,
        cwd: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let result = self.open_inner(value, cwd, window, cx);
        if let Err(e) = &result {
            self.error = Some(e.clone());
            self.loading = false;
            cx.notify();
        }
        result
    }

    fn open_inner(
        &mut self,
        value: &str,
        cwd: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let value = if self.is_browser {
            let value = super::automation::browser_address(value)?;
            if self.mobile {
                super::automation::device_url(&value, true)
            } else {
                value
            }
        } else {
            value.to_owned()
        };
        let target = target(&value, cwd)?;
        self.ensure_browser(window)?;
        if !self.is_browser && !network_url(&target) && super::document::is_document(Path::new(&target)) {
            self.remember_artifact(&target, &[]);
            self.target=target.clone();
            self.title=Path::new(&target).file_name().unwrap_or_default().to_string_lossy().into_owned();
            self.input.update(cx,|i,cx|i.set_value(target.clone(),window,cx));
            self.begin_document(vec![PathBuf::from(target)],1,cx);
            self.save_view(); return Ok(());
        }
        self.document_cancel.store(true,Ordering::Relaxed);
        self.document_epoch+=1;self.document=None;self.document_sources.clear();self.document_page=0;self.document_pending=false;
        let previous = self.server.take();
        let target = previous.as_ref().and_then(|server| server.target_for_url(&target))
            .map(|p| p.to_string_lossy().into_owned()).unwrap_or(target);
        let server = if network_url(&target) {
            None
        } else {
            let root = Path::new(&target).parent().unwrap();
            Some(
                if let Some(previous) = previous.filter(|s| s.root == root) {
                    previous
                } else {
                    Server::start(root).map_err(|e| format!("미리보기 서버: {e}"))?
                },
            )
        };
        let url = match &server {
            Some(server) => server.url(Path::new(&target))?,
            None => target.clone(),
        };
        self.browser
            .borrow()
            .view
            .as_ref()
            .unwrap()
            .load_url(&url)
            .map_err(|e| e.to_string())?;
        self.watcher = None;
        if let Some(server) = &server {
            let updates = self.events.clone();
            let watched_root = server.root.clone();
            match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    if event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove() {
                        let relevant = event.paths.iter().any(|p| {
                            !p.strip_prefix(&watched_root)
                                .unwrap_or(p)
                                .components()
                                .any(|part| {
                                    let s = part.as_os_str().to_string_lossy();
                                    s.starts_with('.') || matches!(&*s, "node_modules" | "target")
                                })
                        });
                        if relevant {
                            let _ = updates.try_send(Event::Changed);
                        }
                    }
                }
            }) {
                Ok(mut watcher) => {
                    if let Err(e) = watcher.watch(&server.root, notify::RecursiveMode::Recursive) {
                        tracing::warn!("preview watch: {e}");
                    } else {
                        self.watcher = Some(watcher);
                    }
                }
                Err(e) => tracing::warn!("preview watch: {e}"),
            }
        }
        self.server = server;
        if !self.is_browser { self.remember_artifact(&target, &[]); }
        self.target = target.clone();
        self.url = url;
        self.dirty = false;
        self.start_loading(cx);
        self.input
            .update(cx, |input, cx| input.set_value(target.clone(), window, cx));
        self.save_view();
        cx.notify();
        Ok(())
    }

    fn start_loading(&mut self, cx: &mut Context<Self>) {
        self.fit_generation += 1;
        self.loading = true;
        self.error = None;
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(15)).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation && this.loading {
                    this.loading = false;
                    this.error = Some("아직 페이지를 불러오지 못했습니다. 서버 실행 여부와 주소를 확인하고 새로고침하세요.".into());
                    cx.notify();
                }
            });
        }).detach();
    }

    fn navigate_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        let cwd = self.cwd.clone();
        let _ = self.open(&value, &cwd, window, cx);
    }

    fn choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.allow(false);
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: !self.is_browser,
            prompt: Some("Preview에서 열기".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let paths = paths.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if let Ok(Ok(Some(paths))) = paths {
                    let values:Vec<String>=paths.iter().map(|p|p.to_string_lossy().into_owned()).collect();
                    let cwd=this.cwd.clone();
                    let _=this.open_selection(&values,&cwd,window,cx);
                }
                this.allow(true);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if !self.document_sources.is_empty() {
            self.begin_document(self.document_sources.clone(),self.document_requested_page.max(1),cx);
            return;
        }
        let result = self
            .browser
            .borrow()
            .view
            .as_ref()
            .map(|view| view.reload());
        if let Some(result) = result {
            match result {
                Ok(()) => self.start_loading(cx),
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        self.dirty = false;
        cx.notify();
    }

    pub fn status(&self) -> Value {
        json!({"supported": true, "target": self.target, "url": self.url, "loading": self.loading,
            "error": self.error, "auto_reload": self.auto, "visible": self.browser.borrow().shown,
            "persistent": self.is_browser,"page_zoom":self.page_zoom,"fit_width":self.fit_width,"mobile":self.mobile,"title":self.title,"page":self.document_page,"pages":self.document.as_ref().map_or(0,|d|d.page_count()),"files":self.document_sources,"document_loading":self.document_pending})
    }

    fn step_zoom(&mut self, increase: bool, window: &mut Window, cx: &mut Context<Self>) {
        let steps = [
            0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0,
        ];
        let zoom = if increase {
            steps
                .into_iter()
                .find(|z| *z > self.page_zoom + 0.005)
                .unwrap_or(2.0)
        } else {
            steps
                .into_iter()
                .rev()
                .find(|z| *z < self.page_zoom - 0.005)
                .unwrap_or(0.25)
        };
        let _ = self.configure_view(&json!({"zoom":zoom}), window, cx);
    }

    pub fn navigate_history(&mut self, forward: bool) {
        if let Some(view) = &self.browser.borrow().view {
            let _ = if forward {
                view.go_forward()
            } else {
                view.go_back()
            };
        }
    }

    fn remember_artifact(&mut self, target:&str, sources:&[String]) {
        let location=ArtifactLocation{target:target.into(),sources:sources.to_vec()};
        if self.history.get(self.history_at)==Some(&location){return;}
        self.history.truncate(if self.history.is_empty(){0}else{self.history_at+1});
        self.history.push(location); self.history_at=self.history.len()-1;
    }

    pub fn navigate_artifact_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_browser || (self.history.len() < 2 && self.document_sources.is_empty()) {
            self.navigate_history(forward);
            return;
        }
        if self.history.len() < 2 { return; }
        let next = if forward {
            self.history_at
                .checked_add(1)
                .filter(|n| *n < self.history.len())
        } else {
            self.history_at.checked_sub(1)
        };
        let Some(next) = next else { return };
        let previous = self.history_at;
        self.history_at = next;
        let location = self.history[next].clone();
        let cwd = self.cwd.clone();
        let result = if location.sources.len()>1 {
            self.open_selection(&location.sources,&cwd,window,cx)
        } else { self.open(&location.target,&cwd,window,cx) };
        if result.is_err() { self.history_at = previous; }
    }

    // Only the isolated debug smoke harness can request page evaluation.
    // Release builds expose no script execution or page-to-host bridge.
    #[cfg(debug_assertions)]
    pub fn test_probe(&self, script: &str) -> Result<(), String> {
        if std::env::var("HERDR_PREVIEW_TEST_CONTROL").as_deref() != Ok("1") {
            return Err("Preview test control is disabled".into());
        }
        let browser = self.browser.borrow();
        let view = browser.view.as_ref().ok_or("No preview page")?;
        let output = mods::app_dir().join("preview-test-result.json");
        view.evaluate_script_with_callback(script, move |value| {
            let _ = crate::persist::write_atomic(&output, value);
        })
        .map_err(|e| e.to_string())
    }

    fn button(
        &self,
        id: &'static str,
        label: &'static str,
        cx: &Context<Self>,
        f: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        div()
            .id(id)
            .h(px(28.))
            .px(px(8.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .text_color(color("muted"))
            .hover(|s| s.bg(color("hover")).text_color(color("foreground")))
            .on_click(cx.listener(move |this, _, window, cx| f(this, window, cx)))
            .child(label)
    }
}

impl Render for Preview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.allowed && self.auto && self.dirty {
            self.reload(cx);
        }
        let browser = self.browser.clone();
        let has_page = self.browser.borrow().view.is_some() && !self.target.is_empty() && !self.document_pending && (self.document_sources.is_empty() || self.document_page>0);
        div()
            .id("artifact-preview")
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .rounded(px(10.))
            .overflow_hidden()
            .border_1()
            .border_color(color("border"))
            .bg(color("background"))
            .text_color(color("foreground"))
            .text_size(px(12.))
            .child(
                div().id("page-toolbar").h(px(34.)).px(px(6.)).gap(px(4.)).flex_none()
                    .flex().items_center().justify_between().overflow_x_scroll().border_b_1().border_color(color("border"))
                    .child(div().flex_none().flex().items_center().gap(px(2.))
                        .child(div().flex_none().mr(px(4.)).font_weight(FontWeight::SEMIBOLD)
                            .child(if self.is_browser { "Browser" } else { "Preview" }))
                        .child(self.button("preview-back", "←", cx, |this,w,cx|this.navigate_artifact_history(false,w,cx)))
                        .child(self.button("preview-forward", "→", cx, |this,w,cx|this.navigate_artifact_history(true,w,cx)))
                        .child(self.button("preview-reload", if self.loading { "…" } else { "↻" }, cx, |this,_,cx|this.reload(cx))))
                    .child(div().flex_none().flex().items_center().gap(px(2.))
                        .child(self.button("page-fit", if self.fit_width { "맞춤 ✓" } else { "맞춤" }, cx, |this,w,cx| {
                            let _=this.configure_view(&json!({"fit":!this.fit_width}),w,cx);
                        }))
                        .child(self.button("page-zoom-out", "−", cx, |this,w,cx|this.step_zoom(false,w,cx)))
                        .child(div().id("page-zoom-reset").w(px(38.)).h(px(28.)).flex_none().flex().items_center().justify_center()
                            .rounded(px(5.)).cursor_pointer().text_color(color("muted")).hover(|s|s.bg(color("hover")))
                            .child(format!("{:.0}%",self.page_zoom*100.))
                            .on_click(cx.listener(|this,_,w,cx|{let _=this.configure_view(&json!({"zoom":1.0}),w,cx);})))
                        .child(self.button("page-zoom-in", "+", cx, |this,w,cx|this.step_zoom(true,w,cx))))
                    .child(div().flex_none().flex().items_center().gap(px(2.))
                        .when(self.is_browser, |d|d
                            .child(self.button("page-device", if self.mobile { "모바일" } else { "PC" }, cx, |this,w,cx|{
                                let _=this.configure_view(&json!({"mobile":!this.mobile}),w,cx);
                            }))
                            .child(self.button("browser-home", "블로그", cx, |this,w,cx|{
                                let _=this.open("https://blog.naver.com/",&this.cwd.clone(),w,cx);
                            })))
                        .when(!self.is_browser, |d|d
                            .child(self.button("preview-open", "파일", cx, |this,w,cx|this.choose(w,cx)))
                            .when(self.server.is_some(),|d|d.child(self.button("preview-auto",if self.auto { "자동 ✓" } else { "자동" },cx,|this,_,cx|{
                                this.auto=!this.auto;cx.notify();
                            }))))),
            )
            .child(
                div().h(px(34.)).p(px(3.)).gap(px(4.)).flex_none().flex().items_center()
                    .border_b_1().border_color(color("border"))
                    .child(div().flex_1().min_w_0().h(px(28.)).px(px(7.)).flex().items_center()
                        .rounded(px(5.)).bg(color("input")).child(Input::new(&self.input)))
                    .child(self.button("preview-go","열기",cx,|this,w,cx|this.navigate_input(w,cx))),
            )
            .when(!self.is_browser && !self.document_sources.is_empty(), |d|d.child(self.document_controls(cx)))
            .when(self.error.is_some(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .p(px(10.))
                        .text_size(px(12.))
                        .text_color(color("blocked"))
                        .child(self.error.clone().unwrap()),
                )
            })
            .child(if has_page {
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        canvas(
                            move |bounds, _, _| bounds,
                            move |bounds, _, _, _| browser.borrow_mut().paint(bounds),
                        )
                        .size_full(),
                    )
                    .into_any_element()
            } else {
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(24.))
                    .justify_center()
                    .items_center()
                    .text_color(color("muted"))
                    .child(div().text_size(px(28.)).child("▧"))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("작업 결과를 여기서 확인하세요"),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .child("PPT·PDF·이미지 파일을 열거나 개발 서버 주소를 입력하세요."),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .child("HTML 안에서 클릭·입력·스크롤할 수 있습니다."),
                    )
                    .into_any_element()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{network_url, target};
    #[test]
    fn accepts_office_documents_as_local_preview_targets() {
        let root=std::env::temp_dir().join(format!("herdr-office-targets-{}",std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["deck.ppt","deck.pptx"] {
            let path=root.join(name);std::fs::write(&path,b"fixture").unwrap();
            assert!(target(path.to_str().unwrap(),"/").is_ok());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn accepts_only_web_urls_and_existing_supported_files() {
        assert!(network_url("http://localhost:3000"));
        assert!(!network_url("javascript:alert(1)"));
        assert!(!network_url("file:///etc/passwd"));
        assert!(!network_url("https://user:password@example.com"));
        assert_eq!(
            target("localhost:3000", "/tmp").unwrap(),
            "http://localhost:3000"
        );
        assert!(target("javascript://alert(1)", "/tmp").is_err());
        assert!(target("/definitely/missing.html", "/tmp").is_err());
    }
}

impl Drop for Preview {
    fn drop(&mut self) {self.document_cancel.store(true,Ordering::Relaxed);}
}
