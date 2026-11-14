//! Workflow acceptance for the default workspace, independent of the retained Classic interface.
use egui::{Key, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp, UiState, workspace::BrowserTab};

fn harness(size: egui::Vec2, scale: f32) -> Harness<'static, SoundApp> {
    let app = SoundApp::new(soundcraft_engine::Engine::default(), None, Services::default());
    let mut h = Harness::builder().with_size(size).with_pixels_per_point(scale).wgpu().build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.input_mut().max_texture_side = Some(8192);
    h.run_steps(5);
    h
}

fn shot(h: &mut Harness<SoundApp>, name: &str) {
    if let Some(directory) = std::env::var_os("SOUNDCRAFT_MODERN_SHOTS") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        h.render().unwrap().save(directory.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn essential_controls_and_equal_start_paths_remain_visible_at_small_and_hidpi_sizes() {
    for (size, scale) in [(vec2(1000.0, 800.0), 1.0), (vec2(1600.0, 1000.0), 1.0), (vec2(1000.0, 800.0), 2.0)] {
        let mut h = harness(size, scale);
        let screen = Rect::from_min_size(egui::Pos2::ZERO, size);
        for label in ["+ Audio track", "+ Instrument track", "Inspector · F4", "Browser · F5", "Mix · F3", "MIDI · F2", "Play", "Stop", "Record"]
        {
            let rect = h.get_by_label(label).rect();
            assert!(screen.contains_rect(rect), "{label} must remain visible at {size:?}: {rect:?}");
        }
        assert!(h.get_by_label("Play").rect().top() > size.y - 100.0);
        assert!(h.query_by_label("Record audio").is_some());
        assert!(h.query_by_label("Write MIDI").is_some());
        for node in h.get_all_by_label("Import audio or MIDI…") {
            let rect = node.rect();
            assert!(
                rect.width() > 150.0 && rect.height() <= 40.0 && screen.contains_rect(rect),
                "Import must be a usable horizontal button: {rect:?}"
            );
        }
        shot(&mut h, &format!("modern-empty-{}-{scale}x", size.x));
        if size.x > 1400.0 {
            h.state_mut().engine = soundcraft_engine::demo::demo_engine();
            h.state_mut().run("window.mix", json!({})).unwrap();
            h.run_steps(5);
            shot(&mut h, "modern-demo-mix");
            h.state_mut().run("ui.theme", json!({"mode":"light"})).unwrap();
            h.run_steps(5);
            shot(&mut h, "modern-demo-mix-light");
        }
    }
}

#[test]
fn modern_palettes_keep_small_text_and_selected_labels_readable() {
    let luminance = |color: egui::Color32| {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    };
    for light in [false, true] {
        let t = soundcraft_ui_egui::theme::modern_tokens(light);
        for (ink, fill) in [
            (t.text, t.panel_bg),
            (t.text_dim, t.panel_bg),
            (t.text, t.button),
            (t.text, t.name_field),
            (t.text, t.row_selected),
            (t.text, t.tempo_ruler),
            (t.auto_write, t.button),
            (t.meter_green, t.panel_bg2),
            (t.meter_yellow, t.panel_bg2),
            (egui::Color32::WHITE, t.accent_dark),
            (egui::Color32::WHITE, t.name_field_sel),
            (egui::Color32::WHITE, t.solo),
            (egui::Color32::WHITE, t.mute),
            (egui::Color32::WHITE, t.rec),
            (egui::Color32::WHITE, t.input),
        ] {
            let (a, b) = (luminance(ink), luminance(fill));
            let contrast = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(contrast >= 4.5, "Modern light={light}: {ink:?} on {fill:?} has only {contrast:.2}:1 contrast");
        }
    }
}

#[test]
fn timeline_icons_keep_accessible_names_selected_state_and_command_behavior() {
    let mut h = harness(vec2(1000.0, 800.0), 1.0);
    for label in [
        "Move",
        "Range",
        "Trim",
        "Draw",
        "Smart",
        "Snap",
        "Zoom out",
        "Zoom in",
        "More tools",
        "Go to start",
        "Stop",
        "Play",
        "Record",
        "Loop",
        "Click",
    ] {
        let rect = h.get_by_role_and_label(egui::accesskit::Role::Button, label).rect();
        assert!(rect.width() >= 30.0 && rect.height() >= 30.0, "{label} needs a usable icon target: {rect:?}");
        assert!(
            !h.output().shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == label)),
            "{label} should be an icon with an accessible name, not a painted caption"
        );
    }
    for (label, tool) in [
        ("Move", soundcraft_model::Tool::Grabber),
        ("Range", soundcraft_model::Tool::Selector),
        ("Trim", soundcraft_model::Tool::Trim),
        ("Draw", soundcraft_model::Tool::Pencil),
        ("Smart", soundcraft_model::Tool::Smart),
    ] {
        h.get_by_label(label).click();
        h.run_steps(3);
        assert_eq!(h.state().engine.session().edit.tool, tool);
    }
    let snap = h.state().engine.session().edit.edit_mode.is_grid();
    h.get_by_label("Snap").click();
    h.run_steps(3);
    assert_ne!(h.state().engine.session().edit.edit_mode.is_grid(), snap);
    let looped = h.state().engine.session().edit.loop_playback;
    h.get_by_label("Loop").click();
    h.run_steps(3);
    assert_ne!(h.state().engine.session().edit.loop_playback, looped);
    let click = h.state().engine.session().edit.click;
    h.get_by_label("Click").click();
    h.run_steps(3);
    assert_ne!(h.state().engine.session().edit.click, click);
    let zoom = h.state().engine.session().edit.zoom.samples_per_px;
    h.get_by_label("Zoom in").click();
    h.run_steps(3);
    assert!(h.state().engine.session().edit.zoom.samples_per_px < zoom);
    h.get_by_label("More tools").click();
    h.run_steps(3);
    assert!(h.query_by_label("Scrub audio").is_some());
    h.key_press(Key::Escape);
    h.run_steps(3);
    shot(&mut h, "modern-timeline-icons");
}

