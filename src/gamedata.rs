//! High-level access to the original game data files.
//!
//! [`GameData`] loads `INDEX.SF2` plus the four data archives it names, decodes
//! records on demand (raw or LZW) with a cache, and provides typed helpers:
//! sprites, palettes, fonts, screens and assembled stages.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::canvas::Canvas;
use crate::font::GlyphFont;
use crate::index::{AssetIndex, Record};
use crate::lzw;
use crate::palette::{self, Palette};
use crate::sprite::{self, Frame};

/// Character codes and display names (12 selectable fighters).
pub const CHARACTERS: &[(&str, &str)] = &[
    ("RYUT", "RYU"),
    ("KENN", "KEN"),
    ("CHLI", "CHUN LI"),
    ("GUIL", "GUILE"),
    ("HOND", "E. HONDA"),
    ("DHAL", "DHALSIM"),
    ("BLAN", "BLANKA"),
    ("ZANG", "ZANGIEF"),
    ("BALR", "BALROG"),
    ("BISO", "M. BISON"),
    ("SAGE", "SAGAT"),
    ("VEGA", "VEGA"),
];

/// A screen-sized image record with an optional companion palette name.
#[derive(Debug, Clone)]
pub struct ScreenInfo {
    /// Record name (e.g. `linINTROBOT`).
    pub name: String,
    /// Image width in pixels.
    pub width: usize,
    /// Image height in pixels.
    pub height: usize,
    /// Bytes of embedded palette header (0 if none).
    pub embedded_palette: usize,
    /// Companion palette record name, if one was found.
    pub palette: Option<String>,
}

/// An assembled stage: vertical 64-pixel slices laid out horizontally.
#[derive(Debug, Clone)]
pub struct Stage {
    /// Stage key (e.g. `BARL`, `JAP1`).
    pub key: String,
    /// Total width in pixels (`64 * slice_count`).
    pub width: usize,
    /// Total height (176 top + 56 bottom = 232).
    pub height: usize,
    /// Palette indices, row-major.
    pub pixels: Vec<u8>,
    /// Most common index in the artwork (the magenta colour key = sky hole).
    pub key_color: u8,
}

/// Loaded game data.
pub struct GameData {
    /// Directory the data was loaded from.
    pub dir: PathBuf,
    /// Parsed `INDEX.SF2`.
    pub index: AssetIndex,
    files: Vec<Vec<u8>>,
    decoded: RefCell<HashMap<usize, Rc<Vec<u8>>>>,
}

/// Locate the game directory: `SF2_DIR` env var, then `./street-fighter-ii`,
/// then the analysis workspace path.
pub fn find_game_dir() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = std::env::var("SF2_DIR")
        .ok()
        .into_iter()
        .chain([
            "street-fighter-ii".to_string(),
            "/tmp/opencode/rea/game/street-fighter-ii".to_string(),
        ])
        .map(PathBuf::from)
        .collect();
    candidates
        .into_iter()
        .find(|p| p.join("INDEX.SF2").is_file())
}

/// Read a file by name, matching case-insensitively (the DOS original used a
/// case-insensitive filesystem; the shipped filenames are uppercase).
fn read_case_insensitive(dir: &Path, name: &str) -> Option<Vec<u8>> {
    let direct = dir.join(name);
    if direct.is_file() {
        return std::fs::read(direct).ok();
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
        {
            return std::fs::read(entry.path()).ok();
        }
    }
    None
}

