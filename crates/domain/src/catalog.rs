//! The catalogue: a folder scan (plano §2) plus a small JSON sidecar for
//! what a scan alone can't know — when a disc was first seen and how many
//! times it's been played. No database: the app is portable, everything it
//! needs lives in plain files next to it (`polystnx_app::dirs`).

use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::library;
use crate::nointro::NoIntroDat;

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("scanning {path}: {source}")]
    Scan {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

type Result<T> = std::result::Result<T, CatalogError>;

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// A ROM as tracked on disk, plus the little state that persists across
/// runs (everything else is recomputed by scanning fresh each `Catalog::open`).
#[derive(Debug, Clone)]
pub struct RomRow {
    pub sha1: String,
    pub crc32: String,
    pub path: String,
    pub size: u64,
    pub internal_name: Option<String>,
    /// Canonical title from the No-Intro DAT, if one was loaded and matched
    /// by CRC32 (plan §4.1).
    pub nointro_name: Option<String>,
    /// The player's own title override (plan revision: hacks/translations
    /// whose ROM header carries the dumper's stamp — "SNESFOREVER.COM.BR" —
    /// instead of a game name). Lives in the sidecar keyed by SHA1, so it
    /// survives file renames and wins over every automatic source.
    pub custom_title: Option<String>,
    /// Extra facts for the shelf panel — label/value pairs (e.g. "ano"/
    /// "1994"). A loaded No-Intro DAT's own fields win when present; the
    /// year/publisher bundled from TOSEC (plan revision) fill in the gaps,
    /// so this can be non-empty even with no DAT loaded at all. Empty only
    /// when neither source has anything for this ROM's CRC32.
    pub nointro_extra: Vec<(String, String)>,
    pub added_at: i64,
    pub last_played_at: Option<i64>,
    pub play_count: u32,
    /// The player's own "favorito" marker (plan revision: "adicionar
    /// marcador de favorito nos jogos; mostrar em uma categoria acima dos
    /// ultimos jogados") — persisted in the sidecar, shown as a strip above
    /// the recent one and a small gold corner dot on the tiles.
    pub favorite: bool,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub rom: RomRow,
}

impl RomRow {
    /// The game's display title — the player's override when set, the
    /// No-Intro name when the DAT resolved one, the internal header name
    /// otherwise, the file stem as a last resort. `Cow` so the common cases
    /// borrow instead of allocating a `String` per call (this is on the
    /// shelf's per-frame sort/filter paths).
    pub fn title(&self) -> Cow<'_, str> {
        if let Some(t) = &self.custom_title {
            return Cow::Borrowed(t);
        }
        if let Some(n) = &self.nointro_name {
            return Cow::Borrowed(n);
        }
        if let Some(n) = &self.internal_name {
            return Cow::Borrowed(n);
        }
        Cow::Owned(
            Path::new(&self.path)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.path.clone()),
        )
    }
}

impl CatalogEntry {
    /// Best display name: No-Intro canonical, else the SNES header's
    /// internal title, else the file stem.
    pub fn title(&self) -> Cow<'_, str> {
        self.rom.title()
    }
}

/// Row order for the shelf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Plan §3.1: last-played first, then most-recently-added.
    Shelf,
    Name,
}

/// What actually needs to survive between runs, keyed by sha1 in the JSON
/// sidecar.
#[derive(Serialize, Deserialize, Default, Clone)]
struct Persisted {
    added_at: i64,
    last_played_at: Option<i64>,
    play_count: u32,
    /// `#[serde(default)]` so sidecars written before favorites existed
    /// load as "nothing is a favorite" instead of failing the whole store.
    #[serde(default)]
    favorite: bool,
    /// The player's own display title (hacks whose ROM header says e.g.
    /// "SNESFOREVER.COM.BR") — absent for every game that never got one.
    #[serde(default)]
    title: Option<String>,
}

pub struct Catalog {
    store_path: PathBuf,
    entries: RefCell<Vec<RomRow>>,
}

impl Catalog {
    /// Scan `roms_dir`, merge in whatever `store_path` (the JSON sidecar)
    /// remembers about each game by serial. Scans once, up front — a
    /// corrected title is there from the shelf's first frame. `dat` is the
    /// optional No-Intro/Redump DAT: quando indexa o serial do disco, o
    /// nome canônico (com a tag de região) vira o título da estante.
    /// Scan sem DAT — o nome canônico fica por conta da tabela embutida.
    pub fn open(roms_dir: &Path, store_path: &Path) -> Result<Self> {
        Self::open_with_dat(roms_dir, store_path, None)
    }

