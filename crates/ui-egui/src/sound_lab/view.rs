//! A continuous physical instrument: knobs, detented selectors, patch jacks and momentary keys.
use super::skin::{BRASS, CREAM, CYAN};
use super::{Arc, Patch, SoundApp, TrackId, Value, hybrid, insert, json, preview, skin, stop, wanted};
use egui::{Color32, Rect, Response, RichText, Stroke, Ui, pos2, vec2};
use hybrid::{Design, Kind, Modulation, Module, Route, Source};
use soundcraft_dsp::{ParamInfo, Taper, Unit};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) enum Plug {
    Audio(String),
    Control(Source),
}
enum Jack {
    Output(Plug),
    Audio(String),
    Cv(String, String),
}
const GAIN: ParamInfo =
    ParamInfo { id: "gain", name: "LEVEL", min: -2.0, max: 2.0, default: 1.0, unit: Unit::None, taper: Taper::Linear, choices: &[] };
const DEPTH: ParamInfo =
    ParamInfo { id: "depth", name: "DEPTH", min: -1.0, max: 1.0, default: 0.1, unit: Unit::None, taper: Taper::Linear, choices: &[] };
const NOTE: ParamInfo =
    ParamInfo { id: "note", name: "NOTE", min: 0.0, max: 127.0, default: 60.0, unit: Unit::None, taper: Taper::Linear, choices: &[] };
const VELOCITY: ParamInfo =
    ParamInfo { id: "velocity", name: "VELOCITY", min: 1.0, max: 127.0, default: 100.0, unit: Unit::None, taper: Taper::Linear, choices: &[] };
const GATE: ParamInfo =
    ParamInfo { id: "gate", name: "GATE", min: 0.05, max: 8.0, default: 1.5, unit: Unit::Seconds, taper: Taper::Log, choices: &[] };
const TAIL: ParamInfo =
    ParamInfo { id: "tail", name: "TAIL", min: 0.0, max: 8.0, default: 1.0, unit: Unit::Seconds, taper: Taper::Linear, choices: &[] };
