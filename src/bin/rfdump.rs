//! `rfdump` — headless asset dumper: decodes the original data files and writes
//! PNG evidence to a directory. Doubles as an end-to-end verification tool.
//!
//! Usage: `rfdump [--game DIR] --out DIR [--what all|screens|stages|chars|fonts|index]`

use std::path::{Path, PathBuf};

use rustfighter::gamedata::{find_game_dir, GameData};

fn usage() -> ! {
    eprintln!(
        "usage: rfdump [--game DIR] --out DIR [--what all|screens|stages|chars|fonts|index]"
    );
    std::process::exit(2);
}

fn main() {
    let mut game: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut what = "all".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--game" => game = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from),
            "--what" => what = args.next().unwrap_or_else(|| usage()),
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown argument: {other}");
                usage();
            }
        }
    }
    let game = game
        .or_else(find_game_dir)
        .unwrap_or_else(|| {
            eprintln!("game directory not found; pass --game or set SF2_DIR");
            std::process::exit(1)
        });
    let out = out.unwrap_or_else(|| PathBuf::from("rfdump-out"));
    let gd = GameData::load(&game).unwrap_or_else(|e| {
        eprintln!("failed to load game data: {e}");
        std::process::exit(1);
    });
    println!(
        "loaded {} records from {} (files: {})",
        gd.index.records.len(),
        game.display(),
        gd.index
            .files
            .iter()
            .filter(|f| !f.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );

    let do_all = what == "all";
    let mut summary = Summary::default();

    if do_all || what == "index" {
        dump_index(&gd, &out, &mut summary);
    }
    if do_all || what == "screens" {
        dump_screens(&gd, &out, &mut summary);
    }
    if do_all || what == "stages" {
        dump_stages(&gd, &out, &mut summary);
    }
    if do_all || what == "chars" {
        dump_characters(&gd, &out, &mut summary);
    }
    if do_all || what == "fonts" {
        dump_fonts(&gd, &out, &mut summary);
    }

    println!(
        "done: {} records decoded, {} sprites ok, {} screens, {} stages, {} character sheets, {} fonts, {} failures",
        summary.records_decoded,
        summary.sprites_ok,
        summary.screens,
        summary.stages,
        summary.char_sheets,
        summary.fonts,
        summary.failures.len()
    );
    for f in summary.failures.iter().take(20) {
        println!("  FAIL: {f}");
    }
}

#[derive(Default)]
struct Summary {
    records_decoded: usize,
    sprites_ok: usize,
    screens: usize,
    stages: usize,
    char_sheets: usize,
    fonts: usize,
    failures: Vec<String>,
}

fn ensure_dir(path: &Path) {
    std::fs::create_dir_all(path).unwrap_or_else(|e| panic!("mkdir {}: {e}", path.display()));
}

fn dump_index(gd: &GameData, out: &Path, s: &mut Summary) {
    ensure_dir(out);
    let mut lines = String::from("file\toffset\tsize\tmethod\tname\n");
    for rec in &gd.index.records {
        match gd.decoded_record(rec) {
            Ok(_) => {
                s.records_decoded += 1;
                if rec.trimmed_name().starts_with("spr") {
                    s.sprites_ok += 1;
                }
            }
            Err(e) => s.failures.push(format!("{}: {e}", rec.trimmed_name())),
        }
        lines.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            rec.file_index(),
            rec.offset,
            rec.size,
            rec.method(),
            rec.trimmed_name()
        ));
    }
    std::fs::write(out.join("index.tsv"), lines).unwrap();
    println!("index: {} records -> {}", gd.index.records.len(), out.join("index.tsv").display());
}

fn dump_screens(gd: &GameData, out: &Path, s: &mut Summary) {
    let dir = out.join("screens");
    ensure_dir(&dir);
    for info in gd.screens() {
        match gd.load_screen(&info) {
            Ok(canvas) => {
                let path = dir.join(format!("{}.png", info.name));
                if canvas.save_png(&path).is_ok() {
                    s.screens += 1;
                } else {
                    s.failures.push(format!("save {}", info.name));
                }
            }
            Err(e) => s.failures.push(format!("screen {}: {e}", info.name)),
        }
    }
    println!("screens: {} -> {}", s.screens, dir.display());
}

fn dump_stages(gd: &GameData, out: &Path, s: &mut Summary) {
    let dir = out.join("stages");
    ensure_dir(&dir);
    for key in gd.stage_keys() {
        match gd.load_stage(&key) {
            Ok(stage) => {
                let palette = gd
                    .palette256(&format!("pal{key}TOP0"))
                    .or_else(|| gd.palette256("palGENERAL"));
                match stage_to_png(&stage, palette.as_ref(), &dir.join(format!("{key}.png"))) {
                    Ok(()) => s.stages += 1,
                    Err(e) => s.failures.push(format!("stage {key}: {e}")),
                }
            }
            Err(e) => s.failures.push(format!("stage {key}: {e}")),
        }
    }
    println!("stages: {} -> {}", s.stages, dir.display());
}

