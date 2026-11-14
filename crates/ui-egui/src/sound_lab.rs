//! FORGE candidates are isolated from the document until explicitly committed.
use crate::SoundApp;
use serde_json::{Value, json};
use soundcraft_audio_io::AudioBuffer;
use soundcraft_dsp::{
    SampleSource,
    hybrid::{
        self, Patch,
        render::{self, Metrics, Settings},
    },
};
use soundcraft_model::{Insert, SourceAudio, TrackId};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

mod cables;
mod skin;
#[cfg(test)]
mod tests;
mod view;
pub use view::show;

#[derive(Clone)]
struct Draft {
    patch: Arc<Patch>,
    base: Insert,
}
struct Ready {
    track: TrackId,
    patch: Arc<Patch>,
    base: Insert,
    settings: Settings,
    audio: Arc<AudioBuffer>,
    metrics: Metrics,
}
struct Job {
    track: TrackId,
    patch: Arc<Patch>,
    base: Insert,
    settings: Settings,
    action: String,
    committed: bool,
    cancel: Arc<AtomicBool>,
    rx: mpsc::Receiver<Result<render::Rendered, String>>,
}
pub struct Runtime {
    cable_layout: cables::Layout,
    hit_regions: Vec<egui::Rect>,
    drafts: std::collections::BTreeMap<TrackId, Draft>,
    job: Option<Job>,
    ready: Option<Ready>,
    pub(crate) settings: Settings,
    pub(crate) new_kind: usize,
    wiring: Option<view::Plug>,
    pub(crate) preview_level: f32,
    pub(crate) error: String,
    pub(crate) skin: skin::Skin,
    active: Option<TrackId>,
    pub(crate) audio_dirty: bool,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            cable_layout: Default::default(),
            hit_regions: Vec::new(),
            drafts: Default::default(),
            job: None,
            ready: None,
            settings: Default::default(),
            new_kind: 0,
            wiring: None,
            preview_level: -6.0,
            error: String::new(),
            skin: Default::default(),
            active: None,
            audio_dirty: false,
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(j) = &self.job {
            j.cancel.store(true, Ordering::Release);
        }
    }
}
impl Runtime {
    pub(crate) fn has_active_candidate(&self) -> bool {
        self.active.is_some_and(|t| self.drafts.contains_key(&t))
    }
}
struct Sample(Arc<SourceAudio>);
impl SampleSource for Sample {
    fn sample_rate(&self) -> f32 {
        self.0.buffer.sample_rate as f32
    }
    fn num_channels(&self) -> usize {
        self.0.buffer.num_channels()
    }
    fn channel(&self, ch: usize) -> Option<&[f32]> {
        self.0.channel(ch)
    }
}

