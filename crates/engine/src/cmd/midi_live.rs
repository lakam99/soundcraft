//! Live MIDI routing and bounded, undoable take checkpoints.
use super::{CommandSpec, always, bad, has_tracks, tracks_required};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_midi::{Sequence, live::MAX_CAPTURE_EVENTS};
use soundcraft_model::{Clip, ClipContent, ClipId};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "midi.input",
            "MIDI Input Routing",
            [],
            None,
            "{track, device?: id|null, device_name?: string, channel?: 1..16|null}",
            has_tracks,
            input
        ),
        cmd!(
            "midi.record_take",
            "Record MIDI",
            [],
            None,
            "{track, clip?: id, start: samples, length: samples, sequence: {notes,ctrls}}",
            always,
            record
        ),
    ]
}

fn input(engine: &mut Engine, params: &Value) -> Result<Value> {
    let ids = tracks_required(engine, "midi.input", params)?;
    if ids.iter().any(|id| !engine.session().track(*id).is_some_and(|track| track.kind.is_midi())) {
        return Err(bad("midi.input", "select an instrument or MIDI track"));
    }
    let device = match params.get("device") {
        Some(Value::Null) => Some(None),
        Some(Value::String(id)) if id.len() <= 1024 => Some(Some(id.clone())),
        None => None,
        _ => return Err(bad("midi.input", "device must be an id or null")),
    };
    let channel = match params.get("channel") {
        Some(Value::Null) => Some(None),
        Some(value) => Some(Some(
            value.as_u64().filter(|value| (1..=16).contains(value)).ok_or_else(|| bad("midi.input", "channel must be 1..16 or null"))? as u8,
        )),
        None => None,
    };
    let name = params.get("device_name").and_then(Value::as_str);
    if name.is_some_and(|name| name.len() > 1024) {
        return Err(bad("midi.input", "device name too long"));
    }
    for id in ids {
        if let Some(track) = engine.session_mut().track_mut(id) {
            if let Some(device) = &device {
                track.midi_input.device = device.clone();
                track.midi_input.device_name = name.map(str::to_string);
            }
            if let Some(channel) = channel {
                track.midi_input.channel = channel;
            }
        }
    }
    Ok(json!({"routed":true}))
}

