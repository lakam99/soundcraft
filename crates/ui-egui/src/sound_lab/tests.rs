use super::*;
use std::time::{Duration, Instant};
fn app() -> (SoundApp, TrackId) {
    let mut app = SoundApp::new_classic(soundcraft_engine::Engine::default(), None, crate::Services::default());
    app.run("soundlab.new", json!({})).unwrap();
    let tid = app.engine.session().tracks[0].id;
    (app, tid)
}
fn change(app: &mut SoundApp, tid: TrackId, name: &str) {
    let mut patch = wanted(app, tid, &insert(app, tid).unwrap()).design().clone();
    patch.name = name.into();
    patch.modules[0].params.insert("pitch".into(), 12.0);
    let revision = app.engine.revision;
    app.run("soundlab.preview", json!({"track":tid.0,"base_revision":revision,"patch":patch})).unwrap();
}
fn wait(app: &mut SoundApp) {
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.sound_lab.job.is_some() && Instant::now() < deadline {
        tick(app, &ctx);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(app.sound_lab.job.is_none());
    assert!(app.sound_lab.error.is_empty(), "{}", app.sound_lab.error);
}
#[test]
fn forge_candidates_are_isolated_commit_is_one_undo_and_stale_requests_are_rejected() {
    let (mut app, tid) = app();
    let before = app.engine.session_arc();
    let revision = app.engine.revision;
    change(&mut app, tid, "Glass halo");
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    assert_ne!(app.audible_session().track(tid).unwrap().instrument, app.engine.session().track(tid).unwrap().instrument);
    let valid = app.sound_lab.drafts[&tid].patch.clone();
    let mut bad = valid.design().clone();
    bad.routes[0].from = "missing".into();
    assert!(app.run("soundlab.preview", json!({"track":tid.0,"base_revision":revision,"patch":bad})).is_err());
    assert_eq!(app.sound_lab.drafts[&tid].patch, valid);
    app.run("soundlab.commit", json!({"track":tid.0})).unwrap();
    assert_eq!(insert(&app, tid).unwrap().hybrid.unwrap().design().name, "Glass halo");
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(app.engine.session(), before.as_ref());
    app.run("edit.redo", json!({})).unwrap();
    change(&mut app, tid, "Another candidate");
    app.run("mix.instrument_param", json!({"track":tid.0,"param":"macro1","value":0.8})).unwrap();
    let edited = app.engine.session_arc();
    assert!(app.run("soundlab.commit", json!({"track":tid.0})).is_err());
    assert!(Arc::ptr_eq(&edited, &app.engine.session_arc()));
    assert!(app.run("soundlab.preview", json!({"track":tid.0,"base_revision":revision,"patch":valid})).is_err());
}
#[test]
fn forge_render_derive_preserves_patch_sample_root_and_undo() {
    let (mut app, tid) = app();
    change(&mut app, tid, "Copper bell");
    assert!(app.run("soundlab.to_sampler", json!({"track":tid.0})).is_err());
    app.run("soundlab.commit", json!({"track":tid.0})).unwrap();
    let before = app.engine.session_arc();
    let original = insert(&app, tid).unwrap();
    app.run("soundlab.render", json!({"track":tid.0,"settings":{"note":57,"velocity":90,"gate":0.1,"tail":0.2,"sample_rate":16000}})).unwrap();
    wait(&mut app);
    assert!(app.sound_lab.ready.as_ref().unwrap().metrics.peak > 0.01);
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    app.run("soundlab.to_sampler", json!({"track":tid.0,"settings":{"note":57,"velocity":90,"gate":0.1,"tail":0.2,"sample_rate":16000}})).unwrap();
    wait(&mut app);
    assert_eq!(app.engine.session().tracks.len(), 2);
    assert!(
        app.engine.session().track(tid).unwrap().mixer.input_monitor,
        "The source's monitor preference is retained; live MIDI monitors only the newly selected sampler"
    );
    assert!(app.engine.session().tracks[1].mixer.input_monitor);
    assert_eq!(insert(&app, tid).unwrap(), original);
    let sampler = app.engine.session().tracks[1].instrument.as_ref().unwrap();
    assert_eq!(sampler.plugin, "sampler");
    assert_eq!(sampler.params["root_note"], 57.0);
    assert_eq!(sampler.params["loop_mode"], 0.0);
    assert!(app.engine.session().pool.contains(sampler.sample.unwrap()));
    let json = serde_json::to_vec(app.engine.session()).unwrap();
    let restored: soundcraft_model::Session = serde_json::from_slice(&json).unwrap();
    assert_eq!(restored.track(tid).unwrap().instrument, Some(original));
    assert_eq!(restored.tracks[1].instrument.as_ref().unwrap().sample, sampler.sample);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(app.engine.session(), before.as_ref());
}
#[test]
fn forge_cancel_close_and_new_document_reject_old_results() {
    let (mut app, tid) = app();
    app.run("soundlab.render", json!({"track":tid.0})).unwrap();
    app.run("soundlab.cancel", json!({})).unwrap();
    assert!(app.sound_lab.job.is_none());
    assert!(app.sound_lab.ready.is_none());
    change(&mut app, tid, "Lost candidate");
    app.ui.plugin_windows.clear();
    tick(&mut app, &egui::Context::default());
    assert!(app.sound_lab.active.is_none());
    assert_eq!(app.audible_session(), app.engine.session_arc());
    app.run("soundlab.render", json!({"track":tid.0})).unwrap();
    app.run("session.new", json!({})).unwrap();
    assert!(app.sound_lab.job.is_none());
    assert!(app.sound_lab.drafts.is_empty());
    assert!(app.sound_lab.ready.is_none());
}
#[test]
fn forge_opening_a_sparse_valid_patch_does_not_create_an_idle_candidate() {
    let (mut app, tid) = app();
    let mut design = Patch::starter().design().clone();
    for module in &mut design.modules {
        module.params.clear();
    }
    design.modules[0].params.insert("shape".into(), 1.5);
    app.engine.execute("synth.patch", &json!({"track":tid.0,"patch":design})).unwrap();
    let before = app.engine.session_arc();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1150.0, 1000.0)).build_ui_state(
        move |ui, app: &mut SoundApp| {
            crate::assistant::begin_frame(app, ui);
            show(app, ui, tid);
        },
        app,
    );
    h.run_steps(4);
    assert!(h.state().sound_lab.drafts.is_empty(), "Displaying defaults and a quantized selector must not stage an unrequested patch edit");
    assert!(Arc::ptr_eq(&before, &h.state().engine.session_arc()));
}
#[test]
fn audio_preferences_save_immediately_and_save_failure_retains_working_choices() {
    let (mut app, _) = app();
    let saved = Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
    let sink = saved.clone();
    app.preferences_writer = Some(Box::new(move |state| {
        *sink.lock().unwrap() = serde_json::to_vec(state).unwrap();
        Ok(())
    }));
    let mut cfg = app.ui.audio.clone();
    cfg.input = Some("Unavailable named input".into());
    cfg.output = Some("Unavailable named output".into());
    cfg.buffer_size = Some(512);
    app.persist_audio_configuration(&cfg).unwrap();
    let restored: crate::UiState = serde_json::from_slice(&saved.lock().unwrap()).unwrap();
    assert_eq!(restored.audio, cfg);
    let old = app.ui.audio.clone();
    app.preferences_writer = Some(Box::new(|_| Err("disk full".into())));
    assert!(app.apply_audio_config(cfg).unwrap_err().contains("disk full"));
    assert_eq!(app.ui.audio, old);
    assert!(app.player.is_none());
}
#[test]
fn forge_cable_arrangement_does_not_edit_the_document_or_undo_history() {
    let (mut app, tid) = app();
    let before = app.engine.session_arc();
    let history = app.engine.undo_history();
    let keys = app.run("soundlab.cables", json!({"track":tid.0})).unwrap();
    let key = keys[0]["key"].as_str().unwrap();
    app.run("soundlab.cables", json!({"track":tid.0,"key":key,"offset":[80,-40]})).unwrap();
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    assert_eq!(app.engine.undo_history(), history);
    assert!(app.run("soundlab.cables", json!({"track":tid.0,"key":key,"offset":[1e20,0]})).is_err());
    assert_eq!(app.run("soundlab.cables", json!({"track":tid.0})).unwrap()[0]["offset"], json!([80.0, -40.0]));
}

