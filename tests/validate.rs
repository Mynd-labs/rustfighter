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
fn ait_sections_and_damage_values_pin_the_real_data() {
    use rustfighter::fight::ait::AitTable;
    use rustfighter::gamedata::{engine_char_index, GameData, ENGINE_CHARACTERS};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    // Engine character order is the DS:0x60 handle-table order.
    assert_eq!(engine_char_index("KENN"), Some(0));
    assert_eq!(engine_char_index("BISO"), Some(11));
    assert_eq!(engine_char_index("ZANG"), Some(4));
    for rec in gd.index.find_prefix("ait") {
        let name = rec.trimmed_name().to_string();
        let table = AitTable::parse(&name, &gd.decoded(&name).unwrap()).expect("ait parses");
        assert!(table.power_table().is_some(), "{name} power table");
        assert!(table.score_table().is_some(), "{name} score table");
        assert!(table.lockout_table().is_some(), "{name} lockout table");
    }
    // KEN's power table (hdr[5]) values at attack ids 0x34..0x37 are the
    // byte-verified data from the analysis.
    let ait = AitTable::parse("aitKEN", &gd.decoded("aitKEN").unwrap()).unwrap();
    let power = ait.power_table().unwrap();
    assert_eq!(
        [power[0x34], power[0x35], power[0x36], power[0x37]],
        [13, 8, 19, 0]
    );
    let score = ait.score_table().unwrap();
    assert_eq!(
        [score[0x34], score[0x35], score[0x36]],
        [150, 150, 150]
    );
}

#[test]
fn brain_runs_on_real_tables() {
    use rustfighter::fight::brain::{FightAssets, FightState};
    use rustfighter::fight::rng::Rng;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("all 36 gameplay tables load");
    assert_eq!(assets.frm.len(), 12);
    assert_eq!(assets.ani.len(), 12);
    assert_eq!(assets.ait.len(), 12);

    // KEN (index 0) vs RYU (index 3): 600 frames of idle input + AI off.
    let mut st = FightState::new(0, 3, 0);
    st.rng = Rng::seeded(0x1234, 0x5678);
    st.prime(&assets);
    for _ in 0..600 {
        st.run_frame(&assets, [0, 0], [false, false]);
    }
    assert_eq!(st.players[0].energy, 100);
    assert_eq!(st.players[1].energy, 100);
    for p in &st.players {
        assert!(p.x >= 0 && p.x <= 0x110, "x out of stage: {}", p.x);
    }
    // The action/script state must be coherent after 600 frames.
    for i in 0..2 {
        let script_id = (st.objects[i].hotspot / 4) as usize;
        assert!(script_id < 64, "script id out of range: {script_id}");
        let frame = st.objects[i].frame as usize;
        assert!(frame < 200, "ani index out of range: {frame}");
    }

    // Reaction path end-to-end with real data: P2 is hit by a KEN attack 0x34.
    // KEN power[0x34] = 13 -> ((13*2*0xD9)>>8)>>1 = 11 damage; score 150.
    st.players[1].flags_a |= rustfighter::fight::hit::WAS_HIT;
    st.players[1].reaction_anim = 0x34;
    let before = st.players[1].energy;
    st.run_frame(&assets, [0, 0], [false, false]);
    assert_eq!(st.players[1].energy, before - 11, "react damage");
    assert_eq!(st.score[1], 150, "reaction score");
    // KO latches when energy reaches zero.
    st.players[1].energy = 0;
    st.run_frame(&assets, [0, 0], [false, false]);
    assert!(st.players[1].flags_a & rustfighter::fight::hit::KO_FLAG != 0);
    assert!(st.ko_flag);
}

