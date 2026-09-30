//! The one window: a static dark cabinet with the screen recessed into it. Both
//! the running game and the selector draw into that screen area — the game as a
//! frame through a barrel-distorted CRT mesh, the selector as flat 2D (rects,
//! bitmap text, letterboxed images). The cabinet furniture (the chamfer ring
//! from the window edge down to the glass) is redrawn every frame so nothing
//! ever recreates the window. NTSC colour bleed is applied upstream
//! (`xperience-ntsc`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use noto_sans_mono_bitmap::{get_raster, FontWeight, RasterHeight};
use sdl3::pixels::{Color, FColor, PixelFormat as SdlFormat};
use sdl3::rect::Rect;
use sdl3::render::{
    BlendMode, ClippingRect, ScaleMode as SdlScaleMode, Texture, Vertex, WindowCanvas,
};
use sdl3::{AudioSubsystem, VideoSubsystem};

use crate::{audio::AudioOut, PlatformError};

/// CRT tube shape. `WARP` is how hard the edges bow (0 = flat); `VIGNETTE` is
/// how much the corners darken; `GRID` is the mesh resolution.
const CRT_WARP: f32 = 0.06;
const CRT_VIGNETTE: f32 = 0.22;
const CRT_GRID: usize = 32;

/// Cabinet around the tube. The screen is inset from the window by these
/// fractions (a bit more at the bottom for the "chin"); everything outside is
/// the cabinet face, chamfered down to a near-black recess at the screen edge.
const BEZEL_SIDE: f32 = 0.070;
const BEZEL_TOP: f32 = 0.070;
const BEZEL_CHIN: f32 = 0.110;
/// Cabinet face — dark warm-grey plastic. Reads as a surface, still far darker
/// than a lit game screen (plan §3.2).
const CABINET: (u8, u8, u8) = (40, 37, 33);
/// The lip right against the glass, in shadow.
const RECESS: (u8, u8, u8) = (4, 4, 5);
/// Reserved image-cache key for the side panel's logo art.
const PANEL_LOGO_IMG: u64 = u64::MAX - 1;
/// Reserved image-cache key for the panel's most-recent note thumbnail.
const PANEL_NOTE_IMG: u64 = u64::MAX - 2;
/// Reserved image-cache key for the pause book's right-hand page.
const PAUSE_THUMB_IMG: u64 = u64::MAX - 3;
/// Reserved image-cache key for the side panel's cartridge art.
const PANEL_CARTRIDGE_IMG: u64 = u64::MAX - 4;
/// Reserved image-cache key for the idle screen's console-brand logo (plan
/// revision — `assets/console.png`, set once via `Cabinet::set_console_logo`,
/// not per-game).
const PANEL_CONSOLE_LOGO_IMG: u64 = u64::MAX - 5;
/// Reserved image-cache key for the console tag wordmark printed on the
/// slot's loading base (plan revision — `assets/console-tag.png`, set once
/// via `Cabinet::set_slot_tag`, not per-game).
const SLOT_TAG_IMG: u64 = u64::MAX - 6;

/// The pause book: two pages, not warped by the tube — a dedicated screen
/// (plan §3.2/§3.4), not cabinet furniture, so it replaces the whole window
/// rather than sharing space with the panel/cartridge/brand.
const PAUSE_MARGIN: f32 = 0.06;
const PAUSE_GAP: f32 = 0.02;
const PAUSE_TOP: f32 = 0.08;

/// The side panel: a column of plain widgets beside the tube during play —
/// not warped, drawn straight on the window (plan §2's presentation order,
/// §3.2). A fixed fraction of the window, clamped so it neither disappears on
/// a small window nor swallows a huge one.
const PANEL_FRAC: f32 = 0.25;
const PANEL_MIN: u32 = 260;
const PANEL_MAX: u32 = 520;
const PANEL_BG: (u8, u8, u8) = (16, 15, 14);
const PANEL_TEXT: (u8, u8, u8) = (225, 220, 210);
const PANEL_DIM: (u8, u8, u8) = (140, 134, 124);
/// Warm amber for the idle screen's core warning (plan revision) — reads as
/// "atenção" without shouting over the panel's own palette.
const PANEL_WARN: (u8, u8, u8) = (228, 180, 90);
/// Fill for a clickable panel button (`draw_button`) — a shade lighter than
/// `PANEL_BG` so it reads as its own control, not flat background text.
const PANEL_BTN_BG: (u8, u8, u8) = (34, 32, 29);

/// O gabinete do PSX no painel: o console de frente, plástico cinza do
/// SCPH-1001 — corpo, tampa com o hub ao centro, a fenda entre tampa e corpo
/// por onde o disco entra, e a face com slots de memory card, portas de
/// controle e o LED de power.
const PSX_SHELL: (u8, u8, u8) = (176, 172, 162);
const PSX_SHELL_EDGE: (u8, u8, u8) = (126, 122, 112);
const PSX_HUB: (u8, u8, u8) = (146, 142, 132);
const PSX_HUB_RING: (u8, u8, u8) = (120, 116, 107);
const PSX_LED_ON: (u8, u8, u8) = (110, 224, 130);
const PSX_LED_OFF: (u8, u8, u8) = (56, 58, 54);
/// O rótulo gravado dentro do botão Power (cinza claro, como a gravação
/// da carcaça na foto de referência).
const PSX_BTN_TEXT: (u8, u8, u8) = (168, 166, 158);

/// The set's own nameplate: a small wordmark printed into the chin, left of
/// the cartridge — a touch lighter than the cabinet plastic, like an embossed
/// badge rather than a lit label.
/// The console's own brand name — the nameplate's default text before the
/// app calls `Cabinet::set_nameplate` with its own version appended, and
/// what the idle screen's panel falls back to in plain text when there's no
/// logo image loaded (`draw_panel`'s idle branch).
pub const BRAND: &str = "PSX Xperience";
const BRAND_TEXT: (u8, u8, u8) = (92, 86, 78);

/// Glyph cell (Noto Sans Mono, anti-aliased, rasterized once at boot into an
/// atlas texture — see `build_font_atlas`), before scaling: 20px tall gives
/// noticeably bigger, smoother UI text than the old 8x8 bitmap font while
/// staying monospace, so every existing cell-based layout calculation below
/// keeps working unchanged.
const FONT_HEIGHT: RasterHeight = RasterHeight::Size20;
const FONT_WEIGHT: FontWeight = FontWeight::Regular;
/// Advance width and line height of one glyph cell, before scaling.
const GLYPH_W: u32 = 9;
const GLYPH_H: u32 = 20;
/// Atlas covers one contiguous Unicode range: printable Basic Latin through
/// Latin-1 Supplement (space through `ÿ`) — plain ASCII plus the accented
/// letters Portuguese needs (á, ã, ç, é, õ, ...), in one indexable block.
const GLYPH_FIRST: u32 = 0x20;
const GLYPH_LAST: u32 = 0xFF;
const GLYPH_COLS: u32 = GLYPH_LAST - GLYPH_FIRST + 1;

/// Map a char to its column in the font atlas; anything outside the covered
/// range (or with no glyph in the font) falls back to `?`.
fn glyph_index(ch: char) -> u32 {
    let c = ch as u32;
    if (GLYPH_FIRST..=GLYPH_LAST).contains(&c) {
        c - GLYPH_FIRST
    } else {
        '?' as u32 - GLYPH_FIRST
    }
}

/// Pixel layout of a core framebuffer. Mirrors `xperience_emulation::PixelFormat`
/// so the platform layer stays independent of the emulation crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

impl PixelFormat {
    fn sdl(self) -> SdlFormat {
        match self {
            PixelFormat::Rgb1555 => SdlFormat::XRGB1555,
            PixelFormat::Xrgb8888 => SdlFormat::XRGB8888,
            PixelFormat::Rgb565 => SdlFormat::RGB565,
        }
    }
    fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb1555 | PixelFormat::Rgb565 => 2,
            PixelFormat::Xrgb8888 => 4,
        }
    }
}

/// A borrowed core frame ready to upload.
pub struct FrameRef<'a> {
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
    pub format: PixelFormat,
    pub pixels: &'a [u8],
}

pub struct Cabinet {
    canvas: WindowCanvas,
    /// Streaming texture at the incoming frame's resolution (game path).
    src: Option<SrcTexture>,
    /// Cached CRT mesh; rebuilt only when the game rect resizes.
    mesh: Option<CrtMesh>,
    /// Cached cabinet mesh; rebuilt only when the window or screen rect changes.
    bezel: Option<BezelMesh>,
    /// Render target the selector draws its flat 2D into, then composited
    /// through the tube like a game frame.
    screen_tex: Option<SizedTex>,
    /// Small streaming texture for the signal-off snow.
    noise_tex: Option<SizedTex>,
    noise: Vec<u8>,
    /// When the static noise was last regenerated (and at what level) —
    /// `update_noise_tex` caps regeneration at ~30Hz.
    noise_stamp: Option<(std::time::Instant, f32)>,
    // Era o gerador do ruído de RF; a tela azul de AV não sorteia.
    #[allow(dead_code)]
    rng: u32,
    /// Ângulo do disco girando (graus) — incrementa a cada frame ligado.
    spin: f32,
    /// 128 glyphs laid out horizontally, white on transparent (2D path).
    font: Texture,
    images: HashMap<u64, ImgTex>,
    /// The current inner-screen rect (the tube opening).
    screen: Rect,
    /// The side panel content (game path only, plan §3.2). `None` = no game
    /// loaded — the idle/root screen (§3.2 item 1's "Inserir cartucho"
    /// button) draws there instead.
    panel: Option<PanelInfo>,
    /// Elapsed time to show at the bottom of the panel; the caller updates
    /// this once a frame (`set_session_time`).
    session: Duration,
    /// The pause book's content (plan §3.2/§3.4), set once on entering pause
    /// and read every frame while `present_pause` is what's on screen.
    pause: Option<PauseNote>,
    /// A save/load-state or print slot picker (plan revision), set on
    /// opening one and read every frame while `present_modal` is on screen.
    modal: Option<ModalInfo>,
    fullscreen: bool,
    /// Clickable panel buttons drawn last frame, in output/canvas coordinates
    /// — `hit_panel_button` scans this. Repopulated by `present_frame`/
    /// `present_static` right after `draw_panel`.
    panel_buttons: Vec<(PanelButton, Rect)>,
    /// Clickable buttons on the pause book screen ("Continuar"/"Avancar
    /// quadro") drawn last frame — `hit_pause_button` scans this. The pause
    /// book replaces the whole window (no side panel drawn alongside it), so
    /// it needs its own list rather than sharing `panel_buttons`.
    pause_buttons: Vec<(PanelButton, Rect)>,
    /// Clickable buttons on the modal dialog drawn last frame —
    /// `hit_modal_button` scans this, same pattern as `pause_buttons`.
    modal_buttons: Vec<(PanelButton, Rect)>,
    /// The shelf's own flat side panel content (plan revision) — `None`
    /// outside the shelf (game/idle screens keep using `panel` instead).
    shelf_panel: Option<ShelfPanelInfo>,
    /// Clickable buttons on the shelf's flat panel drawn last frame —
    /// `hit_shelf_button` scans this, same pattern as `panel_buttons`.
    shelf_buttons: Vec<(ShelfButton, Rect)>,
    /// The settings screen's own flat panel content (plan revision) — `None`
    /// outside settings.
    settings_panel: Option<SettingsPanelInfo>,
    /// Clickable buttons on the settings panel drawn last frame —
    /// `hit_settings_button` scans this.
    settings_buttons: Vec<(SettingsButton, Rect)>,
    /// The audio subsystem handle, for the ambient static hiss (plan
    /// revision: "som de chiado de tv fora do ar") — the hiss engine itself
    /// lives here because `present_static` is where "TV fora do ar" happens.
    audio: AudioSubsystem,
    /// Whether the ambient hiss should play at all — the settings toggle,
    /// off by default. App-side gate; `present_static` only queues while
    /// this is set.
    static_hiss: bool,
    /// The open hiss stream, kept across frames (idle loops at 60fps);
    /// `None` until the first hissing static frame after the toggle turns
    /// on.
    hiss: Option<AudioOut>,
    /// XORSHIFT state for the hiss samples — same generator the power
    /// bursts use, so the texture of the noise matches.
    hiss_rng: u32,
    /// The idle screen's core-download prompt (plan revision): `Some(label)`
    /// draws a warning plus this button in the idle panel — the label is the
    /// app's own live status ("Baixar núcleo" / progress text). `None`
    /// restores the plain idle panel.
    idle_core_prompt: Option<String>,
    /// O modo dev (plan revision: "konami code na tela inicial do app ...
    /// habilitar botão em cima da configuração") — desligado até o app
    /// religar; ligado, o painel da tela inicial desenha um botão "Dev"
    /// logo acima do "Configurações". Session-only: nunca é persistido, o
    /// segredo não vaza para arquivo nenhum.
    dev_mode: bool,
    /// Linha de status do download do núcleo no painel da tela inicial
    /// (plan revision: "cliquei no icone verde de update do nucleo mas nao
    /// abriu nada") — o download é em segundo plano e o rótulo dele só
    /// existia na tela de setup; aqui o idle mostra "baixando… X MB",
    /// "núcleo atualizado!" ou o motivo da falha.
    core_status: Option<String>,
    /// The foley stream (app-supplied one-shots: insert/eject/power/reset,
    /// plan revision) — opened lazily on the first sound and kept alive for
    /// the process, because a stream dropped right after `queue` destroys
    /// the queued audio before the device ever plays it (the bug that made
    /// the first foley attempt silent).
    foley: Option<AudioOut>,
    /// The "CH 3" channel banner's deadline during gameplay (plan revision:
    /// "quando ligar o console, mostrar por 3 segundos e remover da tela")
    /// — `None` means not showing. The static path (`present_static`)
    /// always draws it instead: no signal, same as an old TV parked on
    /// channel 3.
    ch3_until: Option<Instant>,
    /// Where the cabinet actually drew last frame, in real window/output
    /// pixels — native for displays in the 16:10..16:9 band, else a centered
    /// 16:9 letterbox/pillarbox (plan: don't distort on an ultrawide).
    /// Every other stored rect (`screen`, `panel_buttons`, …) lives in this
    /// rect's own local space; `window_to_output` subtracts its offset
    /// before any hit-test runs.
    canvas_rect: Rect,
    /// The cabinet's own nameplate, printed into the chin by `draw_brand` on
    /// every screen (plan revision: "mostrar versao do app e do SwanStation, onde
    /// esta o nome do app na tv") — starts as just `BRAND`, but the app sets
    /// it once at startup (and again after a core swap) to also carry the
    /// app/core version, via `set_nameplate`.
    nameplate: String,
    /// MOCK (design preview for the achievements notification — RetroAchievements
    /// plan): lines drawn right-aligned in the chin, mirroring `set_nameplate`'s
    /// block on the left. Throwaway scaffolding for `examples/ra_osd_mock.rs`;
    /// the real feature (phase 4) replaces it with a timed OSD queue.
    /// Per-nameplate-line "tem update" flags (plan revision: "quando tiver
    /// update do app ou do SwanStation não mostrar mais a tela cheia e sim um
    /// icone verde no nameplate do lado de cada um") — `(app, core)`. When
    /// set, `draw_brand` prints a small green dot right after that line
    /// (line 0 = app version, line 1 = SwanStation version); the app flips them
    /// via `set_nameplate_updates` once its startup check reports something.
    nameplate_updates: (bool, bool),
    /// Os rects das setas verdes de update no nameplate (app, core) —
    /// preenchidos a cada frame do caminho estático; o idle usa para
    /// abrir os fluxos de atualização (plan revision: "ao clicar no icone
    /// verde de atualizar, atualizar").
    update_arrows: (Option<Rect>, Option<Rect>),
    /// The chin's OSD queue (plan: `docs/plano-retroachievements.md`,
    /// fase 4) — "CONQUISTA DESBLOQUEADA" blocks drawn right-aligned in the
    /// chin, front entry only, expiring by time; the next one slides in
    /// after. Pushed by the runner on an unlock event.
    osd_queue: VecDeque<OsdEntry>,
    /// The chin's persistent RetroAchievements badge: when the account is
    /// on, the chin shows the RA mark + "ATIVADO" + the mode whenever the
    /// OSD queue is empty — an unlock notification still takes the spot
    /// while it lives, and the badge comes back when it expires. `None`
    /// draws nothing (account off, or no cartridge in).
    ra_status: Option<RaStatus>,
    /// Whether the top-left "fechar app" button is drawn/clickable this
    /// screen (plan revision: "criar botao de fechar app no canto superior
    /// esquerdo") — the idle/shelf/settings/history screens turn it on;
    /// gameplay never does (Desligar/Ejetar already cover backing out of a
    /// running game, and a stray click there quitting the whole app outright
    /// would be a much bigger surprise than on a menu screen). Explicitly
    /// set on every screen's own entry, not just left over from whatever ran
    /// before — same pattern `clear_panel`/`set_powered` already follow.

    /// Where the close button drew last frame, in output/canvas coordinates
    /// — `hit_close_button` scans this, same pattern as `panel_buttons`.
    /// Drawn on every screen now (plan revision: "mostrar o fechar e
    /// minimizar em todas as telas"), next to its minimize sibling.
    close_button: Rect,
    /// The top-left minimize button's rect drawn last frame —
    /// `hit_minimize_button` scans this.
    minimize_button: Rect,
}

/// One queued chin OSD block (plan fase 4) — the unlock notification with
/// its expiry.
struct OsdEntry {
    lines: Vec<String>,
    badge: Option<u64>,
    until: Instant,
}

/// The persistent RetroAchievements chin badge's state — just the session
/// mode; the drawing itself is fixed (`draw_chin_ra`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaStatus {
    pub hardcore: bool,
}

/// What the chin draws this frame: a queued notification wins over the
/// persistent RA badge — which is what makes the badge "cede the spot" to
/// each unlock and come back when the notification expires.
enum ChinOsd {
    Notify {
        lines: Vec<String>,
        badge: Option<u64>,
    },
    Ra {
        hardcore: bool,
    },
    None,
}

/// A clickable spot in the side panel: the idle screen's "Inserir cartucho"
/// button, or one of the in-game console commands. `hit_panel_button` turns a
/// click into one of these; the caller (idle screen / `run_game`) decides
/// what each one does. `Hash` is for `runner`'s brief "done!" flash on
/// silent actions (screenshot, note capture, save/load) — keyed by button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelButton {
    Insert,
    Settings,
    /// The idle screen's dev-mode button (plan revision: "habilitar botão em
    /// cima da configuração") — only drawn while the app's dev mode is on,
    /// which only the Konami code on the idle screen can turn on. The dev
    /// menu itself lives app-side.
    Dev,
    /// The idle screen's "Baixar núcleo" button (plan revision: "ao iniciar
    /// o app pela primeira vez e/ou nao tiver o core na pasta, mostrar
    /// botão para baixar") — only drawn while the core is missing; the
    /// download itself lives app-side.
    CoreDownload,
    Power,
    Eject,
    Reset,
    /// Os botões de memory card do painel (plano §3): clicar abre a
    /// biblioteca de cards do slot — a troca em si só acontece desligado (a
    /// trava é do runner, igual à do ejetar).
    Cards1,
    Cards2,
    /// Abre o seletor de discos (jogos m3u, plano §6) — troca o disco no
    /// drive com a sessão viva, estado serializado por baixo.
    Discos,
    /// Pauses and opens the notebook in one click (plan revision — replaces
    /// the old separate "Pausar" button; "Nota" moved to `PrintScreen`).
    Notebook,
    /// Opens the achievements list modal (plan:
    /// `docs/plano-retroachievements.md`, fase 4) — only drawn while an RA
    /// session is active for the loaded game.
    Achievements,
    /// Opens the achievements list on the shelf (plan fase 2/4) — inside
    /// the tube, like the back-cover zoom. Only drawn while the focused
    /// game has an identified RA set.
    ShelfAchievements,
    /// Grabs the current frame and opens a modal to pick which of the 15
    /// note slots to save it into, with a name (plan revision — replaces
    /// the old direct-capture "Nota" button and its slot cycler).
    PrintScreen,
    /// Opens a modal to pick which of the save-state slots to write into
    /// (plan revision — used to save straight into a pre-cycled slot).
    SaveState,
    /// Opens a modal to pick which save-state slot to load, mirroring
    /// `SaveState`.
    LoadState,
    /// Opens the Cheats modal (plan revision — split out of the notebook,
    /// which had no natural home for a checklist next to the photo album).
    /// Absent from the panel entirely for a cartridge with no curated
    /// cheats, same "hide rather than show an empty screen" rule as the
    /// other modal-opening buttons.
    Cheats,
    /// One slot row inside whichever modal (`SaveState`/`LoadState`/
    /// `PrintScreen`) is open right now, or one cheat row inside the Cheats
    /// modal (plan revision — same click addresses both a pick and a
    /// toggle; which it means depends on which modal is open). `u16`, not
    /// `u8` (plan revision): a database-heavy game's Cheats list can run
    /// well past 255 rows.
    ModalSlot(u16),
    /// Confirm the modal's text step (naming a print).
    ModalConfirm,
    /// Back out of whichever modal is open, discarding any choice so far.
    ModalCancel,
    /// Scroll the modal's row grid up/down by one page — only drawn (and
    /// so only clickable) once the row count overflows the card.
    ModalScrollUp,
    ModalScrollDown,
    /// Click the search box on a searchable modal (plan revision — today
    /// only Cheats) to start typing a filter.
    ModalSearchStart,
    /// On a searchable modal (today only Cheats): show every row, only the
    /// ones checked on, or only the ones off (plan revision) — a segmented
    /// three-way control, one of the three always "active" (`lit`).
    ModalFilterAll,
    ModalFilterOn,
    ModalFilterOff,
    /// Drawn only on the pause book screen (not the side panel): resume play.
    PauseContinue,
    /// Pause book: step the right page (the photo album) to an earlier/
    /// later print slot.
    PauseNotePrev,
    PauseNoteNext,
    /// Pause book: open the editor for the left page's currently-shown
    /// text-note slot (plan revision — used to always append a new, blank
    /// free-text page; now edits whichever of the 15 slots is showing,
    /// pre-filled with its saved content).
    PauseWrite,
    /// Pause book, while writing: commit the draft to the notebook.
    PauseDraftSave,
    /// Pause book, while writing: discard the draft, back to the status view.
    PauseDraftCancel,
    /// Pause book: toggle whether the right page's shown print slot is
    /// protected from being overwritten by a future Printscreen capture.
    PauseNotePin,
    /// Pause book: open the editor for the shown print slot's caption
    /// (plan revision) — same text-editor UI as the text-note slots, a
    /// shorter limit and a different destination.
    PauseNoteName,
    /// Pause book: step the left page (the 15 text-note slots, plan
    /// revision) to an earlier/later one.
    PauseTextPrev,
    PauseTextNext,
    /// Pause book: toggle whether the left page's shown text-note slot is
    /// protected from a future overwrite *or delete* (plan revision).
    PauseTextPin,
    /// Pause book: clear the left page's shown text-note slot (plan
    /// revision) — dimmed/unclickable while it's pinned.
    PauseTextDelete,
}

/// A clickable spot on the shelf's own flat info panel (plan revision: the
/// shelf used to draw its details column *inside* the warped screen buffer
/// alongside the grid — "o painel nao pode estar dentro da TV" — so it now
/// sits flat next to the tube, like the in-game side panel, with its own
/// tiny button set). Separate from `PanelButton`: the shelf isn't a loaded
/// game, so none of that enum's meaning applies here.
/// As seções do corpo rolável do painel da estante (plan revision: "o cima
/// e baixo no painel faça assim os botões: capa traseira, cartucho e
/// informações") — cada botão de salto leva o corpo ao primeiro bloco da
/// seção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSection {
    CapaTraseira,
    Cartucho,
    Informacoes,
    /// A primeira página do manual (plan revision: "mostrar primeira
    /// pagina do manual igual mostra o back cover e cartucho / ao clicar
    /// abrir nele visualização").
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShelfButton {
    Back,
    Settings,
    /// Pula o corpo do painel para uma seção (plan revision: no lugar das
    /// antigas setas "^ Cima"/"v Baixo", três botões nomeados) — desenhados
    /// só enquanto a arte/info do jogo focused transborda o painel; o índice
    /// de destino o caller calcula (ele tem o [`ShelfPanelInfo`]).
    PanelJump(PanelSection),
    /// The focused game's back cover, drawn in the panel — clicking it opens
    /// the enlarged view (plan revision: "ao clicar no back cover
    /// possibilitar mostrar em tamanho maior, com botão de fechar").
    Backcover,
    /// O cartucho desenhado na aba dele (plan revision: "colocar zoom igual
    /// ao backcover no cartucho") — mesmo comportamento: clique abre a
    /// arte ampliada, "voltar"/Back fecha.
    Cartridge,
    /// Abrir o manual em PDF do jogo (plan revision: "criar botão no painel
    /// para abrir o manual") — só desenhado quando há PDF na pasta de
    /// manuais; o leitor vive no tubo, lado estante.
    Manual,
    /// Toggle the focused game's favorite marker (plan revision: "adicionar
    /// marcador de favorito nos jogos") — drawn by `draw_shelf_panel` when
    /// `ShelfPanelInfo::favorite` is `Some`.
    ToggleFavorite,
    /// Open the achievements list view (plan: `docs/plano-retroachievements
    /// .md`, fase 2/4) — drawn by `draw_shelf_panel` when
    /// `ShelfPanelInfo::achievements` is set.
    ShelfAchievements,
    /// "Atualizar" (plan revision: "adicionar opção de atualizar a estante
    /// para buscar jogos novos sem precisar abrir e fechar o app") —
    /// re-scan `roms/` and rebuild the shelf.
    Refresh,
}

