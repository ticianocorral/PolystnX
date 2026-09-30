//! The whole thing: idle → selector → game → idle → …, in one process, no
//! shell glue, no installation. Portable: `roms/`, `core/`, `assets/`,
//! `saves/`, `notes/` and a `config/` folder (psx-xperience.cfg, nointro.dat,
//! library.json, hashcache.json) all live in one root (see
//! `xperience_app::dirs`) — next to the executable on Windows/Linux,
//! `~/Documents/PSX Xperience` on macOS — drop ROMs in `roms/` and go.
//!
//! The idle screen (TV off, "Inserir cartucho"/"Configurações" in place of
//! the logo) is the app's home: it's what you see at startup, after backing
//! out of the shelf, and after ejecting a game — only closing the window
//! ends the app. Nothing here uses the keyboard (plan revision:
//! mouse/gamepad only) — in a game, the panel's Power button powers off
//! (state, saves, TV to snow) and back on again, and Eject only takes once
//! off, landing back on the idle screen (plan §3.3). "Configurações" (idle
//! screen or shelf) opens settings (controls, run-ahead, fullscreen, SwanStation
//! core download/update) — see `xperience_app::settings`.
//!
//! Usage:
//!   psx-xperience [--core path/to/swanstation_libretro.{dylib,so,dll}]
//!             [--config psx-xperience.cfg] [--save-dir DIR] [--system-dir DIR]
//!             [--order shelf|name] [--runahead N]
//!
//! No `--core`/`$PSX_XPERIENCE_CORE`? Looks for one already downloaded into
//! `core/` (see the settings screen, "Núcleo") — not included in the app
//! itself, non-commercial SwanStation license (see THIRD-PARTY-NOTICES.md).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use anyhow::{anyhow, Context, Result};
use xperience_app::config::Config;
use xperience_app::core_update;
use xperience_app::idle::{self, IdleExit};
use xperience_app::runner::{run_game, GameExit, GameSpec};
use xperience_app::settings;
use xperience_app::shelf::{self, Pick, ShelfOpts};
use xperience_app::update_check::{self, UpdateNotice};
use xperience_domain::{Catalog, Order};
use xperience_platform::{Cabinet, MenuMode, MenuNav, Platform, RaStatus, Screen};

struct Args {
    /// Explicit override; `None` means "look in `core/` at launch, and again
    /// each time a game is picked" (the settings screen may have just
    /// downloaded one).
    core: Option<PathBuf>,
    config: Option<PathBuf>,
    save_dir: PathBuf,
    system_dir: PathBuf,
    notes_dir: PathBuf,
    order: Order,
    #[allow(dead_code)] // o run-ahead do jogo é do processo filho (--runahead)
    runahead: Option<u32>,
    /// Headless: render one settings screen ("main"|"controls") to `--shot`
    /// and exit, instead of starting the shelf (dev/testing).
    debug_settings: Option<String>,
    /// Headless: render the idle screen to `--shot` and exit (dev/testing) —
    /// shows the first-run setup screen when the core or the DAT is missing.
    debug_idle: bool,
    shot: Option<PathBuf>,
    /// Modo autoplay (o pai é este mesmo binário spawmando a si mesmo):
    /// o disco a abrir direto, sem idle/estante, e a arte do disco.
    autoplay_rom: Option<PathBuf>,
    autoplay_cartridge: Option<PathBuf>,
}

fn default_core_path() -> Option<PathBuf> {
    let p = xperience_app::dirs::core_dir().join(core_update::core_file_name());
    p.is_file().then_some(p)
}