#[test]
fn simplified_channel_controls_keep_recording_and_advanced_routing_reachable() {
    let mut h = harness(vec2(1600.0, 1000.0), 1.0);
    h.state_mut().engine = soundcraft_engine::demo::demo_engine();
    h.run_steps(4);
    let kick = h.state().engine.session().tracks.iter().find(|track| track.name == "Kick").unwrap().id;
    let record = h.get_by_label("Record enable for Kick");
    assert!(record.rect().height() >= 24.0, "Modern lane controls need usable targets");
    record.click();
    h.run_steps(3);
    assert!(h.state().engine.session().track(kick).unwrap().mixer.record_arm);
    assert!(h.query_by_label("Track view for Kick").is_none());
    h.get_by_label("Track controls for Kick").click();
    h.run_steps(3);
    assert!(h.query_by_label("Track view for Kick").is_some(), "advanced lane controls remain available in the disclosure");
    h.key_press(Key::Escape);
    h.run_steps(3);
    h.state_mut().run("window.mix", json!({})).unwrap();
    h.run_steps(4);
    let mixer = h.get_by_role_and_label(egui::accesskit::Role::Tab, "Mix").rect();
    let fader = h.get_by_label("Kick level").rect();
    assert!(fader.top() > mixer.bottom() && fader.bottom() < 914.0 && fader.height() >= 80.0);
    let before = h.state().engine.session().track(kick).unwrap().mixer.volume_db;
    let from = fader.center();
    for (position, pressed) in [(from, true), (from + vec2(0.0, 18.0), true), (from + vec2(0.0, 18.0), false)] {
        h.event(egui::Event::PointerMoved(position));
        if position == from || !pressed {
            h.event(egui::Event::PointerButton { pos: position, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        }
        h.run_steps(2);
    }
    assert!(h.state().engine.session().track(kick).unwrap().mixer.volume_db < before);
    h.get_all_by_label("Mute for Kick").find(|node| node.rect().top() > mixer.bottom()).unwrap().click();
    h.run_steps(3);
    assert!(h.state().engine.session().track(kick).unwrap().mixer.mute);
    assert!(h.query_by_label("Input for Kick").is_none(), "routing wells should not crowd the modern mixer");
    h.get_by_label("Channel details for Kick").click();
    h.run_steps(3);
    assert!(h.query_by_label("Input for Kick").is_some());
    assert!(h.query_by_label("Output for Kick").is_some());
    assert!(h.query_by_label("Automation mode for Kick").is_some());
    shot(&mut h, "modern-channel-details");
}

#[test]
fn audio_and_instrument_workflows_create_real_tracks_and_a_midi_part() {
    let mut h = harness(vec2(1600.0, 1000.0), 1.0);
    h.get_by_label("+ Audio track").click();
    h.run_steps(4);
    assert_eq!(h.state().engine.session().tracks.len(), 1);
    assert_eq!(h.state().engine.session().tracks[0].kind, soundcraft_model::TrackKind::Audio);
    assert!(h.query_by_label("Map audio inputs…").is_some());
    assert!(h.query_by_label("Arm recording").is_some());
    shot(&mut h, "modern-audio");
    h.get_by_label("+ Instrument track").click();
    h.run_steps(4);
    assert_eq!(h.state().engine.session().tracks.len(), 2);
    let track = h.state().engine.session().edit.selected_tracks[0];
    assert_eq!(h.state().engine.session().track(track).unwrap().kind, soundcraft_model::TrackKind::Instrument);
    assert!(h.query_by_role_and_label(egui::accesskit::Role::Slider, "Left channel pan").is_some());
    assert!(h.query_by_role_and_label(egui::accesskit::Role::Slider, "Right channel pan").is_some());
    h.get_by_label("+ MIDI part / write notes").click();
    h.run_steps(5);
    let clip = soundcraft_ui_egui::midi_editor::target_clip(h.state()).expect("a real MIDI part opens for editing");
    assert!(matches!(h.state().engine.session().find_clip(clip).unwrap().1.content, soundcraft_model::ClipContent::Midi { .. }));
    assert!(h.state().ui.show_midi_editor);
    let body = h.ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("soundcraft-midi-body"))).unwrap();
    let note = egui::pos2(body.left() + 80.0, body.top() + 75.0);
    for pressed in [true, false] {
        h.event(egui::Event::PointerMoved(note));
        h.event(egui::Event::PointerButton { pos: note, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
    }
    let notes = h.state_mut().run("midi.notes", json!({"clip":clip.0})).unwrap();
    assert_eq!(notes["notes"].as_array().unwrap().len(), 1, "the new MIDI workflow supports drawing a note");
    shot(&mut h, "modern-midi");
    h.key_press(Key::F3);
    h.run_steps(4);
    assert!(h.state().ui.show_mix);
    assert!(soundcraft_ui_egui::edit_window::rect(h.state().edit_layout.timeline).height() > 100.0);
    shot(&mut h, "modern-mix");
}

#[test]
fn effect_browser_uses_an_empty_slot_and_text_entry_keeps_its_keys() {
    let mut h = harness(vec2(1400.0, 1000.0), 1.0);
    h.state_mut().run("workspace.audioTrack", json!({})).unwrap();
    let track = h.state().engine.session().edit.selected_tracks[0];
    h.state_mut().run("mix.insert", json!({"track":track.0,"slot":0,"plugin":"gain"})).unwrap();
    let effect = soundcraft_dsp::plugins().iter().find(|plugin| !plugin.is_instrument && plugin.id != "gain").unwrap();
    h.state_mut().workspace.browser_tab = BrowserTab::Effects;
    h.state_mut().workspace.search = effect.name.into();
    h.run_steps(4);
    h.get_all_by_label("Add to selected track").next().unwrap().click();
    h.run_steps(4);
    let channel = &h.state().engine.session().track(track).unwrap().mixer;
    assert_eq!(channel.inserts[0].as_ref().unwrap().plugin, "gain");
    assert_eq!(channel.inserts[1].as_ref().unwrap().plugin, effect.id);
    shot(&mut h, "modern-effects");
    h.get_by_label("Search instruments and effects").click();
    h.run_steps(2);
    let shown = h.state().ui.show_mix;
    h.key_press(Key::F3);
    h.run_steps(3);
    assert_eq!(h.state().ui.show_mix, shown, "typing/search focus owns function keys");
}

#[test]
fn legacy_custom_layout_and_preferences_are_recoverable_in_classic() {
    let mut app = SoundApp::new_classic(soundcraft_engine::Engine::default(), None, Services::default());
    app.ui.ui_scale = 1.25;
    app.ui.ai.remember_history = false;
    app.run("ui.dock", json!({"operation":"float","panel":"tracks","rect":[50,70,260,400]})).unwrap();
    let original = serde_json::to_value(&app.ui.docking).unwrap();
    let mut saved = serde_json::to_value(&app.ui).unwrap();
    for field in ["modern_workspace", "classic_layout", "modern_layout", "show_inspector", "show_browser"] {
        saved.as_object_mut().unwrap().remove(field);
    }
    soundcraft_ui_egui::workspace::migrate_legacy(&mut saved).unwrap();
    app.ui = serde_json::from_value::<UiState>(saved).unwrap();
    assert!(app.ui.modern_workspace);
    assert_eq!(app.ui.ui_scale, 1.25);
    assert!(!app.ui.ai.remember_history);
    app.run("workspace.classic", json!({})).unwrap();
    assert_eq!(serde_json::to_value(&app.ui.docking).unwrap(), original);
    let mut restored = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    restored.run_steps(5);
    assert_eq!(serde_json::to_value(&restored.state().ui.docking).unwrap(), original, "Classic rendering must not insert Modern panels");
    assert!(!restored.state().ui.show_inspector && !restored.state().ui.show_browser);
    let app = restored.state_mut();
    app.run("workspace.modern", json!({})).unwrap();
    app.run("workspace.classic", json!({})).unwrap();
    assert_eq!(serde_json::to_value(&app.ui.docking).unwrap(), original);
}
