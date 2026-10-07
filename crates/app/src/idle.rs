//! The idle/root screen: TV off, no game loaded — the app's home state, and
//! also literally the "cartridge ejected" screen (plan revision: one screen,
//! not two — startup, backing out of the shelf, and ejecting a game all land
//! here). In place of a game's logo/cartridge art there's the console's own
//! brand logo (`assets/console.png`, optional) and an "Inserir cartucho"
//! button; "Configurações" sits in the panel's footer. This is the outermost
//! screen now: a gamepad's Back button here, same as closing the window,
//! ends the app (plan revision: mouse/gamepad only, no keyboard shortcuts).
//!
//! First-run setup (plan revision: "ao abrir pela primeira vez e não ter o
//! DAT, o PCSX Rearmed — abrir dentro da tv o aviso para baixar os dois arquivos,
//! com o botão para baixar"): when either the PCSX Rearmed core or the
//! `nointro.dat` is missing, the TV shows a full setup screen with a
//! download button for each, before the usual idle panel. Updates (plan
//! revision: "quando tiver update do app ou do PCSX Rearmed não mostrar mais a
//! tela cheia e sim um icone verde no nameplate do lado de cada um") are no
//! longer a modal — the startup check's result lights a green dot in the
//! chin, next to the app's and the core's version lines.

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use std::sync::mpsc;

use anyhow::Result;
use polystnx_platform::{Cabinet, MenuMode, MenuNav, PanelButton, Platform, Screen, UpdateArrow};

use crate::core_update::{self, CoreUpdateMsg};
use crate::update_check::UpdateNotice;

const SETUP_BG: (u8, u8, u8) = (18, 18, 20);
const SETUP_TEXT: (u8, u8, u8) = (232, 232, 232);
const SETUP_DIM: (u8, u8, u8) = (150, 150, 158);
const SETUP_GREEN: (u8, u8, u8) = (60, 230, 70);

/// The screen's resting static level — startup and "Esc on the shelf" have no
/// prior game to inherit a level from, so they use this (same dim hiss the
/// console settles on after powering off, `runner::OFF_STATIC_LEVEL`).
pub const RESTING_STATIC: f32 = crate::runner::OFF_STATIC_LEVEL;

/// What the player did on the idle screen.
pub enum IdleExit {
    /// Window closed / Cmd-Q, or Esc — this is the root screen, so both end
    /// the app.
    Quit,
    /// "Inserir cartucho" confirmed (Enter/gamepad) or clicked.
    OpenShelf,
    /// "Configurações" clicked — the only way into settings from here (no
    /// keyboard shortcut, mouse/gamepad only).
    OpenSettings,
    /// "Dev" clicked — the dev-mode button that only exists once the Konami
    /// code has been entered on this screen (plan revision: "ultra secreto").
    OpenDev,
    /// "Ligar sem disco" — boot direto na BIOS do console.
    BootBios,
}

/// One clickable region of the setup screen, in output coordinates —
/// `(x, y, w, h)` in the same space `Cabinet::window_to_output` maps clicks
/// into, so `hit_setup_button` and the draw closure stay in sync by both
/// coming from `setup_rects`.
type RectT = (i32, i32, u32, u32);

/// What the player can click on the setup screen.
enum SetupButton {
    Core,
    Dat,
    Bios,
    Continue,
}

/// Button/status geometry for the setup screen, derived from the output
/// size — the single source both `draw_setup` and `hit_setup_button` use.
fn setup_rects(w: u32, h: u32) -> (RectT, RectT, RectT, RectT) {
    let bw = 640.min(w.saturating_sub(160)).max(280);
    let bh = 64u32;
    let x = (w as i32 - bw as i32) / 2;
    let core_y = h as i32 * 22 / 100;
    let dat_y = core_y + bh as i32 + 40;
    let bios_y = dat_y + bh as i32 + 40;
    let cw = 280u32;
    let ch = 56u32;
    let cont = (
        (w as i32 - cw as i32) / 2,
        h as i32 - ch as i32 - 60,
        cw,
        ch,
    );
    (
        (x, core_y, bw, bh),
        (x, dat_y, bw, bh),
        (x, bios_y, bw, bh),
        cont,
    )
}