fn parse_args() -> Result<Args> {
    let mut core = std::env::var_os("PSX_XPERIENCE_CORE").map(PathBuf::from);
    let mut config = None;
    let mut save_dir = None;
    let mut system_dir = None;
    let mut notes_dir = None;
    let mut order = Order::Shelf;
    let mut runahead = None;
    let mut debug_settings = None;
    let mut autoplay_rom = None;
    let mut autoplay_cartridge = None;
    let mut debug_idle = false;
    let mut shot = None;

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or_else(|| anyhow!("{a} needs a value"));
        match a.as_str() {
            "--core" => core = Some(val()?.into()),
            "--config" => config = Some(val()?.into()),
            "--save-dir" => save_dir = Some(val()?.into()),
            "--system-dir" => system_dir = Some(val()?.into()),
            "--notes-dir" => notes_dir = Some(val()?.into()),
            "--debug-settings" => debug_settings = Some(val()?),
            "--debug-idle-shot" => debug_idle = true,
            "--shot" => shot = Some(val()?.into()),
            "--rom" => {
                autoplay_rom = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--rom needs a path"))?
                        .into(),
                )
            }
            "--cartridge" => {
                autoplay_cartridge = Some(
                    it.next()
                        .ok_or_else(|| anyhow!("--cartridge needs a path"))?
                        .into(),
                )
            }
            "--order" => {
                order = match val()?.as_str() {
                    "name" => Order::Name,
                    "shelf" => Order::Shelf,
                    other => anyhow::bail!("--order wants shelf|name, got {other:?}"),
                }
            }
            "--runahead" => {
                runahead = Some(
                    val()?
                        .parse()
                        .map_err(|_| anyhow!("--runahead wants a number (0 disables)"))?,
                )
            }
            "-h" | "--help" => {
                println!("{HELP}");
                std::process::exit(0);
            }
            other => anyhow::bail!("unexpected argument: {other}"),
        }
    }

    let save_dir = save_dir.unwrap_or_else(xperience_app::dirs::saves_dir);
    // O system directory do core é a BIOS (plano §1.1) — bios/, nunca saves/.
    let system_dir = system_dir.unwrap_or_else(xperience_app::dirs::bios_dir);
    let notes_dir = notes_dir.unwrap_or_else(xperience_app::dirs::notes_dir);
    Ok(Args {
        core,
        config,
        save_dir,
        system_dir,
        notes_dir,
        order,
        runahead,
        debug_settings,
        debug_idle,
        shot,
        autoplay_rom,
        autoplay_cartridge,
    })
}

