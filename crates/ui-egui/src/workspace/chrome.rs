use super::{button, run};
use crate::{SoundApp, docking, theme};
use egui::{RichText, Ui, vec2};
use serde_json::json;
use soundcraft_model::Tool;

pub fn show(app: &mut SoundApp, ui: &mut Ui) {
    for (style, size) in
        [(egui::TextStyle::Body, 13.0), (egui::TextStyle::Button, 13.0), (egui::TextStyle::Small, 11.0), (egui::TextStyle::Heading, 19.0)]
    {
        ui.style_mut().text_styles.insert(style, egui::FontId::proportional(size));
    }
    let colors = theme::Tokens::current();
    egui::Panel::top("workspace.actions").exact_size(94.0).frame(egui::Frame::NONE.fill(colors.toolbar_bg).inner_margin(8.0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 7.0);
        actions(app, ui);
        tools(app, ui);
    });
    egui::Panel::bottom("workspace.transport")
        .exact_size(86.0)
        .frame(egui::Frame::NONE.fill(colors.panel_bg2).inner_margin(8.0))
        .show(ui, |ui| transport(app, ui));
    docking::show(app, ui, crate::MainWindow::Edit);
}

fn action(app: &mut SoundApp, ui: &mut Ui, id: &str, label: &str, command: &str) {
    let response = match command {
        "transport.rtz" => timeline_icon(ui, "rtz", label, false, "Go to start (Home)"),
        "view.zoom_out" => timeline_icon(ui, "zoom_out", label, false, "Zoom out"),
        "view.zoom_in" => timeline_icon(ui, "zoom_in", label, false, "Zoom in"),
        _ => ui.add_sized([0.0, 30.0], egui::Button::new(label)),
    };
    crate::assistant::register(app, id, &response, label);
    if response.clicked() {
        run(app, command, json!({}));
    }
}

fn view(app: &mut SoundApp, ui: &mut Ui, label: &str, command: &str, selected: bool) {
    let response = match command {
        "transport.stop" => timeline_icon(ui, "stop", label, selected, "Stop (Space)"),
        "transport.play" => timeline_icon(ui, "play", label, selected, "Play (Space)"),
        "options.loop_playback" => timeline_icon(ui, "loop", label, selected, "Loop playback"),
        "options.click" => timeline_icon(ui, "click", label, selected, "Metronome"),
        _ => ui.add_sized([0.0, 30.0], egui::Button::new(label).selected(selected)),
    };
    crate::assistant::register(app, command, &response, label);
    if response.clicked() {
        run(app, command, json!({}));
    }
}

fn timeline_icon(ui: &mut Ui, icon: &str, label: &str, selected: bool, tooltip: &str) -> egui::Response {
    crate::widgets::icon_button(ui, vec2(34.0, 32.0), icon, selected, tooltip).accessible_name(label)
}

fn actions(app: &mut SoundApp, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        action(app, ui, "workspace.addAudio", "+ Audio track", "workspace.audioTrack");
        let options = ui.menu_button("Audio options…", |ui| {
            for (label, format) in [("Mono audio track", "mono"), ("Stereo audio track", "stereo")] {
                let response = ui.button(label);
                crate::assistant::register(app, &format!("workspace.audio.{format}"), &response, label);
                if response.clicked() {
                    run(app, "workspace.audioTrack", json!({"format":format}));
                    ui.close();
                }
            }
        });
        let response = options.response.accessible_name("Audio track format");
        crate::assistant::register(app, "workspace.audioOptions", &response, "Audio track format");
        action(app, ui, "workspace.addInstrument", "+ Instrument track", "workspace.instrumentTrack");
        import_menu(app, ui, "Import…", "workspace.import");
        ui.separator();
        view(app, ui, "Inspector · F4", "workspace.inspector", app.ui.show_inspector);
        view(app, ui, "Browser · F5", "workspace.browser", app.ui.show_browser);
        view(app, ui, "Mix · F3", "window.toggle_mix_edit", app.ui.show_mix && !app.ui.mix_separate);
        view(app, ui, "MIDI · F2", "window.midi_editor", app.ui.show_midi_editor);
        action(
            app,
            ui,
            "workspace.pulse",
            "PULSE",
            if app
                .engine
                .session()
                .edit
                .selected_clips
                .iter()
                .any(|id| app.engine.session().find_clip(*id).is_some_and(|(_, c)| c.step_sequencer.is_some()))
            {
                "pulse.open"
            } else {
                "pulse.new"
            },
        );
        view(app, ui, "AI Chat", "ai.chat", app.ui.show_ai_chat);
    });
}

