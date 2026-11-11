//! Foto do painel do console em jogo (botões POWER/RESET/OPEN):
//! `cargo run -p polystnx-app --example console_buttons_shot -- /tmp/btn.bmp`.
use polystnx_platform::{FrameRef, PanelButton, PixelFormat, Platform};

fn main() -> anyhow::Result<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/btn.bmp".into());
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_panel(
        None,
        None,
        "Tekken 3 (USA)",
        &[
            (PanelButton::SaveState, "Salvar".to_string(), true),
            (PanelButton::LoadState, "Carregar".to_string(), true),
        ],
    );
    cab.set_powered(true);
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
