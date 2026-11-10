//! Um rasterizador de software com a cara de um console de 32 bits: quadro
//! pequeno ampliado sem filtro, cor de 15 bits (5 por canal), degradês com a
//! matriz de dither 4x4 da GPU do PlayStation, fonte bitmap sem suavização
//! com sombra seca e coordenadas sempre inteiras. O Cheater Fish desenha
//! nele para as telas parecerem de uma ROM de PS1, não de uma interface
//! moderna; o resultado sobe como imagem viva para o tubo.

use noto_sans_mono_bitmap::{get_raster, get_raster_width, FontWeight, RasterHeight};

pub type Rgb = (u8, u8, u8);

/// A matriz de dither ordenado 4x4 que a GPU do PS1 soma aos degradês
/// antes de cortar para 5 bits por canal.
const DITHER: [[i32; 4]; 4] = [
    [-4, 0, -3, 1],
    [2, -2, 3, -1],
    [-3, 1, -4, 0],
    [3, -1, 2, -2],
];

/// Corta um canal de 8 bits para 5 bits e expande de volta (os 3 bits de
/// baixo repetem os de cima), como o framebuffer de 15 bits do console.
pub fn q15(v: i32) -> u8 {
    let q = (v.clamp(0, 255) >> 3) as u8;
    (q << 3) | (q >> 2)
}

fn quant(c: Rgb) -> Rgb {
    (q15(c.0 as i32), q15(c.1 as i32), q15(c.2 as i32))
}

/// Os tamanhos da fonte bitmap.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Font {
    /// 8x16 — o texto das linhas.
    Small,
    /// 32 px — os títulos.
    Big,
}

impl Font {
    fn height(self) -> RasterHeight {
        match self {
            Font::Small => RasterHeight::Size16,
            Font::Big => RasterHeight::Size32,
        }
    }

    /// Largura de uma célula (a fonte é monoespaçada).
    pub fn cell(self) -> i32 {
        get_raster_width(FontWeight::Bold, self.height()) as i32
    }

    /// Altura de uma linha de texto.
    pub fn line(self) -> i32 {
        self.height().val() as i32
    }
}

/// O quadro: `w * h` pixels RGBA (alpha sempre 255).
pub struct Gpu {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u8>,
}

impl Gpu {
    pub fn new(w: i32, h: i32) -> Self {
        Gpu {
            w,
            h,
            px: vec![255; (w * h * 4) as usize],
        }
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = ((y * self.w + x) * 4) as usize;
        self.px[i] = c.0;
        self.px[i + 1] = c.1;
        self.px[i + 2] = c.2;
    }

