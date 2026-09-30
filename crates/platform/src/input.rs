//! Keyboard + gamepad, folded into one SNES-shaped button matrix.
//!
//! Gamepad navigation is the primary path (plan §3.1); the keyboard binds come
//! from a [`KeyMap`] the app builds (defaults plus a config file).

use std::str::FromStr;

use sdl3::gamepad::Button as PadBtn;
use sdl3::keyboard::Keycode;

/// PSX / libretro joypad buttons, in libretro id order. The pad grew up:
/// L2/R2 close the 14 digital buttons of the DualShock (the sticks ride the
/// analog axes, sampled straight off the gamepad — see [`Input::analog`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PadButton {
    B = 0,
    Y = 1,
    Select = 2,
    Start = 3,
    Up = 4,
    Down = 5,
    Left = 6,
    Right = 7,
    A = 8,
    X = 9,
    L = 10,
    R = 11,
    L2 = 12,
    R2 = 13,
}

impl PadButton {
    pub const ALL: [PadButton; 14] = [
        PadButton::B,
        PadButton::Y,
        PadButton::Select,
        PadButton::Start,
        PadButton::Up,
        PadButton::Down,
        PadButton::Left,
        PadButton::Right,
        PadButton::A,
        PadButton::X,
        PadButton::L,
        PadButton::R,
        PadButton::L2,
        PadButton::R2,
    ];

    /// Config token, e.g. `"start"`, `"l"`.
    pub fn token(self) -> &'static str {
        match self {
            PadButton::B => "b",
            PadButton::Y => "y",
            PadButton::Select => "select",
            PadButton::Start => "start",
            PadButton::Up => "up",
            PadButton::Down => "down",
            PadButton::Left => "left",
            PadButton::Right => "right",
            PadButton::A => "a",
            PadButton::X => "x",
            PadButton::L => "l",
            PadButton::R => "r",
            PadButton::L2 => "l2",
            PadButton::R2 => "r2",
        }
    }
}

impl FromStr for PadButton {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        PadButton::ALL
            .into_iter()
            .find(|b| b.token() == s)
            .ok_or(())
    }
}

/// Controller ports. The SNES has more with a multitap; two covers the plan.
pub const MAX_PORTS: usize = 2;

