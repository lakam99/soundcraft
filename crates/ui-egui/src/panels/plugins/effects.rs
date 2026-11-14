//! Designed editors for the built-in effects. Parameters still use the engine command path.
use crate::{SoundApp, theme::Tokens};
use egui::{Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde_json::json;
use soundcraft_dsp::{ParamInfo, PluginInfo, Unit};
use soundcraft_model::{Insert, TrackId};
mod identity;
pub(super) use identity::identity;

pub(super) fn show(app: &mut SoundApp, ui: &mut Ui, track: TrackId, slot: usize, info: &PluginInfo, ins: &Insert) {
    let Some(style) = identity(info.id) else { return };
    let t = Tokens::current();
    let accent =
        if crate::theme::is_light() { crate::theme::rgb(style.accent).lerp_to_gamma(Color32::BLACK, 0.42) } else { crate::theme::rgb(style.accent) };
    let skin = crate::effect_style::skin(app, info);
    let compact = ui.available_height() < 520.0 || ui.ctx().content_rect().height() < 720.0;
    ui.style_mut().visuals.selection.bg_fill = accent.lerp_to_gamma(Color32::BLACK, 0.68);
    ui.style_mut().visuals.widgets.hovered.bg_stroke.color = accent;
    ui.style_mut().visuals.widgets.active.bg_stroke.color = accent;
    ui.set_min_width(360.0);
    ui.spacing_mut().item_spacing = vec2(10.0, 10.0);
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    ui.horizontal(|ui| {
        let response =
            ui.add(egui::Button::new(if ins.bypass { "Bypassed" } else { "Bypass" }).selected(ins.bypass)).accessible_name("Bypass effect");
        crate::assistant::register(app, &format!("effect.{}.{}.bypass", track.0, slot), &response, "Bypass effect");
        if response.clicked() {
            let _ = app.run("mix.insert_bypass", json!({"track":track.0,"slot":slot}));
        }
        let response = ui.menu_button("Presets…", |ui| super::preset_menu(app, ui, track, slot, info, ins)).response;
        crate::assistant::register(app, &format!("effect.{}.{}.presets", track.0, slot), &response, "Effect presets");
        ui.add(egui::Label::new(RichText::new(&ins.preset).small().color(t.text_dim)).truncate());
    });
    let mut selected = app.ui.effect_skins.get(info.id).copied().unwrap_or_default();
    let response = craft_ui::ComboBox::from_id_salt(("effect-skin", track.0, slot))
        .selected_text(selected.label())
        .show_ui(ui, |ui| {
            for choice in crate::effect_style::Skin::ALL {
                ui.selectable_value(&mut selected, choice, choice.label());
            }
        })
        .response
        .accessible_name("Effect skin");
    crate::assistant::register(app, &format!("effect.{}.{}.skin", track.0, slot), &response, "Effect skin");
    if selected != app.ui.effect_skins.get(info.id).copied().unwrap_or_default() {
        let _ = app.run("effects.skin", json!({"plugin":info.id,"skin":selected}));
    }
    // Long effects scroll inside the native window, including on small displays.
    egui::ScrollArea::vertical()
        .id_salt(("effect-body", track.0, slot))
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height((ui.ctx().content_rect().height() - 170.0).max(180.0))
        .show(ui, |ui| {
            egui::Frame::new()
                .fill(skin.card().lerp_to_gamma(accent, 0.07))
                .corner_radius(skin.radius())
                .stroke(Stroke::new(1.0, t.border))
                .inner_margin(if compact { 10 } else { 14 })
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        let width = (ui.available_width() - 142.0).max(160.0);
                        ui.vertical(|ui| {
                            ui.set_width(width);
                            ui.label(RichText::new(style.series).size(11.0).strong().color(t.text_dim));
                            ui.label(RichText::new(info.name).size(if compact { 20.0 } else { 23.0 }).strong()).on_hover_text(style.purpose);
                            if !compact {
                                ui.label(RichText::new(style.purpose).size(12.0).color(t.text_dim));
                            }
                        });
                        ui.vertical(|ui| {
                            let (rect, _) = ui.allocate_exact_size(vec2(126.0, if compact { 44.0 } else { 72.0 }), Sense::hover());
                            sketch(ui, rect, info, ins, accent);
                            ui.label(RichText::new("Settings illustration").size(10.0).color(t.text_dim));
                        });
                    });
                });
            if matches!(info.id, "eq_1band" | "eq_7band") {
                response_plot(ui, info, ins, accent, app.engine.session().sample_rate.as_f64() as f32, compact);
            }
            if info.id == "channel_strip" {
                ui.horizontal_wrapped(|ui| {
                    for (label, param) in [("Input", ""), ("Gate", "gate_on"), ("Compressor", "comp_on"), ("EQ", "eq_on"), ("Output", "")] {
                        let on = param.is_empty() || value(info, ins, param) >= 0.5;
                        ui.label(RichText::new(if on { label.to_owned() } else { format!("{label} off") }).color(if on {
                            t.text
                        } else {
                            t.text_dim
                        }));
                        if label != "Output" {
                            ui.label("→");
                        }
                    }
                });
            }
            if info.params.is_empty() {
                ui.label("No adjustable parameters. Use Bypass to compare the polarity change.");
            }
            let mut sections: Vec<(&str, Vec<&ParamInfo>)> = Vec::new();
            for param in info.params {
                let section = section(info.id, param.id, style.focus.contains(&param.id));
                if let Some((_, params)) = sections.iter_mut().find(|(name, _)| *name == section) {
                    params.push(param);
                } else {
                    sections.push((section, vec![param]));
                }
            }
            // Focus controls lead, while each original parameter occurs exactly once in the editor.
            sections.sort_by_key(|(name, _)| if *name == "Main controls" { 0 } else { 1 });
            for (section, params) in sections {
                ui.add_space(4.0);
                ui.label(RichText::new(section).strong().size(13.0));
                let columns = if ui.available_width() >= 440.0 && params.len() > 1 { 2 } else { 1 };
                for row in params.chunks(columns) {
                    ui.columns(columns, |columns| {
                        for (column, param) in columns.iter_mut().zip(row) {
                            parameter(app, column, track, slot, param, ins, (accent, skin));
                        }
                    });
                }
            }
        });
}

