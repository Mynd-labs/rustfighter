//! Sprite frames: 4-bit-per-pixel nibble RLE.
//!
//! A decoded sprite record (after the LZW stage, if any) is:
//!
//! ```text
//! [u8 width][u8 height][RLE stream]
//! ```
//!
//! The RLE stream expands to exactly `ceil(width * height / 2)` bytes:
//!
//! * `00 NN`  — `NN` zero bytes (2*NN transparent pixels);
//! * `b != 0` — the byte `b` literally.
//!
//! Pixel packing: two pixels per byte, **high nibble first**, row-major with no
//! row padding. Palette index 0 is transparent.

/// One decoded sprite frame.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Frame width in pixels.
    pub width: u8,
    /// Frame height in pixels.
    pub height: u8,
    /// Palette indices, row-major, `width * height` entries; 0 = transparent.
    pub pixels: Vec<u8>,
}

/// Errors decoding a sprite record.
#[derive(Debug)]
pub enum SpriteError {
    /// Record does not even contain the `[w][h]` header.
    TooShort,
    /// RLE expansion length does not match `ceil(w*h/2)`.
    BadLength { expected: usize, actual: usize },
}

impl std::fmt::Display for SpriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpriteError::TooShort => write!(f, "sprite record shorter than its 2-byte header"),
            SpriteError::BadLength { expected, actual } => write!(
                f,
                "sprite RLE expanded to {actual} bytes, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for SpriteError {}

/// Expand the RLE stream to the packed-byte form (no size check).
pub fn expand_rle(rle: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < rle.len() {
        if rle[i] == 0 {
            let count = rle.get(i + 1).copied().unwrap_or(0) as usize;
            out.resize(out.len() + count, 0);
            i += 2;
        } else {
            out.push(rle[i]);
            i += 1;
        }
    }
    out
}

/// Decode a `[w][h][RLE]` sprite record (payload after any LZW stage).
pub fn decode(data: &[u8]) -> Result<Frame, SpriteError> {
    if data.len() < 2 {
        return Err(SpriteError::TooShort);
    }
    let width = data[0];
    let height = data[1];
    let expected = (width as usize * height as usize).div_ceil(2);
    let packed = expand_rle(&data[2..]);
    if packed.len() != expected {
        return Err(SpriteError::BadLength {
            expected,
            actual: packed.len(),
        });
    }
    let mut pixels = Vec::with_capacity(width as usize * height as usize);
    for byte in packed {
        pixels.push(byte >> 4);
        pixels.push(byte & 0x0F);
    }
    pixels.truncate(width as usize * height as usize);
    Ok(Frame {
        width,
        height,
        pixels,
    })
}
