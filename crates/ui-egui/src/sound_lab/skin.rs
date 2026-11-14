//! Original image-backed material, with live vector dial indicators and readouts.
use egui::{Color32, Rect, Stroke, Ui, pos2, vec2};
pub const CREAM: Color32 = Color32::from_rgb(238, 225, 195);
pub const BRASS: Color32 = Color32::from_rgb(221, 181, 105);
pub const CYAN: Color32 = Color32::from_rgb(123, 230, 211);
#[derive(Default)]
pub struct Skin {
    panel: Option<egui::TextureHandle>,
    knob: Option<egui::TextureHandle>,
    cable: Option<egui::TextureHandle>,
    plug: Option<egui::TextureHandle>,
    attempted: bool,
}
fn texture(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    match image::load_from_memory(bytes) {
        Ok(image) => {
            // Generated material sizes can exceed a renderer's limit, especially on older GPUs.
            // Resize before load_texture: egui otherwise panics instead of rejecting the texture.
            let limit = u32::try_from(ctx.input(|i| i.max_texture_side).min(4096)).ok().filter(|n| *n > 0)?;
            let image = if image.width() > limit || image.height() > limit { image.thumbnail(limit, limit) } else { image };
            let rgba = image.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            Some(ctx.load_texture(name, egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()), egui::TextureOptions::LINEAR))
        }
        Err(e) => {
            log::warn!("FORGE material unavailable: {e}");
            None
        }
    }
}

impl Skin {
    pub fn ensure(&mut self, ctx: &egui::Context) {
        if !self.attempted {
            self.attempted = true;
            self.panel = texture(ctx, "forge-panel", include_bytes!("../../../../assets/sound-lab/forge-panel.png"));
            self.knob = texture(ctx, "forge-knob", include_bytes!("../../../../assets/sound-lab/forge-knob.png"));
            self.cable = texture(ctx, "forge-cable", include_bytes!("../../../../assets/sound-lab/forge-cable.png"));
            self.plug = texture(ctx, "forge-plug", include_bytes!("../../../../assets/sound-lab/forge-plug.png"));
        }
    }
    pub fn knob(&self) -> Option<egui::TextureId> {
        self.knob.as_ref().map(|t| t.id())
    }
    pub fn cable(&self) -> Option<egui::TextureId> {
        self.cable.as_ref().map(|t| t.id())
    }
    pub fn plug(&self) -> Option<egui::TextureId> {
        self.plug.as_ref().map(|t| t.id())
    }
    pub fn panel(&self, ui: &Ui, rect: Rect) {
        ui.painter().rect_filled(rect, 8.0, Color32::from_rgb(26, 43, 41));
        if let Some(texture) = &self.panel {
            // Nine slices preserve the round screws, wood rails and edge thickness when resized.
            let uvx = [0.0, 0.055, 0.945, 1.0];
            let uvy = [0.0, 0.075, 0.925, 1.0];
            let border = 32.0f32.min(rect.width() / 4.0).min(rect.height() / 4.0);
            let xs = [rect.left(), rect.left() + border, rect.right() - border, rect.right()];
            let ys = [rect.top(), rect.top() + border, rect.bottom() - border, rect.bottom()];
            for y in 0..3 {
                for x in 0..3 {
                    if let (Some(&left), Some(&right), Some(&top), Some(&bottom), Some(&u), Some(&v), Some(&u2), Some(&v2)) =
                        (xs.get(x), xs.get(x + 1), ys.get(y), ys.get(y + 1), uvx.get(x), uvy.get(y), uvx.get(x + 1), uvy.get(y + 1))
                    {
                        ui.painter().image(
                            texture.id(),
                            Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
                            Rect::from_min_max(pos2(u, v), pos2(u2, v2)),
                            Color32::WHITE,
                        );
                    }
                }
            }
        }
    }
}
pub fn plate() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(14, 25, 25, 244))
        .stroke(Stroke::new(1.0, Color32::from_rgb(76, 88, 77)))
        .corner_radius(5)
        .inner_margin(12)
}
pub fn knob(ui: &mut Ui, p: &soundcraft_dsp::ParamInfo, value: &mut f32, texture: Option<egui::TextureId>) -> egui::Response {
    let response = crate::effect_style::knob(ui, p, value, BRASS, crate::effect_style::Skin::Graphite);
    let rect = response.rect.shrink(6.0);
    let center = rect.center();
    if let Some(id) = texture {
        ui.painter().image(id, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    let angle = std::f32::consts::PI * (0.75 + 1.5 * p.to_normalized(*value));
    let dir = vec2(angle.cos(), angle.sin());
    ui.painter().line_segment([center + dir * 10.0, center + dir * 22.0], Stroke::new(4.0, Color32::from_rgb(24, 24, 21)));
    ui.painter().line_segment([center + dir * 10.0, center + dir * 22.0], Stroke::new(2.0, CREAM));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forge_materials_fit_a_small_renderer_without_panicking() {
        let ctx = egui::Context::default();
        let mut skin = Skin::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut output = ctx.run_ui(egui::RawInput { max_texture_side: Some(1024), ..Default::default() }, |ui| skin.ensure(ui.ctx()));
            output.textures_delta.clear();
        }));
        assert!(result.is_ok());
        for texture in [skin.panel, skin.knob, skin.cable, skin.plug].into_iter().flatten() {
            let [w, h] = texture.size();
            assert!(w <= 1024 && h <= 1024);
        }
    }
}
