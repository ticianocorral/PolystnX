//! The emulator run-loop, factored out of the `emu-run` binary so the unified
//! `polystnx` binary can call it between selector visits. Presentation is fixed:
//! RF NTSC + CRT-tube warp (see docs/fase-0.md).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use polystnx_emulation::{
    AnalogStick, Button, Core, Frame as EmuFrame, PixelFormat as EmuFormat, MEMORY_SAVE_RAM,
};
use polystnx_platform::{
    Cabinet, FrameRef, ModalBackdrop, PanelButton, PixelFormat as PlatFormat, Platform, UiEvent,
    MAX_PORTS,
};

use crate::config::Config;

/// How often to flush battery SRAM to disk while playing (frames ≈ 10 s).
const SRAM_FLUSH_FRAMES: u32 = 600;
/// Save-state slots, `0`..`9`.
const SLOTS: u8 = 10;
/// Max characters a pause-book free-text note may hold (plan revision) —
/// enforced live, as it's typed, not just on save.
const NOTE_CHAR_LIMIT: usize = 240;
/// The dim, near-still hiss the screen idles at once the console is off
/// (plan §3.3) — also what the next screen fades in from, and the idle/root
/// screen's resting level (`crate::idle`).
pub(crate) const OFF_STATIC_LEVEL: f32 = 0.12;

/// Why the run-loop returned.
/// How long an unlock notification stays up (plan fase 4).
const OSD_UNLOCK_TTL: Duration = Duration::from_secs(6);

/// Stable image id for a badge name — distinct namespace from the art
/// hashes (their ids are sha1-derived u64s; this flips a high bit).
fn osd_badge_id(badge: &str) -> u64 {
    // FNV-1a: stable, dependency-free, distinct namespace from the art ids
    // (which are sha1-derived; this flips the top bit).
    let mut v: u64 = 0xcbf2_9ce4_8422_2325;
    for b in badge.as_bytes() {
        v ^= *b as u64;
        v = v.wrapping_mul(0x0000_0100_0000_01b3);
    }
    v | 0x8000_0000_0000_0000
}

pub enum GameExit {
    /// Ejected — the caller should show the idle/root screen again (not the
    /// shelf directly). Carries the signal-off static level the screen
    /// settled on, so the next screen can fade in over it instead of a fresh
    /// burst (plan §3.3, "a estante entra por cima").
    Ejected { static_level: f32 },
    /// Window close / Cmd-Q / headless self-check done — tear the app down.
    Quit,
}

/// Everything [`run_game`] needs for one session.
pub struct GameSpec {
    pub core: PathBuf,
    pub rom: PathBuf,
    pub system_dir: PathBuf,
    pub save_dir: PathBuf,
    /// Where per-game notebooks live: `<title>/01.png`..`15.png` for the
    /// photo slots, `<title>/01.txt`..`15.txt` for the text-note slots
    /// (both fixed at 15, plan revision — numbered independently of each
    /// other) (plan §3.4).
    pub notes_dir: PathBuf,
    /// Speculative frames past the shown one; `None` = take the config value.
    pub runahead: Option<u32>,
    /// Headless self-check: `(path, frame)` — run to `frame`, dump a BMP, exit.
    pub shot: Option<(PathBuf, u32)>,
    /// Local logo art (`assets/logo/<rom>.*`) for the top of the side panel.
    /// `None` shows the ROM's name instead (plan §3.2, item 1).
    pub logo: Option<PathBuf>,
    /// O memory card físico encaixado no slot 1 (`memcards/<nome>.mcr`,
    /// biblioteca compartilhada do plano §3). Quando presente, ele É o save:
    /// o conteúdo entra no `SAVE_RAM` ao inserir o disco e o flush periódico
    /// volta para o card — nunca para uma pasta do jogo. `None` conserva o
    /// comportamento herdado do SNES (um sram por jogo).
    pub card1: Option<PathBuf>,
    /// O mesmo para o slot 2 (`mem_id` 1 no libretro). `None` = slot vazio
    /// até o jogador escolher um card no botão "MC slot 2".
    pub card2: Option<PathBuf>,
    /// O nome canônico (No-Intro, do DAT) para o TOPO DO PAINEL. Os arquivos
    /// de save/nota/cheat continuam chaveados pelo nome do arquivo do jogo —
    /// trocar o nome canônico nunca orfana um save.
    pub display_title: Option<String>,
    /// Ligar SEM disco: boot direto na BIOS do console (menu de clock).
    pub bios: bool,
    /// A estante inteira (título, caminho) — alimenta o modal "Inserir
    /// disco" da troca quente com a tampa aberta.
    pub library: Vec<(String, PathBuf)>,
    /// Local cartridge art (`assets/cartridge/<rom>.*`), shown in the panel
    /// alongside the logo when present (plan revision) — `None` just skips
    /// that block, no fallback needed.
    pub cartridge: Option<PathBuf>,
    /// Headless self-check: skip straight to the console-off signal-off snow
    /// and save `shot` there, instead of running the game to `shot`'s frame
    /// count.
    pub shot_off: bool,
    /// Headless self-check: force one `NoteCapture` at `shot`'s frame (or
    /// frame 1 without one), exactly like clicking "Nota" live — so `--shot`
    /// can prove out the panel's notebook block without a window.
    pub debug_note_capture: bool,
    /// Headless self-check: skip straight to the pause book (plan §3.2/§3.4)
    /// instead of gameplay, so `--shot` can prove it out against whatever
    /// notes already exist on disk for this ROM.
    pub debug_shot_pause: bool,
    /// Headless self-check: skip straight to one of the save/load-state or
    /// print slot pickers (plan revision) instead of gameplay — `"save"`,
    /// `"load"`, or `"print"`; anything else is ignored (no modal shown).
    pub debug_shot_modal: Option<String>,
    /// Dev/testing: `Some(dir)` salva frames da animação de inserção do
    /// cartucho em `dir` (o gif do site é gerado com isto) e sai, sem
    /// live loop.
    pub debug_cart_anim: Option<std::path::PathBuf>,
}

const PAD: [(Button, polystnx_platform::PadButton); 14] = {
    use polystnx_platform::PadButton as P;
    [
        (Button::B, P::B),
        (Button::Y, P::Y),
        (Button::Select, P::Select),
        (Button::Start, P::Start),
        (Button::Up, P::Up),
        (Button::Down, P::Down),
        (Button::Left, P::Left),
        (Button::Right, P::Right),
        (Button::A, P::A),
        (Button::X, P::X),
        (Button::L, P::L),
        (Button::R, P::R),
        (Button::L2, P::L2),
        (Button::R2, P::R2),
    ]
};

/// A path-hostile character in a game title, replaced with `_` so it can't
/// escape its parent directory or fail to create — shared by every per-game
/// folder (`saves/<title>/`, `notes/<title>/`), and by `rom_rename` (plan
/// revision) when it turns a No-Intro name into a ROM file name.
pub(crate) fn sanitize_dir_name(title: &str) -> String {
    let safe: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect();
    if safe.is_empty() {
        "___".to_string()
    } else {
        safe
    }
}

/// `save_dir/<game title>/` — one folder per game, named for a human
/// browsing it rather than the ROM hash (plan revision — mirrors
/// `note_dir`: readability over rename-proofing nobody asked for here).
fn game_dir(save_dir: &Path, title: &str) -> PathBuf {
    save_dir.join(sanitize_dir_name(title))
}

/// `save_dir/<title>/<slot>.state` — plain slot number, no hash prefix
/// (plan revision).
fn state_file(save_dir: &Path, title: &str, slot: u8) -> PathBuf {
    game_dir(save_dir, title).join(format!("{slot}.state"))
}

/// `save_dir/<title>/<slot>.disc` — the disc file name the state was made
/// on (plano §3.1: um state amarra o disco em que foi salvo; restaurar com
/// outro disco no drive é lixo).
fn disc_stamp_path(save_dir: &Path, title: &str, slot: u8) -> PathBuf {
    game_dir(save_dir, title).join(format!("{slot}.disc"))
}

/// The disc's file name, for the state stamps (`jogo.chd`).
fn disc_name(rom: &Path) -> String {
    rom.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `save_dir/<title>/sram.srm` — battery SRAM, one file per game (plan
/// revision — used to be `<hash>.srm` directly under `save_dir`).
fn sram_file(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("sram.srm")
}

/// `save_dir/<title>/cheats.txt` — plain text either way, so the extension
/// might as well say so (plan revision — used to be `<hash>.cheats`).
fn cheat_state_path(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("cheats.txt")
}

/// `save_dir/<title>/playtime.txt` — total seconds ever spent powered on,
/// plain text like `cheats.txt` (plan revision: "mostrar tempo total de
/// jogo do game").
fn playtime_path(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("playtime.txt")
}

/// The per-game folder name `run_game` itself uses — the ROM's file stem,
/// not the shelf's `CatalogEntry::title()` (which prefers a No-Intro/
/// internal name when one exists). Exposed so the shelf can read
/// `total_playtime_secs` for the exact folder a session actually wrote to,
/// instead of guessing at a name that might not match.
pub fn rom_title(rom_path: &Path) -> String {
    rom_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "???".to_string())
}

/// Total time this game has spent powered on, across every session ever
/// played — `0` for a game never played (no file yet) or a corrupt one
/// (never worth failing the panel over one bad number).
pub fn total_playtime_secs(save_dir: &Path, title: &str) -> u64 {
    fs::read_to_string(playtime_path(save_dir, title))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Add this session's powered-on seconds to the running total and save it
/// back — best-effort, same as the other per-game sidecars; a `0` (the
/// console was never turned on this session) skips the write entirely.
fn add_playtime(save_dir: &Path, title: &str, secs: u64) {
    if secs == 0 {
        return;
    }
    let total = total_playtime_secs(save_dir, title) + secs;
    let _ = fs::create_dir_all(game_dir(save_dir, title));
    let _ = fs::write(playtime_path(save_dir, title), total.to_string());
}

/// Rows for the "Salvar" modal (plan revision — replaces the old slot
/// cycler): one per save-state slot, always clickable (saving overwrites
/// whatever was there), labelled "(vazio)" for slots with nothing in them
/// yet purely as information.
fn save_slot_rows(save_dir: &Path, title: &str) -> Vec<(String, bool)> {
    (0..SLOTS)
        .map(|s| {
            let filled = state_file(save_dir, title, s).exists();
            let label = if filled {
                format!("Slot {s}")
            } else {
                format!("Slot {s} (vazio)")
            };
            (label, true)
        })
        .collect()
}

/// Rows for the "Carregar" modal, mirroring `save_slot_rows` — an empty
/// slot is disabled instead of just noted, since there's nothing to load.
fn load_slot_rows(save_dir: &Path, title: &str) -> Vec<(String, bool)> {
    (0..SLOTS)
        .map(|s| {
            let filled = state_file(save_dir, title, s).exists();
            let label = if filled {
                format!("Slot {s}")
            } else {
                format!("Slot {s} (vazio)")
            };
            (label, filled)
        })
        .collect()
}

/// O arquivo que o PCSX Rearmed usa para o card do SLOT 2 (o core o cria
/// no save dir com memcard2 habilitado e o escreve ao sair).
const MC2_SHARED_FILE: &str = "pcsx-card2.mcd";
/// O arquivo do card do SLOT 1 no PCSX Rearmed — a fonte de verdade do
/// core no boot; o SAVE_RAM é espelho e NÃO sobrevive à sessão.
const MC1_SHARED_FILE: &str = "pcsx-card1.mcd";

/// Os `.mcr` da pasta, ordenados — a fonte das duas vistas da biblioteca.
fn list_cards(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("mcr"))
                .unwrap_or(false)
        })
        .collect()
}

/// A biblioteca com a informação de cada card (plano revision: "lista com
/// todos existentes, com as infos dos blocos salvos neles, inclusive com os
/// ícones dos jogos"): ícone do primeiro save + "N/15 blocos" no rótulo.
/// A linha 0 é sempre "(criar cartão novo)".
type CardModalRow = (String, bool, Option<(u32, u32, Vec<u8>)>);

fn card_rows_with_icons(
    dir: &Path,
    current1: Option<&Path>,
    current2: Option<&Path>,
) -> (Vec<CardModalRow>, Vec<PathBuf>) {
    let mut cards = list_cards(dir);
    cards.sort();
    let mut rows = vec![("(criar cartão novo)".to_string(), true, None)];
    for card in &cards {
        let name = card
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let info = crate::memcard::inspect(card);
        let (label, icon) = match info {
            Some(info) => {
                // Um card só vive em um slot — o rótulo diz qual.
                let where_ = if current1 == Some(card.as_path()) {
                    " - no slot 1"
                } else if current2 == Some(card.as_path()) {
                    " - no slot 2"
                } else {
                    ""
                };
                (
                    format!("{name} - {}/15 blocos{}", info.used, where_),
                    info.saves
                        .first()
                        .and_then(|s| s.icon.clone())
                        .map(|d| (16u32, 16u32, d)),
                )
            }
            None => (format!("{name} - inválido"), None),
        };
        rows.push((label, true, icon));
    }
    (rows, cards)
}

// --- core em thread própria (plano revision: "faca tudo") ------------------
//
// O core bloqueia: boot, troca BIOS→jogo e FMV pesada seguram
// `core.run()` por centenas de ms — e com o loop sequencial, nada era
// redesenhado (quadro congelado, disco parado). O worker dono do core
// responde a comandos; a interface continua desenhando a 60 fps o último
// quadro recebido, com o disco girando por relógio.

/// Snapshot de entrada de um tick — o `Input` da plataforma não é Send.
#[derive(Default)]
struct PadSnapshot {
    buttons: Vec<(usize, Button, bool)>,
    analog: Vec<(usize, i16, i16, i16, i16)>,
}

enum CoreCmd {
    Run {
        input: PadSnapshot,
    },
    Reset,
    CheatReset,
    CheatSet {
        i: u32,
        on: bool,
        code: String,
    },
    SaveState {
        tx: std::sync::mpsc::Sender<Vec<u8>>,
    },
    LoadState {
        bytes: Vec<u8>,
    },
    Sram {
        tx: std::sync::mpsc::Sender<Option<Vec<u8>>>,
    },
    SramAt {
        id: u32,
        tx: std::sync::mpsc::Sender<Option<Vec<u8>>>,
    },
    WriteMem {
        id: u32,
        bytes: Vec<u8>,
    },
    SwitchDisc {
        path: PathBuf,
        tx: std::sync::mpsc::Sender<bool>,
    },
    /// Sem disco + Reset: reseta e boota a BIOS (o console fica ligado).
    BootBios,
    /// Abre (`true`)/fecha (`false`) a BANDEJA pela disk control interface —
    /// o jogo emulado VÊ a tampa abrir (telas de erro reais dele).
    TrayEject {
        ejected: bool,
    },
}

/// O que um `Run` produz: frame, aspecto, áudio do quadro e — quando há
/// sessão de RA ativa — cópia da RAM do quadro real (o tick do RA roda no
/// main, contra a mesma memória que o original via `with_memory`).
struct CoreOut {
    frame: Option<EmuFrame>,
    aspect: f32,
    audio: Vec<i16>,
    /// A RAM do jogo como (ponteiro, tamanho) — SEM cópia (era um `to_vec`
    /// de 2 MB por frame com sessão RA ativa). A leitura na main acontece
    /// com o worker bloqueado no recv — ver o SAFETY no tick do RA.
    ram: Option<(usize, usize)>,
}

/// Core bruto (handles de dlopen) atravessando uma fronteira de thread.
struct CoreRunning(Core);
unsafe impl Send for CoreRunning {}
fn spawn_core_worker(
    core: Core,
    runahead: usize,
    want_ram: bool,
    current_disc: PathBuf,
) -> (
    std::sync::mpsc::Sender<CoreCmd>,
    std::sync::mpsc::Receiver<CoreOut>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = std::sync::mpsc::channel::<CoreCmd>();
    let (otx, orx) = std::sync::mpsc::channel::<CoreOut>();
    let handle = std::thread::Builder::new()
        .name("core-psx".into())
        .spawn(move || {
            // Prioridade baixa para a thread do core (macOS: nice afeta só a
            // thread): nas FMVs o software renderer satura as CPUs e a thread
            // de UI (bomba de eventos do SDL) era preterida — a "travada".
            #[cfg(target_os = "macos")]
            unsafe {
                libc::setpriority(libc::PRIO_DARWIN_THREAD, 0, 12);
            }
            let mut running = CoreRunning(core);
            let mut spec_state = Vec::new();
            let mut disc = current_disc;
            while let Ok(cmd) = rx.recv() {
                let core = &mut running.0;
                match cmd {
                    CoreCmd::Run { input } => {
                        for (port, btn, held) in &input.buttons {
                            core.set_button(*port, *btn, *held);
                        }
                        for (port, lx, ly, rx_, ry_) in &input.analog {
                            core.set_analog(*port, AnalogStick::Left, *lx, *ly);
                            core.set_analog(*port, AnalogStick::Right, *rx_, *ry_);
                        }
                        core.run();
                        let audio = core.audio().to_vec();
                        let ram = want_ram
                            .then(|| {
                                core.memory_ptr(polystnx_emulation::MEMORY_SYSTEM_RAM)
                                    .map(|(ptr, len)| (ptr as usize, len))
                            })
                            .flatten();
                        let aspect = core.av_info().aspect_ratio;
                        // Frames especulativos de run-ahead: áudio e RAM
                        // capturados do quadro real acima; o frame exibido é
                        // o que o core reporta depois deles, e o estado volta
                        // ao real em seguida — exatamente o fluxo antigo.
                        let speculated = runahead > 0 && core.save_state_into(&mut spec_state);
                        if speculated {
                            for _ in 0..runahead {
                                core.run();
                            }
                        }
                        let frame = core.take_frame();
                        if speculated {
                            core.load_state(&spec_state);
                        }
                        let _ = otx.send(CoreOut {
                            frame,
                            aspect,
                            audio,
                            ram,
                        });
                    }
                    CoreCmd::Reset => core.reset(),
                    CoreCmd::TrayEject { ejected } => {
                        if core.set_eject_state(ejected).is_none() {
                            log::warn!("worker: core sem disk control interface");
                        }
                    }
                    CoreCmd::BootBios => {
                        core.reset();
                        if core.load_bios().is_err() {
                            log::warn!("worker: boot pela BIOS recusado");
                        }
                    }
                    CoreCmd::CheatReset => core.cheat_reset(),
                    CoreCmd::CheatSet { i, on, code } => {
                        core.cheat_set(i, on, &code);
                    }
                    CoreCmd::SaveState { tx } => {
                        let _ = tx.send(core.save_state().unwrap_or_default());
                    }
                    CoreCmd::LoadState { bytes } => {
                        core.load_state(&bytes);
                    }
                    CoreCmd::Sram { tx } => {
                        let _ = tx.send(core.sram());
                    }
                    CoreCmd::SramAt { id, tx } => {
                        let _ = tx.send(core.memory(id));
                    }
                    CoreCmd::WriteMem { id, bytes } => {
                        core.write_memory(id, &bytes);
                    }
                    CoreCmd::SwitchDisc { path, tx } => {
                        // Mesma coreografia do fluxo antigo: estado volta por
                        // cima da recarga; falha, volta ao disco atual.
                        let state = core.save_state();
                        core.reset();
                        if core.load_disc(&path).is_ok() {
                            if let Some(state) = state {
                                core.load_state(&state);
                            }
                            disc = path;
                            let _ = tx.send(true);
                        } else {
                            let _ = core.load_disc(&disc);
                            let _ = tx.send(false);
                        }
                    }
                }
            }
        })
        .expect("thread do core");
    (tx, orx, handle)
}

