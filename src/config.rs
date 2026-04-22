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
    pub autosave_on_close: bool,
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            autosave_on_close: true,
            autoload_on_open: true,
            snap_to_grid: false,
            inspector_mode: InspectorMode::Auto,
            dmx_bypass_on_startup: false,
        }
    }
}

impl AppConfig {
    fn config_path() -> PathBuf {
        PathBuf::from(CONFIG_FILENAME)
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
}
