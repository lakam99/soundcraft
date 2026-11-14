use super::{INFO, Input, Kind, MACROS, MAX_MODULES, Patch};
use crate::{
    Plugin, PluginInfo, SampleSource,
    biquad::{Biquad, FilterType},
    osc::{Osc, Waveform},
    params::Params,
    plugins::synth::{Adsr, AdsrRates, EventKind, EventQueue},
    util::{note_hz, sane_sr},
};
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

#[derive(Clone, Copy)]
struct State {
    osc: Osc,
    env: Adsr,
    rates: AdsrRates,
    filter: Biquad,
    cursor: f64,
    z1: f32,
    z2: f32,
}
impl State {
    fn new(seed: u32) -> Self {
        Self { osc: Osc::new(seed), env: Adsr::new(), rates: AdsrRates::default(), filter: Biquad::default(), cursor: 0.0, z1: 0.0, z2: 0.0 }
    }
}
struct Voice {
    active: bool,
    note: u8,
    velocity: f32,
    down: bool,
    gate: bool,
    age: u64,
    released: u64,
    states: [State; MAX_MODULES],
    values: [f32; MAX_MODULES],
}
impl Voice {
    fn new() -> Self {
        Self {
            active: false,
            note: 60,
            velocity: 0.0,
            down: false,
            gate: false,
            age: 0,
            released: 0,
            states: std::array::from_fn(|i| State::new(i as u32 + 17)),
            values: [0.0; MAX_MODULES],
        }
    }
}
pub struct HybridSynth {
    patch: Arc<Patch>,
    params: Params,
    voices: [Voice; 16],
    events: EventQueue,
    sample: Option<Arc<dyn SampleSource>>,
    sr: f32,
    pedal: bool,
    bend: f32,
    tick: u64,
}
impl Default for HybridSynth {
    fn default() -> Self {
        Self::new()
    }
}
impl HybridSynth {
    pub fn new() -> Self {
        Self {
            patch: Arc::new(Patch::default()),
            params: Params::new(&MACROS),
            voices: std::array::from_fn(|_| Voice::new()),
            events: EventQueue::new(),
            sample: None,
            sr: 48000.0,
            pedal: false,
            bend: 1.0,
            tick: 0,
        }
    }
    fn event(&mut self, event: EventKind) {
        match event {
            EventKind::On(note, velocity) => {
                let index = self
                    .voices
                    .iter()
                    .position(|v| !v.active)
                    .unwrap_or_else(|| self.voices.iter().enumerate().min_by_key(|(_, v)| v.age).map_or(0, |(i, _)| i));
                if let Some(v) = self.voices.get_mut(index) {
                    *v = Voice::new();
                    v.active = true;
                    v.note = note;
                    v.velocity = f32::from(velocity) / 127.0;
                    v.down = true;
                    v.gate = true;
                    v.age = self.tick;
                    for state in &mut v.states {
                        state.env.gate_on();
                    }
                }
            }
            EventKind::Off(note) => {
                for v in self.voices.iter_mut().filter(|v| v.active && v.note == note) {
                    v.down = false;
                    if !self.pedal {
                        release(v);
                    }
                }
            }
            EventKind::Sustain(on) => {
                self.pedal = on;
                if !on {
                    for v in self.voices.iter_mut().filter(|v| v.active && !v.down) {
                        release(v);
                    }
                }
            }
            EventKind::Bend(ratio) => self.bend = ratio,
            EventKind::AllOff => {
                for v in &mut self.voices {
                    v.active = false;
                }
                self.pedal = false;
            }
            EventKind::ResetControllers => {
                self.pedal = false;
                self.bend = 1.0;
                for v in self.voices.iter_mut().filter(|v| v.active && !v.down) {
                    release(v);
                }
            }
        }
    }
}
fn release(v: &mut Voice) {
    v.gate = false;
    v.released = 0;
    for s in &mut v.states {
        s.env.gate_off();
    }
}

