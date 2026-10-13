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
fn character_tables_parse_and_chain_resolves() {
    use rustfighter::fight::ait::AitTable;
    use rustfighter::fight::ani::{build_hitboxes, AniTable};
    use rustfighter::fight::frm::FrmTable;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };

    // ani: 12 tables x 200 frames; every layer handle resolves to a record.
    let mut seen = 0;
    for rec in gd.index.find_prefix("ani") {
        let name = rec.trimmed_name().to_string();
        let table = AniTable::parse(&name, &gd.decoded(&name).unwrap()).expect("ani parses");
        assert_eq!(table.frames.len(), 200, "{name} frame count");
        for (fi, frame) in table.frames.iter().enumerate() {
            for handle in [frame.t_handle, frame.b_handle] {
                if handle == 0xF33A || handle == 0 {
                    continue;
                }
                assert!(
                    gd.record_for_handle(handle).is_some(),
                    "{name} frame {fi}: handle {handle:#06x} does not resolve"
                );
            }
        }
        seen += 1;
    }
    assert_eq!(seen, 12, "12 character ani tables");

    // frm: 12 tables x 64 scripts; every list terminates with valid ani indices.
    let mut seen = 0;
    for rec in gd.index.find_prefix("frm") {
        let name = rec.trimmed_name().to_string();
        let bytes = gd.decoded(&name).unwrap();
        let table = FrmTable::parse(&name, &bytes).expect("frm parses");
        assert_eq!(table.scripts.len(), 64, "{name} script count");
        for script_id in 0..64 {
            let (frames, terminated) = table.frame_list(script_id);
            assert!(terminated, "{name} script {script_id} list unterminated");
            assert!(!frames.is_empty(), "{name} script {script_id} empty list");
            for f in &frames {
                assert!(
                    f.ani_index < 200,
                    "{name} script {script_id}: ani index {}",
                    f.ani_index
                );
            }
        }
        seen += 1;
    }
    assert_eq!(seen, 12, "12 character frm tables");

    // ait: 12 tables; the action map section always starts at offset 22.
    let mut seen = 0;
    for rec in gd.index.find_prefix("ait") {
        let name = rec.trimmed_name().to_string();
        let table = AitTable::parse(&name, &gd.decoded(&name).unwrap()).expect("ait parses");
        assert_eq!(table.header[1], 22, "{name} action-map base");
        seen += 1;
    }
    assert_eq!(seen, 12, "12 character ait tables");

    // End-to-end KEN worked example: action 0x0B -> script 36 -> frames
    // 111..113 -> ani[111] = sprKENNT112 / sprKENNB112.
    let ait = AitTable::parse("aitKEN", &gd.decoded("aitKEN").unwrap()).unwrap();
    let (script, _param) = ait.script_for_action(0x0B).expect("action 0x0B maps");
    assert_eq!(script, 36);
    let frm = FrmTable::parse("frmKEN", &gd.decoded("frmKEN").unwrap()).unwrap();
    let (frames, terminated) = frm.frame_list(script as usize);
    assert!(terminated);
    let indices: Vec<u8> = frames.iter().map(|f| f.ani_index).collect();
    assert_eq!(&indices[..4], &[111, 112, 113, 113], "KEN script 36 frames");
    let ani = AniTable::parse("aniKEN", &gd.decoded("aniKEN").unwrap()).unwrap();
    let f111 = ani.frame(111).unwrap();
    let t = gd
        .record_for_handle(f111.t_handle)
        .unwrap()
        .trimmed_name()
        .to_string();
    let b = gd
        .record_for_handle(f111.b_handle)
        .unwrap()
        .trimmed_name()
        .to_string();
    assert_eq!(t, "sprKENNT112");
    assert_eq!(b, "sprKENNB112");

    // Hitboxes build on every KEN frame with sane geometry.
    for fi in 0..200 {
        let boxes = build_hitboxes(ani.frame(fi).unwrap(), 100, 200, false);
        for (i, b) in boxes.iter().enumerate() {
            if let Some(b) = b {
                assert!(b.right > b.left, "frame {fi} box {i} geometry");
                assert!(b.bottom > b.top, "frame {fi} box {i} geometry");
            }
        }
    }
}

#[test]
fn fighter_rendering_uses_real_ani_sprites() {
    use rustfighter::canvas::Canvas;
    use rustfighter::fight::ani::AniTable;
    use rustfighter::fight::render::{draw_ani_frame, install_character_palettes};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let ani = AniTable::parse("aniKEN", &gd.decoded("aniKEN").unwrap()).unwrap();
    let pal = gd.palette16("palKENNT001").expect("palKENNT001");
    let mut canvas = Canvas::new(320, 200);
    install_character_palettes(&mut canvas, &pal, &pal);
    let mut drawn_total = 0usize;
    for idx in [0usize, 111, 112, 113, 125, 199] {
        let frame = ani.frame(idx).unwrap();
        let mut lookup = |h: u16| -> Option<rustfighter::sprite::Frame> {
            gd.record_for_handle(h)
                .and_then(|r| gd.sprite(r.trimmed_name()).ok())
        };
        let stats = draw_ani_frame(&mut canvas, frame, 100, 150, false, 0x30, &mut lookup);
        drawn_total += stats.t_pixels + stats.b_pixels;
    }
    assert!(
        drawn_total > 100,
        "expected meaningful sprite pixels across KEN frames, got {drawn_total}"
    );
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
