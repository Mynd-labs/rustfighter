//! Sound subsystem — data layer of the port.
//!
//! Ported from the reverse-engineered routines (see `docs/port/sound.md`):
//!
//! * `voc*` — Creative Voice 1.10 files, codec 0 (8-bit unsigned PCM), played by
//!   the Sound Blaster path (`sfx_play` = `0x19A66`; the DOS driver is
//!   single-voice — a new sample cuts the previous one).
//! * `mit*` — Standard MIDI Files, format 0, 96 ppq, played via the MPU/AdLib
//!   path (`music_start` = `0x19CD6`; `TUNE*` = AdLib renditions, `AT*` = Roland
//!   renditions, `mitRFX` = shared percussion track).
//! * `tun*` — PC-speaker note tables: `(period, duration)` pairs terminated by
//!   `0xFFFF`, `period == 0` = rest; frequency = `1_193_182 / period` Hz.
//!
//! This module is deliberately dependency-free and testable headlessly; audio
//! output (resampling into a device sink) is layered on top elsewhere.

use crate::gamedata::GameData;

/// A decoded Creative Voice sample.
#[derive(Debug, Clone)]
pub struct Voc {
    /// Asset name (e.g. `vocPUNCH`).
    pub name: String,
    /// Sample rate in Hz derived from the time constant: `1e6 / (256 - TC)`.
    pub rate_hz: u32,
    /// Unsigned 8-bit mono PCM (as stored; silence = 0x80).
    pub samples: Vec<u8>,
    /// Number of type-1 data blocks found (`vocBUZZ` has 8; the DOS driver
    /// only ever played the first).
    pub blocks: usize,
}

impl Voc {
    /// Samples as signed 16-bit (centred).
    pub fn samples_i16(&self) -> Vec<i16> {
        self.samples
            .iter()
            .map(|&b| ((b as i32) - 128) as i16 * 256)
            .collect()
    }

    /// Linear-interpolation resample into `out_hz`.
    pub fn resample(&self, out_hz: u32) -> Vec<i16> {
        if self.rate_hz == 0 || out_hz == 0 {
            return Vec::new();
        }
        let src = self.samples_i16();
        let ratio = self.rate_hz as f64 / out_hz as f64;
        let out_len = ((src.len() as f64) / ratio) as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = pos - idx as f64;
            let a = src.get(idx).copied().unwrap_or(0) as f64;
            let b = src.get(idx + 1).copied().unwrap_or(0) as f64;
            out.push((a + (b - a) * frac) as i16);
        }
        out
    }
}

/// Errors decoding a VOC file.
#[derive(Debug, PartialEq, Eq)]
pub enum VocError {
    /// Not a Creative Voice file.
    BadMagic,
    /// Truncated block structure.
    Truncated,
    /// No data block found.
    NoData,
}

impl std::fmt::Display for VocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VocError::BadMagic => write!(f, "not a Creative Voice file"),
            VocError::Truncated => write!(f, "truncated VOC block structure"),
            VocError::NoData => write!(f, "VOC contains no data block"),
        }
    }
}

impl std::error::Error for VocError {}

const VOC_MAGIC: &[u8; 20] = b"Creative Voice File\x1a";

/// Parse a Creative Voice 1.10 file.
pub fn parse_voc(name: &str, data: &[u8]) -> Result<Voc, VocError> {
    if data.len() < 26 || &data[..20] != VOC_MAGIC {
        return Err(VocError::BadMagic);
    }
    let header_size = u16::from_le_bytes([data[0x14], data[0x15]]) as usize;
    let mut pos = header_size.max(26);
    let mut samples: Vec<u8> = Vec::new();
    let mut rate_hz = 0u32;
    let mut blocks = 0usize;
    while pos < data.len() {
        let block_type = data[pos];
        pos += 1;
        if block_type == 0 {
            break; // end of file
        }
        if pos + 3 > data.len() {
            return Err(VocError::Truncated);
        }
        let len = u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], 0]) as usize;
        pos += 3;
        if pos + len > data.len() {
            return Err(VocError::Truncated);
        }
        let body = &data[pos..pos + len];
        pos += len;
        match block_type {
            1 => {
                if body.len() < 2 {
                    return Err(VocError::Truncated);
                }
                let tc = body[0];
                let codec = body[1];
                if codec != 0 {
                    // Only codec 0 (8-bit PCM) exists in the shipped data.
                    return Err(VocError::Truncated);
                }
                if rate_hz == 0 {
                    rate_hz = 1_000_000 / (256 - tc as u32);
                }
                samples.extend_from_slice(&body[2..]);
                blocks += 1;
            }
            2 => {
                // Continuation: append to the previous block's data.
                samples.extend_from_slice(body);
            }
            _ => {} // 3..9 (silence, markers, text, ...) are unused by the game
        }
    }
    if samples.is_empty() {
        return Err(VocError::NoData);
    }
    Ok(Voc {
        name: name.to_string(),
        rate_hz,
        samples,
        blocks,
    })
}

