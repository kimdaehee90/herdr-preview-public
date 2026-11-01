//! Tabs own independent pages. Only the selected page is painted or focused.
use super::Preview;
use crate::{mods, settings};
use gpui::*;
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_TABS: usize = 32;
static NEXT_TAB: AtomicU64 = AtomicU64::new(0);

struct Tab {
    id: String,
    page: Option<Entity<Preview>>,
    _observe: Option<Subscription>,
    saved_url: String,
    saved_title: String,
    initial: Option<String>,
}

pub struct PagePanel {
    is_browser: bool,
    space: String,
    cwd: String,
    tabs: Vec<Tab>,
    active: usize,
    restored: bool,
    allowed: bool,
    error: Option<String>,
}

fn saved(path: &std::path::Path) -> Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn restored_tabs(data: &Value, space: &str) -> (Vec<String>, usize) {
    let manifest = &data["_tabs"][space];
    let mut ids = Vec::new();
    if let Some(order) = manifest["order"].as_array() {
        for id in order.iter().filter_map(Value::as_str).take(MAX_TABS) {
            if (id == space
                || (id.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric)
                    && id.len() <= 96
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))))
                && !ids.iter().any(|v| v == id)
            {
                ids.push(id.to_owned());
            }
        }
    }
    // Retain the legacy URL/view keys for the original single page.
    if ids.is_empty() {
        ids.push(space.to_owned());
    }
    let active = manifest["active"]
        .as_str()
        .and_then(|id| ids.iter().position(|v| v == id))
        .unwrap_or(0);
    (ids, active)
}

