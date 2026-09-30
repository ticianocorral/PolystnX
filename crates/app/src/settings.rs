//! The settings screen — the idle screen's/shelf's "Configurações" button,
//! redrawn on the shelf's own layout (plan revision: "a tela de
//! configurações não está no padrão do resto do app — faça a tv na mesma
//! proporção e coloque o painel"): rows inside the tube, exactly like the
//! shelf's grid area, and a flat side panel carrying the section buttons
//! ("jogo", "vídeo", "sistema", "controles") plus "Voltar". No nesting
//! deeper than a section: click a row to act on it (or gamepad nav +
//! Confirm/Back — plan revision: no keyboard shortcuts). Edits save to
//! `psx-xperience.cfg` immediately, not on some later "apply" step — there's
//! nothing to lose by backing out.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::Result;
use xperience_platform::{
    Cabinet, GamepadBtn, MenuMode, MenuNav, PadButton, Platform, Screen, SettingsButton,
    SettingsPanelInfo,
};

use crate::config::Config;
use crate::core_update::{self, CoreUpdateMsg};
use crate::rom_rename::{self, RenameOutcome};

const BG: (u8, u8, u8) = (18, 18, 20);
const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
const HINT: (u8, u8, u8) = (120, 116, 108);

const MARGIN: i32 = 40;
const ROW_H: i32 = 30;
/// Where the row list starts, vertically — right under the title (drawn at
/// scale 2, so `2 * GLYPH_H` tall) plus a little breathing room. Shared by
/// the draw functions and the click hit-test so they can't drift apart.
const LIST_TOP: i32 = MARGIN + 50;
const RUNAHEAD_MAX: u32 = 4;

/// The settings sections, in panel order — the tube shows one section's
/// rows at a time; the panel's `Section(i)` buttons switch between them
/// (plan revision: "se for interessante faça secoes na configuração").
const SECTION_NAMES: [&str; 6] = [
    "jogo",
    "vídeo",
    "sistema",
    "conquistas",
    "controles",
    "bios",
];
const SEC_JOGO: usize = 0;
const SEC_VIDEO: usize = 1;
const SEC_SISTEMA: usize = 2;
const SEC_CONQUISTAS: usize = 3;
const SEC_CONTROLES: usize = 4;
const SEC_BIOS: usize = 5;

/// How many rows a section shows — shared by the nav clamps and the click
/// hit-test so they can't drift apart. `controles` is the key-bind list, so
/// its count comes from `cfg.keymap.describe()`.
/// Which "conquistas" row the text-entry editor is attached to.
const RA_ROW_USER: usize = 0;
const RA_ROW_TOKEN: usize = 1;
const RA_ROW_SENHA: usize = 2;

fn row_count(sec: usize, cfg: &Config) -> usize {
    match sec {
        SEC_JOGO => 2,
        SEC_VIDEO => 3,
        SEC_SISTEMA => 3,
        // usuário / token / senha / testar login / hardcore
        SEC_CONQUISTAS => 5,
        SEC_CONTROLES => cfg.keymap.describe().len(),
        // uma linha por arquivo de BIOS na pasta
        SEC_BIOS => crate::bios::list(&crate::dirs::bios_dir()).len().max(1),
        _ => 0,
    }
}

/// Which row (if any) a screen-local click y lands in, given how many rows
/// are actually on screen right now — settings rows span the full width, so
/// only y matters. Shared by the draw functions and the click handling in
/// `run`, so they can't drift apart.
fn row_at(y: i32, count: usize) -> Option<usize> {
    if y < LIST_TOP {
        return None;
    }
    let i = ((y - LIST_TOP) / ROW_H) as usize;
    (i < count).then_some(i)
}

/// Where the "Núcleo" row's background download stands right now — drives
/// both its label and whether `Confirm` on it starts a new one.
enum CoreStatus {
    Idle,
    Downloading { downloaded: u64, total: Option<u64> },
    Done,
    Failed(String),
}

/// The "testar login" row's state — the same Idle/busy/ok/fail shape the
/// core row uses, minus progress (one small request).
enum LoginStatus {
    Idle,
    Checking,
    Ok(String),
    Failed(String),
}

