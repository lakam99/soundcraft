use super::*;
fn app() -> SoundApp {
    let mut app = SoundApp::new_classic(soundcraft_engine::Engine::default(), None, crate::Services::default());
    app.run("pulse.new", json!({"kind":"arp"})).unwrap();
    app
}
#[test]
fn bulk_candidate_audition_store_undo_and_stale_protection() {
    let mut app = app();
    let before = app.engine.session_arc();
    let revision = app.engine.revision;
    let changes: Vec<_> = (0..16).map(|i| json!({"step":i,"pitch":(i%4)*2,"velocity":90})).collect();
    app.run("pulse.steps", json!({"base_revision":revision,"changes":changes})).unwrap();
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    let candidate = app.sequencer.draft.as_ref().unwrap().program.clone();
    assert!(app.run("pulse.steps", json!({"base_revision":revision,"changes":[{"step":0,"on":false},{"step":0,"on":true}]})).is_err());
    assert_eq!(app.sequencer.draft.as_ref().unwrap().program, candidate);
    for bank in [json!(-1), json!(4294967296u64), json!("0")] {
        assert!(app.run("pulse.steps", json!({"base_revision":revision,"bank":bank,"changes":[{"step":0,"pitch":1}]})).is_err());
        assert_eq!(app.sequencer.draft.as_ref().unwrap().program, candidate);
    }
    app.run("pulse.run", json!({})).unwrap();
    assert!(app.sequencer.running);
    let latch = app.run("pulse.latch", json!({})).unwrap_err();
    assert!(latch.contains("Stop PULSE"));
    assert!(app.sequencer.running);
    let c = app.sequencer.active.unwrap();
    assert_ne!(app.audible_session().find_clip(c).unwrap().1.content, before.find_clip(c).unwrap().1.content);
    app.run("pulse.stop", json!({})).unwrap();
    assert!(!app.sequencer.running);
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    app.run("pulse.commit", json!({})).unwrap();
    assert!(app.sequencer.draft.is_none());
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(app.engine.session(), before.as_ref());
    assert!(app.run("pulse.preview", json!({"base_revision":revision,"program":candidate})).is_err());
}
#[test]
fn printing_is_portable_and_save_reload_preserves_the_program() {
    let mut app = app();
    let c = app.sequencer.active.unwrap();
    let (track, original) = app.engine.session().find_clip(c).unwrap();
    let original = original.clone();
    let before = app.engine.session_arc();
    let v = app.run("pulse.print", json!({"track":track.0,"at":original.end()})).unwrap();
    let printed = ClipId(v["clip"].as_u64().unwrap());
    let out = app.engine.session().find_clip(printed).unwrap().1;
    assert_eq!(out.content, original.content);
    assert!(out.step_sequencer.is_none());
    assert_eq!(*app.engine.session().find_clip(c).unwrap().1, original);
    let json = serde_json::to_string(app.engine.session()).unwrap();
    let reopened: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(reopened.find_clip(c).unwrap().1.step_sequencer, original.step_sequencer);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(app.engine.session(), before.as_ref());
}
#[test]
fn forge_and_pattern_candidates_compose_without_replacing_music() {
    let mut app = app();
    let c = app.sequencer.active.unwrap();
    let track = app.engine.session().find_clip(c).unwrap().0;
    app.run("soundlab.open", json!({"track":track.0})).unwrap();
    let v = app.run("soundlab.inspect", json!({"track":track.0})).unwrap();
    let mut patch = v["committed"].clone();
    patch["name"] = json!("Sequence sound");
    app.run("soundlab.preview", json!({"track":track.0,"base_revision":app.engine.revision,"patch":patch})).unwrap();
    let before = app.engine.session_arc();
    app.run("pulse.steps", json!({"base_revision":app.engine.revision,"changes":[{"step":0,"pitch":12}]})).unwrap();
    app.run("pulse.run", json!({})).unwrap();
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    assert_eq!(app.audible_session().track(track).unwrap().instrument.as_ref().unwrap().hybrid.as_ref().unwrap().design().name, "Sequence sound");
    assert!(app.run("pulse.print", json!({"track":track.0,"at":96000})).is_err());
    app.run("pulse.steps", json!({"base_revision":app.engine.revision,"changes":[{"step":0,"pitch":7}]})).unwrap();
    assert!(!app.sequencer.running);
}

#[test]
fn real_forge_and_drum_patterns_render_audio_and_reset_releases_notes() {
    for kind in ["arp", "drum"] {
        let mut app = SoundApp::new_classic(soundcraft_engine::Engine::default(), None, crate::Services::default());
        app.run("pulse.new", json!({"kind":kind})).unwrap();
        app.run("pulse.run", json!({})).unwrap();
        let s = app.audible_session();
        let mut mix = soundcraft_mix::MixEngine::new(48000.0, 512);
        let mut out = vec![vec![0.0; 512]; 2];
        let mut peak = 0.0f32;
        for pos in (0..48000).step_by(512) {
            mix.render(&s, pos, 512, &mut out);
            for x in out.iter().flatten() {
                assert!(x.is_finite());
                peak = peak.max(x.abs());
            }
        }
        assert!(peak > 0.001, "{kind} must be audible, got {peak}");
        mix.reset();
        mix.render(&s, 480000, 512, &mut out);
        assert!(out.iter().flatten().all(|x| x.abs() < 1e-5), "Reset must release voices: {kind}");
        app.run("pulse.stop", json!({})).unwrap();
        assert!(!app.is_playing());
    }
}

