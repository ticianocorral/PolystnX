//! Runner: notes (extraído de `runner/mod.rs`, sem mudança de comportamento).

use super::*;

/// Fixed note slots per game (plan revision, replacing an open-ended
/// timestamped list): a screenshot into the notebook always lands in one of
/// these, chosen by the player, same shape as the save-state slots.
pub(super) const NOTE_SLOTS: u8 = 15;
/// Max characters a slot's caption may hold — short, it's a label, not a
/// page (contrast `NOTE_CHAR_LIMIT` for the free-text notebook page).
pub(super) const SLOT_NAME_LIMIT: usize = 40;

/// Which of three things the app's one text editor is currently editing
/// (plan revision) — they share all the input-polling plumbing, just write
/// to a different place, have a different character limit, and (for
/// `PrintName`) render in the modal instead of the notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoteEdit {
    None,
    /// Editing the notebook's currently-shown text-note slot (plan
    /// revision — used to always append a fresh, blank page to an
    /// unbounded, unreadable log; now edits whichever of the 15 slots the
    /// left page is showing, pre-filled with its saved content). Saving an
    /// empty draft deletes the slot instead of writing one.
    Text,
    /// Renaming the notebook's currently-shown print slot's caption (from
    /// "Nomear print" on the notebook's right page).
    SlotName,
    /// Naming the print just captured into this slot (plan revision — the
    /// Printscreen modal's second step): unlike `SlotName`, committing this
    /// one also writes `runner::print_capture`'s image to disk for the first
    /// time — the capture and the name land together.
    PrintName(u8),
    /// Renomear um memory card (o alvo vive em `card_rename`) — commit
    /// renomeia o `.mcr` na biblioteca.
    CardRename,
    /// Typing the Cheats modal's search filter (plan revision — the
    /// libretro-database expansion made some games' lists long enough that
    /// finding one by eye/scroll alone stopped being practical). Unlike
    /// every other `NoteEdit`, committing this one writes nothing to
    /// disk — it just updates `Cabinet`'s in-memory filter and returns to
    /// the Cheats modal's row grid rather than closing it.
    CheatSearch,
}

impl NoteEdit {
    pub(super) fn limit(self) -> usize {
        match self {
            NoteEdit::None => 0,
            NoteEdit::Text => NOTE_CHAR_LIMIT,
            NoteEdit::SlotName
            | NoteEdit::PrintName(_)
            | NoteEdit::CheatSearch
            | NoteEdit::CardRename => SLOT_NAME_LIMIT,
        }
    }

    pub(super) fn heading(self) -> &'static str {
        match self {
            NoteEdit::None => "",
            NoteEdit::Text => "editando anotação (vazio apaga, clique fora cancela)",
            NoteEdit::SlotName => "renomeando o print (clique fora cancela)",
            NoteEdit::PrintName(_) => "nome do print (opcional)",
            NoteEdit::CheatSearch => "buscar cheat (vazio mostra todos)",
            NoteEdit::CardRename => "renomear memory card (clique fora cancela)",
        }
    }
}

/// A save/load-state or print slot picker, or the cheats checklist (plan
/// revision) — mutually exclusive with `paused` (the notebook has no
/// slot-picking or cheats of its own any more) and gates gameplay stepping
/// the same way `paused` does, so the frozen frame behind the dialog doesn't
/// keep moving while a choice is pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Modal {
    None,
    SaveSlot,
    LoadSlot,
    /// Picking which of the 15 note slots to save the just-grabbed print
    /// into — see `runner::print_capture`.
    PrintSlot,
    /// The curated cheats checklist (plan revision — split out of the
    /// notebook into its own menu). Unlike the other three, a pick
    /// (`ModalPick`) toggles a row in place and leaves the modal open
    /// instead of closing it.
    Cheats,
    /// The achievements list (plan: `docs/plano-retroachievements.md`,
    /// fase 4) — read-only rows with the earned state; a pick just closes.
    Achievements,
    /// A biblioteca de memory cards (plano §3): os `.mcr` de `memcards/` —
    /// escolher um encaixa no slot 1 (só desligado; a trava é no
    /// `OpenCards`), e a primeira linha cria um cartão novo.
    Cards,
    /// As ações sobre o card escolhido (usar/renomear/apagar) — o alvo e o
    /// slot vivem em `card_action`.
    CardsAction,
    /// A confirmação do apagar (destrutivo — nunca apaga direto).
    CardsConfirm,
}