/// One event of a Standard MIDI File track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmfEvent {
    /// Delta time in ticks from the previous event.
    pub delta: u32,
    /// Raw MIDI message bytes (running status already expanded).
    pub message: Vec<u8>,
}

/// A decoded Standard MIDI file (format 0).
#[derive(Debug, Clone)]
pub struct Smf {
    /// Asset name (e.g. `mitSF2TUNE1`).
    pub name: String,
    /// Ticks per quarter note (`mit*` files use 96).
    pub division: u16,
    /// All events of the single track, in order.
    pub events: Vec<SmfEvent>,
    /// First tempo meta event value (microseconds per quarter note), if any.
    pub tempo_us: Option<u32>,
}

/// Errors decoding an SMF file.
#[derive(Debug, PartialEq, Eq)]
pub enum SmfError {
    /// Missing `MThd` chunk.
    BadHeader,
    /// Truncated chunk structure.
    Truncated,
    /// Unsupported (non-format-0) file.
    UnsupportedFormat,
}

impl std::fmt::Display for SmfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SmfError::BadHeader => write!(f, "missing MThd chunk"),
            SmfError::Truncated => write!(f, "truncated SMF data"),
            SmfError::UnsupportedFormat => write!(f, "unsupported SMF format"),
        }
    }
}

impl std::error::Error for SmfError {}

fn read_u32(data: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(pos..pos + 4)?.try_into().ok()?))
}