/// Encaixa `path` num slot de memory card (`mem_id` 0 = slot 1, 1 = slot 2
/// no libretro): o conteúdo vira a memória da sessão e o flush passa a
/// apontar para o arquivo. Card sem arquivo ainda nasce do core na próxima
/// materialização.
fn seat_card(
    tx: &std::sync::mpsc::Sender<CoreCmd>,
    sram_path: &mut PathBuf,
    current: &mut Option<PathBuf>,
    mem_id: u32,
    path: PathBuf,
) {
    match fs::read(&path) {
        Ok(bytes) => {
            let n = bytes.len();
            let _ = tx.send(CoreCmd::WriteMem {
                id: MEMORY_SAVE_RAM + mem_id,
                bytes,
            });
            log::info!(
                "card {}: {} no SAVE_RAM ({n} bytes)",
                mem_id + 1,
                path.display()
            );
        }
        Err(e) => {
            log::info!(
                "card {}: {} ainda não existe ({e}) — o core formata",
                mem_id + 1,
                path.display()
            );
        }
    }
    *sram_path = path.clone();
    *current = Some(path);
}

/// How long a silent action's button shows "feito!" after firing (plan
/// revision, fixing a real report: clicking Nota gave zero on-screen
/// feedback, so a player clicked it seven times thinking nothing happened —
/// it had, every time). Long enough to register as intentional, short
/// enough to not look stuck.
const FLASH_DURATION: Duration = Duration::from_millis(900);

/// Advance `next` by one frame and wait for it: sleep the bulk of the wait,
/// then spin the last ~2ms (a plain `thread::sleep` can overshoot by more
/// than a frame's worth of jitter). Falls back to resyncing when the loop
/// fell behind, so it never spirals. Shared by runner/shelf/idle/settings.
pub(crate) fn pace_frame(next: &mut Instant, frame_time: Duration) {
    *next += frame_time;
    let now = Instant::now();
    if *next <= now {
        if polystnx_platform::trace_enabled() {
            let late = now - *next;
            if late > Duration::from_millis(2) {
                log::warn!(
                    "trace: frame atrasado {:.1} ms (loop não fechou o budget)",
                    late.as_secs_f64() * 1e3
                );
            }
        }
        *next = now; // fell behind; resync so we don't spiral
        return;
    }
    let wait = *next - now;
    if wait > Duration::from_millis(2) {
        std::thread::sleep(wait - Duration::from_millis(2));
    }
    while Instant::now() < *next {
        std::thread::yield_now();
    }
}

/// Whether `b`'s flash is still showing — `flash` maps a button to when it
/// last fired, only ever holding entries for buttons that flash at all.
fn flashed(flash: &HashMap<PanelButton, Instant>, b: PanelButton) -> bool {
    flash.get(&b).is_some_and(|t| t.elapsed() < FLASH_DURATION)
}

/// How long Reset's rocker stays "up" after a click before springing back
/// down on its own (plan revision — a momentary switch, not a toggle like
/// Power's). Shorter than `FLASH_DURATION`: a real spring-back reads as
/// snappy, not as a held state to notice.
const RESET_SPRING: Duration = Duration::from_millis(220);

/// How long the "CH 3" channel banner stays over the picture after power-on
/// (plan revision: "quando ligar o console, mostrar por 3 segundos e remover
/// da tela"). While the console is off, the static path draws it constantly
/// instead — see `Cabinet::present_static`.
const CH3_FLASH: Duration = Duration::from_secs(3);

/// Whether Reset's rocker should be drawn up right now — same `flash` map
/// the "feito!" labels use, just a different (shorter) window.
fn reset_pressed(flash: &HashMap<PanelButton, Instant>) -> bool {
    flash
        .get(&PanelButton::Reset)
        .is_some_and(|t| t.elapsed() < RESET_SPRING)
}

/// The side panel's command legend (plan §3.2, item 3; plan revision:
/// mouse/gamepad only, so labels don't carry a key name any more) — some
/// live-refreshed every frame via `Cabinet::set_commands` since their text
/// depends on state (`flash`), not just the console being on. Power/Eject/
/// Reset aren't in here: they're drawn as their own rocker-switch widgets
/// straight off `panel.powered`/`panel.reset_pressed` (see `draw_panel`/
/// `draw_rocker`). Salvar/Carregar/Printscreen no longer carry a slot
/// number in their label (plan revision — confusing next to the old
/// cyclers): clicking any of the three now opens a modal to pick one, so
/// the label just names the action. Cheats (plan revision — split out of
/// the notebook) is absent entirely when `has_cheats` is false — this
/// cartridge has no curated codes, so a button that always opened an empty
/// checklist would just be clutter.
#[allow(clippy::too_many_arguments)]
fn command_rows(
    flash: &HashMap<PanelButton, Instant>,
    has_cheats: bool,
    has_achievements: bool,
    has_discs: bool,
    all_slots_pinned: bool,
    lid_open: bool,
    disc_in: bool,
    has_library: bool,
    bios: bool,
) -> Vec<(PanelButton, String)> {
    let label = |b: PanelButton, base: &str| -> String {
        if flashed(flash, b) {
            format!("{base} (feito!)")
        } else {
            base.to_string()
        }
    };
    // Boot pela BIOS: a estante inteira vira um comando — inserir um jogo
    // é fechá-lo no drive da sessão viva, como no console real parado no
    // menu da BIOS.
    let mut rows = if bios && has_library {
        vec![(PanelButton::DiscInserter, "Estante de games".to_string())]
    } else {
        vec![(PanelButton::Notebook, "Anotações".to_string())]
    };
    if bios {
        return rows;
    }
    if has_achievements {
        rows.push((PanelButton::Achievements, "Conquistas".to_string()));
    }
    if has_cheats {
        rows.push((PanelButton::Cheats, "Cheats".to_string()));
    }
    if has_discs {
        rows.push((PanelButton::Discos, "Discos".to_string()));
    }
    // O botão da tampa: remover o disco assentado ou inserir outro — só
    // existe com a tampa aberta (a trava é física, como no console).
    if lid_open {
        if disc_in {
            rows.push((PanelButton::DiscRemover, "Remover disco".to_string()));
        } else if has_library {
            rows.push((PanelButton::DiscInserter, "Inserir disco".to_string()));
        }
    }
    rows.push((
        PanelButton::PrintScreen,
        if all_slots_pinned {
            // Every slot resists overwrite — a click here would have
            // nowhere to land, so say so instead of the normal label
            // (plan revision: "avisar quando 15 fixados").
            "Printscreen: sem espaço (15 fixados)".to_string()
        } else {
            label(PanelButton::PrintScreen, "Printscreen")
        },
    ));
    rows.push((
        PanelButton::SaveState,
        label(PanelButton::SaveState, "Salvar"),
    ));
    rows.push((
        PanelButton::LoadState,
        label(PanelButton::LoadState, "Carregar"),
    ));
    rows
}

/// Write battery SRAM to `path` if it changed since the last flush.
fn flush_sram(path: &Path, last: &mut Option<Vec<u8>>, cur: Option<Vec<u8>>) {
    if let Some(cur) = cur {
        if last.as_ref() != Some(&cur) {
            match fs::write(path, &cur) {
                Ok(_) => log::info!("SRAM flushed -> {}", path.display()),
                Err(e) => log::warn!("SRAM flush failed: {e}"),
            }
            *last = Some(cur);
        }
    }
}

/// Desligar (plan §3.3): a short burst of RF snow through the tube with a
/// decaying buzz, settling to a dim near-still hiss — never a full-screen
/// flash, and the noise cuts rather than lingers.
fn power_off_burst(plat: &Platform, cab: &mut Cabinet) -> f32 {
    const RATE: u32 = 22_050;
    const SPAN: Duration = Duration::from_millis(650);
    // The real power-switch foley replaces the synthesized buzz (plan
    // revision); the visual burst plays either way.
    let has_sfx = crate::sfx::play(cab, crate::sfx::Sfx::PowerOff);
    let audio = (!has_sfx).then(|| plat.open_audio(RATE).ok()).flatten();
    let frame = Duration::from_millis(16);
    let mut rng: u32 = 0x1234_5678;
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        // Entrada composta: a imagem colapsa para o preto e assenta no azul
        // escuro de repouso — sem flash (o brilho era ficção de RF).
        let level = OFF_STATIC_LEVEL * (1.0 - t);
        cab.present_static(level);

        if let Some(a) = &audio {
            let n = (RATE / 60) as usize;
            let amp = ((1.0 - t) * 8000.0) as i32;
            let mut buf = Vec::with_capacity(n * 2);
            for _ in 0..n {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let s = (((rng >> 8) & 0xFFFF) as i32 - 0x8000) * amp / 0x8000;
                let v = s.clamp(-32000, 32000) as i16;
                buf.push(v);
                buf.push(v);
            }
            a.queue(&buf);
        }
        std::thread::sleep(frame);
    }
    if let Some(a) = &audio {
        a.clear(); // buzz cut, not fade-out tail
    }
    OFF_STATIC_LEVEL
}

/// Ligar de novo: the mirror of `power_off_burst` — snow clears from a dim
/// hiss back up to a brief bright burst, then cuts to the game resuming
/// exactly where it was paused.
fn power_on_burst(plat: &Platform, cab: &mut Cabinet) {
    const RATE: u32 = 22_050;
    const SPAN: Duration = Duration::from_millis(120);
    let has_sfx = crate::sfx::play(cab, crate::sfx::Sfx::PowerOn);
    let audio = (!has_sfx).then(|| plat.open_audio(RATE).ok()).flatten();
    let frame = Duration::from_millis(16);
    let mut rng: u32 = 0x8765_4321;
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        // Ligar na entrada composta: o brilho CAI para o preto e o quadro do
        // jogo chega em seguida — a TV "trocando de entrada".
        let level = OFF_STATIC_LEVEL * (1.0 - t);
        cab.present_static(level);

        if let Some(a) = &audio {
            let n = (RATE / 60) as usize;
            let amp = (t * 8000.0) as i32;
            let mut buf = Vec::with_capacity(n * 2);
            for _ in 0..n {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let s = (((rng >> 8) & 0xFFFF) as i32 - 0x8000) * amp / 0x8000;
                let v = s.clamp(-32000, 32000) as i16;
                buf.push(v);
                buf.push(v);
            }
            a.queue(&buf);
        }
        std::thread::sleep(frame);
    }
    if let Some(a) = &audio {
        a.clear();
    }
}

/// The session clock's live value (plan revision: "considerar o tempo que o
/// jogo esta rodando, com o power ligado") — `elapsed` is what's already
/// banked from earlier power-on stretches this session, `since` is when the
/// current one began (`None` while powered off, in which case the clock is
/// just frozen at `elapsed`).
fn live_session_time(elapsed: Duration, since: Option<Instant>) -> Duration {
    elapsed + since.map(|t| t.elapsed()).unwrap_or_default()
}

/// Ejetar with the console still on: the lock resists — a short mechanical
/// thump, nothing else (plan §3.3, "a alavanca resiste, com um clunk seco").
fn eject_clunk(plat: &Platform) {
    tone_click(plat, 90.0);
}

/// A short damped tone burst at `freq` Hz — the shared shape behind every
/// mechanical "click" in this file (`eject_clunk`'s resist-thump, and the
/// insert/eject animations' seat/unseat clicks below): a plain sine ramping
/// from full volume down to silence over ~90ms, via a continuous phase
/// accumulator so it doesn't pop at the start. A no-op if no audio device is
/// available.
fn tone_click(plat: &Platform, freq: f32) {
    const RATE: u32 = 22_050;
    let Some(audio) = plat.open_audio(RATE).ok() else {
        return;
    };
    let n = (RATE as f32 * 0.09) as usize;
    let mut buf = Vec::with_capacity(n * 2);
    let mut phase = 0f32;
    for i in 0..n {
        let env = 1.0 - i as f32 / n as f32;
        phase += freq / RATE as f32;
        let s = (phase * std::f32::consts::TAU).sin() * env * env;
        let v = (s * 12000.0) as i16;
        buf.push(v);
        buf.push(v);
    }
    audio.queue(&buf);
    std::thread::sleep(Duration::from_millis(100));
}

/// The cartridge sliding into the console's slot (plan revision: "a animação
/// deveria estar onde está o cartucho durante a gameplay, não uma
/// transição") — played in the panel's own cartridge block (`Cabinet::
/// set_cartridge_motion` driving `draw_panel_slot`), right where the
/// cartridge lives for the rest of the session: the game's art drops into
/// the slot's dark mouth with a smoothstep ease until it seats, over the
/// signal-off CRT. Silenciosa de propósito (plan revision: "remover som ao
/// encaixar e remover o disco") — o movimento é o próprio feedback. A no-op
/// with no cartridge art to animate — the slot would sit empty. Skipped for
/// a headless `--shot` capture by the caller (`spec.shot.is_none()`, same
/// reasoning `power_on_burst`/`power_off_burst` don't need — there's no
/// button to click there, so this has no live trigger to skip in the first
/// place; the guard is really about the "plain `--shot`" mode that still
/// runs the live loop for a few frames).
fn cartridge_insert_animation(cab: &mut Cabinet) {
    const SPAN: Duration = Duration::from_millis(520);
    let frame = Duration::from_millis(16);
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        cab.set_cartridge_motion(Some((t, false)));
        // Só estática — a estante NÃO volta no meio da animação (era o
        // "pisca": estática -> estante -> estática em meio segundo).
        cab.present_static(OFF_STATIC_LEVEL);
        std::thread::sleep(frame);
    }
    cab.set_cartridge_motion(None);
    cab.present_static(OFF_STATIC_LEVEL);
}

/// The mirror of `cartridge_insert_animation`, played right as an
/// already-off cartridge actually leaves (`UiEvent::Eject`'s second branch,
/// plan revision: "criar animacao de... ejetar cartucho") — the same panel
/// slot in reverse: the cartridge pops up out of the mouth and rises clear
/// of the block, easing out as it goes. Silenciosa de propósito (plan
/// revision: "remover som ao encaixar e remover o disco"). Also a no-op
/// with no cartridge art. `cab.clear_panel()` on the way to the idle screen
/// right after drops the panel entirely, so there's no stuck mid-motion
/// state left over to reset here.
fn cartridge_eject_animation(cab: &mut Cabinet) {
    const SPAN: Duration = Duration::from_millis(430);
    let frame = Duration::from_millis(16);
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        cab.set_cartridge_motion(Some((t, true)));
        cab.present_static(OFF_STATIC_LEVEL);
        std::thread::sleep(frame);
    }
}

/// One `0`/`1` per line, in the curated list's order. Missing/short/garbled
/// files just mean "start with everything off" — nothing to migrate.
fn load_cheat_state(path: &Path, len: usize) -> Vec<bool> {
    let mut state = vec![false; len];
    if let Ok(text) = fs::read_to_string(path) {
        for (slot, line) in state.iter_mut().zip(text.lines()) {
            *slot = line.trim() == "1";
        }
    }
    state
}

fn save_cheat_state(path: &Path, state: &[bool]) {
    let text: String = state
        .iter()
        .map(|&on| if on { "1\n" } else { "0\n" })
        .collect();
    if let Err(e) = fs::write(path, text) {
        log::warn!("cheat state flush failed: {e}");
    }
}

/// `(description, on)` pairs for the panel — cheap enough to rebuild on every
/// toggle/navigate, there are only ever a handful.
fn cheat_rows(defs: &[polystnx_domain::CheatDef], state: &[bool]) -> Vec<(String, bool)> {
    defs.iter()
        .zip(state)
        .map(|(d, &on)| (d.desc.to_string(), on))
        .collect()
}

/// `cheat_rows`' pairs, formatted for the Cheats modal's row grid (plan
/// revision — its own menu, not a page in the notebook): every row stays
/// `enabled: true` (toggling is always available, unlike a save/load/print
/// slot that can be dimmed), with an `[x]`/`[ ]` mark standing in for the
/// on/off state a modal row otherwise has no way to show.
fn cheat_modal_rows(cheats: &[(String, bool)]) -> Vec<(String, bool)> {
    cheats
        .iter()
        .map(|(desc, on)| (format!("{}{desc}", if *on { "[x] " } else { "[ ] " }), true))
        .collect()
}

/// Decode scraped art (panel logo, note thumbnail, …) small enough for its
/// slot, keeping alpha for transparent logos.
fn decode_art(path: &Path, max: u32) -> Result<(u32, u32, Vec<u8>)> {
    let img = image::open(path)?.thumbnail(max, max).to_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

/// Decode a core frame's raw pixels into plain RGB8 — the un-warped, un-NTSC'd
/// picture, not what's on screen, so a captured password stays legible on the
/// page (plan §3.4: "é como se fazia no papel").
fn frame_to_rgb8(frame: &EmuFrame) -> image::RgbImage {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let mut img = image::RgbImage::new(frame.width, frame.height);
    for y in 0..h {
        let row = &frame.pixels[y * frame.pitch..];
        for x in 0..w {
            let rgb = match frame.format {
                EmuFormat::Rgb565 => {
                    let px = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let (r5, g6, b5) = ((px >> 11) & 0x1F, (px >> 5) & 0x3F, px & 0x1F);
                    [
                        ((r5 << 3) | (r5 >> 2)) as u8,
                        ((g6 << 2) | (g6 >> 4)) as u8,
                        ((b5 << 3) | (b5 >> 2)) as u8,
                    ]
                }
                EmuFormat::Rgb1555 => {
                    let px = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let (r5, g5, b5) = ((px >> 10) & 0x1F, (px >> 5) & 0x1F, px & 0x1F);
                    [
                        ((r5 << 3) | (r5 >> 2)) as u8,
                        ((g5 << 3) | (g5 >> 2)) as u8,
                        ((b5 << 3) | (b5 >> 2)) as u8,
                    ]
                }
                // XRGB8888, little-endian bytes: B, G, R, X.
                EmuFormat::Xrgb8888 => {
                    let o = x * 4;
                    [row[o + 2], row[o + 1], row[o]]
                }
            };
            img.put_pixel(x as u32, y as u32, image::Rgb(rgb));
        }
    }
    img
}

/// Fixed note slots per game (plan revision, replacing an open-ended
/// timestamped list): a screenshot into the notebook always lands in one of
/// these, chosen by the player, same shape as the save-state slots.
const NOTE_SLOTS: u8 = 15;
/// Max characters a slot's caption may hold — short, it's a label, not a
/// page (contrast `NOTE_CHAR_LIMIT` for the free-text notebook page).
const SLOT_NAME_LIMIT: usize = 40;

/// Which of three things the app's one text editor is currently editing
/// (plan revision) — they share all the input-polling plumbing, just write
/// to a different place, have a different character limit, and (for
/// `PrintName`) render in the modal instead of the notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteEdit {
    None,
    /// Editing the notebook's currently-shown text-note slot (plan
    /// revision — used to always append a fresh, blank page to an
    /// unbounded, unreadable log; now edits whichever of the 15 slots the
    /// left page is showing, pre-filled with its saved content). Saving an
    /// empty draft deletes the slot instead of writing one.
    Text,
    /// Renaming the notebook's currently-shown print slot's caption (from
    /// "Nomear print" on the notebook's right page).
    SlotName,
    /// Naming the print just captured into this slot (plan revision — the
    /// Printscreen modal's second step): unlike `SlotName`, committing this
    /// one also writes `runner::print_capture`'s image to disk for the first
    /// time — the capture and the name land together.
    PrintName(u8),
    /// Renomear um memory card (o alvo vive em `card_rename`) — commit
    /// renomeia o `.mcr` na biblioteca.
    CardRename,
    /// Typing the Cheats modal's search filter (plan revision — the
    /// libretro-database expansion made some games' lists long enough that
    /// finding one by eye/scroll alone stopped being practical). Unlike
    /// every other `NoteEdit`, committing this one writes nothing to
    /// disk — it just updates `Cabinet`'s in-memory filter and returns to
    /// the Cheats modal's row grid rather than closing it.
    CheatSearch,
}

impl NoteEdit {
    fn limit(self) -> usize {
        match self {
            NoteEdit::None => 0,
            NoteEdit::Text => NOTE_CHAR_LIMIT,
            NoteEdit::SlotName
            | NoteEdit::PrintName(_)
            | NoteEdit::CheatSearch
            | NoteEdit::CardRename => SLOT_NAME_LIMIT,
        }
    }

