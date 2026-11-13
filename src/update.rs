//! Updating the app from its GitHub releases.
//!
//! CI publishes a rolling `beta` pre-release (made again on every push to
//! main, so its tag never changes: builds are told apart by commit) and a
//! release per `v*` tag. The app asks GitHub which one is newest on its
//! channel, and on request downloads the package for its platform, checks it
//! against the published SHA256SUMS and swaps it in. Nothing here runs unless
//! the user has update checks switched on or asks for one.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const REPO: &str = "pascalhuerst/lightbeat";
const SUMS_ASSET: &str = "SHA256SUMS";
const STAGING: &str = ".lightbeat-update";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Channel {
    /// Every push to main.
    #[default]
    Beta,
    /// Tagged releases without a suffix.
    Stable,
}

impl Channel {
    pub const ALL: [Channel; 2] = [Channel::Beta, Channel::Stable];

    pub fn label(self) -> &'static str {
        match self {
            Channel::Beta => "Beta",
            Channel::Stable => "Stable",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub tag: String,
    pub title: String,
    /// The commit it was built from.
    pub commit: String,
    pub notes: String,
    pub page: String,
    pub assets: Vec<Asset>,
}

pub fn parse_release(json: &serde_json::Value) -> Option<Release> {
    let text = |key: &str| json.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let assets = json
        .get("assets")?
        .as_array()?
        .iter()
        .filter_map(|a| {
            Some(Asset {
                name: a.get("name")?.as_str()?.to_string(),
                url: a.get("browser_download_url")?.as_str()?.to_string(),
            })
        })
        .collect();
    Some(Release {
        tag: json.get("tag_name")?.as_str()?.to_string(),
        title: text("name"),
        commit: text("target_commitish"),
        notes: text("body"),
        page: text("html_url"),
        assets,
    })
}

/// Numbers of a version such as "0.2.0-beta.3" for comparing; a release
/// without suffix is newer than the same version with one.
fn version_key(v: &str) -> (Vec<u64>, bool, Vec<u64>) {
    let v = v.trim_start_matches('v');
    let (core, suffix) = v.split_once('-').map_or((v, None), |(c, s)| (c, Some(s)));
    let nums = |s: &str| s.split(['.', '-']).filter_map(|p| p.parse::<u64>().ok()).collect::<Vec<_>>();
    (nums(core), suffix.is_none(), suffix.map(nums).unwrap_or_default())
}

/// Whether `release` is newer than the running build.
pub fn is_newer(release: &Release, channel: Channel, current_version: &str, current_commit: &str) -> bool {
    match channel {
        Channel::Beta => {
            // The rolling pre-release keeps its tag, so only the commit tells.
            !release.commit.is_empty()
                && !current_commit.is_empty()
                && !(release.commit.starts_with(current_commit) || current_commit.starts_with(&release.commit))
        }
        Channel::Stable => version_key(&release.tag) > version_key(current_version),
    }
}

/// The package for this platform: ("linux", "x86_64") or ("macos", "aarch64").
pub fn pick_asset<'a>(release: &'a Release, os: &str, arch: &str) -> Option<&'a Asset> {
    let (platform, ext) = match os {
        "linux" => (format!("linux-{arch}"), ".tar.gz"),
        // The packaging script names the Mac build after `uname -m`.
        "macos" => (format!("macos-{}", if arch == "aarch64" { "arm64" } else { arch }), ".zip"),
        _ => return None,
    };
    release.assets.iter().find(|a| a.name.contains(&platform) && a.name.ends_with(ext))
}

/// `sha256sum` output: "<hex>  <file>" per line.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            Some((name.trim().trim_start_matches('*').to_string(), hash.trim().to_lowercase()))
        })
        .collect()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(&format!("LightBeat/{}", crate::version::version()))
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(30))
        .build()
}

pub fn fetch_release(channel: Channel) -> Result<Release, String> {
    let url = match channel {
        Channel::Beta => format!("https://api.github.com/repos/{REPO}/releases/tags/beta"),
        Channel::Stable => format!("https://api.github.com/repos/{REPO}/releases/latest"),
    };
    let response = agent()
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => format!("there is no {} release yet", channel.label().to_lowercase()),
            other => other.to_string(),
        })?;
    let text = response.into_string().map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parse_release(&json).ok_or_else(|| "unexpected answer from GitHub".to_string())
}