const LEVEL: ParamInfo = ParamInfo {
    id: "preview_level",
    name: "AUDITION LEVEL",
    min: -48.0,
    max: 0.0,
    default: -6.0,
    unit: Unit::Db,
    taper: Taper::Linear,
    choices: &[],
};
const MODULE: ParamInfo = ParamInfo {
    id: "module",
    name: "MODULE",
    min: 0.0,
    max: 9.0,
    default: 0.0,
    unit: Unit::Choice,
    taper: Taper::Linear,
    choices: &["OSCILLATOR", "NOISE", "SAMPLE", "FILTER", "DRIVE", "RESONATOR", "AMPLIFIER", "OUTPUT", "ENVELOPE", "LFO"],
};
fn register(app: &mut SoundApp, id: &str, r: &Response, label: &str) {
    if !id.starts_with("soundlab.cable.") {
        app.sound_lab.hit_regions.push(r.interact_rect);
    }
    crate::assistant::register(app, id, r, label);
}
fn key(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, enabled: bool, lit: bool) -> bool {
    let r = ui
        .push_id(id, |ui| {
            let (rect, mut r) = ui.allocate_exact_size(vec2((label.len() as f32 * 7.0 + 24.0).max(66.0), 30.0), egui::Sense::click());
            if enabled && r.clicked() {
                r.request_focus();
            }
            let pressed = enabled
                && (r.is_pointer_button_down_on()
                    || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter))));
            if pressed && r.has_focus() {
                r.mark_changed();
            }
            let p = ui.painter();
            p.rect_filled(rect.translate(vec2(0.0, 2.0)), 3.0, Color32::BLACK);
            let face = rect.shrink(1.0).translate(vec2(0.0, if pressed { 1.0 } else { 0.0 }));
            p.rect_filled(face, 3.0, if lit { Color32::from_rgb(118, 90, 47) } else { Color32::from_rgb(49, 55, 48) });
            p.rect_stroke(face, 3.0, Stroke::new(1.0, if r.has_focus() { CYAN } else { Color32::from_rgb(130, 120, 91) }), egui::StrokeKind::Inside);
            p.line_segment([face.left_top() + vec2(3.0, 1.0), face.right_top() + vec2(-3.0, 1.0)], Stroke::new(1.0, Color32::from_white_alpha(45)));
            p.text(
                face.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(11.0),
                if enabled { CREAM } else { Color32::from_rgb(116, 119, 101) },
            );
            r.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, label));
            r
        })
        .inner;
    register(app, id, &r, label);
    enabled && (r.clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter))))
}
fn action(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, p: Value, enabled: bool) {
    if key(app, ui, id, label, enabled, false) {
        let _ = app.run(id, p);
    }
}
fn dial(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, p: &ParamInfo, value: &mut f32, texture: Option<egui::TextureId>) {
    ui.push_id(id, |ui| {
        ui.vertical(|ui| {
            ui.set_width(88.0);
            let label_response = ui.label(RichText::new(label).size(10.0).color(CREAM));
            let r = skin::knob(ui, p, value, texture);
            register(app, id, &r, label);
            if !p.choices.is_empty() {
                let center = r.rect.center();
                for n in 0..p.choices.len() {
                    let a = std::f32::consts::PI * (0.75 + 1.5 * n as f32 / p.choices.len().saturating_sub(1).max(1) as f32);
                    ui.painter().circle_filled(center + vec2(a.cos(), a.sin()) * 31.0, 1.8, BRASS);
                }
            }
            ui.scope(|ui| {
                let v = ui.visuals_mut();
                v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                v.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 61, 53);
                v.override_text_color = Some(BRASS);
                if !p.choices.is_empty() {
                    ui.label(egui::RichText::new(p.format(*value)).size(10.0).color(BRASS));
                    return;
                }
                let suffix = match p.unit {
                    Unit::Db => " dB",
                    Unit::Hz => " Hz",
                    Unit::Ms => " ms",
                    Unit::Seconds => " s",
                    Unit::Semitones => " st",
                    Unit::Cents => " ct",
                    Unit::Ratio => " :1",
                    Unit::Percent => " %",
                    _ => "",
                };
                let r = ui
                    .add(egui::DragValue::new(value).range(p.min..=p.max).speed((p.max - p.min) / 500.0).suffix(suffix))
                    .labelled_by(label_response.id);
                register(app, &format!("{id}.value"), &r, &format!("{label} exact value"));
            });
        });
    });
}
fn jack(
    app: &mut SoundApp,
    ui: &mut Ui,
    id: &str,
    label: &str,
    endpoint: Jack,
    positions: &mut BTreeMap<String, egui::Pos2>,
    events: &mut Vec<Jack>,
) {
    let color = match &endpoint {
        Jack::Audio(_) | Jack::Output(Plug::Audio(_)) => BRASS,
        _ => CYAN,
    };
    let semantic = match &endpoint {
        Jack::Output(Plug::Audio(module)) => format!("{module} audio output"),
        Jack::Output(Plug::Control(source)) => match source {
            Source::Module(id) => format!("{id} CV output"),
            Source::Macro(n) => format!("Macro {} CV output", n + 1),
            Source::Velocity => "Velocity CV output".into(),
            Source::Key => "Key CV output".into(),
        },
        Jack::Audio(module) => format!("{module} audio or FM input"),
        Jack::Cv(module, param) => format!("{module} {param} CV input"),
    };
    ui.push_id(id, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                let (rect, r) = ui.allocate_exact_size(vec2(26.0, 26.0), egui::Sense::click());
                let c = rect.center();
                positions.insert(id.into(), c);
                let p = ui.painter();
                p.circle_filled(c, 10.0, Color32::from_rgb(23, 26, 24));
                p.circle_stroke(c, 9.0, Stroke::new(2.5, BRASS));
                p.circle_filled(c, 4.6, Color32::BLACK);
                p.circle_stroke(c, 6.0, Stroke::new(1.2, color));
                if r.hovered() || r.has_focus() {
                    p.circle_stroke(c, 12.0, Stroke::new(1.2, CYAN));
                }
                ui.label(RichText::new(label).size(9.0).color(CREAM));
                r.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, true, &semantic));
                register(app, id, &r, &semantic);
                if r.clicked() {
                    r.request_focus();
                    events.push(endpoint);
                } else if r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Space) || i.key_pressed(egui::Key::Enter)) {
                    events.push(endpoint);
                }
            });
            ui.add_space(36.0);
        });
    });
}
pub fn show(app: &mut SoundApp, ui: &mut Ui, tid: TrackId) -> bool {
    app.sound_lab.hit_regions.clear();
    let Ok(base) = insert(app, tid) else { return false };
    let mut open = true;
    if app.sound_lab.active != Some(tid) {
        app.sound_lab.cable_layout = Default::default();
        stop(app);
        app.live_midi.reset();
        app.sound_lab.active = Some(tid);
        app.sound_lab.audio_dirty = true;
    }
    app.sound_lab.skin.ensure(ui.ctx());
    let texture = app.sound_lab.skin.knob();
    let patch = wanted(app, tid, &base);
    let mut design = patch.design().clone();
    let height = (ui.ctx().content_rect().height() - 130.0).clamp(430.0, 830.0);
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width().min((ui.ctx().content_rect().width() - 70.0).max(640.0)).max(640.0), height),
        egui::Sense::hover(),
    );
    app.sound_lab.skin.panel(ui, rect);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(28.0, 20.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    let ui = &mut child;
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(CREAM);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(37, 48, 42);
    ui.visuals_mut().clone_from(&visuals);
    ui.spacing_mut().item_spacing = vec2(10.0, 6.0);
    let mut positions = BTreeMap::new();
    let mut events = Vec::new();
    let candidate = app.sound_lab.drafts.contains_key(&tid);
    ui.horizontal(|ui| {
        ui.label(RichText::new("F O R G E").size(27.0).strong().color(BRASS));
        ui.add_space(16.0);
        ui.add(
            egui::Label::new(
                RichText::new(format!(
                    "{} · {}",
                    design.name.to_uppercase(),
                    app.engine.session().track(tid).map(|t| t.name.as_str()).unwrap_or("Instrument")
                ))
                .size(14.0)
                .color(CREAM),
            )
            .truncate(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if key(app, ui, "soundlab.close", "CLOSE", true, false) {
                let _ = app.run("soundlab.close", json!({"track":tid.0}));
                open = false;
            }
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 3.0, if candidate { BRASS } else { CYAN });
            ui.label(RichText::new(if candidate { "PREVIEW" } else { "STORED" }).size(10.0));
        });
    });
    ui.horizontal_wrapped(|ui| {
        action(app, ui, "soundlab.audition", "AUDITION", json!({"track":tid.0}), app.sound_lab.job.is_none());
        action(app, ui, "soundlab.stop", "STOP", json!({}), true);
        action(app, ui, "soundlab.commit", "STORE", json!({"track":tid.0}), candidate);
        action(app, ui, "soundlab.discard", "REVERT", json!({"track":tid.0}), candidate);
        if key(app, ui, "soundlab.assist", "ASSIST", true, false) {
            let _ = app.run("ai.chat", json!({"value":true}));
        }
        ui.label(
            RichText::new(
                app.sound_lab
                    .wiring
                    .as_ref()
                    .map(|_| "PATCH LEAD IN HAND · CLICK AN INPUT · ESC CANCELS")
                    .unwrap_or("OUTPUT > INPUT TO PATCH · STORE BEFORE RECORDING"),
            )
            .size(9.0)
            .color(BRASS),
        );
    });
    let remaining = (rect.bottom() - 20.0 - ui.cursor().top()).max(90.0);
    let clip = ui.clip_rect().intersect(Rect::from_min_max(ui.cursor().min, rect.max - vec2(20.0, 20.0)));
    egui::ScrollArea::vertical().id_salt(("forge_rack", tid.0)).max_height(remaining).show(ui, |ui| {
        ui.horizontal(|ui| {
            display(app, ui, tid, (ui.available_width() - 420.0).max(190.0), 120.0, &design, &base);
            for i in 0..4 {
                if let (Some(m), Some(p)) = (design.macros.get_mut(i), hybrid::MACROS.get(i)) {
                    ui.vertical(|ui| {
                        dial(app, ui, &format!("soundlab.macro.{i}"), &m.name.to_uppercase(), p, &mut m.value, texture);
                        jack(app, ui, &format!("macro:{i}"), "CV", Jack::Output(Plug::Control(Source::Macro(i as u8))), &mut positions, &mut events);
                    });
                }
            }
        });
        skin::plate().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let mut note = f32::from(app.sound_lab.settings.note);
                dial(app, ui, "soundlab.note", "NOTE", &NOTE, &mut note, texture);
                app.sound_lab.settings.note = note.round() as u8;
                let mut velocity = f32::from(app.sound_lab.settings.velocity);
                dial(app, ui, "soundlab.velocity", "VELOCITY", &VELOCITY, &mut velocity, texture);
                app.sound_lab.settings.velocity = velocity.round() as u8;
                let mut gate = app.sound_lab.settings.gate;
                dial(app, ui, "soundlab.gate", "GATE", &GATE, &mut gate, texture);
                app.sound_lab.settings.gate = gate;
                let mut tail = app.sound_lab.settings.tail;
                dial(app, ui, "soundlab.tail", "TAIL", &TAIL, &mut tail, texture);
                app.sound_lab.settings.tail = tail;
                let mut level = app.sound_lab.preview_level;
                dial(app, ui, "soundlab.preview_level", "AUDITION LEVEL", &LEVEL, &mut level, texture);
                app.sound_lab.preview_level = level;
                ui.vertical(|ui| {
                    jack(app, ui, "velocity", "VELOCITY CV", Jack::Output(Plug::Control(Source::Velocity)), &mut positions, &mut events);
                    jack(app, ui, "key", "KEY CV", Jack::Output(Plug::Control(Source::Key)), &mut positions, &mut events);
                });
            });
        });
        let columns = if ui.available_width() >= 840.0 { 2 } else { 1 };
        let width = (ui.available_width() - 12.0 * (columns - 1) as f32) / columns as f32;
        let mut remove = None;
        for chunk in design.modules.chunks_mut(columns) {
            ui.horizontal_top(|ui| {
                for module in chunk {
                    ui.allocate_ui_with_layout(vec2(width, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        skin::plate().show(ui, |ui| {
                            ui.set_min_width(width - 26.0);
                            ui.set_max_width(width - 26.0);
                            let name = module.id.replace('_', " ").to_uppercase();
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(if module.id == "osc_a" {
                                        "OSCILLATOR A"
                                    } else if module.id == "sub" {
                                        "SUB OSCILLATOR"
                                    } else if module.id.starts_with("module_") {
                                        module.kind.label()
                                    } else {
                                        &name
                                    })
                                    .size(12.0)
                                    .strong()
                                    .color(BRASS),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if key(app, ui, &format!("soundlab.remove.{}", module.id), "REMOVE", module.kind != Kind::Output, false) {
                                        remove = Some(module.id.clone());
                                    }
                                });
                            });
                            let per_row = ((width - 26.0) / 100.0).floor().max(1.0) as usize;
                            for parameters in module.kind.params().chunks(per_row) {
                                ui.horizontal_top(|ui| {
                                    for p in parameters {
                                        let mut value = module.params.get(p.id).copied().unwrap_or(p.default);
                                        let before = value;
                                        ui.vertical(|ui| {
                                            dial(
                                                app,
                                                ui,
                                                &format!("soundlab.param.{}.{}", module.id, p.id),
                                                &p.name.to_uppercase(),
                                                p,
                                                &mut value,
                                                texture,
                                            );
                                            if p.choices.is_empty() {
                                                jack(
                                                    app,
                                                    ui,
                                                    &format!("p:{}:{}", module.id, p.id),
                                                    "CV",
                                                    Jack::Cv(module.id.clone(), p.id.into()),
                                                    &mut positions,
                                                    &mut events,
                                                );
                                            }
                                        });
                                        if value != before {
                                            module.params.insert(p.id.into(), value);
                                        }
                                    }
                                });
                            }
                            ui.horizontal_wrapped(|ui| {
                                if module.kind.accepts_audio() {
                                    jack(
                                        app,
                                        ui,
                                        &format!("i:{}", module.id),
                                        if module.kind == Kind::Oscillator { "FM IN" } else { "AUDIO IN" },
                                        Jack::Audio(module.id.clone()),
                                        &mut positions,
                                        &mut events,
                                    );
                                }
                                if !module.kind.control() && module.kind != Kind::Output {
                                    jack(
                                        app,
                                        ui,
                                        &format!("a:{}", module.id),
                                        "AUDIO OUT",
                                        Jack::Output(Plug::Audio(module.id.clone())),
                                        &mut positions,
                                        &mut events,
                                    );
                                }
                                if module.kind != Kind::Output {
                                    jack(
                                        app,
                                        ui,
                                        &format!("c:{}", module.id),
                                        "CV OUT",
                                        Jack::Output(Plug::Control(Source::Module(module.id.clone()))),
                                        &mut positions,
                                        &mut events,
                                    );
                                }
                            });
                        });
                    });
                }
            });
        }
        if let Some(id) = remove {
            design.modules.retain(|m| m.id != id);
            design.routes.retain(|r| r.from != id && r.to != id);
            design.modulation.retain(|m| m.target != id && !matches!(&m.source,Source::Module(s)if s==&id));
        }
        skin::plate().show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut kind = app.sound_lab.new_kind as f32;
                dial(app, ui, "soundlab.module_selector", "EXPANSION", &MODULE, &mut kind, texture);
                app.sound_lab.new_kind = kind.round() as usize;
                if key(
                    app,
                    ui,
                    "soundlab.add_module",
                    "INSERT MODULE",
                    design.modules.len() < hybrid::MAX_MODULES && app.sound_lab.new_kind != 7,
                    false,
                ) && let Some(kind) = Kind::ALL.get(app.sound_lab.new_kind).filter(|k| **k != Kind::Output)
                    && let Some(id) = (1..100).map(|n| format!("module_{n}")).find(|id| !design.modules.iter().any(|m| m.id == *id))
                {
                    design.modules.push(Module::new(&id, *kind));
                }
            });
        });
        patch_bay(app, ui, &mut design, texture);
        skin::plate().show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                action(app, ui, "soundlab.to_sampler", "PRINT TO SAMPLER", json!({"track":tid.0}), !candidate && app.sound_lab.job.is_none());
                action(app, ui, "soundlab.to_audio", "PRINT TO AUDIO", json!({"track":tid.0}), !candidate && app.sound_lab.job.is_none());
                file_action(app, ui, tid, "synth.sample_load", "LOAD SAMPLE", false, !candidate);
                file_action(app, ui, tid, "soundlab.export", "EXPORT WAV", true, true);
                file_action(app, ui, tid, "soundlab.preset_save", "SAVE PATCH", true, true);
                file_action(app, ui, tid, "soundlab.preset_load", "LOAD PATCH", false, true);
            });
        });
    });
    let painted = super::cables::paint(
        ui,
        &design,
        &positions,
        clip,
        (app.sound_lab.skin.cable(), app.sound_lab.skin.plug()),
        &app.sound_lab.hit_regions,
        &mut app.sound_lab.cable_layout,
    );
    if let Some((id, response, label)) = painted.handle {
        register(app, &id, &response, &label);
    }
    if let Some((modulation, index)) = painted.unplug {
        if modulation {
            if index < design.modulation.len() {
                design.modulation.remove(index);
            }
        } else if index < design.routes.len() {
            design.routes.remove(index);
        }
    }
    if let Some(plug) = &app.sound_lab.wiring {
        let source = match plug {
            Plug::Audio(id) => format!("a:{id}"),
            Plug::Control(source) => source_key(source),
        };
        if let (Some(from), Some(to)) = (positions.get(&source), ui.input(|i| i.pointer.hover_pos())) {
            let color = if matches!(plug, Plug::Audio(_)) { BRASS } else { CYAN };
            super::cables::loose(ui, *from, to, rect.shrink(20.0), (app.sound_lab.skin.cable(), app.sound_lab.skin.plug()), color);
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) || (events.is_empty() && ui.input(|i| i.pointer.any_click())) {
        app.sound_lab.wiring = None;
    }
    for event in events {
        match event {
            Jack::Output(plug) => app.sound_lab.wiring = Some(plug),
            Jack::Audio(to) => match app.sound_lab.wiring.clone() {
                Some(Plug::Audio(from)) => {
                    design.routes.push(Route { from, to, gain: 1.0 });
                    app.sound_lab.wiring = None;
                }
                _ => app.sound_lab.error = "Pick an AUDIO OUT plug before patching an audio/FM input.".into(),
            },
            Jack::Cv(target, param) => match app.sound_lab.wiring.clone() {
                Some(Plug::Control(source)) => {
                    let amount = if param == "gain"
                        && matches!(&source,Source::Module(id)if design.modules.iter().any(|m|m.id==*id&&m.kind==Kind::Envelope))
                    {
                        1.0
                    } else {
                        0.1
                    };
                    design.modulation.push(Modulation { source, target, param, amount });
                    app.sound_lab.wiring = None;
                }
                _ => app.sound_lab.error = "Pick a CV OUT plug before patching a parameter's CV input.".into(),
            },
        }
    }
    if design != *patch.design() {
        match Patch::compile(design) {
            Ok(p) => {
                if let Err(e) = preview(app, tid, base, Arc::new(p)) {
                    app.sound_lab.error = e;
                }
            }
            Err(e) => app.sound_lab.error = e,
        }
    }
    if !app.sound_lab.error.is_empty() {
        ui.painter().text(
            rect.left_bottom() + vec2(32.0, -7.0),
            egui::Align2::LEFT_BOTTOM,
            &app.sound_lab.error,
            egui::FontId::proportional(11.0),
            Color32::from_rgb(255, 191, 130),
        );
    }
    crate::assistant::register_area(app, "soundlab.panel", rect, ui.ctx(), "FORGE analog instrument");
    open
}
fn source_key(source: &Source) -> String {
    match source {
        Source::Module(id) => format!("c:{id}"),
        Source::Macro(n) => format!("macro:{n}"),
        Source::Velocity => "velocity".into(),
        Source::Key => "key".into(),
    }
}
fn patch_bay(app: &mut SoundApp, ui: &mut Ui, d: &mut Design, texture: Option<egui::TextureId>) {
    skin::plate().show(ui, |ui| {
        ui.label(RichText::new("P A T C H   B A Y").color(BRASS));
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (i, r) in d.routes.iter_mut().enumerate() {
                ui.vertical(|ui| {
                    dial(app, ui, &format!("soundlab.route.{i}.gain"), &format!("{} > {}", r.from, r.to), &GAIN, &mut r.gain, texture);
                    if key(app, ui, &format!("soundlab.route.{i}.unplug"), "UNPLUG", true, false) {
                        remove = Some(i);
                    }
                });
            }
        });
        if let Some(i) = remove {
            d.routes.remove(i);
        }
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            for (i, m) in d.modulation.iter_mut().enumerate() {
                ui.vertical(|ui| {
                    let source = match &m.source {
                        Source::Module(id) => id.clone(),
                        Source::Macro(n) => d.macros.get(usize::from(*n)).map(|m| m.name.clone()).unwrap_or_default(),
                        Source::Velocity => "Velocity".into(),
                        Source::Key => "Key".into(),
                    };
                    dial(
                        app,
                        ui,
                        &format!("soundlab.mod.{i}.amount"),
                        &format!("{source} > {}.{}", m.target, m.param),
                        &DEPTH,
                        &mut m.amount,
                        texture,
                    );
                    if key(app, ui, &format!("soundlab.mod.{i}.unplug"), "UNPLUG", true, false) {
                        remove = Some(i);
                    }
                });
            }
        });
        if let Some(i) = remove {
            d.modulation.remove(i);
        }
    });
}
fn file_action(app: &mut SoundApp, ui: &mut Ui, tid: TrackId, id: &str, label: &str, save: bool, enabled: bool) {
    if !key(app, ui, id, label, enabled, false) {
        return;
    }
    let chosen = if save {
        app.services
            .pick_save
            .as_ref()
            .and_then(|pick| pick(label, std::path::Path::new(if id == "soundlab.export" { "Forge.wav" } else { "Forge.forge.json" })))
    } else {
        app.services
            .pick_open
            .as_ref()
            .and_then(|pick| pick(label, if id == "synth.sample_load" { &["wav", "aiff", "flac", "mp3", "ogg"] } else { &["json"] }))
    };
    if let Some(path) = chosen {
        let _ = app.run(id, json!({"track":tid.0,"path":path.to_string_lossy()}));
    }
}
fn display(app: &SoundApp, ui: &mut Ui, tid: TrackId, width: f32, height: f32, design: &Design, base: &soundcraft_model::Insert) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 9.0, Color32::from_rgb(3, 14, 13));
    p.rect_stroke(rect, 9.0, Stroke::new(5.0, Color32::from_rgb(80, 75, 53)), egui::StrokeKind::Inside);
    let inset = rect.shrink(13.0);
    for i in 1..8 {
        let x = inset.left() + inset.width() * i as f32 / 8.0;
        p.line_segment([pos2(x, inset.top()), pos2(x, inset.bottom() - 17.0)], Stroke::new(0.5, Color32::from_rgb(17, 40, 32)));
    }
    for i in 1..5 {
        let y = inset.top() + (inset.height() - 17.0) * i as f32 / 5.0;
        p.line_segment([pos2(inset.left(), y), pos2(inset.right(), y)], Stroke::new(0.5, Color32::from_rgb(17, 40, 32)));
    }
    let center = inset.center();
    if let Some(r) = app.sound_lab.ready.as_ref().filter(|r| r.track == tid) {
        let scale = 28.0 / r.metrics.peak.max(0.00001);
        let color = if r.base != *base || r.patch.design() != design { BRASS } else { CYAN };
        for (i, (lo, hi)) in r.metrics.waveform_min.iter().zip(&r.metrics.waveform_max).enumerate() {
            let x = inset.left() + i as f32 / r.metrics.waveform_min.len().saturating_sub(1).max(1) as f32 * inset.width();
            p.line_segment([pos2(x, center.y - lo * scale), pos2(x, center.y - hi * scale)], Stroke::new(1.3, color));
        }
        p.text(
            inset.left_bottom(),
            egui::Align2::LEFT_BOTTOM,
            format!("Peak {:.1} / RMS {:.1} dBFS", 20.0 * r.metrics.peak.max(0.000001).log10(), 20.0 * r.metrics.rms.max(0.000001).log10()),
            egui::FontId::monospace(10.0),
            color,
        );
    } else {
        p.line_segment([pos2(inset.left(), center.y), pos2(inset.right(), center.y)], Stroke::new(1.0, CYAN.gamma_multiply(0.4)));
    }
    p.text(
        inset.left_top(),
        egui::Align2::LEFT_TOP,
        if app.sound_lab.job.is_some() {
            "RENDERING · STOP CANCELS"
        } else if app.sound_lab.ready.as_ref().filter(|r| r.track == tid).is_some_and(|r| r.base != *base || r.patch.design() != design) {
            "STALE AUDITION · AUTO SCALE"
        } else {
            "LAST AUDITION · AUTO SCALE"
        },
        egui::FontId::monospace(9.0),
        CYAN,
    );
    p.line_segment([rect.left_top() + vec2(8.0, 5.0), rect.right_top() + vec2(-8.0, 5.0)], Stroke::new(1.0, Color32::from_rgb(145, 129, 86)));
}
