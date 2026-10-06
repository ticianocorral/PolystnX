//! The selector shelf, factored out of the `selector` binary so `polystnx` can
//! show it between games: a scrollable grid of covers (or a multicart-style
//! list when no cover art is around) with a details panel, mouse and gamepad
//! navigation only (plan revision: no keyboard shortcuts, so type-to-search
//! is gone too — except for the shelf's own title filter, the one deliberate
//! exception, same as the pause book's free-text note). Cover/logo art is
//! local — dropped by hand into `assets/cover/`/`assets/logo/` (plan §4.3) —
//! matched by the ROM's file name (peeling variant tags down to the base
//! name) and decoded lazily as tiles scroll into view. The details panel itself is drawn flat by
//! `Cabinet` (plan revision: "o painel nao pode estar dentro da TV") —
//! outside the tube's warp, like the in-game side panel — so this module only
//! ever lays out the grid inside `Cabinet::shelf_screen_size`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::Result;
use polystnx_domain::{Catalog, CatalogEntry, Order};
use polystnx_platform::{
    Cabinet, MenuMode, MenuNav, PanelSection, Platform, Screen, ShelfButton, ShelfPanelInfo,
};

use crate::idle;

// Quadrado: capa de PSX é a fronda da caixa de CD (jewel case) — a
// referência do Spawn tem 850x850 — e não a caixa 4:3 paisagem do SNES.
// `image_fit` completa o encaixe para os raros scans não-quadrados.
const TILE_W: u32 = 236;
const TILE_H: u32 = 236;
const GAP: u32 = 18;
const MARGIN: i32 = 28;
/// Room reserved at the header row for the "N games" label and the filter
/// box — both sit above the grid/recent row, inside the tube like the rest
/// of the shelf's own content.
const HEADER_H: i32 = 28;
/// Thin strip reserved at the grid's right edge for the scroll indicator
/// (plan revision: "mostrar rolagem na tv para visualizar os jogos") —
/// scrolling itself already worked (wheel/d-pad/page keys); this just makes
/// it visible, and how much more there is to see.
const SCROLLBAR_W: u32 = 6;
const SCROLLBAR_RESERVE: u32 = 16;

/// Row height when the shelf falls back to a plain text list (no cover art
/// loaded at all, plan §3.1) — a multicart-style menu instead of a grid of
/// empty tiles.
const LIST_ROW_H: u32 = 22;
const LIST_GAP: u32 = 6;

/// O que o RA completa por jogo além do DAT: `(lançamento, extras)` —
/// extras como `("editora", "Konami")`.
type RaExtras = (Option<String>, Vec<(String, String)>);

/// A "jogados recentemente" strip above the main grid/list (plan revision) —
/// the last played games, always in a single row. Both strips scroll
/// horizontally now (see `STRIP_CAP`), so no fixed five-item cap anymore.
const RECENT_TILE_W: u32 = 150;
const RECENT_TILE_H: u32 = 150;
const RECENT_GAP: u32 = 14;
const RECENT_LABEL_H: i32 = 24;

/// The main grid/list's own section heading (plan revision: "colocar um
/// titulo (Todos os jogos)") — mirrors the "jogados recentemente" label
/// above the recent strip, so both sections read the same way.
const ALL_GAMES_LABEL_H: i32 = 24;
const ALL_GAMES_TITLE: &str = "todos os jogos";

/// A "favoritos" strip above the recent one (plan revision: "adicionar
/// marcador de favorito nos jogos; mostrar em uma categoria acima dos
/// ultimos jogados") — same tile geometry as the recent strip, one row.
/// Both strips scroll horizontally (plan revision: "inserir rolagem
/// horizontal em favoritos e jogados recentemente"): more markers than fit
/// stay one cursor-move away instead of being cut off by a hard cap. The
/// label height mirrors the other two sections'.
const FAV_LABEL_H: i32 = 24;
const FAV_TITLE: &str = "favoritos";
/// Hard caps well past what fits on screen at once — the strips scroll, but
/// a run of thousands of played games shouldn't grow them without bound.
const STRIP_CAP: usize = 60;
/// The recent strip shows at most this many games (plan revision: "jogados
/// recentemente mostrar apenas no maximo 8 jogos") — favorites keep the
/// wider `STRIP_CAP`.
const RECENT_CAP: usize = 8;

/// The title filter box (plan revision: "colocar filtro para facilitar o
/// encontro dos games na lista") — the shelf's own one deliberate keyboard
/// exception, same pattern as the pause book's free-text note.
const FILTER_BOX_W: u32 = 240;
const FILTER_BOX_H: u32 = 24;
const FILTER_LIMIT: usize = 40;

/// "Histórico" button (plan revision: "colocar um botão para histórico
/// listando os jogos mais jogados") — drawn in the header, left of the
/// filter box, same row/height.
const HISTORY_BTN_W: u32 = 110;

/// "Atualizar" button (plan revision: "colocar botão de atualizar estante
/// do lado do histórico") — same header row, to the "histórico" button's
/// left; re-scans roms/ without leaving the shelf.
const REFRESH_BTN_W: u32 = 110;

const BG: (u8, u8, u8) = (18, 18, 20);
const TILE_BG: (u8, u8, u8, u8) = (34, 34, 40, 255);
const HILITE: (u8, u8, u8, u8) = (240, 200, 80, 255);
const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
/// Text color on top of the `HILITE` selection bar in list mode — dark, for
/// contrast against the bright fill.
const HILITE_TEXT: (u8, u8, u8) = (24, 20, 12);
/// How opaque the shelf sits over the resting TV static (plan §3.3) — under
/// 1.0 so the signal-off snow bleeds through faintly instead of a flat
/// background, but not enough to fight with reading the grid/list.
const SHELF_ALPHA: f32 = 0.92;

/// What the player did on the shelf.
pub enum Pick {
    /// Launch this ROM (already marked played in the catalogue).
    Play {
        rom: PathBuf,
        /// Local logo art (`assets/logo/<rom stem>.*`), if one exists — for
        /// the side panel during play (plan §3.2).
        wheel: Option<PathBuf>,
        /// Local cartridge art (`assets/cartridge/<rom stem>.*`), if one
        /// exists — shown in the panel alongside the logo (plan revision).
        cartridge: Option<PathBuf>,
        /// O nome canônico (No-Intro, do DAT) para o topo do painel — os
        /// arquivos de save/nota continuam chaveados pelo nome do arquivo.
        title: Option<String>,
    },
    /// Clicked "Voltar", or a gamepad's Back button — back out to the
    /// idle/root screen. The app keeps running.
    Back,
    /// Window closed / Cmd-Q — tear the app down.
    Quit,
    /// Clicked "Configurações" — open the settings screen, then come back to
    /// the shelf.
    Settings,
    /// Clicked "Histórico" — open the all-time most-played list (plan
    /// revision), then come back to the shelf.
    History,
    /// Clicked "Atualizar" (plan revision: "adicionar opção de atualizar a
    /// estante para buscar jogos novos sem precisar abrir e fechar o app")
    /// — the caller re-scans `roms/` and reopens the shelf with the fresh
    /// catalog.
    Refresh,
}

/// Knobs for [`run`].
pub struct ShelfOpts {
    pub order: Order,
    /// Headless smoke test: stop after N frames and return [`Pick::Quit`].
    pub max_frames: Option<u64>,
    /// Headless: on the last frame, save the shelf (through the tube) here.
    pub shot: Option<PathBuf>,
    /// Ease in from residual signal-off static instead of cutting in cold —
    /// the static level to fade from (plan §3.3). `None` draws from frame one.
    pub fade_in: Option<f32>,
    /// Headless only: pre-apply this as the title filter from frame one,
    /// as if the player had already typed and committed it — there's no way
    /// to simulate real typing in a headless run, so this is how `--shot`
    /// verifies the filtered view (plan revision).
    pub preset_filter: Option<String>,
    /// RetroAchievements account (plan: `docs/plano-retroachievements.md`,
    /// fase 2) — `Some((user, web_key, connect_token))` enables the per-game
    /// identification
    /// that shows "conquistas: N" in the panel; `None` keeps the shelf
    /// entirely RA-free.
    pub ra: Option<(String, String, String)>,
}

impl Default for ShelfOpts {
    fn default() -> Self {
        Self {
            order: Order::Shelf,
            max_frames: None,
            shot: None,
            fade_in: None,
            preset_filter: None,
            ra: None,
        }
    }
}

/// Texture keys for a game's art. No tempo do cartucho eram fatias do SHA1
/// (hex); para disco a chave estável é o **serial**, que não é hex — então a
/// família inteira vira splitmix64 sobre a chave, com um sal por uso
/// (mesma fórmula do `manual_page_id`, sem o bit de família badge).
fn art_id(key: &str, salt: u64) -> u64 {
    let mut acc = 0x9E37_79B9_7F4A_7C15u64 ^ salt.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    for b in key.bytes() {
        acc ^= b as u64;
        acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        acc ^= acc >> 33;
    }
    acc
}

/// Texture key for a game's cover.
fn cover_id(key: &str) -> u64 {
    art_id(key, 0)
}

/// Texture key for a game's wheel logo.
fn wheel_id(key: &str) -> u64 {
    art_id(key, 1)
}

/// Texture key for a game's disc art (era o cartucho no irmão de SNES; a
/// cena do PSX usa `assets/disc/`, Fase 3 — a estante mantém o slot).
fn cartridge_id(key: &str) -> u64 {
    art_id(key, 2)
}

/// Texture key de uma página do manual em PDF (plan revision: "colocar
/// suporte para abrir manual pdf") — hash do sha1 + página com o bit alto
/// da família badge (nunca colide com as chaves de arte do jogo).
fn manual_page_id(sha1: &str, page: usize) -> u64 {
    format!("{sha1}:{page}")
        .bytes()
        .fold(0x9E37_79B9_7F4A_7C15u64, |mut acc, b| {
            acc ^= b as u64;
            acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
            acc ^= acc >> 33;
            acc
        })
        | (1 << 62)
}