impl Plugin for HybridSynth {
    fn info(&self) -> &'static PluginInfo {
        &INFO
    }
    fn prepare(&mut self, sample_rate: f32, _max_block: usize, _channels: usize) {
        self.sr = sane_sr(sample_rate);
        self.reset();
    }
    fn reset(&mut self) {
        for v in &mut self.voices {
            v.active = false;
        }
        self.events.clear();
        self.pedal = false;
        self.bend = 1.0;
    }
    fn set_param(&mut self, id: &str, value: f32) -> bool {
        self.params.set(id, value).is_some()
    }
    fn param(&self, id: &str) -> Option<f32> {
        self.params.get(id)
    }
    fn set_hybrid_patch(&mut self, patch: Arc<Patch>) -> bool {
        if !Arc::ptr_eq(&self.patch, &patch) {
            let same = self.patch.design().modules.iter().map(|m| (&m.id, m.kind)).eq(patch.design().modules.iter().map(|m| (&m.id, m.kind)));
            self.patch = patch;
            if !same {
                self.reset();
            }
        }
        true
    }
    fn set_sample(&mut self, sample: Option<Arc<dyn SampleSource>>) -> bool {
        self.sample = sample;
        self.reset();
        true
    }
    fn note_on(&mut self, offset: usize, note: u8, velocity: u8) {
        if note <= 127 && velocity <= 127 {
            self.events.push(offset, if velocity == 0 { EventKind::Off(note) } else { EventKind::On(note, velocity) });
        }
    }
    fn note_off(&mut self, offset: usize, note: u8) {
        self.events.push(offset, EventKind::Off(note));
    }
    fn all_notes_off(&mut self) {
        self.reset();
    }
    fn midi(&mut self, offset: usize, msg: [u8; 3]) {
        let [s, a, b] = msg;
        let event = match (s & 0xf0, a) {
            (0xb0, 64) => EventKind::Sustain(b >= 64),
            (0xb0, 120 | 123) => EventKind::AllOff,
            (0xb0, 121) => EventKind::ResetControllers,
            (0xe0, _) => EventKind::Bend(2f32.powf((((i32::from(b & 127) << 7) + i32::from(a & 127) - 8192) as f32 / 8192.0) * 2.0 / 12.0)),
            _ => return,
        };
        self.events.push(offset, event);
    }
    fn process(&mut self, io: &mut [Vec<f32>], frames: usize) {
        let count = io.iter().map(Vec::len).min().unwrap_or(0).min(frames);
        for channel in io.iter_mut() {
            channel.iter_mut().take(count).for_each(|v| *v = 0.0);
        }
        let mut ordered = [None; crate::plugins::synth::MAX_EVENTS];
        for (slot, event) in ordered.iter_mut().zip(self.events.sorted()) {
            *slot = Some(*event);
        }
        self.events.clear();
        let mut next = 0;
        let macros = std::array::from_fn(|i| self.params.v(i));
        let master = crate::db_to_gain(self.params.v(4));
        for frame in 0..count {
            while let Some(event) = ordered.get(next).and_then(Option::as_ref).filter(|event| event.offset <= frame) {
                self.event(event.kind);
                next += 1;
            }
            let mut out = [0.0f32; 2];
            for voice in self.voices.iter_mut().filter(|v| v.active) {
                let signal = render_voice(voice, &self.patch, &macros, self.sample.as_deref(), self.sr, self.bend, self.tick);
                out[0] += signal[0];
                out[1] += signal[1];
            }
            for (ch, channel) in io.iter_mut().enumerate() {
                if let Some(value) = channel.get_mut(frame) {
                    let x = if ch < 2 { out.get(ch).copied().unwrap_or(0.0) } else { 0.0 } * master;
                    *value = if x.is_finite() { x / (1.0 + x.abs()) } else { 0.0 };
                }
            }
            self.tick = self.tick.wrapping_add(1);
        }
    }
}

