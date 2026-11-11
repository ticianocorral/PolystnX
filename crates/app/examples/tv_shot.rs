//! Fotos do queixo e do vidro da TV pelo caminho VIVO (o mesmo `present` do
//! app): ligada (n=60), desligada (n=240), aquecendo (n=600) e com o volume
//! aceso (n=1200). Rodar:
//! `PSX_XPERIENCE_DEBUG_COMPOSITE=/tmp/tv cargo run -p polystnx-app --example tv_shot`
//! — escreve /tmp/tv.<n>.bmp.

fn main() -> anyhow::Result<()> {
    let plat = polystnx_platform::Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate(&format!(
        "PolystnX v{}\nPCSX Rearmed r26",
        env!("CARGO_PKG_VERSION")
    ));
    // `off K` fotografa o desligar K quadros depois do clique (n=240);
    // `on K`, o ligar K quadros depois do clique (n=600). Sem argumentos, o
    // roteiro padrão (n=60/240/600/1200).
    let mode = std::env::args().nth(1);
    let k: u32 = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let (off_at, on_at) = match mode.as_deref() {
        Some("off") => (240 - k, u32::MAX),
        Some("on") => (100, 600 - k),
        _ => (100, 560),
    };
    for n in 0..1230u32 {
        if n == off_at {
            cab.toggle_tv_power(); // desliga: a imagem esmaga e some
        }
        if n == on_at {
            cab.toggle_tv_power(); // liga: a imagem sobe do preto
        }
        match n {
            1180 => cab.nudge_volume(-5), // acende o OSD de volume
            1190 => {
                // abre o menu de imagem e sobe o brilho em 10 (n=1200)
                cab.apply_tv_control(polystnx_platform::TvControl::Menu);
                cab.apply_tv_control(polystnx_platform::TvControl::VolUp);
                cab.apply_tv_control(polystnx_platform::TvControl::VolUp);
            }
            _ => {}
        }
        if n >= 1195 {
            // o anel do foco do controle (renovado a cada quadro, como a tela dona)
            cab.set_pad_focus(Some(polystnx_platform::PadFocus::Tv(
                polystnx_platform::TvControl::Menu,
            )));
        }
        cab.frame_idle_2d((18, 18, 20), |d| {
            // Um quadro neutro: o degradê de um céu de menu, sem texto.
            let (w, h) = d.size();
            for row in 0..h as i32 {
                let t = row as f32 / h as f32;
                let c = (
                    (36.0 + 40.0 * t) as u8,
                    (52.0 + 50.0 * t) as u8,
                    (110.0 + 60.0 * t) as u8,
                    255,
                );
                d.fill(0, row, w, 1, c);
            }
        });
    }
    Ok(())
}
