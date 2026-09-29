//! Platform layer: everything OS- and SDL-specific. Nothing above this line in
//! the architecture knows SDL exists.

mod audio;
mod cabinet;
mod input;

pub use audio::AudioOut;
pub use cabinet::{
    Cabinet, FrameRef, PanelButton, PanelSection, PixelFormat, RaStatus, Screen, SettingsButton,
    SettingsPanelInfo, ShelfButton, ShelfPanelInfo, UpdateArrow, BRAND, DEMO_BADGE_IMG,
    RA_LOGO_IMG,
};
pub use input::{Input, KeyMap, PadButton, PadMap, UiEvent, MAX_PORTS};
// The SDL gamepad button enum, for callers that hold a captured press
// (`MenuInput::captured_pad`) — the app crate doesn't link SDL itself.
pub use sdl3::gamepad::Button as GamepadBtn;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button as PadBtn, Gamepad};
use sdl3::joystick::JoystickId;
use sdl3::mouse::MouseButton;
use thiserror::Error;

/// Diagnóstico de engasgo, gateado por `PSX_XPERIENCE_TRACE=1` na hora de rodar.
/// Com ele ligado, warns apontam qual fase estourou o budget do frame:
/// `poll_*` (fila de eventos SDL + gamepad), `present` (submissão/vsync de
/// GPU — estourar aqui aponta para compositor/tela, não para o app) e o
/// atraso de frame no `pace_frame` (loop não fechou o budget por qualquer
/// motivo). Sem a variável, o custo é um `OnceLock` lido uma vez.
pub fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("PSX_XPERIENCE_TRACE").is_some_and(|v| v != "0"))
}

/// Cronometra o escopo onde vive e, com o trace ligado, avisa se passar de
/// ~2 ms. `None` (sem custo) quando desligado.
pub(crate) struct TraceSpan {
    name: &'static str,
    start: Instant,
}

impl TraceSpan {
    pub(crate) fn new(name: &'static str) -> Option<Self> {
        trace_enabled().then_some(Self {
            name,
            start: Instant::now(),
        })
    }
}

impl Drop for TraceSpan {
    fn drop(&mut self) {
        let took = self.start.elapsed();
        if took > Duration::from_millis(2) {
            log::warn!(
                "trace: {} levou {:.1} ms",
                self.name,
                took.as_secs_f64() * 1e3
            );
        }
    }
}

/// Left-button-down position, in window coordinates, or `None` for anything
/// else. Shared between `poll` and `poll_menu` so the SDL event match isn't
/// duplicated.
fn left_click_at(event: &Event) -> Option<(i32, i32)> {
    match event {
        Event::MouseButtonDown {
            mouse_btn: MouseButton::Left,
            x,
            y,
            ..
        } => Some((*x as i32, *y as i32)),
        _ => None,
    }
}

/// Right-button-down position, in window coordinates — the text fields'
/// "colar aqui" gesture, mirroring `left_click_at`.
fn right_click_at(event: &Event) -> Option<(i32, i32)> {
    match event {
        Event::MouseButtonDown {
            mouse_btn: MouseButton::Right,
            x,
            y,
            ..
        } => Some((*x as i32, *y as i32)),
        _ => None,
    }
}

/// What `poll_menu` should do with keydowns this call. Mouse click + gamepad
/// nav (`MENU_PAD_MAP`) drive `Nav` regardless — no keyboard shortcuts left
/// for navigation itself. `CaptureKey` is the one deliberate exception: its
/// whole job is recording a keyboard key to bind for gameplay input, so it's
/// the only mode `poll_menu` still reads keydowns for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuMode {
    /// Grid/list browsing: mouse click, gamepad d-pad/buttons, and the mouse
    /// wheel (mapped to `Up`/`Down`) are the only inputs.
    Nav,
    /// Rebinding a control: the next key pressed OR gamepad button pressed
    /// comes back raw in `captured_key`/`captured_pad`; Escape cancels instead
    /// of being captured.
    CaptureKey,
}

/// A directional / confirm / back intent from the selector's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuNav {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    PageUp,
    PageDown,
    Home,
    End,
}