/// Run the settings screen until the player backs all the way out. Returns
/// `true` if the whole app should quit (window closed / Cmd-Q) instead of
/// returning to the shelf.
pub fn run(plat: &mut Platform, cab: &mut Cabinet, cfg: &mut Config) -> Result<bool> {
    let mut sec = SEC_JOGO;
    let mut sel: usize = 0;
    let mut controls_top: usize = 0;
    let mut awaiting_key: Option<usize> = None; // index into keymap.describe()
    let mut core_status = CoreStatus::Idle;
    let mut core_worker: Option<Receiver<CoreUpdateMsg>> = None;
    // The last "Renomear ROMs" result, shown as that row's own label until
    // the next click (plan revision) — synchronous (plain disk renames, no
    // network), so unlike the core download there's no worker/progress to
    // track, just a one-shot summary.
    let mut rename_status: Option<String> = None;
    // RA account rows (plan: `docs/plano-retroachievements.md`, fase 1):
    // the free-text editor (same deliberate keyboard exception as the
    // shelf's filter box) and the login-test worker.
    let mut ra_editing: Option<usize> = None;
    let mut ra_draft = String::new();
    let mut login_status = LoginStatus::Idle;
    let mut login_worker: Option<Receiver<Result<String, String>>> = None;
    // O worker em andamento é o login Connect (senha → token)? Se sim, o
    // `Ok` guarda o token no cfg em vez de só exibir.
    let mut login_connect = false;
    let frame_time = Duration::from_millis(16);
    let mut next = Instant::now();
    cab.set_close_button(true);

    loop {
        if let Some(rx) = &login_worker {
            match rx.try_recv() {
                Ok(Ok(token)) => {
                    if login_connect {
                        cfg.ra_connect = Some(token);
                        let _ = cfg.save();
                        login_status =
                            LoginStatus::Ok("conta conectada — conquistas liberadas".to_string());
                        login_connect = false;
                    } else {
                        login_status = LoginStatus::Ok(token);
                    }
                    login_worker = None;
                }
                Ok(Err(e)) => {
                    login_status = LoginStatus::Failed(e);
                    login_worker = None;
                    login_connect = false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => login_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &core_worker {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    CoreUpdateMsg::Progress { downloaded, total } => {
                        core_status = CoreStatus::Downloading { downloaded, total };
                    }
                    CoreUpdateMsg::Done => {
                        core_status = CoreStatus::Done;
                        core_worker = None;
                        break;
                    }
                    CoreUpdateMsg::Failed(e) => {
                        core_status = CoreStatus::Failed(e);
                        core_worker = None;
                        break;
                    }
                }
            }
        }

        // While editing an RA field, ONLY the text-entry side is polled —
        // a `poll_menu` first would drain (and discard) the TextInput
        // events before `poll_text_entry` ever sees them.
        if let Some(row) = ra_editing {
            let te = plat.poll_text_entry();
            if te.quit {
                return Ok(true);
            }
            if te.backspace {
                ra_draft.pop();
            }
            for c in te.typed.chars() {
                if ra_draft.chars().count() < 40 {
                    ra_draft.push(c);
                }
            }
            // ⌘V/⌘C (Ctrl no resto): o colado entra pelo mesmo limite de
            // 40 caracteres, e o copiado leva o campo inteiro — rascunhos
            // não têm seleção.
            if let Some(paste) = &te.paste {
                for c in paste.chars() {
                    if ra_draft.chars().count() < 40 {
                        ra_draft.push(c);
                    }
                }
            }
            if te.copy {
                plat.set_clipboard_text(&ra_draft);
            }
            if te.commit {
                if row == RA_ROW_USER {
                    cfg.ra_user = ra_draft.trim().to_string();
                } else if row == RA_ROW_SENHA {
                    // A senha nunca é gravada: vira token Connect por
                    // login2 — ele que autentica a lógica das conquistas e
                    // a submissão (plan revision: a web key ficou readonly).
                    let (user, senha) = (cfg.ra_user.clone(), ra_draft.trim().to_string());
                    if user.is_empty() || senha.is_empty() {
                        login_status = LoginStatus::Failed("preencha usuário e senha".to_string());
                    } else {
                        login_status = LoginStatus::Checking;
                        let (tx, rx) = mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::ra::connect_login(&user, &senha));
                        });
                        login_worker = Some(rx);
                        login_connect = true;
                    }
                } else {
                    cfg.ra_token = ra_draft.trim().to_string();
                }
                let _ = cfg.save();
                if !matches!(login_status, LoginStatus::Checking) {
                    login_status = LoginStatus::Idle;
                }
                ra_editing = None;
                plat.stop_text_input(cab);
            } else if te.cancel {
                ra_editing = None;
                plat.stop_text_input(cab);
            }
            // Clicks during editing do nothing (same rule as the shelf's
            // filter box): finish or cancel the field first.
            cab.set_settings_panel(SettingsPanelInfo {
                title: "configurações".to_string(),
                sections: SECTION_NAMES.iter().map(|s| s.to_string()).collect(),
                selected: sec,
            });
            let render = |d: &mut Screen| match sec {
                SEC_VIDEO => draw_video(d, cfg, sel),
                SEC_SISTEMA => draw_sistema(d, cfg, sel, &core_status, rename_status.as_deref()),
                SEC_CONQUISTAS => {
                    draw_conquistas(d, cfg, sel, &login_status, ra_editing, &ra_draft)
                }
                SEC_CONTROLES => draw_controls(d, cfg, sel, controls_top, awaiting_key),
                SEC_BIOS => draw_bios(d, cfg, sel),
                _ => draw_jogo(d, cfg, sel),
            };
            cab.frame_settings(BG, render);
            crate::runner::pace_frame(&mut next, frame_time);
            continue;
        }

        let poll_mode = if awaiting_key.is_some() {
            MenuMode::CaptureKey
        } else {
            MenuMode::Nav
        };
        let m = plat.poll_menu(poll_mode);
        if m.quit {
            return Ok(true);
        }

        if let Some(row) = awaiting_key {
            // Whichever arrives first binds: a keyboard key on the left side
            // of each row, a gamepad button on the right ("controle:").
            if let Some(name) = m.captured_key {
                bind_row(cfg, row, &name);
                let _ = cfg.save();
                awaiting_key = None;
            } else if let Some(btn) = m.captured_pad {
                bind_pad_row(plat, cfg, row, btn);
                let _ = cfg.save();
                awaiting_key = None;
            } else if m.capture_cancelled {
                awaiting_key = None;
            }
        } else {
            // Mouse click — the panel's section buttons (and "Voltar") come
            // first, then the rows inside the tube. Every action here also
            // has a gamepad-nav path below (`m.nav`), this just adds the
            // click-driven one (plan revision: no keyboard shortcuts).
            if let Some((cx, cy)) = m.click {
                let (ox, oy) = cab.window_to_output(cx, cy);
                if cab.hit_close_button(ox, oy) {
                    return Ok(true);
                }
                if cab.hit_minimize_button(ox, oy) {
                    cab.minimize();
                    continue;
                }
                match cab.hit_settings_button(ox, oy) {
                    Some(SettingsButton::Back) => return Ok(false),
                    Some(SettingsButton::Section(i)) => {
                        sec = i.min(SECTION_NAMES.len() - 1);
                        sel = 0;
                        controls_top = 0;
                    }
                    None => {
                        if let Some((_, ly)) = cab.hit_screen_point(ox, oy) {
                            if let Some(i) = row_at(ly, row_count(sec, cfg)) {
                                sel = i;
                                activate_row(
                                    cab,
                                    cfg,
                                    &mut sec,
                                    &mut sel,
                                    &mut awaiting_key,
                                    &mut ra_editing,
                                    i,
                                    &mut core_status,
                                    &mut core_worker,
                                    &mut rename_status,
                                    &mut login_status,
                                    &mut login_worker,
                                );
                                if ra_editing.is_some() {
                                    ra_draft = String::new();
                                    plat.start_text_input(cab);
                                }
                            }
                        }
                    }
                }
            }
            // Botão direito no usuário/token: abre o campo já colando — o
            // caminho curto do token copiado direto no browser. Só os campos
            // de texto: as outras linhas fazem coisa demais para disparar no
            // botão "errado".
            if let Some((cx, cy)) = m.right_click {
                let (ox, oy) = cab.window_to_output(cx, cy);
                if let Some((_, ly)) = cab.hit_screen_point(ox, oy) {
                    if let Some(i) = row_at(ly, row_count(sec, cfg)) {
                        if sec == SEC_CONQUISTAS
                            && matches!(i, RA_ROW_USER | RA_ROW_TOKEN | RA_ROW_SENHA)
                        {
                            sel = i;
                            ra_editing = Some(i);
                            ra_draft = String::new();
                            if let Some(paste) = plat.paste_from_clipboard() {
                                for c in paste.chars() {
                                    if ra_draft.chars().count() < 40 {
                                        ra_draft.push(c);
                                    }
                                }
                            }
                            plat.start_text_input(cab);
                        }
                    }
                }
            }
            for nav in &m.nav {
                match nav {
                    MenuNav::Up => sel = sel.saturating_sub(1),
                    MenuNav::Down => {
                        sel = (sel + 1).min(row_count(sec, cfg).saturating_sub(1));
                    }
                    // "controles" came from "jogo"'s Controles row, so its
                    // Back goes there; every other section's Back leaves
                    // settings entirely.
                    MenuNav::Back => {
                        if sec == SEC_CONTROLES {
                            sec = SEC_JOGO;
                            sel = 0;
                        } else {
                            return Ok(false);
                        }
                    }
                    MenuNav::Confirm => {
                        let cur = sel;
                        let was_editing = ra_editing.is_some();
                        activate_row(
                            cab,
                            cfg,
                            &mut sec,
                            &mut sel,
                            &mut awaiting_key,
                            &mut ra_editing,
                            cur,
                            &mut core_status,
                            &mut core_worker,
                            &mut rename_status,
                            &mut login_status,
                            &mut login_worker,
                        );
                        if !was_editing && ra_editing.is_some() {
                            ra_draft = String::new();
                            plat.start_text_input(cab);
                        }
                    }
                    // The toggle/slider rows keep their old left/right
                    // feel; everything else ignores them.
                    MenuNav::Left => adjust_row(cfg, sec, sel, cab, false),
                    MenuNav::Right => adjust_row(cfg, sec, sel, cab, true),
                    _ => {}
                }
            }
            // Keep the controles cursor inside its scroll window.
            if sec == SEC_CONTROLES {
                let (_, sh) = cab.shelf_screen_size();
                let visible = ((sh as i32 - MARGIN * 2 - 60) / ROW_H).max(1) as usize;
                if sel < controls_top {
                    controls_top = sel;
                } else if sel >= controls_top + visible {
                    controls_top = sel + 1 - visible;
                }
            }
        }

        // The flat side panel — section buttons + Voltar, rebuilt every
        // frame like the shelf's own panel.
        cab.set_settings_panel(SettingsPanelInfo {
            title: "configurações".to_string(),
            sections: SECTION_NAMES.iter().map(|s| s.to_string()).collect(),
            selected: sec,
        });

        let render = |d: &mut Screen| match sec {
            SEC_VIDEO => draw_video(d, cfg, sel),
            SEC_SISTEMA => draw_sistema(d, cfg, sel, &core_status, rename_status.as_deref()),
            SEC_CONQUISTAS => draw_conquistas(d, cfg, sel, &login_status, ra_editing, &ra_draft),
            SEC_CONTROLES => draw_controls(d, cfg, sel, controls_top, awaiting_key),
            SEC_BIOS => draw_bios(d, cfg, sel),
            _ => draw_jogo(d, cfg, sel),
        };
        cab.frame_settings(BG, render);

        crate::runner::pace_frame(&mut next, frame_time);
    }
}

