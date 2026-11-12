use super::*;
use crate::Services;
use std::time::{Duration, Instant};

fn app() -> SoundApp {
    SoundApp::new(soundcraft_engine::Engine::default(), None, Services::default())
}

#[test]
fn walkthrough_advances_only_after_each_real_input_edit() {
    use egui_kittest::kittest::Queryable;
    let mut app = app();
    app.ui.ai.attach_frames = false;
    app.ui.show_ai_chat = true;
    let initial = app.engine.session().inputs.len();
    let before = app.engine.session_arc();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1500.0, 900.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            begin_frame(app, ui);
            egui::Panel::right("guide-chat").exact_size(420.0).show(ui, |ui| show(app, ui));
            crate::panels::input_setup::show(app, ui, Some(("Test interface", 4)));
            poll(app, ui.ctx());
            paint_guidance(app, ui.ctx());
        },
        app,
    );
    h.run_steps(4);
    let ctx = h.ctx.clone();
    *h.state_mut().assistant.panel.prompt_mut() = "Guide me through adding a mono input and a stereo input".into();
    let mut turn = 0;
    with_panel(h.state_mut(),|panel,host|panel.send_with(host,&ctx,move |_|{
        let current=turn;turn+=1;
        if current<2 {
            let (id,label)=if current==0 {("inputs.add.mono","Create a mono input")}else{("inputs.add.stereo","Create a stereo input")};
            Ok(json!({"status":"completed","output":[{"type":"function_call","call_id":format!("step{current}"),"name":"guide_ui","arguments":json!({"id":id,"label":label,"instruction":format!("Click the highlighted **{}** button.",if current==0 {"+ Mono input"} else {"+ Stereo input"}),"completion":"state","watch_id":"","state_pointer":"/inputCount","comparison":"greater","expected_json":(initial+current).to_string()}).to_string()}]}))
        } else {Ok(reply("Your mono and stereo inputs are ready."))}
    })).unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while command(h.state_mut(), "ai.status", &json!({})).unwrap().unwrap()["walkthrough"]["step"] != 1 && Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    h.run_steps(4);
    assert!(Arc::ptr_eq(&before, &h.state().engine.session_arc()), "the assistant must wait for the user's edit");
    h.query_by_role_and_label(egui::accesskit::Role::Button, "+ Mono input").unwrap().click();
    h.run_steps(3);
    while command(h.state_mut(), "ai.status", &json!({})).unwrap().unwrap()["walkthrough"]["step"] != 2 && Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    h.run_steps(4);
    let status = command(h.state_mut(), "ai.status", &json!({})).unwrap().unwrap();
    assert_eq!(status["highlights"][0]["id"], "inputs.add.stereo");
    assert_eq!(status["highlights"][0]["number"], 2);
    assert_eq!(h.state().engine.session().inputs.len(), initial + 1);
    if let Some(dir) = std::env::var_os("CRAFT_AI_EDITOR_FIXTURES") {
        std::fs::create_dir_all(&dir).unwrap();
        h.render().unwrap().save(std::path::Path::new(&dir).join("soundcraft-walkthrough.png")).unwrap();
    }
    h.query_by_role_and_label(egui::accesskit::Role::Button, "+ Stereo input").unwrap().click();
    h.run_steps(3);
    while h.state().assistant.panel.busy() && Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!h.state().assistant.panel.busy());
    assert_eq!(h.state().engine.session().inputs.len(), initial + 2);
    assert_eq!(command(h.state_mut(), "ai.status", &json!({})).unwrap().unwrap()["highlights"], json!([]));
    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(h.state().engine.session().inputs.len(), initial + 1, "user edits retain normal undo");
}