fn record(engine: &mut Engine, params: &Value) -> Result<Value> {
    const ID: &str = "midi.record_take";
    let track = tracks_required(engine, ID, params)?.first().copied().ok_or_else(|| bad(ID, "track required"))?;
    if !engine.session().track(track).is_some_and(|track| track.kind.is_midi()) {
        return Err(bad(ID, "not a MIDI or instrument track"));
    }
    let start = params.get("start").and_then(Value::as_i64).filter(|n| *n >= 0).ok_or_else(|| bad(ID, "nonnegative start required"))?;
    let length = params
        .get("length")
        .and_then(Value::as_i64)
        .filter(|n| *n > 0 && start.checked_add(*n).is_some())
        .ok_or_else(|| bad(ID, "positive bounded length required"))?;
    let data = params.get("sequence").ok_or_else(|| bad(ID, "sequence required"))?;
    let notes = data.get("notes").and_then(Value::as_array).ok_or_else(|| bad(ID, "notes array required"))?;
    let ctrls = data.get("ctrls").and_then(Value::as_array).ok_or_else(|| bad(ID, "ctrls array required"))?;
    if notes.len().saturating_add(ctrls.len()) > MAX_CAPTURE_EVENTS {
        return Err(bad(ID, "MIDI take event limit exceeded"));
    }
    let mut sequence: Sequence = serde_json::from_value(data.clone()).map_err(|_| bad(ID, "invalid MIDI sequence"))?;
    if sequence.notes.iter().any(|note| {
        note.pitch > 127
            || note.velocity == 0
            || note.velocity > 127
            || note.release_velocity > 127
            || note.channel > 15
            || note.start < 0
            || note.length <= 0
            || note.start.checked_add(note.length).is_none()
    }) || sequence.ctrls.iter().any(|event| {
        event.channel > 15
            || event.tick < 0
            || match event.kind {
                soundcraft_midi::CtrlKind::PitchBend => !(-8192..=8191).contains(&event.value),
                soundcraft_midi::CtrlKind::Cc(number) | soundcraft_midi::CtrlKind::PolyPressure(number) => {
                    number > 127 || !(0..=127).contains(&event.value)
                }
                _ => !(0..=127).contains(&event.value),
            }
    }) {
        return Err(bad(ID, "invalid MIDI event values"));
    }
    sequence.sort();
    let clip = if let Some(value) = params.get("clip") {
        let id = ClipId(value.as_u64().ok_or_else(|| bad(ID, "invalid clip id"))?);
        if !engine.session().find_clip(id).is_some_and(|(owner, clip)| owner == track && matches!(clip.content, ClipContent::Midi { .. })) {
            return Err(bad(ID, "take clip is missing or belongs to another track"));
        }
        let Some(clip) = engine.session_mut().find_clip_mut(id) else {
            return Err(bad(ID, "take clip missing"));
        };
        clip.content = ClipContent::Midi { sequence };
        clip.start = start;
        clip.length = length;
        id
    } else {
        let session = engine.session_mut();
        let id = session.new_clip_id();
        crate::edit::place_clip(session, track, Clip::midi(id, "MIDI take", start, length, sequence));
        id
    };
    Ok(json!({"clip":clip}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn routes_and_takes_are_validated_before_mutation_and_undoable() {
        let mut engine = Engine::default();
        engine.execute("track.new", &json!({"name":"Keys","kind":"instrument"})).unwrap();
        let before = engine.session_arc();
        assert!(engine.execute("midi.input", &json!({"track":"Keys","channel":17})).is_err());
        assert_eq!(engine.session().tracks[0].midi_input.channel, before.tracks[0].midi_input.channel);
        engine.execute("midi.input", &json!({"track":"Keys","device":"stable-port-id","channel":2})).unwrap();
        assert_eq!(engine.session().tracks[0].midi_input.channel, Some(2));
        let seq = Sequence::default();
        let take = engine.execute_merged("midi.record_take", &json!({"track":"Keys","start":0,"length":100,"sequence":seq}), "take").unwrap();
        engine
            .execute_merged("midi.record_take", &json!({"track":"Keys","clip":take["clip"],"start":0,"length":200,"sequence":seq}), "take")
            .unwrap();
        assert!(engine.undo());
        assert!(engine.session().tracks[0].clips().is_empty());
        assert!(engine.redo());
        assert_eq!(engine.session().tracks[0].clips()[0].length, 200);
    }
    #[test]
    fn armed_track_set_is_frozen_during_midi_recording() {
        let mut e = Engine::default();
        for name in ["One", "Two"] {
            e.execute("track.new", &json!({"name":name,"kind":"instrument"})).unwrap();
        }
        e.execute("mix.record_arm", &json!({"track":"One","value":true})).unwrap();
        e.transport.recording = true;
        assert!(e.execute("mix.record_arm", &json!({"track":"Two","value":true})).is_err());
        assert!(!e.session().tracks[1].mixer.record_arm);
        e.transport.recording = false;
        e.execute("mix.record_arm", &json!({"track":"Two","value":true})).unwrap();
    }
    #[test]
    fn hostile_take_parameters_leave_session_and_history_unchanged() {
        let mut e = Engine::default();
        e.execute("track.new", &json!({"name":"Keys","kind":"instrument"})).unwrap();
        let before = e.undo_history();
        for data in [
            json!({"notes":[],"ctrls":[{"tick":0,"channel":0,"kind":{"Cc":128},"value":1}]}),
            json!({"notes":[],"ctrls":[{"tick":0,"channel":0,"kind":"PitchBend","value":8192}]}),
            json!({"notes":[{"start":0,"length":1,"pitch":128,"velocity":1,"release_velocity":0,"channel":0}],"ctrls":[]}),
            json!({"notes":[null],"ctrls":[]}),
        ] {
            assert!(e.execute("midi.record_take", &json!({"track":"Keys","start":0,"length":100,"sequence":data})).is_err());
        }
        assert!(e.execute("midi.record_take", &json!({"track":"Keys","start":i64::MAX,"length":1,"sequence":{"notes":[],"ctrls":[]}})).is_err());
        assert!(e.session().tracks[0].clips().is_empty());
        assert_eq!(e.undo_history(), before);
    }
}