fn tools(app: &mut SoundApp, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        let current = app.engine.session().edit.tool;
        for (tool, icon, label) in [
            (Tool::Grabber, "grabber", "Move"),
            (Tool::Selector, "selector", "Range"),
            (Tool::Trim, "trim", "Trim"),
            (Tool::Pencil, "pencil", "Draw"),
            (Tool::Smart, "smart", "Smart"),
        ] {
            let tooltip = tool.fkey().map_or_else(|| "Smart tool (F6+F7 or F7+F8)".into(), |key| format!("{label} ({key})"));
            let response = timeline_icon(ui, icon, label, current == tool, &tooltip);
            crate::assistant::register(app, &format!("workspace.tool.{}", tool.id()), &response, label);
            if response.clicked() {
                run(app, "edit.tool", json!({"tool":tool.id()}));
            }
        }
        ui.separator();
        let snap = app.engine.session().edit.edit_mode.is_grid();
        let response = timeline_icon(ui, "snap", "Snap", snap, "Snap to grid");
        crate::assistant::register(app, "workspace.snap", &response, "Snap to grid");
        if response.clicked() {
            run(app, "edit.mode", json!({"mode":if snap {"slip"} else {"grid"}}));
        }
        let grid = app.engine.session().edit.grid.label();
        let (grid_icon, _) = ui.allocate_exact_size(vec2(20.0, 24.0), egui::Sense::hover());
        crate::icons::draw(ui.painter(), grid_icon.shrink(2.0), "grid", theme::Tokens::current().text_dim);
        let grid_response = craft_ui::ComboBox::from_id_salt("workspace.grid").selected_text(&grid).width(95.0).show_ui(ui, |ui| {
            for value in ["1 bar", "1/4", "1/8", "1/16", "1/32"] {
                if ui.selectable_label(grid == value, value).clicked() {
                    run(app, "view.grid", json!({"value":value}));
                }
            }
        });
        let response = grid_response.response.accessible_name("Grid").on_hover_text("Grid resolution");
        crate::assistant::register(app, "workspace.grid", &response, "Grid resolution");
        action(app, ui, "workspace.zoomOut", "Zoom out", "view.zoom_out");
        action(app, ui, "workspace.zoomIn", "Zoom in", "view.zoom_in");
        let more = timeline_icon(ui, "more", "More tools", false, "More timeline tools");
        crate::assistant::register(app, "workspace.moreTools", &more, "More timeline tools");
        egui::Popup::menu(&more).show(|ui| {
            for (label, tool) in [("Zoom tool", Tool::Zoom), ("Scrub audio", Tool::Scrubber)] {
                if ui.button(label).clicked() {
                    run(app, "edit.tool", json!({"tool":tool.id()}));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Advanced track creation…").clicked() {
                crate::menus::invoke_menu(app, "track.new", "Track > New...");
                ui.close();
            }
            if ui.button("Classic workspace").clicked() {
                run(app, "workspace.classic", json!({}));
                ui.close();
            }
        });
    });
}

