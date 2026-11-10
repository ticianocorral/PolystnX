//! Runner: cheats (extraído de `runner/mod.rs`, sem mudança de comportamento).

use super::*;

/// One `0`/`1` per line, in the curated list's order. Missing/short/garbled
/// files just mean "start with everything off" — nothing to migrate.
pub(super) fn load_cheat_state(path: &Path, len: usize) -> Vec<bool> {
    let mut state = vec![false; len];
    if let Ok(text) = fs::read_to_string(path) {
        for (slot, line) in state.iter_mut().zip(text.lines()) {
            *slot = line.trim() == "1";
        }
    }
    state
}

pub(super) fn save_cheat_state(path: &Path, state: &[bool]) {
    let text: String = state
        .iter()
        .map(|&on| if on { "1\n" } else { "0\n" })
        .collect();
    if let Err(e) = polystnx_domain::fsutil::write_atomic(path, text) {
        log::warn!("cheat state flush failed: {e}");
    }
}

/// `(description, on)` pairs for the panel — cheap enough to rebuild on every
/// toggle/navigate, there are only ever a handful.
pub(super) fn cheat_rows(
    defs: &[polystnx_domain::CheatDef],
    state: &[bool],
) -> Vec<(String, bool)> {
    defs.iter()
        .zip(state)
        .map(|(d, &on)| (d.desc.to_string(), on))
        .collect()
}

/// `cheat_rows`' pairs, formatted for the Cheats modal's row grid (plan
/// revision — its own menu, not a page in the notebook): every row stays
/// `enabled: true` (toggling is always available, unlike a save/load/print
/// slot that can be dimmed), with an `[x]`/`[ ]` mark standing in for the
/// on/off state a modal row otherwise has no way to show.
pub(super) fn cheat_modal_rows(cheats: &[(String, bool)]) -> Vec<(String, bool)> {
    cheats
        .iter()
        .map(|(desc, on)| (format!("{}{desc}", if *on { "[x] " } else { "[ ] " }), true))
        .collect()
}
