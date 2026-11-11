//! Runner: paths (extraído de `runner/mod.rs`, sem mudança de comportamento).

use super::*;

/// A path-hostile character in a game title, replaced with `_` so it can't
/// escape its parent directory or fail to create — shared by every per-game
/// folder (`saves/<title>/`, `notes/<title>/`), and by `rom_rename` (plan
/// revision) when it turns a No-Intro name into a ROM file name.
pub(crate) fn sanitize_dir_name(title: &str) -> String {
    let safe: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    // Pontos e espaços no fim somem (o Windows os descarta, e "x." viraria
    // outra pasta que "x"); um nome feito só de pontos ("." / "..") apontaria
    // para a pasta atual ou a de cima — vira "___" como o nome vazio. Teto
    // de 200 bytes, abaixo do limite de nome de arquivo dos sistemas comuns.
    let mut safe = safe.trim_end_matches(['.', ' ']).to_string();
    if safe.len() > 200 {
        let mut cut = 200;
        while !safe.is_char_boundary(cut) {
            cut -= 1;
        }
        safe.truncate(cut);
        safe = safe.trim_end_matches(['.', ' ']).to_string();
    }
    if safe.is_empty() {
        "___".to_string()
    } else {
        safe
    }
}

/// `save_dir/<game title>/` — one folder per game, named for a human
/// browsing it rather than the ROM hash (plan revision — mirrors
/// `note_dir`: readability over rename-proofing nobody asked for here).
pub(super) fn game_dir(save_dir: &Path, title: &str) -> PathBuf {
    save_dir.join(sanitize_dir_name(title))
}

/// `save_dir/<title>/<slot>.state` — plain slot number, no hash prefix
/// (plan revision).
pub(super) fn state_file(save_dir: &Path, title: &str, slot: u8) -> PathBuf {
    game_dir(save_dir, title).join(format!("{slot}.state"))
}

/// `save_dir/<title>/<slot>.disc` — the disc file name the state was made
/// on (plano §3.1: um state amarra o disco em que foi salvo; restaurar com
/// outro disco no drive é lixo).
pub(super) fn disc_stamp_path(save_dir: &Path, title: &str, slot: u8) -> PathBuf {
    game_dir(save_dir, title).join(format!("{slot}.disc"))
}

/// The disc's file name, for the state stamps (`jogo.chd`).
pub(super) fn disc_name(rom: &Path) -> String {
    rom.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `save_dir/<title>/sram.srm` — battery SRAM, one file per game (plan
/// revision — used to be `<hash>.srm` directly under `save_dir`).
/// `save_dir/<title>/cheats.txt` — plain text either way, so the extension
/// might as well say so (plan revision — used to be `<hash>.cheats`).
pub(super) fn cheat_state_path(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("cheats.txt")
}

/// `save_dir/<title>/playtime.txt` — total seconds ever spent powered on,
/// plain text like `cheats.txt` (plan revision: "mostrar tempo total de
/// jogo do game").
pub(super) fn playtime_path(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("playtime.txt")
}

/// The per-game folder name `run_game` itself uses — the ROM's file stem,
/// not the shelf's `CatalogEntry::title()` (which prefers a No-Intro/
/// internal name when one exists). Exposed so the shelf can read
/// `total_playtime_secs` for the exact folder a session actually wrote to,
/// instead of guessing at a name that might not match.
pub fn rom_title(rom_path: &Path) -> String {
    rom_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "???".to_string())
}

/// Total time this game has spent powered on, across every session ever
/// played — `0` for a game never played (no file yet) or a corrupt one
/// (never worth failing the panel over one bad number).
pub fn total_playtime_secs(save_dir: &Path, title: &str) -> u64 {
    fs::read_to_string(playtime_path(save_dir, title))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Add this session's powered-on seconds to the running total and save it
/// back — best-effort, same as the other per-game sidecars; a `0` (the
/// console was never turned on this session) skips the write entirely.
pub(super) fn add_playtime(save_dir: &Path, title: &str, secs: u64) {
    if secs == 0 {
        return;
    }
    let total = total_playtime_secs(save_dir, title) + secs;
    let _ = fs::create_dir_all(game_dir(save_dir, title));
    let _ =
        polystnx_domain::fsutil::write_atomic(playtime_path(save_dir, title), total.to_string());
}

/// Rows for the "Salvar" modal (plan revision — replaces the old slot
/// cycler): one per save-state slot, always clickable (saving overwrites
/// whatever was there), labelled "(vazio)" for slots with nothing in them
/// yet purely as information.
pub(super) fn save_slot_rows(save_dir: &Path, title: &str) -> Vec<(String, bool)> {
    (0..SLOTS)
        .map(|s| {
            let filled = state_file(save_dir, title, s).exists();
            let label = if filled {
                format!("Slot {s}")
            } else {
                format!("Slot {s} (vazio)")
            };
            (label, true)
        })
        .collect()
}

/// Rows for the "Carregar" modal, mirroring `save_slot_rows` — an empty
/// slot is disabled instead of just noted, since there's nothing to load.
pub(super) fn load_slot_rows(save_dir: &Path, title: &str) -> Vec<(String, bool)> {
    (0..SLOTS)
        .map(|s| {
            let filled = state_file(save_dir, title, s).exists();
            let label = if filled {
                format!("Slot {s}")
            } else {
                format!("Slot {s} (vazio)")
            };
            (label, filled)
        })
        .collect()
}
