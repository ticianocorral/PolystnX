//! Throwaway: vê a estante real do PolystnX com as capas 3D vazadas — o
//! shot prova a transparência e a contagem de colunas por resolução
//! (1280×800 → 3 por fileira; telas maiores → 5+).
use std::path::PathBuf;

use polystnx_app::dirs;
use polystnx_app::shelf::{self, ShelfOpts};
use polystnx_domain::{Catalog, Order};
use polystnx_platform::Platform;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let w: u32 = it.next().unwrap_or_else(|| "1280".into()).parse().unwrap();
    let h: u32 = it.next().unwrap_or_else(|| "800".into()).parse().unwrap();
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", w, h, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let catalog = Catalog::open(&dirs::roms_dir(), &dirs::library_path())
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // O cursor sobre o primeiro tile — o botão de play desenha nele.
    plat.debug_set_pointer(Some((250, 380)));

    let opts1 = ShelfOpts {
        order: Order::Shelf,
        max_frames: Some(240),
        shot: Some(PathBuf::from(format!("/tmp/shelf_poly1-{w}x{h}.bmp"))),
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    let _ = shelf::run(&mut plat, &mut cab, &catalog, &opts1)?;
    println!("saiu; frame em /tmp/shelf_poly1-{w}x{h}.bmp");
    Ok(())
}