#[test]
fn how_to_guidance_points_at_the_real_input_button_without_creating_an_input() {
    let mut app = app();
    app.ui.ai.attach_frames = false;
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            begin_frame(app, ui);
            crate::panels::input_setup::show(app, ui, Some(("Test interface", 4)));
            poll(app, ui.ctx());
            paint_guidance(app, ui.ctx());
        },
        app,
    );
    h.run_steps(3);
    assert!(elements(h.state()).as_array().unwrap().iter().any(|e| e["id"] == "inputs.add.mono"));
    let before = h.state().engine.session_arc();
    *h.state_mut().assistant.panel.prompt_mut() = "How do I add a mono input?".into();
    let ctx = h.ctx.clone();
    let mut step = 0;
    with_panel(h.state_mut(), |panel, host| panel.send_with(host, &ctx, move |_| {
        step += 1;
        if step == 1 { Ok(json!({"status":"completed","output":[{"type":"function_call","call_id":"guide","name":"highlight_ui","arguments":json!({"targets":[{"id":"inputs.add.mono","label":"Click + Mono input"}]}).to_string()}]})) }
        else { Ok(reply("Click the highlighted + Mono input button.")) }
    })).unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while h.state().assistant.panel.busy() && Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!h.state().assistant.panel.busy());
    h.run_steps(3);
    assert!(Arc::ptr_eq(&before, &h.state().engine.session_arc()));
    let status = command(h.state_mut(), "ai.status", &json!({})).unwrap().unwrap();
    assert_eq!(status["highlights"][0]["id"], "inputs.add.mono");
    assert_eq!(status["highlights"][0]["visible"], true);
    if let Some(dir) = std::env::var_os("CRAFT_AI_EDITOR_FIXTURES") {
        std::fs::create_dir_all(&dir).unwrap();
        h.render().unwrap().save(std::path::Path::new(&dir).join("soundcraft-guidance.png")).unwrap();
    }
    command(h.state_mut(), "ai.clearHighlights", &json!({})).unwrap().unwrap();
    assert_eq!(command(h.state_mut(), "ai.status", &json!({})).unwrap().unwrap()["highlights"], json!([]));
}
fn finish(app: &mut SoundApp) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while app.assistant.panel.busy() && Instant::now() < deadline {
        poll(app, &egui::Context::default());
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(!app.assistant.panel.busy(), "mock assistant timed out");
}

#[test]
fn deleting_an_input_cannot_retarget_its_highlight_to_another_row() {
    let mut app = app();
    for name in ["Vocal", "Guitar"] {
        app.run("setup.inputs", json!({"action":"create","name":name,"format":"mono","channels":[0]})).unwrap();
    }
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            begin_frame(app, ui);
            crate::panels::input_setup::show(app, ui, Some(("Test interface", 4)));
        },
        app,
    );
    h.run_steps(3);
    let before = elements(h.state());
    let old_id = before.as_array().unwrap().iter().find(|e| e["label"] == "Input name: Vocal").unwrap()["id"].as_str().unwrap().to_owned();
    h.state_mut().run("setup.inputs", json!({"action":"delete","name":"Vocal"})).unwrap();
    h.run_steps(3);
    let after = elements(h.state());
    assert!(!after.as_array().unwrap().iter().any(|e| e["id"] == old_id));
    assert!(after.as_array().unwrap().iter().any(|e| e["label"] == "Input name: Guitar"));
}
fn reply(text: &str) -> Value {
    json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":text}]}]})
}

#[test]
fn assistant_maps_inputs_with_real_commands_and_undo_and_readable_activity() {
    let mut app = app();
    app.ui.ai.attach_frames = false;
    *app.assistant.panel.prompt_mut() = "Create a vocal input".into();
    let mut turn = 0;
    with_panel(&mut app, |panel,host| panel.send_with(host,&egui::Context::default(),move |body| {
        turn += 1;
        if turn == 1 {
            let params = json!({"command":"setup.inputs","params":{"action":"create","name":"Vocal","channels":[1]}});
            Ok(json!({"status":"completed","output":[{"type":"function_call","call_id":"map1","name":"soundcraft","arguments":json!({"method":"engine.execute","params_json":params.to_string()}).to_string()}]}))
        } else {
            assert!(body.to_string().contains("map1"));
            Ok(reply("**Vocal** is mapped to channel 2.\n\n- Choose it on a mono track."))
        }
    })).unwrap().unwrap();
    finish(&mut app);
    assert_eq!(app.engine.session().inputs.last().unwrap().name, "Vocal");
    let transcript = app.assistant.panel.transcript();
    assert!(transcript.iter().any(|(role, text)| role == "Tool" && text == "Audio Inputs — Done"));
    assert!(transcript.iter().all(|(_, text)| !text.contains("params_json")));
    assert!(transcript.iter().any(|(_, text)| text.contains("**Vocal**")));
    app.run("edit.undo", json!({})).unwrap();
    assert!(!app.engine.session().inputs.iter().any(|p| p.name == "Vocal"));
}

