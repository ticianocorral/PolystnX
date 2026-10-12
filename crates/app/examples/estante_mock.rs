//! MOCK — a estante de games do PolystnX desenhada como uma estante de
//! verdade: prateleiras de madeira (tampo com veio, suporte, sombra na
//! parede), jogos em pé como caixas com lombada e arte própria, sombra de
//! contato, e o jogo focado puxado pra fora da fileira (o painel lateral
//! mostra as infos dele, com as barras do HowLongToBeat). 100% sintético —
//! nenhum pixel de jogo real.
//!
//! Run: cargo run -p polystnx-app --example estante_mock — escreve
//! /tmp/estante-mock/estante.bmp (converter com sips para png).

use std::path::PathBuf;

use polystnx_platform::{Platform, ShelfPanelInfo};

const LARGURA_CAIXA: i32 = 112;
const ALTURA_CAIXA: i32 = 148;
const PRATELEIRA_H: i32 = 16;
const MARGEM: i32 = 34;

struct Caixa {
    id: u64,
    titulo: &'static str,
    cor: (u8, u8, u8),
}

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/estante-mock");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v1.1.1-beta\nPCSX Rearmed r26");

    // As caixas da estante — homebrews inventados (o alfabeto do mini-font
    // dos mocks é A B C D E F H I K L M N O P Q R S T U V W X Y 0-3 8).
    let caixas = [
        Caixa {
            id: 0xC0FF01,
            titulo: "MUNDO DO TOMATE",
            cor: (200, 40, 40),
        },
        Caixa {
            id: 0xC0FF02,
            titulo: "CENOURA CRAVE",
            cor: (230, 140, 30),
        },
        Caixa {
            id: 0xC0FF03,
            titulo: "SUPER ALFACE",
            cor: (60, 160, 70),
        },
        Caixa {
            id: 0xC0FF04,
            titulo: "MILHO MALUCO",
            cor: (235, 200, 50),
        },
        Caixa {
            id: 0xC0FF05,
            titulo: "TOMATE VERDE",
            cor: (90, 170, 60),
        },
        Caixa {
            id: 0xC0FF06,
            titulo: "SALADA MORTA",
            cor: (150, 60, 160),
        },
        Caixa {
            id: 0xC0FF07,
            titulo: "ABOBRINHA",
            cor: (120, 190, 80),
        },
        Caixa {
            id: 0xC0FF08,
            titulo: "PIMENTA MAGNA",
            cor: (220, 70, 90),
        },
        Caixa {
            id: 0xC0FF09,
            titulo: "RUCULA NOTURNA",
            cor: (70, 110, 200),
        },
    ];
    for c in &caixas {
        cab.set_image(
            c.id,
            LARGURA_CAIXA as u32,
            ALTURA_CAIXA as u32,
            &caixa_image(c),
        );
    }

    // O painel lateral com as infos do jogo puxado (o MUNDO DO TOMATE),
    // incluindo as barras do HowLongToBeat.
    let hltb_icon: u64 = 0x484C_5442;
    if let Ok(icon) = image::load_from_memory(polystnx_app::HLTB_ICON_PNG) {
        let icon = icon.to_rgba8();
        cab.set_image(hltb_icon, icon.width(), icon.height(), icon.as_raw());
    }
    let hltb = Some(polystnx_platform::HltbBars {
        icon: hltb_icon,
        rows: [
            ("história principal", 45u64, (52u8, 124, 246)),
            ("principal + extras", 90, (86, 88, 204)),
            ("completista", 240, (198, 60, 72)),
            ("todos os estilos", 120, (44, 162, 156)),
        ]
        .into_iter()
        .map(|(label, minutos, color)| polystnx_platform::HltbBar {
            label: label.to_string(),
            text: if minutos >= 60 {
                format!("{}h {}min", minutos / 60, minutos % 60)
            } else {
                format!("{minutos}min")
            },
            frac: minutos as f32 / 240.0,
            color,
        })
        .collect(),
    });
    cab.set_shelf_panel(ShelfPanelInfo {
        title: "Mundo do Tomate".to_string(),
        logo_img: Some(caixas[0].id),
        cartridge_img: None,
        backcover_img: None,
        release: Some("1993".to_string()),
        info: vec![
            ("editora".to_string(), "Horta Games".to_string()),
            ("conquistas".to_string(), "20 de 21 (95%)".to_string()),
        ],
        section: polystnx_platform::PanelSection::Informacoes,
        scroll: 0,
        favorite: Some(true),
        achievements: true,
        achievements_reason: None,
        manual_first_page: None,
        award_img: None,

        igdb: None,        hltb,
    });

    cab.capture_shelf(
        (16, 14, 13),
        |d| {
            let (sw, sh) = d.size();
            let (sw, sh) = (sw as i32, sh as i32);

            // Parede: gradiente vertical quente, escuro em cima.
            for y in 0..sh {
                let t = y as f32 / sh as f32;
                d.fill(
                    0,
                    y,
                    sw as u32,
                    1,
                    (
                        (24.0 + 6.0 * t) as u8,
                        (22.0 + 5.0 * t) as u8,
                        (20.0 + 5.0 * t) as u8,
                        255,
                    ),
                );
            }

            // Cabeçalho: contador e busca.
            let dim = (150, 150, 158);
            d.text(MARGEM, 16, 2, (200, 196, 190), "55 games");
            let (bx, bw, bh) = (sw - MARGEM - 280, 280, 32);
            d.outline(bx, 16, bw, bh, 1, (96, 92, 86, 255));
            d.fill(
                bx + 1,
                17,
                (bw - 2) as u32,
                (bh - 2) as u32,
                (30, 27, 24, 255),
            );
            d.text(bx + 10, 22, 1, dim, "buscar...");

            // Três prateleiras: recentes, todos, favoritos.
            let rotulos = ["jogados recentemente", "todos os jogos", "favoritos"];
            let fileiras: [&[usize]; 3] = [&[0, 1, 2, 3], &[4, 5, 6, 7, 8], &[0, 5]];
            let topo = 64;
            let passo = (sh - topo - 8) / 3;
            let foco = (0, 1); // fileira 0, posição 1 — o puxado da estante

            for k in 0..3usize {
                let prateleira_y = topo + passo * (k as i32 + 1) - PRATELEIRA_H;
                let caixa_y = prateleira_y - ALTURA_CAIXA;

                // Rótulo da fileira, na parede, acima das caixas.
                d.text(MARGEM, caixa_y - 26, 1, dim, rotulos[k]);

                // Brilho quente da lâmpada escorrendo na parede, logo
                // acima do tampo.
                for (i, alpha) in [0.10, 0.05].into_iter().enumerate() {
                    let c = mistura((26, 24, 22), (255, 180, 90), alpha);
                    d.fill(
                        MARGEM,
                        prateleira_y - 2 - i as i32,
                        (sw - 2 * MARGEM) as u32,
                        1,
                        (c.0, c.1, c.2, 255),
                    );
                }

                // A madeira: tampo com luz no topo, veio no rosto, boca
                // escura embaixo e suportes nas pontas.
                d.fill(
                    MARGEM,
                    prateleira_y,
                    (sw - 2 * MARGEM) as u32,
                    PRATELEIRA_H as u32,
                    (106, 71, 44, 255),
                );
                d.fill(
                    MARGEM,
                    prateleira_y,
                    (sw - 2 * MARGEM) as u32,
                    2,
                    (150, 110, 68, 255),
                );
                d.fill(
                    MARGEM,
                    prateleira_y + PRATELEIRA_H - 4,
                    (sw - 2 * MARGEM) as u32,
                    4,
                    (62, 42, 26, 255),
                );
                for (dy, c) in [(5, (92u8, 61, 38)), (9, (96, 64, 40)), (12, (90, 60, 37))] {
                    d.fill(
                        MARGEM + 6,
                        prateleira_y + dy,
                        (sw - 2 * MARGEM - 12) as u32,
                        1,
                        (c.0, c.1, c.2, 255),
                    );
                }
                // Juntas das tábuas.
                for jx in [sw / 3, 2 * sw / 3] {
                    d.fill(
                        jx,
                        prateleira_y + 2,
                        1,
                        (PRATELEIRA_H - 4) as u32,
                        (66, 43, 27, 255),
                    );
                }
                // Sombra da prateleira na parede, embaixo.
                for (i, (_, c)) in [(0, (13u8, 11, 10)), (1, (16, 14, 12)), (2, (19, 17, 15))]
                    .into_iter()
                    .enumerate()
                {
                    d.fill(
                        MARGEM + 4,
                        prateleira_y + PRATELEIRA_H + i as i32,
                        (sw - 2 * MARGEM - 8) as u32,
                        1,
                        (c.0, c.1, c.2, 255),
                    );
                }
                // Suportes.
                for sx in [MARGEM + 6, sw - MARGEM - 22] {
                    d.fill(sx, prateleira_y + PRATELEIRA_H, 16, 8, (48, 32, 20, 255));
                }

                // As caixas em pé, com sombra de contato na madeira e na
                // parede (luz vem de cima à esquerda).
                let fileira = fileiras[k];
                let passo_x = LARGURA_CAIXA + 26;
                for (pos, &idx) in fileira.iter().enumerate() {
                    let c = &caixas[idx];
                    let puxado = (k, pos) == foco;
                    let (dx, dy) = if puxado { (-10, 18) } else { (0, 0) };
                    let x = MARGEM + 10 + pos as i32 * passo_x + dx;
                    let y = caixa_y + dy;

                    // Sombra na parede (lado direito) e na madeira (base).
                    let sombra = if puxado { (10, 8, 8) } else { (14, 12, 11) };
                    d.fill(
                        x + LARGURA_CAIXA,
                        y + 6,
                        6,
                        (ALTURA_CAIXA - 6) as u32,
                        (sombra.0, sombra.1, sombra.2, 255),
                    );
                    let base = if puxado {
                        y + ALTURA_CAIXA
                    } else {
                        prateleira_y
                    };
                    d.fill(
                        x + 4,
                        base,
                        (LARGURA_CAIXA + 12) as u32,
                        if puxado { 7 } else { 5 },
                        (26, 17, 11, 255),
                    );

                    d.image_fit(c.id, x, y, LARGURA_CAIXA as u32, ALTURA_CAIXA as u32);

                    // A face lateral direita da caixa (3D barato): faixa
                    // escura da própria cor.
                    let esc = escurece(c.cor, 0.45);
                    d.fill(
                        x + LARGURA_CAIXA - 7,
                        y + 2,
                        5,
                        (ALTURA_CAIXA - 4) as u32,
                        (esc.0, esc.1, esc.2, 255),
                    );
                    // Fio de luz no canto esquerdo.
                    let luz = mistura(c.cor, (255, 255, 255), 0.35);
                    d.fill(
                        x,
                        y + 2,
                        1,
                        (ALTURA_CAIXA - 4) as u32,
                        (luz.0, luz.1, luz.2, 255),
                    );

                    if puxado {
                        d.outline(
                            x - 3,
                            y - 3,
                            (LARGURA_CAIXA + 6) as u32,
                            (ALTURA_CAIXA + 6) as u32,
                            2,
                            (245, 245, 250, 255),
                        );
                    }
                }

                // Favoritos: estrela na primeira caixa da fileira.
                if k == 2 {
                    let x = MARGEM + 10 + LARGURA_CAIXA;
                    d.star(x - 8, caixa_y + 10, 7, (240, 200, 60));
                }
            }

            // Uma pilha de caixas deitadas no fim da última fileira.
            let pilha_x = MARGEM + 10 + fileiras[2].len() as i32 * (LARGURA_CAIXA + 26) + 40;
            let pilha_y = topo + passo * 3 - PRATELEIRA_H;
            for (i, cor) in [(150u8, 60, 160), (70, 110, 200)].into_iter().enumerate() {
                let y = pilha_y - 22 - i as i32 * 24;
                let viva = (cor.0, cor.1, cor.2, 255);
                d.fill(
                    pilha_x,
                    y,
                    (LARGURA_CAIXA + 30) as u32,
                    22,
                    (cor.0 / 5, cor.1 / 5, cor.2 / 5, 255),
                );
                d.outline(pilha_x, y, (LARGURA_CAIXA + 30) as u32, 22, 1, viva);
                let borda = escurece(cor, 0.4);
                d.fill(
                    pilha_x + 3,
                    y + 19,
                    (LARGURA_CAIXA + 24) as u32,
                    2,
                    (borda.0, borda.1, borda.2, 255),
                );
            }
        },
        &out.join("estante.bmp"),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", out.join("estante.bmp").display());
    Ok(())
}

