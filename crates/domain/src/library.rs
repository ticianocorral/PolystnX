//! Folder scan — the front half of plan §2 ("Varredura de roms/ aceitando
//! só .chd, + m3u"). Recursive, one pass, no watching. Discos são grandes:
//! nada de ler bytes para hash — a identidade é o serial, lido de alguns
//! setores do CHD.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::disc::{DiscError, DiscId};

/// Discos que o app aceita. Só `.chd` (plano §2: "CHD como único formato").
pub const DISC_EXTS: [&str; 1] = ["chd"];

/// Lista de discos de um jogo multi-disco: um `.m3u` é UM jogo com N discos.
pub const PLAYLIST_EXTS: [&str; 1] = ["m3u"];

/// Formatos de PSX que existem por aí mas o app não aceita — a varredura
/// avisa (com a ponte do `chdman`) em vez de engolir em silêncio.
pub const UNSUPPORTED_EXTS: [&str; 7] = ["cue", "bin", "iso", "img", "pbp", "ecm", "mds"];

/// Um jogo encontrado no disco: `.chd` solto ou `.m3u` (um jogo, N discos),
/// já identificado pelo serial.
#[derive(Debug, Clone)]
pub struct ScannedDisc {
    pub path: PathBuf,
    pub id: DiscId,
    pub file_size: u64,
    /// mtime, if the OS gave us one — used to order "recently added".
    pub modified: Option<SystemTime>,
}

/// Recursively walk `dir` for discs and identify each. Unreadable files are
/// logged and skipped, not fatal.
pub fn scan(dir: &Path) -> std::io::Result<Vec<ScannedDisc>> {
    let mut cache = DiscCache::default();
    scan_with(dir, &mut cache)
}

/// Same scan, consulting/filling `cache`: a file whose path, size and mtime
/// all match the cache is *not* re-opened, so a warm boot with an unchanged
/// `roms/` folder costs nothing. The catalog persists the cache next to its
/// own sidecar.
pub fn scan_with(dir: &Path, cache: &mut DiscCache) -> std::io::Result<Vec<ScannedDisc>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("scan: skipping {}: {e}", d.display());
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue; // hidden files / dirs
            }
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_dir() {
                stack.push(path);
                continue;
            }
            // Discos moram em outro volume e chegam aqui por symlink — o
            // metadata do DirEntry não segue o link; o do fs segue.
            let meta = std::fs::metadata(&path).ok();
            if !meta.as_ref().map(|m| m.is_file()).unwrap_or(false) {
                continue;
            }
            let Some(ext) = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
            else {
                continue;
            };
            if UNSUPPORTED_EXTS.contains(&ext.as_str()) {
                log::warn!(
                    "scan: {}: .{ext} não é aceito — converta para .chd (chdman createcd -i arquivo.cue -o arquivo.chd)",
                    path.display()
                );
                continue;
            }
            let is_game =
                DISC_EXTS.contains(&ext.as_str()) || PLAYLIST_EXTS.contains(&ext.as_str());
            if !is_game {
                continue;
            }
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta.as_ref().and_then(|m| m.modified().ok());
            let id = match cache.lookup(&path, size, modified) {
                Some(id) => id,
                None => match DiscId::from_path(&path) {
                    Ok(id) => {
                        cache.remember(&path, size, modified, &id);
                        id
                    }
                    Err(DiscError::Read(e)) => {
                        log::warn!("scan: {}: {e}", path.display());
                        continue;
                    }
                },
            };
            out.push(ScannedDisc {
                file_size: size,
                modified,
                path,
                id,
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Path-keyed disc ids, persisted next to the catalog's own sidecar: a file
/// whose path, size and mtime all match is *not* re-opened on the next boot.
#[derive(Serialize, Deserialize, Default)]
pub struct DiscCache {
    entries: HashMap<String, CachedDisc>,
}

#[derive(Serialize, Deserialize)]
struct CachedDisc {
    size: u64,
    mtime_secs: i64,
    mtime_nanos: u32,
    serial: String,
    discs: usize,
}

impl DiscCache {
    pub fn load(path: &Path) -> Self {
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) {
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = fs::write(path, bytes);
        }
    }

    /// The cached id for `path`, only if size and mtime still match.
    fn lookup(&self, path: &Path, size: u64, modified: Option<SystemTime>) -> Option<DiscId> {
        let (secs, nanos) = stamp(modified)?;
        let c = self.entries.get(path.to_string_lossy().as_ref())?;
        if c.size != size || c.mtime_secs != secs || c.mtime_nanos != nanos {
            return None;
        }
        Some(DiscId {
            serial: c.serial.clone(),
            discs: c.discs,
        })
    }

    fn remember(&mut self, path: &Path, size: u64, modified: Option<SystemTime>, id: &DiscId) {
        let (mtime_secs, mtime_nanos) = stamp(modified).unwrap_or((0, 0));
        self.entries.insert(
            path.to_string_lossy().into_owned(),
            CachedDisc {
                size,
                mtime_secs,
                mtime_nanos,
                serial: id.serial.clone(),
                discs: id.discs,
            },
        );
    }
}

/// `(secs, nanos)` since the epoch for an mtime; `None` when the OS gave no
/// timestamp or it predates the epoch (those files just always re-open).
fn stamp(modified: Option<SystemTime>) -> Option<(i64, u32)> {
    let d = modified?.duration_since(UNIX_EPOCH).ok()?;
    Some((d.as_secs() as i64, d.subsec_nanos()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// O serial lido de um CHD real não é reproduzível em teste unitário
    /// (precisa de um CHD de verdade — provado em `xperience-ra`'s tests e
    /// no `docs/fase-2.md`); o que é reproduzível aqui é o cache.
    #[test]
    fn disc_cache_roundtrip_and_mtime_invalidation() {
        let dir = std::env::temp_dir().join(format!("disccache-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("Game (USA).chd");
        std::fs::write(&path, b"nao-e-um-chd-de-verdade").unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        let modified = meta.modified().ok();
        let size = meta.len();
        let id = DiscId {
            serial: "SLUS-00402".to_string(),
            discs: 1,
        };

        let mut cache = DiscCache::default();
        cache.remember(&path, size, modified, &id);

        let hit = cache.lookup(&path, size, modified).expect("cache hit");
        assert_eq!(hit.serial, "SLUS-00402");
        assert_eq!(hit.discs, 1);

        let later = modified.unwrap() + std::time::Duration::from_secs(5);
        assert!(cache.lookup(&path, size, Some(later)).is_none());

        let file = dir.join("disccache.json");
        cache.save(&file);
        let reloaded = DiscCache::load(&file);
        assert!(reloaded.lookup(&path, size, modified).is_some());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Um `.cue` solto não entra na estante — e o aviso tem a ponte do
    /// chdman (o scan o registra, não engole em silêncio).
    #[test]
    fn scan_skips_unsupported_formats_with_a_warning() {
        let dir = std::env::temp_dir().join(format!("scan-unsupported-{}", std::process::id()));
        let roms = dir.join("roms");
        std::fs::create_dir_all(&roms).unwrap();
        std::fs::write(roms.join("Game.cue"), b"FILE \"g.bin\" BINARY\n").unwrap();
        std::fs::write(roms.join("leia-me.txt"), b"txt nope").unwrap();
        let scanned = scan(&roms).unwrap();
        assert!(scanned.is_empty(), "nem cue nem txt entram");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