#[test]
fn new_session_cancels_a_response_before_it_can_edit_the_new_document() {
    let mut app = app();
    app.ui.ai.attach_frames = false;
    let (tx, rx) = mpsc::channel();
    *app.assistant.panel.prompt_mut() = "Edit the old session".into();
    with_panel(&mut app, |panel,host| panel.send_with(host,&egui::Context::default(),move |_| {
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        Ok(json!({"status":"completed","output":[{"type":"function_call","call_id":"old","name":"soundcraft","arguments":json!({"method":"engine.execute","params_json":"{\"command\":\"setup.inputs\",\"params\":{\"action\":\"create\",\"name\":\"Stale\"}}"}).to_string()}]}))
    })).unwrap().unwrap();
    app.run("session.new", json!({"name":"New"})).unwrap();
    tx.send(()).unwrap();
    finish(&mut app);
    assert!(!app.engine.session().inputs.iter().any(|p| p.name == "Stale"));
    assert_eq!(app.assistant.panel.context_items(), 0);
}

#[test]
fn preferences_persist_without_credentials_or_chat_and_tools_reject_self_invocation() {
    let mut app = app();
    app.assistant.panel.set_session_key("sk-test-only-private-key".into()).unwrap();
    *app.assistant.panel.prompt_mut() = "private draft".into();
    app.run("ai.preferences", json!({"key":"ai.model","value":"test-model"})).unwrap();
    app.run("ai.preferences", json!({"key":"ai.rememberKey","value":false})).unwrap();
    let saved = serde_json::to_string(&app.ui).unwrap();
    let loaded: crate::UiState = serde_json::from_str(&saved).unwrap();
    assert_eq!(loaded.ai.model, "test-model");
    assert!(!loaded.ai.remember_key);
    let inspected = app.inspect(&egui::Context::default()).to_string();
    for data in [saved, inspected, app.engine.session().to_json().unwrap()] {
        assert!(!data.contains("sk-test-only-private-key"));
        assert!(!data.contains("private draft"));
    }
    let mut host = Adapter { app: &mut app };
    for args in [
        json!({"method":"engine.execute","params_json":"{\"command\":\"ai.send\"}"}),
        json!({"method":"ui.set","params_json":"{\"ai\":{\"model\":\"bad\"}}"}),
        json!({"method":"ui.text","params_json":"[]"}),
    ] {
        assert!(host.start_tool(&ToolCall::new("soundcraft", args)).is_err());
    }
}

#[test]
fn attached_context_is_session_metadata_with_explicit_input_paths() {
    let mut app = app();
    info().validate().unwrap();
    let mut host = Adapter { app: &mut app };
    let mut pending = host.start_tool(&ToolCall::new("session_context", json!({}))).unwrap();
    let data = host.poll_tool(&mut pending).unwrap().unwrap();
    assert_eq!(data[0]["type"], "input_text");
    assert!(data[0]["text"].as_str().unwrap().contains("In 1-2"));
    assert!(!data.to_string().contains("input_audio"));
}

#[test]
fn window_menu_opens_a_docked_panel_and_layout_recall_keeps_ai_and_device_preferences() {
    let mut app = app();
    let saved = serde_json::to_value(&app.ui).unwrap();
    app.ui.ai.model = "test-model".into();
    app.ui.audio.input = Some("Test interface".into());
    crate::windows::recall_configuration(&mut app, &saved).unwrap();
    assert_eq!(app.ui.ai.model, "test-model");
    assert_eq!(app.ui.audio.input.as_deref(), Some("Test interface"));
    crate::menus::invoke_menu(&mut app, "ai.chat", "Window > AI Chat");
    assert!(app.ui.show_ai_chat);
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    harness.run_steps(4);
    assert!(harness.state().ui.docking.layout.contains(&crate::docking::Panel::AiChat));
    let elements = elements(harness.state());
    for id in ["ai.prompt", "ai.send", "ai.connection", "ai.settings"] {
        assert!(elements.as_array().unwrap().iter().any(|v| v["id"] == id), "missing {id}");
    }
    let prompt = elements.as_array().unwrap().iter().find(|v| v["id"] == "ai.prompt").unwrap();
    let rect = prompt["rect"].as_array().unwrap();
    assert!(rect[2].as_f64().unwrap() - rect[0].as_f64().unwrap() > 320.0, "chat must open wide enough for a readable composer");
    let ctx = harness.ctx.clone();
    let app = harness.state_mut();
    ctx.memory_mut(|m| m.request_focus(egui::Id::unique("ai.key.field")));
    assert!(protect_controls(app, "ui.text", &json!({"text":"unwanted"})).is_err());
    let rect = app.assistant.protected[0].unwrap();
    assert!(protect_controls(app, "ui.click", &json!({"x":rect.center().x,"y":rect.center().y})).is_err());
}