/// One step of the Konami code, input-agnostic — arrows on the keyboard,
/// d-pad on the pad, and the closing B/A either as letter keys or as the
/// pad's East/South (the menu Back/Confirm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KonamiStep {
    Up,
    Down,
    Left,
    Right,
    B,
    A,
}

/// The code itself (plan revision: "deve ser ultra secreto — konami code na
/// tela inicial do app").
const KONAMI: [KonamiStep; 10] = [
    KonamiStep::Up,
    KonamiStep::Up,
    KonamiStep::Down,
    KonamiStep::Down,
    KonamiStep::Left,
    KonamiStep::Right,
    KonamiStep::Left,
    KonamiStep::Right,
    KonamiStep::B,
    KonamiStep::A,
];

/// What one press did to the in-progress Konami sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KonamiFeed {
    /// Matched the expected step; the sequence is still short of complete.
    Advanced,
    /// The final A landed — the code just completed.
    Completed,
    /// Wrong step; the progress reset (to 1 if the press itself starts the
    /// code, i.e. an Up, else to 0).
    Mismatch,
}

/// One frame's worth of menu input.
#[derive(Default)]
pub struct MenuInput {
    pub quit: bool,
    pub nav: Vec<MenuNav>,
    /// The Konami code (↑↑↓↓←→←→BA) completed this frame — arrow keys on
    /// the keyboard plus B/A, or the pad equivalent (d-pad + East/South).
    /// Only tracked while [`Platform::set_konami_watch`] is on, so screens
    /// that opt out never see a swallowed input or a completed sequence.
    /// The app uses it to unlock the dev mode; nothing else reacts to it.
    pub konami: bool,
    /// Set only when `poll_menu` was called with `capture_key: true` and a
    /// key went down this frame: its raw SDL name, for rebinding a control.
    pub captured_key: Option<String>,
    /// Same capture mode, gamepad side: a gamepad button went down this
    /// frame — the settings screen binds whichever arrives first, key or
    /// pad button.
    pub captured_pad: Option<GamepadBtn>,
    /// Set only in capture mode: Escape cancels the capture instead of being
    /// captured as the new binding.
    pub capture_cancelled: bool,
    /// Left click this frame, in window coordinates (see `UiEvent::Click`).
    pub click: Option<(i32, i32)>,
    /// Right click this frame, in window coordinates — the screens use it
    /// only where right-click means something (the text fields' "colar").
    pub right_click: Option<(i32, i32)>,
}

/// One frame's worth of input while writing a free-text note (`Platform::
/// poll_text_entry`) — the pause book's note editor, the one deliberate
/// keyboard-typing exception (plan revision: everything else is
/// mouse/gamepad only).
#[derive(Default)]
pub struct TextEntryInput {
    pub quit: bool,
    /// Composed text typed this frame — usually one character, sometimes
    /// more (IME, paste-like input methods), sometimes empty.
    pub typed: String,
    pub backspace: bool,
    /// Cmd/Ctrl+V this frame: the clipboard's text, sanitized for a
    /// single-line field (control characters dropped), or `None` when no
    /// paste happened / the clipboard couldn't be read.
    pub paste: Option<String>,
    /// Cmd/Ctrl+C this frame — the caller copies the field's whole content
    /// (these drafts have no selection to honor).
    pub copy: bool,
    /// Return/Enter — commit the draft.
    pub commit: bool,
    /// Escape — discard the draft.
    pub cancel: bool,
    /// Left click this frame, in window coordinates.
    pub click: Option<(i32, i32)>,
}

/// Drops control characters from pasted text. Every text field in the app
/// is single-line, and a token copied with a trailing newline (the usual
/// way to get one) would otherwise smuggle it into the draft.
fn sanitize_paste(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// The clipboard's text, sanitized for a single-line field. `None` when the
/// clipboard can't be read or has nothing left worth pasting. Free function
/// because it's called mid-`poll_iter`, with `event_pump` still borrowed.
fn clipboard_paste(video: &sdl3::VideoSubsystem) -> Option<String> {
    let text = video.clipboard().clipboard_text().ok()?;
    let clean = sanitize_paste(&text);
    (!clean.is_empty()).then_some(clean)
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("SDL error: {0}")]
    Sdl(String),
}

