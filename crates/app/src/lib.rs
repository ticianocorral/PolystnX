//! Shared bits for the `xperience-app` binaries.
//!
//! - [`config`] — `psx-xperience.cfg` (run-ahead, fullscreen, key binds).
//! - [`core_update`] — download/update the SwanStation core from the libretro buildbot.
//! - [`dirs`] — the portable app layout (one root: next to the executable
//!   on Windows/Linux, `~/Documents/PSX Xperience` on macOS).
//! - [`devmenu`] — the dev-mode menu (Konami-code secret; blank for now).
//! - [`idle`] — the idle/root screen (`xperience`'s home: TV off, "Inserir cartucho").
//! - [`rom_rename`] — rename ROMs to their canonical No-Intro name (settings-screen action).
//! - [`runner`] — the emulator run-loop (`emu-run`, and `xperience` between games).
//! - [`settings`] — the settings screen (`xperience` only, opened with `O` on the shelf).
//! - [`sfx`] — the console's embedded foley sounds (insert/eject/power/reset).
//! - [`shelf`] — the selector grid (`selector`, and `xperience` between games).
//! - [`update_check`] — startup checks for a newer release/SwanStation core.

pub mod bios;
pub mod config;
pub mod console_art;
pub mod core_update;
pub mod dat_update;
pub mod devmenu;
pub mod dirs;
pub mod idle;
pub mod manual;
pub mod ra;
pub mod rom_rename;
pub mod runner;
pub mod settings;
pub mod sfx;
pub mod shelf;
pub mod update_check;

/// A logo oficial do RetroAchievements (o favicon do site, embutida) — o
/// runner/bin registra no `Cabinet` com [`xperience_platform::RA_LOGO_IMG`]
/// para o badge do queixo. Vermelho/dourado/azul: a marca lida na própria
/// TV.
pub const RA_ICON_PNG: &[u8] = include_bytes!("../assets/ra-icon.png");
