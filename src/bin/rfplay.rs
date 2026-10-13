//! `rfplay` — native windowed player for the original game content (minifb).
//!
//! Starts in **attract mode** (cycles screens, characters and stages
//! automatically). Controls:
//!
//! * `1` screens, `2` characters, `3` stages, `4` fonts, `0` attract
//! * `Left`/`Right` previous/next item (frame step in character mode)
//! * `Up`/`Down` secondary selector (character, stage, variant, palette)
//! * `Space` pause/advance auto-play
//! * `V` sprite bank variant (B/T), `P` character palette
//! * `S` save a PNG screenshot (into your home directory)
//! * `Esc` quit
//!
//! If the original game files cannot be found, a native folder picker is
//! offered (zenity/kdialog); diagnostics are logged to
//! `~/.config/rustfighter/log.txt`.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use minifb::{Key, Window, WindowOptions};

use rustfighter::binfont;
use rustfighter::canvas::Canvas;
use rustfighter::gamedata::{find_game_dir, GameData, ScreenInfo, CHARACTERS};
use rustfighter::palette::Palette;

const W: usize = 320;
const H: usize = 200;
const SKY: [u8; 3] = [24, 30, 56];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Attract,
    Screens,
    Characters,
    Stages,
    Fonts,
}

/// Append one line to `~/.config/rustfighter/log.txt`.
fn log_line(message: &str) {
    let Some(home) = std::env::var_os("HOME") else { return };
    let path = PathBuf::from(home).join(".config/rustfighter/log.txt");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
        let _ = file.flush();
    }
}

fn usage() -> ! {
    println!("usage: rfplay [--game DIR] [--scale N] [--shot DIR]");
    std::process::exit(2)
}

fn main() {
    let mut game: Option<PathBuf> = None;
    let mut scale = 4usize;
    let mut shot: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game" => game = args.next().map(PathBuf::from),
            "--scale" => {
                scale = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(4)
                    .clamp(1, 8)
            }
            "--shot" => shot = args.next().map(PathBuf::from),
            "-h" | "--help" => usage(),
            "--version" | "-V" => {
                println!("rustfighter {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
    }
    log_line(&format!(
        "rfplay {} starting (args: {}, scale {scale})",
        env!("CARGO_PKG_VERSION"),
        std::env::args().skip(1).collect::<Vec<_>>().join(" ")
    ));

    let candidate = game.or_else(find_game_dir);

    // 1. external folder if one was given/found, 2. compiled-in data
    // (standalone builds), 3. folder picker as a last resort.
    let mut gd = candidate.as_deref().and_then(load_external_dir);
    let mut embedded = false;
    if gd.is_none() {
        match GameData::load_embedded() {
            Ok(loaded) => {
                log_line("using compiled-in game data (standalone build)");
                gd = Some(loaded);
                embedded = true;
            }
            Err(e) => log_line(&format!("no compiled-in game data: {e}")),
        }
    }
    if gd.is_none() {
        log_line("no game folder found automatically; opening folder picker");
        if let Some(dir) = pick_game_folder() {
            gd = load_external_dir(&dir);
        }
    }

    // Try to load; on failure show a real error window (or dump it headlessly).
    let Some(gd) = gd else {
        let lines = [
            "STREET FIGHTER II - FILES NOT FOUND",
            "",
            "Put the original game files (folder",
            "with INDEX.SF2 and SF2.EXE) somewhere",
            "easy - e.g. in your Downloads folder.",
            "Then start Rustfighter again.",
            "",
            "It also searches: next to itself,",
            "the current folder, ~/Downloads,",
            "~/Desktop, and ~/Games.",
            "",
            "Alternative: rfplay --game <FOLDER>",
            "or set SF2_DIR environment variable.",
            "",
            "Press ESC to close.   Details in:",
            "~/.config/rustfighter/log.txt",
        ];
        let canvas = error_canvas(&lines);
        if let Some(dir) = &shot {
            let _ = std::fs::create_dir_all(dir);
            let _ = canvas.save_png(&dir.join("error.png"));
            println!("shot: {}", dir.join("error.png").display());
            return;
        }
        error_window(canvas, scale);
        return;
    };

    if !embedded {
        rustfighter::gamedata::save_game_dir(&gd.dir);
    }

    let mut app = App::new(gd);
    if let Some(dir) = shot {
        std::fs::create_dir_all(&dir).expect("create shot dir");
        for (mode, name) in [
            (Mode::Screens, "screens"),
            (Mode::Characters, "characters"),
            (Mode::Stages, "stages"),
            (Mode::Fonts, "fonts"),
        ] {
            app.mode = mode;
            let canvas = app.render();
            let path = dir.join(format!("{name}.png"));
            canvas.save_png(&path).expect("save shot");
            println!("shot: {}", path.display());
        }
        return;
    }

    let mut window = match Window::new(
        "Rustfighter — Street Fighter II (DOS)",
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
    ) {
        Ok(window) => window,
        Err(e) => {
            log_line(&format!("cannot create window: {e}"));
            eprintln!("cannot create window: {e}");
            std::process::exit(1);
        }
    };
    window.set_target_fps(60);
    log_line("window created; entering main loop");

    let mut buffer = vec![0u32; W * H];
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut pressed = Vec::new();
        for key in [
            Key::Key0,
            Key::Key1,
            Key::Key2,
            Key::Key3,
            Key::Key4,
            Key::Left,
            Key::Right,
            Key::Up,
            Key::Down,
            Key::Space,
            Key::S,
            Key::P,
            Key::V,
        ] {
            if window.is_key_pressed(key, minifb::KeyRepeat::No) {
                pressed.push(key);
            }
        }
        app.handle_input(&pressed);
        app.tick();
        let canvas = app.render();
        let rgb = canvas.to_rgb();
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            buffer[i] = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
        }
        window
            .update_with_buffer(&buffer, W, H)
            .expect("window update");
        std::thread::sleep(Duration::from_millis(1));
    }
    log_line("exited");
}

