//! Colour palettes: 6-bit VGA DAC values scaled to 8-bit RGB.
//!
//! Character palettes are 48 bytes = 16 colours x 3 bytes (R, G, B), each value
//! in `0..=63`. Larger palette records are 48-byte banks (e.g. 672 bytes = 14
//! banks) and one full 768-byte 256-colour palette (`palVSLOGO`). Screens such
//! as `linSF2BLOGO` embed their 768-byte palette as the first bytes of the
//! decompressed record.

use std::collections::HashMap;

/// A palette of up to 256 RGB8 colours.
#[derive(Debug, Clone, Default)]
pub struct Palette {
    /// RGB triples, 8-bit per channel.
    pub colors: Vec<[u8; 3]>,
}

impl Palette {
    /// Parse `count * 3` bytes of 6-bit DAC values.
    pub fn from_6bit(data: &[u8], count: usize) -> Self {
        let mut colors = Vec::with_capacity(count);
        for i in 0..count {
            let base = i * 3;
            if base + 2 >= data.len() {
                break;
            }
            colors.push([
                scale6(data[base]),
                scale6(data[base + 1]),
                scale6(data[base + 2]),
            ]);
        }
        Palette { colors }
    }

    /// First 16 colours (character palettes).
    pub fn first16(data: &[u8]) -> Self {
        Self::from_6bit(data, 16)
    }

    /// Extract one 48-byte bank out of a multi-bank palette record.
    pub fn bank16(data: &[u8], bank: usize) -> Self {
        let start = bank * 48;
        let end = (start + 48).min(data.len());
        Self::from_6bit(&data[start..end], 16)
    }

    /// The number of 16-colour banks a palette record contains (at least 1).
    pub fn bank_count(data: &[u8]) -> usize {
        (data.len() / 48).max(1)
    }

    /// Repeat the palette until it has 256 entries (for indexed screens).
    pub fn expand_to_256(&self) -> [[u8; 3]; 256] {
        let mut out = [[0u8; 3]; 256];
        if self.colors.is_empty() {
            return out;
        }
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = self.colors[i % self.colors.len()];
        }
        out
    }

    /// Look up a colour (black when out of range).
    pub fn get(&self, idx: u8) -> [u8; 3] {
        self.colors
            .get(idx as usize)
            .copied()
            .unwrap_or([0, 0, 0])
    }
}

fn scale6(v: u8) -> u8 {
    // 6-bit (0..=63) to 8-bit; matches full-range DAC scaling.
    (((v as u16) * 255) / 63).min(255) as u8
}

/// Detect whether a decompressed record begins with an embedded 768-byte
/// palette: every triple must be in the 6-bit range and the block must be
/// reasonably diverse (guards against dark image data being mistaken for one).
pub fn embedded_palette_len(data: &[u8]) -> usize {
    if data.len() < 768 {
        return 0;
    }
    let head = &data[..768];
    let in_range = head
        .chunks_exact(3)
        .all(|t| t[0] < 0x40 && t[1] < 0x40 && t[2] < 0x40);
    if !in_range {
        return 0;
    }
    let mut distinct: Vec<[u8; 3]> = Vec::new();
    for t in head.chunks_exact(3) {
        let key = [t[0], t[1], t[2]];
        if !distinct.contains(&key) {
            distinct.push(key);
            if distinct.len() >= 16 {
                break;
            }
        }
    }
    if distinct.len() >= 16 {
        768
    } else {
        0
    }
}

/// True when an RGB8 colour is the magenta colour-key used by stage artwork.
pub fn is_sky_key(c: [u8; 3]) -> bool {
    c[0] > 180 && c[1] < 60 && c[2] > 180
}

/// Named-palette helper used by the viewer/dump tools.
#[derive(Debug, Default)]
pub struct PaletteSet {
    by_name: HashMap<String, Vec<u8>>,
}

impl PaletteSet {
    /// Insert a palette record's raw bytes.
    pub fn insert(&mut self, name: &str, raw: Vec<u8>) {
        self.by_name.insert(name.to_string(), raw);
    }

    /// Raw palette bytes by name.
    pub fn raw(&self, name: &str) -> Option<&[u8]> {
        self.by_name.get(name).map(|v| v.as_slice())
    }

    /// A 16-colour palette by name.
    pub fn palette16(&self, name: &str) -> Option<Palette> {
        self.raw(name).map(Palette::first16)
    }

    /// A 256-colour palette by name (expanding/repeating 48-byte banks).
    pub fn palette256(&self, name: &str) -> Option<Palette> {
        self.raw(name).map(|raw| {
            if raw.len() >= 768 {
                Palette::from_6bit(raw, 256)
            } else {
                let base = Palette::from_6bit(raw, raw.len() / 3).colors;
                if base.is_empty() {
                    return Palette::default();
                }
                let mut colors = Vec::with_capacity(256);
                for i in 0..256 {
                    colors.push(base[i % base.len()]);
                }
                Palette { colors }
            }
        })
    }
}
