//! Rename disc files to their canonical name (plan revision, herdada de
//! SNES: "renomear automaticamente no padrao no-intro"), a settings-screen
//! action. Para PSX o nome canônico vem da **tabela embutida por serial**
//! (`xperience_domain::psx`) — a mesma que dá título à estante; um jogo que
//! a tabela não conhece fica com o nome que o usuário deu. Best-effort
//! throughout: a single file that can't be renamed (permissions, a
//! same-named file already there) is skipped and logged, not fatal.
//!
//! Every per-game folder (`saves/<title>/`, `notes/<title>/`) and local art
//! (`assets/{cover,logo,disc}/<title>.*`) is keyed by the file stem — so
//! renaming a disc out from under them would orphan any save state, cheat,
//! note or playtime already recorded, and any art already dropped in. Each
//! rename moves all of those along with it.

use std::fs;
use std::path::Path;

use xperience_domain::{library, psx};

use crate::runner::sanitize_dir_name;

/// One disc actually renamed — the old and new title, for the settings
/// screen's summary line.
pub struct Renamed {
    pub old: String,
    pub new: String,
}

pub enum RenameOutcome {
    Renamed(Vec<Renamed>),
    /// O DAT que este fluxo usava no irmão de SNES não participa mais; o
    /// variante fica para a tela de configurações não mudar de forma.
    NoDat,
    /// Nada em `roms/` precisou de nome novo (ou a tabela ainda não conhece
    /// nada — ela nasce vazia até `scripts/gen_psx_data.py` rodar).
    NothingToDo,
}

/// Scan `roms_dir`, rename every disc whose serial matches a table entry
/// under a different name, moving its save/notes folders and local art to
/// match.
pub fn rename_to_nointro(
    roms_dir: &Path,
    _dat_path: &Path,
    saves_dir: &Path,
    notes_dir: &Path,
    assets_dir: &Path,
) -> RenameOutcome {
    let Ok(scanned) = library::scan(roms_dir) else {
        return RenameOutcome::NothingToDo;
    };

    let mut renamed = Vec::new();
    for disc in scanned {
        let Some(info) = psx::lookup(&disc.id.serial) else {
            continue;
        };
        let Some(old_stem) = disc.path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let new_stem = sanitize_dir_name(&info.title);
        if old_stem == new_stem {
            continue; // already named canonically
        }
        let ext = disc
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("chd");
        let new_path = match disc.path.parent() {
            Some(dir) => dir.join(format!("{new_stem}.{ext}")),
            None => continue,
        };
        if new_path.exists() {
            log::warn!(
                "rom rename: {} already exists, leaving {old_stem} alone",
                new_path.display()
            );
            continue;
        }
        if let Err(e) = fs::rename(&disc.path, &new_path) {
            log::warn!("rom rename: {old_stem} -> {new_stem}: {e}");
            continue;
        }

        // Carry the game's own progress along — same folder/file convention
        // `runner.rs`/`shelf.rs` already use to find it.
        let old_dir_name = sanitize_dir_name(old_stem);
        move_entry(saves_dir, &old_dir_name, &new_stem, &[]);
        move_entry(notes_dir, &old_dir_name, &new_stem, &[]);
        for sub in ["cover", "logo", "cartridge", "disc"] {
            move_entry(
                &assets_dir.join(sub),
                old_stem,
                &new_stem,
                &["png", "jpg", "jpeg"],
            );
        }

        renamed.push(Renamed {
            old: old_stem.to_string(),
            new: new_stem,
        });
    }

    if renamed.is_empty() {
        RenameOutcome::NothingToDo
    } else {
        RenameOutcome::Renamed(renamed)
    }
}

/// Move whatever's under `dir` for `old_key` to `new_key` — a same-named
/// subfolder when `extensions` is empty (saves/notes), or the first file
/// matching one of `extensions` otherwise (local art). A no-op if nothing's
/// there; best-effort if the move itself fails (logged, not propagated —
/// the disc rename it's attached to has already happened either way).
fn move_entry(dir: &Path, old_key: &str, new_key: &str, extensions: &[&str]) {
    let (old, new) = if extensions.is_empty() {
        (dir.join(old_key), dir.join(new_key))
    } else {
        let Some(old) = extensions
            .iter()
            .map(|ext| dir.join(format!("{old_key}.{ext}")))
            .find(|p| p.is_file())
        else {
            return;
        };
        let ext = old
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_string();
        (old, dir.join(format!("{new_key}.{ext}")))
    };
    if !old.exists() || new.exists() {
        return;
    }
    if let Err(e) = fs::rename(&old, &new) {
        log::warn!(
            "rom rename: moving {} -> {}: {e}",
            old.display(),
            new.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("psx-rom-rename-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Um `.chd` que não é um disco de verdade: a varredura não o identifica
    /// e o rename não tem nada a fazer — o fluxo inteiro é não-fatal.
    #[test]
    fn unidentifiable_discs_are_nothing_to_do() {
        let root = scratch_dir("fake-chd");
        let roms_dir = root.join("roms");
        fs::create_dir_all(&roms_dir).unwrap();
        fs::write(roms_dir.join("Game (USA).chd"), b"nao e um chd").unwrap();

        let outcome = rename_to_nointro(&roms_dir, &root.join("qualquer.dat"), &root, &root, &root);
        assert!(matches!(outcome, RenameOutcome::NothingToDo));
        assert!(roms_dir.join("Game (USA).chd").is_file());

        fs::remove_dir_all(&root).ok();
    }

    /// Pasta vazia, nada a fazer — e o `NoDat` herdado nunca dispara mais
    /// (a tabela é embutida, não um arquivo que possa faltar).
    #[test]
    fn empty_roms_is_nothing_to_do() {
        let root = scratch_dir("empty");
        let roms_dir = root.join("roms");
        fs::create_dir_all(&roms_dir).unwrap();
        let outcome = rename_to_nointro(&roms_dir, &root, &root, &root, &root);
        assert!(matches!(outcome, RenameOutcome::NothingToDo));
        fs::remove_dir_all(&root).ok();
    }
}