/// Act on row `i` of the current section — the click path and the gamepad's
/// Confirm both land here, so they can't drift apart.
// One row action touches every screen knob at once; splitting it into
// parameter structs just to appease the arity lint would obscure the
// actual wiring.
#[allow(clippy::too_many_arguments)]
fn activate_row(
    cab: &mut Cabinet,
    cfg: &mut Config,
    sec: &mut usize,
    sel: &mut usize,
    awaiting_key: &mut Option<usize>,
    ra_editing: &mut Option<usize>,
    i: usize,
    core_status: &mut CoreStatus,
    core_worker: &mut Option<Receiver<CoreUpdateMsg>>,
    rename_status: &mut Option<String>,
    login_status: &mut LoginStatus,
    login_worker: &mut Option<Receiver<Result<String, String>>>,
) {
    match *sec {
        SEC_JOGO => match i {
            0 => {
                *sec = SEC_CONTROLES;
                *sel = 0;
            }
            _ => {
                cfg.runahead = (cfg.runahead + 1) % (RUNAHEAD_MAX + 1);
                let _ = cfg.save();
            }
        },
        SEC_VIDEO => match i {
            0 => {
                cfg.fullscreen = !cfg.fullscreen;
                cab.toggle_fullscreen();
                let _ = cfg.save();
            }
            1 => {
                cfg.hiss_on_static = !cfg.hiss_on_static;
                let _ = cfg.save();
                cab.set_static_hiss(cfg.hiss_on_static);
            }
            _ => {
                cfg.mute_game = !cfg.mute_game;
                let _ = cfg.save();
            }
        },
        SEC_SISTEMA => match i {
            0 => start_core_download(core_status, core_worker),
            1 => {
                cfg.check_updates_on_start = !cfg.check_updates_on_start;
                let _ = cfg.save();
            }
            _ => *rename_status = Some(run_rom_rename()),
        },
        SEC_CONQUISTAS => match i {
            RA_ROW_USER | RA_ROW_TOKEN | RA_ROW_SENHA => *ra_editing = Some(i),
            3 => start_login_test(cfg, login_status, login_worker),
            _ => {
                cfg.ra_hardcore = !cfg.ra_hardcore;
                let _ = cfg.save();
            }
        },
        SEC_CONTROLES => *awaiting_key = Some(*sel),
        SEC_BIOS => {
            let files = crate::bios::list(&crate::dirs::bios_dir());
            if let Some(info) = files.get(*sel) {
                let same = cfg.bios_default.as_deref() == Some(info.file.as_str());
                cfg.bios_default = if same { None } else { Some(info.file.clone()) };
                let _ = cfg.save();
                crate::bios::selected_system_dir(
                    &crate::dirs::bios_dir(),
                    cfg.bios_default.as_deref(),
                );
            }
        }
        _ => {}
    }
}