#[test]
fn forge_analog_jacks_connect_without_document_edits_and_escape_cancels_a_lead() {
    let (mut app, tid) = app();
    let mut patch = Patch::starter().design().clone();
    patch.routes.clear();
    let revision = app.engine.revision;
    app.run("soundlab.preview", json!({"track":tid.0,"base_revision":revision,"patch":patch})).unwrap();
    let before = app.engine.session_arc();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1150.0, 1000.0)).build_ui_state(
        move |ui, app: &mut SoundApp| {
            crate::assistant::begin_frame(app, ui);
            show(app, ui, tid);
        },
        app,
    );
    h.run_steps(4);
    let click = |h: &mut egui_kittest::Harness<SoundApp>, id: &str| {
        let wheel = |h: &mut egui_kittest::Harness<SoundApp>, dy: f32, frames: usize| {
            h.hover_at(egui::pos2(1000.0, 450.0));
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers::NONE,
            });
            h.run_steps(frames);
        };
        wheel(h, 10000.0, 25);
        for _ in 0..30 {
            let elements = crate::assistant::elements(h.state());
            let item = elements.as_array().unwrap().iter().find(|v| v["id"] == id);
            if let Some(item) = item {
                let r = &item["rect"];
                let y = (r[1].as_f64().unwrap() + r[3].as_f64().unwrap() / 2.0) as f32;
                if (150.0..=730.0).contains(&y) {
                    break;
                }
                wheel(h, 450.0 - y, 12);
            } else {
                wheel(h, -160.0, 12);
            }
        }
        let elements = crate::assistant::elements(h.state());
        let item = elements.as_array().unwrap().iter().find(|v| v["id"] == id).unwrap_or_else(|| panic!("Missing visible control {id}"));
        let r = &item["rect"];
        let pos = egui::pos2(
            (r[0].as_f64().unwrap() + r[2].as_f64().unwrap() / 2.0) as f32,
            (r[1].as_f64().unwrap() + r[3].as_f64().unwrap() / 2.0) as f32,
        );
        h.hover_at(pos);
        h.run_steps(1);
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
    };
    click(&mut h, "a:osc_a");
    assert!(h.state().sound_lab.wiring.is_some());
    assert!(h.state().sound_lab.drafts[&tid].patch.design().routes.is_empty());
    click(&mut h, "i:filter");
    assert!(h.state().sound_lab.wiring.is_none());
    assert_eq!(h.state().sound_lab.drafts[&tid].patch.design().routes.len(), 1);
    assert!(Arc::ptr_eq(&before, &h.state().engine.session_arc()));
    let count = h.state().sound_lab.drafts[&tid].patch.design().modulation.len();
    click(&mut h, "macro:2");
    click(&mut h, "p:filter:q");
    assert_eq!(h.state().sound_lab.drafts[&tid].patch.design().modulation.len(), count + 1);
    click(&mut h, "a:osc_a");
    let patch = h.state().sound_lab.drafts[&tid].patch.clone();
    click(&mut h, "p:filter:cutoff");
    assert_eq!(h.state().sound_lab.drafts[&tid].patch, patch);
    assert!(h.state().sound_lab.error.contains("CV OUT"));
    h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    assert!(h.state().sound_lab.wiring.is_none());
    h.event(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: egui::Modifiers::NONE });
    h.run_steps(1);
    click(&mut h, "soundlab.param.osc_a.shape");
    for pressed in [true, false] {
        h.event(egui::Event::Key { key: egui::Key::ArrowUp, physical_key: None, pressed, repeat: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
    }
    assert_eq!(h.state().sound_lab.drafts[&tid].patch.design().modules[0].params["shape"], 3.0, "A selector arrow advances one detent");
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::CTRL));
    h.run_steps(3);
    assert_eq!(
        h.state().sound_lab.drafts[&tid].patch.design().modules[0].params["shape"],
        3.0,
        "Holding Ctrl without wheel motion does not turn a selector"
    );
}