fn hit_setup_button(w: u32, h: u32, x: i32, y: i32) -> Option<SetupButton> {
    let (core, dat, bios, cont) = setup_rects(w, h);
    let inside =
        |r: &RectT| x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32;
    if inside(&core) {
        Some(SetupButton::Core)
    } else if inside(&dat) {
        Some(SetupButton::Dat)
    } else if inside(&bios) {
        Some(SetupButton::Bios)
    } else if inside(&cont) {
        Some(SetupButton::Continue)
    } else {
        None
    }
}

/// Download state for one setup button — `label` carries the live progress
/// /failure text, `rx` is `Some` while the worker thread runs.
struct Download {
    label: String,
    rx: Option<mpsc::Receiver<CoreUpdateMsg>>,
    done: bool,
    /// The last run failed (`label` tem o motivo) — o idle usa para manter
    /// o status visível e a seta verde acesa para tentar de novo.
    failed: bool,
}

impl Download {
    fn idle(label: &str) -> Self {
        Self {
            label: label.to_string(),
            rx: None,
            done: false,
            failed: false,
        }
    }

    /// Drain the worker's channel, if one is wired up — the same pump
    /// `idle::run` used to inline for the core alone.
    fn pump(&mut self) {
        let Some(rx) = &self.rx else { return };
        match rx.try_recv() {
            Ok(CoreUpdateMsg::Progress { downloaded, total }) => {
                let mb = downloaded as f64 / 1_048_576.0;
                self.label = match total {
                    Some(t) => format!("baixando... {mb:.1}/{:.1} MB", t as f64 / 1_048_576.0),
                    None => format!("baixando... {mb:.1} MB"),
                };
            }
            Ok(CoreUpdateMsg::Done) => {
                self.done = true;
                self.rx = None;
                self.failed = false;
                self.label = "instalado".to_string();
            }
            Ok(CoreUpdateMsg::Failed(e)) => {
                self.label = format!("falha ao atualizar o núcleo: {e}");
                self.rx = None;
                self.failed = true;
            }
            Err(TryRecvError::Disconnected) => self.rx = None,
            Err(TryRecvError::Empty) => {}
        }
    }
}

