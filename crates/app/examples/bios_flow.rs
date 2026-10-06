//! Diagnóstico headless do fluxo completo BIOS→inserir jogo, dirigindo o
//! run_game REAL: boot pela BIOS (disco ejetado), o gancho
//! PSX_XPERIENCE_DEBUG_AUTOINSERT dispara o InsertDisc + pick, e o shot
//! final mostra em que tela a sessão parou.
//!
//!   PSX_XPERIENCE_DEBUG_AUTOINSERT=1 cargo run -p polystnx-app --example bios_flow -- \
//!       <core> <bios-dir> <boot.chd> <jogo.chd> <save-dir> <shot.bmp>

use std::path::PathBuf;

use polystnx_app::{
    config::Config,
    dirs,
    runner::{run_game, GameSpec},
};
use polystnx_platform::Platform;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let mut it = std::env::args().skip(1);
    let core = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios"))?);
    let boot = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("boot"))?);
    let game = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("game"))?);
    let save_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("save-dir"))?);
    let shot = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("shot"))?);

    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let cfg = Config::load(None)?;
    let spec = GameSpec {
        core,
        rom: PathBuf::new(),
        system_dir: bios_dir.clone(),
        save_dir: save_dir.clone(),
        notes_dir: save_dir.join("notes"),
        runahead: Some(0),
        shot: Some((shot, 900)),
        logo: None,
        card1: None,
        card2: None,
        display_title: None,
        bios: true,
        library: vec![("Spawn".to_string(), boot), ("Tekken 3".to_string(), game)],
        cartridge: None,
        shot_off: false,
        debug_note_capture: false,
        debug_shot_pause: false,
        debug_shot_modal: None,
        debug_cart_anim: None,
    };
    run_game(&mut plat, &mut cab, &spec, &cfg)?;
    println!("exit ok");
    Ok(())
}
