//! Named, session-owned hardware input paths. Channel numbers are zero-based in files/APIs.
use crate::{ChannelFormat, Route, Session};

pub const MAX_INPUT_CHANNELS: usize = 256;
pub const MAX_INPUT_PATHS: usize = 384;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InputPath {
    pub name: String,
    pub format: ChannelFormat,
    pub channels: Vec<Option<usize>>,
    pub active: bool,
}

pub fn default_inputs(channels: usize) -> Vec<InputPath> {
    let channels = channels.min(MAX_INPUT_CHANNELS);
    let mut paths: Vec<_> = (0..channels)
        .map(|i| InputPath { name: format!("In {}", i + 1), format: ChannelFormat::Mono, channels: vec![Some(i)], active: true })
        .collect();
    paths.extend((0..channels.saturating_sub(1)).step_by(2).map(|i| InputPath {
        name: format!("In {}-{}", i + 1, i + 2),
        format: ChannelFormat::Stereo,
        channels: vec![Some(i), Some(i + 1)],
        active: true,
    }));
    paths
}

/// Parse only canonical legacy hardware labels, never arbitrary text or a partial number.
pub fn legacy_input_channel(name: &str, channel: usize) -> Option<usize> {
    let numbers = name.strip_prefix("In ")?;
    let (first, last) = numbers.split_once('-').map_or((numbers, None), |(a, b)| (a, Some(b)));
    let first = first.parse::<usize>().ok()?.checked_sub(1)?;
    let port = first.checked_add(channel)?;
    let last = match last {
        Some(n) => n.parse::<usize>().ok()?,
        None => first.checked_add(1)?,
    };
    (port < MAX_INPUT_CHANNELS && port < last).then_some(port)
}

impl Session {
    /// Allocation-free resolver shared by recording and the real-time monitor. Missing ports
    /// remain disconnected; wrapping onto another microphone would record the wrong source.
    pub fn input_channel(&self, route: &Route, channel: usize) -> Option<usize> {
        let Route::Hardware(name) = route else { return None };
        if let Some(path) = self.inputs.iter().find(|p| p.name == *name) {
            return path.active.then(|| path.channels.get(channel).copied().flatten()).flatten().filter(|p| *p < MAX_INPUT_CHANNELS);
        }
        legacy_input_channel(name, channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_noncontiguous_stereo_and_disabled_ports() {
        let mut s = Session::default();
        s.inputs.push(InputPath { name: "Synth".into(), format: ChannelFormat::Stereo, channels: vec![Some(9), Some(2)], active: true });
        let route = Route::Hardware("Synth".into());
        assert_eq!(s.input_channel(&route, 0), Some(9));
        assert_eq!(s.input_channel(&route, 1), Some(2));
        assert_eq!(s.input_channel(&route, 2), None);
        s.inputs.last_mut().unwrap().active = false;
        assert_eq!(s.input_channel(&route, 0), None);
        assert_eq!(s.input_channel(&Route::None, 0), None);
        assert_eq!(s.input_channel(&Route::Hardware("Hardware 1".into()), 0), None);
    }

    #[test]
    fn old_implicit_inputs_migrate_and_explicit_disconnect_survives_reopening() {
        let mut s = Session::default();
        let t = s.add_track(crate::TrackKind::Audio, ChannelFormat::Stereo, None);
        s.track_mut(t).unwrap().mixer.input = Route::None;
        let mut old = serde_json::to_value(&s).unwrap();
        old["format_version"] = serde_json::json!(1);
        old.as_object_mut().unwrap().remove("inputs");
        let migrated = Session::from_json(&old.to_string()).unwrap();
        assert_eq!(migrated.format_version, 2);
        assert_eq!(migrated.track(t).unwrap().mixer.input, Route::Hardware("In 1-2".into()));
        let loaded = Session::from_json(&s.to_json().unwrap()).unwrap();
        assert_eq!(loaded.track(t).unwrap().mixer.input, Route::None);
    }

    #[test]
    fn full_interface_and_strict_legacy_names() {
        assert_eq!(default_inputs(32).len(), 48);
        assert_eq!(default_inputs(usize::MAX).len(), MAX_INPUT_PATHS);
        assert_eq!(legacy_input_channel("In 12", 0), Some(11));
        assert_eq!(legacy_input_channel("In 1-2", 1), Some(1));
        for name in ["In 0", "In 257", "In 1 garbage", "In 1-", "In 1-no", "In 2-1"] {
            assert_eq!(legacy_input_channel(name, 0), None, "{name}");
        }
    }
}