/// High-level events the app acts on. Meaning (e.g. "power off") is decided a
/// layer up; the platform only reports the intent. None of these are
/// keyboard-bindable any more (plan revision: mouse/gamepad only for every
/// console/UI command) — they're only ever produced by a panel-button
/// `Click` being resolved by the caller, or (`CloseRequested`) the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    /// Power toggle — off saves/signals-off (console dark, cartridge still in
    /// the slot, plan §3.3), on again resumes exactly where it was. Always
    /// mouse-driven now (the panel's Power button); a click while already
    /// mid-transition does nothing new.
    Quit,
    /// The OS asked the window to close (red button, Cmd-Q, `SIGTERM`). Always
    /// means "tear the whole app down", never "go back".
    CloseRequested,
    /// **Ejetar** — only takes effect once the console is off; while it's
    /// still on the lock resists (plan §3.3).
    Eject,
    Reset,
    /// Also what the panel's "Anotacoes" button fires (plan revision — it
    /// replaced "Pausar": clicking it pauses *and* lands straight on the
    /// notebook, one action instead of two).
    TogglePause,
    /// Open the save-state slot picker — a modal, not an immediate save
    /// (plan revision). The game freezes while it's open, same as the
    /// notebook; picking a slot (`ModalPick`) is what actually saves.
    OpenSaveModal,
    /// Open the load-state slot picker, mirroring `OpenSaveModal`.
    OpenLoadModal,
    /// Open the "which slot for this print" picker (plan revision — replaces
    /// the old direct-capture "Nota" button): the frame to save is grabbed
    /// the moment this fires, so whichever slot the player ends up picking
    /// (after the game has frozen) gets the frame from the instant they
    /// clicked, not whatever's on screen once they're done choosing.
    OpenPrintModal,
    /// A slot row clicked inside whichever modal (`OpenSaveModal`/
    /// `OpenLoadModal`/`OpenPrintModal`) is open right now — only reachable
    /// during the row-picking step; the naming step that can follow
    /// (`OpenPrintModal`) is polled via `Platform::poll_text_entry` instead,
    /// same as the notebook's own text editor, so it never produces a
    /// `Click`/`UiEvent` at all (see `PanelButton::ModalConfirm`). `u16`,
    /// not `u8` (plan revision): the Cheats modal can run into the
    /// thousands of rows for a database-heavy game, well past what a byte
    /// addresses.
    ModalPick(u16),
    /// Back out of whichever modal is open, discarding any choice so far —
    /// only reachable during the row-picking step, same caveat as
    /// `ModalPick`.
    ModalCancel,
    /// Scroll whichever modal's row grid is open one page up/down — only
    /// reachable (and only drawn at all) when there are more rows than fit
    /// in the card at once (plan revision: the Cheats modal made lists
    /// long enough that this stopped being hypothetical).
    ModalScrollUp,
    ModalScrollDown,
    /// Abrir a biblioteca de memory cards (plano §3) — clicar no botão
    /// "MC slot 1" do painel; a trava de troca só-desligado é do runner.
    OpenCards,
    /// O mesmo para o "MC slot 2".
    OpenCards2,
    /// Ligar o console SEM disco — boot direto na BIOS.
    BootBios,
    /// Abrir o seletor de discos do jogo (m3u, plano §6) — o bloco
    /// "Discos" da pausa.
    OpenDiscos,
    /// Click the Cheats modal's search box — only drawn (and only
    /// clickable) on a modal that's marked searchable, which today is only
    /// Cheats (plan revision: the libretro-database expansion made
    /// scrolling through everything by hand impractical for some games'
    /// 1000+-cheat lists). Polled via `Platform::poll_text_entry`, same
    /// keyboard-typing exception as the notebook's own editors.
    ModalSearchStart,
    /// On a searchable modal (today only Cheats): switch its on/off state
    /// filter to show every row, only the ones checked on, or only the
    /// ones off (plan revision).
    ModalFilterAll,
    ModalFilterOn,
    ModalFilterOff,
    /// Open the Cheats modal — a checklist of curated codes for this
    /// cartridge (plan revision: its own menu, split out of the notebook,
    /// which was getting crowded and had no room for a "pausar pra editar"
    /// hint to make sense of it). Toggling a row (`ModalPick`) flips it and
    /// keeps the modal open, unlike the other three modals' one-shot pick.
    OpenCheatsModal,
    OpenAchievementsModal,
    /// Pause book: step the right page (the photo album) to an earlier/
    /// later print slot.
    NotePrev,
    NoteNext,
    /// Pause book: open the editor for the left page's currently-shown
    /// text-note slot (plan revision — repurposed from "append a new,
    /// always-blank free-text page": now targets whichever of the 15 slots
    /// `TextPrev`/`TextNext` left showing, pre-filled with what's already
    /// there so editing doesn't start by erasing it) — the one deliberate
    /// keyboard-typing exception, see `Platform::poll_text_entry`.
    NoteWriteStart,
    /// Pause book: toggle whether the right page's shown print slot
    /// resists a future "Printscreen" overwrite (plan revision).
    NotePinToggle,
    /// Pause book: open the editor for the shown print slot's caption
    /// (plan revision) — same keyboard-typing exception as `NoteWriteStart`.
    NoteNameStart,
    /// Pause book: step the left page (the 15 text-note slots, plan
    /// revision) to an earlier/later one — independent of `NotePrev`/
    /// `NoteNext`, which move the *right* page's print slot instead.
    TextPrev,
    TextNext,
    /// Pause book: toggle whether the left page's shown text-note slot
    /// resists a future overwrite *or delete* (plan revision — pin means
    /// "protected" the same way it already does for a print slot).
    TextPinToggle,
    /// Pause book: clear the left page's shown text-note slot — refused
    /// while it's pinned, same protection `TextPinToggle` grants a print
    /// slot against being overwritten.
    TextDelete,
    /// Left mouse button went down, in **window** coordinates — the caller
    /// (which owns the `Cabinet`) converts to output/canvas space via
    /// `Cabinet::window_to_output` before hit-testing anything.
    Click(i32, i32),
}

