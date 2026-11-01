//! Artifact previews and a persistent browser. Pages cannot call Herdr APIs.
#[cfg(target_os = "macos")]
mod automation;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod server;

#[cfg(target_os = "macos")]
pub use macos::Preview;

#[cfg(not(target_os = "macos"))]
pub struct Preview;

#[cfg(not(target_os = "macos"))]
impl Preview {
    pub fn new(_: &str, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> Self {
        Self
    }
    pub fn new_browser(_: &mut gpui::Window, _: &mut gpui::Context<Self>) -> Self {
        Self
    }
    pub fn allow(&mut self, _: bool) {}
    pub fn context(&mut self, _: &str, _: &mut gpui::Window, _: &mut gpui::Context<Self>) {}
    pub fn reload(&mut self, _: &mut gpui::Context<Self>) {}
    pub fn navigate_history(&mut self, _: bool) {}
    pub fn configure_view(
        &mut self,
        _: &serde_json::Value,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        Err("Page display controls are available on macOS.".into())
    }
    pub fn automate(
        &self,
        _: &str,
        _: &serde_json::Value,
        reply: std::sync::mpsc::Sender<serde_json::Value>,
        _: &mut gpui::Context<Self>,
    ) {
        let _ =
            reply.send(serde_json::json!({"error":"Browser automation is available on macOS."}));
    }
    pub fn open(
        &mut self,
        _: &str,
        _: &str,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        Err("Interactive Preview is currently available in the macOS development build.".into())
    }
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"supported": false})
    }
}

#[cfg(not(target_os = "macos"))]
impl gpui::Render for Preview {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{ParentElement, Styled};
        gpui::div()
            .p_4()
            .child("Interactive Preview is available in the macOS development build.")
    }
}
