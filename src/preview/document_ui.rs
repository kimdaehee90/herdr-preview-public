//! Native controls and background jobs for Preview's static documents.
use super::{Event, Preview, color, target};
use crate::preview::{document::Document, paging::parse_page};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_base::input::Input;
use notify::Watcher as _;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

impl Preview {
    pub(super) fn begin_document(
        &mut self,
        paths: Vec<PathBuf>,
        page: usize,
        cx: &mut Context<Self>,
    ) {
        self.document_cancel.store(true, Ordering::Relaxed);
        self.document_cancel = Arc::new(AtomicBool::new(false));
        self.document_epoch += 1;
        self.generation += 1;
        self.fit_generation += 1;
        let epoch = self.document_epoch;
        self.document = None;
        self.document_page = 0;
        self.document_requested_page = page;
        self.document_pending = true;
        self.loading = true;
        self.error = None;
        self.dirty = false;
        self.server = None;
        self.watcher = None;
        self.browser.borrow_mut().hide();
        if let Some(view) = &self.browser.borrow().view {
            let _ = view.load_url("about:blank");
        }
        self.document_sources = paths.clone();
        let events = self.events.clone();
        let cancelled = self.document_cancel.clone();
        cx.background_executor()
            .spawn(async move {
                let result = (|| {
                    let doc = Arc::new(Document::prepare(&paths, &cancelled)?);
                    let page = page.clamp(1, doc.page_count());
                    let image = doc.render(page)?;
                    if cancelled.load(Ordering::Relaxed) {
                        return Err("문서 열기가 취소되었습니다.".into());
                    }
                    Ok((doc, page, image))
                })();
                let _ = events.try_send(Event::DocumentReady(epoch, result));
            })
            .detach();
        cx.notify();
    }