/// Kick off the login test on a worker thread (plan fase 1) — one small
/// request, but network is network: the row shows "testando..." until the
/// `mpsc` reply lands.
fn start_login_test(
    cfg: &Config,
    status: &mut LoginStatus,
    worker: &mut Option<Receiver<Result<String, String>>>,
) {
    if matches!(status, LoginStatus::Checking) {
        return;
    }
    if cfg.ra_user.is_empty() || cfg.ra_token.is_empty() {
        *status = LoginStatus::Failed("preencha usuário e token antes".to_string());
        return;
    }
    let (tx, rx) = mpsc::channel();
    let (user, token) = (cfg.ra_user.clone(), cfg.ra_token.clone());
    std::thread::spawn(move || {
        let _ = tx.send(crate::ra::test_login(&user, &token));
    });
    *worker = Some(rx);
    *status = LoginStatus::Checking;
}

/// The left/right adjustments — run-ahead steps, fullscreen/check-updates
/// toggle. Rows without a horizontal feel ignore them.
fn adjust_row(cfg: &mut Config, sec: usize, sel: usize, cab: &mut Cabinet, right: bool) {
    match (sec, sel) {
        (SEC_JOGO, 1) => {
            cfg.runahead = if right {
                (cfg.runahead + 1).min(RUNAHEAD_MAX)
            } else {
                cfg.runahead.saturating_sub(1)
            };
            let _ = cfg.save();
        }
        (SEC_VIDEO, 0) => {
            cfg.fullscreen = !cfg.fullscreen;
            cab.toggle_fullscreen();
            let _ = cfg.save();
        }
        (SEC_VIDEO, 1) => {
            cfg.hiss_on_static = !cfg.hiss_on_static;
            let _ = cfg.save();
            cab.set_static_hiss(cfg.hiss_on_static);
        }
        (SEC_VIDEO, 2) => {
            cfg.mute_game = !cfg.mute_game;
            let _ = cfg.save();
        }
        (SEC_SISTEMA, 1) => {
            cfg.check_updates_on_start = !cfg.check_updates_on_start;
            let _ = cfg.save();
        }
        (SEC_BIOS, _) => {}
        (SEC_CONQUISTAS, 4) => {
            cfg.ra_hardcore = !cfg.ra_hardcore;
            let _ = cfg.save();
        }
        _ => {}
    }
}

