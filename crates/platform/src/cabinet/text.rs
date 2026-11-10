//! Gabinete: text (extraído de `cabinet.rs`, sem mudança de comportamento).

use super::*;

/// Map a char to its column in the font atlas; anything outside the covered
/// range (or with no glyph in the font) falls back to `?`.
pub(super) fn glyph_index(ch: char) -> u32 {
    let c = atlas_char(ch) as u32;
    if (GLYPH_FIRST..=GLYPH_LAST).contains(&c) {
        c - GLYPH_FIRST
    } else {
        '?' as u32 - GLYPH_FIRST
    }
}

/// Copies fixas e strings dinâmicas (changelog do GitHub, títulos de jogo,
/// status) trazem pontuação tipográfica e setas fora do atlas — cada uma vira
/// o glifo ASCII mais próximo em vez do `?` (plan revision: "corrigir copy
/// com ? que estao aparecendo no app"). Só mapeia o que está FORA de
/// 0x20..=0xFF: o que o Latin-1 já cobre usa o glifo de verdade.
pub(super) fn atlas_char(ch: char) -> char {
    match ch {
        '–' | '—' | '•' => '-',
        '…' => '.',
        '‹' | '←' => '<',
        '›' | '→' => '>',
        '↑' => '^',
        '↓' => 'v',
        '‘' | '’' => '\'',
        '“' | '”' => '"',
        '\u{3000}' => ' ',
        // full-width (Ａ→A, １→1): espelho fixo do ASCII, 0xFEE0 abaixo.
        c if (0xFF01..=0xFF5E).contains(&(c as u32)) => {
            char::from_u32(c as u32 - 0xFEE0).unwrap_or('?')
        }
        c => c,
    }
}

pub(super) fn truncate_to_cols(s: &str, max_cols: usize) -> String {
    if s.chars().count() <= max_cols {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_cols.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// The idle screen's console block: o botão "Estante de games" no topo e o
/// console abaixo, igual ao da tela de jogo. Returns the button rect as the
/// clickable area (`PanelButton::Insert`). (O botão "Ligar sem disco" saiu:
/// o boot ejetado no Rearmed travava o app — main parada no recv do primeiro
/// quadro — e a opção virou armadilha.)
#[allow(clippy::too_many_arguments)]
/// Scale + colour for one of the absolute-coordinate text helpers below —
/// bundled so those functions stay under clippy's argument-count limit.
/// `scale` é fractional (0.7 para rótulos pequenos, 2.0 para títulos).
#[derive(Clone, Copy)]
pub(super) struct TextStyle {
    pub(super) scale: f32,
    pub(super) color: (u8, u8, u8),
    /// Negrito da fonte mono: cada glifo desenhado duas vezes, 1px ao
    /// lado — duplo impacto, o jeito clássico de engrossar bitmap.
    pub(super) bold: bool,
}

impl TextStyle {
    pub(super) fn new(scale: f32, color: (u8, u8, u8)) -> Self {
        Self {
            scale,
            color,
            bold: false,
        }
    }
    pub(super) fn bold(scale: f32, color: (u8, u8, u8)) -> Self {
        Self {
            scale,
            color,
            bold: true,
        }
    }
}

/// A single line of text at absolute window coordinates, truncated to
/// `max_chars` (`usize::MAX` for no truncation) — for cabinet furniture that
/// isn't inside a `Screen`.
pub(super) fn draw_text_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    style: TextStyle,
    s: &str,
    max_chars: usize,
) {
    let (r, g, b) = style.color;
    font.set_color_mod(r, g, b);
    let dw = (GLYPH_W as f32 * style.scale).round() as u32;
    let dh = (GLYPH_H as f32 * style.scale).round() as u32;
    // Avanço inteiro (dw, não a célula fracionária): em escala fracionária
    // (o RESET a 0.7×) o acúmulo fracionário arredondava cada letra para um
    // lado e o espaçamento ficava desigual — "RE SET".
    let mut pen = x as f32;
    for ch in s.chars().take(max_chars) {
        let idx = glyph_index(ch);
        if ch != ' ' {
            let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
            let strikes = if style.bold { 2 } else { 1 };
            for strike in 0..strikes {
                let dst = Rect::new(
                    pen.round() as i32 + (strike as f32 * style.scale).round() as i32,
                    y,
                    dw,
                    dh,
                );
                let _ = canvas.copy(font, src, dst);
            }
        }
        pen += dw as f32;
    }
}

/// [`draw_text_absolute`] com corte na banda visível do card (glifos que
/// cruzam as linhas saem cortados — o card deslizando sob a carcaça e
/// nascendo pelo pé da face).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_text_absolute_band(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    style: TextStyle,
    s: &str,
    max_chars: usize,
    ymin: i32,
    ymax: i32,
) {
    let (r, g, b) = style.color;
    font.set_color_mod(r, g, b);
    let dw = (GLYPH_W as f32 * style.scale).round() as u32;
    let dh = (GLYPH_H as f32 * style.scale).round() as u32;
    let mut pen = x as f32;
    for ch in s.chars().take(max_chars) {
        let idx = glyph_index(ch);
        if ch != ' ' {
            let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
            let dst = Rect::new(pen.round() as i32, y, dw, dh);
            if let Some((src, dst)) = blit_band(src, dst, ymin, ymax) {
                let _ = canvas.copy(font, src, dst);
            }
        }
        pen += dw as f32;
    }
}