fn section(plugin: &str, param: &str, focus: bool) -> &'static str {
    if focus {
        return "Main controls";
    }
    if plugin == "eq_7band" {
        for (prefix, title) in [
            ("hpf_", "High-pass filter"),
            ("low_shelf_", "Low shelf"),
            ("low_mid_", "Low mid"),
            ("mid_", "Mid"),
            ("high_mid_", "High mid"),
            ("high_shelf_", "High shelf"),
            ("lpf_", "Low-pass filter"),
        ] {
            if param.starts_with(prefix) {
                return title;
            }
        }
    }
    if plugin == "channel_strip" {
        if param.starts_with("gate_") {
            return "Gate";
        }
        if param.starts_with("comp_") {
            return "Compressor";
        }
        return "Equalizer";
    }
    if param.starts_with("sc_") {
        return "Sidechain";
    }
    if param.starts_with("mod_") {
        return "Modulation";
    }
    "Fine tuning"
}

fn parameter(
    app: &mut SoundApp,
    ui: &mut Ui,
    track: TrackId,
    slot: usize,
    param: &ParamInfo,
    ins: &Insert,
    material: (Color32, crate::effect_style::Skin),
) {
    let (accent, skin) = material;
    let t = Tokens::current();
    let before = param.clamp(ins.params.get(param.id).copied().unwrap_or(param.default));
    let mut current = before;
    let mut finished = false;
    let mut reset = false;
    egui::Frame::new().fill(skin.card()).stroke(Stroke::new(1.0, t.border)).corner_radius(skin.radius()).inner_margin(10).show(ui, |ui| {
        ui.set_min_width((ui.available_width() - 1.0).max(1.0));
        ui.horizontal(|ui| {
            ui.label(RichText::new(param.name).strong().size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let response = ui.small_button("Reset").accessible_name(format!("Reset {}", param.name)).on_hover_text(format!(
                    "Reset {} to {}",
                    param.name,
                    param.format(param.default)
                ));
                crate::assistant::register(
                    app,
                    &format!("effect.{}.{}.{}.reset", track.0, slot, param.id),
                    &response,
                    &format!("Reset {}", param.name),
                );
                if response.clicked() {
                    current = param.default;
                    reset = true;
                }
            });
        });
        let response = if !param.choices.is_empty() {
            let mut index = current.round().max(0.0) as usize;
            let response = craft_ui::ComboBox::from_id_salt(("effect-choice", track.0, slot, param.id))
                .selected_text(param.choices.get(index).copied().unwrap_or("Unknown"))
                .show_ui(ui, |ui| {
                    for (i, choice) in param.choices.iter().enumerate() {
                        ui.selectable_value(&mut index, i, *choice);
                    }
                })
                .response;
            current = param.clamp(index as f32);
            response
        } else if param.unit == Unit::Toggle {
            let mut on = current >= 0.5;
            let response = ui.checkbox(&mut on, "Enabled");
            current = if on { 1.0 } else { 0.0 };
            response
        } else {
            ui.horizontal(|ui| {
                let response = crate::effect_style::knob(ui, param, &mut current, accent, skin);
                ui.vertical(|ui| {
                    ui.add_space(10.0);
                    let value = ui
                        .add(
                            egui::DragValue::new(&mut current)
                                .range(param.min..=param.max)
                                .speed(0.0)
                                .custom_formatter(|value, _| param.format(value as f32))
                                .custom_parser(|text| parse_value(text, param)),
                        )
                        .accessible_name(format!("{} exact value", param.name));
                    crate::assistant::register(
                        app,
                        &format!("effect.{}.{}.{}.value", track.0, slot, param.id),
                        &value,
                        &format!("{} exact value", param.name),
                    );
                    finished |= value.lost_focus();
                    ui.label(RichText::new("Double-click value").size(10.0).color(t.text_dim));
                });
                response
            })
            .inner
        }
        .accessible_name(param.name);
        crate::assistant::register(app, &format!("effect.{}.{}.{}", track.0, slot, param.id), &response, param.name);
        if response.has_focus() {
            ui.painter().rect_stroke(response.rect.expand(2.0), 4.0, Stroke::new(1.0, accent), egui::StrokeKind::Outside);
        }
        finished |= response.drag_stopped() || response.lost_focus();
    });
    if reset {
        app.engine.end_merge();
    }
    if (current - before).abs() > f32::EPSILON {
        if let Err(error) = app.engine.execute_merged(
            "mix.insert_param",
            &json!({"track":track.0,"slot":slot,"param":param.id,"value":current}),
            &format!("param:{}:{slot}:{}", track.0, param.id),
        ) {
            app.ui.status = error.to_string();
        } else if let Some(player) = &app.player {
            player.editor_set_param(track, slot, param.id, current);
        }
    }
    if finished || reset || ((current - before).abs() > f32::EPSILON && (param.unit == Unit::Toggle || !param.choices.is_empty())) {
        app.engine.end_merge();
    }
}

