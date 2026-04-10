use std::env;
use std::fs;
use std::path::Path;

fn main() {
    // Copy settings.json next to the output binary.
    let out_dir = env::var("OUT_DIR").unwrap();
    // OUT_DIR is like target/release/build/<pkg>/out — walk up to target/<profile>/
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("couldn't find target dir");

    let src = Path::new("settings.json");
    if src.exists() {
        let dst = target_dir.join("settings.json");
        if !dst.exists() {
            let _ = fs::copy(src, dst);
        }
    }

    println!("cargo:rerun-if-changed=settings.json");
}
