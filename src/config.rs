use serde::{Deserialize, Serialize};

use std::path::PathBuf;

const CONFIG_FILENAME: &str = "settings.json";

/// How the inspector side panel is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[derive(Default)]
pub enum InspectorMode {
    /// Always visible.
    Show,
    /// Always hidden.
    Hide,
    /// Visible only when at least one node is selected.
    #[default]
    Auto,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct AppConfig {
    /// If true, the app periodically writes a separate autosave file next to
    /// the project (for crash recovery) while there are unsaved changes. The
    /// autosave is never written over the user's project file — explicit
    /// Save / Save As is the only way to modify that.
    #[serde(alias = "autosave_on_close")]
    pub autosave_enabled: bool,
    pub autoload_on_open: bool,
    pub snap_to_grid: bool,
    #[serde(default)]
    pub inspector_mode: InspectorMode,
    /// If true, DMX output is bypassed (no traffic on the wire) when the app
    /// starts. Useful for venues where you want to wake up to silence and
    /// only flip the switch once you're sure the right project is loaded.
    /// Defaults to false — output is live on startup.
    #[serde(default)]
    pub dmx_bypass_on_startup: bool,
    /// Last known window size in logical points. Restored on startup so the
    /// window opens where the user left it. `None` = use default size.
    #[serde(default)]
    pub window_size: Option<(f32, f32)>,
    /// If true, the window was maximized when last closed and should open
    /// maximized again.
    #[serde(default)]
    pub window_maximized: bool,
    /// Most-recently-opened/saved project paths (front = newest). Capped
    /// and deduped by `push_recent_project`.
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    /// Frame rate while nothing is being interacted with. The UI is
    /// repainted at the display rate for a couple of seconds after any input
    /// and at this rate otherwise; 0 means always at the display rate. The
    /// canvas costs several milliseconds of CPU per frame, so halving the idle
    /// rate roughly halves the UI's share of a show running unattended.
    #[serde(default = "default_idle_repaint_hz")]
    pub idle_repaint_hz: f32,
    /// MSAA sample count for the window. 0 keeps egui's own edge smoothing
    /// ("feathering"), which doubles the triangles it tessellates; a value
    /// such as 4 turns that off and lets the GPU smooth edges instead, which
    /// cut the UI thread's CPU by about a third on the reference project.
    #[serde(default)]
    pub msaa_samples: u16,
}

fn default_idle_repaint_hz() -> f32 {
    30.0
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            autosave_enabled: true,
            autoload_on_open: true,
            snap_to_grid: false,
            inspector_mode: InspectorMode::Auto,
            dmx_bypass_on_startup: false,
            window_size: None,
            window_maximized: false,
            recent_projects: Vec::new(),
            idle_repaint_hz: default_idle_repaint_hz(),
            msaa_samples: 0,
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        crate::paths::data_file(CONFIG_FILENAME)
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
                Err(_) => Self::default(),
            }
        } else {
            Self::default()
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Add a project path to the recent list: move to front, dedupe by path,
    /// cap at `MAX_RECENT`.
    pub fn push_recent_project(&mut self, path: &std::path::Path) {
        const MAX_RECENT: usize = 8;
        self.recent_projects.retain(|p| p != path);
        self.recent_projects.insert(0, path.to_path_buf());
        if self.recent_projects.len() > MAX_RECENT {
            self.recent_projects.truncate(MAX_RECENT);
        }
    }
}
