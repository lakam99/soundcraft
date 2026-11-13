//! A task-oriented workspace. All actions use the same editor dispatcher as menus and automation.
use crate::{MainWindow, SoundApp, UiState, docking};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use soundcraft_model::{ClipId, TrackId, TrackKind};

mod browser;
mod chrome;
mod inspector;

pub use browser::show as browser;
pub use chrome::empty;
pub use chrome::show;
pub use inspector::show as inspector;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutSnapshot {
    docking: docking::Workspace,
    tracks: bool,
    clips: bool,
    midi: bool,
    mix: bool,
    inspector: bool,
    browser: bool,
}

impl LayoutSnapshot {
    fn capture(ui: &UiState) -> Self {
        Self {
            docking: ui.docking.clone(),
            tracks: ui.show_tracks_list,
            clips: ui.show_clip_list,
            midi: ui.show_midi_editor,
            mix: ui.show_mix,
            inspector: ui.show_inspector,
            browser: ui.show_browser,
        }
    }
    fn restore(self, ui: &mut UiState) {
        ui.docking = self.docking;
        ui.show_tracks_list = self.tracks;
        ui.show_clip_list = self.clips;
        ui.show_midi_editor = self.midi;
        ui.show_mix = self.mix;
        ui.show_inspector = self.inspector;
        ui.show_browser = self.browser;
    }
}

impl UiState {
    /// Construct the full specialist interface for hosts that explicitly choose Classic.
    pub fn classic() -> Self {
        Self {
            modern_workspace: false,
            show_inspector: false,
            show_browser: false,
            show_tracks_list: true,
            show_clip_list: true,
            docking: docking::classic_default(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BrowserTab {
    #[default]
    Media,
    Instruments,
    Effects,
}

#[derive(Default)]
pub struct Runtime {
    pub browser_tab: BrowserTab,
    pub search: String,
}

impl SoundApp {
    /// An explicit Classic host, with the same editing engine and saved-panel support.
    pub fn new_classic(engine: soundcraft_engine::Engine, player: Option<soundcraft_playback::Player>, services: crate::Services) -> Self {
        let mut app = Self::new(engine, player, services);
        app.ui = UiState::classic();
        app
    }
}

/// Preserve old custom layouts as Classic while introducing the modern workspace once.
pub fn migrate_legacy(saved: &mut Value) -> Result<(), String> {
    if saved.get("modern_workspace").is_some() {
        return Ok(());
    }
    let mut state: UiState = serde_json::from_value(saved.clone()).map_err(|error| error.to_string())?;
    state.modern_workspace = false;
    state.show_inspector = false;
    state.show_browser = false;
    if saved.get("docking").is_none() {
        state.docking = docking::classic_default();
    }
    if saved.get("show_tracks_list").is_none() {
        state.show_tracks_list = true;
    }
    if saved.get("show_clip_list").is_none() {
        state.show_clip_list = true;
    }
    state.classic_layout = Some(LayoutSnapshot::capture(&state));
    let defaults = UiState::default();
    LayoutSnapshot::capture(&defaults).restore(&mut state);
    state.modern_workspace = true;
    state.window = MainWindow::Edit;
    *saved = serde_json::to_value(state).map_err(|error| error.to_string())?;
    Ok(())
}

fn switch(app: &mut SoundApp, modern: bool) -> Result<Value, String> {
    if app.ui.modern_workspace != modern {
        let current = LayoutSnapshot::capture(&app.ui);
        let saved = if modern { app.ui.modern_layout.clone() } else { app.ui.classic_layout.clone() };
        let next = saved.unwrap_or_else(|| LayoutSnapshot::capture(&if modern { UiState::default() } else { UiState::classic() }));
        docking::validate(&next.docking)?;
        if app.ui.modern_workspace {
            app.ui.modern_layout = Some(current);
        } else {
            app.ui.classic_layout = Some(current);
        }
        let mut state = app.ui.clone();
        next.restore(&mut state);
        state.modern_workspace = modern;
        state.window = MainWindow::Edit;
        docking::cancel_replaced_state(app, &state);
        app.ui = state;
    }
    Ok(json!({"workspace":if modern {"modern"} else {"classic"}}))
}

pub fn selected(app: &SoundApp) -> Option<TrackId> {
    app.engine.session().edit.selected_tracks.last().copied()
}

fn activate(app: &mut SoundApp, panel: &str) -> Result<Value, String> {
    app.ui.window = MainWindow::Edit;
    docking::command(app, "ui.dock", &json!({"operation":"activate","panel":panel}))
}

/// High-level workflow actions stay discoverable through the existing command registry.
pub fn command(app: &mut SoundApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    let toggle = |current: bool| params.get("value").and_then(Value::as_bool).unwrap_or(!current);
    Some(match id {
        "workspace.modern" => switch(app, true),
        "workspace.classic" => switch(app, false),
        "workspace.inspector" => {
            app.ui.show_inspector = toggle(app.ui.show_inspector);
            if app.ui.show_inspector { activate(app, "inspector") } else { Ok(json!({"value":false})) }
        }
        "workspace.browser" => {
            app.ui.show_browser = toggle(app.ui.show_browser);
            if app.ui.show_browser { activate(app, "browser") } else { Ok(json!({"value":false})) }
        }
        "workspace.audioTrack" => {
            let format = params.get("format").and_then(Value::as_str).unwrap_or("mono");
            if !matches!(format, "mono" | "stereo") {
                Err("Audio track format must be mono or stereo".into())
            } else {
                app.run("track.new", json!({"kind":"audio","format":format,"name":"Audio"}))
            }
        }
        "workspace.startMidi" => app.run("workspace.instrumentTrack", json!({})).and_then(|_| new_part(app)),
        "workspace.instrumentTrack" => {
            let plugin = params.get("plugin").and_then(Value::as_str).unwrap_or("subtractive_synth");
            let name = crate::mix_window::plugin_info(plugin).filter(|info| info.is_instrument).map(|info| info.name);
            match name {
                Some(name) => app.run("track.new", json!({"kind":"instrument","format":"stereo","name":name,"instrument":plugin})),
                None => Err("Choose an available instrument from the browser".into()),
            }
        }
        "workspace.midiPart" => new_part(app),
        "workspace.audioSetup" => app.run("window.playback_engine", json!({"value":true})),
        _ => return None,
    })
}

fn new_part(app: &mut SoundApp) -> Result<Value, String> {
    let track = selected(app)
        .filter(|id| app.engine.session().track(*id).is_some_and(|track| matches!(track.kind, TrackKind::Midi | TrackKind::Instrument)))
        .ok_or("Select an instrument or MIDI track to write notes")?;
    let result = app.run("midi.new_clip", json!({"track":track.0}))?;
    if let Some(id) = result.get("clip").and_then(Value::as_u64) {
        app.engine.session_mut().edit.selected_clips = vec![ClipId(id)];
    }
    app.ui.show_midi_editor = true;
    app.run("edit.tool", json!({"tool":"pencil"}))?;
    activate(app, "midi")?;
    Ok(result)
}

pub(crate) fn run(app: &mut SoundApp, id: &str, params: Value) {
    if let Err(error) = app.run(id, params) {
        app.ui.status = error;
    }
}

pub(crate) fn button(app: &mut SoundApp, ui: &mut egui::Ui, id: &str, label: &str, command: &str, params: Value) -> egui::Response {
    let response = ui.add_sized([ui.available_width().min(220.0), 30.0], egui::Button::new(label));
    crate::assistant::register(app, id, &response, label);
    if response.clicked() {
        run(app, command, params);
    }
    response
}
