//! A bundled year/publisher lookup by CRC32, sourced from TOSEC's SNES
//! "Games" datfile (plan revision: "baixar o que esta la [TOSEC] e embutir
//! no app") — embedded so every install has *some* release-year/publisher
//! info out of the box, no user setup needed (unlike the optional
//! user-supplied No-Intro DAT, which still wins when it has the same fact —
//! see `catalog.rs`'s merge).
//!
//! TOSEC itself has no per-game `<year>`/`<publisher>` XML fields — both are
//! encoded straight into the game's own cataloguing *name* by the TOSEC
//! naming convention instead, e.g. `"Chrono Trigger (1995)(Square)(US)[tr
//! de]"`. Only those two bare facts, pulled back out of the name once at
//! generation time (`scripts/gen_tosec_data.py`, not parsed here at
//! runtime), are kept — TOSEC's own catalogued name/description text never
//! ships in this app; the title shown on screen still comes from the ROM's
//! own header or an optional No-Intro DAT, exactly as before.
//!
//! `tosec_data.txt` (checked in next to this file) is plain TSV, one CRC32
//! per line: `CRC32\t<year, or empty>\t<publisher, or empty>`. TOSEC
//! doesn't publish an explicit reuse license for its datfiles the way
//! libretro-database does (CC BY-SA 4.0, see `cheats.rs`) — see
//! `THIRD-PARTY-NOTICES.md` for what that means here: only bare facts
//! (a year, a publisher's name) are embedded, not TOSEC's own catalogued
//! text.
//!
//! Some entries here exist for a CRC32 TOSEC never catalogued itself — a
//! revision/region dump it doesn't have under that exact hash, but a
//! No-Intro dat's `id`/`cloneofid` linkage says is the same game as one
//! TOSEC does have. `gen_tosec_data.py` uses that linkage only to decide
//! which CRC32s share a value; the value itself is still 100% TOSEC's,
//! never anything read from No-Intro's own name/description text.

use std::collections::HashMap;
use std::sync::OnceLock;

const DATA: &str = include_str!("tosec_data.txt");

/// A CRC32's bundled facts — either field may be missing even for a CRC32
/// that's in the table at all (rare; TOSEC almost always has both once it
/// has either).
pub struct TosecInfo {
    pub year: Option<&'static str>,
    pub publisher: Option<&'static str>,
}

/// Keyed by CRC32 in upper hex, matching `RomId::crc32`'s own formatting —
/// built once, the first time any lookup happens.
fn index() -> &'static HashMap<&'static str, TosecInfo> {
    static INDEX: OnceLock<HashMap<&'static str, TosecInfo>> = OnceLock::new();
    INDEX.get_or_init(|| {
        DATA.lines()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\t');
                let crc = parts.next()?;
                let year = parts.next()?;
                let publisher = parts.next()?;
                let info = TosecInfo {
                    year: (!year.is_empty()).then_some(year),
                    publisher: (!publisher.is_empty()).then_some(publisher),
                };
                Some((crc, info))
            })
            .collect()
    })
}

/// Look up a ROM's bundled year/publisher by its headerless CRC32 — `None`
/// if this CRC32 isn't in the bundled TOSEC set at all (most SNES ROMs
/// that exist are, but a very new dump or an unusual hack/translation
/// might not be).
pub fn lookup(crc32: &str) -> Option<&'static TosecInfo> {
    let upper = crc32.to_ascii_uppercase();
    index().get(upper.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_crc32_returns_none() {
        assert!(lookup("00000000").is_none());
    }

    #[test]
    fn data_file_is_well_formed_and_non_trivial() {
        // Loose sanity check on the embedded data itself, not a specific
        // game — catches an accidentally-empty or malformed regeneration
        // without pinning to any one CRC32 that a future re-run of
        // `gen_tosec_data.py` (against a newer TOSEC release) might drop.
        let idx = index();
        assert!(
            idx.len() > 1000,
            "expected a few thousand entries, got {}",
            idx.len()
        );
        assert!(idx
            .values()
            .any(|i| i.year.is_some() && i.publisher.is_some()));
        for key in idx.keys().take(50) {
            assert_eq!(key.len(), 8, "{key} doesn't look like an 8-hex-digit CRC32");
            assert!(key
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
        }
    }
}
