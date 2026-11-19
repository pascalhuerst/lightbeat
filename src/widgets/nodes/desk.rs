//! The Desk's own data: pages and the lock flag (the drawing is in
//! `graph/desk.rs`).

use serde::{Deserialize, Serialize};

/// One tab of the Desk. The id never changes; the order is the list's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeskPage {
    pub id: u64,
    pub name: String,
}

/// What the Desk keeps once per project: the pages and whether it is locked.
/// Saved at the root level (`ProjectFile::desk`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeskDoc {
    pub pages: Vec<DeskPage>,
    pub locked: bool,
    /// Ids already handed out, so that a deleted page's id is never given to
    /// a new one (its items would land on it).
    pub issued: u64,
}

impl DeskDoc {
    /// A project without a Desk writes nothing.
    pub fn is_default(&self) -> bool {
        self.pages.is_empty() && !self.locked
    }
}