/// Render a diagnostics canvas with the embedded fallback font.
fn error_canvas(lines: &[&str]) -> Canvas {
    let mut canvas = Canvas::new(W, H);
    canvas.palette = Palette {
        colors: vec![[12, 12, 24], [255, 255, 255], [255, 210, 80]],
    };
    canvas.clear(0);
    for (i, line) in lines.iter().enumerate() {
        let colour = if i == 0 { 2 } else { 1 };
        binfont::draw_text(&mut canvas, line, 4, 6 + i as i32 * 13, colour);
    }
    canvas
}

/// Show a diagnostics window until the user closes it.
fn error_window(canvas: Canvas, scale: usize) {
    let mut window = Window::new(
        "Rustfighter — setup needed",
        W,
        H,
        WindowOptions {
            resize: true,
            scale: match scale {
                1 => minifb::Scale::X1,
                2 => minifb::Scale::X2,
                8 => minifb::Scale::X8,
                _ => minifb::Scale::X2,
            },
            ..WindowOptions::default()
        },
    )
    .expect("cannot create window");
    let rgb = canvas.to_rgb();
    let mut buffer = vec![0u32; W * H];
    for (i, px) in rgb.chunks_exact(3).enumerate() {
        buffer[i] = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
    }
    while window.is_open() && !window.is_key_down(Key::Escape) {
        window.update_with_buffer(&buffer, W, H).expect("update");
        std::thread::sleep(Duration::from_millis(16));
    }
}

struct App {
    gd: GameData,
    mode: Mode,
    // screens
    screens: Vec<ScreenInfo>,
    screen_idx: usize,
    screen_pan: i32,
    // characters
    char_idx: usize,
    variant: char,
    palette_idx: usize,
    frames: Vec<String>,
    frame_idx: usize,
    // stages
    stages: Vec<String>,
    stage_idx: usize,
    stage_x: i32,
    // misc
    paused: bool,
    tick: u64,
    attract_phase: usize,
    attract_timer: u64,
    shot: u32,
    font_name: String,
}

impl App {
    fn new(gd: GameData) -> Self {
        let mut screens = gd.screens();
        let priority = |name: &str| -> usize {
            match name {
                "linSF2BLOGO" => 0,
                "linSELECMAP" => 1,
                "linINTROTOP" => 2,
                "linINTROBOT" => 3,
                "linEASYMESS" => 4,
                "linDRUGSPIC" => 5,
                _ => 9,
            }
        };
        screens.sort_by_key(|s| (priority(&s.name), s.name.clone()));
        let stages = gd.stage_keys();
        let mut app = App {
            frames: Vec::new(),
            gd,
            mode: Mode::Attract,
            screens,
            screen_idx: 0,
            screen_pan: 0,
            char_idx: 0,
            variant: 'B',
            palette_idx: 0,
            frame_idx: 0,
            stages,
            stage_idx: 0,
            stage_x: 0,
            paused: false,
            tick: 0,
            attract_phase: 0,
            attract_timer: 0,
            shot: 0,
            font_name: "fntCOLB0808".to_string(),
        };
        app.reload_frames();
        app
    }

