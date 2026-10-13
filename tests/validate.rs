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

#[cfg(feature = "embedded")]
#[test]
fn embedded_assets_are_complete() {
    use rustfighter::gamedata::GameData;
    let gd = GameData::load_embedded().expect("embedded game data");
    assert!(gd.is_embedded());
    assert_eq!(gd.index.records.len(), 3749);
    let mut decoded = 0usize;
    for rec in &gd.index.records {
        gd.decoded_record(rec).expect("decode embedded record");
        decoded += 1;
    }
    assert_eq!(decoded, 3749);
}

#[test]
fn sound_assets_decode() {
    use rustfighter::sound::SoundBank;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let bank = SoundBank::load(&gd).expect("sound bank decodes");
    assert_eq!(bank.sfx.len(), 30, "30 VOC samples");
    assert_eq!(bank.tune.len(), 6, "6 TUNE tracks");
    assert_eq!(bank.at.len(), 6, "6 AT tracks");
    assert!(bank.rfx.is_some(), "shared RFX track");
    let buzz = bank.sfx.iter().find(|v| v.name == "vocBUZZ").unwrap();
    assert_eq!(buzz.blocks, 8, "vocBUZZ has 8 blocks");
    for voc in &bank.sfx {
        assert!(
            [4000, 5000, 5988, 6993, 8000, 10000].contains(&voc.rate_hz),
            "{} rate {}",
            voc.name,
            voc.rate_hz
        );
        assert!(!voc.samples.is_empty());
    }
    for smf in bank.tune.iter().chain(bank.at.iter()) {
        assert_eq!(smf.division, 96, "{} division", smf.name);
        assert!(!smf.events.is_empty(), "{} events", smf.name);
    }
    assert!(bank.speaker.len() >= 15, "tun* tables: {}", bank.speaker.len());
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
