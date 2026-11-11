//! Foto do CD genérico (jogo sem arte) em vários títulos, pelo caminho
//! vivo: `PSX_XPERIENCE_DEBUG_COMPOSITE=/tmp/gcd cargo run -p polystnx-app
//! --example generic_cd_shot` → /tmp/gcd.60.bmp.

use polystnx_app::shelf::draw_generic_cd;

fn main() -> anyhow::Result<()> {
    let plat = polystnx_platform::Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let games = [
        ("Tekken 3 (USA)", Some("1998")),
        (
            "Resident Evil 2 - Dual Shock Ver. (USA) (Disc 1)",
            Some("1998"),
        ),
        ("Legend of Dragoon, The (USA)", Some("2000")),
        ("Crash Bandicoot (USA)", None),
        (
            "Final Fantasy VII International Edition (Japan)",
            Some("1997"),
        ),
        ("Gran Turismo 2 (USA)", Some("1999")),
    ];
    for n in 0..70u32 {
        cab.frame_idle_2d((18, 18, 20), |d| {
            let (w, _) = d.size();
            let tile = (w as i32 - 40) / 3;
            for (i, (title, year)) in games.iter().enumerate() {
                let (col, row) = (i as i32 % 3, i as i32 / 3);
                let (x, y) = (20 + col * tile, 30 + row * 300);
                d.fill(x, y, tile as u32 - 10, 280, (40, 36, 30, 255));
                draw_generic_cd(
                    d,
                    title,
                    *year,
                    (i as u32 + 1).wrapping_mul(2_654_435_761),
                    (x + 4, y + 4, tile as u32 - 18, 272),
                );
            }
        });
        let _ = n;
    }
    Ok(())
}