    fn reload_frames(&mut self) {
        let (code, _) = CHARACTERS[self.char_idx % CHARACTERS.len()];
        self.frames = self.gd.character_frames(code, self.variant);
        self.frame_idx = 0;
    }

    fn handle_input(&mut self, keys: &[Key]) {
        for &key in keys {
            match key {
                Key::Key0 => {
                    self.mode = Mode::Attract;
                    self.attract_timer = 0;
                }
                Key::Key1 => self.mode = Mode::Screens,
                Key::Key2 => self.mode = Mode::Characters,
                Key::Key3 => self.mode = Mode::Stages,
                Key::Key4 => self.mode = Mode::Fonts,
                Key::Space => self.paused = !self.paused,
                Key::S => self.save_screenshot(),
                Key::Left => self.step(-1),
                Key::Right => self.step(1),
                Key::Up => self.secondary(-1),
                Key::Down => self.secondary(1),
                Key::V => {
                    self.variant = if self.variant == 'B' { 'T' } else { 'B' };
                    self.reload_frames();
                }
                Key::P => {
                    let n = self.current_palettes().len().max(1);
                    self.palette_idx = (self.palette_idx + 1) % n;
                }
                _ => {}
            }
        }
    }

    fn step(&mut self, delta: i32) {
        match self.mode {
            Mode::Attract | Mode::Screens => {
                if delta == 0 {
                    return;
                }
                let n = self.screens.len().max(1) as i32;
                self.screen_idx = ((self.screen_idx as i32 + delta).rem_euclid(n)) as usize;
                self.screen_pan = 0;
                self.attract_timer = 0;
            }
            Mode::Characters => {
                if self.frames.is_empty() {
                    return;
                }
                let n = self.frames.len() as i32;
                self.frame_idx = ((self.frame_idx as i32 + delta).rem_euclid(n)) as usize;
            }
            Mode::Stages => {
                self.stage_x += delta * 8;
            }
            Mode::Fonts => {}
        }
    }

    fn secondary(&mut self, delta: i32) {
        match self.mode {
            Mode::Attract | Mode::Screens => {
                self.screen_pan += delta * 8;
            }
            Mode::Characters => {
                let n = CHARACTERS.len() as i32;
                self.char_idx = ((self.char_idx as i32 + delta).rem_euclid(n)) as usize;
                self.reload_frames();
            }
            Mode::Stages => {
                if delta == 0 {
                    return;
                }
                let n = self.stages.len().max(1) as i32;
                self.stage_idx = ((self.stage_idx as i32 + delta).rem_euclid(n)) as usize;
                self.stage_x = 0;
            }
            Mode::Fonts => {}
        }
    }

    fn current_palettes(&self) -> Vec<String> {
        let (code, _) = CHARACTERS[self.char_idx % CHARACTERS.len()];
        self.gd.character_palettes(code)
    }

    fn tick(&mut self) {
        self.tick += 1;

        if self.mode == Mode::Attract && !self.paused {
            self.attract_timer += 1;
            // Each phase lasts ~20 seconds, then rotate.
            if self.attract_timer % 1200 == 0 {
                self.attract_phase = (self.attract_phase + 1) % 3;
                self.attract_timer = 1;
                if self.attract_phase == 2 {
                    self.stage_idx = (self.stage_idx + 1) % self.stages.len().max(1);
                    self.stage_x = 0;
                }
            }
        }

        if self.paused {
            return;
        }
        if self.tick % 6 == 0 {
            match self.mode {
                Mode::Characters => {
                    if !self.frames.is_empty() {
                        self.frame_idx = (self.frame_idx + 1) % self.frames.len();
                    }
                }
                Mode::Stages => {
                    self.stage_x += 1;
                }
                Mode::Attract => match self.attract_phase {
                    0 => {} // screens: handled below (slower cadence)
                    1 => {
                        if !self.frames.is_empty() {
                            self.frame_idx = (self.frame_idx + 1) % self.frames.len();
                        }
                    }
                    _ => {
                        self.stage_x += 1;
                    }
                },
                _ => {}
            }
        }
        // Screens advance every ~4 seconds in attract/screens mode.
        if self.tick % 240 == 0 {
            let showing_screens = self.mode == Mode::Screens
                || (self.mode == Mode::Attract && self.attract_phase == 0);
            if showing_screens && !self.screens.is_empty() {
                self.screen_idx = (self.screen_idx + 1) % self.screens.len();
                self.screen_pan = 0;
            }
        }
    }