    pub fn open_with_dat(
        roms_dir: &Path,
        store_path: &Path,
        dat: Option<&NoIntroDat>,
    ) -> Result<Self> {
        // O cache de identificação ride next to this sidecar: a warm boot
        // with an unchanged `roms/` folder skips re-opening every disc.
        let hashcache_path = store_path.with_file_name("hashcache.json");
        let mut hash_cache = library::DiscCache::load(&hashcache_path);
        let scanned =
            library::scan_with(roms_dir, &mut hash_cache).map_err(|source| CatalogError::Scan {
                path: roms_dir.display().to_string(),
                source,
            })?;
        let persisted = load_store(store_path)?;
        let now = now();
        let mut entries: Vec<RomRow> = scanned
            .into_iter()
            .map(|r| {
                // A chave estável do jogo é o **serial** (`RomRow.sha1` — o
                // nome do campo é do tempo de cartucho; para disco, o serial
                // é o identificador que nunca muda, e é o que sustenta
                // playtime, favoritos, notas e o cache de RA).
                let key = r.id.serial.clone();
                let p = persisted.get(&key).cloned().unwrap_or(Persisted {
                    added_at: r
                        .modified
                        .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(now),
                    last_played_at: None,
                    play_count: 0,
                    favorite: false,
                    title: None,
                });
                // Título: primeiro o DAT (No-Intro/Redump, pelo serial — o
                // nome canônico "Tekken 3 (USA)"); sem DAT, a tabela PSX
                // embutida, também pelo serial; sem ambos, o nome do arquivo.
                let psx_hit = crate::psx::lookup(&r.id.serial);
                let dat_hit = dat.and_then(|d| d.lookup_serial(&r.id.serial));
                let nointro_name = dat_hit
                    .map(|p| p.name.clone())
                    .or_else(|| psx_hit.as_ref().map(|p| p.title.clone()));
                let mut nointro_extra = Vec::new();
                if let Some(region) = dat_hit
                    .and_then(|p| p.category.clone())
                    .or_else(|| psx_hit.as_ref().and_then(|p| p.region.clone()))
                {
                    nointro_extra.push(("região".to_string(), region));
                }
                RomRow {
                    sha1: key,
                    crc32: String::new(),
                    path: r.path.to_string_lossy().into_owned(),
                    size: r.file_size,
                    internal_name: Some(r.id.serial),
                    nointro_name,
                    custom_title: p.title,
                    nointro_extra,
                    added_at: p.added_at,
                    last_played_at: p.last_played_at,
                    play_count: p.play_count,
                    favorite: p.favorite,
                }
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        hash_cache.save(&hashcache_path);
        Ok(Self {
            store_path: store_path.to_path_buf(),
            entries: RefCell::new(entries),
        })
    }

    /// Flip a game's favorite marker (the shelf panel's
    /// "favoritar"/"remover favorito" button) and persist.
    pub fn set_favorite(&self, sha1: &str, favorite: bool) -> Result<()> {
        {
            let mut entries = self.entries.borrow_mut();
            if let Some(row) = entries.iter_mut().find(|r| r.sha1 == sha1) {
                row.favorite = favorite;
            }
        }
        self.save()
    }

    pub fn mark_played(&self, sha1: &str) -> Result<()> {
        {
            let mut entries = self.entries.borrow_mut();
            if let Some(row) = entries.iter_mut().find(|r| r.sha1 == sha1) {
                row.last_played_at = Some(now());
                row.play_count += 1;
            }
        }
        self.save()
    }

    pub fn list(&self, order: Order) -> Result<Vec<CatalogEntry>> {
        let mut rows: Vec<RomRow> = self.entries.borrow().clone();
        match order {
            Order::Shelf => rows.sort_by_key(|r| {
                (
                    r.last_played_at.is_none(),
                    Reverse(r.last_played_at.unwrap_or(0)),
                    Reverse(r.added_at),
                )
            }),
            Order::Name => rows.sort_by_cached_key(|r| r.title().to_lowercase()),
        }
        Ok(rows.into_iter().map(|rom| CatalogEntry { rom }).collect())
    }

    pub fn counts(&self) -> Result<usize> {
        Ok(self.entries.borrow().len())
    }

    fn save(&self) -> Result<()> {
        let map: HashMap<String, Persisted> = self
            .entries
            .borrow()
            .iter()
            .map(|r| {
                (
                    r.sha1.clone(),
                    Persisted {
                        added_at: r.added_at,
                        last_played_at: r.last_played_at,
                        play_count: r.play_count,
                        favorite: r.favorite,
                        title: r.custom_title.clone(),
                    },
                )
            })
            .collect();
        let text = serde_json::to_string_pretty(&map).map_err(|source| CatalogError::Json {
            path: self.store_path.display().to_string(),
            source,
        })?;
        std::fs::write(&self.store_path, text).map_err(|source| CatalogError::Io {
            path: self.store_path.display().to_string(),
            source,
        })
    }
}

/// A missing or corrupted sidecar just means "nothing remembered yet" — not
/// worth a hard failure of the whole app over play-count history.
fn load_store(path: &Path) -> Result<HashMap<String, Persisted>> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(path).map_err(|source| CatalogError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(sha1: &str, name: &str, added_at: i64) -> RomRow {
        RomRow {
            sha1: sha1.to_string(),
            crc32: "AAAAAAAA".to_string(),
            path: format!("/roms/{name}.sfc"),
            size: 0x8000,
            internal_name: Some(name.to_uppercase()),
            nointro_name: None,
            custom_title: None,
            nointro_extra: Vec::new(),
            added_at,
            last_played_at: None,
            play_count: 0,
            favorite: false,
        }
    }

    #[test]
    fn title_prefers_the_player_override_over_everything() {
        let mut r = row("abc", "World Cup 2026", 0);
        assert_eq!(r.title(), "WORLD CUP 2026"); // cabeçalho interno
        r.nointro_name = Some("Canonical (World) (Rev 1)".into());
        assert_eq!(r.title(), "Canonical (World) (Rev 1)");
        // O override do jogador ganha até do DAT.
        r.custom_title = Some("ISS World Cup 2026".into());
        assert_eq!(r.title(), "ISS World Cup 2026");
    }

    fn fake_catalog(rows: Vec<RomRow>) -> Catalog {
        let store_path = std::env::temp_dir().join(format!(
            "polystnx-catalog-test-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&store_path);
        Catalog {
            store_path,
            entries: RefCell::new(rows),
        }
    }

    #[test]
    fn shelf_order_puts_played_then_recent() {
        let cat = fake_catalog(vec![
            row("old", "a", 100),
            row("new", "b", 200),
            row("played", "c", 50),
        ]);
        cat.mark_played("played").unwrap();

        let ids: Vec<_> = cat
            .list(Order::Shelf)
            .unwrap()
            .into_iter()
            .map(|e| e.rom.sha1)
            .collect();
        assert_eq!(ids, ["played", "new", "old"]);
        let _ = std::fs::remove_file(&cat.store_path);
    }

    #[test]
    fn title_prefers_nointro_then_internal_then_file_stem() {
        let mut r = row("a", "internal", 1);
        let entry = CatalogEntry { rom: r.clone() };
        assert_eq!(entry.title(), "INTERNAL");

        r.nointro_name = Some("Canonical Name".to_string());
        let entry = CatalogEntry { rom: r.clone() };
        assert_eq!(entry.title(), "Canonical Name");

        r.internal_name = None;
        r.nointro_name = None;
        let entry = CatalogEntry { rom: r };
        assert_eq!(entry.title(), "internal"); // falls back to the file stem
    }

    #[test]
    fn mark_played_persists_in_the_sidecar() {
        // A persistência não depende de scan (que exigiria um CHD real):
        // marca num catálogo em memória e confere o sidecar gravado.
        let cat = fake_catalog(vec![row("SLUS-00402", "tekken3", 0)]);
        cat.mark_played("SLUS-00402").unwrap();
        let store = load_store(&cat.store_path).unwrap();
        assert_eq!(store.get("SLUS-00402").unwrap().play_count, 1);
        assert!(store.get("SLUS-00402").unwrap().last_played_at.is_some());
        let _ = std::fs::remove_file(&cat.store_path);
    }

    #[test]
    fn favorite_persists_in_the_sidecar() {
        let cat = fake_catalog(vec![row("SLUS-00402", "tekken3", 0)]);
        assert!(!cat.list(Order::Name).unwrap()[0].rom.favorite);
        cat.set_favorite("SLUS-00402", true).unwrap();
        let store = load_store(&cat.store_path).unwrap();
        assert!(store.get("SLUS-00402").unwrap().favorite);
        let _ = std::fs::remove_file(&cat.store_path);
    }

    /// O título de PSX: tabela embutida pelo serial quando conhece; serial
    /// (internal_name) e nome de arquivo como fallbacks.
    #[test]
    fn disc_title_falls_back_serial_then_file_stem() {
        let mut r = row("SLUS-00402", "tekken3", 0);
        r.internal_name = Some("SLUS-00402".to_string()); // o serial no disco
        let entry = CatalogEntry { rom: r.clone() };
        assert_eq!(entry.title(), "SLUS-00402"); // internal_name = serial
        r.nointro_name = Some("Tekken 3".to_string());
        assert_eq!(CatalogEntry { rom: r.clone() }.title(), "Tekken 3");
        r.internal_name = None;
        r.nointro_name = None;
        assert_eq!(CatalogEntry { rom: r }.title(), "tekken3");
    }
}
