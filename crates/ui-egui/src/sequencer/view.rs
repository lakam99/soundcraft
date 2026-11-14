//! Continuous hardware faceplate: no tabs, hidden parameter pages or popup-only controls.
use super::{Clip, SoundApp, Value, current, json, preview, stop};
use crate::sound_lab::skin::{BRASS, CREAM, CYAN};
use egui::{Color32, FontId, Rect, Response, RichText, Sense, Stroke, Ui, pos2, vec2};
use soundcraft_midi::sequencer::{Direction, Lane, Mode, Order};

fn register(app: &mut SoundApp, id: &str, r: &Response, label: &str) {
    crate::assistant::register(app, id, r, label);
}
fn key(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, lit: bool, enabled: bool, width: f32) -> bool {
    let enabled = enabled && ui.is_enabled();
    let (rect, r) = ui.allocate_exact_size(vec2(width, 29.0), Sense::click());
    let p = ui.painter();
    let down = r.is_pointer_button_down_on() && enabled;
    p.rect_filled(rect.translate(vec2(0.0, 2.0)), 3.0, Color32::BLACK);
    let face = rect.shrink(1.0).translate(vec2(0.0, if down { 1.0 } else { 0.0 }));
    p.rect_filled(face, 3.0, if lit { Color32::from_rgb(115, 87, 44) } else { Color32::from_rgb(47, 54, 48) });
    p.rect_stroke(face, 3.0, Stroke::new(1.0, if r.has_focus() { CYAN } else { Color32::from_rgb(130, 120, 91) }), egui::StrokeKind::Inside);
    p.line_segment([face.left_top() + vec2(3.0, 1.0), face.right_top() + vec2(-3.0, 1.0)], Stroke::new(1.0, Color32::from_white_alpha(45)));
    let color = if enabled { CREAM } else { Color32::GRAY };
    let mut job = egui::text::LayoutJob::simple(label.to_owned(), FontId::proportional(10.0), color, (width - 8.0).max(1.0));
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let galley = p.layout_job(job);
    p.with_clip_rect(face.shrink2(vec2(3.0, 0.0))).galley(face.center() - galley.size() * 0.5, galley, color);
    let r = r.on_hover_text(label);
    r.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Button, enabled, label));
    register(app, id, &r, label);
    if r.clicked() && enabled {
        r.request_focus();
    }
    enabled && (r.clicked() || (r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space))))
}
struct Dial<'a> {
    range: std::ops::RangeInclusive<i32>,
    choices: &'a [&'a str],
    size: f32,
}
fn dial(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, value: i32, face: Dial<'_>) -> i32 {
    let min = *face.range.start();
    let max = *face.range.end();
    let choices = face.choices;
    let size = face.size;
    let mut value = value;
    ui.push_id(id, |ui| {
        ui.vertical(|ui| {
            ui.set_width(size + 14.0);
            if size >= 40.0 {
                ui.label(RichText::new(label).color(CREAM).size(9.0));
            }
            let (rect, r) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
            if r.dragged() {
                let delta = ui.input(|i| i.pointer.delta());
                let speed = if ui.input(|i| i.modifiers.shift) { 0.04 } else { 0.25 };
                let id = ui.scope_id().with("fraction");
                let mut acc = ui.ctx().data_mut(|d| d.get_temp::<f32>(id).unwrap_or(value as f32));
                acc = (acc + (delta.x - delta.y) * speed).clamp(min as f32, max as f32);
                value = acc.round() as i32;
                ui.ctx().data_mut(|d| d.insert_temp(id, acc));
            } else {
                ui.ctx().data_mut(|d| d.remove::<f32>(ui.scope_id().with("fraction")));
            }
            if r.clicked() {
                r.request_focus();
            }
            if r.double_clicked() {
                value = match id {
                    "pulse.root" => 60,
                    "pulse.length" => 16,
                    "pulse.cycles" | "pulse.arp.octaves" | "pulse.step.ratchets" => 1,
                    "pulse.step.velocity" | "pulse.step.chance" => 100,
                    "pulse.step.gate" => 60,
                    _ => 0,
                }
                .clamp(min, max);
            }
            if ui.is_enabled() && r.has_focus() {
                let delta = ui.input(|i| i.key_pressed(egui::Key::ArrowUp) as i32 - i.key_pressed(egui::Key::ArrowDown) as i32);
                value = (value + delta).clamp(min, max);
            }
            let p = ui.painter();
            let center = rect.center();
            p.circle_filled(center + vec2(0.0, 2.0), size * 0.46, Color32::BLACK);
            if let Some(texture) = app.sequencer.skin.knob() {
                p.image(
                    texture,
                    rect.shrink(2.0),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::from_white_alpha(if ui.is_enabled() { 255 } else { 90 }),
                );
            } else {
                p.circle_filled(center, size * 0.43, Color32::from_rgb(64, 67, 60));
            }
            let angle = std::f32::consts::PI * (0.75 + 1.5 * (value - min) as f32 / (max - min).max(1) as f32);
            let v = vec2(angle.cos(), angle.sin());
            p.line_segment([center + v * size * 0.17, center + v * size * 0.36], Stroke::new(2.0, CREAM));
            if r.has_focus() {
                p.circle_stroke(center, size * 0.49, Stroke::new(1.0, CYAN));
            }
            r.widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::Slider, ui.is_enabled(), label));
            register(app, id, &r, label);
            if choices.is_empty() {
                let r = ui.add(egui::DragValue::new(&mut value).range(min..=max).speed(0.15)).accessible_name(label);
                register(app, &format!("{id}.value"), &r, label);
            } else {
                ui.label(RichText::new(choices.get((value - min) as usize).copied().unwrap_or("—")).color(BRASS).size(9.0));
            }
        })
    });
    value
}
fn command_key(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, lit: bool, enabled: bool, p: Value) {
    if key(app, ui, id, label, lit, enabled, 68.0)
        && let Err(e) = app.run(id, p)
    {
        app.sequencer.error = e;
    }
}
fn heading(ui: &mut Ui, label: &str) {
    ui.add_space(5.0);
    ui.label(RichText::new(label).size(10.0).strong().color(BRASS));
}
fn display(ui: &mut Ui, text: impl Into<String>) {
    egui::Frame::new().fill(Color32::from_rgb(12, 25, 23)).inner_margin(6.0).corner_radius(3.0).show(ui, |ui| {
        ui.set_max_width(ui.available_width().min(650.0));
        ui.add(egui::Label::new(RichText::new(text.into()).monospace().size(10.0).color(CYAN)).wrap());
    });
}