/// Accept the exact strings displayed by the shared DSP metadata as well as plain numeric entry.
fn parse_value(text: &str, param: &ParamInfo) -> Option<f64> {
    let text = text.trim();
    if param.unit == Unit::Db && text.eq_ignore_ascii_case("-inf dB") {
        return Some(f64::from(param.min));
    }
    let (number, scale) = if param.unit == Unit::Hz && text.ends_with("kHz") {
        (text.strip_suffix("kHz")?, 1000.0)
    } else {
        let suffix = match param.unit {
            Unit::Db => "dB",
            Unit::Hz => "Hz",
            Unit::Ms => "ms",
            Unit::Seconds => "s",
            Unit::Percent => "%",
            Unit::Ratio => ":1",
            Unit::Semitones => "st",
            Unit::Cents => "ct",
            _ => "",
        };
        (if suffix.is_empty() { text } else { text.strip_suffix(suffix).unwrap_or(text) }, 1.0)
    };
    let number = number.trim().parse::<f64>().ok()? * scale;
    number.is_finite().then_some(number)
}

fn value(info: &PluginInfo, ins: &Insert, id: &str) -> f32 {
    info.param(id).map_or(0.0, |param| param.clamp(ins.params.get(id).copied().unwrap_or(param.default)))
}

fn normalized(info: &PluginInfo, ins: &Insert, id: &str) -> f32 {
    info.param(id).map_or(0.0, |param| param.to_normalized(value(info, ins, id)))
}

fn phaser_stages(info: &PluginInfo, ins: &Insert) -> usize {
    info.param("stages")
        .and_then(|param| param.choices.get(value(info, ins, "stages") as usize))
        .and_then(|label| label.parse::<usize>().ok())
        .unwrap_or(6)
        .clamp(1, 12)
}

fn compressed_level(level: f32, threshold: f32, ratio: f32, knee: f32, makeup: f32) -> f32 {
    let slope = 1.0 - 1.0 / ratio.max(1.0);
    let over = level - threshold;
    let reduction = if knee > 1e-3 && 2.0 * over.abs() <= knee {
        let t = over + knee / 2.0;
        slope * t * t / (2.0 * knee)
    } else {
        slope * over.max(0.0)
    };
    level - reduction + makeup
}

