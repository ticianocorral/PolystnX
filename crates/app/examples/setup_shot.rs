//! Headless shot of the first-run setup screen (missing core + missing
//! DAT) — dev/preview only: `cargo run --example setup_shot -- out.bmp`.
//! Also draws the nameplate's green "tem update" dots (app + core) so both
//! new visuals can be eyeballed in one image.

use xperience_app::idle;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/setup.bmp".into());
    let plat = xperience_platform::Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PSX Xperience", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PSX Xperience v0.14.0\nSwanStation 1.63 TaC2rea");
    cab.set_nameplate_updates(true, true);
    idle::capture_preview(
        &mut cab,
        idle::RESTING_STATIC,
        false,
        false,
        std::path::Path::new(&path),
    )?;
    println!("wrote {path}");
    Ok(())
}
