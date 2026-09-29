//! No-Intro DAT parsing (plan §4.1): canonical ROM titles by CRC32. No
//! network here — the app downloads the DAT itself when missing (setup
//! screen, `xperience_app::dat_update`, from the libretro-database mirror)
//! or the user drops one at `xperience_app::dirs::nointro_dat_path`.
//! Entirely optional: without it, the catalog falls back to the SNES
//! header's internal title or the file name, same as before.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum NoIntroError {
    #[error("could not read DAT {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not parse DAT: {0}")]
    Parse(#[from] roxmltree::Error),
}

/// One game's entry: the canonical name every DAT has, plus whatever a
/// richer DAT (TOSEC-style, or a Logiqx one with per-game extras) happens to
/// also carry — all optional, since a plain No-Intro DAT usually has none of
/// them (plan revision: "se o DAT tiver informacoes do jogo, preencher no
/// painel" — conditional on the DAT actually having something beyond the
/// name).
#[derive(Debug, Clone, Default)]
pub struct NoIntroGameInfo {
    pub name: String,
    pub description: Option<String>,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub category: Option<String>,
}

/// Game info, keyed by the ROM's headerless CRC32 in upper hex (e.g.
/// `"05FBB855"`) — matches `RomId::crc32`'s own formatting exactly, so
/// lookups need no reformatting on that side.
pub struct NoIntroDat {
    by_crc32: HashMap<String, NoIntroGameInfo>,
}

/// Child element of `<game>`, if present and non-empty, as plain text.
fn child_text<'a>(game: roxmltree::Node<'a, 'a>, tag: &str) -> Option<String> {
    let text = game
        .children()
        .find(|n| n.has_tag_name(tag))?
        .text()?
        .trim();
    (!text.is_empty()).then(|| text.to_string())
}

