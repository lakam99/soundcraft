//! Original identities for every built-in effect. Hosted editors keep their own presentation.
#[derive(Clone, Copy)]
pub(crate) struct Identity {
    pub series: &'static str,
    pub purpose: &'static str,
    pub accent: [u8; 3],
    pub focus: &'static [&'static str],
}

pub(crate) fn identity(id: &str) -> Option<Identity> {
    let (series, purpose, accent, focus): (_, _, _, &[_]) = match id {
        "eq_1band" => {
            ("FOCUS / 01", "One precise filter. Find the frequency, then shape its gain and width.", [74, 202, 211], &["type", "freq", "gain", "q"])
        }
        "eq_7band" => {
            ("SPECTRUM / 07", "Carve a place for every part, from low-end weight to high-end air.", [107, 166, 245], &["input_gain", "output_gain"])
        }
        "compressor" => (
            "CONTOUR / 01",
            "Control peaks and shape the punch with threshold, ratio and timing.",
            [158, 151, 245],
            &["threshold", "ratio", "attack", "release"],
        ),
        "expander_gate" => (
            "THRESHOLD / 02",
            "Open space between sounds. Set what gets through and how it closes.",
            [192, 150, 243],
            &["threshold", "range", "attack", "hold"],
        ),
        "de_esser" => (
            "SILK / 03",
            "Tame sharp sibilance. Use Listen to find the detection frequency.",
            [242, 143, 153],
            &["freq", "threshold", "range", "listen"],
        ),
        "maximizer" => (
            "CEILING / 04",
            "Raise perceived level while keeping peaks under a chosen ceiling.",
            [245, 133, 119],
            &["threshold", "ceiling", "release"],
        ),
        "channel_strip" => {
            ("CHANNEL / 05", "One path for input level, gate, compression, EQ and output.", [143, 190, 206], &["input_gain", "output_gain"])
        }
        "room_reverb" => {
            ("ROOM / 01", "Place the sound in a space. Shape its size, early gap and decay.", [235, 185, 110], &["size", "decay", "pre_delay", "mix"])
        }
        "plate_reverb" => {
            ("PLATE / 02", "A dense, diffuse tail for presence, sheen and sustain.", [228, 153, 193], &["decay", "diffusion", "damping", "mix"])
        }
        "mod_delay" => {
            ("ECHO / 01", "Make time audible with repeating taps and a moving feedback trail.", [92, 210, 190], &["time", "feedback", "mix"])
        }
        "auto_pan" => (
            "ORBIT / 01",
            "Move a sound through the stereo field with a chosen path and source.",
            [179, 211, 114],
            &["source", "path", "rate", "width"],
        ),
        "chorus" => {
            ("ENSEMBLE / 02", "Add width and movement with closely spaced, modulated voices.", [114, 211, 218], &["rate", "depth", "mix", "spread"])
        }
        "flanger" => (
            "CROSSING / 03",
            "Sweep a short delay against the dry sound for a resonant moving texture.",
            [187, 151, 242],
            &["delay", "feedback", "rate", "depth"],
        ),
        "phaser" => {
            ("PHASE / 04", "Move a bank of phase-shifting stages through the sound.", [144, 163, 242], &["stages", "center", "rate", "depth"])
        }
        "ring_mod" => (
            "MULTIPLY / 05",
            "Explore ring, frequency and comb textures with a moving modulator.",
            [225, 208, 117],
            &["effect", "frequency", "mod_source", "mix"],
        ),
        "saturator" => {
            ("EMBER / 01", "Add soft harmonic weight. Balance drive, tone and the dry sound.", [235, 164, 112], &["drive", "tone", "mix", "output"])
        }
        "lofi" => {
            ("PIXEL / 02", "Trade resolution for texture with bit depth and sample-rate reduction.", [169, 219, 130], &["bits", "sample_rate", "mix"])
        }
        "rectifier" => {
            ("FOLD / 03", "Reshape polarity into new harmonics, then blend it with the original.", [242, 177, 109], &["mode", "mix", "output"])
        }
        "subharmonic" => (
            "FOUNDATION / 04",
            "Balance the lower, upper and direct paths to build low-frequency weight.",
            [183, 155, 222],
            &["range", "lower_gain", "upper_gain", "direct_gain"],
        ),
        "pitch_shifter" => {
            ("INTERVAL / 01", "Transpose in semitones, fine-tune in cents and blend the result.", [139, 206, 236], &["semitones", "cents", "mix"])
        }
        "time_shift" => {
            ("ALIGN / 02", "Delay a signal in milliseconds and samples to align its timing.", [220, 193, 125], &["delay_ms", "delay_samples"])
        }
        "gain" => ("LEVEL / 01", "A clear, simple level adjustment with a smooth gain change.", [112, 212, 165], &["gain"]),
        "trim" => ("BALANCE / 02", "Set the input level and flip polarity when needed.", [156, 194, 220], &["gain", "invert"]),
        "invert" => ("POLARITY / 03", "Flip the sign of the signal. Bypass returns the original polarity.", [225, 153, 218], &[]),
        "dc_offset_removal" => ("CENTER / 04", "Remove a constant offset with a low-frequency high-pass filter.", [133, 215, 192], &["cutoff"]),
        "signal_generator" => {
            ("SOURCE / 05", "Create a reference tone or noise. Start with a safe output level.", [235, 176, 130], &["waveform", "freq", "level"])
        }
        "dither" => ("GRAIN / 06", "Prepare a lower bit depth with dither and optional noise shaping.", [190, 175, 231], &["bits", "noise_shaping"]),
        _ => return None,
    };
    Some(Identity { series, purpose, accent, focus })
}

#[cfg(test)]
mod tests {
    use super::identity;
    #[test]
    fn every_effect_has_a_distinct_identity_and_valid_focus_controls() {
        let mut identities = std::collections::HashSet::new();
        let effects: Vec<_> = soundcraft_dsp::plugins().iter().filter(|info| !info.is_instrument).collect();
        assert_eq!(effects.len(), 27);
        for info in effects {
            let style = identity(info.id).expect("every built-in effect needs a designed editor");
            assert!(identities.insert(style.series), "{} needs its own visual identity", info.id);
            assert!(style.purpose.len() > 30);
            for param in style.focus {
                assert!(info.param(param).is_some(), "{} has no {param} control", info.id);
            }
        }
        assert!(identity("clap:third-party").is_none());
        assert!(identity("sampler").is_none());
    }
}
