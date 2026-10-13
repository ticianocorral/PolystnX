//! Prints REAIS do site: as telas de verdade do app — catálogo real do
//! jogador, núcleo PCSX Rearmed real, disco real da biblioteca (Resident
//! Evil 3) e a animação de inserção do disco pelo caminho de verdade
//! (`run_game`). Nenhum pixel é desenhado à mão aqui; tudo vem do app como
//! o jogador vê.
//!
//! Rodar: `cargo run -p polystnx-app --example site_screens_real` — escreve
//! BMPs em /tmp/site-real/ (inicial, estante, hero, cartanim/cart-NN,
//! ra-badge, ra-notificacao). Converter para site/assets/ com
//! `sips -s format jpeg` e montar o GIF do disco com os frames cart-NN.

use std::path::PathBuf;
use std::time::Duration;

use polystnx_app::config::Config;
use polystnx_app::dirs;
use polystnx_app::idle;
use polystnx_app::runner::{self, GameSpec};
use polystnx_app::shelf::{self, ShelfOpts};
use polystnx_domain::{Catalog, NoIntroDat, Order};
use polystnx_platform::Platform;

/// O disco do site: a capa, a logo e a arte do disco existem na biblioteca
/// do autor, e o boot é rápido o bastante para chegar ao jogo no frame do
/// shot.
const DISCO: &str = "Resident Evil 3 - Nemesis (USA, Canada)";

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/site-real");
    std::fs::create_dir_all(&out)?;

    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v1.2.2-beta\nPCSX Rearmed r26");

    // --- Tela inicial de verdade: tubo em estática no AV 1, painel idle
    //     real ("Estante de games" + rodapé), sem prompts de setup. ---
    cab.clear_panel();
    idle::capture_preview(
        &mut cab,
        idle::RESTING_STATIC,
        true,
        true,
        true,
        &out.join("inicial.bmp"),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote inicial.bmp");

    // --- Estante de verdade: catálogo real (os CHDs do jogador, capas da
    //     pasta assets/cover), cursor parado no primeiro tile. SEM cliques:
    //     clicar faz a estante sair por Pick::Play antes do frame do shot
    //     (e o estante.bmp nunca é gravado). ---
    let dat = NoIntroDat::load(&dirs::nointro_dat_path()).ok();
    let catalog = Catalog::open_with_dat(
        &dirs::roms_dir(),
        &dirs::library_path(),
        dat.as_ref(),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    plat.debug_set_pointer(Some((160, 380)));
    let opts = ShelfOpts {
        order: Order::Shelf,
        max_frames: Some(240),
        shot: Some(out.join("estante.bmp")),
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    let _ = shelf::run(&mut plat, &mut cab, &catalog, &opts)?;
    println!("wrote estante.bmp");

    // --- Hero + animação do disco: o Resident Evil 3 da biblioteca rodando
    //     no núcleo real, com a arte do próprio jogador. Duas passadas: a
    //     primeira só salva os frames da inserção do disco e sai; a segunda
    //     roda até o frame do shot. Lançamento SEM card, igual ao app (a
    //     inserção de card é manual, pelo modal). ---
    let mut cfg = Config::load(None)?;
    let base = GameSpec {
        core: dirs::core_dir().join("pcsx_rearmed_libretro.dylib"),
        rom: dirs::roms_dir().join(format!("{DISCO}.chd")),
        system_dir: dirs::roms_dir().parent().unwrap().join("bios"),
        save_dir: dirs::saves_dir(),
        notes_dir: dirs::notes_dir(),
        shot: None,
        logo: Some(dirs::assets_dir().join("logo").join(format!("{DISCO}.png"))),
        card1: None,
        display_title: None,
        cartridge: Some(dirs::assets_dir().join("disc").join(format!("{DISCO}.png"))),
        shot_off: false,
        debug_note_capture: false,
        debug_shot_pause: false,
        debug_shot_modal: None,
        debug_cart_anim: None,
    };

    let mut anim = base.clone_shallow();
    anim.debug_cart_anim = Some(out.join("cartanim"));
    let _ = runner::run_game(&mut plat, &mut cab, &anim, &mut cfg)?;
    println!("wrote cartanim frames");

    let mut hero = base.clone_shallow();
    // 2400 frames (~40 s de jogo): o boot do RE3 atravessa BIOS, tela preta
    // e logo da Capcom antes da intro — no frame 900 o tubo ainda estava
    // preto.
    hero.shot = Some((out.join("hero.bmp"), 2400));
    let _ = runner::run_game(&mut plat, &mut cab, &hero, &mut cfg)?;
    println!("wrote hero.bmp");

    // --- Conquistas: o OSD e o badge reais do RA sobre o frame REAL do
    //     jogo (o hero.bmp volta como FrameRef — só o texto da notificação
    //     é stand-in, desbloqueio de verdade exigiria conta + unlock ao
    //     vivo). ---
    {
        use polystnx_platform::{FrameRef, PixelFormat, DEMO_BADGE_IMG, RA_LOGO_IMG};
        let mut cab = plat
            .create_cabinet("PolystnX", 1280, 800, false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        cab.set_nameplate("PolystnX v1.2.2-beta\nPCSX Rearmed r26");
        if let Ok(icon) = image::load_from_memory(polystnx_app::RA_ICON_PNG) {
            let icon = icon.to_rgba8();
            cab.set_image(RA_LOGO_IMG, icon.width(), icon.height(), icon.as_raw());
        }
        // BMP 32 bits bottom-up decodificado na mão (o crate `image` do
        // workspace vem sem o recurso BMP) — direto para XRGB8888.
        let (w, h) = (1280u32, 800u32);
        let raw = std::fs::read(out.join("hero.bmp"))?;
        let off = u32::from_le_bytes([raw[10], raw[11], raw[12], raw[13]]) as usize;
        let stride = (w as usize) * 4;
        let mut px = vec![0u8; stride * h as usize];
        for y in 0..h as usize {
            let src = &raw[off + y * stride..off + (y + 1) * stride];
            // linha y (de baixo) vira linha h-1-y (de cima); BMP já é B,G,R,X.
            let dst = &mut px[(h as usize - 1 - y) * stride..(h as usize - y) * stride];
            dst.copy_from_slice(src);
        }
        let frame = FrameRef {
            width: w,
            height: h,
            pitch: (w * 4) as usize,
            format: PixelFormat::Xrgb8888,
            pixels: &px,
        };
        cab.set_powered(true);
        cab.set_session_time(Duration::from_secs(1_493));
        cab.set_ra_status(Some(polystnx_platform::RaStatus { hardcore: true }));

        // Badge fixo (fila de OSD vazia).
        cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("ra-badge.bmp"))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        println!("wrote ra-badge.bmp");

        // Notificação no queixo da TV — badge sintético (o real é baixado
        // da conta; aqui o círculo do RA serve de ícone).
        cab.set_image(DEMO_BADGE_IMG, 64, 64, &ra_badge());
        cab.push_osd(
            &["CONQUISTA DESBLOQUEADA", "Salvando a Jill", "+5 pontos"],
            Some(DEMO_BADGE_IMG),
            Duration::from_secs(6),
        );
        cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("ra-notificacao.bmp"))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        println!("wrote ra-notificacao.bmp");
    }

    Ok(())
}

/// Badge 64×64 simples (disco do RA) — stand-in do badge que a conta real
/// baixa, só para a notificação do site.
fn ra_badge() -> Vec<u8> {
    const S: u32 = 64;
    let mut px = vec![0u8; (S * S * 4) as usize];
    let red = (200u8, 40u8, 40u8, 255u8);
    let cream = (245u8, 238u8, 220u8, 255u8);
    for y in 0..S as i32 {
        for x in 0..S as i32 {
            let (dx, dy) = (x - 32, y - 32);
            let d = (dx * dx + dy * dy) as f32;
            let i = ((y * S as i32 + x) * 4) as usize;
            if d <= 30.0 * 30.0 {
                let c = if d >= 26.0 * 26.0 { cream } else { red };
                px[i] = c.0;
                px[i + 1] = c.1;
                px[i + 2] = c.2;
                px[i + 3] = 255;
            }
        }
    }
    px
}

/// `GameSpec` não é `Clone` (nada pesado, mas campos todos simples) — um
/// helper mínimo para as duas passadas.
trait GameSpecExt {
    fn clone_shallow(&self) -> GameSpec;
}

impl GameSpecExt for GameSpec {
    fn clone_shallow(&self) -> GameSpec {
        GameSpec {
            core: self.core.clone(),
            rom: self.rom.clone(),
            system_dir: self.system_dir.clone(),
            save_dir: self.save_dir.clone(),
            notes_dir: self.notes_dir.clone(),
            shot: self.shot.clone(),
            logo: self.logo.clone(),
            card1: self.card1.clone(),
            display_title: self.display_title.clone(),
            cartridge: self.cartridge.clone(),
            shot_off: self.shot_off,
            debug_note_capture: self.debug_note_capture,
            debug_shot_pause: self.debug_shot_pause,
            debug_shot_modal: self.debug_shot_modal.clone(),
            debug_cart_anim: self.debug_cart_anim.clone(),
        }
    }
}
