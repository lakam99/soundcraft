//! MIDI device lifecycle, live activity and checkpointed musical takes.
use crate::SoundApp;
use serde_json::{Value, json};
use soundcraft_model::{ClipId, TrackId};
use soundcraft_playback::midi_input::{InputRoute, MidiBus, MidiInputs, Status};
use std::collections::BTreeMap;
use std::sync::{Arc, atomic::Ordering};

pub struct Take {
    track: TrackId,
    clip: Option<ClipId>,
    start: i64,
    end: Option<i64>,
    capture: soundcraft_midi::live::Capture,
}
pub struct Runtime {
    pub bus: Arc<MidiBus>,
    pub status: Status,
    pub hardware_enabled: bool,
    inputs: Option<MidiInputs>,
    pub attached: bool,
    routes: Vec<InputRoute>,
    held: BTreeMap<(TrackId, u8, u8), u8>,
    sustain: BTreeMap<(TrackId, u8), u8>,
    epoch: u64,
    takes: Vec<Take>,
    last_checkpoint: f64,
    last_status: f64,
    lost: u64,
    received: u64,
    pub activity_at: f64,
    finishing: bool,
    take_error: Option<String>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            bus: Arc::new(MidiBus::default()),
            status: Status::default(),
            hardware_enabled: false,
            inputs: None,
            attached: false,
            routes: Vec::new(),
            held: BTreeMap::new(),
            sustain: BTreeMap::new(),
            epoch: 0,
            takes: Vec::new(),
            last_checkpoint: 0.0,
            last_status: -1.0,
            lost: 0,
            received: 0,
            activity_at: -10.0,
            finishing: false,
            take_error: None,
        }
    }
}
impl Runtime {
    pub(crate) fn held_notes(&self, track: TrackId) -> Vec<u8> {
        let mut keys: Vec<u8> = self.held.keys().filter(|(t, _, _)| *t == track).map(|(_, _, n)| *n).collect();
        keys.sort();
        keys.dedup();
        keys
    }
    pub(crate) fn missing_input(&self, session: &soundcraft_model::Session) -> Option<String> {
        session
            .tracks
            .iter()
            .filter(|t| t.kind.is_midi() && t.mixer.record_arm)
            .find(|track| !self.status.devices.iter().any(|d| d.connected && track.midi_input.device.as_ref().is_none_or(|id| id == &d.id)))
            .map(|track| track.name.clone())
    }
    pub fn recording(&self) -> bool {
        !self.takes.is_empty()
    }
    pub fn monitored(&self) -> bool {
        self.bus.active.load(Ordering::Acquire)
    }
    pub fn reset(&mut self) {
        self.bus.panic();
        self.held.clear();
        self.sustain.clear();
    }
}

impl SoundApp {
    pub fn enable_midi_input(&mut self, enabled: bool) {
        self.live_midi.hardware_enabled = enabled;
        if !enabled {
            self.live_midi.inputs = None;
            self.live_midi.bus.active.store(false, Ordering::Release);
            self.live_midi.reset();
        }
    }

    fn ensure_midi(&mut self) {
        if self.live_midi.hardware_enabled && self.live_midi.inputs.is_none() {
            match MidiInputs::new(Arc::clone(&self.live_midi.bus)) {
                Ok(inputs) => self.live_midi.inputs = Some(inputs),
                Err(error) => self.live_midi.status.error = Some(error),
            }
        }
        if !self.live_midi.attached
            && let Some(player) = &self.player
        {
            player.set_midi_input(Arc::clone(&self.live_midi.bus));
            self.live_midi.attached = true;
        }
    }

