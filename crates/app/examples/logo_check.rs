//! Captura a idle com a logo nova + a tampa (diagnóstico).
use polystnx_app::idle;
use polystnx_platform::Platform;
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v1.0.0-beta\nPCSX Rearmed r26");
    idle::capture_preview(
        &mut cab,
        idle::RESTING_STATIC,
        true,
        true,
        true,
        Path::new("/tmp/logo-check.bmp"),
    )?;
    println!("ok");
    Ok(())
}
