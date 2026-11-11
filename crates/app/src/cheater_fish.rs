//! Cheater Fish: o acessório de cheats antes do jogo, no estilo dos
//! cartuchos de trapaça dos anos 90 (GameShark e companhia). Ao ligar o
//! console com um disco que tem cheats, ele roda ANTES do jogo: uma tela de
//! carregamento com o peixe de óculos e, direto, a lista dos cheats
//! disponíveis para aquele jogo — liga/desliga cada um e escolhe "Start
//! Game". O que ficou ligado é aplicado ao núcleo pelo chamador.
//!
//! É só interface: o peixe não é um programa emulado (o disco do PS1 não
//! tem um "cartucho de trapaça" para rodar antes). Mas as telas são
//! renderizadas como as de uma ROM de 32 bits: num quadro pequeno, com cor
//! de 15 bits, dither de GPU, fonte bitmap dura e polígonos que tremem um
//! pixel ([`crate::psx_gpu`]), ampliado sem filtro dentro do tubo.

use std::time::{Duration, Instant};

use anyhow::Result;
use polystnx_domain::CheatDef;
use polystnx_platform::{Cabinet, MenuInput, MenuMode, MenuNav, PanelButton, Platform};

use crate::psx_gpu::{Font, Gpu, Rgb};

/// Como a tela terminou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FishOutcome {
    /// "Start Game": o jogo começa com os cheats que ficaram ligados.
    Start,
    /// A janela foi fechada: o app inteiro sai.
    Quit,
    /// Clicou no POWER do console: desliga, sem começar o jogo.
    PowerOff,
    /// Clicou no OPEN/CLOSE do console: ejeta o disco e volta à estante.
    Eject,
}

/// O que o jogador fez num quadro, venha do mouse, do teclado ou do
/// controle: as intenções de menu mais o que saiu do console.
struct FishInput {
    nav: Vec<MenuNav>,
    /// Clique na janela, já em coordenadas de saída.
    click: Option<(i32, i32)>,
    quit: bool,
}

/// O nome SDL de uma tecla vira a intenção de menu equivalente. Nos menus o
/// teclado não navega (só mouse e controle); aqui ele precisa, porque o
/// Cheater Fish é uma tela de escolha longa.
fn key_nav(name: &str) -> Option<MenuNav> {
    Some(match name {
        "Up" | "W" => MenuNav::Up,
        "Down" | "S" => MenuNav::Down,
        "PageUp" | "Page Up" => MenuNav::PageUp,
        "PageDown" | "Page Down" => MenuNav::PageDown,
        "Home" => MenuNav::Home,
        "End" => MenuNav::End,
        "Return" | "Enter" | "Keypad Enter" | "Space" | "Z" => MenuNav::Confirm,
        "Backspace" | "X" => MenuNav::Back,
        _ => return None,
    })
}

/// Lê o quadro de entrada: mouse (clique), controle (os botões já vêm como
/// intenções) e teclado (as teclas chegam cruas no modo `CaptureKey`; o
/// Esc vira `Back`).
fn read_input(plat: &mut Platform) -> FishInput {
    let m: MenuInput = plat.poll_menu(MenuMode::CaptureKey);
    let mut nav = m.nav;
    if let Some(n) = m.captured_key.as_deref().and_then(key_nav) {
        nav.push(n);
    }
    if m.capture_cancelled {
        nav.push(MenuNav::Back);
    }
    FishInput {
        nav,
        click: m.click,
        quit: m.quit,
    }
}

/// O clique caiu num botão do console (a lateral)? POWER e OPEN/CLOSE
/// continuam valendo durante o Cheater Fish, como numa TV de verdade.
fn console_click(cab: &mut Cabinet, ox: i32, oy: i32) -> Option<FishOutcome> {
    match cab.hit_panel_button(ox, oy) {
        Some(PanelButton::Power) => Some(FishOutcome::PowerOff),
        Some(PanelButton::Eject) => Some(FishOutcome::Eject),
        _ => None,
    }
}

/// Quanto dura a tela de carregamento (o jogador pode pular com A).
const LOADING: Duration = Duration::from_millis(2600);
/// O id da imagem viva onde o quadro de baixa resolução sobe para o tubo.
const FISH_IMG: u64 = u64::MAX - 21;
/// Quantas vezes o quadro interno é ampliado até o tubo (pixels grossos).
const UPSCALE: i32 = 2;

