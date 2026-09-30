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

/// Game info, keyed two ways: SNES-era lookup by the ROM's headerless CRC32
/// in upper hex (e.g. `"05FBB855"`), and — what PSX discs use — the factory
/// **serial** (`SLUS-00402`), which the Redump/libretro DATs carry per game
/// and which is exactly the identity the disc scan already extracts.
pub struct NoIntroDat {
    by_crc32: HashMap<String, NoIntroGameInfo>,
    by_serial: HashMap<String, NoIntroGameInfo>,
    /// Índice pela **base** do serial: o DAT Redump registra variantes com
    /// sufixo (`SLUS-00923D` demo, `SLUS-00923GH` Greatest Hits), mas o
    /// `SYSTEM.CNF` do disco só carrega `SLUS-00923`. Primeiro registro com
    /// a mesma base vence (variantes com nomes diferentes são raras e
    /// indistinguíveis sem mais metadado).
    by_serial_base: HashMap<String, NoIntroGameInfo>,
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
        let (by_crc32, by_serial) = if text.trim_start().starts_with("clrmamepro") {
            parse_clrmamepro(&text)
        } else {
            let doc = roxmltree::Document::parse(&text)?;
            let mut by_crc32 = HashMap::new();
            let mut by_serial = HashMap::new();
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
                if let Some(serial) = child_text(game, "serial") {
                    insert_preferred(&mut by_serial, normalize_serial(&serial), info.clone());
                }
                for rom in game.children().filter(|n| n.has_tag_name("rom")) {
                    if let Some(crc) = rom.attribute("crc") {
                        by_crc32.insert(crc.to_ascii_uppercase(), info.clone());
                    }
                    if let Some(serial) = rom.attribute("serial") {
                        insert_preferred(&mut by_serial, normalize_serial(serial), info.clone());
                    }
                }
            }
            (by_crc32, by_serial)
        };
        // A base do serial alimenta o índice de variantes — vale para os
        // dois sabores de DAT, que chegam aqui no mesmo mapa.
        let mut by_serial_base: HashMap<String, (usize, NoIntroGameInfo)> = HashMap::new();
        for (serial, info) in &by_serial {
            let Some(base) = base_serial(serial) else {
                continue;
            };
            match by_serial_base.get(base) {
                // Empate de base: vence a entrada menos específica (retail
                // acima de Beta/Demo, menos tags, nome mais curto) e, entre
                // iguais, o serial mais curto — determinístico, diferente da
                // ordem do HashMap.
                Some((len, existing))
                    if entry_rank(&existing.name) <= entry_rank(&info.name)
                        && (entry_rank(&existing.name) != entry_rank(&info.name)
                            || *len <= serial.len()) => {}
                _ => {
                    by_serial_base.insert(base.to_string(), (serial.len(), info.clone()));
                }
            }
        }
        let by_serial_base = by_serial_base
            .into_iter()
            .map(|(k, (_, v))| (k, v))
            .collect();
        log::info!(
            "no-intro DAT: {} entradas por CRC, {} por serial, de {}",
            by_crc32.len(),
            by_serial.len(),
            path.display()
        );
        Ok(Self {
            by_crc32,
            by_serial,
            by_serial_base,
        })
    }

    pub fn lookup(&self, crc32: &str) -> Option<&NoIntroGameInfo> {
        self.by_crc32.get(&crc32.to_ascii_uppercase())
    }

    /// PSX: o serial de fábrica (`SLUS-00402`) — o que o scan do disco já
    /// extrai — casa direto com o `serial` que o DAT Redump carrega; sem
    /// entrada exata, pela base (`SLUS-00923` acha as variantes `…D`/`…GH`).
    pub fn lookup_serial(&self, serial: &str) -> Option<&NoIntroGameInfo> {
        let serial = normalize_serial(serial);
        self.by_serial
            .get(&serial)
            .or_else(|| base_serial(&serial).and_then(|b| self.by_serial_base.get(b)))
    }
}

/// `SLUS-00923D` → `SLUS-00923`: os 10 primeiros caracteres quando o serial
/// normalizado tem o formato canônico (`LLLL-ddddd`) seguido de sufixo de
/// variante. `None` para um serial que nem tem base.
fn base_serial(serial: &str) -> Option<&str> {
    let b = serial.get(..10)?;
    let bytes = b.as_bytes();
    let shaped = bytes.len() == 10
        && bytes[..4].iter().all(u8::is_ascii_uppercase)
        && bytes[4] == b'-'
        && bytes[5..].iter().all(u8::is_ascii_digit);
    shaped.then_some(b)
}

/// `slus_004.02` → `SLUS-00402` — a mesma normalização do scan de discos
/// (`xperience_ra::hash::normalize_serial`), replicada aqui para o domain
/// não depender do crate de RA para uma função de três linhas.
fn normalize_serial(raw: &str) -> String {
    raw.trim()
        .to_ascii_uppercase()
        .replace('_', "-")
        .replace('.', "")
}

/// The quoted value of a `key "value"` line (clrmamepro flavour) — `None`
/// unless the line is exactly that key with a quoted string on it.
fn clrmame_quoted(line: &str, key: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(key)?.trim_start();
    let value = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some(value.to_string())
}

/// O DAT registra variantes sob o MESMO serial (`SCUS-94204` retail e
/// `(Beta)`), e o mesmo serial pode aparecer em vários blocos — o último
/// `insert` vencia, pela ordem do arquivo/HashMap. `insert_preferred` mantém
/// a entrada menos "específica": sem marcador de compilação não-final
/// (Beta/Demo/Proto/Sample/Promo/Debug), menos tags entre parênteses, nome
/// mais curto.
fn entry_rank(name: &str) -> (usize, usize, usize) {
    let lower = name.to_ascii_lowercase();
    const MARKERS: [&str; 7] = [
        "(beta", "(demo", "(proto", "(sample", "(promo", "(debug", "(review",
    ];
    let marked = MARKERS.iter().filter(|m| lower.contains(*m)).count();
    let parens = name.matches('(').count();
    (marked, parens, name.len())
}