/// One note slot's protection/caption (plan revision) — everything defaults
/// to "untouched" (not pinned, no caption) for a slot nobody's set either on.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct SlotMeta {
    #[serde(default)]
    pub(super) pinned: bool,
    #[serde(default)]
    pub(super) label: String,
}

/// `notes_dir/<title>/slots.json`'s shape: which of the 15 photo slots and
/// which of the 15 text-note slots (plan revision — the two are numbered
/// independently, slot 3's photo and slot 3's text share nothing but a
/// number) are pinned or captioned. Absent entries mean the default
/// (`SlotMeta::default()`) — most slots never need a real entry.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct NotesMeta {
    #[serde(default)]
    pub(super) slots: std::collections::BTreeMap<u8, SlotMeta>,
    #[serde(default)]
    pub(super) text_slots: std::collections::BTreeMap<u8, SlotMeta>,
}

impl NotesMeta {
    pub(super) fn slot(&self, slot: u8) -> SlotMeta {
        self.slots.get(&slot).cloned().unwrap_or_default()
    }

    pub(super) fn text_slot(&self, slot: u8) -> SlotMeta {
        self.text_slots.get(&slot).cloned().unwrap_or_default()
    }
}

/// Every one of the 15 slots resists overwrite — Printscreen has nowhere
/// left to put a new capture (plan revision: "avisar quando 15 fixados").
pub(super) fn all_slots_pinned(meta: &NotesMeta) -> bool {
    (1..=NOTE_SLOTS).all(|s| meta.slot(s).pinned)
}

/// Rows for the Printscreen modal's slot-picking step (plan revision): one
/// per note slot, in order, labelled with its number plus whether it's
/// filled/pinned; a pinned slot is disabled, same protection `NoteCapture`
/// used to enforce by auto-redirecting instead.
pub(super) fn print_slot_rows(
    notes_dir: &Path,
    title: &str,
    meta: &NotesMeta,
) -> Vec<(String, bool)> {
    (1..=NOTE_SLOTS)
        .map(|s| {
            let m = meta.slot(s);
            let filled = note_slot_path(notes_dir, title, s).exists();
            let label = if m.pinned {
                format!("Slot {s} (fixado)")
            } else if filled {
                format!("Slot {s}")
            } else {
                format!("Slot {s} (vazio)")
            };
            (label, !m.pinned)
        })
        .collect()
}

pub(super) fn notes_meta_path(notes_dir: &Path, title: &str) -> PathBuf {
    note_dir(notes_dir, title).join("slots.json")
}

