use std::env;
use std::fs;
use std::path::Path;

/// Tell the program which commit it is built from and, in a release build
/// made by CI, which release it is: the updater compares them with the
/// latest release.
fn stamp_version() {
    let commit = env::var("GITHUB_SHA").ok().filter(|c| !c.is_empty()).or_else(|| {
        let out = std::process::Command::new("git").args(["rev-parse", "HEAD"]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    });
    println!("cargo:rustc-env=LIGHTBEAT_COMMIT={}", commit.unwrap_or_default());
    if let Ok(release) = env::var("LIGHTBEAT_RELEASE") {
        println!("cargo:rustc-env=LIGHTBEAT_RELEASE={release}");
    }
    println!("cargo:rerun-if-env-changed=LIGHTBEAT_RELEASE");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=.git/HEAD");
}

fn main() {
    stamp_version();
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("couldn't find target dir");

    // Copy default config files next to the output binary (if not already there).
    for filename in &["settings.json", "setup.json"] {
        let src = Path::new(filename);
        if src.exists() {
            let dst = target_dir.join(filename);
            if !dst.exists() {
                let _ = fs::copy(src, dst);
            }
        }
        println!("cargo:rerun-if-changed={}", filename);
    }
}