// A paleta azul da tela de menu dos cartuchos de trapaça.
const BG: (u8, u8, u8) = (20, 26, 92);
const SILVER_HI: Rgb = (232, 236, 252);
const SILVER: Rgb = (180, 186, 218);
const ROW_TOP: Rgb = (128, 140, 198);
const ROW_BOT: Rgb = (88, 100, 160);
const ROW_SHADOW: Rgb = (30, 34, 90);
const ROW_TEXT: Rgb = (240, 242, 255);
const SELECT_TOP: Rgb = (250, 238, 100);
const SELECT_BOT: Rgb = (206, 188, 40);
const SELECT_TEXT: Rgb = (28, 28, 64);
const INK: Rgb = (18, 26, 104);
const ON_TOP: Rgb = (96, 240, 120);
const ON_BOT: Rgb = (30, 160, 56);
const INFO_TEXT: Rgb = (232, 236, 250);
const INFO_HI: Rgb = (248, 248, 150);

// ---- o peixe de óculos -----------------------------------------------------

/// Largura e altura do desenho do peixe, em "pixels" de pixel-art.
pub const FISH_W: usize = 36;
pub const FISH_H: usize = 24;

/// O peixe de óculos escuros, em pixel-art, olhando para a direita: corpo
/// azul com a barriga clara, cauda e nadadeiras laranja, óculos pretos com
/// o reflexo e a haste. `None` = transparente. Gerado por geometria (em vez
/// de uma matriz de caracteres) e fechado por um contorno escuro de 1 px.
pub fn fish_pixels() -> Vec<Option<Rgb>> {
    let mut px: Vec<Option<Rgb>> = vec![None; FISH_W * FISH_H];
    let (cx, cy) = (18.0f32, 12.5f32);
    let (rx, ry) = (11.5f32, 7.2f32);
    let fin: Rgb = (244, 170, 52);
    let fin_dark: Rgb = (210, 120, 34);
    for y in 0..FISH_H {
        for x in 0..FISH_W {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = (fx - cx) / rx;
            let dy = (fy - cy) / ry;
            let body = dx * dx + dy * dy <= 1.0;
            // Cauda: um leque que abre para a esquerda.
            let tail = (1.5..=8.5).contains(&fx) && (fy - cy).abs() <= (8.5 - fx) * 0.85 + 1.6;
            // Nadadeira dorsal: um triângulo sobre o corpo.
            let dorsal = fy >= 3.0 + (fx - 16.0).abs() * 0.75
                && fy <= cy - ry * 0.55
                && (11.0..=22.0).contains(&fx);
            // Nadadeira de baixo, menor.
            let belly_fin = fy <= 22.0 - (fx - 16.0).abs() * 0.8
                && fy >= cy + ry * 0.7
                && (12.0..=20.0).contains(&fx);
            let c = if body {
                // Degradê vertical: azul em cima, claro embaixo.
                let t = ((fy - (cy - ry)) / (2.0 * ry)).clamp(0.0, 1.0);
                let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
                Some((mix(54, 196), mix(142, 232), mix(222, 248)))
            } else if tail || dorsal || belly_fin {
                Some(if (x + y) % 5 == 0 { fin_dark } else { fin })
            } else {
                None
            };
            px[y * FISH_W + x] = c;
        }
    }
    // Boca: um traço escuro na ponta do focinho.
    for x in 28..31 {
        px[14 * FISH_W + x] = Some((22, 34, 82));
    }
    // Óculos escuros: duas lentes redondas, ponte e haste até o meio do corpo.
    let frame: Rgb = (14, 16, 30);
    let lens: Rgb = (30, 34, 58);
    let lens_centers = [(22.0f32, 10.0f32), (28.5f32, 10.0f32)];
    for y in 0..FISH_H {
        for x in 0..FISH_W {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            for (lx, ly) in lens_centers {
                let d = ((fx - lx).powi(2) + (fy - ly).powi(2)).sqrt();
                if d <= 3.3 {
                    px[y * FISH_W + x] = Some(if d <= 2.4 { lens } else { frame });
                }
            }
        }
    }
    for x in 25..26 {
        px[10 * FISH_W + x] = Some(frame); // a ponte entre as lentes
    }
    for x in 14..19 {
        px[9 * FISH_W + x] = Some(frame); // a haste
    }
    // O reflexo nas lentes.
    for (lx, ly) in [(21usize, 8usize), (27, 8)] {
        px[ly * FISH_W + lx] = Some((214, 228, 255));
        px[ly * FISH_W + lx + 1] = Some((150, 176, 230));
        px[(ly + 1) * FISH_W + lx] = Some((150, 176, 230));
    }
    // Contorno escuro de 1 px em volta de tudo.
    let outline: Rgb = (10, 18, 54);
    let src = px.clone();
    for y in 0..FISH_H {
        for x in 0..FISH_W {
            if src[y * FISH_W + x].is_some() {
                continue;
            }
            let near = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|(dx, dy)| {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    nx >= 0
                        && ny >= 0
                        && (nx as usize) < FISH_W
                        && (ny as usize) < FISH_H
                        && src[ny as usize * FISH_W + nx as usize].is_some()
                });
            if near {
                px[y * FISH_W + x] = Some(outline);
            }
        }
    }
    px
}

