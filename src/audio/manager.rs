//! Owner of audio input streams + analyzer threads. Pattern mirrors
//! `InputControllerManager`: persistent configs in SetupFile, runtime state
//! kept here for the engine and UI to read.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::audio::analyzers::{AnalyzerConfig, AnalyzerInstance, AnalyzerKind, spawn_analyzer};
use crate::audio::backend::{
    AudioBackendKind, DEFAULT_SAMPLE_RATE, InputStream, StreamRequest, backend_for, mk_subscriber,
};
use crate::engine::types::{ParamDef, ParamValue};

/// How often the manager re-enumerates audio devices in `tick_reconnect`.
/// Enumerating ALSA can be slow and noisy (probes dsnoop/dmix), so we throttle.
const DEVICE_RESCAN_INTERVAL: Duration = Duration::from_secs(3);

// ---------- persistent config (SetupFile) ----------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioInputConfig {
    pub id: u32,
    pub name: String,
    /// Which audio backend owns this input. Defaults to Cpal so setups that
    /// predate the backend selector still load unchanged.
    #[serde(default)]
    pub backend: AudioBackendKind,
    /// Backend-specific device display name; empty = no mapping yet.
    pub device_name: String,
    #[serde(default)]
    pub sample_rate: Option<u32>,
    #[serde(default)]
    pub buffer_size_frames: Option<u32>,
    #[serde(default)]
    pub analyzers: Vec<AnalyzerConfig>,
}

// ---------- runtime --------------------------------------------------------

pub struct AudioInputRuntime {
    pub id: u32,
    pub name: String,
    pub backend: AudioBackendKind,
    pub device_name: String,
    pub sample_rate: Option<u32>,
    pub buffer_size_frames: Option<u32>,
    /// Desired analyzer kinds. Source of truth for reconcile and save.
    pub analyzer_kinds: Vec<AnalyzerKind>,
    /// Live analyzer instances. Populated by reconcile when the stream opens;
    /// kept in lockstep with `analyzer_kinds`.
    pub analyzers: Vec<AnalyzerInstance>,
    pub status: ConnectionStatus,
    /// Negotiated sample rate from the backend once the stream opens (0 until then).
    pub actual_sample_rate: u32,
    pub actual_buffer_frames: usize,
    /// Audio Input node id currently bound to this input (None = available).
    /// Used by the node inspector to enforce exclusive selection.
    pub bound_to: Option<u64>,
}

impl AudioInputRuntime {
    fn from_config(c: &AudioInputConfig) -> Self {
        Self {
            id: c.id,
            name: c.name.clone(),
            backend: c.backend,
            device_name: c.device_name.clone(),
            sample_rate: c.sample_rate,
            buffer_size_frames: c.buffer_size_frames,
            analyzer_kinds: c.analyzers.iter().map(|a| a.kind).collect(),
            analyzers: Vec::new(),
            status: ConnectionStatus::Disconnected,
            actual_sample_rate: 0,
            actual_buffer_frames: 0,
            bound_to: None,
        }
    }

    pub fn to_config(&self) -> AudioInputConfig {
        AudioInputConfig {
            id: self.id,
            name: self.name.clone(),
            backend: self.backend,
            device_name: self.device_name.clone(),
            sample_rate: self.sample_rate,
            buffer_size_frames: self.buffer_size_frames,
            analyzers: self
                .analyzer_kinds
                .iter()
                .map(|k| AnalyzerConfig::new(*k))
                .collect(),
        }
    }

    pub fn analyzer_param_defs(&self) -> Vec<ParamDef> {
        let mut defs = Vec::new();
        for (i, a) in self.analyzers.iter().enumerate() {
            let prefix = format!("a{}.", i);
            for mut def in a.current_params() {
                let name = match &mut def {
                    ParamDef::Float { name, .. } => name,
                    ParamDef::Int { name, .. } => name,
                    ParamDef::Bool { name, .. } => name,
                    ParamDef::Choice { name, .. } => name,
                };
                *name = format!("{}{}", prefix, name);
                defs.push(def);
            }
        }
        defs
    }

    /// Route a `set_param` call by global index to (analyzer_index, local_index).
    pub fn route_param(&self, global_index: usize) -> Option<(usize, usize)> {
        let mut acc = 0usize;
        for (ai, a) in self.analyzers.iter().enumerate() {
            let n = a.current_params().len();
            if global_index < acc + n {
                return Some((ai, global_index - acc));
            }
            acc += n;
        }
        None
    }

