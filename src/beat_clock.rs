use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::link_controller::LinkController;

/// Information delivered to listeners on each matching beat.
#[derive(Debug, Clone)]
pub struct BeatInfo {
    /// Absolute beat number since the timeline started.
    pub beat: u64,
    /// Current tempo in BPM.
    pub tempo: f64,
    /// Whether transport is currently playing.
    pub playing: bool,
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

    pub fn every_with_offset(n: u64, offset: u64) -> Self {
        Self { divisor: n, offset: offset % n }
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
    // Keep a handle so the thread lives as long as BeatClock does.
    _thread: thread::JoinHandle<()>,
}

impl BeatClock {
    /// Start the beat clock. Spawns a polling thread (~1ms resolution).
    pub fn new(quantum: f64) -> Self {
        let subscribers: Arc<Mutex<Vec<Subscription>>> = Arc::new(Mutex::new(Vec::new()));
        let subs = Arc::clone(&subscribers);

        let handle = thread::spawn(move || {
            let link = LinkController::new(quantum);
            let mut last_beat: Option<u64> = None;
            let mut was_playing = false;

            loop {
                // Drain link-level events (tempo/peers changes handled here if needed)
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

                if state.playing {
                    let current_beat = state.beat.floor() as u64;

                    // Detect beat crossings
                    if let Some(prev) = last_beat {
                        // Fire for every beat we crossed since the last poll
                        if current_beat > prev {
                            let subs = subs.lock().unwrap();
                            for beat in (prev + 1)..=current_beat {
                                let info = BeatInfo {
                                    beat,
                                    tempo: state.tempo,
                                    playing: true,
                                };
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
                    // Just stopped — reset so we re-sync on next play
                    last_beat = None;
                }

                was_playing = state.playing;
                thread::sleep(Duration::from_millis(1));
            }
        });

        Self {
            subscribers,
            _thread: handle,
        }
    }

    /// Register a listener with a beat pattern. Returns a handle that keeps
    /// the subscription alive.
    pub fn subscribe(
        &self,
        pattern: BeatPattern,
        listener: Arc<Mutex<dyn BeatListener>>,
    ) -> SubscriptionHandle {
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