impl GameData {
    /// Load and cache all data files named by `INDEX.SF2`.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let index_bytes = std::fs::read(dir.join("INDEX.SF2"))
            .map_err(|e| format!("cannot read {}/INDEX.SF2: {e}", dir.display()))?;
        let index = AssetIndex::parse(&index_bytes).map_err(|e| e.to_string())?;
        let mut files = Vec::with_capacity(index.files.len());
        for name in &index.files {
            if name.is_empty() {
                files.push(Vec::new());
                continue;
            }
            files.push(read_case_insensitive(dir, name).ok_or_else(|| {
                format!("cannot read {} or any case variant of it", dir.join(name).display())
            })?);
        }
        Ok(GameData {
            dir: dir.to_path_buf(),
            index,
            files,
            decoded: RefCell::new(HashMap::new()),
        })
    }

    /// Find a record by trimmed name.
    pub fn record(&self, name: &str) -> Option<&Record> {
        self.index.find(name)
    }

    /// Decode (raw or LZW) a record's payload, with caching.
    pub fn decoded_record(&self, rec: &Record) -> Result<Rc<Vec<u8>>, String> {
        if let Some(hit) = self.decoded.borrow().get(&rec.index) {
            return Ok(hit.clone());
        }
        let file = self
            .files
            .get(rec.file_index())
            .ok_or_else(|| format!("record {} has no data file", rec.name))?;
        let payload = rec.payload(file);
        let out = match rec.method() {
            0 => payload.to_vec(),
            3 => lzw::decode_payload_checked(payload)?,
            m => return Err(format!("unsupported method {m} for {}", rec.trimmed_name())),
        };
        let rc = Rc::new(out);
        self.decoded.borrow_mut().insert(rec.index, rc.clone());
        Ok(rc)
    }

    /// Decode a record by name.
    pub fn decoded(&self, name: &str) -> Result<Rc<Vec<u8>>, String> {
        let rec = self
            .record(name)
            .ok_or_else(|| format!("no such asset: {name}"))?
            .clone();
        self.decoded_record(&rec)
    }

    /// Decode a sprite frame by record name.
    pub fn sprite(&self, name: &str) -> Result<Frame, String> {
        let data = self.decoded(name)?;
        sprite::decode(&data).map_err(|e| format!("{name}: {e}"))
    }

    /// A 16-colour palette record.
    pub fn palette16(&self, name: &str) -> Option<Palette> {
        let data = self.decoded(name).ok()?;
        Some(Palette::first16(&data))
    }

    /// A 256-colour palette record.
    pub fn palette256(&self, name: &str) -> Option<Palette> {
        let data = self.decoded(name).ok()?;
        if data.len() >= 768 {
            Some(Palette::from_6bit(&data, 256))
        } else {
            let bank = Palette::from_6bit(&data, data.len() / 3);
            if bank.colors.is_empty() {
                return None;
            }
            let mut colors = Vec::with_capacity(256);
            for i in 0..256 {
                colors.push(bank.colors[i % bank.colors.len()]);
            }
            Some(Palette { colors })
        }
    }

    /// Load a font record (`fnt*`).
    pub fn font(&self, name: &str) -> Option<GlyphFont> {
        let data = self.decoded(name).ok()?;
        GlyphFont::from_name(name, &data)
    }

    /// Screen-sized `lin*` records from `GENERAL.SF2`, with palette pairing.
    pub fn screens(&self) -> Vec<ScreenInfo> {
        let mut out = Vec::new();
        for rec in self.index.find_prefix("lin") {
            if rec.file_index() != 2 {
                continue;
            }
            let Ok(data) = self.decoded_record(rec) else {
                continue;
            };
            let name = rec.trimmed_name();
            let Some((width, height, embedded)) = screen_dims(name, &data) else {
                continue;
            };
            if height < 40 {
                continue;
            }
            let stem = &name[3..];
            let palette = if embedded > 0 {
                None
            } else {
                let direct = format!("pal{stem}");
                if self.record(&direct).is_some() {
                    Some(direct)
                } else if self.record("palGENERAL").is_some() {
                    Some("palGENERAL".to_string())
                } else {
                    None
                }
            };
            out.push(ScreenInfo {
                name: name.to_string(),
                width,
                height,
                embedded_palette: embedded,
                palette,
            });
        }
        out
    }

    /// Load a screen into a canvas using its own or companion palette.
    pub fn load_screen(&self, info: &ScreenInfo) -> Result<Canvas, String> {
        let data = self.decoded(&info.name)?;
        let palette = if info.embedded_palette > 0 {
            Palette::from_6bit(&data[..768], 256)
        } else if let Some(p) = &info.palette {
            self.palette256(p)
                .ok_or_else(|| format!("palette {p} unavailable"))?
        } else {
            Palette::default()
        };
        let body = &data[info.embedded_palette..];
        let mut canvas = Canvas::new(info.width, info.height);
        let n = (info.width * info.height).min(body.len());
        canvas.pixels[..n].copy_from_slice(&body[..n]);
        canvas.palette = palette;
        Ok(canvas)
    }

    /// Stage keys found in the data (families with `lin<KEY>TOP0` records).
    pub fn stage_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self
            .index
            .records
            .iter()
            .filter_map(|r| {
                let n = r.trimmed_name();
                let rest = n.strip_prefix("lin")?;
                let key = rest.strip_suffix("TOP0")?;
                if key.is_empty() {
                    None
                } else {
                    Some(key.to_string())
                }
            })
            .collect();
        keys.sort();
        keys.dedup();
        keys
    }

    /// Assemble a stage from its 64-wide vertical slices (TOP 176 + BOT 56).
    pub fn load_stage(&self, key: &str) -> Result<Stage, String> {
        let mut tops: Vec<Rc<Vec<u8>>> = Vec::new();
        let mut bots: Vec<Rc<Vec<u8>>> = Vec::new();
        for i in 0..64 {
            match self.decoded(&format!("lin{key}TOP{i}")) {
                Ok(d) => tops.push(d),
                Err(_) => break,
            }
        }
        for i in 0..64 {
            match self.decoded(&format!("lin{key}BOT{i}")) {
                Ok(d) => bots.push(d),
                Err(_) => break,
            }
        }
        if tops.is_empty() {
            return Err(format!("stage {key} has no TOP slices"));
        }
        let slice_w = 64;
        let top_h = 176;
        let bot_h = 56;
        let bot_slices = if bots.len() == tops.len() { bots.len() } else { 0 };
        let width = slice_w * tops.len();
        let height = top_h + if bot_slices > 0 { bot_h } else { 0 };
        let mut pixels = vec![0u8; width * height];
        let mut hist = [0usize; 256];
        for (i, data) in tops.iter().enumerate() {
            if data.len() != slice_w * top_h {
                return Err(format!("lin{key}TOP{i} has unexpected slice size {}", data.len()));
            }
            for y in 0..top_h {
                let src = &data[y * slice_w..(y + 1) * slice_w];
                let dst = (y * width + i * slice_w) as usize;
                pixels[dst..dst + slice_w].copy_from_slice(src);
                for &b in src {
                    hist[b as usize] += 1;
                }
            }
        }
        for (i, data) in bots.iter().take(bot_slices).enumerate() {
            if data.len() != slice_w * bot_h {
                return Err(format!("lin{key}BOT{i} has unexpected slice size {}", data.len()));
            }
            for y in 0..bot_h {
                let src = &data[y * slice_w..(y + 1) * slice_w];
                let dst = ((top_h + y) * width + i * slice_w) as usize;
                pixels[dst..dst + slice_w].copy_from_slice(src);
                for &b in src {
                    hist[b as usize] += 1;
                }
            }
        }
        let key_color = hist.iter().enumerate().max_by_key(|(_, &c)| c).map(|(i, _)| i as u8).unwrap_or(0);
        Ok(Stage {
            key: key.to_string(),
            width,
            height,
            pixels,
            key_color,
        })
    }

    /// Character sprite frame names for one variant (`'B'` or `'T'`).
    pub fn character_frames(&self, code: &str, variant: char) -> Vec<String> {
        let prefix = format!("spr{code}{variant}");
        self.index
            .find_prefix(&prefix)
            .into_iter()
            .map(|r| r.trimmed_name().to_string())
            .collect()
    }

    /// Companion character palette names (`pal<CODE>T001/T002`).
    pub fn character_palettes(&self, code: &str) -> Vec<String> {
        self.index
            .find_prefix(&format!("pal{code}T"))
            .into_iter()
            .map(|r| r.trimmed_name().to_string())
            .collect()
    }
}

/// Determine screen geometry from a decompressed `lin*` record.
///
/// Returns `(width, height, embedded_palette_len)` when the record looks like a
/// full screen or banner.
pub fn screen_dims(name: &str, data: &[u8]) -> Option<(usize, usize, usize)> {
    // Exact full-record sizes first: these screens carry no embedded palette and
    // a dark leading image region must not be mistaken for one.
    match data.len() {
        64000 => return Some((320, 200, 0)),
        32768 => return Some((256, 128, 0)),
        _ => {}
    }
    let embedded = palette::embedded_palette_len(data);
    let body = data.len().checked_sub(embedded)?;
    // The logo band is 384x94 under its embedded 768-byte palette.
    if name.ends_with("SF2BLOGO") && body == 384 * 94 {
        return Some((384, 94, embedded));
    }
    if embedded > 0 && body % 320 == 0 && (8..=250).contains(&(body / 320)) {
        return Some((320, body / 320, embedded));
    }
    if body % 320 == 0 && (40..=250).contains(&(body / 320)) {
        return Some((320, body / 320, embedded));
    }
    if body % 256 == 0 && (40..=300).contains(&(body / 256)) {
        return Some((256, body / 256, embedded));
    }
    None
}
