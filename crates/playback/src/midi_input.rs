//! Bounded live MIDI transport. Device callbacks never touch the document or audio plugins.
use soundcraft_model::TrackId;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const MIDI_QUEUE_CAPACITY: usize = 2048;
#[derive(Clone, Copy, Debug)]
pub struct Packet {
    pub track: TrackId,
    pub sample: i64,
    pub message: [u8; 3],
    pub recording: bool,
    epoch: u64,
}

/// Preallocated queues with nonblocking access, plus an out-of-band panic flag for lost events.
pub struct MidiBus {
    audio: Mutex<VecDeque<Packet>>,
    capture: Mutex<VecDeque<Packet>>,
    pub active: AtomicBool,
    pub position: AtomicI64,
    pub recording: AtomicBool,
    panic: AtomicBool,
    pub lost: AtomicU64,
    pub received: AtomicU64,
    epoch: AtomicU64,
    origin: web_time::Instant,
    clock_at: AtomicU64,
    rate: AtomicU64,
    advancing: AtomicBool,
}
impl Default for MidiBus {
    fn default() -> Self {
        Self {
            audio: Mutex::new(VecDeque::with_capacity(MIDI_QUEUE_CAPACITY)),
            capture: Mutex::new(VecDeque::with_capacity(MIDI_QUEUE_CAPACITY)),
            active: AtomicBool::new(false),
            position: AtomicI64::new(0),
            recording: AtomicBool::new(false),
            panic: AtomicBool::new(false),
            lost: AtomicU64::new(0),
            received: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            origin: web_time::Instant::now(),
            clock_at: AtomicU64::new(0),
            rate: AtomicU64::new(48000),
            advancing: AtomicBool::new(false),
        }
    }
}
impl MidiBus {
    /// Anchor the sample clock at block start. Receive-time interpolation retains close key events
    /// instead of quantizing every event in one device buffer to the same sample.
    pub fn clock(&self, sample: i64, rate: u32, advancing: bool) {
        self.position.store(sample, Ordering::Release);
        self.rate.store(u64::from(rate), Ordering::Release);
        self.clock_at.store(self.elapsed_ns(), Ordering::Release);
        self.advancing.store(advancing, Ordering::Release);
    }
    fn elapsed_ns(&self) -> u64 {
        u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
    pub fn sample_now(&self) -> i64 {
        let sample = self.position.load(Ordering::Acquire);
        if !self.advancing.load(Ordering::Acquire) {
            return sample;
        }
        interpolated(sample, self.elapsed_ns().saturating_sub(self.clock_at.load(Ordering::Acquire)), self.rate.load(Ordering::Acquire))
    }
    fn push(queue: &Mutex<VecDeque<Packet>>, packet: Packet) -> bool {
        if let Ok(mut queue) = queue.try_lock()
            && queue.len() < MIDI_QUEUE_CAPACITY
        {
            queue.push_back(packet);
            true
        } else {
            false
        }
    }
    pub fn send(&self, track: TrackId, message: [u8; 3], armed: bool) {
        let Some(message) = soundcraft_midi::live::channel_message(&message) else {
            return;
        };
        let packet = Packet {
            track,
            sample: self.sample_now(),
            message,
            recording: armed && self.recording.load(Ordering::Acquire),
            epoch: self.epoch.load(Ordering::Acquire),
        };
        if !Self::push(&self.audio, packet) || !Self::push(&self.capture, packet) {
            self.lost.fetch_add(1, Ordering::Relaxed);
            self.panic();
        }
        self.received.fetch_add(1, Ordering::Relaxed);
    }
    pub fn panic_track(&self, track: TrackId) {
        let packet = Packet {
            track,
            sample: self.position.load(Ordering::Acquire),
            message: [0xff, 0, 0],
            recording: false,
            epoch: self.epoch.load(Ordering::Acquire),
        };
        if !Self::push(&self.audio, packet) || !Self::push(&self.capture, packet) {
            self.lost.fetch_add(1, Ordering::Relaxed);
            self.panic();
        }
    }
    pub fn pop_audio(&self) -> Option<Packet> {
        self.audio.try_lock().ok()?.pop_front()
    }
    pub fn pop_capture(&self) -> Option<Packet> {
        self.capture.try_lock().ok()?.pop_front()
    }
    pub fn panic(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.panic.store(true, Ordering::Release);
    }
    pub fn current(&self, packet: &Packet) -> bool {
        packet.epoch == self.epoch.load(Ordering::Acquire)
    }
    pub fn generation(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
    pub fn take_panic(&self) -> bool {
        self.panic.swap(false, Ordering::AcqRel)
    }
    pub fn clear_audio(&self) {
        if let Ok(mut queue) = self.audio.try_lock() {
            queue.clear();
        }
    }
}
fn interpolated(sample: i64, nanoseconds: u64, rate: u64) -> i64 {
    // A stalled audio clock cannot grow a runaway take. The host separately detects device loss.
    let offset = nanoseconds.min(100_000_000).saturating_mul(rate.min(768000)) / 1_000_000_000;
    sample.saturating_add(i64::try_from(offset).unwrap_or(i64::MAX))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRoute {
    pub track: TrackId,
    pub device: Option<String>,
    pub channel: Option<u8>,
    pub armed: bool,
    pub instrument: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Status {
    pub devices: Vec<Device>,
    pub error: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::MidiInputs;

#[cfg(target_arch = "wasm32")]
pub struct MidiInputs;
#[cfg(target_arch = "wasm32")]
impl MidiInputs {
    pub fn new(_: Arc<MidiBus>) -> Result<Self, String> {
        Err("Live MIDI input currently needs the desktop editor".into())
    }
    pub fn set_routes(&self, _: Vec<InputRoute>) {}
    pub fn refresh(&self) {}
    pub fn status(&self) -> Status {
        Status { devices: Vec::new(), error: Some("Live MIDI input currently needs the desktop editor".into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn events_within_one_buffer_keep_distinct_sample_times() {
        assert_eq!(interpolated(1000, 1_000_000, 48000), 1048);
        assert_eq!(interpolated(1000, 4_000_000, 48000), 1192);
        assert_eq!(interpolated(i64::MAX, u64::MAX, u64::MAX), i64::MAX);
    }
    #[test]
    fn input_flood_is_bounded_and_requests_all_notes_off() {
        let bus = MidiBus::default();
        bus.recording.store(true, Ordering::Release);
        for _ in 0..MIDI_QUEUE_CAPACITY + 10 {
            bus.send(TrackId(1), [0x90, 60, 100], true);
        }
        assert!(bus.take_panic());
        assert!(bus.lost.load(Ordering::Relaxed) > 0);
        let mut count = 0;
        while let Some(packet) = bus.pop_capture() {
            assert!(packet.recording);
            count += 1;
        }
        assert_eq!(count, MIDI_QUEUE_CAPACITY);
    }
}
