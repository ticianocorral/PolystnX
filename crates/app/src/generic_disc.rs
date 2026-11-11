//! O disco padrão do console: quando o jogo não tem arte de disco, o CD que
//! gira na face do console é este — prata com brilho angular, anel do cubo,
//! furo central e o nome do jogo escrito na área impressa. Gerado por código
//! (RGBA, `SIZE` x `SIZE`), com os cantos e o furo transparentes como a arte
//! de verdade fica depois da máscara do runner.

use noto_sans_mono_bitmap::{get_raster, get_raster_width, FontWeight, RasterHeight};

pub const SIZE: u32 = 512;

/// O disco genérico de `title`, como `(largura, altura, rgba)`.
pub fn generic_disc(title: &str) -> (u32, u32, Vec<u8>) {
    let n = SIZE as usize;
    let mut px = vec![0u8; n * n * 4];
    let c = n as f32 / 2.0;
    let r = c;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (dx * dx + dy * dy).sqrt();
            if d > r || d < r * 0.125 {
                continue; // fora do disco ou o furo: transparente
            }
            let t = d / r;
            let ang = dy.atan2(dx);
            // Prata com dois lóbulos de brilho, como a face de um CD.
            let sheen = 0.5 + 0.5 * (ang * 2.0 + 0.6).cos();
            let mut v = 168.0 + 58.0 * sheen * (0.4 + 0.6 * t);
            let mut tint = (0.0, 0.0, 0.0);
            if t < 0.30 {
                // O cubo: anel de plástico claro e fosco.
                v = 206.0 - 12.0 * ((t - 0.2).abs() * 10.0);
            } else if t < 0.33 {
                v *= 0.78; // o degrau entre o cubo e a área de dados
            } else if t > 0.965 {
                v *= 0.85; // a borda
            } else {
                // A área impressa: leve tom frio e arco-íris bem fraco.
                tint = (
                    8.0 * (ang * 3.0).sin(),
                    6.0 * (ang * 3.0 + 2.1).sin(),
                    10.0 * (ang * 3.0 + 4.2).sin(),
                );
            }
            let i = (y * n + x) * 4;
            px[i] = (v + tint.0).clamp(0.0, 255.0) as u8;
            px[i + 1] = (v + tint.1 + 3.0).clamp(0.0, 255.0) as u8;
            px[i + 2] = (v + tint.2 + 8.0).clamp(0.0, 255.0) as u8;
            px[i + 3] = 255;
        }
    }
    // O nome do jogo: até 3 linhas, centradas na metade de cima do disco.
    let text = crate::shelf::handwritten_title(title);
    let cell = get_raster_width(FontWeight::Bold, RasterHeight::Size32);
    let cols = ((r * 1.25) as usize / cell).max(6);
    let lines = crate::shelf::wrap_lines(&text, cols, 3);
    let line_h = 36i32;
    let top = (c - r * 0.62) as i32;
    for (li, line) in lines.iter().enumerate() {
        let w = (line.chars().count() * cell) as i32;
        let mut pen = c as i32 - w / 2;
        let y0 = top + li as i32 * line_h;
        for ch in line.chars() {
            if let Some(g) = get_raster(ch, FontWeight::Bold, RasterHeight::Size32) {
                for (gy, row) in g.raster().iter().enumerate() {
                    for (gx, &v) in row.iter().enumerate() {
                        let (x, y) = (pen + gx as i32, y0 + gy as i32);
                        if x < 0 || y < 0 || x >= n as i32 || y >= n as i32 {
                            continue;
                        }
                        let i = (y as usize * n + x as usize) * 4;
                        if px[i + 3] == 0 {
                            continue;
                        }
                        let a = v as u32;
                        for (k, ink) in [24u32, 26, 40].into_iter().enumerate() {
                            let o = px[i + k] as u32;
                            px[i + k] = ((o * (255 - a) + ink * a) / 255) as u8;
                        }
                    }
                }
            }
            pen += cell as i32;
        }
    }
    (SIZE, SIZE, px)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_is_round_with_a_hole_and_has_ink() {
        let (w, h, px) = generic_disc("Tekken 3 (USA)");
        assert_eq!((w, h), (SIZE, SIZE));
        let a = |x: usize, y: usize| px[(y * SIZE as usize + x) * 4 + 3];
        assert_eq!(a(0, 0), 0, "canto transparente");
        assert_eq!(a(256, 256), 0, "furo central");
        assert_eq!(a(256, 100), 255, "área do disco opaca");
        // Há pixels escuros (a tinta do título) na metade de cima.
        let dark = (60..200)
            .flat_map(|y| (100..400).map(move |x| (x, y)))
            .any(|(x, y)| a(x, y) == 255 && px[(y * SIZE as usize + x) * 4] < 80);
        assert!(dark);
    }
}
