//! Bounded offline audition/sample rendering; callers run this on a worker, not the audio/UI thread.
use super::{HybridSynth, Patch};
use crate::{Plugin, SampleBuffer, SampleSource};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub note: u8,
    pub velocity: u8,
    pub gate: f32,
    pub tail: f32,
    pub sample_rate: u32,
    pub macros: [f32; 4],
    pub level: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self { note: 60, velocity: 100, gate: 1.5, tail: 1.0, sample_rate: 48000, macros: [0.5; 4], level: -6.0 }
    }
}
impl Settings {
    pub fn frames(&self) -> Result<usize, String> {
        if self.note > 127
            || self.velocity == 0
            || self.velocity > 127
            || !self.gate.is_finite()
            || !self.tail.is_finite()
            || !(0.05..=8.0).contains(&self.gate)
            || !(0.0..=8.0).contains(&self.tail)
            || !(8000..=192000).contains(&self.sample_rate)
            || self.macros.iter().any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("Render requires note 0..127, velocity 1..127, gate .05..8s, tail 0..8s and rate 8..192kHz".into());
        }
        if !self.level.is_finite() || !(-48.0..=0.0).contains(&self.level) {
            return Err("Master level must be -48..0 dB".into());
        }
        let n = ((self.gate + self.tail) * self.sample_rate as f32).ceil() as usize;
        if n > 2_000_000 {
            return Err("Render exceeds two million stereo frames; lower the duration or sample rate".into());
        }
        Ok(n)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct Metrics {
    pub frames: usize,
    pub seconds: f32,
    pub peak: f32,
    pub rms: f32,
    pub zero_crossing_rate: f32,
    pub waveform: Vec<f32>,
    pub waveform_min: Vec<f32>,
    pub waveform_max: Vec<f32>,
}
pub struct Rendered {
    pub audio: SampleBuffer,
    pub metrics: Metrics,
}
pub fn render(patch: Arc<Patch>, settings: Settings, sample: Option<Arc<dyn SampleSource>>, cancel: &AtomicBool) -> Result<Rendered, String> {
    let frames = settings.frames()?;
    let mut synth = HybridSynth::new();
    synth.prepare(settings.sample_rate as f32, 256, 2);
    synth.set_hybrid_patch(patch);
    synth.set_sample(sample);
    for (i, value) in settings.macros.iter().enumerate() {
        if let Some(p) = super::MACROS.get(i) {
            synth.set_param(p.id, *value);
        }
    }
    synth.set_param("level", settings.level);
    synth.note_on(0, settings.note, settings.velocity);
    let off = (settings.gate * settings.sample_rate as f32).round() as usize;
    let mut channels = vec![vec![0.0; frames]; 2];
    let mut block = vec![vec![0.0; 256]; 2];
    let mut released = false;
    for start in (0..frames).step_by(256) {
        if cancel.load(Ordering::Acquire) {
            return Err("Render cancelled".into());
        }
        let count = (frames - start).min(256);
        if !released && off < start.saturating_add(count) {
            synth.note_off(off.saturating_sub(start), settings.note);
            released = true;
        }
        synth.process(&mut block, count);
        for (out, input) in channels.iter_mut().zip(&block) {
            if let Some(dst) = out.get_mut(start..start + count) {
                dst.copy_from_slice(input.get(..count).ok_or("Render buffer mismatch")?);
            }
        }
    }
    let mut peak = 0.0f32;
    let mut sum = 0.0f64;
    let mut crossings = 0usize;
    let mut previous = 0.0f32;
    for channel in &channels {
        for &v in channel {
            if !v.is_finite() {
                return Err("Synth produced non-finite audio".into());
            }
            peak = peak.max(v.abs());
            sum += f64::from(v) * f64::from(v);
        }
    }
    if let Some(first) = channels.first() {
        for &v in first {
            if v.signum() != previous.signum() {
                crossings = crossings.saturating_add(1);
            }
            previous = v;
        }
    }
    let width = frames.div_ceil(192).max(1);
    let waveform = channels
        .first()
        .map(|c| c.chunks(width).map(|xs| xs.iter().copied().max_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap_or(0.0)).collect())
        .unwrap_or_default();
    let waveform_min = channels.first().map(|c| c.chunks(width).map(|xs| xs.iter().copied().fold(0.0f32, f32::min)).collect()).unwrap_or_default();
    let waveform_max = channels.first().map(|c| c.chunks(width).map(|xs| xs.iter().copied().fold(0.0f32, f32::max)).collect()).unwrap_or_default();
    Ok(Rendered {
        audio: SampleBuffer { sample_rate: settings.sample_rate as f32, channels },
        metrics: Metrics {
            frames,
            seconds: frames as f32 / settings.sample_rate as f32,
            peak,
            rms: (sum / (frames.saturating_mul(2).max(1)) as f64).sqrt() as f32,
            zero_crossing_rate: crossings as f32 / (frames.max(1)) as f32,
            waveform,
            waveform_min,
            waveform_max,
        },
    })
}