/// A arte da caixa (RGBA no tamanho exato da caixa): fundo escuro da cor,
/// moldura viva, lombada à esquerda com duas marcas, céu com sol e chão, o
/// nome em duas linhas e a tag da editora embaixo.
fn caixa_image(c: &Caixa) -> Vec<u8> {
    let w = LARGURA_CAIXA;
    let h = ALTURA_CAIXA;
    let mut px = vec![0u8; (w * h * 4) as usize];
    let (r, g, b) = c.cor;
    let fundo = (r / 5, g / 5, b / 5, 255);
    let viva = (r, g, b, 255);
    fill_rgba(&mut px, w as u32, h as u32, 0, 0, w, h, fundo);

    // Céu mais claro no terço de cima, sol, e chão faixado embaixo.
    let ceu = mistura((r / 5, g / 5, b / 5), c.cor, 0.18);
    fill_rgba(
        &mut px,
        w as u32,
        h as u32,
        3,
        3,
        w - 6,
        h / 3,
        (ceu.0, ceu.1, ceu.2, 255),
    );
    disco(&mut px, w as u32, h as u32, w - 34, 24, 10, viva);
    disco(
        &mut px,
        w as u32,
        h as u32,
        w - 34,
        24,
        6,
        (255, 244, 200, 255),
    );
    let chao = escurece(c.cor, 0.3);
    fill_rgba(
        &mut px,
        w as u32,
        h as u32,
        3,
        h - 46,
        w - 6,
        16,
        (chao.0, chao.1, chao.2, 255),
    );

    // Moldura.
    for t in 0..3 {
        fill_rgba(&mut px, w as u32, h as u32, t, t, w - t * 2, 1, viva);
        fill_rgba(
            &mut px,
            w as u32,
            h as u32,
            t,
            h - 1 - t,
            w - t * 2,
            1,
            viva,
        );
        fill_rgba(&mut px, w as u32, h as u32, t, t, 1, h - t * 2, viva);
        fill_rgba(
            &mut px,
            w as u32,
            h as u32,
            w - 1 - t,
            t,
            1,
            h - t * 2,
            viva,
        );
    }
    // Lombada: faixa escura à esquerda com duas marcas da cor.
    let lomb = escurece(c.cor, 0.35);
    fill_rgba(
        &mut px,
        w as u32,
        h as u32,
        3,
        3,
        10,
        h - 6,
        (lomb.0, lomb.1, lomb.2, 255),
    );
    fill_rgba(&mut px, w as u32, h as u32, 6, 14, 4, 18, viva);
    fill_rgba(&mut px, w as u32, h as u32, 6, 40, 4, 8, viva);

    // O nome, em duas linhas centradas (a primeira palavra grande; a sobra
    // desce para a escala pequena quando não cabe).
    let (a, bb) = c.titulo.split_once(' ').unwrap_or((c.titulo, ""));
    let cabe = |texto: &str, s: i32| texto.len() as i32 * 6 * s <= w - 24;
    let s1 = [2, 1].into_iter().find(|&s| cabe(a, s)).unwrap_or(1);
    let x1 = (w - a.len() as i32 * 6 * s1) / 2 + 3;
    draw_text_rgba(&mut px, w as u32, h as u32, x1, h / 2 - 16, s1, a, viva);
    let s2 = [2, 1].into_iter().find(|&s| cabe(bb, s)).unwrap_or(1);
    let x2 = (w - bb.len() as i32 * 6 * s2) / 2 + 3;
    draw_text_rgba(
        &mut px,
        w as u32,
        h as u32,
        x2,
        h / 2 + 6,
        s2,
        bb,
        (235, 235, 235, 255),
    );
    // Tag da editora.
    draw_text_rgba(
        &mut px,
        w as u32,
        h as u32,
        w / 2 - 30,
        h - 26,
        1,
        "HORTA GAMES",
        (200, 200, 200, 255),
    );
    px
}

