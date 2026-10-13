//! Build script: when the `embedded` feature is enabled, packs the game data
//! named by `SF2_DIR` into `$OUT_DIR/assets.bin` for `include_bytes!`.
//!
//! File format (`RFA1`):
//!
//! ```text
//! magic   "RFA1"
//! u32     entry count (little-endian)
//! repeat: u16 name length, name bytes, u32 data length, data bytes
//! ```

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=SF2_DIR");
    if std::env::var_os("CARGO_FEATURE_EMBEDDED").is_none() {
        return;
    }
    let dir = std::env::var("SF2_DIR").unwrap_or_else(|_| {
        panic!("building with `--features embedded` requires SF2_DIR=<folder with the game files>")
    });
    let dir = PathBuf::from(&dir);
    println!("cargo:rerun-if-changed={}", dir.display());

    let wanted = [
        "INDEX.SF2",
        "SPRITE1.SF2",
        "SPRITE2.SF2",
        "GENERAL.SF2",
        "BACKDROP.SF2",
        "CONFIG.SF2",
        "HIGHSCOR.SF2",
        "EXTRAS.SF2",
        "SBDRIVER.DRV",
    ];
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for name in wanted {
        let Some(path) = find_case_insensitive(&dir, name) else {
            panic!("{} not found in {}", name, dir.display());
        };
        entries.push((
            name.to_string(),
            std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display())),
        ));
    }

    let mut blob: Vec<u8> = b"RFA1".to_vec();
    blob.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for (name, data) in &entries {
        blob.extend_from_slice(&(name.len() as u16).to_le_bytes());
        blob.extend_from_slice(name.as_bytes());
        blob.extend_from_slice(&(data.len() as u32).to_le_bytes());
        blob.extend_from_slice(data);
    }
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("assets.bin"), &blob).expect("write assets.bin");
    println!(
        "cargo:warning=embedded {} game files ({:.1} MB)",
        entries.len(),
        blob.len() as f64 / (1024.0 * 1024.0)
    );
}

fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            return Some(entry.path());
        }
    }
    None
}