/// Kick off a background download of the SwanStation core (plan: "opção pra
/// baixar o SwanStation... e opção de update do núcleo" — one action serves both,
/// the buildbot only ever serves "latest"). No-op while one is already in
/// flight. The standard background-thread + `mpsc` shape of this app —
/// `settings::run`'s loop drains it with `try_recv()` every frame.
fn start_core_download(status: &mut CoreStatus, worker: &mut Option<Receiver<CoreUpdateMsg>>) {
    if matches!(status, CoreStatus::Downloading { .. }) {
        return;
    }
    let Some(url) = core_update::core_download_url() else {
        *status = CoreStatus::Failed("sem build automática pra esta plataforma".to_string());
        return;
    };
    let (tx, rx) = mpsc::channel();
    let dest = crate::dirs::core_dir();
    std::thread::spawn(move || core_update::download_and_install(url, &dest, &tx));
    *worker = Some(rx);
    *status = CoreStatus::Downloading {
        downloaded: 0,
        total: None,
    };
}

/// How long since the installed core was written, in the coarse terms
/// `draw_sistema`'s row wants — `None` if there's no core installed at all.
fn core_installed_label() -> Option<String> {
    let path = crate::dirs::core_dir().join(core_update::core_file_name());
    let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
    let days = modified.elapsed().unwrap_or_default().as_secs() / 86_400;
    Some(match days {
        0 => "instalado hoje".to_string(),
        1 => "instalado há 1 dia".to_string(),
        n => format!("instalado há {n} dias"),
    })
}