#[test]
fn fireball_spawns_and_hits_with_real_tables() {
    use rustfighter::fight::brain::{FightAssets, FightState};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    // RYU as P1 (engine index 3), KEN as P2; RYU faces right and fires.
    let mut st = FightState::new(3, 0, 0);
    st.prime(&assets);
    // Let the stand script finish so the animation lock (obj[0x38] bit 2)
    // clears, exactly as the engine requires before a new action can start.
    for _ in 0..90 {
        st.run_frame(&assets, [0, 0], [false, false]);
    }
    // Move the opponent to the far wall so the fireball has flight time
    // (a fireball can legitimately trade with an opponent's reach box).
    st.players[1].x = 0x110;
    st.debug_action = Some((0, 0x35));
    let mut spawned = false;
    for _ in 0..300 {
        // AI off: with working walks, KEN would close in and interrupt the
        // scripted fireball the test is measuring.
        st.run_frame(&assets, [0, 0], [false, false]);
        eprintln!(
            "dbg: action={:#04x} sub={:#04x} mode={:#04x} scale={:#06x} flags38={:#04x} x={} proj={}",
            st.objects[0].action,
            st.objects[0].sub_action,
            st.objects[0].mode,
            st.objects[0].speed_scale,
            st.objects[0].flags38,
            st.players[0].x,
            st.projectiles.len()
        );
        if !st.projectiles.is_empty() {
            spawned = true;
            break;
        }
    }
    st.debug_action = None;
    assert!(spawned, "fireball must spawn during action 0x35");
    let proj = st.projectiles[0];
    assert_eq!(proj.owner, 0);
    assert_eq!(proj.x_speed, 0x1E, "facing right");
    assert_eq!(proj.action, 0x35);
    // Let it fly into KEN: he gets the was-hit bit (reaction next frame),
    // then the real RYU power[0x35] = 8 -> ((8*2*217)>>8)>>1 = 6 damage.
    let before = st.players[1].energy;
    for _ in 0..60 {
        st.run_frame(&assets, [0, 0], [false, false]);
    }
    assert!(st.players[1].energy < before, "fireball must damage KEN");
    assert!(st.projectiles.is_empty(), "fireball removed after the hit");
}

#[test]
fn real_input_fires_hadouken_through_the_recogniser() {
    use rustfighter::fight::brain::{FightAssets, FightState};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    let mut st = FightState::new(0, 3, 0); // KEN (P1) vs RYU
    st.prime(&assets);
    for _ in 0..30 {
        st.run_frame(&assets, [0, 0], [false, false]);
    }
    // QCF + punch: down, down-right, right, then hold punch (0x10|0x20).
    let script: [(u8, usize); 5] = [(0x00, 10), (0x02, 3), (0x0A, 3), (0x08, 3), (0x30, 30)];
    let mut fired = false;
    'outer: for (byte, dur) in script {
        for _ in 0..dur {
            st.run_frame(&assets, [byte, 0], [false, false]);
            eprintln!(
                "byte {byte:#04x}: action={:#04x} sub={:#04x} mode={:#04x} scale={:#06x} flags38={:#04x} x={} proj={}",
                st.objects[0].action,
                st.objects[0].sub_action,
                st.objects[0].mode,
                st.objects[0].speed_scale,
                st.objects[0].flags38,
                st.players[0].x,
                st.projectiles.len()
            );
            if !st.projectiles.is_empty() {
                fired = true;
                break 'outer;
            }
        }
    }
    assert!(
        fired,
        "QCF+P input must be recognised as action 0x35 and spawn a fireball"
    );
    assert_eq!(st.objects[0].action, 0x35);
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

#[test]
fn held_right_walks_p1() {
    use rustfighter::fight::brain::{FightAssets, FightState};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    let mut st = FightState::new(0, 3, 0); // KEN (P1) vs RYU
    st.prime(&assets);
    let x0 = st.players[0].x;
    // Measure the first 8 frames: the walk frames step 4 px each (`0x10AB1`).
    for f in 0..8 {
        st.run_frame(&assets, [0x08, 0], [false, false]);
        eprintln!(
            "f{f}: action={:#04x} facing={:#04x} x={} x_speed={}",
            st.objects[0].action,
            st.objects[0].facing,
            st.players[0].x,
            st.objects[0].x_speed
        );
    }
    let x1 = st.players[0].x;
    let dx = (x1 - x0) as f32;
    assert!(
        (25.0..=50.0).contains(&dx),
        "walk distance after 8 frames {dx} out of range (x {x0} -> {x1}, action {:#04x})",
        st.objects[0].action
    );
    for _ in 0..120 {
        st.run_frame(&assets, [0x08, 0], [false, false]);
    }
    // Left: 0x40 must walk the other way (facing turns first).
    let xr = st.players[0].x;
    for _ in 0..60 {
        st.run_frame(&assets, [0x04, 0], [false, false]);
    }
    assert!(
        st.players[0].x < xr,
        "P1 should walk left after the turn: {} -> {}",
        xr,
        st.players[0].x
    );
}

