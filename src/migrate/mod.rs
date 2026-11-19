//! Bringing saved files up to the format this build reads.
//!
//! Every kind of file has a `Schema`: an ordered list of steps, each of which
//! rewrites the JSON of one version into the next. A file is upgraded as plain
//! JSON before it is parsed into the program's types, so the loader, the nodes
//! and the widgets only ever know the current format and carry no code for
//! older ones. Steps only go upwards; a file from a newer build is refused.
//!
//! To change a format: append a `Step` to the schema's list (its `to` is the
//! last one plus one) and write it against the JSON as it was then. A step is
//! frozen history: it must not call into code that describes today's nodes,
//! because that code will change again and the step still has to produce what
//! version `to` looked like.

pub mod project;
pub mod setup;

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

const VERSION_KEY: &str = "version";

pub struct Step {
    /// The version a document has after this step.
    pub to: u32,
    pub apply: fn(&mut Value),
}

pub struct Schema {
    /// What the user calls the file: "project", "setup".
    pub name: &'static str,
    /// The version of a file that carries no version field.
    pub baseline: u32,
    pub steps: &'static [Step],
    /// Documents of the same schema nested in this one (a subgraph's inner
    /// graph). Each carries a version of its own and is upgraded on its own.
    pub nested: fn(&mut Value) -> Vec<&mut Value>,
}

pub fn no_nested(_: &mut Value) -> Vec<&mut Value> {
    Vec::new()
}

impl Schema {
    pub const fn current(&self) -> u32 {
        self.baseline + self.steps.len() as u32
    }

    fn version_of(&self, doc: &Value) -> Result<u32, String> {
        match doc.get(VERSION_KEY) {
            None | Some(Value::Null) => Ok(self.baseline),
            Some(v) => v
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| format!("the {} has a version that is not a number", self.name)),
        }
    }

    /// Upgrade `doc` and everything nested in it. Returns the lowest version
    /// that was found, which equals `current()` when nothing had to change.
    pub fn upgrade(&self, doc: &mut Value) -> Result<u32, String> {
        if !doc.is_object() {
            return Err(format!("not a LightBeat {}", self.name));
        }
        let from = self.version_of(doc)?;
        let current = self.current();
        if from > current {
            return Err(format!(
                "this {} was saved by a newer LightBeat (format {from}, this build reads up to {current}); update LightBeat to open it",
                self.name
            ));
        }
        for step in self.steps.iter().filter(|s| s.to > from) {
            (step.apply)(doc);
        }
        if from < current {
            doc[VERSION_KEY] = Value::from(current);
        }
        let mut lowest = from;
        for inner in (self.nested)(doc) {
            lowest = lowest.min(self.upgrade(inner)?);
        }
        Ok(lowest)
    }

    /// Parse a file's text, upgraded in memory. The second value is the
    /// version the text was in when it was older than this build's.
    pub fn parse<T: DeserializeOwned>(&self, json: &str) -> Result<(T, Option<u32>), String> {
        let mut doc: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let from = self.upgrade(&mut doc)?;
        let value = serde_json::from_value(doc).map_err(|e| e.to_string())?;
        Ok((value, (from < self.current()).then_some(from)))
    }

    /// Read a file. An older one is upgraded on disk as well: the original is
    /// copied to a backup named after its version first, and only when that
    /// copy exists is the file rewritten.
    pub fn load_file<T: DeserializeOwned + Serialize>(&self, path: &Path) -> Result<(T, Option<Migration>), String> {
        let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let (value, from) = self.parse::<T>(&json)?;
        let Some(from) = from else { return Ok((value, None)) };
        let backup = keep_original(path, from, &json).and_then(|backup| {
            let upgraded = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
            crate::paths::write_atomic(path, upgraded).map_err(|e| e.to_string())?;
            Ok(backup)
        });
        let migration = Migration { what: self.name, file: path.to_path_buf(), from, to: self.current(), backup };
        Ok((value, Some(migration)))
    }
}

/// What happened to a file that was older than this build's format.
pub struct Migration {
    what: &'static str,
    file: PathBuf,
    pub from: u32,
    pub to: u32,
    /// Where the original went, or why the file on disk was left as it was.
    pub backup: Result<PathBuf, String>,
}

impl Migration {
    pub fn message(&self) -> String {
        let Migration { what, from, to, .. } = self;
        let file = self.file.display();
        match &self.backup {
            Ok(backup) => format!(
                "The {what} {file} was in an older format ({from}) and has been converted to the current one ({to}). The original is kept as {}.",
                backup.display()
            ),
            Err(e) => format!(
                "The {what} {file} is in an older format ({from}) and was converted to the current one ({to}) for this session. The file itself was left as it is, because no backup of it could be written ({e})."
            ),
        }
    }
}

