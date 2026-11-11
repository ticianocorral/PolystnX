//! Os móveis EXTRAS da estante, portados do SixteeN: madeira, expositor de
//! metal, rack de som, quarto anos 90 e fliperama. O tema padrão (`Ps1`)
//! continua sendo o desenho original de `shelf.rs`; aqui só moram os
//! estilos alternativos — fundo, montantes, travessa e prateleiras.

use crate::config::ShelfTheme;
use crate::shelf::MARGIN;
use polystnx_platform::Screen;

/// Quem sabe pintar retângulos (a tela da estante).
pub trait Paint {
    fn fill(&mut self, x: i32, y: i32, w: u32, h: u32, c: (u8, u8, u8, u8));
}

impl Paint for Screen<'_> {
    fn fill(&mut self, x: i32, y: i32, w: u32, h: u32, c: (u8, u8, u8, u8)) {
        Screen::fill(self, x, y, w, h, c);
    }
}

/// O fundo, os montantes e a travessa do tema (nada para o padrão).
pub fn draw_back(d: &mut impl Paint, theme: ShelfTheme, scr_w: u32, scr_h: u32, top: i32) {
    match theme {
        ShelfTheme::Ps1 => {}
        ShelfTheme::Wood => draw_cabinet(d, scr_w, scr_h, top),
        ShelfTheme::Metal => draw_rack(d, scr_w, scr_h, top),
        ShelfTheme::HiFi | ShelfTheme::Memphis | ShelfTheme::Arcade => {
            draw_case(d, scr_w, scr_h, top, &CaseLook::of(theme))
        }
    }
}

/// A prateleira do tema sob uma fileira (nada para o padrão). `tags` são os
/// centros das colunas, para as etiquetas do expositor de metal.
pub fn draw_shelf(
    d: &mut impl Paint,
    theme: ShelfTheme,
    x: i32,
    w: u32,
    y: i32,
    h: i32,
    tags: &[i32],
) {
    match theme {
        ShelfTheme::Ps1 => {}
        ShelfTheme::Wood => draw_shelf_board(d, x, w, y, h),
        ShelfTheme::Metal => draw_rack_shelf(d, x, w, y, h, tags),
        ShelfTheme::HiFi | ShelfTheme::Memphis | ShelfTheme::Arcade => {
            draw_case_shelf(d, x, w, y, h, &CaseLook::of(theme))
        }
    }
}
/// O MÓVEL em volta das capas: o painel de fundo (madeira escura com veios
/// verticais, escurecendo para os cantos como o fundo de um armário),
/// os dois montantes laterais e a travessa do topo — a grade vira o
/// interior de uma estante, em vez de capas soltas no escuro. `top` é onde
/// a travessa assenta (logo abaixo do cabeçalho).
fn draw_cabinet(d: &mut impl Paint, scr_w: u32, scr_h: u32, top: i32) {
    const POST: i32 = 12;
    let x0 = MARGIN - POST - 4;
    let x1 = scr_w as i32 - MARGIN + 4;
    let bottom = scr_h as i32;
    let inner_w = (x1 - x0 - POST * 2).max(1) as u32;
    let inner_h = (bottom - top).max(1) as u32;
    let ix = x0 + POST;

    // Painel de fundo: tom base + veios verticais em faixas de tons vizinhos.
    d.fill(ix, top, inner_w, inner_h, (20, 16, 14, 255));
    let mut gx = ix;
    let mut k = 0u32;
    while gx < x1 - POST {
        let tone = [(21, 17, 15), (18, 15, 13), (23, 19, 16), (20, 16, 14)][(k % 4) as usize];
        let wv = 9 + (k * 7 % 13) as i32;
        d.fill(
            gx,
            top,
            wv.min(x1 - POST - gx) as u32,
            inner_h,
            (tone.0, tone.1, tone.2, 255),
        );
        gx += wv;
        k += 1;
    }
    // Vinheta: o fundo escurece para as laterais e para baixo.
    for i in 0..10 {
        let a = (60 - i * 6) as u8;
        d.fill(ix + i * 3, top, 3, inner_h, (0, 0, 0, a));
        d.fill(x1 - POST - (i + 1) * 3, top, 3, inner_h, (0, 0, 0, a));
    }
    // Sombra da travessa no fundo, logo abaixo dela.
    for i in 0..8 {
        d.fill(
            ix,
            top + 10 + i * 2,
            inner_w,
            2,
            (0, 0, 0, (90 - i * 11) as u8),
        );
    }

    // Montantes laterais: face, quina iluminada e fio de sombra interno.
    for (px, light_left) in [(x0, true), (x1 - POST, false)] {
        d.fill(px, top - 10, POST as u32, inner_h + 10, (52, 38, 29, 255));
        let edge = if light_left { px } else { px + POST - 2 };
        d.fill(edge, top - 10, 2, inner_h + 10, (84, 64, 50, 255));
        let inner = if light_left { px + POST - 1 } else { px };
        d.fill(inner, top, 1, inner_h, (26, 18, 12, 255));
    }
    // Travessa do topo.
    d.fill(x0, top - 10, (x1 - x0) as u32, 10, (56, 41, 31, 255));
    d.fill(x0, top - 10, (x1 - x0) as u32, 2, (90, 70, 55, 255));
    d.fill(x0, top - 1, (x1 - x0) as u32, 1, (32, 22, 15, 255));
}

