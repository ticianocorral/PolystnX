//! Standalone shelf preview: a scrollable grid of covers (or a multicart
//! list, see `polystnx_app::shelf`) with a details panel, mouse and
//! gamepad navigation only (no keyboard). On confirm it prints the chosen ROM
//! path to stdout and exits 0; on cancel it exits 1. The shelf itself lives
//! in `polystnx_app::shelf`, shared with the unified `xperience` binary —
//! this binary is just a dev/test harness for it, reading `roms/` next to
//! wherever it's run from (same portable layout as `xperience`, see
//! `polystnx_app::dirs`).

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use polystnx_app::dirs;
use polystnx_app::shelf::{self, Pick, ShelfOpts};
use polystnx_domain::{Catalog, Order};
use polystnx_platform::Platform;

fn main() -> Result<()> {
    polystnx_app::dirs::migrate_legacy_data_root();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let order = match arg(&args, "--order") {
        Some("name") => Order::Name,
        _ => Order::Shelf,
    };
    // Headless smoke test: run N frames then exit 0 without picking anything;
    // with --shot, save the last frame (shelf through the tube) as a BMP.
    let max_frames: Option<u64> = arg(&args, "--frames").and_then(|s| s.parse().ok());
    let shot = arg(&args, "--shot").map(PathBuf::from);
    let preset_filter = arg(&args, "--filter").map(str::to_string);
    let debug_history = args.iter().any(|a| a == "--debug-history-shot");

    let catalog = Catalog::open(&dirs::roms_dir(), &dirs::library_path())
        .with_context(|| "opening the catalog")?;

    let mut plat = Platform::new().map_err(|e| anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow!(e.to_string()))?;

    // Headless self-check: the "Histórico" screen, dev/testing only.
    if let (true, Some(path)) = (debug_history, &shot) {
        shelf::capture_history_preview(&mut cab, &catalog, path)?;
        log::info!("wrote {} (history preview)", path.display());
        return Ok(());
    }

    let opts = ShelfOpts {
        order,
        max_frames,
        shot,
        fade_in: None,
        preset_filter,
        ra: None,
    };
    match shelf::run(&mut plat, &mut cab, &catalog, &opts)? {
        Pick::Play { rom, .. } => {
            println!("{}", rom.display());
            Ok(())
        }
        // The `--frames` smoke test ends here too; that's a clean exit, not a cancel.
        Pick::Quit if max_frames.is_some() => Ok(()),
        Pick::Quit => std::process::exit(1),
        // This standalone tool has no idle/root screen to fall back to —
        // Esc backing out is the same cancel as closing the window.
        Pick::Back => std::process::exit(1),
        Pick::Settings => {
            eprintln!("settings screen isn't wired up in `selector` — use `xperience`");
            std::process::exit(1);
        }
        Pick::History => {
            eprintln!(
                "historico's live loop isn't wired up in `selector` — use `xperience`, or \
                 --debug-history-shot --shot for a headless preview"
            );
            std::process::exit(1);
        }
        Pick::Refresh => {
            eprintln!("the refresh button isn't wired up in `selector` — use `xperience`");
            std::process::exit(1);
        }
    }
}

fn arg<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}
