//! `rfplay` — native windowed player for the original game content (minifb).
//!
//! Controls:
//! * `1` screens, `2` characters, `3` stages, `4` fonts
//! * `Left`/`Right` previous/next item (frame step in character mode)
//! * `Up`/`Down` secondary selector (character, stage, variant, palette)
//! * `Space` pause/advance auto-play
//! * `S` save a PNG screenshot
//! * `Esc` quit

use std::path::PathBuf;
use std::time::Duration;

use minifb::{Key, Window, WindowOptions};

use rustfighter::canvas::Canvas;
use rustfighter::gamedata::{find_game_dir, GameData, ScreenInfo, CHARACTERS};
use rustfighter::palette::Palette;

const W: usize = 320;
const H: usize = 200;
const SKY: [u8; 3] = [24, 30, 56];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Screens,
    Characters,
    Stages,
    Fonts,
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
            "-h" | "--help" => {
                println!("usage: rfplay [--game DIR] [--scale N] [--shot DIR]");
                return;
            }
            "--version" | "-V" => {
                println!("rustfighter {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                return;
            }
        }
    }
    let game = match game {
        Some(dir) => dir,
        None => find_game_dir()
            .or_else(pick_game_folder)
            .unwrap_or_else(|| {
                eprintln!("Street Fighter II game files not found.");
                eprintln!("Put the original game files in a folder, pass --game <DIR>,");
                eprintln!("set SF2_DIR, or choose the folder in the dialog when it appears.");
                std::process::exit(1);
            }),
    };
    if !rustfighter::gamedata::is_game_dir(&game) {
        eprintln!(
            "{} does not look like the game folder (INDEX.SF2 not found).",
            game.display()
        );
        std::process::exit(1);
    }
    // Remember this folder for next time.
    rustfighter::gamedata::save_game_dir(&game);
    let gd = GameData::load(&game).unwrap_or_else(|e| {
        eprintln!("failed to load game data: {e}");
        std::process::exit(1);
    });

    let mut app = App::new(gd);
    if let Some(dir) = shot {
        // Headless verification: render each mode once and save PNGs.
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
    let mut window = Window::new(
        "rustfighter — Street Fighter II (DOS) native player",
        W,
        H,
        WindowOptions {
            resize: true,
            scale: match scale {
                1 => minifb::Scale::X1,
                2 => minifb::Scale::X2,
                4 => minifb::Scale::X4,
                8 => minifb::Scale::X8,
                _ => minifb::Scale::X2,
            },
            ..WindowOptions::default()
        },
    )
    .expect("cannot create window");
    window.set_target_fps(60);

    let mut buffer = vec![0u32; W * H];
    let mut last_keys = Vec::new();
    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut pressed = Vec::new();
        for key in [
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
        let rgb = app.canvas_to_rgb(&canvas);
        for (i, px) in rgb.chunks_exact(3).enumerate() {
            buffer[i] = ((px[0] as u32) << 16) | ((px[1] as u32) << 8) | px[2] as u32;
        }
        window
            .update_with_buffer(&buffer, W, H)
            .expect("window update");
        last_keys.clear();
        last_keys.extend(pressed);
        std::thread::sleep(Duration::from_millis(1));
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
    shot: u32,
    font_name: String,
}

impl App {
    fn new(gd: GameData) -> Self {
        let mut screens = gd.screens();
        // Present the most recognisable screens first.
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
            mode: Mode::Screens,
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
                    let n = self
                        .current_palettes()
                        .len()
                        .max(1);
                    self.palette_idx = (self.palette_idx + 1) % n;
                }
                _ => {}
            }
        }
    }

    fn step(&mut self, delta: i32) {
        match self.mode {
            Mode::Screens => {
                if delta == 0 {
                    return;
                }
                let n = self.screens.len().max(1) as i32;
                self.screen_idx = ((self.screen_idx as i32 + delta).rem_euclid(n)) as usize;
                self.screen_pan = 0;
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
            Mode::Screens => {
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
                _ => {}
            }
        }
    }

    fn save_screenshot(&mut self) {
        let canvas = self.render();
        let path = format!("/tmp/rfplay_shot_{}.png", self.shot);
        match canvas.save_png(std::path::Path::new(&path)) {
            Ok(()) => println!("screenshot: {path}"),
            Err(e) => eprintln!("screenshot failed: {e}"),
        }
        self.shot += 1;
    }

    fn render(&self) -> Canvas {
        match self.mode {
            Mode::Screens => self.render_screen(),
            Mode::Characters => self.render_character(),
            Mode::Stages => self.render_stage(),
            Mode::Fonts => self.render_fonts(),
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
                // Screens use magenta palette slots as "runtime background" keys;
                // present them as a dark backdrop.
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
                // Every magenta palette slot is a sky hole; recolour them all.
                for c in pal.colors.iter_mut() {
                    if rustfighter::palette::is_sky_key(*c) {
                        *c = SKY;
                    }
                }
                canvas.palette = pal.clone();
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
        let _ = &self.font_name;
        if let Some(font) = self.gd.font("fntCOLB0808") {
            canvas.draw_text(&font, "RUSTFIGHTER", 8, 16);
            canvas.draw_text(&font, "STREET FIGHTER II DOS", 8, 28);
            canvas.draw_text(&font, "0123456789", 8, 48);
            canvas.draw_text(&font, "The quick brown fox", 8, 68);
            // glyph grid of the color font at 1:1
            if let Some(big) = self.gd.font("fntCOLR1016") {
                big_glyph_grid(&mut canvas, &big, 8, 88);
            }
        } else {
            canvas.draw_text(
                &rustfighter::font::GlyphFont::from_name("fntA0808", &[1u8; 8 * 8 * 96]).unwrap(),
                "FONT UNAVAILABLE",
                8,
                8,
            );
        }
        self.draw_label(&mut canvas, "fonts", 0);
        canvas
    }

    fn draw_label(&self, canvas: &mut Canvas, text: &str, line: i32) {
        if let Some(font) = self.gd.font(&self.font_name) {
            canvas.draw_text(&font, text, 2, 2 + line * (font.height as i32 + 1));
        }
    }

    fn canvas_to_rgb(&self, canvas: &Canvas) -> Vec<u8> {
        canvas.to_rgb()
    }
}

fn big_glyph_grid(canvas: &mut Canvas, font: &rustfighter::font::GlyphFont, x0: i32, y0: i32) {
    let mut cx = x0;
    let mut cy = y0;
    for (i, glyph) in font.glyphs.iter().enumerate() {
        for y in 0..font.height {
            for x in 0..font.width {
                if font.pixel(glyph, x, y) != 0 {
                    canvas.rect(cx + x as i32, cy + y as i32, 1, 1, 2);
                }
            }
        }
        cx += font.width as i32 + 1;
        if (i + 1) % 16 == 0 {
            cx = x0;
            cy += font.height as i32 + 2;
        }
    }
}

/// Ask the desktop for the game folder (zenity on GNOME, kdialog on KDE).
/// Returns `None` when no dialog tool is available or the choice is invalid.
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
        let mut chosen: Option<PathBuf> = None;
        for _ in 0..2 {
            let Ok(output) = std::process::Command::new(bin).args(args).output() else {
                break;
            };
            if !output.status.success() {
                break; // cancelled or unavailable
            }
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
            if rustfighter::gamedata::is_game_dir(&path) {
                chosen = Some(path);
                break;
            }
        }
        if let Some(path) = chosen {
            return Some(path);
        }
        // kdialog needs a display too; fall through to it only if zenity ran.
        if std::process::Command::new(bin)
            .arg("--version")
            .output()
            .is_err()
        {
            continue;
        }
    }
    None
}
