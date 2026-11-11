//! The one window: a static dark cabinet with the screen recessed into it. Both
//! the running game and the selector draw into that screen area — the game as a
//! frame through a barrel-distorted CRT mesh, the selector as flat 2D (rects,
//! bitmap text, letterboxed images). The cabinet furniture (the chamfer ring
//! from the window edge down to the glass) is redrawn every frame so nothing
//! ever recreates the window. NTSC colour bleed is applied upstream
//! (`polystnx-ntsc`).

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

mod audio;
mod chin;
mod chrome;
mod composite;
mod geometry;
mod panel;
mod panel_art;
mod pause;
mod picture;
mod scenes;
mod setters;
mod shelf_scenes;
mod text;
mod tvset;
use self::chin::*;
use self::chrome::*;
use self::geometry::*;
use self::panel::*;
use self::panel_art::*;
use self::pause::*;
pub use self::picture::Picture;
use self::text::*;
use self::tvset::*;
pub use self::tvset::{TvControl, TvPower};

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
/// O leitor de CD real (foto recortada) — o placeholder sob o disco.
pub const CD_READER_IMG: u64 = u64::MAX - 8;
/// O vidro da tampa de acrílico (reflexos pré-renderizados com gradiente).
pub const LID_GLASS_IMG: u64 = u64::MAX - 9;
/// A mesa do spindle (onde o CD assenta) — desenhada girando com o disco.
pub const CD_SEAT_IMG: u64 = u64::MAX - 10;
/// A logo PolyStn do memory card desenhado no slot — a MESMA arte da
/// console logo, com `set_color_mod` preto na textura (o desenho do card
/// quer a marca em silhueta, não colorida).
pub const MC_LOGO_IMG: u64 = u64::MAX - 11;

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
const PSX_LED_ON: (u8, u8, u8) = (110, 224, 130);
const PSX_LED_OFF: (u8, u8, u8) = (56, 58, 54);
/// O rótulo "POWER" gravado dentro do botão — verde escuro, como na foto
/// de referência.
const PSX_BTN_TEXT: (u8, u8, u8) = (70, 235, 205);
/// O rótulo "OPEN" do botão de eject — azul escuro, mesmo tratamento.
const PSX_BTN_OPEN: (u8, u8, u8) = (125, 190, 255);

/// The set's own nameplate: a small wordmark printed into the chin, left of
/// the cartridge — a touch lighter than the cabinet plastic, like an embossed
/// badge rather than a lit label.
/// The console's own brand name — the nameplate's default text before the
/// app calls `Cabinet::set_nameplate` with its own version appended, and
/// what the idle screen's panel falls back to in plain text when there's no
/// logo image loaded (`draw_panel`'s idle branch).
pub const BRAND: &str = "PolystnX";
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

/// Pixel layout of a core framebuffer. Mirrors `polystnx_emulation::PixelFormat`
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