/// Download `url` into `dest`, counting bytes in `progress`; returns the
/// SHA-256 of what was written.
fn download(url: &str, dest: &Path, progress: &Mutex<(u64, u64)>) -> Result<String, String> {
    let response = agent().get(url).call().map_err(|e| e.to_string())?;
    let total = response.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    *progress.lock() = (0, total);
    let mut reader = response.into_reader();
    let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress.lock().0 = done;
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// What gets replaced: the binary on Linux, the whole app bundle on a Mac
/// (replacing only the executable would break the bundle's signature, which
/// Apple Silicon insists on).
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// The file or folder to replace.
    pub path: PathBuf,
    pub is_bundle: bool,
}

impl Target {
    /// Work out what is running; an error says why it cannot be replaced.
    ///
    /// Worked out once, at the first call (at start): after an update the
    /// running file has been renamed, and the system would then report the
    /// path it has now.
    pub fn of_running_app() -> Result<Self, String> {
        static CACHE: std::sync::OnceLock<Result<Target, String>> = std::sync::OnceLock::new();
        CACHE
            .get_or_init(|| {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                Self::from_exe(&exe, cfg!(target_os = "macos"))
            })
            .clone()
    }

    pub fn from_exe(exe: &Path, macos: bool) -> Result<Self, String> {
        let in_build_dir = exe.components().any(|c| c.as_os_str() == "target")
            && exe.parent().is_some_and(|p| p.ends_with("release") || p.ends_with("debug"));
        if in_build_dir {
            return Err("this is a build from the source folder; update it with git and cargo".into());
        }
        if macos {
            // .../LightBeat.app/Contents/MacOS/lightbeat
            let bundle = exe.ancestors().nth(3).filter(|b| b.extension().is_some_and(|e| e == "app"));
            return bundle
                .map(|b| Self { path: b.to_path_buf(), is_bundle: true })
                .ok_or_else(|| "it was not started from LightBeat.app".to_string());
        }
        Ok(Self { path: exe.to_path_buf(), is_bundle: false })
    }

    fn staging_dir(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).join(STAGING)
    }

    fn old_path(&self) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(".old");
        self.path.with_file_name(name)
    }
}

/// Swap `new` (a binary, or a bundle) in for the target, keeping the old one
/// beside it until the next start. `new` must be on the same filesystem.
pub fn swap_in(target: &Target, new: &Path) -> Result<(), String> {
    let old = target.old_path();
    remove_any(&old);
    std::fs::rename(&target.path, &old).map_err(|e| format!("could not move the current version aside: {e}"))?;
    if let Err(e) = std::fs::rename(new, &target.path) {
        // Put it back so the app still starts.
        let _ = std::fs::rename(&old, &target.path);
        return Err(format!("could not put the new version in place: {e}"));
    }
    Ok(())
}

fn remove_any(path: &Path) {
    if path.is_dir() {
        let _ = std::fs::remove_dir_all(path);
    } else {
        let _ = std::fs::remove_file(path);
    }
}

/// Remove what the last update left behind. Called at start, when the new
/// version has proved it starts.
pub fn clean_up_leftovers() {
    if let Ok(target) = Target::of_running_app() {
        remove_any(&target.old_path());
        remove_any(&target.staging_dir());
    }
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    let out = std::process::Command::new(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Unpack a downloaded package and find the thing to install in it.
fn unpack(package: &Path, into: &Path, target: &Target) -> Result<PathBuf, String> {
    if target.is_bundle {
        run("ditto", &["-x".as_ref(), "-k".as_ref(), package.as_os_str(), into.as_os_str()])?;
        let app = into.join("LightBeat.app");
        if !app.is_dir() {
            return Err("the package has no LightBeat.app".into());
        }
        run("codesign", &["--verify".as_ref(), "--deep".as_ref(), app.as_os_str()])
            .map_err(|e| format!("the new app's signature is not valid ({e})"))?;
        Ok(app)
    } else {
        run("tar", &["-xzf".as_ref(), package.as_os_str(), "-C".as_ref(), into.as_os_str()])?;
        let binary = std::fs::read_dir(into)
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .map(|d| d.path().join("lightbeat"))
            .find(|p| p.is_file())
            .ok_or_else(|| "the package has no lightbeat binary".to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755));
        }
        Ok(binary)
    }
}

