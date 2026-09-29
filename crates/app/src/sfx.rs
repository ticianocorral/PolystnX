//! The console's mechanical sound effects (plan revision: "consegue usar os
//! sons daqui nas ações de inserir, ejetar, power on, power off e reset?" —
//! Pixabay, see THIRD-PARTY-NOTICES.md): real foley WAVs embedded in the
//! binary, one per console action, trimmed to roughly each action's length
//! with a short fade at the cut. A minimal PCM-WAV parser (the files are our
//! own fixed format: 22 050 Hz, 16-bit) decodes them at first use; anything
//! unexpected just fails the load and the callers fall back to the old
//! synthesized clicks.

use std::sync::OnceLock;

use xperience_platform::Cabinet;

/// The embedded effects, at `RATE` Hz mono S16LE.
pub const RATE: u32 = 22_050;

pub enum Sfx {
    /// Cartridge sliding into the slot (the insert animation's own length).
    Insert,
    /// Cartridge leaving the slot (the eject animation's).
    Eject,
    /// Power switch flipping on — plays over `power_on_burst` and keeps
    /// ringing briefly into the resumed game.
    PowerOn,
    /// Power switch flipping off — replaces the synthesized buzz cut.
    PowerOff,
    /// Reset button press.
    Reset,
    /// Achievement unlocked (plan revision: "coloque uma notificação sonora
    /// ao ganhar uma conquista") — a synthesized ascending chime, made
    /// in-repo (no Pixabay credit due), same fixed format as the foley set.
    Achievement,
}

struct Embedded(&'static [u8]);

fn embedded(name: Sfx) -> Embedded {
    match name {
        Sfx::Insert => Embedded(include_bytes!("sfx/insert.wav")),
        Sfx::Eject => Embedded(include_bytes!("sfx/eject.wav")),
        Sfx::PowerOn => Embedded(include_bytes!("sfx/power_on.wav")),
        Sfx::PowerOff => Embedded(include_bytes!("sfx/power_off.wav")),
        Sfx::Reset => Embedded(include_bytes!("sfx/reset.wav")),
        Sfx::Achievement => Embedded(include_bytes!("sfx/achievement.wav")),
    }
}

/// All five effects, decoded once on first use (copied out of the embedded
/// bytes) and kept for the process lifetime — power toggles repeat. One bank
/// instead of a per-call cache: a single `OnceLock<Option<..>>` used to
/// serve whichever sound decoded first for *every* action.
struct Bank {
    insert: Option<Vec<i16>>,
    eject: Option<Vec<i16>>,
    power_on: Option<Vec<i16>>,
    power_off: Option<Vec<i16>>,
    reset: Option<Vec<i16>>,
    achievement: Option<Vec<i16>>,
}

static BANK: OnceLock<Bank> = OnceLock::new();

fn bank() -> &'static Bank {
    BANK.get_or_init(|| Bank {
        insert: decode(embedded(Sfx::Insert).0),
        eject: decode(embedded(Sfx::Eject).0),
        power_on: decode(embedded(Sfx::PowerOn).0),
        power_off: decode(embedded(Sfx::PowerOff).0),
        reset: decode(embedded(Sfx::Reset).0),
        achievement: decode(embedded(Sfx::Achievement).0),
    })
}

/// Play `name` on the cabinet's persistent foley stream. Returns `false`
/// when there's no audio device or the embedded data didn't parse — callers
/// keep their old synthesized path in that case. Non-blocking: the samples
/// are queued and the call returns while SDL plays them.
pub fn play(cab: &mut Cabinet, name: Sfx) -> bool {
    let b = bank();
    let bank_match = match name {
        Sfx::Insert => &b.insert,
        Sfx::Eject => &b.eject,
        Sfx::PowerOn => &b.power_on,
        Sfx::PowerOff => &b.power_off,
        Sfx::Reset => &b.reset,
        Sfx::Achievement => &b.achievement,
    };
    let Some(samples) = bank_match else {
        return false;
    };
    cab.play_foley(samples, RATE)
}

/// Minimal RIFF/WAVE reader for our own files: 16-bit PCM, skipping straight
/// from the `fmt ` chunk to the `data` chunk. Stereo would interleave, but
/// we only ever write mono — treat anything else as a parse failure.
fn decode(bytes: &[u8]) -> Option<Vec<i16>> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut pos = 12usize;
    let mut samples: Option<Vec<i16>> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = &bytes[pos + 8..(pos + 8 + len).min(bytes.len())];
        if id == b"fmt " {
            let format = u16::from_le_bytes(body[0..2].try_into().ok()?);
            let channels = u16::from_le_bytes(body[2..4].try_into().ok()?);
            let bits = u16::from_le_bytes(body[14..16].try_into().ok()?);
            if format != 1 || channels != 1 || bits != 16 {
                return None;
            }
        } else if id == b"data" {
            samples = Some(
                body.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| i16::from_le_bytes(*c))
                    .collect(),
            );
        }
        pos += 8 + len + (len & 1); // chunks are word-aligned
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every embedded effect must parse and hold the audio we trimmed to —
    /// a broken `include_bytes!` path or parser would otherwise only surface
    /// as silent buttons at runtime.
    #[test]
    fn embedded_effects_parse_with_expected_length_and_content() {
        for (name, min_ms, max_ms) in [
            (Sfx::Insert, 1100, 1300),
            (Sfx::Eject, 1200, 1400),
            (Sfx::PowerOn, 1100, 1300),
            (Sfx::PowerOff, 650, 850),
            (Sfx::Reset, 800, 1000),
            (Sfx::Achievement, 800, 1000),
        ] {
            let d = decode(embedded(name).0).unwrap_or_else(|| panic!("falhou o parse"));
            let ms = d.len() as u64 * 1000 / RATE as u64;
            assert!(
                (min_ms..=max_ms).contains(&ms),
                "duração inesperada: {ms}ms"
            );
            let peak = d.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!(peak > 2000, "amostra quase silenciosa (pico {peak})");
        }
    }
}