// ---------------------------------------------------------------- helpers

fn mistura(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

fn escurece(c: (u8, u8, u8), f: f32) -> (u8, u8, u8) {
    (
        (c.0 as f32 * f) as u8,
        (c.1 as f32 * f) as u8,
        (c.2 as f32 * f) as u8,
    )
}

fn fill_rgba(px: &mut [u8], w: u32, h: u32, x: i32, y: i32, ww: i32, hh: i32, c: (u8, u8, u8, u8)) {
    for yy in y..y + hh {
        for xx in x..x + ww {
            put_rgba(px, w, h, xx, yy, c);
        }
    }
}

fn disco(px: &mut [u8], w: u32, h: u32, cx: i32, cy: i32, r: i32, c: (u8, u8, u8, u8)) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                put_rgba(px, w, h, cx + dx, cy + dy, c);
            }
        }
    }
}

fn put_rgba(px: &mut [u8], w: u32, h: u32, x: i32, y: i32, c: (u8, u8, u8, u8)) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = ((y as u32 * w + x as u32) * 4) as usize;
    if i + 3 < px.len() {
        px[i] = c.0;
        px[i + 1] = c.1;
        px[i + 2] = c.2;
        px[i + 3] = c.3;
    }
}

fn draw_text_rgba(
    px: &mut [u8],
    w: u32,
    h: u32,
    x0: i32,
    y0: i32,
    s: i32,
    text: &str,
    c: (u8, u8, u8, u8),
) {
    let mut pen = x0;
    for ch in text.chars() {
        if let Some(g) = glyph(ch) {
            for (gy, row) in g.iter().enumerate() {
                for (gx, cell) in row.chars().enumerate() {
                    if cell != '#' {
                        continue;
                    }
                    for yy in 0..s {
                        for xx in 0..s {
                            put_rgba(
                                px,
                                w,
                                h,
                                pen + gx as i32 * s + xx,
                                y0 + gy as i32 * s + yy,
                                c,
                            );
                        }
                    }
                }
            }
        }
        pen += 6 * s;
    }
}

