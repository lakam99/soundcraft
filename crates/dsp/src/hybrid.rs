//! FORGE: a bounded, compiled modular patch. Graph compilation happens outside audio processing.
use crate::{
    ParamInfo, Unit,
    params::{choice, lin, log},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod render;
mod voice;
pub use voice::HybridSynth;
pub const ID: &str = "hybrid_synth";
pub const MAX_MODULES: usize = 16;
pub const MAX_ROUTES: usize = 48;
pub const MAX_MODULATIONS: usize = 48;
pub const MAX_PARAMS: usize = 8;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Oscillator,
    Noise,
    Sample,
    Filter,
    Saturator,
    Resonator,
    Vca,
    Output,
    Envelope,
    Lfo,
}
impl Kind {
    pub const ALL: [Self; 10] = [
        Self::Oscillator,
        Self::Noise,
        Self::Sample,
        Self::Filter,
        Self::Saturator,
        Self::Resonator,
        Self::Vca,
        Self::Output,
        Self::Envelope,
        Self::Lfo,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Oscillator => "Oscillator",
            Self::Noise => "Noise",
            Self::Sample => "Sample",
            Self::Filter => "Filter",
            Self::Saturator => "Drive",
            Self::Resonator => "Resonator",
            Self::Vca => "Amplifier",
            Self::Output => "Output",
            Self::Envelope => "Envelope",
            Self::Lfo => "LFO",
        }
    }
    pub fn control(self) -> bool {
        matches!(self, Self::Envelope | Self::Lfo)
    }
    pub fn accepts_audio(self) -> bool {
        matches!(self, Self::Oscillator | Self::Filter | Self::Saturator | Self::Resonator | Self::Vca | Self::Output)
    }
    pub fn params(self) -> &'static [ParamInfo] {
        match self {
            Self::Oscillator => &OSC,
            Self::Noise => &NOISE,
            Self::Sample => &SAMPLE,
            Self::Filter => &FILTER,
            Self::Saturator => &DRIVE,
            Self::Resonator => &BODY,
            Self::Vca => &VCA,
            Self::Output => &OUTPUT,
            Self::Envelope => &ENV,
            Self::Lfo => &LFO,
        }
    }
}
const OSC: [ParamInfo; 7] = [
    choice("shape", "Wave", &["Sine", "Triangle", "Saw", "Square", "Wavetable", "Additive"], 0),
    lin("pitch", "Pitch", -48.0, 48.0, 0.0, Unit::Semitones),
    lin("detune", "Detune", -100.0, 100.0, 0.0, Unit::Cents),
    lin("level", "Level", -60.0, 0.0, -6.0, Unit::Db),
    lin("morph", "Morph", 0.0, 1.0, 0.3, Unit::None),
    lin("harmonics", "Partials", 1.0, 16.0, 8.0, Unit::None),
    lin("fm", "FM depth", 0.0, 5000.0, 0.0, Unit::Hz),
];
const NOISE: [ParamInfo; 2] = [choice("color", "Color", &["White", "Pink"], 0), lin("level", "Level", -60.0, 0.0, -24.0, Unit::Db)];
const SAMPLE: [ParamInfo; 3] = [
    lin("root", "Root note", 0.0, 127.0, 60.0, Unit::None),
    lin("pitch", "Pitch", -48.0, 48.0, 0.0, Unit::Semitones),
    lin("level", "Level", -60.0, 0.0, -6.0, Unit::Db),
];
const FILTER: [ParamInfo; 3] = [
    choice("mode", "Mode", &["Low pass", "High pass", "Band pass"], 0),
    log("cutoff", "Cutoff", 20.0, 20000.0, 2500.0, Unit::Hz),
    log("q", "Resonance", 0.5, 12.0, 0.7, Unit::None),
];
const DRIVE: [ParamInfo; 2] = [lin("drive", "Drive", 1.0, 16.0, 1.5, Unit::None), lin("mix", "Mix", 0.0, 1.0, 0.25, Unit::None)];
const BODY: [ParamInfo; 3] = [
    log("ratio", "Tune ratio", 0.25, 16.0, 1.0, Unit::Ratio),
    log("decay", "Decay", 10.0, 3000.0, 300.0, Unit::Ms),
    lin("mix", "Mix", 0.0, 1.0, 0.5, Unit::None),
];
const VCA: [ParamInfo; 2] = [lin("gain", "Gain", 0.0, 1.0, 0.0, Unit::None), lin("level", "Level", -60.0, 0.0, 0.0, Unit::Db)];
const OUTPUT: [ParamInfo; 2] = [lin("level", "Level", -48.0, 0.0, -6.0, Unit::Db), lin("pan", "Pan", -1.0, 1.0, 0.0, Unit::None)];
const ENV: [ParamInfo; 4] = [
    log("attack", "Attack", 0.5, 5000.0, 12.0, Unit::Ms),
    log("decay", "Decay", 1.0, 5000.0, 220.0, Unit::Ms),
    lin("sustain", "Sustain", 0.0, 1.0, 0.7, Unit::None),
    log("release", "Release", 1.0, 8000.0, 350.0, Unit::Ms),
];
const LFO: [ParamInfo; 3] = [
    log("rate", "Rate", 0.02, 40.0, 1.0, Unit::Hz),
    choice("shape", "Shape", &["Sine", "Triangle", "Square"], 0),
    lin("depth", "Depth", 0.0, 1.0, 1.0, Unit::None),
];
pub static MACROS: [ParamInfo; 5] = [
    lin("macro1", "Character 1", 0.0, 1.0, 0.5, Unit::None),
    lin("macro2", "Character 2", 0.0, 1.0, 0.5, Unit::None),
    lin("macro3", "Character 3", 0.0, 1.0, 0.5, Unit::None),
    lin("macro4", "Character 4", 0.0, 1.0, 0.5, Unit::None),
    lin("level", "Master", -48.0, 0.0, -6.0, Unit::Db),
];
pub static INFO: crate::PluginInfo = crate::PluginInfo {
    id: ID,
    name: "FORGE Hybrid",
    short_name: "FORGE",
    category: crate::Category::Instrument,
    params: &MACROS,
    is_instrument: true,
    audiosuite: false,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Module {
    pub id: String,
    pub kind: Kind,
    #[serde(default)]
    pub params: BTreeMap<String, f32>,
}
impl Module {
    pub fn new(id: &str, kind: Kind) -> Self {
        Self { id: id.into(), kind, params: kind.params().iter().map(|p| (p.id.into(), p.default)).collect() }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub from: String,
    pub to: String,
    #[serde(default = "unity")]
    pub gain: f32,
}
fn unity() -> f32 {
    1.0
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "id")]
pub enum Source {
    Module(String),
    Macro(u8),
    Velocity,
    Key,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Modulation {
    pub source: Source,
    pub target: String,
    pub param: String,
    pub amount: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Macro {
    pub name: String,
    pub value: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub modules: Vec<Module>,
    pub routes: Vec<Route>,
    #[serde(default)]
    pub modulation: Vec<Modulation>,
    pub macros: [Macro; 4],
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct Patch {
    design: Design,
    plan: Plan,
}
#[derive(Debug, Clone)]
struct Plan {
    order: Vec<usize>,
    nodes: Vec<Node>,
    output: usize,
    release: f32,
}
#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    values: [f32; MAX_PARAMS],
    audio: Vec<(usize, f32)>,
    mods: Vec<Mod>,
}
#[derive(Debug, Clone, Copy)]
enum Input {
    Node(usize),
    Macro(usize),
    Velocity,
    Key,
}
#[derive(Debug, Clone, Copy)]
struct Mod {
    source: Input,
    param: usize,
    amount: f32,
}
impl PartialEq for Patch {
    fn eq(&self, other: &Self) -> bool {
        self.design == other.design
    }
}
impl Serialize for Patch {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.design.serialize(s)
    }
}
impl<'de> Deserialize<'de> for Patch {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let design = Design::deserialize(d)?;
        Self::compile(design).map_err(serde::de::Error::custom)
    }
}
impl Default for Patch {
    fn default() -> Self {
        Self::starter()
    }
}
impl Patch {
    pub fn design(&self) -> &Design {
        &self.design
    }
    pub fn compile(design: Design) -> Result<Self, String> {
        if design.name.is_empty() || design.name.len() > 128 || design.description.len() > 4096 {
            return Err("Patch needs a name (128 bytes max) and a description up to 4096 bytes".into());
        }
        let count = design.modules.len();
        if count == 0 || count > MAX_MODULES || design.routes.len() > MAX_ROUTES || design.modulation.len() > MAX_MODULATIONS {
            return Err("Patch limit: 16 modules, 48 audio routes and 48 modulations".into());
        }
        for m in &design.macros {
            if m.name.len() > 40 || !m.value.is_finite() || !(0.0..=1.0).contains(&m.value) {
                return Err("Macros require short names and values from 0 to 1".into());
            }
        }
        let mut names = BTreeMap::new();
        let mut nodes = Vec::new();
        let mut release = 0.1f32;
        for (i, module) in design.modules.iter().enumerate() {
            if module.id.is_empty()
                || module.id.len() > 48
                || !module.id.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-'))
                || names.insert(module.id.clone(), i).is_some()
            {
                return Err("Module IDs must be unique short letters/numbers/underscores".into());
            }
            if module.params.len() > MAX_PARAMS {
                return Err("Too many module parameters".into());
            }
            for (id, value) in &module.params {
                let Some(p) = module.kind.params().iter().find(|p| p.id == id) else {
                    return Err(format!("Unknown parameter {id} on {}", module.id));
                };
                if !value.is_finite() || *value < p.min || *value > p.max {
                    return Err(format!("{}.{id} is outside {}..{}", module.id, p.min, p.max));
                }
            }
            let mut values = [0.0; MAX_PARAMS];
            for (j, p) in module.kind.params().iter().enumerate() {
                if let Some(v) = values.get_mut(j) {
                    *v = p.clamp(module.params.get(p.id).copied().unwrap_or(p.default));
                }
            }
            if module.kind == Kind::Envelope {
                release = release.max(values.get(3).copied().unwrap_or(100.0) / 1000.0);
            }
            if module.kind == Kind::Resonator {
                release = release.max(values.get(1).copied().unwrap_or(300.0) / 1000.0);
            }
            nodes.push(Node { kind: module.kind, values, audio: Vec::new(), mods: Vec::new() });
        }
        let index = |id: &str| names.get(id).copied().ok_or_else(|| format!("Unknown module {id}"));
        let output = index(&design.output)?;
        if nodes.get(output).is_none_or(|n| n.kind != Kind::Output) || nodes.iter().filter(|n| n.kind == Kind::Output).count() != 1 {
            return Err("Choose exactly one Output module".into());
        }
        let mut edges = vec![BTreeSet::new(); count];
        let mut unique = BTreeSet::new();
        for r in &design.routes {
            let from = index(&r.from)?;
            let to = index(&r.to)?;
            if !r.gain.is_finite() || r.gain.abs() > 2.0 || !unique.insert((from, to)) {
                return Err("Routes require unique connections and gain -2..2".into());
            }
            if nodes.get(from).is_none_or(|n| n.kind.control() || n.kind == Kind::Output) || nodes.get(to).is_none_or(|n| !n.kind.accepts_audio()) {
                return Err("Audio routes connect sound modules; use modulation for envelopes and LFOs".into());
            }
            if let Some(edge) = edges.get_mut(to) {
                edge.insert(from);
            }
            if let Some(node) = nodes.get_mut(to) {
                node.audio.push((from, r.gain));
            }
        }
        for m in &design.modulation {
            let target = index(&m.target)?;
            let kind = nodes.get(target).map(|n| n.kind).ok_or("Missing target")?;
            let param = kind
                .params()
                .iter()
                .position(|p| p.id == m.param)
                .ok_or_else(|| format!("Unknown modulation destination {}.{}", m.target, m.param))?;
            if !m.amount.is_finite() || m.amount.abs() > 1.0 || kind.params().get(param).is_some_and(|p| !p.choices.is_empty()) {
                return Err("Modulation uses continuous parameters and amount -1..1".into());
            }
            // CV can lengthen a release/ringing tail beyond its base value. Keep the voice alive
            // for the destination's bounded maximum, rather than truncating the audible tail.
            if m.amount != 0.0
                && ((kind == Kind::Envelope && m.param == "release") || (kind == Kind::Resonator && m.param == "decay"))
                && let Some(p) = kind.params().get(param)
            {
                release = release.max(p.max / 1000.0);
            }
            let source = match &m.source {
                Source::Module(id) => {
                    let from = index(id)?;
                    if let Some(edge) = edges.get_mut(target) {
                        edge.insert(from);
                    }
                    Input::Node(from)
                }
                Source::Macro(n) if *n < 4 => Input::Macro(usize::from(*n)),
                Source::Velocity => Input::Velocity,
                Source::Key => Input::Key,
                _ => return Err("Macro source must be 0..3".into()),
            };
            if let Some(node) = nodes.get_mut(target) {
                node.mods.push(Mod { source, param, amount: m.amount });
            }
        }
        let mut order = Vec::new();
        let mut done = BTreeSet::new();
        while order.len() < count {
            let next = (0..count).find(|i| !done.contains(i) && edges.get(*i).is_some_and(|deps| deps.iter().all(|d| done.contains(d))));
            let Some(i) = next else {
                return Err("Feedback cycle detected. Remove the circular audio/modulation connection".into());
            };
            done.insert(i);
            order.push(i);
        }
        // Unplugged audio paths are intentionally valid and silent, like a physical patch bay.
        Ok(Self { design, plan: Plan { order, nodes, output, release } })
    }
    pub fn starter() -> Self {
        let mut modules = vec![
            Module::new("osc_a", Kind::Oscillator),
            Module::new("filter", Kind::Filter),
            Module::new("drive", Kind::Saturator),
            Module::new("amp", Kind::Vca),
            Module::new("out", Kind::Output),
            Module::new("env", Kind::Envelope),
            Module::new("lfo", Kind::Lfo),
            Module::new("sub", Kind::Oscillator),
        ];
        if let Some(osc) = modules.first_mut() {
            osc.params.insert("shape".into(), 2.0);
        }
        if let Some(sub) = modules.iter_mut().find(|m| m.id == "sub") {
            sub.params.insert("pitch".into(), -12.0);
            sub.params.insert("level".into(), -24.0);
        }
        if let Some(lfo) = modules.iter_mut().find(|m| m.id == "lfo") {
            lfo.params.insert("depth".into(), 0.5);
        }
        if let Some(drive) = modules.iter_mut().find(|m| m.id == "drive") {
            drive.params.insert("drive".into(), 5.5);
        }
        let design = Design {
            name: "Copper Dawn".into(),
            description: "A warm playable starting point. Brightness opens the filter; Motion animates it.".into(),
            modules,
            routes: [("osc_a", "filter"), ("sub", "filter"), ("filter", "drive"), ("drive", "amp"), ("amp", "out")]
                .into_iter()
                .map(|(from, to)| Route { from: from.into(), to: to.into(), gain: 1.0 })
                .collect(),
            modulation: vec![
                Modulation { source: Source::Module("env".into()), target: "amp".into(), param: "gain".into(), amount: 1.0 },
                Modulation { source: Source::Macro(0), target: "filter".into(), param: "cutoff".into(), amount: 0.35 },
                Modulation { source: Source::Macro(1), target: "lfo".into(), param: "depth".into(), amount: 0.5 },
                Modulation { source: Source::Module("lfo".into()), target: "filter".into(), param: "cutoff".into(), amount: 0.06 },
                Modulation { source: Source::Macro(2), target: "sub".into(), param: "level".into(), amount: 0.3 },
                Modulation { source: Source::Macro(3), target: "drive".into(), param: "drive".into(), amount: 0.25 },
            ],
            macros: ["Brightness", "Motion", "Body", "Edge"].map(|name| Macro { name: name.into(), value: 0.5 }),
            output: "out".into(),
        };
        // The literal starter is known to be valid. A silent fallback still keeps the app alive.
        Self::compile(design.clone())
            .unwrap_or_else(|_| Self { design, plan: Plan { order: Vec::new(), nodes: Vec::new(), output: 0, release: 0.1 } })
    }
}
