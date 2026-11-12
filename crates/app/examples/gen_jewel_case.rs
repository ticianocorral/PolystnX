//! Põe a capa 2D (frente quadrada) dentro de uma caixa de CD (jewel case):
//! lombada com as dobradiças, moldura de acrílico e o brilho do plástico.
//! `cargo run -p polystnx-app --release --example gen_jewel_case -- capa.jpg saida.png`

use image::{imageops, Rgba, RgbaImage};

const W: u32 = 1380;
const H: u32 = 1250;
const ART: u32 = 1200;
const ART_X: i32 = 140;
const ART_Y: i32 = 25;

fn in_round(x: f32, y: f32, w: f32, h: f32, r: f32) -> bool {
    let cx = x.clamp(r, w - r);
    let cy = y.clamp(r, h - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

fn blend(px: &mut Rgba<u8>, c: [u8; 3], a: f32) {
    for (dst, src) in px.0.iter_mut().zip(c) {
        *dst = (*dst as f32 * (1.0 - a) + src as f32 * a).round() as u8;
    }
    px.0[3] = (px.0[3] as f32 + (255.0 - px.0[3] as f32) * a).round() as u8;
}

fn main() -> anyhow::Result<()> {
    let mut a = std::env::args().skip(1);
    let (src, out) = (
        a.next()
            .ok_or_else(|| anyhow::anyhow!("uso: capa.jpg saida.png"))?,
        a.next().unwrap_or_else(|| "caixa.png".into()),
    );
    let art = image::open(&src)?.to_rgba8();
    let art = imageops::resize(&art, ART, ART, imageops::FilterType::Lanczos3);
    let mut img = RgbaImage::from_pixel(W, H, Rgba([0, 0, 0, 0]));
    let (cw, ch) = (W as f32 - 26.0, H as f32 - 26.0);

    // A sombra no chão: camadas de retângulos arredondados cada vez mais
    // juntos da caixa, deslocadas para baixo e para a direita.
    for (grow, alpha) in [(12.0f32, 0.05f32), (8.0, 0.07), (4.0, 0.09), (0.0, 0.12)] {
        for y in 0..H {
            for x in 0..W {
                let (fx, fy) = (x as f32 - 6.0 + grow, y as f32 - 10.0 + grow);
                if fx >= 0.0
                    && fy >= 0.0
                    && in_round(fx, fy, cw + 2.0 * grow, ch + 2.0 * grow, 22.0)
                {
                    blend(img.get_pixel_mut(x, y), [0, 0, 0], alpha);
                }
            }
        }
    }

    // O corpo da caixa: acrílico claro; a lombada mais escura (a bandeja).
    for y in 0..(H - 26) {
        for x in 0..(W - 26) {
            let (fx, fy) = (x as f32, y as f32);
            if !in_round(fx, fy, cw, ch, 18.0) {
                continue;
            }
            let t = fy / ch;
            let base = if x < 128 {
                // lombada: preto-azulado com degradê
                [
                    (36.0 - 14.0 * t) as u8,
                    (38.0 - 14.0 * t) as u8,
                    (44.0 - 14.0 * t) as u8,
                ]
            } else {
                [
                    (226.0 - 30.0 * t) as u8,
                    (232.0 - 30.0 * t) as u8,
                    (238.0 - 28.0 * t) as u8,
                ]
            };
            *img.get_pixel_mut(x, y) = Rgba([base[0], base[1], base[2], 255]);
        }
    }
    // As dobradiças da lombada: quatro dentes claros, dois em cima e dois embaixo.
    for (y0, hh) in [
        (40, 120),
        (170, 60),
        (ch as i32 - 230, 60),
        (ch as i32 - 160, 120),
    ] {
        for y in y0..y0 + hh {
            for x in 18..110 {
                let (fx, fy) = (x as f32 - 18.0, (y - y0) as f32);
                if in_round(fx, fy, 92.0, hh as f32, 10.0) {
                    let t = (fy / hh as f32 - 0.5).abs();
                    let v = 150.0 + 60.0 * (1.0 - t * 2.0);
                    *img.get_pixel_mut(x as u32, y as u32) =
                        Rgba([v as u8, (v + 4.0) as u8, (v + 8.0) as u8, 255]);
                }
            }
        }
    }
    // A arte, com um filete escuro em volta.
    for y in -2..(ART as i32 + 2) {
        for x in -2..(ART as i32 + 2) {
            let (px, py) = (ART_X + x, ART_Y + y);
            let inside = (0..ART as i32).contains(&x) && (0..ART as i32).contains(&y);
            let p = if inside {
                *art.get_pixel(x as u32, y as u32)
            } else {
                Rgba([20, 22, 26, 255])
            };
            *img.get_pixel_mut(px as u32, py as u32) = p;
        }
    }
    // O brilho do acrílico: faixa diagonal suave sobre a arte e a moldura.
    for y in 0..(H - 26) {
        for x in 140..(W - 26) {
            let (fx, fy) = (x as f32, y as f32);
            if !in_round(fx, fy, cw, ch, 18.0) {
                continue;
            }
            let d = ((fx - 140.0) + fy * 0.9 - 760.0) / 140.0;
            let streak = (-(d * d)).exp() * 0.16;
            let corner = ((1.0 - (fx - 140.0) / 700.0).max(0.0) * (1.0 - fy / 600.0).max(0.0))
                .powi(2)
                * 0.18;
            blend(
                img.get_pixel_mut(x, y),
                [255, 255, 255],
                (streak + corner).min(0.3),
            );
        }
    }
    // A borda: fio claro em cima/esquerda e sombra embaixo/direita.
    for y in 0..(H - 26) {
        for x in 0..(W - 26) {
            let (fx, fy) = (x as f32, y as f32);
            if !in_round(fx, fy, cw, ch, 18.0) {
                continue;
            }
            let edge_in = !in_round(fx - 3.0, fy - 3.0, cw - 6.0, ch - 6.0, 15.0);
            if edge_in {
                let light = (1.0 - fx / cw) * 0.5 + (1.0 - fy / ch) * 0.5;
                if light > 0.55 {
                    blend(img.get_pixel_mut(x, y), [255, 255, 255], 0.55);
                } else {
                    blend(img.get_pixel_mut(x, y), [30, 34, 40], 0.45);
                }
            }
        }
    }
    img.save(&out)?;
    println!("{out}");
    Ok(())
}