#[test]
fn jump_directions_follow_the_action() {
    use rustfighter::fight::brain::{FightAssets, FightState};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    for (action, expect_neg) in [(0x43u8, true), (0x44, false)] {
        let mut st = FightState::new(0, 3, 0); // KEN faces right
        st.prime(&assets);
        for _ in 0..90 {
            st.run_frame(&assets, [0, 0], [false, false]);
        }
        st.debug_action = Some((0, action));
        let mut seen = None;
        for _ in 0..10 {
            st.run_frame(&assets, [0, 0], [false, false]);
            if st.objects[0].flags38 & 0x80 != 0 {
                seen = Some(st.objects[0].x_speed);
                break;
            }
        }
        st.debug_action = None;
        let speed = seen.unwrap_or_else(|| panic!("action {action:#04x} must activate"));
        assert_eq!(
            speed < 0,
            expect_neg,
            "action {action:#04x}: x_speed {speed} (negative expected: {expect_neg})"
        );
    }
}

#[test]
fn walk_script_headers_probe() {
    use rustfighter::fight::brain::FightAssets;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    for action in [0x40u8, 0x41, 0x42, 0x43, 0x44, 0x35] {
        let Some((script, param)) = assets.ait[0].script_for_action(action) else {
            println!("action {action:#04x}: no script");
            continue;
        };
        let hdr = assets.frm[0].script((script / 4) as usize);
        println!(
            "action {action:#04x}: script {script:#04x} param {param:#x} hdr flags {:#04x}",
            hdr.map(|h| h.flags).unwrap_or(0xFF)
        );
    }
}

#[test]
fn walk_frames_probe() {
    use rustfighter::fight::brain::FightAssets;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    // KEN: walk script 0x04 (id 1), jump 0x05 (id 1?) probe several scripts.
    for script_id in [1usize, 2, 3, 5] {
        let frames = assets.frm[0].frame_list(script_id).0;
        let desc: Vec<String> = frames
            .iter()
            .take(8)
            .map(|f| format!("({:#04x},dx{},dy{})", f.ani_index, f.dx, f.dy))
            .collect();
        println!("script {script_id}: {}", desc.join(" "));
    }
}

#[test]
fn melee_hit_damages_once_not_per_frame() {
    use rustfighter::fight::brain::{FightAssets, FightState};
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    let mut st = FightState::new(0, 3, 0); // KEN (P1) vs RYU
    st.prime(&assets);
    for _ in 0..90 {
        st.run_frame(&assets, [0, 0], [false, false]);
    }
    // Put RYU within punch range and punch (button-only input -> the
    // per-character normal attack from the ait tables).
    st.players[1].x = st.players[0].x + 0x30;
    st.players[1].y = st.players[0].y;
    let before = st.players[1].energy;
    for _ in 0..12 {
        st.run_frame(&assets, [0x30, 0], [false, false]);
    }
    let loss = before.saturating_sub(st.players[1].energy);
    assert!(loss > 0, "the punch must connect (energy {before} -> {})", st.players[1].energy);
    assert!(
        loss <= 25,
        "a single punch lost {loss} HP in 12 frames - the connect latch is not holding"
    );
}

#[test]
fn ai_table_probe() {
    use rustfighter::fight::brain::FightAssets;
    let Some(gd) = data() else {
        eprintln!("skip: game data not found");
        return;
    };
    let assets = FightAssets::load(&gd).expect("assets");
    // Section 3 = the AI distance-bucket table: 8 buckets x 3 states x 8 rolls.
    for char in [0usize, 3] {
        let Some(t) = assets.ait[char].section(3) else {
            println!("char {char}: no section 3");
            continue;
        };
        println!("char {char} section3 len={}", t.len());
        for bucket in 0..8usize {
            for state in 0..3usize {
                let idx = bucket * 24 + state * 8;
                let row: Vec<String> = (0..8)
                    .map(|r| format!("{:#04x}", t.get(idx + r).copied().unwrap_or(0xFF)))
                    .collect();
                println!("  b{bucket} s{state}: {}", row.join(" "));
            }
        }
    }
}
