//! PROBE TEMPORÁRIO (não commitar): o painel de jogo nas resoluções do
//! Steam Deck (1280×800, 1280×720) vs FHD (1920×1080), para conferir o
//! layout apertado. Sem arte de jogo: retângulos chapados.
//!
//! cargo run -p polystnx-app --example deck_layout_probe -- 1280 800

use std::path::PathBuf;

use polystnx_platform::{FrameRef, PanelButton, PixelFormat, Platform};

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let w: u32 = it.next().unwrap_or_else(|| "1280".into()).parse().unwrap();
    let h: u32 = it.next().unwrap_or_else(|| "800".into()).parse().unwrap();
    let out = PathBuf::from("/tmp/deck-probe");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", w, h, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v1.1.0-beta\nPCSX Rearmed r26");

    let logo = flat(480, 120, (150, 40, 40, 255));
    let label = disc_art();
    let (lw, lh) = (256u32, 256u32);
    let commands = vec![
        (PanelButton::Notebook, "Anotações".to_string()),
        (PanelButton::Cheats, "Cheats".to_string()),
        (PanelButton::PrintScreen, "Printscreen".to_string()),
        (PanelButton::SaveState, "Salvar".to_string()),
        (PanelButton::LoadState, "Carregar".to_string()),
    ];
    cab.set_panel(
        Some((480, 120, &logo)),
        Some((lw, lh, &label)),
        "Mundo do Tomate (homebrew)",
        &commands,
    );
    cab.set_card_labels(Some("test"));
    cab.set_card_motion(None);
    cab.set_analog_led(true);
    cab.set_powered(true);

    // Cenário de mentira: gradiente céu + chão.
    let (gw, gh) = (512u32, 448u32);
    let mut px = vec![0u8; (gw * gh * 4) as usize];
    for y in 0..gh {
        for x in 0..gw {
            let (r, g, b) = if y < gh * 2 / 3 {
                (90, 140, 220)
            } else {
                (110, 80, 50)
            };
            let i = ((y * gw + x) * 4) as usize;
            px[i] = b;
            px[i + 1] = g;
            px[i + 2] = r;
            px[i + 3] = 255;
        }
    }
    let frame = FrameRef {
        width: gw,
        height: gh,
        pitch: (gw * 4) as usize,
        format: PixelFormat::Xrgb8888,
        pixels: &px,
    };
    let path = out.join(format!("painel-{w}x{h}.bmp"));
    cab.capture_bmp(&frame, 4.0 / 3.0, &path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", path.display());
    Ok(())
}

fn flat(w: u32, h: u32, c: (u8, u8, u8, u8)) -> Vec<u8> {
    vec![c.0, c.1, c.2, c.3]
        .into_iter()
        .cycle()
        .take((w * h * 4) as usize)
        .collect()
}

/// Disco redondo de mentira (RGBA com cantos transparentes): prata com furo
/// central e UMA marca fora do centro — sem ela o giro não muda um pixel.
fn disc_art() -> Vec<u8> {
    let (w, h) = (256u32, 256u32);
    let (cx, cy) = (128.0f32, 128.0f32);
    let mut img = image::RgbaImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        *px = if d > 126.0 {
            image::Rgba([0, 0, 0, 0])
        } else if d < 16.0 {
            image::Rgba([40, 38, 36, 255])
        } else if (135.0..145.0).contains(&(x as f32)) && (40.0..90.0).contains(&(y as f32)) {
            image::Rgba([70, 70, 80, 255])
        } else {
            let v = (
                210 - (d * 0.35) as u8,
                212 - (d * 0.35) as u8,
                218 - (d * 0.3) as u8,
                255,
            );
            image::Rgba([v.0, v.1, v.2, 255])
        };
    }
    img.into_raw()
}
