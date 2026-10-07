//! Clica no "continuar" da tela de setup (headless) e captura o frame
//! seguinte: se o setup sumiu, o fluxo funciona.
use std::sync::mpsc;
use std::time::Duration;
use polystnx_app::idle::{self, IdleExit};
use polystnx_app::update_check::UpdateNotice;
use polystnx_platform::Platform;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("setup_click: iniciando (clique no continuar em 800ms)");
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    log::info!("agendando cliques no 'continuar' (640,745) em 800ms e 1800ms");
    plat.push_synthetic_click_later(640, 612, Duration::from_millis(800));
    plat.push_synthetic_click_later(640, 612, Duration::from_millis(1800));
    // sem mais cliques — o primeiro deve fechar o setup

    let (ntx, notice_rx) = mpsc::channel::<UpdateNotice>();
    drop(ntx);
    let mut notice_rx = Some(notice_rx);
    let exit = idle::run(&mut plat, &mut cab, 0.0, &mut notice_rx, false, false)?;
    log::info!("idle::run saiu");
    Ok(())
}