    fn save_screenshot(&mut self) {
        let canvas = self.render();
        let dir = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let path = dir.join(format!("rustfighter-shot-{}.png", self.shot));
        match canvas.save_png(&path) {
            Ok(()) => {
                println!("screenshot: {}", path.display());
                log_line(&format!("screenshot saved: {}", path.display()));
            }
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
        self.shot += 1;
    }

    fn render(&self) -> Canvas {
        match self.mode {
            Mode::Attract => match self.attract_phase {
                0 => self.render_screen(),
                1 => self.render_character(),
                _ => self.render_stage(),
            },
            Mode::Screens => self.render_screen(),
            Mode::Characters => self.render_character(),
            Mode::Stages => self.render_stage(),
            Mode::Fonts => self.render_fonts(),
        }
    }

    fn draw_label(&self, canvas: &mut Canvas, text: &str, line: i32) {
        if let Some(font) = self.gd.font(&self.font_name) {
            canvas.draw_text(&font, text, 2, 2 + line * (font.height as i32 + 1));
        } else {
            binfont::draw_text(canvas, text, 2, 2 + line * 13, 1);
        }
    }

    fn render_screen(&self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        if let Some(info) = self.screens.get(self.screen_idx) {
            if let Ok(screen) = self.gd.load_screen(info) {
                let x = (W as i32 - screen.width as i32) / 2 + self.screen_pan;
                let y = (H as i32 - screen.height as i32) / 2;
                for sy in 0..screen.height {
                    for sx in 0..screen.width {
                        let px = screen.pixels[sy * screen.width + sx];
                        canvas.set(x + sx as i32, y + sy as i32, px);
                    }
                }
                canvas.palette = screen.palette.clone();
                for c in canvas.palette.colors.iter_mut() {
                    if rustfighter::palette::is_sky_key(*c) {
                        *c = [12, 12, 20];
                    }
                }
                self.draw_label(&mut canvas, &info.name, 0);
            } else {
                canvas.palette = Palette::default();
                self.draw_label(&mut canvas, "screen unavailable", 0);
            }
        }
        canvas
    }

    fn render_character(&self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        canvas.clear(0);
        let (code, display) = CHARACTERS[self.char_idx % CHARACTERS.len()];
        if let Some(name) = self.frames.get(self.frame_idx) {
            if let Ok(frame) = self.gd.sprite(name) {
                let mut fpal = Palette::default();
                let palettes = self.current_palettes();
                if let Some(pname) = palettes.get(self.palette_idx % palettes.len().max(1)) {
                    if let Some(p) = self.gd.palette16(pname) {
                        fpal = p;
                    }
                }
                // Index 0 = backdrop, index 1 = label colour; sprite palette
                // entries 1..=15 map to canvas indices 2..=16.
                let mut colors = vec![[0u8, 0, 0]; 17];
                colors[0] = [40, 40, 64];
                colors[1] = [255, 255, 255];
                for i in 1..=15 {
                    if let Some(c) = fpal.colors.get(i) {
                        colors[i + 1] = *c;
                    }
                }
                canvas.palette = Palette { colors };
                let x = (W as i32 - frame.width as i32) / 2;
                let y = (H as i32 - frame.height as i32) / 2;
                for fy in 0..frame.height as i32 {
                    for fx in 0..frame.width as i32 {
                        let px = frame.pixels[fy as usize * frame.width as usize + fx as usize];
                        if px != 0 {
                            canvas.set(x + fx, y + fy, px + 1);
                        }
                    }
                }
                self.draw_label(
                    &mut canvas,
                    &format!("{display} {code} {variant}", variant = self.variant),
                    0,
                );
                self.draw_label(&mut canvas, name, 1);
                let pname = palettes
                    .get(self.palette_idx % palettes.len().max(1))
                    .cloned()
                    .unwrap_or_default();
                self.draw_label(&mut canvas, &pname, 2);
                return canvas;
            }
        }
        self.draw_label(&mut canvas, "no frame", 0);
        canvas
    }

    fn render_stage(&self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        canvas.palette = Palette::default();
        if let Some(key) = self.stages.get(self.stage_idx) {
            if let Ok(stage) = self.gd.load_stage(key) {
                let mut pal = self
                    .gd
                    .palette256(&format!("pal{key}TOP0"))
                    .or_else(|| self.gd.palette256("palGENERAL"))
                    .unwrap_or_default();
                while pal.colors.len() < 2 {
                    pal.colors.push([0, 0, 0]);
                }
                for c in pal.colors.iter_mut() {
                    if rustfighter::palette::is_sky_key(*c) {
                        *c = SKY;
                    }
                }
                canvas.palette = pal;
                let max_pan = (stage.width as i32 - W as i32).max(0);
                let pan = self.stage_x.rem_euclid(max_pan.max(1)).min(max_pan);
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
                self.draw_label(&mut canvas, &format!("stage {key}"), 0);
                return canvas;
            }
        }
        self.draw_label(&mut canvas, "stage unavailable", 0);
        canvas
    }

    fn render_fonts(&self) -> Canvas {
        let mut canvas = Canvas::new(W, H);
        canvas.palette = Palette {
            colors: vec![[16, 16, 32], [255, 255, 255], [255, 200, 0]],
        };
        canvas.clear(0);
        let mut y = 16;
        if let Some(font) = self.gd.font("fntCOLB0808") {
            canvas.draw_text(&font, "STREET FIGHTER II DOS", 8, y);
            y += 14;
            canvas.draw_text(&font, "GAME FONT 8x8  0123456789", 8, y);
            y += 14;
        }
        if let Some(font) = self.gd.font("fntCOLR1016") {
            canvas.draw_text(&font, "GAME FONT 10x16", 8, y);
            y += 20;
        }
        binfont::draw_text(&mut canvas, "EMBEDDED FALLBACK FONT 8x12", 8, y, 2);
        y += 16;
        binfont::draw_text(&mut canvas, "abcdefghijklmnopqrstuvwxyz", 8, y, 1);
        y += 14;
        binfont::draw_text(&mut canvas, "ABCDEFGHIJKLMNOPQRSTUVWXYZ", 8, y, 1);
        y += 14;
        binfont::draw_text(&mut canvas, "0123456789 (.,:;!?%)", 8, y, 1);
        self.draw_label(&mut canvas, "fonts", 0);
        canvas
    }
}

/// Load a game folder, logging the outcome.
fn load_external_dir(dir: &std::path::Path) -> Option<GameData> {
    if !rustfighter::gamedata::is_game_dir(dir) {
        log_line(&format!("{} has no INDEX.SF2", dir.display()));
        return None;
    }
    match GameData::load(dir) {
        Ok(gd) => {
            log_line(&format!(
                "loaded {} records from {}",
                gd.index.records.len(),
                dir.display()
            ));
            Some(gd)
        }
        Err(e) => {
            log_line(&format!("failed to load {}: {e}", dir.display()));
            None
        }
    }
}

/// Ask the desktop for the game folder (zenity or kdialog), with timeouts so a
/// broken dialog tool can never hang the app. Accepts either the folder itself
/// or a folder that contains the extracted game directory.
fn pick_game_folder() -> Option<PathBuf> {
    let attempts: [(&str, [&str; 3]); 2] = [
        (
            "zenity",
            [
                "--file-selection",
                "--directory",
                "--title=Select the Street Fighter II game folder",
            ],
        ),
        (
            "kdialog",
            [
                "--getexistingdirectory",
                ".",
                "Select the Street Fighter II game folder",
            ],
        ),
    ];
    for (bin, args) in attempts {
        for _ in 0..2 {
            let Some((status, stdout)) = run_with_timeout(bin, &args, Duration::from_secs(180))
            else {
                break; // tool unavailable or timed out: try the next tool
            };
            if !status.success() {
                return None; // user cancelled: stop asking
            }
            let path = PathBuf::from(String::from_utf8_lossy(&stdout).trim().to_string());
            if rustfighter::gamedata::is_game_dir(&path) {
                log_line(&format!("picker chose {}", path.display()));
                return Some(path);
            }
            if let Some(found) = rustfighter::gamedata::find_game_in(&path, 3) {
                log_line(&format!("picker chose {} (found {})", path.display(), found.display()));
                return Some(found);
            }
        }
    }
    None
}

/// Run a process with a hard timeout; returns its exit status and stdout.
/// (`wait_with_output` cannot be used after `try_wait` reaped the child.)
fn run_with_timeout(
    bin: &str,
    args: &[&str],
    timeout: Duration,
) -> Option<(std::process::ExitStatus, Vec<u8>)> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take();
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut buffer = Vec::new();
                if let Some(handle) = stdout.as_mut() {
                    let _ = handle.read_to_end(&mut buffer);
                }
                return Some((status, buffer));
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    log_line(&format!("{bin} timed out"));
                    return None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return None,
        }
    }
}

