//! `rfight` — the integrated native fight build.
//!
//! Runs the ported engine end to end: input (`CONFIG.SF2` bindings) → fighter
//! brain → hit resolution/damage → round loop, rendered with the real sprites,
//! stages and palettes.
//!
//! Controls (provisional until the input-recogniser decode lands):
//! * arrow keys — move/jump (P1), `Enter` punch, `Space` kick
//! * `1`–`4` — debug: trigger attack actions `0x34`–`0x37` directly
//! * `R` — reset the round, `F1` — toggle hitbox overlay, `Esc` — quit
//!
//! `--shot DIR` runs headless for N frames and dumps a PNG (verification).

use std::path::PathBuf;
use std::time::Duration;

use minifb::{Key, Window, WindowOptions};

use rustfighter::binfont;
use rustfighter::canvas::Canvas;
use rustfighter::fight::ani::build_hitboxes;
use rustfighter::fight::brain::{FightAssets, FightState};
use rustfighter::fight::render::{draw_ani_frame, install_character_palettes, PALETTE_BASE_P1, PALETTE_BASE_P2};
use rustfighter::fight::round::RoundOutcome;
use rustfighter::fight::select::{Dir, Select};
use rustfighter::fight::rng::Rng;
use rustfighter::audio::AudioOut;
use rustfighter::sound::SoundBank;
use rustfighter::gamedata::{find_game_dir, GameData, ENGINE_TABLE_STEMS};
use rustfighter::input::{Button, InputConfig, InputState};
use rustfighter::palette::Palette;

const W: usize = 320;
const H: usize = 200;
const GROUND: i16 = 200;

fn usage() -> ! {
    println!("usage: rfight [--game DIR] [--scale N] [--p1 STEM] [--p2 STEM] [--stage KEY] [--shot DIR] [--frames N]");
    std::process::exit(2);
}