/// Keyboard bindings for actual SNES gameplay input (D-pad/face buttons) —
/// the one thing that's still keyboard by default, since a gamepad isn't
/// guaranteed to be plugged in. Built from [`KeyMap::default`] then
/// overridden per the app's config. Gamepad bindings stay fixed (SDL's
/// controller DB already normalises devices). Every console/UI command
/// (eject, reset, pause, save state, ...) used to have a rebindable key here
/// too; it doesn't any more — those are mouse/gamepad-menu only (plan
/// revision), so there's nothing left to bind for them.
#[derive(Debug, Clone, Default)]
pub struct KeyMap {
    pad: Vec<(Keycode, PadButton)>,
}

impl KeyMap {
    /// The built-in layout (arrows = dpad, Z/X = △/□, 1/2 = L2/R2, …).
    pub fn defaults() -> Self {
        let pad = [
            ("Up", PadButton::Up),
            ("Down", PadButton::Down),
            ("Left", PadButton::Left),
            ("Right", PadButton::Right),
            ("Z", PadButton::B),
            ("X", PadButton::A),
            ("A", PadButton::Y),
            ("S", PadButton::X),
            ("Q", PadButton::L),
            ("W", PadButton::R),
            ("1", PadButton::L2),
            ("2", PadButton::R2),
            ("Return", PadButton::Start),
            ("Right Shift", PadButton::Select),
        ];
        let mut m = KeyMap::default();
        for (name, b) in pad {
            m.bind_pad(name, b).expect("valid default key name");
        }
        m
    }

    /// Parse an SDL key name (`"Left Shift"`, `"F2"`, `"]"`). Case-sensitive as
    /// SDL defines it.
    pub fn parse_key(name: &str) -> Option<Keycode> {
        Keycode::from_name(name)
    }

    /// Rebind a pad button; the previous key(s) for it are dropped.
    pub fn bind_pad(&mut self, key_name: &str, button: PadButton) -> Result<(), String> {
        let k = Self::parse_key(key_name).ok_or_else(|| format!("unknown key {key_name:?}"))?;
        self.pad.retain(|&(kk, bb)| kk != k && bb != button);
        self.pad.push((k, button));
        Ok(())
    }

    /// `(action token, SDL key name)` for every current bind — for writing
    /// out a config file.
    pub fn describe(&self) -> Vec<(String, String)> {
        let mut v = Vec::new();
        for b in PadButton::ALL {
            if let Some(&(k, _)) = self.pad.iter().find(|&&(_, bb)| bb == b) {
                v.push((b.token().to_string(), k.name()));
            }
        }
        v
    }

    pub(crate) fn pad_for(&self, k: Keycode) -> Option<PadButton> {
        self.pad.iter().find(|&&(kk, _)| kk == k).map(|&(_, b)| b)
    }
}

/// Gamepad-button layout: which SDL gamepad button drives each SNES button —
/// the gamepad counterpart of [`KeyMap`]. SDL's controller DB normalizes the
/// button *names* across devices, but clones can still land a press in the
/// wrong slot — so the layout is rebindable in the settings screen, same as
/// the keyboard's, and persists in the config's `[gamepad]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PadMap {
    pad: Vec<(PadBtn, PadButton)>,
}

impl PadMap {
    /// The built-in layout (d-pad, B/A/Y/X on South/East/West/North, L/R on
    /// the shoulders, select/start on back/start).
    pub fn defaults() -> Self {
        let mut m = PadMap::default();
        for (btn, b) in DEFAULT_PAD_MAP {
            m.pad.push((btn, b));
        }
        m
    }