    pub(crate) fn poll_live_midi(&mut self) {
        let epoch = self.live_midi.bus.generation();
        if epoch != self.live_midi.epoch {
            self.live_midi.epoch = epoch;
            self.close_midi_keys(None);
        }
        let selected = self.engine.session().edit.selected_tracks.last().copied();
        let routes: Vec<_> = self
            .engine
            .session()
            .tracks
            .iter()
            .filter(|track| {
                !self.sequencer.running
                    && track.kind.is_midi()
                    && !track.inactive
                    && track.instrument.is_some()
                    && (track.mixer.record_arm || (track.mixer.input_monitor && Some(track.id) == selected))
            })
            .take(128)
            .map(|track| InputRoute {
                track: track.id,
                device: track.midi_input.device.clone(),
                channel: track.midi_input.channel,
                armed: track.mixer.record_arm,
                instrument: track.instrument.as_ref().map_or_else(String::new, |instrument| instrument.plugin.clone()),
            })
            .collect();
        if self.live_midi.hardware_enabled && self.engine.session().tracks.iter().any(|track| track.kind.is_midi()) {
            self.ensure_midi();
        }
        if routes != self.live_midi.routes {
            let changed: Vec<_> = self.live_midi.routes.iter().filter(|old| !routes.contains(old)).map(|route| route.track).collect();
            for track in changed {
                self.live_midi.bus.panic_track(track);
                self.close_midi_keys(Some(track));
            }
            if let Some(inputs) = &self.live_midi.inputs {
                inputs.set_routes(routes.clone());
            }
            self.live_midi.routes = routes;
        }
        if self.player.is_none() {
            self.live_midi.bus.position.store(self.engine.transport.position, Ordering::Release);
        }
        for _ in 0..soundcraft_playback::midi_input::MIDI_QUEUE_CAPACITY {
            let Some(packet) = self.live_midi.bus.pop_capture() else {
                break;
            };
            if !self.live_midi.bus.current(&packet) {
                continue;
            }
            let [status, key, velocity] = packet.message;
            if status == 0xff {
                self.close_midi_keys(Some(packet.track));
                continue;
            }
            let held_key = (packet.track, status & 15, key);
            match status & 0xf0 {
                0x90 => {
                    self.live_midi.held.insert(held_key, velocity);
                }
                0x80 => {
                    self.live_midi.held.remove(&held_key);
                }
                _ => {}
            }
            if status & 0xf0 == 0xb0 {
                if key == 64 {
                    self.live_midi.sustain.insert((packet.track, status & 15), velocity);
                } else if matches!(key, 120 | 121 | 123) {
                    self.live_midi.sustain.remove(&(packet.track, status & 15));
                    if key != 121 {
                        self.live_midi.held.retain(|(track, channel, _), _| *track != packet.track || *channel != status & 15);
                    }
                }
            }
            if packet.recording {
                if self.live_midi.finishing {
                    self.engine.transport.position = self.engine.transport.position.max(packet.sample);
                }
                let session = self.engine.session_arc();
                for take in self.live_midi.takes.iter_mut().filter(|take| take.track == packet.track) {
                    let tick = session
                        .tempo
                        .samples_to_ticks(packet.sample, session.sample_rate)
                        .saturating_sub(session.tempo.samples_to_ticks(take.start, session.sample_rate));
                    if let Err(error) = take.capture.push(tick, packet.message) {
                        self.ui.status = error.into();
                        self.live_midi.take_error = Some(error.into());
                        self.engine.transport_requests.push(soundcraft_engine::TransportRequest::Stop);
                    }
                }
            }
        }
        let received = self.live_midi.bus.received.load(Ordering::Relaxed);
        if received != self.live_midi.received {
            self.live_midi.received = received;
            self.live_midi.activity_at = self.uptime;
        }
        let lost = self.live_midi.bus.lost.load(Ordering::Acquire);
        if lost != self.live_midi.lost {
            self.live_midi.lost = lost;
            self.close_midi_keys(None);
            if self.live_midi.recording() && !self.live_midi.finishing {
                self.stop_play();
            }
            self.ui.status = "MIDI input overflow: notes were silenced and the take kept. Reconnect the keyboard before continuing.".into();
        }
        if self.uptime - self.live_midi.last_status >= 0.25 {
            self.live_midi.last_status = self.uptime;
            if let Some(inputs) = &self.live_midi.inputs {
                self.live_midi.status = inputs.status();
            }
            if self.live_midi.recording()
                && !self.live_midi.finishing
                && self.live_midi.status.error.as_ref().is_some_and(|error| error.contains("disconnected") || error.contains("worker failed"))
            {
                self.stop_play();
                self.ui.status = self.live_midi.status.error.clone().unwrap_or_default();
            }
        }
        if self.live_midi.recording() && !self.live_midi.finishing && self.uptime - self.live_midi.last_checkpoint >= 0.2 {
            self.checkpoint_midi(false);
        }
        if self.player.is_none() {
            self.live_midi.bus.clear_audio();
        }
    }

