//! Read-only native MIDI diagnostic: cargo run -p soundcraft-playback --example midi_input -- 90.
//! Does not open audio devices, change a session or persist played notes.
use soundcraft_model::TrackId;
use soundcraft_playback::midi_input::{InputRoute, MidiBus, MidiInputs};
use std::sync::Arc;
use std::time::{Duration, Instant};
fn main() -> Result<(), String> {
    let seconds = std::env::args().nth(1).and_then(|value| value.parse::<u64>().ok()).unwrap_or(5).clamp(1, 300);
    let bus = Arc::new(MidiBus::default());
    let inputs = MidiInputs::new(Arc::clone(&bus))?;
    inputs.set_routes(vec![InputRoute { track: TrackId(1), device: None, channel: None, armed: false, instrument: "diagnostic".into() }]);
    let start = Instant::now();
    let mut last = 0;
    let mut on = 0;
    let mut off = 0;
    let mut controls = 0;
    while start.elapsed() < Duration::from_secs(seconds) {
        while let Some(packet) = bus.pop_capture() {
            match packet.message[0] & 0xf0 {
                0x90 => on += 1,
                0x80 => off += 1,
                _ => controls += 1,
            }
        }
        bus.clear_audio();
        let elapsed = start.elapsed().as_secs();
        if elapsed > last {
            last = elapsed;
            println!("seconds={elapsed} devices={:?} note_on={on} note_off={off} controllers={controls}", inputs.status());
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    if on > 0 && off > 0 {
        println!("HARDWARE_NOTES_RECEIVED_AND_RELEASED");
    }
    Ok(())
}
