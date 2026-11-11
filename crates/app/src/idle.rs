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
use polystnx_platform::{
    Cabinet, MenuMode, MenuNav, PadFocus, PanelButton, Platform, Screen, TvControl, UpdateArrow,
};

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
    let bh = 64u32.min((h / 14).max(44));
    let x = (w as i32 - bw as i32) / 2;
    // Tela apertada (Steam Deck, buffer 685x525): o grupo de TRÊS botões +
    // continuar cabe justo — agrupa do topo (22%) com gap 28; telas altas
    // mantêm o espaçamento largo (44).
    let gap = if h >= 700 { 40 } else { 34 };
    let core_y = h as i32 * 24 / 100;
    let dat_y = core_y + bh as i32 + gap;
    let bios_y = dat_y + bh as i32 + gap;
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

    /// Começa o download do núcleo numa thread (o idle drena o canal a cada
    /// quadro). Sem build automática para este sistema, deixa só o aviso.
    fn start_core(&mut self) {
        match core_update::core_download_url() {
            Some(url) => {
                let (tx, rx) = mpsc::channel();
                let dest = crate::dirs::core_dir();
                std::thread::spawn(move || core_update::download_and_install(&url, &dest, &tx));
                self.rx = Some(rx);
                self.done = false;
                self.failed = false;
                self.label = "baixando...".to_string();
            }
            None => self.label = "sem build automática - use configurações".to_string(),
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

/// O manual do console (plan revision: "fazer um botao em cima das
/// configurações na tela inicial para visualizar esse pdf"), um por
/// idioma — gerados por
/// `examples/gen_console_manual.rs`.
const CONSOLE_MANUAL: &[u8] = include_bytes!("../assets/console-manual-pt.pdf");
const CONSOLE_MANUAL_KEY: &str = "console-manual-pt";

/// O leitor do manual do console aberto no tubo — o mesmo leitor dos
/// manuais da estante (`shelf::draw_manual_view`): páginas renderizadas sob
/// demanda por um worker, que vivem como texturas sob `key`.
struct ConsoleManual {
    /// A chave das texturas das páginas (o "sha1" do leitor da estante) —
    /// uma por idioma, para a troca de idioma não reaproveitar páginas.
    key: &'static str,
    pages: usize,
    page: usize,
    /// O worker não conseguiu abrir o PDF (morreu sem dizer quantas
    /// páginas tem) — a tela avisa em vez de ficar "abrindo" para sempre.
    failed: bool,
    requested: std::collections::HashSet<usize>,
    rx: Receiver<crate::manual::ManualEv>,
    tx: mpsc::Sender<usize>,
    rects: (
        crate::shelf::ViewBtn,
        crate::shelf::ViewBtn,
        crate::shelf::ViewBtn,
    ),
}

impl ConsoleManual {
    /// Grava o PDF do idioma corrente na pasta temporária (o renderizador
    /// lê de arquivo) e sobe o worker. `None` = não deu para gravar.
    fn open() -> Option<Self> {
        let (key, bytes) = (CONSOLE_MANUAL_KEY, CONSOLE_MANUAL);
        // Na pasta do app, não no temp: no Linux o `/tmp` é compartilhado e
        // um arquivo de mesmo nome e tamanho deixado por outro usuário seria
        // aberto pelo renderizador de PDF.
        let dir = crate::dirs::app_root().join("cache");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{key}.pdf"));
        // Já gravado numa abertura anterior (mesmo tamanho): não regrava os
        // alguns MB na thread de UI a cada clique.
        let fresh = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
        if !fresh {
            if let Err(e) = std::fs::write(&path, bytes) {
                log::warn!("console manual: writing {}: {e}", path.display());
                return None;
            }
        }
        let (ev_tx, rx) = mpsc::channel();
        let (tx, req_rx) = mpsc::channel::<usize>();
        std::thread::spawn(move || {
            let pages = match crate::manual::page_count(&path) {
                Ok(n) => n,
                Err(e) => {
                    log::warn!("console manual: {e}");
                    return;
                }
            };
            let _ = ev_tx.send(crate::manual::ManualEv::Ready {
                sha1: key.to_string(),
                pages,
            });
            for page in req_rx {
                let (w, h, rgba) = match crate::manual::render_page(&path, page, 2048) {
                    Ok(Some(img)) => (img.w, img.h, img.rgba),
                    _ => (0, 0, Vec::new()),
                };
                let _ = ev_tx.send(crate::manual::ManualEv::Page {
                    sha1: key.to_string(),
                    page,
                    w,
                    h,
                    rgba,
                });
            }
        });
        Some(Self {
            key,
            pages: 0,
            page: 1,
            failed: false,
            requested: std::collections::HashSet::new(),
            rx,
            tx,
            rects: ((0, 0, 1, 1), (0, 0, 1, 1), (0, 0, 1, 1)),
        })
    }

    /// Eventos do worker (contagem e páginas viram texturas) e o pedido da
    /// página corrente e da seguinte assim que faltam.
    fn poll(&mut self, cab: &mut Cabinet) {
        loop {
            let ev = match self.rx.try_recv() {
                Ok(ev) => ev,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.failed = self.pages == 0;
                    break;
                }
            };
            match ev {
                crate::manual::ManualEv::Ready { pages, .. } => self.pages = pages,
                crate::manual::ManualEv::Page {
                    page, w, h, rgba, ..
                } => {
                    if w > 0 {
                        cab.set_image(crate::shelf::manual_page_id(self.key, page), w, h, &rgba);
                    } else {
                        log::warn!("console manual: page {page} didn't render");
                    }
                }
            }
        }
        for page in [self.page, self.page + 1] {
            // Página já em textura (de uma abertura anterior): não renderiza
            // de novo.
            if self.pages > 0
                && page <= self.pages
                && !cab.has_image(crate::shelf::manual_page_id(self.key, page))
                && self.requested.insert(page)
            {
                let _ = self.tx.send(page);
            }
        }
    }

    fn prev(&mut self) {
        self.page = self.page.saturating_sub(1).max(1);
    }

    fn next(&mut self) {
        if self.page < self.pages {
            self.page += 1;
        }
    }
}

/// Um alvo do foco do controle na tela inicial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleFocus {
    Panel(PanelButton),
    Arrow(UpdateArrow),
    /// Um botão físico do queixo da TV — sem eles, quem só tem controle não
    /// liga a TV, não mexe no volume nem abre o menu de imagem.
    Tv(TvControl),
}