fn render_voice(v: &mut Voice, patch: &Patch, macros: &[f32; 4], sample: Option<&dyn SampleSource>, sr: f32, bend: f32, tick: u64) -> [f32; 2] {
    if !v.gate {
        v.released = v.released.saturating_add(1);
        if v.released as f32 > sr * (patch.plan.release + 0.12) {
            v.active = false;
            return [0.0; 2];
        }
    }
    let hz = note_hz(f32::from(v.note)) * bend;
    let mut pan = 0.0;
    for &index in &patch.plan.order {
        let Some(node) = patch.plan.nodes.get(index) else {
            continue;
        };
        let input = node.audio.iter().map(|(i, g)| v.values.get(*i).copied().unwrap_or(0.0) * g).sum::<f32>().clamp(-16.0, 16.0);
        let mut values = node.values;
        for m in &node.mods {
            let source = match m.source {
                Input::Node(i) => v.values.get(i).copied().unwrap_or(0.0),
                Input::Macro(i) => macros.get(i).copied().unwrap_or(0.5) * 2.0 - 1.0,
                Input::Velocity => v.velocity,
                Input::Key => (f32::from(v.note) - 60.0) / 60.0,
            };
            if let (Some(info), Some(value)) = (node.kind.params().get(m.param), values.get_mut(m.param)) {
                *value = info.from_normalized(info.to_normalized(*value) + source.clamp(-1.0, 1.0) * m.amount);
            }
        }
        let p = |i: usize| values.get(i).copied().unwrap_or(0.0);
        let Some(state) = v.states.get_mut(index) else {
            continue;
        };
        let value = match node.kind {
            Kind::Oscillator => {
                let freq = (hz * 2f32.powf((p(1) + p(2) / 100.0) / 12.0) + input * p(6)).clamp(1.0, sr * 0.45);
                let inc = freq / sr;
                let phase = state.osc.phase;
                let wave = match p(0) as u8 {
                    1 => Waveform::Triangle,
                    2 => Waveform::Saw,
                    3 => Waveform::Square,
                    _ => Waveform::Sine,
                };
                let mut alt = state.osc;
                let base = state.osc.next(wave, inc);
                let raw = match p(0) as u8 {
                    4 => base * (1.0 - p(4)) + alt.next(Waveform::Saw, inc) * p(4),
                    5 => {
                        let mut sum = 0.0;
                        let mut weight = 0.0;
                        for h in 1..=16 {
                            if h as f32 > p(5).round() || freq * h as f32 >= sr * 0.45 {
                                break;
                            }
                            let a = 1.0 / (h as f32).powf(0.5 + p(4) * 2.5);
                            sum += (phase * TAU * h as f32).sin() * a;
                            weight += a;
                        }
                        sum / weight.max(1.0)
                    }
                    _ => base,
                };
                raw * crate::db_to_gain(p(3))
            }
            Kind::Noise => state.osc.next(if p(0) >= 0.5 { Waveform::PinkNoise } else { Waveform::WhiteNoise }, 0.0) * crate::db_to_gain(p(1)),
            Kind::Sample => {
                if let Some(sample) = sample {
                    let i = state.cursor.max(0.0) as usize;
                    let frac = (state.cursor - i as f64) as f32;
                    let a = sample.channel(0).and_then(|c| c.get(i)).copied().unwrap_or(0.0);
                    let b = sample.channel(0).and_then(|c| c.get(i.saturating_add(1))).copied().unwrap_or(0.0);
                    state.cursor +=
                        (f64::from(sample.sample_rate()) / f64::from(sr)) * 2f64.powf((f64::from(v.note) - f64::from(p(0)) + f64::from(p(1))) / 12.0);
                    (a + (b - a) * frac) * crate::db_to_gain(p(2))
                } else {
                    0.0
                }
            }
            Kind::Filter => {
                if tick.is_multiple_of(32) || v.age == tick {
                    state.filter.set(
                        match p(0) as u8 {
                            1 => FilterType::HighPass,
                            2 => FilterType::BandPass,
                            _ => FilterType::LowPass,
                        },
                        p(1),
                        p(2),
                        0.0,
                        sr,
                    );
                }
                state.filter.process_sample(input)
            }
            Kind::Saturator => input * (1.0 - p(1)) + (input * p(0)).tanh() * p(1),
            Kind::Resonator => {
                let freq = (hz * p(0)).clamp(20.0, sr * 0.45);
                let r = (-6.908 / (p(1) * 0.001 * sr).max(1.0)).exp().min(0.9999);
                let y = (input + 2.0 * r * (TAU * freq / sr).cos() * state.z1 - r * r * state.z2).clamp(-128.0, 128.0);
                state.z2 = state.z1;
                state.z1 = y;
                input * (1.0 - p(2)) + (y * (1.0 - r) * 8.0).tanh() * p(2)
            }
            Kind::Vca => input * p(0) * crate::db_to_gain(p(1)),
            Kind::Output => {
                pan = p(1);
                input * crate::db_to_gain(p(0))
            }
            Kind::Envelope => {
                if tick.is_multiple_of(32) || v.age == tick {
                    state.rates = AdsrRates::new(p(0), p(1), p(2), p(3), sr);
                }
                state.env.next(&state.rates)
            }
            Kind::Lfo => {
                state.osc.next(
                    match p(1) as u8 {
                        1 => Waveform::Triangle,
                        2 => Waveform::Square,
                        _ => Waveform::Sine,
                    },
                    p(0) / sr,
                ) * p(2)
            }
        };
        if let Some(out) = v.values.get_mut(index) {
            *out = if value.is_finite() { value.clamp(-32.0, 32.0) } else { 0.0 };
        }
    }
    let signal = v.values.get(patch.plan.output).copied().unwrap_or(0.0) * v.velocity;
    let attack = ((tick.saturating_sub(v.age)) as f32 / (sr * 0.003)).min(1.0);
    let fade = if v.gate { 1.0 } else { ((patch.plan.release + 0.12) * sr - v.released as f32).clamp(0.0, sr * 0.02) / (sr * 0.02) };
    let angle = (pan + 1.0) * PI * 0.25;
    [signal * attack * fade * angle.cos(), signal * attack * fade * angle.sin()]
}