// ---- desenho (no rasterizador estilo PS1) ---------------------------------

/// O tremor de polígono do PS1: a geometria é inteira, então uma moldura
/// que se move devagar "pula" de pixel em pixel. Devolve o deslocamento
/// (-1, 0 ou 1) para o quadro `frame`, com `phase` para dessincronizar.
fn jitter(frame: u32, phase: u32) -> i32 {
    [0, 0, 1, 0, 0, -1][((frame / 14 + phase) % 6) as usize]
}

/// `text` cortado para caber em `max_cols` colunas, com reticências.
fn fit(text: &str, max_cols: usize) -> String {
    if text.chars().count() <= max_cols {
        return text.to_string();
    }
    let mut s: String = text.chars().take(max_cols.saturating_sub(3)).collect();
    s.push_str("...");
    s
}

/// O tamanho do quadro interno para um tubo de `buf` pixels.
pub fn internal_size(buf: (u32, u32)) -> (i32, i32) {
    (
        (buf.0 as i32 / UPSCALE).max(160),
        (buf.1 as i32 / UPSCALE).max(120),
    )
}

/// A tela de carregamento: o peixe balançando com bolhas, o título e a
/// barra enchendo.
pub fn render_loading(g: &mut Gpu, fish: &[Option<Rgb>], title: &str, progress: f32, frame: u32) {
    g.water(frame);
    let (w, h) = (g.w, g.h);
    let scale = if h >= 300 { 3 } else { 2 };
    let (fw, fh) = (FISH_W as i32 * scale, FISH_H as i32 * scale);
    let bob = ((frame as f32 * 0.09).sin() * 3.0).round() as i32;
    let fx = (w - fw) / 2;
    let fy = (h - fh) / 2 - 46 + bob;
    // Bolhas subindo atrás do peixe.
    for i in 0..5u32 {
        let t = (frame + i * 23) % 100;
        let bx =
            fx + fw - 18 - i as i32 * 5 + ((t as f32 * 0.2 + i as f32).sin() * 3.0).round() as i32;
        let by = fy + 12 - t as i32;
        if by > 0 {
            let r = 2 + (i % 2) as i32;
            g.rect(bx, by, r, r, (190, 220, 255));
            g.rect(bx, by, 1, 1, (255, 255, 255));
        }
    }
    g.sprite(fish, FISH_W as i32, FISH_H as i32, fx, fy, scale);
    g.text_center(w / 2, fy + fh + 8, Font::Big, SILVER_HI, "CHEATER FISH");
    g.text_center(w / 2, fy + fh + 8 + 34, Font::Small, SILVER, "Version 1.0");
    // A barra de carregamento.
    let bw = (w * 3 / 5).max(120);
    let bx = (w - bw) / 2;
    let by = h - 58;
    g.bevel(bx - 4, by - 4, bw + 8, 22, Some(((4, 6, 22), (10, 12, 40))));
    let segs = 20;
    let lit = (progress.clamp(0.0, 1.0) * segs as f32) as i32;
    for s in 0..segs {
        let sx = bx + 2 + s * (bw - 4) / segs;
        let sw = ((bw - 4) / segs - 2).max(2);
        if s < lit {
            g.grad(sx, by, sw, 14, SELECT_TOP, SELECT_BOT);
        } else {
            g.rect(sx, by, sw, 14, (24, 28, 76));
        }
    }
    let cols = (w / Font::Small.cell()) as usize - 2;
    let msg = fit(&format!("Loading cheat codes for {title}..."), cols);
    g.text_center(w / 2, h - 30, Font::Small, INFO_TEXT, &msg);
}