/// Run the rename synchronously (plain disk renames — fast enough even for
/// a few hundred ROMs that there's no need for `start_core_download`'s
/// background-thread treatment) and turn the result into the row's next
/// label (plan: "renomear automaticamente no padrao no-intro").
fn run_rom_rename() -> String {
    let outcome = rom_rename::rename_to_nointro(
        &crate::dirs::roms_dir(),
        &crate::dirs::nointro_dat_path(),
        &crate::dirs::saves_dir(),
        &crate::dirs::notes_dir(),
        &crate::dirs::assets_dir(),
    );
    match outcome {
        RenameOutcome::Renamed(list) => {
            for r in &list {
                log::info!("rom rename: {} -> {}", r.old, r.new);
            }
            match list.len() {
                1 => "Renomear ROMs: 1 rom renomeada".to_string(),
                n => format!("Renomear ROMs: {n} roms renomeadas"),
            }
        }
        RenameOutcome::NothingToDo => "Renomear ROMs: nenhuma precisava de nome novo".to_string(),
    }
}

fn core_row_label(status: &CoreStatus) -> String {
    match status {
        CoreStatus::Downloading { downloaded, total } => {
            let mb = *downloaded as f64 / 1_048_576.0;
            match total {
                Some(t) => format!(
                    "Núcleo: baixando... {mb:.1}/{:.1} MB",
                    *t as f64 / 1_048_576.0
                ),
                None => format!("Núcleo: baixando... {mb:.1} MB"),
            }
        }
        CoreStatus::Done => "Núcleo: atualizado com sucesso".to_string(),
        CoreStatus::Failed(e) => format!("Núcleo: falha - {e}"),
        CoreStatus::Idle => match core_installed_label() {
            Some(installed) => format!("Núcleo: atualizar ({installed})"),
            None => "Núcleo: baixar".to_string(),
        },
    }
}

fn bind_row(cfg: &mut Config, row: usize, key_name: &str) {
    let Some((action, _)) = cfg.keymap.describe().into_iter().nth(row) else {
        return;
    };
    // Every row here is a gameplay button now — console/UI commands aren't
    // rebindable any more (plan revision: mouse/gamepad only).
    let Ok(b) = action.parse::<PadButton>() else {
        return;
    };
    if let Err(e) = cfg.keymap.bind_pad(key_name, b) {
        log::warn!("rebind {action} -> {key_name}: {e}");
    }
}

/// The gamepad half of a captured row: `btn` (the physical pad button the
/// player pressed) becomes the driver of that row's SNES action, and the
/// live platform map is swapped so it takes effect without leaving settings.
fn bind_pad_row(plat: &mut Platform, cfg: &mut Config, row: usize, btn: GamepadBtn) {
    let Some((action, _)) = cfg.padmap.describe().into_iter().nth(row) else {
        return;
    };
    let Ok(b) = action.parse::<PadButton>() else {
        return;
    };
    let name = btn.string();
    if let Err(e) = cfg.padmap.bind(&name, b) {
        log::warn!("rebind {action} -> {name}: {e}");
        return;
    }
    plat.set_pad_map(cfg.padmap.clone());
}

