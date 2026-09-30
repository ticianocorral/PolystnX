//! The bare emulator: a libretro core loads and runs, with video, sound, a pad,
//! save states, battery SRAM and run-ahead — no bezel, no selector.
//! Presentation is fixed: RF NTSC + CRT-tube warp.
//!
//! Usage:
//!   emu-run --core <path/to/swanstation_libretro.{dylib,so,dll}> --rom <game.sfc>
//!           [--system-dir DIR] [--save-dir DIR] [--runahead N]
//!
//! The core path also reads from $PSX_XPERIENCE_CORE. See docs/fase-0.md for where
//! to get the core, docs/fase-1.md for save states / SRAM / run-ahead. The
//! run-loop itself lives in `xperience_app::runner`, shared with `xperience`.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use xperience_app::config::Config;
use xperience_app::runner::{run_game, GameSpec};

struct Args {
    core: PathBuf,
    rom: PathBuf,
    system_dir: PathBuf,
    save_dir: PathBuf,
    /// Per-ROM notebooks (plan §3.4); defaults to `<save-dir>/notes`.
    notes_dir: PathBuf,
    config: Option<PathBuf>,
    /// O memory card físico do slot 1 (biblioteca `memcards/`, plano §3):
    /// o conteúdo vira o `SAVE_RAM` da sessão e o flush volta para o arquivo.
    card1: Option<PathBuf>,
    card2: Option<PathBuf>,
    /// Speculative frames past the shown one; `None` = take the config value.
    runahead: Option<u32>,
    /// Headless self-check: run N frames, save the composited window, exit.
    shot: Option<PathBuf>,
    shot_frame: u32,
    /// Logo art for the side panel (dev/testing).
    logo: Option<PathBuf>,
    /// Cartridge art for the side panel (dev/testing).
    cartridge: Option<PathBuf>,
    /// Headless: preview the idle "console off" screen instead of gameplay.
    shot_off: bool,
    /// Headless: force one note capture on the first frame (dev/testing).
    debug_note_capture: bool,
    /// Headless: preview the pause book instead of gameplay (dev/testing).
    debug_shot_pause: bool,
    /// Headless: preview one of the save/load/print/cheats modals instead
    /// of gameplay (dev/testing) — "save", "load", "print", or "cheats".
    debug_shot_modal: Option<String>,
    debug_cart_anim: Option<std::path::PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut core = std::env::var_os("PSX_XPERIENCE_CORE").map(PathBuf::from);
    let mut rom = None;
    let mut system_dir = None;
    let mut save_dir = None;
    let mut notes_dir = None;
    let mut config = None;
    let mut card1 = None;
    let mut card2 = None;
    let mut runahead = None;
    let mut shot = None;
    let mut shot_frame = 180u32;
    let mut logo = None;
    let mut cartridge = None;
    let mut shot_off = false;
    let mut debug_note_capture = false;
    let mut debug_shot_pause = false;
    let mut debug_shot_modal = None;
    let mut debug_cart_anim = None;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--core" => {
                core = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--core needs a value"))?
                        .into(),
                )
            }
            "--rom" => {
                rom = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--rom needs a value"))?
                        .into(),
                )
            }
            "--system-dir" => {
                system_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--system-dir needs a value"))?
                        .into(),
                )
            }
            "--save-dir" => {
                save_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--save-dir needs a value"))?
                        .into(),
                )
            }
            "--config" => {
                config = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--config needs a path"))?
                        .into(),
                )
            }
            "--card1" => {
                card1 = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--card1 needs a path (.mcr)"))?
                        .into(),
                )
            }
            "--card2" => {
                card2 = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--card2 needs a path (.mcr)"))?
                        .into(),
                )
            }
            "--runahead" => {
                runahead = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--runahead needs a number"))?
                        .parse()
                        .map_err(|_| anyhow!("--runahead wants a number (0 disables)"))?,
                )
            }
            "--shot" => {
                shot = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--shot needs a path (.bmp)"))?
                        .into(),
                )
            }
            "--shot-frame" => {
                shot_frame = it
                    .next()
                    .ok_or_else(|| anyhow!("--shot-frame needs a number"))?
                    .parse()
                    .map_err(|_| anyhow!("--shot-frame wants a number"))?
            }
            "--logo" => {
                logo = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--logo needs a path (image)"))?
                        .into(),
                )
            }
            "--cartridge" => {
                cartridge = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--cartridge needs a path (image)"))?
                        .into(),
                )
            }
            "--shot-off" => shot_off = true,
            "--debug-cart-anim" => {
                debug_cart_anim = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--debug-cart-anim needs a path"))?
                        .into(),
                );
            }
            "--notes-dir" => {
                notes_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--notes-dir needs a path"))?
                        .into(),
                )
            }
            "--debug-note-capture" => debug_note_capture = true,
            "--debug-shot-pause" => debug_shot_pause = true,
            "--debug-shot-modal" => {
                debug_shot_modal =
                    Some(it.next().ok_or_else(|| {
                        anyhow!("--debug-shot-modal needs save/load/print/cheats")
                    })?)
            }
            "-h" | "--help" => {
                println!("{}", HELP);
                std::process::exit(0);
            }
            other => bail!("unexpected argument: {other}"),
        }
    }

    let core = core.ok_or_else(|| anyhow!("no core: pass --core or set $PSX_XPERIENCE_CORE"))?;
    let rom = rom.ok_or_else(|| anyhow!("no ROM: pass --rom <file>"))?;
    let save_dir = save_dir.unwrap_or_else(|| PathBuf::from("."));
    // system directory do core = a BIOS (plano §1.1) — igual ao app.
    let system_dir = system_dir.unwrap_or_else(xperience_app::dirs::bios_dir);
    let notes_dir = notes_dir.unwrap_or_else(|| save_dir.join("notes"));
    Ok(Args {
        core,
        rom,
        system_dir,
        save_dir,
        notes_dir,
        config,
        card1,
        card2,
        runahead,
        shot,
        shot_frame,
        logo,
        cartridge,
        shot_off,
        debug_note_capture,
        debug_shot_pause,
        debug_shot_modal,
        debug_cart_anim,
    })
}

