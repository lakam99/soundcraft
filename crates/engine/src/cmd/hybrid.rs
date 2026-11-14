//! Typed modular patch commands and transactional sampler derivation.
use super::{CommandSpec, always, bad, has_tracks, tracks_required};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_dsp::hybrid::{self, Kind, Patch};
use soundcraft_model::{ChannelFormat, Insert, TrackId, TrackKind};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "synth.catalog","Discover FORGE Modules",[],None,"{} → module parameter ranges, routing limits and an editable starter patch",always,catalog),
        cmd!(
            "synth.patch",
            "Set FORGE Patch",
            [],
            None,
            "{track,patch?: complete patch}: query current patch or atomically validate/set a complete graph; AI sound design should use soundlab.preview before committing",
            has_tracks,
            patch
        ),
        cmd!(
            "synth.sample_load",
            "Load FORGE Sample Source",
            [],
            None,
            "{track,path}: attach an audio source to FORGE without replacing the synth patch; saved with the session",
            has_tracks,
            sample_load
        ),
    ]
}
fn sample_load(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "synth.sample_load";
    if e.transport.recording {
        return Err(bad(id, "Stop recording before loading a sample"));
    }
    let tid = tracks_required(e, id, p)?.first().copied().ok_or_else(|| bad(id, "track required"))?;
    if e.session().track(tid).and_then(|t| t.instrument.as_ref()).is_none_or(|i| i.plugin != hybrid::ID) {
        return Err(bad(id, "Choose a FORGE instrument track"));
    }
    let path = p.get("path").and_then(Value::as_str).ok_or_else(|| bad(id, "path required"))?;
    let bytes = crate::io::read_file(path)?;
    let (info, audio) = soundcraft_audio_io::decode(&bytes, std::path::Path::new(path).extension().and_then(|s| s.to_str()))
        .map_err(|err| bad(id, err.to_string()))?;
    if audio.frames() == 0 || audio.num_channels() == 0 || audio.frames().saturating_mul(audio.num_channels()) > 20_000_000 {
        return Err(bad(id, "Sample must contain audio and fit within twenty million sample values"));
    }
    if audio.channels.iter().flatten().any(|x| !x.is_finite()) {
        return Err(bad(id, "Sample contains non-finite audio"));
    }
    let audio = crate::io::at_rate(audio, e.session().sample_rate.hz(), 20_000_000)
        .map_err(|r| bad(id, r.message("Sample exceeds the source size limit")))?;
    let source = crate::io::add_source(e.session_mut(), "FORGE Sample", audio, Some(path), info.format);
    let ins = e.session_mut().track_mut(tid).and_then(|t| t.instrument.as_mut()).ok_or_else(|| bad(id, "Track disappeared"))?;
    ins.sample = Some(source);
    Ok(json!({"track":tid,"source":source}))
}
pub fn parse(value: &Value) -> std::result::Result<Arc<Patch>, String> {
    for (field, max) in [("modules", hybrid::MAX_MODULES), ("routes", hybrid::MAX_ROUTES), ("modulation", hybrid::MAX_MODULATIONS)] {
        if value.get(field).and_then(Value::as_array).is_some_and(|v| v.len() > max) {
            return Err(format!("{field} exceeds patch limit {max}"));
        }
    }
    serde_json::from_value::<Patch>(value.clone()).map(Arc::new).map_err(|e| e.to_string())
}
fn catalog(_: &mut Engine, _: &Value) -> Result<Value> {
    Ok(
        json!({"instrument":hybrid::ID,"modules":Kind::ALL.iter().map(|kind|json!({"kind":kind,"name":kind.label(),"parameters":kind.params(),"audio_input":kind.accepts_audio(),"control":kind.control()})).collect::<Vec<_>>(),"starter":Patch::default(),"limits":{"modules":hybrid::MAX_MODULES,"routes":hybrid::MAX_ROUTES,"modulation":hybrid::MAX_MODULATIONS},"rules":"IDs are stable. Audio routes feed oscillators as FM, or processors as audio. Modulation adds normalized amount (-1..1); macro sources 0..3 are bipolar around .5. No feedback cycles. Envelope to amplifier.gain makes a playable envelope. Sample modules use the track instrument's sample source."}),
    )
}
fn patch(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "synth.patch";
    let track = tracks_required(e, id, p)?.first().copied().ok_or_else(|| bad(id, "track required"))?;
    if let Some(value) = p.get("patch") {
        let compiled = parse(value).map_err(|s| bad(id, s))?;
        set(e, track, compiled).map_err(|s| bad(id, s))?;
    }
    let ins = e.session().track(track).and_then(|t| t.instrument.as_ref()).ok_or_else(|| bad(id, "instrument track required"))?;
    Ok(json!({"track":track,"patch":ins.hybrid.as_deref().cloned().unwrap_or_default(),"macros":ins.params,"sample":ins.sample}))
}
pub fn set(e: &mut Engine, track: TrackId, patch: Arc<Patch>) -> std::result::Result<(), String> {
    if e.transport.recording {
        return Err("Stop recording before replacing an instrument graph".into());
    }
    let old = e.session().track(track).filter(|t| t.kind == TrackKind::Instrument).ok_or("Select an instrument track")?;
    let sample = old.instrument.as_ref().and_then(|i| i.sample);
    if patch.design().modules.iter().any(|m| m.kind == Kind::Sample) && sample.is_none() {
        return Err("Load a sample source before using a Sample module".into());
    }
    let mut insert = Insert::new(hybrid::ID);
    for (i, m) in patch.design().macros.iter().enumerate() {
        if let Some(info) = hybrid::MACROS.get(i) {
            insert.params.insert(info.id.into(), m.value);
        }
    }
    insert.params.insert(
        "level".into(),
        old.instrument.as_ref().filter(|i| i.plugin == hybrid::ID).and_then(|i| i.params.get("level")).copied().unwrap_or(-6.0),
    );
    insert.sample = sample;
    insert.preset = patch.design().name.clone();
    insert.hybrid = Some(patch);
    let Some(track) = e.session_mut().track_mut(track) else {
        return Err("Track disappeared".into());
    };
    track.instrument = Some(insert);
    Ok(())
}
/// Attach a worker-rendered snapshot as a new sampler/audio track, preserving its original synth.
pub fn derive(
    e: &mut Engine,
    source_track: TrackId,
    audio: soundcraft_audio_io::AudioBuffer,
    name: &str,
    note: u8,
    sampler: bool,
) -> std::result::Result<Value, String> {
    if e.transport.recording {
        return Err("Stop recording before creating a derived track".into());
    }
    let original = e.session().track(source_track).ok_or("Source track disappeared")?;
    let route = original.midi_input.clone();
    if audio.sample_rate != e.session().sample_rate.hz()
        || audio.frames() == 0
        || audio.frames() > 2_000_000
        || audio.num_channels() != 2
        || audio.channels.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Invalid rendered sound (derive at the session sample rate)".into());
    }
    let before = e.session_arc();
    let session = e.session_mut();
    let source = crate::io::add_source(session, name, audio, None, soundcraft_audio_io::FileFormat::Wav);
    let track = session.add_track(if sampler { TrackKind::Instrument } else { TrackKind::Audio }, ChannelFormat::Stereo, None);
    let cid = (!sampler).then(|| session.new_clip_id());
    let frames = session.source(source).map_or(0, |s| s.frames);
    let t = session.track_mut(track).ok_or("New track missing")?;
    t.name = format!("{name} {}", if sampler { "Sampler" } else { "Audio" });
    t.midi_input = route;
    if sampler {
        let mut ins = Insert::new("sampler");
        ins.sample = Some(source);
        ins.params.insert("root_note".into(), f32::from(note));
        ins.params.insert("loop_mode".into(), 0.0);
        t.instrument = Some(ins);
        t.mixer.input_monitor = true;
    }
    if let Some(cid) = cid {
        crate::edit::place_clip(session, track, soundcraft_model::Clip::audio(cid, name, source, 0, 0, frames as i64));
    }
    if let Some(original) = session.track_mut(source_track) {
        // Live MIDI monitors only the selected track; armed tracks listen regardless of selection.
        // Disarm the source so the newly selected sampler owns the keyboard without doubling.
        original.mixer.record_arm = false;
    }
    session.edit.selected_tracks = vec![track];
    session.edit.selected_clips.clear();
    e.push_undo(if sampler { "Create sampler from FORGE" } else { "Render FORGE audio" }, before);
    Ok(json!({"track":track,"source":source,"original_track":source_track,"root_note":note,"frames":frames,"sampler":sampler}))
}