fn draw_jogo(d: &mut Screen, cfg: &Config, sel: usize) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, "jogo");
    let mut y = LIST_TOP;
    let rows = [
        "Controles".to_string(),
        format!("Run-ahead: {} quadro(s)", cfg.runahead),
    ];
    for (i, row) in rows.iter().enumerate() {
        draw_row(d, x, y, row, i == sel);
        y += ROW_H;
    }
    draw_hint(
        d,
        "clique numa opção pra mudar -- setas do gamepad ajustam, A confirma",
    );
}

fn draw_video(d: &mut Screen, cfg: &Config, sel: usize) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, "vídeo");
    let mut y = LIST_TOP;
    let rows = [
        format!(
            "Tela cheia: {}",
            if cfg.fullscreen {
                "ligada"
            } else {
                "desligada"
            }
        ),
        format!(
            "Chiado da TV fora do ar: {}",
            if cfg.hiss_on_static {
                "ligado"
            } else {
                "desligado"
            }
        ),
        format!(
            "Silenciar o jogo (só o leitor): {}",
            if cfg.mute_game { "ligado" } else { "desligado" }
        ),
    ];
    for (i, row) in rows.iter().enumerate() {
        draw_row(d, x, y, row, i == sel);
        y += ROW_H;
    }
    draw_hint(
        d,
        "clique numa opção pra alternar (o chiado toca na TV desligada)",
    );
}

fn draw_sistema(
    d: &mut Screen,
    cfg: &Config,
    sel: usize,
    core_status: &CoreStatus,
    rename_status: Option<&str>,
) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, "sistema");
    let mut y = LIST_TOP;
    let rows = [
        core_row_label(core_status),
        format!(
            "Verificar atualizações ao abrir: {}",
            if cfg.check_updates_on_start {
                "sim"
            } else {
                "não"
            }
        ),
        rename_status
            .unwrap_or("Renomear ROMs para o padrão No-Intro")
            .to_string(),
    ];
    for (i, row) in rows.iter().enumerate() {
        draw_row(d, x, y, row, i == sel);
        y += ROW_H;
    }
    draw_hint(d, "clique numa opção pra executar ou alternar");
}

fn draw_conquistas(
    d: &mut Screen,
    cfg: &Config,
    sel: usize,
    login: &LoginStatus,
    editing: Option<usize>,
    draft: &str,
) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, "conquistas");
    let field = |row: usize, label: &str, value: &str| {
        if editing == Some(row) {
            format!("{label}: {draft}_")
        } else if value.is_empty() {
            format!("{label}: (vazio)")
        } else {
            // Segredos (token, senha) não voltam para a tela, nem
            // mascarados — só a certeza de que estão lá.
            if row == RA_ROW_TOKEN {
                format!("{label}: (configurado)")
            } else {
                format!("{label}: {value}")
            }
        }
    };
    let senha_label = if cfg.ra_connect.is_some() {
        "Senha: (conectado)".to_string()
    } else {
        "Senha: (vazio — conecta a lógica das conquistas)".to_string()
    };
    let login_label = match login {
        LoginStatus::Idle => {
            if cfg.ra_user.is_empty() && cfg.ra_token.is_empty() {
                "Testar login".to_string()
            } else {
                "Testar login (não testado)".to_string()
            }
        }
        LoginStatus::Checking => "Testar login: testando...".to_string(),
        LoginStatus::Ok(l) => format!("Testar login: ok — {l}"),
        LoginStatus::Failed(e) => format!("Testar login: {e}"),
    };
    let rows = [
        field(RA_ROW_USER, "Usuário", &cfg.ra_user),
        field(RA_ROW_TOKEN, "Token da web API", &cfg.ra_token),
        if editing == Some(RA_ROW_SENHA) {
            format!("Senha: {draft}_")
        } else {
            senha_label
        },
        login_label,
        format!(
            "Modo hardcore: {}",
            if cfg.ra_hardcore {
                "ligado (sem cheats/savestates)"
            } else {
                "desligado"
            }
        ),
    ];
    let mut y = LIST_TOP;
    for (i, row) in rows.iter().enumerate() {
        draw_row(d, x, y, row, i == sel);
        y += ROW_H;
    }
    d.text(
        x,
        y + 8,
        1,
        DIM,
        "token: retroachievements.org -> settings -> web api",
    );
    d.text(
        x,
        y + 8 + 20,
        1,
        DIM,
        "senha: a mesma do site — libera a lógica e o envio das conquistas",
    );
    draw_hint(
        d,
        "clique edita, botão direito cola, cmd+c copia -- enter confirma, esc cancela",
    );
}