/// Onde cada linha da lista caiu neste quadro (coordenadas do quadro
/// interno), para o clique.
#[derive(Default)]
pub struct Layout {
    /// `(índice da linha, x, y, w, h)` de cada linha visível.
    pub rows: Vec<(usize, i32, i32, i32, i32)>,
    /// Quantas linhas cabem na lista.
    pub visible: usize,
}

/// A lista de cheats do jogo, direto: título, painel com as linhas, caixa
/// de informação e o texto de ajuda rolando.
#[allow(clippy::too_many_arguments)]
pub fn render_list(
    g: &mut Gpu,
    fish: &[Option<Rgb>],
    title: &str,
    defs: &[CheatDef],
    state: &[bool],
    sel: usize,
    top: usize,
    frame: u32,
) -> Layout {
    g.water(frame);
    let (w, h) = (g.w, g.h);
    let cell = Font::List.cell();
    let mut layout = Layout::default();

    // A barra de título, com o peixinho à esquerda.
    let jb = jitter(frame, 0);
    g.bevel(
        10,
        4 + jb,
        w - 20,
        28,
        Some(((170, 178, 226), (140, 148, 204))),
    );
    g.sprite(fish, FISH_W as i32, FISH_H as i32, 18, 6 + jb, 1);
    g.text_center(w / 2, 10 + jb, Font::Small, INK, "Cheater Fish Version 1.0");

    // O painel central da lista.
    let info_h = 70;
    let hint_h = 20;
    let (px, py) = (22, 40);
    let (pw, ph) = (w - 44, h - py - info_h - hint_h - 12);
    let jp = jitter(frame, 2);
    g.bevel(px, py + jp, pw, ph, Some(((28, 34, 112), (16, 20, 74))));
    g.text_center(
        w / 2,
        py + 8 + jp,
        Font::Small,
        SILVER_HI,
        "Select Cheat Codes",
    );

    let (row_h, gap) = (22, 3);
    let list_top = py + 8 + 16 + 8;
    let list_h = (py + ph - 10) - list_top;
    layout.visible = (list_h / (row_h + gap)).max(1) as usize;
    let (ix, iw) = (px + 12, pw - 24 - 10);
    let max_cols = ((iw - 34) / cell).max(4) as usize;
    let total_rows = defs.len() + 1; // "Start Game" + um por cheat
    for slot in 0..layout.visible {
        let idx = top + slot;
        if idx >= total_rows {
            break;
        }
        let y = list_top + slot as i32 * (row_h + gap);
        let selected = idx == sel;
        g.rect(ix + 2, y + 2, iw, row_h, ROW_SHADOW);
        if selected {
            g.grad(ix, y, iw, row_h, SELECT_TOP, SELECT_BOT);
        } else {
            g.grad(ix, y, iw, row_h, ROW_TOP, ROW_BOT);
        }
        g.rect(ix, y, iw, 1, (255, 255, 255));
        let fg = if selected { SELECT_TEXT } else { ROW_TEXT };
        if idx == 0 {
            g.text(ix + 8, y + 1, Font::List, fg, "Start Game");
        } else {
            let on = state.get(idx - 1).copied().unwrap_or(false);
            // A caixinha: verde com degradê quando ligado.
            g.rect(ix + 7, y + 5, 12, 12, (8, 10, 36));
            if on {
                g.grad(ix + 8, y + 6, 10, 10, ON_TOP, ON_BOT);
            } else {
                g.rect(ix + 8, y + 6, 10, 10, (52, 58, 112));
            }
            let name = fit(defs[idx - 1].desc, max_cols);
            g.text(ix + 26, y + 1, Font::List, fg, &name);
        }
        layout.rows.push((idx, ix, y, iw, row_h));
    }
    // A barra de rolagem.
    let sbx = px + pw - 18;
    g.rect(sbx, list_top, 6, list_h, (8, 10, 34));
    if total_rows > layout.visible {
        let th = ((list_h as usize * layout.visible) / total_rows).max(12) as i32;
        let span = list_h - th;
        let ty = list_top + (span as usize * top / (total_rows - layout.visible).max(1)) as i32;
        g.grad(sbx, ty, 6, th, SILVER_HI, SILVER);
    }

    // A caixa de informação embaixo.
    let iy = py + ph + 8;
    let ji = jitter(frame, 4);
    g.bevel(px, iy + ji, pw, info_h, Some(((4, 6, 20), (6, 8, 28))));
    let on_count = state.iter().filter(|&&b| b).count();
    g.text_center(
        w / 2,
        iy + 8 + ji,
        Font::Small,
        INFO_TEXT,
        &format!("{} Cheat Codes Available", defs.len()),
    );
    let name = fit(title, (pw / cell) as usize - 4);
    g.text_center(w / 2, iy + 25 + ji, Font::Small, INFO_HI, &name);
    g.text_center(
        w / 2,
        iy + 42 + ji,
        Font::Small,
        INFO_TEXT,
        &format!("{on_count} Cheat Codes Are On"),
    );

    // O texto de ajuda rolando embaixo, como nos cartuchos.
    let hint = "up/down and 'A' to select. Press 'B' or choose Start Game to play.      ";
    let hint_w = hint.chars().count() as i32 * cell;
    let hy = h - hint_h + 2;
    let mut x = -((frame as i32) % hint_w);
    while x < w {
        g.text(x, hy, Font::Small, SILVER_HI, hint);
        x += hint_w;
    }
    layout
}

