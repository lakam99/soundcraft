//! Bounded, deterministic analog step programs, compiled to ordinary MIDI.
use crate::{Note, Sequence};
use serde::{Deserialize, Serialize};

pub const MAX_NOTES: usize = 65_536;

fn bounded<'de, D, T, const N: usize>(d: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Visitor<T, const N: usize>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for Visitor<T, N> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {N} entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(x) = a.next_element()? {
                if out.len() == N {
                    return Err(serde::de::Error::custom("sequencer array limit exceeded"));
                }
                out.push(x);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Visitor::<T, N>(std::marker::PhantomData))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Forward,
    Reverse,
    Pendulum,
    Random,
    Walk,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Melody,
    Drum,
    Arp,
    Chord,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    #[default]
    Up,
    Down,
    Pendulum,
    Memory,
    Random,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step {
    pub on: bool,
    /// Semitones relative to the lane root (ignored for drum lanes).
    pub pitch: i8,
    pub velocity: u8,
    pub gate: u8,
    pub chance: u8,
    pub ratchets: u8,
    pub tie: bool,
    pub accent: bool,
}
impl Default for Step {
    fn default() -> Self {
        Self { on: false, pitch: 0, velocity: 100, gate: 60, chance: 100, ratchets: 1, tie: false, accent: false }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Lane {
    pub name: String,
    pub mode: Mode,
    pub root: u8,
    pub drum_note: u8,
    pub transpose: i8,
    pub direction: Direction,
    #[serde(deserialize_with = "bounded::<_, _, 8>")]
    pub chord: Vec<i8>,
    pub octaves: u8,
    pub order: Order,
    pub muted: bool,
    pub solo: bool,
    #[serde(deserialize_with = "bounded::<_, _, 64>")]
    pub steps: Vec<Step>,
}
impl Default for Lane {
    fn default() -> Self {
        Self {
            name: "Voice 1".into(),
            mode: Mode::Melody,
            root: 60,
            drum_note: 36,
            transpose: 0,
            direction: Direction::Forward,
            chord: vec![0, 4, 7],
            octaves: 1,
            order: Order::Up,
            muted: false,
            solo: false,
            steps: vec![Step::default(); 16],
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pattern {
    pub name: String,
    pub length: u8,
    /// Clock division in MIDI ticks: 960, 480, 240, 120, 60 or triplets 640, 320, 160, 80.
    pub rate: u16,
    /// Delay odd steps by this percentage of one clock interval (0..75).
    pub swing: u8,
    #[serde(deserialize_with = "bounded::<_, _, 8>")]
    pub lanes: Vec<Lane>,
}
impl Default for Pattern {
    fn default() -> Self {
        Self { name: "Pattern A".into(), length: 16, rate: 240, swing: 0, lanes: vec![Lane::default()] }
    }
}
impl Pattern {
    /// A pendulum completes its outward and return trip before the next bank.
    pub fn clock_len(&self) -> usize {
        let len = usize::from(self.length);
        if self.lanes.iter().any(|l| l.direction == Direction::Pendulum) { len.saturating_mul(2).saturating_sub(2).max(1) } else { len }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Program {
    pub name: String,
    #[serde(deserialize_with = "bounded::<_, _, 8>")]
    pub banks: Vec<Pattern>,
    pub active_bank: u8,
    /// Empty plays the active bank. Entries select banks, in order, then repeat for `cycles`.
    #[serde(deserialize_with = "bounded::<_, _, 32>")]
    pub chain: Vec<u8>,
    pub cycles: u8,
    pub seed: u32,
}
impl Default for Program {
    fn default() -> Self {
        Self { name: "PULSE sequence".into(), banks: vec![Pattern::default()], active_bank: 0, chain: Vec::new(), cycles: 1, seed: 1 }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub sequence: Sequence,
    pub ticks: i64,
    pub clock: Vec<Clock>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub bank: u8,
    pub lane: u8,
    pub step: u8,
    pub start: i64,
    pub end: i64,
}

fn random(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    state.rotate_right(16)
}
fn pendulum(i: usize, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    let p = i % (2 * n - 2);
    if p < n { p } else { 2 * n - 2 - p }
}
impl Program {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.len() > 128
            || self.banks.is_empty()
            || self.banks.len() > 8
            || usize::from(self.active_bank) >= self.banks.len()
            || self.chain.len() > 32
            || self.chain.iter().any(|b| usize::from(*b) >= self.banks.len())
            || !(1..=32).contains(&self.cycles)
        {
            return Err("Invalid program: 1–8 banks, 1–32 cycles, at most 32 chain entries and valid bank indices required".into());
        }
        for b in &self.banks {
            if b.name.len() > 128
                || !(1..=64).contains(&b.length)
                || ![960, 480, 240, 120, 60, 640, 320, 160, 80].contains(&b.rate)
                || b.swing > 75
                || b.lanes.is_empty()
                || b.lanes.len() > 8
            {
                return Err("Invalid pattern length, rate, swing or lane count".into());
            }
            for l in &b.lanes {
                if l.name.len() > 128
                    || l.root > 127
                    || l.drum_note > 127
                    || !(-24..=24).contains(&l.transpose)
                    || !(1..=4).contains(&l.octaves)
                    || l.chord.is_empty()
                    || l.chord.len() > 8
                    || l.chord.iter().any(|n| !(-48..=48).contains(n))
                    || l.steps.is_empty()
                    || l.steps.len() > 64
                {
                    return Err("Invalid lane root, chord, octave range or steps".into());
                }
                for s in &l.steps {
                    if !(-48..=48).contains(&s.pitch)
                        || !(1..=127).contains(&s.velocity)
                        || !(1..=100).contains(&s.gate)
                        || s.chance > 100
                        || !(1..=8).contains(&s.ratchets)
                        || (s.tie && (s.ratchets != 1 || s.chance != 100))
                    {
                        return Err("Invalid step: pitch ±48, velocity 1–127, gate 1–100, chance 0–100, ratchets 1–8; ties require chance 100 and one ratchet".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn compile(&self) -> Result<Compiled, String> {
        self.validate()?;
        let chain = if self.chain.is_empty() { vec![self.active_bank] } else { self.chain.clone() };
        // Bound even silent programs: rests must not permit arbitrarily costly clock expansion.
        let slots: usize = chain.iter().filter_map(|b| self.banks.get(usize::from(*b))).map(|b| b.clock_len() * b.lanes.len()).sum::<usize>()
            * usize::from(self.cycles);
        if slots > 65_536 {
            return Err("Program exceeds 65536 clock slots; reduce passes, chain length or voices".into());
        }
        let mut out = Sequence::default();
        let mut clock = Vec::new();
        let mut ticks = 0i64;
        let mut rng = self.seed;
        for cycle in 0..usize::from(self.cycles) {
            for bank in &chain {
                let Some(b) = self.banks.get(usize::from(*bank)) else {
                    return Err("Bank missing".into());
                };
                let len = usize::from(b.length);
                let clocks = b.clock_len();
                let rate = i64::from(b.rate);
                let end = ticks + clocks as i64 * rate;
                let swung = |i: usize| (i as i64 * rate + if i % 2 == 1 { rate * i64::from(b.swing) / 100 } else { 0 }).min(clocks as i64 * rate);
                let solo = b.lanes.iter().any(|l| l.solo && !l.muted);
                for (lane, l) in b.lanes.iter().enumerate().filter(|(_, l)| !l.muted && (!solo || l.solo)) {
                    let mut previous: Vec<usize> = Vec::new();
                    let mut walk = 0usize;
                    let mut chord = l.chord.clone();
                    if l.order != Order::Memory {
                        chord.sort();
                        chord.dedup();
                    }
                    let pitches: Vec<i16> = (0..l.octaves).flat_map(|o| chord.iter().map(move |n| i16::from(*n) + 12 * i16::from(o))).collect();
                    for slot in 0..clocks {
                        let i = cycle * clocks + slot;
                        let index = match l.direction {
                            Direction::Forward => slot % len,
                            Direction::Reverse => len - 1 - slot % len,
                            Direction::Pendulum => pendulum(i, len),
                            Direction::Random => random(&mut rng) as usize % len,
                            Direction::Walk => {
                                let x = walk;
                                walk = if random(&mut rng) & 1 == 0 { (walk + 1) % len } else { (walk + len - 1) % len };
                                x
                            }
                        };
                        let start = ticks + swung(slot);
                        let finish = ticks + swung(slot + 1);
                        clock.push(Clock { bank: *bank, lane: lane as u8, step: index as u8, start, end: finish });
                        let Some(s) = l.steps.get(index) else {
                            previous.clear();
                            continue;
                        };
                        if !s.on {
                            previous.clear();
                            continue;
                        }
                        if s.tie {
                            for j in &previous {
                                if let Some(n) = out.notes.get_mut(*j) {
                                    n.length = (finish - 1 - n.start).max(1);
                                }
                            }
                            continue;
                        }
                        previous.clear();
                        if s.chance == 0 || (s.chance < 100 && random(&mut rng) % 100 >= u32::from(s.chance)) {
                            continue;
                        }
                        let root = i16::from(l.root) + i16::from(l.transpose) + i16::from(s.pitch);
                        let arp = match l.order {
                            Order::Up | Order::Memory => i % pitches.len(),
                            Order::Down => pitches.len() - 1 - i % pitches.len(),
                            Order::Pendulum => pendulum(i, pitches.len()),
                            Order::Random => random(&mut rng) as usize % pitches.len(),
                        };
                        let keys: Vec<i16> = match l.mode {
                            Mode::Melody => vec![root],
                            Mode::Drum => vec![i16::from(l.drum_note)],
                            Mode::Arp => vec![root + pitches.get(arp).copied().unwrap_or(0)],
                            Mode::Chord => pitches.iter().map(|p| root + p).collect(),
                        };
                        for r in 0..i64::from(s.ratchets) {
                            let at = start + (finish - start) * r / i64::from(s.ratchets);
                            let next = start + (finish - start) * (r + 1) / i64::from(s.ratchets);
                            for pitch in &keys {
                                if !(0..=127).contains(pitch) {
                                    return Err("A generated pitch falls outside MIDI 0–127; reduce root, transposition or octave span".into());
                                }
                                if out.notes.len() >= MAX_NOTES {
                                    return Err("Program exceeds 65536 notes; reduce cycles, chain, ratchets or chord voices".into());
                                }
                                previous.push(out.notes.len());
                                out.notes.push(Note {
                                    pitch: *pitch as u8,
                                    velocity: s.velocity.saturating_add(if s.accent { 20 } else { 0 }).min(127),
                                    release_velocity: 64,
                                    channel: if l.mode == Mode::Drum { 9 } else { 0 },
                                    start: at,
                                    length: ((next - at) * i64::from(s.gate) / 100).max(1).min((end - 1 - at).max(1)),
                                });
                            }
                            // A following tie extends only the final ratchet, never all repeated attacks.
                            if r + 1 < i64::from(s.ratchets) {
                                previous.clear();
                            }
                        }
                    }
                }
                ticks = end;
            }
        }
        out.sort();
        Ok(Compiled { sequence: out, ticks, clock })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arp() -> Program {
        let mut p = Program::default();
        let l = &mut p.banks[0].lanes[0];
        l.mode = Mode::Arp;
        l.steps.iter_mut().for_each(|s| s.on = true);
        p
    }
    #[test]
    fn arpeggio_is_real_chord_order_and_print_is_repeatable() {
        let p = arp();
        let a = p.compile().unwrap();
        assert_eq!(a.sequence.notes.iter().take(6).map(|n| n.pitch).collect::<Vec<_>>(), [60, 64, 67, 60, 64, 67]);
        assert_eq!(a.ticks, 3840);
        assert_eq!(a, p.compile().unwrap());
    }
    #[test]
    fn pendulum_completes_a_return_trip_even_with_one_pass() {
        let mut p = Program::default();
        let b = &mut p.banks[0];
        b.length = 3;
        let l = &mut b.lanes[0];
        l.direction = Direction::Pendulum;
        for (i, s) in l.steps.iter_mut().take(3).enumerate() {
            s.on = true;
            s.pitch = (i * 2) as i8;
        }
        let c = p.compile().unwrap();
        assert_eq!(c.sequence.notes.iter().map(|n| n.pitch).collect::<Vec<_>>(), [60, 62, 64, 62]);
        assert_eq!(c.ticks, 960);
        assert_eq!(c.clock.iter().map(|c| c.step).collect::<Vec<_>>(), [0, 1, 2, 1]);
    }
    #[test]
    fn swing_ratchets_ties_rests_and_bank_boundaries() {
        let mut p = arp();
        let b = &mut p.banks[0];
        b.length = 4;
        b.swing = 50;
        b.lanes[0].steps[0].ratchets = 2;
        b.lanes[0].steps[1].tie = true;
        b.lanes[0].steps[2].on = false;
        b.lanes[0].steps[3].tie = true;
        p.cycles = 2;
        let a = p.compile().unwrap();
        assert_eq!(a.sequence.notes.len(), 4);
        assert_eq!(a.sequence.notes[1].start, 180);
        assert_eq!(a.sequence.notes[1].end(), 479);
        assert!(a.sequence.notes.iter().all(|n| n.end() < a.ticks));
    }
    #[test]
    fn hostile_data_and_allocation_limits_are_errors() {
        let mut p = arp();
        p.banks[0].length = 0;
        assert!(p.compile().is_err());
        p.banks[0].length = 16;
        p.banks[0].lanes[0].octaves = 0;
        assert!(p.compile().is_err());
        let mut v = serde_json::to_value(arp()).unwrap();
        v["banks"][0]["lanes"][0]["steps"] = serde_json::json!(vec![Step::default(); 65]);
        assert!(serde_json::from_value::<Program>(v).is_err());
        let mut p = arp();
        p.cycles = 32;
        p.chain = vec![0; 32];
        p.banks[0].lanes[0].steps.iter_mut().for_each(|s| s.ratchets = 8);
        assert!(p.compile().is_err());
    }
    #[test]
    fn random_chance_drums_and_single_step_pendulum_are_bounded() {
        let mut p = arp();
        let l = &mut p.banks[0].lanes[0];
        l.mode = Mode::Drum;
        l.direction = Direction::Random;
        l.steps.iter_mut().for_each(|s| s.chance = 50);
        p.cycles = 4;
        let a = p.compile().unwrap();
        assert_eq!(a, p.compile().unwrap());
        assert!(a.sequence.notes.len() < 64);
        assert!(a.sequence.notes.iter().all(|n| n.pitch == 36 && n.channel == 9));
        p.banks[0].length = 1;
        p.banks[0].lanes[0].direction = Direction::Pendulum;
        assert!(p.compile().is_ok());
    }
}