fn read_vlq(data: &[u8], pos: &mut usize) -> Option<u32> {
    let mut value = 0u32;
    for _ in 0..4 {
        let byte = *data.get(*pos)?;
        *pos += 1;
        value = (value << 7) | (byte & 0x7F) as u32;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    Some(value)
}

/// Parse a Standard MIDI Format 0 file.
pub fn parse_smf(name: &str, data: &[u8]) -> Result<Smf, SmfError> {
    if data.len() < 14 || &data[0..4] != b"MThd" {
        return Err(SmfError::BadHeader);
    }
    let header_len = read_u32(data, 4).ok_or(SmfError::Truncated)? as usize;
    if data.len() < 8 + header_len {
        return Err(SmfError::Truncated);
    }
    let format = u16::from_be_bytes([data[8], data[9]]);
    let tracks = u16::from_be_bytes([data[10], data[11]]);
    let division = u16::from_be_bytes([data[12], data[13]]);
    if format != 0 || tracks != 1 {
        return Err(SmfError::UnsupportedFormat);
    }
    let mut pos = 8 + header_len;
    if data.get(pos..pos + 4) != Some(b"MTrk") {
        return Err(SmfError::BadHeader);
    }
    pos += 4;
    let track_len = read_u32(data, pos).ok_or(SmfError::Truncated)? as usize;
    pos += 4;
    let track = data.get(pos..pos + track_len).ok_or(SmfError::Truncated)?;

    let mut events = Vec::new();
    let mut tempo_us = None;
    let mut cursor = 0usize;
    let mut running_status = 0u8;
    while cursor < track.len() {
        let delta = read_vlq(track, &mut cursor).ok_or(SmfError::Truncated)?;
        let mut status = *track.get(cursor).ok_or(SmfError::Truncated)?;
        if status & 0x80 == 0 {
            status = running_status; // running status
        } else {
            cursor += 1;
        }
        running_status = status;
        match status {
            0xFF => {
                let meta_type = *track.get(cursor).ok_or(SmfError::Truncated)?;
                cursor += 1;
                let len = read_vlq(track, &mut cursor).ok_or(SmfError::Truncated)? as usize;
                let body = track.get(cursor..cursor + len).ok_or(SmfError::Truncated)?;
                cursor += len;
                if meta_type == 0x51 && len == 3 {
                    tempo_us = Some(u32::from_be_bytes([0, body[0], body[1], body[2]]));
                }
                events.push(SmfEvent {
                    delta,
                    message: vec![0xFF, meta_type],
                });
            }
            0xF0 | 0xF7 => {
                let len = read_vlq(track, &mut cursor).ok_or(SmfError::Truncated)? as usize;
                cursor += len;
            }
            _ => {
                let len = match status >> 4 {
                    0x8 | 0x9 | 0xA | 0xB | 0xE => 2,
                    0xC | 0xD => 1,
                    _ => return Err(SmfError::Truncated),
                };
                let data_bytes = track
                    .get(cursor..cursor + len)
                    .ok_or(SmfError::Truncated)?;
                cursor += len;
                let mut message = Vec::with_capacity(len + 1);
                message.push(status);
                message.extend_from_slice(data_bytes);
                events.push(SmfEvent { delta, message });
            }
        }
    }
    Ok(Smf {
        name: name.to_string(),
        division,
        events,
        tempo_us,
    })
}

/// A decoded PC-speaker tune (`tun*`).
#[derive(Debug, Clone)]
pub struct Tun {
    /// Asset name (e.g. `tunBSFX1`).
    pub name: String,
    /// `(period, duration)` pairs; `0xFFFF` terminator removed.
    pub notes: Vec<(u16, u16)>,
}

impl Tun {
    /// Frequency in Hz for a period value (`0` = rest).
    pub fn frequency(period: u16) -> f32 {
        if period == 0 {
            0.0
        } else {
            1_193_182.0 / period as f32
        }
    }
}

/// Parse a `tun*` note table (`0xFFFF`-terminated `u16` pairs).
pub fn parse_tun(name: &str, data: &[u8]) -> Tun {
    let mut notes = Vec::new();
    let mut pos = 0usize;
    while pos + 4 <= data.len() {
        let period = u16::from_le_bytes([data[pos], data[pos + 1]]);
        if period == 0xFFFF {
            break;
        }
        let duration = u16::from_le_bytes([data[pos + 2], data[pos + 3]]);
        notes.push((period, duration));
        pos += 4;
    }
    Tun {
        name: name.to_string(),
        notes,
    }
}

/// The decoded sound assets of the game, keyed the way the engine uses them.
#[derive(Debug, Default)]
pub struct SoundBank {
    /// All `voc*` samples in the engine's id order (alphabetical, id 0 = `vocBALL`).
    pub sfx: Vec<Voc>,
    /// `mitSF2TUNE1..6` — AdLib/OPL renditions (music ids 0..=5).
    pub tune: Vec<Smf>,
    /// `mitSF2AT1..6` — Roland renditions.
    pub at: Vec<Smf>,
    /// Shared percussion track (`mitRFX`).
    pub rfx: Option<Smf>,
    /// All `tun*` tables by name.
    pub speaker: Vec<Tun>,
}

impl SoundBank {
    /// Decode every sound asset from the game data.
    pub fn load(gd: &GameData) -> Result<Self, String> {
        let mut bank = SoundBank::default();

        // voc* sorted alphabetically == the engine's id table (docs/port/sound.md §9).
        let mut voc_names: Vec<String> = gd
            .index
            .find_prefix("voc")
            .into_iter()
            .map(|r| r.trimmed_name().to_string())
            .collect();
        voc_names.sort();
        for name in voc_names {
            let data = gd.decoded(&name)?;
            match parse_voc(&name, &data) {
                Ok(voc) => bank.sfx.push(voc),
                Err(e) => return Err(format!("{name}: {e}")),
            }
        }

        // Music: TUNE1..6 and AT1..6 in numeric order, plus the shared RFX.
        for i in 1..=6 {
            let name = format!("mitSF2TUNE{i}");
            let data = gd
                .decoded(&name)
                .map_err(|e| format!("{name}: {e}"))?;
            bank.tune.push(parse_smf(&name, &data).map_err(|e| format!("{name}: {e}"))?);
        }
        for i in 1..=6 {
            let name = format!("mitSF2AT{i}");
            let data = gd.decoded(&name).map_err(|e| format!("{name}: {e}"))?;
            bank.at.push(parse_smf(&name, &data).map_err(|e| format!("{name}: {e}"))?);
        }
        if let Ok(data) = gd.decoded("mitRFX") {
            bank.rfx = Some(parse_smf("mitRFX", &data).map_err(|e| format!("mitRFX: {e}"))?);
        }
        let rfx_alt = "mitRFX  ".trim_end().to_string();
        if bank.rfx.is_none() {
            if let Ok(data) = gd.decoded(&rfx_alt) {
                bank.rfx = Some(parse_smf("mitRFX", &data).map_err(|e| format!("mitRFX: {e}"))?);
            }
        }

        // PC-speaker tunes.
        for rec in gd.index.find_prefix("tun") {
            let name = rec.trimmed_name().to_string();
            let data = gd.decoded(&name)?;
            bank.speaker.push(parse_tun(&name, &data));
        }
        bank.speaker.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(bank)
    }

    /// Sfx id (0..) -> sample, mirroring the engine's `0x19A66` table.
    pub fn sfx_by_id(&self, id: usize) -> Option<&Voc> {
        self.sfx.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voc_time_constant_to_rate() {
        assert_eq!(1_000_000 / (256 - 6), 4000);
        assert_eq!(1_000_000 / (256 - 56), 5000);
        assert_eq!(1_000_000 / (256 - 113), 6993);
        assert_eq!(1_000_000 / (256 - 131), 8000);
        assert_eq!(1_000_000 / (256 - 156), 10000);
    }

    #[test]
    fn tun_frequency_and_rest() {
        assert_eq!(Tun::frequency(0), 0.0);
        // Period 0x04A8 -> ~1000 Hz.
        let f = Tun::frequency(0x04A8);
        assert!((f - 1000.0).abs() < 5.0, "f={f}");
    }

    #[test]
    fn tun_terminator_stops_parsing() {
        let data = [0x00, 0x01, 0x10, 0x00, 0xFF, 0xFF, 0xAA, 0xBB];
        let tun = parse_tun("t", &data);
        assert_eq!(tun.notes, vec![(0x0100, 0x0010)]);
    }
}