fn draw_controls(d: &mut Screen, cfg: &Config, sel: usize, top: usize, awaiting: Option<usize>) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, "controles");
    let mut y = LIST_TOP;

    let binds = cfg.keymap.describe();
    let pads = cfg.padmap.describe();
    let (_, sh) = d.size();
    let visible = ((sh as i32 - MARGIN * 2 - 60) / ROW_H).max(1) as usize;
    for (i, (action, key)) in binds.iter().enumerate().skip(top).take(visible) {
        let pad = pads.get(i).map(|(_, b)| b.as_str()).unwrap_or("-");
        let pad = if pad.is_empty() { "-" } else { pad };
        let label = if awaiting == Some(i) {
            format!("{action}: aperte uma tecla ou botão do controle... (esc cancela)")
        } else {
            format!("{action}: {key} | controle: {pad}")
        };
        draw_row(d, x, y, &label, i == sel);
        y += ROW_H;
    }

    draw_hint(
        d,
        "clique numa ação pra trocar a tecla ou o botão do controle -- esc volta pro jogo",
    );
}

/// The one-line help at the bottom of the tube, inside it like the shelf's
/// own footer text (shared by every section so the sections read the same).
fn draw_hint(d: &mut Screen, s: &str) {
    d.text(MARGIN, d.size().1 as i32 - MARGIN, 1, HINT, s);
}

/// Headless preview of one section (dev/testing) — not part of the
/// interactive `run` loop. Accepts the section names, plus the old
/// "main"/"controls" aliases.
pub fn capture_preview(
    cab: &mut Cabinet,
    cfg: &Config,
    screen: &str,
    path: &std::path::Path,
) -> Result<()> {
    let sec = match screen {
        "video" => SEC_VIDEO,
        "sistema" => SEC_SISTEMA,
        "conquistas" => SEC_CONQUISTAS,
        "controls" => SEC_CONTROLES,
        _ => SEC_JOGO,
    };
    cab.set_settings_panel(SettingsPanelInfo {
        title: "configurações".to_string(),
        sections: SECTION_NAMES.iter().map(|s| s.to_string()).collect(),
        selected: sec,
    });
    let render = |d: &mut Screen| match sec {
        SEC_VIDEO => draw_video(d, cfg, 0),
        SEC_SISTEMA => draw_sistema(d, cfg, 0, &CoreStatus::Idle, None),
        SEC_CONQUISTAS => draw_conquistas(d, cfg, 0, &LoginStatus::Idle, None, ""),
        SEC_CONTROLES => draw_controls(d, cfg, 0, 0, None),
        SEC_BIOS => draw_bios(d, cfg, 0),
        _ => draw_jogo(d, cfg, 0),
    };
    cab.set_close_button(true);
    cab.capture_settings(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn draw_row(d: &mut Screen, x: i32, y: i32, label: &str, selected: bool) {
    let (cursor, color) = if selected { ("> ", TEXT) } else { ("  ", DIM) };
    d.text(x, y, 1, color, &format!("{cursor}{label}"));
}

/// A seção BIOS: uma linha por arquivo em `bios/`, com a informação da
/// própria ROM; a escolhida como padrão leva a marca.
fn draw_bios(d: &mut Screen, cfg: &Config, sel: usize) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, "bios");
    let mut y = LIST_TOP;
    let files = crate::bios::list(&crate::dirs::bios_dir());
    if files.is_empty() {
        d.text(
            x,
            y,
            1,
            DIM,
            "nenhuma BIOS em bios/ — coloque o arquivo .BIN lá",
        );
        return;
    }
    let chosen = cfg
        .bios_default
        .clone()
        .unwrap_or_else(|| "automático (o core varre a pasta)".into());
    d.text(x, y, 1, DIM, &format!("padrão: {chosen}"));
    y += ROW_H + 8;
    for (i, info) in files.iter().enumerate() {
        let mut label = info.label();
        if cfg.bios_default.as_deref() == Some(info.file.as_str()) {
            label.push_str("  ← padrão");
        }
        let (cursor, color) = if i == sel { ("> ", TEXT) } else { ("  ", DIM) };
        d.text(x, y, 1, color, &format!("{cursor}{label}"));
        y += ROW_H;
    }
}