/// A clickable spot on the settings screen's flat panel (plan revision: "a
/// tela de configurações não está no padrão do resto do app — faça a tv na
/// mesma proporção e coloque o painel") — the sections live in the panel,
/// like the shelf's own buttons do. `Section(i)` switches the settings list
/// shown inside the tube; `Back` leaves settings entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsButton {
    Section(usize),
    Back,
}

/// Qual seta de update do nameplate foi clicada (plan revision: "ao
/// clicar no icone verde de atualizar (tanto do app quanto o nucleo),
/// atualizar").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdateArrow {
    /// A seta da linha do próprio app — abre o changelog + botão atualizar.
    App,
    /// A seta da linha do SwanStation — baixa e instala o core na hora.
    Core,
}

/// The settings screen's flat panel content — one button per section plus
/// the always-present "Voltar". Drawn by `draw_settings_panel`, hit-tested
/// through `hit_settings_button`; settings.rs sets it every frame like the
/// shelf does with [`ShelfPanelInfo`].
pub struct SettingsPanelInfo {
    pub title: String,
    /// Section labels, top to bottom — `Section(i)` indexes into this.
    pub sections: Vec<String>,
    /// Which section is currently shown in the tube (button drawn lit).
    pub selected: usize,
}

/// The pause book: `captures` is the fixed slot count (plan revision, 15),
/// shared by both pages — the right page's photo album and the left page's
/// text notes (plan revision: the left page used to be a status line plus
/// an always-blank "write a new page" button over an unbounded, unreadable
/// append log; now it's a slot viewer just like the right page, since
/// "I wrote something and can't see it again" was a real complaint).
struct PauseNote {
    title: String,
    /// Total slots per page (plan revision: a fixed 1..=15 for both the
    /// photo album and the text notes) — the pagination bound both share.
    captures: usize,
    /// 0-based index of the print slot currently shown on the right page.
    page: usize,
    has_thumb: bool,
    /// Whether the shown print slot is protected from a future
    /// Printscreen overwrite (plan revision), and its caption if one was
    /// given — both empty/false for a slot nobody's touched yet.
    pinned: bool,
    slot_label: String,
    /// 0-based index of the text-note slot currently shown on the left
    /// page (plan revision) — independent of `page`, the right page's own
    /// cursor.
    text_page: usize,
    /// Whether the shown text-note slot is protected from a future
    /// overwrite *or delete* (plan revision) — same meaning `pinned` has
    /// for a print slot.
    text_pinned: bool,
    /// The shown text-note slot's saved content, or `None` for an empty
    /// one (plan revision) — this is the whole point of the left page now:
    /// showing back what was actually written there.
    text_content: Option<String>,
    /// `Some(text)` while the player is editing something (plan revision:
    /// either a text-note slot or a print's caption — `draft_heading`
    /// says which): replaces the left page's content with a live, editable
    /// draft. `draft_limit` is the max character count `text` may reach —
    /// shown as a counter alongside it.
    draft: Option<String>,
    draft_limit: usize,
    draft_heading: String,
}

/// One clickable row in a `ModalInfo`'s slot grid — `enabled` false dims it
/// the same way `draw_button` dims anything that wouldn't do anything right
/// now (e.g. "Carregar" on an empty slot, or a pinned print slot).
struct ModalRow {
    label: String,
    enabled: bool,
}

/// A modal dialog (plan revision) — save/load-state, "which slot for this
/// print", and the Cheats checklist: a title, a grid of rows, and a Cancel
/// button; lighter than the two-page pause book, since picking (or, for
/// Cheats, toggling) a row is the whole job here. Once a print's slot is
/// picked, `draft` replaces the row grid with a text field (name it) plus
/// Confirm/Cancel — same idea as the pause book's own draft mode
/// (`PauseNote::draft`), just a lighter home for it.
struct ModalInfo {
    title: String,
    rows: Vec<ModalRow>,
    /// Index of the first visible row-of-columns (plan revision — a
    /// database-heavy Cheats list can run to thousands of rows, so
    /// `draw_modal` only ever renders a page of them at a time). Preserved
    /// across a `set_modal` call that keeps the dialog open (Cheats
    /// refreshing its own checkmarks after a toggle) so a click deep in a
    /// long list doesn't jump the view back to the top; reset to 0 only on
    /// a fresh open (see `Cabinet::set_modal`) or a new search filter (see
    /// `Cabinet::set_modal_search`).
    scroll: usize,
    /// Whether this modal offers a search box at all (plan revision — only
    /// Cheats does; Save/Load/Print's lists are always small enough that
    /// hunting for one by eye is fine). Set once by whoever opens the
    /// modal, alongside `set_modal`.
    searchable: bool,
    /// Current filter text (plan revision) — empty means "show every row".
    /// A row matches when its label contains this, case-insensitively.
    /// Preserved across `set_modal` the same way `scroll` is.
    search_query: String,
    /// On/off state filter (plan revision, Cheats-only like `search_query`
    /// — drawn only when `searchable`): `None` shows every row, `Some(true)`
    /// only the ones checked on, `Some(false)` only the ones off. Derived
    /// per-row from the `[x] `/`[ ] ` prefix `cheat_modal_rows` already
    /// encodes into the label for display — see `row_checked` — rather
    /// than threading a second parallel array through `set_modal` just for
    /// this. Preserved across `set_modal` the same way `search_query` is.
    cheat_filter: Option<bool>,
    draft: Option<String>,
    draft_limit: usize,
    draft_heading: String,
}

/// What to draw at the top of the side panel: the `wheel` logo if we have
/// it, else the ROM's title, plus cartridge art right below when there's a
/// local file for it (plan revision — both are optional, independent of
/// each other). `commands` is the button legend (plan §3.2, item 3), one
/// clickable row per entry, rebuilt every frame by the caller since several
/// labels are live state (e.g. a "(feito!)" flash) — see
/// `Cabinet::set_commands`. `cheats` (item 4, plan §4.4) is
/// informational-only here now — `(description, on)` pairs, only the ones
/// that are on get drawn, plain text; toggling lives in its own Cheats modal
/// (plan revision), reachable from a `commands` row like any other.
struct PanelInfo {
    has_logo: bool,
    // No tempo do cartucho gateava o bloco; o console PSX desenha sempre.
    #[allow(dead_code)]
    has_cartridge: bool,
    title: String,
    commands: Vec<(PanelButton, String)>,
    /// Whether the console is on right now — dims/brightens the command
    /// buttons (Eject only clunks while powered, Reset and everything past
    /// it only act while powered), and which way the Power rocker sits. Set
    /// via `Cabinet::set_powered`; starts `false` (plan revision: picking a
    /// game only inserts the cartridge, it doesn't start it — the player
    /// presses Power themselves, same as the real console).
    powered: bool,
    /// Reset's rocker is momentary (plan revision): true for a short spring
    /// window right after a click, then the caller (`runner`) lets it lapse —
    /// see `Cabinet::set_reset_pressed`. Power's rocker has no such flag; its
    /// position is just `powered` itself (a real toggle, stays where left).
    reset_pressed: bool,
    cheats: Vec<(String, bool)>,
    /// Notebook block (item 5, plan §3.4): absent entirely when this is 0 —
    /// no "no notes" filler.
    note_count: usize,
    has_note_thumb: bool,
    /// A pinned text-note slot's content (plan revision — independent of
    /// the photo thumbnail above: either, both, or neither can be showing
    /// at once, "mostrar apenas uma nota e/ou uma imagem"), already
    /// snippet-length by the time it gets here (see `runner::
    /// panel_text_snippet`) — the panel itself doesn't truncate anything.
    text_note: Option<String>,
    /// Insert/eject progress for the panel's cartridge slot (plan revision)
    /// — `None` is the resting seated state every normal frame draws;
    /// `Some((t, ejecting))` while `runner`'s animation drives it, drawn by
    /// `draw_panel_slot`. Reset to `None` by `set_panel`.
    cartridge_motion: Option<(f32, bool)>,
}

/// What the shelf's flat side panel shows for the currently-selected game
/// (plan revision — moved out of the warped screen buffer so its text reads
/// crisp and its logo doesn't bow with the tube curvature, same as the
/// in-game panel already did). `logo_img`/`cartridge_img`/`backcover_img`
/// are texture ids the shelf already cached via `Cabinet::set_image` (its
/// own per-game local art, hashed from the ROM's sha1) — `None` falls back
/// to the title in text for the logo, and simply draws nothing for the
/// other two (plan revision: "abaixo da logo... colocar o back cover
/// tambem" — a second, independent local image under the cartridge art,
/// same `assets/<kind>/<rom>.*` convention, just its own folder,
/// `assets/backcover/`). `release` is the game's release year from the
/// No-Intro DAT, if one was loaded and had it (plan revision — replaces the
/// play count here; `info` is whatever else the DAT carried beyond the bare
/// title, plus the shelf's own file/play facts ("se o DAT tiver
/// informacoes do jogo, preencher no painel") — label/value pairs, empty
/// when the DAT has nothing extra or wasn't loaded at all.
#[derive(Clone)]
pub struct ShelfPanelInfo {
    pub title: String,
    pub logo_img: Option<u64>,
    pub cartridge_img: Option<u64>,
    pub backcover_img: Option<u64>,
    pub release: Option<String>,
    pub info: Vec<(String, String)>,
    /// A seção ativa do corpo (plan revision: "a ideia é mostrar um info
    /// de cada vez mesmo em resolução grande, mudar ao clicar") — o corpo
    /// vira abas: só os blocos da seção escolhida são desenhados, e os três
    /// botões do topo trocam a seção.
    pub section: PanelSection,
    /// How many of the scrollable blocks below the logo/title header
    /// (back cover, cartridge, release, `info`) to skip before drawing —
    /// the caller's own running counter (plan revision: "criar rolagem no
    /// painel quando necessario"), reset to 0 whenever the focused game
    /// changes. `draw_shelf_panel` clamps this itself, so an over-large
    /// value (scrolled past the end) is harmless.
    pub scroll: usize,
    /// The focused game's favorite marker (plan revision: "adicionar
    /// marcador de favorito nos jogos") — `Some(false)` draws a
    /// "Favoritar" button, `Some(true)` a "Remover favorito" one (both
    /// `ShelfButton::ToggleFavorite`), `None` no button at all (history /
    /// empty panel).
    pub favorite: Option<bool>,
    /// The focused game is identified on RetroAchievements (plan fase 2) —
    /// draws a "Conquistas" button (`ShelfButton::ShelfAchievements`)
    /// between "Favoritar" and "Configurações".
    pub achievements: bool,
    /// A primeira página do manual em PDF (plan revision: o corpo da aba
    /// manual a mostra como as artes, e o clique nela abre o leitor).
    /// `None` = sem manual para o jogo.
    pub manual_first_page: Option<u64>,
    /// O motivo pelo qual o botão Conquistas aparece **desativado** (plan
    /// revision: "avisar o usuário com tooltip no botão conquistas,
    /// mostrar mas deixar desativado") — o RA marcou o hash como versão
    /// não suportada, então o botão desenha apagado com o motivo no
    /// rótulo e o clique nele não faz nada. `None` = destravado.
    pub achievements_reason: Option<String>,
    /// A medalha de prêmio do jogo (plan fase 4) — textura já registrada
    /// pelo caller (`set_image`), desenhada à esquerda do número da linha
    /// "conquistas".
    pub award_img: Option<u64>,
}

/// Medalha + respiro antes do número — compartilhados pela medida da
/// altura e pelo desenho de `PanelBlock::RaField`.
const AWARD_MEDAL_W: i32 = 12;
const AWARD_MEDAL_H: i32 = 15;
const AWARD_MEDAL_GAP: i32 = 5;

struct SrcTexture {
    tex: Texture,
    w: u32,
    h: u32,
    format: PixelFormat,
}

/// A plain RGBA texture kept at a known size.
struct SizedTex {
    tex: Texture,
    w: u32,
    h: u32,
}

struct CrtMesh {
    verts: Vec<Vertex>,
    indices: Vec<i32>,
    w: u32,
    h: u32,
}

/// The chamfered ring from the window edge to the recessed screen.
struct BezelMesh {
    verts: Vec<Vertex>,
    indices: Vec<i32>,
    /// Cache key: (window w, window h, screen w, screen h).
    key: (u32, u32, u32, u32),
}

struct ImgTex {
    tex: Texture,
    w: u32,
    h: u32,
    /// The opaque content's bounding box inside the texture, in texture
    /// pixels — the art's transparent margins excluded (`content_bbox`).
    /// The cartridge slot scene fits and seats the cartridge by this box, so
    /// the cart's actual body — not its empty canvas — spans the base and
    /// meets the slot mouth.
    content: (u32, u32, u32, u32),
}

