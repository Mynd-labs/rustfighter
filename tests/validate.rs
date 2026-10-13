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
fn ani_tables_parse_and_handles_resolve() {
    use rustfighter::fight::ani::{build_hitboxes, AniTable};
    use rustfighter::gamedata::GameData;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let mut parsed = 0;
    for rec in gd.index.find_prefix("ani") {
        let name = rec.trimmed_name().to_string();
        let bytes = gd.decoded(&name).expect("ani decodes");
        let table = AniTable::parse(&name, &bytes).expect("ani parses");
        assert_eq!(table.frames.len(), 200, "{name} frame count");
        // Every sprite handle in every frame must resolve to a real record.
        for (fi, frame) in table.frames.iter().enumerate() {
            for &(handle, _off) in &frame.sprites {
                if handle == 0xFFF8 || handle == 0xF33A {
                    continue;
                }
                // Handles are only valid while loaded; 0 means absent in some frames.
                if handle == 0 {
                    continue;
                }
                assert!(
                    gd.record_for_handle(handle).is_some(),
                    "{name} frame {fi}: handle {handle:#06x} does not resolve"
                );
            }
        }
        parsed += 1;
    }
    assert_eq!(parsed, 12, "12 character ani tables");

    // Spot check: KEN frame 0 sprites are KENN sprite records.
    let ken = AniTable::parse("aniKEN", &gd.decoded("aniKEN").unwrap()).unwrap();
    let f0 = ken.frame(0).unwrap();
    let names: Vec<String> = f0
        .sprites
        .iter()
        .filter(|(h, _)| *h != 0xFFF8 && *h != 0xF33A && *h != 0)
        .filter_map(|(h, _)| gd.record_for_handle(*h))
        .map(|r| r.trimmed_name().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.starts_with("sprKENN")),
        "frame 0 sprites should include a KENN bank: {names:?}"
    );

    // Hitbox construction runs for all frames.
    for fi in 0..200 {
        let frame = ken.frame(fi).unwrap();
        let boxes = build_hitboxes(frame, 100, 200, false);
        for (i, b) in boxes.iter().enumerate() {
            if let Some(b) = b {
                assert!(b.right > b.left, "frame {fi} box {i} geometry");
                assert!(b.bottom > b.top, "frame {fi} box {i} geometry");
            }
        }
    }
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