    pub(crate) fn begin_midi_recording(&mut self, retrigger: bool) {
        self.live_midi.take_error = None;
        let start = self.punch.map_or(self.record_start, |range| range.start);
        let end = self.punch.map(|range| range.end);
        self.live_midi.takes = self
            .engine
            .session()
            .tracks
            .iter()
            .filter(|track| track.kind.is_midi() && track.mixer.record_arm)
            .take(128)
            .map(|track| Take { track: track.id, clip: None, start, end, capture: soundcraft_midi::live::Capture::default() })
            .collect();
        if !self.live_midi.recording() {
            return;
        }
        self.engine.end_merge();
        self.engine.hold_merge(Some("midi-recording"));
        self.live_midi.bus.clock(self.record_start, self.engine.session().sample_rate.hz(), false);
        for (&(track, channel), &value) in &self.live_midi.sustain {
            if value >= 64 && self.live_midi.takes.iter().any(|take| take.track == track) {
                if retrigger {
                    self.live_midi.bus.send(track, [0xb0 | channel, 64, value], false);
                }
                for take in self.live_midi.takes.iter_mut().filter(|take| take.track == track) {
                    let _ = take.capture.push(0, [0xb0 | channel, 64, value]);
                }
            }
        }
        // Held keys at Record become notes at the take boundary, and are rearticulated after Play resets the instrument.
        for (&(track, channel, key), &velocity) in &self.live_midi.held {
            if retrigger && self.live_midi.takes.iter().any(|take| take.track == track) {
                self.live_midi.bus.send(track, [0x90 | channel, key, velocity], false);
            }
        }
        self.live_midi.bus.recording.store(true, Ordering::Release);
        let session = self.engine.session();
        for (&(track, channel, key), &velocity) in &self.live_midi.held {
            for take in self.live_midi.takes.iter_mut().filter(|take| take.track == track) {
                let tick = session
                    .tempo
                    .samples_to_ticks(self.record_start, session.sample_rate)
                    .saturating_sub(session.tempo.samples_to_ticks(take.start, session.sample_rate));
                let _ = take.capture.push(tick, [0x90 | channel, key, velocity]);
            }
        }
        self.checkpoint_midi(false);
    }

    fn close_midi_keys(&mut self, track: Option<TrackId>) {
        let session = self.engine.session();
        let at = self.engine.transport.position;
        for take in self.live_midi.takes.iter_mut().filter(|take| track.is_none_or(|id| take.track == id)) {
            let tick = session
                .tempo
                .samples_to_ticks(at, session.sample_rate)
                .saturating_sub(session.tempo.samples_to_ticks(take.start, session.sample_rate));
            take.capture.close_all(tick);
            for channel in 0..16 {
                let _ = take.capture.push(tick, [0xb0 | channel, 64, 0]);
            }
        }
        self.live_midi.held.retain(|(id, _, _), _| track.is_some_and(|track| *id != track));
        self.live_midi.sustain.retain(|(id, _), _| track.is_some_and(|track| *id != track));
    }