impl From<sdl3::Error> for PlatformError {
    fn from(e: sdl3::Error) -> Self {
        PlatformError::Sdl(e.to_string())
    }
}

impl From<String> for PlatformError {
    fn from(e: String) -> Self {
        PlatformError::Sdl(e)
    }
}

/// Owns the SDL context and its subsystems. Create one, keep it alive for the
/// whole process.
pub struct Platform {
    pub sdl: sdl3::Sdl,
    pub video_subsystem: sdl3::VideoSubsystem,
    pub audio_subsystem: sdl3::AudioSubsystem,
    event_pump: sdl3::EventPump,
    gamepad_subsystem: sdl3::GamepadSubsystem,
    /// Open gamepads, in order — index n drives port n. Paired with the
    /// `JoystickId` each was opened from, so a device change only touches the
    /// pads that actually left or arrived (see `refresh_gamepads_if_changed`).
    gamepads: Vec<(JoystickId, Gamepad)>,
    /// The full id list SDL reported on the last gamepad sync — the diff
    /// basis that keeps a connect/disconnect flap from reopening anything.
    seen_ids: Vec<JoystickId>,
    /// A gamepad appeared or left since the last sync; the next `poll*` call
    /// resolves it against `seen_ids` (cheap check, zero cost while idle).
    devices_pending: bool,
    /// Rising-edge tracking for `poll_menu`'s gamepad buttons.
    menu_prev: [bool; MENU_PAD_MAP.len()],
    /// SNES-button layout of the gamepads — the pair table `sample_gamepads`
    /// walks. Defaults to the factory layout; the settings screen swaps pairs
    /// through `set_pad_map` (persisted by the app).
    pad_map: PadMap,
    /// Whether `poll_menu` is currently watching for the Konami code — on
    /// only while the app's idle (home) screen runs, so the sequence never
    /// half-triggers (or swallows a pad Back/Confirm) anywhere else.
    konami_watch: bool,
    /// How much of [`KONAMI`] is currently matched, while `konami_watch` is
    /// on — an index into the sequence, reset on any wrong step.
    konami_step: usize,
    /// Cliques sintéticos agendados (testes de UI) pendentes de disparo.
    scheduled_clicks: Vec<(Instant, i32, i32)>,
}

const MENU_PAD_MAP: [(PadBtn, MenuNav); 8] = [
    (PadBtn::DPadUp, MenuNav::Up),
    (PadBtn::DPadDown, MenuNav::Down),
    (PadBtn::DPadLeft, MenuNav::Left),
    (PadBtn::DPadRight, MenuNav::Right),
    (PadBtn::South, MenuNav::Confirm),
    (PadBtn::East, MenuNav::Back),
    (PadBtn::LeftShoulder, MenuNav::PageUp),
    (PadBtn::RightShoulder, MenuNav::PageDown),
];

/// A keydown's Konami step, when it has one — arrow keys for the directions,
/// the letters B and A for the finish (the classic keyboard form of the
/// code).
fn konami_key_step(k: sdl3::keyboard::Keycode) -> Option<KonamiStep> {
    use sdl3::keyboard::Keycode;
    match k {
        Keycode::Up => Some(KonamiStep::Up),
        Keycode::Down => Some(KonamiStep::Down),
        Keycode::Left => Some(KonamiStep::Left),
        Keycode::Right => Some(KonamiStep::Right),
        Keycode::B => Some(KonamiStep::B),
        Keycode::A => Some(KonamiStep::A),
        _ => None,
    }
}

