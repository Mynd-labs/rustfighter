//! 8-bpp glyph fonts stored in `GENERAL.SF2`.
//!
//! `fnt*` records decompress to a flat array of glyph bitmaps, one byte per
//! pixel; the glyph cell size is encoded in the record name (`fntCOLB0808` =
//! 8x8, `fntCOLR1016` = 10x16, `fntNUMB1419` = 14x19 digits, ...). Text fonts
//! cover ASCII 32..127 (96 glyphs); `fntNUMB*` holds the ten digits only.

/// A loaded bitmap font.
#[derive(Debug, Clone)]
pub struct GlyphFont {
    /// Glyph cell width in pixels.
    pub width: usize,
    /// Glyph cell height in pixels.
    pub height: usize,
    /// ASCII code of the first glyph.
    pub first_char: u8,
    /// Glyph bitmaps, `width * height` bytes each.
    pub glyphs: Vec<Vec<u8>>,
}

impl GlyphFont {
    /// Parse a font record by name and its decompressed bytes.
    ///
    /// Glyph dimensions come from the trailing four digits of the name (e.g.
    /// `0808`) or the literal `4X4` marker in `fntDEBUG4X4`.
    pub fn from_name(name: &str, data: &[u8]) -> Option<Self> {
        let dims = if let Some(pos) = name.find("4X4") {
            let _ = pos;
            (4, 4)
        } else {
            let tail: String = name.chars().rev().take(4).collect::<String>().chars().rev().collect();
            if tail.len() == 4 && tail.chars().all(|c| c.is_ascii_digit()) {
                let w = tail[0..2].parse::<usize>().ok()?;
                let h = tail[2..4].parse::<usize>().ok()?;
                (w, h)
            } else {
                return None;
            }
        };
        let (w, h) = dims;
        if w == 0 || h == 0 {
            return None;
        }
        let cell = w * h;
        let count = data.len() / cell;
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            glyphs.push(data[i * cell..(i + 1) * cell].to_vec());
        }
        let first_char = if name.starts_with("fntNUMB") { b'0' } else { 32 };
        Some(GlyphFont {
            width: w,
            height: h,
            first_char,
            glyphs,
        })
    }

    /// Fetch a glyph by character.
    pub fn glyph(&self, ch: u8) -> Option<&[u8]> {
        let idx = (ch.wrapping_sub(self.first_char)) as usize;
        self.glyphs.get(idx).map(|v| v.as_slice())
    }

    /// Pixel value for `(x, y)` inside a glyph.
    pub fn pixel(&self, glyph: &[u8], x: usize, y: usize) -> u8 {
        glyph[y * self.width + x]
    }
}
