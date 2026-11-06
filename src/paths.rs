//! Where LightBeat keeps its own files: `settings.json`, `setup.json` and the
//! default project.
//!
//! They used to be read from the working directory, which is right for
//! running from a checkout and wrong for an installed app: a macOS app starts
//! in `/`, and a Linux binary starts wherever the user happens to be. So the
//! working directory is still used when it already holds one of the files
//! (the way it is run during development), and otherwise a per-user
//! directory is.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Files whose presence in the working directory means "run from here".
const MARKERS: [&str; 3] = ["settings.json", "setup.json", "project.json"];

/// Where files go. Set `LIGHTBEAT_HOME` to choose it yourself.
pub fn data_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let cwd = std::env::current_dir().unwrap_or_default();
        let dir = choose(
            std::env::var_os("LIGHTBEAT_HOME"),
            MARKERS.iter().any(|m| cwd.join(m).exists()),
            std::env::var_os("HOME"),
            std::env::var_os("XDG_CONFIG_HOME"),
            cfg!(target_os = "macos"),
        );
        if !dir.as_os_str().is_empty() {
            // Best effort: a failure shows up when something is saved.
            let _ = std::fs::create_dir_all(&dir);
        }
        dir
    })
}

/// A file in the data directory.
pub fn data_file(name: &str) -> PathBuf {
    data_dir().join(name)
}

/// The directory to use; empty means the working directory.
fn choose(
    lightbeat_home: Option<OsString>,
    cwd_has_files: bool,
    home: Option<OsString>,
    xdg_config_home: Option<OsString>,
    macos: bool,
) -> PathBuf {
    if let Some(dir) = lightbeat_home.filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    if cwd_has_files {
        return PathBuf::new();
    }
    if macos {
        if let Some(home) = home.filter(|h| !h.is_empty()) {
            return Path::new(&home).join("Library/Application Support/LightBeat");
        }
    } else if let Some(xdg) = xdg_config_home.filter(|x| !x.is_empty()) {
        return Path::new(&xdg).join("lightbeat");
    } else if let Some(home) = home.filter(|h| !h.is_empty()) {
        return Path::new(&home).join(".config/lightbeat");
    }
    // No home to speak of: keep the old behaviour.
    PathBuf::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(env: Option<&str>, cwd: bool, home: Option<&str>, xdg: Option<&str>, macos: bool) -> PathBuf {
        choose(env.map(Into::into), cwd, home.map(Into::into), xdg.map(Into::into), macos)
    }

    #[test]
    fn an_explicit_home_wins_over_everything() {
        assert_eq!(dir(Some("/data"), true, Some("/home/u"), None, false), PathBuf::from("/data"));
        assert_eq!(dir(Some(""), false, Some("/home/u"), None, false), PathBuf::from("/home/u/.config/lightbeat"));
    }

    #[test]
    fn a_working_directory_that_has_the_files_is_used_as_it_always_was() {
        assert_eq!(dir(None, true, Some("/home/u"), None, false), PathBuf::new());
        assert_eq!(dir(None, true, Some("/Users/u"), None, true), PathBuf::new());
    }

    #[test]
    fn otherwise_each_platform_uses_its_own_place() {
        assert_eq!(dir(None, false, Some("/home/u"), None, false), PathBuf::from("/home/u/.config/lightbeat"));
        assert_eq!(dir(None, false, Some("/home/u"), Some("/home/u/cfg"), false), PathBuf::from("/home/u/cfg/lightbeat"));
        assert_eq!(
            dir(None, false, Some("/Users/u"), Some("/ignored"), true),
            PathBuf::from("/Users/u/Library/Application Support/LightBeat")
        );
    }

    #[test]
    fn without_a_home_the_working_directory_is_all_there_is() {
        assert_eq!(dir(None, false, None, None, false), PathBuf::new());
        assert_eq!(dir(None, false, Some(""), None, true), PathBuf::new());
    }
}