const HELP: &str = "psx-xperience [--core <lib>] [--config psx-xperience.cfg]\n\
       [--save-dir DIR] [--system-dir DIR] [--notes-dir DIR]\n\
       [--order shelf|name] [--runahead N]\n\
       [--debug-settings main|controls --shot out.bmp]\n\
\n\
Idle (TV off) → selector → game → idle, one process. Mouse/gamepad only —\n\
no keyboard shortcut for anything but gameplay input (D-pad/buttons). The\n\
idle screen's \"Inserir cartucho\" opens the shelf; it's what you land on at\n\
startup, after backing out of the shelf, and after ejecting a game. In a\n\
game, the panel's Power button powers off (saves, TV to snow) and back on\n\
again; Eject only takes once off, back to idle.\n\
\"Configurações\" (idle screen or shelf) opens settings (controls, núcleo\n\
SwanStation, run-ahead, fullscreen, checar atualizações ao abrir) — saved\n\
straight to psx-xperience.cfg. Window-close on the idle screen, or closing a\n\
game window, ends the app — no ceremony there.\n\
\n\
The cabinet's nameplate shows the app's own version and, once a core is\n\
loaded, SwanStation's. Unless turned off in settings, startup also checks\n\
GitHub for a newer release and the buildbot for a fresher SwanStation core —\n\
when either finds something, a green dot lights up in the nameplate next\n\
to that version (silently skipped on any network hiccup, never a hard\n\
failure). First run without the SwanStation core or nointro.dat? The idle\n\
screen opens on a setup page inside the TV, one download button each.\n\
\n\
Portable: roms/, core/, assets/ (cover/logo art, matched by ROM file name),\n\
saves/, notes/, psx-xperience.cfg, library.json all live in one root — next to\n\
this executable on Windows/Linux, ~/Documents/PSX Xperience on macOS.\n\
Drop ROMs into roms/ and go; no --core/$PSX_XPERIENCE_CORE? Use\n\
the settings screen's \"Núcleo\" to download SwanStation automatically, or drop\n\
it into core/ by hand (not included — non-commercial license, see\n\
THIRD-PARTY-NOTICES.md). An optional nointro.dat at the root gives games\n\
their canonical No-Intro name.";

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // O "será atualizado ao reiniciar" (plan revision): um update baixado
    // pela tela do app vale aqui — substitui o binário/bundle antes de
    // qualquer SDL. Melhor-esforço; falha loga e segue com o atual.
    xperience_app::update_check::apply_pending_update();
    let args = parse_args()?;

    for dir in [
        xperience_app::dirs::roms_dir(),
        xperience_app::dirs::core_dir(),
        xperience_app::dirs::assets_dir().join("logo"),
        xperience_app::dirs::assets_dir().join("cover"),
        xperience_app::dirs::assets_dir().join("disc"),
        xperience_app::dirs::assets_dir().join("backcover"),
        args.save_dir.clone(),
        args.notes_dir.clone(),
        xperience_app::dirs::retroachievements_dir(),
        xperience_app::dirs::config_dir(),
    ] {
        let _ = std::fs::create_dir_all(&dir);
    }
    migrate_old_data();

    let mut cfg = Config::load(args.config.as_deref())?;
    if let Some(p) = &cfg.source {
        log::info!("config: {}", p.display());
    }

    // Re-run whenever the player hits "Atualizar" on the shelf (plan
    // revision) — a fresh scan of roms/ merged with the same persisted
    // sidecar, no app restart needed. Discos: título vem da tabela embutida
    // por serial; sem fluxo de DAT no caminho.
    let open_catalog = || {
        Catalog::open(
            &xperience_app::dirs::roms_dir(),
            &xperience_app::dirs::library_path(),
        )
    };
    let mut catalog = open_catalog().with_context(|| "opening the catalog")?;
    log::info!("{} rom(s) in roms/", catalog.counts()?);

    let mut plat = Platform::new().map_err(|e| anyhow!(e.to_string()))?;
    plat.set_pad_map(cfg.padmap.clone());
    // One window for the whole session — shelf and game both draw into it.
    let mut cab = plat
        .create_cabinet("PSX Xperience", 1280, 800, cfg.fullscreen)
        .map_err(|e| anyhow!(e.to_string()))?;
    // DEBUG: autoplay — pula idle e estante, vai reto ao jogo. É o mesmo
    // mecanismo do lançamento por processo (abaixo) exposto para dev.
    if let Some(rom) = args.autoplay_rom.clone().or_else(|| {
        std::env::var("PSX_XPERIENCE_AUTOPLAY")
            .ok()
            .map(PathBuf::from)
    }) {
        let cartridge = args.autoplay_cartridge.clone(); // a arte vai via --cartridge do pai
        let core = xperience_app::dirs::core_dir().join(core_update::core_file_name());
        let spec = GameSpec {
            core,
            rom,
            system_dir: xperience_app::dirs::roms_dir()
                .parent()
                .unwrap()
                .join("bios"),
            save_dir: xperience_app::dirs::saves_dir(),
            notes_dir: xperience_app::dirs::notes_dir(),
            runahead: None,
            shot: None,
            logo: None,
            card1: None,
            card2: None,
            cartridge,
            shot_off: false,
            debug_note_capture: false,
            debug_shot_pause: false,
            debug_shot_modal: None,
            debug_cart_anim: None,
        };
        match run_game(&mut plat, &mut cab, &spec, &cfg) {
            Ok(GameExit::Ejected { .. }) | Ok(GameExit::Quit) | Err(_) => {}
        }
        return Ok(());
    }
    // The ambient static hiss (opt-in, settings "vídeo") — gate applied once
    // here and live on every settings toggle after.
    cab.set_static_hiss(cfg.hiss_on_static);
    // RetroAchievements, uma vez para todas as telas (início, estante, jogo):
    // a logo oficial (favicon embutido) do badge e o badge em si — conta
    // configurada = "RA ATIVADO" no queixo do que for que a tela esteja na TV.
    if let Ok(icon) = image::load_from_memory(xperience_app::RA_ICON_PNG) {
        let icon = icon.to_rgba8();
        cab.set_image(
            xperience_platform::RA_LOGO_IMG,
            icon.width(),
            icon.height(),
            icon.as_raw(),
        );
    }
    // O RA exige a conta conectada (senha na tela de configurações): sem o
    // token Connect não há lógica das conquistas nem envio — só metadados.
    // Reavaliado a cada volta das configurações: o login acontece com o app
    // aberto, e selo/estante têm que religar na hora (sem reiniciar).
    let mut shelf_opts = ShelfOpts {
        order: args.order,
        max_frames: None,
        shot: None,
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    refresh_ra(&cfg, &mut cab, &mut shelf_opts);

    // Headless self-check: render one settings screen and exit.
    if let (Some(screen), Some(path)) = (&args.debug_settings, &args.shot) {
        settings::capture_preview(&mut cab, &cfg, screen, path)?;
        log::info!("wrote {} (settings preview: {screen})", path.display());
        return Ok(());
    }
    if let (true, Some(path)) = (args.debug_idle, &args.shot) {
        let core_path = args.core.clone().or_else(default_core_path);
        cab.set_nameplate(&core_update::nameplate_text(core_path.as_deref()));
        idle::capture_preview(
            &mut cab,
            idle::RESTING_STATIC,
            core_path.is_some(),
            xperience_app::dat_update::dat_installed(),
            path,
        )?;
        log::info!("wrote {} (idle preview)", path.display());
        return Ok(());
    }

    let mut idle_static = idle::RESTING_STATIC;
    let mut core_path = args.core.clone().or_else(default_core_path);
    cab.set_nameplate(&core_update::nameplate_text(core_path.as_deref()));

    // Startup update checks (plan revision: "verificar se tem update... e se
    // o SwanStation esta atualizado" — later revision: not a modal anymore, the
    // result lights the green dots in the nameplate), opt-out in settings —
    // network calls, so they run on their own thread; `idle::run` drains
    // the result whenever it's ready.
    let mut update_rx: Option<Receiver<UpdateNotice>> = None;
    if cfg.check_updates_on_start {
        let (tx, rx) = mpsc::channel();
        let core_dir = xperience_app::dirs::core_dir();
        let app_version = env!("CARGO_PKG_VERSION").to_string();
        std::thread::spawn(move || update_check::check(core_dir, app_version, tx));
        update_rx = Some(rx);
    }

    // Dev/CI: pula o idle e abre a estante na primeira iteração.
    let mut skip_idle_once = std::env::var_os("PSX_XPERIENCE_AUTO_SHELF").is_some();
    'app: loop {
        // O código Konami (↑↑↓↓←→←→BA) só é escutado na tela inicial — o
        // watcher liga aqui e desliga ao sair dela, para que navegar com
        // d-pad na estante/configurações nunca engula um Back/Confirm nem
        // complete a sequência por acidente (plan revision: devmode ultra
        // secreto). Em memória, nunca persistido.
        plat.set_konami_watch(true);
        let exit = if skip_idle_once {
            skip_idle_once = false;
            IdleExit::OpenShelf
        } else {
            idle::run(
                &mut plat,
                &mut cab,
                idle_static,
                &mut update_rx,
                core_path.is_some(),
                xperience_app::dat_update::dat_installed(),
            )?
        };
        plat.set_konami_watch(false);
        // The idle screen's own "Baixar núcleo" button may have just
        // installed one — re-resolve (cheap when core/ is unchanged) and
        // refresh the nameplate either way.
        core_path = args.core.clone().or_else(default_core_path);
        cab.set_nameplate(&core_update::nameplate_text(core_path.as_deref()));
        match exit {
            IdleExit::Quit => break 'app,
            IdleExit::OpenShelf => shelf_opts.fade_in = Some(idle_static),
            IdleExit::OpenDev => {
                // O menu do devmode (plan revision: "por enquanto criar menu
                // em branco apenas com o botão voltar") — `true` aqui é o
                // fechamento da janela dentro dele, que encerra o app.
                if xperience_app::devmenu::run(&mut plat, &mut cab)? {
                    break 'app;
                }
                continue 'app;
            }
            IdleExit::OpenSettings => {
                if settings::run(&mut plat, &mut cab, &mut cfg)? {
                    break 'app;
                }
                refresh_ra(&cfg, &mut cab, &mut shelf_opts);
                // A core download may have just finished.
                core_path = args.core.clone().or_else(default_core_path);
                cab.set_nameplate(&core_update::nameplate_text(core_path.as_deref()));
                continue 'app;
            }
        }

        'shelf: loop {
            let (rom, logo, cartridge) =
                match shelf::run(&mut plat, &mut cab, &catalog, &shelf_opts)? {
                    Pick::Quit => break 'app,
                    Pick::Back => {
                        idle_static = idle::RESTING_STATIC;
                        break 'shelf;
                    }
                    Pick::Settings => {
                        let quit = settings::run(&mut plat, &mut cab, &mut cfg)?;
                        if quit {
                            break 'app;
                        }
                        refresh_ra(&cfg, &mut cab, &mut shelf_opts);
                        // A core download may have just finished.
                        core_path = args.core.clone().or_else(default_core_path);
                        cab.set_nameplate(&core_update::nameplate_text(core_path.as_deref()));
                        continue;
                    }
                    // "Histórico" (plan revision) — a `Pick::Back` from in
                    // there means "back to the shelf", not all the way to
                    // idle, so it's handled here rather than falling through
                    // to the `Pick::Back` arm above.
                    Pick::History => match shelf::run_history(&mut plat, &mut cab, &catalog)? {
                        Pick::Play {
                            rom,
                            wheel,
                            cartridge,
                        } => (rom, wheel, cartridge),
                        Pick::Settings => {
                            let quit = settings::run(&mut plat, &mut cab, &mut cfg)?;
                            if quit {
                                break 'app;
                            }
                            refresh_ra(&cfg, &mut cab, &mut shelf_opts);
                            core_path = args.core.clone().or_else(default_core_path);
                            cab.set_nameplate(&core_update::nameplate_text(core_path.as_deref()));
                            continue;
                        }
                        Pick::Quit => break 'app,
                        _ => continue,
                    },
                    Pick::Play {
                        rom,
                        wheel,
                        cartridge,
                    } => (rom, wheel, cartridge),
                    // "Atualizar" (plan revision) — rescan roms/ and come
                    // straight back to the shelf with the fresh catalog.
                    Pick::Refresh => {
                        catalog = open_catalog().with_context(|| "refreshing the catalog")?;
                        log::info!("{} rom(s) in roms/", catalog.counts()?);
                        continue;
                    }
                };
            shelf_opts.fade_in = None; // consumed

            // A Metal guarda estado do 2D da estante que corrompe a textura
            // do jogo (o "quadrado colorido"): a troca recria o canvas —
            // a TV trocando de entrada. O mesmo na volta, no Ejected.
            drop(cab);
            cab = plat
                .create_cabinet("PSX Xperience", 1280, 800, cfg.fullscreen)
                .map_err(|e| anyhow!(e.to_string()))?;

            let Some(core) = &core_path else {
                if no_core_screen(&mut plat, &mut cab)? {
                    break 'app;
                }
                continue;
            };
            // O jogo roda NESTE processo e nesta janela — o mesmo caminho do
            // SNES Xperience. (O bug do "quadrado colorido" nunca foi da
            // Metal: o system_dir default apontava saves/, a BIOS nunca era
            // encontrada e o boot não saía do lugar. Corrigido no parse_args.)
            let spec = GameSpec {
                core: core.clone(),
                rom,
                system_dir: args.system_dir.clone(),
                save_dir: args.save_dir.clone(),
                notes_dir: args.notes_dir.clone(),
                runahead: args.runahead,
                shot: None,
                logo,
                card1: None,
                card2: None,
                cartridge,
                shot_off: false,
                debug_note_capture: false,
                debug_shot_pause: false,
                debug_shot_modal: None,
                debug_cart_anim: None,
            };
            match run_game(&mut plat, &mut cab, &spec, &cfg)? {
                GameExit::Ejected { static_level } => {
                    idle_static = static_level;
                    break 'shelf;
                }
                GameExit::Quit => break 'app,
            }
        }
    }

    log::info!("bye");
    Ok(())
}

