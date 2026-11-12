//! Foto do painel do console em jogo (botões POWER/RESET/OPEN):
//! `cargo run -p polystnx-app --example console_buttons_shot -- /tmp/btn.bmp [card]`.
use polystnx_platform::{FrameRef, PanelButton, PixelFormat, Platform};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/btn.bmp".into());
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    polystnx_app::console_art::load_mc_logo(&mut cab);
    let logo = std::env::var("LOGO")
        .ok()
        .and_then(|p| image::open(p).ok())
        .map(|i| i.thumbnail(512, 512).to_rgba8());
    cab.set_panel(
        logo.as_ref()
            .map(|i| (i.width(), i.height(), i.as_raw().as_slice())),
        None,
        "Tekken 3 (USA)",
        &[
            (PanelButton::SaveState, "Salvar".to_string(), true),
            (PanelButton::LoadState, "Carregar".to_string(), true),
        ],
    );
    cab.set_powered(true);
    if std::env::args().nth(2).as_deref() == Some("card") {
        cab.set_card_labels(Some("Meu Save"));
        if let Some(t) = std::env::args().nth(3).and_then(|v| v.parse::<f32>().ok()) {
            cab.set_card_motion(Some((t, true)));
        }
    }
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
