//! PULSE programs and MIDI printing share the engine's transactional undo boundary.
use super::{CommandSpec, always, bad, clip_id, tracks_required};
use crate::{Engine, Result, cmd};
use serde_json::{Value, json};
use soundcraft_midi::sequencer::{Mode, Program};
use soundcraft_model::{Clip, ClipContent};
use std::sync::Arc;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(query "sequencer.catalog", "Discover PULSE Sequencer", [], None, "{} → starter program, limits, modes, rates and bulk editing workflow", always, catalog),
        cmd!(query "sequencer.inspect", "Inspect Step Program", [], None, "{clip} → complete editable program, owning track, MIDI consistency, base_revision", always, inspect),
        cmd!(
            "sequencer.create",
            "Create Step Program",
            [],
            None,
            "{track, at?:samples, program?:complete program}: adds a separate MIDI part; preserves existing music and instrument",
            always,
            create
        ),
        cmd!(
            "sequencer.set",
            "Store Step Program",
            [],
            None,
            "{clip, program:complete program, base_revision}: atomic bulk update; use pulse.preview then pulse.commit for AI sound design",
            always,
            set
        ),
        cmd!(
            "sequencer.detach",
            "Detach Step Program",
            [],
            None,
            "{clip}: keep MIDI and remove sequencer metadata for piano-roll editing",
            always,
            detach
        ),
        cmd!(
            "sequencer.print",
            "Print Pattern to MIDI",
            [],
            None,
            "{clip, track, at:samples}: copy accepted pattern to an explicit MIDI/instrument target and position; preserves source; mute source if overlapping",
            always,
            print
        ),
    ]
}
pub fn parse(v: &Value) -> std::result::Result<Program, String> {
    // Provider/control input is already bounded by the transport; cap cloning here too.
    if v.to_string().len() > 512_000 {
        return Err("Program JSON exceeds 512 KB".into());
    }
    let p: Program = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    p.validate()?;
    Ok(p)
}
fn catalog(_: &mut Engine, _: &Value) -> Result<Value> {
    let mut arp = Program::default();
    if let Some(l) = arp.banks.first_mut().and_then(|b| b.lanes.first_mut()) {
        l.mode = Mode::Arp;
        l.steps.iter_mut().for_each(|s| s.on = true);
    }
    Ok(json!({"name":"PULSE", "starter":Program::default(),"arp_starter":arp,
        "rates_ticks":{"quarter":960,"eighth":480,"sixteenth":240,"thirty_second":120,"sixty_fourth":60,"quarter_triplet":640,"eighth_triplet":320,"sixteenth_triplet":160,"thirty_second_triplet":80},
        "limits":{"banks":8,"lanes_per_bank":8,"steps_per_lane":64,"chain_entries":32,"cycles":32,"notes":65536},
        "modes":["melody","drum","arp","chord"],"directions":["forward","reverse","pendulum","random","walk"],"arp_orders":["up","down","pendulum","memory","random"],
        "drum_notes":{"kick":36,"snare":38,"closed_hat":42,"open_hat":46,"clap":39},
        "rules":"Use pulse.open/preview/steps/run/commit to audition candidates before storing. A lane root is an absolute MIDI note; step.pitch and chord entries are semitone offsets. Arp uses chord memory and octave span; ratchets repeat its chosen pitch. Missing steps are rests. Ties extend the previous attack, never cross pattern boundaries, and need chance 100 / one ratchet. Chance is per step. Seed and cycles make variation repeatable; printed MIDI matches preview. Drum mode stores fixed drum_note on channel 10; the current instrument host plays one timbre per track, so choose a dedicated drum instrument or drum sampler. All lanes share their part's instrument. Printing preserves the original. Detach before editing notes independently."}))
}
fn writable(e: &Engine, id: &str) -> Result<()> {
    if e.transport.recording {
        return Err(bad(id, "Stop recording before editing a step program"));
    }
    Ok(())
}
pub fn length(s: &soundcraft_model::Session, at: i64, ticks: i64) -> i64 {
    let start = s.tempo.samples_to_ticks(at, s.sample_rate);
    s.tempo.tick_to_samples(start.saturating_add(ticks), s.sample_rate).saturating_sub(at).max(1)
}
fn space(
    s: &soundcraft_model::Session,
    track: soundcraft_model::TrackId,
    at: i64,
    len: i64,
    except: Option<soundcraft_model::ClipId>,
    id: &str,
) -> Result<()> {
    if at < 0 || len <= 0 || at.checked_add(len).is_none() {
        return Err(bad(id, "Pattern exceeds the supported timeline range"));
    }
    let t = s.track(track).filter(|t| t.playlist().is_some()).ok_or_else(|| bad(id, "Target playlist not found"))?;
    let range = soundcraft_time::Range::new(at, at.saturating_add(len));
    if t.clips().iter().any(|c| Some(c.id) != except && c.range().overlaps(&range)) {
        return Err(bad(
            id,
            "Destination contains another part. Choose an empty position or a separate instrument/sampler track; existing music is preserved",
        ));
    }
    Ok(())
}
pub(crate) fn reflow(e: &mut Engine) -> Result<()> {
    let s = e.session();
    let changes: Vec<_> = s
        .all_clips()
        .filter_map(|c| {
            let p = c.step_sequencer.as_ref()?;
            let compiled = p.compile().ok()?;
            if !matches!(&c.content,ClipContent::Midi{sequence} if *sequence==compiled.sequence) {
                return None;
            }
            let len = length(s, c.start, compiled.ticks);
            (len != c.length).then_some((c.id, len))
        })
        .collect();
    if changes.is_empty() {
        return Ok(());
    }
    // A tempo change must not lengthen a sequencer part over existing music.
    // Reject the entire clock edit before changing any extent, including alternate playlists.
    for t in &s.tracks {
        for p in &t.playlists {
            for c in &p.clips {
                if let Some((_, len)) = changes.iter().find(|(id, _)| *id == c.id) {
                    if c.time_locked || c.edit_locked {
                        return Err(bad("sequencer.clock", "Unlock PULSE parts before changing their clock extent"));
                    }
                    let end = c.start.checked_add(*len).ok_or_else(|| bad("sequencer.clock", "Pattern exceeds the timeline range"))?;
                    let r = soundcraft_time::Range::new(c.start, end);
                    if *len > c.length && p.clips.iter().any(|other| other.id != c.id && other.range().overlaps(&r)) {
                        return Err(bad(
                            "sequencer.clock",
                            "This tempo would extend a PULSE part over another part. Move following music to leave room, then retry; the clock edit was not applied",
                        ));
                    }
                }
            }
        }
    }
    let s = e.session_mut();
    for t in &mut s.tracks {
        for p in &mut t.playlists {
            for c in &mut p.clips {
                if let Some((_, len)) = changes.iter().find(|(id, _)| *id == c.id) {
                    c.length = *len;
                }
            }
        }
    }
    Ok(())
}
fn inspect(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = clip_id(p, "sequencer.inspect")?;
    let (t, c) = e.session().find_clip(id).ok_or_else(|| bad("sequencer.inspect", "Part not found"))?;
    let program = c.step_sequencer.as_ref().ok_or_else(|| bad("sequencer.inspect", "This part has no step program"))?;
    let compiled = program.compile().map_err(|s| bad("sequencer.inspect", s))?;
    Ok(json!({"clip":id,"track":t,"program":program,"base_revision":e.revision,
        "midi_matches":matches!(&c.content,ClipContent::Midi{sequence} if *sequence == compiled.sequence),"ticks":compiled.ticks,"notes":compiled.sequence.notes.len()}))
}
fn create(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "sequencer.create";
    writable(e, id)?;
    let track = tracks_required(e, id, p)?.first().copied().ok_or_else(|| bad(id, "Choose a MIDI or instrument track"))?;
    if !e.session().track(track).is_some_and(|t| t.kind.is_midi()) {
        return Err(bad(id, "Choose a MIDI or instrument track"));
    }
    let program = p.get("program").map(parse).transpose().map_err(|s| bad(id, s))?.unwrap_or_default();
    let compiled = program.compile().map_err(|s| bad(id, s))?;
    let mut at = p
        .get("at")
        .map(|v| v.as_i64().filter(|n| *n >= 0).ok_or_else(|| bad(id, "at must be a nonnegative sample position")))
        .transpose()?
        .unwrap_or(e.session().edit.selection.start.max(0));
    let mut len = length(e.session(), at, compiled.ticks);
    if p.get("at").is_none() && space(e.session(), track, at, len, None, id).is_err() {
        at = e.session().track(track).map(|t| t.clips().iter().map(Clip::end).max().unwrap_or(at).max(at)).unwrap_or(at);
        len = length(e.session(), at, compiled.ticks);
    }
    space(e.session(), track, at, len, None, id)?;
    let s = e.session_mut();
    let clip = s.new_clip_id();
    let mut c = Clip::midi(clip, program.name.clone(), at, len, compiled.sequence);
    c.step_sequencer = Some(Arc::new(program));
    // Never use the overwrite placement helper for generated music.
    let pl = s.track_mut(track).and_then(|t| t.playlist_mut()).ok_or_else(|| bad(id, "Target playlist disappeared"))?;
    pl.clips.push(c);
    pl.sort();
    s.edit.selected_clips = vec![clip];
    s.edit.selected_tracks = vec![track];
    Ok(json!({"clip":clip,"track":track,"base_revision":e.revision}))
}
fn set(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "sequencer.set";
    writable(e, id)?;
    if p.get("base_revision").and_then(Value::as_u64) != Some(e.revision) {
        return Err(bad(id, "Session changed; inspect the part and preview again"));
    }
    let clip = clip_id(p, id)?;
    let (track, c) = e.session().find_clip(clip).ok_or_else(|| bad(id, "Part not found"))?;
    if c.edit_locked || c.time_locked {
        return Err(bad(id, "Unlock the part first"));
    }
    let old = c.step_sequencer.as_ref().ok_or_else(|| bad(id, "Not a step program"))?;
    if !matches!(&c.content,ClipContent::Midi{sequence} if old.compile().is_ok_and(|v|v.sequence == *sequence)) {
        return Err(bad(id, "MIDI was edited separately; detach this part or create a new step program"));
    }
    let program = parse(p.get("program").ok_or_else(|| bad(id, "program required"))?).map_err(|s| bad(id, s))?;
    let compiled = program.compile().map_err(|s| bad(id, s))?;
    let len = length(e.session(), c.start, compiled.ticks);
    space(e.session(), track, c.start, len, Some(clip), id)?;
    let c = e.session_mut().find_clip_mut(clip).ok_or_else(|| bad(id, "Part disappeared"))?;
    c.content = ClipContent::Midi { sequence: compiled.sequence };
    c.length = len;
    c.step_sequencer = Some(Arc::new(program));
    Ok(json!({"clip":clip,"base_revision":e.revision}))
}
fn detach(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "sequencer.detach";
    writable(e, id)?;
    let clip = clip_id(p, id)?;
    let (_, c) = e.session().find_clip(clip).ok_or_else(|| bad(id, "Part not found"))?;
    if c.edit_locked {
        return Err(bad(id, "Unlock the part first"));
    }
    e.session_mut().find_clip_mut(clip).ok_or_else(|| bad(id, "Part disappeared"))?.step_sequencer = None;
    Ok(json!({"clip":clip}))
}
fn print(e: &mut Engine, p: &Value) -> Result<Value> {
    let id = "sequencer.print";
    writable(e, id)?;
    let clip = clip_id(p, id)?;
    let track = tracks_required(e, id, p)?.first().copied().ok_or_else(|| bad(id, "Explicit target track required"))?;
    if p.get("track").is_none() || !e.session().track(track).is_some_and(|t| t.kind.is_midi()) {
        return Err(bad(id, "Explicit MIDI/instrument target track required"));
    }
    let at = p.get("at").and_then(Value::as_i64).filter(|n| *n >= 0).ok_or_else(|| bad(id, "Explicit nonnegative sample position required"))?;
    let (_, c) = e.session().find_clip(clip).ok_or_else(|| bad(id, "Part not found"))?;
    let program = c.step_sequencer.as_ref().ok_or_else(|| bad(id, "Not a step program"))?;
    let compiled = program.compile().map_err(|s| bad(id, s))?;
    if !matches!(&c.content,ClipContent::Midi{sequence} if *sequence==compiled.sequence) {
        return Err(bad(id, "MIDI no longer matches the program; resolve separately edited notes first"));
    }
    let mut copy = c.clone();
    copy.start = at;
    copy.length = length(e.session(), at, compiled.ticks);
    copy.step_sequencer = None;
    copy.name = format!("{} MIDI", c.name);
    copy.muted = false;
    copy.edit_locked = false;
    copy.time_locked = false;
    copy.group = None;
    copy.loop_id = None;
    copy.auto_name = None;
    space(e.session(), track, at, copy.length, None, id)?;
    let s = e.session_mut();
    copy.id = s.new_clip_id();
    let out = copy.id;
    let pl = s.track_mut(track).and_then(|t| t.playlist_mut()).ok_or_else(|| bad(id, "Target playlist disappeared"))?;
    pl.clips.push(copy);
    pl.sort();
    Ok(json!({"clip":out,"source":clip,"track":track,"at":at,"source_preserved":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (Engine, soundcraft_model::TrackId, soundcraft_model::ClipId) {
        let mut e = Engine::default();
        e.execute("track.new", &json!({"kind":"instrument","instrument":"drum_synth","name":"Kit"})).unwrap();
        let track = e.session().tracks[0].id;
        let mut p = Program::default();
        p.banks[0].lanes[0].steps[0].on = true;
        let v = e.execute("sequencer.create", &json!({"track":track.0,"program":p})).unwrap();
        (e, track, soundcraft_model::ClipId(v["clip"].as_u64().unwrap()))
    }
    #[test]
    fn tempo_extent_reflows_in_the_same_undo_and_note_edits_are_protected() {
        let (mut e, _, clip) = setup();
        let before = e.session_arc();
        let original = e.session().find_clip(clip).unwrap().1.clone();
        e.execute("event.tempo", &json!({"bpm":60})).unwrap();
        assert_eq!(e.session().find_clip(clip).unwrap().1.length, original.length * 2);
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session(), before.as_ref());
        e.execute("midi.note_add", &json!({"clip":clip.0,"pitch":73,"start_ticks":0})).unwrap();
        let edited = e.session_arc();
        let p = original.step_sequencer.unwrap();
        assert!(e.execute("sequencer.set", &json!({"clip":clip.0,"program":p,"base_revision":e.revision})).is_err());
        assert!(Arc::ptr_eq(&edited, &e.session_arc()));
        e.execute("sequencer.detach", &json!({"clip":clip.0})).unwrap();
        assert!(e.session().find_clip(clip).unwrap().1.step_sequencer.is_none());
    }
    #[test]
    fn invalid_and_locked_program_edits_are_atomic() {
        let (mut e, track, clip) = setup();
        let before = e.session_arc();
        let mut p = Program::default();
        p.banks[0].lanes[0].steps[0].ratchets = 0;
        assert!(e.execute("sequencer.create", &json!({"track":track.0,"program":p})).is_err());
        assert!(Arc::ptr_eq(&before, &e.session_arc()));
        let mut p = Program::default();
        p.banks[0].lanes[0].root = 127;
        p.banks[0].lanes[0].steps[0].on = true;
        p.banks[0].lanes[0].steps[0].pitch = 1;
        assert!(e.execute("sequencer.set", &json!({"clip":clip.0,"program":p,"base_revision":e.revision})).is_err());
        assert!(Arc::ptr_eq(&before, &e.session_arc()));
        e.session_mut().find_clip_mut(clip).unwrap().edit_locked = true;
        let locked = e.session_arc();
        assert!(e.execute("sequencer.set", &json!({"clip":clip.0,"program":Program::default(),"base_revision":e.revision})).is_err());
        assert!(Arc::ptr_eq(&locked, &e.session_arc()));
    }
    #[test]
    fn creation_and_print_never_overwrite_the_source_or_existing_music() {
        let (mut e, track, clip) = setup();
        let original = e.session().find_clip(clip).unwrap().1.clone();
        let before = e.session_arc();
        assert!(e.execute("sequencer.create", &json!({"track":track.0,"at":original.start})).is_err());
        assert!(e.execute("sequencer.print", &json!({"clip":clip.0,"track":track.0,"at":original.start})).is_err());
        assert!(Arc::ptr_eq(&before, &e.session_arc()));
        let v = e.execute("sequencer.create", &json!({"track":track.0})).unwrap();
        let next = soundcraft_model::ClipId(v["clip"].as_u64().unwrap());
        assert!(e.session().find_clip(next).unwrap().1.start >= original.end());
        assert_eq!(*e.session().find_clip(clip).unwrap().1, original);
        let before = e.session_arc();
        assert!(e.execute("sequencer.print", &json!({"clip":clip.0,"track":track.0,"at":original.end()})).is_err());
        assert!(Arc::ptr_eq(&before, &e.session_arc()));
    }
    #[test]
    fn conflicting_tempo_change_rolls_back_the_clock_and_every_clip() {
        let (mut e, track, _) = setup();
        e.execute("sequencer.create", &json!({"track":track.0})).unwrap();
        let before = e.session_arc();
        assert!(e.execute("event.tempo", &json!({"bpm":60})).is_err());
        assert_eq!(e.session(), before.as_ref());
        e.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(e.session().track(track).unwrap().clips().len(), 1);
    }
}
