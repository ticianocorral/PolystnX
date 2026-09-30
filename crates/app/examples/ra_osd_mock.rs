//! MOCK — design preview for the achievements notification (RetroAchievements
//! plan): renders an in-game cabinet (synthetic scene in the tube, real panel,
//! real nameplate) with the notification block in the chin's right side, and
//! captures it headlessly via `Cabinet::capture_bmp`.
//!
//! Run: `cargo run -p xperience-app --example ra_osd_mock` — writes BMPs to
//! docs/mocks/ (convert with `sips -s format png` or any image tool).
//!
//! Now uses the real OSD queue (`Cabinet::push_osd`) that ships in-game.

use std::path::PathBuf;
use std::time::Duration;

use xperience_platform::{FrameRef, PanelButton, PixelFormat, Platform, DEMO_BADGE_IMG};

fn main() -> anyhow::Result<()> {
    let out_dir = PathBuf::from("docs/mocks");
    std::fs::create_dir_all(&out_dir)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PSX Xperience", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // What the app shows in-game today: nameplate with versions, the panel's
    // command rows, power on, a running session clock.
    cab.set_nameplate("PSX Xperience v0.14.0\nSwanStation 1.63");
    cab.set_panel(
        None,
        None,
        "Super Mario World (USA)",
        &[
            (PanelButton::Notebook, "Anotações".to_string()),
            (PanelButton::Cheats, "Cheats".to_string()),
            (PanelButton::PrintScreen, "Printscreen".to_string()),
            (PanelButton::SaveState, "Salvar".to_string()),
            (PanelButton::LoadState, "Carregar".to_string()),
        ],
    );
    cab.set_powered(true);
    cab.set_session_time(Duration::from_secs(4935));

    let pixels = synthetic_scene(512, 448);
    let frame = FrameRef {
        width: 512,
        height: 448,
        pitch: 512 * 4,
        format: PixelFormat::Xrgb8888,
        pixels: &pixels,
    };

    // A real 64×64 badge from retroachievements.org, for the badge variant.
    let badge = image::open(out_dir.join("badge-59505.png"))?.to_rgba8();
    cab.set_image(
        DEMO_BADGE_IMG,
        badge.width(),
        badge.height(),
        badge.as_raw(),
    );

    let shots: &[(&str, &[&str])] = &[
        (
            "ra-notificacao.bmp",
            &[
                "CONQUISTA DESBLOQUEADA",
                "Yoshi Rules The World",
                "+10 pontos",
            ],
        ),
        (
            "ra-notificacao-nome-longo.bmp",
            &[
                "CONQUISTA DESBLOQUEADA",
                "Okay, But This Is The Last Time, I Promise",
                "+25 pontos",
            ],
        ),
        (
            "ra-notificacao-com-imagem.bmp",
            &[
                "CONQUISTA DESBLOQUEADA",
                "Yoshi Rules The World",
                "+10 pontos",
            ],
        ),
    ];
    for (file, lines) in shots {
        cab.push_osd(lines, Some(DEMO_BADGE_IMG), Duration::from_secs(6));
        cab.capture_bmp(&frame, 4.0 / 3.0, &out_dir.join(file))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        println!("wrote {}", out_dir.join(file).display());
    }
    Ok(())
}

/// A cheerful platformer-ish scene — sky, clouds, hills, checker ground,
/// blocks, coins and a little runner — just so the tube shows a "game".
fn synthetic_scene(w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as i32, h as i32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let put = |px: &mut [u8], x: i32, y: i32, c: (u8, u8, u8)| {
        if x < 0 || y < 0 || x >= w || y >= h {
            return;
        }
        let i = ((y * w + x) * 4) as usize;
        px[i] = c.2;
        px[i + 1] = c.1;
        px[i + 2] = c.0; // XRGB8888, little-endian
    };

    for y in 0..h {
        let t = y as f32 / h as f32;
        let c = ((90.0 + 30.0 * t) as u8, (150.0 + 30.0 * t) as u8, 250u8);
        for x in 0..w {
            put(&mut px, x, y, c);
        }
    }
    for (cx, cy, cw) in [(80, 60, 90), (300, 100, 120), (430, 50, 70)] {
        for dy in -12..=12 {
            for dx in -(cw / 2)..(cw / 2) {
                let inside =
                    (dx * dx) as f32 / ((cw * cw) / 4) as f32 + (dy * dy) as f32 / 144.0 < 1.0;
                if inside {
                    put(&mut px, cx + dx, cy + dy, (252, 252, 250));
                }
            }
        }
    }
    for x in 0..w {
        let y0 = 260 + ((x as f32 * 0.012).sin() * 40.0) as i32;
        for y in y0..h {
            put(&mut px, x, y, (44, 120, 52));
        }
    }
    for x in 0..w {
        let y0 = 320 + ((x as f32 * 0.02 + 1.7).sin() * 30.0) as i32;
        for y in y0..h {
            put(&mut px, x, y, (70, 170, 70));
        }
    }
    for y in (h - 64)..h {
        for x in 0..w {
            let c = if ((x / 16) + (y / 16)) % 2 == 0 {
                (208, 120, 48)
            } else {
                (176, 96, 40)
            };
            put(&mut px, x, y, c);
        }
    }
    for bx in [180, 228, 276, 324] {
        for dy in 0..28 {
            for dx in 0..28 {
                let c = if dy < 2 || dx < 2 || dy > 25 || dx > 25 {
                    (120, 70, 20)
                } else {
                    (240, 180, 60)
                };
                put(&mut px, bx + dx, 150 + dy, c);
            }
        }
    }
    for (cx, cy) in [(130, 190), (352, 205)] {
        for dy in -10..=10 {
            for dx in -6..=6 {
                let k = (dx * dx) as f32 / 36.0 + (dy * dy) as f32 / 100.0;
                if k <= 1.0 {
                    let c = if k > 0.55 {
                        (230, 170, 40)
                    } else {
                        (250, 220, 90)
                    };
                    put(&mut px, cx + dx, cy + dy, c);
                }
            }
        }
    }
    // A little runner standing on the ground: cap, face, overalls, shoes.
    let (px0, py0, s) = (240, h - 64 - 40, 4);
    let sprite = [
        "  rrrrrr  ",
        " rrrrrrrr ",
        "  ssffes  ",
        "  sfffeo  ",
        "  bbbbbb  ",
        " bbbbbbbb ",
        " bb bb bb ",
        " yyyyyyyy ",
        " yy yy yy ",
        " nn    nn ",
    ];
    for (dy, row) in sprite.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            let c = match ch {
                'r' => (220, 40, 40),
                's' => (250, 200, 150),
                'f' => (250, 220, 170),
                'e' => (20, 20, 20),
                'o' => (240, 230, 220),
                'b' => (40, 80, 220),
                'y' => (240, 200, 60),
                'n' => (80, 50, 30),
                _ => continue,
            };
            for yy in 0..s {
                for xx in 0..s {
                    put(
                        &mut px,
                        px0 + dx as i32 * s + xx,
                        py0 + dy as i32 * s + yy,
                        c,
                    );
                }
            }
        }
    }
    px
}