/// A pad-driven menu intent's Konami step — the d-pad for the directions,
/// East/B and South/A (the menu's Back/Confirm) for the finish.
fn konami_nav_step(nav: MenuNav) -> Option<KonamiStep> {
    match nav {
        MenuNav::Up => Some(KonamiStep::Up),
        MenuNav::Down => Some(KonamiStep::Down),
        MenuNav::Left => Some(KonamiStep::Left),
        MenuNav::Right => Some(KonamiStep::Right),
        MenuNav::Back => Some(KonamiStep::B),
        MenuNav::Confirm => Some(KonamiStep::A),
        MenuNav::PageUp | MenuNav::PageDown | MenuNav::Home | MenuNav::End => None,
    }
}

/// Feed one press into the in-progress Konami sequence — a free function
/// over just the progress index, so `poll_menu` can call it while the event
/// pump / open pads still borrow parts of `self`.
fn konami_feed(progress: &mut usize, step: KonamiStep) -> KonamiFeed {
    if step == KONAMI[*progress] {
        *progress += 1;
        if *progress == KONAMI.len() {
            *progress = 0;
            return KonamiFeed::Completed;
        }
        return KonamiFeed::Advanced;
    }
    *progress = if step == KONAMI[0] { 1 } else { 0 };
    KonamiFeed::Mismatch
}

impl Platform {
    pub fn new() -> Result<Self, PlatformError> {
        // Fullscreen sem os Spaces do macOS: a transição nativa de zoom lia
        // como "abre de um tamanho e dá uma aumentada" — com Spaces fora, a
        // janela cobre a tela na hora, sem animação. Vale para janelas
        // criadas depois do hint (o init abaixo).
        sdl3::hint::set("SDL_VIDEO_MAC_FULLSCREEN_SPACES", "0");
        let sdl = sdl3::init()?;
        let video_subsystem = sdl.video()?;
        let audio_subsystem = sdl.audio()?;
        let gamepad_subsystem = sdl.gamepad()?;
        let event_pump = sdl.event_pump()?;
        let mut me = Self {
            sdl,
            video_subsystem,
            audio_subsystem,
            event_pump,
            gamepad_subsystem,
            gamepads: Vec::new(),
            seen_ids: Vec::new(),
            devices_pending: false,
            menu_prev: [false; MENU_PAD_MAP.len()],
            pad_map: PadMap::defaults(),
            konami_watch: false,
            konami_step: 0,
            scheduled_clicks: Vec::new(),
        };
        me.sync_gamepads();
        Ok(me)
    }

    /// (Re)open the connected gamepads (up to `MAX_PORTS`), keeping every pad
    /// that's already open: `SDL_OpenGamepad` runs on the main thread (IOKit/
    /// GCController binding, easily tens of ms), so a USB controller whose
    /// connection flaps must not reopen the *stable* pads — each reopen lands
    /// right in the UI's frame budget and reads as stutter. Ports follow SDL's
    /// enumeration order, same as before.
    fn sync_gamepads(&mut self) {
        let Ok(ids) = self.gamepad_subsystem.gamepads() else {
            return;
        };
        self.seen_ids = ids.clone();
        self.seen_ids.sort_unstable();
        let mut leftovers = std::mem::take(&mut self.gamepads);
        let mut gamepads = Vec::with_capacity(leftovers.len());
        for id in ids.into_iter().take(input::MAX_PORTS) {
            if let Some(pos) = leftovers.iter().position(|(pid, _)| *pid == id) {
                gamepads.push(leftovers.swap_remove(pos));
            } else if let Ok(pad) = self.gamepad_subsystem.open(id) {
                log::info!(
                    "gamepad port {}: {}",
                    gamepads.len(),
                    pad.name().unwrap_or_default()
                );
                gamepads.push((id, pad));
            }
        }
        // Whatever `leftovers` still holds was unplugged; dropping it closes
        // the pads.
        self.gamepads = gamepads;
    }

    /// Resolve a pending hotplug: only when the connected-id set actually
    /// differs from the last sync does anything reopen. A flap that comes and
    /// goes between two frames (or removes and re-adds before the next poll
    /// sees both events) collapses into a no-op; a real change syncs once.
    fn refresh_gamepads_if_changed(&mut self) {
        if !self.devices_pending {
            return;
        }
        self.devices_pending = false;
        let Ok(ids) = self.gamepad_subsystem.gamepads() else {
            return;
        };
        let mut now = ids;
        now.sort_unstable();
        if now != self.seen_ids {
            self.sync_gamepads();
        }
    }