/// O EXPOSITOR DE METAL da locadora em volta das capas — a alternativa ao
/// móvel de madeira, na mesma geometria de `draw_cabinet`: o fundo de chapa
/// perfurada (a parede de ganchos das locadoras) escurecendo para os
/// cantos, os dois montantes de aço com a cremalheira de rasgos onde as
/// prateleiras encaixam e a travessa do topo com o filete vermelho da
/// sinalização da loja.
fn draw_rack(d: &mut impl Paint, scr_w: u32, scr_h: u32, top: i32) {
    const POST: i32 = 12;
    const HOLE_STEP: i32 = 14;
    let x0 = MARGIN - POST - 4;
    let x1 = scr_w as i32 - MARGIN + 4;
    let bottom = scr_h as i32;
    let inner_w = (x1 - x0 - POST * 2).max(1) as u32;
    let inner_h = (bottom - top).max(1) as u32;
    let ix = x0 + POST;

    // Chapa perfurada: aço pintado e a grade de furos, cada furo com a
    // borda de baixo pegando luz (a chapa tem espessura).
    d.fill(ix, top, inner_w, inner_h, (42, 45, 49, 255));
    let mut hy = top + 8;
    while hy < bottom - 2 {
        let mut hx = ix + 8;
        while hx < x1 - POST - 4 {
            d.fill(hx, hy, 3, 3, (20, 21, 24, 255));
            d.fill(hx, hy + 3, 3, 1, (60, 64, 69, 255));
            hx += HOLE_STEP;
        }
        hy += HOLE_STEP;
    }
    // Vinheta: o fundo escurece para as laterais.
    for i in 0..10 {
        let a = (60 - i * 6) as u8;
        d.fill(ix + i * 3, top, 3, inner_h, (0, 0, 0, a));
        d.fill(x1 - POST - (i + 1) * 3, top, 3, inner_h, (0, 0, 0, a));
    }
    // Sombra da travessa na chapa, logo abaixo dela.
    for i in 0..8 {
        d.fill(
            ix,
            top + 10 + i * 2,
            inner_w,
            2,
            (0, 0, 0, (90 - i * 11) as u8),
        );
    }

    // Montantes: aço escovado, quina iluminada, fio de sombra interno e a
    // cremalheira — rasgos verticais no meio da face.
    for (px, light_left) in [(x0, true), (x1 - POST, false)] {
        d.fill(px, top - 10, POST as u32, inner_h + 10, (86, 90, 96, 255));
        let edge = if light_left { px } else { px + POST - 2 };
        d.fill(edge, top - 10, 2, inner_h + 10, (152, 158, 166, 255));
        let inner = if light_left { px + POST - 1 } else { px };
        d.fill(inner, top, 1, inner_h, (30, 32, 36, 255));
        let mut sy = top + 4;
        while sy < bottom - 8 {
            d.fill(px + POST / 2 - 1, sy, 3, 7, (24, 25, 28, 255));
            d.fill(px + POST / 2 - 1, sy + 7, 3, 1, (124, 130, 138, 255));
            sy += 14;
        }
    }
    // Travessa do topo, com o filete vermelho da sinalização da locadora.
    d.fill(x0, top - 10, (x1 - x0) as u32, 10, (80, 84, 90, 255));
    d.fill(x0, top - 10, (x1 - x0) as u32, 2, (150, 156, 164, 255));
    d.fill(x0, top - 6, (x1 - x0) as u32, 2, (176, 34, 38, 255));
    d.fill(x0, top - 1, (x1 - x0) as u32, 1, (30, 32, 36, 255));
}

