//! Material-backed rubber leads with gravity sag and image-backed metal/rubber connectors.
use super::{
    hybrid::{Design, Source},
    skin::{BRASS, CYAN},
};
use egui::{Color32, Pos2, Rect, Stroke, TextureId, Ui, pos2, vec2};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
pub(super) struct Layout {
    offsets: BTreeMap<String, egui::Vec2>,
    drag: Option<String>,
    focus: Option<String>,
}
pub(super) struct Painted {
    pub unplug: Option<(bool, usize)>,
    pub handle: Option<(String, egui::Response, String)>,
}
impl Layout {
    pub(super) fn command(&mut self, d: &Design, p: &serde_json::Value) -> Result<serde_json::Value, String> {
        let keys: Vec<_> = d
            .routes
            .iter()
            .enumerate()
            .map(|(i, r)| format!("false:{i}:a:{}:i:{}", r.from, r.to))
            .chain(d.modulation.iter().enumerate().map(|(i, m)| format!("true:{i}:{}:p:{}:{}", source_key(&m.source), m.target, m.param)))
            .collect();
        self.offsets.retain(|key, _| keys.contains(key));
        if let Some(key) = p.get("key").and_then(serde_json::Value::as_str) {
            if !keys.iter().any(|k| k == key) {
                return Err("Unknown patch lead; inspect the current cable keys".into());
            }
            if let Some(value) = p.get("offset") {
                let value: [f32; 2] = serde_json::from_value(value.clone()).map_err(|_| "offset must be [x,y]")?;
                if value.iter().any(|v| !v.is_finite() || v.abs() > 4096.0) {
                    return Err("Cable offset must be finite and within +/-4096 pixels".into());
                }
                self.offsets.insert(key.into(), vec2(value[0], value[1]));
            }
            if p.get("reset").and_then(serde_json::Value::as_bool) == Some(true) {
                self.offsets.remove(key);
            }
        }
        Ok(serde_json::json!(
            keys.iter()
                .map(|key| {
                    let offset = self.offsets.get(key).copied().unwrap_or_default();
                    serde_json::json!({"key":key,"offset":[offset.x,offset.y]})
                })
                .collect::<Vec<_>>()
        ))
    }
}
fn drape(a: Pos2, b: Pos2, index: usize, clip: Rect, key: &str, layout: &Layout) -> [Pos2; 4] {
    let mut points = curve(a, b, index, clip);
    let center = clip.center().x;
    let lane = (index % 4) as f32 * 11.0;
    let gutter = if key.contains("macro:") {
        clip.right() - 16.0 - lane
    } else if (a.x - center) * (b.x - center) <= 0.0 {
        center + lane - 16.0
    } else if a.x < center {
        clip.left() + 16.0 + lane
    } else {
        clip.right() - 16.0 - lane
    };
    let offset = layout.offsets.get(key).copied().unwrap_or_default();
    if let Some(point) = points.get_mut(1) {
        point.x = (gutter + offset.x).clamp(clip.left() + 4.0, clip.right().max(clip.left() + 8.0) - 4.0);
        point.y = a.y + 76.0 + offset.y;
    }
    if let Some(point) = points.get_mut(2) {
        point.x = (gutter + offset.x).clamp(clip.left() + 4.0, clip.right().max(clip.left() + 8.0) - 4.0);
        point.y = b.y + 150.0 + offset.y;
    }
    points
}