/// O stream do leitor de CD: posição no loop embutido e volume corrente
/// (fade de entrada/saída em passos de tick).
struct CdNoise {
    stream: AudioOut,
    pos: usize,
    level: f32,
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
    /// Ângulo do disco girando (graus) — avança por RELÓGIO, não por frame
    /// emulado: quando o core hesita (boot, troca BIOS→jogo, FMV pesada), o
    /// disco continua na velocidade certa em vez de congelar junto. Ao
    /// parar, fica no ângulo em que parou (o motor não "rebobina").
    spin: f32,
    /// Velocidade atual do giro (°/s) — rampa até 420 com o console ligado
    /// e tampa fechada, rampa até 0 quando não (inércia do motor).
    spin_speed: f32,
    /// Última marca de relógio usada pelo giro (para o delta de ângulo).
    spin_last: Option<Instant>,
    /// Mesh do caminho 2D (estante/config/idle-2d): a TELA inteira, em cache
    /// (era reconstruído por frame — 1.089 vértices + índices descartados).
    mesh_2d: Option<CrtMesh>,
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
    /// Última vez que o foley teve som na fila (ver `reap_idle_audio`).
    foley_busy_at: Instant,
    /// A TV: `None` = ligada; ver `tvset.rs`.
    tv_power: Option<TvPower>,
    /// O menu OSD de imagem aberto: o item em ajuste e o último toque.
    tv_menu: Option<(usize, Instant)>,
    /// O foco do controle e quantos quadros ele ainda vale.
    pad_focus: Option<(PadFocus, u8)>,
    /// Ajustes de imagem (neutro = quadro sem processamento).
    picture: Picture,
    composite: bool,
    /// Rascunhos do filtro de imagem, reaproveitados entre quadros.
    picture_buf: Vec<u8>,
    picture_tmp: Vec<u8>,
    /// Volume da TV, 0..=16 (o jogo multiplica a amostra por isso).
    volume: u8,
    /// Até quando o OSD de barrinhas de volume fica na tela.
    volume_osd_until: Option<Instant>,
    /// Os rects dos controles do queixo, registrados a cada frame.
    tv_power_buttons: Option<TvChinControls>,
    /// O vidro morto pronto numa textura (refeita só ao mudar o tamanho).
    dead_glass: Option<DeadGlass>,
    /// A foto do tubo no instante em que o desligar começa (a imagem que se
    /// esmaga numa linha).
    collapse_snap: Option<Texture>,
    /// O loop do leitor de CD (hesitação do core): stream dedicado com fade
    /// de entrada/saída — nunca corta o som no meio seco.
    cd_noise: Option<CdNoise>,
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
    /// every screen (plan revision: "mostrar versao do app e do PCSX Rearmed, onde
    /// esta o nome do app na tv") — starts as just `BRAND`, but the app sets
    /// it once at startup (and again after a core swap) to also carry the
    /// app/core version, via `set_nameplate`.
    nameplate: String,
    /// MOCK (design preview for the achievements notification — RetroAchievements
    /// plan): lines drawn right-aligned in the chin, mirroring `set_nameplate`'s
    /// block on the left. Throwaway scaffolding for `examples/ra_osd_mock.rs`;
    /// the real feature (phase 4) replaces it with a timed OSD queue.
    /// Per-nameplate-line "tem update" flags (plan revision: "quando tiver
    /// update do app ou do PCSX Rearmed não mostrar mais a tela cheia e sim um
    /// icone verde no nameplate do lado de cada um") — `(app, core)`. When
    /// set, `draw_brand` prints a small green dot right after that line
    /// (line 0 = app version, line 1 = PCSX Rearmed version); the app flips them
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
    /// O aviso residente do chin na abertura — "em qual binário eu estou?"
    /// (versão + renderer ativo) tem resposta na TV, não no log; caindo em
    /// software, o aviso degradado toma o lugar. `None` = prazo vencido.
    chin_notice: Option<ChinNotice>,
    /// O painel de desempenho do modo dev ("infos de desempenho" no menu
    /// dev): "~N FPS / CPU T / RAM M" no topo central da TV. Os números
    /// chegam por `set_perf_readout` (runner, 1×/s, leituras de sistema
    /// throttled); sem o painel ligado, nada é desenhado.
    perf_overlay: bool,
    perf_fps: u32,
    perf_cpu: Option<u32>,
    perf_ram_mb: Option<u32>,
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

/// What the chin draws this frame: a queued notification wins, then the
/// degraded-mode boot notice, then the persistent RA badge — which is what
/// makes the badge "cede the spot" to each unlock and come back when the
/// notification expires.
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

/// O alvo do anel de foco do controle (ver [`Cabinet::set_pad_focus`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadFocus {
    Panel(PanelButton),
    Tv(TvControl),
    Arrow(UpdateArrow),
}