/// A game was picked but no core is available yet — points at the settings
/// screen instead of crashing the whole app (which used to happen at
/// startup, before there was anything to download a core *from*). Returns
/// `true` if the whole app should quit.
/// (Re)avalia o estado do RA — selo no queixo e credenciais que a estante
/// carrega. Chamado no arranque e a cada volta das configurações: o login
/// por senha acontece com o app aberto, e sem isto o token novo só seria
/// visto no próximo arranque (o RA "sumia" até reiniciar).
fn refresh_ra(cfg: &Config, cab: &mut Cabinet, shelf_opts: &mut ShelfOpts) {
    let on = !cfg.ra_user.is_empty() && cfg.ra_connect.is_some();
    cab.set_ra_status(on.then_some(RaStatus {
        hardcore: cfg.ra_hardcore,
    }));
    shelf_opts.ra = on.then(|| {
        (
            cfg.ra_user.clone(),
            cfg.ra_token.clone(),
            cfg.ra_connect.clone().unwrap_or_default(),
        )
    });
}

fn no_core_screen(plat: &mut Platform, cab: &mut Cabinet) -> Result<bool> {
    loop {
        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(true);
        }
        if m.nav
            .iter()
            .any(|n| matches!(n, MenuNav::Confirm | MenuNav::Back))
        {
            return Ok(false);
        }
        let render = |d: &mut Screen| {
            d.text(40, 40, 2, (232, 232, 232), "núcleo não encontrado");
            d.text_wrapped(
                40,
                90,
                700,
                1,
                (150, 150, 158),
                "para jogar é necessário o núcleo SwanStation: volte à tela inicial e \
                 clique em \"Baixar núcleo SwanStation\", baixe pelo menu de configurações \
                 ou coloque o arquivo em core/ à mão.",
            );
            d.text(
                40,
                d.size().1 as i32 - 40,
                1,
                (150, 150, 158),
                "enter ou esc: voltar pra estante",
            );
        };
        cab.frame_2d((18, 18, 20), render);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

/// One-time, best-effort copy of real play progress (saves/notebooks) from
/// the pre-portable `~/.local/share/psx-xperience` location, if the new
/// folders are still empty and the old ones exist. The catalog's play-count
/// history is deliberately NOT migrated (fresh start) — but a save state or
/// SRAM file is actual game progress, worth not losing silently just because
/// the app changed where it looks. macOS/Linux only (old installs on native
/// Windows used `%APPDATA%`, not covered here — lower value to chase).
fn migrate_old_data() {
    if let Some(home) = std::env::var_os("HOME") {
        let old_base = PathBuf::from(home).join(".local/share/psx-xperience");
        migrate_pairs([
            (old_base.join("saves"), xperience_app::dirs::saves_dir()),
            (old_base.join("notes"), xperience_app::dirs::notes_dir()),
        ]);
    }
    migrate_macos_bundle_sibling();
    migrate_ra_out_of_saves();
    migrate_root_files_into_config();
}

/// Plan revision: "criar pasta config e colocar o cfg, o dat, library e o
/// hash" — move `psx-xperience.cfg`, `nointro.dat`, `library.json` e
/// `hashcache.json` da raiz do app para `config/`. Idempotente: só move
/// quando o destino não existe (um `--config` explícito nunca é tocado —
/// a migração só trata os nomes padrão na raiz). Roda antes do
/// `Config::load` e da abertura do catálogo, que leem os caminhos novos.
fn migrate_root_files_into_config() {
    let config = xperience_app::dirs::config_dir();
    for item in [
        "psx-xperience.cfg",
        "nointro.dat",
        "library.json",
        "hashcache.json",
    ] {
        let src = xperience_app::dirs::app_root().join(item);
        let dst = config.join(item);
        if !src.is_file() || dst.exists() {
            continue;
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => log::info!("migrado: {item} -> config/{item}"),
            Err(e) => log::warn!("migrando {item} para config/: {e}"),
        }
    }
}

/// Plan revision: "dados do retroachievements da pasta save devem ficar em
/// uma pasta 'retroachievements' na raiz - pasta save apenas são os saves
/// dos jogos" — move os quatro itens do RA (`ra-cache/`, `ra-earned/`,
/// `ra-progress/`, `ra-pending.jsonl`) de `saves/` para
/// `retroachievements/`. Idempotente: só move quando o destino não existe
/// (dois lados vivos = fica como está; nada do RA é sobrescrito).
fn migrate_ra_out_of_saves() {
    let ra = xperience_app::dirs::retroachievements_dir();
    let saves = xperience_app::dirs::saves_dir();
    for item in ["ra-cache", "ra-earned", "ra-progress", "ra-pending.jsonl"] {
        let src = saves.join(item);
        let dst = ra.join(item);
        if !src.exists() || dst.exists() {
            continue;
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => log::info!("migrado: saves/{item} -> retroachievements/{item}"),
            Err(e) => log::warn!("migrando saves/{item}: {e}"),
        }
    }
}

/// macOS only, and only relevant for the brief window before `dirs::app_root`
/// moved to `~/Documents/PSX Xperience`: the very first 0.4.0 DMG builds
/// created `roms/`/`core/`/`assets/`/`saves/`/`notes/` next to the `.app`
/// (typically inside `/Applications`, not writable/expected for user data on
/// this platform). If that old layout exists next to the running `.app` and
/// the new Documents folders are still empty, copy it over once.
#[cfg(target_os = "macos")]
fn migrate_macos_bundle_sibling() {
    use std::ffi::OsStr;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let dir = exe.parent().unwrap_or_else(|| Path::new("."));
    let old_root = (|| {
        if dir.file_name() != Some(OsStr::new("MacOS")) {
            return None;
        }
        let contents = dir.parent()?;
        if contents.file_name() != Some(OsStr::new("Contents")) {
            return None;
        }
        let bundle = contents.parent()?;
        if bundle.extension() != Some(OsStr::new("app")) {
            return None;
        }
        bundle.parent().map(Path::to_path_buf)
    })();
    let Some(old_root) = old_root else {
        return; // dev build, not a packaged .app — nothing to migrate
    };
    migrate_pairs([
        (old_root.join("roms"), xperience_app::dirs::roms_dir()),
        (old_root.join("core"), xperience_app::dirs::core_dir()),
        (old_root.join("assets"), xperience_app::dirs::assets_dir()),
        (old_root.join("saves"), xperience_app::dirs::saves_dir()),
        (old_root.join("notes"), xperience_app::dirs::notes_dir()),
    ]);
}

#[cfg(not(target_os = "macos"))]
fn migrate_macos_bundle_sibling() {}

fn migrate_pairs<const N: usize>(pairs: [(PathBuf, PathBuf); N]) {
    for (old_dir, new_dir) in pairs {
        if !old_dir.is_dir() {
            continue;
        }
        let new_has_data = std::fs::read_dir(&new_dir)
            .map(|mut it| it.next().is_some())
            .unwrap_or(false);
        if new_has_data {
            continue;
        }
        match copy_dir_all(&old_dir, &new_dir) {
            Ok(()) => log::info!("migrated {} -> {}", old_dir.display(), new_dir.display()),
            Err(e) => log::warn!("migrating {}: {e}", old_dir.display()),
        }
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}