/// A prateleira de aço do expositor sob uma fileira de caixas — irmã de
/// `draw_shelf_board`, no mesmo vão de `GAP`: o tampo de chapa visto de
/// cima, a canaleta de preço na frente (com uma etiqueta de papel sob cada
/// coluna, em `tags`) e as mãos-francesas de chapa dobrada nas pontas.
fn draw_rack_shelf(d: &mut impl Paint, x: i32, w: u32, y: i32, h: i32, tags: &[i32]) {
    const DEPTH: i32 = 9;
    // O tampo entrando para o fundo, com a quina de trás escura.
    d.fill(x, y - DEPTH, w, DEPTH as u32, (100, 104, 110, 255));
    d.fill(x, y - DEPTH, w, 1, (32, 34, 38, 255));
    d.fill(x, y - DEPTH + 1, w, 2, (84, 88, 94, 255));
    // A dobra da frente pegando a luz.
    d.fill(x, y - 1, w, 1, (196, 202, 210, 255));

    // A canaleta: lábio de cima, a face funda com o friso, lábio de baixo.
    let face_h = (h - 6).max(1);
    d.fill(x, y, w, 2, (146, 152, 160, 255));
    d.fill(x, y + 2, w, face_h as u32, (66, 70, 76, 255));
    d.fill(x, y + 2, w, 1, (40, 42, 46, 255));
    d.fill(x, y + 2 + face_h - 1, w, 1, (116, 122, 130, 255));
    d.fill(x, y + h - 4, w, 4, (30, 32, 36, 255));

    // As etiquetas de preço: papel amarelado, uma linha de "título" e o
    // preço em vermelho — cada uma sob o centro de uma coluna de capas.
    let (tw, th) = (28, (face_h - 3).max(3));
    for &cx in tags {
        let tx = cx - tw / 2;
        d.fill(tx, y + 3, tw as u32, th as u32, (232, 224, 196, 255));
        d.fill(tx + 3, y + 4, 12, 1, (70, 66, 60, 255));
        d.fill(tx + tw - 10, y + 3 + th - 3, 7, 2, (186, 38, 40, 255));
    }

    // Sombra do tampo na chapa, embaixo.
    for (i, c) in [(16u8, 17u8, 19u8), (22u8, 23u8, 26u8), (30u8, 32u8, 35u8)]
        .into_iter()
        .enumerate()
    {
        d.fill(
            x + 4,
            y + h + i as i32,
            w.saturating_sub(8),
            1,
            (c.0, c.1, c.2, 255),
        );
    }

    // Mão-francesa de chapa sob cada ponta: um triângulo reto, a face
    // encostada no montante — curta, como a de madeira.
    let r: i32 = 8;
    for i in 0..r {
        let dx = (r - i).max(1);
        d.fill(x + 2, y + h + i, dx as u32, 1, (78, 82, 88, 255));
        d.fill(
            x + w as i32 - 2 - dx,
            y + h + i,
            dx as u32,
            1,
            (78, 82, 88, 255),
        );
    }
}

/// Os temas novos (rack de som, quarto, fliperama) dividem a mesma
/// carpintaria — fundo, dois montantes, travessa e prateleiras — e mudam
/// só o material: as cores de cada peça e o desenho do fundo.
struct CaseLook {
    wall: (u8, u8, u8),
    pattern: Wallpaper,
    post: (u8, u8, u8),
    post_hi: (u8, u8, u8),
    post_lo: (u8, u8, u8),
    /// Um filete ao longo da travessa e da face interna dos montantes: o
    /// friso prateado do rack, a fita colorida do quarto, o neon do
    /// fliperama (com brilho na parede).
    trim: (u8, u8, u8),
    trim_glow: bool,
    shelf: Shelf,
}