/// Run the idle screen until the player opens the shelf or quits.
/// `static_level` is the signal-off snow to show — the steady dim hiss at
/// startup or after Esc on the shelf, or whatever level a just-ejected game
/// settled on (already steady by the time Eject fires, so no extra fade is
/// needed here either way). `notice_rx` is the startup update-check's
/// channel — `None` once the check has reported (its result went straight
/// to the nameplate's green dots) or found nothing.
pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    static_level: f32,
    notice_rx: &mut Option<Receiver<UpdateNotice>>,
    core_installed: bool,
    dat_installed: bool,
    bios_installed: bool,
) -> Result<IdleExit> {
    // Whatever game was loaded before (if any) is gone now — without this the
    // panel keeps showing its stale logo/commands instead of the "Inserir
    // cartucho" button, and there'd be nothing to click.
    cab.clear_panel();
    cab.set_close_button(true);
    crate::console_art::load_brand_images(cab);
    // The idle screen is where this app spends most of its life — pace it
    // (it used to spin unthrottled, redrawing 60fps+ of static forever).
    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;
    let mut frames_log = 0u64;

    // First-run setup (plan revision: see module docs): the TV shows the
    // two download buttons until the player hits "continuar". After that
    // the plain idle panel takes over (with the old core prompt if the
    // core is still missing).
    let mut setup = core_missing_at_start(core_installed, dat_installed, bios_installed);
    let mut core = Download::idle("baixar núcleo PCSX Rearmed");
    if core_installed {
        core.done = true;
        core.label = "instalado".to_string();
    }
    let mut dat = Download::idle("baixar nointro.dat");
    // A BIOS: o label do botão na tela de setup ("instalada" quando o
    // any_installed acha uma .bin utilizável em bios/).
    let mut bios_installed = bios_installed;
    let mut bios_label = if bios_installed {
        "instalada".to_string()
    } else {
        "colocar a BIOS (atualizar)".to_string()
    };
    if dat_installed {
        dat.done = true;
        dat.label = "instalado".to_string();
    }
    let mut core_missing = !core_installed;
    // Feedback do download do núcleo na seta verde (plan revision: "cliquei
    // no icone verde de update do nucleo mas nao abriu nada"): a confirmação
    // fica alguns segundos no painel depois que instala.
    let mut core_ok_until: Option<Instant> = None;
    // Updates (plan revision: "ao clicar no icone verde de atualizar"):
    // o notice é GUARDADO (o changelog/asset o leitor da tela usa), a
    // aba de update do app abre a tela com changelog + botão, e o core
    // baixa na hora. `core_stale` controla a seta até o download valer.
    let mut app_update: Option<crate::update_check::AppUpdate> = None;
    let mut core_stale = false;
    let mut update_view: Option<crate::update_check::AppUpdate> = None;
    let mut update_rx: Option<Receiver<Result<String, String>>> = None;
    let mut update_progress_rx: Option<Receiver<u32>> = None;
    let mut update_pct: Option<u32> = None;
    let mut update_status = String::new();
    let mut update_rects: (UpdateBtn, UpdateBtn) = ((0, 0, 1, 1), (0, 0, 1, 1));

    loop {
        let core_was_done = core.done;
        core.pump();
        dat.pump();
        // A core download just finished (setup screen or panel button) —
        // rebuild the nameplate right away, so the PCSX Rearmed line appears
        // without waiting for the player to leave this screen.
        if core.done && !core_was_done {
            let p = core_update::default_core_path();
            cab.set_nameplate(&core_update::nameplate_text(p.as_deref()));
            // O app reconhece a nova versão na hora — e a seta do core
            // apaga (o meta do install já vale para o próximo arranque).
            if core_stale {
                core_stale = false;
                cab.set_nameplate_updates(app_update.is_some(), false);
            }
            core_ok_until = Some(Instant::now() + Duration::from_secs(4));
        }
        if core.rx.is_none() && core.done {
            core_missing = false;
        }
        if let Some(rx) = notice_rx.as_ref() {
            match rx.try_recv() {
                // Updates aren't a modal anymore (plan revision) — the
                // result just lights the green dots in the nameplate: line
                // 0 for the app, line 1 for the core.
                Ok(n) => {
                    cab.set_nameplate_updates(n.app_update.is_some(), n.core_stale);
                    app_update = n.app_update;
                    core_stale = n.core_stale;
                    *notice_rx = None;
                }
                Err(TryRecvError::Disconnected) => *notice_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        // Download do update do app em andamento: percentual vivo e o
        // final ("será atualizado ao reiniciar") no rótulo da tela.
        while let Some(rx) = update_progress_rx.take() {
            match rx.try_recv() {
                Ok(pct) => {
                    update_pct = Some(pct);
                    update_status = format!("baixando... {pct}%");
                    update_progress_rx = Some(rx);
                    break;
                }
                Err(TryRecvError::Empty) => {
                    update_progress_rx = Some(rx);
                    break;
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }
        if let Some(rx) = &update_rx {
            match rx.try_recv() {
                Ok(Ok(file)) => {
                    update_status = format!("{file} baixado — será atualizado ao reiniciar o app");
                    update_pct = None;
                    update_rx = None;
                }
                Ok(Err(e)) => {
                    update_status = format!("falhou: {e}");
                    update_rx = None;
                }
                Err(TryRecvError::Disconnected) => update_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        let m = plat.poll_menu(MenuMode::Nav);
        frames_log += 1;
        if m.quit {
            return Ok(IdleExit::Quit);
        }
        // O código Konami (↑↑↓↓←→←→BA — teclado ou controle) completa aqui:
        // religa o devmode da sessão e o painel passa a mostrar o botão "Dev"
        // acima do "Configurações" (plan revision: "ultra secreto"). Nada na
        // tela denuncia a escuta — sem flash, sem som; o log só sai no
        // stderr de quem rodou pelo terminal.
        if m.konami {
            cab.set_dev_mode(true);
            log::info!("devmode: on");
        }

        if setup {
            if frames_log.is_multiple_of(120) {
                log::info!("setup ativo (frames {frames_log})");
            }
            // O MESMO (w, h) do draw_setup: o buffer do paint_shelf
            // (canvas − painel − bezel), não o screen_size (canvas cheio).
            // Em ultrawide a diferença era ~230px e o "continuar" morria.
            let (w, h) = cab.shelf_buffer_size();
            let mut dismiss = false;
            if let Some((x, y)) = m.click {
                // The close/minimize pair lives in cabinet-canvas space; the
                // setup buttons in the screen buffer's own space (the screen
                // is inset by the bezel) — each with its own mapping.
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
                if cab.hit_minimize_button(ox, oy) {
                    cab.minimize();
                    continue;
                }
                // O setup é desenhado no buffer 2D e compostp pelo CRT mesh
                // (barril) — o botão VISÍVEL perto do fundo da TV está
                // deslocado do buffer. O hit_screen_point DES-destorce o
                // clique de volta ao buffer (o mesmo que os tiles da
                // estante usam).
                let Some((sx, sy)) = cab.hit_screen_point(ox, oy) else {
                    continue;
                };
                log::info!("setup: clique em janela ({x},{y}) → buffer ({sx},{sy})");
                match hit_setup_button(w, h, sx, sy) {
                    Some(SetupButton::Core) if core.rx.is_none() && !core.done => {
                        if let Some(url) = core_update::core_download_url() {
                            let (tx, rx) = mpsc::channel();
                            let dest = crate::dirs::core_dir();
                            std::thread::spawn(move || {
                                core_update::download_and_install(url, &dest, &tx)
                            });
                            core.rx = Some(rx);
                            core.failed = false;
                        } else {
                            core.label = "sem build automática - use configurações".to_string();
                        }
                    }
                    Some(SetupButton::Dat) if dat.rx.is_none() && !dat.done => {
                        let (tx, rx) = mpsc::channel();
                        let dest = crate::dat_update::dat_path();
                        std::thread::spawn(move || {
                            crate::dat_update::download_and_install(&dest, &tx)
                        });
                        dat.rx = Some(rx);
                    }
                    // BIOS: "atualizar" re-varre a pasta bios/ (o usuário
                    // pode ter soltado o SCPH*.BIN lá agora) — o continuar
                    // só libera quando houver uma BIOS utilizável.
                    Some(SetupButton::Bios) => {
                        bios_installed = crate::bios::any_installed(&crate::dirs::bios_dir());
                        bios_label = if bios_installed {
                            "instalada".to_string()
                        } else {
                            "sem BIOS em bios/ (atualizar)".to_string()
                        };
                        if bios_installed {
                            log::info!("setup: BIOS detectada — continuar liberado");
                        }
                    }
                    Some(SetupButton::Continue) => {
                        if bios_installed {
                            log::info!("setup: continuar clicado — dismiss");
                            dismiss = true;
                        } else {
                            log::info!("setup: continuar bloqueado sem BIOS");
                            cab.push_osd(
                                &[
                                    "SEM A BIOS NÃO DÁ PARA CONTINUAR",
                                    "COLOQUE O SCPH*.BIN EM bios/ E CLIQUE ATUALIZAR",
                                ],
                                None,
                                Duration::from_secs(4),
                            );
                        }
                    }
                    _ => {}
                }
            }
            // Confirm (Enter/gamepad A) on the setup screen means
            // "continuar" — Back still quits.
            for nav in &m.nav {
                match nav {
                    MenuNav::Confirm => {
                        if bios_installed {
                            dismiss = true;
                        } else {
                            cab.push_osd(
                                &["SEM A BIOS NÃO DÁ PARA CONTINUAR"],
                                None,
                                Duration::from_secs(3),
                            );
                        }
                    }
                    MenuNav::Back => return Ok(IdleExit::Quit),
                    _ => {}
                }
            }
            if dismiss {
                setup = false;
            } else {
                let core_label = core.label.clone();
                let dat_label = dat.label.clone();
                let core_done = core.done;
                let dat_done = dat.done;
                let core_busy = core.rx.is_some();
                let dat_busy = dat.rx.is_some();
                let render = |d: &mut Screen| {
                    draw_setup(
                        d,
                        &core_label,
                        core_done,
                        core_busy,
                        &dat_label,
                        dat_done,
                        dat_busy,
                        &bios_label,
                        bios_installed,
                    )
                };
                cab.frame_idle_2d(SETUP_BG, render);
                crate::runner::pace_frame(&mut next, frame);
                continue;
            }
            // Just dismissed: the same click/Confirm that dismissed the
            // setup screen must not also land on the idle panel below —
            // give it a frame of its own.
            cab.set_idle_core_prompt(core_missing.then_some("Baixar núcleo PCSX Rearmed"));
            crate::runner::pace_frame(&mut next, frame);
            continue;
        }

        cab.set_idle_core_prompt(core_missing.then_some("Baixar núcleo PCSX Rearmed"));

        // Tela de update do app aberta: dona do tubo (changelog + botão).
        if update_view.is_some() {
            let (upt_tag, upt_body, asset_url) = {
                let u = update_view.as_ref().expect("checado acima");
                (u.tag.clone(), u.changelog.clone(), u.asset_url.clone())
            };
            for nav in &m.nav {
                if matches!(nav, MenuNav::Back) && update_rx.is_none() {
                    update_view = None;
                }
            }
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
                if let Some((sx, sy)) = cab.hit_screen_point(ox, oy) {
                    let (btn, voltar) = update_rects;
                    if in_update_rect(sx, sy, voltar) {
                        update_view = None;
                    } else if in_update_rect(sx, sy, btn)
                        && update_rx.is_none()
                        && asset_url.is_some()
                    {
                        // "Atualizar": baixa o asset da plataforma para o
                        // diretório de updates — o arranque seguinte aplica
                        // (plan revision: "avisar que sera atualizado ao
                        // reiniciar").
                        let (tx, rx) = mpsc::channel();
                        let (ptx, prx) = mpsc::channel();
                        let url = asset_url.unwrap_or_default();
                        let dest = crate::dirs::update_dir();
                        std::thread::spawn(move || {
                            let result = crate::update_check::download_asset(&url, &dest, &ptx)
                                .map(|p| {
                                    p.file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .into_owned()
                                });
                            let _ = tx.send(result);
                        });
                        update_rx = Some(rx);
                        update_progress_rx = Some(prx);
                        update_pct = None;
                        update_status = "baixando...".to_string();
                    }
                }
            }
            let busy = update_rx.is_some();
            let status = update_status.clone();
            let render = |d: &mut Screen| {
                update_rects = draw_update_view(d, &upt_tag, &upt_body, &status, busy, update_pct);
            };
            cab.frame_idle_2d(SETUP_BG, render);
            crate::runner::pace_frame(&mut next, frame);
            continue;
        }

        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return Ok(IdleExit::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                continue;
            }
            // Setas verdes do nameplate (plan revision): app abre a tela
            // de changelog; core baixa e instala na hora.
            match cab.hit_update_arrow(ox, oy) {
                Some(UpdateArrow::App) if app_update.is_some() => {
                    update_view = app_update.clone();
                    update_status.clear();
                }
                Some(UpdateArrow::Core) if core_stale && core.rx.is_none() => {
                    match core_update::core_download_url() {
                        Some(url) => {
                            let (tx, rx) = mpsc::channel();
                            let dest = crate::dirs::core_dir();
                            std::thread::spawn(move || {
                                core_update::download_and_install(url, &dest, &tx)
                            });
                            core.rx = Some(rx);
                            core.done = false;
                            core.failed = false;
                            core.label = "baixando...".to_string();
                        }
                        None => core.label = "sem build automática - use configurações".into(),
                    }
                }
                _ => {}
            }
            match cab.hit_panel_button(ox, oy) {
                // Sem o PCSX Rearmed instalado (plan revision: "só desative o
                // botão de config e de inserir cartucho se o usuário não
                // baixou o PCSX Rearmed") — os dois ficam inertes; o painel
                // oferece o download do core no lugar.
                Some(PanelButton::Insert) if !core_missing => return Ok(IdleExit::OpenShelf),
                Some(PanelButton::BootBios) if !core_missing => return Ok(IdleExit::BootBios),
                Some(PanelButton::Settings) if !core_missing => return Ok(IdleExit::OpenSettings),
                // O botão só existe com o devmode ligado (só o painel o
                // desenha), mas o gate fica explícito: sem devmode, cliques
                // fantasmas não abrem nada.
                Some(PanelButton::Dev) if cab.dev_mode() => return Ok(IdleExit::OpenDev),
                Some(PanelButton::CoreDownload) if core.rx.is_none() => {
                    match core_update::core_download_url() {
                        Some(url) => {
                            let (tx, rx) = mpsc::channel();
                            let dest = crate::dirs::core_dir();
                            std::thread::spawn(move || {
                                core_update::download_and_install(url, &dest, &tx)
                            });
                            core.rx = Some(rx);
                            core.done = false;
                            core.failed = false;
                            core.label = "baixando...".to_string();
                        }
                        None => {
                            core.label = "Sem build automática - use configurações".to_string();
                        }
                    }
                }
                // Already downloading? The click is ignored until the
                // worker reports Done/Failed.
                Some(PanelButton::CoreDownload) => {}
                _ => {}
            }
        }
        for nav in m.nav {
            match nav {
                // Same gate as the panel buttons above: without the core
                // there's nowhere to go but out — Confirm is inert, Back
                // still quits.
                MenuNav::Confirm if !core_missing => return Ok(IdleExit::OpenShelf),
                MenuNav::Back => return Ok(IdleExit::Quit),
                _ => {}
            }
        }
        // A seta verde do núcleo não abre tela nenhuma: o download roda em
        // segundo plano, e o painel mostra o andamento (plan revision:
        // "cliquei no icone verde de update do nucleo mas nao abriu nada").
        let core_status = if core.rx.is_some() || core.failed {
            Some(core.label.clone())
        } else if let Some(until) = core_ok_until {
            (Instant::now() < until).then(|| "núcleo atualizado!".to_string())
        } else {
            None
        };
        cab.set_core_status(core_status.as_deref());
        cab.present_static(static_level);
        crate::runner::pace_frame(&mut next, frame);
    }
}

/// `(x, y, w, h)` em coordenadas de tela — rect de botão da tela de update.
type UpdateBtn = (i32, i32, u32, u32);

const UPDATE_MARGIN: i32 = 40;

fn in_update_rect(x: i32, y: i32, r: UpdateBtn) -> bool {
    x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32
}

/// A tela de update do app (plan revision: "quando for update do app
/// mostrar o changelog e o botão de atualizar, e dai avisar que sera
/// atualizado ao reiniciar") — título, changelog do release, status do
/// download e os botões "atualizar"/"voltar". Devolve os rects.
fn draw_update_view(
    d: &mut Screen,
    tag: &str,
    changelog: &str,
    status: &str,
    busy: bool,
    _pct: Option<u32>,
) -> (UpdateBtn, UpdateBtn) {
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let x = UPDATE_MARGIN;

    d.text(
        x,
        UPDATE_MARGIN,
        2,
        SETUP_TEXT,
        &format!("atualização {tag}"),
    );
    // Changelog em texto — o body vem em markdown do GitHub; as marcas
    // (#, *, `) são ruído no fonte de pixel.
    let clean: String = changelog
        .replace('\r', "")
        .chars()
        .filter(|c| !matches!(c, '#' | '*' | '`'))
        .collect();
    let lines: Vec<&str> = clean.lines().take(24).collect();
    let mut y = UPDATE_MARGIN + 60;
    for line in &lines {
        if y > h - 150 {
            d.text(x, y, 1, SETUP_DIM, "…");
            break;
        }
        d.text_wrapped(x, y, (w - UPDATE_MARGIN * 2) as u32, 1, SETUP_TEXT, line);
        y += 22;
    }

    let btn = (w - UPDATE_MARGIN * 2 - 220, h - 96, 220u32, 44u32);
    let voltar = (x, h - 96, 140u32, 44u32);
    d.text(x, h - 130, 1, SETUP_DIM, status);
    // O botão some (dim, sem hit) enquanto baixa e depois de baixado —
    // o status ocupa o lugar.
    let can_update = !busy && !status.contains("baixado");
    let label = if can_update {
        "atualizar"
    } else {
        "atualizado"
    };
    if can_update {
        d.outline(
            btn.0,
            btn.1,
            btn.2,
            btn.3,
            2,
            (SETUP_GREEN.0, SETUP_GREEN.1, SETUP_GREEN.2, 255),
        );
        d.text(
            btn.0 + (btn.2 as i32 - label.chars().count() as i32 * 18) / 2,
            btn.1 + 12,
            2,
            SETUP_GREEN,
            label,
        );
    } else {
        d.outline(btn.0, btn.1, btn.2, btn.3, 2, (90, 90, 96, 255));
        d.text(
            btn.0 + (btn.2 as i32 - label.chars().count() as i32 * 18) / 2,
            btn.1 + 12,
            2,
            SETUP_DIM,
            label,
        );
    }
    d.outline(
        voltar.0,
        voltar.1,
        voltar.2,
        voltar.3,
        2,
        (SETUP_TEXT.0, SETUP_TEXT.1, SETUP_TEXT.2, 255),
    );
    d.text(
        voltar.0 + (voltar.2 as i32 - "voltar".chars().count() as i32 * 18) / 2,
        voltar.1 + 12,
        2,
        SETUP_TEXT,
        "voltar",
    );
    (btn, voltar)
}

/// Whether the setup screen should come up — any of the three (plan
/// revision: "liberar continuar somente com a bios": core, DAT e BIOS).
fn core_missing_at_start(core_installed: bool, dat_installed: bool, bios_installed: bool) -> bool {
    !core_installed || !dat_installed || !bios_installed
}

/// The first-run setup screen, drawn inside the TV: a download button per
/// missing file, each with its own live status line underneath, plus
/// "continuar" to get to the regular console. Done buttons restyle to a
/// green "instalado" instead of disappearing, so the layout never jumps.
#[allow(clippy::too_many_arguments)]
fn draw_setup(
    d: &mut Screen,
    core_label: &str,
    core_done: bool,
    core_busy: bool,
    dat_label: &str,
    dat_done: bool,
    dat_busy: bool,
    bios_label: &str,
    bios_done: bool,
) {
    /// Same glyph metrics the platform font uses — `Screen::text` advances
    /// `GLYPH_W * scale` per char, `GLYPH_H * scale` per line.
    const CELL: i32 = 9;
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let (core_r, dat_r, bios_r, cont_r) = setup_rects(w as u32, h as u32);
    let center_x = |s: &str, scale: u32| (w - s.chars().count() as i32 * CELL * scale as i32) / 2;

    d.text(
        center_x("bem-vindo ao polystnx", 3),
        h * 12 / 100,
        3,
        SETUP_TEXT,
        "bem-vindo ao polystnx",
    );
    d.text(
        center_x("para jogar, faltam dois downloads e a bios", 1),
        h * 12 / 100 + 74,
        1,
        SETUP_DIM,
        "para jogar, faltam dois downloads e a bios",
    );

    fn draw_button(d: &mut Screen, r: &RectT, label: &str, color: (u8, u8, u8)) {
        const CELL2: i32 = 18; // CELL * scale 2, for centering the label
        let (cr, cg, cb) = color;
        d.outline(r.0, r.1, r.2, r.3, 2, (cr, cg, cb, 255));
        d.text(
            r.0 + (r.2 as i32 - label.chars().count() as i32 * CELL2) / 2,
            r.1 + (r.3 as i32 - 40) / 2,
            2,
            color,
            label,
        );
    }
    let (mut core_color, mut dat_color, mut bios_color) = (SETUP_TEXT, SETUP_TEXT, SETUP_TEXT);
    if core_busy {
        core_color = SETUP_DIM;
    } else if core_done {
        core_color = SETUP_GREEN;
    }
    if dat_busy {
        dat_color = SETUP_DIM;
    } else if dat_done {
        dat_color = SETUP_GREEN;
    }
    if bios_done {
        bios_color = SETUP_GREEN;
    }
    draw_button(d, &core_r, core_label, core_color);
    draw_button(d, &dat_r, dat_label, dat_color);
    draw_button(d, &bios_r, bios_label, bios_color);
    d.text(
        core_r.0 + 4,
        core_r.1 + core_r.3 as i32 + 8,
        1,
        SETUP_DIM,
        "necessário para rodar os jogos",
    );
    d.text(
        dat_r.0 + 4,
        dat_r.1 + dat_r.3 as i32 + 8,
        1,
        SETUP_DIM,
        "opcional: nomes canônicos, ano e editora dos jogos",
    );
    d.text(
        bios_r.0 + 4,
        bios_r.1 + bios_r.3 as i32 + 8,
        1,
        SETUP_DIM,
        "necessária: a BIOS do console (SCPH*.BIN) em bios/",
    );

    // O continuar fica CINZA (desabilitado) sem BIOS — não há o que
    // continuar sem ela (plan revision: "liberar continuar somente com a
    // bios").
    let cont_color = if bios_done { SETUP_TEXT } else { SETUP_DIM };
    draw_button(d, &cont_r, "continuar", cont_color);
    d.text(
        center_x("enter também continua", 1),
        cont_r.1 + cont_r.3 as i32 + 10,
        1,
        SETUP_DIM,
        "enter também continua",
    );
}

/// Headless preview of the idle screen (dev/testing) — same setup as `run`,
/// one frame captured through the tube instead of a live loop. Shows the
/// first-run setup screen when either download is missing, the plain idle
/// panel otherwise.
pub fn capture_preview(
    cab: &mut Cabinet,
    static_level: f32,
    core_installed: bool,
    dat_installed: bool,
    bios_installed: bool,
    path: &Path,
) -> Result<()> {
    cab.clear_panel();
    if core_missing_at_start(core_installed, dat_installed, bios_installed) {
        let render = |d: &mut Screen| {
            draw_setup(
                d,
                if core_installed {
                    "instalado"
                } else {
                    "baixar núcleo PCSX Rearmed"
                },
                core_installed,
                false,
                if dat_installed {
                    "instalado"
                } else {
                    "baixar nointro.dat"
                },
                dat_installed,
                false,
                if bios_installed {
                    "instalada"
                } else {
                    "colocar a BIOS (atualizar)"
                },
                bios_installed,
            )
        };
        cab.capture_idle_2d(SETUP_BG, render, path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    } else {
        crate::console_art::load_brand_images(cab);
        cab.capture_static_bmp(static_level, path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}
