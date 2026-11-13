use super::{Arc, Device, InputRoute, MidiBus, Mutex, Ordering, Status};
use midir::{MidiInput, MidiInputConnection};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::PoisonError;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::{Duration, Instant};

struct Shared {
    running: AtomicBool,
    refresh: AtomicU64,
    routes: Mutex<Vec<InputRoute>>,
    status: Mutex<Status>,
    bus: Arc<MidiBus>,
}
pub struct MidiInputs {
    shared: Arc<Shared>,
}
impl MidiInputs {
    pub fn new(bus: Arc<MidiBus>) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            refresh: AtomicU64::new(0),
            routes: Mutex::new(Vec::new()),
            status: Mutex::new(Status::default()),
            bus,
        });
        let worker = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("soundcraft-midi-input".into())
            .spawn(move || {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&worker))).is_err() {
                    worker.bus.panic();
                    worker.status.lock().unwrap_or_else(PoisonError::into_inner).error =
                        Some("MIDI input worker failed. Press Reconnect to retry.".into());
                }
            })
            .map_err(|error| format!("Could not start MIDI input: {error}"))?;
        Ok(Self { shared })
    }
    pub fn set_routes(&self, routes: Vec<InputRoute>) {
        let mut current = self.shared.routes.lock().unwrap_or_else(PoisonError::into_inner);
        if *current != routes {
            *current = routes;
            self.shared.refresh.fetch_add(1, Ordering::Release);
        }
        self.shared.bus.active.store(!current.is_empty(), Ordering::Release);
    }
    pub fn refresh(&self) {
        self.shared.refresh.fetch_add(1, Ordering::Release);
    }
    pub fn status(&self) -> Status {
        self.shared.status.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}
impl Drop for MidiInputs {
    fn drop(&mut self) {
        self.shared.running.store(false, Ordering::Release);
        self.shared.bus.panic();
    }
}

struct Callback {
    shared: Arc<Shared>,
    device: String,
    // Note-offs return to the original recipients even after selection/routing changes.
    held: BTreeMap<(u8, u8), Vec<(soundcraft_model::TrackId, bool)>>,
}
impl Callback {
    fn receive(&mut self, bytes: &[u8]) {
        let Some(message) = soundcraft_midi::live::channel_message(bytes) else {
            return;
        };
        let [status, note, _] = message;
        let kind = status & 0xf0;
        let key = (status & 15, note);
        let destinations = if kind == 0x80 {
            self.held.remove(&key).unwrap_or_default()
        } else {
            let Ok(routes) = self.shared.routes.try_lock() else {
                self.shared.bus.lost.fetch_add(1, Ordering::Relaxed);
                self.shared.bus.panic();
                return;
            };
            let targets: Vec<_> = routes
                .iter()
                .take(128)
                .filter(|route| {
                    route.device.as_ref().is_none_or(|id| id == &self.device) && route.channel.is_none_or(|channel| channel == (status & 15) + 1)
                })
                .map(|route| (route.track, route.armed))
                .collect();
            if kind == 0x90
                && let Some(previous) = self.held.insert(key, targets.clone())
            {
                for (track, armed) in previous {
                    self.shared.bus.send(track, [0x80 | (status & 15), note, 0], armed);
                }
            }
            targets
        };
        for (track, armed) in destinations {
            self.shared.bus.send(track, message, armed);
        }
    }
}