fn insert(app: &SoundApp, track: TrackId) -> Result<Insert, String> {
    app.engine
        .session()
        .track(track)
        .filter(|t| t.kind == soundcraft_model::TrackKind::Instrument)
        .and_then(|t| t.instrument.clone())
        .ok_or_else(|| "Choose an instrument track".into())
}
fn current_patch(ins: &Insert) -> Arc<Patch> {
    let patch = ins.hybrid.clone().unwrap_or_else(|| Arc::new(Patch::default()));
    let mut design = patch.design().clone();
    for (i, m) in design.macros.iter_mut().enumerate() {
        if let Some(v) = hybrid::MACROS.get(i).and_then(|p| ins.params.get(p.id)).copied() {
            m.value = v;
        }
    }
    if design == *patch.design() { patch } else { Patch::compile(design).map(Arc::new).unwrap_or(patch) }
}
fn track(app: &SoundApp, p: &Value) -> Result<TrackId, String> {
    if let Some(id) = p.get("track").and_then(Value::as_u64) {
        return Ok(TrackId(id));
    }
    if let Some(name) = p.get("track").and_then(Value::as_str) {
        return app.engine.session().tracks.iter().find(|t| t.name == name).map(|t| t.id).ok_or_else(|| "Track not found".into());
    }
    app.engine.session().edit.selected_tracks.first().copied().ok_or_else(|| "Select an instrument track".into())
}
pub fn summary(app: &SoundApp) -> Value {
    json!({"active_track":app.sound_lab.active,"candidates":app.sound_lab.drafts.keys().collect::<Vec<_>>(),"rendering":app.sound_lab.job.is_some(),"commands":"Discover synth.catalog and soundlab.* for bounded hybrid sound design. Preview first; commit explicitly."})
}
pub fn params(id: &str) -> Option<&'static str> {
    Some(match id {
        "soundlab.close" => "{track?}: close the FORGE editor, stop auditions and restore the committed live sound",
        "soundlab.new" => "{}: create a playable FORGE instrument track and open its editor",
        "soundlab.open" => "{track?:id/name}: open FORGE on an instrument track; installs starter only when explicitly requested with create:true",
        "soundlab.inspect" => "{track?:id/name}: current patch, candidate, base_revision, render settings and real measurements",
        "soundlab.cables" => {
            "{track?,key?:string,offset?:[x,y],reset?:bool}: inspect or reposition a cable drape; sockets, DSP, document and Undo remain unchanged"
        }
        "soundlab.preview" => {
            "{track,base_revision:u64,patch:complete graph}: validate a candidate without editing the document; revision must match inspect"
        }
        "soundlab.commit" => "{track}: make the current candidate one undoable instrument edit; refuses stale base",
        "soundlab.discard" => "{track}: discard candidate and restore committed sound",
        "soundlab.render" | "soundlab.audition" => {
            "{track,settings?:{note:0..127,velocity:1..127,gate:.05..8,tail:0..8,sample_rate:8000..192000,macros:[0..1;4]},committed?:bool}: start cancellable worker; audition additionally plays a finite preview at conservative gain; inspect polls result"
        }
        "soundlab.to_sampler" | "soundlab.to_audio" => {
            "{track,settings?:...}: render committed patch into a NEW sampler/audio track, preserving original; commit candidate first; one Undo"
        }
        "soundlab.stop" | "soundlab.cancel" => "{}: stop finite audition and cancel active rendering",
        "soundlab.export" => "{track,path}: write the matching completed preview as WAV; refuses existing files",
        "soundlab.preset_save" | "soundlab.preset_load" => {
            "{track,path}: save/load a portable FORGE JSON patch; save refuses existing files, load creates a candidate"
        }
        _ => return None,
    })
}
pub fn command(app: &mut SoundApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    params(id)?;
    let result = run(app, id, p);
    if let Err(e) = &result {
        app.ui.status = e.clone();
        app.sound_lab.error = e.clone();
    }
    Some(result)
}
fn run(app: &mut SoundApp, id: &str, p: &Value) -> Result<Value, String> {
    if matches!(id, "soundlab.stop" | "soundlab.cancel") {
        stop(app);
        return Ok(json!({"stopped":true}));
    }
    if id == "soundlab.new" {
        app.run("workspace.instrumentTrack", json!({"plugin":hybrid::ID}))?;
        return run(app, "soundlab.open", &json!({}));
    }
    let tid = track(app, p)?;
    let base = insert(app, tid)?;
    match id {
        "soundlab.close" => {
            stop(app);
            app.live_midi.reset();
            app.ui.plugin_windows.retain(|(t, slot)| *t != tid || *slot != soundcraft_mix::INSTRUMENT_SLOT);
            if app.sound_lab.active == Some(tid) {
                app.sound_lab.active = None;
                app.sound_lab.audio_dirty = true;
            }
            Ok(json!({"closed":true}))
        }
        "soundlab.cables" => {
            if app.sound_lab.active != Some(tid) {
                return Err("Open this FORGE instrument before arranging its patch leads".into());
            }
            let patch = wanted(app, tid, &base);
            app.sound_lab.cable_layout.command(patch.design(), p)
        }
        "soundlab.open" => {
            if base.plugin != hybrid::ID {
                if p.get("create").and_then(Value::as_bool) != Some(true) {
                    return Err(
                        "This track uses a different instrument. Use create:true to replace it with the FORGE starter (Undo supported)".into()
                    );
                }
                app.engine.execute("synth.patch", &json!({"track":tid.0,"patch":Patch::default()})).map_err(|e| e.to_string())?;
            }
            let slot = soundcraft_mix::INSTRUMENT_SLOT;
            if !app.ui.plugin_windows.contains(&(tid, slot)) {
                app.ui.plugin_windows.push((tid, slot));
            }
            if app.sound_lab.active != Some(tid) {
                app.sound_lab.cable_layout = Default::default();
                stop(app);
                app.live_midi.reset();
                app.sound_lab.audio_dirty = true;
            }
            app.sound_lab.active = Some(tid);
            Ok(json!({"open":true,"track":tid}))
        }
        "soundlab.inspect" => {
            let draft = app.sound_lab.drafts.get(&tid);
            let ready = app.sound_lab.ready.as_ref().filter(|r| r.track == tid);
            Ok(
                json!({"track":tid,"base_revision":app.engine.revision,"committed":current_patch(&base),"candidate":draft.map(|d|&d.patch),"candidate_stale":draft.is_some_and(|d|d.base!=base),"settings":app.sound_lab.settings,"render_settings":ready.map(|r|r.settings),"rendering":app.sound_lab.job.as_ref().is_some_and(|j|j.track==tid),"measurements":ready.map(|r|&r.metrics),"preview_stale":ready.is_some_and(|r|r.base!=base||r.patch!=wanted(app,tid,&base)),"error":app.sound_lab.error}),
            )
        }
        "soundlab.preview" => {
            if p.get("base_revision").and_then(Value::as_u64) != Some(app.engine.revision) {
                return Err("Session changed: inspect the patch again before building a candidate".into());
            }
            let patch = soundcraft_engine::cmd::hybrid::parse(p.get("patch").ok_or("Complete patch required")?)?;
            preview(app, tid, base, patch)?;
            Ok(json!({"candidate":true,"track":tid,"base_revision":app.engine.revision}))
        }
        "soundlab.commit" => {
            let draft = app.sound_lab.drafts.get(&tid).ok_or("No candidate to commit")?;
            if draft.base != base {
                return Err("The instrument changed after this candidate was built. Inspect and rebuild it before committing".into());
            }
            let patch = draft.patch.clone();
            app.engine.execute("synth.patch", &json!({"track":tid.0,"patch":patch})).map_err(|e| e.to_string())?;
            let committed = insert(app, tid)?;
            if let Some(ready) = app.sound_lab.ready.as_mut().filter(|r| r.track == tid && r.patch == patch) {
                ready.base = committed;
            }
            app.sound_lab.drafts.remove(&tid);
            app.sound_lab.audio_dirty = true;
            stop(app);
            Ok(json!({"committed":true,"track":tid}))
        }
        "soundlab.discard" => {
            app.sound_lab.drafts.remove(&tid);
            app.sound_lab.audio_dirty = true;
            stop(app);
            Ok(json!({"discarded":true}))
        }
        "soundlab.render" | "soundlab.audition" | "soundlab.to_sampler" | "soundlab.to_audio" => {
            if app.engine.transport.recording {
                return Err("Stop recording before auditioning or rendering a patch".into());
            }
            let derive = matches!(id, "soundlab.to_sampler" | "soundlab.to_audio");
            if derive && app.sound_lab.drafts.contains_key(&tid) {
                return Err("Press STORE or REVERT in FORGE before creating a sampler/audio track".into());
            }
            let patch = if p.get("committed").and_then(Value::as_bool) == Some(true) { current_patch(&base) } else { wanted(app, tid, &base) };
            let mut settings = if let Some(v) = p.get("settings") {
                serde_json::from_value(v.clone()).map_err(|e| format!("Render settings: {e}"))?
            } else {
                app.sound_lab.settings
            };
            if p.get("settings").is_none_or(|v| v.get("macros").is_none()) {
                settings.macros = patch.design().macros.clone().map(|m| m.value);
            }
            if p.get("settings").is_none_or(|v| v.get("level").is_none()) {
                settings.level = base.params.get("level").copied().unwrap_or(-6.0);
            }
            if derive {
                settings.sample_rate = app.engine.session().sample_rate.hz();
            }
            settings.frames()?;
            let sample = base.sample.and_then(|s| app.engine.session().pool.get(s).cloned()).map(|s| Arc::new(Sample(s)) as Arc<dyn SampleSource>);
            if patch.design().modules.iter().any(|m| m.kind == hybrid::Kind::Sample) && sample.is_none() {
                return Err("This patch's sample source is unavailable".into());
            }
            stop(app);
            let cancel = Arc::new(AtomicBool::new(false));
            let rx = spawn_render(patch.clone(), settings, sample, cancel.clone())?;
            app.sound_lab.error.clear();
            app.sound_lab.settings = settings;
            app.sound_lab.job = Some(Job {
                track: tid,
                patch,
                base,
                settings,
                action: id.into(),
                committed: p.get("committed").and_then(Value::as_bool) == Some(true) || derive,
                cancel,
                rx,
            });
            Ok(json!({"started":true,"track":tid,"action":id,"render_sample_rate":settings.sample_rate}))
        }
        "soundlab.export" => {
            let ready = app
                .sound_lab
                .ready
                .as_ref()
                .filter(|r| r.track == tid && r.base == base && r.patch == wanted(app, tid, &base))
                .ok_or("Render the current patch first")?;
            let path = p.get("path").and_then(Value::as_str).ok_or("Export path required")?;
            let bytes = soundcraft_audio_io::encode(&ready.audio, &soundcraft_audio_io::EncodeOptions::default()).map_err(|e| e.to_string())?;
            write_new(path, &bytes)?;
            Ok(json!({"path":path,"frames":ready.audio.frames()}))
        }
        "soundlab.preset_save" => {
            let path = p.get("path").and_then(Value::as_str).ok_or("Preset path required")?;
            let patch = wanted(app, tid, &base);
            if patch.design().modules.iter().any(|m| m.kind == hybrid::Kind::Sample) {
                return Err("Sample patches travel with the session; portable presets currently require synthesized sources".into());
            }
            let bytes = serde_json::to_vec_pretty(&patch).map_err(|e| e.to_string())?;
            write_new(path, &bytes)?;
            Ok(json!({"path":path}))
        }
        "soundlab.preset_load" => {
            let path = p.get("path").and_then(Value::as_str).ok_or("Preset path required")?;
            let bytes = soundcraft_engine::io::read_file(path).map_err(|e| e.to_string())?;
            if bytes.len() > 256_000 {
                return Err("Patch file exceeds 256KB".into());
            }
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            let patch = soundcraft_engine::cmd::hybrid::parse(&value)?;
            preview(app, tid, base, patch)?;
            Ok(json!({"candidate":true}))
        }
        _ => Err("Unknown sound lab command".into()),
    }
}
fn write_new(path: &str, bytes: &[u8]) -> Result<(), String> {
    soundcraft_engine::file_access::check_write(path)?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| format!("Cannot create file (existing files are protected): {e}"))?;
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())
    }
    #[cfg(target_arch = "wasm32")]
    {
        soundcraft_engine::io::write_file(path, bytes).map_err(|e| e.to_string())
    }
}
fn wanted(app: &SoundApp, tid: TrackId, base: &Insert) -> Arc<Patch> {
    app.sound_lab.drafts.get(&tid).filter(|d| d.base == *base).map(|d| d.patch.clone()).unwrap_or_else(|| current_patch(base))
}
fn preview(app: &mut SoundApp, tid: TrackId, base: Insert, patch: Arc<Patch>) -> Result<(), String> {
    if base.plugin != hybrid::ID {
        return Err("Choose a FORGE instrument, or create one with soundlab.new before building a patch".into());
    }
    if app.engine.transport.recording || app.live_midi.recording() {
        return Err("Stop recording before changing the preview patch".into());
    }
    if patch.design().modules.iter().any(|m| m.kind == hybrid::Kind::Sample) && base.sample.is_none() {
        return Err("Load a sample source before adding a Sample module".into());
    }
    if app.sound_lab.drafts.len() >= 32 && !app.sound_lab.drafts.contains_key(&tid) {
        return Err("Press REVERT on an unused FORGE candidate before opening another (32 maximum)".into());
    }
    stop(app);
    if app.sound_lab.active != Some(tid) {
        app.sound_lab.cable_layout = Default::default();
        app.live_midi.reset();
    }
    app.sound_lab.drafts.insert(tid, Draft { patch, base });
    app.ui.plugin_windows.retain(|(t, slot)| *t != tid || *slot != soundcraft_mix::INSTRUMENT_SLOT);
    app.ui.plugin_windows.push((tid, soundcraft_mix::INSTRUMENT_SLOT));
    app.sound_lab.active = Some(tid);
    app.sound_lab.audio_dirty = true;
    app.sound_lab.error.clear();
    Ok(())
}
pub fn stop(app: &mut SoundApp) {
    if let Some(j) = app.sound_lab.job.take() {
        j.cancel.store(true, Ordering::Release);
    }
    if let Some(p) = &app.player {
        p.stop_audition();
    }
}
pub fn tick(app: &mut SoundApp, ctx: &egui::Context) {
    let active = app.sound_lab.active;
    if active.is_some_and(|t| !app.ui.plugin_windows.contains(&(t, soundcraft_mix::INSTRUMENT_SLOT)) || insert(app, t).is_err()) {
        stop(app);
        app.live_midi.reset();
        app.sound_lab.active = None;
        app.sound_lab.audio_dirty = true;
    }
    let result = app.sound_lab.job.as_ref().and_then(|j| match j.rx.try_recv() {
        Ok(r) => Some(r),
        Err(mpsc::TryRecvError::Disconnected) => Some(Err("Synth worker disconnected".into())),
        Err(mpsc::TryRecvError::Empty) => None,
    });
    if app.sound_lab.job.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
    }
    let Some(result) = result else { return };
    let Some(job) = app.sound_lab.job.take() else { return };
    let result: Result<(), String> = (|| {
        let base = insert(app, job.track)?;
        let expected = if job.committed { current_patch(&base) } else { wanted(app, job.track, &base) };
        if base != job.base || expected != job.patch {
            return Err("Render discarded because its source patch changed".into());
        }
        let rendered = result?;
        let audio = Arc::new(AudioBuffer { sample_rate: job.settings.sample_rate, channels: rendered.audio.channels });
        if job.action.starts_with("soundlab.to_") {
            let v = soundcraft_engine::cmd::hybrid::derive(
                &mut app.engine,
                job.track,
                (*audio).clone(),
                &job.patch.design().name,
                job.settings.note,
                job.action == "soundlab.to_sampler",
            )?;
            if let Some(t) = v.get("track").and_then(Value::as_u64) {
                app.ui.plugin_windows.push((TrackId(t), soundcraft_mix::INSTRUMENT_SLOT));
            }
            app.ui.status = "Created a separate playable track; the original FORGE patch is preserved. Undo removes the derived track.".into();
        }
        if job.action == "soundlab.audition" {
            app.player
                .as_ref()
                .ok_or("Connect an output device to audition")?
                .audition(audio.clone(), soundcraft_dsp::db_to_gain(app.sound_lab.preview_level))?;
        }
        app.sound_lab.ready =
            Some(Ready { track: job.track, patch: job.patch, base: job.base, settings: job.settings, audio, metrics: rendered.metrics });
        Ok(())
    })();
    if let Err(e) = result {
        app.ui.status = e.clone();
        app.sound_lab.error = e;
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn spawn_render(
    patch: Arc<Patch>,
    settings: Settings,
    sample: Option<Arc<dyn SampleSource>>,
    cancel: Arc<AtomicBool>,
) -> Result<mpsc::Receiver<Result<render::Rendered, String>>, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("forge-render".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| render::render(patch, settings, sample, &cancel)))
                .unwrap_or_else(|_| Err("Synth worker failed; the project remains unchanged".into()));
            let _ = tx.send(result);
        })
        .map_err(|e| format!("Cannot start synth worker: {e}"))?;
    Ok(rx)
}
#[cfg(target_arch = "wasm32")]
fn spawn_render(
    _patch: Arc<Patch>,
    _settings: Settings,
    _sample: Option<Arc<dyn SampleSource>>,
    _cancel: Arc<AtomicBool>,
) -> Result<mpsc::Receiver<Result<render::Rendered, String>>, String> {
    Err("Offline FORGE rendering requires the native editor".into())
}
impl SoundApp {
    pub(crate) fn audible_session(&self) -> Arc<soundcraft_model::Session> {
        let base = self.engine.session_arc();
        let Some(tid) = self.sound_lab.active else { return base };
        let Some(draft) = self.sound_lab.drafts.get(&tid) else { return base };
        if base.track(tid).and_then(|t| t.instrument.as_ref()) != Some(&draft.base) {
            return base;
        }
        let mut session = (*base).clone();
        if let Some(t) = session.track_mut(tid) {
            let mut ins = draft.base.clone();
            ins.plugin = hybrid::ID.into();
            ins.hybrid = Some(draft.patch.clone());
            for (i, m) in draft.patch.design().macros.iter().enumerate() {
                if let Some(p) = hybrid::MACROS.get(i) {
                    ins.params.insert(p.id.into(), m.value);
                }
            }
            t.instrument = Some(ins);
        }
        Arc::new(session)
    }
}