#[derive(Clone, Copy)]
enum Wallpaper {
    /// Fórmica preta com veio fino de "black ash" — o rack de som.
    Ash,
    /// Papel de parede Memphis: confete de rabiscos, triângulos e bolinhas.
    Memphis,
    /// O carpete preto de fliperama, com o desenho fluorescente de planetas,
    /// estrelas e raios brilhando na luz negra.
    Cosmic,
}

#[derive(Clone, Copy)]
enum Shelf {
    /// Vidro fumê com a quina verde do vidro de verdade.
    Glass,
    /// Laca branca com a borda de fita colorida.
    Lacquer((u8, u8, u8)),
    /// Tábua preta com um tubo de neon na frente.
    Neon((u8, u8, u8)),
}

impl CaseLook {
    fn of(theme: ShelfTheme) -> Self {
        match theme {
            ShelfTheme::Memphis => CaseLook {
                wall: (38, 24, 56),
                pattern: Wallpaper::Memphis,
                post: (226, 222, 214),
                post_hi: (250, 248, 242),
                post_lo: (150, 144, 136),
                trim: (236, 72, 140),
                trim_glow: false,
                shelf: Shelf::Lacquer((40, 190, 196)),
            },
            ShelfTheme::Arcade => CaseLook {
                wall: (10, 8, 18),
                pattern: Wallpaper::Cosmic,
                post: (22, 20, 28),
                post_hi: (54, 50, 64),
                post_lo: (6, 5, 10),
                trim: (255, 60, 190),
                trim_glow: true,
                shelf: Shelf::Neon((60, 230, 255)),
            },
            _ => CaseLook {
                wall: (17, 17, 19),
                pattern: Wallpaper::Ash,
                post: (30, 30, 33),
                post_hi: (66, 66, 72),
                post_lo: (10, 10, 12),
                trim: (176, 180, 188),
                trim_glow: false,
                shelf: Shelf::Glass,
            },
        }
    }
}

