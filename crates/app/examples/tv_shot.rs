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
    cab.set_nameplate("PolystnX v1.3.12-beta\nPCSX Rearmed r26");
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
            d.text(40, 40, 2, (232, 232, 232), "tv de teste");
            // Barras de cor para ver a imagem esmagar e subir.
            let (w, h) = d.size();
            let bars = [
                (220, 60, 60),
                (230, 200, 60),
                (70, 200, 90),
                (60, 140, 230),
                (180, 90, 220),
            ];
            for (i, c) in bars.iter().enumerate() {
                d.fill(
                    40 + i as i32 * 130,
                    140,
                    120,
                    h.saturating_sub(300),
                    (c.0, c.1, c.2, 255),
                );
            }
            let _ = w;
        });
    }
    Ok(())
}
