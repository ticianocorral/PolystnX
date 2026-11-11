//! O vídeo composto (cabo AV amarelo/branco/vermelho): luminância e croma
//! viajam no mesmo fio, e a TV separa os dois com banda de croma bem mais
//! estreita que a de luma. O que se vê (e o que este filtro reproduz, só na
//! horizontal, como no sinal de verdade):
//!
//! - a **cor borra** para os lados — o croma é filtrado por uma janela larga
//!   e chega um pouco atrasado, sangrando para a direita das bordas;
//! - a **luma suaviza** de leve — o detalhe fino perde o fio;
//! - a cor perde um pouco de pureza (saturação ~92%).
//!
//! Sem neve, fantasma ou chiado: isso é RF (antena), não AV.

/// Aplica o filtro a um quadro BGRX (`w * h * 4` bytes), no lugar.
pub(super) fn apply(buf: &mut [u8], w: usize, h: usize) {
    if w < 8 || buf.len() < w * h * 4 {
        return;
    }
    // Janela do croma: ~1/110 da largura (3 px em 320, 6 em 640), no mínimo 2.
    let radius = (w / 110).max(2);
    let delay = (radius / 2).max(1);
    let mut luma = vec![0f32; w];
    let mut ci = vec![0f32; w];
    let mut cq = vec![0f32; w];
    let mut bi = vec![0f32; w];
    let mut bq = vec![0f32; w];
    for row in buf.chunks_exact_mut(w * 4).take(h) {
        for (x, p) in row.as_chunks::<4>().0.iter().enumerate() {
            let (b, g, r) = (p[0] as f32, p[1] as f32, p[2] as f32);
            luma[x] = 0.299 * r + 0.587 * g + 0.114 * b;
            ci[x] = 0.596 * r - 0.274 * g - 0.322 * b;
            cq[x] = 0.211 * r - 0.523 * g + 0.312 * b;
        }
        box_blur(&ci, &mut bi, radius);
        box_blur(&cq, &mut bq, radius);
        for (x, p) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            // Luma: [1 2 1] / 4 nas bordas do quadro repete o vizinho.
            let l = (luma[x.saturating_sub(1)] + 2.0 * luma[x] + luma[(x + 1).min(w - 1)]) / 4.0;
            // Croma atrasado: o que chega agora saiu `delay` pixels atrás.
            let s = x.saturating_sub(delay);
            let (i, q) = (bi[s] * 0.92, bq[s] * 0.92);
            let r = l + 0.956 * i + 0.621 * q;
            let g = l - 0.272 * i - 0.647 * q;
            let b = l - 1.106 * i + 1.703 * q;
            p[0] = b.round().clamp(0.0, 255.0) as u8;
            p[1] = g.round().clamp(0.0, 255.0) as u8;
            p[2] = r.round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// Média móvel centrada de largura `2 * r + 1` (bordas repetem o vizinho).
fn box_blur(src: &[f32], dst: &mut [f32], r: usize) {
    let n = src.len();
    let at = |i: isize| src[i.clamp(0, n as isize - 1) as usize];
    let r = r as isize;
    let mut sum: f32 = (-r..=r).map(at).sum();
    let k = (2 * r + 1) as f32;
    for (x, d) in dst.iter_mut().enumerate() {
        *d = sum / k;
        let x = x as isize;
        sum += at(x + r + 1) - at(x - r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: usize, h: usize, f: impl Fn(usize) -> [u8; 3]) -> Vec<u8> {
        let mut v = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let [r, g, b] = f(x);
                v.extend_from_slice(&[b, g, r, 255]);
            }
        }
        v
    }

    #[test]
    fn flat_gray_stays_gray() {
        let mut b = frame(32, 2, |_| [120, 120, 120]);
        apply(&mut b, 32, 2);
        for p in b.as_chunks::<4>().0 {
            assert!((p[0] as i32 - 120).abs() <= 1 && (p[1] as i32 - 120).abs() <= 1);
        }
    }

    #[test]
    fn color_bleeds_past_a_hard_edge() {
        // Metade esquerda vermelha, direita preta: logo depois da borda o
        // vermelho ainda sangra (croma atrasado e borrado).
        let mut b = frame(64, 1, |x| if x < 32 { [255, 0, 0] } else { [0, 0, 0] });
        apply(&mut b, 64, 1);
        let px = |x: usize| (b[x * 4 + 2] as i32, b[x * 4 + 1] as i32);
        let (r, g) = px(33);
        assert!(r > g + 4, "vermelho sangra para a direita: r={r} g={g}");
    }

    #[test]
    fn tiny_or_short_buffers_do_not_panic() {
        apply(&mut [0u8; 16], 2, 2);
        apply(&mut [0u8; 4], 64, 64);
    }
}