    /// Rebind a SNES button to an SDL gamepad-button name (`"south"`,
    /// `"dpad_up"`, …). One button per action and one action per button —
    /// the previous pairs on either side are dropped, like [`KeyMap::bind_pad`].
    pub fn bind(&mut self, button_name: &str, b: PadButton) -> Result<(), String> {
        let Some(btn) = PadBtn::from_string(button_name) else {
            return Err(format!("unknown gamepad button {button_name:?}"));
        };
        self.pad.retain(|&(bb, pb)| bb != btn && pb != b);
        self.pad.push((btn, b));
        Ok(())
    }

    /// `(action token, SDL gamepad button name)` for every SNES button, in
    /// libretro order — the settings rows' and the config file's source.
    /// An unbound action comes back with an empty name.
    pub fn describe(&self) -> Vec<(String, String)> {
        PadButton::ALL
            .iter()
            .map(|&b| {
                let name = self
                    .pad
                    .iter()
                    .find(|&&(_, pb)| pb == b)
                    .map(|&(bb, _)| bb.string())
                    .unwrap_or_default();
                (b.token().to_string(), name)
            })
            .collect()
    }

    pub(crate) fn pairs(&self) -> &[(PadBtn, PadButton)] {
        &self.pad
    }
}

/// The factory layout every X-input-style pad ships with.
const DEFAULT_PAD_MAP: [(PadBtn, PadButton); 12] = [
    (PadBtn::DPadUp, PadButton::Up),
    (PadBtn::DPadDown, PadButton::Down),
    (PadBtn::DPadLeft, PadButton::Left),
    (PadBtn::DPadRight, PadButton::Right),
    (PadBtn::South, PadButton::B),
    (PadBtn::East, PadButton::A),
    (PadBtn::West, PadButton::Y),
    (PadBtn::North, PadButton::X),
    (PadBtn::LeftShoulder, PadButton::L),
    (PadBtn::RightShoulder, PadButton::R),
    (PadBtn::Back, PadButton::Select),
    (PadBtn::Start, PadButton::Start),
];

/// Per-frame button + analog state. Filled by `Platform::poll`. Keyboard
/// drives port 0; gamepad *n* drives port *n*.
#[derive(Default)]
pub struct Input {
    keys: [bool; 14],
    pads: [[bool; 14]; MAX_PORTS],
    /// `[port][stick] = (x, y)` in the full ±i16 range, already deadzoned.
    sticks: [[(i16, i16); 2]; MAX_PORTS],
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    /// Button `b` held on `port` this frame (keyboard counts for port 0).
    pub fn held(&self, port: usize, b: PadButton) -> bool {
        let i = b as usize;
        (port == 0 && self.keys[i]) || (port < MAX_PORTS && self.pads[port][i])
    }

    /// Stick `stick` (0 = left, 1 = right) on `port` this frame, ±i16.
    pub fn analog(&self, port: usize, stick: usize) -> (i16, i16) {
        if port < MAX_PORTS && stick < 2 {
            self.sticks[port][stick]
        } else {
            (0, 0)
        }
    }

    pub(crate) fn set_key(&mut self, b: PadButton, down: bool) {
        self.keys[b as usize] = down;
    }

    pub(crate) fn set_pad(&mut self, port: usize, b: PadButton, down: bool) {
        if port < MAX_PORTS {
            self.pads[port][b as usize] = down;
        }
    }

    pub(crate) fn set_stick(&mut self, port: usize, stick: usize, x: i16, y: i16) {
        if port < MAX_PORTS && stick < 2 {
            self.sticks[port][stick] = (x, y);
        }
    }

    pub(crate) fn clear_pads(&mut self) {
        self.pads = [[false; 14]; MAX_PORTS];
        self.sticks = [[(0, 0); 2]; MAX_PORTS];
    }
}