    fn checkpoint_midi(&mut self, finish: bool) {
        self.live_midi.last_checkpoint = self.uptime;
        let at = self.engine.transport.position;
        let session = self.engine.session_arc();
        for take in &mut self.live_midi.takes {
            let base = session.tempo.samples_to_ticks(take.start, session.sample_rate);
            let tick = session.tempo.samples_to_ticks(at, session.sample_rate).saturating_sub(base);
            let limit = take.end.map(|end| session.tempo.samples_to_ticks(end, session.sample_rate).saturating_sub(base));
            let mut sequence = take.capture.snapshot(tick, limit);
            if finish {
                for channel in 0..16 {
                    sequence.ctrls.push(soundcraft_midi::CtrlEvent {
                        tick: limit.unwrap_or(tick).min(tick).saturating_sub(1).max(0),
                        channel,
                        kind: soundcraft_midi::CtrlKind::Cc(64),
                        value: 0,
                    });
                }
            }
            let mut params = json!({"track":take.track.0,"start":take.start,"length":take.end.unwrap_or(at).min(at).saturating_sub(take.start).max(1),"sequence":sequence});
            if let Some(id) = take.clip {
                params["clip"] = json!(id.0);
            }
            match self.engine.execute_merged("midi.record_take", &params, "midi-recording") {
                Ok(value) => take.clip = value.get("clip").and_then(Value::as_u64).map(ClipId),
                Err(error) => {
                    let message = format!("MIDI take checkpoint failed: {error}. Previously saved notes are kept.");
                    self.live_midi.take_error = Some(message.clone());
                    self.ui.status = message;
                    if !self.live_midi.finishing {
                        self.engine.transport_requests.push(soundcraft_engine::TransportRequest::Stop);
                    }
                }
            }
        }
    }

    pub(crate) fn finish_midi_recording(&mut self) {
        if !self.live_midi.recording() || self.live_midi.finishing {
            return;
        }
        self.live_midi.finishing = true;
        self.live_midi.bus.recording.store(false, Ordering::Release);
        // Drain messages admitted before Stop before closing all held keys.
        self.poll_live_midi();
        self.close_midi_keys(None);
        self.checkpoint_midi(true);
        let count = self.live_midi.takes.len();
        self.live_midi.takes.clear();
        self.live_midi.reset();
        self.live_midi.finishing = false;
        if let Some(error) = &self.live_midi.take_error {
            self.ui.status = error.clone();
        } else if count > 0 {
            self.ui.status = format!("Recorded {count} MIDI take(s). Play to listen; Undo removes the take.");
        }
    }
}