const HELP: &str = "emu-run --core <lib> --rom <game.sfc> [--system-dir D] [--save-dir D]\n\
       [--config config.toml] [--runahead N]\n\
       [--shot out.bmp [--shot-frame N]]   headless: run N frames, dump one, exit\n\
       [--logo img.png]                    show a logo atop the side panel\n\
       [--cartridge img.png]               show cartridge art below the logo\n\
       [--shot-off]                        with --shot, preview the idle off screen\n\
       [--notes-dir DIR]                   per-ROM notebooks (default: <save-dir>/notes)\n\
       [--debug-note-capture]              force one note capture at --shot-frame (dev/testing)\n\
       [--debug-shot-pause]                preview the pause book instead of gameplay (dev/testing)\n\
       [--debug-shot-modal save|load|print|cheats] preview a modal instead of gameplay (dev/testing)\n\
\n\
Presentation is fixed: RF NTSC + CRT-tube warp (knobs are consts in the source).\n\
Battery SRAM and 10 save-state slots live in --save-dir/<game title>/, same\n\
folder-per-game layout as the notebook (--notes-dir).\n\
Player 1 = keyboard or gamepad 1; player 2 = gamepad 2.\n\
Keyboard binds (gameplay only, see below) and the run-ahead default come from\n\
config.toml (see docs/fase-1).\n\
\n\
default keys (gameplay only): arrows=dpad  Z=B X=A A=Y S=X Q=L W=R\n\
      Enter=Start  RShift=Select\n\
\n\
Everything else — power, eject, reset, the notebook (Anotacoes: free text\n\
and a fixed 15-slot photo album per game, pin a slot to feature it in the\n\
side panel), Cheats (a checklist modal sourced from the full\n\
libretro-database SNES set, matched by the ROM's own title — absent when\n\
nothing in there matches), Printscreen, save/load state (all three open a\n\
slot-picker modal) — is a clickable button in the side panel (or the\n\
pause book's own buttons once paused); none of it is keyboard-bindable\n\
any more (plan revision: mouse/gamepad only for console commands,\n\
config.toml's [keyboard] section only ever holds gameplay binds).\n\
Closing the window always quits, on or off — no ceremony.";

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = parse_args()?;

    let cfg = Config::load(args.config.as_deref())?;
    if let Some(p) = &cfg.source {
        log::info!("config: {}", p.display());
    }

    let mut platform = xperience_platform::Platform::new().map_err(|e| anyhow!(e.to_string()))?;
    platform.set_pad_map(cfg.padmap.clone());
    let mut cabinet = platform
        .create_cabinet("PSX Xperience", 1024, 768, false)
        .map_err(|e| anyhow!(e.to_string()))?;
    if cfg.fullscreen {
        cabinet.toggle_fullscreen();
    }
    let spec = GameSpec {
        core: args.core,
        rom: args.rom,
        system_dir: args.system_dir,
        save_dir: args.save_dir,
        notes_dir: args.notes_dir,
        runahead: args.runahead,
        shot: args.shot.map(|p| (p, args.shot_frame)),
        logo: args.logo,
        card1: args.card1,
        card2: args.card2,
        display_title: None,
        cartridge: args.cartridge,
        shot_off: args.shot_off,
        debug_note_capture: args.debug_note_capture,
        debug_shot_pause: args.debug_shot_pause,
        debug_shot_modal: args.debug_shot_modal,
        debug_cart_anim: args.debug_cart_anim,
    };
    // Standalone: "back" and "close" both just end the process.
    run_game(&mut platform, &mut cabinet, &spec, &cfg)?;
    log::info!("bye");
    Ok(())
}
