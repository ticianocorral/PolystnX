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
    for n in 0..1230u32 {
        match n {
            100 => cab.toggle_tv_power(), // desliga: colapsa e vira vidro morto
            560 => cab.toggle_tv_power(), // liga: aquece (n=600 pega no meio)
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
        });
    }
    Ok(())
}