/// Quadros que o anel de foco do controle vale a cada `set_pad_focus` — a
/// tela dona renova todo quadro; menos que isto = não renovado.
const PAD_FOCUS_TTL: u8 = 3;
/// A cor fósforo lavanda do anel de foco.
const FOSFORO: (u8, u8, u8) = (186, 138, 244);

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
    /// The idle screen's "Manual do console" button — opens the console's
    /// own PDF manual on the tube; the reader itself lives app-side.
    ConsoleManual,
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
    /// O botão ANALOG da entrada 1 — o botão do DualShock: liga o modo
    /// analógico E a vibração (o LED vermelho vira verde).
    Analog1,
    /// Pauses and opens the notebook in one click (plan revision — replaces
    /// the old separate "Pausar" button; "Nota" moved to `PrintScreen`).
    Notebook,
    /// Opens the achievements list modal (plan:
    /// `docs/plano-retroachievements.md`, fase 4) — drawn whenever the RA
    /// account is configured, desativado (row apagada) quando o jogo não
    /// tem set (plan revision: "mostre o botão mas mostre desativado").
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
/// O fundo sobre o qual o modal é composto (`present_modal`): a estática
/// da TV desligada (com o nível) ou o último quadro do jogo.
#[derive(Debug, Clone, Copy)]
pub enum ModalBackdrop {
    /// TV fora do ar — o nível de ruído vem do chamador (o app usa o seu
    /// `OFF_STATIC_LEVEL`).
    Static(f32),
    /// O último quadro do jogo (retido pela plataforma).
    Frame { aspect_ratio: f32 },
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
    /// A seta da linha do PCSX Rearmed — baixa e instala o core na hora.
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
    /// Ícone 16×16 RGBA do save (memory cards) — `None` nos modais de
    /// texto puro. A textura nasce em `set_modal_with_icons` e morre com o
    /// `ModalInfo` inteiro no `clear_modal`.
    icon: Option<Texture>,
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
    /// Há disco no drive (gira o disco e liga o poço do leitor).
    has_cartridge: bool,
    title: String,
    /// `(kind, label, enabled)` — `enabled: false` draws the row apagada
    /// (plan revision: "mostre o botão mas mostre desativado", o Conquistas
    /// de jogo sem set) e o clique nela continua chegando ao caller, que
    /// decide o que fazer (o runner mostra o porquê no OSD).
    commands: Vec<(PanelButton, String, bool)>,
    /// Whether the console is on right now — dims/brightens the command
    /// buttons (Eject only clunks while powered, Reset and everything past
    /// it only act while powered), and which way the Power rocker sits. Set
    /// via `Cabinet::set_powered`; starts `false` (plan revision: picking a
    /// game only inserts the cartridge, it doesn't start it — the player
    /// presses Power themselves, same as the real console).
    powered: bool,
    /// A tampa translúcida está aberta (botão OPEN) — o disco para de
    /// girar, o vidro sai de cima e o botão de remover aparece.
    lid_open: bool,
    /// Há disco no drive (some quando removido com a tampa aberta).
    disc_in: bool,
    /// Reset's rocker is momentary (plan revision): true for a short spring
    /// window right after a click, then the caller (`runner`) lets it lapse —
    /// see `Cabinet::set_reset_pressed`. Power's rocker has no such flag; its
    /// position is just `powered` itself (a real toggle, stays where left).
    reset_pressed: bool,
    /// O nome do memory card encaixado em cada slot — impresso no painel do
    /// adesivo da porta (plan revision: "mostrar o nome dele no espacinho").
    /// `None` = slot vazio, o painel fica liso.
    card1_label: Option<String>,
    /// As entradas de controle (plan revision: "no controle 1 mostrar texto
    /// conectado, no controle 2 caso não tenha mostrar o slot"; e o drag):
    /// trocados?, segunda entrada com gamepad?, entrada sob o cursor no
    /// arrasto (highlight verde).

    /// O modo analógico da entrada 1 (o botão ANALOG do DualShock):
    /// desligado = os sticks ficam mudos para o core (e a vibração junto).
    analog1: bool,
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
    /// O encaixe do memory card (plan revision — o card desliza no slot
    /// como o cartucho do SNES): `Some((t, inserindo))` com t 0..1 — 1 =
    /// encaixado. `None` = parado no estado final.
    card_motion: Option<(f32, bool)>,
}

