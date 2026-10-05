//! Throwaway: reproduz o bug do favoritar na resolução Steam Deck
//! (1280x800) — roda a estante real, clica em "Favoritar" e "Voltar" com
//! cliques sintéticos e salva o frame final (opts.shot captura na saída).
use std::path::PathBuf;
use std::time::Duration;

use polystnx_app::dirs;
use polystnx_app::shelf::{self, ShelfOpts};
use polystnx_domain::{Catalog, Order};
use polystnx_platform::Platform;

fn main() -> anyhow::Result<()> {
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let catalog = Catalog::open(&dirs::roms_dir(), &dirs::library_path())
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // "antes": sem cliques, shot cedo — para comparar com o depois.
    let antes = std::env::args().any(|a| a == "--antes");
    let (shot, frames) = if antes {
        (PathBuf::from("/tmp/fav_before.bmp"), 240)
    } else {
        // Favoritar no painel direito (y~686) aos 4s; o shot captura o
        // estado aos 700 frames (~11,6s), já com a faixa de favoritos.
        if !antes {
            plat.push_synthetic_click_later(1120, 686, Duration::from_secs(4));
        }
        (PathBuf::from("/tmp/fav_after.bmp"), 700)
    };

    let opts = ShelfOpts {
        order: Order::Shelf,
        max_frames: Some(frames),
        shot: Some(shot),
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    let _ = shelf::run(&mut plat, &mut cab, &catalog, &opts)?;
    println!("saiu; frame final em /tmp/fav_after.bmp");
    Ok(())
}