pub(crate) fn command(app: &mut SoundApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    Some(match id {
        "midi.devices" => {
            app.ensure_midi();
            Ok(json!(app.live_midi.status))
        }
        "midi.refresh" => {
            if app.live_midi.status.error.as_ref().is_some_and(|error| error.contains("worker failed")) {
                app.live_midi.inputs = None;
                app.live_midi.routes.clear();
            }
            app.ensure_midi();
            if let Some(inputs) = &app.live_midi.inputs {
                inputs.refresh();
                Ok(json!({"refreshing":true}))
            } else {
                Err("MIDI device input is disabled in this host".into())
            }
        }
        "midi.panic" => {
            app.live_midi.reset();
            app.close_midi_keys(None);
            Ok(json!({"silenced":true}))
        }
        "midi.send" => {
            let track =
                params.get("track").and_then(Value::as_u64).map(TrackId).or_else(|| app.engine.session().edit.selected_tracks.last().copied());
            let Some(track) = track.filter(|track| app.engine.session().track(*track).is_some_and(|track| track.kind.is_midi())) else {
                return Some(Err("Select a MIDI or instrument track".into()));
            };
            let message = params
                .get("message")
                .and_then(Value::as_array)
                .filter(|bytes| (2..=3).contains(&bytes.len()))
                .and_then(|bytes| bytes.iter().map(|value| value.as_u64().and_then(|value| u8::try_from(value).ok())).collect::<Option<Vec<_>>>())
                .and_then(|bytes| soundcraft_midi::live::channel_message(&bytes));
            match message {
                Some(message) => {
                    app.live_midi.bus.send(track, message, true);
                    Ok(json!({"sent":true}))
                }
                None => Err("Expected a valid 2- or 3-byte MIDI channel message".into()),
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn sequencer_latches_only_its_target_chord_and_keeps_memory_after_release() {
        let mut app = crate::SoundApp::new_classic(soundcraft_engine::Engine::default(), None, crate::Services::default());
        app.run("pulse.new", serde_json::json!({})).unwrap();
        let track = app.engine.session().tracks[0].id;
        for note in [60, 63, 67] {
            app.live_midi.held.insert((track, 0, note), 100);
        }
        app.live_midi.held.insert((soundcraft_model::TrackId(999), 0, 80), 100);
        let before = app.engine.session_arc();
        app.run("pulse.latch", serde_json::json!({})).unwrap();
        let state = app.run("pulse.inspect", serde_json::json!({})).unwrap();
        assert_eq!(state["program"]["banks"][0]["lanes"][0]["chord"], serde_json::json!([0, 3, 7]));
        app.live_midi.held.clear();
        app.run("pulse.run", serde_json::json!({})).unwrap();
        let clip = app.engine.session().edit.selected_clips[0];
        let audible = app.audible_session();
        let soundcraft_model::ClipContent::Midi { sequence } = &audible.find_clip(clip).unwrap().1.content else { panic!("Expected MIDI") };
        assert_eq!(sequence.notes.iter().take(3).map(|n| n.pitch).collect::<Vec<_>>(), [60, 63, 67]);
        assert!(std::sync::Arc::ptr_eq(&before, &app.engine.session_arc()));
    }
    use super::*;
    fn app() -> SoundApp {
        let mut app = SoundApp::new(soundcraft_engine::Engine::default(), None, crate::Services::default());
        app.run("track.new", json!({"kind":"instrument","name":"Keys","instrument":"subtractive_synth"})).unwrap();
        app.run("mix.record_arm", json!({"track":"Keys","value":true})).unwrap();
        app
    }
    fn send(app: &mut SoundApp, sample: i64, message: [u8; 3]) {
        let track = app.engine.session().tracks[0].id;
        app.live_midi.bus.position.store(sample, Ordering::Release);
        app.live_midi.bus.send(track, message, true);
        app.engine.transport.position = sample;
        app.poll_live_midi();
    }
    #[test]
    fn midi_only_take_grows_checkpoints_without_microphone_and_is_one_undo() {
        let _palette = crate::theme::lock_test_palette();
        let mut app = app();
        app.run("midi.input", json!({"track":"Keys","device":"saved-keyboard","device_name":"Test keyboard","channel":3})).unwrap();
        app.run("transport.record", json!({})).unwrap();
        app.handle_transport(0.0);
        assert!(app.live_midi.recording());
        assert!(app.recorder.is_none());
        assert!(!app.recorder_failed);
        send(&mut app, 0, [0x92, 60, 107]);
        send(&mut app, 12000, [0xb2, 64, 127]);
        app.uptime = 0.3;
        send(&mut app, 24000, [0x82, 60, 43]);
        assert_eq!(app.engine.session().tracks[0].clips().len(), 1);
        assert!(app.run("track.delete", json!({"track":"Keys"})).is_err());
        // Ordinary document/runtime changes and focus changes cannot split a checkpointed take.
        app.run("track.rename", json!({"track":"Keys","name":"Piano"})).unwrap();
        app.run("mix.input_monitor", json!({"track":"Piano","value":true})).unwrap();
        assert!(app.run("mix.record_arm", json!({"track":"Piano","value":false})).is_err());
        app.engine.end_merge();
        send(&mut app, 30000, [0x92, 64, 90]);
        app.engine.transport.position = 48000;
        app.run("transport.stop", json!({})).unwrap();
        app.handle_transport(0.0);
        assert!(!app.live_midi.recording());
        let clip = &app.engine.session().tracks[0].clips()[0];
        let soundcraft_model::ClipContent::Midi { sequence } = &clip.content else { panic!("MIDI take") };
        assert_eq!(sequence.notes.len(), 2);
        assert_eq!(sequence.notes[0].release_velocity, 43);
        assert_eq!(sequence.notes[0].channel, 2);
        assert!(sequence.notes.iter().all(|n| n.length > 0));
        assert!(sequence.ctrls.iter().any(|c| c.kind == soundcraft_midi::CtrlKind::Cc(64) && c.value == 0));
        let restored: soundcraft_model::Session = serde_json::from_value(json!(app.engine.session())).unwrap();
        assert_eq!(restored.tracks[0].midi_input.device.as_deref(), Some("saved-keyboard"));
        assert_eq!(restored.tracks[0].clips().len(), 1);
        assert!(app.engine.undo());
        assert!(app.engine.session().tracks[0].clips().is_empty());
        assert!(app.engine.redo());
        assert_eq!(app.engine.session().tracks[0].clips().len(), 1);
    }
    #[test]
    fn monitoring_and_silencing_create_no_clips_and_loop_record_refuses_before_mutation() {
        let _palette = crate::theme::lock_test_palette();
        let mut app = app();
        send(&mut app, 0, [0x90, 60, 100]);
        send(&mut app, 100, [0x80, 60, 0]);
        assert!(app.engine.session().tracks[0].clips().is_empty());
        app.run("midi.panic", json!({})).unwrap();
        assert!(app.live_midi.held.is_empty());
        app.engine.session_mut().edit.loop_record = true;
        let before = app.engine.undo_history();
        app.run("transport.record", json!({})).unwrap();
        app.handle_transport(0.0);
        assert!(!app.engine.transport.recording);
        assert!(!app.live_midi.recording());
        assert!(app.recorder.is_none());
        assert_eq!(app.engine.undo_history(), before);
        assert!(app.ui.status.contains("Loop"));
    }
    #[test]
    fn missing_saved_device_is_not_replaced_by_another_connected_keyboard() {
        let _palette = crate::theme::lock_test_palette();
        let mut app = app();
        app.run("midi.input", json!({"track":"Keys","device":"missing-device"})).unwrap();
        app.live_midi.status.devices.push(soundcraft_playback::midi_input::Device {
            id: "other-device".into(),
            name: "Other".into(),
            connected: true,
            error: None,
        });
        assert_eq!(app.live_midi.missing_input(app.engine.session()).as_deref(), Some("Keys"));
    }
    #[test]
    fn disconnect_and_panic_forget_held_keys_and_sustain_before_the_next_take() {
        let _palette = crate::theme::lock_test_palette();
        let mut app = app();
        send(&mut app, 0, [0x90, 60, 100]);
        send(&mut app, 0, [0xb0, 64, 127]);
        send(&mut app, 0, [0xb0, 123, 0]);
        assert!(app.live_midi.held.is_empty(), "All Notes Off must not seed phantom notes at Record");
        send(&mut app, 0, [0x90, 60, 100]);
        send(&mut app, 0, [0xb0, 64, 127]);
        assert!(!app.live_midi.held.is_empty());
        assert!(!app.live_midi.sustain.is_empty());
        app.live_midi.bus.panic();
        app.poll_live_midi();
        assert!(app.live_midi.held.is_empty());
        assert!(app.live_midi.sustain.is_empty());
        app.run("transport.record", json!({})).unwrap();
        app.handle_transport(0.0);
        app.engine.transport.position = 48000;
        app.run("transport.stop", json!({})).unwrap();
        app.handle_transport(0.0);
        let soundcraft_model::ClipContent::Midi { sequence } = &app.engine.session().tracks[0].clips()[0].content else { panic!("MIDI take") };
        assert!(sequence.notes.is_empty(), "a reconnected keyboard must not replay old held keys");
    }
    #[test]
    fn hostile_live_messages_never_enter_device_or_capture_queues() {
        let _palette = crate::theme::lock_test_palette();
        let mut app = app();
        for message in
            [json!([]), json!([144]), json!([144, 60]), json!([144, 60, 256]), json!([240, 60, 1]), json!([144, 60, -1]), json!([144, 60, 1, 2])]
        {
            assert!(app.run("midi.send", json!({"message":message})).is_err());
        }
        assert_eq!(app.live_midi.bus.received.load(Ordering::Relaxed), 0);
        assert!(app.run("midi.send", json!({"track":u64::MAX,"message":[144,60,1]})).is_err());
    }
    #[test]
    fn hostile_postroll_cannot_overflow_when_starting_a_midi_punch_take() {
        let _palette = crate::theme::lock_test_palette();
        let mut app = app();
        let edit = &mut app.engine.session_mut().edit;
        edit.selection = soundcraft_time::Range::new(0, 128);
        edit.pre_post_roll = true;
        edit.post_roll = i64::MAX;
        app.run("transport.record", json!({})).unwrap();
        app.handle_transport(0.0);
        assert!(app.live_midi.recording());
        app.run("transport.stop", json!({})).unwrap();
        app.handle_transport(0.0);
        assert!(!app.live_midi.recording());
    }
}
