//! Fotos das telas do Cheater Fish pelo caminho vivo: o carregamento
//! (n=60) e a lista de cheats de um jogo de verdade (n=240).
//! `PSX_XPERIENCE_DEBUG_COMPOSITE=/tmp/fish cargo run -p polystnx-app --example fish_shot`

use polystnx_app::cheater_fish::{
    fish_pixels, internal_size, present, render_list, render_loading,
};
use polystnx_app::psx_gpu::Gpu;

fn main() -> anyhow::Result<()> {
    let plat = polystnx_platform::Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let title = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Tekken 3 (USA)".into());
    let defs = polystnx_domain::cheats_for_title(&title);
    let mut state = vec![false; defs.len()];
    for i in [0usize, 2, 3] {
        if let Some(s) = state.get_mut(i) {
            *s = true;
        }
    }
    let fish = fish_pixels();
    let (iw, ih) = internal_size(cab.shelf_buffer_size());
    let mut gpu = Gpu::new(iw, ih);
    for n in 0..260u32 {
        if n < 120 {
            render_loading(&mut gpu, &fish, &title, n as f32 / 120.0, n);
        } else {
            render_list(&mut gpu, &fish, &title, &defs, &state, 2, 0, n);
        }
        present(&mut cab, &gpu);
    }
    Ok(())
}
