//! `INDEX.SF2` — the game's asset directory.
//!
//! Layout (verified against the loader code, `FUN_1000_3575`):
//!
//! ```text
//! 0x0000  8 x 16-byte NUL-padded data-file names
//! 0x0080  3749 x 17-byte records:
//!           u8  flags   high nibble = file index (0..3)
//!                       low nibble  = method (0 raw, 3 LZW)
//!           u24 offset  little-endian offset inside the data file
//!           u16 size    little-endian stored size in bytes
//!           11B name    space-padded ASCII (no NUL)
//! ```
//!
//! The loader computes `record_count = (file_size - 0x80) / 17`, which this parser
//! also enforces.

/// A single asset-directory record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// Raw flag byte: `>> 4` = data file index, `& 0x0F` = storage method.
    pub flags: u8,
    /// Offset of the payload inside its data file.
    pub offset: u32,
    /// Stored size in bytes (compressed size for method 3).
    pub size: u16,
    /// Space-padded asset name, exactly 11 characters as stored.
    pub name: String,
    /// Index of this record in the directory (this is the runtime handle basis:
    /// the engine encodes handle `SI = -(index + 1)`).
    pub index: usize,
}

impl Record {
    /// Data-file index (`0..=7`), the high nibble of the flag byte.
    pub fn file_index(&self) -> usize {
        (self.flags >> 4) as usize
    }

    /// Storage method, the low nibble of the flag byte (`0` raw, `3` LZW).
    pub fn method(&self) -> u8 {
        self.flags & 0x0F
    }

    /// Asset name with trailing spaces trimmed.
    pub fn trimmed_name(&self) -> &str {
        self.name.trim_end()
    }
}

/// In-memory representation of `INDEX.SF2`.
#[derive(Debug, Clone)]
pub struct AssetIndex {
    /// Data-file names from the 8 header slots (empty string for unused slots).
    pub files: Vec<String>,
    /// All directory records, in file order.
    pub records: Vec<Record>,
}

/// Errors produced while parsing `INDEX.SF2`.
#[derive(Debug)]
pub enum IndexError {
    /// File is smaller than the 0x80-byte header.
    TooShort,
    /// The remaining bytes after the header are not a whole number of records.
    TruncatedRecord,
    /// A record name was not valid ASCII.
    BadName,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::TooShort => write!(f, "INDEX.SF2 is shorter than its 128-byte header"),
            IndexError::TruncatedRecord => write!(f, "INDEX.SF2 trailing bytes are not a whole record"),
            IndexError::BadName => write!(f, "INDEX.SF2 contains a non-ASCII asset name"),
        }
    }
}

impl std::error::Error for IndexError {}

impl AssetIndex {
    /// Parse the raw bytes of `INDEX.SF2`.
    pub fn parse(data: &[u8]) -> Result<Self, IndexError> {
        if data.len() < 0x80 {
            return Err(IndexError::TooShort);
        }
        let mut files = Vec::with_capacity(8);
        for i in 0..8 {
            let slot = &data[i * 16..(i + 1) * 16];
            let end = slot.iter().position(|&b| b == 0).unwrap_or(slot.len());
            files.push(String::from_utf8_lossy(&slot[..end]).into_owned());
        }
        let rest = &data[0x80..];
        if rest.len() % 17 != 0 {
            return Err(IndexError::TruncatedRecord);
        }
        let mut records = Vec::with_capacity(rest.len() / 17);
        for (i, chunk) in rest.chunks_exact(17).enumerate() {
            let flags = chunk[0];
            let offset = u32::from_le_bytes([chunk[1], chunk[2], chunk[3], 0]);
            let size = u16::from_le_bytes([chunk[4], chunk[5]]);
            let name_bytes = &chunk[6..17];
            if !name_bytes.is_ascii() {
                return Err(IndexError::BadName);
            }
            records.push(Record {
                flags,
                offset,
                size,
                name: String::from_utf8_lossy(name_bytes).into_owned(),
                index: i,
            });
        }
        Ok(AssetIndex { files, records })
    }

    /// Find a record by trimmed name (first match).
    pub fn find(&self, name: &str) -> Option<&Record> {
        self.records.iter().find(|r| r.trimmed_name() == name)
    }

    /// Find all records whose trimmed name starts with `prefix`.
    pub fn find_prefix(&self, prefix: &str) -> Vec<&Record> {
        self.records
            .iter()
            .filter(|r| r.trimmed_name().starts_with(prefix))
            .collect()
    }
}

impl Record {
    /// Payload slice inside the file image (`data` must be the whole data file).
    pub fn payload<'a>(&self, data: &'a [u8]) -> &'a [u8] {
        let start = self.offset as usize;
        let end = start + self.size as usize;
        &data[start..end.min(data.len())]
    }
}