/// `project.json` at version 3 is kept as `project.v3.json`. A backup that is
/// already there with other contents is never overwritten; the name is
/// numbered instead.
fn keep_original(path: &Path, version: u32, contents: &str) -> Result<PathBuf, String> {
    let stem = path.file_stem().and_then(|s| s.to_str()).ok_or("the file has no name")?;
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    for n in 1..100 {
        let numbered = if n == 1 { String::new() } else { format!("-{n}") };
        let backup = path.with_file_name(format!("{stem}.v{version}{numbered}{ext}"));
        match std::fs::read_to_string(&backup) {
            Ok(existing) if existing == contents => return Ok(backup),
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                crate::paths::write_atomic(&backup, contents).map_err(|e| e.to_string())?;
                return Ok(backup);
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("too many backups of this version are already there".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Doc {
        version: u32,
        name: String,
        #[serde(default)]
        inner: Vec<Doc>,
    }

    fn rename_title(doc: &mut Value) {
        if let Some(title) = doc.as_object_mut().and_then(|o| o.remove("title")) {
            doc["name"] = title;
        }
    }

    fn shout(doc: &mut Value) {
        let name = doc["name"].as_str().unwrap_or_default().to_uppercase();
        doc["name"] = Value::from(name);
    }

    fn inner(doc: &mut Value) -> Vec<&mut Value> {
        doc.get_mut("inner").and_then(Value::as_array_mut).map(|a| a.iter_mut().collect()).unwrap_or_default()
    }

    const TEST: Schema = Schema {
        name: "test file",
        baseline: 0,
        steps: &[Step { to: 1, apply: rename_title }, Step { to: 2, apply: shout }],
        nested: inner,
    };

    #[test]
    fn every_schema_numbers_its_steps_without_gaps() {
        for schema in [&project::SCHEMA, &setup::SCHEMA, &TEST] {
            for (i, step) in schema.steps.iter().enumerate() {
                assert_eq!(step.to, schema.baseline + 1 + i as u32, "{} step {i}", schema.name);
            }
        }
    }

    #[test]
    fn only_the_steps_after_a_files_version_run() {
        let (doc, from): (Doc, _) = TEST.parse(r#"{"title": "a"}"#).unwrap();
        assert_eq!((doc.name.as_str(), doc.version, from), ("A", 2, Some(0)));
        // At version 1 the rename is behind it: a stray `title` is left alone.
        let (doc, from): (Doc, _) = TEST.parse(r#"{"version": 1, "title": "x", "name": "b"}"#).unwrap();
        assert_eq!((doc.name.as_str(), from), ("B", Some(1)));
        let (doc, from): (Doc, _) = TEST.parse(r#"{"version": 2, "name": "c"}"#).unwrap();
        assert_eq!((doc.name.as_str(), from), ("c", None));
    }

    #[test]
    fn nested_documents_are_upgraded_from_their_own_version() {
        let mut doc = json!({"version": 2, "name": "root", "inner": [{"version": 1, "name": "old"}, {"version": 2, "name": "new"}]});
        assert_eq!(TEST.upgrade(&mut doc).unwrap(), 1, "the oldest level is what gets reported");
        assert_eq!(doc["name"], "root");
        assert_eq!(doc["inner"][0], json!({"version": 2, "name": "OLD"}));
        assert_eq!(doc["inner"][1]["name"], "new");
    }

    #[test]
    fn a_file_from_a_newer_build_is_refused() {
        let e = TEST.parse::<Doc>(r#"{"version": 3, "name": "x"}"#).err().unwrap();
        assert!(e.contains("newer LightBeat"), "{e}");
        assert!(TEST.parse::<Doc>("[1, 2]").is_err());
        assert!(TEST.parse::<Doc>(r#"{"version": "two", "name": "x"}"#).is_err());
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lightbeat-migrate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_old_file_is_backed_up_under_its_version_and_rewritten() {
        let dir = temp_dir("backup");
        let path = dir.join("show.json");
        let original = r#"{"version": 1, "name": "a"}"#;
        std::fs::write(&path, original).unwrap();

        let (doc, migration): (Doc, _) = TEST.load_file(&path).unwrap();
        let migration = migration.expect("it was old");
        assert_eq!((migration.from, migration.to), (1, 2));
        let backup = migration.backup.clone().unwrap();
        assert_eq!(backup, dir.join("show.v1.json"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);
        assert!(migration.message().contains("show.v1.json"));
        assert_eq!(doc.name, "A");

        // The file is current now: opening it again changes nothing.
        let (again, migration): (Doc, _) = TEST.load_file(&path).unwrap();
        assert!(migration.is_none());
        assert_eq!(again, doc);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_backup_with_other_contents_is_not_overwritten() {
        let dir = temp_dir("numbered");
        let path = dir.join("show.json");
        std::fs::write(dir.join("show.v1.json"), "an earlier original").unwrap();
        std::fs::write(&path, r#"{"version": 1, "name": "a"}"#).unwrap();
        let (_, migration): (Doc, _) = TEST.load_file(&path).unwrap();
        assert_eq!(migration.unwrap().backup.unwrap(), dir.join("show.v1-2.json"));
        assert_eq!(std::fs::read_to_string(dir.join("show.v1.json")).unwrap(), "an earlier original");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_does_not_parse_after_the_upgrade_is_left_untouched() {
        let dir = temp_dir("invalid");
        let path = dir.join("show.json");
        let original = r#"{"version": 1, "name": "a", "inner": 5}"#;
        std::fs::write(&path, original).unwrap();
        assert!(TEST.load_file::<Doc>(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(!dir.join("show.v1.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