#[test]
fn viewing_unstored_rests_is_navigation_and_compact_gate_keys_are_visible() {
    use egui_kittest::kittest::Queryable;
    let mut app = app();
    let clip = app.sequencer.active.unwrap();
    let mut p = Program::default();
    p.banks[0].length = 64;
    p.banks[0].lanes[0].steps[0].on = true;
    app.run("sequencer.set", json!({"clip":clip.0,"program":p,"base_revision":app.engine.revision})).unwrap();
    app.run("pulse.open", json!({})).unwrap();
    app.run("pulse.run", json!({})).unwrap();
    let before = app.engine.session_arc();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 650.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            crate::assistant::begin_frame(app, ui);
            show(app, ui.ctx());
        },
        app,
    );
    h.run_steps(4);
    let elements = crate::assistant::elements(h.state());
    let gate = elements.as_array().unwrap().iter().find(|e| e["id"] == "pulse.step.15.on").unwrap();
    assert!(gate["rect"][1].as_f64().unwrap() + gate["rect"][3].as_f64().unwrap() < 640.0, "{gate}");
    h.get_by_label("17–32").click();
    h.run_steps(3);
    assert_eq!(h.state().sequencer.page, 1);
    assert!(h.state().sequencer.draft.is_none());
    assert!(h.state().sequencer.running);
    assert!(Arc::ptr_eq(&before, &h.state().engine.session_arc()));
    h.state_mut().run("pulse.close", json!({})).unwrap();
    assert!(!h.state().sequencer.running);
    assert!(Arc::ptr_eq(&before, &h.state().engine.session_arc()));
}

#[test]
fn isolated_preview_disables_monitors_automation_and_song_transport_takes_over() {
    let mut app = app();
    let clip = app.sequencer.active.unwrap();
    app.run("track.new", json!({"kind":"audio","name":"Monitored mic"})).unwrap();
    app.run("workspace.instrumentTrack", json!({})).unwrap();
    for t in &mut app.engine.session_mut().tracks {
        t.mixer.input_monitor = true;
        t.mixer.record_arm = true;
    }
    let before = app.engine.session_arc();
    app.run("pulse.run", json!({"clip":clip.0})).unwrap();
    let audible = app.audible_session();
    assert!(audible.tracks.iter().all(|t| !t.mixer.input_monitor && !t.mixer.record_arm));
    app.write_automation();
    assert!(Arc::ptr_eq(&before, &app.engine.session_arc()));
    app.engine.transport_requests.push(soundcraft_engine::TransportRequest::PlayFrom(12345));
    app.handle_transport(0.0);
    assert!(!app.sequencer.running);
    assert!(app.is_playing());
    assert_eq!(app.engine.transport.position, 12345);
    app.stop_play();
    app.run("pulse.run", json!({"clip":clip.0})).unwrap();
    app.engine.transport_requests.push(soundcraft_engine::TransportRequest::HalfSpeed);
    app.handle_transport(0.0);
    assert!(!app.sequencer.running);
    app.stop_play();
    assert!(!app.is_playing());
}

#[test]
fn navigation_does_not_stop_preview_but_music_edits_do() {
    let mut app = app();
    app.run("pulse.run", json!({})).unwrap();
    app.engine.session_mut().edit.zoom.scroll = 1234;
    tick(&mut app);
    assert!(app.sequencer.running);
    app.engine.session_mut().tracks[0].mixer.volume_db = -12.0;
    tick(&mut app);
    assert!(!app.sequencer.running);
}
#[test]
fn invalid_open_preserves_the_working_preview_and_edit_view() {
    let mut app = app();
    app.run("pulse.run", json!({})).unwrap();
    let active = app.sequencer.active;
    app.sequencer.page = 1;
    app.sequencer.step = 17;
    for id in [json!(999999), json!("wrong type")] {
        assert!(app.run("pulse.open", json!({"clip":id})).is_err());
        assert_eq!(app.sequencer.active, active);
        assert_eq!(app.sequencer.page, 1);
        assert_eq!(app.sequencer.step, 17);
        assert!(app.sequencer.running);
    }
}
#[test]
fn edit_midi_keeps_independent_notes_and_undo_restores_program_metadata() {
    let mut app = app();
    let clip = app.sequencer.active.unwrap();
    app.run("pulse.steps", json!({"base_revision":app.engine.revision,"changes":[{"step":0,"pitch":12}]})).unwrap();
    assert!(app.run("pulse.detach", json!({})).is_err());
    assert!(app.sequencer.draft.is_some());
    app.run("pulse.discard", json!({})).unwrap();
    let saved = app.engine.session_arc();
    for bad in [json!("bad"), json!(-1), json!(1.5)] {
        assert!(app.run("pulse.detach", json!({"clip":bad})).is_err());
        assert!(Arc::ptr_eq(&saved, &app.engine.session_arc()));
        assert_eq!(app.sequencer.active, Some(clip));
    }
    app.run("midi.note_add", json!({"clip":clip.0,"pitch":73,"start_ticks":100})).unwrap();
    let before = app.engine.session().find_clip(clip).unwrap().1.clone();
    assert!(app.run("pulse.open", json!({"clip":clip.0})).is_err());
    app.run("pulse.detach", json!({"clip":clip.0})).unwrap();
    let c = app.engine.session().find_clip(clip).unwrap().1;
    assert!(c.step_sequencer.is_none());
    assert_eq!(c.content, before.content);
    assert!(app.ui.show_midi_editor);
    assert!(!app.sequencer.open);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(app.engine.session().find_clip(clip).unwrap().1, &before);
}