fn insert_preferred(
    map: &mut HashMap<String, NoIntroGameInfo>,
    key: String,
    info: NoIntroGameInfo,
) {
    match map.get(&key) {
        Some(existing) if entry_rank(&existing.name) <= entry_rank(&info.name) => {}
        _ => {
            map.insert(key, info);
        }
    }
}

/// Parse clrmamepro text (what `libretro-database`'s `metadat/no-intro/`
/// DATs — and the Redump `Sony - PlayStation` one the PSX app fetches —
/// are): a `clrmamepro (` header, then one `game (` block per title
/// carrying a quoted `name` (plus `serial`/`region` where the DAT has
/// them), each with one `rom (` entry whose `crc XXXXXXXX` token is the CRC
/// lookup key. Only what the lookups need is kept.
fn parse_clrmamepro(
    text: &str,
) -> (
    HashMap<String, NoIntroGameInfo>,
    HashMap<String, NoIntroGameInfo>,
) {
    let mut by_crc32 = HashMap::new();
    let mut by_serial = HashMap::new();
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
                } else if let Some(region) = clrmame_quoted(line, "region") {
                    // A região vira o `category` quando o DAT não traz um —
                    // é o que sobra de metadado para o painel do catálogo.
                    if info.category.is_none() {
                        info.category = Some(region);
                    }
                } else if let Some(serial) = clrmame_quoted(line, "serial") {
                    insert_preferred(&mut by_serial, normalize_serial(&serial), info.clone());
                }
            }
            if let Some(crc_start) = line.find("crc ") {
                let crc: String = line[crc_start + 4..]
                    .trim_start()
                    .chars()
                    .take_while(|c| !c.is_whitespace() && *c != ')')
                    .collect();
                if crc.len() == 8 {
                    insert_preferred(&mut by_crc32, crc.to_ascii_uppercase(), info.clone());
                }
            }
            // Serial dentro do `rom (...)` (o DAT Redump repete lá).
            if in_rom {
                if let Some(serial) = clrmame_quoted(line, "serial") {
                    insert_preferred(&mut by_serial, normalize_serial(&serial), info.clone());
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
    (by_crc32, by_serial)
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

    #[test]
    fn redump_serial_lookup_gives_the_canonical_name() {
        let path = write_dat(
            r#"clrmamepro (
	name "Sony - PlayStation"
)

game (
	name "Tekken 3 (USA)"
	region "USA"
	serial "SLUS-00402"
	rom ( name "Tekken 3 (USA) (Track 1).bin" size 632532768 crc 8131AF42 serial "SLUS-00402" )
)
"#,
        );
        let dat = NoIntroDat::load(&path).unwrap();
        assert_eq!(
            dat.lookup_serial("SLUS-00402").map(|i| i.name.as_str()),
            Some("Tekken 3 (USA)")
        );
        assert_eq!(
            dat.lookup_serial("slus_004.02").map(|i| i.name.as_str()),
            Some("Tekken 3 (USA)")
        );
        assert!(dat.lookup_serial("SCUS-00000").is_none());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn base_serial_matches_discs_the_dat_registers_as_variants() {
        // O caso real: o disco carrega SLUS-00923; o DAT Redump só tem as
        // variantes …D (demo) e …GH (Greatest Hits).
        let path = write_dat(
            r#"clrmamepro (
)

game (
	name "Resident Evil 3 - Nemesis (USA, Canada)"
	region "USA"
	serial "SLUS-00923D"
	rom ( name "a.bin" size 1 crc 00000001 )
)

game (
	name "Resident Evil 3 - Nemesis (USA, Canada) (GH)"
	region "USA"
	serial "SLUS-00923GH"
	rom ( name "b.bin" size 1 crc 00000002 )
)
"#,
        );
        let dat = NoIntroDat::load(&path).unwrap();
        assert_eq!(
            dat.lookup_serial("SLUS-00923").map(|i| i.name.as_str()),
            Some("Resident Evil 3 - Nemesis (USA, Canada)")
        );
        // Exato continua vencendo a base.
        assert_eq!(
            dat.lookup_serial("SLUS-00923GH").map(|i| i.name.as_str()),
            Some("Resident Evil 3 - Nemesis (USA, Canada) (GH)")
        );
        assert_eq!(base_serial("SLUS-00923"), Some("SLUS-00923"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn retail_wins_when_the_same_serial_has_a_beta_entry() {
        // O caso real do Spawn: retail e Beta compartilham o serial
        // SCUS-94204 — a estante nunca pode virar "(Beta)".
        let path = write_dat(
            r#"clrmamepro (
)

game (
	name "Spawn - The Eternal (USA) (Beta)"
	region "USA"
	serial "SCUS-94204"
	rom ( name "beta.bin" size 1 crc 00000001 )
)

game (
	name "Spawn - The Eternal (USA)"
	region "USA"
	serial "SCUS-94204"
	rom ( name "retail.bin" size 1 crc 00000002 )
)
"#,
        );
        let dat = NoIntroDat::load(&path).unwrap();
        assert_eq!(
            dat.lookup_serial("SCUS-94204").map(|i| i.name.as_str()),
            Some("Spawn - The Eternal (USA)")
        );
        let _ = fs::remove_file(&path);
    }
}