impl PagePanel {
    pub fn new(space: &str, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self::with_mode(space, false)
    }
    pub fn new_browser(space: &str, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self::with_mode(&format!("browser-{space}"), true)
    }
    fn with_mode(space: &str, is_browser: bool) -> Self {
        Self {
            is_browser,
            space: space.into(),
            cwd: String::new(),
            tabs: Vec::new(),
            active: 0,
            restored: false,
            allowed: false,
            error: None,
        }
    }
    fn path(&self) -> PathBuf {
        mods::app_dir().join(if self.is_browser {
            "browser.json"
        } else {
            "preview.json"
        })
    }
    fn persist(&self, closed: Option<&str>) {
        let path = self.path();
        let mut data = saved(&path);
        if !data["_tabs"].is_object() {
            data["_tabs"] = json!({});
        }
        data["_tabs"][&self.space] = json!({"order":self.tabs.iter().map(|t| &t.id).collect::<Vec<_>>(),"active":self.tabs.get(self.active).map(|t| &t.id)});
        if let Some(id) = closed {
            // Never remove another space's legacy state.
            if id != self.space {
                data.as_object_mut().unwrap().remove(id);
                if let Some(view) = data["_view"].as_object_mut() {
                    view.remove(id);
                }
            }
        }
        let _ = crate::persist::write_atomic(&path, serde_json::to_string_pretty(&data).unwrap());
    }
    pub fn context(&mut self, cwd: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.cwd = cwd.into();
        if !self.restored {
            self.restored = true;
            let mut data = saved(&self.path());
            if self.is_browser && data.get("_browser_legacy_owner").is_none() {
                // Migrate the original shared Browser once. Preserve its URL,
                // view settings and login profile without cloning an editor.
                if !data["_tabs"].is_object() {
                    data["_tabs"] = json!({});
                }
                let manifest = data["_tabs"]["browser"].clone();
                data["_tabs"][&self.space] = if manifest.is_object() {
                    manifest
                } else {
                    json!({"order":["browser"],"active":"browser"})
                };
                data["_browser_legacy_owner"] = json!(self.space);
                let _ = crate::persist::write_atomic(
                    &self.path(),
                    serde_json::to_string_pretty(&data).unwrap(),
                );
            }
            let (ids, active) = restored_tabs(&data, &self.space);
            self.tabs = ids
                .into_iter()
                .map(|id| Tab {
                    saved_url: data[&id].as_str().unwrap_or_default().into(),
                    saved_title: data["_view"][&id]["title"]
                        .as_str()
                        .unwrap_or_default()
                        .into(),
                    id,
                    page: None,
                    _observe: None,
                    initial: None,
                })
                .collect();
            self.active = active;
            self.ensure_active(window, cx);
            self.persist(None);
        } else if let Some(page) = self.page() {
            page.update(cx, |p, cx| p.context(cwd, window, cx));
        }
    }
    pub fn has_tab(&self, id: &str) -> bool {
        self.tabs.iter().any(|t| t.id == id)
    }
    fn page(&self) -> Option<Entity<Preview>> {
        self.tabs.get(self.active).and_then(|t| t.page.clone())
    }
    fn ensure_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_page(self.active, window, cx);
    }
    fn ensure_page(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs[index].page.is_none() {
            let id = self.tabs[index].id.clone();
            let page = cx.new(|cx| {
                if self.is_browser {
                    Preview::new_browser_tab(&id, window, cx)
                } else {
                    Preview::new(&id, window, cx)
                }
            });
            let observe = cx.observe_in(&page, window, |this, page, window, cx| {
                if let Some(url) = page.update(cx, |p, _| p.take_popup()) {
                    let _ = this.new_tab(Some(&url), window, cx);
                }
                cx.notify();
            });
            let initial = self.tabs[index].initial.take();
            page.update(cx, |p, cx| {
                p.context_initial(&self.cwd, initial.as_deref(), window, cx)
            });
            self.tabs[index].page = Some(page);
            self.tabs[index]._observe = Some(observe);
        }
        self.apply_visibility(cx);
    }
    fn apply_visibility(&mut self, cx: &mut Context<Self>) {
        for (i, tab) in self.tabs.iter().enumerate() {
            if let Some(page) = &tab.page {
                page.update(cx, |p, _| p.allow(self.allowed && i == self.active));
            }
        }
    }
    pub fn allow(&mut self, allowed: bool, cx: &mut Context<Self>) {
        self.allowed = allowed;
        self.apply_visibility(cx);
    }
    pub fn new_tab(
        &mut self,
        target: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        if self.tabs.len() >= MAX_TABS {
            return Err(format!(
                "At most {MAX_TABS} page tabs can be open. Close a tab first."
            ));
        }
        self.error = None;
        // Validate before hiding a page or creating an entry.
        if let Some(target) = target {
            if self.is_browser {
                super::normalize_browser_target(target)?;
            } else {
                super::normalize_preview_target(target, &self.cwd)?;
            }
        }
        if let Some(page) = self.page() {
            page.update(cx, |p, _| {
                p.invalidate_automation();
                p.allow(false);
            });
        }
        let serial = NEXT_TAB.fetch_add(1, Ordering::Relaxed);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let id = format!("view-{stamp:x}-{serial:x}");
        self.tabs.push(Tab {
            id: id.clone(),
            page: None,
            _observe: None,
            saved_url: String::new(),
            saved_title: String::new(),
            initial: target.map(str::to_owned),
        });
        self.active = self.tabs.len() - 1;
        self.ensure_active(window, cx);
        self.persist(None);
        cx.notify();
        Ok(json!({"tab":id,"active_tab":id}))
    }
    pub fn select_tab(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let next = self
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or("Page tab not found")?;
        if next != self.active {
            if let Some(page) = self.page() {
                page.update(cx, |p, _| {
                    p.invalidate_automation();
                    p.allow(false);
                });
            }
            self.active = next;
            self.ensure_active(window, cx);
            if let Some(page) = self.page() {
                page.update(cx, |p, _| p.invalidate_automation());
            }
            self.persist(None);
            cx.notify();
        }
        Ok(json!({"active_tab":id}))
    }
    pub fn close_tab(
        &mut self,
        id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let index = if let Some(id) = id {
            self.tabs
                .iter()
                .position(|t| t.id == id)
                .ok_or("Page tab not found")?
        } else {
            self.active
        };
        let closing_active = index == self.active;
        if let Some(page) = &self.tabs[index].page {
            page.update(cx, |p, _| {
                p.invalidate_automation();
                p.allow(false);
            });
        }
        let closed = self.tabs.remove(index).id;
        if self.tabs.is_empty() {
            // A fresh ID prevents a closed last page/draft being reopened.
            self.active = 0;
            self.new_tab(None, window, cx)?;
        } else {
            if index < self.active {
                self.active -= 1;
            }
            self.active = self.active.min(self.tabs.len() - 1);
            self.ensure_active(window, cx);
            if closing_active {
                if let Some(page) = self.page() {
                    page.update(cx, |p, _| p.invalidate_automation());
                }
            }
        }
        self.persist(Some(&closed));
        cx.notify();
        Ok(json!({"closed":closed,"active_tab":self.tabs[self.active].id}))
    }
    pub fn open(
        &mut self,
        target: &str,
        cwd: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.page()
            .ok_or("No active page")?
            .update(cx, |p, cx| p.open(target, cwd, window, cx))
    }
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.page() {
            p.update(cx, |p, cx| p.reload(cx));
        }
    }
    pub fn navigate_history(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.page() {
            p.update(cx, |p, _| p.navigate_history(forward));
        }
    }
    pub fn navigate_artifact_history(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        if let Some(p) = self.page() {
            p.update(cx, |p, cx| p.navigate_artifact_history(forward, window, cx));
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            self.navigate_history(forward, cx);
        }
    }
    pub fn configure_view(
        &mut self,
        args: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.page()
            .ok_or("No active page")?
            .update(cx, |p, cx| p.configure_view(args, window, cx))
    }
    pub fn automate(
        &mut self,
        command: &str,
        args: &Value,
        reply: std::sync::mpsc::Sender<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = if let Some(id) = args.get("tab") {
            let Some(index) = id
                .as_str()
                .and_then(|id| self.tabs.iter().position(|t| t.id == id))
            else {
                let _=reply.send(json!({"error":"Browser tab not found; use a tab id from status or browser.tab.new."}));
                return;
            };
            index
        } else {
            self.active
        };
        self.ensure_page(index, window, cx);
        if let Some(page) = &self.tabs[index].page {
            page.update(cx, |p, cx| p.automate(command, args, reply, cx));
        } else {
            let _ = reply.send(json!({"error":"No browser page"}));
        }
    }
    pub fn status(&self, cx: &App) -> Value {
        let mut status = self
            .page()
            .map(|p| p.read(cx).status())
            .unwrap_or_else(|| json!({"supported":cfg!(target_os="macos")}));
        status["active_tab"] = json!(self.tabs.get(self.active).map(|t| &t.id));
        status["tabs"] = json!(self.tabs.iter().enumerate().map(|(i,t)| {
            let page = t.page.as_ref().map(|p| p.read(cx).status());
            json!({"id":t.id,"active":i==self.active,"url":page.as_ref().and_then(|s| s["url"].as_str()).unwrap_or_else(|| t.saved_url.as_str()),"title":page.as_ref().and_then(|s|s["title"].as_str()).unwrap_or_else(||t.saved_title.as_str()),"visible":page.as_ref().and_then(|s|s["visible"].as_bool()).unwrap_or(false),"materialized":t.page.is_some()})
        }).collect::<Vec<_>>());
        status
    }
    #[cfg(all(debug_assertions, target_os = "macos"))]
    pub fn test_probe(&self, script: &str, cx: &App) -> Result<(), String> {
        self.page().ok_or("No page")?.read(cx).test_probe(script)
    }
    #[cfg(all(debug_assertions, target_os = "macos"))]
    pub fn test_native_focus(&self, action: &str, cx: &App) -> Value {
        self.page()
            .map(|p| p.read(cx).test_native_focus(action))
            .unwrap_or_else(|| json!({"error":"No page"}))
    }
}

