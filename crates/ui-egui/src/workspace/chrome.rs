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
    let response = ui.add_sized([0.0, 30.0], egui::Button::new(label));
    crate::assistant::register(app, id, &response, label);
    if response.clicked() {
        run(app, command, json!({}));
    }
}

fn view(app: &mut SoundApp, ui: &mut Ui, label: &str, command: &str, selected: bool) {
    let response = ui.add_sized([0.0, 30.0], egui::Button::new(label).selected(selected));
    crate::assistant::register(app, command, &response, label);
    if response.clicked() {
        run(app, command, json!({}));
    }
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
        view(app, ui, "AI Chat", "ai.chat", app.ui.show_ai_chat);
    });
}

fn tools(app: &mut SoundApp, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        let current = app.engine.session().edit.tool;
        for (tool, label) in
            [(Tool::Grabber, "Move"), (Tool::Selector, "Range"), (Tool::Trim, "Trim"), (Tool::Pencil, "Draw"), (Tool::Smart, "Smart")]
        {
            let response = ui.add_sized([0.0, 28.0], egui::Button::new(label).selected(current == tool));
            crate::assistant::register(app, &format!("workspace.tool.{}", tool.id()), &response, label);
            if response.clicked() {
                run(app, "edit.tool", json!({"tool":tool.id()}));
            }
        }
        ui.separator();
        let mut snap = app.engine.session().edit.edit_mode.is_grid();
        let response = ui.checkbox(&mut snap, "Snap");
        crate::assistant::register(app, "workspace.snap", &response, "Snap to grid");
        if response.changed() {
            run(app, "edit.mode", json!({"mode":if snap {"grid"} else {"slip"}}));
        }
        let grid = app.engine.session().edit.grid.label();
        let grid_response = craft_ui::ComboBox::from_label("Grid").selected_text(&grid).width(95.0).show_ui(ui, |ui| {
            for value in ["1 bar", "1/4", "1/8", "1/16", "1/32"] {
                if ui.selectable_label(grid == value, value).clicked() {
                    run(app, "view.grid", json!({"value":value}));
                }
            }
        });
        crate::assistant::register(app, "workspace.grid", &grid_response.response, "Grid resolution");
        action(app, ui, "workspace.zoomOut", "Zoom out", "view.zoom_out");
        action(app, ui, "workspace.zoomIn", "Zoom in", "view.zoom_in");
        ui.menu_button("More tools", |ui| {
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
    ui.spacing_mut().item_spacing = vec2(9.0, 5.0);
    ui.horizontal_wrapped(|ui| {
        action(app, ui, "transport.rtz", "Go to start", "transport.rtz");
        view(app, ui, "Stop", "transport.stop", !app.is_playing());
        view(app, ui, "Play", "transport.play", app.is_playing());
        let response = ui.add_sized(
            [86.0, 32.0],
            egui::Button::new(RichText::new("Record").color(egui::Color32::from_rgb(245, 100, 110))).selected(app.engine.transport.recording),
        );
        crate::assistant::register(app, "transport.record", &response, "Record audio");
        if response.clicked() {
            run(app, "transport.record", json!({}));
        }
        response.on_hover_text(app.record_hint().unwrap_or("Record armed audio tracks"));
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
            egui::Color32::from_rgb(100, 210, 170)
        } else {
            egui::Color32::from_rgb(245, 185, 100)
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