    /// The one window: a dark cabinet with the screen recessed into it. Both the
    /// game and the selector draw into that screen area; nothing recreates it.
    pub fn create_cabinet(
        &self,
        title: &str,
        width: u32,
        height: u32,
        fullscreen: bool,
    ) -> Result<Cabinet, PlatformError> {
        Cabinet::new(
            &self.video_subsystem,
            &self.audio_subsystem,
            title,
            width,
            height,
            fullscreen,
        )
    }

    /// Testing hook (example harnesses): push a synthetic left-button
    /// down+up at window coordinates into the event queue, indistinguishable
    /// from a real click to `poll`/`poll_menu`.
    pub fn push_synthetic_click(&self, x: i32, y: i32) {
        for down in [true, false] {
            let ev = if down {
                Event::MouseButtonDown {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: x as f32,
                    y: y as f32,
                }
            } else {
                Event::MouseButtonUp {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: x as f32,
                    y: y as f32,
                }
            };
            let _ = self.sdl.event().expect("event subsystem").push_event(ev);
        }
    }

    /// Testing hook, `push_synthetic_click`'s scheduled variant: registers a
    /// left click at window coordinates to be pushed `delay` from now — the
    /// due ones fire at the top of the next `poll`/`poll_menu` (same thread;
    /// the SDL context can't cross threads). Lets example harnesses drive
    /// menus that are already inside their own loop.
    pub fn push_synthetic_click_later(&mut self, x: i32, y: i32, delay: Duration) {
        self.scheduled_clicks.push((Instant::now() + delay, x, y));
    }

    /// Dispara os cliques agendados que já venceram.
    fn fire_due_clicks(&mut self) {
        let now = Instant::now();
        let due: Vec<_> = self
            .scheduled_clicks
            .iter()
            .filter(|(at, _, _)| *at <= now)
            .map(|(_, x, y)| (*x, *y))
            .collect();
        self.scheduled_clicks.retain(|(at, _, _)| *at > now);
        for (x, y) in due {
            self.push_synthetic_click(x, y);
        }
    }

