//! The mixer stays beneath the timeline, remains editable, and keeps its docked placement.
use egui::{PointerButton, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use soundcraft_ui_egui::{MainWindow, Services, SoundApp, UiState};

fn harness(saved: Option<UiState>, size: egui::Vec2, scale: f32) -> Harness<'static, SoundApp> {
    let mut app = SoundApp::new_classic(soundcraft_engine::Engine::default(), None, Services::default());
    app.run("track.new", json!({"name":"Voice"})).unwrap();
    if let Some(saved) = saved {
        app.ui = saved;
    } else {
        app.run("window.mix", json!({})).unwrap();
    }
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

#[test]
fn mixer_panel_is_below_the_timeline_and_its_controls_are_visible() {
    for (size, scale) in [(vec2(1600.0, 1000.0), 1.0), (vec2(1000.0, 800.0), 1.0), (vec2(1600.0, 1000.0), 2.0)] {
        let mut h = harness(None, size, scale);
        let mixer = h.get_by_role_and_label(egui::accesskit::Role::Tab, "Mix").rect();
        let timeline = soundcraft_ui_egui::edit_window::rect(h.state().edit_layout.timeline);
        assert!(
            timeline.height() > 100.0 && mixer.top() >= timeline.bottom(),
            "the timeline retains working space above Mix: {timeline:?}, {mixer:?}"
        );
        let fader = h.get_by_label("Voice level").rect();
        assert!(fader.top() > mixer.bottom() && fader.bottom() < size.y - 22.0, "the fader must fit the bottom panel: {fader:?}");
        let before = h.state().engine.session().track_by_name("Voice").unwrap().mixer.volume_db;
        let from = fader.center();
        let to = from + vec2(0.0, 15.0);
        h.event(egui::Event::PointerMoved(from));
        h.event(egui::Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
        h.event(egui::Event::PointerMoved(to));
        h.run_steps(2);
        h.event(egui::Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
        assert!(h.state().engine.session().track_by_name("Voice").unwrap().mixer.volume_db < before);
        h.get_all_by_label("Mute for Voice").find(|node| node.rect().top() > mixer.bottom()).unwrap().click();
        h.run_steps(3);
        assert!(h.state().engine.session().track_by_name("Voice").unwrap().mixer.mute);
        h.get_by_label("Details").click();
        h.run_steps(3);
        assert!(
            h.query_by_label_contains("INSERTS A-E").is_some()
                || h.output().shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "INSERTS A-E"))
        );
        if let Some(directory) = std::env::var_os("SOUNDCRAFT_MIX_PANEL_SHOTS") {
            let directory = std::path::PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            h.render().unwrap().save(directory.join(format!("mix-panel-{}-{scale}x-details.png", size.x))).unwrap();
        }
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        if let Some(directory) = std::env::var_os("SOUNDCRAFT_MIX_PANEL_SHOTS") {
            h.render().unwrap().save(std::path::PathBuf::from(directory).join(format!("mix-panel-{}-{scale}x.png", size.x))).unwrap();
        }
    }
}

#[test]
fn mixer_panel_close_reopen_midi_and_restart_keep_the_timeline() {
    let mut h = harness(None, vec2(1400.0, 1000.0), 1.0);
    h.get_by_role_and_label(egui::accesskit::Role::Tab, "Mix").click_button(PointerButton::Secondary);
    h.run_steps(3);
    h.get_by_label("Close Panel").click();
    h.run_steps(3);
    assert!(h.query_by_role_and_label(egui::accesskit::Role::Tab, "Mix").is_none());
    h.state_mut().run("window.mix", json!({})).unwrap();
    h.run_steps(3);
    assert_eq!(h.state().ui.window, MainWindow::Edit);
    h.state_mut().run("ui.set", json!({"window":"Mix"})).unwrap();
    assert_eq!(h.state().ui.window, MainWindow::Edit, "legacy UI automation also opens the panel");
    let rect = h.get_by_role_and_label(egui::accesskit::Role::Tab, "Mix").rect();
    h.state_mut().run("window.midi_editor", json!({"value":true})).unwrap();
    h.run_steps(3);
    h.state_mut().run("window.mix", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Voice level").is_some(), "Window > Mix activates its tab beside MIDI");
    let saved = serde_json::to_value(&h.state().ui).unwrap();
    let mut reopened = harness(Some(serde_json::from_value(saved).unwrap()), vec2(1400.0, 1000.0), 1.0);
    assert_eq!(reopened.get_by_role_and_label(egui::accesskit::Role::Tab, "Mix").rect().top(), rect.top());
    reopened.state_mut().run("window.toggle_mix_edit", json!({})).unwrap();
    reopened.run_steps(3);
    assert!(reopened.query_by_role_and_label(egui::accesskit::Role::Tab, "Mix").is_none());
    assert!(soundcraft_ui_egui::edit_window::rect(reopened.state().edit_layout.timeline).height() > 100.0);
}

#[test]
fn old_full_window_mix_preferences_restore_the_timeline_and_preserve_other_preferences() {
    let mut saved = serde_json::to_value(UiState::classic()).unwrap();
    saved["window"] = json!("Mix");
    saved["ui_scale"] = json!(1.25);
    saved.as_object_mut().unwrap().remove("show_mix");
    soundcraft_ui_egui::docking::reset_old_layouts(&mut saved);
    let restored: UiState = serde_json::from_value(saved).unwrap();
    assert_eq!(restored.window, MainWindow::Edit);
    assert_eq!(restored.ui_scale, 1.25);
    let h = harness(Some(restored), vec2(1400.0, 1000.0), 1.0);
    assert!(h.query_by_role_and_label(egui::accesskit::Role::Tab, "Mix").is_some());
    assert!(soundcraft_ui_egui::edit_window::rect(h.state().edit_layout.timeline).height() > 100.0);
}