/// O PDF do manual da ROM em `dir` (`assets/manual/<nome da rom>.pdf`),
/// com o mesmo casamento por nome/variantes da arte local.
fn find_local_manual(dir: &Path, rom_path: &str) -> Option<PathBuf> {
    let stem = Path::new(rom_path).file_stem()?.to_str()?;
    let mut candidates = vec![stem.to_string()];
    let mut cur = stem.to_string();
    while cur.ends_with(')') {
        match cur.rfind(" (") {
            Some(idx) => {
                cur.truncate(idx);
                candidates.push(cur.clone());
            }
            None => break,
        }
    }
    for cand in candidates {
        let p = dir.join(format!("{cand}.pdf"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// `(x, y, w, h)` em coordenadas de tela — o rect de um botão do leitor.
type ViewBtn = (i32, i32, u32, u32);

/// O leitor de manual na TV (plan revision: "abrir na tv como o back cover
/// com opção de ver as paginas") — página ampliada + `‹`/`›` + `voltar`.
/// Devolve os rects dos três botões para o hit-test do frame seguinte.
fn draw_manual_view(
    d: &mut Screen,
    sha1: &str,
    page: usize,
    pages: usize,
    page_ready: bool,
) -> (ViewBtn, ViewBtn, ViewBtn) {
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let btn = (110u32, 34u32);
    let voltar = (w / 2 - btn.0 as i32 / 2, h - 64, btn.0, btn.1);
    let prev = (w / 2 - btn.0 as i32 - 90, h - 64, btn.0, btn.1);
    let next = (w / 2 + 90, h - 64, btn.0, btn.1);

    d.text(
        MARGIN,
        MARGIN,
        2,
        TEXT,
        &format!(
            "manual — página {page} de {pages}",
            pages = if pages > 0 {
                pages.to_string()
            } else {
                "…".to_string()
            }
        ),
    );
    let id = manual_page_id(sha1, page);
    let area = (
        MARGIN,
        MARGIN + 60,
        (w - MARGIN * 2) as u32,
        (h - MARGIN * 2 - 140) as u32,
    );
    if page_ready {
        d.image_fit(id, area.0, area.1, area.2, area.3);
    } else if pages > 0 {
        d.text(MARGIN, area.1 + 20, 1, DIM, "renderizando a página...");
    } else {
        d.text(MARGIN, area.1 + 20, 1, DIM, "abrindo o manual...");
    }

    let mut draw_nav = |r: &(i32, i32, u32, u32), label: &str| {
        d.outline(r.0, r.1, r.2, r.3, 2, (TEXT.0, TEXT.1, TEXT.2, 255));
        d.text(
            r.0 + (r.2 as i32 - label.chars().count() as i32 * 9) / 2,
            r.1 + 7,
            1,
            TEXT,
            label,
        );
    };
    draw_nav(&prev, "< página");
    draw_nav(&next, "página >");
    draw_nav(&voltar, "voltar");
    (prev, next, voltar)
}

/// Texture key for an achievement badge — hashed from the badge name with a
/// fixed splitmix64-style mix (the name has no sha1 of its own to slice),
/// ending in a high bit the game-art keys never set.
fn badge_image_id(name: &str) -> u64 {
    name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |mut acc, b| {
        acc ^= b as u64;
        acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        acc ^= acc >> 33;
        acc
    }) | (1 << 62)
}

/// Texture key for a game's back-cover art (plan revision: "abaixo da
/// logo... colocar o back cover tambem") — same derivation as
/// `cartridge_id`, a different fixed salt so it can't collide with any of
/// the other three.
/// The strips' scroll affordance (plan revision: "inserir rolagem
/// horizontal em favoritos e jogados recentemente"; depois "nao consigo
/// clicar nas setas") — real buttons in the strip's header row, right
/// side, same outline style as the header's "histórico" button: "<" slides
/// the strip back a tile, ">" forward, each click one step. Drawn whenever
/// the strip overflows (like the panel's own "Cima"/"Baixo" pair), dimmed
/// at the end it can't move past.
/// `(left, right)` header-button rects for one strip, screen-local.
type StripArrows = ((i32, i32, u32, u32), (i32, i32, u32, u32));

pub fn strip_arrow_rects(scr_w: u32, y0: i32) -> StripArrows {
    let bw = 36u32;
    let bh = 22u32;
    let by = y0 - RECENT_LABEL_H + 1;
    let right = (scr_w as i32 - MARGIN - bw as i32, by, bw, bh);
    let left = (right.0 - 8 - bw as i32, by, bw, bh);
    (left, right)
}

fn draw_strip_arrows(d: &mut Screen, scr_w: u32, y0: i32, scroll: usize, len: usize, vis: usize) {
    let (left, right) = strip_arrow_rects(scr_w, y0);
    for (rect, glyph, more) in [(left, "<", scroll > 0), (right, ">", scroll + vis < len)] {
        let (bx, by, bw, bh) = rect;
        d.outline(bx, by, bw, bh, 1, (150, 150, 158, 255));
        d.fill(bx + 1, by + 1, bw - 2, bh - 2, (34, 34, 40, 255));
        d.text(
            bx + (bw as i32 - 9) / 2,
            by + (bh as i32 - 20) / 2,
            1,
            if more { TEXT } else { DIM },
            glyph,
        );
    }
}

/// The shelf's achievements view (plan revision: "mostrar a lista de
/// conquistas e pontuação total dentro da tv com botão de voltar, parecido
/// com o back cover") — title, "N/M — X de Y pontos", then the per-
/// achievement rows (earned in green with a star-ish [x], locked dim),
/// scrolled by `top`, with a flat "voltar" button bottom-centre. Returns
/// that button's screen-local rect for the click hit-test.
fn draw_achievements_view(
    d: &mut Screen,
    data: Option<&crate::ra::ShelfAchievements>,
    badges: &std::collections::HashMap<String, u64>,
    top: usize,
) -> (i32, i32, u32, u32) {
    const ROW_H: i32 = 22;
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let x = MARGIN;
    let green = (60, 230, 70);
    let voltar = (w / 2 - 70, h - 64, 140u32, 34u32);

    match data {
        None => {
            d.text(x, MARGIN, 2, DIM, "conquistas indisponíveis");
        }
        Some(list) => {
            let (all_pts, got_pts) = list.points();
            d.text_wrapped(x, MARGIN, (w - MARGIN * 2) as u32, 2, TEXT, &list.title);
            d.text(
                x,
                MARGIN + 56,
                1,
                DIM,
                &format!(
                    "conquistas {}/{} ({} hardcore) — {} de {} pontos",
                    list.earned.len(),
                    list.achievements.len(),
                    list.earned.values().filter(|&&hc| hc).count(),
                    got_pts,
                    all_pts
                ),
            );
            let rows = (h - MARGIN * 2 - 130) / ROW_H;
            let visible = rows.max(1) as usize;
            // Hardcore em dourado (a cor do prêmio no RA), softcore em
            // verde — plan revision: "como sei qual tipo ganhei ou já
            // tenho?".
            const GOLD: (u8, u8, u8) = (238, 192, 62);
            for (i, a) in list.achievements.iter().enumerate().skip(top).take(visible) {
                let y = MARGIN + 84 + (i - top) as i32 * ROW_H;
                let (mark, color) = match list.earned.get(&a.id) {
                    Some(true) => ("[HC]", GOLD),
                    Some(false) => ("[SC]", green),
                    None => ("[ ]", DIM),
                };
                let label = format!("{mark} {} ({} pts)", a.title, a.points);
                // Badge 18×18 à esquerda (quando a textura já baixou); o
                // texto abre mão do espaço dele.
                const BADGE: u32 = 18;
                if let Some(id) = badges.get(&a.badge) {
                    d.image_fit(*id, x, y, BADGE, BADGE);
                }
                let text_x = x + 24;
                // Truncate to the tube's width so nothing wraps.
                let max_chars = ((w - MARGIN * 2 - 26) / 9).max(8) as usize;
                let label: String = if label.chars().count() > max_chars {
                    format!(
                        "{}...",
                        label.chars().take(max_chars - 3).collect::<String>()
                    )
                } else {
                    label
                };
                d.text(text_x, y, 1, color, &label);
            }
            if list.achievements.len() > visible {
                let note = if top + visible < list.achievements.len() {
                    "v para rolar"
                } else {
                    "^ v"
                };
                d.text(w - MARGIN - 90, MARGIN + 56, 1, DIM, note);
            }
        }
    }

    // "voltar" — same flat style the back-cover zoom uses.
    d.outline(
        voltar.0,
        voltar.1,
        voltar.2,
        voltar.3,
        1,
        (150, 150, 158, 255),
    );
    d.fill(
        voltar.0 + 1,
        voltar.1 + 1,
        voltar.2 - 2,
        voltar.3 - 2,
        (34, 34, 40, 255),
    );
    d.text(
        voltar.0 + (voltar.2 as i32 - 9 * 6) / 2,
        voltar.1 + (voltar.3 as i32 - 20) / 2,
        1,
        TEXT,
        "voltar",
    );
    voltar
}

fn backcover_id(sha1: &str) -> u64 {
    wheel_id(sha1) ^ 0xC2B2_AE3D_27D4_EB4F
}

/// Art resolution for locally-supplied assets, tried in order for
/// `dir/<candidate>.{png,jpg,jpeg}`:
///
/// 1. the game's canonical display title when there is one (`Tekken 3
///    (USA)`) — the No-Intro naming the DAT gives, so art downloaded under
///    the standard name finds its game no matter what the .chd is called;
/// 2. the rom's file stem — the original convention (`roms/Aladdin.chd`
///    matches `assets/cover/Aladdin.png`);
/// 3. the stem with its trailing "(...)" tags peeled one group at a time
///    ("X (USA) (Rev 1)" -> "X (USA)" -> "X"), so a single base-named art
///    file serves every variant ROM of the same game.
pub(crate) fn find_local_art(dir: &Path, rom_path: &str, display: Option<&str>) -> Option<PathBuf> {
    let stem = Path::new(rom_path).file_stem()?.to_str()?;
    let mut candidates: Vec<String> = display.map(|d| vec![d.to_string()]).unwrap_or_default();
    candidates.push(stem.to_string());
    let mut cur = stem.to_string();
    while cur.ends_with(')') {
        match cur.rfind(" (") {
            Some(idx) => {
                cur.truncate(idx);
                candidates.push(cur.clone());
            }
            None => break,
        }
    }
    for cand in candidates {
        for ext in ["png", "jpg", "jpeg"] {
            let p = dir.join(format!("{cand}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Mark `entry` played and build its launch — shared by Enter/A confirm and
/// clicking the already-selected tile.
fn pick_play(
    catalog: &Catalog,
    logo_dir: &Path,
    cartridge_dir: &Path,
    entry: &CatalogEntry,
) -> Pick {
    let _ = catalog.mark_played(&entry.rom.sha1);
    let wheel = find_local_art(logo_dir, &entry.rom.path, Some(&entry.title()));
    let cartridge = find_local_art(cartridge_dir, &entry.rom.path, Some(&entry.title()));
    Pick::Play {
        rom: PathBuf::from(&entry.rom.path),
        wheel,
        cartridge,
        title: Some(entry.title().into_owned()),
    }
}

/// Whether `entry`'s title matches a filter query — empty matches everything,
/// otherwise a case-insensitive substring test (plan revision: "colocar
/// filtro para facilitar o encontro dos games na lista").
/// The panel's own file facts (plan revision: "o painel... muito vazio") —
/// always available straight from the scan (or, for `playtime_secs`, from
/// the per-game sidecar `runner::total_playtime_secs` already writes), no
/// DAT/local art needed, so a plain title-only game still gets a panel with
/// something in it besides two buttons. Appended after any No-Intro extras.
/// (A "jogado" row lived here once — the last-played date is already the
/// recent strip's whole job, and the row was what overflowed into the
/// panel's buttons.)
fn game_info_lines(entry: &CatalogEntry, playtime_secs: u64) -> Vec<(String, String)> {
    vec![
        ("disco".to_string(), cart_size(entry.rom.size)),
        ("tempo total".to_string(), format_playtime(playtime_secs)),
    ]
}

/// O tamanho do disco de CD-ROM em MB ("disco: 652 MB"). Discos de PSX não
/// têm os "mask sizes" de cartucho de SNES — é o tamanho real do arquivo,
/// arredondado para cima para o MB inteiro.
fn cart_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    let mb = bytes.div_ceil(MIB);
    format!("{mb} MB")
}

/// Total powered-on time for a game (plan revision: "mostrar tempo total de
/// jogo") — `0` (never turned on, or never played) reads as "nunca" rather
/// than a confusing "0min".
fn format_playtime(secs: u64) -> String {
    if secs == 0 {
        return "nunca".to_string();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    match (h, m) {
        (0, 0) => "menos de 1min".to_string(),
        (0, m) => format!("{m}min"),
        (h, m) => format!("{h}h {m}min"),
    }
}

fn filter_box_rect(scr_w: u32) -> (i32, i32, u32, u32) {
    (
        scr_w as i32 - MARGIN - FILTER_BOX_W as i32,
        MARGIN - 18,
        FILTER_BOX_W,
        FILTER_BOX_H,
    )
}

fn history_button_rect(scr_w: u32) -> (i32, i32, u32, u32) {
    let (fx, fy, _, fh) = filter_box_rect(scr_w);
    (fx - 12 - HISTORY_BTN_W as i32, fy, HISTORY_BTN_W, fh)
}

fn refresh_button_rect(scr_w: u32) -> (i32, i32, u32, u32) {
    let (hx, hy, _, fh) = history_button_rect(scr_w);
    (hx - 8 - REFRESH_BTN_W as i32, hy, REFRESH_BTN_W, fh)
}

/// Geometry of the shelf's item grid — image tiles, or (no cover art loaded
/// at all, plan §3.1) a single-column text list styled like a pirate NES
/// multicart menu. One column of navigation math (`cols`/`vis_rows`) serves
/// both: list mode is just `cols == 1` with a short row instead of a tile.
struct GridLayout {
    x0: i32,
    y0: i32,
    /// Cell stride, including the gap.
    cell_w: i32,
    cell_h: i32,
    /// The clickable item itself, within its cell (no gap).
    item_w: i32,
    item_h: i32,
    cols: usize,
    vis_rows: usize,
    list_mode: bool,
}

impl GridLayout {
    /// `top_y` is where the grid itself starts, below the header row and any
    /// "jogados recentemente" strip above it (plan revision — the panel used
    /// to eat into this width; now the grid gets the whole shelf screen).
    ///
    /// Steam Deck (e qualquer tela onde 260px de tile rendem só 2 colunas):
    /// a grade nunca mostra menos que 3 colunas × 2 linhas — o tile encolhe,
    /// mantendo o 4:3, antes de perder uma coluna ou uma linha.
    fn new(scr_w: u32, scr_h: u32, list_mode: bool, top_y: i32) -> Self {
        let grid_w = scr_w.saturating_sub(MARGIN as u32 * 2 + SCROLLBAR_RESERVE);
        let (cell_w, cell_h, item_w, item_h, cols, x0) = if list_mode {
            let cell_h = (LIST_ROW_H + LIST_GAP) as i32;
            (
                grid_w as i32,
                cell_h,
                grid_w as i32,
                LIST_ROW_H as i32,
                1,
                MARGIN,
            )
        } else {
            let gap = GAP as i32;
            let cols = ((grid_w as i32 + gap) / (TILE_W as i32 + gap)).max(3);
            // Encolhe o tile até a fileira caber `cols` colunas com gap.
            let item_w = (TILE_W as i32)
                .min((grid_w as i32 - gap * (cols - 1)) / cols)
                .max(48);
            let mut item_h = item_w * TILE_H as i32 / TILE_W as i32;
            // E até 2 linhas couberem na altura (se a tela for baixa demais
            // para os tiles já encolhidos pela largura).
            let avail_h = scr_h as i32 - top_y - MARGIN;
            let two_rows_h = (avail_h / 2 - gap).max(36);
            if item_h > two_rows_h {
                item_h = two_rows_h;
            }
            let item_w = item_h * TILE_W as i32 / TILE_H as i32;
            let cell_w = item_w + gap;
            let cell_h = item_h + gap;
            // Alinhada à ESQUERDA (plan revision: "alinhar a estante da
            // lista dos jogos a esquerda") — a grade nasce na margem e a
            // sobra de largura fica toda à direita.
            let x0 = MARGIN;
            (cell_w, cell_h, item_w, item_h, cols as usize, x0)
        };
        let vis_rows = ((scr_h as i32 - top_y - MARGIN) / cell_h).max(1) as usize;
        Self {
            x0,
            y0: top_y,
            cell_w,
            cell_h,
            item_w,
            item_h,
            cols,
            vis_rows,
            list_mode,
        }
    }

    /// Top-left of item `i`'s cell, relative to the grid's own row window
    /// (`top_row` is the first visible row) — `None` if it's scrolled out of
    /// view.
    fn cell_pos(&self, i: usize, top_row: usize) -> Option<(i32, i32)> {
        let row = i / self.cols;
        if row < top_row || row >= top_row + self.vis_rows {
            return None;
        }
        let col = i % self.cols;
        Some((
            self.x0 + col as i32 * self.cell_w,
            self.y0 + (row - top_row) as i32 * self.cell_h,
        ))
    }

    /// Which visible item (if any) a screen-local point sits inside — `None`
    /// in the gap between cells, past the last column, below the last
    /// visible row, or past the end of `view`.
    fn tile_at(&self, x: i32, y: i32, top_row: usize, view_len: usize) -> Option<usize> {
        let (dx, dy) = (x - self.x0, y - self.y0);
        if dx < 0 || dy < 0 || dx % self.cell_w >= self.item_w || dy % self.cell_h >= self.item_h {
            return None;
        }
        let col = (dx / self.cell_w) as usize;
        let row_in_view = (dy / self.cell_h) as usize;
        if col >= self.cols || row_in_view >= self.vis_rows {
            return None;
        }
        let i = (top_row + row_in_view) * self.cols + col;
        (i < view_len).then_some(i)
    }

    fn total_rows(&self, view_len: usize) -> usize {
        view_len.div_ceil(self.cols)
    }
}

/// Geometry shared by the "favoritos" and "jogados recentemente" strips —
/// always a single row, horizontally scrollable (unlike `GridLayout`): the
/// caller windows it with its own scroll offset.
struct RecentLayout {
    x0: i32,
    y0: i32,
    cell_w: i32,
}

impl RecentLayout {
    fn new(y0: i32) -> Self {
        Self {
            x0: MARGIN,
            y0,
            cell_w: (RECENT_TILE_W + RECENT_GAP) as i32,
        }
    }

    fn item_pos(&self, i: usize) -> (i32, i32) {
        (self.x0 + i as i32 * self.cell_w, self.y0)
    }

    fn hit(&self, x: i32, y: i32, count: usize) -> Option<usize> {
        let (dx, dy) = (x - self.x0, y - self.y0);
        if dx < 0
            || dy < 0
            || dy >= RECENT_TILE_H as i32
            || dx % self.cell_w >= RECENT_TILE_W as i32
        {
            return None;
        }
        let i = (dx / self.cell_w) as usize;
        (i < count).then_some(i)
    }
}

/// `roms/` has nothing in it — a friendlier landing than a hard error, since
/// an empty ROMs folder is the expected first-launch state for a portable,
/// autoexecutável app, not a misconfiguration.
fn empty_roms_screen(plat: &mut Platform, cab: &mut Cabinet) -> Result<Pick> {
    let frame = Duration::from_millis(16);
    cab.set_shelf_panel(empty_shelf_panel());
    cab.set_close_button(true);
    let refresh_rect = refresh_button_rect(cab.shelf_screen_size().0);
    loop {
        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        if m.nav.contains(&MenuNav::Back) {
            return Ok(Pick::Back);
        }
        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return Ok(Pick::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                continue;
            }
            if cab
                .hit_screen_point(ox, oy)
                .is_some_and(|(lx, ly)| in_rect(lx, ly, refresh_rect))
            {
                return Ok(Pick::Refresh);
            }
            match cab.hit_shelf_button(ox, oy) {
                Some(ShelfButton::Back) => return Ok(Pick::Back),
                Some(ShelfButton::Settings) => return Ok(Pick::Settings),
                // No game focused on this screen, so nothing to jump to (and
                // no back cover to enlarge) — "Atualizar" lives in the
                // header now, handled above.
                Some(ShelfButton::PanelJump(_))
                | Some(ShelfButton::Backcover)
                | Some(ShelfButton::Cartridge)
                | Some(ShelfButton::Manual)
                | Some(ShelfButton::ToggleFavorite)
                | Some(ShelfButton::Refresh)
                | Some(ShelfButton::ShelfAchievements)
                | None => {}
            }
        }
        let (scr_w, _) = cab.shelf_screen_size();
        let render = |d: &mut Screen| {
            draw_refresh_button(d, refresh_rect);
            d.text(MARGIN, MARGIN, 2, TEXT, "nenhuma rom encontrada");
            d.text_wrapped(
                MARGIN,
                MARGIN + 40,
                scr_w.saturating_sub(MARGIN as u32 * 2),
                1,
                DIM,
                "copie seus arquivos .sfc/.smc/.zip para a pasta roms/, ao lado \
                 do executável, e clique em Atualizar (ou volte e abra a estante \
                 de novo).",
            );
        };
        cab.frame_shelf(BG, render);
        std::thread::sleep(frame);
    }
}

/// Show the shelf in `cab` (the one persistent window) until the player picks a
/// game or cancels. `plat` and `cab` both outlive the call.
pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    catalog: &Catalog,
    opts: &ShelfOpts,
) -> Result<Pick> {
    let all_scanned = catalog.list(opts.order)?;
    if all_scanned.is_empty() {
        return empty_roms_screen(plat, cab);
    }
    let mut all = all_scanned;

    let cover_dir = crate::dirs::assets_dir().join("cover");
    let logo_dir = crate::dirs::assets_dir().join("logo");
    let cartridge_dir = crate::dirs::assets_dir().join("disc");
    let backcover_dir = crate::dirs::assets_dir().join("backcover");
    // Never re-stat a game's art more than once per shelf visit — most games
    // won't have any, and disk isn't free even if it's cheap.
    let mut tried_cover: HashSet<String> = HashSet::new();
    // Back cover enlarged (plan revision: "ao clicar no back cover
    // possibilitar mostrar em tamanho maior"): Some = the "voltar" button's
    // screen-local rect — the zoom replaces the tube's content until
    // dismissed (the flat panel stays).
    // A arte ampliada (zoom): o id da imagem no cabinet + o rect do botão
    // "voltar" do frame atual. Antes era só do back cover (plan revision:
    // "colocar zoom igual ao backcover no cartucho" — agora os dois).
    let mut zoom_close: Option<u64> = None;
    let mut zoom_rect: (i32, i32, u32, u32) = (0, 0, 1, 1);
    // O manual em PDF aberto na TV (plan revision) — worker decodifica as
    // páginas sob demanda; o estado guarda o pedido em voo para não
    // repetir. `manual_rects` são os botões do frame anterior.
    struct ManualState {
        sha1: String,
        pages: usize,
        page: usize,
        requested: HashSet<usize>,
    }
    let mut manual: Option<ManualState> = None;
    // Vários workers podem estar vivos (preview da 1ª página por jogo +
    // o leitor aberto); todos falam ManualEv.
    let mut manual_channels: Vec<Receiver<crate::manual::ManualEv>> = Vec::new();
    let mut manual_tx: Option<std::sync::mpsc::Sender<usize>> = None;
    // Contagem de páginas por jogo (do preview) — o leitor nasce com ela.
    let mut manual_pages: HashMap<String, usize> = HashMap::new();
    let mut manual_preview_tried: HashSet<String> = HashSet::new();
    let mut manual_rects: (ViewBtn, ViewBtn, ViewBtn) = ((0, 0, 1, 1), (0, 0, 1, 1), (0, 0, 1, 1));
    // O PDF do jogo focused, resolvido uma vez por jogo (fs por frame não).
    let manual_dir = crate::dirs::assets_dir().join("manual");
    let mut manual_paths: HashMap<String, Option<PathBuf>> = HashMap::new();
    let mut tried_logo: HashSet<String> = HashSet::new();
    let mut tried_cartridge: HashSet<String> = HashSet::new();
    let mut tried_backcover: HashSet<String> = HashSet::new();

    let mut sel: usize = 0;
    let mut top_row: usize = 0;
    // The "jogados recentemente" strip has its own cursor (plan revision) —
    // independent of `sel`, the main grid/list's own.
    let mut in_recent = false;
    let mut recent_idx: usize = 0;
    // The "favoritos" strip above it has its own cursor too — same shape,
    // one more zone the d-pad can land on.
    let mut in_fav = false;
    let mut fav_idx: usize = 0;
    // Each strip's own horizontal scroll — the index of its first visible
    // tile, kept clamped to the cursor by the per-frame pass below.
    let mut fav_scroll: usize = 0;
    let mut recent_scroll: usize = 0;
    // The panel's own scroll position (plan revision: "criar rolagem no
    // painel quando necessario") — reset whenever the focused game changes,
    // via `last_focus` below; `draw_shelf_panel` clamps it to whatever the
    // currently-focused game's content actually needs, so letting it run
    // free on repeated "v Baixo" clicks past the end is harmless.
    let mut panel_scroll: usize = 0;
    // A aba ativa do painel (plan revision: "mostrar um info de cada vez
    // mesmo em resolução grande, mudar ao clicar") — persiste entre jogos
    // de propósito: quem organizou por informações continua nelas. A inicial
    // é "informações" (plan revision: "na estante sempre abrir informações
    // como padrao").
    let mut panel_section = PanelSection::Informacoes;
    let mut last_focus: Option<(u8, usize)> = None;
    // The title filter (plan revision) — `filter_query` is what's actually
    // applied; `filter_draft`/`editing_filter` are live only while typing,
    // same split the pause book's free-text note uses.
    let mut filter_query = opts.preset_filter.clone().unwrap_or_default();
    let mut filter_draft = String::new();
    let mut editing_filter = false;
    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;
    let mut frame_no = 0u64;
    // The filtered/sorted listings are rebuilt only when the applied filter
    // changes (and once up front) — re-sorting ~700 entries with fresh String
    // allocations at 60fps was the shelf's biggest per-frame cost. While the
    // player is still TYPING, `filter_draft` moves but `filter_query` doesn't,
    // so typing doesn't rebuild either.
    let mut applied_filter: Option<String> = None;
    let mut view: Vec<CatalogEntry> = Vec::new();
    let mut recent: Vec<CatalogEntry> = Vec::new();
    let mut favorites: Vec<CatalogEntry> = Vec::new();
    // RA identification (plan fase 2): one result per game (None = the
    // server doesn't know it / no set), one hash+fetch in flight at a
    // time, each game tried at most once per visit — the disk cache makes
    // later visits instant.
    let mut ra_games: std::collections::HashMap<String, Option<crate::ra::RaGame>> =
        std::collections::HashMap::new();
    let mut ra_tried: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ra_worker: Option<Receiver<Result<Option<crate::ra::RaGame>, String>>> = None;
    let mut ra_pending: String = String::new();
    // The panel's "X de Y (Z%)" line (plan revision) — the tally per game,
    // cached per visit: it costs a full ROM hash, so it's computed once when
    // the game first reaches the panel and refreshed when the completion
    // progress lands.
    let mut ra_tally: std::collections::HashMap<
        String,
        Option<(usize, usize, Option<crate::ra::Award>)>,
    > = std::collections::HashMap::new();
    // Lançamento/editora que o RA tem e o DAT/TOSEC não, mesmo cache de
    // um-cálculo-por-foco (leitura do JSON da identificação).
    let mut ra_meta: std::collections::HashMap<String, RaExtras> = std::collections::HashMap::new();
    // sha1 → hash RA, guardado pela própria identificação para o painel não
    // refazer o hash da ROM a cada frame.
    let mut ra_hashes: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    // Completion progress (o substituto do GetUserUnlocks, morto na API
    // atual): uma chamada por visita traz os counts de todo o perfil — a
    // fonte das conquistas ganhas fora deste app.
    let mut ra_completion: Option<std::collections::HashMap<u64, crate::ra::CompletionEntry>> =
        None;
    let mut ra_completion_tried = false;
    let mut ra_completion_worker: Option<
        Receiver<Result<std::collections::HashMap<u64, crate::ra::CompletionEntry>, String>>,
    > = None;
    // Badges das conquistas do jogo focado: o prefetch baixa os PNGs em
    // background e o frame registra as texturas direto do disco (umas
    // poucas por frame), então a lista preenche os badges conforme eles
    // chegam — para qualquer jogo, não só o primeiro aberto.
    let mut badge_ids: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut badge_tried: HashSet<String> = HashSet::new();
    // Earned do servidor para os [x] da lista de conquistas (uma tentativa
    // por jogo por visita — o disco cacheia por game id): o worker une no
    // arquivo local `retroachievements/ra-earned/<hash>.json` e devolve o
    // set resultante.
    let mut ra_earned_worker: Option<Receiver<(String, Option<crate::ra::EarnedMap>)>> = None;
    let mut ra_earned_tried: HashSet<String> = HashSet::new();
    // The achievements view (plan revision: "mostrar a lista de conquistas
    // e pontuação total dentro da tv com botão de voltar, parecido com o
    // back cover") — open/closed, its scroll, its data (loaded once per
    // open), and the voltar button's screen-local rect. `ach_hash` guarda o
    // hash RA do jogo aberto, para o earned do servidor (que chega
    // identificado pelo mesmo hash) casar com a lista na tela.
    let mut ach_view = false;
    let mut ach_top: usize = 0;
    let mut ach_data: Option<crate::ra::ShelfAchievements> = None;
    let mut ach_hash: Option<String> = None;
    let mut ach_voltar: (i32, i32, u32, u32) = (0, 0, 0, 0);
    cab.set_close_button(true);

    // Frames left in the "entering over the static" ease-in (§3.3), if any.
    const FADE_IN_FRAMES: u32 = 18;
    let mut fade_frame: u32 = 0;

    loop {
        frame_no += 1;
        // DEBUG: auto-abre o primeiro jogo após N frames — bissecção do
        // quadrado da Metal (a estante rodou de verdade antes do jogo).
        if let Ok(n) = std::env::var("PSX_XPERIENCE_SHELF_FRAMES") {
            if let Ok(n) = n.parse::<u64>() {
                if frame_no == n {
                    if let Some(e) = view.first() {
                        let logo_dir = crate::dirs::assets_dir().join("logo");
                        let cart_dir = crate::dirs::assets_dir().join("disc");
                        return Ok(pick_play(catalog, &logo_dir, &cart_dir, e));
                    }
                }
            }
        }
        if opts.max_frames.is_some_and(|n| frame_no > n) {
            return Ok(Pick::Quit);
        }

        // The general listing is always alphabetical (plan revision) — the
        // recent strip above it already covers "what did I just play",
        // freeing this one up to just be a plain, predictable A-Z browse.
        if applied_filter.as_deref() != Some(filter_query.as_str()) {
            let query_lc = filter_query.to_lowercase();
            view = all
                .iter()
                .filter(|e| e.title().to_lowercase().contains(&query_lc))
                .cloned()
                .collect();
            view.sort_by_key(|e| e.title().to_lowercase());
            // The recent strip only makes sense browsing the unfiltered
            // shelf — once a search narrows things, the whole point is
            // finding a specific game, not re-surfacing what was just
            // played.
            // Most recently played first, descending (plan revision:
            // "organizar jogados recentemente pelo ordem de ultimo jogado
            // decrescente") — already was, kept explicit.
            recent = if filter_query.is_empty() {
                let mut r: Vec<CatalogEntry> = all
                    .iter()
                    .filter(|e| e.rom.last_played_at.is_some())
                    .cloned()
                    .collect();
                r.sort_by_key(|e| std::cmp::Reverse(e.rom.last_played_at.unwrap_or(0)));
                r.truncate(RECENT_CAP);
                r
            } else {
                Vec::new()
            };
            // Favorites sit above the recent strip (plan revision) — same
            // "only while browsing the unfiltered shelf" rule, alphabetical
            // (plan revision: "organizar favoritos em ordem alfabetica"),
            // so the row reads like a curated mini-shelf.
            favorites = if filter_query.is_empty() {
                let mut f: Vec<CatalogEntry> =
                    all.iter().filter(|e| e.rom.favorite).cloned().collect();
                f.sort_by_key(|e| e.title().to_lowercase());
                f.truncate(STRIP_CAP);
                f
            } else {
                Vec::new()
            };
            applied_filter = Some(filter_query.clone());
        }
        if sel >= view.len() {
            sel = view.len().saturating_sub(1);
        }
        let (scr_w, scr_h) = cab.shelf_screen_size();
        // How many strip tiles fit between the margins — the window the
        // strips' scroll offsets keep the cursor inside.
        let strip_vis = (((scr_w as i32 - MARGIN * 2) as u32)
            .saturating_div(RECENT_TILE_W + RECENT_GAP))
        .max(1) as usize;
        let show_recent = !recent.is_empty();
        if recent_idx >= recent.len() {
            recent_idx = recent.len().saturating_sub(1);
        }
        if !show_recent {
            in_recent = false;
        }
        recent_scroll = recent_scroll.min(recent_idx);
        if recent_idx >= recent_scroll + strip_vis {
            recent_scroll = recent_idx + 1 - strip_vis;
        }
        let show_fav = !favorites.is_empty();
        if fav_idx >= favorites.len() {
            fav_idx = favorites.len().saturating_sub(1);
        }
        if !show_fav {
            in_fav = false;
        }
        fav_scroll = fav_scroll.min(fav_idx);
        if fav_idx >= fav_scroll + strip_vis {
            fav_scroll = fav_idx + 1 - strip_vis;
        }

        // No cover art loaded anywhere in view yet: a text list (multicart
        // menu) reads as deliberate, where a grid of empty tiles reads as
        // broken (plan §3.1).
        let list_mode = !view.iter().any(|e| cab.has_image(cover_id(&e.rom.sha1)));
        let fav_block_h = if show_fav {
            FAV_LABEL_H + RECENT_TILE_H as i32 + GAP as i32
        } else {
            0
        };
        // Steam Deck (e qualquer tela baixa): as duas faixas + a grade não
        // cabem — ao favoritar, os tiles da grade viravam selos (125×94) e a
        // estante "desconfigurava". Favoritos tem prioridade: se com as duas
        // faixas os tiles da grade ficarem abaixo de ~150 de altura, a faixa
        // de jogados recentemente cede a vez (some desta tela até a janela
        // crescer ou um favorito ser removido).
        let mut show_recent = show_recent;
        if show_fav && show_recent {
            let top_com_todas = MARGIN
                + HEADER_H
                + fav_block_h
                + RECENT_LABEL_H
                + RECENT_TILE_H as i32
                + GAP as i32
                + ALL_GAMES_LABEL_H;
            let avail = scr_h as i32 - top_com_todas - MARGIN;
            let two_rows = (avail / 2 - GAP as i32).max(36);
            if two_rows < 150 {
                show_recent = false;
            }
        }
        let recent_block_h = if show_recent {
            RECENT_LABEL_H + RECENT_TILE_H as i32 + GAP as i32
        } else {
            0
        };
        if !show_recent {
            in_recent = false;
        }
        let all_games_label_y = MARGIN + HEADER_H + fav_block_h + recent_block_h;
        let grid_top_y = all_games_label_y + ALL_GAMES_LABEL_H;
        let grid = GridLayout::new(scr_w, scr_h, list_mode, grid_top_y);
        let recent_layout = RecentLayout::new(MARGIN + HEADER_H + fav_block_h + RECENT_LABEL_H);
        let fav_layout = RecentLayout::new(MARGIN + HEADER_H + FAV_LABEL_H);
        let filter_rect = filter_box_rect(scr_w);
        let history_rect = history_button_rect(scr_w);
        let refresh_rect = refresh_button_rect(scr_w);

        // Input.
        if editing_filter {
            let te = plat.poll_text_entry();
            if te.quit {
                return Ok(Pick::Quit);
            }
            if te.backspace {
                filter_draft.pop();
            }
            for c in te.typed.chars() {
                if filter_draft.chars().count() < FILTER_LIMIT {
                    filter_draft.push(c);
                }
            }
            // ⌘V/⌘C (Ctrl no resto) — mesmo esquema de clipboard dos outros
            // campos de texto.
            if let Some(paste) = &te.paste {
                for c in paste.chars() {
                    if filter_draft.chars().count() < FILTER_LIMIT {
                        filter_draft.push(c);
                    }
                }
            }
            if te.copy {
                plat.set_clipboard_text(&filter_draft);
            }
            if te.commit {
                filter_query = filter_draft.trim().to_string();
                editing_filter = false;
                plat.stop_text_input(cab);
                sel = 0;
                top_row = 0;
                in_recent = false;
            } else if te.cancel {
                editing_filter = false;
                plat.stop_text_input(cab);
            }
        } else {
            let m = plat.poll_menu(MenuMode::Nav);
            if m.quit {
                return Ok(Pick::Quit);
            }
            if zoom_close.is_some() {
                // The enlarged back cover is up (inside the tube): only the
                // "voltar" button (or Back) dismisses it — clicks elsewhere
                // do nothing. The button lives in the warped screen buffer,
                // so its rect is screen-local and goes through
                // `hit_screen_point`, not raw output coordinates.
                if m.nav.iter().any(|n| matches!(n, MenuNav::Back)) {
                    zoom_close = None;
                }
                if let Some((x, y)) = m.click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab
                        .hit_screen_point(ox, oy)
                        .is_some_and(|(lx, ly)| in_rect(lx, ly, zoom_rect))
                    {
                        zoom_close = None;
                    }
                }
            } else if manual.is_some() {
                // O leitor de manual dono do tubo: ‹/› viram página (os
                // rects vieram do frame anterior), voltar/Back fecha.
                for nav in m.nav.iter() {
                    if let Some(m) = manual.as_mut() {
                        match nav {
                            MenuNav::Left => m.page = m.page.saturating_sub(1).max(1),
                            MenuNav::Right if m.page < m.pages => m.page += 1,
                            MenuNav::Back => {
                                manual = None;
                                manual_tx = None;
                            }
                            _ => {}
                        }
                    }
                }
                if manual.is_some() {
                    if let Some((x, y)) = m.click {
                        let (ox, oy) = cab.window_to_output(x, y);
                        if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                            let (prev, next, voltar) = manual_rects;
                            if in_rect(lx, ly, prev) {
                                if let Some(mm) = manual.as_mut() {
                                    mm.page = mm.page.saturating_sub(1).max(1);
                                }
                            } else if in_rect(lx, ly, next) {
                                if let Some(mm) = manual.as_mut() {
                                    if mm.page < mm.pages {
                                        mm.page += 1;
                                    }
                                }
                            } else if in_rect(lx, ly, voltar) {
                                manual = None;
                                manual_tx = None;
                            }
                        }
                    }
                }
            } else if ach_view {
                // The achievements list owns the tube while it's up: d-pad
                // /wheel scroll, Back or "voltar" closes.
                let (_, sh) = cab.shelf_screen_size();
                let row_h = 22;
                let visible = ((sh as i32 - MARGIN * 2 - HEADER_H - 60) / row_h).max(1) as usize;
                let len = ach_data.as_ref().map_or(0, |d| d.achievements.len());
                for nav in &m.nav {
                    match nav {
                        MenuNav::Up | MenuNav::PageUp => ach_top = ach_top.saturating_sub(1),
                        MenuNav::Down | MenuNav::PageDown => {
                            if ach_top + visible < len {
                                ach_top += 1;
                            }
                        }
                        MenuNav::Home => ach_top = 0,
                        MenuNav::End => ach_top = len.saturating_sub(visible),
                        MenuNav::Back => ach_view = false,
                        _ => {}
                    }
                }
                if let Some((x, y)) = m.click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab.hit_close_button(ox, oy) {
                        return Ok(Pick::Quit);
                    }
                    if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                        continue;
                    }
                    if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                        if in_rect(lx, ly, ach_voltar) {
                            ach_view = false;
                        }
                    }
                }
            } else {
                for nav in m.nav {
                    match nav {
                        MenuNav::Left => {
                            if in_recent {
                                recent_idx = recent_idx.saturating_sub(1);
                            } else {
                                sel = sel.saturating_sub(1);
                            }
                        }
                        MenuNav::Right => {
                            if in_recent {
                                if recent_idx + 1 < recent.len() {
                                    recent_idx += 1;
                                }
                            } else if sel + 1 < view.len() {
                                sel += 1;
                            }
                        }
                        MenuNav::Up => {
                            if in_fav {
                                // Already on the topmost strip.
                            } else if in_recent {
                                if show_fav {
                                    in_recent = false;
                                    in_fav = true;
                                    fav_idx = recent_idx.min(favorites.len().saturating_sub(1));
                                }
                            } else if show_fav && sel < grid.cols {
                                in_fav = true;
                                fav_idx = sel.min(favorites.len().saturating_sub(1));
                            } else if show_recent && sel < grid.cols {
                                in_recent = true;
                                recent_idx = sel.min(recent.len().saturating_sub(1));
                            } else {
                                sel = sel.saturating_sub(grid.cols);
                            }
                        }
                        MenuNav::Down => {
                            if in_fav {
                                in_fav = false;
                                if show_recent {
                                    in_recent = true;
                                    recent_idx = fav_idx.min(recent.len().saturating_sub(1));
                                } else {
                                    sel = fav_idx.min(view.len().saturating_sub(1));
                                }
                            } else if in_recent {
                                in_recent = false;
                                sel = recent_idx.min(view.len().saturating_sub(1));
                            } else if sel + grid.cols < view.len() {
                                sel += grid.cols;
                            }
                        }
                        MenuNav::PageUp => {
                            if !in_recent {
                                sel = sel.saturating_sub(grid.cols * grid.vis_rows);
                            }
                        }
                        MenuNav::PageDown => {
                            if !in_recent {
                                sel = (sel + grid.cols * grid.vis_rows)
                                    .min(view.len().saturating_sub(1))
                            }
                        }
                        MenuNav::Home => {
                            if show_fav {
                                in_fav = true;
                                fav_idx = 0;
                            } else if show_recent {
                                in_recent = true;
                                recent_idx = 0;
                            } else {
                                sel = 0;
                            }
                        }
                        MenuNav::End => {
                            in_recent = false;
                            in_fav = false;
                            sel = view.len().saturating_sub(1);
                        }
                        MenuNav::Back => return Ok(Pick::Back),
                        MenuNav::Confirm => {
                            let picked = if in_fav {
                                favorites.get(fav_idx)
                            } else if in_recent {
                                recent.get(recent_idx)
                            } else {
                                view.get(sel)
                            };
                            if let Some(e) = picked {
                                return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                            }
                        }
                    }
                }

                // Botão direito na caixa do filtro: abre já colando — o
                // mesmo caminho curto do token nas configurações.
                if let Some((x, y)) = m.right_click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                        if in_rect(lx, ly, filter_rect) {
                            editing_filter = true;
                            filter_draft = filter_query.clone();
                            if let Some(paste) = plat.paste_from_clipboard() {
                                for c in paste.chars() {
                                    if filter_draft.chars().count() < FILTER_LIMIT {
                                        filter_draft.push(c);
                                    }
                                }
                            }
                            plat.start_text_input(cab);
                        }
                    }
                }

                // Mouse: click a tile to select it, click the already-selected one
                // to launch — the same two-step a controller does (move, then A).
                // The flat panel's "Configurações"/"Voltar" buttons (plan
                // revision: now outside the tube) are the only way into either
                // without a gamepad now.
                if let Some((x, y)) = m.click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab.hit_close_button(ox, oy) {
                        return Ok(Pick::Quit);
                    }
                    if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                    }
                    if let Some(hit) = cab.hit_shelf_button(ox, oy) {
                        match hit {
                            ShelfButton::Back => return Ok(Pick::Back),
                            ShelfButton::Settings => return Ok(Pick::Settings),
                            ShelfButton::ShelfAchievements => {
                                let picked = if in_fav {
                                    favorites.get(fav_idx)
                                } else if in_recent {
                                    recent.get(recent_idx)
                                } else {
                                    view.get(sel)
                                };
                                if let Some(e) = picked {
                                    ach_data = crate::ra::shelf_achievements(std::path::Path::new(
                                        &e.rom.path,
                                    ));
                                    if ach_data.is_some() {
                                        ach_view = true;
                                        ach_top = 0;
                                    }
                                    // A lista aberta é deste jogo: o earned do
                                    // servidor (quando chegar) só vale para ela.
                                    if let Some((user, web_key, _connect)) = &opts.ra {
                                        let hash = match ra_hashes.get(&e.rom.sha1) {
                                            Some(h) => Some(h.clone()),
                                            None => crate::ra::hash_rom(std::path::Path::new(
                                                &e.rom.path,
                                            ))
                                            .ok(),
                                        };
                                        if let Some(hash) = hash {
                                            ach_hash = Some(hash.clone());
                                            if ra_earned_tried.insert(hash.clone()) {
                                                ra_earned_worker =
                                                    Some(crate::ra::fetch_game_earned_worker(
                                                        user, web_key, &hash,
                                                    ));
                                            }
                                        }
                                    }
                                }
                            }
                            ShelfButton::Refresh => {}
                            // "Atualizar" moved to the header row (plan
                            // revision: "colocar botão de atualizar estante
                            // do lado do histórico").
                            // Salto de seção do painel (plan revision: no
                            // lugar das setas de rolar, capa traseira /
                            // cartucho / informações).
                            ShelfButton::PanelJump(section) => {
                                // Abas (plan revision): clicar troca a seção
                                // visível; clicar de novo em "informações"
                                // avança campo a campo (a única aba que
                                // transborda).
                                if panel_section != section {
                                    panel_section = section;
                                    panel_scroll = 0;
                                } else if section == PanelSection::Informacoes {
                                    panel_scroll += 1;
                                }
                            }
                            ShelfButton::ToggleFavorite => {
                                // Flip the focused game's marker in the
                                // sidecar, then update this visit's own
                                // copy of the catalog and rebuild the
                                // strips — no re-scan involved.
                                let picked = if in_fav {
                                    favorites.get(fav_idx)
                                } else if in_recent {
                                    recent.get(recent_idx)
                                } else {
                                    view.get(sel)
                                };
                                if let Some(e) = picked {
                                    let sha1 = e.rom.sha1.clone();
                                    let fav = !e.rom.favorite;
                                    if catalog.set_favorite(&sha1, fav).is_ok() {
                                        if let Some(row) =
                                            all.iter_mut().find(|e| e.rom.sha1 == sha1)
                                        {
                                            row.rom.favorite = fav;
                                        }
                                        applied_filter = None;
                                    }
                                }
                            }
                            ShelfButton::Backcover => {
                                // Open the enlarged view: re-decode the file at
                                // full resolution for a crisp enlargement (the
                                // panel texture is thumbnailed to 512).
                                let picked = if in_fav {
                                    favorites.get(fav_idx)
                                } else if in_recent {
                                    recent.get(recent_idx)
                                } else {
                                    view.get(sel)
                                };
                                if let Some(e) = picked {
                                    let bid = backcover_id(&e.rom.sha1);
                                    if cab.has_image(bid) {
                                        if let Some(path) = find_local_art(
                                            &backcover_dir,
                                            &e.rom.path,
                                            Some(&e.title()),
                                        ) {
                                            if let Ok((w, h, rgba)) = decode_art_scaled(&path, 2048)
                                            {
                                                cab.set_image(bid, w, h, &rgba);
                                                zoom_close = Some(bid);
                                            }
                                        }
                                    }
                                }
                            }
                            ShelfButton::Manual => {
                                // Abrir o leitor na TV (plan revision):
                                // worker decodifica páginas sob demanda.
                                let picked = if in_fav {
                                    favorites.get(fav_idx)
                                } else if in_recent {
                                    recent.get(recent_idx)
                                } else {
                                    view.get(sel)
                                };
                                if let Some(e) = picked {
                                    if let Some(path) =
                                        manual_paths.get(&e.rom.sha1).cloned().flatten()
                                    {
                                        let thread_path = path.clone();
                                        let open_sha1 = e.rom.sha1.clone();
                                        let (ev_tx, ev_rx) = mpsc::channel();
                                        let (req_tx, req_rx) = std::sync::mpsc::channel();
                                        std::thread::spawn(move || {
                                            let path = thread_path;
                                            let Ok(pages) = crate::manual::page_count(&path) else {
                                                return;
                                            };
                                            let _ = ev_tx.send(crate::manual::ManualEv::Ready {
                                                sha1: open_sha1.clone(),
                                                pages,
                                            });
                                            for page in req_rx {
                                                match crate::manual::render_page(&path, page, 2048)
                                                {
                                                    Ok(Some(img)) => {
                                                        let _ = ev_tx.send(
                                                            crate::manual::ManualEv::Page {
                                                                sha1: open_sha1.clone(),
                                                                page,
                                                                w: img.w,
                                                                h: img.h,
                                                                rgba: img.rgba,
                                                            },
                                                        );
                                                    }
                                                    _ => {
                                                        let _ = ev_tx.send(
                                                            crate::manual::ManualEv::Page {
                                                                sha1: open_sha1.clone(),
                                                                page,
                                                                w: 0,
                                                                h: 0,
                                                                rgba: Vec::new(),
                                                            },
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                        manual = Some(ManualState {
                                            sha1: e.rom.sha1.clone(),
                                            pages: manual_pages
                                                .get(&e.rom.sha1)
                                                .copied()
                                                .unwrap_or(0),
                                            page: 1,
                                            requested: HashSet::new(),
                                        });
                                        manual_channels.push(ev_rx);
                                        manual_tx = Some(req_tx);
                                    }
                                }
                            }
                            ShelfButton::Cartridge => {
                                // Zoom do cartucho, igual ao do back cover
                                // (plan revision): re-decodifica a arte em
                                // resolução cheia e abre ampliado no tubo.
                                let picked = if in_fav {
                                    favorites.get(fav_idx)
                                } else if in_recent {
                                    recent.get(recent_idx)
                                } else {
                                    view.get(sel)
                                };
                                if let Some(e) = picked {
                                    let cid = cartridge_id(&e.rom.sha1);
                                    if cab.has_image(cid) {
                                        if let Some(path) = find_local_art(
                                            &cartridge_dir,
                                            &e.rom.path,
                                            Some(&e.title()),
                                        ) {
                                            if let Ok((w, h, rgba)) = decode_art_scaled(&path, 2048)
                                            {
                                                cab.set_image(cid, w, h, &rgba);
                                                zoom_close = Some(cid);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                        if in_rect(lx, ly, filter_rect) {
                            editing_filter = true;
                            filter_draft = filter_query.clone();
                            plat.start_text_input(cab);
                        } else if in_rect(lx, ly, history_rect) {
                            return Ok(Pick::History);
                        } else if in_rect(lx, ly, refresh_rect) {
                            return Ok(Pick::Refresh);
                        } else if in_rect(lx, ly, strip_arrow_rects(scr_w, fav_layout.y0).0)
                            && fav_scroll > 0
                        {
                            fav_scroll -= 1;
                            if fav_idx > fav_scroll + strip_vis - 1 {
                                fav_idx = fav_scroll + strip_vis - 1;
                            }
                        } else if in_rect(lx, ly, strip_arrow_rects(scr_w, fav_layout.y0).1)
                            && fav_scroll + strip_vis < favorites.len()
                        {
                            fav_scroll += 1;
                            if fav_idx < fav_scroll {
                                fav_idx = fav_scroll;
                            }
                        } else if in_rect(lx, ly, strip_arrow_rects(scr_w, recent_layout.y0).0)
                            && recent_scroll > 0
                        {
                            recent_scroll -= 1;
                            if recent_idx > recent_scroll + strip_vis - 1 {
                                recent_idx = recent_scroll + strip_vis - 1;
                            }
                        } else if in_rect(lx, ly, strip_arrow_rects(scr_w, recent_layout.y0).1)
                            && recent_scroll + strip_vis < recent.len()
                        {
                            recent_scroll += 1;
                            if recent_idx < recent_scroll {
                                recent_idx = recent_scroll;
                            }
                        } else if let Some(i) = show_fav
                            .then(|| {
                                fav_layout
                                    .hit(lx, ly, strip_vis)
                                    .map(|i| i + fav_scroll)
                                    .filter(|i| *i < favorites.len())
                            })
                            .flatten()
                        {
                            if in_fav && i == fav_idx {
                                if let Some(e) = favorites.get(i) {
                                    return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                                }
                            } else {
                                in_recent = false;
                                in_fav = true;
                                fav_idx = i;
                            }
                        } else if let Some(i) = show_recent
                            .then(|| {
                                recent_layout
                                    .hit(lx, ly, strip_vis)
                                    .map(|i| i + recent_scroll)
                                    .filter(|i| *i < recent.len())
                            })
                            .flatten()
                        {
                            if in_recent && i == recent_idx {
                                if let Some(e) = recent.get(i) {
                                    return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                                }
                            } else {
                                in_fav = false;
                                in_recent = true;
                                recent_idx = i;
                            }
                        } else if let Some(i) = grid.tile_at(lx, ly, top_row, view.len()) {
                            if !in_recent && i == sel {
                                if let Some(e) = view.get(i) {
                                    return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                                }
                            } else {
                                in_recent = false;
                                in_fav = false;
                                sel = i;
                            }
                        }
                    }
                }
            }
        }

        // Keep selection visible.
        let sel_row = sel.checked_div(grid.cols).unwrap_or(0);
        if sel_row < top_row {
            top_row = sel_row;
        } else if sel_row >= top_row + grid.vis_rows {
            top_row = sel_row + 1 - grid.vis_rows;
        }

        // Local art for whatever just scrolled into view — no network, no
        // worker thread, so this happens inline, capped at ONE decode per
        // frame: scrolling a full page fills over a dozen frames instead of
        // hitching on dozens of JPEG decodes in a single one. Each sha1 is
        // tried at most once per visit, whether or not a file turns up; a
        // tile whose budget ran out simply waits for the next frame.
        let mut decode_budget = 1usize;
        for (i, entry) in view.iter().enumerate() {
            if grid.cell_pos(i, top_row).is_none() {
                continue;
            }
            let id = cover_id(&entry.rom.sha1);
            if cab.has_image(id) {
                continue;
            }
            if decode_budget == 0 {
                break;
            }
            if !tried_cover.insert(entry.rom.sha1.clone()) {
                continue;
            }
            if let Some(path) = find_local_art(&cover_dir, &entry.rom.path, Some(&entry.title())) {
                if let Ok((w, h, rgba)) = decode_art(&path) {
                    cab.set_image(id, w, h, &rgba);
                    decode_budget -= 1;
                }
            }
        }
        // The favorites strip is outside the scroll window too — its own
        // pass, same one-decode-per-frame cap.
        let mut fav_budget = 1usize;
        for (i, entry) in favorites
            .iter()
            .enumerate()
            .skip(fav_scroll)
            .take(strip_vis)
        {
            if fav_budget == 0 {
                break;
            }
            let id = cover_id(&entry.rom.sha1);
            if cab.has_image(id) {
                continue;
            }
            if !tried_cover.insert(entry.rom.sha1.clone()) {
                continue;
            }
            if let Some(path) = find_local_art(&cover_dir, &entry.rom.path, Some(&entry.title())) {
                if let Ok((w, h, rgba)) = decode_art(&path) {
                    cab.set_image(id, w, h, &rgba);
                    fav_budget -= 1;
                }
            }
            let _ = i;
        }
        // The recent strip sits outside the main grid's scroll window, so it
        // needs its own pass — a recently-played game might not be among the
        // rows currently visible below. Same one-decode-per-frame cap.
        let mut recent_budget = 1usize;
        for (i, entry) in recent
            .iter()
            .enumerate()
            .skip(recent_scroll)
            .take(strip_vis)
        {
            if recent_budget == 0 {
                break;
            }
            let id = cover_id(&entry.rom.sha1);
            if cab.has_image(id) {
                continue;
            }
            if !tried_cover.insert(entry.rom.sha1.clone()) {
                continue;
            }
            if let Some(path) = find_local_art(&cover_dir, &entry.rom.path, Some(&entry.title())) {
                if let Ok((w, h, rgba)) = decode_art(&path) {
                    cab.set_image(id, w, h, &rgba);
                    recent_budget -= 1;
                }
            }
            let _ = i;
        }

        let focused: Option<&CatalogEntry> = if in_fav {
            favorites.get(fav_idx)
        } else if in_recent {
            recent.get(recent_idx)
        } else {
            view.get(sel)
        };
        // A new game focused starts its panel scrolled to the top again —
        // otherwise switching from a long-content game to a short one could
        // leave the scroll position pointing past the end of the new one
        // until `draw_shelf_panel`'s own clamp kicks in.
        let zone = |in_fav: bool, in_recent: bool| {
            if in_fav {
                0u8
            } else if in_recent {
                1
            } else {
                2
            }
        };
        let focus_key = focused.is_some().then_some((
            zone(in_fav, in_recent),
            if in_fav {
                fav_idx
            } else if in_recent {
                recent_idx
            } else {
                sel
            },
        ));
        if focus_key != last_focus {
            panel_scroll = 0;
            last_focus = focus_key;
        }
        if let Some(e) = focused {
            let wid = wheel_id(&e.rom.sha1);
            if !cab.has_image(wid) && tried_logo.insert(e.rom.sha1.clone()) {
                if let Some(path) = find_local_art(&logo_dir, &e.rom.path, Some(&e.title())) {
                    if let Ok((w, h, rgba)) = decode_art(&path) {
                        cab.set_image(wid, w, h, &rgba);
                    }
                }
            }
            let cid = cartridge_id(&e.rom.sha1);
            if !cab.has_image(cid) && tried_cartridge.insert(e.rom.sha1.clone()) {
                if let Some(path) = find_local_art(&cartridge_dir, &e.rom.path, Some(&e.title())) {
                    if let Ok((w, h, rgba)) = decode_art(&path) {
                        cab.set_image(cid, w, h, &rgba);
                    }
                }
            }
            let bid = backcover_id(&e.rom.sha1);
            if !cab.has_image(bid) && tried_backcover.insert(e.rom.sha1.clone()) {
                if let Some(path) = find_local_art(&backcover_dir, &e.rom.path, Some(&e.title())) {
                    if let Ok((w, h, rgba)) = decode_art(&path) {
                        cab.set_image(bid, w, h, &rgba);
                    }
                }
            }
        }

        // RA identification (plan fase 2): drain the in-flight reply, and
        // kick the next one for the focused game when the account is on.
        if let Some(rx) = &ra_worker {
            match rx.try_recv() {
                Ok(Ok(g)) => {
                    ra_games.insert(ra_pending.clone(), g);
                    ra_worker = None;
                }
                Ok(Err(e)) => {
                    log::info!("ra identify {}: {e}", ra_pending);
                    ra_games.insert(ra_pending.clone(), None);
                    ra_worker = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => ra_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &ra_completion_worker {
            match rx.try_recv() {
                Ok(Ok(map)) => {
                    log::info!("ra: progresso do perfil carregado ({} jogos)", map.len());
                    ra_completion = Some(map);
                    // Tallys já montados nasceram sem o servidor — refazer.
                    ra_tally.clear();
                    ra_completion_worker = None;
                }
                Ok(Err(e)) => {
                    log::info!("ra completion: {e}");
                    ra_completion_worker = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => ra_completion_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if !ra_completion_tried && opts.ra.is_some() {
            ra_completion_tried = true;
            let (tx, rx) = mpsc::channel();
            let (user, web_key, _connect) = opts.ra.clone().unwrap();
            std::thread::spawn(move || {
                let _ = tx.send(crate::ra::fetch_completion(&user, &web_key));
            });
            ra_completion_worker = Some(rx);
        }
        // Earned do servidor chegando: o set já foi unido no disco — a lista
        // aberta deste jogo ganha os [x] (e o "X/Y" do cabeçalho) na hora.
        if let Some(rx) = &ra_earned_worker {
            match rx.try_recv() {
                Ok((hash, Some(ids))) => {
                    if ach_hash.as_deref() == Some(hash.as_str()) {
                        if let Some(list) = ach_data.as_mut() {
                            list.earned = ids;
                        }
                    }
                    ra_earned_worker = None;
                }
                Ok((hash, None)) => {
                    log::info!("ra earned {hash}: sem ids do servidor");
                    ra_earned_worker = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => ra_earned_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        // Badges do jogo aberto que já estão no disco viram texturas — umas
        // poucas por frame para o primeiro carregamento não engasgar. O
        // prefetch continua baixando em background: na medida que os PNGs
        // aparecem, os badges entram na lista (de qualquer jogo aberto).
        if ach_view {
            if let Some(list) = ach_data.as_ref() {
                let mut novos = 0;
                for a in &list.achievements {
                    if novos >= 4 {
                        break;
                    }
                    if badge_ids.contains_key(&a.badge) {
                        continue;
                    }
                    if let Some(path) = crate::ra::badge_path(&a.badge) {
                        if let Ok(img) = image::open(&path).map(|i| i.to_rgba8()) {
                            let id = badge_image_id(&a.badge);
                            cab.set_image(id, img.width(), img.height(), img.as_raw());
                            badge_ids.insert(a.badge.clone(), id);
                            novos += 1;
                        }
                    }
                }
            }
        }
        if ra_worker.is_none() {
            if let (Some((user, web_key, connect)), Some(e)) = (&opts.ra, focused.as_ref()) {
                let sha1 = e.rom.sha1.clone();
                if !ra_tried.contains(&sha1) {
                    ra_tried.insert(sha1.clone());
                    match crate::ra::hash_rom(std::path::Path::new(&e.rom.path)) {
                        Err(err) => {
                            log::info!("ra hash {}: {err}", e.rom.path);
                            ra_games.insert(sha1, None);
                        }
                        Ok(hash) => {
                            ra_hashes.insert(sha1.clone(), hash.clone());
                            if let Some(g) = crate::ra::cached_game(&hash) {
                                ra_games.insert(sha1, Some(g));
                            } else if crate::ra::is_cached_unknown(&hash) {
                                ra_games.insert(sha1, None);
                            } else {
                                let (tx, rx) = mpsc::channel();
                                let (user, web_key, connect) =
                                    (user.clone(), web_key.clone(), connect.clone());
                                std::thread::spawn(move || {
                                    let _ = tx.send(crate::ra::fetch_game(
                                        &user, &web_key, &connect, &hash,
                                    ));
                                });
                                ra_worker = Some(rx);
                                ra_pending = sha1;
                            }
                        }
                    }
                }
            }
        }

        // Manual (plan revision): eventos do worker — contagem ao abrir e
        // páginas renderizadas viram texturas; a página corrente (e a
        // seguinte, de brincadeira) são pedidas assim que faltam.
        let mut live_channels = Vec::with_capacity(manual_channels.len());
        for rx in manual_channels.drain(..) {
            match rx.try_recv() {
                Ok(crate::manual::ManualEv::Ready { sha1, pages }) => {
                    manual_pages.insert(sha1.clone(), pages);
                    if let Some(m) = manual.as_mut() {
                        if m.sha1 == sha1 {
                            m.pages = pages;
                        }
                    }
                    live_channels.push(rx);
                }
                Ok(crate::manual::ManualEv::Page {
                    sha1,
                    page,
                    w,
                    h,
                    rgba,
                }) => {
                    // A textura é registrada SEMPRE (preview incluído) — o
                    // registro anterior dependia do leitor estar aberto e
                    // descartava a página do preview (plan revision: "a
                    // primeira pgina nao aparece no painel").
                    if w > 0 {
                        cab.set_image(manual_page_id(&sha1, page), w, h, &rgba);
                    }
                    if let Some(m) = manual.as_mut() {
                        if m.sha1 == sha1 {
                            m.requested.insert(page);
                        }
                    }
                    live_channels.push(rx);
                }
                // O worker morreu (fechou sem página nenhuma) — sem canal.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => live_channels.push(rx),
            }
        }
        manual_channels = live_channels;
        if let (Some(m), Some(tx)) = (manual.as_mut(), manual_tx.as_ref()) {
            if m.pages > 0 && !m.requested.contains(&m.page) {
                m.requested.insert(m.page);
                let _ = tx.send(m.page);
            }
            let next_page = m.page + 1;
            if next_page <= m.pages && !m.requested.contains(&next_page) {
                m.requested.insert(next_page);
                let _ = tx.send(next_page);
            }
        }

        // The flat side panel (plan revision — drawn by `Cabinet` itself,
        // outside the tube): whatever's focused right now, DAT extras and
        // all when the DAT had any ("se o DAT tiver informacoes do jogo,
        // preencher no painel").
        let shelf_panel = match focused {
            Some(e) => {
                let wid = wheel_id(&e.rom.sha1);
                let cid = cartridge_id(&e.rom.sha1);
                let bid = backcover_id(&e.rom.sha1);
                let save_title = crate::runner::rom_title(Path::new(&e.rom.path));
                let playtime =
                    crate::runner::total_playtime_secs(&crate::dirs::saves_dir(), &save_title);
                // The DAT's year, if it had one, moves into its own "release"
                // slot (plan revision) instead of sitting in the generic
                // info list alongside publisher/category/description.
                let mut nointro = e.rom.nointro_extra.clone();
                let dat_release = nointro
                    .iter()
                    .position(|(k, _)| k == "ano")
                    .map(|i| nointro.remove(i).1);
                let mut info = nointro;
                let mut release = dat_release;
                let mut award_img: Option<u64> = None;
                if let Some(Some(_)) = ra_games.get(&e.rom.sha1) {
                    // O RA completa o que o DAT/TOSEC não cobre — muitos jogos
                    // ficavam sem lançamento e editora por não estarem na
                    // tabela (uma leitura do JSON da identificação por foco).
                    let ra_meta_entry = ra_meta
                        .entry(e.rom.sha1.clone())
                        .or_insert_with(|| {
                            ra_hashes
                                .get(&e.rom.sha1)
                                .map(|h| crate::ra::release_and_extras(h))
                                .unwrap_or((None, Vec::new()))
                        })
                        .clone();
                    release = release.or(ra_meta_entry.0);
                    if !info.iter().any(|(k, _)| k == "editora") {
                        if let Some(editora) = ra_meta_entry.1.first() {
                            info.push(editora.clone());
                        }
                    }
                    // "30 de 58 (52%)" — o tally custa um hash de ROM inteiro,
                    // então fica cacheado por visita (e refresco quando o
                    // completion progress do servidor chega).
                    let tally = *ra_tally.entry(e.rom.sha1.clone()).or_insert_with(|| {
                        ra_hashes
                            .get(&e.rom.sha1)
                            .and_then(|h| crate::ra::tally_from_cache(h, ra_completion.as_ref()))
                    });
                    // A medalha de prêmio do RA desenhada ao lado do número
                    // da linha de conquistas (pixel-art gerada por
                    // `medal_rgba`, uma textura por variante).
                    if let Some((earned, total, award)) = tally {
                        let pct = (earned * 100 + total / 2).checked_div(total).unwrap_or(0);
                        info.push((
                            "conquistas".to_string(),
                            format!("{earned} de {total} ({pct}%)"),
                        ));
                        // "zerado (softcore)" / "mastered" — o prêmio em
                        // palavras, junto das infos (plan revision).
                        if let Some(a) = award {
                            info.push(("status".to_string(), a.label().to_string()));
                        }
                        if let Some(medal) = award.map(|a| a.medal()) {
                            let id = crate::ra::medal_image_id(medal);
                            if !cab.has_image(id) {
                                let (w, h, rgba) = crate::ra::medal_rgba(medal);
                                cab.set_image(id, w, h, &rgba);
                            }
                            award_img = Some(id);
                        }
                    }
                    // Os badges do set já baixando em background — a notificação
                    // e a lista da estante os encontram em cache na hora.
                    if badge_tried.insert(e.rom.sha1.clone()) {
                        crate::ra::prefetch_badges(std::path::Path::new(&e.rom.path));
                    }
                }
                info.extend(game_info_lines(e, playtime));
                let manual_path = manual_paths
                    .entry(e.rom.sha1.clone())
                    .or_insert_with(|| find_local_manual(&manual_dir, &e.rom.path))
                    .clone();
                // Preview: contagem + primeira página decodificadas uma vez
                // por jogo — a aba manual as mostra como as artes mostram
                // capa/cartucho (plan revision).
                if let Some(preview_path) = &manual_path {
                    if manual_preview_tried.insert(e.rom.sha1.clone()) {
                        let (ev_tx, ev_rx) = mpsc::channel();
                        let preview_path = preview_path.clone();
                        let preview_sha1 = e.rom.sha1.clone();
                        std::thread::spawn(move || {
                            let pages = crate::manual::page_count(&preview_path).unwrap_or(0);
                            let _ = ev_tx.send(crate::manual::ManualEv::Ready {
                                sha1: preview_sha1.clone(),
                                pages,
                            });
                            if let Ok(Some(img)) = crate::manual::render_page(&preview_path, 1, 512)
                            {
                                let _ = ev_tx.send(crate::manual::ManualEv::Page {
                                    sha1: preview_sha1.clone(),
                                    page: 1,
                                    w: img.w,
                                    h: img.h,
                                    rgba: img.rgba,
                                });
                            }
                        });
                        manual_channels.push(ev_rx);
                    }
                }
                let ra_state = ra_games.get(&e.rom.sha1);
                ShelfPanelInfo {
                    title: e.title().into_owned(),
                    logo_img: cab.has_image(wid).then_some(wid),
                    cartridge_img: cab.has_image(cid).then_some(cid),
                    backcover_img: cab.has_image(bid).then_some(bid),
                    release,
                    info,
                    section: panel_section,
                    scroll: panel_scroll,
                    favorite: Some(e.rom.favorite),
                    // Identificado = botão desenhado; hash marcado "versão
                    // não suportada" = botão desativado com o motivo
                    // (plan revision).
                    achievements: ra_state.is_some_and(|g| g.is_some()),
                    achievements_reason: ra_state
                        .and_then(|g| g.as_ref())
                        .filter(|g| g.unsupported)
                        .map(|_| "versão sem suporte".to_string()),
                    manual_first_page: manual_path
                        .is_some()
                        .then(|| manual_page_id(&e.rom.sha1, 1)),
                    award_img,
                }
            }
            None => ShelfPanelInfo {
                title: "nenhum jogo encontrado".to_string(),
                logo_img: None,
                cartridge_img: None,
                backcover_img: None,
                release: None,
                info: Vec::new(),
                section: PanelSection::Informacoes,
                scroll: 0,
                favorite: None,
                achievements: false,
                achievements_reason: None,
                manual_first_page: None,
                award_img: None,
            },
        };
        cab.set_shelf_panel(shelf_panel);

        // --- draw (into a screen-sized buffer, then warped through the tube) --
        let page_ready = manual
            .as_ref()
            .is_some_and(|m| cab.has_image(manual_page_id(&m.sha1, m.page)));
        let render = |d: &mut Screen| {
            if let Some(m) = &manual {
                manual_rects = draw_manual_view(d, &m.sha1, m.page, m.pages, page_ready);
                return;
            }
            if ach_view {
                // The list owns the tube (plan revision: "mostrar a lista de
                // conquistas e pontuação total dentro da tv com botão de
                // voltar, parecido com o back cover"); the button rect comes
                // back for the click hit-test.
                ach_voltar = draw_achievements_view(d, ach_data.as_ref(), &badge_ids, ach_top);
                return;
            }
            let count_label = if filter_query.is_empty() {
                format!("{} games", view.len())
            } else {
                format!("{} de {} games", view.len(), all.len())
            };
            d.text(MARGIN, MARGIN - 12, 2, DIM, &count_label);
            draw_filter_box(d, filter_rect, &filter_query, &filter_draft, editing_filter);
            draw_history_button(d, history_rect);
            draw_refresh_button(d, refresh_rect);

            if show_fav {
                d.text(MARGIN, MARGIN + HEADER_H, 1, DIM, FAV_TITLE);
                for (i, entry) in favorites
                    .iter()
                    .enumerate()
                    .skip(fav_scroll)
                    .take(strip_vis)
                {
                    // Window-relative position — the strip slides left as it
                    // scrolls, not the absolute slot that would leave a
                    // blank gap at the start.
                    let (x, y) = fav_layout.item_pos(i - fav_scroll);
                    d.fill(x, y, RECENT_TILE_W, RECENT_TILE_H, TILE_BG);
                    let id = cover_id(&entry.rom.sha1);
                    if d.has_image(id) {
                        d.image_fit(id, x + 4, y + 4, RECENT_TILE_W - 8, RECENT_TILE_H - 8);
                    } else {
                        d.text_wrapped(
                            x + 6,
                            y + 8,
                            RECENT_TILE_W - 12,
                            1,
                            DIM,
                            &entry.title().to_uppercase(),
                        );
                    }
                    if in_fav && i == fav_idx {
                        d.outline(
                            x - 3,
                            y - 3,
                            RECENT_TILE_W + 6,
                            RECENT_TILE_H + 6,
                            3,
                            HILITE,
                        );
                    }
                }
                draw_strip_arrows(
                    d,
                    scr_w,
                    fav_layout.y0,
                    fav_scroll,
                    favorites.len(),
                    strip_vis,
                );
            }

            if show_recent {
                d.text(
                    MARGIN,
                    MARGIN + HEADER_H + fav_block_h,
                    1,
                    DIM,
                    "jogados recentemente",
                );
                for (i, entry) in recent
                    .iter()
                    .enumerate()
                    .skip(recent_scroll)
                    .take(strip_vis)
                {
                    let (x, y) = recent_layout.item_pos(i - recent_scroll);
                    d.fill(x, y, RECENT_TILE_W, RECENT_TILE_H, TILE_BG);
                    let id = cover_id(&entry.rom.sha1);
                    if d.has_image(id) {
                        d.image_fit(id, x + 4, y + 4, RECENT_TILE_W - 8, RECENT_TILE_H - 8);
                    } else {
                        d.text_wrapped(
                            x + 6,
                            y + 8,
                            RECENT_TILE_W - 12,
                            1,
                            DIM,
                            &entry.title().to_uppercase(),
                        );
                    }
                    if in_recent && i == recent_idx {
                        d.outline(
                            x - 3,
                            y - 3,
                            RECENT_TILE_W + 6,
                            RECENT_TILE_H + 6,
                            3,
                            HILITE,
                        );
                    }
                }
                draw_strip_arrows(
                    d,
                    scr_w,
                    recent_layout.y0,
                    recent_scroll,
                    recent.len(),
                    strip_vis,
                );
            }

            d.text(MARGIN, all_games_label_y, 1, DIM, ALL_GAMES_TITLE);

            if grid.list_mode {
                // Multicart menu: a plain numbered list, selection as an
                // inverted bar — no tile, no art, nothing pretending there's
                // a cover coming.
                for (i, entry) in view.iter().enumerate() {
                    let Some((x, y)) = grid.cell_pos(i, top_row) else {
                        continue;
                    };
                    let selected = !in_recent && i == sel;
                    if selected {
                        d.fill(x, y, grid.item_w as u32, grid.item_h as u32, HILITE);
                    }
                    let label = format!("{:03}  {}", i + 1, entry.title().to_uppercase());
                    let color = if selected { HILITE_TEXT } else { TEXT };
                    d.text(x + 6, y + 3, 1, color, &label);
                }
            } else {
                for (i, entry) in view.iter().enumerate() {
                    let Some((x, y)) = grid.cell_pos(i, top_row) else {
                        continue;
                    };
                    d.fill(x, y, TILE_W, TILE_H, TILE_BG);
                    let id = cover_id(&entry.rom.sha1);
                    if d.has_image(id) {
                        d.image_fit(id, x + 4, y + 4, TILE_W - 8, TILE_H - 8);
                    } else {
                        d.text_wrapped(
                            x + 8,
                            y + 10,
                            TILE_W - 16,
                            1,
                            DIM,
                            &entry.title().to_uppercase(),
                        );
                    }
                    if !in_recent && !in_fav && i == sel {
                        d.outline(x - 3, y - 3, TILE_W + 6, TILE_H + 6, 3, HILITE);
                    }
                }
            }

            draw_scrollbar(d, &grid, view.len(), top_row, scr_w);
        };

        if let (Some(path), true) = (&opts.shot, opts.max_frames == Some(frame_no)) {
            cab.capture_shelf(BG, render, path)?;
            return Ok(Pick::Quit);
        }
        // The shelf sits on the same tube as the game — the signal-off snow
        // stays faintly visible underneath it the whole time (plan §3.3),
        // not just during the entrance. Easing in from a fresh eject just
        // ramps *toward* that resting `SHELF_ALPHA` instead of straight to
        // fully opaque.
        // Back cover enlarged: it replaces the tube's content while up —
        // drawn through the same warp, on the TV — and the flat panel
        // keeps showing beside it.
        if let Some(img) = zoom_close {
            match focused {
                Some(_) => {
                    let r = cab.frame_image_zoom(img, "voltar");
                    zoom_rect = (r.x(), r.y(), r.width(), r.height());
                }
                None => zoom_close = None,
            }
        } else {
            match opts.fade_in {
                Some(level) if fade_frame < FADE_IN_FRAMES => {
                    fade_frame += 1;
                    let alpha = (fade_frame as f32 / FADE_IN_FRAMES as f32) * SHELF_ALPHA;
                    cab.frame_shelf_fade_in(BG, render, level, alpha);
                }
                _ => cab.frame_shelf_fade_in(BG, render, idle::RESTING_STATIC, SHELF_ALPHA),
            }
        }

        crate::runner::pace_frame(&mut next, frame);
    }
}

/// A simple, scrollable ranking of every game with any recorded playtime,
/// most time played first (plan revision: "colocar um botao para historico
/// listando os jogos mais jogados em todo periodo") — the shelf's own
/// "Histórico" button. Clicking a row launches it, same as the shelf's own
/// grid; "Voltar" here means "back to the shelf", not all the way to idle
/// (the caller, `polystnx.rs`'s main loop, treats this call's `Pick::Back`
/// that way, same as it already special-cases `Pick::Settings`).
pub fn run_history(plat: &mut Platform, cab: &mut Cabinet, catalog: &Catalog) -> Result<Pick> {
    let logo_dir = crate::dirs::assets_dir().join("logo");
    let cartridge_dir = crate::dirs::assets_dir().join("disc");
    let ranked = ranked_by_playtime(catalog)?;

    let mut sel: usize = 0;
    let mut top: usize = 0;
    let frame = Duration::from_millis(16);

    cab.set_shelf_panel(empty_shelf_panel());
    cab.set_close_button(true);

    loop {
        let (scr_w, scr_h) = cab.shelf_screen_size();
        let vis_rows = ((scr_h as i32 - MARGIN * 2 - HEADER_H) / HISTORY_ROW_H).max(1) as usize;
        if sel >= ranked.len() {
            sel = ranked.len().saturating_sub(1);
        }

        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        for nav in &m.nav {
            match nav {
                MenuNav::Up => sel = sel.saturating_sub(1),
                MenuNav::Down if sel + 1 < ranked.len() => sel += 1,
                MenuNav::PageUp => sel = sel.saturating_sub(vis_rows),
                MenuNav::PageDown => sel = (sel + vis_rows).min(ranked.len().saturating_sub(1)),
                MenuNav::Home => sel = 0,
                MenuNav::End => sel = ranked.len().saturating_sub(1),
                MenuNav::Back => return Ok(Pick::Back),
                MenuNav::Confirm => {
                    if let Some((e, _)) = ranked.get(sel) {
                        return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                    }
                }
                _ => {}
            }
        }
        if sel < top {
            top = sel;
        } else if sel >= top + vis_rows {
            top = sel + 1 - vis_rows;
        }

        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return Ok(Pick::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                continue;
            }
            if let Some(hit) = cab.hit_shelf_button(ox, oy) {
                match hit {
                    ShelfButton::Back => return Ok(Pick::Back),
                    ShelfButton::Settings => return Ok(Pick::Settings),
                    ShelfButton::Refresh => return Ok(Pick::Refresh),
                    // The history screen's panel never has any per-game
                    // content, so it never scrolls (and never has a back
                    // cover to enlarge).
                    ShelfButton::Backcover | ShelfButton::Cartridge | ShelfButton::Manual => {}
                    ShelfButton::ToggleFavorite => {}
                    ShelfButton::ShelfAchievements => {}
                    ShelfButton::PanelJump(_) => {}
                }
            } else if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                if let Some(i) = history_row_at(lx, ly, top, ranked.len()) {
                    if i == sel {
                        let (e, _) = &ranked[i];
                        return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                    }
                    sel = i;
                }
            }
        }

        let render =
            |d: &mut Screen| draw_history_list(d, &ranked, top, vis_rows, Some(sel), scr_w);
        cab.frame_shelf(BG, render);

        std::thread::sleep(frame);
    }
}

/// Headless preview of the history screen (dev/testing) — same setup as
/// `run_history`, one frame captured through the tube instead of a live loop.
pub fn capture_history_preview(cab: &mut Cabinet, catalog: &Catalog, path: &Path) -> Result<()> {
    let ranked = ranked_by_playtime(catalog)?;
    cab.set_shelf_panel(empty_shelf_panel());
    cab.set_close_button(true);
    let (scr_w, scr_h) = cab.shelf_screen_size();
    let vis_rows = ((scr_h as i32 - MARGIN * 2 - HEADER_H) / HISTORY_ROW_H).max(1) as usize;
    let render = |d: &mut Screen| draw_history_list(d, &ranked, 0, vis_rows, None, scr_w);
    cab.capture_shelf(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Every scanned game with `total_playtime_secs > 0`, most time played
/// first — a game never (knowingly) turned on doesn't clutter a "most
/// played" ranking.
fn ranked_by_playtime(catalog: &Catalog) -> Result<Vec<(CatalogEntry, u64)>> {
    let saves_dir = crate::dirs::saves_dir();
    let mut ranked: Vec<(CatalogEntry, u64)> = catalog
        .list(Order::Name)?
        .into_iter()
        .map(|e| {
            let title = crate::runner::rom_title(Path::new(&e.rom.path));
            let secs = crate::runner::total_playtime_secs(&saves_dir, &title);
            (e, secs)
        })
        .filter(|(_, secs)| *secs > 0)
        .collect();
    ranked.sort_by_key(|(_, secs)| std::cmp::Reverse(*secs));
    Ok(ranked)
}

/// A blank flat panel — just the Voltar/Configuracoes buttons, no game
/// focused (the history list isn't a "selection" the way the shelf's grid
/// is; clicking a row launches straight away instead of just selecting it).
fn empty_shelf_panel() -> ShelfPanelInfo {
    ShelfPanelInfo {
        section: PanelSection::Informacoes,
        achievements_reason: None,
        manual_first_page: None,
        title: String::new(),
        logo_img: None,
        cartridge_img: None,
        backcover_img: None,
        release: None,
        info: Vec::new(),
        scroll: 0,
        favorite: None,
        achievements: false,
        award_img: None,
    }
}

const HISTORY_ROW_H: i32 = 28;

/// Which visible ranked row (if any) a screen-local point sits inside.
fn history_row_at(x: i32, y: i32, top: usize, len: usize) -> Option<usize> {
    let row_top = MARGIN + HEADER_H;
    if x < MARGIN || y < row_top {
        return None;
    }
    let i = top + ((y - row_top) / HISTORY_ROW_H) as usize;
    (i < len).then_some(i)
}

fn draw_history_list(
    d: &mut Screen,
    ranked: &[(CatalogEntry, u64)],
    top: usize,
    vis_rows: usize,
    sel: Option<usize>,
    scr_w: u32,
) {
    d.text(MARGIN, MARGIN - 12, 2, DIM, "histórico - mais jogados");
    if ranked.is_empty() {
        d.text_wrapped(
            MARGIN,
            MARGIN + 30,
            scr_w.saturating_sub(MARGIN as u32 * 2),
            1,
            DIM,
            "nenhum jogo tem tempo registrado ainda -- jogue algo com o console ligado.",
        );
        return;
    }
    let row_w = scr_w.saturating_sub(MARGIN as u32 * 2);
    for (i, (entry, secs)) in ranked.iter().enumerate().skip(top).take(vis_rows) {
        let y = MARGIN + HEADER_H + (i - top) as i32 * HISTORY_ROW_H;
        let selected = sel == Some(i);
        if selected {
            d.fill(MARGIN, y, row_w, HISTORY_ROW_H as u32 - 4, HILITE);
        }
        let color = if selected { HILITE_TEXT } else { TEXT };
        let label = format!("{:02}. {}", i + 1, entry.title().to_uppercase());
        d.text(MARGIN + 6, y + 4, 1, color, &label);
        let time_label = format_playtime(*secs);
        let tw = time_label.chars().count() as i32 * 9;
        d.text(MARGIN + row_w as i32 - tw - 6, y + 4, 1, color, &time_label);
    }
}

fn in_rect(x: i32, y: i32, (rx, ry, rw, rh): (i32, i32, u32, u32)) -> bool {
    x >= rx && y >= ry && x < rx + rw as i32 && y < ry + rh as i32
}

/// The title filter box (plan revision): shows the live draft with a text
/// cursor while typing, the applied query otherwise, or a placeholder when
/// empty — long text scrolls from the right so the cursor stays visible.
fn draw_filter_box(
    d: &mut Screen,
    (fx, fy, fw, fh): (i32, i32, u32, u32),
    query: &str,
    draft: &str,
    editing: bool,
) {
    d.outline(fx, fy, fw, fh, 1, (150, 150, 158, 255));
    d.fill(fx + 1, fy + 1, fw - 2, fh - 2, (34, 34, 40, 255));
    let (raw, color) = if editing {
        (format!("{draft}_"), TEXT)
    } else if !query.is_empty() {
        (query.to_string(), TEXT)
    } else {
        ("buscar...".to_string(), DIM)
    };
    let max_chars = ((fw as i32 - 16) / 9).max(4) as usize;
    let shown: String = if raw.chars().count() > max_chars {
        let tail: Vec<char> = raw.chars().rev().take(max_chars).collect();
        tail.into_iter().rev().collect()
    } else {
        raw
    };
    d.text(fx + 8, fy + (fh as i32 - 16) / 2, 1, color, &shown);
}

/// The "Histórico" button — same plain box look as the filter box, just a
/// static label (plan revision: "colocar um botão para histórico").
fn draw_history_button(d: &mut Screen, (bx, by, bw, bh): (i32, i32, u32, u32)) {
    d.outline(bx, by, bw, bh, 1, (150, 150, 158, 255));
    d.fill(bx + 1, by + 1, bw - 2, bh - 2, (34, 34, 40, 255));
    d.text(bx + 8, by + (bh as i32 - 16) / 2, 1, TEXT, "histórico");
}

fn draw_refresh_button(d: &mut Screen, (bx, by, bw, bh): (i32, i32, u32, u32)) {
    d.outline(bx, by, bw, bh, 1, (150, 150, 158, 255));
    d.fill(bx + 1, by + 1, bw - 2, bh - 2, (34, 34, 40, 255));
    d.text(bx + 8, by + (bh as i32 - 16) / 2, 1, TEXT, "atualizar");
}

/// A thin vertical scroll indicator at the grid's right edge (plan revision:
/// "mostrar rolagem na tv para visualizar os jogos") — only drawn once the
/// list actually overflows the visible rows.
fn draw_scrollbar(d: &mut Screen, grid: &GridLayout, view_len: usize, top_row: usize, scr_w: u32) {
    let total_rows = grid.total_rows(view_len);
    if total_rows <= grid.vis_rows {
        return;
    }
    let track_x = scr_w as i32 - MARGIN - SCROLLBAR_W as i32;
    let track_y = grid.y0;
    let track_h = (grid.vis_rows as i32 * grid.cell_h - GAP as i32).max(SCROLLBAR_W as i32);
    d.fill(
        track_x,
        track_y,
        SCROLLBAR_W,
        track_h as u32,
        (60, 60, 68, 255),
    );

    let thumb_h = ((grid.vis_rows as f32 / total_rows as f32) * track_h as f32)
        .max(16.0)
        .min(track_h as f32) as u32;
    let max_scroll = (total_rows - grid.vis_rows) as f32;
    let scroll_frac = if max_scroll > 0.0 {
        (top_row as f32 / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_y = track_y + (scroll_frac * (track_h - thumb_h as i32).max(0) as f32) as i32;
    d.fill(track_x, thumb_y, SCROLLBAR_W, thumb_h, HILITE);
}

/// Decode a cover or logo PNG/JPEG to tightly-packed RGBA, downscaled for
/// memory (covers feed 200x150 tiles, logos a ~340px panel slot — 512 covers
/// both at >2x and keeps alpha for transparent logos).
fn decode_art(path: &Path) -> Result<(u32, u32, Vec<u8>)> {
    decode_art_scaled(path, 512)
}

/// Same decode, with an explicit size cap — the back cover's zoomed view
/// re-decodes at full resolution so the enlargement stays crisp.
fn decode_art_scaled(path: &Path, max: u32) -> Result<(u32, u32, Vec<u8>)> {
    let img = image::open(path)?.thumbnail(max, max).to_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::find_local_art;

    #[test]
    fn finds_art_for_variant_roms_via_the_base_name() {
        let dir = std::env::temp_dir().join(format!("shelf-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("Killer Instinct (USA).jpg");
        std::fs::write(&base, b"jpg").unwrap();

        // Exact stem hits.
        let hit = find_local_art(&dir, "/x/Killer Instinct (USA).sfc", None).unwrap();
        assert_eq!(hit, base);
        // A variant ROM ("Rev 1") peels tags down to the base-named file.
        let hit = find_local_art(&dir, "/x/Killer Instinct (USA) (Rev 1).sfc", None).unwrap();
        assert_eq!(hit, base);

        // PNG wins over JPG for the same stem.
        std::fs::write(dir.join("Killer Instinct (USA).png"), b"png").unwrap();
        let hit = find_local_art(&dir, "/x/Killer Instinct (USA).sfc", None).unwrap();
        assert_eq!(hit.extension().unwrap(), "png");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_found_leaves_none() {
        let dir = std::env::temp_dir().join(format!("shelf-art-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            find_local_art(&dir, "/x/Missing Game (USA).sfc", None),
            None
        );
        // A stem that only shares a prefix must not match anything.
        std::fs::write(dir.join("Game (USA).jpg"), b"x").unwrap();
        assert_eq!(find_local_art(&dir, "/x/Gam.sfc", None), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod cart_tests {
    use super::*;

    #[test]
    fn cart_size_shows_the_disc_mb() {
        let mib = 1024 * 1024;
        // Discos de PSX: o tamanho real do arquivo, em MB inteiros.
        assert_eq!(cart_size(652 * mib + 4096), "653 MB"); // Tekken 3 (Track 1 ~652 MB)
        assert_eq!(cart_size(mib), "1 MB");
        assert_eq!(cart_size(0), "0 MB");
    }
}