/// Download, check, unpack and swap in `release`. Errors leave the running
/// version as it was.
pub fn install(release: &Release, progress: &Mutex<(u64, u64)>) -> Result<(), String> {
    let target = Target::of_running_app()?;
    let asset = pick_asset(release, std::env::consts::OS, std::env::consts::ARCH)
        .ok_or_else(|| "this release has no package for this computer".to_string())?;
    let sums_url = release
        .assets
        .iter()
        .find(|a| a.name == SUMS_ASSET)
        .map(|a| a.url.clone())
        .ok_or_else(|| "this release has no checksums; open its page and update by hand".to_string())?;

    let stage = target.staging_dir();
    remove_any(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| format!("cannot write next to the app ({e}); update by hand"))?;

    let result = (|| {
        let sums = agent().get(&sums_url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
        let expected = parse_sums(&sums).remove(&asset.name).ok_or("the checksums do not list this package")?;
        let package = stage.join(&asset.name);
        let actual = download(&asset.url, &package, progress)?;
        if actual != expected {
            return Err("the download does not match its checksum".to_string());
        }
        let unpacked = stage.join("unpacked");
        std::fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
        let new = unpack(&package, &unpacked, &target)?;
        // Same folder as the app, so the swap is a rename.
        let ready = stage.join(if target.is_bundle { "LightBeat.app" } else { "lightbeat" });
        std::fs::rename(&new, &ready).map_err(|e| e.to_string())?;
        swap_in(&target, &ready)
    })();
    if result.is_err() {
        remove_any(&stage);
    }
    result
}

/// Start the app again once this process has exited. The waiter outlives us,
/// so it works whenever the window actually closes (including after a "save
/// changes?" question).
pub fn relaunch_after_exit() -> Result<(), String> {
    let target = Target::of_running_app()?;
    let pid = std::process::id().to_string();
    let (program, args): (PathBuf, Vec<std::ffi::OsString>) = if target.is_bundle {
        ("open".into(), vec!["-n".into(), target.path.clone().into_os_string()])
    } else {
        (target.path.clone(), Vec::new())
    };
    std::process::Command::new("sh")
        .args(["-c", "while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; shift; exec \"$@\"", "sh", &pid])
        .arg(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// State for the UI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Default)]
pub enum State {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    Installing,
    /// Installed; the app needs a restart to run it.
    Installed(Release),
    Failed(String),
}

/// Runs the network work on threads and keeps the answer for the UI.
#[derive(Clone, Default)]
pub struct Updater {
    state: Arc<Mutex<State>>,
    progress: Arc<Mutex<(u64, u64)>>,
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().clone()
    }

    /// Bytes done and total of the download in progress.
    pub fn progress(&self) -> (u64, u64) {
        *self.progress.lock()
    }

    /// Ask GitHub whether there is something newer. Does nothing while a
    /// check or install is under way.
    pub fn check(&self, channel: Channel) {
        {
            let mut s = self.state.lock();
            if matches!(*s, State::Checking | State::Installing) {
                return;
            }
            *s = State::Checking;
        }
        let state = self.state.clone();
        std::thread::spawn(move || {
            let next = match fetch_release(channel) {
                Ok(r) if is_newer(&r, channel, crate::version::version(), crate::version::commit()) => State::Available(r),
                Ok(_) => State::UpToDate,
                Err(e) => State::Failed(format!("could not check for updates: {e}")),
            };
            *state.lock() = next;
        });
    }

    pub fn install(&self, release: Release) {
        {
            let mut s = self.state.lock();
            if matches!(*s, State::Installing) {
                return;
            }
            *s = State::Installing;
        }
        *self.progress.lock() = (0, 0);
        let (state, progress) = (self.state.clone(), self.progress.clone());
        std::thread::spawn(move || {
            let next = match install(&release, &progress) {
                Ok(()) => State::Installed(release),
                Err(e) => State::Failed(format!("the update was not installed: {e}")),
            };
            *state.lock() = next;
        });
    }

    pub fn dismiss(&self) {
        let mut s = self.state.lock();
        if matches!(*s, State::Failed(_) | State::UpToDate) {
            *s = State::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, commit: &str, assets: &[&str]) -> Release {
        Release {
            tag: tag.into(),
            title: String::new(),
            commit: commit.into(),
            notes: String::new(),
            page: String::new(),
            assets: assets.iter().map(|n| Asset { name: n.to_string(), url: format!("https://x/{n}") }).collect(),
        }
    }

    #[test]
    fn a_release_is_read_from_the_github_answer() {
        let json = serde_json::json!({
            "tag_name": "beta", "name": "Beta (build 7, abc1234)", "target_commitish": "abc1234def",
            "body": "notes", "html_url": "https://github.com/x/y/releases/tag/beta",
            "assets": [{"name": "a.zip", "browser_download_url": "https://x/a.zip", "size": 1}]
        });
        let r = parse_release(&json).unwrap();
        assert_eq!((r.tag.as_str(), r.commit.as_str()), ("beta", "abc1234def"));
        assert_eq!(r.assets, vec![Asset { name: "a.zip".into(), url: "https://x/a.zip".into() }]);
        assert!(parse_release(&serde_json::json!({"message": "Not Found"})).is_none());
    }

    #[test]
    fn the_beta_channel_compares_commits_and_the_stable_one_versions() {
        let beta = release("beta", "abc1234def5678", &[]);
        assert!(!is_newer(&beta, Channel::Beta, "0.1.0-beta.3", "abc1234"), "same commit, short form");
        assert!(is_newer(&beta, Channel::Beta, "0.1.0-beta.2", "9999999"));
        assert!(!is_newer(&beta, Channel::Beta, "0.1.0", ""), "a build that does not know its commit never offers one");
        let stable = release("v0.2.0", "", &[]);
        assert!(is_newer(&stable, Channel::Stable, "0.1.0", ""));
        assert!(is_newer(&stable, Channel::Stable, "0.2.0-beta.9", ""), "the release follows its betas");
        assert!(!is_newer(&stable, Channel::Stable, "0.2.0", ""));
        assert!(!is_newer(&stable, Channel::Stable, "0.10.0", ""), "numbers, not text");
    }

    #[test]
    fn the_package_for_each_platform_is_picked_by_name() {
        let r = release("beta", "", &[
            "lightbeat-0.1.0-beta.7-linux-x86_64.tar.gz",
            "LightBeat-0.1.0-beta.7-macos-arm64.zip",
            "SHA256SUMS",
        ]);
        assert!(pick_asset(&r, "linux", "x86_64").unwrap().name.ends_with("linux-x86_64.tar.gz"));
        assert!(pick_asset(&r, "macos", "aarch64").unwrap().name.ends_with("macos-arm64.zip"));
        assert!(pick_asset(&r, "macos", "x86_64").is_none(), "no Intel build");
        assert!(pick_asset(&r, "windows", "x86_64").is_none());
    }

    #[test]
    fn checksums_are_read_from_sha256sum_output() {
        let sums = parse_sums("ABC123  one.zip\ndef456 *two.tar.gz\n\n");
        assert_eq!(sums["one.zip"], "abc123");
        assert_eq!(sums["two.tar.gz"], "def456");
    }

    #[test]
    fn the_running_app_is_replaceable_unless_it_is_a_source_build() {
        assert!(Target::from_exe(Path::new("/home/p/lightbeat/target/release/lightbeat"), false).is_err());
        let t = Target::from_exe(Path::new("/opt/lightbeat/lightbeat"), false).unwrap();
        assert!(!t.is_bundle && t.path == Path::new("/opt/lightbeat/lightbeat"));
        let t = Target::from_exe(Path::new("/Applications/LightBeat.app/Contents/MacOS/lightbeat"), true).unwrap();
        assert!(t.is_bundle && t.path == Path::new("/Applications/LightBeat.app"));
        assert!(Target::from_exe(Path::new("/usr/local/bin/lightbeat"), true).is_err());
    }

    #[test]
    fn swapping_keeps_the_old_version_beside_the_new_one_and_puts_it_back_on_failure() {
        let dir = std::env::temp_dir().join(format!("lightbeat-swap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = Target { path: dir.join("lightbeat"), is_bundle: false };
        std::fs::write(&target.path, "old").unwrap();
        let new = dir.join("new");
        std::fs::write(&new, "new").unwrap();

        swap_in(&target, &new).unwrap();
        assert_eq!(std::fs::read_to_string(&target.path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(dir.join("lightbeat.old")).unwrap(), "old");

        // A package that is missing leaves what runs in place.
        assert!(swap_in(&target, &dir.join("missing")).is_err());
        assert_eq!(std::fs::read_to_string(&target.path).unwrap(), "new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_linux_package_is_unpacked_to_its_binary() {
        let dir = std::env::temp_dir().join(format!("lightbeat-unpack-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let pack = dir.join("lightbeat-9.9.9-linux-x86_64");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("lightbeat"), "binary").unwrap();
        let tarball = dir.join("p.tar.gz");
        run("tar", &["-czf".as_ref(), tarball.as_os_str(), "-C".as_ref(), dir.as_os_str(), "lightbeat-9.9.9-linux-x86_64".as_ref()]).unwrap();
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();
        let target = Target { path: dir.join("lightbeat"), is_bundle: false };
        let binary = unpack(&tarball, &out, &target).unwrap();
        assert_eq!(std::fs::read_to_string(binary).unwrap(), "binary");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