impl Render for PagePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.status(cx);
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .h(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("page-tab-strip")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .gap(px(3.))
                            .overflow_x_scroll()
                            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                                let id = tab.id.clone();
                                let close_id = id.clone();
                                let data = &state["tabs"][index];
                                let title = data["title"]
                                    .as_str()
                                    .filter(|s| !s.is_empty())
                                    .or_else(|| data["url"].as_str().filter(|s| !s.is_empty()))
                                    .unwrap_or("새 탭")
                                    .to_owned();
                                div()
                                    .id(SharedString::from(format!("page-tab-{id}")))
                                    .w(px(158.))
                                    .min_w(px(80.))
                                    .flex_none()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .px(px(8.))
                                    .rounded(px(6.))
                                    .bg(settings::color(if index == self.active {
                                        "selected"
                                    } else {
                                        "chrome"
                                    }))
                                    .text_color(settings::color("foreground"))
                                    .text_size(px(12.))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        let _ = this.select_tab(&id, window, cx);
                                    }))
                                    .child(div().flex_1().min_w_0().truncate().child(title))
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "page-tab-close-{close_id}"
                                            )))
                                            .flex_none()
                                            .px(px(4.))
                                            .rounded(px(3.))
                                            .hover(|s| s.bg(settings::color("hover")))
                                            .child("×")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                let _ = this.close_tab(Some(&close_id), window, cx);
                                            })),
                                    )
                            })),
                    )
                    .child(
                        div()
                            .id("page-tab-add")
                            .w(px(30.))
                            .h(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(5.))
                            .cursor_pointer()
                            .bg(settings::color("chrome"))
                            .text_color(settings::color("foreground"))
                            .child("+")
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Err(error) = this.new_tab(None, window, cx) {
                                    this.error = Some(error);
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .children(self.error.as_ref().map(|error| {
                div()
                    .flex_none()
                    .px(px(8.))
                    .text_color(settings::color("blocked"))
                    .child(error.clone())
            }))
            .child(div().flex_1().min_w_0().min_h_0().children(self.page()))
    }
}

#[cfg(test)]
mod tests {
    use super::restored_tabs;
    use serde_json::json;
    #[test]
    fn restores_legacy_and_validates_tab_manifest_without_losing_other_spaces() {
        assert_eq!(
            restored_tabs(&json!({"browser":"https://example.com"}), "browser"),
            (vec!["browser".to_owned()], 0)
        );
        let state = json!({"_tabs":{"sp1":{"order":["first","second","first","../escape","_view",42],"active":"second"},"sp2":{"order":["other"],"active":"other"}}});
        // Reserved underscore keys may not become a page URL key.
        let (ids, active) = restored_tabs(&state, "sp1");
        assert_eq!(ids, vec!["first".to_owned(), "second".to_owned()]);
        assert_eq!(active, 1);
        assert!(!ids.iter().any(|id| id.contains('/')));
        assert_eq!(ids.iter().filter(|id| *id == "first").count(), 1);
        assert_eq!(restored_tabs(&state, "sp2"), (vec!["other".to_owned()], 0));
    }
}