/// Um gerador pseudoaleatório pequeno e fixo (o mesmo desenho a cada
/// frame) para espalhar os enfeites do papel de parede.
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// O móvel dos temas novos em volta das capas, na geometria de
/// `draw_cabinet`: fundo com o desenho do tema, vinheta, sombra da
/// travessa, montantes e travessa no material do tema.
fn draw_case(d: &mut impl Paint, scr_w: u32, scr_h: u32, top: i32, look: &CaseLook) {
    const POST: i32 = 12;
    let x0 = MARGIN - POST - 4;
    let x1 = scr_w as i32 - MARGIN + 4;
    let bottom = scr_h as i32;
    let inner_w = (x1 - x0 - POST * 2).max(1) as u32;
    let inner_h = (bottom - top).max(1) as u32;
    let ix = x0 + POST;
    let ix1 = x1 - POST;
    let w = look.wall;
    d.fill(ix, top, inner_w, inner_h, (w.0, w.1, w.2, 255));

    match look.pattern {
        Wallpaper::Ash => {
            // Veio fino e reto, de tons quase iguais — o laminado do rack.
            let mut gx = ix;
            let mut k = 0u32;
            while gx < ix1 {
                let v = (hash32(k) % 5) as u8;
                let wv = 2 + (hash32(k + 77) % 5) as i32;
                d.fill(
                    gx,
                    top,
                    wv.min(ix1 - gx) as u32,
                    inner_h,
                    (w.0 + v, w.1 + v, w.2 + v, 255),
                );
                gx += wv;
                k += 1;
            }
        }
        Wallpaper::Memphis => {
            // Confete Memphis: cada célula da grade ganha um enfeite, numa
            // das cores da época, em tom abafado para as capas mandarem.
            const PALETTE: [(u8, u8, u8); 4] = [
                (150, 60, 110),
                (48, 132, 140),
                (170, 140, 50),
                (92, 84, 170),
            ];
            let cell = 46;
            let mut i = 0u32;
            let mut cy = top + 8;
            while cy < bottom {
                let mut cx = ix + 6;
                while cx < ix1 - 10 {
                    let h = hash32(i * 2654 + 17);
                    let (c, a) = (PALETTE[(h % 4) as usize], 150u8);
                    let px = cx + (h >> 8) as i32 % 24;
                    let py = cy + (h >> 16) as i32 % 24;
                    match (h >> 4) % 4 {
                        // Rabisco em zigue-zague.
                        0 => {
                            for k in 0..5 {
                                let up = if k % 2 == 0 { 0 } else { 4 };
                                d.fill(px + k * 4, py + up, 5, 2, (c.0, c.1, c.2, a));
                            }
                        }
                        // Triângulo cheio.
                        1 => {
                            for k in 0..8 {
                                d.fill(
                                    px + 8 - k,
                                    py + k,
                                    (k * 2 + 1) as u32,
                                    1,
                                    (c.0, c.1, c.2, a),
                                );
                            }
                        }
                        // Bolinha.
                        2 => {
                            for (dy, ww) in [(0, 2), (1, 4), (2, 6), (3, 6), (4, 4), (5, 2)] {
                                d.fill(
                                    px + (6 - ww) / 2,
                                    py + dy,
                                    ww as u32,
                                    1,
                                    (c.0, c.1, c.2, a),
                                );
                            }
                        }
                        // Tracinhos paralelos.
                        _ => {
                            for k in 0..3 {
                                d.fill(px + k * 5, py + k * 2, 2, 9, (c.0, c.1, c.2, a));
                            }
                        }
                    }
                    cx += cell;
                    i += 1;
                }
                cy += cell;
            }
        }
        Wallpaper::Cosmic => {
            // Estrelinhas por toda parte, e alguns planetas com anel e
            // raios — o desenho fluorescente do carpete na luz negra.
            const GLOW: [(u8, u8, u8); 4] = [
                (255, 70, 200),
                (70, 230, 255),
                (250, 240, 80),
                (140, 90, 255),
            ];
            let area = inner_w as i32 * inner_h as i32;
            for k in 0..(area / 900).max(1) as u32 {
                let h = hash32(k * 7 + 3);
                let x = ix + (h % inner_w) as i32;
                let y = top + ((h >> 12) % inner_h) as i32;
                let c = GLOW[(h >> 28) as usize % 4];
                d.fill(x, y, 2, 2, (c.0, c.1, c.2, 120));
            }
            for k in 0..(area / 26000).max(1) as u32 {
                let h = hash32(k * 131 + 9);
                let x = ix + 10 + (h % inner_w.saturating_sub(40).max(1)) as i32;
                let y = top + 10 + ((h >> 10) % inner_h.saturating_sub(30).max(1)) as i32;
                let c = GLOW[(h >> 26) as usize % 4];
                if h & 1 == 0 {
                    // Planeta com anel.
                    for (dy, ww) in [(0, 4), (1, 8), (2, 10), (3, 10), (4, 8), (5, 4)] {
                        d.fill(
                            x + (10 - ww) / 2,
                            y + dy,
                            ww as u32,
                            1,
                            (c.0, c.1, c.2, 110),
                        );
                    }
                    d.fill(x - 5, y + 3, 20, 1, (c.0, c.1, c.2, 150));
                } else {
                    // Raio em zigue-zague.
                    for k in 0..4 {
                        d.fill(
                            x + if k % 2 == 0 { 0 } else { 4 },
                            y + k * 4,
                            6,
                            4,
                            (c.0, c.1, c.2, 110),
                        );
                    }
                }
            }
        }
    }

    // Vinheta: o fundo escurece para as laterais.
    for i in 0..10 {
        let a = (60 - i * 6) as u8;
        d.fill(ix + i * 3, top, 3, inner_h, (0, 0, 0, a));
        d.fill(ix1 - (i + 1) * 3, top, 3, inner_h, (0, 0, 0, a));
    }
    // Sombra da travessa no fundo, logo abaixo dela.
    for i in 0..8 {
        d.fill(
            ix,
            top + 10 + i * 2,
            inner_w,
            2,
            (0, 0, 0, (90 - i * 11) as u8),
        );
    }

    let t = look.trim;
    for (px, light_left) in [(x0, true), (ix1, false)] {
        d.fill(
            px,
            top - 10,
            POST as u32,
            inner_h + 10,
            (look.post.0, look.post.1, look.post.2, 255),
        );
        let edge = if light_left { px } else { px + POST - 2 };
        let hi = look.post_hi;
        d.fill(edge, top - 10, 2, inner_h + 10, (hi.0, hi.1, hi.2, 255));
        let inner = if light_left { px + POST - 2 } else { px };
        d.fill(inner, top, 2, inner_h, (t.0, t.1, t.2, 255));
        if look.trim_glow {
            // O neon lava a parede ao lado do montante.
            for k in 0..6 {
                let gx = if light_left {
                    px + POST + k * 2
                } else {
                    px - 2 - k * 2
                };
                d.fill(gx, top, 2, inner_h, (t.0, t.1, t.2, (34 - k * 5) as u8));
            }
        }
    }
    let (p, hi, lo) = (look.post, look.post_hi, look.post_lo);
    let bw = (x1 - x0) as u32;
    d.fill(x0, top - 10, bw, 10, (p.0, p.1, p.2, 255));
    d.fill(x0, top - 10, bw, 2, (hi.0, hi.1, hi.2, 255));
    d.fill(x0, top - 5, bw, 2, (t.0, t.1, t.2, 255));
    d.fill(x0, top - 1, bw, 1, (lo.0, lo.1, lo.2, 255));
}

