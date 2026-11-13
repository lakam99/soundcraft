use super::{BrowserTab, button, run, selected};
use crate::SoundApp;
use egui::{RichText, Ui};
use serde_json::json;
use soundcraft_model::TrackKind;

pub fn show(app: &mut SoundApp, ui: &mut Ui) {
    egui::Frame::new().inner_margin(12).show(ui, |ui| body(app, ui));
}

fn body(app: &mut SoundApp, ui: &mut Ui) {
    egui::ScrollArea::vertical().id_salt("workspace.inspector").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 9.0;
        let Some(id) = selected(app) else {
            ui.heading("Track inspector");
            ui.label("Select a track to see its input, instrument and channel settings.");
            ui.label("Add a track from the toolbar or the arrangement to get started.");
            return;
        };
        let session = app.engine.session_arc();
        let Some(track) = session.track(id) else { return };
        ui.heading(&track.name);
        ui.label(
            RichText::new(match track.kind {
                TrackKind::Instrument => "Instrument track",
                TrackKind::Midi => "MIDI track",
                TrackKind::Audio => "Audio track",
                _ => "Channel",
            })
            .small()
            .weak(),
        );
        ui.separator();
        if track.kind.is_midi() {
            instrument(app, ui, track);
        } else {
            input(app, ui, track);
        }
        ui.separator();
        ui.strong("Channel");
        let mut db = track.mixer.volume_db;
        ui.label("Volume (dB)");
        let response = ui.add_sized([ui.available_width(), 24.0], egui::Slider::new(&mut db, -144.0..=12.0)).accessible_name("Selected track volume");
        crate::assistant::register(app, "inspector.volume", &response, "Selected track volume");
        if response.changed()
            && let Err(error) = app.engine.execute_merged("mix.volume", &json!({"tracks":[id.0],"db":db}), &format!("inspector.volume.{}", id.0))
        {
            app.ui.status = error.to_string();
        }
        if response.drag_stopped() {
            app.engine.end_merge();
        }
        pan(app, ui, track);
        ui.horizontal(|ui| {
            for (label, command, on) in [("Mute", "mix.mute", track.mixer.mute), ("Solo", "mix.solo", track.mixer.solo)] {
                let response = ui.add_sized([70.0, 30.0], egui::Button::new(label).selected(on));
                crate::assistant::register(app, &format!("inspector.{command}"), &response, label);
                if response.clicked() {
                    run(app, command, json!({"tracks":[id.0]}));
                }
            }
        });
        ui.label("Output");
        route(app, ui, track, false);
        ui.separator();
        ui.strong("Effects");
        for (slot, insert) in track.mixer.inserts.iter().enumerate().filter_map(|(slot, insert)| insert.as_ref().map(|insert| (slot, insert))) {
            let label = crate::mix_window::plugin_info(&insert.plugin).map_or(insert.plugin.clone(), |info| info.name.to_string());
            ui.label(&label);
            ui.horizontal_wrapped(|ui| {
                let response = ui.button("Open").accessible_name(format!("Open {label}"));
                crate::assistant::register(app, &format!("inspector.effect.{slot}"), &response, "Open effect");
                if response.clicked() {
                    app.pending_open = Some((id, slot));
                }
                let response = ui.selectable_label(insert.bypass, "Bypass").accessible_name(format!("Bypass {label}"));
                crate::assistant::register(app, &format!("inspector.bypass.{slot}"), &response, "Bypass effect");
                if response.clicked() {
                    run(app, "mix.insert_bypass", json!({"track":id.0,"slot":slot}));
                }
                let response = ui.button("Remove").accessible_name(format!("Remove {label}"));
                crate::assistant::register(app, &format!("inspector.remove.{slot}"), &response, "Remove effect");
                if response.clicked() {
                    run(app, "mix.insert_remove", json!({"track":id.0,"slot":slot}));
                }
            });
        }
        let response = ui.add_sized([ui.available_width(), 30.0], egui::Button::new("Add effect…"));
        crate::assistant::register(app, "inspector.addEffect", &response, "Add effect");
        if response.clicked() {
            app.workspace.browser_tab = BrowserTab::Effects;
            run(app, "workspace.browser", json!({"value":true}));
        }
    });
}

