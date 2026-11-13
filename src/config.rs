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


// A field added later must not wipe the saved settings of an older file.
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    /// If true, the app periodically writes a separate autosave file next to
    /// the project (for crash recovery) while there are unsaved changes. The
    /// autosave is never written over the user's project file — explicit
    /// Save / Save As is the only way to modify that.
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
    /// Ask GitHub for a newer release now and then. With this off the app
    /// makes no network request of its own accord.
    #[serde(default = "default_true")]
    pub check_for_updates: bool,
    #[serde(default)]
    pub update_channel: crate::update::Channel,
    /// How far ahead of the music every clock runs, in ms, to make up for the
    /// delay to the light. Kept with the settings, not the setup: it belongs
    /// to this room and this computer.
    #[serde(default)]
    pub latency_ms: f32,
    /// Which panels and windows are open, so the next start looks the same.
    #[serde(default = "default_true")]
    pub show_add_nodes: bool,
    #[serde(default)]
    pub show_setup: bool,
    #[serde(default)]
    pub show_dmx_monitor: bool,
    #[serde(default)]
    pub setup_tab: crate::app::panels::SetupTab,
}

fn default_true() -> bool {
    true
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
            check_for_updates: true,
            update_channel: crate::update::Channel::default(),
            latency_ms: 0.0,
            show_add_nodes: true,
            show_setup: false,
            show_dmx_monitor: false,
            setup_tab: crate::app::panels::SetupTab::default(),
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
                Ok(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                    eprintln!("settings: {} did not parse, using defaults: {e}", path.display());
                    Self::default()
                }),
                Err(e) => {
                    eprintln!("settings: could not read {}, using defaults: {e}", path.display());
                    Self::default()
                }
            }
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        crate::paths::write_atomic(&path, json).map_err(|e| {
            let message = format!("could not write {}: {e}", path.display());
            eprintln!("settings: {message}");
            message
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_settings_file_gets_defaults_for_new_fields() {
        let c: AppConfig = serde_json::from_str(r#"{"snap_to_grid": true}"#).unwrap();
        assert!(c.snap_to_grid);
        assert!(c.show_add_nodes);
        assert!(!c.show_setup);
        assert!(!c.show_dmx_monitor);
        assert_eq!(c.setup_tab, crate::app::panels::SetupTab::Interfaces);
        assert_eq!(c.idle_repaint_hz, 30.0);
    }

    #[test]
    fn panel_state_survives_a_round_trip() {
        let mut c = AppConfig::default();
        c.show_add_nodes = false;
        c.show_setup = true;
        c.show_dmx_monitor = true;
        c.setup_tab = crate::app::panels::SetupTab::AudioInputs;
        c.msaa_samples = 4;
        let back: AppConfig = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert!(!back.show_add_nodes && back.show_setup && back.show_dmx_monitor);
        assert_eq!(back.setup_tab, crate::app::panels::SetupTab::AudioInputs);
        assert_eq!(back.msaa_samples, 4);
    }
}
