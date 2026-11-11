//! Gera a marca prateada do queixo da TV (`brand_polystnx.rgba`) a partir do
//! nome "PolyStnX" da logo do console: recorta o letreiro, e dá a ele o
//! acabamento de cromado escovado (degradê metálico, chanfro com luz de cima
//! à esquerda e um filete escuro de contorno), como a marca prateada do
//! SixteeN. Dev tool — a saída é versionada em `crates/platform/src/assets`.
//!
//! `cargo run -p polystnx-app --example gen_brand -- <saida.rgba> [preview.png]`
//! Escreve RGBA cru (largura/altura impressas na saída).

fn blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    // Caixa separável: duas passadas (horizontal e vertical) com janela 2r+1.
    let mut tmp = vec![0.0; w * h];
    let k = (2 * r + 1) as f32;
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for dx in 0..=2 * r {
                let xx = (x + dx).saturating_sub(r).min(w - 1);
                s += src[y * w + xx];
            }
            tmp[y * w + x] = s / k;
        }
    }
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for dy in 0..=2 * r {
                let yy = (y + dy).saturating_sub(r).min(h - 1);
                s += tmp[yy * w + x];
            }
            out[y * w + x] = s / k;
        }
    }
    out
}

fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).expect("saida.rgba");
    let preview = std::env::args().nth(2);
    let logo = image::open("crates/app/assets/console_logo.png")?.to_rgba8();
    let (lw, lh) = logo.dimensions();
    // O letreiro "PolyStnX": a faixa de baixo da logo (acima dela mora o P).
    let (y0, y1) = ((lh as f32 * 0.70) as u32, lh);
    // Caixa justa em x: onde há alpha nessa faixa.
    let (mut x0, mut x1) = (lw, 0);
    let (mut ty0, mut ty1) = (y1, y0);
    for y in y0..y1 {
        for x in 0..lw {
            if logo.get_pixel(x, y)[3] > 24 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                ty0 = ty0.min(y);
                ty1 = ty1.max(y);
            }
        }
    }
    let pad = 8u32;
    let (cw, ch) = (x1 - x0 + 1, ty1 - ty0 + 1);
    // Escala x3: mais pixels para o chanfro (a marca desce redimensionada).
    let s = 3usize;
    let (w, h) = ((cw + 2 * pad) as usize * s, (ch + 2 * pad) as usize * s);
    let mut a = vec![0.0f32; w * h];
    for y in 0..ch {
        for x in 0..cw {
            let al = logo.get_pixel(x0 + x, ty0 + y)[3] as f32 / 255.0;
            for dy in 0..s {
                for dx in 0..s {
                    let (px, py) = ((x + pad) as usize * s + dx, (y + pad) as usize * s + dy);
                    a[py * w + px] = al;
                }
            }
        }
    }
    // O letreiro original é traço fino: engorda o suficiente para o chanfro
    // ler como letra cromada (dilatação por blur + limiar).
    let fat = blur(&a, w, h, 5 * s / 3);
    let solid: Vec<f32> = fat
        .iter()
        .map(|v| ((v - 0.18) * 5.0).clamp(0.0, 1.0))
        .collect();
    // Campo de altura: o sólido borrado — o chanfro nasce do gradiente.
    let height = blur(&solid, w, h, 3 * s);
    let rim = blur(&solid, w, h, s * 2 / 3); // contorno escuro fino
    let mut px = vec![0u8; w * h * 4];
    let lum = |x: usize, y: usize| height[y.min(h - 1) * w + x.min(w - 1)];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let cov = solid[y * w + x];
            let halo = rim[y * w + x];
            // Luz de cima à esquerda sobre o gradiente do relevo.
            let gx = lum(x + 1, y) - lum(x.saturating_sub(1), y);
            let gy = lum(x, y + 1) - lum(x, y.saturating_sub(1));
            let bevel = (-(gx + gy) * 9.0).clamp(-1.0, 1.0);
            // Cromado: céu claro em cima, horizonte escuro no meio, reflexo
            // quente embaixo — em função da linha dentro do letreiro.
            let t = y as f32 / h as f32;
            let chrome = if t < 0.48 {
                252.0 - 135.0 * (t / 0.48).powf(1.3)
            } else {
                82.0 + 150.0 * ((t - 0.48) / 0.52).powf(0.8)
            };
            // Escovado: ruído horizontal leve.
            let hsh =
                ((x as u32).wrapping_mul(2654435761) ^ (y as u32 / 2).wrapping_mul(40503)) >> 24;
            let brushed = (hsh as f32 / 255.0 - 0.5) * 14.0;
            let v = (chrome + bevel * 80.0 + brushed).clamp(0.0, 255.0);
            // Miolo prateado; borda escura onde a cobertura cai.
            let edge = (1.0 - cov).max(0.0);
            let dark = (halo * (1.0 - cov * 0.0)).clamp(0.0, 1.0);
            let body = v * (0.35 + 0.65 * cov);
            let alpha = (halo.max(cov) * 255.0).round();
            let c = if cov > 0.85 {
                body
            } else {
                body * (0.4 + 0.6 * cov) * (1.0 - edge * 0.5)
            };
            let (r, g, b) = (c * 0.97, c * 0.99, (c * 1.04).min(255.0));
            let _ = dark;
            px[i] = r as u8;
            px[i + 1] = g as u8;
            px[i + 2] = b as u8;
            px[i + 3] = alpha as u8;
        }
    }
    // Desce 3x por média de área (alpha pré-multiplicado): o chanfro foi
    // calculado em alta, a marca embutida fica pequena.
    let (ow, oh) = (w / s, h / s);
    let mut small = vec![0u8; ow * oh * 4];
    for y in 0..oh {
        for x in 0..ow {
            let (mut r, mut g, mut b, mut al) = (0.0f32, 0.0, 0.0, 0.0);
            for dy in 0..s {
                for dx in 0..s {
                    let i = ((y * s + dy) * w + x * s + dx) * 4;
                    let a = px[i + 3] as f32 / 255.0;
                    r += px[i] as f32 * a;
                    g += px[i + 1] as f32 * a;
                    b += px[i + 2] as f32 * a;
                    al += a;
                }
            }
            let o = (y * ow + x) * 4;
            if al > 0.0 {
                small[o] = (r / al) as u8;
                small[o + 1] = (g / al) as u8;
                small[o + 2] = (b / al) as u8;
            }
            small[o + 3] = (al / (s * s) as f32 * 255.0).round() as u8;
        }
    }
    let (w, h, px) = (ow, oh, small);
    std::fs::write(&out, &px)?;
    println!("{w}x{h}");
    if let Some(p) = preview {
        // Prévia sobre o cinza escuro do queixo.
        let mut img = image::RgbImage::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                let a = px[i + 3] as f32 / 255.0;
                let bg = 22.0;
                let m = |c: u8| (c as f32 * a + bg * (1.0 - a)) as u8;
                img.put_pixel(
                    x as u32,
                    y as u32,
                    image::Rgb([m(px[i]), m(px[i + 1]), m(px[i + 2])]),
                );
            }
        }
        img.save(p)?;
    }
    Ok(())
}
