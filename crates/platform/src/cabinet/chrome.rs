//! Gabinete: chrome (extraído de `cabinet.rs`, sem mudança de comportamento).

use super::*;

/// The top-left "fechar app" button (plan revision: "criar botao de fechar
/// app no canto superior esquerdo") — a small square with an "X", same
/// visual language as `draw_button`. Fixed distance from the cabinet's own
/// top-left corner (canvas-local coordinates, the same space
/// `window_to_output` maps clicks into), so it sits in the same spot
/// regardless of window size or ultrawide letterboxing. Callers draw the
/// pair last, on top of whatever else the screen drew, on EVERY screen now
/// (plan revision: "mostrar o fechar e minimizar em todas as telas").
/// Returns the rect drawn, for `Cabinet::close_button`/`hit_close_button`.
pub(super) fn draw_close_button(canvas: &mut WindowCanvas, font: &mut Texture) -> Rect {
    const MARGIN: i32 = 14;
    const SIZE: u32 = 32;
    draw_button(
        canvas,
        font,
        Rect::new(MARGIN, MARGIN, SIZE, SIZE),
        "X",
        true,
    )
}

/// The minimize button, sitting right next to `draw_close_button` (plan
/// revision: "colocar botão de minimizar ao lado de fechar") — same square
/// look, a dash for the iconify glyph. Returns the rect drawn, for
/// `Cabinet::minimize_button`/`hit_minimize_button`.
pub(super) fn draw_minimize_button(canvas: &mut WindowCanvas, font: &mut Texture) -> Rect {
    const MARGIN: i32 = 14;
    const SIZE: u32 = 32;
    const GAP: i32 = 8;
    draw_button(
        canvas,
        font,
        Rect::new(MARGIN + SIZE as i32 + GAP, MARGIN, SIZE, SIZE),
        "-",
        true,
    )
}

/// The set's nameplate, printed into the chin left of the tube — part of the
/// cabinet itself, so unlike the panel it's drawn in every context (shelf,
/// game, idle-off) and never disappears. A no-op if the chin is too short to
/// hold it. `label` is `Cabinet::nameplate` (plan revision — `BRAND` plus the
/// app/core version once the app calls `set_nameplate`).
/// The channel banner's green — vivid OSD green, like the phosphor filter a
/// TV applies to its own on-screen display — no PSX é "AV 1": entrada
/// composta (cabos RCA), não RF (plano §6, revisão do usuário).
pub(super) const OSD_GREEN: (u8, u8, u8) = (60, 230, 70);
/// O cinza claro das linhas secundárias do chin (aviso de abertura, OSD de
/// conquistas) — constante nomeada para o teste parear o contrato.
pub(super) const LIGHT_GREY_ROW: (u8, u8, u8) = (235, 235, 225);

/// The channel banner ("AV 1" no PSX: a TV fica na entrada composta; o
/// selo aparece na troca de entrada e na ausência de sinal) —
/// green, top-right of the tube. Drawn flat on the glass, deliberately NOT
/// warped with the picture: an OSD is the TV's own overlay, not part of the
/// signal (same reasoning as `draw_brand`'s chin text).
pub(super) fn draw_ch3_osd(canvas: &mut WindowCanvas, font: &mut Texture, screen: Rect) {
    const LABEL: &str = "AV 1";
    const SCALE: u32 = 3;
    let w = GLYPH_W as i32 * SCALE as i32 * LABEL.chars().count() as i32;
    // Longe do canto: o tubo curvo come a ponta do retângulo da tela.
    let (margin_x, margin_y) = (56, 44);
    let x = screen.right() - margin_x - w;
    let y = screen.top() + margin_y;
    // A 2px dark offset keeps the green readable over bright snow without a
    // full outline pass.
    draw_text_absolute(
        canvas,
        font,
        x + 2,
        y + 2,
        TextStyle::new(SCALE as f32, (12, 14, 12)),
        LABEL,
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(SCALE as f32, OSD_GREEN),
        LABEL,
        usize::MAX,
    );
}

/// O anel do foco do controle em volta de um botão: contorno lavanda de 3 px
/// (a cor fósforo da estante) com um filete escuro por fora, para ler sobre
/// qualquer plástico ou painel.
pub(super) fn draw_focus_ring(canvas: &mut WindowCanvas, r: Rect) {
    canvas.set_blend_mode(BlendMode::Blend);
    // Contorno de `t` px cuja borda externa fica `grow` px fora de `r`.
    let ring = |canvas: &mut WindowCanvas, grow: i32, t: i32, c: Color| {
        canvas.set_draw_color(c);
        let (x, y) = (r.x() - grow, r.y() - grow);
        let (w, h) = (r.width() as i32 + grow * 2, r.height() as i32 + grow * 2);
        for edge in [
            Rect::new(x, y, w as u32, t as u32),
            Rect::new(x, y + h - t, w as u32, t as u32),
            Rect::new(x, y, t as u32, h as u32),
            Rect::new(x + w - t, y, t as u32, h as u32),
        ] {
            let _ = canvas.fill_rect(edge);
        }
    };
    ring(canvas, 5, 2, Color::RGBA(10, 10, 14, 220));
    ring(
        canvas,
        3,
        3,
        Color::RGBA(FOSFORO.0, FOSFORO.1, FOSFORO.2, 255),
    );
}
