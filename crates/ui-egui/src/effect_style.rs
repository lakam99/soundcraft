//! Original procedural materials and accessible rotary controls for built-in effects.
use crate::{SoundApp, theme::Tokens};
use egui::{Color32, Key, Response, Sense, Stroke, Ui, vec2};
use soundcraft_dsp::{Category, ParamInfo, PluginInfo};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skin {
    #[default]
    Auto,
    Graphite,
    Brushed,
    Porcelain,
    Ember,
}
impl Skin {
    pub const ALL: [Self; 5] = [Self::Auto, Self::Graphite, Self::Brushed, Self::Porcelain, Self::Ember];
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Effect identity",
            Self::Graphite => "Graphite",
            Self::Brushed => "Brushed alloy",
            Self::Porcelain => "Porcelain",
            Self::Ember => "Ember",
        }
    }
    pub fn resolved(self, info: &PluginInfo) -> Self {
        if self != Self::Auto {
            return self;
        }
        match info.category {
            Category::Reverb | Category::Modulation => Self::Porcelain,
            Category::Dynamics | Category::Delay => Self::Brushed,
            Category::Harmonic | Category::PitchShift => Self::Ember,
            _ => Self::Graphite,
        }
    }
    pub fn radius(self) -> u8 {
        match self {
            Self::Graphite => 3,
            Self::Brushed => 5,
            Self::Ember => 8,
            _ => 16,
        }
    }
    pub fn face(self) -> Color32 {
        let t = Tokens::current();
        match self {
            Self::Brushed => t.panel_bg.lerp_to_gamma(t.text_dim, 0.17),
            Self::Porcelain => t.panel_bg.lerp_to_gamma(t.text_dim, 0.09),
            Self::Ember => t.panel_bg.lerp_to_gamma(Color32::from_rgb(170, 88, 37), 0.14),
            _ => t.panel_bg,
        }
    }
    pub fn card(self) -> Color32 {
        let t = Tokens::current();
        match self {
            Self::Brushed => t.slot_bg,
            Self::Porcelain => t.panel_bg2.lerp_to_gamma(t.text_dim, 0.09),
            Self::Ember => t.panel_bg2.lerp_to_gamma(Color32::from_rgb(170, 88, 37), 0.1),
            _ => t.panel_bg2,
        }
    }
}
pub fn skin(app: &SoundApp, info: &PluginInfo) -> Skin {
    app.ui.effect_skins.get(info.id).copied().unwrap_or_default().resolved(info)
}

/// Relative vertical travel, normalized through DSP metadata; plain scrolling never changes sound.
pub fn knob(ui: &mut Ui, param: &ParamInfo, current: &mut f32, accent: Color32, skin: Skin) -> Response {
    let before = *current;
    let (rect, mut response) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::click_and_drag());
    if response.clicked() {
        response.request_focus();
    }
    let fine = ui.input(|i| i.modifiers.shift);
    let step = if fine { 0.001 } else { 0.01 };
    let mut normalized = param.to_normalized(*current);
    let initial_normalized = normalized;
    if response.dragged() {
        normalized -= response.drag_delta().y * if fine { 0.0005 } else { 0.005 };
    }
    if response.hovered() && ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        normalized += wheel * if fine { 0.0001 } else { 0.001 };
        if wheel != 0.0 {
            ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
        }
    }
    if response.has_focus() {
        ui.input_mut(|i| {
            for (key, direction) in [(Key::ArrowUp, 1.0), (Key::ArrowRight, 1.0), (Key::ArrowDown, -1.0), (Key::ArrowLeft, -1.0)] {
                if i.consume_key(egui::Modifiers::NONE, key) || i.consume_key(egui::Modifiers::SHIFT, key) {
                    normalized += direction * step;
                }
            }
            if i.consume_key(egui::Modifiers::NONE, Key::Home) {
                normalized = param.to_normalized(param.default);
            }
        });
    }
    if response.double_clicked() {
        *current = param.default;
    } else if normalized != initial_normalized {
        *current = param.from_normalized(normalized);
    }
    if *current != before {
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), f64::from(*current), param.name));
    let t = Tokens::current();
    let p = ui.painter();
    let center = rect.center();
    let radius = 23.0;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let point = |angle: f32, r: f32| center + vec2(angle.cos(), angle.sin()) * r;
    let fraction = param.to_normalized(*current);
    let segments = if skin == Skin::Ember { 24 } else { 40 };
    for i in 0..segments {
        let a = start + sweep * i as f32 / segments as f32;
        let b = start + sweep * (i as f32 + if skin == Skin::Ember { 0.64 } else { 1.0 }) / segments as f32;
        let color = if (i as f32 / segments as f32) <= fraction { accent } else { t.border };
        p.line_segment([point(a, radius + 5.0), point(b, radius + 5.0)], Stroke::new(if skin == Skin::Ember { 3.0 } else { 2.0 }, color));
    }
    p.circle_filled(center + vec2(0.0, 2.0), radius, Color32::BLACK.gamma_multiply(0.25));
    let porcelain = skin == Skin::Porcelain;
    p.circle_filled(center, radius, if porcelain { skin.card().lerp_to_gamma(t.text_dim, 0.15) } else { skin.card() });
    p.circle_stroke(center, radius, Stroke::new(if porcelain { 1.5 } else { 1.0 }, if porcelain { t.text_dim } else { t.border }));
    if skin == Skin::Brushed {
        for y in (-15..=15).step_by(3) {
            let yf = y as f32;
            let half = (radius * radius - yf * yf).max(0.0).sqrt() - 2.0;
            p.line_segment([center + vec2(-half, yf), center + vec2(half, yf)], Stroke::new(0.5, t.text_dim.gamma_multiply(0.15)));
        }
    }
    if skin == Skin::Graphite {
        for i in 0..12 {
            let a = i as f32 * std::f32::consts::TAU / 12.0;
            p.line_segment([point(a, 17.0), point(a, 20.0)], Stroke::new(1.0, t.border));
        }
    }
    let angle = start + sweep * fraction;
    p.line_segment([point(angle, 8.0), point(angle, 19.0)], Stroke::new(3.0, accent));
    if response.has_focus() {
        p.rect_stroke(rect, 4.0, Stroke::new(1.5, accent), egui::StrokeKind::Inside);
    }
    response.on_hover_text(format!(
        "{}: {}\nDrag up/down · Shift: fine · Ctrl/⌘ + wheel · arrows · double-click: reset",
        param.name,
        param.format(*current)
    ))
}

pub(crate) fn command(app: &mut SoundApp, id: &str, params: &serde_json::Value) -> Option<Result<serde_json::Value, String>> {
    if id != "effects.skin" {
        return None;
    }
    Some((|| {
        let plugin = params
            .get("plugin")
            .and_then(serde_json::Value::as_str)
            .filter(|id| soundcraft_dsp::plugins().iter().any(|info| info.id == *id && !info.is_instrument))
            .ok_or("Choose a built-in effect")?;
        let skin: Skin = serde_json::from_value(params.get("skin").cloned().ok_or("skin required")?).map_err(|_| "Unknown effect skin")?;
        app.ui.effect_skins.insert(plugin.to_owned(), skin);
        Ok(serde_json::json!({"plugin":plugin,"skin":skin}))
    })())
}
