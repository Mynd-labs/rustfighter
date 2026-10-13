//! End-to-end validation against the original game files.
//!
//! These tests run only when the game data is available; set `SF2_DIR` or place
//! the release in `./street-fighter-ii`. They mirror the Python validations from
//! the reverse-engineering phase: every index record must decode to its declared
//! size, and every sprite record must expand to `ceil(w*h/2)` bytes.

use rustfighter::gamedata::{find_game_dir, GameData};

fn data() -> Option<GameData> {
    let dir = find_game_dir()?;
    match GameData::load(&dir) {
        Ok(gd) => Some(gd),
        Err(e) => {
            eprintln!("skip: cannot load game data from {}: {e}", dir.display());
            None
        }
    }
}

#[test]
fn every_record_decodes_to_declared_size() {
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    assert_eq!(gd.index.records.len(), 3749, "unexpected record count");
    let mut decoded = 0usize;
    let mut failures = Vec::new();
    for rec in &gd.index.records {
        match gd.decoded_record(rec) {
            Ok(_) => decoded += 1,
            Err(e) => failures.push(format!("{}: {e}", rec.trimmed_name())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} record(s) failed: {:?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
    assert_eq!(decoded, 3749);
}

#[test]
fn every_sprite_expands_exactly() {
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let mut ok = 0usize;
    let mut failures = Vec::new();
    for rec in gd.index.find_prefix("spr") {
        match gd.sprite(rec.trimmed_name()) {
            Ok(_) => ok += 1,
            Err(e) => failures.push(format!("{}: {e}", rec.trimmed_name())),
        }
    }
    assert!(
        failures.is_empty(),
        "{} sprite(s) failed: {:?}",
        failures.len(),
        &failures[..failures.len().min(10)]
    );
    assert_eq!(ok, 3223, "expected 3223 sprite records");
}

#[test]
fn stages_assemble_with_expected_slices() {
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let keys = gd.stage_keys();
    assert!(keys.len() >= 15, "expected at least 15 stage families, got {keys:?}");
    for key in keys {
        let stage = gd.load_stage(&key).unwrap_or_else(|e| panic!("stage {key}: {e}"));
        assert_eq!(stage.height, 232, "stage {key} height");
        assert_eq!(stage.width % 64, 0, "stage {key} width");
        assert_eq!(stage.pixels.len(), stage.width * 232);
    }
}

#[test]
fn screens_have_plausible_geometry() {
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let screens = gd.screens();
    // The release contains six full-screen `lin` records (the rest are strips,
    // portraits and outro fragments).
    assert!(screens.len() >= 6, "expected at least 6 screens, got {}", screens.len());
    for want in ["linSELECMAP", "linINTROBOT", "linEASYMESS"] {
        assert!(
            screens.iter().any(|s| s.name == want),
            "expected screen {want} in {:?}",
            screens.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }
    for info in &screens {
        assert!(info.width >= 256);
        assert!(info.height >= 40);
    }
}
