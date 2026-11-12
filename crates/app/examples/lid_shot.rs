//! Foto do console em jogo com o disco assentado, tampa fechada ou aberta:
//! `cargo run -p polystnx-app --example lid_shot -- /tmp/lid.bmp [open]`.
use polystnx_platform::{FrameRef, PanelButton, PixelFormat, Platform};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/lid.bmp".into());
    let open = std::env::args().nth(2).as_deref() == Some("open");
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    polystnx_app::console_art::load_mc_logo(&mut cab);
    polystnx_app::console_art::load_cd_reader(&mut cab);
    polystnx_app::console_art::load_cd_seat(&mut cab);
    let disc = std::env::var("DISC")
        .ok()
        .and_then(|p| image::open(p).ok())
        .map(|i| i.thumbnail(512, 512).to_rgba8());
    cab.set_panel(
        None,
        disc.as_ref()
            .map(|i| (i.width(), i.height(), i.as_raw().as_slice())),
        "Alien Trilogy (USA)",
        &[(PanelButton::SaveState, "Salvar".to_string(), true)],
    );
    cab.set_powered(true);
    cab.set_drive(open, true);
    let pixels = vec![40u8; 320 * 240 * 4];
    let frame = FrameRef {
        width: 320,
        height: 240,
        pitch: 320 * 4,
        format: PixelFormat::Xrgb8888,
        pixels: &pixels,
    };
    cab.capture_bmp(&frame, 4.0 / 3.0, std::path::Path::new(&out))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