    fn heading(self) -> &'static str {
        match self {
            NoteEdit::None => "",
            NoteEdit::Text => "editando anotação (vazio apaga, clique fora cancela)",
            NoteEdit::SlotName => "renomeando o print (clique fora cancela)",
            NoteEdit::PrintName(_) => "nome do print (opcional)",
            NoteEdit::CheatSearch => "buscar cheat (vazio mostra todos)",
            NoteEdit::CardRename => "renomear memory card (clique fora cancela)",
        }
    }
}

/// A save/load-state or print slot picker, or the cheats checklist (plan
/// revision) — mutually exclusive with `paused` (the notebook has no
/// slot-picking or cheats of its own any more) and gates gameplay stepping
/// the same way `paused` does, so the frozen frame behind the dialog doesn't
/// keep moving while a choice is pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modal {
    None,
    SaveSlot,
    LoadSlot,
    /// Picking which of the 15 note slots to save the just-grabbed print
    /// into — see `runner::print_capture`.
    PrintSlot,
    /// The curated cheats checklist (plan revision — split out of the
    /// notebook into its own menu). Unlike the other three, a pick
    /// (`ModalPick`) toggles a row in place and leaves the modal open
    /// instead of closing it.
    Cheats,
    /// The achievements list (plan: `docs/plano-retroachievements.md`,
    /// fase 4) — read-only rows with the earned state; a pick just closes.
    Achievements,
    /// A biblioteca de memory cards (plano §3): os `.mcr` de `memcards/` —
    /// escolher um encaixa no slot 1 (só desligado; a trava é no
    /// `OpenCards`), e a primeira linha cria um cartão novo.
    Cards,
    /// A mesma biblioteca, encaixando no slot 2 (os botões "MC slot 1/2").
    Cards2,
    /// As ações sobre o card escolhido (usar/renomear/apagar) — o alvo e o
    /// slot vivem em `card_action`.
    CardsAction,
    /// A confirmação do apagar (destrutivo — nunca apaga direto).
    CardsConfirm,
    /// O seletor de discos de um jogo m3u (plano §6): troca o disco no
    /// drive com a sessão viva, estado serializado por baixo.
    Discos,
    /// A troca quente: escolher QUALQUER jogo da estante com a tampa aberta
    /// e o console ligado (plano revision: "trocar disco com o power ligado").
    Inserir,
}

/// One note slot's protection/caption (plan revision) — everything defaults
/// to "untouched" (not pinned, no caption) for a slot nobody's set either on.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct SlotMeta {
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    label: String,
}

/// `notes_dir/<title>/slots.json`'s shape: which of the 15 photo slots and
/// which of the 15 text-note slots (plan revision — the two are numbered
/// independently, slot 3's photo and slot 3's text share nothing but a
/// number) are pinned or captioned. Absent entries mean the default
/// (`SlotMeta::default()`) — most slots never need a real entry.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct NotesMeta {
    #[serde(default)]
    slots: std::collections::BTreeMap<u8, SlotMeta>,
    #[serde(default)]
    text_slots: std::collections::BTreeMap<u8, SlotMeta>,
}

impl NotesMeta {
    fn slot(&self, slot: u8) -> SlotMeta {
        self.slots.get(&slot).cloned().unwrap_or_default()
    }

    fn text_slot(&self, slot: u8) -> SlotMeta {
        self.text_slots.get(&slot).cloned().unwrap_or_default()
    }
}

/// Every one of the 15 slots resists overwrite — Printscreen has nowhere
/// left to put a new capture (plan revision: "avisar quando 15 fixados").
fn all_slots_pinned(meta: &NotesMeta) -> bool {
    (1..=NOTE_SLOTS).all(|s| meta.slot(s).pinned)
}

/// Rows for the Printscreen modal's slot-picking step (plan revision): one
/// per note slot, in order, labelled with its number plus whether it's
/// filled/pinned; a pinned slot is disabled, same protection `NoteCapture`
/// used to enforce by auto-redirecting instead.
fn print_slot_rows(notes_dir: &Path, title: &str, meta: &NotesMeta) -> Vec<(String, bool)> {
    (1..=NOTE_SLOTS)
        .map(|s| {
            let m = meta.slot(s);
            let filled = note_slot_path(notes_dir, title, s).exists();
            let label = if m.pinned {
                format!("Slot {s} (fixado)")
            } else if filled {
                format!("Slot {s}")
            } else {
                format!("Slot {s} (vazio)")
            };
            (label, !m.pinned)
        })
        .collect()
}

fn notes_meta_path(notes_dir: &Path, title: &str) -> PathBuf {
    note_dir(notes_dir, title).join("slots.json")
}