fn pan(app: &mut SoundApp, ui: &mut Ui, track: &soundcraft_model::Track) {
    for (index, value) in track.mixer.pan.iter().take(2).enumerate() {
        let label = if track.mixer.pan.len() == 1 {
            "Pan"
        } else if index == 0 {
            "Left channel pan"
        } else {
            "Right channel pan"
        };
        let mut pan = *value;
        ui.label(label);
        let response = ui.add_sized([ui.available_width(), 24.0], egui::Slider::new(&mut pan, -1.0..=1.0)).accessible_name(label);
        crate::assistant::register(app, &format!("inspector.pan.{index}"), &response, label);
        if response.changed()
            && let Err(error) = app.engine.execute_merged(
                "mix.pan",
                &json!({"tracks":[track.id.0],"pan":pan,"index":index}),
                &format!("inspector.pan.{}.{index}", track.id.0),
            )
        {
            app.ui.status = error.to_string();
        }
        if response.drag_stopped() {
            app.engine.end_merge();
        }
    }
}

fn input(app: &mut SoundApp, ui: &mut Ui, track: &soundcraft_model::Track) {
    ui.strong("Recording");
    ui.label("Input");
    route(app, ui, track, true);
    button(app, ui, "inspector.inputs", "Map audio inputs…", "window.io_setup", json!({"value":true}));
    ui.horizontal_wrapped(|ui| {
        for (label, command, on) in
            [("Monitor", "mix.input_monitor", track.mixer.input_monitor), ("Arm recording", "mix.record_arm", track.mixer.record_arm)]
        {
            let response = ui.add_sized([0.0, 30.0], egui::Button::new(label).selected(on));
            crate::assistant::register(app, &format!("inspector.{command}"), &response, label);
            if response.clicked() {
                run(app, command, json!({"tracks":[track.id.0]}));
            }
        }
    });
    ui.label(RichText::new("Choose an input → arm this track → Record.").small().weak());
}

fn instrument(app: &mut SoundApp, ui: &mut Ui, track: &soundcraft_model::Track) {
    ui.strong("Instrument");
    let name = track
        .instrument
        .as_ref()
        .map(|insert| crate::mix_window::plugin_info(&insert.plugin).map_or(insert.plugin.clone(), |info| info.name.to_string()))
        .unwrap_or_else(|| "Choose an instrument".into());
    let response = ui.add_sized([ui.available_width(), 30.0], egui::Button::new(name));
    crate::assistant::register(app, "inspector.instrument", &response, "Choose or edit instrument");
    egui::Popup::menu(&response).show(|ui| crate::mix_window::instrument_menu(app, ui, track.id));
    button(app, ui, "inspector.writeNotes", "+ MIDI part / write notes", "workspace.midiPart", json!({}));
    button(app, ui, "inspector.midiEditor", "Open MIDI editor", "window.midi_editor", json!({"value":true}));
    ui.label(RichText::new("Draw notes in the MIDI editor or import a MIDI file. Live MIDI input is not available in this build.").small().weak());
}

fn route(app: &mut SoundApp, ui: &mut Ui, track: &soundcraft_model::Track, input: bool) {
    let label = if input { crate::mix_window::input_name(app, track) } else { crate::mix_window::route_name(app, &track.mixer.output) };
    let response = ui.add_sized([ui.available_width(), 30.0], egui::Button::new(label));
    crate::assistant::register(
        app,
        if input { "inspector.input" } else { "inspector.output" },
        &response,
        if input { "Selected track input" } else { "Selected track output" },
    );
    crate::mix_window::route_popup(app, &response, track.id, input);
}
