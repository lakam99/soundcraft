//! The web editor keeps its existing build; native HTTPS and OS credentials stay native.
use crate::SoundApp;
use serde_json::Value;
#[derive(Default)]
pub(crate) struct Runtime {}
pub fn command(_app: &mut SoundApp, id: &str, _params: &Value) -> Option<Result<Value, String>> {
    id.starts_with("ai.").then(|| Err("AI Chat requires the native desktop editor".into()))
}
pub fn document_replaced(_app: &mut SoundApp) {}
pub fn poll(_app: &mut SoundApp, _ctx: &egui::Context) {}
pub fn begin_frame(_app: &mut SoundApp, _ui: &egui::Ui) {}
pub fn register(_app: &mut SoundApp, _id: &str, _response: &egui::Response, _label: &str) {}
pub fn register_area(_app: &mut SoundApp, _id: &str, _rect: egui::Rect, _ctx: &egui::Context, _label: &str) {}
pub fn paint_guidance(_app: &mut SoundApp, _ctx: &egui::Context) {}
pub fn show(_app: &mut SoundApp, ui: &mut egui::Ui) {
    ui.label("AI Chat is available in the desktop editor.");
}
pub fn settings(app: &mut SoundApp, ui: &mut egui::Ui) {
    show(app, ui);
}

pub fn elements(_app: &SoundApp) -> Value {
    serde_json::json!([])
}