fn main() {
    let mut game: Option<PathBuf> = None;
    let mut scale = 4usize;
    let mut p1 = "KEN".to_string();
    let mut p2 = "RYU".to_string();
    let mut stage_key: Option<String> = None;
    let mut shot: Option<PathBuf> = None;
    let mut frames: u32 = 90;
    let mut fireball_at: Option<u32> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game" => game = args.next().map(PathBuf::from),
            "--scale" => scale = args.next().and_then(|s| s.parse().ok()).unwrap_or(4).clamp(1, 8),
            "--p1" => p1 = args.next().unwrap_or_else(|| usage()).to_uppercase(),
            "--p2" => p2 = args.next().unwrap_or_else(|| usage()).to_uppercase(),
            "--stage" => stage_key = args.next(),
            "--shot" => shot = args.next().map(PathBuf::from),
            "--frames" => frames = args.next().and_then(|s| s.parse().ok()).unwrap_or(90),
            "--fireball-at" => fireball_at = args.next().and_then(|s| s.parse().ok()),
            "-h" | "--help" => usage(),
            "--version" => {
                println!("rustfighter {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
    }
    let gd = match game.or_else(find_game_dir) {
        Some(dir) => GameData::load(&dir).unwrap_or_else(|e| {
            eprintln!("failed to load game data from {}: {e}", dir.display());
            std::process::exit(1);
        }),
        None => GameData::load_embedded().unwrap_or_else(|_| {
            eprintln!("game files not found; pass --game, set SF2_DIR, or use a standalone build");
            std::process::exit(1);
        }),
    };
    let assets = FightAssets::load(&gd).unwrap_or_else(|e| {
        eprintln!("failed to load fight tables: {e}");
        std::process::exit(1);
    });
    let p1_index = ENGINE_TABLE_STEMS.iter().position(|&s| s == p1).unwrap_or(0);
    let p2_index = ENGINE_TABLE_STEMS.iter().position(|&s| s == p2).unwrap_or(3);
    let audio = AudioOut::new();
    let sound_bank = SoundBank::load(&gd).ok();
    let input_config = InputConfig::from_game_dir(&gd.dir);
    let stage_keys = gd.stage_keys();
    let stage_key = stage_key.unwrap_or_else(|| stage_keys.first().cloned().unwrap_or_default());

    let mut game_state = Game {
        gd,
        audio,
        sound_bank,
        assets,
        phase: Phase::Title,
        select: Select::new(false, 12),
        flags: Vec::new(),
        cursors: Vec::new(),
        fights: FightState::new(p1_index, p2_index, 0),
        stage_key,
        stage: None,
        stage_w: 0,
        input: input_config,
        keys: InputState::default(),
        banner: None,
        banner_timer: 0,
        hitbox_overlay: false,
        wins: [0, 0],
        round_no: 1,
        opponent_index: 0,
        continues_left: 3,
        continue_timer: 0,
        title_cursor: 0,
        title_logo: None,
        title_marker: None,
        title_message: None,
    };
    game_state.fights.objects[0].y = GROUND;
    game_state.fights.objects[1].y = GROUND;
    game_state.fights.ground_y = 0;
    game_state.load_stage();
    if let Some(stage) = &game_state.stage {
        game_state.fights.stage_max_pan = (stage.width as i32 - W as i32).max(0);
    }
    game_state.fights.rng = Rng::seeded(0x9E37, 0x79B9);
    game_state.fights.prime(&game_state.assets);
    game_state.load_select_assets();

    if let Some(dir) = shot {
        std::fs::create_dir_all(&dir).expect("shot dir");
        if std::env::var("SHOT_SELECT").is_ok() {
            game_state.phase = Phase::SelectScreen;
            game_state.load_select_assets();
            let canvas = game_state.render();
            let path = dir.join("select.png");
            canvas.save_png(&path).expect("save shot");
            println!("shot: {}", path.display());
            return;
        }
        if std::env::var("SHOT_FIGHT").is_ok() {
            game_state.phase = Phase::Fight;
        }
        for f in 0..frames {
            if fireball_at == Some(f) {
                game_state.fights.debug_action = Some((0, 0x35));
            }
            if fireball_at.map(|at| f > at && f <= at + 10) == Some(true) {
                game_state.fights.debug_action = Some((0, 0x35));
            }
            if fireball_at.map(|at| f > at + 10) == Some(true) {
                game_state.fights.debug_action = None;
            }
            game_state.step_frame([0u8; 2], [false, true]);
        }
        if fireball_at.is_some() {
            println!(
                "projectiles: {} sounds: {:?}",
                game_state.fights.projectiles.len(),
                game_state.fights.sound_events
            );
        }
        // Debug: state + sprite resolution for both fighters.
        for i in 0..2 {
            let obj = game_state.fights.objects[i];
            let char_index = (obj.char2 / 2) as usize;
            let frame = game_state.assets.ani[char_index]
                .frame(obj.frame as usize)
                .copied();
            let names: Vec<String> = frame
                .map(|f| {
                    [f.t_handle, f.b_handle]
                        .iter()
                        .filter(|h| **h != 0 && **h != 0xF33A)
                        .filter_map(|h| {
                            game_state
                                .gd
                                .record_for_handle(*h)
                                .map(|r| r.trimmed_name().to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            println!(
                "P{} char={} x={} y={} action={:#x} frame={} energy={} sprites={:?}",
                i + 1,
                char_index,
                obj.x,
                obj.y,
                obj.action,
                obj.frame,
                game_state.fights.players[i].energy,
                names
            );
        }
        let canvas = game_state.render();
        let path = dir.join("fight.png");
        canvas.save_png(&path).expect("save shot");
        println!("shot: {}", path.display());
        return;
    }

    let mut window = Window::new(
        "Rustfighter — fight",
        W,
        H,
        WindowOptions {
            resize: true,
            scale: match scale {
                1 => minifb::Scale::X1,
                2 => minifb::Scale::X2,
                8 => minifb::Scale::X8,
                _ => minifb::Scale::X4,
            },
            ..WindowOptions::default()
        },
    )
    .expect("cannot create window");
    window.set_target_fps(60);

    let mut buffer = vec![0u32; W * H];
    while window.is_open() {
        // Host keys -> engine scancodes (CONFIG.SF2 bindings).
        let mut down = [false; 256];
        // Capture presses as *events* (a fast tap between two frames must never
        // be lost, mirroring the DOS keyboard ISR) and polled held state.
        for (scancode, key) in SCANCODE_KEYS {
            down[scancode as usize] = window.is_key_down(key)
                || window.is_key_pressed(key, minifb::KeyRepeat::No);
        }
        game_state.keys.update(&game_state.input, |s| down[s as usize]);
        // Provisional raw byte for P1: direction bits (1/2/4/8) + edges.
        let mut raw = 0u8;
        if game_state.keys.is_held(0, Button::Up) {
            raw |= 0x01;
        }
        if game_state.keys.is_held(0, Button::Down) {
            raw |= 0x02;
        }
        if game_state.keys.is_held(0, Button::Left) {
            raw |= 0x04;
        }
        if game_state.keys.is_held(0, Button::Right) {
            raw |= 0x08;
        }
        // Bit table (docs/port/input-recogniser.md §2.1): 0x10 = attack held,
        // 0x20 = punch, 0x40 = kick.
        if game_state.keys.is_held(0, Button::Punch) {
            raw |= 0x10 | 0x20;
        }
        if game_state.keys.is_held(0, Button::Kick) {
            raw |= 0x10 | 0x40;
        }
        // Global quit: ESC held in Title or with Shift in Fight.
        if game_state.phase == Phase::Title
            && window.is_key_pressed(Key::Escape, minifb::KeyRepeat::No)
        {
            break;
        }
        if window.is_key_pressed(Key::Q, minifb::KeyRepeat::No)
            && window.is_key_down(Key::LeftCtrl)
        {
            break;
        }
        // Debug attack keys 1..4 (override the chooser for P1).
        game_state.fights.debug_action = None;
        for (i, key) in [Key::Key1, Key::Key2, Key::Key3, Key::Key4].iter().enumerate() {
            if window.is_key_pressed(*key, minifb::KeyRepeat::No) {
                game_state.fights.debug_action = Some((0, 0x34 + i as u8));
            }
            if window.is_key_down(*key) {
                // hold to chain the action's commits
                game_state.fights.debug_action = Some((0, 0x34 + i as u8));
            }
        }
        if window.is_key_pressed(Key::R, minifb::KeyRepeat::No) {
            game_state.reset_round();
        }
        if window.is_key_pressed(Key::F1, minifb::KeyRepeat::No) {
            game_state.hitbox_overlay = !game_state.hitbox_overlay;
        }
        match game_state.phase {
            Phase::Title => {
                if game_state.keys.just_pressed(0, Button::Up) {
                    game_state.title_cursor = (game_state.title_cursor + 3) % 4;
                    game_state.title_message = None;
                }
                if game_state.keys.just_pressed(0, Button::Down) {
                    game_state.title_cursor = (game_state.title_cursor + 1) % 4;
                    game_state.title_message = None;
                }
                // Accept the configured attack buttons *and* raw Enter/Space,
                // so a remapped CONFIG cannot trap the player on the title.
                if game_state.keys.just_pressed(0, Button::Punch)
                    || game_state.keys.just_pressed(0, Button::Kick)
                    || window.is_key_pressed(Key::Enter, minifb::KeyRepeat::No)
                    || window.is_key_pressed(Key::Space, minifb::KeyRepeat::No)
                {
                    match game_state.title_cursor {
                        // `0x14379` - GAME START.
                        0 => {
                            game_state.phase = Phase::SelectScreen;
                            game_state.load_select_assets();
                        }
                        // `0x1506F` / `0x14F7D` - not ported yet.
                        1 => {
                            game_state.title_message =
                                Some("V.S. BATTLE - NOT PORTED YET".to_string())
                        }
                        2 => {
                            game_state.title_message =
                                Some("OPTION MODE - NOT PORTED YET".to_string())
                        }
                        // `0x5076` - QUIT TO DOS.
                        _ => std::process::exit(0),
                    }
                }
            }
            Phase::SelectScreen => {
                let dbg = std::env::var_os("RF_DEBUG_SELECT").is_some();
                let before_cell = game_state.select.cells[0];
                if dbg {
                    for (name, btn) in [
                        ("UP", Button::Up),
                        ("DOWN", Button::Down),
                        ("LEFT", Button::Left),
                        ("RIGHT", Button::Right),
                    ] {
                        if game_state.keys.just_pressed(0, btn) {
                            eprintln!("select: {name} pressed (cell {before_cell})");
                        }
                    }
                }
                // P1 cursor: arrows move, punch/kick confirm, ESC aborts.
                if game_state.keys.just_pressed(0, Button::Up) {
                    game_state.select.move_cursor(0, Dir::Up);
                }
                if game_state.keys.just_pressed(0, Button::Down) {
                    game_state.select.move_cursor(0, Dir::Down);
                }
                if game_state.keys.just_pressed(0, Button::Left) {
                    game_state.select.move_cursor(0, Dir::Left);
                }
                if game_state.keys.just_pressed(0, Button::Right) {
                    game_state.select.move_cursor(0, Dir::Right);
                }
                if game_state.keys.just_pressed(0, Button::Punch)
                    || game_state.keys.just_pressed(0, Button::Kick)
                {
                    if game_state.select.confirm(0) {
                        let mut rng = game_state.fights.rng;
                        game_state.select.cpu_pick(&mut rng);
                        game_state.fights.rng = rng;
                        let (_, chars) = game_state.select.outcome();
                        game_state.start_fight(chars);
                    }
                }
                if dbg && game_state.select.cells[0] != before_cell {
                    let cell = game_state.select.cells[0];
                    let char2 = rustfighter::fight::select::CELL_TO_CHAR2[cell as usize];
                    let (mx, my, mdx, mdy) =
                        rustfighter::fight::select::MARKER_POS[(char2 / 2) as usize];
                    eprintln!(
                        "select: cell {before_cell} -> {cell} char2={char2:#04x} marker=({mx},{my}) d=({} ,{})",
                        mdx as i8, mdy as i8
                    );
                }
                if window.is_key_down(Key::Escape) {
                    game_state.phase = Phase::Title;
                }
            }
            Phase::Fight => {
                game_state.step_frame([raw, 0], [false, true]);
                if window.is_key_pressed(Key::Escape, minifb::KeyRepeat::No) {
                    game_state.phase = Phase::Title;
                }
            }
            Phase::Continue => {
                game_state.continue_timer = game_state.continue_timer.saturating_sub(1);
                let confirmed = game_state.keys.just_pressed(0, Button::Punch)
                    || game_state.keys.just_pressed(0, Button::Kick);
                if confirmed {
                    game_state.continues_left -= 1;
                    let c1 = (game_state.fights.objects[0].char2 / 2) as usize;
                    let c2 = game_state.opponent_index % 12;
                    game_state.start_fight_indices(c1, c2);
                } else if game_state.continue_timer == 0
                    || window.is_key_pressed(Key::Escape, minifb::KeyRepeat::No)
                {
                    game_state.phase = Phase::Title;
                    game_state.continues_left = 3;
                }
            }
        }
        let canvas = game_state.render();
        let rgb = canvas.to_rgb();
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            buffer[i] = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
        }
        window.update_with_buffer(&buffer, W, H).expect("update");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Title,
    SelectScreen,
    Fight,
    Continue,
}

/// The keys we feed through the scancode layer (CONFIG.SF2 defaults).
/// Title menu items (`DS:0x43DE`).
const TITLE_ITEMS: [&str; 4] = ["GAME START", "V.S. BATTLE", "OPTION MODE", "QUIT TO DOS"];

const SCANCODE_KEYS: [(u8, Key); 12] = [
    (0x48, Key::Up),
    (0x50, Key::Down),
    (0x4B, Key::Left),
    (0x4D, Key::Right),
    (0x1C, Key::Enter),
    (0x39, Key::Space),
    (0x10, Key::Q),
    (0x1E, Key::A),
    (0x2C, Key::Z),
    (0x2D, Key::X),
    (0x02, Key::Key1),
    (0x03, Key::Key2),
];

struct Game {
    gd: GameData,
    audio: Option<AudioOut>,
    sound_bank: Option<SoundBank>,
    assets: FightAssets,
    phase: Phase,
    select: Select,
    flags: Vec<Option<rustfighter::sprite::Frame>>,
    cursors: Vec<Option<rustfighter::sprite::Frame>>,
    fights: FightState,
    stage_key: String,
    stage: Option<rustfighter::gamedata::Stage>,
    stage_w: i32,
    input: InputConfig,
    keys: InputState,
    banner: Option<String>,
    banner_timer: u32,
    hitbox_overlay: bool,
    /// Round wins per player (`0x2A2C+2` / `0x2A32+2` battle records).
    wins: [u8; 2],
    /// Round number within the current bout (`0x2A3A`).
    round_no: u8,
    /// Opponent ladder index (`0x451A`-driven roster progression).
    opponent_index: usize,
    /// Remaining continues (`0x2A3E` = 3) and the countdown frames.
    continues_left: u8,
    continue_timer: u32,
    /// Title menu cursor (`0x1933B`, 0..4).
    title_cursor: usize,
    /// Decoded title logo (`linSF2BLOGO`, 288x128, drawn at (0x10,0)).
    title_logo: Option<Canvas>,
    /// Title cursor sprite (`sprSELETCUR`).
    title_marker: Option<rustfighter::sprite::Frame>,
    /// Status line for the entries that are not ported yet.
    title_message: Option<String>,
}

impl Game {
    fn load_select_assets(&mut self) {
        self.flags = rustfighter::fight::select::FLAG_SPRITES
            .iter()
            .map(|name| self.gd.sprite(name).ok())
            .collect();
        self.cursors = rustfighter::fight::select::CURSOR_SPRITES
            .iter()
            .map(|name| self.gd.sprite(name).ok())
            .collect();
        if std::env::var_os("RF_DEBUG_SELECT").is_some() {
            for (i, name) in rustfighter::fight::select::CURSOR_SPRITES.iter().enumerate() {
                let ok = self.cursors.get(i).map(|c| c.is_some()).unwrap_or(false);
                eprintln!("select: cursor sprite {name} loaded={ok}");
                if let Err(e) = self.gd.sprite(name) {
                    eprintln!("select:   load error: {e}");
                }
            }
        }
    }

    /// Build the fight from the select outcome (char2 values).
    fn start_fight(&mut self, chars: [u8; 2]) {
        let c1 = (chars[0] / 2) as usize;
        let c2 = (chars[1] / 2) as usize;
        self.opponent_index = c2;
        self.wins = [0, 0];
        self.round_no = 1;
        self.start_fight_indices(c1, c2);
    }

    /// Build (or rebuild) the fight for two character indices.
    fn start_fight_indices(&mut self, c1: usize, c2: usize) {
        self.fights = FightState::new(c1, c2, 0);
        self.fights.objects[0].y = GROUND;
        self.fights.objects[1].y = GROUND;
        self.fights.ground_y = 0;
        if let Some(stage) = &self.stage {
            self.fights.stage_max_pan = (stage.width as i32 - W as i32).max(0);
        }
        self.fights.rng = rustfighter::fight::rng::Rng::seeded(0x2B1D, 0x77C3);
        self.fights.prime(&self.assets);
        self.banner = None;
        self.phase = Phase::Fight;
        self.load_stage();
    }

    fn load_stage(&mut self) {
        if let Ok(stage) = self.gd.load_stage(&self.stage_key) {
            self.stage_w = stage.width as i32;
            self.stage = Some(stage);
        }
    }

    fn reset_round(&mut self) {
        self.fights.round = rustfighter::fight::round::Round::new();
        self.banner = None;
        self.banner_timer = 0;
        self.fights.players[0].energy = 100;
        self.fights.players[1].energy = 100;
        self.fights.players[0].x = 0x50;
        self.fights.players[1].x = 0xD0;
        self.fights.players[0].flags_a &= !rustfighter::fight::hit::KO_FLAG;
        self.fights.players[1].flags_a &= !rustfighter::fight::hit::KO_FLAG;
        self.fights.ko_flag = false;
    }

    /// One game frame: brain for both fighters, then round bookkeeping.
    fn step_frame(&mut self, inputs: [u8; 2], ai: [bool; 2]) {
        self.fights.run_frame(&self.assets, inputs, ai);
        // Drain the character-hook sound requests into the single-voice mixer
        // (a new sample cuts the previous one, like the DOS driver).
        if let (Some(audio), Some(bank)) = (&self.audio, &self.sound_bank) {
            for id in self.fights.sound_events.drain(..) {
                if let Some(voc) = bank.sfx.get(id as usize) {
                    audio.play_voc(voc);
                }
            }
        } else {
            self.fights.sound_events.clear();
        }
        if self.banner.is_none() {
            let energies = [self.fights.players[0].energy, self.fights.players[1].energy];
            if let Some(outcome) = self.fights.round.tick(energies, false) {
                // Battle records: winner takes a round win (`0x2A2C+2`).
                let winner = match outcome {
                    RoundOutcome::Winner { player } => Some(player),
                    RoundOutcome::TimeOver { winner } => winner,
                    RoundOutcome::Draw => None,
                };
                if let Some(p) = winner {
                    self.wins[p] = self.wins[p].saturating_add(1);
                }
                let text = if self.wins[0] >= 2 {
                    "YOU WIN THE BOUT".to_string()
                } else if self.wins[1] >= 2 {
                    "YOU LOSE - GAME OVER".to_string()
                } else {
                    match outcome {
                        RoundOutcome::Winner { player } => format!("PLAYER {} WINS THE ROUND", player + 1),
                        RoundOutcome::Draw => "DRAW GAME".to_string(),
                        RoundOutcome::TimeOver { winner: Some(p) } => {
                            format!("TIME OVER - PLAYER {} WINS", p + 1)
                        }
                        RoundOutcome::TimeOver { winner: None } => "TIME OVER - DRAW".to_string(),
                    }
                };
                self.banner = Some(text);
                self.banner_timer = 180;
            }
        } else {
            self.banner_timer = self.banner_timer.saturating_sub(1);
            if self.banner_timer == 0 {
                // Bout over? Next opponent, or back to the title on a loss.
                if self.wins[0] >= 2 {
                    self.wins = [0, 0];
                    self.round_no = 1;
                    // Ladder progression: next opponent in engine order, then
                    // wrap through the expanded roster (bosses last).
                    self.opponent_index = (self.opponent_index + 1) % 12;
                    let next = ENGINE_TABLE_STEMS[(self.opponent_index % 12).max(1)];
                    let c1 = (self.fights.objects[0].char2 / 2) as usize;
                    let c2 = ENGINE_TABLE_STEMS
                        .iter()
                        .position(|&s| s == next)
                        .unwrap_or(3);
                    self.start_fight_indices(c1, c2);
                } else if self.wins[1] >= 2 {
                    self.wins = [0, 0];
                    self.round_no = 1;
                    if self.continues_left > 0 {
                        self.phase = Phase::Continue;
                        self.continue_timer = 10 * 60; // 10 seconds
                    } else {
                        self.phase = Phase::Title;
                        self.continues_left = 3;
                    }
                } else {
                    self.round_no += 1;
                    self.reset_round();
                }
            }
        }
    }

    fn render(&mut self) -> Canvas {
        match self.phase {
            Phase::Title => return self.render_title(),
            Phase::SelectScreen => return self.render_select(),
            Phase::Continue => return self.render_continue(),
            Phase::Fight => {}
        }
        let mut canvas = Canvas::new(W, H);
        canvas.clear(0);
        // Character palettes from the real assets.
        let pal1 = self.character_palette(0);
        let pal2 = self.character_palette(1);
        // Stage first (behind everything), scrolled by the camera.
        if let Some(stage) = self.stage.clone() {
            let palette = self
                .gd
                .palette256(&format!("pal{}TOP0", stage.key))
                .or_else(|| self.gd.palette256("palGENERAL"))
                .unwrap_or_default();
            canvas.palette = palette;
            let camera = self.fights.camera_x as i32;
            let max_pan = (stage.width as i32 - W as i32).max(0);
            let pan = camera.clamp(0, max_pan);
            let y_off = (H as i32 - stage.height as i32) / 2;
            for sy in 0..stage.height {
                for sx in 0..W {
                    let sxx = sx as i32 + pan;
                    if sxx >= stage.width as i32 {
                        continue;
                    }
                    let px = stage.pixels[sy * stage.width + sxx as usize];
                    canvas.set(sx as i32, y_off + sy as i32, px);
                }
            }
            // Palette slots: replace the magenta sky holes.
            for c in canvas.palette.colors.iter_mut() {
                if rustfighter::palette::is_sky_key(*c) {
                    *c = [24, 30, 56];
                }
            }
        }
        install_character_palettes(&mut canvas, &pal1, &pal2);
        // Fighters.
        for i in 0..2 {
            let obj = self.fights.objects[i];
            let char_index = (obj.char2 / 2) as usize;
            let frame_index = obj.frame as usize;
            if let Some(frame) = self.assets.ani[char_index].frame(frame_index).copied() {
                let base = if i == 0 { PALETTE_BASE_P1 } else { PALETTE_BASE_P2 };
                let mut lookup = |h: u16| -> Option<rustfighter::sprite::Frame> {
                    self.gd
                        .record_for_handle(h)
                        .and_then(|r| self.gd.sprite(r.trimmed_name()).ok())
                };
                draw_ani_frame(
                    &mut canvas,
                    &frame,
                    obj.x,
                    obj.y,
                    obj.facing & 4 != 0,
                    base,
                    &mut lookup,
                );
            }
            if self.hitbox_overlay {
                let flipped = obj.facing & 4 != 0;
                if let Some(frame) = self.assets.ani[char_index].frame(frame_index) {
                    let boxes = build_hitboxes(frame, self.fights.players[i].x, self.fights.players[i].y, i == 0);
                    for b in boxes.iter().flatten() {
                        for x in b.left..=b.right {
                            canvas.set(x as i32, b.top as i32, 3);
                            canvas.set(x as i32, b.bottom as i32, 3);
                        }
                        for y in b.top..=b.bottom {
                            canvas.set(b.left as i32, y as i32, 3);
                            canvas.set(b.right as i32, y as i32, 3);
                        }
                    }
                }
                let _ = flipped;
            }
        }
        // Projectiles (per-kind sprite sets).
        for p in &self.fights.projectiles {
            let set = match p.kind {
                rustfighter::fight::chars::ProjKind::Fireball => &self.assets.fireball,
                rustfighter::fight::chars::ProjKind::Sonic => &self.assets.sonic,
                rustfighter::fight::chars::ProjKind::TigerShot => &self.assets.tiger,
            };
            let sprite = if set.is_empty() {
                None
            } else {
                Some(&set[p.anim as usize % set.len()])
            };
            if let Some(sprite) = sprite {
                let base = if p.owner == 0 { PALETTE_BASE_P1 } else { PALETTE_BASE_P2 };
                let x = p.x as i32 - sprite.width as i32 / 2;
                let y = p.y as i32 - sprite.height as i32 / 2;
                rustfighter::fight::render::draw_layer(&mut canvas, sprite, x, y, base);
            }
        }
        self.draw_hud(&mut canvas);
        canvas
    }

    fn character_palette(&self, i: usize) -> Palette {
        // Palette assets use the *sprite* code order (palKENNT001, ...).
        let sprite_code = rustfighter::gamedata::ENGINE_CHARACTERS
            [(self.fights.objects[i].char2 / 2) as usize];
        self.gd
            .palette16(&format!("pal{sprite_code}T001"))
            .unwrap_or_default()
    }

    fn render_title(&mut self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        canvas.clear(0);
        canvas.palette.colors.resize(256, [0, 0, 0]);
        canvas.palette.colors[254] = [255, 255, 255];
        // SF2 logo: `linSF2BLOGO` (288x128) at (0x10, 0) with `palSF2BLOGO`
        // (`0x15DC3`; also used by the credits and option screens).
        if self.title_logo.is_none() {
            self.title_logo = self
                .gd
                .screens()
                .into_iter()
                .find(|s| s.name == "linSF2BLOGO")
                .and_then(|info| self.gd.load_screen(&info).ok());
        }
        if let Some(logo) = &self.title_logo {
            for sy in 0..logo.height {
                for sx in 0..logo.width {
                    let px = logo.pixels[sy * logo.width + sx];
                    canvas.set(16 + sx as i32, sy as i32, px);
                }
            }
            canvas.palette = logo.palette.clone();
            for c in canvas.palette.colors.iter_mut() {
                if rustfighter::palette::is_sky_key(*c) {
                    *c = [0, 0, 0];
                }
            }
            canvas.palette.colors[254] = [255, 255, 255];
        }
        // The 4 items (`DS:0x43DE`) at CX=0x74, DX=0x88+0xC*i.
        for (i, item) in TITLE_ITEMS.iter().enumerate() {
            binfont::draw_text(&mut canvas, item, 116, 136 + 12 * i as i32, 254);
        }
        // Cursor at the current item (`0x193BB`, sprite `sprSELETCUR`).
        if self.title_marker.is_none() {
            self.title_marker = self.gd.sprite("sprSELETCUR").ok();
        }
        if let Some(marker) = &self.title_marker {
            let y = 136 + 12 * self.title_cursor as i32 - 4;
            rustfighter::fight::render::draw_layer(&mut canvas, marker, 98, y, 0x70);
        }
        // Bottom line: the hint, or the "not ported" note for the entry.
        let hint = self
            .title_message
            .as_deref()
            .unwrap_or("ARROWS MOVE  ENTER SELECT  ESC QUIT");
        binfont::draw_text(&mut canvas, hint, 30, 186, 254);
        canvas
    }

    fn render_continue(&mut self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        canvas.clear(0);
        canvas.palette.colors.resize(256, [0, 0, 0]);
        canvas.palette.colors[254] = [255, 255, 255];
        binfont::draw_text(&mut canvas, "CONTINUE?", 120, 70, 254);
        let secs = self.continue_timer / 60;
        binfont::draw_text(&mut canvas, &format!("{secs:2}"), 152, 90, 254);
        binfont::draw_text(&mut canvas, "PRESS ENTER/SPACE TO CONTINUE", 40, 120, 254);
        canvas
    }

    fn render_select(&mut self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        canvas.clear(0);
        canvas.palette.colors.resize(256, [0, 0, 0]);
        canvas.palette.colors[254] = [255, 255, 255];
        // World map.
        if let Some(info) = self.gd.screens().into_iter().find(|s| s.name == "linSELECMAP") {
            if let Ok(map) = self.gd.load_screen(&info) {
                let mx = 32i32;
                let my = 36i32;
                for sy in 0..map.height {
                    for sx in 0..map.width {
                        let px = map.pixels[sy * map.width + sx];
                        canvas.set(mx + sx as i32, my + sy as i32, px);
                    }
                }
                canvas.palette = map.palette.clone();
                for c in canvas.palette.colors.iter_mut() {
                    if rustfighter::palette::is_sky_key(*c) {
                        *c = [16, 16, 40];
                    }
                }
                canvas.palette.colors[254] = [255, 255, 255];
            }
        }
        // Flags for every character at their map markers.
        for (i, flag) in self.flags.iter().enumerate().take(12) {
            if let Some(flag) = flag {
                let (x, y, _dx, _dy) = rustfighter::fight::select::MARKER_POS[i];
                let fx = 32 + x as i32 - flag.width as i32 / 2;
                let fy = 36 + y as i32 - flag.height as i32 / 2;
                rustfighter::fight::render::draw_layer(&mut canvas, flag, fx, fy, 0x70);
            }
        }
        // Cursors at the markers of the current cells.
        for player in 0..2 {
            let cell = self.select.cells[player];
            let char2 = rustfighter::fight::select::CELL_TO_CHAR2[cell as usize];
            let (x, y, dx, dy) = rustfighter::fight::select::MARKER_POS[(char2 / 2) as usize];
            if let Some(cursor) = self.cursors.get(player).and_then(|c| c.as_ref()) {
                // `dx`/`dy` are signed bytes stored raw (`0x44E4`): cast via i8.
                let cx = 32 + x as i32 + (dx as i8) as i32 - cursor.width as i32 / 2;
                let cy = 36 + y as i32 + (dy as i8) as i32 - cursor.height as i32 / 2;
                rustfighter::fight::render::draw_layer(&mut canvas, cursor, cx, cy, 0x70);
            }
        }
        binfont::draw_text(&mut canvas, "PLAYER SELECT", 110, 4, 254);
        binfont::draw_text(&mut canvas, "1P", 8, 170, 254);
        binfont::draw_text(&mut canvas, "ARROWS MOVE  ENTER/SPACE PICK  ESC BACK", 30, 186, 254);
        if self.select.locked[0] {
            binfont::draw_text(&mut canvas, "READY", 270, 170, 254);
        }
        canvas
    }

    /// Provisional HUD: energy bars + timer digits (exact 0x1488D style pending).
    fn draw_hud(&mut self, canvas: &mut Canvas) {
        canvas.palette.colors.resize(256, [0, 0, 0]);
        canvas.palette.colors[254] = [255, 255, 255];
        canvas.palette.colors[253] = [80, 80, 80];
        canvas.palette.colors[250] = [255, 224, 0];
        canvas.palette.colors[251] = [128, 0, 0];
        canvas.palette.colors[252] = [48, 24, 8];
        // VITAL bars: yellow energy anchored at the outer edge over a dark
        // track, the lost part left red at the inner edge (arcade layout; the
        // exact `0x1488D` blit geometry is still to be decoded).
        const BAR_W: i32 = 104;
        for i in 0..2 {
            let energy = self.fights.players[i].energy.min(100) as i32;
            let yellow = (energy * BAR_W / 100).clamp(0, BAR_W);
            let rest = BAR_W - yellow;
            let x = if i == 0 { 16 } else { 312 - BAR_W };
            canvas.rect(x, 10, BAR_W, 7, 252);
            if i == 0 {
                canvas.rect(x + yellow, 10, rest, 7, 251);
                canvas.rect(x, 10, yellow, 7, 250);
            } else {
                canvas.rect(x, 10, rest, 7, 251);
                canvas.rect(x + rest, 10, yellow, 7, 250);
            }
        }
        // Timer digits with the game's digit font, masked to the HUD colour.
        if let Some(font) = self.gd.font("fntNUMB1419") {
            // HUD counts: the 200-unit counter displays as 99..0 (0x1488D
            // derives the digits from `DS:0x308A`; two units per count).
            let secs = (self.fights.round.timer as u32 / 2).min(99);
            let text = format!("{secs:02}");
            let mut cx = 148i32;
            for ch in text.bytes() {
                if let Some(glyph) = font.glyph(ch) {
                    for gy in 0..font.height {
                        for gx in 0..font.width {
                            if font.pixel(glyph, gx, gy) != 0 {
                                canvas.set(cx + gx as i32, 6 + gy as i32, 254);
                            }
                        }
                    }
                }
                cx += font.width as i32;
            }
        }
        if let Some(banner) = &self.banner {
            let banner = banner.clone();
            binfont::draw_text(canvas, &banner, 60, 80, 254);
        }
        // Wins markers (battle records) next to each label.
        let wins = format!("{}", self.wins[0]);
        let _ = wins;
        binfont::draw_text(
            canvas,
            &format!("WINS {}", self.wins[0]),
            4,
            176,
            254,
        );
        binfont::draw_text(
            canvas,
            &format!("WINS {}", self.wins[1]),
            240,
            176,
            254,
        );
        // Footer labels: fighters + stage.
        binfont::draw_text(
            canvas,
            &format!("P1 {}   P2 {}   {}", ENGINE_TABLE_STEMS[(self.fights.objects[0].char2 / 2) as usize], ENGINE_TABLE_STEMS[(self.fights.objects[1].char2 / 2) as usize], self.stage_key),
            4,
            188,
            254,
        );
    }
}