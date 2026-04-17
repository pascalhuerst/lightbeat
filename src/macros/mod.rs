//! Macros: a saveable, browsable library of subgraphs.
//!
//! - A `.lbm` file wraps a subgraph's inner graph (a `ProjectFile`) with
//!   metadata (name, group path, description, tags, creator, date).
//! - On disk: `~/.local/share/lightbeat/macros/<group>/[<subgroup>/...]/<name>.lbm`.
//! - The `LibraryManager` scans this directory and exposes a flat list of
//!   `MacroEntry` (metadata only); the actual graph is loaded on demand at
//!   instantiation time.
//! - Instantiating a macro creates a Subgraph node with the macro's inner
//!   graph remapped to fresh `NodeId`s.

pub mod library;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::nodes::meta::subgraph::SubgraphPortDef;
use crate::project::ProjectFile;

pub const MACRO_FORMAT_VERSION: u32 = 1;
pub const MACRO_EXTENSION: &str = "lbm";

/// On-disk representation of a saved macro.
#[derive(Clone, Serialize, Deserialize)]
pub struct Macro {
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// External input ports of the wrapping subgraph.
    #[serde(default)]
    pub inputs: Vec<SubgraphPortDef>,
    /// External output ports of the wrapping subgraph.
    #[serde(default)]
    pub outputs: Vec<SubgraphPortDef>,
    /// The subgraph's inner graph — same shape as `ProjectFile`.
    pub graph: ProjectFile,
}

fn default_format_version() -> u32 { MACRO_FORMAT_VERSION }

impl Macro {
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&s).map_err(|e| e.to_string())
    }
}

/// Default library root: `$HOME/.local/share/lightbeat/macros`.
pub fn default_library_root() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join("lightbeat").join("macros")
}

/// ISO-8601 timestamp suitable for the macro's `date` field. Uses the
/// system's UTC time via std (no chrono dep).
pub fn now_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Manual minimal ISO-8601 (UTC). For a polished date format we'd add
    // chrono, but this is fine for a metadata stamp.
    let (year, month, day, hour, minute, second) = unix_to_ymdhms(secs as i64);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, minute, second)
}

fn unix_to_ymdhms(secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    // Simple Gregorian conversion (good enough for far-future stamps).
    let days_per_400y = 146_097_i64;
    let days_per_100y = 36_524_i64;
    let days_per_4y = 1_461_i64;

    let mut secs_of_day = secs.rem_euclid(86_400);
    let mut days = secs.div_euclid(86_400);

    let hour = (secs_of_day / 3600) as u32; secs_of_day %= 3600;
    let minute = (secs_of_day / 60) as u32;
    let second = (secs_of_day % 60) as u32;

    // Days since 1970-01-01 → days since 0000-03-01 (shift epoch to 0000-03-01).
    days += 719_468;
    let era = days.div_euclid(days_per_400y);
    let doe = days - era * days_per_400y; // 0..146096
    let yoe = (doe - doe / days_per_4y + doe / days_per_100y - doe / (days_per_400y - 1)) / 365;
    let y = yoe as i32 + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (mp + (if mp < 10 { 3 } else { -9 })) as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d, hour, minute, second)
}