fn run(shared: &Arc<Shared>) {
    let mut connections: BTreeMap<String, MidiInputConnection<Callback>> = BTreeMap::new();
    let mut failed = BTreeSet::new();
    let mut generation = u64::MAX;
    let mut last = Instant::now() - Duration::from_secs(2);
    while shared.running.load(Ordering::Acquire) {
        let refresh = shared.refresh.load(Ordering::Acquire);
        if refresh != generation || last.elapsed() >= Duration::from_secs(1) {
            if refresh != generation {
                failed.clear();
                generation = refresh;
            }
            last = Instant::now();
            let input = match MidiInput::new("SoundCraft MIDI discovery") {
                Ok(input) => input,
                Err(error) => {
                    shared.status.lock().unwrap_or_else(PoisonError::into_inner).error = Some(format!("MIDI devices unavailable: {error}"));
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            let routes = shared.routes.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let ports: Vec<_> = input.ports().into_iter().take(32).collect();
            let present: BTreeSet<_> = ports.iter().map(|port| port.id()).collect();
            let gone: Vec<_> = connections
                .keys()
                .filter(|id| !present.contains(*id) || !routes.iter().any(|route| route.device.as_ref().is_none_or(|device| device == *id)))
                .cloned()
                .collect();
            let disconnected = gone.iter().any(|id| !present.contains(id));
            for id in gone {
                connections.remove(&id);
                if !present.contains(&id) {
                    shared.bus.panic();
                }
            }
            let mut devices = Vec::new();
            for port in ports {
                let id = port.id();
                let name = input.port_name(&port).unwrap_or_else(|_| "Unavailable MIDI device".into());
                let wanted = routes.iter().any(|route| route.device.as_ref().is_none_or(|device| device == &id));
                let mut error = None;
                if wanted && !connections.contains_key(&id) && !failed.contains(&id) {
                    let opened = MidiInput::new("SoundCraft MIDI input").map_err(|error| error.to_string()).and_then(|input| {
                        input
                            .connect(
                                &port,
                                "SoundCraft keyboard",
                                |_, bytes, callback: &mut Callback| {
                                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback.receive(bytes))).is_err() {
                                        callback.shared.bus.lost.fetch_add(1, Ordering::Relaxed);
                                        callback.shared.bus.panic();
                                    }
                                },
                                Callback { shared: Arc::clone(shared), device: id.clone(), held: BTreeMap::new() },
                            )
                            .map_err(|error| error.to_string())
                    });
                    match opened {
                        Ok(connection) => {
                            connections.insert(id.clone(), connection);
                        }
                        Err(problem) => {
                            error = Some(format!("Cannot open MIDI input: {problem}. Close other MIDI apps and press Reconnect."));
                            failed.insert(id.clone());
                        }
                    }
                }
                if failed.contains(&id) && error.is_none() {
                    error = Some("MIDI input unavailable. Press Reconnect to retry.".into());
                }
                devices.push(Device { id: id.clone(), name, connected: connections.contains_key(&id), error });
            }
            let mut status = shared.status.lock().unwrap_or_else(PoisonError::into_inner);
            status.devices = devices;
            if disconnected {
                status.error = Some("MIDI keyboard disconnected. The take is kept; reconnect the keyboard to continue.".into());
            } else if status.devices.iter().any(|device| device.connected) {
                status.error = None;
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn route(track: u64, channel: Option<u8>) -> InputRoute {
        InputRoute { track: soundcraft_model::TrackId(track), device: Some("keyboard".into()), channel, armed: true, instrument: "synth".into() }
    }
    #[test]
    fn note_off_returns_to_original_track_after_routing_changes_and_filters_channels() {
        let bus = Arc::new(MidiBus::default());
        let shared = Arc::new(Shared {
            running: AtomicBool::new(true),
            refresh: AtomicU64::new(0),
            routes: Mutex::new(vec![route(1, Some(2))]),
            status: Mutex::new(Status::default()),
            bus: Arc::clone(&bus),
        });
        let mut callback = Callback { shared: Arc::clone(&shared), device: "keyboard".into(), held: BTreeMap::new() };
        callback.receive(&[0x90, 60, 100]);
        assert!(bus.pop_capture().is_none());
        callback.receive(&[0x91, 60, 100]);
        assert_eq!(bus.pop_capture().unwrap().track, soundcraft_model::TrackId(1));
        *shared.routes.lock().unwrap() = vec![route(2, None)];
        callback.receive(&[0x81, 60, 0]);
        assert_eq!(bus.pop_capture().unwrap().track, soundcraft_model::TrackId(1));
        callback.receive(&[0x91, 62, 100]);
        assert_eq!(bus.pop_capture().unwrap().track, soundcraft_model::TrackId(2));
    }
}