fn response_plot(ui: &mut Ui, info: &PluginInfo, ins: &Insert, accent: Color32, sample_rate: f32, compact: bool) {
    let t = Tokens::current();
    ui.label(RichText::new("Frequency response").strong().size(12.0));
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), if compact { 96.0 } else { 146.0 }), Sense::hover());
    ui.painter().rect_filled(r, 7.0, t.slot_bg);
    let plot = r.shrink2(vec2(26.0, 20.0));
    for db in [-12.0f32, 0.0, 12.0] {
        let y = plot.center().y - db / 18.0 * plot.height() * 0.5;
        ui.painter().hline(plot.x_range(), y, Stroke::new(1.0, t.border));
        ui.painter().text(pos2(r.min.x + 3.0, y), egui::Align2::LEFT_CENTER, format!("{db:.0}"), crate::theme::regular(10.0), t.text_dim);
    }
    for (label, fraction) in [("20 Hz", 0.0), ("1 kHz", (50.0f32).ln() / (1000.0f32).ln()), ("20 kHz", 1.0)] {
        ui.painter().text(
            pos2(plot.min.x + fraction * plot.width(), r.max.y - 9.0),
            egui::Align2::CENTER_CENTER,
            label,
            crate::theme::regular(10.0),
            t.text_dim,
        );
    }
    let frequencies: Vec<f32> = (0..160).map(|i| 20.0 * 1000.0f32.powf(i as f32 / 159.0)).collect();
    let params: Vec<_> = info.params.iter().map(|param| (param.id, value(info, ins, param.id))).collect();
    let response = if info.id == "eq_7band" {
        soundcraft_dsp::eq7_response(&params, &frequencies, sample_rate)
    } else {
        soundcraft_dsp::eq1_response(&params, &frequencies, sample_rate)
    };
    let points = response
        .iter()
        .enumerate()
        .map(|(i, db)| pos2(plot.min.x + plot.width() * i as f32 / 159.0, plot.center().y - db.clamp(-18.0, 18.0) / 18.0 * plot.height() * 0.5))
        .collect();
    let ink = if crate::theme::is_light() { accent.lerp_to_gamma(Color32::BLACK, 0.4) } else { accent };
    ui.painter().with_clip_rect(plot).add(egui::Shape::line(points, Stroke::new(2.0, ink)));
}