impl NoIntroDat {
    /// Parse a No-Intro DAT: either the Logiqx `<datafile>` XML a
    /// DAT-o-MATIC export produces (`<game name="...">` per title, one or
    /// more `<rom crc="...">` children, plus whichever of
    /// `<description>`/`<year>`/`<publisher>`/`<category>` the DAT happens
    /// to include — all optional), or the clrmamepro text flavour the
    /// libretro-database mirror serves (the setup screen's download source,
    /// same data). Whatever the flavour, entries land keyed by the ROM's
    /// headerless CRC32 in upper hex.
    pub fn load(path: &Path) -> Result<Self, NoIntroError> {
        let text = fs::read_to_string(path).map_err(|source| NoIntroError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let by_crc32 = if text.trim_start().starts_with("clrmamepro") {
            parse_clrmamepro(&text)
        } else {
            let doc = roxmltree::Document::parse(&text)?;
            let mut by_crc32 = HashMap::new();
            for game in doc.descendants().filter(|n| n.has_tag_name("game")) {
                let Some(name) = game.attribute("name") else {
                    continue;
                };
                let info = NoIntroGameInfo {
                    name: name.to_string(),
                    description: child_text(game, "description"),
                    year: child_text(game, "year"),
                    publisher: child_text(game, "publisher")
                        .or_else(|| child_text(game, "manufacturer")),
                    category: child_text(game, "category"),
                };
                for rom in game.children().filter(|n| n.has_tag_name("rom")) {
                    if let Some(crc) = rom.attribute("crc") {
                        by_crc32.insert(crc.to_ascii_uppercase(), info.clone());
                    }
                }
            }
            by_crc32
        };
        log::info!(
            "no-intro DAT: {} entries loaded from {}",
            by_crc32.len(),
            path.display()
        );
        Ok(Self { by_crc32 })
    }

    pub fn lookup(&self, crc32: &str) -> Option<&NoIntroGameInfo> {
        self.by_crc32.get(&crc32.to_ascii_uppercase())
    }
}

/// The quoted value of a `key "value"` line (clrmamepro flavour) — `None`
/// unless the line is exactly that key with a quoted string on it.
fn clrmame_quoted(line: &str, key: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(key)?.trim_start();
    let value = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some(value.to_string())
}

/// Parse clrmamepro text (what `libretro-database`'s
/// `metadat/no-intro/*.dat` files are): a `clrmamepro (` header, then one
/// `game (` block per title carrying a quoted `name`, each with one `rom (`
/// entry whose `crc XXXXXXXX` token is the lookup key. Only what the
/// lookups need is kept — none of the region/serializer metadata.
fn parse_clrmamepro(text: &str) -> HashMap<String, NoIntroGameInfo> {
    let mut by_crc32 = HashMap::new();
    let mut game: Option<NoIntroGameInfo> = None;
    let mut in_rom = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("game (") {
            game = Some(NoIntroGameInfo::default());
            in_rom = false;
            continue;
        }
        if line.starts_with("rom (") {
            in_rom = true;
        }
        if let Some(info) = game.as_mut() {
            if !in_rom {
                if let Some(name) = clrmame_quoted(line, "name") {
                    info.name = name;
                }
            }
            if let Some(crc_start) = line.find("crc ") {
                let crc: String = line[crc_start + 4..]
                    .trim_start()
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != ')')
                    .collect();
                if crc.len() == 8 {
                    by_crc32.insert(crc.to_ascii_uppercase(), info.clone());
                }
            }
        }
        if line == ")" {
            if in_rom {
                in_rom = false;
            } else {
                game = None;
            }
        }
    }
    by_crc32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_dat(contents: &str) -> std::path::PathBuf {
        // PID alone isn't unique enough — cargo runs tests in this file
        // concurrently on separate threads of the same process, and two
        // tests racing on one shared path could each load the other's
        // contents. Thread id disambiguates them, same fix `catalog.rs`'s
        // own temp-file tests already use.
        let path = std::env::temp_dir().join(format!(
            "xperience-nointro-test-{}-{:?}.dat",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn resolves_name_by_crc32_case_insensitively() {
        let path = write_dat(
            r#"<?xml version="1.0"?>
<datafile>
  <game name="Super Test World (World)">
    <rom name="Super Test World (World).sfc" size="1048576" crc="deadbeef" md5="x" sha1="y"/>
  </game>
</datafile>"#,
        );
        let dat = NoIntroDat::load(&path).unwrap();
        assert_eq!(
            dat.lookup("DEADBEEF").map(|i| i.name.as_str()),
            Some("Super Test World (World)")
        );
        assert!(dat.lookup("00000000").is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(NoIntroDat::load(Path::new("/does/not/exist.dat")).is_err());
    }

    #[test]
    fn clrmamepro_flavour_parses_like_the_xml_one() {
        let path = write_dat(
            r#"clrmamepro (
	name "Nintendo - Super Nintendo Entertainment System"
	description "Nintendo - Super Nintendo Entertainment System"
	version "2026.08.01"
)

game (
	name "'96 Zenkoku Koukou Soccer Senshuken (Japan)"
	region "Japan"
	rom ( name "'96 Zenkoku Koukou Soccer Senshuken (Japan).sfc" size 1572864 crc 05FBB855 md5 3369347F7663B133CE445C1523B959B1F sha1 c0ffee )
)

game (
	name "Bayou Billy (USA)"
	rom ( name "Bayou Billy (USA).sfc" size 262144 crc deadbeef )
)
"#,
        );
        let dat = NoIntroDat::load(&path).unwrap();
        assert_eq!(
            dat.lookup("05fbb855").map(|i| i.name.as_str()),
            Some("'96 Zenkoku Koukou Soccer Senshuken (Japan)")
        );
        assert_eq!(
            dat.lookup("DEADBEEF").map(|i| i.name.as_str()),
            Some("Bayou Billy (USA)")
        );
        // The rom line's own `name "..."` must not overwrite the game's.
        assert_ne!(
            dat.lookup("DEADBEEF").map(|i| i.name.as_str()),
            Some("Bayou Billy (USA).sfc")
        );
        assert!(dat.lookup("00000000").is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn optional_extras_are_read_when_present_and_absent_otherwise() {
        let path = write_dat(
            r#"<?xml version="1.0"?>
<datafile>
  <game name="Rich Game (World)">
    <description>A game with extras</description>
    <year>1994</year>
    <publisher>Acme Software</publisher>
    <category>Platform</category>
    <rom name="Rich Game (World).sfc" size="1048576" crc="AAAAAAAA" md5="x" sha1="y"/>
  </game>
  <game name="Plain Game (World)">
    <rom name="Plain Game (World).sfc" size="1048576" crc="BBBBBBBB" md5="x" sha1="y"/>
  </game>
</datafile>"#,
        );
        let dat = NoIntroDat::load(&path).unwrap();

        let rich = dat.lookup("aaaaaaaa").unwrap();
        assert_eq!(rich.description.as_deref(), Some("A game with extras"));
        assert_eq!(rich.year.as_deref(), Some("1994"));
        assert_eq!(rich.publisher.as_deref(), Some("Acme Software"));
        assert_eq!(rich.category.as_deref(), Some("Platform"));

        let plain = dat.lookup("bbbbbbbb").unwrap();
        assert!(plain.description.is_none());
        assert!(plain.year.is_none());
        assert!(plain.publisher.is_none());
        assert!(plain.category.is_none());

        let _ = fs::remove_file(&path);
    }
}