fn source_key(source: &Source) -> String {
    match source {
        Source::Module(id) => format!("c:{id}"),
        Source::Macro(n) => format!("macro:{n}"),
        Source::Velocity => "velocity".into(),
        Source::Key => "key".into(),
    }
}
pub(super) fn curve(a: Pos2, b: Pos2, index: usize, clip: Rect) -> [Pos2; 4] {
    let sag = (70.0 + a.distance(b) * 0.22).clamp(80.0, 260.0);
    let lateral = if (a.x - b.x).abs() < (a.y - b.y).abs() * 0.7 { if index.is_multiple_of(2) { 70.0 } else { -70.0 } } else { 0.0 };
    let a = a + vec2(0.0, 36.0);
    let b = b + vec2(0.0, 36.0);
    let left = clip.left() + 12.0;
    let right = clip.right().max(left + 24.0) - 12.0;
    [a, pos2((a.x + lateral).clamp(left, right), a.y + sag), pos2((b.x + lateral).clamp(left, right), b.y + sag), b]
}
fn sample(points: [Pos2; 4], t: f32) -> Pos2 {
    let [a, c1, c2, b] = points;
    let u = 1.0 - t;
    (a.to_vec2() * u * u * u + c1.to_vec2() * 3.0 * u * u * t + c2.to_vec2() * 3.0 * u * t * t + b.to_vec2() * t * t * t).to_pos2()
}
fn draw_cord(p: &egui::Painter, points: [Pos2; 4], color: Color32, texture: Option<TextureId>, hovered: bool) {
    let centers: Vec<_> = (0..=96).map(|i| sample(points, i as f32 / 96.0)).collect();
    let shadow: Vec<_> = centers.iter().map(|p| *p + vec2(2.5, 4.0)).collect();
    p.add(egui::Shape::line(shadow, Stroke::new(13.0, Color32::from_black_alpha(105))));
    p.add(egui::Shape::line(
        centers.clone(),
        Stroke::new(if hovered { 11.0 } else { 9.5 }, if hovered { color } else { Color32::from_rgb(29, 32, 29) }),
    ));
    let Some(texture) = texture else { return };
    let mut mesh = egui::Mesh::with_texture(texture);
    for (i, &center) in centers.iter().enumerate() {
        let previous = centers.get(i.saturating_sub(1)).copied().unwrap_or(center);
        let next = centers.get(i + 1).copied().unwrap_or(center);
        let tangent = (next - previous).normalized();
        let normal = vec2(-tangent.y, tangent.x) * 5.0;
        let u = i as f32 / 96.0;
        mesh.vertices.push(egui::epaint::Vertex { pos: center + normal, uv: pos2(u, 0.405), color });
        mesh.vertices.push(egui::epaint::Vertex { pos: center - normal, uv: pos2(u, 0.595), color });
        if i > 0 {
            let b = (i * 2) as u32;
            mesh.indices.extend_from_slice(&[b - 2, b - 1, b, b - 1, b + 1, b]);
        }
    }
    p.add(mesh);
}
fn plug(p: &egui::Painter, center: Pos2, texture: Option<TextureId>, color: Color32) {
    if let Some(id) = texture {
        p.image(id, Rect::from_min_size(center - vec2(25.0, 12.0), vec2(52.0, 52.0)), Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
    } else {
        p.circle_filled(center, 7.0, BRASS);
        p.line_segment([center, center + vec2(0.0, 36.0)], Stroke::new(9.0, color));
    }
    // A small insulation band identifies the lead without changing its brass material.
    p.line_segment([center + vec2(-3.0, 27.0), center + vec2(3.0, 27.0)], Stroke::new(2.0, color));
}
pub(super) fn paint(
    ui: &Ui,
    d: &Design,
    positions: &BTreeMap<String, Pos2>,
    clip: Rect,
    textures: (Option<TextureId>, Option<TextureId>),
    blocked: &[Rect],
    layout: &mut Layout,
) -> Painted {
    let leads: Vec<_> = d
        .routes
        .iter()
        .enumerate()
        .map(|(i, r)| (false, i, format!("a:{}", r.from), format!("i:{}", r.to), BRASS))
        .chain(d.modulation.iter().enumerate().map(|(i, m)| (true, i, source_key(&m.source), format!("p:{}:{}", m.target, m.param), CYAN)))
        .filter_map(|(modulation, index, from, to, color)| {
            Some((modulation, index, from.clone(), to.clone(), *positions.get(&from)?, *positions.get(&to)?, color))
        })
        .collect();
    let keys: Vec<_> = leads.iter().map(|(m, i, from, to, _, _, _)| format!("{m}:{i}:{from}:{to}")).collect();
    layout.offsets.retain(|key, _| keys.contains(key));
    if !ui.input(|i| i.pointer.primary_down()) {
        layout.drag = None;
    }
    if layout.drag.is_none()
        && layout.focus.as_ref().is_some_and(|key| ui.ctx().memory(|m| m.focused()) != Some(egui::Id::unique(("forge-cable", key))))
    {
        layout.focus = None;
    }
    let pointer = ui.input(|i| i.pointer.hover_pos()).filter(|p| clip.contains(*p));
    let control = pointer.is_some_and(|p| blocked.iter().any(|r| r.contains(p)));
    let mut nearest = None;
    let mut distance = 9.0f32;
    for (n, (_, _, _, _, a, b, _)) in leads.iter().enumerate() {
        let Some(key) = keys.get(n) else { continue };
        let points = drape(*a, *b, n, clip, key, layout);
        if let Some(pointer) = pointer {
            for i in 0..=96 {
                let d = pointer.distance(sample(points, i as f32 / 96.0));
                if d <= distance {
                    distance = d;
                    nearest = Some(n);
                }
            }
        }
    }
    let active = layout.drag.as_ref().or(layout.focus.as_ref()).and_then(|key| keys.iter().position(|k| k == key));
    let selected = if layout.drag.is_some() {
        active
    } else if !control {
        nearest.or(active)
    } else {
        None
    };
    let mut handle = None;
    if let Some(n) = selected
        && let Some(key) = keys.get(n)
        && let Some((_, _, from, to, a, b, _)) = leads.get(n)
    {
        let at = pointer.unwrap_or_else(|| sample(drape(*a, *b, n, clip, key, layout), 0.5));
        let id = egui::Id::unique(("forge-cable", key));
        let response = ui.interact(Rect::from_center_size(at, vec2(18.0, 18.0)), id, egui::Sense::click_and_drag());
        if response.clicked() || response.drag_started() || response.is_pointer_button_down_on() {
            response.request_focus();
            layout.focus = Some(key.clone());
        }
        if response.drag_started() || response.is_pointer_button_down_on() {
            layout.drag = Some(key.clone());
        }
        let mut delta = if response.dragged() { response.drag_delta() } else { egui::Vec2::ZERO };
        if response.has_focus() {
            ui.input_mut(|i| {
                for (k, d) in [
                    (egui::Key::ArrowLeft, vec2(-8.0, 0.0)),
                    (egui::Key::ArrowRight, vec2(8.0, 0.0)),
                    (egui::Key::ArrowUp, vec2(0.0, -8.0)),
                    (egui::Key::ArrowDown, vec2(0.0, 8.0)),
                ] {
                    if i.consume_key(egui::Modifiers::NONE, k) {
                        delta += d;
                    }
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Home) {
                    layout.offsets.remove(key);
                }
            });
        }
        if delta.is_finite() && delta != egui::Vec2::ZERO {
            let offset = layout.offsets.entry(key.clone()).or_default();
            *offset += delta;
            offset.x = offset.x.clamp(-clip.width().max(1.0), clip.width().max(1.0));
            offset.y = offset.y.clamp(-clip.height().max(1.0), clip.height().max(1.0));
        }
        if response.drag_stopped() {
            layout.drag = None;
        }
        if response.lost_focus() {
            layout.focus = None;
        }
        let label = format!("Patch lead {from} to {to}; drag to move aside, right-click to unplug");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, &label));
        handle = Some((format!("soundlab.cable.{key}"), response, label));
    }
    let p = ui.painter().with_clip_rect(clip);
    let mut occupied = BTreeSet::new();
    let mut unplug = None;
    for (n, (modulation, index, from, to, a, b, color)) in leads.iter().enumerate() {
        let hovered = selected == Some(n);
        let Some(key) = keys.get(n) else { continue };
        let points = drape(*a, *b, n, clip, key, layout);
        draw_cord(&p, points, *color, textures.0, hovered);
        occupied.insert(from.clone());
        occupied.insert(to.clone());
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            p.circle_stroke(*a, 12.0, Stroke::new(2.0, *color));
            p.circle_stroke(*b, 12.0, Stroke::new(2.0, *color));
            if !control && ui.input(|i| i.pointer.secondary_clicked()) {
                unplug = Some((*modulation, *index));
            }
        }
    }
    for id in occupied {
        if let Some(&center) = positions.get(&id) {
            plug(&p, center, textures.1, if id.starts_with('a') || id.starts_with('i') { BRASS } else { CYAN });
        }
    }
    Painted { unplug, handle }
}
pub(super) fn loose(ui: &Ui, from: Pos2, to: Pos2, clip: Rect, textures: (Option<TextureId>, Option<TextureId>), color: Color32) {
    let p = ui.painter().with_clip_rect(clip);
    let points = curve(from, to - vec2(0.0, 36.0), 1, clip);
    draw_cord(&p, points, color, textures.0, true);
    plug(&p, from, textures.1, color);
    plug(&p, to - vec2(0.0, 36.0), textures.1, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vertical_cable_has_a_real_bow_and_mesh_endpoints_align_with_plug_boots() {
        let a = pos2(100.0, 100.0);
        let b = pos2(100.0, 500.0);
        let points = curve(a, b, 0, Rect::from_min_max(Pos2::ZERO, pos2(800.0, 900.0)));
        assert_eq!(sample(points, 0.0), a + vec2(0.0, 36.0));
        assert_eq!(sample(points, 1.0), b + vec2(0.0, 36.0));
        assert!(sample(points, 0.5).x > 125.0);
        assert!((0..=96).all(|i| sample(points, i as f32 / 96.0).is_finite()));
    }
    #[test]
    fn forge_cable_drag_changes_only_drape_and_keeps_sockets_and_graph_fixed() {
        let mut d = super::super::hybrid::Patch::starter().design().clone();
        d.routes = vec![super::super::hybrid::Route { from: "osc_a".into(), to: "filter".into(), gain: 1.0 }];
        d.modulation.clear();
        let before = d.clone();
        let clip = Rect::from_min_max(Pos2::ZERO, pos2(800.0, 650.0));
        let a = pos2(100.0, 100.0);
        let b = pos2(600.0, 400.0);
        let key = "false:0:a:osc_a:i:filter";
        let point = sample(drape(a, b, 0, clip, key, &Layout::default()), 0.5);
        let mut h = egui_kittest::Harness::builder().with_size(vec2(800.0, 650.0)).build_ui_state(
            move |ui, layout: &mut Layout| {
                egui::Window::new("FORGE gesture test").title_bar(false).default_pos(Pos2::ZERO).default_size(vec2(780.0, 630.0)).show(
                    ui.ctx(),
                    |ui| {
                        let positions = BTreeMap::from([("a:osc_a".into(), a), ("i:filter".into(), b)]);
                        let result = paint(ui, &d, &positions, clip, (None, None), &[], layout);
                        assert!(result.unplug.is_none());
                    },
                );
            },
            Layout::default(),
        );
        h.run_steps(2);
        h.hover_at(point);
        h.run_steps(2);
        h.event(egui::Event::PointerButton { pos: point, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        h.hover_at(point + vec2(-40.0, 0.0));
        h.run_steps(1);
        h.hover_at(point + vec2(-80.0, 0.0));
        h.run_steps(1);
        h.event(egui::Event::PointerButton {
            pos: point + vec2(-80.0, 0.0),
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(2);
        assert!(h.state().offsets.get(key).is_some_and(|v| v.x < -30.0));
        let after = drape(a, b, 0, clip, key, h.state());
        assert_eq!(sample(after, 0.0), a + vec2(0.0, 36.0));
        assert_eq!(sample(after, 1.0), b + vec2(0.0, 36.0));
        assert_ne!(sample(after, 0.5), point);
        assert_eq!(before.routes[0].gain, 1.0);
    }
}