#[test]
#[ignore = "writes then deletes a dummy key in an isolated OS credential profile"]
fn native_vault_remembers_key_across_editor_restarts() {
    fn initialize(app: &mut SoundApp) {
        app.ui.show_ai_chat = true;
        let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).build_ui_state(
            |ui, app: &mut &mut SoundApp| {
                app.logic(&ui.ctx().clone());
                app.ui(ui);
            },
            app,
        );
        harness.run_steps(4);
    }
    fn idle(app: &mut SoundApp) -> Result<Value, String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            poll(app, &egui::Context::default());
            let status = command(app, "ai.status", &json!({})).ok_or("status unavailable")??;
            if status["credentialBusy"] == false {
                return Ok(status);
            }
            if Instant::now() > deadline {
                return Err("credential operation timed out".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let dir = std::env::temp_dir().join(format!("soundcraft-vault-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let profile = dir.join("ui.json");
    let mut first = app();
    first.ai_preferences_path = Some(profile.clone());
    initialize(&mut first);
    idle(&mut first).unwrap();
    first.assistant.panel.set_session_key("sk-soundcraft-dummy-vault-test".into()).unwrap();
    command(&mut first, "ai.saveKey", &json!({})).unwrap().unwrap();
    let saved = idle(&mut first).unwrap();
    drop(first);
    let mut second = app();
    second.ai_preferences_path = Some(profile);
    initialize(&mut second);
    let restored = idle(&mut second).unwrap();
    let private_key_restored = second.assistant.panel.has_key();
    command(&mut second, "ai.forgetKey", &json!({})).unwrap().unwrap();
    let forgotten = idle(&mut second).unwrap();
    assert_eq!(saved["savedKey"], true, "{saved}");
    assert!(private_key_restored, "{restored}");
    assert_eq!(forgotten["savedKey"], false, "{forgotten}");
    assert!(!second.assistant.panel.has_key());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore = "renders the actual docked Markdown/tool UI and input setup using a GPU"]
fn ecosystem_panel_visual_check() {
    use egui_kittest::kittest::Queryable;
    let mut app = app();
    app.ui.ai.attach_frames = false;
    app.ui.show_ai_chat = true;
    *app.assistant.panel.prompt_mut() = "Set up my vocal recording".into();
    app.assistant.panel.set_session_key("sk-visual-test-dummy-key".into()).unwrap();
    let mut visual_turn = 0;
    with_panel(&mut app,|panel,host| panel.send_with(host,&egui::Context::default(), move |_| {
        visual_turn+=1;
        if visual_turn==1 {
            return Ok(json!({"status":"completed","output":[{"type":"function_call","call_id":"visual-map","name":"soundcraft","arguments":json!({"method":"engine.execute","params_json":json!({"command":"setup.inputs","params":{"action":"create","name":"Vocal","channels":[0]}}).to_string()}).to_string()}]}));
        }
        Ok(reply("## Ready to record\n\nChoose **Vocal** on a mono track, then arm it.\n\n- Channel 1: microphone\n- Channel 2: guitar\n\n| Input | Port |\n|---|---|\n| Vocal | 1 |\n| Guitar | 2 |\n\n`setup.inputs` keeps mappings in your session."))
    })).unwrap().unwrap();
    finish(&mut app);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/visual-check");
    std::fs::create_dir_all(&dir).unwrap();
    let mut harness = egui_kittest::Harness::builder().with_size(egui::vec2(1600.0, 1000.0)).with_max_steps(500).wgpu().build_ui_state(
        |ui, app: &mut SoundApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    harness.run_steps(6);
    assert!(harness.query_by_label("AI Chat").is_some());
    harness.render().unwrap().save(dir.join("ai-chat.png")).unwrap();
    harness.state_mut().ui.show_io_setup = true;
    harness.run_steps(4);
    let first = harness.get_by_label("Input name: In 1").rect();
    let second = harness.get_by_label("Input name: In 2").rect();
    assert!(second.min.y > first.max.y, "each input path must occupy a separate grid row");
    assert!(first.width() >= 170.0, "named inputs need enough room for useful names");
    harness.render().unwrap().save(dir.join("input-setup.png")).unwrap();
    harness.state_mut().ui.show_io_setup = false;
    harness.state_mut().ui.show_preferences = true;
    harness.state_mut().ui.preferences_page = crate::preferences::PreferencesPage::AiChat;
    harness.run_steps(4);
    harness.render().unwrap().save(dir.join("ai-preferences.png")).unwrap();
}