/// A prateleira dos temas novos sob uma fileira de capas — irmã de
/// `draw_shelf_board`, no mesmo vão: o tampo visto de cima, a frente e a
/// sombra na parede, cada material com o seu detalhe.
fn draw_case_shelf(d: &mut impl Paint, x: i32, w: u32, y: i32, h: i32, look: &CaseLook) {
    const DEPTH: i32 = 9;
    let face_h = (h - 6).max(1);
    match look.shelf {
        Shelf::Glass => {
            // Vidro fumê: o tampo deixa ver a parede, a quina da frente é
            // a borda verde do vidro, e o trilho preto do rack segura.
            d.fill(x, y - DEPTH, w, DEPTH as u32, (120, 140, 150, 46));
            d.fill(x, y - DEPTH, w, 1, (200, 220, 225, 70));
            d.fill(x, y - 1, w, 1, (220, 240, 236, 170));
            d.fill(x, y, w, 3, (96, 150, 128, 210));
            d.fill(x, y, w, 1, (200, 236, 220, 230));
            d.fill(x, y + 3, w, (face_h - 1).max(1) as u32, (24, 24, 27, 255));
            d.fill(x, y + 3, w, 1, (150, 154, 162, 255));
            d.fill(x, y + h - 4, w, 4, (12, 12, 14, 255));
        }
        Shelf::Lacquer(edge) => {
            d.fill(x, y - DEPTH, w, DEPTH as u32, (214, 210, 202, 255));
            d.fill(x, y - DEPTH, w, 1, (120, 114, 108, 255));
            d.fill(x, y - 1, w, 1, (252, 250, 246, 255));
            d.fill(x, y, w, face_h as u32, (edge.0, edge.1, edge.2, 255));
            d.fill(
                x,
                y,
                w,
                2,
                (
                    edge.0.saturating_add(50),
                    edge.1.saturating_add(40),
                    edge.2.saturating_add(40),
                    255,
                ),
            );
            d.fill(
                x,
                y + face_h,
                w,
                (h - face_h).max(1) as u32,
                (edge.0 / 2, edge.1 / 2, edge.2 / 2, 255),
            );
        }
        Shelf::Neon(c) => {
            d.fill(x, y - DEPTH, w, DEPTH as u32, (30, 28, 36, 255));
            d.fill(x, y - DEPTH, w, 1, (8, 6, 12, 255));
            d.fill(x, y, w, face_h as u32, (18, 16, 24, 255));
            // O tubo: núcleo quase branco, a cor em volta, e o brilho
            // escorrendo na parede embaixo da prateleira.
            let ty = y + face_h / 2 - 1;
            d.fill(x + 6, ty - 1, w.saturating_sub(12), 4, (c.0, c.1, c.2, 255));
            d.fill(x + 6, ty, w.saturating_sub(12), 2, (230, 250, 255, 255));
            for k in 0..3 {
                d.fill(
                    x + 6,
                    ty - 3 - k,
                    w.saturating_sub(12),
                    1,
                    (c.0, c.1, c.2, (70 - k * 20) as u8),
                );
            }
            d.fill(x, y + h - 4, w, 4, (10, 8, 14, 255));
            for k in 0..8 {
                d.fill(
                    x + 8,
                    y + h + k,
                    w.saturating_sub(16),
                    1,
                    (c.0, c.1, c.2, (40 - k * 5) as u8),
                );
            }
            return;
        }
    }
    // Sombra do tampo na parede, embaixo.
    for (i, a) in [120u8, 80, 40].into_iter().enumerate() {
        d.fill(
            x + 4,
            y + h + i as i32,
            w.saturating_sub(8),
            1,
            (0, 0, 0, a),
        );
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
/// (A "jogado" row lived here once — the last-played date is the
/// Histórico's job, and the row was what overflowed into the panel's
/// buttons.)
/// A prateleira de madeira sob uma fileira de caixas (a estante de
/// verdade, do mock `estante_mock` para o app): fita LED no lábio superior
/// com brilho escorrendo na parede (atrás das capas — as caixas desenhadas
/// por cima recortam o brilho e ficam retroiluminadas), três pranchas de
/// tons vizinhos com veios e juntas, boca escura e mão francesa
/// (quarto-de-círculo) sob as duas pontas — curta, para não invadir a copy
/// da fileira de baixo. Mora no vão de `GAP`/`STRIP_GAP` entre fileiras —
/// nenhuma geometria de clique muda.
fn draw_shelf_board(d: &mut impl Paint, x: i32, w: u32, y: i32, h: i32) {
    // O tampo visto de cima, entrando para o fundo: uma faixa mais clara
    // acima do lábio, com a quina do fundo escura — a prateleira ganha
    // profundidade e as capas pousam EM CIMA dela.
    const DEPTH: i32 = 9;
    d.fill(x, y - DEPTH, w, DEPTH as u32, (70, 53, 41, 255));
    d.fill(x, y - DEPTH, w, 1, (28, 19, 13, 255));
    d.fill(x, y - DEPTH + 1, w, 2, (56, 42, 33, 255));
    // A quina da frente do tampo pegando a luz.
    d.fill(x, y - 1, w, 1, (112, 90, 72, 255));

    // O tampo: três pranchas de tons vizinhos (madeira de verdade nunca é
    // uma cor só), lábio lavado pela fita, veios e juntas, boca escura.
    let face_h = (h - 6).max(1);
    let prancha = (w as i32 / 3).max(1);
    for (p, tom) in [(0, (62, 47, 36)), (1, (59, 45, 34)), (2, (65, 49, 38))] {
        let px = x + p * prancha;
        let pw = if p == 2 {
            w as i32 - 2 * prancha
        } else {
            prancha
        };
        d.fill(
            px,
            y + 2,
            pw as u32,
            face_h as u32,
            (tom.0, tom.1, tom.2, 255),
        );
    }
    d.fill(x, y, w, 2, (96, 76, 60, 255));
    let veios = [
        (5u32, 8i32, (52, 39, 30u8)),
        (7, 30, (55, 41, 32u8)),
        (9, 14, (50, 38, 29u8)),
    ];
    for (dy, ox, c) in veios {
        if h > dy as i32 + 6 {
            d.fill(
                x + ox,
                y + dy as i32,
                w.saturating_sub(ox as u32 + 24),
                1,
                (c.0, c.1, c.2, 255),
            );
        }
    }
    for p in [1, 2] {
        d.fill(x + p * prancha, y + 2, 1, face_h as u32, (40, 28, 20, 255));
    }
    d.fill(x, y + h - 4, w, 4, (36, 26, 19, 255));

    // Sombra do tampo na parede, embaixo.
    for (i, c) in [(13u8, 11u8, 10u8), (16u8, 14u8, 12u8), (19u8, 17u8, 15u8)]
        .into_iter()
        .enumerate()
    {
        d.fill(
            x + 4,
            y + h + i as i32,
            w.saturating_sub(8),
            1,
            (c.0, c.1, c.2, 255),
        );
    }

    // Mão francesa nos cantos: quarto-de-círculo sob cada ponta — r curto
    // para a ponta não descer na copy da fileira de baixo (o vão entre a
    // prateleira e o que vem depois é de 14 a 18px).
    let r: i32 = 8;
    for i in 0..r {
        let dx = (((r * r - i * i) as f32).sqrt().round() as i32).max(1);
        d.fill(x + 2, y + h + i, dx as u32, 1, (52, 38, 29, 255));
        d.fill(
            x + w as i32 - 2 - dx,
            y + h + i,
            dx as u32,
            1,
            (52, 38, 29, 255),
        );
    }
}
