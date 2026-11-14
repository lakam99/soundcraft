use super::{SoundApp, Value, bounce, menus};

impl SoundApp {
    /// Run a command; errors go to the status line. Returns the result.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, String> {
        if let Some(result) = crate::effect_style::command(self, id, &params) {
            return result;
        }
        if self.live_midi.recording()
            && matches!(
                id,
                "midi.input"
                    | "mix.instrument"
                    | "edit.undo"
                    | "edit.redo"
                    | "session.new"
                    | "session.open"
                    | "session.close"
                    | "session.revert"
                    | "transport.locate"
                    | "transport.rtz"
                    | "transport.play_from"
            )
        {
            return Err("Stop MIDI recording before changing routing, instruments, projects, position or undo history".into());
        }
        if let Some(result) = crate::live_midi::command(self, id, &params) {
            return result;
        }
        if matches!(id, "session.new" | "session.open" | "session.close" | "session.revert") && bounce::refuses_session_switch(self) {
            return Err(self.ui.status.clone());
        }
        if let Some(r) = crate::assistant::command(self, id, &params) {
            return r;
        }
        if let Some(r) = menus::run_ui_command(self, id, &params) {
            return r;
        }
        if id.starts_with("session.save") || id.starts_with("file.bounce") || id == "track.save_preset" {
            // Third-party plugins keep state the session can't see: store it first.
            self.capture_plugin_states();
        }
        let replacing = matches!(id, "session.new" | "session.open" | "session.close" | "session.revert");
        match self.engine.execute(id, &params) {
            Ok(v) => {
                if replacing {
                    // Note indices and gestures belong to the old session's clips (#443).
                    self.document_replaced();
                }
                if replacing || (id.starts_with("session.save") && !self.engine.is_dirty()) {
                    self.release_autosave(false);
                }
                if id == "app.quit" {
                    self.quit_requested = true;
                }
                if id == "options.scrolling" {
                    // Re-selecting a scrolling mode resumes the playhead follow.
                    self.edit_layout.follow_hold = false;
                }
                if id == "view.zoom_toggle_cancel" && v["canceled"] == true {
                    self.edit_layout.forget_zoom_back();
                }
                if id == "track.clear_clip_indicators" {
                    self.meters.values_mut().chain([&mut self.main_meter]).for_each(|m| m.clip = false);
                }
                Ok(v)
            }
            Err(e) => {
                self.ui.status = e.to_string();
                log::warn!("{id}: {e}");
                Err(e.to_string())
            }
        }
    }
}