    pub fn open_selection(
        &mut self,
        values: &[String],
        cwd: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let result = (|| {
            if values.is_empty() {
                return Err("미리볼 파일을 선택하세요.".into());
            }
            if values.len() == 1 {
                return self.open(&values[0], cwd, window, cx);
            }
            let paths: Vec<PathBuf> = values
                .iter()
                .map(|v| target(v, cwd).map(PathBuf::from))
                .collect::<Result<_, _>>()?;
            if paths
                .iter()
                .any(|p| !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
            {
                return Err("여러 파일을 함께 열 때는 PNG 파일만 선택하세요.".into());
            }
            self.ensure_browser(window)?;
            self.target = paths[0].to_string_lossy().into_owned();
            let sources: Vec<String> = paths
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            self.remember_artifact(&self.target.clone(), &sources);
            self.title = format!(
                "{} 외 {}장",
                paths[0].file_name().unwrap_or_default().to_string_lossy(),
                paths.len() - 1
            );
            self.input
                .update(cx, |i, cx| i.set_value(self.target.clone(), window, cx));
            self.begin_document(paths, 1, cx);
            self.save_view();
            Ok(())
        })();
        if let Err(e) = &result {
            self.error = Some(e.clone());
            cx.notify();
        }
        result
    }

    pub(super) fn finish_document(
        &mut self,
        result: Result<(Arc<Document>, usize, PathBuf), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.document_pending = false;
        match result {
            Ok((doc, page, image)) => {
                self.document = Some(doc);
                self.watch_document();
                if let Err(error) = self.show_document_image(page, &image, window, cx) {
                    self.error = Some(error);
                    self.loading = false;
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.loading = false;
            }
        }
        cx.notify();
    }

    fn watch_document(&mut self) {
        let sources: Vec<PathBuf> = self
            .document_sources
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        let events = self.events.clone();
        let Ok(mut watcher) =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    if (event.kind.is_modify() || event.kind.is_create() || event.kind.is_remove())
                        && event.paths.iter().any(|p| sources.contains(p))
                    {
                        let _ = events.try_send(Event::Changed);
                    }
                }
            })
        else {
            return;
        };
        let mut folders = std::collections::HashSet::new();
        for path in &self.document_sources {
            if let Some(parent) = path.parent() {
                folders.insert(parent.to_owned());
            }
        }
        for folder in folders {
            if watcher
                .watch(&folder, notify::RecursiveMode::NonRecursive)
                .is_err()
            {
                return;
            }
        }
        self.watcher = Some(watcher);
    }

    pub(super) fn show_document_image(
        &mut self,
        page: usize,
        image: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc = self.document.as_ref().ok_or("문서를 먼저 열어주세요.")?;
        if self.server.as_ref().is_none_or(|s| s.root != doc.root()) {
            self.server = Some(super::Server::start(doc.root()).map_err(|e| e.to_string())?);
        }
        let url = self.server.as_ref().unwrap().url(image)?;
        self.browser
            .borrow()
            .view
            .as_ref()
            .ok_or("미리보기를 열지 못했습니다.")?
            .load_url(&url)
            .map_err(|e| e.to_string())?;
        self.document_page = page;
        self.document_requested_page = page;
        self.url = url;
        self.page_input
            .update(cx, |i, cx| i.set_value(page.to_string(), window, cx));
        self.start_loading(cx);
        self.save_view();
        Ok(())
    }

    pub(super) fn request_document_page(
        &mut self,
        page: usize,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let doc = self
            .document
            .clone()
            .ok_or("문서 준비가 끝난 뒤 페이지를 선택하세요.")?;
        let page = parse_page(&page.to_string(), doc.page_count())?;
        self.document_epoch += 1;
        let epoch = self.document_epoch;
        let events = self.events.clone();
        self.document_requested_page = page;
        self.loading = true;
        self.error = None;
        cx.background_executor()
            .spawn(async move {
                let result = doc.render(page).map(|image| (page, image));
                let _ = events.try_send(Event::PageReady(epoch, result));
            })
            .detach();
        cx.notify();
        Ok(())
    }

    pub(super) fn jump_document_input(&mut self, cx: &mut Context<Self>) {
        let text = self.page_input.read(cx).value().to_string();
        let result = parse_page(&text, self.document.as_ref().map_or(0, |d| d.page_count()))
            .and_then(|p| self.request_document_page(p, cx));
        if let Err(error) = result {
            self.error = Some(error);
            cx.notify();
        }
    }

    pub(super) fn document_controls(&self, cx: &Context<Self>) -> impl IntoElement {
        let total = self.document.as_ref().map_or(0, |d| d.page_count());
        let previous = self.document_page > 1 && !self.loading;
        let next = self.document_page < total && !self.loading;
        div()
            .id("document-pages")
            .h(px(34.))
            .px(px(8.))
            .gap(px(8.))
            .flex_none()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(color("border"))
            .child(div().text_color(color("muted")).child("페이지"))
            .child(
                div()
                    .id("document-previous")
                    .h(px(26.))
                    .px(px(8.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .opacity(if previous { 1. } else { 0.35 })
                    .hover(|s| s.bg(color("hover")))
                    .child("‹ 이전")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if previous {
                            let _ = this.request_document_page(this.document_page - 1, cx);
                        }
                    })),
            )
            .child(
                div()
                    .w(px(48.))
                    .h(px(26.))
                    .px(px(6.))
                    .rounded(px(4.))
                    .bg(color("input"))
                    .child(Input::new(&self.page_input)),
            )
            .child(div().text_color(color("muted")).child(if total == 0 {
                "/ …".to_owned()
            } else {
                format!("/ {total}")
            }))
            .child(
                div()
                    .id("document-next")
                    .h(px(26.))
                    .px(px(8.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .opacity(if next { 1. } else { 0.35 })
                    .hover(|s| s.bg(color("hover")))
                    .child("다음 ›")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if next {
                            let _ = this.request_document_page(this.document_page + 1, cx);
                        }
                    })),
            )
            .when(self.document_pending, |d| {
                d.child(div().text_color(color("muted")).child("슬라이드 준비 중…"))
            })
    }
}