/// [`draw_text_absolute_band`] na dupla passada do bold (a sombra de 1 px).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_text_bold_band(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    style: TextStyle,
    s: &str,
    max_chars: usize,
    ymin: i32,
    ymax: i32,
) {
    draw_text_absolute_band(canvas, font, x, y, style, s, max_chars, ymin, ymax);
    draw_text_absolute_band(canvas, font, x + 1, y, style, s, max_chars, ymin, ymax);
}

/// Texto em negrito: o traço da fonte é fino (1 px), então o bold é o
/// desenho duas vezes com 1 px de deslocamento — engorda o traço sem
/// borrar (o mesmo truque dos créditos de fliperama).
pub(super) fn draw_text_bold(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    style: TextStyle,
    s: &str,
    max_chars: usize,
) {
    draw_text_absolute(canvas, font, x, y, style, s, max_chars);
    draw_text_absolute(canvas, font, x + 1, y, style, s, max_chars);
}

/// Word-wrapped text at absolute window coordinates, mirroring
/// `Screen::text_wrapped`'s layout. Returns the y past the last line.
pub(super) fn draw_text_wrapped_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    max_w: u32,
    style: TextStyle,
    s: &str,
) -> i32 {
    let cols = ((max_w as f32) / (GLYPH_W as f32 * style.scale)).max(1.0) as usize;
    let row = (GLYPH_H as f32 * style.scale) as i32;
    let mut line = String::new();
    let mut line_len = 0usize;
    let mut cy = y;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if !line.is_empty() && line_len + 1 + word_len > cols {
            draw_text_absolute(canvas, font, x, cy, style, &line, usize::MAX);
            cy += row + 2;
            line.clear();
            line_len = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_len += 1;
        }
        line.push_str(word);
        line_len += word_len;
        // `.chars()` (not `split_at`, byte-indexed): a wrap point that fell
        // mid-character would panic on any accented/multi-byte word.
        while line_len > cols {
            let head: String = line.chars().take(cols).collect();
            let tail: String = line.chars().skip(cols).collect();
            draw_text_absolute(canvas, font, x, cy, style, &head, usize::MAX);
            cy += row + 2;
            line = tail;
            line_len = line.chars().count();
        }
    }
    if !line.is_empty() {
        draw_text_absolute(canvas, font, x, cy, style, &line, usize::MAX);
        cy += row + 2;
    }
    cy
}

