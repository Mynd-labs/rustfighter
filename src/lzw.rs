//! The game's disk-compression codec ("method 3").
//!
//! Ported from the reverse-engineered algorithm (validated against all 3,749
//! records of the shipped data set; see `docs/findings/general-sf2-formats.md`
//! and `docs/findings/asset-loader.md`).
//!
//! Stream layout:
//!
//! ```text
//! payload[0..2] = u16 LE decompressed size
//! payload[2..]  = bit-packed stream, LSB-first
//! ```
//!
//! The code stream is a variable-width LZW variant:
//!
//! * codes start at 9 bits and grow to at most 16 (mask `(mask << 1) | 0xFF`);
//! * `code <= 0xFF`  — literal byte, then a new dictionary entry is recorded;
//! * `code == 0x100` — escape; the next 2 bits select:
//!     * `0` widen the code width,
//!     * `1` reset the dictionary (the following 9-bit code is a literal),
//!     * `2`/`3` end of stream;
//! * `code > 0x100`  — dictionary reference: `(start, len)` span of the output,
//!   including the KwKwK case (reference to the entry being created).

/// Decode a method-3 payload (including its 2-byte size header).
///
/// Returns the expanded bytes. The declared size is advisory; use
/// [`decode_payload_checked`] to enforce it.
pub fn decode_payload(payload: &[u8]) -> Vec<u8> {
    if payload.len() < 2 {
        return Vec::new();
    }
    let declared = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    let out = decode_stream(&payload[2..]);
    debug_assert!(
        out.len() == declared,
        "LZW output length {} != declared {}",
        out.len(),
        declared
    );
    out
}

/// Decode a method-3 payload and enforce the declared size.
pub fn decode_payload_checked(payload: &[u8]) -> Result<Vec<u8>, String> {
    if payload.len() < 2 {
        return Err("payload too short for size header".into());
    }
    let declared = u16::from_le_bytes([payload[0], payload[1]]) as usize;
    let out = decode_stream(&payload[2..]);
    if out.len() != declared {
        return Err(format!(
            "decompressed length {} != declared {}",
            out.len(),
            declared
        ));
    }
    Ok(out)
}

/// Decode the raw code stream (payload without its size header).
pub fn decode_stream(stream: &[u8]) -> Vec<u8> {
    let total_bits = stream.len() * 8;
    let mut bitpos: usize = 0;

    // LSB-first bit reader; bits past the end read as 0 (mirrors the original
    // big-integer-style reader).
    let read = |bitpos: &mut usize, nbits: u32| -> u32 {
        let mut v: u32 = 0;
        for i in 0..nbits as usize {
            let p = *bitpos + i;
            let bit = if p < total_bits {
                (stream[p >> 3] >> (p & 7)) & 1
            } else {
                0
            };
            v |= (bit as u32) << i;
        }
        *bitpos += nbits as usize;
        v
    };

    let mut out: Vec<u8> = Vec::new();
    let mut mask: u32 = 0x1FF; // 9-bit codes initially
    let mut table: Vec<(usize, usize)> = Vec::with_capacity(256); // slot -> (start, len)
    let mut prev_len: usize = 1;

    // The first code after (re)start is always a literal byte.
    let first = read(&mut bitpos, 9);
    out.push((first & 0xFF) as u8);

    while bitpos + 9 <= total_bits {
        let width = if mask == 0x1FF { 9 } else { 32 - mask.leading_zeros() };
        let code = read(&mut bitpos, width);
        if code <= 0xFF {
            out.push(code as u8);
            table.push((out.len() - 1 - prev_len, prev_len + 1));
            prev_len = 1;
            continue;
        }
        if code == 0x100 {
            let extra = read(&mut bitpos, 2);
            match extra {
                0 => {
                    mask = (mask << 1) | 0xFF;
                    continue;
                }
                1 => {
                    let literal = read(&mut bitpos, 9);
                    out.push((literal & 0xFF) as u8);
                    mask = 0x1FF;
                    table.clear();
                    prev_len = 1;
                    continue;
                }
                _ => break,
            }
        }
        // Dictionary reference. Slot number = code - 0x101.
        let slot = (code - 0x101) as usize;
        if slot < table.len() {
            let (start, len) = table[slot];
            table.push((out.len() - prev_len, prev_len + 1));
            for i in 0..len {
                let v = out.get(start + i).copied().unwrap_or(0);
                out.push(v);
            }
            prev_len = len;
        } else {
            // KwKwK: reference to the slot currently being created.
            let start = out.len() - prev_len;
            table.push((start, prev_len + 1));
            let len = prev_len + 1;
            for i in 0..len {
                let v = out.get(start + i).copied().unwrap_or(0);
                out.push(v);
            }
            prev_len = len;
        }
    }
    out
}