/// Sobe o quadro do rasterizador como imagem viva e o ampliou sem filtro
/// por todo o tubo.
pub fn present(cab: &mut Cabinet, g: &Gpu) {
    cab.update_live_image(FISH_IMG, g.w as u32, g.h as u32, &g.px);
    let (bw, bh) = cab.shelf_buffer_size();
    cab.frame_idle_2d(BG, |d| d.image_fit(FISH_IMG, 0, 0, bw, bh));
}

fn pace(next: &mut Instant, plat: &Platform, cab: &Cabinet) {
    crate::runner::pace_ui(next, crate::runner::UI_FRAME, plat, cab);
}

/// Roda a tela do Cheater Fish: o carregamento e a lista. `state` (um bool
/// por cheat, na ordem de `defs`) é alterado no lugar; devolve como
/// terminou. A janela fechada devolve [`FishOutcome::Quit`].
pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    title: &str,
    defs: &[CheatDef],
    state: &mut [bool],
) -> Result<FishOutcome> {
    let fish = fish_pixels();
    let mut next = Instant::now();
    plat.prime_menu_edges();
    let started = Instant::now();
    let mut frame: u32 = 0;
    let (iw, ih) = internal_size(cab.shelf_buffer_size());
    let mut gpu = Gpu::new(iw, ih);

    // 1) Carregamento: o peixe, o nome do jogo e a barra enchendo.
    while started.elapsed() < LOADING {
        let m = read_input(plat);
        if m.quit {
            return Ok(FishOutcome::Quit);
        }
        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            cab.try_tv_click(ox, oy);
            if cab.hit_close_button(ox, oy) {
                return Ok(FishOutcome::Quit);
            }
            if let Some(out) = console_click(cab, ox, oy) {
                return Ok(out);
            }
        }
        // A (ou Enter/Espaço) pula o carregamento.
        if m.nav.iter().any(|n| matches!(n, MenuNav::Confirm)) {
            break;
        }
        let progress = started.elapsed().as_secs_f32() / LOADING.as_secs_f32();
        render_loading(&mut gpu, &fish, title, progress, frame);
        present(cab, &gpu);
        frame += 1;
        pace(&mut next, plat, cab);
    }

    // 2) A lista de cheats: direto, sem menu principal.
    let total = defs.len() + 1;
    let mut sel = 0usize;
    let mut top = 0usize;
    let mut layout = Layout::default();
    loop {
        let m = read_input(plat);
        if m.quit {
            return Ok(FishOutcome::Quit);
        }
        let mut activate: Option<usize> = None;
        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            cab.try_tv_click(ox, oy);
            if cab.hit_close_button(ox, oy) {
                return Ok(FishOutcome::Quit);
            }
            if let Some(out) = console_click(cab, ox, oy) {
                return Ok(out);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
            } else if let Some((sx, sy)) = cab.hit_screen_point(ox, oy) {
                log::debug!("cheater fish: clique janela ({x},{y}) → tubo ({sx},{sy})");
                // Do tubo (pixels de tela) para o quadro interno.
                let (bw, _) = cab.shelf_buffer_size();
                let k = bw as f32 / gpu.w as f32;
                let (cx, cy) = ((sx as f32 / k) as i32, (sy as f32 / k) as i32);
                for &(idx, rx, ry, rw, rh) in &layout.rows {
                    if cx >= rx && cx < rx + rw && cy >= ry && cy < ry + rh {
                        sel = idx;
                        activate = Some(idx);
                    }
                }
            }
        }
        let page = layout.visible.max(1);
        for nav in &m.nav {
            match nav {
                MenuNav::Up | MenuNav::Left => sel = sel.saturating_sub(1),
                MenuNav::Down | MenuNav::Right => sel = (sel + 1).min(total - 1),
                MenuNav::PageUp => sel = sel.saturating_sub(page),
                MenuNav::PageDown => sel = (sel + page).min(total - 1),
                MenuNav::Home => sel = 0,
                MenuNav::End => sel = total - 1,
                MenuNav::Confirm => activate = Some(sel),
                // B: começa o jogo com o que estiver ligado.
                MenuNav::Back => return Ok(FishOutcome::Start),
            }
        }
        if let Some(idx) = activate {
            if idx == 0 {
                return Ok(FishOutcome::Start);
            }
            if let Some(s) = state.get_mut(idx - 1) {
                *s = !*s;
                log::info!("cheater fish: {:?} → {}", defs[idx - 1].desc, *s);
            }
        }
        // A seleção sempre visível.
        if layout.visible > 0 {
            if sel < top {
                top = sel;
            } else if sel >= top + layout.visible {
                top = sel + 1 - layout.visible;
            }
        }
        layout = render_list(&mut gpu, &fish, title, defs, state, sel, top, frame);
        present(cab, &gpu);
        frame += 1;
        pace(&mut next, plat, cab);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fish_has_glasses_and_a_closed_outline() {
        let px = fish_pixels();
        assert_eq!(px.len(), FISH_W * FISH_H);
        // Tem corpo desenhado e nenhum pixel colorido encosta na borda do quadro.
        assert!(px.iter().filter(|p| p.is_some()).count() > 250);
        for x in 0..FISH_W {
            assert!(px[x].is_none(), "linha de cima vazia");
            assert!(
                px[(FISH_H - 1) * FISH_W + x].is_none(),
                "linha de baixo vazia"
            );
        }
        // As lentes escuras (o par de óculos) estão onde o desenho as põe.
        assert_eq!(px[10 * FISH_W + 22], Some((30, 34, 58)));
        assert_eq!(px[10 * FISH_W + 28], Some((30, 34, 58)));
    }

    #[test]
    fn long_cheat_names_are_cut_with_an_ellipsis() {
        assert_eq!(fit("Infinite Lives", 20), "Infinite Lives");
        assert_eq!(fit("Infinite Lives And Health", 12), "Infinite ...");
    }

    #[test]
    fn the_list_lays_out_rows_inside_the_frame() {
        let defs = polystnx_domain::cheats_for_title("Tekken 3 (USA)");
        let state = vec![false; defs.len()];
        let mut g = Gpu::new(413, 330);
        let fish = fish_pixels();
        let l = render_list(&mut g, &fish, "Tekken 3 (USA)", &defs, &state, 0, 0, 0);
        assert!(l.visible >= 5, "cabem linhas: {}", l.visible);
        assert_eq!(l.rows.len(), l.visible.min(defs.len() + 1));
        for &(_, x, y, w, h) in &l.rows {
            assert!(x >= 0 && y >= 0 && x + w <= g.w && y + h <= g.h);
        }
    }

    #[test]
    fn keyboard_keys_map_to_menu_intents() {
        assert_eq!(key_nav("Up"), Some(MenuNav::Up));
        assert_eq!(key_nav("Down"), Some(MenuNav::Down));
        assert_eq!(key_nav("Return"), Some(MenuNav::Confirm));
        assert_eq!(key_nav("Space"), Some(MenuNav::Confirm));
        assert_eq!(key_nav("Backspace"), Some(MenuNav::Back));
        assert_eq!(key_nav("PageDown"), Some(MenuNav::PageDown));
        assert_eq!(key_nav("F13"), None);
    }

    #[test]
    fn polygon_jitter_stays_within_one_pixel() {
        for f in 0..200 {
            for p in 0..6 {
                assert!(jitter(f, p).abs() <= 1);
            }
        }
    }
}