/// Uma barra do bloco HowLongToBeat do painel — o rótulo dim em cima
/// ("história principal") e a barra colorida com o tempo dentro, largura
/// proporcional ao duration (`frac` 0..1 contra o maior tempo do jogo; o
/// desenho impõe um mínimo visível).
#[derive(Clone)]
pub struct HltbBar {
    pub label: String,
    /// O tempo formatado ("1h 59min") — escrito dentro da barra quando
    /// cabe, logo depois dela quando não cabe.
    pub text: String,
    pub frac: f32,
    pub color: (u8, u8, u8),
}

/// O bloco HowLongToBeat da aba informações — as 4 barras de tempo (como o
/// card do site: história, + extras, completista, todos os estilos) sob um
/// cabeçalho com o ícone "H". `icon` é a textura registrada pelo caller via
/// `Cabinet::set_image`; sem ela o cabeçalho fica só com o texto.
#[derive(Clone)]
pub struct HltbBars {
    pub icon: u64,
    pub rows: Vec<HltbBar>,
}

/// A seção de nota da IGDB na aba informações — cabeçalho com a logo e o
/// valor centrado. `icon` é a textura registrada pelo caller.
#[derive(Clone)]
pub struct IgdbSection {
    pub icon: u64,
    /// A nota formatada ("68 / 100").
    pub nota: String,
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
    /// between "Favoritar" and "Configurações". Desativado quando
    /// `achievements_reason` é `Some` (plan revision: "mostre o botão mas
    /// mostre desativado") — identificado sem set conta como identificado.
    pub achievements: bool,
    /// A primeira página do manual em PDF (plan revision: o corpo da aba
    /// manual a mostra como as artes, e o clique nela abre o leitor).
    /// `None` = sem manual para o jogo.
    pub manual_first_page: Option<u64>,
    /// O motivo pelo qual o botão Conquistas aparece **desativado** (plan
    /// revision: "avisar o usuário com tooltip no botão conquistas,
    /// mostrar mas deixar desativado") — impresso no rótulo entre
    /// parênteses ("versão sem suporte" para hash marcado pelo RA, "sem
    /// conquistas" para set vazio) e o clique nele não faz nada. `None` =
    /// destravado.
    pub achievements_reason: Option<String>,
    /// A medalha de prêmio do jogo (plan fase 4) — textura já registrada
    /// pelo caller (`set_image`), desenhada à esquerda do número da linha
    /// "conquistas".
    pub award_img: Option<u64>,
    /// O bloco HowLongToBeat (as 4 barras de tempo com o ícone "H") no fim
    /// da aba informações — `None` e o jogo não ganha bloco nenhum (o site
    /// não conhece o jogo; tabela embutida, nada de rede).
    pub hltb: Option<HltbBars>,
    /// A seção de nota da IGDB (logo + valor) na aba informações — `None`
    /// e o jogo não ganha seção (sem nota na IGDB para ele).
    pub igdb: Option<IgdbSection>,
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
    /// A arte tem transparência de verdade (algum pixel com alpha abaixo de
    /// 250 — render 3D em ângulo, fundo vazado). Carregada NA textura, não
    /// num registro paralelo: a textura sobrevive entre visitas à estante,
    /// e a flag tem que sobreviver junto.
    vazado: bool,
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
        // Software é o último nome da lista do hint — chegar aqui significa
        // que NENHUM driver de hardware subiu. O gabinete (warp em malha +
        // véus de alpha por frame) em CPU satura um núcleo e compete com o
        // core do emulador: loga WARN e arma o aviso no chin.
        let sw_notice = canvas.renderer_name.eq_ignore_ascii_case("software");
        if sw_notice {
            log::warn!(
                "renderer software — sem aceleração de GPU, desempenho reduzido \
                 (o ambiente pode forçar um driver via SDL_RENDER_DRIVER)"
            );
        }
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
            spin: 0.0,
            spin_speed: 0.0,
            spin_last: None,
            mesh_2d: None,
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
            foley_busy_at: Instant::now(),
            tv_power: None,
            tv_menu: None,
            pad_focus: None,
            picture: Picture::default(),
            composite: false,
            picture_buf: Vec::new(),
            picture_tmp: Vec::new(),
            volume: 16,
            volume_osd_until: None,
            tv_power_buttons: None,
            dead_glass: None,
            collapse_snap: None,
            hiss_rng: 0x2545_f491,
            foley: None,
            cd_noise: None,
            ch3_until: None,
            canvas_rect,
            nameplate: BRAND.to_string(),
            nameplate_updates: (false, false),
            update_arrows: (None, None),
            osd_queue: VecDeque::new(),
            ra_status: None,
            // Só o aviso de renderer por software (degradado) aparece na
            // abertura; a versão e o nome do renderer saíram do queixo.
            chin_notice: sw_notice.then(sw_boot_notice),
            perf_overlay: std::env::var_os("POLYSTNX_PERF").is_some(),
            perf_fps: 0,
            perf_cpu: None,
            perf_ram_mb: None,
            close_button: Rect::new(0, 0, 0, 0),
            minimize_button: Rect::new(0, 0, 0, 0),
        })
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

    /// Um quadrilátero cheio, de cantos livres (sentido horário) — a
    /// etiqueta de papel torta do cartucho genérico.
    pub fn quad(&mut self, pts: [(f32, f32); 4], c: (u8, u8, u8, u8)) {
        let color = FColor::RGBA(
            c.0 as f32 / 255.0,
            c.1 as f32 / 255.0,
            c.2 as f32 / 255.0,
            c.3 as f32 / 255.0,
        );
        let verts = pts.map(|(x, y)| Vertex {
            position: sdl3::render::FPoint::new(x, y),
            color,
            tex_coord: sdl3::render::FPoint::new(0.0, 0.0),
        });
        let _ = self
            .canvas
            .render_geometry(&verts, None::<&Texture>, &[0, 1, 2, 0, 2, 3][..]);
    }

    /// `s` escrito à mão: a linha corre inclinada `slope` (px por px) a
    /// partir de `(x, y)`, e cada letra ganha um tremido próprio — sobe ou
    /// desce um pouco, gira alguns graus, encolhe ou cresce um tico, sai
    /// com mais ou menos tinta. O sorteio vem de `seed`, então o mesmo
    /// texto sai sempre com a mesma letra. Devolve o avanço em x.
    pub fn text_hand(
        &mut self,
        (x, y): (f32, f32),
        slope: f32,
        size: f32,
        c: (u8, u8, u8),
        s: &str,
        seed: u32,
    ) -> f32 {
        let mut r = seed.wrapping_mul(2_654_435_761) | 1;
        let mut rnd = move || {
            r ^= r << 13;
            r ^= r >> 17;
            r ^= r << 5;
            (r >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
        };
        let k = size / GLYPH_H as f32;
        let adv = GLYPH_W as f32 * k * 0.92;
        // Uma onda lenta na linha, de quem escreve sem régua.
        let (wave_f, wave_p) = (0.6 + 0.3 * rnd(), rnd() * 3.0);
        let angle = slope.atan().to_degrees() as f64;
        let mut pen = 0.0f32;
        for (i, ch) in s.chars().enumerate() {
            let jit = (rnd(), rnd(), rnd(), rnd());
            if ch != ' ' {
                let idx = glyph_index(ch);
                let g = 1.0 + 0.08 * jit.0;
                let (gw, gh) = (GLYPH_W as f32 * k * g, GLYPH_H as f32 * k * g);
                let lift = (i as f32 * wave_f + wave_p).sin() * size * 0.05 + jit.1 * size * 0.04;
                let ink = 0.8 + 0.2 * jit.3;
                self.font.set_color_mod(
                    (c.0 as f32 * ink) as u8,
                    (c.1 as f32 * ink) as u8,
                    (c.2 as f32 * ink) as u8,
                );
                let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
                let dst =
                    sdl3::render::FRect::new(x + pen, y + pen * slope + lift + (size - gh), gw, gh);
                // Duas passadas meio pixel ao lado: o traço da caneta, mais
                // grosso que o da fonte.
                for nudge in [0.0, size * 0.035] {
                    let mut at = dst;
                    at.x += nudge;
                    let _ = self.canvas.copy_ex(
                        self.font,
                        src,
                        at,
                        angle + jit.2 as f64 * 6.0,
                        None,
                        false,
                        false,
                    );
                }
            }
            pen += adv * (1.0 + 0.06 * jit.0);
        }
        pen
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

    /// A imagem `id` tem transparência de verdade (fundo vazado) — o gêmeo
    /// do [`Cabinet::image_vazado`] para o desenho da estante.
    pub fn image_vazado(&self, id: u64) -> bool {
        self.images.get(&id).is_some_and(|img| img.vazado)
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

    /// Como [`Screen::image_fit`], com a arte ancorada na BASE do rect —
    /// para capa vazada (render 3D em ângulo, fundo transparente): a caixa
    /// pousa com a base na prateleira em vez de flutuar centrada.
    pub fn image_fit_bottom(&mut self, id: u64, x: i32, y: i32, bw: u32, bh: u32) {
        let Some(img) = self.images.get(&id) else {
            return;
        };
        let (iw, ih) = (img.w as f32, img.h as f32);
        let scale = (bw as f32 / iw).min(bh as f32 / ih);
        let dw = (iw * scale).round() as i32;
        let dh = (ih * scale).round() as i32;
        let dx = x + (bw as i32 - dw) / 2;
        let dy = y + bh as i32 - dh;
        let _ = self.canvas.copy(
            &img.tex,
            None::<sdl3::render::FRect>,
            Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
        );
    }
}

/// Image id the RA mock example registers its badge under.
pub const DEMO_BADGE_IMG: u64 = u64::MAX;

/// Image id for the RetroAchievements logo the app registers (the favicon,
/// embedded in the app crate) — the persistent chin badge draws it instead
/// of the pixel gamepad when it's there.
pub const RA_LOGO_IMG: u64 = u64::MAX - 7;

impl Cabinet {
    /// Drop expired fronts, then draw the front block — or, with the queue
    /// empty, the degraded-mode notice / the persistent RetroAchievements
    /// badge (when armed).
    fn draw_osd_front(&mut self) {
        let now = Instant::now();
        while self.osd_queue.front().is_some_and(|e| e.until <= now) {
            self.osd_queue.pop_front();
        }
        let (_, out_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let Some(entry) = self.osd_queue.front() else {
            self.draw_chin_persistent_inner(out_h);
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
        if self.chin_notice_live() {
            let notice = self.chin_notice.as_ref().expect("live implies some");
            return ChinOsd::Notify {
                lines: notice.lines.iter().map(|(l, _)| l.clone()).collect(),
                badge: None,
            };
        }
        match self.ra_status {
            Some(s) => ChinOsd::Ra {
                hardcore: s.hardcore,
            },
            None => ChinOsd::None,
        }
    }

    /// The chin's persistent block on every screen that isn't the gameplay
    /// one: the degraded-mode notice in its first seconds after launch, then
    /// the RA badge (`composite_screen` covers
    /// início/estante/configurações; the idle's `present_static` and its
    /// capture call it directly).
    fn draw_chin_persistent(&mut self) {
        self.reap_idle_audio();
        let (_, out_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.draw_chin_persistent_inner(out_h);
    }

    /// `draw_chin_persistent`'s body, shared with `draw_osd_front` (whose
    /// `out_h` is already in hand): the boot notice while it lives, the RA
    /// badge after.
    fn draw_chin_persistent_inner(&mut self, out_h: u32) {
        if self.chin_notice_live() {
            let notice = self.chin_notice.as_ref().expect("live implies some");
            draw_chin_notice(
                &mut self.canvas,
                &mut self.font,
                self.screen,
                out_h,
                &self.nameplate,
                notice,
            );
            return;
        }
        let Some(status) = self.ra_status else {
            return;
        };
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

    /// O aviso de abertura ainda vive?
    fn chin_notice_live(&self) -> bool {
        self.chin_notice
            .as_ref()
            .is_some_and(|n| Instant::now() < n.until)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        fit_aspect_in, ra_badge_rows, screen_area, sw_notice_rows, wrapped_height, GLYPH_H,
        OSD_GREEN,
    };
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
    fn sw_notice_rows_pin_the_degraded_contract() {
        let rows = sw_notice_rows();
        // Título no âmbar de aviso (jamais o verde do OSD — degradado não é
        // "tudo certo"), consequência no cinza claro do chin.
        assert_eq!(rows[0].0, "SEM ACELERAÇÃO DE GPU");
        assert_ne!(rows[0].1, OSD_GREEN);
        assert_eq!(rows[1], ("DESEMPENHO REDUZIDO", (235, 235, 225)));
    }

    #[test]
    fn perf_line_omits_unavailable_segments_and_stays_ascii() {
        use super::perf_line;
        assert_eq!(
            perf_line(34, Some(52), Some(84)),
            "34 FPS  CPU 52C  RAM 84 MB"
        );
        // Sem sysfs/proc (macOS, permissão negada): só o fps — sem placeholders.
        assert_eq!(perf_line(60, None, None), "60 FPS");
        assert_eq!(perf_line(60, Some(61), None), "60 FPS  CPU 61C");
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

    #[test]
    fn atlas_char_traduz_o_que_o_atlas_nao_tem() {
        use super::{atlas_char, glyph_index};
        // pontuação tipográfica e setas viram o ASCII mais próximo — nada de
        // "?" na copy (plan revision: "corrigir copy com ?")
        assert_eq!(atlas_char('—'), '-');
        assert_eq!(atlas_char('–'), '-');
        assert_eq!(atlas_char('•'), '-');
        assert_eq!(atlas_char('…'), '.');
        assert_eq!(atlas_char('←'), '<');
        assert_eq!(atlas_char('→'), '>');
        assert_eq!(atlas_char('↑'), '^');
        assert_eq!(atlas_char('↓'), 'v');
        assert_eq!(atlas_char('‘'), '\'');
        assert_eq!(atlas_char('“'), '"');
        assert_eq!(atlas_char('\u{3000}'), ' ');
        // full-width é o espelho ASCII do bloco U+FF01..U+FF5E
        assert_eq!(atlas_char('Ａ'), 'A');
        assert_eq!(atlas_char('１'), '1');
        // o Latin-1 que o atlas TEM passa intocado — acento usa o glifo real
        assert_eq!(atlas_char('ç'), 'ç');
        assert_eq!(atlas_char('Õ'), 'Õ');
        // e o que não tem tradução cai no '?' de sempre, desenhável
        assert_eq!(glyph_index('—'), glyph_index('-'));
        assert_ne!(glyph_index('—'), glyph_index('?'));
        assert_eq!(glyph_index('⌘'), glyph_index('?'));
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

/// Quadros estéreo ainda na fila de um stream opcional.
fn a_queued(a: &Option<AudioOut>) -> usize {
    a.as_ref().map_or(0, |a| a.queued_frames())
}

/// `PSX_XPERIENCE_DEBUG_COMPOSITE`, lido uma vez (era um `getenv` com
/// alocação a cada quadro apresentado).
fn debug_composite_path() -> Option<&'static str> {
    static P: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    P.get_or_init(|| std::env::var("PSX_XPERIENCE_DEBUG_COMPOSITE").ok())
        .as_deref()
}