/// Texto de painel centralizado na largura útil — o que estourar cai no
/// wrap alinhado à esquerda de sempre. Devolve o y logo abaixo do texto.
pub(super) fn draw_text_centered_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    inner_w: u32,
    style: TextStyle,
    s: &str,
) -> i32 {
    let per = GLYPH_W as i32 + if style.bold { 1 } else { 0 };
    let tw = s.chars().count() as i32 * per - if style.bold { 1 } else { 0 };
    if tw <= inner_w as i32 {
        draw_text_absolute(
            canvas,
            font,
            x + (inner_w as i32 - tw) / 2,
            y,
            style,
            s,
            usize::MAX,
        );
        y + GLYPH_H as i32
    } else {
        draw_text_wrapped_absolute(canvas, font, x, y, inner_w, style, s)
    }
}

/// Truncate `text` to at most `max_chars`, replacing the last one with an
/// ellipsis when it doesn't fit — plain `...` rather than `…` since the
/// font atlas only covers Basic Latin through Latin-1 Supplement (see
/// `GLYPH_FIRST`/`GLYPH_LAST`), which that single-character ellipsis falls
/// outside of. A no-op (returns `text` unchanged, borrowed) when it
/// already fits.
pub(super) fn clip_label(text: &str, max_chars: usize) -> Cow<'_, str> {
    if text.chars().count() <= max_chars {
        return Cow::Borrowed(text);
    }
    if max_chars <= 3 {
        // No room for both content and "...": just hard-crop.
        return Cow::Owned(text.chars().take(max_chars).collect());
    }
    let keep: String = text.chars().take(max_chars - 3).collect();
    Cow::Owned(format!("{keep}..."))
}

/// Line-count math shared by [`Cabinet::text_wrapped`]'s layout and
/// [`Cabinet::wrapped_height`]. Mirrors the wrap loop: greedy word packing into
/// `cols` chars, hard-splitting any word longer than a line.
pub(super) fn wrapped_height(max_w: u32, scale: u32, s: &str) -> i32 {
    let cols = (max_w / (GLYPH_W * scale)).max(1) as usize;
    let row = (GLYPH_H * scale) as i32;
    let mut lines = 0i32;
    let mut len = 0usize;
    let mut open = false;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if open && len + 1 + word_len > cols {
            lines += 1;
            len = 0;
            open = false;
        }
        if open {
            len += 1;
        }
        len += word_len;
        open = true;
        while len > cols {
            lines += 1;
            len -= cols;
        }
    }
    if open {
        lines += 1;
    }
    lines * (row + 2)
}

/// Rasterize the covered Unicode range once into one wide texture strip —
/// each cell holds one anti-aliased glyph (alpha = the crate's per-pixel
/// intensity, RGB left white so every draw call tints it via
/// `set_color_mod`), left-aligned in its `GLYPH_W`-wide cell exactly like
/// the atlas the old bitmap font used, just with grayscale edges instead of
/// hard 1-bit ones.
pub(super) fn build_font_atlas(canvas: &mut WindowCanvas) -> Result<Texture, PlatformError> {
    let w = GLYPH_COLS * GLYPH_W;
    let h = GLYPH_H;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for i in 0..GLYPH_COLS {
        let Some(ch) = char::from_u32(GLYPH_FIRST + i) else {
            continue;
        };
        let Some(glyph) = get_raster(ch, FONT_WEIGHT, FONT_HEIGHT) else {
            continue;
        };
        for (row, line) in glyph.raster().iter().enumerate().take(h as usize) {
            for (col, &v) in line.iter().enumerate().take(GLYPH_W as usize) {
                if v == 0 {
                    continue;
                }
                let px = i * GLYPH_W + col as u32;
                let o = ((row as u32 * w + px) * 4) as usize;
                rgba[o..o + 4].copy_from_slice(&[255, 255, 255, v]);
            }
        }
    }
    let mut tex = canvas
        .create_texture_static(SdlFormat::RGBA32, w, h)
        .map_err(|e| PlatformError::Sdl(e.to_string()))?;
    tex.update(None, &rgba, (w * 4) as usize)
        .map_err(|e| PlatformError::Sdl(e.to_string()))?;
    tex.set_blend_mode(BlendMode::Blend);
    tex.set_scale_mode(SdlScaleMode::Nearest);
    Ok(tex)
}
