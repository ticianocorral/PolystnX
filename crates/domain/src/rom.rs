//! ROM identification — section 4.1 of the plan.
//!
//! Hashes are taken over the *headerless* ROM so they line up with the No-Intro
//! DATs. A 512-byte copier header (SNES `.smc`) is detected by size parity and
//! stripped before hashing.

use std::fs;
use std::path::Path;

use sha1::{Digest, Sha1};

#[derive(Debug, Clone)]
pub struct RomId {
    /// Length of the copier header that was skipped (0 or 512).
    pub header_len: usize,
    /// Headerless payload length in bytes.
    pub rom_len: usize,
    pub crc32: String,
    pub sha1: String,
    /// 21-byte title from the SNES internal header, trimmed. Best-effort.
    pub internal_name: Option<String>,
    /// LoROM / HiROM guess, for diagnostics.
    pub mapper: Mapper,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Mapper {
    LoRom,
    HiRom,
    Unknown,
}

#[derive(Debug, thiserror::Error)]
pub enum RomError {
    #[error("could not read ROM {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("file is too small to be a SNES ROM ({0} bytes)")]
    TooSmall(usize),
}

impl RomId {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RomError> {
        let path = path.as_ref();
        let bytes = fs::read(path).map_err(|source| RomError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_bytes(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RomError> {
        if bytes.len() < 0x8000 {
            return Err(RomError::TooSmall(bytes.len()));
        }
        // A raw SNES ROM is a multiple of 32 KiB. A leading 512-byte copier
        // header shows up as a 512-byte remainder.
        let header_len = if bytes.len() % 1024 == 512 { 512 } else { 0 };
        let rom = &bytes[header_len..];

        let mut crc = crc32fast::Hasher::new();
        crc.update(rom);
        let crc32 = format!("{:08X}", crc.finalize());

        let sha1 = format!("{:x}", Sha1::digest(rom));

        let (mapper, internal_name) = read_internal_header(rom);

        Ok(RomId {
            header_len,
            rom_len: rom.len(),
            crc32,
            sha1,
            internal_name,
            mapper,
        })
    }
}

/// Probe the two common header locations and keep whichever looks more like
/// printable ASCII text.
fn read_internal_header(rom: &[u8]) -> (Mapper, Option<String>) {
    const LO: usize = 0x7FC0;
    const HI: usize = 0xFFC0;

    let lo = title_at(rom, LO);
    let hi = title_at(rom, HI);
    match (score(&lo), score(&hi)) {
        (l, h) if h > l => (Mapper::HiRom, hi),
        (l, _) if l > 0 => (Mapper::LoRom, lo),
        _ => (Mapper::Unknown, None),
    }
}

fn title_at(rom: &[u8], off: usize) -> Option<String> {
    let end = off + 21;
    if rom.len() < end {
        return None;
    }
    let raw = &rom[off..end];
    let s: String = raw
        .iter()
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                b as char
            } else {
                ' '
            }
        })
        .collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn score(name: &Option<String>) -> usize {
    match name {
        None => 0,
        Some(s) => s.chars().filter(|c| c.is_ascii_alphanumeric()).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 32 KiB blob with a LoROM-looking title should hash headerless and,
    /// with 512 bytes prepended, produce the *same* hashes.
    #[test]
    fn strips_copier_header() {
        let mut rom = vec![0u8; 0x8000];
        for (i, b) in b"TEST ROM TITLE".iter().enumerate() {
            rom[0x7FC0 + i] = *b;
        }
        let bare = RomId::from_bytes(&rom).unwrap();
        assert_eq!(bare.header_len, 0);
        assert_eq!(bare.rom_len, 0x8000);

        let mut headered = vec![0u8; 512];
        headered.extend_from_slice(&rom);
        let hd = RomId::from_bytes(&headered).unwrap();
        assert_eq!(hd.header_len, 512);
        assert_eq!(hd.sha1, bare.sha1);
        assert_eq!(hd.crc32, bare.crc32);
        assert_eq!(hd.internal_name.as_deref(), Some("TEST ROM TITLE"));
    }

    #[test]
    fn rejects_tiny_files() {
        assert!(matches!(
            RomId::from_bytes(&[0u8; 16]),
            Err(RomError::TooSmall(16))
        ));
    }
}
