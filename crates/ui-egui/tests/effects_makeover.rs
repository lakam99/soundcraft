//! Built-in effect editors preserve editing, presets, bypass and bounded window geometry.
use egui::{Key, Rect, vec2};
use egui_kittest::{Harness, kittest::Queryable};
use serde_json::json;
use soundcraft_ui_egui::{Services, SoundApp};

fn harness(plugin: &str, light: bool, size: egui::Vec2, scale: f32) -> Harness<'static, SoundApp> {
    let mut app = SoundApp::new(soundcraft_engine::Engine::default(), None, Services::default());
    app.run("track.new", json!({"name":"Voice"})).unwrap();
    app.run("mix.insert", json!({"track":"Voice","slot":0,"plugin":plugin})).unwrap();
    if light {
        app.run("ui.theme", json!({"mode":"light"})).unwrap();
    }
    let track = app.engine.session().tracks[0].id;
    app.ui.plugin_windows = vec![(track, 0)];
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
    if let Some(path) = std::env::var_os("SOUNDCRAFT_EFFECT_SHOTS") {
        let path = std::path::PathBuf::from(path);
        std::fs::create_dir_all(&path).unwrap();
        h.render().unwrap().save(path.join(format!("{name}.png"))).unwrap();
    }
}

#[test]
fn all_twenty_seven_effects_have_designed_bounded_accessible_editors() {
    for info in soundcraft_dsp::plugins().iter().filter(|info| !info.is_instrument) {
        let mut h = harness(info.id, false, vec2(1200.0, 800.0), 1.0);
        let track = h.state().engine.session().tracks[0].id;
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique(("plugin", track.0, 0usize)))).unwrap();
        assert!(Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 800.0)).contains_rect(window), "{}: {window:?}", info.id);
        assert!(h.query_by_label("Settings illustration").is_some(), "{} has its designed header", info.id);
        assert!(h.query_by_label("Presets…").is_some());
        if info.id == "eq_7band" {
            let control = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Input").rect();
            assert!(window.contains_rect(control), "EQ7 must open with an editable primary control visible: {control:?}");
        }
        shot(&mut h, info.id);
    }
}

#[test]
fn discrete_toggles_have_separate_undo_steps_and_long_eq_controls_are_reachable() {
    let mut h = harness("trim", false, vec2(1200.0, 800.0), 1.0);
    let track = h.state().engine.session().tracks[0].id;
    for _ in 0..2 {
        h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Phase Invert").click();
        h.run_steps(3);
    }
    assert_eq!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["invert"], 0.0);
    assert!(h.state_mut().engine.undo());
    h.run_steps(3);
    assert_eq!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["invert"], 1.0);
    drop(h);

    let mut h = harness("eq_7band", false, vec2(1000.0, 650.0), 1.0);
    let track = h.state().engine.session().tracks[0].id;
    let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique(("plugin", track.0, 0usize)))).unwrap();
    let mut found = false;
    for _ in 0..25 {
        if let Some(control) = h.query_by_role_and_label(egui::accesskit::Role::ComboBox, "LPF Slope")
            && window.contains_rect(control.rect())
        {
            control.click();
            found = true;
            break;
        }
        h.event(egui::Event::PointerMoved(egui::pos2(window.center().x, window.bottom() - 40.0)));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -220.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
        });
        h.run_steps(3);
    }
    assert!(found, "the last EQ band must be reachable inside the scroll area");
    h.run_steps(3);
    h.get_by_label("6 dB/oct").click();
    h.run_steps(3);
    assert_eq!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["lpf_slope"], 0.0);
    shot(&mut h, "eq_7band-last-controls");
}

#[test]
fn light_hidpi_and_long_effects_fit_a_small_display() {
    for plugin in ["eq_7band", "channel_strip", "room_reverb", "lofi", "auto_pan", "invert"] {
        let mut h = harness(plugin, true, vec2(1000.0, 650.0), 2.0);
        let track = h.state().engine.session().tracks[0].id;
        let window = h.ctx.memory(|m| m.area_rect(egui::Id::unique(("plugin", track.0, 0usize)))).unwrap();
        assert!(window.bottom() <= 650.0 && window.right() <= 1000.0, "{plugin}: {window:?}");
        if plugin == "eq_7band" {
            let control = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Input").rect();
            assert!(window.contains_rect(control), "a short high-DPI window must show an editable primary row: {control:?}");
        }
        shot(&mut h, &format!("{plugin}-light-2x"));
    }
}

#[test]
fn parameter_edit_reset_bypass_factory_preset_and_undo_keep_working() {
    let mut h = harness("gain", false, vec2(1200.0, 800.0), 1.0);
    let track = h.state().engine.session().tracks[0].id;
    let gain = h.get_by_role_and_label(egui::accesskit::Role::Slider, "Gain");
    gain.click();
    h.key_press(Key::ArrowLeft);
    h.run_steps(3);
    let changed = h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["gain"];
    assert!(changed < 0.0, "keyboard editing must change the engine value");
    h.get_by_label("Reset Gain").click();
    h.run_steps(3);
    assert_eq!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["gain"], 0.0);
    assert!(h.state_mut().engine.undo());
    h.run_steps(3);
    assert_eq!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["gain"], changed);
    h.get_by_label("Bypass effect").click();
    h.run_steps(3);
    assert!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().bypass);
    h.get_by_label("Presets…").click();
    h.run_steps(3);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "<factory default>").click();
    h.run_steps(3);
    assert_eq!(h.state().engine.session().track(track).unwrap().mixer.inserts[0].as_ref().unwrap().params["gain"], 0.0);
}
