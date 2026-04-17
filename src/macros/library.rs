//! Manages the on-disk macro library: scans the directory, exposes a flat
//! list of metadata-only entries the UI can browse.

use std::path::PathBuf;

use super::{Macro, MACRO_EXTENSION};

/// One macro discovered in the library, with metadata cached but the actual
/// graph loaded on demand.
#[derive(Debug, Clone)]
pub struct MacroEntry {
    /// Absolute path to the `.lbm` file.
    pub path: PathBuf,
    /// Group path relative to the library root, "/"-separated. Empty for the
    /// root group.
    pub group: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub creator: String,
    pub date: String,
}

pub struct LibraryManager {
    pub root: PathBuf,
    pub entries: Vec<MacroEntry>,
    /// Last error encountered during a scan. Surfaced in the UI.
    pub last_error: Option<String>,
}

impl LibraryManager {
    pub fn new(root: PathBuf) -> Self {
        let mut mgr = Self { root, entries: Vec::new(), last_error: None };
        mgr.rescan();
        mgr
    }

    /// Re-scan the library directory and rebuild `entries`.
    pub fn rescan(&mut self) {
        self.entries.clear();
        self.last_error = None;
        if !self.root.exists() {
            return;
        }
        let root = self.root.clone();
        if let Err(e) = self.scan_dir(&root, String::new()) {
            self.last_error = Some(e);
        }
        // Sort by group then name for stable display.
        self.entries.sort_by(|a, b| {
            a.group.cmp(&b.group).then_with(|| a.name.cmp(&b.name))
        });
    }

    fn scan_dir(&mut self, dir: &std::path::Path, group: String) -> Result<(), String> {
        let read = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in read {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|e| e.to_string())?;
            if file_type.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                let sub_group = if group.is_empty() { name } else { format!("{}/{}", group, name) };
                let _ = self.scan_dir(&path, sub_group);
            } else if path.extension().and_then(|e| e.to_str()) == Some(MACRO_EXTENSION) {
                match Macro::load_from_file(&path) {
                    Ok(m) => self.entries.push(MacroEntry {
                        path: path.clone(),
                        group: group.clone(),
                        name: m.name,
                        description: m.description,
                        tags: m.tags,
                        creator: m.creator,
                        date: m.date,
                    }),
                    Err(e) => eprintln!("Failed to read macro '{}': {}", path.display(), e),
                }
            }
        }
        Ok(())
    }

    /// Compute the on-disk file path for a (group, name) pair.
    pub fn path_for(&self, group: &str, name: &str) -> PathBuf {
        let mut p = self.root.clone();
        for segment in group.split('/').filter(|s| !s.is_empty()) {
            p = p.join(segment);
        }
        p = p.join(format!("{}.{}", sanitize_filename(name), MACRO_EXTENSION));
        p
    }

    /// Delete a macro from disk and refresh.
    pub fn delete(&mut self, path: &std::path::Path) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        self.rescan();
        Ok(())
    }
}

/// Make a filename safe by replacing path separators and other troublesome
/// characters. Whitespace is preserved (just trimmed).
pub fn sanitize_filename(name: &str) -> String {
    let mut s: String = name.chars()
        .map(|c| match c {
            '/' | '\\' | '\0' => '_',
            c => c,
        })
        .collect();
    s = s.trim().to_string();
    if s.is_empty() { s = "untitled".to_string(); }
    s
}
