use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::objects::fixture::Fixture;
use crate::objects::group::Group;
use crate::objects::object::Object;
use crate::objects::output::OutputConfig;

const SETUP_FILENAME: &str = "setup.json";

// ---------------------------------------------------------------------------
// Setup file format
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default)]
pub struct SetupFile {
    /// Fixture templates (channel definitions).
    pub fixtures: Vec<Fixture>,
    /// Object instances (fixture type + address + interface).
    #[serde(default)]
    pub objects: Vec<Object>,
    pub interfaces: Vec<SavedInterface>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedInterface {
    pub id: u32,
    pub name: String,
    pub config: OutputConfig,
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// File I/O
// ---------------------------------------------------------------------------

pub fn setup_path() -> PathBuf {
    PathBuf::from(SETUP_FILENAME)
}

pub fn save_setup(setup: &SetupFile) -> Result<(), String> {
    let json = serde_json::to_string_pretty(setup).map_err(|e| e.to_string())?;
    std::fs::write(setup_path(), json).map_err(|e| e.to_string())
}

pub fn load_setup() -> Result<SetupFile, String> {
    let path = setup_path();
    if !path.exists() {
        return Ok(SetupFile::default());
    }
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}