/// An original, bounded settings illustration, never a simulated live scope or signal meter.
fn sketch(ui: &Ui, r: Rect, info: &PluginInfo, ins: &Insert, accent: Color32) {
    let ink = if crate::theme::is_light() { accent.lerp_to_gamma(Color32::BLACK, 0.4) } else { accent };
    let faint = ink.gamma_multiply(0.35);
    let p = ui.painter().with_clip_rect(r);
    let at = |x: f32, y: f32| pos2(r.min.x + x * r.width(), r.min.y + y * r.height());
    let n = |id| normalized(info, ins, id);
    let path = |points: Vec<_>, color, width| {
        p.add(egui::Shape::line(points, Stroke::new(width, color)));
    };
    match info.id {
        "room_reverb" => {
            let size = 0.45 + n("size") * 0.4;
            let outer = Rect::from_center_size(r.center(), r.size() * size);
            p.rect_stroke(outer, 4.0, Stroke::new(2.0, ink), egui::StrokeKind::Inside);
            p.rect_stroke(outer.shrink(9.0), 2.0, Stroke::new(1.0, faint), egui::StrokeKind::Inside);
            for corner in [outer.left_top(), outer.right_top(), outer.left_bottom(), outer.right_bottom()] {
                p.line_segment([r.center(), corner], Stroke::new(1.0, faint));
            }
            p.circle_filled(r.center(), 4.0, ink);
        }
        "plate_reverb" => {
            for i in 0..8 {
                let x = 0.08 + i as f32 * 0.1;
                p.line_segment(
                    [at(x, 0.8), at((x + 0.24 + n("decay") * 0.1).min(0.98), 0.2)],
                    Stroke::new(1.5, ink.gamma_multiply(1.0 - i as f32 * 0.07)),
                );
            }
            p.rect_stroke(r.shrink(7.0), 7.0, Stroke::new(1.0, faint), egui::StrokeKind::Inside);
        }
        "mod_delay" | "time_shift" => {
            let spacing = if info.id == "time_shift" { n("delay_ms") } else { n("time") };
            p.hline(r.x_range(), at(0.0, 0.8).y, Stroke::new(1.0, faint));
            for i in 0..if info.id == "time_shift" { 2 } else { 6 } {
                let x = 0.08 + i as f32 * (0.08 + spacing * 0.07);
                let height = if i == 0 || info.id == "time_shift" { 0.6 } else { 0.6 * (value(info, ins, "feedback").abs() / 100.0).powi(i) };
                p.line_segment([at(x, 0.8), at(x, 0.8 - height.max(0.03))], Stroke::new(3.0, ink));
            }
        }
        "eq_1band" | "eq_7band" | "de_esser" | "phaser" => {
            let count = match info.id {
                "eq_7band" => 7,
                "phaser" => phaser_stages(info, ins),
                _ => 1,
            };
            for i in 0..count {
                let x = (i + 1) as f32 / (count + 1) as f32;
                p.circle_stroke(at(x, 0.5), if count == 1 { 18.0 } else { 7.0 }, Stroke::new(2.0, ink));
            }
            p.line_segment([at(0.0, 0.5), at(1.0, 0.5)], Stroke::new(1.0, faint));
            if info.id == "de_esser" {
                p.line_segment([at(0.65, 0.15), at(0.9, 0.15)], Stroke::new(4.0, ink));
            }
        }
        "channel_strip" | "subharmonic" => {
            let count = if info.id == "channel_strip" { 4 } else { 3 };
            for i in 0..count {
                let x = 0.05 + i as f32 * 0.24;
                p.rect_filled(
                    Rect::from_min_max(at(x, if i % 2 == 0 { 0.25 } else { 0.4 }), at(x + 0.17, 0.75)),
                    3.0,
                    ink.gamma_multiply(0.35 + i as f32 * 0.15),
                );
            }
            p.line_segment([at(0.02, 0.5), at(0.98, 0.5)], Stroke::new(1.0, ink));
        }
        "auto_pan" => {
            p.circle_stroke(r.center(), 26.0, Stroke::new(1.5, faint));
            let position = value(info, ins, "manual") / 100.0 * n("width");
            p.line_segment([at(0.1, 0.5), at(0.9, 0.5)], Stroke::new(2.0, ink));
            p.circle_filled(at(0.5 + position * 0.35, 0.5), 5.0, ink);
            for (text, x) in [("L", 0.03), ("R", 0.97)] {
                p.text(at(x, 0.5), egui::Align2::CENTER_CENTER, text, crate::theme::bold(11.0), Tokens::current().text);
            }
        }
        "expander_gate" => {
            let hold = 0.35 + n("hold") * 0.3;
            path(vec![at(0.05, 0.85), at(0.25, 0.2), at(hold, 0.2), at(0.95, 0.85)], ink, 2.5);
        }
        "compressor" => {
            let points = (0..64)
                .map(|i| {
                    let x = i as f32 / 63.0;
                    let output = compressed_level(
                        -60.0 + 60.0 * x,
                        value(info, ins, "threshold"),
                        value(info, ins, "ratio"),
                        value(info, ins, "knee"),
                        value(info, ins, "makeup"),
                    );
                    at(0.06 + x * 0.88, 0.9 - ((output + 60.0) / 72.0).clamp(0.0, 1.0) * 0.8)
                })
                .collect();
            path(points, ink, 2.5);
        }
        "maximizer" => {
            let threshold = n("threshold");
            let corner = at(0.3 + threshold * 0.3, 0.5);
            path(vec![at(0.08, 0.86), corner, at(0.92, if info.id == "maximizer" { 0.5 - n("ceiling") * 0.2 } else { 0.15 })], ink, 2.5);
            p.circle_filled(corner, 4.0, ink);
        }
        "pitch_shifter" => {
            let shift = value(info, ins, "semitones") / 24.0;
            for i in 0..6 {
                let x = 0.08 + i as f32 * 0.15;
                p.line_segment([at(x, 0.7), at(x + 0.12, 0.7)], Stroke::new(3.0, faint));
                p.line_segment([at(x, 0.45 - shift * 0.25), at(x + 0.12, 0.45 - shift * 0.25)], Stroke::new(3.0, ink));
            }
        }
        _ => {
            // Distinct mathematical motifs illustrate the controls, without analyzing audio.
            let curves = if matches!(info.id, "chorus" | "flanger" | "invert" | "dc_offset_removal" | "trim") { 2 } else { 1 };
            for layer in 0..curves {
                let points = (0..96)
                    .map(|i| {
                        let x = i as f32 / 95.0;
                        let phase = x * std::f32::consts::TAU;
                        let wave = match info.id {
                            "chorus" => (phase + layer as f32 * (0.3 + n("depth"))).sin() * 0.5,
                            "flanger" => (phase * (1.0 + layer as f32 * 0.5) + n("delay")).sin() * 0.6,
                            "ring_mod" => match value(info, ins, "effect") as usize {
                                1 => (phase + (phase * 3.0).sin() * n("mod_depth") * 5.0).sin() * 0.7,
                                2 | 3 => (phase.sin() + (phase * (2.0 + n("frequency") * 5.0)).sin() * 0.5) * 0.45,
                                _ => phase.sin() * (phase * (2.0 + n("frequency") * 6.0)).sin() * 0.7,
                            },
                            "saturator" => (phase.sin() * (1.0 + n("drive") * 6.0)).tanh() * 0.7,
                            "lofi" => {
                                let steps = 2.0 + n("bits") * 12.0;
                                (phase.sin() * steps).round() / steps * 0.7
                            }
                            "rectifier" => {
                                if value(info, ins, "mode") >= 0.5 {
                                    phase.sin().abs() * 0.7
                                } else {
                                    phase.sin().max(0.0) * 0.7
                                }
                            }
                            "gain" | "trim" => {
                                phase.sin()
                                    * (if layer == 0 && info.id == "trim" {
                                        0.3
                                    } else {
                                        (soundcraft_dsp::db_to_gain(value(info, ins, "gain")) * 0.45).clamp(0.02, 0.8)
                                    })
                            }
                            "invert" => phase.sin() * if layer == 0 { 0.65 } else { -0.65 },
                            "dc_offset_removal" => phase.sin() * 0.35 + if layer == 0 { 0.35 } else { 0.0 },
                            "signal_generator" => match value(info, ins, "waveform") as usize {
                                1 => {
                                    if phase.sin() >= 0.0 {
                                        0.6
                                    } else {
                                        -0.6
                                    }
                                }
                                2 => (x * 2.0 - 1.0) * 0.65,
                                3 => (1.0 - (x * 2.0 - 1.0).abs() * 2.0) * 0.65,
                                4 | 5 => (((i * 37) % 19) as f32 / 19.0 - 0.5) * 0.9,
                                _ => phase.sin() * 0.65,
                            },
                            "dither" => (((i * 37) % 19) as f32 / 19.0 - 0.5) * (0.2 + (1.0 - n("bits")) * 0.6),
                            _ => phase.sin() * 0.6,
                        };
                        at(0.04 + x * 0.92, 0.5 - wave * 0.42)
                    })
                    .collect();
                path(points, if layer == 0 && curves > 1 { faint } else { ink }, 2.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_value;
    #[test]
    fn exact_value_entry_accepts_display_units_and_rejects_nonfinite_input() {
        for (plugin, param, text, expected) in [
            ("eq_1band", "freq", "2.50 kHz", 2500.0),
            ("compressor", "threshold", "-12.5 dB", -12.5),
            ("compressor", "ratio", "4.0:1", 4.0),
            ("mod_delay", "time", "125.5 ms", 125.5),
            ("room_reverb", "decay", "2.50 s", 2.5),
            ("pitch_shifter", "semitones", "-7 st", -7.0),
            ("pitch_shifter", "cents", "12 ct", 12.0),
            ("gain", "gain", "-inf dB", -96.0),
        ] {
            let info = soundcraft_dsp::plugin_info(plugin).unwrap().param(param).unwrap();
            assert_eq!(parse_value(text, info), Some(expected));
            assert_eq!(parse_value("NaN", info), None);
            assert_eq!(parse_value("inf", info), None);
        }
    }

    #[test]
    fn illustrations_decode_choice_values_and_compressor_shape() {
        let info = soundcraft_dsp::plugin_info("phaser").unwrap();
        let mut insert = soundcraft_model::Insert::new("phaser");
        assert_eq!(super::phaser_stages(info, &insert), 6);
        insert.params.insert("stages".into(), 4.0);
        assert_eq!(super::phaser_stages(info, &insert), 12);
        assert_eq!(super::compressed_level(0.0, -20.0, 4.0, 0.0, 0.0), -15.0);
        assert_eq!(super::compressed_level(-30.0, -20.0, 4.0, 0.0, 0.0), -30.0);
        assert_eq!(super::compressed_level(0.0, -20.0, 1.0, 0.0, 0.0), 0.0);
    }
}
