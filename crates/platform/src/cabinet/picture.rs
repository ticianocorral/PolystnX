//! Os ajustes de imagem do menu da TV (brilho, contraste, cor, nitidez)
//! aplicados ao quadro do jogo. Só roda com algum ajuste fora do neutro
//! (50): no padrão o quadro segue direto para a textura, sem custo.

/// Os ajustes de imagem do menu OSD — cada um de 0 a 100, 50 = neutro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    pub brilho: u8,
    pub contraste: u8,
    pub cor: u8,
    pub nitidez: u8,
}

impl Default for Picture {
    fn default() -> Self {
        Self {
            brilho: 50,
            contraste: 50,
            cor: 50,
            nitidez: 50,
        }
    }
}

impl Picture {
    pub(super) const ITEMS: usize = 4;

    pub(super) fn get(&self, item: usize) -> u8 {
        [self.brilho, self.contraste, self.cor, self.nitidez][item]
    }

    pub(super) fn set(&mut self, item: usize, v: u8) {
        let v = v.min(100);
        match item {
            0 => self.brilho = v,
            1 => self.contraste = v,
            2 => self.cor = v,
            _ => self.nitidez = v,
        }
    }

    pub(super) fn label(item: usize) -> &'static str {
        match item {
            0 => "BRILHO",
            1 => "CONTRASTE",
            2 => "COR",
            _ => "NITIDEZ",
        }
    }

    /// Tudo no neutro: o quadro não precisa de processamento.
    pub fn is_neutral(&self) -> bool {
        *self == Picture::default()
    }
}

/// Aplica `p` a um quadro XRGB8888 (B, G, R, X em memória), de `pitch` bytes
/// por linha, em `out` (`w * h * 4` bytes, sem folga de linha). `tmp` é o
/// rascunho da nitidez, reaproveitado entre quadros.
pub(super) fn apply(
    src: &[u8],
    w: usize,
    h: usize,
    pitch: usize,
    p: Picture,
    out: &mut Vec<u8>,
    tmp: &mut Vec<u8>,
) {
    out.clear();
    out.resize(w * h * 4, 255);
    if src.len() < pitch * h.saturating_sub(1) + w * 4 {
        return; // buffer curto demais: devolve o quadro preto, sem pânico
    }
    // Brilho: soma ±60; contraste: gira em torno do cinza médio (0,4x..1,6x).
    let add = (p.brilho as f32 - 50.0) * 1.2;
    let gain = 1.0 + (p.contraste as f32 - 50.0) / 50.0 * 0.6;
    let mut lut = [0u8; 256];
    for (i, v) in lut.iter_mut().enumerate() {
        *v = ((i as f32 - 128.0) * gain + 128.0 + add)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    // Cor: 0 = preto e branco, 50 = original, 100 = o dobro da saturação.
    let sat = p.cor as f32 / 50.0;
    for y in 0..h {
        let row = &src[y * pitch..y * pitch + w * 4];
        let dst = &mut out[y * w * 4..(y + 1) * w * 4];
        for (s, d) in row
            .as_chunks::<4>()
            .0
            .iter()
            .zip(dst.as_chunks_mut::<4>().0.iter_mut())
        {
            let (b, g, r) = (s[0] as f32, s[1] as f32, s[2] as f32);
            let l = 0.299 * r + 0.587 * g + 0.114 * b;
            let f = |c: f32| lut[(l + (c - l) * sat).round().clamp(0.0, 255.0) as usize];
            d[0] = f(b);
            d[1] = f(g);
            d[2] = f(r);
        }
    }
    // Nitidez: > 50 acentua as bordas (cruz de 4 vizinhos), < 50 suaviza.
    if p.nitidez != 50 && w > 2 && h > 2 {
        let amt = (p.nitidez as f32 - 50.0) / 50.0; // -1..1
        tmp.clear();
        tmp.extend_from_slice(out);
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    let ctr = tmp[i + c] as f32;
                    let nb = (tmp[i - 4 + c] as f32
                        + tmp[i + 4 + c] as f32
                        + tmp[i - w * 4 + c] as f32
                        + tmp[i + w * 4 + c] as f32)
                        / 4.0;
                    out[i + c] = (ctr + (ctr - nb) * amt * 1.5).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(b: u8, g: u8, r: u8) -> Vec<u8> {
        [b, g, r, 255].repeat(4 * 4)
    }

    #[test]
    fn neutral_picture_is_flagged_and_identity_in_the_lut() {
        assert!(Picture::default().is_neutral());
        let src = px(10, 120, 200);
        let (mut out, mut tmp) = (Vec::new(), Vec::new());
        apply(&src, 4, 4, 16, Picture::default(), &mut out, &mut tmp);
        assert_eq!(&out[..3], &[10, 120, 200]);
    }

    #[test]
    fn brightness_and_color_move_the_pixel() {
        let src = px(100, 100, 100);
        let (mut out, mut tmp) = (Vec::new(), Vec::new());
        let bright = Picture {
            brilho: 100,
            ..Picture::default()
        };
        apply(&src, 4, 4, 16, bright, &mut out, &mut tmp);
        assert!(out[0] > 100);
        let gray = Picture {
            cor: 0,
            ..Picture::default()
        };
        let src = px(0, 0, 255);
        apply(&src, 4, 4, 16, gray, &mut out, &mut tmp);
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2], "cor 0 = preto e branco");
    }

    #[test]
    fn short_buffer_does_not_panic() {
        let (mut out, mut tmp) = (Vec::new(), Vec::new());
        apply(&[0u8; 8], 4, 4, 16, Picture::default(), &mut out, &mut tmp);
        assert_eq!(out.len(), 4 * 4 * 4);
    }
}
