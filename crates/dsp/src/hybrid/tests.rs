use super::*;
use crate::{Plugin, SampleBuffer};
use std::sync::{Arc, atomic::AtomicBool};
fn simple() -> Design {
    let mut d = Patch::starter().design().clone();
    d.modules = vec![Module::new("osc", Kind::Oscillator), Module::new("out", Kind::Output)];
    d.routes = vec![Route { from: "osc".into(), to: "out".into(), gain: 1.0 }];
    d.modulation.clear();
    d
}
fn render(d: Design) -> render::Rendered {
    render::render(
        Arc::new(Patch::compile(d).unwrap()),
        render::Settings { note: 69, gate: 0.25, tail: 0.3, sample_rate: 16000, ..Default::default() },
        None,
        &AtomicBool::new(false),
    )
    .unwrap()
}
#[test]
fn hybrid_starter_compiles_serializes_and_renders_a_released_note() {
    let p = Patch::starter();
    assert_eq!(p.plan.nodes.len(), 8);
    let serialized = serde_json::to_vec(&p).unwrap();
    let restored: Patch = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(p, restored);
    let r = render(p.design().clone());
    assert!(r.metrics.peak > 0.01 && r.metrics.peak < 1.0);
    assert!(r.metrics.rms > 0.005);
    assert!(r.audio.channels.iter().flatten().all(|v| v.is_finite()));
    let tail = r.audio.channels[0].iter().rev().take(100).map(|v| v.abs()).sum::<f32>();
    assert!(tail < 1.0, "release tail: {tail}");
}
#[test]
fn hybrid_routing_sources_and_processors_change_real_audio() {
    let d = simple();
    let a = render(d.clone());
    let channel = &a.audio.channels[0];
    let crossings = channel[100..3000].windows(2).filter(|s| s[0] <= 0.0 && s[1] > 0.0).count();
    let hz = crossings as f32 / (2900.0 / 16000.0);
    assert!((hz - 440.0).abs() < 12.0, "{hz}");
    for kind in [Kind::Filter, Kind::Saturator, Kind::Resonator] {
        let mut d = d.clone();
        d.modules.push(Module::new("fx", kind));
        d.routes = vec![Route { from: "osc".into(), to: "fx".into(), gain: 1.0 }, Route { from: "fx".into(), to: "out".into(), gain: 1.0 }];
        let b = render(d);
        assert_ne!(a.audio.channels, b.audio.channels, "{}", kind.label());
    }
    for shape in [1.0, 2.0, 3.0, 4.0, 5.0] {
        let mut d = d.clone();
        d.modules[0].params.insert("shape".into(), shape);
        assert_ne!(a.audio.channels, render(d).audio.channels);
    }
    let mut fm = d.clone();
    let mut modulator = Module::new("fm", Kind::Oscillator);
    modulator.params.insert("pitch".into(), 12.0);
    fm.modules.push(modulator);
    fm.modules[0].params.insert("fm".into(), 500.0);
    fm.routes.push(Route { from: "fm".into(), to: "osc".into(), gain: 1.0 });
    assert_ne!(a.audio.channels, render(fm).audio.channels);
    let mut noise = d.clone();
    noise.modules[0] = Module::new("osc", Kind::Noise);
    assert_ne!(a.audio.channels, render(noise).audio.channels);
    let mut modulated = d.clone();
    modulated.modulation.push(Modulation { source: Source::Macro(0), target: "osc".into(), param: "pitch".into(), amount: 0.25 });
    let p = Arc::new(Patch::compile(modulated).unwrap());
    let mut settings = render::Settings { gate: 0.2, tail: 0.1, sample_rate: 8000, ..Default::default() };
    let first = render::render(p.clone(), settings, None, &AtomicBool::new(false)).unwrap();
    settings.macros[0] = 1.0;
    let second = render::render(p, settings, None, &AtomicBool::new(false)).unwrap();
    assert_ne!(first.audio.channels, second.audio.channels);
}
#[test]
fn hybrid_rejects_hostile_graphs_without_panicking() {
    let good = simple();
    let mut bad = Vec::new();
    let mut d = good.clone();
    d.routes.push(Route { from: "out".into(), to: "osc".into(), gain: 1.0 });
    bad.push(d);
    let mut d = good.clone();
    d.modulation.push(Modulation { source: Source::Module("out".into()), target: "osc".into(), param: "pitch".into(), amount: 1.0 });
    bad.push(d);
    let mut d = good.clone();
    d.modules[0].params.insert("pitch".into(), f32::NAN);
    bad.push(d);
    let mut d = good.clone();
    d.modules[0].id = d.output.clone();
    bad.push(d);
    let mut d = good.clone();
    d.routes[0].from = "missing".into();
    bad.push(d);
    let mut d = good.clone();
    d.modules.extend((0..16).map(|i| Module::new(&format!("m{i}"), Kind::Lfo)));
    bad.push(d);
    for d in bad {
        assert!(std::panic::catch_unwind(|| Patch::compile(d)).unwrap().is_err());
    }
    assert!(render::Settings { gate: f32::INFINITY, ..Default::default() }.frames().is_err());
    assert!(render::Settings { gate: 8.0, tail: 8.0, sample_rate: 192000, ..Default::default() }.frames().is_err());
    assert!(render::render(Arc::new(Patch::starter()), Default::default(), None, &AtomicBool::new(true)).is_err());
}
#[test]
fn hybrid_unplugged_output_is_a_valid_silent_patch() {
    let mut d = simple();
    d.routes.clear();
    let r = render(d);
    assert_eq!(r.metrics.peak, 0.0);
}
#[test]
fn hybrid_cv_release_and_post_amplifier_resonance_are_not_cut_off_early() {
    let mut d = Patch::starter().design().clone();
    d.modulation.push(Modulation { source: Source::Macro(0), target: "env".into(), param: "release".into(), amount: 0.4 });
    let p = Arc::new(Patch::compile(d).unwrap());
    let r = render::render(
        p,
        render::Settings { gate: 0.1, tail: 1.2, sample_rate: 8000, macros: [1.0, 0.5, 0.5, 0.5], ..Default::default() },
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(r.audio.channels[0][6400..7000].iter().any(|v| v.abs() > 0.0001));
    let mut d = simple();
    d.modules.extend([Module::new("amp", Kind::Vca), Module::new("env", Kind::Envelope), Module::new("body", Kind::Resonator)]);
    d.modules.iter_mut().find(|m| m.id == "env").unwrap().params.insert("release".into(), 10.0);
    let body = d.modules.iter_mut().find(|m| m.id == "body").unwrap();
    body.params.insert("decay".into(), 1000.0);
    body.params.insert("mix".into(), 1.0);
    d.routes = [("osc", "amp"), ("amp", "body"), ("body", "out")].map(|(from, to)| Route { from: from.into(), to: to.into(), gain: 1.0 }).to_vec();
    d.modulation = vec![Modulation { source: Source::Module("env".into()), target: "amp".into(), param: "gain".into(), amount: 1.0 }];
    let r = render::render(
        Arc::new(Patch::compile(d).unwrap()),
        render::Settings { gate: 0.1, tail: 1.0, sample_rate: 16000, note: 69, ..Default::default() },
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(r.audio.channels[0][9600..10000].iter().any(|v| v.abs() > 0.000001));
}
#[test]
fn hybrid_sample_playback_and_sustain_reset_have_finite_tails() {
    let mut d = simple();
    d.modules[0] = Module::new("osc", Kind::Sample);
    let sample: Arc<dyn crate::SampleSource> = Arc::new(SampleBuffer { sample_rate: 8000.0, channels: vec![vec![0.4; 8000]] });
    let mut synth = HybridSynth::new();
    synth.prepare(8000.0, 256, 2);
    synth.set_hybrid_patch(Arc::new(Patch::compile(d).unwrap()));
    synth.set_sample(Some(sample));
    synth.note_on(7, 60, 100);
    let mut block = vec![vec![0.0; 256]; 2];
    synth.process(&mut block, 256);
    assert!(block[0][..7].iter().all(|x| *x == 0.0));
    assert!(block[0][50..].iter().any(|x| *x > 0.01));
    synth.midi(0, [0xb0, 64, 127]);
    synth.note_off(0, 60);
    for _ in 0..6 {
        synth.process(&mut block, 256);
    }
    assert!(block[0].iter().any(|x| *x > 0.01));
    synth.midi(0, [0xb0, 64, 0]);
    for _ in 0..15 {
        synth.process(&mut block, 256);
    }
    assert!(block.iter().flatten().all(|x| *x == 0.0));
}
#[test]
fn hybrid_tone_edits_preserve_held_voice_and_topology_edits_reset_it() {
    let d = simple();
    let mut synth = HybridSynth::new();
    synth.prepare(16000.0, 256, 2);
    synth.set_hybrid_patch(Arc::new(Patch::compile(d.clone()).unwrap()));
    synth.note_on(0, 60, 100);
    let mut block = vec![vec![0.0; 256]; 2];
    synth.process(&mut block, 256);
    let mut changed = d.clone();
    changed.modules[0].params.insert("pitch".into(), 12.0);
    synth.set_hybrid_patch(Arc::new(Patch::compile(changed.clone()).unwrap()));
    synth.process(&mut block, 256);
    assert!(block[0].iter().any(|x| x.abs() > 0.01));
    changed.modules.push(Module::new("extra", Kind::Lfo));
    synth.set_hybrid_patch(Arc::new(Patch::compile(changed).unwrap()));
    synth.process(&mut block, 256);
    assert!(block.iter().flatten().all(|x| *x == 0.0));
}