    /// Drain events for a menu screen: mouse click, mouse wheel (-> `Up`/
    /// `Down`), and gamepad d-pad/buttons (rising edge only, `MENU_PAD_MAP`)
    /// always feed `nav`. In `CaptureKey` mode only, a keydown is captured
    /// raw instead — see [`MenuMode`].
    pub fn poll_menu(&mut self, mode: MenuMode) -> MenuInput {
        use sdl3::keyboard::Keycode;
        self.fire_due_clicks();
        let _span = TraceSpan::new("poll_menu");
        let mut out = MenuInput::default();
        for event in self.event_pump.poll_iter() {
            if let Some(pos) = left_click_at(&event) {
                out.click = Some(pos);
                continue;
            }
            if let Some(pos) = right_click_at(&event) {
                out.right_click = Some(pos);
                continue;
            }
            match event {
                Event::Quit { .. } => out.quit = true,
                Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => {
                    self.devices_pending = true
                }
                Event::MouseWheel { y, .. } => {
                    if y > 0.0 {
                        out.nav.push(MenuNav::Up);
                    } else if y < 0.0 {
                        out.nav.push(MenuNav::Down);
                    }
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat,
                    ..
                } if mode == MenuMode::CaptureKey => {
                    if repeat {
                        continue;
                    }
                    if k == Keycode::Escape {
                        out.capture_cancelled = true;
                    } else if out.captured_key.is_none() {
                        out.captured_key = Some(k.name());
                    }
                }
                Event::GamepadButtonDown { button, .. } if mode == MenuMode::CaptureKey => {
                    out.captured_pad = Some(button);
                }
                // Nav mode normally ignores the keyboard entirely (no
                // keyboard shortcuts) — the one exception is the Konami
                // watcher: while the idle screen has it on, arrows + B/A
                // feed the secret sequence and nothing else.
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } if mode == MenuMode::Nav && self.konami_watch => {
                    if let Some(step) = konami_key_step(k) {
                        if matches!(
                            konami_feed(&mut self.konami_step, step),
                            KonamiFeed::Completed
                        ) {
                            out.konami = true;
                        }
                    }
                }
                _ => {}
            }
        }
        self.refresh_gamepads_if_changed();
        // Gamepad: rising edges only.
        let pad = self.gamepads.first().map(|(_, pad)| pad);
        for (i, (btn, nav)) in MENU_PAD_MAP.iter().enumerate() {
            let down = pad.map(|p| p.button(*btn)).unwrap_or(false);
            if down && !self.menu_prev[i] {
                // The pad's Konami steps ride the same buttons the menus use:
                // the d-pad is inert on the idle screen, but the closing B/A
                // are Back/Confirm there — quit the app / open the shelf. So
                // once the arrow run is matched, those two presses feed the
                // sequence AND are swallowed; anywhere short of that they
                // pass through untouched.
                let mut swallow = false;
                if self.konami_watch {
                    if let Some(step) = konami_nav_step(*nav) {
                        let closing = matches!(step, KonamiStep::B | KonamiStep::A);
                        match konami_feed(&mut self.konami_step, step) {
                            KonamiFeed::Completed => {
                                out.konami = true;
                                swallow = closing;
                            }
                            // "Advanced" with a closing step is exactly the
                            // B at position 8 (B only matches there) — the
                            // one Back press that must not quit the app.
                            KonamiFeed::Advanced => swallow = closing,
                            KonamiFeed::Mismatch => {}
                        }
                    }
                }
                if !swallow {
                    out.nav.push(*nav);
                }
            }
            self.menu_prev[i] = down;
        }
        out
    }

    /// Turn on OS text composition (IME, dead keys, the works) for `cab`'s
    /// window — call before the first `poll_text_entry` of a writing session
    /// (plan revision: the pause book's free-text note is the one deliberate
    /// keyboard-typing exception to "mouse/gamepad only").
    pub fn start_text_input(&self, cab: &Cabinet) {
        self.video_subsystem.text_input().start(cab.window());
    }

    /// Turn text composition back off — call once the draft is saved or
    /// cancelled, so a stray keypress elsewhere doesn't get eaten as text.
    pub fn stop_text_input(&self, cab: &Cabinet) {
        self.video_subsystem.text_input().stop(cab.window());
    }

    /// Drain events while writing free text: composed text (`Event::
    /// TextInput`, which handles layout/IME properly — no hand-rolled
    /// shift/keycode mapping), Backspace, Return (commit), Escape (cancel),
    /// a click (to hit "Salvar"/"Cancelar" or click away) and the field
    /// copy/paste shortcuts (⌘C/⌘V on macOS, Ctrl elsewhere — the one
    /// thing TextInput events don't carry, and what makes the RA token
    /// pasteable straight from the browser). Nothing else is read —
    /// gameplay input stays untouched while a note is open.
    pub fn poll_text_entry(&mut self) -> TextEntryInput {
        use sdl3::keyboard::{Keycode, Mod};
        let _span = TraceSpan::new("poll_text_entry");
        let mut out = TextEntryInput::default();
        for event in self.event_pump.poll_iter() {
            if let Some(pos) = left_click_at(&event) {
                out.click = Some(pos);
                continue;
            }
            // Botão direito enquanto digita: colar — o mesmo `paste` do
            // atalho de teclado, sem tirar a mão do mouse.
            if matches!(
                event,
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Right,
                    ..
                }
            ) {
                out.paste = clipboard_paste(&self.video_subsystem);
                continue;
            }
            match event {
                Event::Quit { .. } => out.quit = true,
                Event::TextInput { text, .. } => out.typed.push_str(&text),
                // Hotplug durante a digitação não é lido aqui, mas o evento
                // seria engolido (e o controle ficaria morto até o próximo
                // flap) — marca para o sync resolver antes do próximo frame.
                Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => {
                    self.devices_pending = true
                }
                Event::KeyDown {
                    keycode: Some(k),
                    keymod,
                    repeat: false,
                    ..
                } => {
                    let shortcut = keymod
                        .intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
                    if shortcut {
                        match k {
                            Keycode::C => out.copy = true,
                            Keycode::V => out.paste = clipboard_paste(&self.video_subsystem),
                            _ => {}
                        }
                    }
                    match k {
                        Keycode::Backspace => out.backspace = true,
                        Keycode::Return | Keycode::KpEnter => out.commit = true,
                        Keycode::Escape => out.cancel = true,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        self.refresh_gamepads_if_changed();
        out
    }

    /// Copy `text` to the OS clipboard — the text fields' Cmd/Ctrl+C. The
    /// drafts have no selection, so it's always the whole field. `false`
    /// when the OS refuses; there's nothing better to do than ignore it.
    pub fn set_clipboard_text(&self, text: &str) -> bool {
        self.video_subsystem
            .clipboard()
            .set_clipboard_text(text)
            .is_ok()
    }

    /// The clipboard's text, sanitized for a single-line field — what a
    /// text field pastes (right-clicking a closed field opens it already
    /// pasting). `None` when the clipboard can't be read or has nothing
    /// left worth pasting.
    pub fn paste_from_clipboard(&self) -> Option<String> {
        clipboard_paste(&self.video_subsystem)
    }

    pub fn open_audio(&self, sample_rate: u32) -> Result<AudioOut, PlatformError> {
        AudioOut::new(&self.audio_subsystem, sample_rate)
    }

    pub fn new_input(&self) -> Input {
        Input::new()
    }

    /// Drain the event queue, update `input` via `keymap` (gameplay D-pad/
    /// buttons only — every console/UI command is mouse-only now, reported
    /// as [`UiEvent::Click`] for the caller to resolve via
    /// `Cabinet::hit_panel_button`), and return the click plus an OS close
    /// request ([`UiEvent::CloseRequested`]), if either happened this frame.
    pub fn poll(&mut self, input: &mut Input, keymap: &KeyMap) -> Vec<UiEvent> {
        let _span = TraceSpan::new("poll");
        let mut out = Vec::new();
        for event in self.event_pump.poll_iter() {
            if let Some((x, y)) = left_click_at(&event) {
                out.push(UiEvent::Click(x, y));
                continue;
            }
            match event {
                Event::Quit { .. } => out.push(UiEvent::CloseRequested),
                Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => {
                    self.devices_pending = true
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => {
                    if let Some(b) = keymap.pad_for(k) {
                        input.set_key(b, true);
                    }
                }
                Event::KeyUp {
                    keycode: Some(k), ..
                } => {
                    if let Some(b) = keymap.pad_for(k) {
                        input.set_key(b, false);
                    }
                }
                _ => {}
            }
        }
        self.refresh_gamepads_if_changed();
        self.sample_gamepads(input);
        out
    }

    /// Swap the gamepad SNES-button layout (settings screen); takes effect on
    /// the next `poll`.
    pub fn set_pad_map(&mut self, map: PadMap) {
        self.pad_map = map;
    }

    /// Turn the Konami code watcher on/off (the app's idle screen turns it on
    /// for as long as it runs; every other screen leaves it off). Turning it
    /// on (re)starts the sequence from zero.
    pub fn set_konami_watch(&mut self, on: bool) {
        self.konami_watch = on;
        self.konami_step = 0;
    }

    fn sample_gamepads(&self, input: &mut Input) {
        input.clear_pads();
        for (port, (_, pad)) in self.gamepads.iter().enumerate() {
            for (btn, mapped) in self.pad_map.pairs() {
                if pad.button(*btn) {
                    input.set_pad(port, *mapped, true);
                }
            }
            // DualShock: sticks sampled straight off the axes (deadzone under
            // ~12% to absorb drift), L2/R2 are the trigger axes with a press
            // threshold — SDL models them as axes, not buttons.
            const DEADZONE: i32 = 4000;
            const TRIGGER: i16 = 8192;
            let stick = |axis: Axis| -> i16 {
                let raw = pad.axis(axis) as i32;
                if raw.abs() < DEADZONE {
                    0
                } else {
                    raw as i16
                }
            };
            input.set_stick(port, 0, stick(Axis::LeftX), stick(Axis::LeftY));
            input.set_stick(port, 1, stick(Axis::RightX), stick(Axis::RightY));
            if pad.axis(Axis::TriggerLeft) >= TRIGGER {
                input.set_pad(port, PadButton::L2, true);
            }
            if pad.axis(Axis::TriggerRight) >= TRIGGER {
                input.set_pad(port, PadButton::R2, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{konami_feed, konami_nav_step, sanitize_paste, KonamiFeed, KonamiStep, MenuNav};

    #[test]
    fn sanitize_paste_drops_control_chars_and_keeps_the_rest() {
        assert_eq!(sanitize_paste("abc123-_."), "abc123-_.");
        // A token copied from the browser usually comes with a newline.
        assert_eq!(sanitize_paste("tok3n\n"), "tok3n");
        assert_eq!(sanitize_paste("li\nne\rtab\there"), "linetabhere");
        assert_eq!(sanitize_paste("são çedilha ✨"), "são çedilha ✨");
        assert_eq!(sanitize_paste("\n\r\t"), "");
    }

    /// Feed a whole sequence, returning what the last press did.
    fn feed_all(progress: &mut usize, steps: &[KonamiStep]) -> KonamiFeed {
        let mut last = KonamiFeed::Mismatch;
        for &s in steps {
            last = konami_feed(progress, s);
        }
        last
    }

    use KonamiStep::{Down, Left, Right, Up, A, B};

    #[test]
    fn konami_completes_on_the_exact_sequence() {
        let mut p = 0;
        assert_eq!(
            feed_all(
                &mut p,
                &[Up, Up, Down, Down, Left, Right, Left, Right, B, A]
            ),
            KonamiFeed::Completed
        );
    }

    #[test]
    fn konami_a_wrong_step_kills_the_run() {
        let mut p = 0;
        // Almost there, then a stray Down on the Left run.
        assert_eq!(
            feed_all(
                &mut p,
                &[Up, Up, Down, Down, Left, Right, Left, Right, B, B]
            ),
            KonamiFeed::Mismatch
        );
        // And the dead run doesn't finish on a lucky A.
        assert_ne!(
            feed_all(&mut p, &[A]),
            KonamiFeed::Completed,
            "progress reset to 0; a lone A is nothing"
        );
    }

    #[test]
    fn konami_completion_restarts_from_zero() {
        let mut p = 0;
        assert_eq!(
            feed_all(
                &mut p,
                &[Up, Up, Down, Down, Left, Right, Left, Right, B, A]
            ),
            KonamiFeed::Completed
        );
        // Holding Up (one press after a completed code) only re-arms step 1.
        assert_eq!(konami_feed(&mut p, Up), KonamiFeed::Advanced);
        assert_eq!(p, 1);
    }

    #[test]
    fn konami_up_reuses_the_tail_of_a_broken_run() {
        let mut p = 0;
        // Up Up Up: the third Up breaks the run but itself restarts it
        // (the code begins with Up), so only one fresh press is lost.
        assert_eq!(
            feed_all(&mut p, &[Up, Up, Up]),
            KonamiFeed::Mismatch,
            "the third Up is a mismatch"
        );
        assert_eq!(p, 1, "…but it left the run armed at step 1");
        assert_eq!(
            feed_all(&mut p, &[Up, Down, Down, Left, Right, Left, Right, B, A]),
            KonamiFeed::Completed
        );
    }

    #[test]
    fn konami_pad_map_sends_the_finishing_buttons() {
        // On the pad the code ends in East then South — the menu's Back and
        // Confirm — which is exactly why those two presses need swallowing
        // while the run is live.
        assert_eq!(konami_nav_step(MenuNav::Back), Some(B));
        assert_eq!(konami_nav_step(MenuNav::Confirm), Some(A));
        assert_eq!(konami_nav_step(MenuNav::PageUp), None);
    }
}