/// The bounding box of pixels with any alpha, in texture pixels — an
/// image-sized box when the art has no transparency at all. Scans at most
/// one pass over the RGBA buffer (art textures are loaded once).
fn content_bbox(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, u32, u32) {
    let (mut min_x, mut min_y) = (w, h);
    let (mut max_x, mut max_y) = (0u32, 0u32);
    let mut seen = false;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                seen = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if !seen {
        return (0, 0, w, h);
    }
    (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

impl Cabinet {
    pub(crate) fn new(
        video: &VideoSubsystem,
        audio: &AudioSubsystem,
        title: &str,
        width: u32,
        height: u32,
        fullscreen: bool,
    ) -> Result<Self, PlatformError> {
        // Fullscreen from birth, when requested (plan revision: "tem como
        // fazer abrir direto na forma correta?") — e nascendo **oculta**: o
        // macOS ainda mostrava um frame do tamanho bruto 1280×800 antes de
        // aplicar o fullscreen (com a transição nativa de zoom por cima), o
        // que lia como "abre de um tamanho e dá uma aumentada". Oculta +
        // fullscreen desde a criação, a janela só aparece quando já está no
        // formato final.
        let mut builder = video.window(title, width, height);
        builder.position_centered().resizable();
        if fullscreen {
            builder.fullscreen().hidden();
        }
        let window = builder
            .build()
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let mut canvas = window.into_canvas();
        log::info!(
            "video: {} | renderer: {}",
            video.current_video_driver(),
            canvas.renderer_name
        );
        canvas.set_blend_mode(BlendMode::Blend);
        canvas.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        canvas.clear();
        canvas.present();
        if fullscreen {
            // Já preparada (fullscreen aplicado, primeiro frame pronto),
            // aparece sem tamanho intermediário.
            canvas.window_mut().show();
        }

        let font = build_font_atlas(&mut canvas)?;
        let (w, h) = canvas.output_size().unwrap_or((width, height));
        let canvas_rect = cabinet_canvas_rect(w, h);
        Ok(Self {
            canvas,
            src: None,
            mesh: None,
            bezel: None,
            screen_tex: None,
            noise_tex: None,
            noise: Vec::new(),
            noise_stamp: None,
            rng: 0x9E37_79B9,
            spin: 0.0,
            font,
            images: HashMap::new(),
            screen: screen_area(canvas_rect.width(), canvas_rect.height()),
            panel: None,
            session: Duration::ZERO,
            pause: None,
            modal: None,
            fullscreen: false,
            panel_buttons: Vec::new(),
            pause_buttons: Vec::new(),
            modal_buttons: Vec::new(),
            shelf_panel: None,
            shelf_buttons: Vec::new(),
            settings_panel: None,
            settings_buttons: Vec::new(),
            audio: audio.clone(),
            idle_core_prompt: None,
            dev_mode: false,
            core_status: None,
            static_hiss: false,
            hiss: None,
            hiss_rng: 0x2545_f491,
            foley: None,
            ch3_until: None,
            canvas_rect,
            nameplate: BRAND.to_string(),
            nameplate_updates: (false, false),
            update_arrows: (None, None),
            osd_queue: VecDeque::new(),
            ra_status: None,
            close_button: Rect::new(0, 0, 0, 0),
            minimize_button: Rect::new(0, 0, 0, 0),
        })
    }

    /// Replace the cabinet's nameplate text (plan revision) — the app calls
    /// this once at startup with its own version, and again whenever the
    /// installed SwanStation core changes (a download/update via settings).
    pub fn set_nameplate(&mut self, text: &str) {
        self.nameplate = text.to_string();
    }

    /// Which nameplate lines get the green "tem update" dot — `(app, core)`,
    /// matching the block `set_nameplate` stacked (`draw_brand` draws the dot
    /// right after line 0 / line 1, respectively). `false, false` clears both.
    pub fn set_nameplate_updates(&mut self, app: bool, core: bool) {
        self.nameplate_updates = (app, core);
    }

    /// Qual seta verde de update do nameplate um ponto (output) acerta —
    /// rects preenchidos pelo caminho estático (`present_static`), onde o
    /// nameplate é clicável (idle). (plan revision: "ao clicar no icone
    /// verde de atualizar".)
    pub fn hit_update_arrow(&self, ox: i32, oy: i32) -> Option<UpdateArrow> {
        let (app, core) = self.update_arrows;
        if app.is_some_and(|r| r.contains_point((ox, oy))) {
            Some(UpdateArrow::App)
        } else if core.is_some_and(|r| r.contains_point((ox, oy))) {
            Some(UpdateArrow::Core)
        } else {
            None
        }
    }

    /// MOCK (design preview for the achievements notification — see
    /// `demo_osd`): shows a line block in the chin's right side; an empty
    /// slice clears it.
    /// Arm/disarm the persistent RetroAchievements chin badge — the runner
    /// calls it with the live session's mode on cart insert and with `None`
    /// when the cartridge leaves (or the account is off). No visual effect
    /// while the OSD queue has a live notification: that draws first.
    pub fn set_ra_status(&mut self, status: Option<RaStatus>) {
        self.ra_status = status;
    }

    pub fn push_osd(&mut self, lines: &[&str], badge: Option<u64>, ttl: Duration) {
        if lines.is_empty() {
            return;
        }
        self.osd_queue.push_back(OsdEntry {
            lines: lines.iter().map(|l| l.to_string()).collect(),
            badge,
            until: Instant::now() + ttl,
        });
    }

    /// The idle panel's core prompt (plan revision: "avisar que para jogar é
    /// necessário o download do core") — `Some(button_label)` shows the
    /// warning line plus a "CoreDownload" button; `None` hides both. Only
    /// ever rendered on the idle (panel-less) screen.
    pub fn set_idle_core_prompt(&mut self, label: Option<&str>) {
        self.idle_core_prompt = label.map(|l| l.to_string());
    }

    /// O modo dev (plan revision: "konami code na tela inicial do app") —
    /// ligado só pelo app, ao reconhecer o código; enquanto ligado, o painel
    /// da tela inicial desenha o botão "Dev" acima do "Configurações".
    /// Nunca é persistido nem lido de arquivo: o segredo vive só na sessão.
    pub fn set_dev_mode(&mut self, on: bool) {
        self.dev_mode = on;
    }

    /// O estado atual do modo dev — o app consulta para saber se a sessão
    /// já o desbloqueou (o botão em si o Cabinet desenha sozinho).
    pub fn dev_mode(&self) -> bool {
        self.dev_mode
    }

    /// A linha de status do download do núcleo no painel do idle ("baixando…
    /// X MB" / "núcleo atualizado!" / motivo da falha) — `None` não desenha
    /// nada. O app atualiza a cada frame enquanto o download vive.
    pub fn set_core_status(&mut self, status: Option<&str>) {
        self.core_status = status.map(|s| s.to_string());
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        let _ = self.canvas.window_mut().set_fullscreen(self.fullscreen);
    }

    /// The underlying window — `Platform::start_text_input`/`stop_text_input`
    /// need it (SDL's text-input API is per-window).
    pub(crate) fn window(&self) -> &sdl3::video::Window {
        self.canvas.window()
    }

    /// Convert a click's window coordinates (what SDL reports) into the
    /// cabinet's own local space — what `panel_rect`/`draw_panel`/`screen`
    /// all lay out in. Two corrections stack here: window → real output
    /// pixels (they differ on a HiDPI display), then output → cabinet-local
    /// (subtracting `canvas_rect`'s offset — on an ultrawide window the
    /// cabinet is letterboxed, not the whole output). A point outside
    /// `canvas_rect` (in the letterbox bars) still comes back as a
    /// coordinate — callers bounds-check against their own rects, e.g.
    /// `hit_panel_button`/`hit_screen_point`.
    pub fn window_to_output(&self, x: i32, y: i32) -> (i32, i32) {
        let (ww, wh) = self.canvas.window().size();
        let (ow, oh) = self.canvas.output_size().unwrap_or((ww, wh));
        let sx = ow as f32 / ww.max(1) as f32;
        let sy = oh as f32 / wh.max(1) as f32;
        let (ox, oy) = ((x as f32 * sx) as i32, (y as f32 * sy) as i32);
        (ox - self.canvas_rect.x(), oy - self.canvas_rect.y())
    }

    /// Like [`Cabinet::window_to_output`], but in the 2D screen buffer's own
    /// coordinate space (what `frame_2d`'s `draw` closure paints in — the
    /// screen is inset by the bezel inside the cabinet canvas). A screen
    /// drawn with hand-laid buttons hit-tests clicks by pairing this with
    /// its `Screen::size` layout.
    pub fn window_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
        let (ox, oy) = self.window_to_output(x, y);
        (ox - self.screen.x(), oy - self.screen.y())
    }

    /// Show/hide the top-left "fechar app" button (plan revision) — call
    /// once on entering a screen that should offer it (idle/shelf/settings/
    /// history), and with `false` on entering one that shouldn't
    /// (gameplay) — the flag has no default that fits every screen, so it's
    /// never implicitly reset between them.
    pub fn set_close_button(&mut self, _show: bool) {
        // The fechar/minimizar pair is drawn on every screen now (plan
        // revision); kept as a no-op so existing callers keep compiling.
    }

    /// Whether an output-space point lands on the close button drawn last
    /// frame — always `false` while `set_close_button(false)` is in effect,
    /// even if a stale rect from an earlier screen is still sitting in
    /// `close_button`. Coordinates from a click go through
    /// `window_to_output` first.
    pub fn hit_close_button(&self, out_x: i32, out_y: i32) -> bool {
        self.close_button.contains_point((out_x, out_y))
    }

    /// Whether a click (already mapped by `window_to_output`) landed on the
    /// top-left minimize button. Screens call `minimize()` when this hits.
    pub fn hit_minimize_button(&self, out_x: i32, out_y: i32) -> bool {
        self.minimize_button.contains_point((out_x, out_y))
    }

    /// Iconify the window (plan revision: "botão de minimizar ao lado de
    /// fechar") — the app keeps running; restoring is the user's click on
    /// the Dock/taskbar, like any OS window.
    pub fn minimize(&mut self) {
        let _ = self.canvas.window_mut().minimize();
    }

    /// Which panel button, if any, sits under an output-space point — the
    /// idle screen's "Inserir cartucho" or one of the in-game commands drawn
    /// last frame. Coordinates from a click go through `window_to_output`
    /// first.
    pub fn hit_panel_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.panel_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Same as `hit_panel_button`, for the pause book's own buttons — a
    /// separate list since the pause screen replaces the whole window
    /// instead of sharing it with the side panel (see `pause_buttons`).
    pub fn hit_pause_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.pause_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Same as `hit_panel_button`, for the shelf's own flat panel (plan
    /// revision) — a separate list since the shelf isn't a loaded game.
    pub fn hit_shelf_button(&self, out_x: i32, out_y: i32) -> Option<ShelfButton> {
        self.shelf_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Show the shelf's flat side panel content for whatever game is
    /// selected right now (plan revision) — rebuilt every frame the
    /// selection might have changed, same as the shelf already rebuilds its
    /// own grid draw closure; the struct is small enough that this is cheap.
    pub fn set_shelf_panel(&mut self, info: ShelfPanelInfo) {
        self.shelf_panel = Some(info);
    }

    /// Settings screen's flat panel content — see [`SettingsPanelInfo`]
    /// (plan revision: settings now lives on the shelf's layout, tube +
    /// panel, and the panel carries the section buttons).
    pub fn set_settings_panel(&mut self, info: SettingsPanelInfo) {
        self.settings_panel = Some(info);
    }
    pub fn hit_settings_button(&self, out_x: i32, out_y: i32) -> Option<SettingsButton> {
        self.settings_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Map an output/canvas-space click into the 2D screen buffer's local
    /// coordinates — what `Screen::fill`/`text`/`image_fit` see, e.g. in the
    /// shelf's grid (plan §3.1). `None` outside the tube. Approximate: the
    /// buffer is warped onto the tube by `build_crt_mesh` (a mild barrel bow,
    /// `CRT_WARP`), which this ignores — a click right at the curved edge can
    /// land a few px off, but shelf tiles sit well inside it.
    pub fn hit_screen_point(&self, out_x: i32, out_y: i32) -> Option<(i32, i32)> {
        let (lx, ly) = (out_x - self.screen.x(), out_y - self.screen.y());
        if lx < 0 || ly < 0 || lx >= self.screen.width() as i32 || ly >= self.screen.height() as i32
        {
            return None;
        }
        let (sw, sh) = (self.screen.width() as f32, self.screen.height() as f32);
        // Displaced grid coords the click arrived at (-1..1 per axis).
        let dx = (lx as f32 / sw) * 2.0 - 1.0;
        let dy = (ly as f32 / sh) * 2.0 - 1.0;
        // Recover the undisplaced ones: forward is cx' = cx*(1-W*cy^2),
        // cy' = cy*(1-W*cx^2) — divide out the partner axis' factor until it
        // stops moving (sub-pixel after three rounds; eight for free).
        let (mut cx, mut cy) = (dx, dy);
        for _ in 0..8 {
            cy = dy / (1.0 - CRT_WARP * cx * cx);
            cx = dx / (1.0 - CRT_WARP * cy * cy);
        }
        if !(-1.0..=1.0).contains(&cx) || !(-1.0..=1.0).contains(&cy) {
            // In the recess between the warped picture's corner and the
            // bezel — nothing drawn there to click.
            return None;
        }
        Some((
            (((cx * 0.5 + 0.5) * sw) as i32).clamp(0, sw as i32 - 1),
            (((cy * 0.5 + 0.5) * sh) as i32).clamp(0, sh as i32 - 1),
        ))
    }

    /// Show the side panel during play: `logo` (width, height, RGBA) is the
    /// local art if there is one, else the panel falls back to `title` in
    /// text (plan §3.2, item 1); `cartridge` is a second, independent local
    /// image drawn right below it when present (plan revision — neither
    /// needs the other). `commands` is the initial button legend (item 3,
    /// plan revision: mouse-only) — see `set_commands` for the per-frame
    /// updates that follow (labels like a "(feito!)" flash change live).
    /// Call once per game.
    pub fn set_panel(
        &mut self,
        logo: Option<(u32, u32, &[u8])>,
        cartridge: Option<(u32, u32, &[u8])>,
        title: &str,
        commands: &[(PanelButton, String)],
    ) {
        let has_logo = if let Some((w, h, rgba)) = logo {
            self.set_image(PANEL_LOGO_IMG, w, h, rgba);
            true
        } else {
            false
        };
        let has_cartridge = if let Some((w, h, rgba)) = cartridge {
            self.set_image(PANEL_CARTRIDGE_IMG, w, h, rgba);
            true
        } else {
            false
        };
        self.panel = Some(PanelInfo {
            has_logo,
            has_cartridge,
            title: title.to_string(),
            commands: commands.to_vec(),
            // A freshly inserted cartridge (plan revision): the console
            // doesn't boot itself any more — see `run_game`'s initial
            // `powered = false` — so the rocker starts down, not up.
            powered: false,
            reset_pressed: false,
            cheats: Vec::new(),
            note_count: 0,
            has_note_thumb: false,
            text_note: None,
            // Seated in the slot until `runner`'s insert animation says
            // otherwise (it runs right after this).
            cartridge_motion: None,
        });
    }

    /// Refresh the command legend's labels (plan revision: a "(feito!)"
    /// flash on a silent action is live state, so the caller rebuilds and
    /// passes this every frame instead of once). A no-op before `set_panel`.
    pub fn set_commands(&mut self, commands: &[(PanelButton, String)]) {
        if let Some(panel) = &mut self.panel {
            panel.commands = commands.to_vec();
        }
    }

    /// Dim/brighten the command buttons to match the console's power state
    /// (Eject only clunks while powered, Reset only acts while powered) — see
    /// `PanelInfo::powered`. A no-op before `set_panel`.
    pub fn set_powered(&mut self, powered: bool) {
        if let Some(panel) = &mut self.panel {
            panel.powered = powered;
        }
    }

    /// Reset's rocker springs up for a short moment after a click, then back
    /// down on its own (plan revision) — the caller (`runner`) recomputes
    /// this every frame from its own click timestamp, same pattern as the
    /// "(feito!)" flash on the other buttons.
    pub fn set_reset_pressed(&mut self, pressed: bool) {
        if let Some(panel) = &mut self.panel {
            panel.reset_pressed = pressed;
        }
    }

    /// Drop the last game's panel (logo, commands, cheats, notes, clock) —
    /// call on the way back to the idle/root screen, so `draw_panel` shows
    /// the "Inserir cartucho" button instead of the previous game's stale
    /// info. A no-op if there's nothing set.
    pub fn clear_panel(&mut self) {
        self.panel = None;
    }

    /// The idle screen's console-brand logo (plan revision: the idle/root
    /// screen — startup, backing out of the shelf, or ejecting a game, all
    /// the same screen — shows this where a game's own logo would go).
    /// `assets/console.png`, decoded once by the caller (`idle::run`); a
    /// no-op with `None` (the file doesn't exist) — `draw_panel`'s idle
    /// branch falls back to plain text the same way a missing per-game logo
    /// does.
    pub fn set_console_logo(&mut self, logo: Option<(u32, u32, &[u8])>) {
        if let Some((w, h, rgba)) = logo {
            self.set_image(PANEL_CONSOLE_LOGO_IMG, w, h, rgba);
        }
    }

    /// The console tag wordmark printed on the slot's loading base (plan
    /// revision: "imagem console-tag... na base do cartucho, alinhado a
    /// esquerda") — `assets/console-tag.png`, loaded once by the app and
    /// drawn by `draw_slot_furniture` on every screen that shows the slot.
    /// `None` removes a previously loaded tag (file gone), leaving the bare
    /// groove line.
    pub fn set_slot_tag(&mut self, tag: Option<(u32, u32, &[u8])>) {
        match tag {
            Some((w, h, rgba)) => self.set_image(SLOT_TAG_IMG, w, h, rgba),
            None => {
                self.images.remove(&SLOT_TAG_IMG);
            }
        }
    }

    /// Update the panel's notebook block (plan §3.4, item 5): `count` of the
    /// 15 print slots that are pinned (plan revision — used to be however
    /// many were filled, shown regardless of pin; now the panel only
    /// features what the player deliberately pinned), `thumb` a pinned
    /// slot's image (width, height, RGBA) if there's one to show, `text` a
    /// pinned text-note slot's content if there's one of those to show —
    /// independent of the photo side, so either, both, or neither can be
    /// present at once. Call once at game start and again after every
    /// capture, write, or pin toggle (either kind). A no-op before
    /// `set_panel`.
    pub fn set_notes(
        &mut self,
        count: usize,
        thumb: Option<(u32, u32, &[u8])>,
        text: Option<&str>,
    ) {
        let has_thumb = if let Some((w, h, rgba)) = thumb {
            self.set_image(PANEL_NOTE_IMG, w, h, rgba);
            true
        } else {
            false
        };
        if let Some(panel) = &mut self.panel {
            panel.note_count = count;
            panel.has_note_thumb = has_thumb;
            panel.text_note = text.map(str::to_string);
        }
    }

    /// Update the side panel's cheat list (plan §4.4): `cheats` is
    /// `(description, on)` pairs in the curated order, shown informationally
    /// here (only the ones that are on, plain text) — toggling lives in the
    /// Cheats modal now (plan revision: its own menu, not a page inside the
    /// notebook). Call once at game start and again on every toggle — the
    /// list is always tiny. A no-op before `set_panel`.
    pub fn set_cheats(&mut self, cheats: &[(String, bool)]) {
        if let Some(panel) = &mut self.panel {
            panel.cheats = cheats.to_vec();
        }
    }

    /// Update the session clock shown at the bottom of the panel. Call once a
    /// frame; it just stores the value for the next present/capture.
    pub fn set_session_time(&mut self, elapsed: Duration) {
        self.session = elapsed;
    }

    /// Load the pause book's content (plan §3.2/§3.4): call once when pause
    /// opens, not every frame — `present_pause` just redraws what's already
    /// set. `captures` is the fixed slot count shared by both pages (plan
    /// revision: always 15). Neither page has anything to show yet — follow
    /// with `set_pause_page` and `set_pause_text_page`.
    pub fn set_pause_note(&mut self, title: &str, captures: usize) {
        self.pause = Some(PauseNote {
            title: title.to_string(),
            captures,
            page: captures.saturating_sub(1),
            has_thumb: false,
            pinned: false,
            slot_label: String::new(),
            text_page: captures.saturating_sub(1),
            text_pinned: false,
            text_content: None,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Show a different capture on the right page (pagination, plan
    /// revision) — call on entering pause (page = the last one) and again on
    /// every Prev/Next/pin/rename change. `pinned`/`label` are that slot's
    /// current protection state and caption. A no-op before `set_pause_note`.
    pub fn set_pause_page(
        &mut self,
        page: usize,
        pinned: bool,
        label: &str,
        thumb: Option<(u32, u32, &[u8])>,
    ) {
        let has_thumb = if let Some((w, h, rgba)) = thumb {
            self.set_image(PAUSE_THUMB_IMG, w, h, rgba);
            true
        } else {
            false
        };
        if let Some(p) = &mut self.pause {
            p.page = page;
            p.has_thumb = has_thumb;
            p.pinned = pinned;
            p.slot_label = label.to_string();
        }
    }

    /// Show a different text-note slot on the left page (plan revision —
    /// mirrors `set_pause_page`, the right page's own pagination): call on
    /// entering pause and again on every Prev/Next/pin/write/delete change.
    /// `content` is `None` for an empty slot. A no-op before
    /// `set_pause_note`.
    pub fn set_pause_text_page(&mut self, page: usize, pinned: bool, content: Option<&str>) {
        if let Some(p) = &mut self.pause {
            p.text_page = page;
            p.text_pinned = pinned;
            p.text_content = content.map(str::to_string);
        }
    }

    /// Enter/update/leave an editor on the left page (plan revision: either
    /// the free-text note or a slot's caption — `heading` names which,
    /// shown above the live text): `Some(text)` shows `text` in place of the
    /// status view, with a `.../limit` counter; `None` leaves editing (any
    /// `heading` passed alongside `None` is ignored). Call on every
    /// keystroke while editing — cheap, at most a few hundred characters.
    /// A no-op before `set_pause_note`.
    pub fn set_pause_draft(&mut self, draft: Option<&str>, limit: usize, heading: &str) {
        if let Some(p) = &mut self.pause {
            p.draft = draft.map(str::to_string);
            p.draft_limit = limit;
            p.draft_heading = heading.to_string();
        }
    }

    /// Open (or replace) a save/load-state, print slot, or Cheats picker
    /// (plan revision): `title` names the dialog, `rows` is one `(label,
    /// enabled)` pair per slot in order — a disabled row (e.g. "Carregar"
    /// on an empty slot, or a pinned print slot) still shows, just dimmed
    /// and unclickable. Scroll position, `searchable`, and any active
    /// search filter all carry over from whatever modal was already open
    /// (Cheats calls this again after every toggle, to redraw its
    /// checkmarks, without wanting the view to jump back to the top or
    /// forget the filter) — only a genuinely fresh open (no modal was
    /// showing) starts scrolled to the top with no filter and not
    /// searchable; call `set_modal_searchable` right after opening one that
    /// should be.
    pub fn set_modal(&mut self, title: &str, rows: &[(String, bool)]) {
        let scroll = self.modal.as_ref().map_or(0, |m| m.scroll);
        let searchable = self.modal.as_ref().is_some_and(|m| m.searchable);
        let search_query = self
            .modal
            .as_ref()
            .map_or_else(String::new, |m| m.search_query.clone());
        let cheat_filter = self.modal.as_ref().and_then(|m| m.cheat_filter);
        self.modal = Some(ModalInfo {
            title: title.to_string(),
            rows: rows
                .iter()
                .map(|(label, enabled)| ModalRow {
                    label: label.clone(),
                    enabled: *enabled,
                })
                .collect(),
            scroll,
            searchable,
            search_query,
            cheat_filter,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Set the Cheats modal's on/off state filter (plan revision): `None`
    /// shows every row, `Some(true)`/`Some(false)` only the ones checked
    /// on/off. Resets `scroll` to 0, same reasoning as `set_modal_search`.
    /// A no-op before `set_modal`.
    pub fn set_modal_filter(&mut self, filter: Option<bool>) {
        if let Some(m) = &mut self.modal {
            m.cheat_filter = filter;
            m.scroll = 0;
        }
    }

    /// Mark whether the open modal offers a search box (plan revision —
    /// today only the Cheats modal does). Call once, right after opening
    /// it — `set_modal`'s own refreshes (Cheats redrawing its checkmarks
    /// after each toggle) carry this forward on their own.
    pub fn set_modal_searchable(&mut self, searchable: bool) {
        if let Some(m) = &mut self.modal {
            m.searchable = searchable;
        }
    }

    /// Set the modal's search filter (plan revision): empty clears it. A
    /// new filter resets `scroll` to 0 — whatever page was showing under
    /// the old filter is unlikely to mean anything under the new one. A
    /// no-op before `set_modal`.
    pub fn set_modal_search(&mut self, query: &str) {
        if let Some(m) = &mut self.modal {
            m.search_query = query.to_string();
            m.scroll = 0;
        }
    }

    /// The modal's current search filter, e.g. to pre-fill the search
    /// box's text field when the player reopens it to refine a query.
    /// Empty (not `None`) before `set_modal` — same "nothing to filter" as
    /// an explicitly cleared search.
    pub fn modal_search_query(&self) -> &str {
        self.modal.as_ref().map_or("", |m| m.search_query.as_str())
    }

    /// Scroll the modal's row grid by `delta` visual rows (negative = up) —
    /// a no-op before `set_modal`. `draw_modal` clamps the visible result
    /// against however many rows actually overflow the card, so over-
    /// scrolling just stops at the last page rather than needing a bound
    /// here too.
    pub fn scroll_modal(&mut self, delta: i32) {
        if let Some(m) = &mut self.modal {
            m.scroll = (m.scroll as i32 + delta).max(0) as usize;
        }
    }

    /// Enter/update/leave the modal's naming step (plan revision — the print
    /// picker's second step, same idea as `set_pause_draft`): `Some(text)`
    /// replaces the row grid with `text` and a `.../limit` counter. A no-op
    /// before `set_modal`.
    pub fn set_modal_draft(&mut self, draft: Option<&str>, limit: usize, heading: &str) {
        if let Some(m) = &mut self.modal {
            m.draft = draft.map(str::to_string);
            m.draft_limit = limit;
            m.draft_heading = heading.to_string();
        }
    }

    /// Close whichever modal is open — call once the player picks a slot (or
    /// finishes naming a print) and the action's done, or on Cancel.
    pub fn clear_modal(&mut self) {
        self.modal = None;
    }

    /// Same as `hit_panel_button`, for the modal dialog's own buttons — a
    /// separate list since the modal replaces the whole window instead of
    /// sharing it with the side panel (see `modal_buttons`).
    pub fn hit_modal_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.modal_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Present of every frame, through the trace watchdog (`PSX_XPERIENCE_TRACE=
    /// 1`): present is where a GPU/compositor/display stall shows up — telling
    /// it apart from a slow app loop is the whole point of the trace.
    fn present_and_time(&mut self) {
        let _span = super::TraceSpan::new("present");
        self.canvas.present();
    }

    /// Draw the modal dialog to the window, replacing the whole window like
    /// the pause book does (plan revision).
    pub fn present_modal(&mut self) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = rect;
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(rect));
        self.modal_buttons = draw_modal(
            &mut self.canvas,
            &mut self.font,
            self.modal.as_ref(),
            rect.width(),
            rect.height(),
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_modal`] but composited into an offscreen
    /// target and saved as a BMP (headless preview).
    pub fn capture_modal_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let modal = self.modal.as_ref();
        let font = &mut self.font;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(rect));
            let _ = draw_modal(c, font, modal, rect.width(), rect.height());
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Draw the pause book to the window: two pages, not warped by the tube,
    /// replacing the whole window rather than sharing it with the cabinet
    /// (plan §3.2/§3.4), plus its own clickable "Continuar" button — mouse/
    /// gamepad only, no keyboard (plan revision: the frame-step button that
    /// used to sit next to it is gone, a dev-only leftover nobody used
    /// through the actual UI).
    pub fn present_pause(&mut self) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = rect;
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(rect));
        self.pause_buttons = draw_pause_book(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.pause.as_ref(),
            rect.width(),
            rect.height(),
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_pause`] but composited into an offscreen
    /// target and saved as a BMP (headless preview).
    pub fn capture_pause_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let pause = self.pause.as_ref();
        let font = &mut self.font;
        let images = &self.images;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(rect));
            let _ = draw_pause_book(c, font, images, pause, rect.width(), rect.height());
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Whether image `id` is already decoded and cached — e.g. to decide
    /// whether any cover art exists at all before choosing how to lay out
    /// the shelf (grid vs. list, plan §3.1). Same lookup `Screen::has_image`
    /// does from inside a `frame_2d` closure, exposed here for callers that
    /// need the answer *before* they can build that closure.
    pub fn has_image(&self, id: u64) -> bool {
        self.images.contains_key(&id)
    }

    /// Size of the recessed screen area — what the selector lays itself out in.
    pub fn screen_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let s = screen_area(rect.width(), rect.height());
        (s.width(), s.height())
    }

    /// Like [`Cabinet::screen_size`], but for the shelf specifically (plan
    /// revision): the tube's own area minus the flat side panel, since the
    /// shelf's grid and the game's video now share the exact same split
    /// (`panel_rect` carved out of `cabinet_canvas_rect` first) instead of
    /// the panel living inside the warped buffer.
    pub fn shelf_screen_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let (out_w, out_h) = (rect.width(), rect.height());
        let cab_w = out_w.saturating_sub(panel_rect(out_w, out_h).width());
        let s = screen_area(cab_w, out_h);
        (s.width(), s.height())
    }

    // --- game path -------------------------------------------------------

    /// Draw one frame to the window. `aspect_ratio <= 0` means "use 4:3".
    pub fn present_frame(&mut self, frame: &FrameRef, aspect_ratio: f32) {
        // O disco gira enquanto o console está ligado com disco assentado.
        if self
            .panel
            .as_ref()
            .is_some_and(|p| p.has_cartridge && p.powered)
        {
            self.spin = (self.spin + 7.0) % 360.0; // ~1 volta por segundo
        }
        self.ensure_src(frame.width, frame.height, frame.format);
        self.upload(frame);

        let (real_w, real_h) = self
            .canvas
            .output_size()
            .unwrap_or((frame.width, frame.height));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (out_w, out_h) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(out_w, out_h);
        let cab_w = out_w.saturating_sub(panel.width());
        let screen = screen_area(cab_w, out_h);
        self.screen = screen;
        let dst = fit_aspect_in(screen, resolve_aspect(aspect_ratio));
        self.ensure_mesh(dst);
        self.ensure_bezel(cab_w, out_h, dst);

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let src = self.src.take().unwrap();
        let mesh = self.mesh.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&src.tex), &mesh.indices[..]);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.src = Some(src);
        self.mesh = Some(mesh);
        self.bezel = Some(bezel);

        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            out_h,
            &self.nameplate,
            self.nameplate_updates,
        );
        self.draw_osd_front();
        // The timed "CH 3" flash (plan revision): power-on shows the channel
        // banner over the picture for a few seconds, then it's gone. Expired
        // deadlines clear here so the banner truly disappears from the frame
        // instead of relying on the next flash.
        if self.ch3_until.is_some_and(|until| Instant::now() >= until) {
            self.ch3_until = None;
        }
        if self.ch3_until.is_some() {
            draw_ch3_osd(&mut self.canvas, &mut self.font, self.screen);
        }
        // The picture is back, so any queued hiss goes — cut, not faded
        // (same rule the power bursts use for their own buzz).
        if let Some(h) = &self.hiss {
            h.clear();
        }
        // Fechar/minimizar on every screen, gameplay included (plan
        // revision: "mostrar o fechar e minimizar em todas as telas").
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            self.spin,
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
        );
        self.canvas.set_viewport(None);
        // DEBUG-CI: dump do composto (o que a Metal desenhou de fato).
        if let Ok(path) = std::env::var("PSX_XPERIENCE_DEBUG_COMPOSITE") {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n == 30 || n == 300 || n == 700 || n == 1500 {
                let named = format!("{path}.{n}.bmp");
                let _ = self
                    .canvas
                    .read_pixels(None::<Rect>)
                    .and_then(|s| s.save_bmp(std::path::Path::new(&named)));
                log::info!("debug composite: {} -> {named}", n);
            }
        }
        self.present_and_time();
    }

    /// Render one frame into an offscreen target and save it as a BMP. Works
    /// headless (a background window never composites on macOS), so the real
    /// output — cabinet, CRT warp and all — can be eyeballed without a window.
    pub fn capture_bmp(
        &mut self,
        frame: &FrameRef,
        aspect_ratio: f32,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.ensure_src(frame.width, frame.height, frame.format);
        self.upload(frame);

        let (real_w, real_h) = self
            .canvas
            .output_size()
            .unwrap_or((frame.width, frame.height));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (out_w, out_h) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(out_w, out_h);
        let cab_w = out_w.saturating_sub(panel.width());
        let screen = screen_area(cab_w, out_h);
        self.screen = screen;
        let dst = fit_aspect_in(screen, resolve_aspect(aspect_ratio));
        self.ensure_mesh(dst);
        self.ensure_bezel(cab_w, out_h, dst);

        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;

        let src = self.src.take().unwrap();
        let mesh = self.mesh.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let osd = self.osd_for_capture();
        let font = &mut self.font;
        let images = &self.images;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&src.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            let _ = draw_brand(c, font, screen, out_h, nameplate, nameplate_updates);
            match &osd {
                ChinOsd::Notify { lines, badge } => {
                    draw_chin_osd(c, font, images, screen, out_h, nameplate, lines, *badge);
                }
                ChinOsd::Ra { hardcore } => {
                    draw_chin_ra(c, font, images, screen, out_h, nameplate, *hardcore);
                }
                ChinOsd::None => {}
            }
            draw_panel(
                c,
                font,
                images,
                panel_info,
                self.spin,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
            );
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.src = Some(src);
        self.mesh = Some(mesh);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    fn upload(&mut self, frame: &FrameRef) {
        let expected_pitch = frame.width as usize * frame.format.bytes_per_pixel();
        let src = self.src.as_mut().unwrap();
        let pitch = if frame.pitch == 0 {
            expected_pitch
        } else {
            frame.pitch
        };
        let r = src.tex.update(None, frame.pixels, pitch);
        // DEBUG: dumps do frame cru (bissecção do quadrado) + log das primeiras.
        {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 3 {
                log::info!(
                    "DEBUG upload #{n}: {}x{} {:?} pixels={} pitch={} (esperado {expected_pitch}) ok={}",
                    frame.width,
                    frame.height,
                    frame.format,
                    frame.pixels.len(),
                    pitch,
                    r.is_ok()
                );
            }
            if let Ok(base) = std::env::var("PSX_XPERIENCE_DEBUG_RAW") {
                if matches!(n, 30 | 300 | 1500) && frame.format == crate::PixelFormat::Xrgb8888 {
                    let mut bmp = vec![0u8; 54 + frame.pixels.len()];
                    bmp[0..2].copy_from_slice(b"BM");
                    let size = (54 + frame.pixels.len()) as u32;
                    bmp[2..6].copy_from_slice(&size.to_le_bytes());
                    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
                    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
                    bmp[18..22].copy_from_slice(&(frame.width as i32).to_le_bytes());
                    bmp[22..26].copy_from_slice(&(-(frame.height as i32)).to_le_bytes());
                    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
                    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
                    bmp[54..].copy_from_slice(frame.pixels);
                    let name = format!("{}.raw{}.bmp", base.trim_end_matches(".bmp"), n);
                    let _ = std::fs::write(name, bmp);
                }
            }
        }
        r.expect("upload frame");
    }

    fn ensure_src(&mut self, w: u32, h: u32, format: PixelFormat) {
        let stale = match &self.src {
            Some(s) => s.w != w || s.h != h || s.format != format,
            None => true,
        };
        if stale {
            log::info!(
                "DEBUG ensure_src: criando {}x{} {:?} (era {:?})",
                w,
                h,
                format,
                self.src.as_ref().map(|s| (s.w, s.h, s.format))
            );
            let mut tex = self
                .canvas
                .create_texture_streaming(format.sdl(), w, h)
                .expect("create streaming texture");
            tex.set_scale_mode(SdlScaleMode::Linear);
            self.src = Some(SrcTexture { tex, w, h, format });
        }
    }

    fn ensure_mesh(&mut self, dst: Rect) {
        let (w, h) = (dst.width(), dst.height());
        if matches!(&self.mesh, Some(m) if m.w == w && m.h == h) {
            return;
        }
        self.mesh = Some(build_crt_mesh(dst, 1.0));
    }

    fn ensure_bezel(&mut self, out_w: u32, out_h: u32, screen: Rect) {
        let key = (out_w, out_h, screen.width(), screen.height());
        if matches!(&self.bezel, Some(b) if b.key == key) {
            return;
        }
        self.bezel = Some(build_bezel_mesh(out_w, out_h, screen, key));
    }

    // --- 2D path (selector) --------------------------------------------
    //
    // The selector draws flat 2D into an offscreen buffer the size of the tube
    // opening, which is then composited through the CRT mesh just like a game
    // frame — so the shelf bulges with the same tube.

    /// Register/replace an image from tightly-packed RGBA8 (call between frames).
    pub fn set_image(&mut self, id: u64, w: u32, h: u32, rgba: &[u8]) {
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        let mut tex = match self.canvas.create_texture_static(SdlFormat::RGBA32, w, h) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("cabinet: texture {w}x{h}: {e}");
                return;
            }
        };
        if tex.update(None, rgba, (w * 4) as usize).is_err() {
            return;
        }
        tex.set_blend_mode(BlendMode::Blend);
        tex.set_scale_mode(SdlScaleMode::Linear);
        let content = content_bbox(w, h, rgba);
        self.images.insert(id, ImgTex { tex, w, h, content });
    }

    /// Draw a 2D frame: `draw` renders into a screen-sized buffer (coords
    /// 0..screen), which is then warped through the tube, framed and presented.
    pub fn frame_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_2d(bg, draw);
        self.composite_screen();
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d`], but for the shelf specifically (plan
    /// revision): `draw` only ever sees the narrower grid area (the panel
    /// column is reserved first), and the shelf's own info panel is drawn
    /// flat, outside the tube's warp, right after it.
    pub fn frame_shelf<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_shelf();
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d`] but composited into an offscreen target and
    /// saved as a BMP (headless — a background window never composites on macOS).
    pub fn capture_2d<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_2d(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::capture_2d`], but for the shelf specifically (plan
    /// revision) — reserves the panel column like `frame_shelf` does, and
    /// draws the flat info panel into the same offscreen target so the BMP
    /// matches what a real window would show.
    pub fn capture_shelf<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let images = &self.images;
        let shelf_panel = self.shelf_panel.as_ref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            let _ = draw_shelf_panel(c, font, images, shelf_panel, panel);
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Show the green "CH 3" channel banner over the picture for `dur`
    /// (plan revision: "quando ligar o console, mostrar por 3 segundos e
    /// remover da tela") — the runner calls this on power-on, the way an old
    /// TV flashes the channel number when you tune it. While the tube is on
    /// static instead, `present_static` draws the banner constantly, no
    /// timer involved.
    pub fn flash_ch3(&mut self, dur: Duration) {
        self.ch3_until = Some(Instant::now() + dur);
    }

    /// Settings gate for the ambient static hiss (plan revision: "colocar
    /// na configuração para tocar ou não. por padrão vem desligado") — off
    /// by default; `present_static` queues the white noise only while this
    /// is set. Turning it off drops the stream immediately.
    pub fn set_static_hiss(&mut self, on: bool) {
        self.static_hiss = on;
        if !on {
            self.hiss = None;
        }
    }

    /// Queue a one-shot sound (mono `samples` at `rate`) on the cabinet's
    /// persistent foley stream — the stream outlives the call, so the queued
    /// audio actually plays (a stream dropped immediately after `queue`
    /// takes its audio with it). Returns `false` when no audio device is
    /// available; callers keep their fallback in that case.
    pub fn play_foley(&mut self, samples: &[i16], rate: u32) -> bool {
        if self.foley.is_none() {
            match AudioOut::new(&self.audio, rate) {
                Ok(a) => self.foley = Some(a),
                Err(e) => {
                    log::warn!("foley stream: {e}");
                    return false;
                }
            }
        }
        // Mono → the interleaved stereo `AudioOut` wants. Non-blocking: SDL
        // buffers and plays on its own; overlapping sounds queue in order.
        let mut buf = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            buf.push(s);
            buf.push(s);
        }
        if let Some(a) = &self.foley {
            a.queue(&buf);
        }
        true
    }

    /// Queue one frame's worth of hiss at `level` (the same 0..1 the visual
    /// snow uses, so blizzards hiss louder than the resting hiss). Opens the
    /// stream lazily on the first hissing frame.
    fn queue_static_hiss(&mut self, level: f32) {
        const RATE: u32 = 22_050;
        if self.hiss.is_none() {
            match AudioOut::new(&self.audio, RATE) {
                Ok(a) => self.hiss = Some(a),
                Err(e) => {
                    log::warn!("static hiss: {e}");
                    self.static_hiss = false;
                    return;
                }
            }
        }
        let n = (RATE / 60) as usize;
        let amp = (level.clamp(0.0, 1.0) * 1800.0) as i32;
        let mut buf = Vec::with_capacity(n * 2);
        for _ in 0..n {
            self.hiss_rng ^= self.hiss_rng << 13;
            self.hiss_rng ^= self.hiss_rng >> 17;
            self.hiss_rng ^= self.hiss_rng << 5;
            let s = (((self.hiss_rng >> 8) & 0xFFFF) as i32 - 0x8000) * amp / 0x8000;
            let v = s.clamp(-32000, 32000) as i16;
            buf.push(v);
            buf.push(v);
        }
        if let Some(a) = &self.hiss {
            a.queue(&buf);
        }
    }

    /// A transição estante → console (plano §6): o último quadro da estante
    /// (guardado em `screen_tex`) dissolve sobre o azul de AV — `t` 0.0..=1.0,
    /// 0 = estante inteira, 1 = só a tela azul. Mesmo tubo, mesma TV: nada de
    /// "outra tela abrindo por cima".
    pub fn present_shelf_fade(&mut self, level: f32, t: f32) {
        let t = t.clamp(0.0, 1.0);
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.ensure_mesh(self.screen);
        self.update_noise_tex(level);

        // Idêntico ao present_static — painel, brand, OSDs e botões no lugar —
        // trocando SÓ o conteúdo do tubo: o azul de AV por baixo e o último
        // quadro da estante (screen_tex) com alpha caindo por cima. O painel
        // permanece contínuo: muda o que está na TV, não a tela inteira.
        let mesh = self.mesh.take().unwrap();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));
        let st = self.screen_tex.as_mut();
        // Sem quadro da estante guardado (entrada direta no jogo — autoplay —
        // ou textura já larga), o tubo mostra só o azul de AV: nada para
        // dissolver, e nunca um unwrap em None.
        if let Some(st) = st {
            st.tex.set_alpha_mod(((1.0 - t) * 255.0) as u8);
            let _ = self
                .canvas
                .render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            st.tex.set_alpha_mod(255);
        }
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        self.mesh = Some(mesh);
        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            wh,
            &self.nameplate,
            self.nameplate_updates,
        );
        draw_ch3_osd(&mut self.canvas, &mut self.font, self.screen);
        self.draw_ra_badge();
        if self.static_hiss {
            self.queue_static_hiss(level);
        } else if let Some(h) = &self.hiss {
            h.clear();
        }
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            self.spin,
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
        );
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// One frame of signal-off snow through the tube, cartridge still visible
    /// in its slot if one is set. `level` 1.0 = a full blizzard, 0.0 = a dim,
    /// near-still hiss. Never a full-screen flash.
    pub fn present_static(&mut self, level: f32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.ensure_mesh(self.screen);
        self.update_noise_tex(level);

        // The mesh is cached like the gameplay path's — rebuilding 1,089
        // vertices every frame for a screen that only changes on
        // window-resize was pure waste.
        let mesh = self.mesh.take().unwrap();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));
        let nt = self.noise_tex.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&nt.tex), &mesh.indices[..]);
        self.noise_tex = Some(nt);
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        self.mesh = Some(mesh);
        // O retorno PRECISA ser guardado: são os rects clicáveis das setas
        // verdes de update — sem isto o idle desenha as setas mas o clique
        // nunca acerta (hit_update_arrow lia uma lista sempre vazia).
        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            wh,
            &self.nameplate,
            self.nameplate_updates,
        );
        // Sem sinal (plan revision): the channel banner stays up the whole
        // time the TV is showing snow — game inserted but powered off, the
        // idle screen, all of it — like a real set parked on channel 3.
        draw_ch3_osd(&mut self.canvas, &mut self.font, self.screen);
        // A conta ativa marca presença até na estática: o badge do RA no
        // queixo, do lado oposto ao nameplate.
        self.draw_ra_badge();
        // The ambient hiss (opt-in via settings): follows the same level as
        // the visual snow, so a burst hisses loud and the resting hiss stays
        // a quiet background shhh.
        if self.static_hiss {
            self.queue_static_hiss(level);
        } else if let Some(h) = &self.hiss {
            h.clear();
        }
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            self.spin,
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
        );
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_static`] but composited into an offscreen
    /// target and saved as a BMP (headless — eyeball the power-off / idle-off
    /// screen, cartridge and all, without a window).
    pub fn capture_static_bmp(
        &mut self,
        level: f32,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.update_noise_tex(level);
        let screen = self.screen;

        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let nt = self.noise_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let ra_status = self.ra_status;
        let font = &mut self.font;
        let images = &self.images;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&nt.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(c, font, screen, wh, nameplate, nameplate_updates);
            draw_ch3_osd(c, font, screen);
            if let Some(status) = ra_status {
                draw_chin_ra(c, font, images, screen, wh, nameplate, status.hardcore);
            }
            draw_panel(
                c,
                font,
                images,
                panel_info,
                self.spin,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
            );
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.noise_tex = Some(nt);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Drive the panel's cartridge slot animation (plan revision: "a
    /// animação deveria estar onde está o cartucho durante a gameplay, não
    /// uma transição"). `Some((t, ejecting))` animates the cartridge art in
    /// the panel's own cartridge block — `t` 0.0..=1.0 progress, `ejecting`
    /// mirrors the motion (see `draw_panel_slot`); every `present_static`/
    /// gameplay frame while this is set draws that progress. `None` is the
    /// resting state — the cartridge seated in the slot, which is also what
    /// `set_panel` starts with. A no-op before `set_panel`; `runner`'s two
    /// animation functions are the only callers.
    pub fn set_cartridge_motion(&mut self, motion: Option<(f32, bool)>) {
        if let Some(panel) = &mut self.panel {
            panel.cartridge_motion = motion;
        }
    }

    /// Like [`Cabinet::frame_2d`], but blended up from residual signal-off snow
    /// instead of cutting in cold: `static_level` is the snow still showing
    /// behind it, `shelf_alpha` (0..1) how much of the drawn frame shows on top
    /// (plan §3.3, "a estante entra por cima"). Call with `shelf_alpha` ramping
    /// 0.0 -> 1.0 over the first handful of frames after a game closes.
    pub fn frame_2d_fade_in<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        static_level: f32,
        shelf_alpha: f32,
    ) {
        self.paint_2d(bg, draw);
        self.update_noise_tex(static_level);
        let canvas_rect = self.canvas_rect;

        let mesh_static = build_crt_mesh(self.screen, 1.0);
        let mesh_shelf = build_crt_mesh(self.screen, shelf_alpha.clamp(0.0, 1.0));

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let nt = self.noise_tex.take().unwrap();
        let _ = self.canvas.render_geometry(
            &mesh_static.verts,
            Some(&nt.tex),
            &mesh_static.indices[..],
        );
        self.noise_tex = Some(nt);

        let st = self.screen_tex.take().unwrap();
        let _ =
            self.canvas
                .render_geometry(&mesh_shelf.verts, Some(&st.tex), &mesh_shelf.indices[..]);
        self.screen_tex = Some(st);

        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );

        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d_fade_in`], but for the shelf specifically
    /// (plan revision): reserves the panel column like `frame_shelf` does,
    /// and draws it flat, in the same viewport, right after the blended
    /// grid/static — the panel itself never fades with the signal, same as
    /// the in-game panel doesn't.
    pub fn frame_shelf_fade_in<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        static_level: f32,
        shelf_alpha: f32,
    ) {
        self.paint_shelf(bg, draw);
        self.update_noise_tex(static_level);
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());

        let mesh_static = build_crt_mesh(self.screen, 1.0);
        let mesh_shelf = build_crt_mesh(self.screen, shelf_alpha.clamp(0.0, 1.0));

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let nt = self.noise_tex.take().unwrap();
        let _ = self.canvas.render_geometry(
            &mesh_static.verts,
            Some(&nt.tex),
            &mesh_static.indices[..],
        );
        self.noise_tex = Some(nt);

        let st = self.screen_tex.take().unwrap();
        let _ =
            self.canvas
                .render_geometry(&mesh_shelf.verts, Some(&st.tex), &mesh_shelf.indices[..]);
        self.screen_tex = Some(st);

        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );
        self.shelf_buttons = draw_shelf_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.shelf_panel.as_ref(),
            panel,
        );
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);

        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Fill the noise texture for the current window size at `level` (1.0 =
    /// full blizzard, 0.0 = a dim near-still hiss); leaves it in `noise_tex`.
    fn update_noise_tex(&mut self, level: f32) {
        const NW: u32 = 320;
        const NH: u32 = 240;
        // The hiss regenerates at ~30Hz (and whenever the level moves) —
        // 60Hz of fresh 307KB noise + texture upload is invisible busywork.
        let now = Instant::now();
        if let Some((t, l)) = self.noise_stamp {
            if now.duration_since(t) < Duration::from_millis(30) && (l - level).abs() < 0.005 {
                return;
            }
        }
        self.noise_stamp = Some((now, level));
        self.ensure_noise_tex(NW, NH);
        // Sem sinal na entrada composta = a tela azul famosa da TV ("AV 1").
        // O level ainda governa o brilho: os bursts de power piscam a tela
        // mais clara antes de assentar no azul escuro de repouso.
        {
            let Self { noise, .. } = &mut *self;
            if noise.len() != (NW * NH * 4) as usize {
                *noise = vec![0u8; (NW * NH * 4) as usize];
            }
            let k = level.clamp(0.0, 1.0);
            // Azul de AV: quase nada de vermelho, verde pela metade.
            let (r, g, b) = (8u32, 32u32, (60.0 + 140.0 * k) as u32);
            let px = [r as u8, g as u8, b.min(255) as u8, 255];
            for p in noise.as_chunks_mut::<4>().0 {
                *p = px;
            }
        }
        let nt = self.noise_tex.as_mut().unwrap();
        let _ = nt.tex.update(None, &self.noise, (NW * 4) as usize);
    }

    /// The back cover enlarged (plan revision: "ao clicar no back cover
    /// possibilitar mostrar em tamanho maior"): the image aspect-preserved as
    /// large as the *tube* allows over a dark backdrop, with a "voltar" button
    /// centred right below it — drawn into the screen buffer and warped
    /// through the CRT mesh like the shelf itself, so the zoom happens on the
    /// TV, not flat over the whole window (plan revision: "abrir ele dentro
    /// da tv"). The flat side panel keeps drawing beside it, untouched.
    /// Returns the button's rect in screen-local coordinates — hit-test it
    /// through `hit_screen_point`, not raw output coordinates.
    pub fn frame_image_zoom(&mut self, id: u64, label: &str) -> Rect {
        let (sw, sh) = self.shelf_screen_size();
        let pad = 24i32;
        let btn_h = GLYPH_H as i32 + 14;
        let btn_w = (GLYPH_W as i32 * label.chars().count() as i32) + 28;
        let btn = Rect::new(
            (sw as i32 - btn_w).max(pad) / 2,
            sh as i32 - pad - btn_h,
            btn_w as u32,
            btn_h as u32,
        );
        let img_w = sw.saturating_sub(pad as u32 * 2);
        let img_h = (sh as i32 - pad * 2 - btn_h - 14).max(60) as u32;
        self.paint_shelf((10, 10, 11), |d| {
            d.image_fit(id, pad, pad, img_w, img_h);
            d.outline(
                btn.x(),
                btn.y(),
                btn.width(),
                btn.height(),
                1,
                (PANEL_TEXT.0, PANEL_TEXT.1, PANEL_TEXT.2, 255),
            );
            d.fill(
                btn.x() + 1,
                btn.y() + 1,
                btn.width().saturating_sub(2),
                btn.height().saturating_sub(2),
                (PANEL_BTN_BG.0, PANEL_BTN_BG.1, PANEL_BTN_BG.2, 255),
            );
            let text_w = GLYPH_W as i32 * label.chars().count() as i32;
            d.text(
                btn.x() + (btn.width() as i32 - text_w) / 2,
                btn.y() + (btn.height() as i32 - GLYPH_H as i32) / 2,
                1,
                PANEL_TEXT,
                label,
            );
        });
        self.composite_shelf();
        self.present_and_time();
        btn
    }

    /// Render `draw` into the screen buffer. Shared by `frame_2d` / `capture_2d`.
    fn paint_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.canvas_rect = cabinet_canvas_rect(real_w, real_h);
        let ww = self.canvas_rect.width();
        self.paint_2d_avail(bg, draw, ww);
    }

    /// Like [`Cabinet::paint_2d`], but reserving the flat side-panel column
    /// first (plan revision: "o painel nao pode estar dentro da TV") — the
    /// shelf's own screen buffer only ever spans the narrower grid area.
    fn paint_shelf<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.canvas_rect = cabinet_canvas_rect(real_w, real_h);
        let (out_w, out_h) = (self.canvas_rect.width(), self.canvas_rect.height());
        let cab_w = out_w.saturating_sub(panel_rect(out_w, out_h).width());
        self.paint_2d_avail(bg, draw, cab_w);
    }

    /// Shared by `paint_2d`/`paint_shelf`: `self.canvas_rect` must already be
    /// set; `avail_w` is how much of its width the screen buffer gets (all
    /// of it for the plain 2D path, minus the panel for the shelf's).
    fn paint_2d_avail<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F, avail_w: u32) {
        let wh = self.canvas_rect.height();
        self.screen = screen_area(avail_w, wh);
        let (sw, sh) = (self.screen.width(), self.screen.height());
        self.ensure_screen_tex(sw, sh);
        self.ensure_bezel(avail_w, wh, self.screen);

        let Self {
            canvas,
            screen_tex,
            font,
            images,
            ..
        } = self;
        let images = &*images;
        let st = screen_tex.as_mut().unwrap();
        let _ = canvas.with_texture_canvas(&mut st.tex, |c| {
            c.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
            c.clear();
            let mut s = Screen {
                canvas: c,
                font,
                images,
                w: sw,
                h: sh,
            };
            draw(&mut s);
        });
        // The clip lives on the shared underlying renderer; clear it.
        self.canvas.set_clip_rect(ClippingRect::None);
    }

    /// Warp the screen buffer through the tube into the live window, then frame.
    fn composite_screen(&mut self) {
        let mesh = build_crt_mesh(self.screen, 1.0);
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(self.canvas_rect));
        let st = self.screen_tex.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
        self.screen_tex = Some(st);
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            self.canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );
        self.draw_ra_badge();
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::composite_screen`], but also draws the settings
    /// screen's flat panel (plan revision) right after the warped content —
    /// the section buttons and "Voltar", hit-tested via
    /// `hit_settings_button`.
    fn composite_settings(&mut self) {
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.settings_buttons = draw_settings_panel(
            &mut self.canvas,
            &mut self.font,
            self.settings_panel.as_ref(),
            panel,
        );
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::frame_shelf`], but for the settings screen (plan
    /// revision: "a tela de configurações não está no padrão do resto do
    /// app") — the exact same split as the shelf (warped tube on the left,
    /// flat panel on the right), so the TV keeps the same proportions
    /// everywhere.
    pub fn frame_settings<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_settings();
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d`], but in the idle screen's own layout (plan
    /// revision: "na tela de download fazer igual o padrão das outras telas,
    /// não esticar a tv e mostrar o painel lateral") — the screen buffer
    /// only spans the cabinet column (same split as the shelf/settings), so
    /// the TV keeps its normal proportions, and the idle side panel draws
    /// and stays clickable around it. Hit-testing inside `draw` pairs with
    /// `window_to_screen`.
    pub fn frame_idle_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            self.spin,
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::capture_shelf`], but drawing the settings panel
    /// (headless preview — the BMP matches what a real window would show).
    pub fn capture_settings<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let settings_panel = self.settings_panel.as_ref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            let _ = draw_settings_panel(c, font, settings_panel, panel);
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::frame_idle_2d`], but composited into an offscreen
    /// target and saved as a BMP (headless preview — the BMP matches what a
    /// real window would show, idle panel included).
    pub fn capture_idle_2d<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let images = &self.images;
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            let _ = draw_panel(
                c,
                font,
                images,
                panel_info,
                0.0,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
            );
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::composite_screen`], but also draws the shelf's flat
    /// side panel (plan revision) right after the warped grid, in the same
    /// viewport — undistorted, like the in-game panel already is.
    fn composite_shelf(&mut self) {
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.shelf_buttons = draw_shelf_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.shelf_panel.as_ref(),
            panel,
        );
        self.canvas.set_viewport(None);
    }

    fn ensure_screen_tex(&mut self, w: u32, h: u32) {
        if matches!(&self.screen_tex, Some(s) if s.w == w && s.h == h) {
            return;
        }
        let mut tex = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, w, h)
            .expect("create screen target");
        tex.set_scale_mode(SdlScaleMode::Linear);
        // So `frame_2d_fade_in`'s vertex alpha can blend it over the snow.
        tex.set_blend_mode(BlendMode::Blend);
        self.screen_tex = Some(SizedTex { tex, w, h });
    }

    fn ensure_noise_tex(&mut self, w: u32, h: u32) {
        if matches!(&self.noise_tex, Some(s) if s.w == w && s.h == h) {
            return;
        }
        let mut tex = self
            .canvas
            .create_texture_streaming(SdlFormat::RGBA32, w, h)
            .expect("create noise texture");
        tex.set_scale_mode(SdlScaleMode::Linear);
        self.noise_tex = Some(SizedTex { tex, w, h });
    }
}

/// A 2D drawing surface, screen-local coordinates (0,0 = top-left of the tube
/// opening). Handed to the `frame_2d` / `capture_2d` closure.
pub struct Screen<'a> {
    canvas: &'a mut WindowCanvas,
    font: &'a mut Texture,
    images: &'a HashMap<u64, ImgTex>,
    w: u32,
    h: u32,
}

impl Screen<'_> {
    /// Size of the drawing surface.
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    pub fn fill(&mut self, x: i32, y: i32, w: u32, h: u32, c: (u8, u8, u8, u8)) {
        self.canvas.set_draw_color(Color::RGBA(c.0, c.1, c.2, c.3));
        let _ = self.canvas.fill_rect(Rect::new(x, y, w, h));
    }

    /// A small filled five-point star centred on (cx, cy) — the favorite
    /// marker (plan revision: "o icone de favorito coloca uma estrela
    /// vermelha"). The font atlas has no star glyph, so it's a polygon: ten
    /// points alternating outer/inner radius, filled as a fan.
    pub fn star(&mut self, cx: i32, cy: i32, r: u32, c: (u8, u8, u8)) {
        let (cxf, cyf, rf) = (cx as f32, cy as f32, r as f32);
        let point = |k: usize| {
            let radius = if k.is_multiple_of(2) { rf } else { rf * 0.45 };
            let angle = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
            sdl3::render::FPoint::new(cxf + radius * angle.cos(), cyf + radius * angle.sin())
        };
        let vtx = |p: sdl3::render::FPoint| Vertex {
            position: p,
            color: FColor::RGBA(
                c.0 as f32 / 255.0,
                c.1 as f32 / 255.0,
                c.2 as f32 / 255.0,
                1.0,
            ),
            tex_coord: sdl3::render::FPoint::new(0.0, 0.0),
        };
        let mut verts = vec![vtx(sdl3::render::FPoint::new(cxf, cyf))];
        for k in 0..10 {
            verts.push(vtx(point(k)));
        }
        let mut indices = Vec::with_capacity(30);
        for k in 1..=10 {
            indices.extend_from_slice(&[0, k, k % 10 + 1]);
        }
        let _ = self
            .canvas
            .render_geometry(&verts, None::<&Texture>, &indices[..]);
    }

    pub fn outline(&mut self, x: i32, y: i32, w: u32, h: u32, thick: u32, c: (u8, u8, u8, u8)) {
        let t = thick as i32;
        self.fill(x, y, w, thick, c);
        self.fill(x, y + h as i32 - t, w, thick, c);
        self.fill(x, y, thick, h, c);
        self.fill(x + w as i32 - t, y, thick, h, c);
    }

    /// Draw `s` at `(x, y)`, `scale`x the glyph cell. Returns the advance width.
    pub fn text(&mut self, x: i32, y: i32, scale: u32, c: (u8, u8, u8), s: &str) -> i32 {
        self.font.set_color_mod(c.0, c.1, c.2);
        let cell = (GLYPH_W * scale) as i32;
        let mut pen = x;
        for ch in s.chars() {
            let idx = glyph_index(ch);
            if ch != ' ' {
                let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
                let dst = Rect::new(pen, y, GLYPH_W * scale, GLYPH_H * scale);
                let _ = self.canvas.copy(self.font, src, dst);
            }
            pen += cell;
        }
        pen - x
    }

    /// Word-wrap `s` into `max_w`, returning the y past the last line.
    pub fn text_wrapped(
        &mut self,
        x: i32,
        y: i32,
        max_w: u32,
        scale: u32,
        c: (u8, u8, u8),
        s: &str,
    ) -> i32 {
        let cols = (max_w / (GLYPH_W * scale)).max(1) as usize;
        let row = (GLYPH_H * scale) as i32;
        let mut line = String::new();
        let mut line_len = 0usize;
        let mut cy = y;
        for word in s.split_whitespace() {
            let word_len = word.chars().count();
            if !line.is_empty() && line_len + 1 + word_len > cols {
                self.text(x, cy, scale, c, &line);
                cy += row + 2;
                line.clear();
                line_len = 0;
            }
            if !line.is_empty() {
                line.push(' ');
                line_len += 1;
            }
            line.push_str(word);
            line_len += word_len;
            // `.chars()` (not `split_at`, byte-indexed): a wrap point that
            // fell mid-character would panic on any accented/multi-byte word.
            while line_len > cols {
                let head: String = line.chars().take(cols).collect();
                let tail: String = line.chars().skip(cols).collect();
                self.text(x, cy, scale, c, &head);
                cy += row + 2;
                line = tail;
                line_len = line.chars().count();
            }
        }
        if !line.is_empty() {
            self.text(x, cy, scale, c, &line);
            cy += row + 2;
        }
        cy
    }

    /// Height [`Screen::text_wrapped`] would take for `s`, without drawing.
    pub fn wrapped_height(&self, max_w: u32, scale: u32, s: &str) -> i32 {
        wrapped_height(max_w, scale, s)
    }

    /// Clip drawing to `rect`; `None` clears the clip.
    pub fn clip(&mut self, rect: Option<(i32, i32, u32, u32)>) {
        self.canvas.set_clip_rect(match rect {
            Some((x, y, w, h)) => ClippingRect::Some(Rect::new(x, y, w, h)),
            None => ClippingRect::None,
        });
    }

    pub fn has_image(&self, id: u64) -> bool {
        self.images.contains_key(&id)
    }

    /// Draw image `id` letterboxed inside the box, centered. No-op if unknown.
    pub fn image_fit(&mut self, id: u64, x: i32, y: i32, bw: u32, bh: u32) {
        let Some(img) = self.images.get(&id) else {
            return;
        };
        let (iw, ih) = (img.w as f32, img.h as f32);
        let scale = (bw as f32 / iw).min(bh as f32 / ih);
        let dw = (iw * scale).round() as i32;
        let dh = (ih * scale).round() as i32;
        let dx = x + (bw as i32 - dw) / 2;
        let dy = y + (bh as i32 - dh) / 2;
        let _ = self.canvas.copy(
            &img.tex,
            None::<sdl3::render::FRect>,
            Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
        );
    }
}

fn resolve_aspect(a: f32) -> f32 {
    if a > 0.0 {
        a
    } else {
        4.0 / 3.0
    }
}

fn centered_in(area: Rect, w: u32, h: u32) -> Rect {
    let x = area.x() + (area.width() as i32 - w as i32) / 2;
    let y = area.y() + (area.height() as i32 - h as i32) / 2;
    Rect::new(x, y, w, h)
}

/// Largest `aspect`-shaped rect that fits inside `area`, centered in it. Fills
/// the height unless that would overflow the width, then fills the width.
fn fit_aspect_in(area: Rect, aspect: f32) -> Rect {
    let mut h = area.height();
    let mut w = (h as f32 * aspect).round() as u32;
    if w > area.width() {
        w = area.width();
        h = (w as f32 / aspect).round() as u32;
    }
    centered_in(area, w, h)
}

/// Where the cabinet actually draws within the real window/display: the
/// whole display when its shape is in the 16:10..16:9 band (Steam Deck's
/// 1280x800, laptops, modern TVs — the composition draws proportionally, so
/// the cabinet simply gets a touch taller), centered otherwise: displays
/// outside the band keep the largest 16:9 rect that fits — an ultrawide
/// monitor gets letterbox bars on the sides instead of a stretched-wide
/// cabinet. `screen_area`/`panel_rect` (and everything downstream) only
/// ever see this rect's width/height, never the raw output size.
fn cabinet_canvas_rect(out_w: u32, out_h: u32) -> Rect {
    const MIN_ASPECT: f32 = 1.6; // 16:10 — Steam Deck
    const MAX_ASPECT: f32 = 16.0 / 9.0;
    let out_aspect = out_w as f32 / out_h.max(1) as f32;
    let aspect = out_aspect.clamp(MIN_ASPECT, MAX_ASPECT);
    fit_aspect_in(Rect::new(0, 0, out_w, out_h), aspect)
}

/// The cabinet opening: the window inset by the bezel fractions (a wider chin).
fn screen_area(out_w: u32, out_h: u32) -> Rect {
    let sx = (out_w as f32 * BEZEL_SIDE).round() as i32;
    let ty = (out_h as f32 * BEZEL_TOP).round() as i32;
    let by = (out_h as f32 * BEZEL_CHIN).round() as i32;
    let w = (out_w as i32 - 2 * sx).max(16) as u32;
    let h = (out_h as i32 - ty - by).max(16) as u32;
    Rect::new(sx, ty, w, h)
}

/// A four-quad ring from the window edge (cabinet colour) to the recessed
/// screen edge (near-black), so the screen sits in a shadowed well.
fn build_bezel_mesh(out_w: u32, out_h: u32, screen: Rect, key: (u32, u32, u32, u32)) -> BezelMesh {
    let norm = |c: (u8, u8, u8)| {
        FColor::RGBA(
            c.0 as f32 / 255.0,
            c.1 as f32 / 255.0,
            c.2 as f32 / 255.0,
            1.0,
        )
    };
    let (cab, rec) = (norm(CABINET), norm(RECESS));
    let z = sdl3::render::FPoint::new(0.0, 0.0);
    let vtx = |x: i32, y: i32, c: FColor| Vertex {
        position: sdl3::render::FPoint::new(x as f32, y as f32),
        color: c,
        tex_coord: z,
    };

    let (or, ob) = (out_w as i32, out_h as i32);
    let (il, it, ir, ib) = (screen.left(), screen.top(), screen.right(), screen.bottom());

    let verts = vec![
        vtx(0, 0, cab),
        vtx(or, 0, cab),
        vtx(or, ob, cab),
        vtx(0, ob, cab), // 0..3 outer
        vtx(il, it, rec),
        vtx(ir, it, rec),
        vtx(ir, ib, rec),
        vtx(il, ib, rec), // 4..7 inner
    ];
    #[rustfmt::skip]
    let indices = vec![
        0, 1, 5, 0, 5, 4, // top
        1, 2, 6, 1, 6, 5, // right
        2, 3, 7, 2, 7, 6, // bottom
        3, 0, 4, 3, 4, 7, // left
    ];
    BezelMesh {
        verts,
        indices,
        key,
    }
}

/// The top-left "fechar app" button (plan revision: "criar botao de fechar
/// app no canto superior esquerdo") — a small square with an "X", same
/// visual language as `draw_button`. Fixed distance from the cabinet's own
/// top-left corner (canvas-local coordinates, the same space
/// `window_to_output` maps clicks into), so it sits in the same spot
/// regardless of window size or ultrawide letterboxing. Callers draw the
/// pair last, on top of whatever else the screen drew, on EVERY screen now
/// (plan revision: "mostrar o fechar e minimizar em todas as telas").
/// Returns the rect drawn, for `Cabinet::close_button`/`hit_close_button`.
fn draw_close_button(canvas: &mut WindowCanvas, font: &mut Texture) -> Rect {
    const MARGIN: i32 = 14;
    const SIZE: u32 = 32;
    draw_button(
        canvas,
        font,
        Rect::new(MARGIN, MARGIN, SIZE, SIZE),
        "X",
        true,
    )
}

/// The minimize button, sitting right next to `draw_close_button` (plan
/// revision: "colocar botão de minimizar ao lado de fechar") — same square
/// look, a dash for the iconify glyph. Returns the rect drawn, for
/// `Cabinet::minimize_button`/`hit_minimize_button`.
fn draw_minimize_button(canvas: &mut WindowCanvas, font: &mut Texture) -> Rect {
    const MARGIN: i32 = 14;
    const SIZE: u32 = 32;
    const GAP: i32 = 8;
    draw_button(
        canvas,
        font,
        Rect::new(MARGIN + SIZE as i32 + GAP, MARGIN, SIZE, SIZE),
        "-",
        true,
    )
}

/// The set's nameplate, printed into the chin left of the tube — part of the
/// cabinet itself, so unlike the panel it's drawn in every context (shelf,
/// game, idle-off) and never disappears. A no-op if the chin is too short to
/// hold it. `label` is `Cabinet::nameplate` (plan revision — `BRAND` plus the
/// app/core version once the app calls `set_nameplate`).
/// The channel banner's green — vivid OSD green, like the phosphor filter a
/// TV applies to its own on-screen display — no PSX é "AV 1": entrada
/// composta (cabos RCA), não RF (plano §6, revisão do usuário).
const OSD_GREEN: (u8, u8, u8) = (60, 230, 70);

/// The channel banner ("AV 1" no PSX: a TV fica na entrada composta; o
/// selo aparece na troca de entrada e na ausência de sinal) —
/// green, top-right of the tube. Drawn flat on the glass, deliberately NOT
/// warped with the picture: an OSD is the TV's own overlay, not part of the
/// signal (same reasoning as `draw_brand`'s chin text).
fn draw_ch3_osd(canvas: &mut WindowCanvas, font: &mut Texture, screen: Rect) {
    const LABEL: &str = "AV 1";
    const SCALE: u32 = 3;
    let w = GLYPH_W as i32 * SCALE as i32 * LABEL.chars().count() as i32;
    let margin = 24;
    let x = screen.right() - margin - w;
    let y = screen.top() + margin;
    // A 2px dark offset keeps the green readable over bright snow without a
    // full outline pass.
    draw_text_absolute(
        canvas,
        font,
        x + 2,
        y + 2,
        TextStyle::new(SCALE, (12, 14, 12)),
        LABEL,
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(SCALE, OSD_GREEN),
        LABEL,
        usize::MAX,
    );
}

/// Image id the RA mock example registers its badge under.
pub const DEMO_BADGE_IMG: u64 = u64::MAX;

/// Image id for the RetroAchievements logo the app registers (the favicon,
/// embedded in the app crate) — the persistent chin badge draws it instead
/// of the pixel gamepad when it's there.
pub const RA_LOGO_IMG: u64 = u64::MAX - 7;

impl Cabinet {
    /// Drop expired fronts, then draw the front block — or, with the queue
    /// empty, the persistent RetroAchievements badge (when armed).
    fn draw_osd_front(&mut self) {
        let now = Instant::now();
        while self.osd_queue.front().is_some_and(|e| e.until <= now) {
            self.osd_queue.pop_front();
        }
        let (_, out_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let Some(entry) = self.osd_queue.front() else {
            if let Some(status) = self.ra_status {
                draw_chin_ra(
                    &mut self.canvas,
                    &mut self.font,
                    &self.images,
                    self.screen,
                    out_h,
                    &self.nameplate,
                    status.hardcore,
                );
            }
            return;
        };
        draw_chin_osd(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.screen,
            out_h,
            &self.nameplate,
            &entry.lines,
            entry.badge,
        );
    }

    /// `draw_osd_front`'s decision, for the offscreen capture path (whose
    /// closure can't take `&mut self`) — expire and decide instead.
    fn osd_for_capture(&self) -> ChinOsd {
        let now = Instant::now();
        if let Some(e) = self.osd_queue.front().filter(|e| e.until > now) {
            return ChinOsd::Notify {
                lines: e.lines.clone(),
                badge: e.badge,
            };
        }
        match self.ra_status {
            Some(s) => ChinOsd::Ra {
                hardcore: s.hardcore,
            },
            None => ChinOsd::None,
        }
    }

    /// The persistent RA badge, for every chin that isn't the gameplay one
    /// (`composite_screen` covers início/estante/configurações; the idle's
    /// `present_static` and its capture call it directly).
    fn draw_ra_badge(&mut self) {
        let Some(status) = self.ra_status else {
            return;
        };
        let (_, out_h) = self.canvas.output_size().unwrap_or((1280, 720));
        draw_chin_ra(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.screen,
            out_h,
            &self.nameplate,
            status.hardcore,
        );
    }
}

/// The badge's two rows and their colors — pure so the tests can pin the
/// contract: "RA ATIVADO" always in the OSD green, the mode in amber when
/// hardcore and in the nameplate grey when softcore.
fn ra_badge_rows(hardcore: bool) -> [(&'static str, (u8, u8, u8)); 2] {
    [
        ("RA ATIVADO", OSD_GREEN),
        (
            if hardcore { "HARDCORE" } else { "SOFTCORE" },
            if hardcore {
                (240, 180, 60)
            } else {
                (235, 235, 225)
            },
        ),
    ]
}

/// The persistent RetroAchievements badge — the same chin corner as the
/// unlock block, shown whenever that block isn't on screen: the RA logo
/// (the registered favicon; falls back to the pixel gamepad tile) left of
/// two rows, "RA ATIVADO" in the OSD green and the mode below (amber for
/// hardcore, the nameplate grey for softcore). Same 2px dark shadow as
/// everything else on the glass.
fn draw_chin_ra(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    out_h: u32,
    nameplate: &str,
    hardcore: bool,
) {
    const MARGIN: i32 = 24;
    const TILE: i32 = 56;
    const TILE_BG: (u8, u8, u8) = (18, 22, 52);
    const TILE_EDGE: (u8, u8, u8) = (240, 180, 60);
    const GOLD: (u8, u8, u8) = (240, 190, 70);
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 {
        return;
    }
    let cell = GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let rows = ra_badge_rows(hardcore);
    // Same left limit as the unlock block: the nameplate's widest line.
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let right = screen.right() - MARGIN;
    let left_limit = screen.left() + nameplate_w + MARGIN * 2 + TILE + gap;
    let max_cols = ((right - left_limit).max(cell) / cell) as usize;
    let shown: Vec<(String, (u8, u8, u8))> = rows
        .iter()
        .map(|(l, c)| (truncate_to_cols(l, max_cols), *c))
        .collect();
    let block_h = 2 * row + gap;
    let text_w = shown
        .iter()
        .map(|(l, _)| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let mut y = chin_top + (chin_h - block_h) / 2;
    let tile_x = right - text_w - gap - TILE;
    let tile_y = chin_top + (chin_h - TILE) / 2;

    // The mark: the official RA favicon (registered by the app) when it's
    // there; the pixel gamepad tile is the fallback so the badge never
    // depends on the image having been registered.
    if images.contains_key(&RA_LOGO_IMG) {
        draw_image_absolute(
            canvas,
            images,
            RA_LOGO_IMG,
            tile_x,
            tile_y,
            TILE as u32,
            TILE as u32,
        );
    } else {
        canvas.set_draw_color(Color::RGB(TILE_EDGE.0, TILE_EDGE.1, TILE_EDGE.2));
        let _ = canvas.fill_rect(Rect::new(tile_x, tile_y, TILE as u32, TILE as u32));
        canvas.set_draw_color(Color::RGB(TILE_BG.0, TILE_BG.1, TILE_BG.2));
        let _ = canvas.fill_rect(Rect::new(
            tile_x + 2,
            tile_y + 2,
            (TILE - 4) as u32,
            (TILE - 4) as u32,
        ));
        const PAD: [&str; 7] = [
            "..............",
            "##############",
            "#..#......o.o#",
            "#.###........#",
            "#..#.........#",
            "##############",
            "..............",
        ];
        let s = 3;
        let px = tile_x + (TILE - 14 * s) / 2;
        let py = tile_y + (TILE - 7 * s) / 2;
        canvas.set_draw_color(Color::RGB(GOLD.0, GOLD.1, GOLD.2));
        for (gy, line) in PAD.iter().enumerate() {
            for (gx, ch) in line.chars().enumerate() {
                if ch == '.' {
                    continue;
                }
                let _ = canvas.fill_rect(Rect::new(
                    px + gx as i32 * s,
                    py + gy as i32 * s,
                    s as u32,
                    s as u32,
                ));
            }
        }
    }
    for (line, color) in &shown {
        let x = right - line.chars().count() as i32 * cell;
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(1, (12, 14, 12)),
            line,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(1, *color),
            line,
            usize::MAX,
        );
        y += row + gap;
    }
}

/// The achievements notification (plan fase 4): a right-aligned block in
/// the chin, mirroring `draw_brand` on the left. First line in the OSD
/// green (the "CONQUISTA DESBLOQUEADA" header), the achievement name in
/// near-white, the trailing line (points) in the nameplate's own grey —
/// all flat on the glass with the same 2px dark shadow as the CH 3 banner.
/// Lines longer than the space between the nameplate and the right margin
/// are truncated with "..." so the two blocks never touch. `badge` is the
/// image id registered via `set_image`, drawn left of the text.
#[allow(clippy::too_many_arguments)] // canvas/font/images + 5 layout facts
fn draw_chin_osd(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    out_h: u32,
    nameplate: &str,
    lines: &[String],
    badge: Option<u64>,
) {
    const MARGIN: i32 = 24;
    const LIGHT: (u8, u8, u8) = (235, 235, 225);
    const BADGE: u32 = 56;
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 || lines.is_empty() {
        return;
    }
    let cell = GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let has_badge = badge.is_some_and(|id| images.contains_key(&id));
    let badge_room = if has_badge { BADGE as i32 + 12 } else { 0 };
    // The nameplate's widest line sets this block's left limit, so a long
    // achievement name truncates instead of colliding with it.
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let right = screen.right() - MARGIN;
    let left_limit = screen.left() + nameplate_w + MARGIN * 2 + badge_room;
    let max_cols = ((right - left_limit).max(cell) / cell) as usize;
    let shown: Vec<String> = lines
        .iter()
        .map(|l| truncate_to_cols(l, max_cols))
        .collect();
    let block_h = shown.len() as i32 * row + (shown.len() as i32 - 1) * gap;
    let mut y = chin_top + (chin_h - block_h) / 2;
    let text_w = shown
        .iter()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    if has_badge {
        // Vertically centred in the chin, the same line as the text block's.
        draw_image_absolute(
            canvas,
            images,
            badge.unwrap_or(0),
            right - text_w - badge_room,
            chin_top + (chin_h - BADGE as i32) / 2,
            BADGE,
            BADGE,
        );
    }
    for (i, line) in shown.iter().enumerate() {
        let color = if i == 0 {
            OSD_GREEN
        } else if i + 1 == shown.len() {
            BRAND_TEXT
        } else {
            LIGHT
        };
        let x = right - line.chars().count() as i32 * cell;
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(1, (12, 14, 12)),
            line,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(1, color),
            line,
            usize::MAX,
        );
        y += row + gap;
    }
}

fn truncate_to_cols(s: &str, max_cols: usize) -> String {
    if s.chars().count() <= max_cols {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_cols.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// `(rect da seta do app, rect da seta do core)` — `None` quando a linha
/// não tem seta neste frame. Saída de [`draw_brand`] para o hit-test do
/// idle (plan revision: "ao clicar no icone verde de atualizar").
type UpdateArrowRects = (Option<Rect>, Option<Rect>);

fn draw_brand(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    label: &str,
    updates: (bool, bool),
) -> UpdateArrowRects {
    let mut arrows: UpdateArrowRects = (None, None);
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 {
        return arrows;
    }
    // A '\n' in the label stacks lines (plan revision: "no nameplate colocar
    // a versão do SwanStation abaixo do psx xperience") — the block centred in
    // the chin. When the chin can't fit them all, keep only the first (the
    // app's own name/version) rather than spilling over the bezel.
    let lines: Vec<&str> = label.split('\n').collect();
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let fits = chin_h >= (lines.len() as i32 * row + (lines.len() as i32 - 1) * gap).max(row);
    let shown = if fits { &lines[..] } else { &lines[..1] };
    let block_h = shown.len() as i32 * row + (shown.len() as i32 - 1) * gap;
    let mut y = chin_top + (chin_h - block_h) / 2;
    for (i, line) in shown.iter().enumerate() {
        draw_text_absolute(
            canvas,
            font,
            screen.left(),
            y,
            TextStyle::new(1, BRAND_TEXT),
            line,
            usize::MAX,
        );
        // The "tem update" arrow (plan revision: "um icone verde no
        // nameplate ... uma seta verde pra cima com update") — right after
        // the line's own text, vertically centred on it. Line 0 is the
        // app's version, line 1 the core's.
        let wants_arrow = i == 0 && updates.0 || i == 1 && updates.1;
        if wants_arrow {
            let ax = screen.left() + line.chars().count() as i32 * GLYPH_W as i32 + 10;
            draw_update_arrow(canvas, ax, y + row / 2);
            // Área de clique generosa (o ícone tem 11×12 px) — uma faixa
            // da altura da linha, um pouco mais larga que a seta.
            let hit = Rect::new(ax - 4, y, 19u32, row as u32);
            if i == 0 {
                arrows.0 = Some(hit);
            } else {
                arrows.1 = Some(hit);
            }
        }
        y += row + gap;
    }
    arrows
}

/// The nameplate's "tem update" marker (plan revision: "uma seta verde pra
/// cima com update") — a chunky pixel arrow pointing up, in the same green
/// as the CH 3 banner / OSD header, with the furniture's usual 1px dark
/// drop shadow. 11×12 px: a stepped triangle head (1-3-5-7-9 px rows,
/// matching the bitmap font's chunkiness) over a 3px stem.
fn draw_update_arrow(canvas: &mut WindowCanvas, x: i32, y_center: i32) {
    const HEAD_ROWS: [i32; 5] = [1, 3, 5, 7, 9];
    const STEM_W: i32 = 3;
    const STEM_H: i32 = 3;
    let total_h = HEAD_ROWS.len() as i32 + STEM_H;
    let top = y_center - total_h / 2;
    let cx = x + 5; // centre line of the widest head row
    let mut draw = |color: Color, ox: i32, oy: i32| {
        canvas.set_draw_color(color);
        for (r, &w) in HEAD_ROWS.iter().enumerate() {
            let _ = canvas.fill_rect(Rect::new(cx - w / 2 + ox, top + r as i32 + oy, w as u32, 1));
        }
        let _ = canvas.fill_rect(Rect::new(
            cx - STEM_W / 2 + ox,
            top + HEAD_ROWS.len() as i32 + oy,
            STEM_W as u32,
            STEM_H as u32,
        ));
    };
    draw(Color::RGB(12, 14, 12), 1, 1); // shadow
    draw(Color::RGB(OSD_GREEN.0, OSD_GREEN.1, OSD_GREEN.2), 0, 0);
}

/// Like `Screen::image_fit`, but at absolute window coordinates instead of
/// offset into the 2D screen buffer — for cabinet furniture and panel art.
fn draw_image_absolute(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    id: u64,
    x: i32,
    y: i32,
    bw: u32,
    bh: u32,
) {
    let Some(img) = images.get(&id) else {
        return;
    };
    let (iwf, ihf) = (img.w as f32, img.h as f32);
    let scale = (bw as f32 / iwf).min(bh as f32 / ihf);
    let dw = (iwf * scale).round() as i32;
    let dh = (ihf * scale).round() as i32;
    let dx = x + (bw as i32 - dw) / 2;
    let dy = y + (bh as i32 - dh) / 2;
    let _ = canvas.copy(
        &img.tex,
        None,
        Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
    );
}

/// Círculo preenchido rasterizado linha a linha (o canvas só tem retângulos).
fn fill_circle(canvas: &mut WindowCanvas, color: (u8, u8, u8), cx: i32, cy: i32, r: i32) {
    canvas.set_draw_color(Color::RGB(color.0, color.1, color.2));
    for dy in -r..=r {
        let half = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half, cy + dy, (half * 2).max(1) as u32, 1));
    }
}
/// PS1: um arco na borda superior, não um anel inteiro). `half_deg` é o
/// semi-ângulo do arco a partir do topo; `r_out` é o raio externo e
/// `r_in` o interno (a espessura do arco é `r_out - r_in`).
#[allow(clippy::too_many_arguments)]
fn fill_arc(
    canvas: &mut WindowCanvas,
    color: (u8, u8, u8),
    cx: i32,
    cy: i32,
    r_out: i32,
    r_in: i32,
    half_deg: f32,
    bottom: bool,
) {
    canvas.set_draw_color(Color::RGB(color.0, color.1, color.2));
    let max_x = (half_deg.to_radians().sin() * r_out as f32).ceil() as i32;
    for dx in -max_x..=max_x {
        let xx = dx.abs() as f32;
        let ho = ((r_out as f32 * r_out as f32 - xx * xx).sqrt()).round() as i32;
        let hi = if xx < r_in as f32 {
            ((r_in as f32 * r_in as f32 - xx * xx).sqrt()).round() as i32
        } else {
            0
        };
        if ho <= hi {
            continue;
        }
        // uma coluna de 1 px por dx: a faixa entre os dois raios, no lado
        // pedido (baixo: de cy+hi a cy+ho; cima: de cy-ho a cy-hi)
        let (y0, hgt) = if bottom {
            (cy + hi, (ho - hi).max(1))
        } else {
            (cy - ho, (ho - hi).max(1))
        };
        let _ = canvas.fill_rect(Rect::new(cx + dx, y0, 1, hgt as u32));
    }
}

/// Os retângulos clicáveis que a face do console devolve ao painel.
#[derive(Clone, Copy)]
struct FaceHits {
    power: Rect,
    eject: Rect,
    reset: Rect,
    cards1: Rect,
    cards2: Rect,
}

/// O bloco do console no painel, sem o corpo: fundo limpo, apenas os três
/// botões redondos do PS1 FAT (Power com LED, Reset menor acima dele, e o
/// Open/Eject do outro lado), o espaço central livre para o disco girar e
/// os dois botões de memory card no padrão das opções do painel. Retorna a
/// área central onde o disco se assenta.
fn draw_slot_furniture(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    block: Rect,
    led_on: bool,
    reset_pressed: bool,
) -> (Rect, FaceHits) {
    let fill = |canvas: &mut WindowCanvas, color: (u8, u8, u8), r: Rect| {
        canvas.set_draw_color(Color::RGB(color.0, color.1, color.2));
        let _ = canvas.fill_rect(r);
    };

    // Botões de memory card: dois, lado a lado, no padrão das opções do
    // painel (draw_button), ocupando a base do bloco.
    let btn_h = (GLYPH_H + 12) as i32;
    let half_w = (block.width() as i32 / 2 - 6).max(1);
    let mc_y = block.bottom() - btn_h - 2;
    let mc1 = Rect::new(block.x(), mc_y, half_w as u32, btn_h as u32);
    let mc2 = Rect::new(block.x() + half_w + 12, mc_y, half_w as u32, btn_h as u32);
    draw_button(canvas, font, mc1, "MC slot 1", true);
    draw_button(canvas, font, mc2, "MC slot 2", true);

    // Os três botões redondos, centrados na altura útil acima dos MCs. As
    // primeiras linhas do bloco são faixa exclusiva do wordmark — o disco
    // girando não pode passar por cima dele.
    let wordmark_h = 30;
    let face_top = block.y() + 6 + wordmark_h;
    let face_bottom = mc_y - 10;
    let face_h = (face_bottom - face_top).max(1);
    let btn_r = (face_h as f32 * 0.26).round() as i32;
    let col_l = block.x() + 10 + btn_r;
    let col_r = block.right() - 10 - btn_r;
    let btn_cy = face_top + face_h / 2;

    // Reset: redondo menor, diretamente acima do Power.
    let reset_r = (btn_r as f32 * 0.55).round() as i32;
    let reset_cy = btn_cy - btn_r - reset_r - 6;
    fill_circle(canvas, PSX_SHELL_EDGE, col_l, reset_cy, reset_r);
    fill_circle(canvas, PSX_SHELL, col_l, reset_cy, reset_r - 1);
    let reset_hit = Rect::new(
        col_l - reset_r,
        reset_cy - reset_r,
        (reset_r * 2) as u32,
        (reset_r * 2) as u32,
    );
    if reset_pressed {
        fill_circle(canvas, PSX_HUB_RING, col_l, reset_cy, reset_r - 2);
    }

    // Power: redondo, com o símbolo I/O (anel + traço vertical) e o LED
    // verde no topo da borda, como na foto.
    let power_cy = btn_cy + reset_r + 10;
    fill_circle(canvas, PSX_SHELL_EDGE, col_l, power_cy, btn_r);
    fill_circle(canvas, PSX_SHELL, col_l, power_cy, btn_r - 1);
    // O LED verde do PS1 original (foto de referência): uma fenda na borda
    // de BAIXO do botão, e o rótulo "POWER" gravado dentro do botão.
    fill_arc(
        canvas,
        if led_on { PSX_LED_ON } else { PSX_LED_OFF },
        col_l,
        power_cy,
        btn_r - 4,
        btn_r - 9,
        38.0,
        true,
    );
    let label = "POWER";
    let lw = label.chars().count() as i32 * GLYPH_W as i32;
    draw_text_absolute(
        canvas,
        font,
        col_l - lw / 2,
        power_cy - GLYPH_H as i32 / 2 - 2,
        TextStyle::new(1, PSX_BTN_TEXT),
        label,
        usize::MAX,
    );
    let power_hit = Rect::new(
        col_l - btn_r,
        power_cy - btn_r,
        (btn_r * 2) as u32,
        (btn_r * 2) as u32,
    );

    // Open/Eject: redondo do mesmo tamanho, símbolo ▲ sobre barra.
    fill_circle(canvas, PSX_SHELL_EDGE, col_r, power_cy, btn_r);
    fill_circle(canvas, PSX_SHELL, col_r, power_cy, btn_r - 1);
    let tri_w = 5;
    for i in 0..3 {
        fill(
            canvas,
            PSX_HUB,
            Rect::new(
                col_r - tri_w + i,
                power_cy - 7 + i,
                (tri_w - i * 2).max(1) as u32,
                1,
            ),
        );
    }
    fill(canvas, PSX_HUB, Rect::new(col_r - 5, power_cy - 2, 10, 2));
    let eject_hit = Rect::new(
        col_r - btn_r,
        power_cy - btn_r,
        (btn_r * 2) as u32,
        (btn_r * 2) as u32,
    );

    // A marca do console entre os botões (quando há a arte do wordmark).
    if let Some(tag) = images.get(&SLOT_TAG_IMG) {
        let (tcx, tcy, tcw, tch) = tag.content;
        let (tcw, tch) = (tcw.max(1), tch.max(1));
        let gap_w = (col_r - btn_r - (col_l + btn_r)).max(1) as f32;
        let max_h = 22.0f32;
        let scale = (max_h / tch as f32).min(gap_w / tcw as f32);
        let tw = (tcw as f32 * scale).round().max(1.0) as u32;
        let th = (tch as f32 * scale).round().max(1.0) as u32;
        let src = Rect::new(tcx as i32, tcy as i32, tcw.min(tag.w), tch.min(tag.h));
        let dst = Rect::new(
            block.x() + ((block.width() as i32 - tw as i32) / 2).max(0),
            block.y() + 4,
            tw,
            th,
        );
        let _ = canvas.copy(&tag.tex, src, dst);
    }

    // O espaço central livre, entre as duas colunas de botões: é onde o
    // disco se assenta e gira.
    let disc_l = col_l + btn_r + 3;
    let disc_r = col_r - btn_r - 3;
    let disc_t = face_top;
    let disc_b = face_bottom;
    let disc_w = (disc_r - disc_l).max(1);
    let disc_h = (disc_b - disc_t).max(1);
    let side = disc_w.min(disc_h);
    let disc = Rect::new(
        block.x() + (block.width() as i32 - side) / 2,
        face_top + (face_h - side) / 2,
        side as u32,
        side as u32,
    );
    let hits = FaceHits {
        power: power_hit,
        eject: eject_hit,
        reset: reset_hit,
        cards1: mc1,
        cards2: mc2,
    };
    (disc, hits)
}

/// Draw the panel's console block: o PSX com o disco do jogo entrando pela
/// fenda da tampa. `t`/`ejecting` come from `PanelInfo::cartridge_motion`
/// (`None` outside the animation means seated — tampa fechada, disco
/// inteiro escondido dentro do console, `SEAT_HIDDEN_FRAC` > 1 garante o
/// clip cem por cento opaco). Requires `PANEL_CARTRIDGE_IMG` to be loaded —
/// `draw_panel` only calls this under `has_cartridge`.
#[allow(clippy::too_many_arguments)]
fn draw_panel_slot(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    block: Rect,
    t: f32,
    ejecting: bool,
    led_on: bool,
    reset_pressed: bool,
    spin_deg: f32,
    spinning: bool,
) -> FaceHits {
    let (disc, hits) = draw_slot_furniture(canvas, font, images, block, led_on, reset_pressed);

    // O disco: desce de cima, ENCAIXA no centro da área livre (entre os
    // botões) e, com o console ligado, GIRA (a arte rotaciona em torno do
    // eixo). Menor que o bloco: cabe no espaço entre Power/Reset e Eject.
    if let Some(art) = images.get(&PANEL_CARTRIDGE_IMG) {
        let t = t.clamp(0.0, 1.0);
        let e = if ejecting {
            1.0 - (1.0 - t) * (1.0 - t)
        } else {
            t * t * (3.0 - 2.0 * t)
        };
        let p = if ejecting { 1.0 - e } else { e };
        // Assentado: centrado no espaço livre entre os botões.
        // O disco é redentro dentro do quad da textura: girado, ele
        // varre o MESMO círculo — cabe inteiro no vão entre os botões.
        let side = (disc.width().min(disc.height()) as f32) * 0.99;
        let hub_cx = disc.x() as f32 + disc.width() as f32 / 2.0;
        let hub_cy = disc.y() as f32 + disc.height() as f32 / 2.0;
        let hw = side / 2.0;
        let enter_cy = block.y() as f32 - side / 2.0;
        // O centro do disco desce do topo até o eixo e ENCAIXA nele.
        let ccx = hub_cx;
        let ccy = enter_cy + (hub_cy - enter_cy) * p;
        // Gira apenas assentado e com o console ligado.
        let angle = if spinning && p >= 1.0 && !ejecting {
            spin_deg
        } else {
            0.0
        };
        let s = angle.to_radians().sin();
        let c = angle.to_radians().cos();
        let white = sdl3::pixels::FColor::WHITE;
        let corner = |sx: f32, sy: f32| -> Vertex {
            let (lx, ly) = (sx * hw, sy * hw);
            Vertex {
                position: sdl3::render::FPoint::new(ccx + lx * c - ly * s, ccy + lx * s + ly * c),
                color: white,
                tex_coord: sdl3::render::FPoint::new((sx + 1.0) / 2.0, (sy + 1.0) / 2.0),
            }
        };
        let verts = [
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ];
        let _ = canvas.render_geometry(&verts, Some(&art.tex), &[0, 1, 2, 0, 2, 3]);
    }
    hits
}

/// The idle screen's console block: o mesmo gabinete PSX, com a fenda
/// vazia e o botão "Inserir disco" centrado na área livre acima da tampa.
/// Returns the button's rect as the clickable area (`PanelButton::Insert`).
fn draw_idle_slot(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    font: &mut Texture,
    block: Rect,
    label: &str,
    reset_pressed: bool,
) -> Rect {
    let (_disc, _hits) = draw_slot_furniture(canvas, font, images, block, false, reset_pressed);
    let btn_w = (block.width() as f32 * 0.7).round() as u32;
    let btn_h = (GLYPH_H as i32 * 2 + 16) as u32;
    let btn = Rect::new(
        block.x() + (block.width() as i32 - btn_w as i32) / 2,
        block.y() + 6 + (((block.height() as i32) - 12 - btn_h as i32) / 2).max(0),
        btn_w,
        btn_h,
    );
    draw_button(canvas, font, btn, label, true)
}

/// Scale + colour for one of the absolute-coordinate text helpers below —
/// bundled so those functions stay under clippy's argument-count limit.
#[derive(Clone, Copy)]
struct TextStyle {
    scale: u32,
    color: (u8, u8, u8),
}

impl TextStyle {
    fn new(scale: u32, color: (u8, u8, u8)) -> Self {
        Self { scale, color }
    }
}

/// A single line of text at absolute window coordinates, truncated to
/// `max_chars` (`usize::MAX` for no truncation) — for cabinet furniture that
/// isn't inside a `Screen`.
fn draw_text_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    style: TextStyle,
    s: &str,
    max_chars: usize,
) {
    let (r, g, b) = style.color;
    font.set_color_mod(r, g, b);
    let cell = (GLYPH_W * style.scale) as i32;
    let mut pen = x;
    for ch in s.chars().take(max_chars) {
        let idx = glyph_index(ch);
        if ch != ' ' {
            let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
            let dst = Rect::new(pen, y, GLYPH_W * style.scale, GLYPH_H * style.scale);
            let _ = canvas.copy(font, src, dst);
        }
        pen += cell;
    }
}

/// Word-wrapped text at absolute window coordinates, mirroring
/// `Screen::text_wrapped`'s layout. Returns the y past the last line.
fn draw_text_wrapped_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    max_w: u32,
    style: TextStyle,
    s: &str,
) -> i32 {
    let cols = (max_w / (GLYPH_W * style.scale)).max(1) as usize;
    let row = (GLYPH_H * style.scale) as i32;
    let mut line = String::new();
    let mut line_len = 0usize;
    let mut cy = y;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if !line.is_empty() && line_len + 1 + word_len > cols {
            draw_text_absolute(canvas, font, x, cy, style, &line, usize::MAX);
            cy += row + 2;
            line.clear();
            line_len = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_len += 1;
        }
        line.push_str(word);
        line_len += word_len;
        // `.chars()` (not `split_at`, byte-indexed): a wrap point that fell
        // mid-character would panic on any accented/multi-byte word.
        while line_len > cols {
            let head: String = line.chars().take(cols).collect();
            let tail: String = line.chars().skip(cols).collect();
            draw_text_absolute(canvas, font, x, cy, style, &head, usize::MAX);
            cy += row + 2;
            line = tail;
            line_len = line.chars().count();
        }
    }
    if !line.is_empty() {
        draw_text_absolute(canvas, font, x, cy, style, &line, usize::MAX);
        cy += row + 2;
    }
    cy
}

/// Where the side panel sits: a column on the right, `PANEL_FRAC` of the
/// window, clamped so it neither collapses nor swallows a small window.
fn panel_rect(out_w: u32, out_h: u32) -> Rect {
    let w = ((out_w as f32) * PANEL_FRAC)
        .round()
        .clamp(PANEL_MIN as f32, PANEL_MAX as f32) as u32;
    let w = w.min(out_w.saturating_sub(64));
    Rect::new((out_w - w) as i32, 0, w, out_h)
}

/// Draw the side panel: background, logo (or title) at top, session timer at
/// the bottom. Plain widgets, not warped by the tube (plan §2, §3.2). `None`
/// (no game loaded — the idle/root screen) draws just the "Inserir cartucho"
/// button in the logo's spot. Returns the clickable buttons drawn this frame,
/// in `rect`'s (output/canvas) coordinate space — the caller stores them for
/// `Cabinet::hit_panel_button`.
#[allow(clippy::too_many_arguments)]
fn draw_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    panel: Option<&PanelInfo>,
    spin: f32,
    rect: Rect,
    session: Duration,
    idle_core_prompt: Option<&str>,
    dev_mode: bool,
    core_status: Option<&str>,
) -> Vec<(PanelButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let y = rect.y() + pad;

    let Some(panel) = panel else {
        // Idle/root screen == the "cartridge ejected" screen (plan revision:
        // one screen, not two — startup, backing out of the shelf, and
        // ejecting a game all land here). Same slots a loaded game uses
        // (logo, then the cartridge block) with idle-appropriate stand-ins:
        // the console's own brand logo where a game's logo would sit, and
        // the cartridge block drawn as the same console slot — with an
        // "Inserir cartucho" button standing where a seated cartridge would
        // (plan revision: "na tela inicial... mostrar como se fosse a tela
        // do jogo, porem no lugar do cartucho inserido, mostrar botão
        // inserir cartucho"). No command legend — there's nothing loaded to
        // command — and Configuracoes moves to the footer, the same spot
        // the session clock uses during play.
        let mut cy = if images.contains_key(&PANEL_CONSOLE_LOGO_IMG) {
            draw_image_absolute(canvas, images, PANEL_CONSOLE_LOGO_IMG, x, y, inner_w, 110);
            y + 110
        } else {
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                y,
                inner_w,
                TextStyle::new(2, PANEL_TEXT),
                BRAND,
            )
        };
        cy += 8;
        // Same footprint as the game panel's cartridge block, so backing out
        // of the shelf or ejecting lands on a panel shaped exactly like the
        // gameplay one.
        const INSERT_H: u32 = 230;
        let insert_block = Rect::new(x, cy, inner_w, INSERT_H);
        let insert_drawn = draw_idle_slot(
            canvas,
            images,
            font,
            insert_block,
            "Estante de games",
            false,
        );
        cy += INSERT_H as i32;

        // The console's own controls, the same geometry the game panel
        // draws (plan revision: "é como se fosse a tela do jogo mesmo") —
        // Power/Reset/Eject e os MC slots já vivem na face do console —
        // os botões redondos do PS1 FAT desenhados pelo draw_idle_slot
        // acima. (Os rockers deslizantes POWER/RESET/EJETAR do SNES eram
        // desenhados aqui; saíram junto com o console antigo.)

        let btn_h = (GLYPH_H + 12) as i32;
        let settings = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
        let mut buttons = vec![
            (PanelButton::Insert, insert_drawn),
            (
                PanelButton::Settings,
                draw_button(canvas, font, settings, "Configurações", true),
            ),
        ];
        // O botão do modo dev (plan revision: "habilitar botão em cima da
        // configuração") — só existe com o devmode ligado, e o devmode só
        // entra pelo código Konami nesta tela. Entra na pilha entre o
        // "Configurações" e o aviso de core, que sobe junto.
        let mut btn_top = settings.y();
        if dev_mode {
            let dev = Rect::new(x, btn_top - 8 - btn_h, inner_w, btn_h as u32);
            btn_top = dev.y();
            buttons.push((
                PanelButton::Dev,
                draw_button(canvas, font, dev, "Dev", true),
            ));
        }
        // The core prompt (plan revision: "avisar que para jogar é
        // necessário o download do core" / "coloque o aviso em cima do
        // botão de download"): warning line directly above the button, both
        // just above the Configurações footer (ou do Dev, quando ele existe).
        if let Some(label) = idle_core_prompt {
            const WARN: &str = "Para jogar é necessário baixar o núcleo SwanStation.";
            let btn = Rect::new(x, btn_top - 8 - btn_h, inner_w, btn_h as u32);
            let warn_h = wrapped_height(inner_w, 1, WARN);
            let warn_y = btn.y() - 6 - warn_h;
            if warn_y > cy {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    x,
                    warn_y,
                    inner_w,
                    TextStyle::new(1, PANEL_WARN),
                    WARN,
                );
                buttons.push((
                    PanelButton::CoreDownload,
                    draw_button(canvas, font, btn, label, true),
                ));
            }
        } else if let Some(status) = core_status {
            // Andamento do download do núcleo na seta verde (plan revision:
            // "cliquei no icone verde de update do nucleo mas nao abriu
            // nada") — linha dim acima do rodapé; sem botão, o clique é na
            // seta do nameplate.
            let warn_h = wrapped_height(inner_w, 1, status);
            let warn_y = btn_top - 6 - warn_h;
            if warn_y > cy {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    x,
                    warn_y,
                    inner_w,
                    TextStyle::new(1, PANEL_WARN),
                    status,
                );
            }
        }
        return buttons;
    };

    // 1. Logo, or the title if there isn't one (plan §3.2, item 1).
    let mut cy = if panel.has_logo {
        draw_image_absolute(canvas, images, PANEL_LOGO_IMG, x, y, inner_w, 110);
        y + 110
    } else {
        draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            y,
            inner_w,
            TextStyle::new(2, PANEL_TEXT),
            &panel.title,
        )
    };

    // 1b. O console (o bloco é o próprio PSX, `draw_panel_slot`): o disco
    // entra pela fenda na animação e fica escondido em repouso — arte ou
    // sem arte, o console está sempre na cena. Os slots de memory card do
    // corpo viram o hit da biblioteca de cards (plano §3).
    // O console está sempre na cena (o bloco 1b atribui incondicionalmente);
    // a linha abaixo silencia o clippy que não vê isso.
    #[allow(unused_assignments)]
    let mut console_face_hits: Option<FaceHits> = None;
    {
        cy += 8;
        const CARTRIDGE_H: u32 = 220;
        let (t, ejecting) = panel.cartridge_motion.unwrap_or((1.0, false));
        let face_hits = draw_panel_slot(
            canvas,
            font,
            images,
            Rect::new(x, cy, inner_w, CARTRIDGE_H),
            t,
            ejecting,
            panel.powered,
            panel.reset_pressed,
            spin,
            panel.powered,
        );
        console_face_hits = Some(face_hits);
        cy += CARTRIDGE_H as i32;
    }

    // Below this y, stop — leave the session clock's own band (item 6)
    // clear. There are a lot more command rows than there used to be (plan
    // revision added six), so a long title plus a game with several curated
    // cheats can genuinely run out of room; a clean cutoff beats spilling
    // into the clock. No scrolling yet (a real gap, not silently accepted —
    // see `docs/fase-4.md`'s revision note).
    let limit = rect.bottom() - pad - (GLYPH_H as i32 + 12);

    // 2. Power / Eject / Reset vivem NA FACE do console (a foto do PS1 FAT:
    // Power redondo com o LED, Reset pequeno acima dele, Open/Eject do
    // outro lado no mesmo tamanho). Os hits chegam do bloco 1b.
    let mut buttons = Vec::new();
    if let Some(hits) = console_face_hits {
        buttons.push((PanelButton::Power, hits.power));
        buttons.push((PanelButton::Eject, hits.eject));
        buttons.push((PanelButton::Reset, hits.reset));
        buttons.push((PanelButton::Cards1, hits.cards1));
        buttons.push((PanelButton::Cards2, hits.cards2));
    }

    // 3. Commands — the console's own buttons, not the emulator's extras
    // (plan §3.2, item 3), every one of them clickable (plan revision:
    // mouse/gamepad only, no keyboard legend any more). Everything here
    // only acts while powered — Power/Eject/Reset are drawn separately
    // above (item 2) with their own dim rules, so they're filtered out of
    // this list rather than drawn twice.
    let other_commands: Vec<&(PanelButton, String)> = panel
        .commands
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k,
                PanelButton::Power | PanelButton::Eject | PanelButton::Reset
            )
        })
        .collect();
    if !other_commands.is_empty() && cy < limit {
        cy += 16;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1, PANEL_DIM),
            "comandos",
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 6;
    }
    for (kind, label) in other_commands {
        // Check the row's projected bottom, not just its top — otherwise
        // the last row drawn can still poke into the session clock's band.
        if cy + 4 + (GLYPH_H + 6) as i32 > limit {
            break;
        }
        cy += 4;
        // Tighter than a lone idle-screen button (plan revision: nine of
        // these stack now, not three — every extra pixel per row adds up).
        let btn = Rect::new(x, cy, inner_w, GLYPH_H + 6);
        let drawn = draw_button(canvas, font, btn, label, panel.powered);
        cy = drawn.bottom();
        buttons.push((*kind, drawn));
    }

    // 4. Cheats — just a count here (plan revision: used to list every
    // active description, one per line, which could run the panel out of
    // room on its own for a game with several curated cheats on at once;
    // the actual list lives in the Cheats modal, reachable from a
    // `commands` row above, so nothing is lost by not repeating it here).
    // Absent entirely with none on, not a "0 cheats" filler.
    let cheats_on_count = panel.cheats.iter().filter(|(_, on)| *on).count();
    // Room for the header plus one line — the count line never wraps to
    // more than that (it's always short: "N cheats ativados").
    let cheats_fit = cy + 16 + (GLYPH_H as i32 + 2) <= limit;
    if cheats_on_count > 0 && cheats_fit {
        cy += 16;
        let label = if cheats_on_count == 1 {
            "1 cheat ativado".to_string()
        } else {
            format!("{cheats_on_count} cheats ativados")
        };
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1, PANEL_TEXT),
            &label,
        );
    }

    // 5. Notes — a pinned print's thumbnail + counter, and/or a pinned
    // text-note slot's content (plan §3.2, item 5; §3.4; plan revision:
    // the two are independent — either, both, or neither can be showing at
    // once, "mostrar apenas uma nota e/ou uma imagem"). Used to show
    // whatever was captured/viewed most recently regardless of pin, which
    // was surprising; now it's opt-in on both sides. Absent entirely with
    // nothing pinned either way, not a "no notes" filler. Same room-check
    // shape as cheats, above — the thumbnail and the text block each add
    // their own extra room when present.
    let text_h = panel
        .text_note
        .as_ref()
        .map_or(0, |t| wrapped_height(inner_w, 1, t) + 6);
    let notes_h = 16
        + (GLYPH_H as i32 + 6)
        + if panel.has_note_thumb { 70 + 6 } else { 0 }
        + if panel.note_count > 0 {
            GLYPH_H as i32 + 6
        } else {
            0
        }
        + text_h;
    if (panel.note_count > 0 || panel.text_note.is_some()) && cy + notes_h <= limit {
        cy += 16;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1, PANEL_DIM),
            "notas",
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 6;
        if panel.has_note_thumb {
            draw_image_absolute(canvas, images, PANEL_NOTE_IMG, x, cy, inner_w, 70);
            cy += 70 + 6;
        }
        if panel.note_count > 0 {
            let label = if panel.note_count == 1 {
                "1 slot fixado".to_string()
            } else {
                format!("{} slots fixados", panel.note_count)
            };
            draw_text_absolute(
                canvas,
                font,
                x,
                cy,
                TextStyle::new(1, PANEL_TEXT),
                &label,
                usize::MAX,
            );
            cy += GLYPH_H as i32 + 6;
        }
        if let Some(text) = &panel.text_note {
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_TEXT),
                text,
            );
        }
    }

    // 6. Session clock, pinned to the bottom (plan §3.2, item 6).
    let secs = session.as_secs();
    let stamp = if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    };
    let ty = rect.bottom() - pad - GLYPH_H as i32;
    draw_text_absolute(
        canvas,
        font,
        x,
        ty,
        TextStyle::new(1, PANEL_DIM),
        "sessão",
        usize::MAX,
    );
    let label_w = (GLYPH_W as i32) * "sessão ".chars().count() as i32;
    draw_text_absolute(
        canvas,
        font,
        x + label_w,
        ty,
        TextStyle::new(1, PANEL_TEXT),
        &stamp,
        usize::MAX,
    );

    buttons
}

/// Draw the shelf's own flat side panel (plan revision — moved out of the
/// warped screen buffer so its text and art render undistorted, the same
/// treatment the in-game panel already gets): logo/title, cartridge art,
/// play count, then whatever extra info the No-Intro DAT carried beyond the
/// bare title, and finally the shelf's own Voltar/Configuracoes buttons.
/// `None` (nothing selected yet — an empty catalogue) still draws the two
/// buttons. Returns the clickable buttons drawn this frame, in `rect`'s
/// (output/canvas) coordinate space — the caller stores them for
/// `Cabinet::hit_shelf_button`.
/// The settings screen's flat panel (plan revision: "coloque o painel ... se
/// for interessante faça secoes na configuração"): title, one button per
/// section (the current one lit), and "Voltar" pinned at the bottom — the
/// same bottom-button stack the shelf panel uses.
fn draw_settings_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    panel: Option<&SettingsPanelInfo>,
    rect: Rect,
) -> Vec<(SettingsButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let mut y = rect.y() + pad;

    let Some(panel) = panel else {
        return Vec::new();
    };
    draw_text_wrapped_absolute(
        canvas,
        font,
        x,
        y,
        inner_w,
        TextStyle::new(2, PANEL_TEXT),
        &panel.title,
    );
    y += 2 * GLYPH_H as i32 + 16;

    let btn_h = (GLYPH_H + 12) as i32;
    let back_rect = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
    let mut buttons = vec![(
        SettingsButton::Back,
        draw_button(canvas, font, back_rect, "Voltar", true),
    )];

    // Section buttons, top to bottom under the title — the currently shown
    // section lit, the others dim (same lit/dim language as every other
    // panel button).
    let limit = back_rect.y() - 12;
    for (i, name) in panel.sections.iter().enumerate() {
        if y + btn_h > limit {
            break;
        }
        let r = Rect::new(x, y, inner_w, btn_h as u32);
        buttons.push((
            SettingsButton::Section(i),
            draw_button(canvas, font, r, name, i == panel.selected),
        ));
        y += btn_h + 8;
    }

    buttons
}

fn draw_shelf_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    panel: Option<&ShelfPanelInfo>,
    rect: Rect,
) -> Vec<(ShelfButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let y = rect.y() + pad;
    let panel_favorite = panel.and_then(|p| p.favorite);
    let panel_achievements = panel.is_some_and(|p| p.achievements);
    let panel_ach_reason = panel.and_then(|p| p.achievements_reason.as_deref());

    // Stacked buttons at the bottom, "Favoritar" / "Conquistas" /
    // "Configurações" / "Voltar" top to bottom — "Voltar" always the last
    // one (plan revision: "botao voltar sempre o ultimo botao do painel,
    // acima dele coloque configurações e depois acima o de favoritos");
    // "Conquistas" sits between favoritar e configurações (plan revision:
    // "quando ligar o RA colocar botão de lista de conquistas entre
    // favoritar e configurações"); "Atualizar" moved out of the panel into
    // the header row beside "histórico".
    let btn_h = (GLYPH_H + 12) as i32;
    let back_rect = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
    let settings_rect = Rect::new(x, back_rect.y() - 8 - btn_h, inner_w, btn_h as u32);
    // A pilha cresce para cima: Conquistas (quando o RA identificou) e
    // Favoritar — cada um só quando existe, e o corte do corpo acompanha
    // o botão de cima de verdade. O Manual vive nas abas do topo.
    let mut top = settings_rect.y();
    let ach_rect = panel_achievements.then(|| {
        let r = Rect::new(x, top - 8 - btn_h, inner_w, btn_h as u32);
        top = r.y();
        r
    });
    let fav_rect = panel_favorite.map(|_| {
        let r = Rect::new(x, top - 8 - btn_h, inner_w, btn_h as u32);
        top = r.y();
        r
    });
    let mut buttons = vec![
        (
            ShelfButton::Back,
            draw_button(canvas, font, back_rect, "Voltar", true),
        ),
        (
            ShelfButton::Settings,
            draw_button(canvas, font, settings_rect, "Configurações", true),
        ),
    ];
    if let Some(ach_r) = ach_rect {
        // Bloqueado (RA marcou o hash como versão não suportada): botão
        // apagado com o motivo no rótulo e SEM hit — clicar não faz nada
        // (plan revision: "mostrar mas deixar desativado").
        match panel_ach_reason {
            Some(reason) => {
                let _ = draw_button(
                    canvas,
                    font,
                    ach_r,
                    &format!("Conquistas ({reason})"),
                    false,
                );
            }
            None => buttons.push((
                ShelfButton::ShelfAchievements,
                draw_button(canvas, font, ach_r, "Conquistas", true),
            )),
        }
    }
    // The favorite button sits above the trio, drawn before the early
    // `None` return so it never depends on the panel having content.
    if let (Some(fav), Some(fav_r)) = (panel_favorite, fav_rect) {
        buttons.push((
            ShelfButton::ToggleFavorite,
            draw_button(
                canvas,
                font,
                fav_r,
                if fav { "Remover favorito" } else { "Favoritar" },
                true,
            ),
        ));
    }

    let Some(panel) = panel else {
        return buttons;
    };

    // Everything above the buttons — same cutoff rule `draw_panel` uses
    // for its command rows: a clean stop beats spilling into the buttons.
    // The cutoff is the TOP OF THE FIRST BUTTON ACTUALLY DRAWN (the stack
    // grows upward from "Voltar") — an earlier revision stopped one button
    // short, which was harmless while that slot sat empty and became a real
    // overflow the moment the RA's "Conquistas" button filled it (the info
    // text ran straight over the button).
    let limit = top - 12;

    let cy = if let Some(id) = panel.logo_img {
        draw_image_absolute(canvas, images, id, x, y, inner_w, 110);
        y + 110
    } else {
        draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            y,
            inner_w,
            TextStyle::new(2, PANEL_TEXT),
            &panel.title,
        )
    };

    // Abas (plan revision: "a ideia é mostrar um info de cada vez mesmo em
    // resolução grande, mudar ao clicar") — o corpo mostra SÓ os blocos da
    // seção ativa: capa traseira (imagem), cartucho (imagem) ou
    // informações (lançamento + campos do DAT/RA). Os três botões do topo
    // trocam a aba; a seção sem conteúdo mostra um aviso dim.
    let capa_blocks: Vec<PanelBlock> = panel
        .backcover_img
        .map(|id| vec![PanelBlock::Image(id, 280)])
        .unwrap_or_default();
    let cart_blocks: Vec<PanelBlock> = panel
        .cartridge_img
        .map(|id| vec![PanelBlock::Image(id, 210)])
        .unwrap_or_default();
    let mut info_blocks: Vec<PanelBlock> = Vec::new();
    if let Some(release) = &panel.release {
        info_blocks.push(PanelBlock::Release(release));
    }
    for (label, value) in &panel.info {
        // A linha de conquistas ganha a medalha de prêmio do RA quando o
        // servidor concedeu um.
        match panel.award_img.filter(|_| label == "conquistas") {
            Some(medal) => info_blocks.push(PanelBlock::RaField(label, value, medal)),
            None => info_blocks.push(PanelBlock::Field(label, value)),
        }
    }
    let manual_blocks: Vec<PanelBlock> = panel
        .manual_first_page
        .map(|id| vec![PanelBlock::Image(id, 280)])
        .unwrap_or_default();
    let (blocks, empty_label): (Vec<PanelBlock>, &str) = match panel.section {
        PanelSection::CapaTraseira => (capa_blocks, "sem capa traseira"),
        PanelSection::Cartucho => (cart_blocks, "sem disco"),
        PanelSection::Informacoes => (info_blocks, "sem informações"),
        PanelSection::Manual => (manual_blocks, "sem manual"),
    };

    let has = |s: PanelSection| match s {
        PanelSection::CapaTraseira => panel.backcover_img.is_some(),
        PanelSection::Cartucho => panel.cartridge_img.is_some(),
        PanelSection::Informacoes => panel.release.is_some() || !panel.info.is_empty(),
        PanelSection::Manual => panel.manual_first_page.is_some(),
    };

    // A aba ativa transborda? (só "informações", com muitos campos,
    // realisticamente) — se sim, o `scroll` do caller pula blocos dentro
    // dela; as outras seções têm um bloco só.
    let total_h: i32 = blocks.iter().map(|b| panel_block_height(inner_w, b)).sum();
    let scrollable = cy + total_h > limit;

    let scroll_btn_h = btn_h;
    let start = if scrollable {
        panel.scroll.min(blocks.len().saturating_sub(1))
    } else {
        0
    };

    // Plan revision ("eu queria aqueles 3 botões de organização das infos
    // do game na tela grande também"): os saltos ficam SEMPRE visíveis com
    // jogo focused — em telas grandes o conteúdo costuma caber sem rolagem
    // e antes os botões nem apareciam. Sem transbordo o salto é no-op
    // (tudo já está na tela). Como a fileira de baixo das antigas setas
    // sumiu, o corpo ganha o espaço dela de volta.
    let gap = 6i32;
    let half = (inner_w as i32 - gap) / 2;
    let active = panel.section;
    // Quatro abas em duas fileiras: informações | capa traseira, depois
    // manual | disco (a aba "cartucho" do SNES virou "disco" — o PSX não
    // tem cartucho).
    let tabs = [
        (
            ShelfButton::PanelJump(PanelSection::Informacoes),
            "informações",
            active == PanelSection::Informacoes || has(PanelSection::Informacoes),
        ),
        (
            ShelfButton::PanelJump(PanelSection::CapaTraseira),
            "capa traseira",
            active == PanelSection::CapaTraseira || has(PanelSection::CapaTraseira),
        ),
        (
            ShelfButton::PanelJump(PanelSection::Manual),
            "manual",
            active == PanelSection::Manual || has(PanelSection::Manual),
        ),
        (
            ShelfButton::PanelJump(PanelSection::Cartucho),
            "disco",
            active == PanelSection::Cartucho || has(PanelSection::Cartucho),
        ),
    ];
    // Respiro entre a logo/título e a fileira de botões: sem ele os botões
    // nascem colados na borda de baixo da logo (plan revision: "baixar um
    // pouco para os botões não ficarem sobrepostos com a logo").
    let mut body_top = cy + 10;
    for (n, (btn, label, lit)) in tabs.iter().enumerate() {
        let row = (n / 2) as i32;
        let col = (n % 2) as i32;
        let r = Rect::new(
            x + col * (half + gap),
            body_top + row * (scroll_btn_h + 8),
            half as u32,
            scroll_btn_h as u32,
        );
        buttons.push((*btn, draw_button(canvas, font, r, label, *lit)));
    }
    body_top += (scroll_btn_h + 8) * 2;

    let mut cy = body_top;
    if blocks.is_empty() {
        // Aba escolhida sem conteúdo (não tem a arte local, o DAT não
        // trouxe campos) — aviso dim em vez de corpo vazio.
        let _ = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1, PANEL_DIM),
            empty_label,
        );
    }
    for block in &blocks[start..] {
        let h = panel_block_height(inner_w, block);
        if cy + h > limit {
            break;
        }
        let block_top = cy;
        cy = draw_panel_block(canvas, font, images, x, cy, inner_w, block);
        // The back cover is clickable (plan revision: "ao clicar no back
        // cover possibilitar mostrar em tamanho maior, com botão de
        // fechar") — outline it as the affordance and hand the hit rect up.
        if let (PanelBlock::Image(id, _), Some(bc)) = (block, panel.backcover_img) {
            if *id == bc {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                // Borda preta (plan revision: "a borda do back cover coloque
                // em preto") — discreta sobre o painel claro.
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Backcover, hit));
            }
        }
        // O cartucho ganha o mesmo tratamento do back cover (plan revision:
        // "colocar zoom igual ao backcover no cartucho").
        if let (PanelBlock::Image(id, _), Some(cc)) = (block, panel.cartridge_img) {
            if *id == cc {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Cartridge, hit));
            }
        }
        // E a primeira página do manual (plan revision: "ao clicar abrir
        // nele visualização") — abre o leitor de páginas.
        if let (PanelBlock::Image(id, _), Some(mp)) = (block, panel.manual_first_page) {
            if *id == mp {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Manual, hit));
            }
        }
    }

    // (sem fileira de setas embaixo — os saltos nomeados acima cobrem a
    // navegação do corpo)

    buttons
}

/// One scrollable piece of the shelf panel's body (plan revision: "criar
/// rolagem no painel quando necessario") — see `draw_shelf_panel`.
/// `Release` stays its own variant rather than folding into `Field`
/// because it draws label and value on the *same* line (a small
/// headline stat), unlike `Field`'s stacked label-then-value.
enum PanelBlock<'a> {
    Image(u64, u32),
    Release(&'a str),
    /// label/valor com a medalha de prêmio à esquerda do valor — a linha
    /// "conquistas" quando o servidor concedeu um prêmio ao jogo.
    RaField(&'a str, &'a str, u64),
    Field(&'a str, &'a str),
}

/// `block`'s height in pixels, gap included — must match `draw_panel_block`
/// exactly, or "does it fit" and "how tall did it draw" disagree (the bug
/// `draw_shelf_panel`'s own history note above the `info` loop describes).
fn panel_block_height(inner_w: u32, block: &PanelBlock) -> i32 {
    match block {
        PanelBlock::Image(_, h) => 8 + *h as i32,
        PanelBlock::Release(_) => 8 + GLYPH_H as i32,
        PanelBlock::RaField(label, value, _) => {
            10 + wrapped_height(inner_w, 1, label) + wrapped_height(inner_w, 1, value)
        }
        PanelBlock::Field(label, value) => {
            10 + wrapped_height(inner_w, 1, label) + wrapped_height(inner_w, 1, value)
        }
    }
}

/// Draw `block` at `cy`, returning the y just past it.
fn draw_panel_block(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    x: i32,
    cy: i32,
    inner_w: u32,
    block: &PanelBlock,
) -> i32 {
    match block {
        PanelBlock::Image(id, h) => {
            let cy = cy + 8;
            draw_image_absolute(canvas, images, *id, x, cy, inner_w, *h);
            cy + *h as i32
        }
        PanelBlock::Release(value) => {
            let cy = cy + 8;
            draw_text_absolute(
                canvas,
                font,
                x,
                cy,
                TextStyle::new(1, PANEL_DIM),
                "lançamento",
                usize::MAX,
            );
            draw_text_absolute(
                canvas,
                font,
                x + (GLYPH_W * 11) as i32,
                cy,
                TextStyle::new(1, PANEL_TEXT),
                value,
                usize::MAX,
            );
            cy + GLYPH_H as i32
        }
        PanelBlock::RaField(label, value, medal) => {
            let cy = cy + 10;
            let cy = draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_DIM),
                label,
            );
            // Número primeiro, medalha logo depois — a fonte é monoespaçada
            // (GLYPH_W por caractere no size 1), então o fim do texto é
            // conhecido sem medição.
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_TEXT),
                value,
            );
            let text_w = value.chars().count() as i32 * GLYPH_W as i32;
            draw_image_absolute(
                canvas,
                images,
                *medal,
                x + text_w + AWARD_MEDAL_GAP,
                cy + (GLYPH_H as i32 - AWARD_MEDAL_H) / 2,
                AWARD_MEDAL_W as u32,
                AWARD_MEDAL_H as u32,
            );
            cy + GLYPH_H as i32
        }
        PanelBlock::Field(label, value) => {
            let cy = cy + 10;
            let cy = draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_DIM),
                label,
            );
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_TEXT),
                value,
            )
        }
    }
}

/// Draw one clickable panel button: a filled box (brighter/bordered when
/// `lit`, flush with the panel background otherwise) with `text` centered
/// inside — clipped to fit the box, with a trailing `...` if it doesn't
/// (plan revision: `text` used to just draw past the box's edge unclipped
/// when too long for it, visually merging into whatever sat next to it —
/// harmless while every label here was short, a real problem once the
/// Cheats modal started passing database-length descriptions through).
/// Returns the box's rect for hit-testing.
fn draw_button(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    rect: Rect,
    text: &str,
    lit: bool,
) -> Rect {
    let (bg, border, fg) = if lit {
        (PANEL_BTN_BG, PANEL_TEXT, PANEL_TEXT)
    } else {
        (PANEL_BG, PANEL_BTN_BG, PANEL_DIM)
    };
    canvas.set_draw_color(Color::RGB(border.0, border.1, border.2));
    let _ = canvas.fill_rect(rect);
    let inset = Rect::new(
        rect.x() + 1,
        rect.y() + 1,
        rect.width() - 2,
        rect.height() - 2,
    );
    canvas.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
    let _ = canvas.fill_rect(inset);

    let side_pad = 6i32;
    let max_chars = (((rect.width() as i32 - side_pad * 2) / GLYPH_W as i32).max(1)) as usize;
    let shown = clip_label(text, max_chars);
    let text_w = (GLYPH_W as i32) * shown.chars().count() as i32;
    let tx = rect.x() + (rect.width() as i32 - text_w).max(4) / 2;
    let ty = rect.y() + (rect.height() as i32 - GLYPH_H as i32) / 2;
    draw_text_absolute(
        canvas,
        font,
        tx,
        ty,
        TextStyle::new(1, fg),
        &shown,
        usize::MAX,
    );
    rect
}

/// Truncate `text` to at most `max_chars`, replacing the last one with an
/// ellipsis when it doesn't fit — plain `...` rather than `…` since the
/// font atlas only covers Basic Latin through Latin-1 Supplement (see
/// `GLYPH_FIRST`/`GLYPH_LAST`), which that single-character ellipsis falls
/// outside of. A no-op (returns `text` unchanged, borrowed) when it
/// already fits.
fn clip_label(text: &str, max_chars: usize) -> Cow<'_, str> {
    if text.chars().count() <= max_chars {
        return Cow::Borrowed(text);
    }
    if max_chars <= 3 {
        // No room for both content and "...": just hard-crop.
        return Cow::Owned(text.chars().take(max_chars).collect());
    }
    let keep: String = text.chars().take(max_chars - 3).collect();
    Cow::Owned(format!("{keep}..."))
}

/// Where the pause book's two pages sit: a symmetric spread with a spine gap
/// between them, margins all round. Not tied to the tube's geometry — this
/// screen replaces the whole window (plan §3.2/§3.4).
fn pause_pages(out_w: u32, out_h: u32) -> (Rect, Rect) {
    let margin = (out_w as f32 * PAUSE_MARGIN) as i32;
    let gap = (out_w as f32 * PAUSE_GAP) as i32;
    let top = (out_h as f32 * PAUSE_TOP) as i32;
    let page_w = ((out_w as i32 - 2 * margin - gap) / 2).max(32) as u32;
    let page_h = (out_h as i32 - 2 * top).max(32) as u32;
    let left = Rect::new(margin, top, page_w, page_h);
    let right = Rect::new(margin + page_w as i32 + gap, top, page_w, page_h);
    (left, right)
}

/// Draw the pause book: left page is the notebook's status (how many pages
/// captured so far, or a hint if there are none), right page is the most
/// recent capture, full-size, plus two clickable buttons below the pages —
/// "Continuar" (resume) and "Avancar quadro" (step one frame, plan revision:
/// mouse/gamepad only, no keyboard). `None` (shouldn't happen — always set
/// before `present_pause`/`capture_pause_bmp` are called) draws nothing and
/// returns no buttons.
fn draw_pause_book(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    pause: Option<&PauseNote>,
    out_w: u32,
    out_h: u32,
) -> Vec<(PanelButton, Rect)> {
    canvas.set_draw_color(Color::RGB(CABINET.0, CABINET.1, CABINET.2));
    let _ = canvas.fill_rect(Rect::new(0, 0, out_w, out_h));
    let Some(pause) = pause else {
        return Vec::new();
    };

    let (left, right) = pause_pages(out_w, out_h);
    for page in [left, right] {
        canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
        let _ = canvas.fill_rect(page);
    }

    let pad = 24i32;
    let lx = left.x() + pad;
    let ly = left.y() + pad;
    let lw = left.width().saturating_sub(pad as u32 * 2);
    let btn_h = (GLYPH_H + 10) as i32;
    let mut buttons = Vec::new();

    let mut cy = draw_text_wrapped_absolute(
        canvas,
        font,
        lx,
        ly,
        lw,
        TextStyle::new(2, PANEL_TEXT),
        &pause.title,
    );
    cy += 16;

    if let Some(draft) = &pause.draft {
        // Editing mode (plan revision: either the free-text note or a
        // slot's caption, `draft_heading` says which): the rest of the left
        // page becomes a live text editor — Continuar/Avancar quadro (below
        // both pages) and the cheats list are hidden until the draft is
        // saved/cancelled, so there's nothing to accidentally lose by
        // clicking.
        draw_text_absolute(
            canvas,
            font,
            lx,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &pause.draft_heading,
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 8;
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            lx,
            cy,
            lw,
            TextStyle::new(1, PANEL_TEXT),
            &format!("{draft}_"),
        );
        cy += 8;
        draw_text_absolute(
            canvas,
            font,
            lx,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &format!("{}/{}", draft.chars().count(), pause.draft_limit),
            usize::MAX,
        );

        let cancel_y = left.bottom() - pad - btn_h;
        let save_y = cancel_y - btn_h - 6;
        let save_btn = Rect::new(lx, save_y, lw, btn_h as u32);
        let cancel_btn = Rect::new(lx, cancel_y, lw, btn_h as u32);
        buttons.push((
            PanelButton::PauseDraftSave,
            draw_button(canvas, font, save_btn, "Salvar", true),
        ));
        buttons.push((
            PanelButton::PauseDraftCancel,
            draw_button(canvas, font, cancel_btn, "Cancelar", true),
        ));
    } else {
        // Left page: a text-note slot viewer (plan revision — mirrors the
        // right page's photo album layout almost exactly, bottom-up: pin/
        // delete row, write row, nav row, then the info line, then
        // whatever's left for the content itself).
        draw_text_absolute(
            canvas,
            font,
            lx,
            cy,
            TextStyle::new(1, PANEL_DIM),
            "anotações",
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 8;

        let pin_del_y = left.bottom() - pad - btn_h;
        let write_y = pin_del_y - 6 - btn_h;
        let nav_y = write_y - 6 - btn_h;
        let info_y = nav_y - 6 - (GLYPH_H as i32 + 6);

        match &pause.text_content {
            Some(text) => {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    lx,
                    cy,
                    lw,
                    TextStyle::new(1, PANEL_TEXT),
                    text,
                );
            }
            None => {
                draw_text_absolute(
                    canvas,
                    font,
                    lx,
                    cy,
                    TextStyle::new(1, PANEL_DIM),
                    "slot vazio",
                    usize::MAX,
                );
            }
        }

        let info = format!(
            "{}/{}{}",
            pause.text_page + 1,
            pause.captures,
            if pause.text_pinned { " (fixado)" } else { "" }
        );
        draw_text_absolute(
            canvas,
            font,
            lx,
            info_y,
            TextStyle::new(1, PANEL_DIM),
            &info,
            usize::MAX,
        );

        let half = (lw / 2).saturating_sub(4);
        let prev_btn = Rect::new(lx, nav_y, half, btn_h as u32);
        let next_btn = Rect::new(lx + half as i32 + 8, nav_y, half, btn_h as u32);
        buttons.push((
            PanelButton::PauseTextPrev,
            draw_button(canvas, font, prev_btn, "< anterior", pause.text_page > 0),
        ));
        buttons.push((
            PanelButton::PauseTextNext,
            draw_button(
                canvas,
                font,
                next_btn,
                "próxima >",
                pause.text_page + 1 < pause.captures,
            ),
        ));

        let write_btn = Rect::new(lx, write_y, lw, btn_h as u32);
        buttons.push((
            PanelButton::PauseWrite,
            draw_button(
                canvas,
                font,
                write_btn,
                if pause.text_content.is_some() {
                    "Editar"
                } else {
                    "Escrever"
                },
                !pause.text_pinned,
            ),
        ));

        let pin_btn = Rect::new(lx, pin_del_y, half, btn_h as u32);
        let del_btn = Rect::new(lx + half as i32 + 8, pin_del_y, half, btn_h as u32);
        buttons.push((
            PanelButton::PauseTextPin,
            draw_button(
                canvas,
                font,
                pin_btn,
                if pause.text_pinned { "Fixado" } else { "Fixar" },
                true,
            ),
        ));
        buttons.push((
            PanelButton::PauseTextDelete,
            draw_button(
                canvas,
                font,
                del_btn,
                "Apagar",
                pause.text_content.is_some() && !pause.text_pinned,
            ),
        ));
    }

    // Right page: the selected slot's image, a pin/caption row, and
    // pagination through all 15 (plan revision — used to always show just
    // the latest, no pinning or naming).
    let rx = right.x() + pad;
    let ry = right.y() + pad;
    let rw = right.width().saturating_sub(pad as u32 * 2);
    // Bottom-up: pin/rename row, then Prev/Next, then the info line(s) —
    // always reserved (whether or not there's a caption) so the image's
    // height doesn't jump around as captions come and go.
    let pin_y = right.bottom() - pad - btn_h;
    let nav_y = pin_y - 6 - btn_h;
    let label_y = nav_y - 6 - (GLYPH_H as i32 + 4);
    let info_y = label_y - (GLYPH_H as i32 + 6);
    let img_h = (info_y - ry).max(0) as u32;

    if pause.has_thumb {
        draw_image_absolute(canvas, images, PAUSE_THUMB_IMG, rx, ry, rw, img_h);
    } else {
        draw_text_absolute(
            canvas,
            font,
            rx,
            ry,
            TextStyle::new(1, PANEL_DIM),
            "slot vazio",
            usize::MAX,
        );
    }

    let info = format!(
        "{}/{}{}",
        pause.page + 1,
        pause.captures,
        if pause.pinned { " (fixado)" } else { "" }
    );
    draw_text_absolute(
        canvas,
        font,
        rx,
        info_y,
        TextStyle::new(1, PANEL_DIM),
        &info,
        usize::MAX,
    );
    if !pause.slot_label.is_empty() {
        draw_text_absolute(
            canvas,
            font,
            rx,
            label_y,
            TextStyle::new(1, PANEL_TEXT),
            &pause.slot_label,
            usize::MAX,
        );
    }

    let half = (rw / 2).saturating_sub(4);
    let prev_btn = Rect::new(rx, nav_y, half, btn_h as u32);
    let next_btn = Rect::new(rx + half as i32 + 8, nav_y, half, btn_h as u32);
    buttons.push((
        PanelButton::PauseNotePrev,
        draw_button(canvas, font, prev_btn, "< anterior", pause.page > 0),
    ));
    buttons.push((
        PanelButton::PauseNoteNext,
        draw_button(
            canvas,
            font,
            next_btn,
            "próxima >",
            pause.page + 1 < pause.captures,
        ),
    ));

    let pin_btn = Rect::new(rx, pin_y, half, btn_h as u32);
    let name_btn = Rect::new(rx + half as i32 + 8, pin_y, half, btn_h as u32);
    buttons.push((
        PanelButton::PauseNotePin,
        draw_button(
            canvas,
            font,
            pin_btn,
            if pause.pinned { "Fixado" } else { "Fixar" },
            true,
        ),
    ));
    buttons.push((
        PanelButton::PauseNoteName,
        draw_button(
            canvas,
            font,
            name_btn,
            if pause.slot_label.is_empty() {
                "Nomear print"
            } else {
                "Renomear"
            },
            true,
        ),
    ));

    // Button below both pages, in the same margin band above them
    // (`pause_pages` leaves a symmetric top/bottom gap of `PAUSE_TOP`) —
    // hidden while writing (commit/cancel the draft first). Spans both pages
    // (plan revision: used to split this band with "Avancar quadro", a
    // dev-only frame-step nobody reached through the actual UI — removed,
    // so the one real action left gets the whole width).
    if pause.draft.is_none() {
        let band = (out_h as f32 * PAUSE_TOP) as i32;
        let bottom_btn_h = (GLYPH_H + 12) as i32;
        let btn_y = out_h as i32 - band + (band - bottom_btn_h).max(0) / 2;
        let continue_btn = Rect::new(
            left.x(),
            btn_y,
            (right.right() - left.x()).max(0) as u32,
            bottom_btn_h as u32,
        );
        buttons.push((
            PanelButton::PauseContinue,
            draw_button(canvas, font, continue_btn, "Continuar", true),
        ));
    }

    buttons
}

/// Draw the modal dialog: a single centered card on a plain background,
/// replacing the whole window like the pause book does (plan revision) but
/// much lighter — a title, then either a grid of clickable slot rows (two
/// columns once there are more than a handful, so the 15 print slots don't
/// make the card comically tall) plus a Cancel button, or — once a print's
/// slot is picked — a text field (name it) with Confirm/Cancel. `None`
/// (shouldn't happen — always set before `present_modal`/`capture_modal_bmp`
/// are called) draws nothing and returns no buttons.
fn draw_modal(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    modal: Option<&ModalInfo>,
    out_w: u32,
    out_h: u32,
) -> Vec<(PanelButton, Rect)> {
    canvas.set_draw_color(Color::RGB(CABINET.0, CABINET.1, CABINET.2));
    let _ = canvas.fill_rect(Rect::new(0, 0, out_w, out_h));
    let Some(modal) = modal else {
        return Vec::new();
    };

    let pad = 24i32;
    let btn_h = (GLYPH_H + 10) as i32;
    let row_h = (GLYPH_H + 6) as i32;
    let whole = Rect::new(0, 0, out_w, out_h);
    let mut buttons = Vec::new();

    // Naming step: the row grid is replaced by a text field, same card width
    // either way so the dialog doesn't visibly jump size between steps.
    let card_w = 460u32.min(out_w.saturating_sub(80));
    if let Some(draft) = &modal.draft {
        let card_h = pad as u32 * 2 + GLYPH_H + 8 + GLYPH_H * 2 + 12 + btn_h as u32;
        let card = centered_in(whole, card_w, card_h);
        canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
        let _ = canvas.fill_rect(card);

        let x = card.x() + pad;
        let inner_w = card.width().saturating_sub(pad as u32 * 2);
        let mut cy = card.y() + pad;
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1, PANEL_DIM),
            &modal.draft_heading,
        );
        cy += 8;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(2, PANEL_TEXT),
            draft,
            usize::MAX,
        );
        let counter = format!("{}/{}", draft.chars().count(), modal.draft_limit);
        let counter_w = (GLYPH_W as i32) * counter.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            card.right() - pad - counter_w,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &counter,
            usize::MAX,
        );
        cy += GLYPH_H as i32 * 2 + 12;

        let gap = 12i32;
        let half_w = ((inner_w as i32 - gap) / 2).max(1) as u32;
        let confirm = Rect::new(x, cy, half_w, btn_h as u32);
        let cancel = Rect::new(x + half_w as i32 + gap, cy, half_w, btn_h as u32);
        buttons.push((
            PanelButton::ModalConfirm,
            draw_button(canvas, font, confirm, "Salvar", true),
        ));
        buttons.push((
            PanelButton::ModalCancel,
            draw_button(canvas, font, cancel, "Cancelar", true),
        ));
        return buttons;
    }

    // Row-picking step. A search filter (plan revision — only Cheats sets
    // `searchable`) narrows which of `modal.rows` take part below; each
    // survivor keeps its *original* index (`ModalSlot(i)` has to, since
    // that index is what the click handler uses to look the row back up
    // in `cheat_defs`/`cheat_state`) — same "iterate everything, skip what
    // doesn't apply this frame" shape the scroll clipping below already
    // uses, just one more skip condition.
    let query = modal.search_query.trim();
    let visible: Vec<usize> = modal
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| query.is_empty() || contains_ignore_ascii_case(&row.label, query))
        .filter(|(_, row)| match modal.cheat_filter {
            None => true,
            Some(want_on) => row_checked(&row.label) == Some(want_on),
        })
        .map(|(i, _)| i)
        .collect();

    let cols = if visible.len() > 8 { 2usize } else { 1usize };
    let rows_per_col = visible.len().div_ceil(cols).max(1);
    let col_gap = 16i32;
    // Wide enough for the longest label in the *unfiltered* list (so
    // narrowing a search doesn't make the card leap wider/narrower every
    // keystroke), but never so wide it'd spill past the window — whatever
    // doesn't fit even then still clips with an ellipsis (`draw_button`),
    // this is just about not truncating the common case. Floors at 200 to
    // keep Save/Load/Print (short "Slot N" labels) exactly as wide as
    // before this revision.
    let longest_label = modal
        .rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0) as u32;
    let desired_col_w = (longest_label + 2) * GLYPH_W + 12;
    let max_col_w = ((out_w.saturating_sub(160))
        .saturating_sub(col_gap as u32 * (cols as u32 - 1))
        / cols as u32)
        .max(120);
    let col_w = desired_col_w.max(200).min(max_col_w);
    let card_w =
        card_w.max(pad as u32 * 2 + col_w * cols as u32 + col_gap as u32 * (cols as u32 - 1));
    let card_w = card_w.min(out_w.saturating_sub(40));

    // How many rows fit without any scroll UI eating into the budget —
    // matches every dialog that's had room to just show everything so far
    // (save/load's 10, print's 15). Only once a list (the Cheats modal, for
    // a database-heavy game) overflows even a generous card does scrolling
    // — and the two extra rows it costs — enter the picture at all.
    // Two extra rows on a searchable modal: the search box, and the
    // on/off state filter (plan revision) right below it.
    let search_h = if modal.searchable {
        ((btn_h + 8) * 2) as u32
    } else {
        0
    };
    let max_card_h = out_h.saturating_sub(40);
    let fixed_overhead = pad as u32 * 2 + GLYPH_H + 16 + search_h + 12 + btn_h as u32;
    let budget_no_scroll = max_card_h.saturating_sub(fixed_overhead) as i32;
    let rows_that_fit = (budget_no_scroll / row_h).max(1) as usize;
    let scrollable = !visible.is_empty() && rows_per_col > rows_that_fit;
    let visible_rows = if scrollable {
        let scroll_ui_h = (btn_h + 8) * 2;
        let budget = budget_no_scroll - scroll_ui_h;
        (budget / row_h).max(1) as usize
    } else {
        rows_per_col
    };
    let scroll = if scrollable {
        modal.scroll.min(rows_per_col - visible_rows)
    } else {
        0
    };

    let extra_h = if scrollable {
        ((btn_h + 8) * 2) as u32
    } else {
        0
    };
    let card_h = fixed_overhead + extra_h + visible_rows as u32 * row_h as u32;
    let card = centered_in(whole, card_w, card_h.min(max_card_h));
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(card);

    let x = card.x() + pad;
    let inner_w = card.width().saturating_sub(pad as u32 * 2);
    let mut cy = card.y() + pad;
    draw_text_absolute(
        canvas,
        font,
        x,
        cy,
        TextStyle::new(2, PANEL_TEXT),
        &modal.title,
        usize::MAX,
    );
    let filtering_active = modal.searchable && (!query.is_empty() || modal.cheat_filter.is_some());
    if scrollable || filtering_active {
        // Same "counter next to a scale-2 heading" placement the naming
        // step's char counter already uses, just against the title instead
        // of a draft's text — no extra row spent on it. Reflects the
        // *filtered* count when a search and/or the on/off filter is
        // active, so "3/1209" (say) reads as "these 3 are what matched",
        // not a bound on the game's real cheat count.
        let counter = if visible.is_empty() {
            "0".to_string()
        } else {
            format!(
                "{}-{}/{}",
                scroll + 1,
                (scroll + visible_rows).min(rows_per_col),
                rows_per_col
            )
        };
        let counter_w = (GLYPH_W as i32) * counter.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            card.right() - pad - counter_w,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &counter,
            usize::MAX,
        );
    }
    cy += GLYPH_H as i32 + 16;

    if modal.searchable {
        let label = if modal.search_query.is_empty() {
            "Buscar...".to_string()
        } else {
            format!("Buscar: \"{}\"", modal.search_query)
        };
        let search_box = Rect::new(x, cy, inner_w, btn_h as u32);
        buttons.push((
            PanelButton::ModalSearchStart,
            draw_button(canvas, font, search_box, &label, true),
        ));
        cy += btn_h + 8;

        // On/off state filter (plan revision): a three-way segmented
        // control, whichever matches `modal.cheat_filter` drawn `lit`
        // (highlighted) — clicking either of the other two switches to it,
        // clicking the active one is a harmless no-op (same "not guarded,
        // the click just re-applies the same state" shape `Fixar`/`Fixado`
        // already has for a print slot).
        let seg_gap = 8i32;
        let seg_w = ((inner_w as i32 - seg_gap * 2) / 3).max(1) as u32;
        let all_btn = Rect::new(x, cy, seg_w, btn_h as u32);
        let on_btn = Rect::new(x + (seg_w as i32 + seg_gap), cy, seg_w, btn_h as u32);
        let off_btn = Rect::new(x + (seg_w as i32 + seg_gap) * 2, cy, seg_w, btn_h as u32);
        buttons.push((
            PanelButton::ModalFilterAll,
            draw_button(canvas, font, all_btn, "Todos", modal.cheat_filter.is_none()),
        ));
        buttons.push((
            PanelButton::ModalFilterOn,
            draw_button(
                canvas,
                font,
                on_btn,
                "Ligados",
                modal.cheat_filter == Some(true),
            ),
        ));
        buttons.push((
            PanelButton::ModalFilterOff,
            draw_button(
                canvas,
                font,
                off_btn,
                "Desligados",
                modal.cheat_filter == Some(false),
            ),
        ));
        cy += btn_h + 8;
    }

    if visible.is_empty() {
        // Reachable with a search and/or the on/off filter active and
        // nothing matching — opening the modal at all already requires a
        // non-empty row list, so an *unfiltered* empty view can't happen.
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1, PANEL_DIM),
            "nenhum cheat encontrado",
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 12;
    } else {
        if scrollable {
            let up = Rect::new(x, cy, inner_w, btn_h as u32);
            buttons.push((
                PanelButton::ModalScrollUp,
                draw_button(canvas, font, up, "^ Cima", scroll > 0),
            ));
            cy += btn_h + 8;
        }

        let rows_top = cy;
        for (pos, &i) in visible.iter().enumerate() {
            let col = pos / rows_per_col;
            let row_in_col = pos % rows_per_col;
            if row_in_col < scroll || row_in_col >= scroll + visible_rows {
                continue; // scrolled out of view this frame
            }
            let row = &modal.rows[i];
            let rx = x + col as i32 * (col_w as i32 + col_gap);
            let ry = rows_top + (row_in_col - scroll) as i32 * row_h;
            let rect = Rect::new(rx, ry, col_w, (row_h - 4) as u32);
            buttons.push((
                PanelButton::ModalSlot(i as u16),
                draw_button(canvas, font, rect, &row.label, row.enabled),
            ));
        }
        cy = rows_top + visible_rows as i32 * row_h;

        if scrollable {
            let down = Rect::new(x, cy, inner_w, btn_h as u32);
            buttons.push((
                PanelButton::ModalScrollDown,
                draw_button(
                    canvas,
                    font,
                    down,
                    "v Baixo",
                    scroll + visible_rows < rows_per_col,
                ),
            ));
            cy += btn_h + 8;
        }
    }
    cy += 12;

    let cancel = Rect::new(x, cy, inner_w, btn_h as u32);
    buttons.push((
        PanelButton::ModalCancel,
        draw_button(canvas, font, cancel, "Cancelar", true),
    ));

    buttons
}

/// Case-insensitive ASCII substring search — used to filter the Cheats
/// modal's rows by the player's typed query. Byte-level, no allocation
/// (contrast `str::to_lowercase`, which would allocate a fresh copy of
/// every row's label on every frame the modal is open): fine for this
/// database's plain-ASCII descriptions, same as `RomId`'s own comparisons
/// elsewhere in the app.
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    n.is_empty() || h.len() >= n.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// Whether a Cheats modal row's label reads as checked on/off, from the
/// `[x] `/`[ ] ` prefix `cheat_modal_rows` (the one and only place that
/// builds these labels) always puts in front of the description —
/// `None` for a row with neither prefix (every other modal's rows: "Slot
/// N", never checkbox-shaped). Reused for the on/off state filter instead
/// of threading a second parallel array through `set_modal` just for it.
fn row_checked(label: &str) -> Option<bool> {
    if label.starts_with("[x] ") {
        Some(true)
    } else if label.starts_with("[ ] ") {
        Some(false)
    } else {
        None
    }
}

/// Build a textured grid over `dst` whose vertex positions are barrel-distorted
/// (edges bow out, corners pull in) with an edge vignette baked into the vertex
/// colours. Texture coordinates stay a plain grid, so the picture — not just the
/// outline — curves. `alpha` (1.0 normally) lets a caller fade the whole tube in.
fn build_crt_mesh(dst: Rect, alpha: f32) -> CrtMesh {
    let n = CRT_GRID;
    let (ox, oy) = (dst.x() as f32, dst.y() as f32);
    let (dw, dh) = (dst.width() as f32, dst.height() as f32);

    let mut verts = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let u = i as f32 / n as f32; // 0..1 texture / grid coord
            let v = j as f32 / n as f32;
            let cx = u * 2.0 - 1.0; // -1..1
            let cy = v * 2.0 - 1.0;

            // Barrel: corners move toward the centre, mid-edges stay put.
            let dx = cx * (1.0 - CRT_WARP * cy * cy);
            let dy = cy * (1.0 - CRT_WARP * cx * cx);

            let px = ox + (dx * 0.5 + 0.5) * dw;
            let py = oy + (dy * 0.5 + 0.5) * dh;

            let r2 = (cx * cx + cy * cy).min(2.0) / 2.0;
            let shade = (1.0 - CRT_VIGNETTE * r2 * r2).clamp(0.0, 1.0);

            verts.push(Vertex {
                position: sdl3::render::FPoint::new(px, py),
                color: FColor::RGBA(shade, shade, shade, alpha),
                tex_coord: sdl3::render::FPoint::new(u, v),
            });
        }
    }

    let stride = (n + 1) as i32;
    let mut indices = Vec::with_capacity(n * n * 6);
    for j in 0..n as i32 {
        for i in 0..n as i32 {
            let a = j * stride + i;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    CrtMesh {
        verts,
        indices,
        w: dst.width(),
        h: dst.height(),
    }
}

/// Line-count math shared by [`Cabinet::text_wrapped`]'s layout and
/// [`Cabinet::wrapped_height`]. Mirrors the wrap loop: greedy word packing into
/// `cols` chars, hard-splitting any word longer than a line.
fn wrapped_height(max_w: u32, scale: u32, s: &str) -> i32 {
    let cols = (max_w / (GLYPH_W * scale)).max(1) as usize;
    let row = (GLYPH_H * scale) as i32;
    let mut lines = 0i32;
    let mut len = 0usize;
    let mut open = false;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if open && len + 1 + word_len > cols {
            lines += 1;
            len = 0;
            open = false;
        }
        if open {
            len += 1;
        }
        len += word_len;
        open = true;
        while len > cols {
            lines += 1;
            len -= cols;
        }
    }
    if open {
        lines += 1;
    }
    lines * (row + 2)
}

/// Rasterize the covered Unicode range once into one wide texture strip —
/// each cell holds one anti-aliased glyph (alpha = the crate's per-pixel
/// intensity, RGB left white so every draw call tints it via
/// `set_color_mod`), left-aligned in its `GLYPH_W`-wide cell exactly like
/// the atlas the old bitmap font used, just with grayscale edges instead of
/// hard 1-bit ones.
fn build_font_atlas(canvas: &mut WindowCanvas) -> Result<Texture, PlatformError> {
    let w = GLYPH_COLS * GLYPH_W;
    let h = GLYPH_H;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for i in 0..GLYPH_COLS {
        let Some(ch) = char::from_u32(GLYPH_FIRST + i) else {
            continue;
        };
        let Some(glyph) = get_raster(ch, FONT_WEIGHT, FONT_HEIGHT) else {
            continue;
        };
        for (row, line) in glyph.raster().iter().enumerate().take(h as usize) {
            for (col, &v) in line.iter().enumerate().take(GLYPH_W as usize) {
                if v == 0 {
                    continue;
                }
                let px = i * GLYPH_W + col as u32;
                let o = ((row as u32 * w + px) * 4) as usize;
                rgba[o..o + 4].copy_from_slice(&[255, 255, 255, v]);
            }
        }
    }
    let mut tex = canvas
        .create_texture_static(SdlFormat::RGBA32, w, h)
        .map_err(|e| PlatformError::Sdl(e.to_string()))?;
    tex.update(None, &rgba, (w * 4) as usize)
        .map_err(|e| PlatformError::Sdl(e.to_string()))?;
    tex.set_blend_mode(BlendMode::Blend);
    tex.set_scale_mode(SdlScaleMode::Nearest);
    Ok(tex)
}

#[cfg(test)]
mod tests {
    use super::{fit_aspect_in, ra_badge_rows, screen_area, wrapped_height, GLYPH_H, OSD_GREEN};
    use sdl3::rect::Rect;

    #[test]
    fn ra_badge_rows_pin_the_ativado_contract() {
        let hardcore = ra_badge_rows(true);
        let softcore = ra_badge_rows(false);
        // A primeira linha é sempre o "ativado" verde do OSD; o modo muda
        // de texto e de cor (âmbar no hardcore, cinza claro no softcore).
        assert_eq!(hardcore[0], ("RA ATIVADO", OSD_GREEN));
        assert_eq!(softcore[0], ("RA ATIVADO", OSD_GREEN));
        assert_eq!(hardcore[1].0, "HARDCORE");
        assert_eq!(softcore[1].0, "SOFTCORE");
        assert_ne!(hardcore[1].1, softcore[1].1);
    }

    #[test]
    fn content_bbox_finds_the_opaque_region() {
        // 4x3 image: only the middle row's middle two pixels are opaque.
        let mut rgba = vec![0u8; 4 * 3 * 4];
        for (x, y) in [(1usize, 1usize), (2, 1)] {
            rgba[(y * 4 + x) * 4 + 3] = 255;
        }
        assert_eq!(super::content_bbox(4, 3, &rgba), (1, 1, 2, 1));
        // A fully opaque image gets the full canvas; a fully transparent
        // one falls back to it too (no empty box to fit).
        let solid = vec![255u8; 2 * 2 * 4];
        assert_eq!(super::content_bbox(2, 2, &solid), (0, 0, 2, 2));
        assert_eq!(super::content_bbox(2, 2, &[0u8; 2 * 2 * 4]), (0, 0, 2, 2));
    }

    #[test]
    fn screen_area_insets_with_a_wider_chin() {
        let s = screen_area(1000, 1000);
        assert_eq!(s.x(), 70); // 7% side
        assert_eq!(s.y(), 70); // 7% top
        assert_eq!(s.width(), 860); // 1000 - 2*70
        assert_eq!(s.height(), 820); // 1000 - 70 top - 110 chin
                                     // Never collapses to nothing on a tiny window.
        assert!(screen_area(4, 4).width() >= 16);
    }

    #[test]
    fn fit_aspect_in_centers_a_43_rect() {
        // A wide area: 4:3 fills the height, centered horizontally.
        let r = fit_aspect_in(Rect::new(0, 0, 800, 300), 4.0 / 3.0);
        assert_eq!(r.height(), 300);
        assert_eq!(r.width(), 400);
        assert_eq!(r.x(), 200);
        assert_eq!(r.y(), 0);
        // A tall area: 4:3 fills the width instead.
        let r = fit_aspect_in(Rect::new(0, 0, 400, 900), 4.0 / 3.0);
        assert_eq!(r.width(), 400);
        assert_eq!(r.height(), 300);
        assert_eq!(r.y(), 300);
    }

    #[test]
    fn wrapped_height_counts_lines() {
        let row = GLYPH_H as i32 + 2; // one line's advance at scale 1
        assert_eq!(wrapped_height(90, 1, ""), 0);
        // 90px / 9px advance = 10 cols. "hello world" -> "hello" + " world"
        // = 11 > 10, so two lines.
        assert_eq!(wrapped_height(90, 1, "hello world"), 2 * row);
        // Fits on one line.
        assert_eq!(wrapped_height(90, 1, "hello you"), row);
        // A single word longer than the line is hard-split.
        assert_eq!(wrapped_height(90, 1, &"x".repeat(25)), 3 * row);
    }
}

#[cfg(test)]
mod canvas_rect_tests {
    use super::cabinet_canvas_rect;

    #[test]
    fn deck_and_16x9_displays_fill_natively() {
        // Steam Deck 1280x800 (16:10): the canvas fills the display — no
        // letterbox bars (plan revision: melhor experiencia no Deck).
        assert_eq!(
            cabinet_canvas_rect(1280, 800),
            sdl3::rect::Rect::new(0, 0, 1280, 800)
        );
        // 16:9 TVs/monitors: unchanged, fills exactly.
        assert_eq!(
            cabinet_canvas_rect(1920, 1080),
            sdl3::rect::Rect::new(0, 0, 1920, 1080)
        );
        assert_eq!(
            cabinet_canvas_rect(1280, 720),
            sdl3::rect::Rect::new(0, 0, 1280, 720)
        );
    }

    #[test]
    fn ultrawide_keeps_the_16x9_letterbox() {
        // 3440x1440 (21:9): 16:9 canvas centered, side bars.
        let r = cabinet_canvas_rect(3440, 1440);
        assert_eq!((r.width(), r.height()), (2560, 1440));
        assert_eq!(r.x(), 440);
        assert_eq!(r.y(), 0);
    }

    #[test]
    fn shapes_outside_the_band_clamp_to_it() {
        // 4:3 display: clamps to 16:10 (narrower pillarbox than 16:9).
        let r = cabinet_canvas_rect(1280, 1024);
        assert_eq!((r.width(), r.height()), (1280, 800));
        assert_eq!(r.y(), 112);
        // 3:2 (Surface): clamps to 16:10 as well.
        let r = cabinet_canvas_rect(1500, 1000);
        assert_eq!((r.width(), r.height()), (1500, 938));
        assert_eq!(r.y(), 31);
    }
}