    pub fn set_param(&self, global_index: usize, value: ParamValue) {
        if let Some((ai, local)) = self.route_param(global_index) {
            self.analyzers[ai].set_param(local, value);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connected,
    Waiting,
}

pub type SharedAudioInputs = Arc<Mutex<Vec<AudioInputRuntime>>>;

// ---------- manager --------------------------------------------------------

pub struct AudioInputManager {
    pub shared: SharedAudioInputs,
    /// Active per-input streams. Dropped when the corresponding input is
    /// removed or its config (backend/device/rate/buffer) changes.
    streams: Vec<ActiveStream>,
    /// Cached device names per backend so we don't re-enumerate on every UI
    /// frame (backend enumeration can be slow and stderr-noisy).
    cached_devices: HashMap<AudioBackendKind, Vec<String>>,
    last_device_scan: Option<Instant>,
}

struct ActiveStream {
    input_id: u32,
    backend: AudioBackendKind,
    device_name: String,
    /// What the user *requested*. Compared against the desired config to
    /// decide whether the stream needs to be reopened. The backend may
    /// negotiate a different actual rate, but that's irrelevant for matching.
    requested_sample_rate: Option<u32>,
    buffer_size: Option<u32>,
    _stream: InputStream,
}

impl AudioInputManager {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Mutex::new(Vec::new())),
            streams: Vec::new(),
            cached_devices: HashMap::new(),
            last_device_scan: None,
        }
    }

    /// Common sample rates to offer in the UI. The backend negotiates the
    /// closest one the device actually supports — no per-device probing.
    pub const COMMON_SAMPLE_RATES: &'static [u32] = &[44_100, 48_000, 88_200, 96_000, 192_000];

    /// Mark `input_id` as bound to `node_id`, clearing any previous binding
    /// that node held. Used by the Audio Input node widget to enforce
    /// exclusive selection. Pass `input_id = 0` to release without binding
    /// to anything new.
    pub fn rebind(shared: &SharedAudioInputs, node_id: u64, input_id: u32) {
        let mut state = shared.lock().unwrap();
        for c in state.iter_mut() {
            if c.bound_to == Some(node_id) {
                c.bound_to = None;
            }
        }
        if input_id != 0
            && let Some(c) = state.iter_mut().find(|c| c.id == input_id) {
                c.bound_to = Some(node_id);
            }
    }

    /// Release `node_id`'s binding (called from widget Drop).
    pub fn release(shared: &SharedAudioInputs, node_id: u64) {
        let mut state = shared.lock().unwrap();
        for c in state.iter_mut() {
            if c.bound_to == Some(node_id) {
                c.bound_to = None;
            }
        }
    }

    /// Cached device names for a specific backend — used by the UI's device
    /// dropdown. Refreshed by `force_rescan` or the throttled scan in
    /// `tick_reconnect`.
    pub fn cached_devices_for(&self, backend: AudioBackendKind) -> &[String] {
        self.cached_devices.get(&backend).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// Force a fresh device enumeration across all backends (UI "Refresh" button).
    pub fn force_rescan(&mut self) {
        self.last_device_scan = Some(Instant::now());
        self.rescan_devices();
        self.reconcile();
    }

    fn rescan_devices(&mut self) {
        for kind in AudioBackendKind::ALL {
            let names = backend_for(*kind).list_input_names();
            self.cached_devices.insert(*kind, names);
        }
    }

    /// Replace the entire input set (called on setup load/undo/redo).
    pub fn set_inputs(&mut self, inputs: &[AudioInputConfig]) {
        self.streams.clear();
        let mut state = self.shared.lock().unwrap();
        *state = inputs.iter().map(AudioInputRuntime::from_config).collect();
        drop(state);
        self.reconcile();
    }

    pub fn export(&self) -> Vec<AudioInputConfig> {
        let state = self.shared.lock().unwrap();
        state.iter().map(AudioInputRuntime::to_config).collect()
    }

    pub fn add_input(&mut self, name: String) -> u32 {
        let mut state = self.shared.lock().unwrap();
        let id = state.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        state.push(AudioInputRuntime {
            id,
            name,
            backend: AudioBackendKind::default(),
            device_name: String::new(),
            sample_rate: Some(DEFAULT_SAMPLE_RATE),
            buffer_size_frames: None,
            analyzer_kinds: Vec::new(),
            analyzers: Vec::new(),
            status: ConnectionStatus::Disconnected,
            actual_sample_rate: 0,
            actual_buffer_frames: 0,
            bound_to: None,
        });
        drop(state);
        self.reconcile();
        id
    }

    pub fn remove_input(&mut self, id: u32) {
        let mut state = self.shared.lock().unwrap();
        state.retain(|c| c.id != id);
        drop(state);
        self.reconcile();
    }

    pub fn rename(&mut self, id: u32, name: String) {
        let mut state = self.shared.lock().unwrap();
        if let Some(c) = state.iter_mut().find(|c| c.id == id) {
            c.name = name;
        }
    }

    /// Switch the backend for an input. Clears the device-name mapping since
    /// device names aren't portable across backends.
    pub fn set_backend(&mut self, id: u32, backend: AudioBackendKind) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                if c.backend == backend { return; }
                c.backend = backend;
                c.device_name = String::new();
                c.status = ConnectionStatus::Disconnected;
                c.analyzers.clear();
            }
        }
        self.reconcile();
    }

    pub fn set_device(&mut self, id: u32, device_name: String) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                c.device_name = device_name;
                c.status = ConnectionStatus::Disconnected;
                c.analyzers.clear(); // analyzer threads are tied to a stream — start fresh
            }
        }
        self.reconcile();
    }

    pub fn set_sample_rate(&mut self, id: u32, rate: Option<u32>) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                c.sample_rate = rate;
                c.analyzers.clear();
            }
        }
        self.reconcile();
    }

    pub fn set_buffer_size(&mut self, id: u32, frames: Option<u32>) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == id) {
                c.buffer_size_frames = frames;
            }
        }
        self.reconcile();
    }

    pub fn add_analyzer(&mut self, input_id: u32, kind: AnalyzerKind) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == input_id) {
                c.analyzer_kinds.push(kind);
            }
        }
        // Streams are immutable after creation — drop the existing stream so
        // reconcile re-opens it with the new subscriber set.
        self.streams.retain(|s| s.input_id != input_id);
        self.reconcile();
    }

    pub fn remove_analyzer(&mut self, input_id: u32, analyzer_index: usize) {
        {
            let mut state = self.shared.lock().unwrap();
            if let Some(c) = state.iter_mut().find(|c| c.id == input_id)
                && analyzer_index < c.analyzer_kinds.len() {
                    c.analyzer_kinds.remove(analyzer_index);
                }
        }
        self.streams.retain(|s| s.input_id != input_id);
        self.reconcile();
    }

    /// Periodic reconcile. Re-enumerates devices at most every
    /// `DEVICE_RESCAN_INTERVAL`; between scans this is a no-op. Mutations
    /// (add/remove/change device) explicitly call `reconcile()` themselves
    /// so a fresh device list isn't required on every frame.
    pub fn tick_reconnect(&mut self) {
        let due = self
            .last_device_scan
            .map(|t| t.elapsed() >= DEVICE_RESCAN_INTERVAL)
            .unwrap_or(true);
        if !due {
            return;
        }
        // Only enumerate when there's an input that actually wants a stream
        // but doesn't have one yet (Waiting). Avoids periodic ALSA noise
        // when nothing is configured or everything is already connected.
        let has_pending = {
            let state = self.shared.lock().unwrap();
            state.iter().any(|c| {
                !c.device_name.is_empty() && !self.streams.iter().any(|s| s.input_id == c.id)
            })
        };
        if !has_pending {
            return;
        }
        self.last_device_scan = Some(Instant::now());
        self.rescan_devices();
        self.reconcile_internal();
    }

    fn reconcile(&mut self) {
        self.rescan_devices();
        self.last_device_scan = Some(Instant::now());
        self.reconcile_internal();
    }

    fn reconcile_internal(&mut self) {
        // Snapshot desired configs from the persistent kind list (not the
        // live `analyzers` vec — that's only populated after a successful
        // stream open).
        let desired: Vec<(u32, AudioBackendKind, String, Option<u32>, Option<u32>, Vec<AnalyzerKind>)> = {
            let state = self.shared.lock().unwrap();
            state
                .iter()
                .map(|c| {
                    (
                        c.id,
                        c.backend,
                        c.device_name.clone(),
                        c.sample_rate,
                        c.buffer_size_frames,
                        c.analyzer_kinds.clone(),
                    )
                })
                .collect()
        };

        // Drop streams whose configured request no longer matches (or whose
        // device disappeared). We compare the *request*, not the negotiated
        // rate — the backend may pick a different rate when the device can't
        // honor the request, and we don't want that to cause a respawn loop.
        let cached = self.cached_devices.clone();
        self.streams.retain(|s| {
            desired.iter().any(|(id, backend, name, sr, bs, _)| {
                *id == s.input_id
                    && *backend == s.backend
                    && *name == s.device_name
                    && *sr == s.requested_sample_rate
                    && *bs == s.buffer_size
                    && cached.get(&s.backend)
                        .map(|names| names.iter().any(|n| n == &s.device_name))
                        .unwrap_or(false)
            })
        });

        // Clear stale analyzer instances on inputs whose stream was just dropped.
        // Their threads exited when the stream closed (recv() returned Err
        // because the senders were dropped); the stale handles would otherwise
        // expose frozen output values.
        {
            let live_ids: Vec<u32> = self.streams.iter().map(|s| s.input_id).collect();
            let mut state = self.shared.lock().unwrap();
            for c in state.iter_mut() {
                if !live_ids.contains(&c.id) && !c.analyzers.is_empty() {
                    c.analyzers.clear();
                }
            }
        }

        // Open streams for desired configs that don't have one yet.
        for (id, backend, device_name, sr, bs, kinds) in &desired {
            if device_name.is_empty() {
                let mut state = self.shared.lock().unwrap();
                if let Some(c) = state.iter_mut().find(|c| c.id == *id) {
                    c.status = ConnectionStatus::Disconnected;
                }
                continue;
            }
            let already_open = self.streams.iter().any(|s| s.input_id == *id);
            if already_open {
                continue;
            }
            let available_for_backend = cached.get(backend).cloned().unwrap_or_default();
            if !available_for_backend.iter().any(|n| n == device_name) {
                let mut state = self.shared.lock().unwrap();
                if let Some(c) = state.iter_mut().find(|c| c.id == *id) {
                    c.status = ConnectionStatus::Waiting;
                }
                continue;
            }

            // Build subscribers — one per analyzer.
            let mut senders = Vec::with_capacity(kinds.len());
            let mut rxs = Vec::with_capacity(kinds.len());
            for _ in kinds {
                let (tx, rx) = mk_subscriber();
                senders.push(tx);
                rxs.push(rx);
            }

            let request = StreamRequest {
                sample_rate: *sr,
                buffer_size_frames: *bs,
            };
            let backend_impl = backend_for(*backend);
            match backend_impl.open_input(device_name, request, senders) {
                Ok(stream) => {
                    let actual_sr = stream.sample_rate;
                    let actual_bs = stream.observed_chunk_frames();

                    // Spawn analyzer instances.
                    let mut analyzers: Vec<AnalyzerInstance> = Vec::with_capacity(kinds.len());
                    for (kind, rx) in kinds.iter().zip(rxs) {
                        analyzers.push(spawn_analyzer(*kind, rx, actual_sr));
                    }

                    self.streams.push(ActiveStream {
                        input_id: *id,
                        backend: *backend,
                        device_name: device_name.clone(),
                        requested_sample_rate: *sr,
                        buffer_size: *bs,
                        _stream: stream,
                    });

                    let mut state = self.shared.lock().unwrap();
                    if let Some(c) = state.iter_mut().find(|c| c.id == *id) {
                        c.analyzers = analyzers;
                        c.status = ConnectionStatus::Connected;
                        c.actual_sample_rate = actual_sr;
                        c.actual_buffer_frames = actual_bs;
                    }
                }
                Err(e) => {
                    eprintln!("[audio/{}] open '{}': {}", backend.label(), device_name, e);
                    let mut state = self.shared.lock().unwrap();
                    if let Some(c) = state.iter_mut().find(|c| c.id == *id) {
                        c.status = ConnectionStatus::Waiting;
                    }
                }
            }
        }
    }
}
