use ableton_link::Link;
use std::sync::{mpsc, OnceLock, Mutex};

/// Events pushed from Link's internal thread via callbacks.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum LinkEvent {
    TempoChanged(f64),
    PlayStateChanged(bool),
    NumPeersChanged(usize),
}

/// Current snapshot of the Link session state, captured at a point in time.
pub struct LinkState {
    pub tempo: f64,
    pub beat: f64,
    pub phase: f64,
    pub playing: bool,
    pub num_peers: usize,
}

// Global sender — the extern fn callbacks push events through this.
static EVENT_TX: OnceLock<Mutex<mpsc::Sender<LinkEvent>>> = OnceLock::new();

extern "C" fn on_tempo_changed(bpm: f64) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.lock().unwrap().send(LinkEvent::TempoChanged(bpm));
    }
}

extern "C" fn on_play_state_changed(playing: bool) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.lock().unwrap().send(LinkEvent::PlayStateChanged(playing));
    }
}

extern "C" fn on_num_peers_changed(num_peers: usize) {
    if let Some(tx) = EVENT_TX.get() {
        let _ = tx.lock().unwrap().send(LinkEvent::NumPeersChanged(num_peers));
    }
}

/// Reads Ableton Link session state. This is a passive observer —
/// tempo and transport are controlled by other peers in the session.
///
/// Events (tempo/play/peers changes) arrive via callbacks on Link's
/// internal thread and can be drained with `events()`.
pub struct LinkController {
    link: Link,
    quantum: f64,
    event_rx: mpsc::Receiver<LinkEvent>,
}

impl LinkController {
    /// Join a Link session. `quantum` is the number of beats per phase cycle
    /// (typically 4 for 4/4 time).
    ///
    /// Only one `LinkController` should exist at a time (the callbacks use a global channel).
    pub fn new(quantum: f64) -> Self {
        let (tx, rx) = mpsc::channel();
        EVENT_TX
            .set(Mutex::new(tx))
            .expect("LinkController already created — only one instance allowed");

        let mut link = Link::new(120.0);
        link.set_tempo_callback(on_tempo_changed);
        link.set_start_stop_callback(on_play_state_changed);
        link.set_num_peers_callback(on_num_peers_changed);
        link.enable_start_stop_sync(true);
        link.enable(true);

        Self {
            link,
            quantum,
            event_rx: rx,
        }
    }

    /// Drain all pending events since the last call.
    pub fn events(&self) -> Vec<LinkEvent> {
        self.event_rx.try_iter().collect()
    }

    /// Capture the current Link session state (beat/phase requires polling).
    pub fn state(&self) -> LinkState {
        let now = self.link.clock().micros();
        let mut result = LinkState {
            tempo: 0.0,
            beat: 0.0,
            phase: 0.0,
            playing: false,
            num_peers: self.link.num_peers(),
        };
        self.link.with_app_session_state(|state| {
            result.tempo = state.tempo();
            result.beat = state.beat_at_time(now, self.quantum);
            result.phase = state.phase_at_time(now, self.quantum);
            result.playing = state.is_playing();
        });
        result
    }

}