fn transport(app: &mut SoundApp, ui: &mut Ui) {
    let t = theme::Tokens::current();
    ui.spacing_mut().item_spacing = vec2(9.0, 5.0);
    ui.horizontal_wrapped(|ui| {
        action(app, ui, "transport.rtz", "Go to start", "transport.rtz");
        view(app, ui, "Stop", "transport.stop", !app.is_playing());
        view(app, ui, "Play", "transport.play", app.is_playing());
        let record_tip = format!("Record (F12) · {}", app.record_hint().unwrap_or("Record armed audio tracks"));
        let response = crate::widgets::rec_toggle(ui, vec2(36.0, 32.0), app.engine.transport.recording, &record_tip).accessible_name("Record");
        crate::assistant::register(app, "transport.record", &response, "Record audio");
        if response.clicked() {
            run(app, "transport.record", json!({}));
        }
        view(app, ui, "Loop", "options.loop_playback", app.engine.session().edit.loop_playback);
        let session = app.engine.session();
        let tick = session.tempo.samples_to_ticks(app.position(), session.sample_rate);
        let position = session.format_position(app.position(), session.edit.main_counter);
        ui.label(RichText::new(position).monospace().size(19.0));
        let mut tempo = session.tempo.tempo_at_tick(tick);
        let event = session.tempo.tempos().iter().rev().find(|event| event.tick <= tick).map_or(0, |event| event.tick);
        let at = session.tempo.tick_to_samples(event, session.sample_rate);
        ui.label("Tempo");
        let response = ui.add(egui::DragValue::new(&mut tempo).range(5.0..=1000.0).speed(0.2).suffix(" BPM")).accessible_name("Tempo (BPM)");
        crate::assistant::register(app, "workspace.tempo", &response, "Tempo");
        if response.changed()
            && let Err(error) = app.engine.execute_merged("event.tempo", &json!({"bpm":tempo,"at":at}), "workspace.tempo")
        {
            app.ui.status = error.to_string();
        }
        if response.drag_stopped() || response.lost_focus() {
            app.engine.end_merge();
        }
        view(app, ui, "Click", "options.click", app.engine.session().edit.click);
    });
    ui.horizontal(|ui| {
        let ready = app.player.is_some();
        ui.label(RichText::new(if ready { "Audio connected" } else { "Audio not connected" }).color(if ready {
            t.meter_green
        } else {
            t.meter_yellow
        }));
        action(app, ui, "workspace.audioSetup", "Audio setup…", "workspace.audioSetup");
        let status = if app.ui.status.is_empty() {
            "Space: play / stop · F2: MIDI editor · F3: mix · F4: inspector · F5: browser"
        } else {
            &app.ui.status
        };
        ui.add(egui::Label::new(status).truncate()).on_hover_text(status);
    });
}

pub fn empty(app: &mut SoundApp, ui: &mut Ui) {
    let rect = crate::edit_window::rect(app.edit_layout.timeline);
    let mut child = ui.new_child(egui::UiBuilder::new().id_salt("workspace.start").max_rect(rect.shrink2(vec2(24.0, 24.0))));
    child.vertical_centered(|ui| {
        ui.add_space(22.0);
        ui.heading("Start with a sound");
        ui.label("Record audio, write notes, or bring in a file.");
        ui.add_space(14.0);
        button(app, ui, "workspace.start.audio", "Record audio", "workspace.audioTrack", json!({}));
        button(app, ui, "workspace.start.instrument", "Write MIDI", "workspace.startMidi", json!({}));
        import_menu(app, ui, "Import audio or MIDI…", "workspace.start.import");
        ui.add_space(10.0);
        ui.label("You can also drop audio or MIDI files into the arrangement.");
    });
}

pub(super) fn import_menu(app: &mut SoundApp, ui: &mut Ui, label: &str, id: &str) {
    let width = if ui.layout().is_horizontal() { 0.0 } else { ui.available_width().min(220.0) };
    let response = ui.add(egui::Button::new(label).min_size(vec2(width, 30.0)).wrap_mode(egui::TextWrapMode::Extend));
    crate::assistant::register(app, id, &response, "Import audio or MIDI");
    egui::Popup::menu(&response).show(|ui| {
        for (label, command, path) in
            [("Audio files…", "file.import_audio", "File > Import > Audio..."), ("MIDI file…", "file.import_midi", "File > Import > MIDI...")]
        {
            let response = ui.button(label);
            crate::assistant::register(app, command, &response, label);
            if response.clicked() {
                crate::menus::invoke_menu(app, command, path);
                ui.close();
            }
        }
    });
}