    /// Um retângulo de cor chapada (cortada para 15 bits).
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        let c = quant(c);
        for yy in y.max(0)..(y + h).min(self.h) {
            for xx in x.max(0)..(x + w).min(self.w) {
                self.put(xx, yy, c);
            }
        }
    }

    /// Um degradê vertical (Gouraud) com o dither do PS1.
    pub fn grad(&mut self, x: i32, y: i32, w: i32, h: i32, top: Rgb, bot: Rgb) {
        let hh = (h - 1).max(1);
        for yy in y.max(0)..(y + h).min(self.h) {
            let t = yy - y;
            let lerp = |a: u8, b: u8| a as i32 + (b as i32 - a as i32) * t / hh;
            let base = (lerp(top.0, bot.0), lerp(top.1, bot.1), lerp(top.2, bot.2));
            for xx in x.max(0)..(x + w).min(self.w) {
                let d = DITHER[(yy & 3) as usize][(xx & 3) as usize];
                self.put(xx, yy, (q15(base.0 + d), q15(base.1 + d), q15(base.2 + d)));
            }
        }
    }

    /// A moldura chanfrada de prata: o degradê claro->escuro da face e os
    /// filetes de luz (cima/esquerda) e sombra (baixo/direita). Com `inner`,
    /// o miolo é preenchido por baixo da moldura.
    pub fn bevel(&mut self, x: i32, y: i32, w: i32, h: i32, inner: Option<(Rgb, Rgb)>) {
        self.grad(x, y, w, h, (214, 220, 244), (116, 124, 168));
        self.rect(x, y, w, 2, (240, 244, 255));
        self.rect(x, y, 2, h, (240, 244, 255));
        self.rect(x, y + h - 2, w, 2, (60, 66, 110));
        self.rect(x + w - 2, y, 2, h, (60, 66, 110));
        if let Some((a, b)) = inner {
            self.grad(x + 6, y + 5, w - 12, h - 10, a, b);
            // O filete afundado do miolo.
            self.rect(x + 6, y + 5, w - 12, 1, (10, 14, 44));
            self.rect(x + 6, y + 5, 1, h - 10, (10, 14, 44));
        }
    }

    /// Largura de `s` em pixels.
    pub fn text_w(font: Font, s: &str) -> i32 {
        s.chars().count() as i32 * font.cell()
    }

    /// Desenha `s` com o canto superior esquerdo em `(x, y)`, sem
    /// suavização (limiar duro) e com a sombra seca de 1 px do console.
    /// Devolve a largura desenhada.
    pub fn text(&mut self, x: i32, y: i32, font: Font, c: Rgb, s: &str) -> i32 {
        let cell = font.cell();
        let mut pen = x;
        for ch in s.chars() {
            if let Some(g) = get_raster(ch, FontWeight::Bold, font.height()) {
                for (gy, row) in g.raster().iter().enumerate() {
                    for (gx, &v) in row.iter().enumerate() {
                        if v >= 110 {
                            let (px, py) = (pen + gx as i32, y + gy as i32);
                            self.put(px + 1, py + 1, (6, 8, 30));
                            self.put(px, py, quant(c));
                        }
                    }
                }
            }
            pen += cell;
        }
        pen - x
    }

    /// `text` centrado em `cx`.
    pub fn text_center(&mut self, cx: i32, y: i32, font: Font, c: Rgb, s: &str) {
        let w = Self::text_w(font, s);
        self.text(cx - w / 2, y, font, c, s);
    }

    /// Um sprite de `sw * sh` pixels (linha a linha) ampliado `scale` vezes,
    /// pixels vizinhos (nada de filtro).
    pub fn sprite(&mut self, sprite: &[Option<Rgb>], sw: i32, sh: i32, x: i32, y: i32, scale: i32) {
        for py in 0..sh {
            for px in 0..sw {
                if let Some(c) = sprite[(py * sw + px) as usize] {
                    self.rect(x + px * scale, y + py * scale, scale, scale, c);
                }
            }
        }
    }

    /// O dither ordenado como textura: o fundo "água" do menu, com o mapa
    /// de textura afim do PS1 (sem correção de perspectiva), que faz as
    /// listras ondularem. `t` anima devagar.
    pub fn water(&mut self, t: u32) {
        let tf = t as f32 * 0.04;
        for y in 0..self.h {
            // O degradê base: azul escuro no alto, mais vivo embaixo.
            let g = y * 255 / self.h.max(1);
            for x in 0..self.w {
                // Coordenadas de textura que ondulam (o "wobble" afim).
                let u = x as f32 + (y as f32 * 0.07 + tf).sin() * 5.0;
                let v = y as f32 + (x as f32 * 0.05 + tf * 0.7).cos() * 4.0;
                let streak = (((u * 0.22).sin() + (v * 0.5 + u * 0.08).sin()) * 0.5 + 1.0) * 0.5;
                let lift = (streak * 34.0) as i32;
                let d = DITHER[(y & 3) as usize][(x & 3) as usize];
                let r = 14 + g / 18 + lift / 2 + d;
                let gr = 20 + g / 14 + lift / 2 + d;
                let b = 84 + g / 6 + lift + d;
                self.put(x, y, (q15(r), q15(gr), q15(b)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_are_cut_to_15_bits() {
        // 5 bits por canal: só 32 valores distintos, e o branco continua 255.
        assert_eq!(q15(255), 255);
        assert_eq!(q15(0), 0);
        let distinct: std::collections::BTreeSet<u8> = (0..=255).map(q15).collect();
        assert_eq!(distinct.len(), 32);
    }

    #[test]
    fn gradients_dither_and_stay_in_range() {
        let mut g = Gpu::new(16, 16);
        g.grad(0, 0, 16, 16, (0, 0, 0), (255, 255, 255));
        // Na meia-altura há pixels vizinhos de valores diferentes (o dither).
        let row =
            |y: i32| -> Vec<u8> { (0..16).map(|x| g.px[((y * 16 + x) * 4) as usize]).collect() };
        let mid = row(8);
        assert!(mid.iter().any(|v| *v != mid[0]), "dither mistura valores");
        assert_eq!(row(15)[0], 255);
    }

    #[test]
    fn text_is_hard_edged_and_has_a_drop_shadow() {
        let mut g = Gpu::new(40, 24);
        g.rect(0, 0, 40, 24, (200, 200, 200));
        let w = g.text(2, 2, Font::Small, (255, 255, 255), "A");
        assert_eq!(w, Font::Small.cell());
        // Só há o fundo, o branco do texto e a sombra escura: nada de meio-tom.
        let mut kinds = std::collections::BTreeSet::new();
        for p in g.px.as_chunks::<4>().0 {
            kinds.insert((p[0], p[1], p[2]));
        }
        assert!(kinds.len() <= 3, "{kinds:?}");
    }
}
