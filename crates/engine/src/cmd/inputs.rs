//! Session-owned named inputs; all UI and assistant edits use the same undoable command.
use super::{CommandSpec, always, bad};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_model::{ChannelFormat, InputPath, MAX_INPUT_CHANNELS, MAX_INPUT_PATHS, Route};

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        "setup.inputs",
        "Audio Inputs",
        [],
        None,
        "{action: list|create|update|delete|defaults, name?, new_name?, format?: mono|stereo, channels?: [zero-based port|null], active?: bool, count?: 2}",
        always,
        inputs
    )]
}

fn path(e: &Engine, p: &Value) -> Result<InputPath> {
    let id = "setup.inputs";
    let name = p["name"].as_str().ok_or_else(|| bad(id, "name required"))?;
    let old = e.session().inputs.iter().find(|x| x.name == name);
    let name = p.get("new_name").map_or(Some(name), Value::as_str).ok_or_else(|| bad(id, "new_name must be text"))?.trim();
    if name.is_empty()
        || name.chars().count() > 256
        || ["none", "no input", "main", "master", "out 1-2"].contains(&name.to_ascii_lowercase().as_str())
        || e.session().bus_by_name(name).is_some()
    {
        return Err(bad(id, "choose a unique input name of 1–256 characters"));
    }
    let format = match p.get("format") {
        Some(v) => v.as_str().and_then(ChannelFormat::from_id).ok_or_else(|| bad(id, "unknown format"))?,
        None => old.map_or(ChannelFormat::Mono, |x| x.format),
    };
    if format.channels() > 16 {
        return Err(bad(id, "input paths support at most 16 channels"));
    }
    let channels = match p.get("channels") {
        Some(v) => {
            let list = v.as_array().filter(|v| v.len() == format.channels()).ok_or_else(|| bad(id, "one port or null per path channel required"))?;
            list.iter()
                .map(|v| {
                    if v.is_null() {
                        Ok(None)
                    } else {
                        v.as_u64()
                            .and_then(|v| usize::try_from(v).ok())
                            .filter(|v| *v < MAX_INPUT_CHANNELS)
                            .map(Some)
                            .ok_or_else(|| bad(id, "port must be null or a zero-based integer below 256"))
                    }
                })
                .collect::<Result<Vec<_>>>()?
        }
        None => old.filter(|x| x.format == format).map_or_else(|| vec![None; format.channels()], |x| x.channels.clone()),
    };
    let active = match p.get("active") {
        Some(v) => v.as_bool().ok_or_else(|| bad(id, "active must be true or false"))?,
        None => old.is_none_or(|x| x.active),
    };
    Ok(InputPath { name: name.into(), format, channels, active })
}

fn inputs(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "setup.inputs";
    if e.transport.recording {
        return Err(bad(id, "stop recording before changing input paths"));
    }
    let action = p.get("action").map_or(Some("list"), Value::as_str).ok_or_else(|| bad(id, "action must be text"))?;
    match action {
        "list" => {}
        "defaults" => {
            let count = p["count"].as_u64().filter(|v| (1..=MAX_INPUT_CHANNELS as u64).contains(v)).ok_or_else(|| bad(id, "count must be 1–256"))?;
            e.session_mut().inputs = soundcraft_model::default_inputs(count as usize);
        }
        "create" | "update" | "delete" => edit(e, p, action)?,
        _ => return Err(bad(id, "unknown action")),
    }
    Ok(json!({"inputs": e.session().inputs}))
}

fn edit(e: &mut Engine, p: &Value, action: &str) -> Result<()> {
    let id = "setup.inputs";
    let name = p["name"].as_str().ok_or_else(|| bad(id, "name required"))?;
    let index = e.session().inputs.iter().position(|x| x.name == name);
    if action == "delete" {
        let index = index.ok_or_else(|| bad(id, "input not found"))?;
        let s = e.session_mut();
        s.inputs.remove(index);
        for t in &mut s.tracks {
            if t.mixer.input == Route::Hardware(name.into()) {
                t.mixer.input = Route::None;
            }
        }
        return Ok(());
    }
    if (action == "create" && index.is_some()) || (action == "update" && index.is_none()) {
        return Err(bad(id, "input already exists or cannot be found"));
    }
    let next = path(e, p)?;
    if e.session().inputs.iter().enumerate().any(|(i, x)| Some(i) != index && x.name == next.name) {
        return Err(bad(id, "input name already exists"));
    }
    let s = e.session_mut();
    if let Some(index) = index {
        for t in &mut s.tracks {
            if t.mixer.input == Route::Hardware(name.into()) {
                t.mixer.input = Route::Hardware(next.name.clone());
            }
        }
        if let Some(slot) = s.inputs.get_mut(index) {
            *slot = next;
        }
    } else {
        if s.inputs.len() >= MAX_INPUT_PATHS {
            return Err(bad(id, "input path limit reached"));
        }
        s.inputs.push(next);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soundcraft_model::{Session, TrackKind};
    #[test]
    fn recording_inputs_stay_pinned_until_the_take_stops() {
        let mut e = Engine::default();
        let t = e.session_mut().add_track(TrackKind::Audio, ChannelFormat::Mono, None);
        e.transport.recording = true;
        let before = e.session().clone();
        assert!(e.execute("track.input", &json!({"track":t.0,"input":"In 2"})).is_err());
        assert_eq!(e.session(), &before);
        e.transport.recording = false;
        e.execute("track.input", &json!({"track":t.0,"input":"In 2"})).unwrap();
        assert_eq!(e.session().input_channel(&e.session().track(t).unwrap().mixer.input, 0), Some(1));
    }

    #[test]
    fn large_interfaces_are_undoable_document_changes_and_recording_locks_setup() {
        let mut e = Engine::default();
        let before = e.session().clone();
        e.execute("setup.inputs", &json!({"action":"defaults","count":32})).unwrap();
        assert_eq!(e.session().inputs.len(), 48);
        assert!(e.is_dirty());
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session(), &before);
        assert!(!e.is_dirty());
        e.transport.recording = true;
        assert!(e.execute("setup.inputs", &json!({"action":"defaults","count":32})).is_err());
        assert_eq!(e.session(), &before);
    }

    #[test]
    fn maps_renames_persists_and_undoes_without_creating_a_bus() {
        let mut e = Engine::default();
        let t = e.session_mut().add_track(TrackKind::Audio, ChannelFormat::Stereo, None);
        e.execute("setup.inputs", &json!({"action":"create","name":"Synth","format":"stereo","channels":[11,3]})).unwrap();
        e.execute("track.input", &json!({"track":t.0,"input":"Synth"})).unwrap();
        e.execute("setup.inputs", &json!({"action":"update","name":"Synth","new_name":"Keys"})).unwrap();
        assert_eq!(e.session().input_channel(&e.session().track(t).unwrap().mixer.input, 0), Some(11));
        assert!(e.session().busses.is_empty());
        let loaded = Session::from_json(&e.session().to_json().unwrap()).unwrap();
        assert_eq!(loaded.inputs, e.session().inputs);
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session().track(t).unwrap().mixer.input, Route::Hardware("Synth".into()));
        for value in [json!([-1, 0]), json!([256, 0]), json!([1]), json!(["1", 2])] {
            let before = e.session().clone();
            assert!(e.execute("setup.inputs", &json!({"action":"update","name":"Synth","channels":value})).is_err());
            assert_eq!(e.session(), &before);
        }
        assert!(e.execute("track.input", &json!({"track":t.0,"input":"Typo"})).is_err());
    }
}