/// Missing or unreadable is just "nothing pinned or captioned yet", not an
/// error — same tolerant read as the rest of this portable app's sidecars.
pub(super) fn load_notes_meta(notes_dir: &Path, title: &str) -> NotesMeta {
    fs::read_to_string(notes_meta_path(notes_dir, title))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(super) fn save_notes_meta(notes_dir: &Path, title: &str, meta: &NotesMeta) {
    let dir = note_dir(notes_dir, title);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(s) = serde_json::to_string_pretty(meta) {
        if let Err(e) = polystnx_domain::fsutil::write_atomic(notes_meta_path(notes_dir, title), s)
        {
            log::warn!("saving note slot metadata failed: {e}");
        }
    }
}

/// `notes_dir/<game title>/` — one folder per game, named for a human
/// browsing it rather than the ROM hash (plan revision: readability over
/// rename-proofing, since the player picked this explicitly). Mirrors
/// `game_dir` (`saves/<title>/`) — same sanitizing, same tradeoff.
pub(super) fn note_dir(notes_dir: &Path, title: &str) -> PathBuf {
    notes_dir.join(sanitize_dir_name(title))
}

/// `notes_dir/<title>/01.png` .. `15.png` — 1-indexed to match the slot
/// numbers shown on screen.
pub(super) fn note_slot_path(notes_dir: &Path, title: &str, slot: u8) -> PathBuf {
    note_dir(notes_dir, title).join(format!("{slot:02}.png"))
}

/// `notes_dir/<title>/notas.txt` — the old, pre-revision unbounded
/// free-text append log. Nothing writes here any more (see
/// `note_text_slot_path`, the 15-slot replacement) — this path only still
/// exists so `migrate_legacy_text_notes` can find and split up whatever an
/// earlier version of the app already wrote there.
pub(super) fn legacy_note_text_path(notes_dir: &Path, title: &str) -> PathBuf {
    note_dir(notes_dir, title).join("notas.txt")
}

/// `notes_dir/<title>/01.txt` .. `15.txt` — 1-indexed to match the slot
/// numbers shown on screen, numbered independently of the photo slots
/// (`note_slot_path`) even though they share the same range: a `.png` and
/// a `.txt` never collide, so slot 3's photo and slot 3's text are just two
/// unrelated files that happen to both say "3".
pub(super) fn note_text_slot_path(notes_dir: &Path, title: &str, slot: u8) -> PathBuf {
    note_dir(notes_dir, title).join(format!("{slot:02}.txt"))
}

/// Save the current frame into note `slot` (1..=15), overwriting whatever
/// was there before — same "pick a slot, it replaces what's in it" model as
/// save states.
pub(super) fn save_note_image(
    notes_dir: &Path,
    title: &str,
    slot: u8,
    frame: &EmuFrame,
) -> Result<()> {
    let dir = note_dir(notes_dir, title);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = note_slot_path(notes_dir, title, slot);
    let mut png = std::io::Cursor::new(Vec::new());
    frame_to_rgb8(frame)
        .write_to(&mut png, image::ImageFormat::Png)
        .with_context(|| format!("encoding note image {}", path.display()))?;
    polystnx_domain::fsutil::write_atomic(&path, png.into_inner())
        .with_context(|| format!("saving note image {}", path.display()))
}

/// Text-note `slot`'s saved content (plan revision — one of the 15,
/// mirroring the photo slots), or `None` for an empty slot — a
/// whitespace-only file counts as empty too, same as an empty draft never
/// got saved as one to begin with.
pub(super) fn read_text_slot(notes_dir: &Path, title: &str, slot: u8) -> Option<String> {
    fs::read_to_string(note_text_slot_path(notes_dir, title, slot))
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Save `text` into note-text `slot` (1..=15), overwriting whatever was
/// there before — same "pick a slot, it replaces what's in it" model
/// `save_note_image` already uses for the photo slots.
pub(super) fn save_text_slot(notes_dir: &Path, title: &str, slot: u8, text: &str) -> Result<()> {
    let dir = note_dir(notes_dir, title);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = note_text_slot_path(notes_dir, title, slot);
    polystnx_domain::fsutil::write_atomic(&path, text)
        .with_context(|| format!("saving text note {}", path.display()))
}

/// Clear note-text `slot` — a missing file already means "empty", so this
/// is not an error either way.
pub(super) fn delete_text_slot(notes_dir: &Path, title: &str, slot: u8) {
    let _ = fs::remove_file(note_text_slot_path(notes_dir, title, slot));
}

/// One-time upgrade from the old unbounded `notas.txt` append log into the
/// new 15 numbered slots (plan revision — "I wrote something and can't see
/// it again" was a real complaint: the old log had no viewer at all, just
/// a button that always appended a blank page). A no-op the moment any
/// text slot already exists, so a game already using the new format is
/// never re-split or overwritten. Entries in the old log were separated by
/// a blank line (`append_note_text` used to write `"{text}\n\n"`); the
/// first 15 non-empty ones become slots 1..=15, in the order they were
/// written. `notas.txt` itself is left in place afterward rather than
/// deleted — cheap insurance against a bug here losing anyone's notes
/// outright — but nothing reads it again once this has run once.
pub(super) fn migrate_legacy_text_notes(notes_dir: &Path, title: &str) {
    if (1..=NOTE_SLOTS).any(|s| note_text_slot_path(notes_dir, title, s).is_file()) {
        return;
    }
    let Ok(contents) = fs::read_to_string(legacy_note_text_path(notes_dir, title)) else {
        return;
    };
    let pages: Vec<&str> = contents
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if pages.is_empty() {
        return;
    }
    let mut migrated = 0;
    for (slot, page) in (1..=NOTE_SLOTS).zip(pages) {
        match save_text_slot(notes_dir, title, slot, page) {
            Ok(()) => migrated += 1,
            Err(e) => log::warn!("migrating legacy note into slot {slot} failed: {e}"),
        }
    }
    log::info!("notas.txt: migrated {migrated} page(s) into numbered text-note slots");
}

/// Panel display cap for a pinned text note (plan revision) — the full
/// slot can run up to `NOTE_CHAR_LIMIT`, way more than the side panel has
/// comfortable room for alongside everything else already in it. Mirrors
/// `note_thumb`'s own smaller size (200px) for the panel vs. the full
/// 900px the pause book gets.
pub(super) const PANEL_NOTE_SNIPPET_CHARS: usize = 120;

/// Truncate `text` to at most `max_chars`, with a trailing `...` if it
/// didn't already fit — see `PANEL_NOTE_SNIPPET_CHARS`.
pub(super) fn panel_text_snippet(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{head}...")
}

/// Recompute the panel's notebook block from what's actually on disk: how
/// many of the 15 print slots are pinned (plan revision — used to be how
/// many were filled, shown regardless of pin; a slot nobody deliberately
/// featured showing up in the always-visible panel anyway was the actual
/// complaint), a thumbnail if there's a pinned one to show, and a pinned
/// text note's content if there's one of those too — independent of the
/// photo side ("mostrar apenas uma nota e/ou uma imagem"), so either, both,
/// or neither can end up in the panel. Both sides pick the same way: the
/// currently-viewed slot if it's itself pinned, else the lowest-numbered
/// pinned one, else none. Call at game start, after every capture/write,
/// after cycling either slot, and after every pin toggle (either kind) —
/// cheap, at most 30 file-exists checks.
pub(super) fn refresh_notes(
    cab: &mut Cabinet,
    notes_dir: &Path,
    title: &str,
    slot: u8,
    text_slot: u8,
    meta: &NotesMeta,
) {
    let count = (1..=NOTE_SLOTS).filter(|&s| meta.slot(s).pinned).count();
    let thumb_slot = if meta.slot(slot).pinned {
        Some(slot)
    } else {
        (1..=NOTE_SLOTS).find(|&s| meta.slot(s).pinned)
    };
    let thumb = thumb_slot.and_then(|s| note_thumb(notes_dir, title, s, 200));

    let text_pin_slot = if meta.text_slot(text_slot).pinned {
        Some(text_slot)
    } else {
        (1..=NOTE_SLOTS).find(|&s| meta.text_slot(s).pinned)
    };
    let text = text_pin_slot
        .and_then(|s| read_text_slot(notes_dir, title, s))
        .map(|t| panel_text_snippet(&t, PANEL_NOTE_SNIPPET_CHARS));

    cab.set_notes(
        count,
        thumb.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
        text.as_deref(),
    );
}

/// Decode note `slot`'s image if it's filled, `max` pixels on the long
/// side — `None` for an empty slot or a decode failure either way.
pub(super) fn note_thumb(
    notes_dir: &Path,
    title: &str,
    slot: u8,
    max: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let path = note_slot_path(notes_dir, title, slot);
    path.is_file()
        .then(|| decode_art(&path, max))
        .and_then(Result::ok)
}

/// Push note `slot`'s current image, pin state, and caption onto the pause
/// book's right page — the `set_pause_page` call every slot-change (or
/// pin/rename) site needs.
pub(super) fn show_note_slot(
    cab: &mut Cabinet,
    notes_dir: &Path,
    title: &str,
    slot: u8,
    meta: &NotesMeta,
) {
    let thumb = note_thumb(notes_dir, title, slot, 900);
    let m = meta.slot(slot);
    cab.set_pause_page(
        (slot - 1) as usize,
        m.pinned,
        &m.label,
        thumb.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
    );
}

/// Push text-note `slot`'s saved content and pin state onto the pause
/// book's left page (plan revision, mirrors `show_note_slot`) — the
/// `set_pause_text_page` call every slot-change (or pin/write/delete)
/// site needs.
pub(super) fn show_text_slot(
    cab: &mut Cabinet,
    notes_dir: &Path,
    title: &str,
    slot: u8,
    meta: &NotesMeta,
) {
    let content = read_text_slot(notes_dir, title, slot);
    let pinned = meta.text_slot(slot).pinned;
    cab.set_pause_text_page((slot - 1) as usize, pinned, content.as_deref());
}