pub fn show(app: &mut SoundApp, ctx: &egui::Context) {
    if !app.sequencer.open {
        return;
    }
    let mut open = true;
    egui::Window::new("PULSE • Analog step sequencer")
        .id(egui::Id::unique("pulse.rack"))
        .open(&mut open)
        .title_bar(false)
        .default_pos(pos2(32.0, 37.0))
        .default_size(vec2((ctx.content_rect().width() - 90.0).clamp(850.0, 1120.0), (ctx.content_rect().height() - 85.0).max(400.0)))
        .min_width(820.0)
        .show(ctx, |ui| {
            panel(app, ui);
        });
    if !open {
        stop(app);
        app.sequencer.open = false;
    }
}
fn panel(app: &mut SoundApp, ui: &mut Ui) {
    let Some(clip) = app.sequencer.active else {
        return;
    };
    app.sequencer.skin.ensure(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ui.available_height().max(410.0)), Sense::hover());
    app.sequencer.skin.panel(ui, rect);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(26.0, 17.0))).layout(egui::Layout::top_down(egui::Align::Min)));
    let ui = &mut child;
    ui.visuals_mut().override_text_color = Some(CREAM);
    ui.spacing_mut().item_spacing = vec2(5.0, 3.0);
    let state = current(app, clip);
    ui.horizontal(|ui| {
        ui.label(RichText::new("P U L S E").size(25.0).strong().color(BRASS));
        if let Ok((track, _, _)) = &state {
            display(ui, app.engine.session().track(*track).map(|t| t.name.clone()).unwrap_or_default());
        }
        display(ui, if app.sequencer.draft.is_some() { "PREVIEW • NOT STORED" } else { "STORED" });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| command_key(app, ui, "pulse.close", "CLOSE", false, true, json!({})));
    });
    ui.horizontal(|ui| {
        command_key(app, ui, "pulse.run", "RUN", app.sequencer.running, state.is_ok(), json!({}));
        command_key(app, ui, "pulse.stop", "STOP", false, true, json!({}));
        command_key(app, ui, "pulse.commit", "STORE", false, app.sequencer.draft.is_some() && state.is_ok(), json!({}));
        command_key(app, ui, "pulse.discard", "REVERT", false, app.sequencer.draft.is_some(), json!({}));
        if let Ok((t, _, _)) = &state {
            command_key(
                app,
                ui,
                "soundlab.open",
                "FORGE",
                false,
                app.engine.session().track(*t).and_then(|t| t.instrument.as_ref()).is_some_and(|i| i.plugin == soundcraft_dsp::hybrid::ID),
                json!({"track":t.0}),
            );
        }
        command_key(app, ui, "ai.chat", "ASSIST", false, true, json!({"value":true}));
        let detach = app.sequencer.draft.is_none() && app.engine.session().find_clip(clip).is_some_and(|(_, c)| c.step_sequencer.is_some());
        command_key(app, ui, "pulse.detach", "EDIT MIDI", false, detach, json!({"clip":clip.0}));
        ui.label(RichText::new("CLOCK / SONG TEMPO").color(CREAM).size(9.0));
    });
    if !app.sequencer.error.is_empty() {
        ui.colored_label(Color32::from_rgb(255, 166, 111), app.sequencer.error.clone());
    }
    let Ok((track, c, mut program)) = state else {
        ui.colored_label(BRASS, state.err().unwrap_or_default());
        ui.label("EDIT MIDI keeps the notes and removes the PULSE controls. Undo restores them.");
        return;
    };
    let before = program.clone();
    let mut latch = false;
    let play = play_step(app, &c, app.sequencer.bank, app.sequencer.lane);
    egui::ScrollArea::vertical().id_salt("pulse.faceplate").auto_shrink([false, false]).show(ui, |ui| {
        heading(ui, "PATTERN MEMORY");
        ui.horizontal_wrapped(|ui| {
            for i in 0..program.banks.len() {
                if key(app, ui, &format!("pulse.bank.{i}"), &char::from(b'A' + i as u8).to_string(), app.sequencer.bank == i, true, 30.0) {
                    app.sequencer.bank = i;
                    app.sequencer.lane = 0;
                    app.sequencer.page = 0;
                    app.sequencer.step = 0;
                }
            }
            if key(app, ui, "pulse.bank.add", "+ BANK", false, program.banks.len() < 8, 62.0) {
                let mut bank = program.banks.get(app.sequencer.bank).cloned().unwrap_or_default();
                bank.name = format!("Pattern {}", char::from(b'A' + program.banks.len() as u8));
                program.banks.push(bank);
                app.sequencer.bank = program.banks.len() - 1;
            }
            if key(app, ui, "pulse.bank.play", "PLAY BANK", false, true, 82.0) {
                program.active_bank = app.sequencer.bank as u8;
                program.chain.clear();
            }
            if key(app, ui, "pulse.chain.append", "CHAIN +", false, program.chain.len() < 32, 70.0) {
                program.chain.push(app.sequencer.bank as u8);
            }
            if key(app, ui, "pulse.chain.clear", "CLEAR CHAIN", false, !program.chain.is_empty(), 88.0) {
                program.chain.clear();
            }
            display(
                ui,
                if program.chain.is_empty() {
                    format!("PLAY {}", char::from(b'A' + program.active_bank))
                } else {
                    format!("CHAIN {}", program.chain.iter().map(|i| char::from(b'A' + *i).to_string()).collect::<Vec<_>>().join(" > "))
                },
            );
        });
        app.sequencer.bank = app.sequencer.bank.min(program.banks.len().saturating_sub(1));
        let Some(b) = program.banks.get_mut(app.sequencer.bank) else {
            return;
        };
        heading(ui, "CLOCK & PLAY ORDER");
        ui.horizontal(|ui| {
            b.length = dial(app, ui, "pulse.length", "LENGTH", i32::from(b.length), Dial { range: 1..=64, choices: &[], size: 46.0 }) as u8;
            let rates = [960, 480, 240, 120, 60, 640, 320, 160, 80];
            let index = rates.iter().position(|r| *r == b.rate).unwrap_or(2);
            b.rate = rates
                .get(dial(
                    app,
                    ui,
                    "pulse.rate",
                    "RATE",
                    index as i32,
                    Dial { range: 0..=8, choices: &["1/4", "1/8", "1/16", "1/32", "1/64", "1/4 T", "1/8 T", "1/16 T", "1/32 T"], size: 46.0 },
                ) as usize)
                .copied()
                .unwrap_or(240);
            b.swing = dial(app, ui, "pulse.swing", "SWING %", i32::from(b.swing), Dial { range: 0..=75, choices: &[], size: 46.0 }) as u8;
            program.cycles =
                dial(app, ui, "pulse.cycles", "PASSES", i32::from(program.cycles), Dial { range: 1..=32, choices: &[], size: 46.0 }) as u8;
            ui.vertical(|ui| {
                ui.label(RichText::new("VARIATION SEED").size(9.0));
                let r = ui.add(egui::DragValue::new(&mut program.seed).range(0..=u32::MAX)).accessible_name("Variation seed");
                register(app, "pulse.seed", &r, "Repeatable variation seed");
                ui.label(RichText::new("Chance & random repeat\nafter PASSES × CHAIN.").size(9.0).color(CREAM));
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("EDIT PAGE • 16 KEYS").size(9.0).color(BRASS));
                ui.horizontal(|ui| {
                    for i in 0..4 {
                        if key(app, ui, &format!("pulse.page.{i}"), &format!("{}–{}", i * 16 + 1, i * 16 + 16), app.sequencer.page == i, true, 46.0)
                        {
                            app.sequencer.page = i;
                            app.sequencer.step = i * 16;
                        }
                    }
                });
                ui.label(RichText::new("Page changes the view only.").size(9.0));
            });
        });
        heading(ui, "VOICE LANES • ONE INSTRUMENT");
        ui.horizontal(|ui| {
            for i in 0..b.lanes.len() {
                let label = format!("{} {}", i + 1, b.lanes.get(i).map(|l| l.name.as_str()).unwrap_or(""));
                if key(app, ui, &format!("pulse.lane.{i}"), &label, app.sequencer.lane == i, true, 85.0) {
                    app.sequencer.lane = i;
                }
            }
            if key(app, ui, "pulse.lane.add", "+ VOICE", false, b.lanes.len() < 8, 70.0) {
                let mut l = Lane { name: format!("Voice {}", b.lanes.len() + 1), ..Default::default() };
                if let Some(first) = b.lanes.first().filter(|l| l.mode == Mode::Drum) {
                    l.mode = Mode::Drum;
                    l.drum_note = match b.lanes.len() {
                        1 => 38,
                        2 => 42,
                        3 => 46,
                        4 => 39,
                        _ => 45,
                    };
                    l.name = soundcraft_midi::gm_drum_name(l.drum_note).unwrap_or("Drum").to_string();
                    l.root = first.root;
                }
                b.lanes.push(l);
                app.sequencer.lane = b.lanes.len() - 1;
            }
        });
        app.sequencer.lane = app.sequencer.lane.min(b.lanes.len().saturating_sub(1));
        let Some(l) = b.lanes.get_mut(app.sequencer.lane) else {
            return;
        };
        heading(
            ui,
            if l.mode == Mode::Drum {
                "STEP VELOCITY & GATES • CLICK NUMBER TO EDIT • CLICK KEY FOR REST / ON"
            } else {
                "STEP PITCH & GATES • CLICK NUMBER TO EDIT • CLICK KEY FOR REST / ON"
            },
        );
        let width = ((ui.available_width() - 15.0 * 3.0) / 16.0).max(40.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for col in 0..16 {
                let index = app.sequencer.page * 16 + col;
                ui.vertical(|ui| {
                    ui.set_width(width);
                    let selected = app.sequencer.step == index;
                    if key(app, ui, &format!("pulse.step.{index}.select"), &format!("{:02}", index + 1), selected, true, width) {
                        app.sequencer.step = index;
                    }
                    let inactive = index >= usize::from(b.length);
                    let original = l.steps.get(index).cloned().unwrap_or_default();
                    let mut step = original.clone();
                    ui.add_enabled_ui(!inactive, |ui| {
                        if l.mode == Mode::Drum {
                            step.velocity = dial(
                                app,
                                ui,
                                &format!("pulse.step.{index}.velocity"),
                                "VELOCITY",
                                i32::from(step.velocity),
                                Dial { range: 1..=127, choices: &[], size: (width - 14.0).min(36.0) },
                            ) as u8;
                        } else {
                            step.pitch = dial(
                                app,
                                ui,
                                &format!("pulse.step.{index}.pitch"),
                                "PITCH",
                                i32::from(step.pitch),
                                Dial { range: -48..=48, choices: &[], size: (width - 14.0).min(36.0) },
                            ) as i8;
                        }
                    });
                    let note = (i16::from(l.root) + i16::from(l.transpose) + i16::from(step.pitch)).clamp(0, 127) as u8;
                    ui.label(
                        RichText::new(if l.mode == Mode::Drum { soundcraft_midi::note_name(l.drum_note) } else { soundcraft_midi::note_name(note) })
                            .size(9.0)
                            .color(if inactive { Color32::GRAY } else { CREAM }),
                    );
                    let (r, _) = ui.allocate_exact_size(vec2(width, 6.0), Sense::hover());
                    let center = r.center();
                    ui.painter().circle_filled(center, 3.0, if play == Some(index) { CYAN } else { Color32::from_rgb(29, 48, 41) });
                    if key(
                        app,
                        ui,
                        &format!("pulse.step.{index}.on"),
                        if step.tie {
                            "TIE"
                        } else if step.on {
                            "ON"
                        } else {
                            "REST"
                        },
                        step.on && !inactive,
                        !inactive,
                        width,
                    ) {
                        step.on = !step.on;
                    }
                    if selected {
                        ui.painter().rect_stroke(ui.min_rect().expand(1.0), 2.0, Stroke::new(1.0, BRASS), egui::StrokeKind::Outside);
                    }
                    if !inactive && step != original {
                        l.steps.resize(l.steps.len().max(index + 1), Default::default());
                        if let Some(s) = l.steps.get_mut(index) {
                            *s = step;
                        }
                    }
                });
            }
        });
        let selected = app.sequencer.step.min(63);
        heading(ui, &format!("STEP {:02} • EXPRESSION", selected + 1));
        let mut fill = None;
        let original = l.steps.get(selected).cloned().unwrap_or_default();
        let mut expression = original.clone();
        let s = &mut expression;
        ui.add_enabled_ui(selected < usize::from(b.length), |ui| {
            ui.horizontal(|ui| {
                s.velocity =
                    dial(app, ui, "pulse.step.velocity", "VELOCITY", i32::from(s.velocity), Dial { range: 1..=127, choices: &[], size: 46.0 }) as u8;
                s.gate = dial(app, ui, "pulse.step.gate", "GATE %", i32::from(s.gate), Dial { range: 1..=100, choices: &[], size: 46.0 }) as u8;
                s.chance =
                    dial(app, ui, "pulse.step.chance", "CHANCE %", i32::from(s.chance), Dial { range: 0..=100, choices: &[], size: 46.0 }) as u8;
                s.ratchets =
                    dial(app, ui, "pulse.step.ratchets", "RATCHETS", i32::from(s.ratchets), Dial { range: 1..=8, choices: &[], size: 46.0 }) as u8;
                if s.ratchets > 1 || s.chance != 100 {
                    s.tie = false;
                }
                ui.vertical(|ui| {
                    if key(app, ui, "pulse.step.tie", "TIE", s.tie, true, 65.0) {
                        s.tie = !s.tie;
                        if s.tie {
                            s.on = true;
                            s.ratchets = 1;
                            s.chance = 100;
                        }
                    }
                    if key(app, ui, "pulse.step.accent", "ACCENT", s.accent, true, 65.0) {
                        s.accent = !s.accent;
                    }
                    ui.label(RichText::new("Tie continues previous\nattack; rests break ties.").size(9.0));
                });
                ui.vertical(|ui| {
                    if key(app, ui, "pulse.fill", "ALL ON", false, true, 70.0) {
                        fill = Some(true);
                    }
                    if key(app, ui, "pulse.clear", "ALL REST", false, true, 70.0) {
                        fill = Some(false);
                    }
                });
            });
        });
        if expression != original && selected < usize::from(b.length) {
            l.steps.resize(l.steps.len().max(selected + 1), Default::default());
            if let Some(s) = l.steps.get_mut(selected) {
                *s = expression;
            }
        }
        if let Some(on) = fill {
            if on {
                l.steps.resize(l.steps.len().max(usize::from(b.length)), Default::default());
            }
            for s in &mut l.steps {
                s.on = on;
            }
        }
        heading(ui, "VOICE SETTINGS");
        ui.horizontal(|ui| {
            let modes = [Mode::Melody, Mode::Drum, Mode::Arp, Mode::Chord];
            let i = modes.iter().position(|m| *m == l.mode).unwrap_or(0);
            l.mode = modes
                .get(dial(app, ui, "pulse.mode", "MODE", i as i32, Dial { range: 0..=3, choices: &["MELODY", "DRUM", "ARP", "CHORD"], size: 46.0 })
                    as usize)
                .copied()
                .unwrap_or_default();
            if l.mode == Mode::Drum {
                l.drum_note =
                    dial(app, ui, "pulse.drum_note", "DRUM KEY", i32::from(l.drum_note), Dial { range: 0..=127, choices: &[], size: 46.0 }) as u8;
            } else {
                l.root = dial(app, ui, "pulse.root", "ROOT", i32::from(l.root), Dial { range: 0..=127, choices: &[], size: 46.0 }) as u8;
            }
            ui.add_enabled_ui(l.mode != Mode::Drum, |ui| {
                l.transpose =
                    dial(app, ui, "pulse.transpose", "TRANSPOSE", i32::from(l.transpose), Dial { range: -24..=24, choices: &[], size: 46.0 }) as i8;
            })
            .response
            .on_hover_text("Drum lanes use the fixed DRUM KEY; transposition applies to pitched lanes.");
            let dirs = [Direction::Forward, Direction::Reverse, Direction::Pendulum, Direction::Random, Direction::Walk];
            let i = dirs.iter().position(|m| *m == l.direction).unwrap_or(0);
            l.direction = dirs
                .get(dial(
                    app,
                    ui,
                    "pulse.direction",
                    "DIRECTION",
                    i as i32,
                    Dial { range: 0..=4, choices: &["FORWARD", "REVERSE", "PENDULUM", "RANDOM", "WALK"], size: 46.0 },
                ) as usize)
                .copied()
                .unwrap_or_default();
            ui.vertical(|ui| {
                if key(app, ui, "pulse.mute", "MUTE", l.muted, true, 60.0) {
                    l.muted = !l.muted;
                }
                if key(app, ui, "pulse.solo", "SOLO", l.solo, true, 60.0) {
                    l.solo = !l.solo;
                }
            });
            ui.vertical(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut l.name).desired_width(125.0).char_limit(32)).accessible_name("Lane name");
                register(app, "pulse.lane.name", &r, "Lane name");
                display(
                    ui,
                    if l.mode == Mode::Drum {
                        soundcraft_midi::gm_drum_name(l.drum_note).unwrap_or("Drum").to_string()
                    } else {
                        format!("ROOT {}", soundcraft_midi::note_name(l.root))
                    },
                );
                ui.label(RichText::new("Amber = on  •  cyan = playing\nBrass border = selected edit").size(9.0));
            });
        });
        heading(ui, "ARPEGGIATOR • CHORD MEMORY");
        ui.horizontal(|ui| {
            let orders = [Order::Up, Order::Down, Order::Pendulum, Order::Memory, Order::Random];
            let i = orders.iter().position(|x| *x == l.order).unwrap_or(0);
            l.order = orders
                .get(dial(
                    app,
                    ui,
                    "pulse.arp.order",
                    "ORDER",
                    i as i32,
                    Dial { range: 0..=4, choices: &["UP", "DOWN", "PENDULUM", "MEMORY", "RANDOM"], size: 46.0 },
                ) as usize)
                .copied()
                .unwrap_or_default();
            l.octaves = dial(app, ui, "pulse.arp.octaves", "OCTAVES", i32::from(l.octaves), Dial { range: 1..=4, choices: &[], size: 46.0 }) as u8;
            ui.vertical(|ui| {
                latch = key(app, ui, "pulse.latch", "LATCH KEYS", false, !app.sequencer.running, 98.0);
                ui.label(RichText::new("Stop, hold keys, then latch.\nMemory stays after release.").size(9.0));
                ui.horizontal(|ui| {
                    for (name, chord) in [("MAJOR", vec![0, 4, 7]), ("MINOR", vec![0, 3, 7]), ("SUS", vec![0, 5, 7])] {
                        if key(app, ui, &format!("pulse.chord.{name}"), name, false, true, 51.0) {
                            l.chord = chord;
                            l.mode = Mode::Arp;
                        }
                    }
                });
            });
            ui.vertical(|ui| {
                ui.label(RichText::new("CHORD OFFSETS • SEMITONES FROM ROOT").size(9.0));
                ui.horizontal(|ui| {
                    for (i, n) in l.chord.iter_mut().enumerate() {
                        let r = ui.add(egui::DragValue::new(n).range(-48..=48)).accessible_name(format!("Chord interval {}", i + 1));
                        register(app, &format!("pulse.chord.note.{i}"), &r, "Chord interval");
                    }
                    if key(app, ui, "pulse.chord.add", "+", false, l.chord.len() < 8, 24.0) {
                        l.chord.push(12);
                    }
                    if key(app, ui, "pulse.chord.remove", "−", false, l.chord.len() > 1, 24.0) {
                        l.chord.pop();
                    }
                });
                display(
                    ui,
                    l.chord
                        .iter()
                        .map(|n| soundcraft_midi::note_name((i16::from(l.root) + i16::from(*n)).clamp(0, 127) as u8))
                        .collect::<Vec<_>>()
                        .join("  ")
                        .to_string(),
                );
            });
        });
        heading(ui, "MIDI OUTPUT");
        ui.horizontal(|ui| {
            if key(app, ui, "pulse.print", "PRINT MIDI", false, app.sequencer.draft.is_none() && !app.sound_lab.has_active_candidate(), 100.0)
                && let Err(e) = app.run("pulse.print", json!({"track":track.0,"at":c.end()}))
            {
                app.sequencer.error = e;
            }
            ui.label(
                RichText::new(
                    "Prints after this part on the same track. Source retained.\nAssistant can print to an explicit sampler/instrument and position.",
                )
                .size(10.0),
            );
        });
        ui.add_space(12.0);
    });
    if program != before {
        match preview(app, clip, program) {
            Ok(()) => {
                app.sequencer.error.clear();
                app.ui.status = "PULSE candidate edited. RUN to audition; STORE to keep.".into();
            }
            Err(e) => app.sequencer.error = e,
        }
    }
    if latch && let Err(e) = app.run("pulse.latch", json!({})) {
        app.sequencer.error = e;
    }
}
fn play_step(app: &SoundApp, c: &Clip, bank: usize, lane: usize) -> Option<usize> {
    if !app.sequencer.running {
        return None;
    }
    let s = app.engine.session();
    let tick = s.tempo.samples_to_ticks(app.position(), s.sample_rate) - s.tempo.samples_to_ticks(c.start, s.sample_rate);
    app.sequencer
        .clock
        .iter()
        .find(|c| usize::from(c.bank) == bank && usize::from(c.lane) == lane && tick >= c.start && tick < c.end)
        .map(|c| usize::from(c.step))
}