/// Os botões da TV que o controle percorre, na ordem em que aparecem no
/// queixo (da esquerda para a direita).
const TV_FOCUS: [TvControl; 4] = [
    TvControl::VolDown,
    TvControl::VolUp,
    TvControl::Menu,
    TvControl::Power,
];

/// Os alvos que o controle percorre, em ordem: botões do painel (em ordem
/// de leitura), as setas verdes de update e, por último, os botões da TV.
/// `drawn` é a lista de botões que o painel desenhou no último quadro.
fn idle_items(drawn: &[PanelButton], app_update: bool, core_stale: bool) -> Vec<IdleFocus> {
    let mut v: Vec<IdleFocus> = drawn
        .iter()
        .filter(|b| {
            matches!(
                b,
                PanelButton::Insert
                    | PanelButton::Settings
                    | PanelButton::ConsoleManual
                    | PanelButton::Dev
                    | PanelButton::CoreDownload
            )
        })
        .map(|b| IdleFocus::Panel(*b))
        .collect();
    if app_update {
        v.push(IdleFocus::Arrow(UpdateArrow::App));
    }
    if core_stale {
        v.push(IdleFocus::Arrow(UpdateArrow::Core));
    }
    v.extend(TV_FOCUS.map(IdleFocus::Tv));
    v
}

/// O foco efetivo: o escolhido, se ainda existe; senão o padrão — "Estante
/// de games", ou o download do núcleo quando falta, ou o primeiro da lista.
fn resolve_focus(items: &[IdleFocus], chosen: Option<IdleFocus>) -> IdleFocus {
    if let Some(f) = chosen.filter(|f| items.contains(f)) {
        return f;
    }
    [
        IdleFocus::Panel(PanelButton::Insert),
        IdleFocus::Panel(PanelButton::CoreDownload),
    ]
    .into_iter()
    .find(|f| items.contains(f))
    .or_else(|| items.first().copied())
    .unwrap_or(IdleFocus::Tv(TvControl::Power))
}