fn glyph(ch: char) -> Option<[&'static str; 5]> {
    Some(match ch {
        'A' => [".###.", "#...#", "#####", "#...#", "#...#"],
        'B' => ["####.", "#...#", "####.", "#...#", "####."],
        'C' => [".###.", "#....", "#....", "#....", ".###."],
        'D' => ["####.", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "###..", "#....", "#####"],
        'F' => ["#####", "#....", "####.", "#....", "#...."],
        'G' => [".###.", "#....", "#.##.", "#...#", ".###."],
        'H' => ["#...#", "#...#", "#####", "#...#", "#...#"],
        'I' => ["#####", "..#..", "..#..", "..#..", "#####"],
        'K' => ["#...#", "#..#.", "###..", "#..#.", "#...#"],
        'L' => ["#....", "#....", "#....", "#....", "#####"],
        'M' => ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'N' => ["#...#", "##..#", "#.#.#", "#..##", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", ".###."],
        'P' => ["####.", "#...#", "####.", "#....", "#...."],
        'Q' => [".###.", "#...#", "#.#.#", "#..#.", ".##.#"],
        'R' => ["####.", "#...#", "####.", "#..#.", "#...#"],
        'S' => [".####", "#....", ".###.", "....#", "####."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#.."],
        'U' => ["#...#", "#...#", "#...#", "#...#", ".###."],
        'V' => ["#...#", "#...#", "#...#", "#.#.#", ".#.#."],
        'W' => ["#...#", "#...#", "#.#.#", "##.##", "#...#"],
        'X' => ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"],
        'Y' => ["#...#", ".#.#.", "..#..", "..#..", "..#.."],
        '0' => [".###.", "#..##", "#.#.#", "##..#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "#####"],
        '2' => [".###.", "#...#", "..##.", ".#...", "#####"],
        '3' => ["####.", "....#", ".###.", "....#", "####."],
        '-' => [".....", ".....", "#####", ".....", "....."],
        ' ' => [".....", ".....", ".....", ".....", "....."],
        _ => return None,
    })
}
