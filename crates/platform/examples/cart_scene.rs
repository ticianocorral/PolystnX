//! Headless eyeball of the panel's cartridge slot animation (dev aid — no
//! window): renders a spread of insert/eject progress values to BMPs using
//! SDL's dummy video driver and a synthetic cartridge, captured through the
//! normal `capture_static_bmp` path so the whole panel is in frame.
//!
//! An optional second argument loads real art instead of the synthetic
//! cartridge — a raw RGBA frame (u32 LE width, u32 LE height, then rows):
//!
//! ```sh
//! ffmpeg -i "art.png" -f rawvideo -pix_fmt rgba art.raw
//! python3 -c "import struct; d=open('art.raw','rb').read(); \
//!   open('art.bin','wb').write(struct.pack('<II',700,500)+d)"
//! cargo run -p xperience-platform --example cart_scene -- out art.bin
//! ```

use std::path::PathBuf;

use xperience_platform::{Cabinet, PanelButton, Platform};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("SDL_VIDEODRIVER", "dummy");
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("cart_scene"));
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new()?;
    let mut cab: Cabinet = plat.create_cabinet("cart scene", 1280, 800, false)?;

    // Real art via the raw file, else a stand-in cartridge: 700x500 RGBA
    // like the real `assets/cartridge` scans — grey shell, dark connector
    // edge, label block, transparent margins, so the art's own alpha edges
    // get exercised too.
    let load_raw = |p: &str| {
        let bytes = std::fs::read(p).ok()?;
        if bytes.len() < 8 {
            return None;
        }
        let w = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let h = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        if bytes.len() < 8 + (w as usize) * (h as usize) * 4 {
            return None;
        }
        Some((w, h, bytes[8..].to_vec()))
    };
    let art = std::env::args().nth(2).and_then(|p| load_raw(&p));
    let (w, h, rgba) = art.unwrap_or_else(|| {
        let (w, h) = (700u32, 500u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let m = 12; // transparent margin
                if x < m || y < m || x >= w - m || y >= h - m {
                    continue;
                }
                let (r, g, b) = if y > h - h / 8 {
                    (70, 66, 60) // connector edge
                } else if y > h / 8 && y < h * 58 / 100 && x > w / 8 && x < w - w / 8 {
                    (107, 70, 168) // label — like the real scans, upper face
                } else {
                    (176, 170, 158) // shell
                };
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }
        (w, h, rgba)
    });
    // Optional third/fourth arguments: the console tag and the idle brand
    // logo as raw files (same format) — the real `assets/console-tag.png`
    // and `assets/console.png` in the app.
    if let Some(tag_arg) = std::env::args().nth(3) {
        if let Some((tw, th, trgba)) = load_raw(&tag_arg) {
            cab.set_slot_tag(Some((tw, th, trgba.as_slice())));
        }
    }
    if let Some(logo_arg) = std::env::args().nth(4) {
        if let Some((lw, lh, lrgba)) = load_raw(&logo_arg) {
            cab.set_console_logo(Some((lw, lh, lrgba.as_slice())));
        }
    }
    cab.set_panel(
        None,
        Some((w, h, &rgba)),
        "Exemplo",
        &[(PanelButton::Power, "Ligar".into())],
    );

    // 5% steps for both motions — dense enough to assemble a smooth GIF.
    for ejecting in [false, true] {
        for i in 0..=20usize {
            let t = i as f32 / 20.0;
            cab.set_cartridge_motion(Some((t, ejecting)));
            let kind = if ejecting { "eject" } else { "insert" };
            let name = out.join(format!("{kind}_{:02}.bmp", (t * 100.0) as i32));
            cab.capture_static_bmp(0.0, &name)?;
        }
        println!("{} frames ok", if ejecting { "eject" } else { "insert" });
    }

    // The idle screen: panel dropped, the same slot block with the
    // "Inserir cartucho" button where a seated cartridge would be.
    cab.clear_panel();
    let name = out.join("idle.bmp");
    cab.capture_static_bmp(0.0, &name)?;
    println!("{}", name.display());
    Ok(())
}
