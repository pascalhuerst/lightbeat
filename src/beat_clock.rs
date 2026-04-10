use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::link_controller::LinkController;

/// Information delivered to listeners on each matching beat.
#[derive(Debug, Clone)]
pub struct BeatInfo {
    pub beat: u64,
}

/// Continuously updated snapshot of the Link session, polled ~1ms.
#[derive(Debug, Clone)]
pub struct LinkSnapshot {
    pub tempo: f64,
    pub beat: f64,
    pub phase: f64,
    pub playing: bool,
    pub num_peers: usize,
}

impl Default for LinkSnapshot {
    fn default() -> Self {
        Self {
            tempo: 120.0,
            beat: 0.0,
            phase: 0.0,
            playing: false,
            num_peers: 0,
        }
    }
}

/// Controls which beats a listener cares about.
#[derive(Debug, Clone)]
pub struct BeatPattern {
    /// Fire every `divisor` beats (1 = every beat, 4 = every 4th, etc.)
    pub divisor: u64,
    /// Which beat within the cycle (0-based). e.g. divisor=4, offset=2
    /// fires on beats 2, 6, 10, ...
    pub offset: u64,
}

impl BeatPattern {
    pub fn every(n: u64) -> Self {
        Self { divisor: n, offset: 0 }
    }

    fn matches(&self, beat: u64) -> bool {
        beat % self.divisor == self.offset
    }
}

/// Implement this trait to receive beat callbacks.
pub trait BeatListener: Send {
    fn on_beat(&mut self, info: &BeatInfo);
    fn on_transport_change(&mut self, _playing: bool) {}
}

struct Subscription {
    pattern: BeatPattern,
    listener: Arc<Mutex<dyn BeatListener>>,
}

/// Drives the beat clock in a background thread, polling Link state
/// and dispatching to registered listeners when beats cross.
pub struct BeatClock {
    subscribers: Arc<Mutex<Vec<Subscription>>>,
    snapshot: Arc<Mutex<LinkSnapshot>>,
    _thread: thread::JoinHandle<()>,
}

impl BeatClock {
    /// Start the beat clock. Spawns a polling thread (~1ms resolution).
    pub fn new(quantum: f64) -> Self {
        let subscribers: Arc<Mutex<Vec<Subscription>>> = Arc::new(Mutex::new(Vec::new()));
        let subs = Arc::clone(&subscribers);
        let snapshot: Arc<Mutex<LinkSnapshot>> = Arc::new(Mutex::new(LinkSnapshot::default()));
        let snap = Arc::clone(&snapshot);

        let handle = thread::spawn(move || {
            let link = LinkController::new(quantum);
            let mut last_beat: Option<u64> = None;
            let mut was_playing = false;

            loop {
                for event in link.events() {
                    match event {
                        crate::link_controller::LinkEvent::PlayStateChanged(playing) => {
                            let subs = subs.lock().unwrap();
                            for sub in subs.iter() {
                                sub.listener.lock().unwrap().on_transport_change(playing);
                            }
                        }
                        _ => {}
                    }
                }

                let state = link.state();

                // Update snapshot every poll (~1ms).
                // Normalize phase from [0, quantum) to [0, 1).
                {
                    let mut s = snap.lock().unwrap();
                    s.tempo = state.tempo;
                    s.beat = state.beat;
                    s.phase = state.phase / quantum;
                    s.playing = state.playing;
                    s.num_peers = state.num_peers;
                }

                // Notify subscribers of play state changes,
                // including the initial state on first poll.
                if state.playing != was_playing {
                    let subs = subs.lock().unwrap();
                    for sub in subs.iter() {
                        sub.listener.lock().unwrap().on_transport_change(state.playing);
                    }
                }

                if state.playing {
                    let current_beat = state.beat.floor() as u64;

                    if let Some(prev) = last_beat {
                        if current_beat > prev {
                            let subs = subs.lock().unwrap();
                            for beat in (prev + 1)..=current_beat {
                                let info = BeatInfo { beat };
                                for sub in subs.iter() {
                                    if sub.pattern.matches(beat) {
                                        sub.listener.lock().unwrap().on_beat(&info);
                                    }
                                }
                            }
                        }
                    }

                    last_beat = Some(current_beat);
                } else if was_playing {
                    last_beat = None;
                }

                was_playing = state.playing;
                thread::sleep(Duration::from_millis(1));
            }
        });

        Self {
            subscribers,
            snapshot,
            _thread: handle,
        }
    }

    /// Get a shared reference to the continuously-updated Link snapshot.
    pub fn snapshot(&self) -> Arc<Mutex<LinkSnapshot>> {
        Arc::clone(&self.snapshot)
    }

    /// Register a listener with a beat pattern. Returns a handle that keeps
    /// the subscription alive.
    pub fn subscribe(
        &self,
        pattern: BeatPattern,
        listener: Arc<Mutex<dyn BeatListener>>,
    ) -> SubscriptionHandle {
        // Immediately notify the listener of the current playing state
        // so it doesn't miss an already-running session.
        {
            let snap = self.snapshot.lock().unwrap();
            let mut l = listener.lock().unwrap();
            l.on_transport_change(snap.playing);
        }

        let id = {
            let mut subs = self.subscribers.lock().unwrap();
            let id = subs.len();
            subs.push(Subscription { pattern, listener });
            id
        };
        SubscriptionHandle {
            id,
            subscribers: Arc::clone(&self.subscribers),
        }
    }
}

/// Dropping this removes the subscription.
pub struct SubscriptionHandle {
    id: usize,
    subscribers: Arc<Mutex<Vec<Subscription>>>,
}

impl Drop for SubscriptionHandle {
    fn drop(&mut self) {
        let mut subs = self.subscribers.lock().unwrap();
        if self.id < subs.len() {
            subs.swap_remove(self.id);
        }
    }
}