/// Convert a stage to PNG, replacing every magenta colour-key pixel with a sky
/// colour (stage palettes use one magenta slot per sky index).
fn stage_to_png(
    stage: &rustfighter::gamedata::Stage,
    palette: Option<&rustfighter::palette::Palette>,
    path: &Path,
) -> Result<(), String> {
    let sky = [24u8, 30, 56];
    let pal = palette.cloned().unwrap_or_default();
    let mut rgb = Vec::with_capacity(stage.width * stage.height * 3);
    for &px in &stage.pixels {
        let c = pal.get(px);
        let c = if rustfighter::palette::is_sky_key(c) { sky } else { c };
        rgb.extend_from_slice(&c);
    }
    let img = image::RgbImage::from_raw(stage.width as u32, stage.height as u32, rgb)
        .ok_or("stage buffer size mismatch")?;
    img.save(path).map_err(|e| e.to_string())
}

fn dump_characters(gd: &GameData, out: &Path, s: &mut Summary) {
    let dir = out.join("chars");
    ensure_dir(&dir);
    for (code, display) in rustfighter::gamedata::CHARACTERS {
        let frames = gd.character_frames(code, 'B');
        if frames.is_empty() {
            s.failures.push(format!("no frames for {code}"));
            continue;
        }
        let palettes = gd.character_palettes(code);
        let palette = palettes.first().and_then(|p| gd.palette16(p));
        let Some(palette) = palette else {
            s.failures.push(format!("no palette for {code}"));
            continue;
        };
        // Contact sheet of up to 48 frames, 8 columns.
        let limit = frames.len().min(48);
        let cell_w = 224usize;
        let cell_h = 160usize;
        let cols = 8usize;
        let rows = limit.div_ceil(cols);
        let sheet_w = cols * cell_w;
        let sheet_h = rows * cell_h;
        let mut sheet = rustfighter::canvas::Canvas::new(sheet_w.min(4096), sheet_h.min(4096));
        sheet.palette = palette.clone();
        sheet.clear(0);
        for (i, name) in frames.iter().take(limit).enumerate() {
            if let Ok(frame) = gd.sprite(name) {
                let cx = (i % cols) * cell_w + cell_w / 2 - frame.width as usize / 2;
                let cy = (i / cols) * cell_h + cell_h / 2 - frame.height as usize / 2;
                sheet.blit(&frame, cx as i32, cy as i32, false);
            } else {
                s.failures.push(format!("sprite {name}"));
            }
        }
        let path = dir.join(format!("{code}_B_pal001.png"));
        if sheet.save_png(&path).is_ok() {
            s.char_sheets += 1;
            println!("  {display} ({code}): {} frames", frames.len());
        } else {
            s.failures.push(format!("save {code} sheet"));
        }
    }
    println!("characters: {} -> {}", s.char_sheets, dir.display());
}

fn dump_fonts(gd: &GameData, out: &Path, s: &mut Summary) {
    let dir = out.join("fonts");
    ensure_dir(&dir);
    let names: Vec<String> = gd
        .index
        .find_prefix("fnt")
        .into_iter()
        .map(|r| r.trimmed_name().to_string())
        .collect();
    for name in names {
        let Some(font) = gd.font(&name) else {
            s.failures.push(format!("font {name}"));
            continue;
        };
        // Glyph grid: 16 glyphs per row, scale 2.
        let scale = 2usize;
        let cols = 16usize;
        let rows = font.glyphs.len().div_ceil(cols).max(1);
        let gw = font.width * scale;
        let gh = font.height * scale;
        let mut canvas = rustfighter::canvas::Canvas::new(cols * (gw + 1) + 1, rows * (gh + 1) + 1);
        canvas.clear(0);
        for i in 0..canvas.pixels.len() {
            // dark blue background, leave glyph pixels as-is
            canvas.pixels[i] = 1;
        }
        canvas.palette = rustfighter::palette::Palette {
            colors: vec![[16, 16, 32], [255, 255, 255], [255, 0, 0]],
        };
        for (i, glyph) in font.glyphs.iter().enumerate() {
            let gx = (i % cols) * (gw + 1) + 1;
            let gy = (i / cols) * (gh + 1) + 1;
            for y in 0..font.height {
                for x in 0..font.width {
                    let px = font.pixel(glyph, x, y);
                    if px != 0 {
                        canvas.rect(
                            (gx + x * scale) as i32,
                            (gy + y * scale) as i32,
                            scale as i32,
                            scale as i32,
                            2,
                        );
                    }
                }
            }
        }
        let path = dir.join(format!("{name}.png"));
        if canvas.save_png(&path).is_ok() {
            s.fonts += 1;
        } else {
            s.failures.push(format!("save {name}"));
        }
    }
    println!("fonts: {} -> {}", s.fonts, dir.display());
}