/// Missing or unreadable is just "nothing pinned or captioned yet", not an
/// error — same tolerant read as the rest of this portable app's sidecars.
fn load_notes_meta(notes_dir: &Path, title: &str) -> NotesMeta {
    fs::read_to_string(notes_meta_path(notes_dir, title))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_notes_meta(notes_dir: &Path, title: &str, meta: &NotesMeta) {
    let dir = note_dir(notes_dir, title);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(s) = serde_json::to_string_pretty(meta) {
        if let Err(e) = fs::write(notes_meta_path(notes_dir, title), s) {
            log::warn!("saving note slot metadata failed: {e}");
        }
    }
}

/// `notes_dir/<game title>/` — one folder per game, named for a human
/// browsing it rather than the ROM hash (plan revision: readability over
/// rename-proofing, since the player picked this explicitly). Mirrors
/// `game_dir` (`saves/<title>/`) — same sanitizing, same tradeoff.
fn note_dir(notes_dir: &Path, title: &str) -> PathBuf {
    notes_dir.join(sanitize_dir_name(title))
}

/// `notes_dir/<title>/01.png` .. `15.png` — 1-indexed to match the slot
/// numbers shown on screen.
fn note_slot_path(notes_dir: &Path, title: &str, slot: u8) -> PathBuf {
    note_dir(notes_dir, title).join(format!("{slot:02}.png"))
}

/// `notes_dir/<title>/notas.txt` — the old, pre-revision unbounded
/// free-text append log. Nothing writes here any more (see
/// `note_text_slot_path`, the 15-slot replacement) — this path only still
/// exists so `migrate_legacy_text_notes` can find and split up whatever an
/// earlier version of the app already wrote there.
fn legacy_note_text_path(notes_dir: &Path, title: &str) -> PathBuf {
    note_dir(notes_dir, title).join("notas.txt")
}

/// `notes_dir/<title>/01.txt` .. `15.txt` — 1-indexed to match the slot
/// numbers shown on screen, numbered independently of the photo slots
/// (`note_slot_path`) even though they share the same range: a `.png` and
/// a `.txt` never collide, so slot 3's photo and slot 3's text are just two
/// unrelated files that happen to both say "3".
fn note_text_slot_path(notes_dir: &Path, title: &str, slot: u8) -> PathBuf {
    note_dir(notes_dir, title).join(format!("{slot:02}.txt"))
}

/// Save the current frame into note `slot` (1..=15), overwriting whatever
/// was there before — same "pick a slot, it replaces what's in it" model as
/// save states.
fn save_note_image(notes_dir: &Path, title: &str, slot: u8, frame: &EmuFrame) -> Result<()> {
    let dir = note_dir(notes_dir, title);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = note_slot_path(notes_dir, title, slot);
    frame_to_rgb8(frame)
        .save(&path)
        .with_context(|| format!("saving note image {}", path.display()))
}

/// Text-note `slot`'s saved content (plan revision — one of the 15,
/// mirroring the photo slots), or `None` for an empty slot — a
/// whitespace-only file counts as empty too, same as an empty draft never
/// got saved as one to begin with.
fn read_text_slot(notes_dir: &Path, title: &str, slot: u8) -> Option<String> {
    fs::read_to_string(note_text_slot_path(notes_dir, title, slot))
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Save `text` into note-text `slot` (1..=15), overwriting whatever was
/// there before — same "pick a slot, it replaces what's in it" model
/// `save_note_image` already uses for the photo slots.
fn save_text_slot(notes_dir: &Path, title: &str, slot: u8, text: &str) -> Result<()> {
    let dir = note_dir(notes_dir, title);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = note_text_slot_path(notes_dir, title, slot);
    fs::write(&path, text).with_context(|| format!("saving text note {}", path.display()))
}

/// Clear note-text `slot` — a missing file already means "empty", so this
/// is not an error either way.
fn delete_text_slot(notes_dir: &Path, title: &str, slot: u8) {
    let _ = fs::remove_file(note_text_slot_path(notes_dir, title, slot));
}

/// One-time upgrade from the old unbounded `notas.txt` append log into the
/// new 15 numbered slots (plan revision — "I wrote something and can't see
/// it again" was a real complaint: the old log had no viewer at all, just
/// a button that always appended a blank page). A no-op the moment any
/// text slot already exists, so a game already using the new format is
/// never re-split or overwritten. Entries in the old log were separated by
/// a blank line (`append_note_text` used to write `"{text}\n\n"`); the
/// first 15 non-empty ones become slots 1..=15, in the order they were
/// written. `notas.txt` itself is left in place afterward rather than
/// deleted — cheap insurance against a bug here losing anyone's notes
/// outright — but nothing reads it again once this has run once.
fn migrate_legacy_text_notes(notes_dir: &Path, title: &str) {
    if (1..=NOTE_SLOTS).any(|s| note_text_slot_path(notes_dir, title, s).is_file()) {
        return;
    }
    let Ok(contents) = fs::read_to_string(legacy_note_text_path(notes_dir, title)) else {
        return;
    };
    let pages: Vec<&str> = contents
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if pages.is_empty() {
        return;
    }
    let mut migrated = 0;
    for (slot, page) in (1..=NOTE_SLOTS).zip(pages) {
        match save_text_slot(notes_dir, title, slot, page) {
            Ok(()) => migrated += 1,
            Err(e) => log::warn!("migrating legacy note into slot {slot} failed: {e}"),
        }
    }
    log::info!("notas.txt: migrated {migrated} page(s) into numbered text-note slots");
}

/// Panel display cap for a pinned text note (plan revision) — the full
/// slot can run up to `NOTE_CHAR_LIMIT`, way more than the side panel has
/// comfortable room for alongside everything else already in it. Mirrors
/// `note_thumb`'s own smaller size (200px) for the panel vs. the full
/// 900px the pause book gets.
const PANEL_NOTE_SNIPPET_CHARS: usize = 120;

/// Truncate `text` to at most `max_chars`, with a trailing `...` if it
/// didn't already fit — see `PANEL_NOTE_SNIPPET_CHARS`.
fn panel_text_snippet(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{head}...")
}

/// Recompute the panel's notebook block from what's actually on disk: how
/// many of the 15 print slots are pinned (plan revision — used to be how
/// many were filled, shown regardless of pin; a slot nobody deliberately
/// featured showing up in the always-visible panel anyway was the actual
/// complaint), a thumbnail if there's a pinned one to show, and a pinned
/// text note's content if there's one of those too — independent of the
/// photo side ("mostrar apenas uma nota e/ou uma imagem"), so either, both,
/// or neither can end up in the panel. Both sides pick the same way: the
/// currently-viewed slot if it's itself pinned, else the lowest-numbered
/// pinned one, else none. Call at game start, after every capture/write,
/// after cycling either slot, and after every pin toggle (either kind) —
/// cheap, at most 30 file-exists checks.
fn refresh_notes(
    cab: &mut Cabinet,
    notes_dir: &Path,
    title: &str,
    slot: u8,
    text_slot: u8,
    meta: &NotesMeta,
) {
    let count = (1..=NOTE_SLOTS).filter(|&s| meta.slot(s).pinned).count();
    let thumb_slot = if meta.slot(slot).pinned {
        Some(slot)
    } else {
        (1..=NOTE_SLOTS).find(|&s| meta.slot(s).pinned)
    };
    let thumb = thumb_slot.and_then(|s| note_thumb(notes_dir, title, s, 200));

    let text_pin_slot = if meta.text_slot(text_slot).pinned {
        Some(text_slot)
    } else {
        (1..=NOTE_SLOTS).find(|&s| meta.text_slot(s).pinned)
    };
    let text = text_pin_slot
        .and_then(|s| read_text_slot(notes_dir, title, s))
        .map(|t| panel_text_snippet(&t, PANEL_NOTE_SNIPPET_CHARS));

    cab.set_notes(
        count,
        thumb.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
        text.as_deref(),
    );
}

/// Decode note `slot`'s image if it's filled, `max` pixels on the long
/// side — `None` for an empty slot or a decode failure either way.
fn note_thumb(notes_dir: &Path, title: &str, slot: u8, max: u32) -> Option<(u32, u32, Vec<u8>)> {
    let path = note_slot_path(notes_dir, title, slot);
    path.is_file()
        .then(|| decode_art(&path, max))
        .and_then(Result::ok)
}

/// Push note `slot`'s current image, pin state, and caption onto the pause
/// book's right page — the `set_pause_page` call every slot-change (or
/// pin/rename) site needs.
fn show_note_slot(cab: &mut Cabinet, notes_dir: &Path, title: &str, slot: u8, meta: &NotesMeta) {
    let thumb = note_thumb(notes_dir, title, slot, 900);
    let m = meta.slot(slot);
    cab.set_pause_page(
        (slot - 1) as usize,
        m.pinned,
        &m.label,
        thumb.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
    );
}

/// Push text-note `slot`'s saved content and pin state onto the pause
/// book's left page (plan revision, mirrors `show_note_slot`) — the
/// `set_pause_text_page` call every slot-change (or pin/write/delete)
/// site needs.
fn show_text_slot(cab: &mut Cabinet, notes_dir: &Path, title: &str, slot: u8, meta: &NotesMeta) {
    let content = read_text_slot(notes_dir, title, slot);
    let pinned = meta.text_slot(slot).pinned;
    cab.set_pause_text_page((slot - 1) as usize, pinned, content.as_deref());
}

/// Load the core + ROM and run until the player leaves, drawing into `cab` (the
/// one persistent window). `plat` and `cab` both outlive the call so `polystnx`
/// can reuse them for the next screen.
/// Guarda o quadro no buffer reutilizado do `last_frame` (565 converte
/// para XRGB8888; 888 copia como veio).
fn store_into(frame: &EmuFrame, buf: &mut Vec<u8>) {
    buf.clear();
    if frame.format == EmuFormat::Rgb565 {
        buf.reserve(frame.width as usize * frame.height as usize * 4);
        for chunk in frame.pixels.as_chunks::<2>().0 {
            let v = u16::from_le_bytes([chunk[0], chunk[1]]);
            let r = (((v >> 11) & 0x1f) * 255 / 31) as u8;
            let g = (((v >> 5) & 0x3f) * 255 / 63) as u8;
            let b = ((v & 0x1f) * 255 / 31) as u8;
            buf.extend_from_slice(&[b, g, r, 255]);
        }
    } else {
        buf.extend_from_slice(&frame.pixels);
    }
}

pub fn run_game(
    plat: &mut Platform,
    cab: &mut Cabinet,
    spec: &GameSpec,
    cfg: &Config,
) -> Result<GameExit> {
    let runahead_cfg = spec.runahead.unwrap_or(cfg.runahead);

    // --- load + identify -------------------------------------------------
    // IGUAL AO SNES: nada é apresentado aqui — a tela atual (a estante ou
    // a tela inicial) fica CONGELADA durante o carregar do core/disco
    // (1-2s). Trocar para estática no clique, e voltar a estante no meio
    // da animação, era o que fazia a tela "piscar" / parecer uma nova
    // tela por cima.
    cab.set_powered(false);
    let mut core =
        Core::load(&spec.core).with_context(|| format!("loading core {}", spec.core.display()))?;
    log::info!("core: {} {}", core.system_name(), core.system_version());
    // O PCSX Rearmed persiste os cards em ARQUIVOS próprios (pcsx-cardN.mcd
    // no save dir) e usa o SAVE_RAM só como espelho de saída — escrever nele
    // perde conteúdo. Os flushes por SAVE_RAM ficam para cores que o usam
    // como fonte (SwanStation via --core).
    let core_cards_via_file = core.system_name().to_lowercase().contains("pcsx");
    core.set_directories(&spec.system_dir, &spec.save_dir);
    core.init();
    // O PCSX Rearmed renderiza por software nativamente — o pipeline do app
    // é 2D (framebuffer → tubo), nada de contexto de GPU.
    // O card do SLOT 2 não é exposto pelo protocolo (só o id 0 existe) —
    // ligamos o segundo card do core (plan revision: "ligando o card do
    // slot 2 aos arquivos que o core lê") e sincronizamos por arquivo.
    core.set_variable("pcsx_rearmed_memcard2", "enabled");
    // Console de verdade: a intro do logo do PlayStation TOCA ao ligar (o
    // Rearmed vem com ela desligada) e o áudio de CD-DA fica ligado (o
    // Rearmed vem com `nocdaudio` ligado).
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "enabled");
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    // A ponta de injeção: o card escolhido para o slot 2 é copiado para o
    // arquivo do core ANTES do load (o core o carrega no boot do jogo).
    if let Some(card2) = &spec.card2 {
        let shared = std::path::Path::new(&spec.save_dir).join(MC2_SHARED_FILE);
        if std::fs::copy(card2, &shared).is_ok() {
            log::info!("card 2: {} no arquivo do core", card2.display());
        }
    }

    // Identidade do disco para o log — o serial lido de dentro do CHD
    // (a chave da estante; o core recebe o caminho, disco é need_fullpath).
    if spec.bios {
        log::info!("sem disco — boot na BIOS");
    } else {
        match polystnx_domain::DiscId::from_path(&spec.rom) {
            Ok(id) => log::info!("disco: {} ({} disco/s)", id.serial, id.discs),
            Err(e) => log::warn!("disco: {e}"),
        }
    }

    // Discos não cabem em RAM: o core recebe o caminho (need_fullpath) e
    // lê o CHD por conta própria.
    // Sem disco: boot direto na BIOS (menu do console).
    // Per-game persistence — one folder per game, named for the title like
    // notes already are (plan revision — used to be a flat file per kind,
    // keyed by ROM hash: unreadable next to a folder a player might actually
    // open, and the hash bought rename-proofing nobody asked for here).
    let title = if spec.bios {
        "BIOS".to_string()
    } else {
        rom_title(&spec.rom)
    };
    fs::create_dir_all(game_dir(&spec.save_dir, &title)).ok();
    // O save mora no card físico quando há um card no slot 1 (plano §3) —
    // insert, flush e tudo mais passam a apontar para o arquivo do card.
    // Mutável: a biblioteca de cards troca o encaixado com o console off.
    let mut current_card = spec.card1.clone();
    let mut sram_path = spec
        .card1
        .clone()
        .unwrap_or_else(|| sram_file(&spec.save_dir, &title));
    let initial_sram_path = sram_path.clone();
    // Slot 2: vazio até o jogador escolher um card no botão "MC slot 2" —
    // sem card, nada é lido nem gravado (o core formata o dele em memória).
    let mut current_card2 = spec.card2.clone();
    let mut sram2_path = spec.card2.clone().unwrap_or_default();
    let mut last_sram2: Option<Vec<u8>> = None;

    // CARD 1 vai para o core pelo ARQUIVO dele (pcsx-card1.mcd no save
    // dir), semeado ANTES do load — o Rearmed lê o card do arquivo no boot
    // e descarrega de volta nele ao desligar. O SAVE_RAM desse core é só
    // um espelho de saída: escrever nele (antes OU depois do load) perde o
    // conteúdo — o core empurra o próprio estado por cima (provado em
    // probe: 122 KB sobrescritos numa sessão de 60 s).
    if !spec.bios && sram_path.exists() {
        let seed = std::path::Path::new(&spec.save_dir).join(MC1_SHARED_FILE);
        match std::fs::copy(&sram_path, &seed) {
            Ok(_) => log::info!("card 1: {} semeado no arquivo do core", sram_path.display()),
            Err(e) => log::warn!("card 1: semeando {e}"),
        }
    }

    if spec.bios {
        if core.load_bios().is_err() {
            // Nem todo core aceita boot sem disco (o PCSX Rearmed rejeita
            // `retro_load_game(null)`): explica na TV e volta à idle, em vez
            // de morrer numa tela morta.
            log::info!("core não inicia a BIOS sem disco");
            cab.push_osd(
                &["ESTE CORE NÃO INICIA A BIOS", "INSIRA UM DISCO (OPEN)"],
                None,
                Duration::from_secs(4),
            );
            return Ok(GameExit::Ejected {
                static_level: OFF_STATIC_LEVEL,
            });
        }
    } else {
        core.load_game(&spec.rom, &[])
            .context("core rejected the disc")?;
    }
    // Painel: o nome canônico do DAT quando há — os arquivos continuam
    // chaveados pelo stem.
    let panel_title = spec.display_title.clone().unwrap_or_else(|| title.clone());
    // Note slot (fixed 1..=15, not save-state's 0..=9) — which of the 15
    // the notebook's right page shows; Prev/Next move it while paused
    // there, and picking a print's destination in the Printscreen modal
    // moves it too, so the notebook opens on whatever was captured last.
    let mut note_slot: u8 = 1;
    // Text-note slot (plan revision) — same idea as `note_slot`, but for
    // the left page's 15 text notes; the two cursors are independent, so
    // paging through prints never moves which text slot is shown, or the
    // other way around.
    let mut text_slot: u8 = 1;

    // Card novo (sem arquivo ainda): o que o core formata vira arquivo na
    // biblioteca já — o cartão existe antes de o jogo salvar nele (o flush
    // de changed-only nunca dispararia para um card intocado).
    if spec.card1.is_some() && !sram_path.exists() {
        match core.sram() {
            Some(bytes) => match fs::write(&sram_path, &bytes) {
                Ok(_) => log::info!(
                    "card 1: {} criado ({} bytes, formatado pelo core)",
                    sram_path.display(),
                    bytes.len()
                ),
                Err(e) => log::warn!("card 1: criando {}: {e}", sram_path.display()),
            },
            None => log::warn!("card 1: core não expõe SAVE_RAM; card novo fica em memória"),
        }
    }

    // Sem filtro composto no PSX (plano §2): o frame do core vai direto
    // para a tubo; o filtro do blargg é do irmão de SNES.

    let av = core.av_info();
    log::info!(
        "av: {}x{} (max {}x{}) aspect={:.3} fps={:.3} sr={:.0}",
        av.base_width,
        av.base_height,
        av.max_width,
        av.max_height,
        av.aspect_ratio,
        av.fps,
        av.sample_rate
    );

    // Which note slots are pinned/captioned (plan revision) — loaded once,
    // mutated and re-saved in place on every pin toggle or rename.
    let mut notes_meta = load_notes_meta(&spec.notes_dir, &title);
    // One-time, no-op after the first run for this game — see the
    // function's own doc comment.
    migrate_legacy_text_notes(&spec.notes_dir, &title);

    // --- cheats: the full libretro-database slice for this title (plan
    // §4.4, revision) --- Matched by the ROM's own title (same string
    // saves/notes are keyed by), not the cartridge header any more — see
    // `polystnx_domain::cheats`'s doc comment for why. Empty if nothing in
    // the database lines up with it. Loaded before the side panel below,
    // since its command legend needs to know whether to show the Cheats
    // button at all (plan revision).
    let cheat_defs = polystnx_domain::cheats_for_title(&title);
    let cheat_path = cheat_state_path(&spec.save_dir, &title);
    let mut cheat_state = load_cheat_state(&cheat_path, cheat_defs.len());
    // RetroAchievements hardcore (plan fase 3): cheats stay off entirely.
    // A sessão exige a conta conectada (token Connect): sem ela não há
    // lógica de conquista para ativar nem envio para o servidor.
    let ra_on = !cfg.ra_user.is_empty() && cfg.ra_connect.is_some();
    let ra_hardcore_active = ra_on && cfg.ra_hardcore;
    if ra_hardcore_active && !cheat_defs.is_empty() {
        log::info!("ra hardcore: cheats disabled for this session");
        cheat_state = vec![false; cheat_defs.len()];
    }
    if !cheat_defs.is_empty() {
        core.cheat_reset();
        for (i, (def, &on)) in cheat_defs.iter().zip(&cheat_state).enumerate() {
            core.cheat_set(i as u32, on, def.code);
        }
    }
    // --- side panel: logo, cartridge art, command legend, session timer
    // (plan §3.2) — cartridge art is new (plan revision): a second, optional
    // image alongside the logo, same local-file convention.
    // RetroAchievements session (plan: `docs/plano-retroachievements.md`,
    // fase 3) — armed only when the account is configured; identification
    // and cache reads happen here, on cart insert.
    let mut ra_session = if ra_on {
        match crate::ra::Active::start(
            &spec.rom,
            &cfg.ra_user,
            cfg.ra_connect.as_deref().unwrap_or(""),
            cfg.ra_hardcore,
        ) {
            Ok(Some(a)) => {
                log::info!(
                    "ra: {} — {} conquista(s), hardcore {}",
                    a.game_title,
                    a.achievements().len(),
                    if a.hardcore { "on" } else { "off" }
                );
                Some(a)
            }
            Ok(None) => None,
            Err(e) => {
                log::info!("ra: {e}");
                None
            }
        }
    } else {
        None
    };
    let ra_hardcore_active = ra_session.as_ref().is_some_and(|a| a.hardcore);
    // Earned do servidor para a sessão (uma rede só na primeira vez — o
    // disco cacheia por game id): conquistas ganhas fora deste app entram
    // no set da sessão quando o worker chega, e a modal in-game as marca.
    let mut ra_earned_worker = match (&ra_session, ra_on) {
        (Some(a), true) => Some(crate::ra::fetch_game_earned_worker(
            &cfg.ra_user,
            &cfg.ra_token,
            &a.hash,
        )),
        _ => None,
    };
    // Os badges do set em cache antes da primeira notificação (worker) — o
    // bloco "CONQUISTA DESBLOQUEADA" sai com a imagem de primeira.
    if ra_session.is_some() {
        crate::ra::prefetch_badges(&spec.rom);
    }

    // "Done!" flash for otherwise-silent actions (Nota/Salvar/Carregar) —
    // see `flashed`/`FLASH_DURATION`.
    let mut flash: HashMap<PanelButton, Instant> = HashMap::new();
    // The command legend's last drawn signature (a "(feito!)" flash active?
    // all print slots pinned?) — `None` forces the first frame to draw it.
    let mut prev_sig: Option<(bool, bool, bool, bool)> = None;
    let _ = &prev_sig;
    // Os discos do jogo (m3u, plano §6): `Some` com 2+ discos liga a linha
    // "Discos" dos comandos; o disco corrente começa no primeiro.
    let discs: Option<Vec<PathBuf>> = (spec
        .rom
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("m3u"))
        .unwrap_or(false))
    .then(|| {
        polystnx_domain::disc::playlist(&spec.rom)
            .map_err(|e| log::warn!("m3u {}: {e}", spec.rom.display()))
            .ok()
    })
    .flatten()
    .filter(|d| d.len() > 1);
    let mut current_disc = discs
        .as_ref()
        .and_then(|d| d.first().cloned())
        .unwrap_or_else(|| spec.rom.clone());
    // Drive: tampa translúcida (OPEN abre/fecha sem desligar) e disco
    // presente (removível só com a tampa aberta). Boot pela BIOS: o drive
    // começa VAZIO — o jogo entra pelo botão "Estante de games" do painel.
    let mut lid_open = false;
    let mut disc_in = !spec.bios;
    // A sessão nasceu no boot da BIOS ("Ligar sem disco")? Inserir um jogo
    // pela tampa encerra o modo — o painel passa a se comportar como o de
    // um jogo aberto pela estante.
    let mut bios_session = spec.bios;
    // "Disco arranhado": remover o disco com o console ligado simula erro
    // de leitura — a imagem rasga por ~2,8 s (overlay) e congela. Reset
    // boota a BIOS; inserir outro disco destrava.
    let mut disc_glitch: Option<Instant> = None;
    // Os controles trocados de entrada (o clássico do Metal Gear: clicar na
    // entrada do console passa o pad 1 para a porta 2). O ARRASTO: press
    // numa entrada "pega" o controle, o movimento destaca o alvo e o soltar
    // na outra entrada completa a troca.
    let mut pads_swapped = false;
    let mut pad_drag: Option<u8> = None;
    let mut pad_hover: Option<u8> = None;
    // O modo analógico de cada entrada (o LED vermelho do botão ANALOG):
    // desligado zera os sticks daquela porta para o core.
    let mut analog_on = [true, true];
    // A vibração de cada entrada (o botão RUMBLE com o LED): ligada por
    // padrão; o estado alimenta o LED e o roteamento do core.
    let mut rumble_on = [true, true];
    // O disco deslizando para fora (remoção, `true`) ou de volta ao eixo
    // (inserção, `false`): (início, direção), animado no passe do jogo —
    // o drive desenhado segue `cartridge_motion`, não o estado lógico.
    let mut disc_motion: Option<(Instant, bool)> = None;
    let mut glitch_rng: u32 = 0xC0FF_EEDD;
    let commands = command_rows(
        &flash,
        !cheat_defs.is_empty(),
        ra_session.is_some(),
        discs.is_some(),
        all_slots_pinned(&notes_meta),
        false,
        true,
        !spec.library.is_empty(),
        bios_session,
    );
    let decode_panel_art = |path: &Option<PathBuf>, kind: &str| {
        path.as_ref().and_then(|p| match decode_art(p, 640) {
            Ok(mut img) => {
                // A arte do disco (a "capa" do PSX) gira na face do console:
                // mascarada num círculo inscrito, os cantos do png ficam
                // transparentes e o giro nunca passa do próprio disco.
                if kind == "disco" {
                    let (w, h) = (img.0 as i32, img.1 as i32);
                    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
                    let r = w.min(h) as f32 / 2.0;
                    // Furo central de CD de verdade (Ø15mm num disco de
                    // Ø120mm = 12,5% do raio): por ele aparece a mesa do
                    // spindle, que gira com o MESMO ângulo do disco.
                    let hole = r * 0.125;
                    for y in 0..h {
                        for x in 0..w {
                            let dx = x as f32 + 0.5 - cx;
                            let dy = y as f32 + 0.5 - cy;
                            let d2 = dx * dx + dy * dy;
                            if d2 > r * r || d2 < hole * hole {
                                img.2[(y * w + x) as usize * 4 + 3] = 0;
                            }
                        }
                    }
                }
                Some(img)
            }
            Err(e) => {
                log::warn!("{kind} {}: {e}", p.display());
                None
            }
        })
    };
    // Boot pela BIOS: o topo do painel mostra a logo pixel-art da BIOS
    // (não há jogo de onde tirar logo).
    let logo_img = if spec.bios {
        crate::console_art::bios_panel_logo()
    } else {
        decode_panel_art(&spec.logo, "logo")
    };
    // Console tag wordmark on the slot's base (plan revision) — reloaded per
    // game launch so a direct `emu-run` shows it too; the baked-in image is
    // the fallback (see `console_art`).
    crate::console_art::load_slot_tag(cab);
    crate::console_art::load_cd_reader(cab);
    crate::console_art::load_cd_seat(cab);
    let cartridge_img = decode_panel_art(&spec.cartridge, "disco");
    let has_cartridge_art = cartridge_img.is_some();
    cab.set_panel(
        logo_img.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
        cartridge_img
            .as_ref()
            .map(|(w, h, d)| (*w, *h, d.as_slice())),
        &panel_title,
        &commands,
    );
    // O nome dos cards encaixados vai impresso no adesivo das portas.
    cab.set_card_labels(
        crate::memcard::card_name(current_card.as_deref()).as_deref(),
        crate::memcard::card_name(current_card2.as_deref()).as_deref(),
    );
    // Never inherited from whatever screen ran before (idle/shelf/settings
    // all turn it on) — see `Cabinet::show_close`'s own doc comment for why
    // gameplay doesn't get one.
    cab.set_close_button(false);
    // `set_cheats` has to come *after* `set_panel` — `set_panel` replaces
    // the whole `PanelInfo` (fresh `cheats: Vec::new()` included), so
    // calling this first, as an earlier revision did when the cheats-
    // loading block moved up ahead of the panel block (for `command_rows`'
    // `has_cheats` flag), silently wiped out the count before it ever
    // reached the screen — a real bug, caught by testing the panel's
    // "N cheats ativados" line and finding it never showed up at all.
    cab.set_cheats(&cheat_rows(&cheat_defs, &cheat_state));
    cab.set_drive(lid_open, disc_in);

    // --- notes: the notebook block, empty until the first capture (§3.4) --
    fs::create_dir_all(&spec.notes_dir).ok();
    refresh_notes(
        cab,
        &spec.notes_dir,
        &title,
        note_slot,
        text_slot,
        &notes_meta,
    );

    // Headless self-check: skip straight to the idle "console off" screen.
    if spec.shot_off {
        if let Some((path, _)) = &spec.shot {
            cab.capture_static_bmp(OFF_STATIC_LEVEL, path)
                .map_err(|e| anyhow!(e.to_string()))?;
            log::info!("wrote {} (idle-off preview)", path.display());
        }
        return Ok(GameExit::Quit);
    }

    // Headless self-check: skip straight to the pause book, against whatever
    // notes already exist on disk for this ROM (build them with a separate
    // --debug-note-capture run first).
    if spec.debug_shot_pause {
        if let Some((path, _)) = &spec.shot {
            cab.set_pause_note(&title, NOTE_SLOTS as usize);
            show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
            show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
            cab.capture_pause_bmp(path)
                .map_err(|e| anyhow!(e.to_string()))?;
            log::info!("wrote {} (pause book preview)", path.display());
        }
        return Ok(GameExit::Quit);
    }

    // Headless self-check: skip straight to one of the modals (plan
    // revision) instead of gameplay — same "build the rows, capture" shape
    // as the pause book above, just picking which modal by name.
    if let Some(kind) = &spec.debug_shot_modal {
        if let Some((path, _)) = &spec.shot {
            match kind.as_str() {
                "save" => cab.set_modal("Salvar estado", &save_slot_rows(&spec.save_dir, &title)),
                "load" => cab.set_modal("Carregar estado", &load_slot_rows(&spec.save_dir, &title)),
                "print" => cab.set_modal(
                    "Onde salvar o print?",
                    &print_slot_rows(&spec.notes_dir, &title, &notes_meta),
                ),
                // Not a real step on its own (there's no row list once
                // naming starts) — previews the draft view `NoteEdit::
                // PrintName` switches to after a slot's picked.
                "print-name" => {
                    cab.set_modal("Onde salvar o print?", &[]);
                    cab.set_modal_draft(
                        Some(""),
                        NoteEdit::PrintName(1).limit(),
                        NoteEdit::PrintName(1).heading(),
                    );
                }
                "cheats" => {
                    cab.set_modal(
                        "Cheats",
                        &cheat_modal_rows(&cheat_rows(&cheat_defs, &cheat_state)),
                    );
                    cab.set_modal_searchable(true);
                }
                // Previews the search box already filtered, without a live
                // GUI to type into it — the filter word is fixed (dev/
                // testing only, not configurable from the CLI).
                "cheats-search" => {
                    cab.set_modal(
                        "Cheats",
                        &cheat_modal_rows(&cheat_rows(&cheat_defs, &cheat_state)),
                    );
                    cab.set_modal_searchable(true);
                    cab.set_modal_search("infinit");
                }
                // Previews the on/off state filter, forcing the first
                // cheat on first so "Ligados" has something to show
                // (dev/testing only — no CLI way to pick which).
                "cheats-filtered" => {
                    let mut state = cheat_state.clone();
                    if let Some(first) = state.first_mut() {
                        *first = true;
                    }
                    cab.set_modal(
                        "Cheats",
                        &cheat_modal_rows(&cheat_rows(&cheat_defs, &state)),
                    );
                    cab.set_modal_searchable(true);
                    cab.set_modal_filter(Some(true));
                }
                other => {
                    log::warn!(
                        "--debug-shot-modal {other}: unknown, expected save/load/print/print-name/cheats/cheats-search/cheats-filtered"
                    )
                }
            }
            cab.capture_modal_bmp(path)
                .map_err(|e| anyhow!(e.to_string()))?;
            log::info!("wrote {} (modal preview: {kind})", path.display());
        }
        return Ok(GameExit::Quit);
    }

    // The cartridge visibly sliding into the console's slot (plan revision),
    // right as the console goes from "nothing loaded" to "off, waiting for
    // Ligar" — skipped for a `--shot` capture (dev/testing; no button was
    // clicked to trigger it in the first place) and with no cartridge art to
    // animate.
    if spec.shot.is_none() && has_cartridge_art {
        cartridge_insert_animation(cab);
    }
    if let Some(dir) = &spec.debug_cart_anim {
        std::fs::create_dir_all(dir).map_err(|e| anyhow!(e.to_string()))?;
        for (i, t) in [0.0f32, 0.12, 0.25, 0.4, 0.55, 0.7, 0.85, 1.0]
            .iter()
            .enumerate()
        {
            cab.set_cartridge_motion(Some((*t, false)));
            cab.capture_static_bmp(OFF_STATIC_LEVEL, &dir.join(format!("cart_{i:02}.bmp")))
                .map_err(|e| anyhow!(e.to_string()))?;
        }
        cab.set_cartridge_motion(None);
        cab.capture_static_bmp(OFF_STATIC_LEVEL, &dir.join("cart_08.bmp"))
            .map_err(|e| anyhow!(e.to_string()))?;
        return Ok(GameExit::Quit);
    }

    // --- audio -----------------------------------------------------------
    let audio = plat
        .open_audio(av.sample_rate.round().max(8000.0) as u32)
        .map_err(|e| anyhow!(e.to_string()))?;
    let mut input = plat.new_input();

    let mut paused = false;
    let frame_time = Duration::from_secs_f64(1.0 / av.fps.max(1.0));
    let mut next = Instant::now();
    let mut frames: u32 = 0;
    let mut last_dims: Option<(u32, u32)> = None;
    // Don't let the audio queue run more than ~0.15 s ahead (latency creep).
    let audio_cap = (av.sample_rate / 6.0) as usize;

    // Run-ahead: only if the core actually serializes; hardcore also rules
    // it out (a speculative frame would validate a hit the rewind undoes).
    let mut runahead = runahead_cfg;
    if ra_hardcore_active && runahead > 0 {
        log::info!("ra hardcore: run-ahead disabled");
        runahead = 0;
    }
    if runahead > 0 && core.save_state().is_none() {
        log::warn!("core has no save state — run-ahead disabled");
        runahead = 0;
    }
    let mut last_sram = core.sram();
    // Which note slot to save into once the next frame is ready — set at
    // click time, consumed after `core.run()` produces a real frame.
    let mut note_request: Option<u8> = None;
    // Debug capture lines up with --shot-frame (default: the very first
    // frame) so the saved page actually shows whatever --shot is inspecting,
    // not just a black boot frame.
    let debug_note_frame = spec.shot.as_ref().map_or(1, |(_, f)| *f).max(1);
    // The console: off until the player presses Power (plan revision —
    // picking a game from the shelf only inserts the cartridge, same as
    // real hardware, it doesn't boot itself); off again after a later
    // "Desligar", idling on snow until Eject either way (plan §3.3).
    // Exception: a plain `--shot` dev capture (not `--shot-off`/
    // `--debug-shot-pause`, both of which already returned above) exists to
    // inspect live gameplay, so it starts powered — there's no Power click
    // to send it in headless mode.
    let mut powered = spec.shot.is_some();
    cab.set_powered(powered);
    // The session clock (plan revision: "no tempo da sessao considerar o
    // tempo que o jogo esta rodando, com o power ligado") counts only while
    // powered on, not wall-clock since the cartridge went in — `powered_
    // elapsed` banks whatever was accrued across earlier power-on stretches
    // this session, `powered_since` is when the current stretch began (`None`
    // while off). Also the running total this session contributes to the
    // game's all-time playtime (`total_playtime_secs`) once it ends.
    let mut powered_elapsed = Duration::ZERO;
    let mut powered_since = powered.then(Instant::now);
    let mut static_level = OFF_STATIC_LEVEL;
    // Drives the one deliberate keyboard-typing exception (plan revision) —
    // the free-text note, a slot's caption, or a just-captured print's name
    // (see `NoteEdit`).
    let mut note_edit = NoteEdit::None;
    let mut note_draft = String::new();
    // O card sob ação/renomeação (plano §3 + plan revision "crud para
    // memory cards"): (caminho, port 0|1). `Modal::CardsAction`,
    // `CardsConfirm` e `NoteEdit::CardRename` leem daqui.
    let mut card_action: Option<(PathBuf, u8)> = None;
    let mut card_rename: Option<(PathBuf, u8)> = None;
    // A save/load-state or print slot picker (plan revision) — see `Modal`.
    let mut modal = Modal::None;
    // Set the instant "Printscreen" is clicked; the frame that's live once
    // `core.run()` next produces one gets cloned into `print_capture` below
    // and held there until the player finishes naming it (or cancels).
    let mut print_pending = false;
    let mut print_capture: Option<EmuFrame> = None;
    log::info!("running: rf ntsc + crt tube, run-ahead {runahead}");
    // O core mora na thread dele; a interface continua a 60 fps com o
    // último quadro enquanto o PCSX Rearmed bloqueia (boot, FMV).
    let (core_tx, core_rx, worker_handle) = spawn_core_worker(
        core,
        runahead as usize,
        ra_session.is_some(),
        spec.rom.clone(),
    );
    let mut in_flight = false;
    // Som do leitor de CD (plano revision: "na hesitação do core ou atraso
    // do quadro, inserir o som de leitura do cd"): a última vez que um
    // quadro REAL chegou — `None` desde o ligar (o drive lê no boot). E o
    // plan revision "barulho do leitor tmb quando ocorre loading": loading
    // com tela animada NÃO hesita (frames seguem chegando), mas o jogo roda
    // LENTO enquanto o CD é lido em stream — quadros chegando com vão longo
    // ligam o loop por uma janela curta (o fade do som cobre a emenda).
    let cd_loop: Option<&'static [i16]> = crate::sfx::cd_seek_loop();
    let mut last_frame_at: Option<Instant> = None;
    let mut last_aspect = 4.0_f32 / 3.0;
    let mut drive_reading_until: Option<Instant> = None;
    // O último quadro REAL (o core não é retido entre presents): buffer
    // REUTILIZADO entre frames — sem alocação de 1,2 MB a 60 fps (era o
    // `LastFrame::from_frame` inteiro por frame). XRGB8888 vem como o core
    // entregou (upload direto, zero-conversão); RGB565 é convertido aqui.
    let mut last_frame: Vec<u8> = Vec::new();
    let mut last_frame_dims: Option<(u32, u32)> = None;
    // Drena o quadro em voo antes de uma operação de estado (reset, estado,
    // cheat, card, disco, desligar): o worker termina o quadro corrente.
    macro_rules! drain_core {
        () => {{
            while in_flight {
                match core_rx.recv() {
                    Ok(out) => {
                        in_flight = false;
                        frames += 1;
                        if audio.queued_frames() < audio_cap {
                            audio.queue(&out.audio);
                        }
                        if let Some(frame) = &out.frame {
                            store_into(frame, &mut last_frame);
                            last_frame_dims = Some((frame.width, frame.height));
                            last_aspect = out.aspect;
                        }
                    }
                    Err(_) => {
                        in_flight = false;
                        break;
                    }
                }
            }
        }};
    }

    let exit = 'run: loop {
        // Editing (text or a caption) is the one deliberate keyboard-typing
        // exception (plan revision) — while it's open, poll for composed
        // text/backspace/commit/cancel instead of gameplay input, so a key
        // meant for the editor doesn't also twitch the D-pad underneath it.
        if note_edit != NoteEdit::None {
            // Naming a fresh print (plan revision) renders in the modal, not
            // the notebook — everything else about polling/typing is shared.
            let in_modal = matches!(
                note_edit,
                NoteEdit::PrintName(_) | NoteEdit::CheatSearch | NoteEdit::CardRename
            );
            let te = plat.poll_text_entry();
            if te.quit {
                break 'run GameExit::Quit;
            }
            if te.backspace {
                note_draft.pop();
            }
            for c in te.typed.chars() {
                if note_draft.chars().count() < note_edit.limit() {
                    note_draft.push(c);
                }
            }
            // ⌘V/⌘C (Ctrl no resto) — o mesmo esquema de clipboard de todo
            // campo de texto; o colado redesenha o rascunho como a digitação.
            if let Some(paste) = &te.paste {
                for c in paste.chars() {
                    if note_draft.chars().count() < note_edit.limit() {
                        note_draft.push(c);
                    }
                }
            }
            if te.copy {
                plat.set_clipboard_text(&note_draft);
            }
            if !te.typed.is_empty() || te.backspace || te.paste.is_some() {
                if in_modal {
                    cab.set_modal_draft(Some(&note_draft), note_edit.limit(), note_edit.heading());
                } else {
                    cab.set_pause_draft(Some(&note_draft), note_edit.limit(), note_edit.heading());
                }
            }
            let mut save = te.commit;
            let mut cancel = te.cancel;
            // O rename devolve o jogador ao picker (a lista mostra o nome
            // novo) — reaberto depois do fecho genérico do draft, adiante.
            let mut card_reopen: Option<u8> = None;
            if let Some((x, y)) = te.click {
                let (ox, oy) = cab.window_to_output(x, y);
                let hit = if in_modal {
                    cab.hit_modal_button(ox, oy)
                } else {
                    cab.hit_pause_button(ox, oy)
                };
                match hit {
                    Some(PanelButton::PauseDraftSave) | Some(PanelButton::ModalConfirm) => {
                        save = true
                    }
                    Some(PanelButton::PauseDraftCancel) | Some(PanelButton::ModalCancel) => {
                        cancel = true
                    }
                    _ => {}
                }
            }
            if save {
                match note_edit {
                    NoteEdit::Text => {
                        let trimmed = note_draft.trim();
                        if trimmed.is_empty() {
                            delete_text_slot(&spec.notes_dir, &title, text_slot);
                            log::info!("text slot {text_slot}: cleared (saved empty)");
                        } else {
                            match save_text_slot(&spec.notes_dir, &title, text_slot, trimmed) {
                                Ok(()) => log::info!("text slot {text_slot}: saved"),
                                Err(e) => log::warn!("text slot {text_slot}: save failed: {e}"),
                            }
                        }
                        show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    }
                    NoteEdit::SlotName => {
                        notes_meta.slots.entry(note_slot).or_default().label =
                            note_draft.trim().to_string();
                        save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                        show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                        log::info!("note slot {note_slot}: renamed");
                    }
                    NoteEdit::PrintName(print_slot) => {
                        if let Some(frame) = &print_capture {
                            match save_note_image(&spec.notes_dir, &title, print_slot, frame) {
                                Ok(_) => {
                                    notes_meta.slots.entry(print_slot).or_default().label =
                                        note_draft.trim().to_string();
                                    save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                                    note_slot = print_slot;
                                    refresh_notes(
                                        cab,
                                        &spec.notes_dir,
                                        &title,
                                        note_slot,
                                        text_slot,
                                        &notes_meta,
                                    );
                                    log::info!("print: captured into slot {print_slot}");
                                }
                                Err(e) => log::warn!("print capture failed: {e}"),
                            }
                        }
                    }
                    NoteEdit::CheatSearch => cab.set_modal_search(note_draft.trim()),
                    NoteEdit::CardRename => {
                        if let Some((path, port)) = card_rename.take() {
                            match crate::memcard::rename(&path, note_draft.trim()) {
                                Ok(new_path) => {
                                    log::info!("card renomeado -> {}", new_path.display());
                                    // O card pode estar encaixado em
                                    // QUALQUER slot (o picker onde o
                                    // renomear foi acionado não importa):
                                    // quem aponta para o caminho velho
                                    // acompanha o novo — senão o slot fica
                                    // com o nome/adereço desatualizado e a
                                    // exclusividade nunca o enxerga.
                                    if current_card.as_deref() == Some(path.as_path()) {
                                        current_card = Some(new_path.clone());
                                        sram_path = new_path.clone();
                                    }
                                    if current_card2.as_deref() == Some(path.as_path()) {
                                        current_card2 = Some(new_path.clone());
                                        sram2_path = new_path.clone();
                                    }
                                    cab.set_card_labels(
                                        crate::memcard::card_name(current_card.as_deref())
                                            .as_deref(),
                                        crate::memcard::card_name(current_card2.as_deref())
                                            .as_deref(),
                                    );
                                }
                                Err(e) => {
                                    log::warn!("renomeando {}: {e}", path.display())
                                }
                            }
                            card_reopen = Some(port);
                        }
                    }
                    _ => {}
                }
            }
            if save || cancel {
                if in_modal {
                    cab.set_modal_draft(None, 0, "");
                    if matches!(note_edit, NoteEdit::CheatSearch) {
                        // Searching narrows the same Cheats modal rather
                        // than acting on a pick — stay in it, just back on
                        // the (maybe newly filtered) row grid, unlike
                        // every other in-modal edit here, which is always
                        // a one-shot action that closes the dialog.
                    } else {
                        modal = Modal::None;
                        print_capture = None;
                        cab.clear_modal();
                    }
                } else {
                    cab.set_pause_draft(None, 0, "");
                }
                note_edit = NoteEdit::None;
                note_draft.clear();
                plat.stop_text_input(cab);
                if let Some(port) = card_reopen.take() {
                    // De volta ao picker: a lista mostra o nome novo.
                    let (rows, _) = card_rows_with_icons(
                        &crate::dirs::memcards_dir(),
                        current_card.as_deref(),
                        current_card2.as_deref(),
                    );
                    modal = if port == 1 {
                        Modal::Cards2
                    } else {
                        Modal::Cards
                    };
                    cab.set_modal_with_icons(
                        if port == 1 {
                            "Memory Cards - slot 2"
                        } else {
                            "Memory Cards - slot 1"
                        },
                        &rows,
                    );
                }
            }
            if in_modal {
                let backdrop = if powered && last_frame_at.is_some() {
                    ModalBackdrop::Frame {
                        aspect_ratio: last_aspect,
                    }
                } else {
                    ModalBackdrop::Static(OFF_STATIC_LEVEL)
                };
                cab.present_modal(backdrop);
            } else {
                cab.present_pause();
            }
            pace_frame(&mut next, frame_time);
            continue;
        }

        // A clicked panel/pause-book button becomes exactly the `UiEvent` its
        // key used to send — the match below doesn't need to know clicks
        // exist at all. Which set of buttons a click can land on depends on
        // `paused`: the pause book replaces the whole window (no side panel
        // drawn alongside it), so it has its own hit-test.
        let events: Vec<UiEvent> = plat
            .poll(&mut input, &cfg.keymap)
            .into_iter()
            .filter_map(|ev| match ev {
                UiEvent::MouseUp(x, y) => {
                    // Solta o controle arrastado: sobre a OUTRA entrada
                    // completa a troca; fora dela, cancela.
                    if let Some(from) = pad_drag.take() {
                        let (ox, oy) = cab.window_to_output(x, y);
                        if let Some(to) = cab.hit_pad_port(ox, oy) {
                            if to != from {
                                pads_swapped = !pads_swapped;
                                cab.push_osd(
                                    &[if pads_swapped {
                                        "CONTROLE NA ENTRADA 2"
                                    } else {
                                        "CONTROLE NA ENTRADA 1"
                                    }],
                                    None,
                                    Duration::from_secs(2),
                                );
                            }
                        }
                    }
                    pad_hover = None;
                    None
                }
                UiEvent::MouseMove(x, y) => {
                    // Enquanto arrasta, a entrada sob o cursor ganha destaque.
                    if pad_drag.is_some() {
                        let (ox, oy) = cab.window_to_output(x, y);
                        pad_hover = cab.hit_pad_port(ox, oy);
                    }
                    None
                }
                UiEvent::Click(x, y) => {
                    let (ox, oy) = cab.window_to_output(x, y);
                    // The titlebar pair is drawn on every screen now —
                    // gameplay included (plan revision: "mostrar o fechar e
                    // minimizar em todas as telas"). Checked before every
                    // in-screen hit-test.
                    if cab.hit_close_button(ox, oy) {
                        Some(UiEvent::CloseRequested)
                    } else if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                        None
                    } else if modal != Modal::None {
                        // Not reachable during the naming/searching step —
                        // that's `NoteEdit::PrintName`/`CheatSearch`, polled
                        // via `poll_text_entry` instead (above), so `Click`
                        // never comes through `plat.poll()` while it's open.
                        match cab.hit_modal_button(ox, oy) {
                            Some(PanelButton::ModalSlot(i)) => Some(UiEvent::ModalPick(i)),
                            Some(PanelButton::ModalCancel) => Some(UiEvent::ModalCancel),
                            Some(PanelButton::ModalScrollUp) => Some(UiEvent::ModalScrollUp),
                            Some(PanelButton::ModalScrollDown) => Some(UiEvent::ModalScrollDown),
                            Some(PanelButton::ModalSearchStart) => Some(UiEvent::ModalSearchStart),
                            Some(PanelButton::ModalFilterAll) => Some(UiEvent::ModalFilterAll),
                            Some(PanelButton::ModalFilterOn) => Some(UiEvent::ModalFilterOn),
                            Some(PanelButton::ModalFilterOff) => Some(UiEvent::ModalFilterOff),
                            _ => None,
                        }
                    } else if paused {
                        // Not reachable here while `note_edit != None` — that
                        // branch polls via `poll_text_entry` instead (below
                        // the main match), so `Click` never comes through
                        // `plat.poll()` during it.
                        match cab.hit_pause_button(ox, oy) {
                            Some(PanelButton::PauseContinue) => Some(UiEvent::TogglePause),
                            Some(PanelButton::PauseNotePrev) => Some(UiEvent::NotePrev),
                            Some(PanelButton::PauseNoteNext) => Some(UiEvent::NoteNext),
                            Some(PanelButton::PauseWrite) => Some(UiEvent::NoteWriteStart),
                            Some(PanelButton::PauseNotePin) => Some(UiEvent::NotePinToggle),
                            Some(PanelButton::PauseNoteName) => Some(UiEvent::NoteNameStart),
                            Some(PanelButton::PauseTextPrev) => Some(UiEvent::TextPrev),
                            Some(PanelButton::PauseTextNext) => Some(UiEvent::TextNext),
                            Some(PanelButton::PauseTextPin) => Some(UiEvent::TextPinToggle),
                            Some(PanelButton::PauseTextDelete) => Some(UiEvent::TextDelete),
                            _ => None,
                        }
                    } else {
                        cab.hit_panel_button(ox, oy).and_then(|b| match b {
                            PanelButton::Power => Some(UiEvent::Quit),
                            PanelButton::Eject => Some(UiEvent::Eject),
                            PanelButton::Reset => Some(UiEvent::Reset),
                            PanelButton::Notebook => Some(UiEvent::TogglePause),
                            PanelButton::Cheats => Some(UiEvent::OpenCheatsModal),
                            PanelButton::Achievements => Some(UiEvent::OpenAchievementsModal),
                            PanelButton::BootBios => Some(UiEvent::BootBios),
                            PanelButton::DiscRemover => Some(UiEvent::RemoveDisc),
                            PanelButton::DiscInserter => Some(UiEvent::InsertDisc),
                            PanelButton::Cards1 => Some(UiEvent::OpenCards),
                            PanelButton::Cards2 => Some(UiEvent::OpenCards2),
                            PanelButton::PadPort1 => Some(UiEvent::PadGrab(0)),
                            PanelButton::PadPort2 => Some(UiEvent::PadGrab(1)),
                            PanelButton::Analog1 => Some(UiEvent::AnalogToggle(0)),
                            PanelButton::Analog2 => Some(UiEvent::AnalogToggle(1)),
                            PanelButton::Rumble1 => Some(UiEvent::RumbleToggle(0)),
                            PanelButton::Rumble2 => Some(UiEvent::RumbleToggle(1)),
                            PanelButton::Discos => Some(UiEvent::OpenDiscos),
                            // The shelf's own list button is shelf-side.
                            PanelButton::ShelfAchievements => None,
                            PanelButton::PrintScreen => Some(UiEvent::OpenPrintModal),
                            PanelButton::SaveState => Some(UiEvent::OpenSaveModal),
                            PanelButton::LoadState => Some(UiEvent::OpenLoadModal),
                            // The rest are all idle-screen-, pause-book- or
                            // modal-only, never shown alongside the panel
                            // that's up now. `Dev` is idle-screen-only too
                            // (and its menu lives outside a game).
                            PanelButton::Insert
                            | PanelButton::Settings
                            | PanelButton::Dev
                            | PanelButton::CoreDownload
                            | PanelButton::PauseContinue
                            | PanelButton::PauseNotePrev
                            | PanelButton::PauseNoteNext
                            | PanelButton::PauseWrite
                            | PanelButton::PauseNotePin
                            | PanelButton::PauseNoteName
                            | PanelButton::PauseTextPrev
                            | PanelButton::PauseTextNext
                            | PanelButton::PauseTextPin
                            | PanelButton::PauseTextDelete
                            | PanelButton::PauseDraftSave
                            | PanelButton::PauseDraftCancel
                            | PanelButton::ModalSlot(_)
                            | PanelButton::ModalConfirm
                            | PanelButton::ModalCancel
                            | PanelButton::ModalScrollUp
                            | PanelButton::ModalScrollDown
                            | PanelButton::ModalSearchStart
                            | PanelButton::ModalFilterAll
                            | PanelButton::ModalFilterOn
                            | PanelButton::ModalFilterOff => None,
                        })
                    }
                }
                other => Some(other),
            })
            .collect();
        for ev in events {
            match ev {
                // Sem disco na sessão de jogo: ignorado aqui (o boot da BIOS
                // vive na tela inicial; este é o painel do jogo).
                UiEvent::BootBios => {}
                UiEvent::ToggleLid => {
                    lid_open = !lid_open;
                    // Clique mecânico da tampa: o foley antigo aqui era o
                    // som de cartucho do SNES (plano revision: "remover o
                    // som do cartucho... é o som do snes").
                    if lid_open {
                        eject_clunk(plat);
                    } else {
                        tone_click(plat, 180.0);
                    }
                    // Como o console original: abrir a tampa PARA o leitor
                    // (o jogo vê a bandeja abrir e para de ler; o disco para
                    // de girar); fechar faz o disco girar de volta e o jogo
                    // tenta recuperar o que estava lendo.
                    if disc_in {
                        drain_core!();
                        let _ = core_tx.send(CoreCmd::TrayEject { ejected: lid_open });
                    }
                    cab.set_drive(lid_open, disc_in);
                    cab.push_osd(
                        &[if lid_open {
                            "TAMPA ABERTA — LEITOR PARADO"
                        } else {
                            "TAMPA FECHADA — LENDO"
                        }],
                        None,
                        Duration::from_secs(2),
                    );
                }
                UiEvent::RumbleToggle(port) => {
                    // O botão RUMBLE da entrada: alterna a vibração daquele
                    // controle (o LED vermelho ao lado da copy reflete).
                    rumble_on[port as usize] = !rumble_on[port as usize];
                    let on = rumble_on[port as usize];
                    cab.set_rumble_leds(rumble_on[0], rumble_on[1]);
                    cab.push_osd(
                        &[&format!(
                            "CONTROLE {} VIBRAÇÃO {}",
                            port + 1,
                            if on { "LIGADA" } else { "DESLIGADA" }
                        )],
                        None,
                        Duration::from_secs(2),
                    );
                }
                UiEvent::AnalogToggle(port) => {
                    // O botão ANALOG do controle original: alterna o modo
                    // analógico da entrada (o LED vermelho acende/apaga).
                    analog_on[port as usize] = !analog_on[port as usize];
                    let on = analog_on[port as usize];
                    cab.set_analog_leds(analog_on[0], analog_on[1]);
                    cab.push_osd(
                        &[&format!(
                            "CONTROLE {} ANALOG {}",
                            port + 1,
                            if on { "LIGADO" } else { "DESLIGADO" }
                        )],
                        None,
                        Duration::from_secs(2),
                    );
                }
                UiEvent::RemoveDisc => {
                    if !lid_open {
                        cab.push_osd(&["ABRA A TAMPA (OPEN)"], None, Duration::from_secs(2));
                    } else if disc_in {
                        disc_in = false;
                        cab.set_drive(lid_open, disc_in);
                        // Silencioso (plan revision: "remover som ao encaixar
                        // e remover o disco") — o deslize visual carrega.
                        // O disco desliza para fora enquanto o jogo segue —
                        // sem isto o desenho fica sentado no drive (o visual
                        // é o `cartridge_motion`, não o estado lógico).
                        disc_motion = Some((Instant::now(), true));
                        // Autêntico: ejeta a BANDEJA pela disk control do
                        // core — o JOGO vê a tampa abrir e dispara o próprio
                        // código (telas de erro de leitura reais dele). O
                        // rasgo visual cobre só o instante mecânico.
                        drain_core!();
                        let _ = core_tx.send(CoreCmd::TrayEject { ejected: true });
                        disc_glitch = Some(Instant::now());
                    }
                }
                UiEvent::InsertDisc => {
                    // BIOS: a estante abre direto — o console está parado no
                    // menu com o drive vazio, sem o ritual da tampa.
                    if !spec.bios && !lid_open {
                        cab.push_osd(&["ABRA A TAMPA (OPEN)"], None, Duration::from_secs(2));
                    } else if !spec.bios && disc_in {
                        cab.push_osd(&["REMOVA O DISCO ATUAL"], None, Duration::from_secs(2));
                    } else if spec.library.is_empty() {
                        cab.push_osd(&["NENHUM JOGO NA ESTANTE"], None, Duration::from_secs(2));
                    } else {
                        let rows: Vec<(String, bool)> = spec
                            .library
                            .iter()
                            .map(|(t, _)| (t.clone(), true))
                            .collect();
                        modal = Modal::Inserir;
                        cab.set_modal("Inserir disco", &rows);
                    }
                }
                UiEvent::Quit => {
                    if powered {
                        // Desligar (plan §3.3): flush the cart, then the
                        // signal-off ritual. A second click while already off
                        // does nothing on purpose — it's Ligar (below) now.
                        drain_core!();
                        if !core_cards_via_file {
                            let (sram_tx, sram_rx) = std::sync::mpsc::channel();
                            let _ = core_tx.send(CoreCmd::Sram { tx: sram_tx });
                            let sram = sram_rx.recv().ok().flatten();
                            flush_sram(&sram_path, &mut last_sram, sram);
                            if current_card2.is_some() {
                                let (t2, r2) = std::sync::mpsc::channel();
                                let _ = core_tx.send(CoreCmd::SramAt {
                                    id: MEMORY_SAVE_RAM + 1,
                                    tx: t2,
                                });
                                let s2 = r2.recv().ok().flatten();
                                flush_sram(&sram2_path, &mut last_sram2, s2);
                            }
                        }
                        if let Some(t) = powered_since.take() {
                            powered_elapsed += t.elapsed();
                        }
                        cab.set_session_time(powered_elapsed);
                        static_level = power_off_burst(plat, cab);
                        powered = false;
                        cab.set_powered(false);
                        log::info!("power off — eject to leave, click power to resume");
                    } else {
                        // Ligar de novo: same button as power off, now
                        // toggling back on — the game resumes exactly where
                        // it was, no reload. The "CH 3" banner flashes for a
                        // few seconds over the picture, the way a TV shows
                        // the channel when you tune it (plan revision).
                        // O core JÁ é acionado aqui, em paralelo ao ritual: o
                        // primeiro quadro chega junto com o fim da estática em
                        // vez de um "nada acontece" depois do clique.
                        // A luz do Power acende NO CLIQUE — antes do ritual.
                        cab.set_powered(true);
                        if !in_flight {
                            let mut snap = PadSnapshot::default();
                            for (port, analog_enabled) in
                                analog_on.iter().enumerate().take(MAX_PORTS)
                            {
                                for (rb, pb) in PAD {
                                    snap.buttons.push((port, rb, input.held(port, pb)));
                                }
                                if *analog_enabled {
                                    snap.analog.push((
                                        port,
                                        input.analog(port, 0).0,
                                        input.analog(port, 0).1,
                                        input.analog(port, 1).0,
                                        input.analog(port, 1).1,
                                    ));
                                } else {
                                    snap.analog.push((port, 0, 0, 0, 0));
                                }
                            }
                            let _ = core_tx.send(CoreCmd::Run { input: snap });
                            in_flight = true;
                        }
                        power_on_burst(plat, cab);
                        powered = true;
                        powered_since = Some(Instant::now());
                        cab.flash_ch3(CH3_FLASH);
                        // Console power-on re-arms the RA hit counts (plan
                        // fase 5) — earned stays authoritative locally, and
                        // the server deduplicates.
                        if let Some(ra) = &mut ra_session {
                            ra.reset();
                        }
                        log::info!("power on — resuming");
                    }
                }
                UiEvent::Eject => {
                    if powered {
                        // OPEN/CLOSE com o console ligado: só a tampa — mas
                        // como no console real, abrir PARA o leitor (o jogo
                        // vê a bandeja abrir) e fechar faz o disco voltar a
                        // girar e o jogo tentar recuperar.
                        lid_open = !lid_open;
                        if lid_open {
                            eject_clunk(plat);
                        } else {
                            tone_click(plat, 180.0);
                        }
                        if disc_in {
                            drain_core!();
                            let _ = core_tx.send(CoreCmd::TrayEject { ejected: lid_open });
                        }
                        cab.set_drive(lid_open, disc_in);
                    } else {
                        if let Some(ra) = &mut ra_session {
                            ra.save_progress();
                        }
                        if has_cartridge_art {
                            cartridge_eject_animation(cab);
                        }
                        break 'run GameExit::Ejected { static_level };
                    }
                }
                UiEvent::CloseRequested => break 'run GameExit::Quit,
                UiEvent::Reset => {
                    if powered {
                        drain_core!();
                        if disc_in {
                            let _ = core_tx.send(CoreCmd::Reset);
                        } else {
                            // Sem disco + Reset: o console boota a BIOS.
                            let _ = core_tx.send(CoreCmd::BootBios);
                            cab.push_osd(
                                &["SEM DISCO — BOOT PELA BIOS"],
                                None,
                                Duration::from_secs(3),
                            );
                        }
                        crate::sfx::play(cab, crate::sfx::Sfx::Reset);
                        // Momentary rocker (plan revision) — springs back on
                        // its own next frame via `reset_pressed`/`RESET_SPRING`,
                        // same clock as the "feito!" flashes.
                        flash.insert(PanelButton::Reset, Instant::now());
                    }
                }
                UiEvent::TogglePause if powered => {
                    paused = !paused;
                    if paused {
                        // Load once on the way in; present_pause just
                        // redraws it every frame (plan §3.2/§3.4) — on
                        // whichever slots were last selected.
                        cab.set_pause_note(&title, NOTE_SLOTS as usize);
                        show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                        show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    }
                    log::info!("{}", if paused { "paused" } else { "resumed" });
                }
                UiEvent::NotePrev if paused && note_slot > 1 => {
                    note_slot -= 1;
                    show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                }
                UiEvent::NoteNext if paused && note_slot < NOTE_SLOTS => {
                    note_slot += 1;
                    show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                }
                UiEvent::NoteWriteStart if paused && !notes_meta.text_slot(text_slot).pinned => {
                    note_edit = NoteEdit::Text;
                    note_draft =
                        read_text_slot(&spec.notes_dir, &title, text_slot).unwrap_or_default();
                    cab.set_pause_draft(
                        Some(&note_draft),
                        NoteEdit::Text.limit(),
                        NoteEdit::Text.heading(),
                    );
                    plat.start_text_input(cab);
                }
                UiEvent::NotePinToggle if paused => {
                    let m = notes_meta.slots.entry(note_slot).or_default();
                    m.pinned = !m.pinned;
                    let now_pinned = m.pinned;
                    save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                    show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                    // The panel's own notes block only features pinned slots
                    // now (plan revision) — recompute it so the change is
                    // already reflected once the player resumes, not stale
                    // until the next capture.
                    refresh_notes(
                        cab,
                        &spec.notes_dir,
                        &title,
                        note_slot,
                        text_slot,
                        &notes_meta,
                    );
                    log::info!(
                        "note slot {note_slot}: {}",
                        if now_pinned { "pinned" } else { "unpinned" }
                    );
                }
                UiEvent::NoteNameStart if paused => {
                    note_edit = NoteEdit::SlotName;
                    note_draft = notes_meta.slot(note_slot).label;
                    cab.set_pause_draft(
                        Some(&note_draft),
                        NoteEdit::SlotName.limit(),
                        NoteEdit::SlotName.heading(),
                    );
                    plat.start_text_input(cab);
                }
                UiEvent::TextPrev if paused && text_slot > 1 => {
                    text_slot -= 1;
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                }
                UiEvent::TextNext if paused && text_slot < NOTE_SLOTS => {
                    text_slot += 1;
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                }
                UiEvent::TextPinToggle if paused => {
                    let m = notes_meta.text_slots.entry(text_slot).or_default();
                    m.pinned = !m.pinned;
                    let now_pinned = m.pinned;
                    save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    // Same reasoning as `NotePinToggle`'s own refresh: the
                    // panel only features pinned slots, so a text pin
                    // toggle needs to reach it right away too.
                    refresh_notes(
                        cab,
                        &spec.notes_dir,
                        &title,
                        note_slot,
                        text_slot,
                        &notes_meta,
                    );
                    log::info!(
                        "text slot {text_slot}: {}",
                        if now_pinned { "pinned" } else { "unpinned" }
                    );
                }
                UiEvent::TextDelete if paused && !notes_meta.text_slot(text_slot).pinned => {
                    delete_text_slot(&spec.notes_dir, &title, text_slot);
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    log::info!("text slot {text_slot}: deleted");
                }
                UiEvent::OpenSaveModal if powered => {
                    if ra_hardcore_active {
                        cab.push_osd(
                            &["MODO HARDCORE", "savestates bloqueados"],
                            None,
                            Duration::from_secs(3),
                        );
                        continue;
                    }
                    modal = Modal::SaveSlot;
                    cab.set_modal("Salvar estado", &save_slot_rows(&spec.save_dir, &title));
                }
                UiEvent::OpenLoadModal if powered => {
                    if ra_hardcore_active {
                        cab.push_osd(
                            &["MODO HARDCORE", "loadstates bloqueados"],
                            None,
                            Duration::from_secs(3),
                        );
                        continue;
                    }
                    modal = Modal::LoadSlot;
                    cab.set_modal("Carregar estado", &load_slot_rows(&spec.save_dir, &title));
                }
                UiEvent::OpenAchievementsModal if powered => {
                    let Some(ra) = &ra_session else { continue };
                    let (total, earned) = ra.earned_snapshot();
                    // Cada linha diz o modo do ganho: [HC] hardcore,
                    // [SC] softcore, [ ] não ganha (plan revision: "como
                    // sei qual tipo ganhei ou já tenho?").
                    let rows: Vec<(String, bool)> = ra
                        .achievements()
                        .iter()
                        .map(|a| {
                            let modo = match earned.get(&a.id) {
                                Some(true) => " [HC]",
                                Some(false) => " [SC]",
                                None => "",
                            };
                            (
                                format!("{} ({} pts){}", a.title, a.points, modo),
                                earned.contains_key(&a.id),
                            )
                        })
                        .collect();
                    modal = Modal::Achievements;
                    cab.set_modal(&format!("Conquistas {}/{}", earned.len(), total), &rows);
                }
                UiEvent::OpenCheatsModal if powered && !cheat_defs.is_empty() => {
                    if ra_hardcore_active {
                        cab.push_osd(
                            &["MODO HARDCORE", "cheats bloqueados"],
                            None,
                            Duration::from_secs(3),
                        );
                        continue;
                    }
                    modal = Modal::Cheats;
                    cab.set_modal(
                        "Cheats",
                        &cheat_modal_rows(&cheat_rows(&cheat_defs, &cheat_state)),
                    );
                    cab.set_modal_searchable(true);
                }
                UiEvent::OpenCards => {
                    // Troca quente (plan revision): funciona ligado — o SRAM
                    // do card atual é descarregado no arquivo dele antes do
                    // WriteMem do novo (o físico: puxa um card, encaixa
                    // outro; o jogo lê o que está no slot quando salva).
                    let (rows, _) = card_rows_with_icons(
                        &crate::dirs::memcards_dir(),
                        current_card.as_deref(),
                        current_card2.as_deref(),
                    );
                    modal = Modal::Cards;
                    cab.set_modal_with_icons("Memory Cards - slot 1", &rows);
                }
                UiEvent::OpenCards2 => {
                    let (rows, _) = card_rows_with_icons(
                        &crate::dirs::memcards_dir(),
                        current_card.as_deref(),
                        current_card2.as_deref(),
                    );
                    modal = Modal::Cards2;
                    cab.set_modal_with_icons("Memory Cards - slot 2", &rows);
                }
                UiEvent::OpenDiscos => {
                    let Some(list) = &discs else {
                        continue;
                    };
                    let rows: Vec<(String, bool)> = list
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let name = p
                                .file_stem()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_else(|| format!("disco {}", i + 1));
                            let label = if *p == current_disc {
                                format!("{name} — no drive")
                            } else {
                                name
                            };
                            (label, true)
                        })
                        .collect();
                    modal = Modal::Discos;
                    cab.set_modal("Discos", &rows);
                }
                UiEvent::OpenPrintModal if powered => {
                    if all_slots_pinned(&notes_meta) {
                        log::warn!("print skipped: all {NOTE_SLOTS} slots pinned");
                    } else {
                        // The actual capture happens once `core.run()` next
                        // produces a frame, below — same "wait for a real
                        // frame" pattern `--debug-note-capture` already uses.
                        print_pending = true;
                    }
                }
                UiEvent::ModalPick(i) => match modal {
                    // Save/load/print never have more than 15 rows — safe
                    // to narrow back to `u8` (`ModalPick` is `u16` only
                    // because the Cheats modal, below, can run past 255).
                    Modal::SaveSlot => {
                        let path = state_file(&spec.save_dir, &title, i as u8);
                        drain_core!();
                        let (st_tx, st_rx) = std::sync::mpsc::channel();
                        let _ = core_tx.send(CoreCmd::SaveState { tx: st_tx });
                        let saved = st_rx.recv().ok();
                        match saved.filter(|s| !s.is_empty()) {
                            Some(s) => match fs::write(&path, &s) {
                                Ok(_) => {
                                    log::info!("slot {i}: saved ({} KiB)", s.len() / 1024);
                                    // Carimbo do disco (plano §3.1): qual
                                    // disco estava no drive quando salvou.
                                    let _ = fs::write(
                                        disc_stamp_path(&spec.save_dir, &title, i as u8),
                                        disc_name(&spec.rom),
                                    );
                                    flash.insert(PanelButton::SaveState, Instant::now());
                                    modal = Modal::None;
                                    cab.clear_modal();
                                }
                                Err(e) => log::warn!("slot {i}: save failed: {e}"),
                            },
                            None => log::warn!("core doesn't support save states"),
                        }
                    }
                    Modal::LoadSlot => {
                        let path = state_file(&spec.save_dir, &title, i as u8);
                        if let Ok(stamp) =
                            fs::read_to_string(disc_stamp_path(&spec.save_dir, &title, i as u8))
                        {
                            let cur = disc_name(&spec.rom);
                            if stamp.trim() != cur {
                                log::warn!(
                                    "slot {i}: state é do disco {stamp} — o drive tem {cur}"
                                );
                                cab.push_osd(
                                    &[
                                        "STATE DE OUTRO DISCO",
                                        &format!("salvo no  {stamp}"),
                                        &format!("drive tem  {cur}"),
                                    ],
                                    None,
                                    Duration::from_secs(5),
                                );
                            }
                        }
                        match fs::read(&path) {
                            Ok(s) if !s.is_empty() => {
                                drain_core!();
                                let _ = core_tx.send(CoreCmd::LoadState { bytes: s });
                                log::info!("slot {i}: loaded");
                                flash.insert(PanelButton::LoadState, Instant::now());
                                modal = Modal::None;
                                cab.clear_modal();
                            }
                            // An empty/rejected slot just stays disabled in the
                            // modal — nothing to load, nothing changes.
                            Ok(_) => log::warn!("slot {i}: core rejected the state"),
                            Err(_) => log::warn!("slot {i}: empty"),
                        }
                    }
                    Modal::PrintSlot => {
                        let picked = i as u8 + 1; // modal rows are 0-based, slots 1..=15
                        if !notes_meta.slot(picked).pinned {
                            note_edit = NoteEdit::PrintName(picked);
                            note_draft.clear();
                            cab.set_modal_draft(
                                Some(""),
                                NoteEdit::PrintName(picked).limit(),
                                NoteEdit::PrintName(picked).heading(),
                            );
                            plat.start_text_input(cab);
                        }
                    }
                    // Read-only list (plan fase 4): a pick closes, nothing
                    // toggles.
                    Modal::Achievements => {
                        modal = Modal::None;
                        cab.clear_modal();
                    }
                    Modal::Cheats => {
                        let idx = i as usize;
                        if idx < cheat_defs.len() {
                            cheat_state[idx] = !cheat_state[idx];
                            // A single `cheat_set(idx, false, ...)` isn't
                            // enough to actually undo a sustained memory
                            // patch on every core (a real report: the row
                            // showed off, the effect stayed on) — reset and
                            // reapply every cheat's current state, the same
                            // belt-and-suspenders sequence the boot-time
                            // load above already uses.
                            drain_core!();
                            let _ = core_tx.send(CoreCmd::CheatReset);
                            for (j, (def, &on)) in cheat_defs.iter().zip(&cheat_state).enumerate() {
                                let _ = core_tx.send(CoreCmd::CheatSet {
                                    i: j as u32,
                                    on,
                                    code: def.code.to_string(),
                                });
                            }
                            save_cheat_state(&cheat_path, &cheat_state);
                            let rows = cheat_rows(&cheat_defs, &cheat_state);
                            cab.set_cheats(&rows);
                            cab.set_modal("Cheats", &cheat_modal_rows(&rows));
                            log::info!(
                                "cheat {:?}: {}",
                                cheat_defs[idx].desc,
                                if cheat_state[idx] { "on" } else { "off" }
                            );
                        }
                    }
                    Modal::Cards | Modal::Cards2 => {
                        let port = if modal == Modal::Cards2 { 1 } else { 0 };
                        let dir = crate::dirs::memcards_dir();
                        let (_, cards) = card_rows_with_icons(&dir, None, None);
                        if i == 0 {
                            // CRUD — criar: um card formatado em branco nasce
                            // na biblioteca; encaixar é escolher ele depois.
                            match crate::memcard::create_blank(&dir) {
                                Ok(path) => {
                                    log::info!("card {}: criado", path.display());
                                    cab.push_osd(&["CARD CRIADO"], None, Duration::from_secs(2));
                                }
                                Err(e) => log::warn!("card novo: {e}"),
                            }
                            let (rows, _) = card_rows_with_icons(
                                &dir,
                                current_card.as_deref(),
                                current_card2.as_deref(),
                            );
                            cab.set_modal_with_icons(
                                if port == 1 {
                                    "Memory Cards - slot 2"
                                } else {
                                    "Memory Cards - slot 1"
                                },
                                &rows,
                            );
                            // Fica no picker — o card novo aparece na lista.
                            continue;
                        }
                        let Some(path) = cards.get(i as usize - 1) else {
                            continue;
                        };
                        // Segundo passo: as ações sobre o card, com a lista
                        // dos saves (título, produto, ícone) no alto.
                        card_action = Some((path.clone(), port));
                        let name = crate::memcard::card_name(Some(path.as_path()))
                            .unwrap_or_default();
                        let info = crate::memcard::inspect(path);
                        let used = info.as_ref().map(|c| c.used).unwrap_or(0);
                        let mut rows: Vec<CardModalRow> = info
                            .map(|c| {
                                c.saves
                                    .iter()
                                    .map(|s| {
                                        (
                                            if s.title.is_empty() {
                                                s.product.clone()
                                            } else {
                                                format!("{} — {}", s.title, s.product)
                                            },
                                            false,
                                            s.icon.clone().map(|d| (16u32, 16u32, d)),
                                        )
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        rows.push(("Usar neste slot".to_string(), true, None));
                        rows.push(("Renomear".to_string(), true, None));
                        rows.push(("Apagar".to_string(), true, None));
                        modal = Modal::CardsAction;
                        cab.set_modal_with_icons(
                            &format!("{name} - {used}/15 blocos"),
                            &rows,
                        );
                    }
                    Modal::CardsAction => {
                        modal = Modal::None;
                        cab.clear_modal();
                        let Some((path, port)) = card_action.take() else {
                            continue;
                        };
                        let n_saves = crate::memcard::inspect(&path)
                            .map(|c| c.saves.len())
                            .unwrap_or(0);
                        let n_saves = n_saves as u16;
                        let name = crate::memcard::card_name(Some(path.as_path()))
                            .unwrap_or_default();
                        let seated = if port == 1 {
                            current_card2.as_deref() == Some(path.as_path())
                        } else {
                            current_card.as_deref() == Some(path.as_path())
                        };
                        if i < n_saves {
                            // Linha informativa (um save do card) — clicar não
                            // faz nada; as ações ficam logo abaixo.
                            card_action = Some((path, port));
                            continue;
                        }
                        match i - n_saves {
                            // Usar: encaixa no slot e imprime o nome no
                            // adesivo da porta. Na troca quente (ligado), o
                            // SRAM do card que sai é descarregado no arquivo
                            // dele ANTES do WriteMem do novo — o físico:
                            // puxa um card (que guarda o que tem) e encaixa
                            // o outro.
                            0 => {
                                // Troca quente: descarrega a SRAM dos DOIS
                                // slots antes de mexer (o card que sai de
                                // cada posição guarda o que tem — flush é
                                // only-if-changed, então é de graça).
                                if powered && !core_cards_via_file {
                                    drain_core!();
                                    let (f1_tx, f1_rx) = std::sync::mpsc::channel();
                                    let _ = core_tx.send(CoreCmd::Sram { tx: f1_tx });
                                    flush_sram(
                                        &sram_path,
                                        &mut last_sram,
                                        f1_rx.recv().ok().flatten(),
                                    );
                                    let (f2_tx, f2_rx) = std::sync::mpsc::channel();
                                    let _ = core_tx.send(CoreCmd::SramAt {
                                        id: MEMORY_SAVE_RAM + 1,
                                        tx: f2_tx,
                                    });
                                    flush_sram(
                                        &sram2_path,
                                        &mut last_sram2,
                                        f2_rx.recv().ok().flatten(),
                                    );
                                }
                                // Um card, um slot (o alvo é o PICKER que
                                // abriu: port 0 -> slot 1, port 1 -> slot 2).
                                // Se o card estava no OUTRO slot, sai de lá —
                                // o core recebe uma SRAM zerada naquela porta
                                // (o jogo vê um card em branco se salvar).
                                if port == 1 {
                                    if current_card.as_deref() == Some(path.as_path()) {
                                        let _ = core_tx.send(CoreCmd::WriteMem {
                                            id: MEMORY_SAVE_RAM,
                                            bytes: vec![0; crate::memcard::CARD_SIZE],
                                        });
                                        current_card = None;
                                        sram_path = PathBuf::new();
                                    }
                                    // SLOT 2 = ARQUIVO do core (o protocolo
                                    // só expõe o card 1): copia o card
                                    // escolhido para o arquivo que o core
                                    // carrega/gerencia. Vale a partir do
                                    // próximo boot do jogo.
                                    let shared = std::path::Path::new(&spec.save_dir)
                                        .join(MC2_SHARED_FILE);
                                    match std::fs::copy(&path, &shared) {
                                        Ok(_) => {
                                            sram2_path = shared;
                                            current_card2 = Some(path.clone());
                                        }
                                        Err(e) => {
                                            log::warn!("card 2: copiando: {e}")
                                        }
                                    }
                                } else {
                                    if current_card2.as_deref() == Some(path.as_path()) {
                                        let _ = core_tx.send(CoreCmd::WriteMem {
                                            id: MEMORY_SAVE_RAM + 1,
                                            bytes: vec![0; crate::memcard::CARD_SIZE],
                                        });
                                        current_card2 = None;
                                        sram2_path = PathBuf::new();
                                    }
                                    seat_card(
                                        &core_tx,
                                        &mut sram_path,
                                        &mut current_card,
                                        0,
                                        path.clone(),
                                    );
                                }
                                cab.set_card_labels(
                                    crate::memcard::card_name(current_card.as_deref())
                                        .as_deref(),
                                    crate::memcard::card_name(current_card2.as_deref())
                                        .as_deref(),
                                );
                                cab.push_osd(
                                    &[&format!("CARD: {name}")],
                                    None,
                                    Duration::from_secs(2),
                                );
                            }
                            1 => {
                                // Renomear: o draft vive DENTRO de um modal —
                                // e o braço de ações acabou de fechar o dele
                                // (clear_modal no topo): reabre um hospedeiro
                                // antes, ou o draft não aparece e a TV fica
                                // escura (o bug do "apagou a tela toda").
                                card_rename = Some((path, port));
                                note_edit = NoteEdit::CardRename;
                                note_draft = name;
                                let _ = seated;
                                cab.set_modal("Memory Cards", &[]);
                                cab.set_modal_draft(
                                    Some(&note_draft),
                                    note_edit.limit(),
                                    note_edit.heading(),
                                );
                                plat.start_text_input(cab);
                                continue;
                            }
                            2 => {
                                // Apagar: confirmado em dois passos — e o
                                // card encaixado em QUALQUER slot não apaga
                                // (é o save vivo de um dos slots).
                                if seated
                                    || current_card.as_deref() == Some(path.as_path())
                                    || current_card2.as_deref() == Some(path.as_path())
                                {
                                    cab.push_osd(
                                        &["CARD EM USO", "encaixe outro card antes de apagar"],
                                        None,
                                        Duration::from_secs(3),
                                    );
                                    card_action = Some((path, port));
                                    continue;
                                }
                                // O braço de confirmar precisa do alvo: o
                                // take() do topo já consumiu — devolve.
                                card_action = Some((path, port));
                                modal = Modal::CardsConfirm;
                                cab.set_modal(
                                    &format!("Apagar \"{name}\"?"),
                                    &[
                                        ("Sim, apagar para sempre".to_string(), true),
                                        ("Cancelar".to_string(), true),
                                    ],
                                );
                                continue;
                            }
                            _ => continue,
                        }
                    }
                    Modal::CardsConfirm => {
                        modal = Modal::None;
                        cab.clear_modal();
                        let Some((path, port)) = card_action.take() else {
                            continue;
                        };
                        if i == 0 {
                            match crate::memcard::delete(&path) {
                                Ok(()) => {
                                    log::info!("card {}: apagado", path.display());
                                    cab.push_osd(
                                        &["CARD APAGADO"],
                                        None,
                                        Duration::from_secs(2),
                                    );
                                }
                                Err(e) => log::warn!("card {}: apagando: {e}", path.display()),
                            }
                        }
                        let _ = port;
                    }
                    Modal::Inserir => {
                        modal = Modal::None;
                        cab.clear_modal();
                        let Some((_, path)) = spec.library.get(i as usize) else {
                            continue;
                        };
                        drain_core!();
                        if path == &current_disc {
                            // O MESMO disco de volta: só fecha a bandeja.
                            let _ = core_tx.send(CoreCmd::TrayEject { ejected: false });
                            disc_in = true;
                            cab.set_drive(lid_open, disc_in);
                            disc_motion = Some((Instant::now(), false));
                            cab.push_osd(&["DISCO INSERIDO"], None, Duration::from_secs(2));
                            continue;
                        }
                        let (d_tx, d_rx) = std::sync::mpsc::channel();
                        let _ = core_tx.send(CoreCmd::SwitchDisc {
                            path: path.clone(),
                            tx: d_tx,
                        });
                        if d_rx.recv().unwrap_or(false) {
                            let _ = core_tx.send(CoreCmd::TrayEject { ejected: false });
                            disc_in = true;
                            disc_glitch = None;
                            current_disc = path.clone();
                            // O painel acompanha o jogo inserido — título,
                            // logo e arte do disco, igual ao que a estante
                            // mostra ao abrir o jogo direto. (A sessão de
                            // cheats/notas/RA continua a do boot: na prática
                            // quem insere pela tampa parte da BIOS, onde
                            // tudo isso é vazio.)
                            bios_session = false;
                            prev_sig = None;
                            if let Some((new_title, _)) = spec.library.get(i as usize) {
                                let assets = crate::dirs::assets_dir();
                                let rom_str = path.to_string_lossy();
                                let logo = crate::shelf::find_local_art(
                                    &assets.join("logo"),
                                    &rom_str,
                                    Some(new_title),
                                );
                                // A arte do disco vive em assets/disc (o
                                // PSX não tem "cartucho"); cartridge fica
                                // como fallback pela convenção antiga.
                                let cart = crate::shelf::find_local_art(
                                    &assets.join("disc"),
                                    &rom_str,
                                    Some(new_title),
                                )
                                .or_else(|| {
                                    crate::shelf::find_local_art(
                                        &assets.join("cartridge"),
                                        &rom_str,
                                        Some(new_title),
                                    )
                                });
                                let logo_img = decode_panel_art(&logo, "logo");
                                let cart_img = decode_panel_art(&cart, "disco");
                                let rows = command_rows(
                                    &flash,
                                    !cheat_defs.is_empty(),
                                    ra_session.is_some(),
                                    discs.is_some(),
                                    all_slots_pinned(&notes_meta),
                                    lid_open,
                                    true,
                                    !spec.library.is_empty(),
                                    false,
                                );
                                cab.set_panel(
                                    logo_img
                                        .as_ref()
                                        .map(|(w, h, d)| (*w, *h, d.as_slice())),
                                    cart_img
                                        .as_ref()
                                        .map(|(w, h, d)| (*w, *h, d.as_slice())),
                                    new_title,
                                    &rows,
                                );
                                // set_panel zera o estado do painel — reafirma
                                // o que está vivo na sessão.
                                cab.set_card_labels(
                                    crate::memcard::card_name(current_card.as_deref())
                                        .as_deref(),
                                    crate::memcard::card_name(current_card2.as_deref())
                                        .as_deref(),
                                );
                                cab.set_powered(powered);
                            }
                            cab.set_drive(lid_open, disc_in);
                            disc_motion = Some((Instant::now(), false));
                            // Silencioso (plan revision: "remover som ao
                            // encaixar e remover o disco").
                            cab.push_osd(&["DISCO INSERIDO"], None, Duration::from_secs(2));
                        } else {
                            cab.push_osd(&["FALHA AO INSERIR"], None, Duration::from_secs(2));
                        }
                    }
                    Modal::Discos => {
                        modal = Modal::None;
                        cab.clear_modal();
                        let Some(list) = &discs else { continue };
                        let Some(path) = list.get(i as usize) else {
                            continue;
                        };
                        if *path == current_disc {
                            continue;
                        }
                        drain_core!();
                        let (d_tx, d_rx) = std::sync::mpsc::channel();
                        let _ = core_tx.send(CoreCmd::SwitchDisc {
                            path: path.clone(),
                            tx: d_tx,
                        });
                        if d_rx.recv().unwrap_or(false) {
                            current_disc = path.clone();
                            log::info!("disco {}: no drive", path.display());
                        } else {
                            log::warn!("disco {}: falha na troca", path.display());
                        }
                    }
                    Modal::None => {}
                },
                UiEvent::ModalCancel => {
                    modal = Modal::None;
                    print_capture = None;
                    cab.clear_modal();
                }
                UiEvent::ModalScrollUp if modal != Modal::None => cab.scroll_modal(-1),
                UiEvent::ModalScrollDown if modal != Modal::None => cab.scroll_modal(1),
                UiEvent::ModalSearchStart if modal == Modal::Cheats => {
                    note_edit = NoteEdit::CheatSearch;
                    note_draft = cab.modal_search_query().to_string();
                    cab.set_modal_draft(
                        Some(&note_draft),
                        NoteEdit::CheatSearch.limit(),
                        NoteEdit::CheatSearch.heading(),
                    );
                    plat.start_text_input(cab);
                }
                UiEvent::ModalFilterAll if modal == Modal::Cheats => cab.set_modal_filter(None),
                UiEvent::ModalFilterOn if modal == Modal::Cheats => {
                    cab.set_modal_filter(Some(true))
                }
                UiEvent::ModalFilterOff if modal == Modal::Cheats => {
                    cab.set_modal_filter(Some(false))
                }
                // The rest only make sense with the console on (or, for the
                // pause-book trio, only while actually paused); ignored
                // otherwise. `Click` never reaches this match — it's already
                // resolved into one of the arms above (or dropped) before
                // the loop.
                UiEvent::TogglePause
                | UiEvent::OpenSaveModal
                | UiEvent::OpenLoadModal
                | UiEvent::OpenAchievementsModal
                | UiEvent::OpenCheatsModal
                | UiEvent::OpenPrintModal
                | UiEvent::ModalScrollUp
                | UiEvent::ModalScrollDown
                | UiEvent::ModalSearchStart
                | UiEvent::ModalFilterAll
                | UiEvent::ModalFilterOn
                | UiEvent::ModalFilterOff
                | UiEvent::NotePrev
                | UiEvent::NoteNext
                | UiEvent::NoteWriteStart
                | UiEvent::NotePinToggle
                | UiEvent::NoteNameStart
                | UiEvent::TextPrev
                | UiEvent::TextNext
                | UiEvent::TextPinToggle
                | UiEvent::TextDelete
                | UiEvent::Click(..)
                // O arrasto do controle: o GRAB abre o drag e o MouseUp/
                // MouseMove são resolvidos no filtro lá em cima — nenhum
                // precisa de tratamento aqui.
                | UiEvent::MouseUp(..)
                | UiEvent::MouseMove(..) => {}
                UiEvent::PadGrab(port) => {
                    // O drag começa: o controle "sai" da entrada e segue o
                    // mouse até ser solto na outra.
                    pad_drag = Some(port);
                    cab.push_osd(
                        &["ARRASTE O CONTROLE PARA A OUTRA ENTRADA"],
                        None,
                        Duration::from_secs(3),
                    );
                }
            }
        }
        // The command legend only changes when a "(feito!)" flash starts or
        // ends, or the print-slots-pinned state flips — rebuilding its
        // Strings at 60fps for an unchanged panel was churn (perf pass).
        let flashing_now = flash.values().any(|t| t.elapsed() < FLASH_DURATION);
        let all_pinned = all_slots_pinned(&notes_meta);
        let sig = (flashing_now, all_pinned, lid_open, disc_in);
        if Some(sig) != prev_sig {
            cab.set_commands(&command_rows(
                &flash,
                !cheat_defs.is_empty(),
                ra_session.is_some(),
                discs.is_some(),
                all_pinned,
                lid_open,
                disc_in,
                !spec.library.is_empty(),
                bios_session,
            ));
            prev_sig = Some(sig);
        }
        cab.set_reset_pressed(reset_pressed(&flash));
        cab.set_rumble_leds(rumble_on[0], rumble_on[1]);
        // A entrada 2 só tem controle com a opção LIGADA na configuração E
        // um segundo gamepad conectado (padrão: um controle só).
        let second = cfg.pad2 && plat.gamepad_count() >= 2;
        cab.set_pad_entries(
            pads_swapped,
            second,
            if pad_drag.is_some() { pad_hover } else { None },
        );

        if !powered {
            cab.set_session_time(live_session_time(powered_elapsed, powered_since));
            // O picker de memory cards (e a fila de ações/confirmação) vive
            // no console DESLIGADO — a trava física — então o modal tem de
            // ser apresentado NESTE ramo: sem isto ele era criado e nunca
            // aparecia (e os cliques nele não roteavam — o modal_buttons
            // só se atualiza no present).
            if modal != Modal::None {
                cab.present_modal(ModalBackdrop::Static(OFF_STATIC_LEVEL));
            } else {
                cab.present_static(OFF_STATIC_LEVEL);
            }
            pace_frame(&mut next, frame_time);
            continue;
        }

        if !paused && modal == Modal::None {
            // Consome o quadro pronto ANTES de alimentar o worker de novo:
            // enviando antes de consumir no mesmo passe, o try_recv nunca
            // apanha o worker (que leva uns ms por frame) e o consumo caía
            // sempre no passe seguinte — cada frame levava DOIS budgets de
            // pacing, 30 fps cravados.
            if in_flight {
                if let Ok(out) = core_rx.try_recv() {
                    in_flight = false;
                    frames += 1;
                    if spec.debug_note_capture && frames == debug_note_frame {
                        note_request = Some(note_slot);
                    }
                    if audio.queued_frames() < audio_cap && !cfg.mute_game {
                        audio.queue(&out.audio);
                    }

                    // Earned do servidor chegando: une no set da sessão.
                    if let Some(rx) = &ra_earned_worker {
                        match rx.try_recv() {
                            Ok((_, Some(ids))) => {
                                if let Some(ra) = &mut ra_session {
                                    ra.absorb_earned(&ids);
                                }
                                ra_earned_worker = None;
                            }
                            Ok((_, None)) => ra_earned_worker = None,
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                ra_earned_worker = None
                            }
                            Err(std::sync::mpsc::TryRecvError::Empty) => {}
                        }
                    }

                    // RA evaluation contra a RAM do quadro real (cópia que o
                    // worker fez antes do run-ahead).
                    if let (true, Some(ra), Some((ptr, len))) = (powered, &mut ra_session, &out.ram)
                    {
                        // SAFETY: o worker está bloqueado no recv — o Run
                        // seguinte só é enviado depois deste bloco — então a
                        // RAM do core não é mutada nem realocada durante a
                        // leitura; o ponteiro é válido enquanto o jogo
                        // carregado vive (re-consultado a cada Run).
                        let ram = unsafe { std::slice::from_raw_parts(*ptr as *const u8, *len) };
                        let unlocks = ra.tick(ram);
                        for unlock in unlocks {
                            log::info!(
                                "ra: CONQUISTA DESBLOQUEADA — {} (+{} pts)",
                                unlock.title,
                                unlock.points
                            );
                            crate::sfx::play(cab, crate::sfx::Sfx::Achievement);
                            ra_session
                                .as_ref()
                                .expect("checked above")
                                .submit_unlock(&unlock);
                            let badge_img = unlock.badge.clone();
                            if !badge_img.is_empty() {
                                match crate::ra::badge_path(&badge_img) {
                                    Some(path) => {
                                        if let Ok((w, h, rgba)) = decode_art(&path, 512) {
                                            cab.set_image(osd_badge_id(&unlock.badge), w, h, &rgba);
                                        }
                                    }
                                    None => {
                                        if let Some(path) = crate::ra::download_badge(&badge_img) {
                                            if let Ok((w, h, rgba)) = decode_art(&path, 512) {
                                                cab.set_image(
                                                    osd_badge_id(&unlock.badge),
                                                    w,
                                                    h,
                                                    &rgba,
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                            cab.push_osd(
                                &[&unlock.title, &format!("+{} pts", unlock.points)],
                                Some(osd_badge_id(&unlock.badge)),
                                OSD_UNLOCK_TTL,
                            );
                        }
                    }

                    if let Some(frame) = &out.frame {
                        // Quadro chegando LENTO (vão > ~70 ms = menos de ~14
                        // fps): o jogo está preso no CD — leitor lendo.
                        if last_frame_at.is_some_and(|t| t.elapsed() > Duration::from_millis(70)) {
                            drive_reading_until = Some(Instant::now() + Duration::from_millis(150));
                        }
                        last_frame_at = Some(Instant::now());
                        // O clique do "Printscreen" que estava pendente segura
                        // ESTE quadro (o primeiro depois do clique) e abre a
                        // modal de slots — a captura original do fluxo antigo.
                        if print_pending {
                            print_capture = Some(frame.clone());
                            print_pending = false;
                            modal = Modal::PrintSlot;
                            cab.set_modal(
                                "Onde salvar o print?",
                                &print_slot_rows(&spec.notes_dir, &title, &notes_meta),
                            );
                        }
                        let dims = (frame.width, frame.height);
                        if Some(dims) != last_dims {
                            log::info!(
                                "core framebuffer: {}x{} ({:?})",
                                dims.0,
                                dims.1,
                                frame.format
                            );
                            last_dims = Some(dims);
                        }
                        // XRGB8888: o FrameRef EMPRESTA os pixels do core
                        // (zero-cópia); RGB565: converte no buffer. Em ambos
                        // o buffer fica guardado para o "reapresenta".
                        let fref: FrameRef = if frame.format == EmuFormat::Xrgb8888 {
                            // Zero-cópia: o FrameRef EMPRESTA os pixels do
                            // core diretamente.
                            FrameRef {
                                width: frame.width,
                                height: frame.height,
                                pitch: frame.width as usize * 4,
                                format: PlatFormat::Xrgb8888,
                                pixels: &frame.pixels,
                            }
                        } else {
                            store_into(frame, &mut last_frame);
                            FrameRef {
                                width: frame.width,
                                height: frame.height,
                                pitch: frame.width as usize * 4,
                                format: PlatFormat::Xrgb8888,
                                pixels: &last_frame,
                            }
                        };
                        cab.set_session_time(live_session_time(powered_elapsed, powered_since));
                        cab.present_frame(&fref, out.aspect);
                        store_into(frame, &mut last_frame);
                        last_frame_dims = Some((frame.width, frame.height));
                        last_aspect = out.aspect;

                        if let Some(slot) = note_request.take() {
                            match save_note_image(&spec.notes_dir, &title, slot, frame) {
                                Ok(_) => {
                                    refresh_notes(
                                        cab,
                                        &spec.notes_dir,
                                        &title,
                                        note_slot,
                                        text_slot,
                                        &notes_meta,
                                    );
                                    log::info!("note: captured into slot {slot}");
                                }
                                Err(e) => log::warn!("note capture failed: {e}"),
                            }
                        }

                        // A captura do --shot acontece num quadro REAL (um
                        // dupe não regenera a imagem; contamos runs e
                        // capturamos no primeiro Some depois do alvo).
                        if frames >= spec.shot.as_ref().map_or(u32::MAX, |(_, at)| *at) {
                            if let Some((path, _)) = &spec.shot {
                                if let Some((w, h)) = last_frame_dims {
                                    let fref = FrameRef {
                                        width: w,
                                        height: h,
                                        pitch: w as usize * 4,
                                        format: PlatFormat::Xrgb8888,
                                        pixels: &last_frame,
                                    };
                                    cab.capture_bmp(&fref, last_aspect, path)
                                        .map_err(|e| anyhow!(e.to_string()))?;
                                    log::info!("wrote {} after {} frames", path.display(), frames);
                                }
                                break 'run GameExit::Quit;
                            }
                        }
                    }

                    // Flush periódico do SRAM quando mudou (via comando, o
                    // core vive na thread do worker) — só para cores cuja
                    // fonte de card é o SAVE_RAM; o Rearmed persiste por
                    // arquivo e o espelho dele não é confiável para isso.
                    if !core_cards_via_file && frames.is_multiple_of(SRAM_FLUSH_FRAMES) {
                        let (tx, rx) = std::sync::mpsc::channel();
                        let _ = core_tx.send(CoreCmd::Sram { tx });
                        if let Ok(s) = rx.recv() {
                            flush_sram(&sram_path, &mut last_sram, s);
                        }
                        if current_card2.is_some() {
                            let (tx, rx) = std::sync::mpsc::channel();
                            let _ = core_tx.send(CoreCmd::SramAt {
                                id: MEMORY_SAVE_RAM + 1,
                                tx,
                            });
                            if let Ok(s) = rx.recv() {
                                flush_sram(&sram2_path, &mut last_sram2, s);
                            }
                        }
                    }
                }
            }
            // Mantém o worker alimentado: um Run em voo por vez. No FIM do
            // passe — o quadro em voo é consumido no próximo, a tempo do
            // pacing, sem desperdiçar uma iteração por frame.
            if !in_flight {
                let mut snap = PadSnapshot::default();
                for (port, analog_enabled) in analog_on.iter().enumerate().take(MAX_PORTS) {
                    // O clássico da troca de entrada: com os controles
                    // "trocados" no painel, o port lê o gamepad da outra
                    // entrada (pad 1 joga como 2 — Psycho Mantis aprova).
                    let src = if pads_swapped {
                        MAX_PORTS - 1 - port
                    } else {
                        port
                    };
                    for (rb, pb) in PAD {
                        snap.buttons.push((port, rb, input.held(src, pb)));
                    }
                    // O botão ANALOG da entrada: desligado, os sticks ficam
                    // mudos (o controle vira digital, igual ao original).
                    if *analog_enabled {
                        snap.analog.push((
                            port,
                            input.analog(src, 0).0,
                            input.analog(src, 0).1,
                            input.analog(src, 1).0,
                            input.analog(src, 1).1,
                        ));
                    } else {
                        snap.analog.push((port, 0, 0, 0, 0));
                    }
                }
                let _ = core_tx.send(CoreCmd::Run { input: snap });
                in_flight = true;
            }
            // O disco deslizando para fora/para dentro (remoção e inserção
            // quentes): anima no mesmo passe do jogo — quem congela a imagem
            // com estática é o cold-eject (saída da tela), não este.
            if let Some((t0, ejecting)) = disc_motion {
                let span = if ejecting { 430.0 } else { 520.0 };
                let t = (t0.elapsed().as_secs_f32() * 1000.0 / span).min(1.0);
                cab.set_cartridge_motion(Some((t, ejecting)));
                if t >= 1.0 {
                    if !ejecting {
                        cab.set_cartridge_motion(None); // assentado de volta
                    } // totalmente fora fica em (1.0, true): p=0, não desenha
                    disc_motion = None;
                }
            }
            // Sem quadro novo (worker ocupado no boot/FMV), reapresenta o
            // último: o gabinete segue vivo e o disco continua girando.
            if let Some((w, h)) = last_frame_dims {
                let fref = FrameRef {
                    width: w,
                    height: h,
                    pitch: w as usize * 4,
                    format: PlatFormat::Xrgb8888,
                    pixels: &last_frame,
                };
                cab.set_session_time(live_session_time(powered_elapsed, powered_since));
                cab.present_frame(&fref, last_aspect);
            }
            // O rasgo do "disco arranhado" durante a janela de erro.
            if let Some(t0) = disc_glitch {
                if t0.elapsed() < Duration::from_millis(1200) {
                    cab.draw_glitch_overlay(&mut glitch_rng);
                } else {
                    disc_glitch = None; // pendurou: quadro congelado
                }
            }
            // O drive "lê" quando o core hesita: mais de 250 ms desde o
            // último quadro real (ou desde o ligar — o boot lê o disco) —
            // ou quando os quadros chegam lento (loading com tela viva:
            // o jogo corre devagar lendo o CD em stream). Com a tampa
            // aberta o leitor está PARADO — nada de som de leitura.
            if let Some(loop_samples) = cd_loop {
                let hesitating = !lid_open
                    && (last_frame_at.is_none_or(|t| t.elapsed() > Duration::from_millis(250))
                        || drive_reading_until.is_some_and(|t| Instant::now() < t));
                cab.tick_cd_noise(hesitating, loop_samples, crate::sfx::RATE);
            }
        } else if paused {
            // Paused, not stepping: the book, not a frozen game frame.
            cab.present_pause();
            if let Some(loop_samples) = cd_loop {
                cab.tick_cd_noise(false, loop_samples, crate::sfx::RATE);
            }
        } else {
            // A save/load-state or print slot picker is open (plan
            // revision): the modal, frozen same as the book is — composto
            // sobre o ÚLTIMO quadro do jogo, dentro da TV.
            cab.present_modal(ModalBackdrop::Frame {
                aspect_ratio: last_aspect,
            });
        }

        pace_frame(&mut next, frame_time);
    };

    // Final SRAM flush on the way out (either exit path) — o worker é
    // desligado depois dos flushes (Quit derruba a thread).
    let (f_tx, f_rx) = std::sync::mpsc::channel();
    let _ = core_tx.send(CoreCmd::Sram { tx: f_tx });
    flush_sram(&sram_path, &mut last_sram, f_rx.recv().ok().flatten());
    // O core desliga AGORA: ao cair, ele escreve os cards nos arquivos
    // (o card 2 mora no ARQUIVO dele). A sincronia do card 2 com a
    // biblioteca espera essa escrita terminar.
    drop(core_tx);
    let _ = worker_handle.join();
    // O card 1 também mora no ARQUIVO do core: o que o jogo salvou na
    // sessão volta para o card encaixado (biblioteca) ou para o sram.srm —
    // desde que o encaixe não tenha mudado na sessão (troca quente troca o
    // destino; o conteúdo do core pertence ao card que estava no boot).
    let core_card1 = std::path::Path::new(&spec.save_dir).join(MC1_SHARED_FILE);
    if sram_path == initial_sram_path && core_card1.exists() && sram_path != core_card1 {
        match std::fs::copy(&core_card1, &sram_path) {
            Ok(_) => log::info!(
                "card 1: {} sincronizado do arquivo do core",
                sram_path.display()
            ),
            Err(e) => log::warn!("card 1: sincronizando {e}"),
        }
    }
    if current_card2.is_some() {
        // O conteúdo que o core escreveu volta para o card da biblioteca
        // que estava encaixado no slot 2.
        let shared = std::path::Path::new(&spec.save_dir).join(MC2_SHARED_FILE);
        if shared.exists() {
            if let Some(dest) = &current_card2 {
                let _ = std::fs::copy(&shared, dest);
                log::info!("card 2: {} sincronizado", dest.display());
            }
        }
    }
    // Bank whatever powered-on stretch was still running (exiting while on,
    // e.g. window closed mid-session) and add it to the all-time total.
    if let Some(t) = powered_since.take() {
        powered_elapsed += t.elapsed();
    }
    add_playtime(&spec.save_dir, &title, powered_elapsed.as_secs());

    if let Some(ra) = &mut ra_session {
        ra.save_progress();
    }
    log::info!("game loop done");
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::card_rows_with_icons;
    use super::{
        add_playtime, cheat_state_path, delete_text_slot, frame_to_rgb8, game_dir,
        legacy_note_text_path, load_cheat_state, migrate_legacy_text_notes, note_dir,
        note_slot_path, note_text_slot_path, read_text_slot, rom_title, save_cheat_state,
        save_note_image, save_text_slot, sram_file, state_file, total_playtime_secs, EmuFrame,
    };
    use polystnx_emulation::PixelFormat as EmuFormat;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("polystnx-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cheat_state_round_trips_through_disk() {
        let dir = scratch_dir("cheats");
        let path = dir.join("test.cheats");

        save_cheat_state(&path, &[true, false, true]);
        assert_eq!(load_cheat_state(&path, 3), vec![true, false, true]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_defaults_everything_off() {
        let path = std::env::temp_dir().join("polystnx-cheat-test-missing.cheats");
        assert_eq!(load_cheat_state(&path, 2), vec![false, false]);
    }

    #[test]
    fn playtime_accumulates_across_sessions() {
        let dir = scratch_dir("playtime");
        assert_eq!(total_playtime_secs(&dir, "Game"), 0);

        add_playtime(&dir, "Game", 90);
        assert_eq!(total_playtime_secs(&dir, "Game"), 90);

        add_playtime(&dir, "Game", 30);
        assert_eq!(total_playtime_secs(&dir, "Game"), 120);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn playtime_zero_seconds_skips_the_write() {
        let dir = scratch_dir("playtime-zero");
        add_playtime(&dir, "Game", 0);
        assert!(!game_dir(&dir, "Game").join("playtime.txt").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn card_library_lists_mcr_files_and_marks_the_current() {
        let dir = scratch_dir("cards");
        // Um card válido (128 KB com o "MC") e um arquivo lixo.
        let mut card = vec![0u8; 131_072];
        card[0..2].copy_from_slice(b"MC");
        std::fs::write(dir.join("Cartão 1.mcr"), card).unwrap();
        std::fs::write(dir.join("RPG.mcr"), b"b").unwrap();
        std::fs::write(dir.join("leia-me.txt"), b"not a card").unwrap();

        let (rows, cards) = card_rows_with_icons(&dir, None, None);
        // Linha 0 é sempre "criar"; depois os .mcr em ordem de nome. O
        // conteúdo b"a" não é um card válido — o rótulo avisa.
        assert_eq!(rows[0].0, "(criar cartão novo)");
        assert_eq!(cards.len(), 2);
        assert!(rows[1].0.starts_with("Cartão 1"));
        assert!(rows[1].0.contains("0/15 blocos"));
        assert!(rows[2].0.contains("inválido"));

        // O card no slot ganha o sufixo "— no slot".
        let (rows, _) = card_rows_with_icons(&dir, Some(&cards[0]), None);
        assert!(rows[1].0.contains("- no slot"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn card_name_generator_skips_existing() {
        let dir = scratch_dir("card-name");
        let make = |n: u32| {
            let mut card = vec![0u8; 131_072];
            card[0..2].copy_from_slice(b"MC");
            std::fs::write(dir.join(format!("Cartão {n}.mcr")), card).unwrap();
        };
        assert_eq!(
            crate::memcard::create_blank(&dir).unwrap().file_name(),
            Some(std::ffi::OsStr::new("Cartão 1.mcr"))
        );
        make(1);
        assert_eq!(
            crate::memcard::create_blank(&dir).unwrap().file_name(),
            Some(std::ffi::OsStr::new("Cartão 2.mcr"))
        );
        make(2);
        assert_eq!(
            crate::memcard::create_blank(&dir).unwrap().file_name(),
            Some(std::ffi::OsStr::new("Cartão 3.mcr"))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rom_title_is_the_file_stem() {
        assert_eq!(
            rom_title(std::path::Path::new("/roms/Super Mario World (USA).sfc")),
            "Super Mario World (USA)"
        );
    }

    #[test]
    fn frame_to_rgb8_decodes_rgb565_bit_layout() {
        // Pure red, green, blue, white — 5-6-5 packed little-endian.
        let px: [u16; 4] = [0xF800, 0x07E0, 0x001F, 0xFFFF];
        let mut pixels = Vec::with_capacity(8);
        for p in px {
            pixels.extend_from_slice(&p.to_le_bytes());
        }
        let frame = EmuFrame {
            width: 4,
            height: 1,
            pitch: 8,
            format: EmuFormat::Rgb565,
            pixels,
        };
        let img = frame_to_rgb8(&frame);
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(img.get_pixel(1, 0).0, [0, 255, 0]);
        assert_eq!(img.get_pixel(2, 0).0, [0, 0, 255]);
        assert_eq!(img.get_pixel(3, 0).0, [255, 255, 255]);
    }

    #[test]
    fn note_slots_save_independently() {
        let dir = scratch_dir("notes");
        let frame = EmuFrame {
            width: 2,
            height: 2,
            pitch: 4,
            format: EmuFormat::Rgb565,
            pixels: vec![0u8; 4 * 2],
        };

        assert!(!note_slot_path(&dir, "Aladdin", 1).is_file());
        save_note_image(&dir, "Aladdin", 1, &frame).unwrap();
        save_note_image(&dir, "Aladdin", 15, &frame).unwrap();
        assert!(note_dir(&dir, "Aladdin").join("01.png").is_file());
        assert!(note_dir(&dir, "Aladdin").join("15.png").is_file());
        // Slot 2 is untouched — the two saves above didn't spill into it.
        assert!(!note_slot_path(&dir, "Aladdin", 2).is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_slot_round_trips_and_deletes() {
        let dir = scratch_dir("notes-text-slot");

        assert_eq!(read_text_slot(&dir, "Aladdin", 1), None);
        save_text_slot(&dir, "Aladdin", 1, "primeira nota").unwrap();
        save_text_slot(&dir, "Aladdin", 15, "outra nota").unwrap();
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("primeira nota".to_string())
        );
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 15),
            Some("outra nota".to_string())
        );
        // Slot 2 was never written — reading it back is empty, not an error.
        assert_eq!(read_text_slot(&dir, "Aladdin", 2), None);

        // Overwriting a slot replaces its content, doesn't append to it.
        save_text_slot(&dir, "Aladdin", 1, "nota substituida").unwrap();
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("nota substituida".to_string())
        );

        delete_text_slot(&dir, "Aladdin", 1);
        assert_eq!(read_text_slot(&dir, "Aladdin", 1), None);
        assert!(!note_text_slot_path(&dir, "Aladdin", 1).is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn whitespace_only_text_slot_reads_back_as_empty() {
        let dir = scratch_dir("notes-text-blank");
        save_text_slot(&dir, "Aladdin", 1, "   \n  ").unwrap();
        assert_eq!(read_text_slot(&dir, "Aladdin", 1), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_notas_txt_migrates_into_numbered_slots() {
        let dir = scratch_dir("notes-migrate");
        let legacy_dir = note_dir(&dir, "Aladdin");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(
            legacy_note_text_path(&dir, "Aladdin"),
            "primeira pagina\n\nsegunda pagina\n\n",
        )
        .unwrap();

        migrate_legacy_text_notes(&dir, "Aladdin");

        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("primeira pagina".to_string())
        );
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 2),
            Some("segunda pagina".to_string())
        );
        // The original file survives the migration untouched.
        assert!(legacy_note_text_path(&dir, "Aladdin").is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migration_is_a_noop_once_any_text_slot_exists() {
        let dir = scratch_dir("notes-migrate-noop");
        std::fs::create_dir_all(note_dir(&dir, "Aladdin")).unwrap();
        std::fs::write(legacy_note_text_path(&dir, "Aladdin"), "pagina antiga\n\n").unwrap();
        save_text_slot(&dir, "Aladdin", 1, "ja no formato novo").unwrap();

        migrate_legacy_text_notes(&dir, "Aladdin");

        // Migration must not have touched slot 1 (already occupied) or
        // spilled the legacy page into slot 2.
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("ja no formato novo".to_string())
        );
        assert_eq!(read_text_slot(&dir, "Aladdin", 2), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn note_dir_sanitizes_path_hostile_titles() {
        let dir = scratch_dir("notes-sanitize");
        let d = note_dir(&dir, "Foo/Bar: The \"Game\"?");
        // A single direct child of `dir` — no path traversal, no nested
        // directories from the slashes/colons in the title.
        assert_eq!(d.parent(), Some(dir.as_path()));
        assert!(!d.file_name().unwrap().to_string_lossy().contains('/'));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_paths_are_grouped_by_game_with_plain_names() {
        let dir = scratch_dir("saves-layout");
        let title = "Aladdin";

        // All under saves_dir/<title>/, not a flat file per kind at the
        // root — same folder-per-game shape as notes.
        assert_eq!(
            state_file(&dir, title, 3),
            game_dir(&dir, title).join("3.state")
        );
        assert_eq!(
            sram_file(&dir, title),
            game_dir(&dir, title).join("sram.srm")
        );
        assert_eq!(
            cheat_state_path(&dir, title),
            game_dir(&dir, title).join("cheats.txt")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn game_dir_sanitizes_path_hostile_titles() {
        let dir = scratch_dir("saves-sanitize");
        let d = game_dir(&dir, "Foo/Bar: The \"Game\"?");
        assert_eq!(d.parent(), Some(dir.as_path()));
        assert!(!d.file_name().unwrap().to_string_lossy().contains('/'));

        std::fs::remove_dir_all(&dir).ok();
    }
}
