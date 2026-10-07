//! Headless shots of the dev mode (dev/testing only):
//! `cargo run --example devmode_shot -- [dir]` writes
//! `devmode-idle.bmp` (the idle panel with the "Dev" button above
//! "Configurações") and `devmenu.bmp` (the blank dev menu with just
//! "voltar"). The dev-mode flag is set directly — the Konami code that
//! normally turns it on is exercised by the platform's unit tests, not
//! here (no synthetic keyboard in the app crate: it doesn't link SDL).

use std::path::{Path, PathBuf};

use polystnx_app::{devmenu, idle};
use polystnx_platform::Platform;

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp".into()));
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v0.14.0\nPCSX Rearmed r26");

    // Dev mode on, as the idle screen would after the Konami code — the
    // idle panel must show "Dev" right above "Configurações".
    cab.set_dev_mode(true);
    let idle_shot = dir.join("devmode-idle.bmp");
    idle::capture_preview(
        &mut cab,
        idle::RESTING_STATIC,
        true,
        true,
        true,
        Path::new(&idle_shot),
    )?;
    println!("wrote {}", idle_shot.display());

    let menu_shot = dir.join("devmenu.bmp");
    devmenu::capture_preview(&mut cab, Path::new(&menu_shot))?;
    println!("wrote {}", menu_shot.display());
    Ok(())
}
