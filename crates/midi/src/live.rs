//! Bounded MIDI 1.0 input parsing and musical-time capture, independent of devices and UI.
use crate::{CtrlEvent, CtrlKind, Note, Sequence};
use std::collections::BTreeMap;

pub const MAX_CAPTURE_EVENTS: usize = 100_000;

/// Channel messages only: realtime clock, active sense and SysEx never enter instrument queues.
pub fn channel_message(bytes: &[u8]) -> Option<[u8; 3]> {
    let status = *bytes.first()?;
    if !(0x80..0xf0).contains(&status) {
        return None;
    }
    let two = matches!(status & 0xf0, 0xc0 | 0xd0);
    let a = *bytes.get(1)?;
    let b = if two { 0 } else { *bytes.get(2)? };
    if a > 127 || b > 127 {
        return None;
    }
    Some([if status & 0xf0 == 0x90 && b == 0 { 0x80 | (status & 15) } else { status }, a, b])
}

#[derive(Default)]
pub struct Capture {
    sequence: Sequence,
    held: BTreeMap<(u8, u8), (i64, u8)>,
}

impl Capture {
    pub fn push(&mut self, tick: i64, message: [u8; 3]) -> Result<(), &'static str> {
        let Some([status, a, b]) = channel_message(&message) else {
            return Err("invalid MIDI channel message");
        };
        // Leave room for final sustain-off events, including when capture stops at the limit.
        if self.sequence.notes.len().saturating_add(self.sequence.ctrls.len()).saturating_add(self.held.len())
            >= MAX_CAPTURE_EVENTS.saturating_sub(32)
        {
            return Err("MIDI take event limit reached; stop and start another take");
        }
        let channel = status & 15;
        match status & 0xf0 {
            0x90 => {
                self.close(channel, a, tick, 64);
                self.held.insert((channel, a), (tick, b));
            }
            0x80 => self.close(channel, a, tick, b),
            0xb0 if matches!(a, 120 | 123) => {
                for pitch in 0..128 {
                    self.close(channel, pitch, tick, 64);
                }
                self.sequence.ctrls.push(CtrlEvent { tick, channel, kind: CtrlKind::Cc(a), value: i32::from(b) });
            }
            kind => {
                let (kind, value) = match kind {
                    0xa0 => (CtrlKind::PolyPressure(a), i32::from(b)),
                    0xb0 => (CtrlKind::Cc(a), i32::from(b)),
                    0xc0 => (CtrlKind::Program, i32::from(a)),
                    0xd0 => (CtrlKind::ChannelPressure, i32::from(a)),
                    0xe0 => (CtrlKind::PitchBend, (i32::from(b) << 7) + i32::from(a) - 8192),
                    _ => return Err("unsupported MIDI message"),
                };
                self.sequence.ctrls.push(CtrlEvent { tick, channel, kind, value });
            }
        }
        Ok(())
    }

    fn close(&mut self, channel: u8, pitch: u8, end: i64, release_velocity: u8) {
        if let Some((start, velocity)) = self.held.remove(&(channel, pitch)) {
            self.sequence.notes.push(Note { pitch, velocity, release_velocity, channel, start, length: end.saturating_sub(start).max(1) });
        }
    }

    pub fn close_all(&mut self, tick: i64) {
        let held = std::mem::take(&mut self.held);
        for ((channel, pitch), (start, velocity)) in held {
            self.sequence.notes.push(Note { pitch, velocity, release_velocity: 64, channel, start, length: tick.saturating_sub(start).max(1) });
        }
    }

    /// Include sounding notes for autosave/visible growing parts; clip pre/post-roll to the punch.
    pub fn snapshot(&self, until: i64, end: Option<i64>) -> Sequence {
        let limit = end.unwrap_or(until).min(until).max(1);
        let mut sequence = self.sequence.clone();
        sequence.notes.extend(self.held.iter().map(|(&(channel, pitch), &(start, velocity))| Note {
            pitch,
            velocity,
            release_velocity: 64,
            channel,
            start,
            length: until.saturating_sub(start).max(1),
        }));
        sequence.notes.retain_mut(|note| {
            let end = note.end().min(limit);
            note.start = note.start.max(0);
            note.length = end.saturating_sub(note.start);
            note.length > 0
        });
        sequence.ctrls.retain_mut(|event| {
            event.tick = event.tick.max(0);
            event.tick <= limit
        });
        sequence.sort();
        sequence
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hostile_and_short_messages_are_ignored_and_zero_velocity_releases() {
        for bytes in [&[][..], &[0x90], &[0x90, 60], &[0x90, 200, 1], &[0xf0, 1, 2], &[0xfe]] {
            assert!(channel_message(bytes).is_none());
        }
        assert_eq!(channel_message(&[0x92, 60, 0]), Some([0x82, 60, 0]));
        assert_eq!(channel_message(&[0xc3, 5]), Some([0xc3, 5, 0]));
    }
    #[test]
    fn recording_preserves_channels_velocities_controllers_and_closes_held_keys() {
        let mut capture = Capture::default();
        capture.push(10, [0x93, 60, 101]).unwrap();
        capture.push(20, [0xb3, 64, 127]).unwrap();
        capture.push(40, [0x83, 60, 33]).unwrap();
        capture.push(50, [0x94, 62, 90]).unwrap();
        let sequence = capture.snapshot(100, None);
        assert_eq!(sequence.notes[0].length, 30);
        assert_eq!(sequence.notes[0].channel, 3);
        assert_eq!(sequence.notes[0].release_velocity, 33);
        assert_eq!(sequence.notes[1].length, 50);
        assert_eq!(sequence.ctrls[0].kind, CtrlKind::Cc(64));
    }
    #[test]
    fn punch_trims_preroll_and_held_notes_without_negative_time() {
        let mut capture = Capture::default();
        capture.push(-20, [0x90, 60, 100]).unwrap();
        let sequence = capture.snapshot(100, Some(70));
        assert_eq!(sequence.notes[0].start, 0);
        assert_eq!(sequence.notes[0].length, 70);
    }
}
