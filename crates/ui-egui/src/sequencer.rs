//! PULSE: isolated step-program candidates and instrument-connected loop audition.
use crate::SoundApp;
use serde_json::{Value, json};
use soundcraft_midi::sequencer::Program;
use soundcraft_model::{Clip, ClipContent, ClipId, Session, TrackId};
use std::sync::Arc;
mod view;
pub use view::show;
#[cfg(test)]
mod tests;

#[derive(Clone)]
struct Draft {
    program: Program,
    base: Clip,
    track: TrackId,
}
#[derive(Default)]
pub struct Runtime {
    active: Option<ClipId>,
    draft: Option<Draft>,
    open: bool,
    pub(crate) running: bool,
    return_position: i64,
    run_revision: u64,
    run_session: Option<Arc<Session>>,
    checked: Option<(ClipId, u64)>,
    clock: Vec<soundcraft_midi::sequencer::Clock>,
    pub(crate) bank: usize,
    pub(crate) lane: usize,
    pub(crate) page: usize,
    pub(crate) step: usize,
    pub(crate) error: String,
    pub(crate) skin: crate::sound_lab::skin::Skin,
}
impl Runtime {
    pub(crate) fn has_candidate(&self) -> bool {
        self.draft.is_some()
    }
}
pub fn params(id: &str) -> Option<&'static str> {
    Some(match id {
        "pulse.new" => {
            "{track?:id, kind?:melody|arp|drum}: create separate playable step part; creates FORGE/Drum Synth only if needed; existing instrument/music preserved"
        }
        "pulse.open" => "{clip?:id}: open existing step part",
        "pulse.inspect" => "{clip?:id}: program/candidate, track, base_revision, running, stale; inspect before edits",
        "pulse.preview" => "{clip?:id, base_revision, program:complete program}: validate bulk candidate, no document change; stops preview",
        "pulse.steps" => {
            "{clip?:id, base_revision, bank?:0, lane?:0, changes:[{step:zero_based, on?,pitch?,velocity?,gate?,chance?,ratchets?,tie?,accent?}]}: atomic up to 64 unique step changes to candidate; no document change"
        }
        "pulse.run" => {
            "{clip?:id}: loop isolated pattern through its current instrument (including FORGE candidate); no microphone or recording; stops song playback first"
        }
        "pulse.stop" => "{}: stop preview and release notes; restore session audio",
        "pulse.commit" => "{clip?:id}: Store valid current candidate in one Undo; reject independently edited MIDI",
        "pulse.discard" => "{}: Revert candidate, preserve stored pattern",
        "pulse.close" => "{}: close panel, stop preview; retains candidate until Store/Revert or project change",
        "pulse.latch" => {
            "{}: stop PULSE first, then capture currently held live MIDI keys into selected lane's arp chord memory; max 8 notes across 48 semitones; no continuous held-note mode"
        }
        "pulse.print" => {
            "{clip?:id, track:id, at:samples}: print accepted pattern as normal MIDI; preserves source; Store FORGE and PULSE first; mute source if overlapping"
        }
        "pulse.detach" => {
            "{clip?:id}: keep existing notes, detach PULSE controls and open the MIDI editor; one Undo restores the program; Store/Revert candidates first"
        }
        _ => return None,
    })
}
pub fn summary(app: &SoundApp) -> Value {
    json!({"active_part":app.sequencer.active,"candidate":app.sequencer.draft.is_some(),"running":app.sequencer.running,"commands":"Discover pulse.* and sequencer.catalog to design melody, drum or chord-memory arpeggio programs. Bulk edit steps; preview before Store."})
}
fn target(app: &SoundApp, p: &Value) -> Result<ClipId, String> {
    if let Some(v) = p.get("clip") {
        return v.as_u64().map(ClipId).ok_or_else(|| "clip must be an existing numeric PULSE part ID".into());
    }
    app.engine
        .session()
        .edit
        .selected_clips
        .iter()
        .copied()
        .find(|id| app.engine.session().find_clip(*id).is_some_and(|(_, c)| c.step_sequencer.is_some()))
        .or(app.sequencer.active)
        .ok_or_else(|| "Select a PULSE part, or press NEW".into())
}
fn current(app: &SoundApp, clip: ClipId) -> Result<(TrackId, Clip, Program), String> {
    let (t, c) = app.engine.session().find_clip(clip).ok_or("Step part not found")?;
    let program = c.step_sequencer.as_ref().ok_or("This is not a PULSE part; create a new one to preserve its notes")?;
    if app.sequencer.checked != Some((clip, app.engine.revision))
        && !matches!(&c.content,ClipContent::Midi{sequence} if program.compile().is_ok_and(|compiled|*sequence==compiled.sequence))
    {
        return Err("This part's MIDI was edited separately. Detach it for piano-roll editing, or create a new PULSE part.".into());
    }
    let p = if let Some(d) = app.sequencer.draft.as_ref().filter(|_| app.sequencer.active == Some(clip)) {
        if d.track != t || d.base != *c {
            return Err("Part changed outside PULSE; REVERT the candidate before editing again".into());
        }
        d.program.clone()
    } else {
        (**program).clone()
    };
    Ok((t, c.clone(), p))
}
fn activate(app: &mut SoundApp, clip: ClipId) -> Result<(), String> {
    let switching = app.sequencer.active != Some(clip);
    if switching && app.sequencer.draft.is_some() {
        return Err("STORE or REVERT the current candidate before opening another part".into());
    }
    // A failed inspection must not stop a valid preview or strand its panel on an invalid part.
    current(app, clip)?;
    if switching {
        stop(app);
        app.sequencer.active = Some(clip);
        app.sequencer.bank = 0;
        app.sequencer.lane = 0;
        app.sequencer.page = 0;
        app.sequencer.step = 0;
    }
    app.sequencer.checked = Some((clip, app.engine.revision));
    app.sequencer.open = true;
    Ok(())
}
fn preview(app: &mut SoundApp, clip: ClipId, program: Program) -> Result<(), String> {
    if app.engine.transport.recording {
        return Err("Stop recording first".into());
    }
    program.compile()?;
    activate(app, clip)?;
    let (track, base, _) = current(app, clip)?;
    if base.edit_locked || base.time_locked {
        return Err("Unlock the part first".into());
    }
    stop(app);
    app.sequencer.draft = Some(Draft { program, base, track });
    Ok(())
}
pub fn stop(app: &mut SoundApp) {
    if !app.sequencer.running {
        return;
    }
    app.sequencer.running = false;
    app.sequencer.run_session = None;
    if let Some(p) = &app.player {
        p.stop();
        p.set_speed(1.0);
        p.update_session(app.audible_session());
    }
    app.sim = None;
    app.engine.transport.playing = false;
    app.engine.transport.position = app.sequencer.return_position;
    app.live_midi.reset();
}
pub(crate) fn song_transport(app: &mut SoundApp, r: &soundcraft_engine::TransportRequest) {
    use soundcraft_engine::TransportRequest as T;
    if !app.sequencer.running
        || !matches!(r, T::Play | T::PlayFrom(_) | T::Locate(_) | T::PlaySelection | T::HalfSpeed | T::Scrub(_) | T::ResetSession)
    {
        return;
    }
    let at = app.engine.transport.position;
    stop(app);
    if matches!(r, T::ResetSession) {
        app.engine.transport.position = at;
    }
}
pub(crate) fn tick(app: &mut SoundApp) {
    let changed = app.engine.revision != app.sequencer.run_revision
        && app.sequencer.run_session.as_ref().is_none_or(|base| {
            let s = app.engine.session();
            base.sample_rate != s.sample_rate
                || base.tempo != s.tempo
                || base.tracks != s.tracks
                || base.busses != s.busses
                || base.outputs != s.outputs
                || base.sources != s.sources
                || base.edit.values != s.edit.values
        });
    if app.sequencer.running && (changed || app.sound_lab.audio_dirty || !app.engine.transport.playing) {
        stop(app);
        app.ui.status = "PULSE preview stopped after an edit. Press RUN to hear the new version.".into();
    }
    app.sequencer.run_revision = app.engine.revision;
    if let Some(clip) = app.sequencer.active
        && app.sequencer.checked != Some((clip, app.engine.revision))
        && current(app, clip).is_ok()
    {
        app.sequencer.checked = Some((clip, app.engine.revision));
    }
}
pub(crate) fn audible(app: &SoundApp, base: Arc<Session>) -> Arc<Session> {
    if !app.sequencer.running {
        return base;
    }
    let Some(clip) = app.sequencer.active else {
        return base;
    };
    let Ok((track, original, program)) = current(app, clip) else {
        return base;
    };
    let Ok(compiled) = program.compile() else {
        return base;
    };
    let mut s = (*base).clone();
    let length = soundcraft_engine::cmd::sequencer::length(&s, original.start, compiled.ticks);
    for t in &mut s.tracks {
        t.mixer.solo = false;
        t.mixer.input_monitor = false;
        t.mixer.record_arm = false;
        if t.id == track {
            t.mixer.mute = false;
        }
        for p in &mut t.playlists {
            for c in &mut p.clips {
                c.muted = c.id != clip;
            }
        }
    }
    if let Some(c) = s.find_clip_mut(clip) {
        c.content = ClipContent::Midi { sequence: compiled.sequence };
        c.length = length;
        c.muted = false;
    }
    Arc::new(s)
}
pub fn command(app: &mut SoundApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if !id.starts_with("pulse.") {
        return None;
    }
    Some(run(app, id, p))
}
fn lane_index(p: &Value, name: &str) -> Result<usize, String> {
    // Reject before casting: a wide selector must never wrap into a valid lane on wasm32.
    let Some(v) = p.get(name) else {
        return Ok(0);
    };
    v.as_u64().filter(|n| *n < 8).map(|n| n as usize).ok_or_else(|| format!("{name} must be a zero-based index from 0 to 7"))
}
fn run(app: &mut SoundApp, id: &str, p: &Value) -> Result<Value, String> {
    match id {
        "pulse.stop" => {
            stop(app);
            return Ok(json!({"stopped":true}));
        }
        "pulse.close" => {
            stop(app);
            app.sequencer.open = false;
            return Ok(json!({"closed":true}));
        }
        "pulse.discard" => {
            stop(app);
            app.sequencer.draft = None;
            app.sequencer.error.clear();
            return Ok(json!({"reverted":true}));
        }
        "pulse.detach" => {
            if app.sequencer.draft.is_some() {
                return Err("STORE or REVERT the candidate before editing its baked MIDI".into());
            }
            let clip = target(app, p)?;
            let (track, c) = app
                .engine
                .session()
                .find_clip(clip)
                .filter(|(_, c)| c.step_sequencer.is_some() && matches!(c.content, ClipContent::Midi { .. }))
                .ok_or("Choose a PULSE MIDI part")?;
            if c.edit_locked {
                return Err("Unlock the part first".into());
            }
            stop(app);
            let value = app.run("sequencer.detach", json!({"clip":clip.0}))?;
            app.sequencer.active = None;
            app.sequencer.open = false;
            let edit = &mut app.engine.session_mut().edit;
            edit.selected_clips = vec![clip];
            edit.selected_tracks = vec![track];
            app.ui.window = crate::MainWindow::Edit;
            app.ui.show_midi_editor = true;
            app.run("ui.dock", json!({"operation":"activate","panel":"midi"}))?;
            return Ok(value);
        }
        "pulse.new" => {
            if app.engine.transport.recording {
                return Err("Stop recording first".into());
            }
            if app.sequencer.draft.is_some() {
                return Err("STORE or REVERT the current candidate before creating another part".into());
            }
            stop(app);
            let kind = p.get("kind").and_then(Value::as_str).unwrap_or("melody");
            if !["melody", "arp", "drum"].contains(&kind) {
                return Err("kind must be melody, arp or drum".into());
            }
            let explicit =
                p.get("track").map(|v| v.as_u64().map(TrackId).ok_or("track must be an existing numeric MIDI/instrument ID")).transpose()?;
            if explicit.is_some_and(|id| !app.engine.session().track(id).is_some_and(|t| t.kind.is_midi())) {
                return Err("Target MIDI/instrument track not found".into());
            }
            if explicit.is_some_and(|id| app.engine.session().track(id).is_none_or(|t| t.instrument.is_none())) {
                return Err("Choose an instrument on the target track before adding PULSE".into());
            }
            let selected = explicit.or_else(|| {
                crate::workspace::selected(app).filter(|id| {
                    app.engine.session().track(*id).is_some_and(|t| {
                        t.kind.is_midi()
                            && t.instrument.is_some()
                            && (kind != "drum" || t.instrument.as_ref().is_some_and(|i| i.plugin == "drum_synth"))
                    })
                })
            });
            let track = if let Some(t) = selected {
                t
            } else {
                app.run("workspace.instrumentTrack", json!({"plugin":if kind=="drum" {"drum_synth"} else {soundcraft_dsp::hybrid::ID}}))?;
                crate::workspace::selected(app).ok_or("Instrument track was not selected")?
            };
            let mut program = Program::default();
            if let Some(b) = program.banks.first_mut()
                && let Some(l) = b.lanes.first_mut()
            {
                l.mode = match kind {
                    "arp" => soundcraft_midi::sequencer::Mode::Arp,
                    "drum" => soundcraft_midi::sequencer::Mode::Drum,
                    _ => soundcraft_midi::sequencer::Mode::Melody,
                };
                for (i, s) in l.steps.iter_mut().enumerate() {
                    s.on = if kind == "drum" { i % 4 == 0 } else { true };
                }
                if kind == "drum" {
                    l.name = "Kick".into();
                }
            }
            let value = app.run("sequencer.create", json!({"track":track.0,"program":program}))?;
            let clip = ClipId(value.get("clip").and_then(Value::as_u64).ok_or("New part not returned")?);
            activate(app, clip)?;
            return Ok(value);
        }
        _ => {}
    }
    let clip = target(app, p)?;
    if matches!(id, "pulse.preview" | "pulse.steps") && p.get("base_revision").and_then(Value::as_u64) != Some(app.engine.revision) {
        return Err("Session changed; inspect PULSE and retry with the current base_revision".into());
    }
    activate(app, clip)?;
    match id {
        "pulse.open" => {}
        "pulse.inspect" => {}
        "pulse.preview" => {
            let program = soundcraft_engine::cmd::sequencer::parse(p.get("program").ok_or("program required")?)?;
            preview(app, clip, program)?;
        }
        "pulse.steps" => {
            let (_, _, mut program) = current(app, clip)?;
            let bank = lane_index(p, "bank")?;
            let lane = lane_index(p, "lane")?;
            let l = program.banks.get_mut(bank).and_then(|b| b.lanes.get_mut(lane)).ok_or("Bank/lane not found")?;
            let changes = p
                .get("changes")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty() && a.len() <= 64)
                .ok_or("changes must contain 1–64 step edits")?;
            let mut seen = std::collections::BTreeSet::new();
            for change in changes {
                let index = change.get("step").and_then(Value::as_u64).filter(|i| *i < 64).ok_or("step must be 0–63")? as usize;
                if !seen.insert(index) {
                    return Err("Duplicate step in bulk edit".into());
                }
                if change.as_object().is_none_or(|o| {
                    o.keys().any(|k| !matches!(k.as_str(), "step" | "on" | "pitch" | "velocity" | "gate" | "chance" | "ratchets" | "tie" | "accent"))
                }) {
                    return Err("Unknown step field".into());
                }
                l.steps.resize(l.steps.len().max(index + 1), Default::default());
                let step = l.steps.get_mut(index).ok_or("Step missing")?;
                let mut value = serde_json::to_value(&*step).map_err(|e| e.to_string())?;
                for (k, v) in change.as_object().into_iter().flatten().filter(|(k, _)| k.as_str() != "step") {
                    value[k] = v.clone();
                }
                *step = serde_json::from_value(value).map_err(|e| e.to_string())?;
            }
            preview(app, clip, program)?;
        }
        "pulse.run" => {
            if app.engine.transport.recording {
                return Err("Stop recording first".into());
            }
            let (track, c, program) = current(app, clip)?;
            if app.engine.session().track(track).is_none_or(|t| t.instrument.is_none()) {
                return Err("Choose an instrument on this track before auditioning PULSE".into());
            }
            let compiled = program.compile()?;
            if compiled.sequence.notes.is_empty() {
                return Err("No audible steps. Enable a step or latch an arp chord first.".into());
            }
            if app.engine.transport.playing {
                app.stop_play();
            }
            app.sequencer.return_position = app.engine.transport.position;
            let end = c.start.saturating_add(soundcraft_engine::cmd::sequencer::length(app.engine.session(), c.start, compiled.ticks));
            app.sequencer.running = true;
            app.live_midi.reset();
            app.sequencer.run_revision = app.engine.revision;
            app.sequencer.run_session = Some(app.engine.session_arc());
            app.sequencer.clock = compiled.clock;
            app.start_play(c.start, Some(end), Some(soundcraft_time::Range::new(c.start, end)));
            app.sound_lab.audio_dirty = false;
        }
        "pulse.commit" => {
            let (_, _, program) = current(app, clip)?;
            if app.sequencer.draft.is_some() {
                stop(app);
                app.run("sequencer.set", json!({"clip":clip.0,"program":program,"base_revision":app.engine.revision}))?;
                app.sequencer.draft = None;
            }
        }
        "pulse.latch" => {
            if app.sequencer.running {
                return Err("Stop PULSE, hold the MIDI chord, then press LATCH KEYS".into());
            }
            let (track, _, mut program) = current(app, clip)?;
            let keys = app.live_midi.held_notes(track);
            if keys.is_empty() || keys.len() > 8 {
                return Err("Hold 1–8 MIDI keys, then press LATCH KEYS".into());
            }
            let root = keys.first().copied().ok_or("No keys held")?;
            if keys.iter().any(|k| k.saturating_sub(root) > 48) {
                return Err("Latched chord must fit within 48 semitones".into());
            }
            let l = program.banks.get_mut(app.sequencer.bank).and_then(|b| b.lanes.get_mut(app.sequencer.lane)).ok_or("Select a lane")?;
            l.root = root;
            l.chord = keys.iter().map(|k| (*k - root) as i8).collect();
            l.mode = soundcraft_midi::sequencer::Mode::Arp;
            preview(app, clip, program)?;
        }
        "pulse.print" => {
            if app.sequencer.draft.is_some() || app.sound_lab.has_active_candidate() {
                return Err("STORE PULSE and FORGE candidates before printing".into());
            }
            stop(app);
            let mut params = p.clone();
            params["clip"] = json!(clip.0);
            return app.run("sequencer.print", params);
        }
        _ => return Err("Unknown PULSE command".into()),
    }
    let (track, _, program) = current(app, clip)?;
    Ok(
        json!({"clip":clip,"track":track,"program":program,"candidate":app.sequencer.draft.is_some(),"base_revision":app.engine.revision,"running":app.sequencer.running,"stale":false}),
    )
}
