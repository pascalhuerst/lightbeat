use std::env;
use std::fs;
use std::path::Path;

fn main() {
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