/// Passo do foco: Cima/Esquerda para trás, Baixo/Direita para frente, com
/// volta nas pontas.
fn step_focus(items: &[IdleFocus], cur: IdleFocus, forward: bool) -> IdleFocus {
    let n = items.len().max(1);
    let at = items.iter().position(|f| *f == cur).unwrap_or(0);
    items[if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
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
    let frame = crate::runner::UI_FRAME;
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
    // O foco do controle na tela inicial e se a legenda já foi mostrada.
    let mut idle_focus: Option<IdleFocus> = None;
    // O manual do console aberto no tubo, se estiver.
    let mut console_manual: Option<ConsoleManual> = None;
    let mut legend_shown = false;
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
                cab.try_tv_click(ox, oy);
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
                                core_update::download_and_install(&url, &dest, &tx)
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
                crate::runner::pace_ui(&mut next, frame, plat, cab);
                continue;
            }
            // Just dismissed: the same click/Confirm that dismissed the
            // setup screen must not also land on the idle panel below —
            // give it a frame of its own.
            cab.set_idle_core_prompt(core_missing.then_some("Baixar núcleo PCSX Rearmed"));
            crate::runner::pace_ui(&mut next, frame, plat, cab);
            continue;
        }

        cab.set_idle_core_prompt(core_missing.then_some("Baixar núcleo PCSX Rearmed"));

        // Manual do console aberto: dono do tubo (‹/› viram página,
        // voltar/Back fecha) — com a TV ligada ou não, como as
        // Configurações.
        if let Some(man) = console_manual.as_mut() {
            let mut close = false;
            for nav in &m.nav {
                match nav {
                    MenuNav::Left | MenuNav::PageUp => man.prev(),
                    MenuNav::Right | MenuNav::PageDown => man.next(),
                    MenuNav::Back => close = true,
                    _ => {}
                }
            }
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
                if cab.hit_panel_button(ox, oy) == Some(PanelButton::ConsoleManual) {
                    close = true;
                }
                if let Some((sx, sy)) = cab.hit_screen_point(ox, oy) {
                    let (prev, next, voltar) = man.rects;
                    if in_update_rect(sx, sy, prev) {
                        man.prev();
                    } else if in_update_rect(sx, sy, next) {
                        man.next();
                    } else if in_update_rect(sx, sy, voltar) {
                        close = true;
                    }
                }
            }
            if close {
                console_manual = None;
            } else {
                man.poll(cab);
                let ready = cab.has_image(crate::shelf::manual_page_id(man.key, man.page));
                let (key, page, pages, failed) = (man.key, man.page, man.pages, man.failed);
                let mut rects = man.rects;
                let render = |d: &mut Screen| {
                    rects = crate::shelf::draw_manual_view(d, key, page, pages, ready, failed);
                };
                cab.frame_idle_2d(SETUP_BG, render);
                man.rects = rects;
                crate::runner::pace_ui(&mut next, frame, plat, cab);
                continue;
            }
        }

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
                cab.try_tv_click(ox, oy);
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
                        let asset = asset_url.expect("checado acima");
                        let dest = crate::dirs::update_dir();
                        std::thread::spawn(move || {
                            let result = crate::update_check::download_asset(&asset, &dest, &ptx)
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
            crate::runner::pace_ui(&mut next, frame, plat, cab);
            continue;
        }

        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            cab.try_tv_click(ox, oy);
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
                Some(UpdateArrow::Core) if core_stale && core.rx.is_none() => core.start_core(),
                _ => {}
            }
            match cab.hit_panel_button(ox, oy) {
                // Sem o PCSX Rearmed instalado (plan revision: "só desative o
                // botão de config e de inserir cartucho se o usuário não
                // baixou o PCSX Rearmed") — os dois ficam inertes; o painel
                // oferece o download do core no lugar.
                Some(PanelButton::Insert) if !core_missing => return Ok(IdleExit::OpenShelf),
                Some(PanelButton::Settings) if !core_missing => return Ok(IdleExit::OpenSettings),
                // O botão só existe com o devmode ligado (só o painel o
                // desenha), mas o gate fica explícito: sem devmode, cliques
                // fantasmas não abrem nada.
                Some(PanelButton::Dev) if cab.dev_mode() => return Ok(IdleExit::OpenDev),
                // O manual do console não depende do núcleo — é só leitura.
                Some(PanelButton::ConsoleManual) => console_manual = ConsoleManual::open(),
                Some(PanelButton::CoreDownload) if core.rx.is_none() => core.start_core(),
                // Already downloading? The click is ignored until the
                // worker reports Done/Failed.
                Some(PanelButton::CoreDownload) => {}
                _ => {}
            }
        }
        // O controle na tela inicial: d-pad passeia pelos botões do painel,
        // pelas setas de update e pelos botões da TV (anel lavanda); A ativa
        // o foco; B sai.
        let items = idle_items(&cab.panel_button_list(), app_update.is_some(), core_stale);
        let mut focus = resolve_focus(&items, idle_focus);
        for nav in m.nav {
            match nav {
                MenuNav::Up | MenuNav::Left => {
                    focus = step_focus(&items, focus, false);
                    idle_focus = Some(focus);
                }
                MenuNav::Down | MenuNav::Right => {
                    focus = step_focus(&items, focus, true);
                    idle_focus = Some(focus);
                }
                // Without the core there's nowhere to go but out — the panel
                // buttons are inert (same gate as the clicks above); the TV
                // buttons and Back still work.
                MenuNav::Confirm => match focus {
                    IdleFocus::Tv(c) => cab.apply_tv_control(c),
                    IdleFocus::Panel(PanelButton::Insert) if !core_missing => {
                        return Ok(IdleExit::OpenShelf)
                    }
                    IdleFocus::Panel(PanelButton::Settings) if !core_missing => {
                        return Ok(IdleExit::OpenSettings)
                    }
                    IdleFocus::Panel(PanelButton::Dev) if cab.dev_mode() => {
                        return Ok(IdleExit::OpenDev)
                    }
                    IdleFocus::Panel(PanelButton::ConsoleManual) => {
                        console_manual = ConsoleManual::open()
                    }
                    IdleFocus::Panel(PanelButton::CoreDownload) if core.rx.is_none() => {
                        core.start_core()
                    }
                    IdleFocus::Arrow(UpdateArrow::App) if app_update.is_some() => {
                        update_view = app_update.clone();
                        update_status.clear();
                    }
                    IdleFocus::Arrow(UpdateArrow::Core) if core_stale && core.rx.is_none() => {
                        core.start_core()
                    }
                    _ => {}
                },
                MenuNav::Back => return Ok(IdleExit::Quit),
                // Start: Configurações direto, como no SixteeN.
                MenuNav::Start if !core_missing => return Ok(IdleExit::OpenSettings),
                _ => {}
            }
        }
        // O anel só aparece para quem está usando o controle; a primeira vez
        // acende a legenda dos botões no OSD do queixo.
        if plat.last_input_was_pad() {
            if items.contains(&focus) {
                cab.set_pad_focus(Some(match focus {
                    IdleFocus::Panel(b) => PadFocus::Panel(b),
                    IdleFocus::Arrow(a) => PadFocus::Arrow(a),
                    IdleFocus::Tv(c) => PadFocus::Tv(c),
                }));
            }
            if !legend_shown {
                legend_shown = true;
                cab.push_osd(
                    &["D-pad escolhe   A confirma", "B sai"],
                    None,
                    Duration::from_secs(6),
                );
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
        crate::runner::pace_ui(&mut next, frame, plat, cab);
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
            d.text(x, y, 1, SETUP_DIM, "...");
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
    fosforo_btn2(
        d,
        &btn,
        label,
        if can_update { SETUP_TEXT } else { SETUP_DIM },
    );
    fosforo_btn2(d, &voltar, "voltar", SETUP_TEXT);
    (btn, voltar)
}

/// Cor dos botões "fósforo" (vermelho), como no SixteeN.
const FOSFORO: (u8, u8, u8) = (240, 70, 64);

/// Botão fósforo em escala 2: sem caixa, rótulo centrado com sublinhado,
/// na cor dada (`SETUP_TEXT` vira o vermelho fósforo; verde e cinza mantêm).
fn fosforo_btn2(d: &mut Screen, r: &RectT, label: &str, color: (u8, u8, u8)) {
    let cor = if color == SETUP_TEXT { FOSFORO } else { color };
    let tw = label.chars().count() as i32 * 18;
    let tx = r.0 + (r.2 as i32 - tw).max(0) / 2;
    let ty = r.1 + (r.3 as i32 - 40) / 2;
    d.text(tx, ty, 2, cor, label);
    if color != SETUP_DIM {
        d.fill(
            tx,
            ty + 42,
            tw.max(18) as u32,
            3,
            (cor.0, cor.1, cor.2, 255),
        );
    }
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
        fosforo_btn2(d, r, label, color);
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
    // Os rótulos descritivos sob os botões só cabem em telas altas — em
    // telas apertadas eles CAIAM SOBRE o botão seguinte (plan revision:
    // "os botoes estao por cima do texto").
    if h >= 750 {
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
    }

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

#[cfg(test)]
mod focus_tests {
    use super::*;

    #[test]
    fn items_end_with_the_tv_buttons_in_chin_order() {
        let items = idle_items(&[PanelButton::Insert, PanelButton::Settings], true, true);
        assert_eq!(items[0], IdleFocus::Panel(PanelButton::Insert));
        assert_eq!(items[2], IdleFocus::Arrow(UpdateArrow::App));
        assert_eq!(items[3], IdleFocus::Arrow(UpdateArrow::Core));
        assert_eq!(
            &items[4..],
            &TV_FOCUS.map(IdleFocus::Tv),
            "VOL-, VOL+, MENU, POWER"
        );
    }

    #[test]
    fn focus_defaults_to_the_shelf_button_and_wraps_around() {
        let items = idle_items(&[PanelButton::Settings, PanelButton::Insert], false, false);
        assert_eq!(
            resolve_focus(&items, None),
            IdleFocus::Panel(PanelButton::Insert)
        );
        let last = *items.last().unwrap();
        assert_eq!(step_focus(&items, last, true), items[0], "volta no fim");
        assert_eq!(step_focus(&items, items[0], false), last, "e no começo");
        // Um foco que sumiu da lista cai no padrão.
        let gone = IdleFocus::Arrow(UpdateArrow::App);
        assert_eq!(
            resolve_focus(&items, Some(gone)),
            IdleFocus::Panel(PanelButton::Insert)
        );
    }
}
