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

/// Character codes and display names (12 selectable fighters), in *display*
/// order for the viewer tools.
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

/// **Engine character order** (`DS:0x60` handle table, index = `char`):
/// `0 KEN, 1 GUILE, 2 E-HONDA, 3 RYU, 4 ZANGIEF, 5 CHUN LI, 6 DHALSIM,
/// 7 BLANKA, 8 BALROG, 9 VEGA, 10 SAGAT, 11 M.BISON`.
///
/// All engine tables (`ait`/`ani`/`frm` handles, block rules `CS:0x22B3`,
/// damage special cases, CPU probability `DS:0x3432`) index by this order.
pub const ENGINE_CHARACTERS: [&str; 12] = [
    "KENN", "GUIL", "HOND", "RYUT", "ZANG", "CHLI", "DHAL", "BLAN", "BALR", "VEGA", "SAGE", "BISO",
];

/// Engine index for a record-name character code.
pub fn engine_char_index(code: &str) -> Option<usize> {
    ENGINE_CHARACTERS.iter().position(|&c| c == code)
}

/// Asset-name stems of the per-character gameplay tables in engine order:
/// `frm<STEM>`, `ani<STEM>`, `ait<STEM>`.
pub const ENGINE_TABLE_STEMS: [&str; 12] = [
    "KEN", "GUILE", "EHONDO", "RYU", "ZANGEIF", "CHUNLI", "DHALISM", "BLANKA", "BALROG", "VEGA",
    "SAGAT", "MBISON",
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

/// Locate the game directory.
///
/// Order: `SF2_DIR` environment variable, the directory remembered in
/// `~/.config/rustfighter/config`, the working directory, the executable's
/// directory (and a `street-fighter-ii`/`game` subdirectory), then shallow
/// scans of `~/Downloads`, `~/Desktop` and `~/Games`.
pub fn find_game_dir() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(dir) = std::env::var("SF2_DIR") {
        candidates.push(PathBuf::from(dir));
    }
    if let Some(dir) = saved_game_dir() {
        candidates.push(dir);
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.clone());
        candidates.push(cwd.join("street-fighter-ii"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
            candidates.push(dir.join("street-fighter-ii"));
            candidates.push(dir.join("game"));
        }
    }
    for c in &candidates {
        if is_game_dir(c) {
            return Some(c.clone());
        }
    }
    // Shallow scan of common download locations.
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for root in ["Downloads", "Desktop", "Games"] {
            let root = home.join(root);
            if root.is_dir() {
                if let Some(found) = scan_for_game(&root, 3) {
                    return Some(found);
                }
            }
        }
    }
    None
}

/// True when `dir` contains the game's asset directory file.
pub fn is_game_dir(dir: &Path) -> bool {
    dir.join("INDEX.SF2").is_file()
        || std::fs::read_dir(dir)
            .map(|mut entries| {
                entries.any(|e| {
                    e.map(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("INDEX.SF2"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
}

fn config_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/rustfighter/config"))
}

/// The remembered game directory, if it still exists.
pub fn saved_game_dir() -> Option<PathBuf> {
    let path = config_path()?;
    let text = std::fs::read_to_string(path).ok()?;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("game_dir=") {
            let dir = PathBuf::from(value.trim());
            if is_game_dir(&dir) {
                return Some(dir);
            }
        }
    }
    None
}

/// Remember a usable game directory for future launches.
pub fn save_game_dir(dir: &Path) {
    let Some(path) = config_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format!("game_dir={}\n", dir.display()));
}

/// Breadth-first search for a directory containing `INDEX.SF2` (public wrapper).
pub fn find_game_in(root: &Path, max_depth: usize) -> Option<PathBuf> {
    scan_for_game(root, max_depth)
}

/// Breadth-first search for a directory containing `INDEX.SF2`.
fn scan_for_game(root: &Path, max_depth: usize) -> Option<PathBuf> {
    let mut queue = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = queue.pop() {
        if is_game_dir(&dir) {
            return Some(dir);
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                queue.push((entry.path(), depth + 1));
            }
        }
    }
    None
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

/// Directory marker used for compiled-in data.
pub const EMBEDDED_DIR: &str = "<embedded>";

#[cfg(feature = "embedded")]
fn imp_embedded() -> Result<GameData, String> {
    let map = crate::embedded::map();
    let index_bytes = map
        .get("INDEX.SF2")
        .ok_or_else(|| "embedded INDEX.SF2 missing".to_string())?;
    let index = AssetIndex::parse(index_bytes).map_err(|e| e.to_string())?;
    let mut files = Vec::with_capacity(index.files.len());
    for name in &index.files {
        if name.is_empty() {
            files.push(Vec::new());
            continue;
        }
        let data = map
            .get(&name.to_uppercase())
            .ok_or_else(|| format!("embedded data file {name} missing"))?;
        files.push(data.to_vec());
    }
    Ok(GameData {
        dir: PathBuf::from(EMBEDDED_DIR),
        index,
        files,
        decoded: RefCell::new(HashMap::new()),
    })
}

#[cfg(not(feature = "embedded"))]
fn imp_embedded() -> Result<GameData, String> {
    Err("this build has no embedded game data (build with --features embedded)".to_string())
}

impl GameData {
    /// Load the compiled-in asset bundle (build with `--features embedded`).
    ///
    /// The bundle contains the original game data, so such builds are personal
    /// use only and must not be redistributed.
    pub fn load_embedded() -> Result<Self, String> {
        imp_embedded()
    }

    /// True when this instance runs from compiled-in data.
    pub fn is_embedded(&self) -> bool {
        self.dir.as_os_str() == EMBEDDED_DIR
    }

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

    /// Resolve an engine asset handle (`SI = -(index+1) mod 0x10000`).
    pub fn record_for_handle(&self, handle: u16) -> Option<&Record> {
        let index = (0x10000u32 - handle as u32 - 1) & 0xFFFF;
        self.index.records.get(index as usize)
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
    // The title/credits/option logo is 288x128 (36,864 bytes - the same byte
    // count as a 256x144 guess, which shreds it into horizontal streaks). The
    // observed draw call places it at (0x10,0) on the 320-wide screen.
    if name.ends_with("SF2BLOGO") && data.len() == 288 * 128 {
        return Some((288, 128, 0));
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
