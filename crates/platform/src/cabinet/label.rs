//! As letras gravadas nos botões redondos do console (POWER, RESET, OPEN):
//! uma fonte vetorial sem serifa (Inter Bold, OFL) desenhada com
//! antialias numa textura pequena, guardada em cache por (texto, tamanho,
//! cor) — o PS1 tem as palavras impressas em sans fino, e a fonte de pixels
//! do resto da interface não chega perto.

use super::*;
use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use std::cell::RefCell;
use std::collections::HashMap;

static INTER: &[u8] = include_bytes!("../../assets/Inter-Bold.otf");

type Key = (String, u32, [u8; 3]);

thread_local! {
    /// As etiquetas prontas. O canvas do SDL vive na thread da UI, então o
    /// cache também.
    static LABELS: RefCell<HashMap<Key, (Texture, u32, u32)>> = RefCell::new(HashMap::new());
}

/// Tamanho do texto em pixels (altura da fonte) que cabe em `max_w` de
/// largura, no máximo `max_px`.
pub(super) fn fit_px(label: &str, max_px: f32, max_w: f32) -> f32 {
    let Ok(font) = FontRef::try_from_slice(INTER) else {
        return max_px;
    };
    let w = text_width(&font, label, max_px);
    if w <= max_w {
        max_px
    } else {
        (max_px * max_w / w).max(6.0)
    }
}

fn text_width(font: &FontRef, label: &str, px: f32) -> f32 {
    let sf = font.as_scaled(PxScale::from(px));
    let track = px * 0.05;
    let mut w = 0.0;
    let mut prev = None;
    for ch in label.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            w += sf.kern(p, id);
        }
        w += sf.h_advance(id) + track;
        prev = Some(id);
    }
    (w - track).max(0.0)
}

/// Onde o texto ancora em `x`.
#[derive(Clone, Copy)]
pub(super) enum Anchor {
    Center,
    Left,
}

/// Desenha `label` centrado em `(cx, cy)`, com `px` de altura de fonte.
pub(super) fn draw_label(
    canvas: &mut WindowCanvas,
    label: &str,
    cx: i32,
    cy: i32,
    px: f32,
    color: (u8, u8, u8),
) {
    draw_text_vec(
        canvas,
        label,
        (cx, cy),
        Anchor::Center,
        px,
        color,
        (i32::MIN, i32::MAX),
    );
}

/// O texto vetorial geral das peças do console: `anchor` diz se `x` é o
/// centro ou a esquerda; `band` é o intervalo de y visível (o card que
/// desliza no slot corta o que está fora dele — sem clip do renderizador).
pub(super) fn draw_text_vec(
    canvas: &mut WindowCanvas,
    label: &str,
    (x, cy): (i32, i32),
    anchor: Anchor,
    px: f32,
    color: (u8, u8, u8),
    (ymin, ymax): (i32, i32),
) {
    let key: Key = (
        label.to_string(),
        (px * 10.0).round() as u32,
        [color.0, color.1, color.2],
    );
    LABELS.with_borrow_mut(|cache| {
        if !cache.contains_key(&key) {
            let Some(built) = build(canvas, label, px, color) else {
                return;
            };
            if cache.len() > 64 {
                cache.clear();
            }
            cache.insert(key.clone(), built);
        }
        if let Some((tex, w, h)) = cache.get(&key) {
            let left = match anchor {
                Anchor::Center => x - *w as i32 / 2,
                Anchor::Left => x,
            };
            let top = cy - *h as i32 / 2;
            // Só as linhas dentro da banda visível.
            let y0 = top.max(ymin);
            let y1 = (top + *h as i32).min(ymax);
            if y1 > y0 {
                let src = Rect::new(0, y0 - top, *w, (y1 - y0) as u32);
                let dst = Rect::new(left, y0, *w, (y1 - y0) as u32);
                let _ = canvas.copy(tex, src, dst);
            }
        }
    });
}

fn build(
    canvas: &WindowCanvas,
    label: &str,
    px: f32,
    color: (u8, u8, u8),
) -> Option<(Texture, u32, u32)> {
    let font = FontRef::try_from_slice(INTER).ok()?;
    let scale = PxScale::from(px);
    let sf = font.as_scaled(scale);
    let track = px * 0.05;
    let w = text_width(&font, label, px).ceil() as u32 + 4;
    let h = (sf.ascent() - sf.descent()).ceil() as u32 + 4;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut pen = 2.0f32;
    let base = 2.0 + sf.ascent();
    let mut prev = None;
    for ch in label.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            pen += sf.kern(p, id);
        }
        let glyph = id.with_scale_and_position(scale, point(pen, base));
        if let Some(outline) = font.outline_glyph(glyph) {
            let b = outline.px_bounds();
            outline.draw(|x, y, cov| {
                let (gx, gy) = (b.min.x as i32 + x as i32, b.min.y as i32 + y as i32);
                if gx < 0 || gy < 0 || gx as u32 >= w || gy as u32 >= h {
                    return;
                }
                let i = ((gy as u32 * w + gx as u32) * 4) as usize;
                let a = (cov * 255.0).round() as u32;
                // Cobertura acumulada de glifos que se tocam: pega a maior.
                if a > rgba[i + 3] as u32 {
                    rgba[i] = color.0;
                    rgba[i + 1] = color.1;
                    rgba[i + 2] = color.2;
                    rgba[i + 3] = a as u8;
                }
            });
        }
        pen += sf.h_advance(id) + track;
        prev = Some(id);
    }
    let mut tex = canvas.create_texture_static(SdlFormat::RGBA32, w, h).ok()?;
    tex.set_blend_mode(BlendMode::Blend);
    tex.set_scale_mode(SdlScaleMode::Nearest);
    tex.update(None, &rgba, (w * 4) as usize).ok()?;
    Some((tex, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_labels_shrink_to_fit_and_short_ones_keep_their_size() {
        assert_eq!(fit_px("OK", 20.0, 200.0), 20.0);
        let px = fit_px("RESET", 20.0, 30.0);
        assert!((6.0..20.0).contains(&px));
        let font = FontRef::try_from_slice(INTER).unwrap();
        assert!(text_width(&font, "RESET", px) <= 30.5);
    }
}
