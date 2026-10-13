//! 8-bit indexed framebuffer with blitter and text helpers, plus RGB conversion
//! through a [`Palette`].

use crate::font::GlyphFont;
use crate::palette::Palette;
use crate::sprite::Frame;

/// An indexed-colour canvas.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Palette index per pixel, row-major.
    pub pixels: Vec<u8>,
    /// Palette used for RGB conversion.
    pub palette: Palette,
}

impl Canvas {
    /// Create a black canvas of the given size.
    pub fn new(width: usize, height: usize) -> Self {
        Canvas {
            width,
            height,
            pixels: vec![0; width * height],
            palette: Palette::default(),
        }
    }

    /// Fill the whole canvas with one palette index.
    pub fn clear(&mut self, index: u8) {
        self.pixels.fill(index);
    }

    /// Set a pixel (out-of-bounds is ignored).
    pub fn set(&mut self, x: i32, y: i32, index: u8) {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return;
        }
        self.pixels[y as usize * self.width + x as usize] = index;
    }

    /// Read a pixel (out-of-bounds returns 0).
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return 0;
        }
        self.pixels[y as usize * self.width + x as usize]
    }

    /// Fill an axis-aligned rectangle.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, index: u8) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, index);
            }
        }
    }

    /// Blit a sprite frame at 1:1. Palette index 0 is transparent.
    pub fn blit(&mut self, frame: &Frame, x: i32, y: i32, flip_x: bool) {
        for fy in 0..frame.height as i32 {
            for fx in 0..frame.width as i32 {
                let sx = if flip_x { frame.width as i32 - 1 - fx } else { fx };
                let px = frame.pixels[(fy as usize) * frame.width as usize + sx as usize];
                if px != 0 {
                    self.set(x + fx, y + fy, px);
                }
            }
        }
    }

    /// Blit a sprite frame scaled by an integer factor (nearest neighbour).
    pub fn blit_scaled(&mut self, frame: &Frame, x: i32, y: i32, scale: i32, flip_x: bool) {
        for fy in 0..frame.height as i32 {
            for fx in 0..frame.width as i32 {
                let sx = if flip_x { frame.width as i32 - 1 - fx } else { fx };
                let px = frame.pixels[(fy as usize) * frame.width as usize + sx as usize];
                if px != 0 {
                    self.rect(x + fx * scale, y + fy * scale, scale, scale, px);
                }
            }
        }
    }

    /// Draw one glyph of `font` at `(x, y)`; glyph indices 0 are transparent.
    pub fn draw_glyph(&mut self, font: &GlyphFont, ch: u8, x: i32, y: i32) {
        if let Some(glyph) = font.glyph(ch) {
            for gy in 0..font.height {
                for gx in 0..font.width {
                    let px = font.pixel(glyph, gx, gy);
                    if px != 0 {
                        self.set(x + gx as i32, y + gy as i32, px);
                    }
                }
            }
        }
    }

    /// Draw a string with `font`, returning the advance width.
    pub fn draw_text(&mut self, font: &GlyphFont, text: &str, x: i32, y: i32) -> i32 {
        let mut cx = x;
        for &b in text.as_bytes() {
            self.draw_glyph(font, b, cx, y);
            cx += font.width as i32;
        }
        cx - x
    }

    /// Convert to packed RGB8 using the canvas palette.
    pub fn to_rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 3);
        for &p in &self.pixels {
            let c = self.palette.get(p);
            out.extend_from_slice(&c);
        }
        out
    }

    /// Convert to packed RGBA8 (alpha 255) using the canvas palette.
    pub fn to_rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for &p in &self.pixels {
            let c = self.palette.get(p);
            out.extend_from_slice(&[c[0], c[1], c[2], 255]);
        }
        out
    }

    /// Save as PNG through the `image` crate.
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), image::ImageError> {
        let img = image::RgbImage::from_raw(
            self.width as u32,
            self.height as u32,
            self.to_rgb(),
        )
        .expect("canvas size matches buffer");
        img.save(path)
    }
}
