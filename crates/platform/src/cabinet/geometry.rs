//! Gabinete: geometry (extraído de `cabinet.rs`, sem mudança de comportamento).

use super::*;

/// The bounding box of pixels with any alpha, in texture pixels — an
/// image-sized box when the art has no transparency at all. Scans at most
/// one pass over the RGBA buffer (art textures are loaded once).
/// Só os testes usam hoje (o encaixe por bounding box saiu com o corpo do
/// console); mantida pela verificação de regressão.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn content_bbox(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, u32, u32) {
    let (mut min_x, mut min_y) = (w, h);
    let (mut max_x, mut max_y) = (0u32, 0u32);
    let mut seen = false;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                seen = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if !seen {
        return (0, 0, w, h);
    }
    (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

pub(super) fn resolve_aspect(a: f32) -> f32 {
    if a > 0.0 {
        a
    } else {
        4.0 / 3.0
    }
}

pub(super) fn centered_in(area: Rect, w: u32, h: u32) -> Rect {
    let x = area.x() + (area.width() as i32 - w as i32) / 2;
    let y = area.y() + (area.height() as i32 - h as i32) / 2;
    Rect::new(x, y, w, h)
}

/// Largest `aspect`-shaped rect that fits inside `area`, centered in it. Fills
/// the height unless that would overflow the width, then fills the width.
pub(super) fn fit_aspect_in(area: Rect, aspect: f32) -> Rect {
    let mut h = area.height();
    let mut w = (h as f32 * aspect).round() as u32;
    if w > area.width() {
        w = area.width();
        h = (w as f32 / aspect).round() as u32;
    }
    centered_in(area, w, h)
}

/// Where the cabinet actually draws within the real window/display: the
/// whole display when its shape is in the 16:10..16:9 band (Steam Deck's
/// 1280x800, laptops, modern TVs — the composition draws proportionally, so
/// the cabinet simply gets a touch taller), centered otherwise: displays
/// outside the band keep the largest 16:9 rect that fits — an ultrawide
/// monitor gets letterbox bars on the sides instead of a stretched-wide
/// cabinet. `screen_area`/`panel_rect` (and everything downstream) only
/// ever see this rect's width/height, never the raw output size.
pub(super) fn cabinet_canvas_rect(out_w: u32, out_h: u32) -> Rect {
    const MIN_ASPECT: f32 = 1.6; // 16:10 — Steam Deck
    const MAX_ASPECT: f32 = 16.0 / 9.0;
    let out_aspect = out_w as f32 / out_h.max(1) as f32;
    let aspect = out_aspect.clamp(MIN_ASPECT, MAX_ASPECT);
    fit_aspect_in(Rect::new(0, 0, out_w, out_h), aspect)
}

/// The cabinet opening: the window inset by the bezel fractions (a wider chin).
pub(super) fn screen_area(out_w: u32, out_h: u32) -> Rect {
    let sx = (out_w as f32 * BEZEL_SIDE).round() as i32;
    let ty = (out_h as f32 * BEZEL_TOP).round() as i32;
    let by = (out_h as f32 * BEZEL_CHIN).round() as i32;
    let w = (out_w as i32 - 2 * sx).max(16) as u32;
    let h = (out_h as i32 - ty - by).max(16) as u32;
    Rect::new(sx, ty, w, h)
}

/// A four-quad ring from the window edge (cabinet colour) to the recessed
/// screen edge (near-black), so the screen sits in a shadowed well.
pub(super) fn build_bezel_mesh(
    out_w: u32,
    out_h: u32,
    screen: Rect,
    key: (u32, u32, u32, u32),
) -> BezelMesh {
    let norm = |c: (u8, u8, u8)| {
        FColor::RGBA(
            c.0 as f32 / 255.0,
            c.1 as f32 / 255.0,
            c.2 as f32 / 255.0,
            1.0,
        )
    };
    let (cab, rec) = (norm(CABINET), norm(RECESS));
    let z = sdl3::render::FPoint::new(0.0, 0.0);
    let vtx = |x: i32, y: i32, c: FColor| Vertex {
        position: sdl3::render::FPoint::new(x as f32, y as f32),
        color: c,
        tex_coord: z,
    };

    let (or, ob) = (out_w as i32, out_h as i32);
    let (il, it, ir, ib) = (screen.left(), screen.top(), screen.right(), screen.bottom());

    let verts = vec![
        vtx(0, 0, cab),
        vtx(or, 0, cab),
        vtx(or, ob, cab),
        vtx(0, ob, cab), // 0..3 outer
        vtx(il, it, rec),
        vtx(ir, it, rec),
        vtx(ir, ib, rec),
        vtx(il, ib, rec), // 4..7 inner
    ];
    #[rustfmt::skip]
    let indices = vec![
        0, 1, 5, 0, 5, 4, // top
        1, 2, 6, 1, 6, 5, // right
        2, 3, 7, 2, 7, 6, // bottom
        3, 0, 4, 3, 4, 7, // left
    ];
    BezelMesh {
        verts,
        indices,
        key,
    }
}

/// Where the side panel sits: a column on the right, `PANEL_FRAC` of the
/// window, clamped so it neither collapses nor swallows a small window.
pub(super) fn panel_rect(out_w: u32, out_h: u32) -> Rect {
    let w = ((out_w as f32) * PANEL_FRAC)
        .round()
        .clamp(PANEL_MIN as f32, PANEL_MAX as f32) as u32;
    let w = w.min(out_w.saturating_sub(64));
    Rect::new((out_w - w) as i32, 0, w, out_h)
}

/// O deslocamento do tubo para um ponto da grade (`cx`, `cy` em -1..1): o
/// barril puxa os cantos para dentro, e o ganho `1 / (1 - WARP)` os traz de
/// volta EXATAMENTE aos cantos do retângulo da tela (a imagem preenche os
/// cantos da TV, sem o recesso arredondado do emulador). O que passaria da
/// borda no meio dos lados é preso nela — o contorno fica reto, e a
/// curvatura só aparece dentro da imagem.
pub(super) fn crt_displace(cx: f32, cy: f32, warp: f32) -> (f32, f32) {
    let gain = 1.0 / (1.0 - warp);
    (
        (cx * (1.0 - warp * cy * cy) * gain).clamp(-1.0, 1.0),
        (cy * (1.0 - warp * cx * cx) * gain).clamp(-1.0, 1.0),
    )
}

/// Build a textured grid over `dst` whose vertex positions are barrel-distorted
/// (edges bow out, corners pull in) with an edge vignette baked into the vertex
/// colours. Texture coordinates stay a plain grid, so the picture — not just the
/// outline — curves. `alpha` (1.0 normally) lets a caller fade the whole tube in.
pub(super) fn build_crt_mesh(dst: Rect, alpha: f32) -> CrtMesh {
    let n = CRT_GRID;
    let (ox, oy) = (dst.x() as f32, dst.y() as f32);
    let (dw, dh) = (dst.width() as f32, dst.height() as f32);

    let mut verts = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let u = i as f32 / n as f32; // 0..1 texture / grid coord
            let v = j as f32 / n as f32;
            let cx = u * 2.0 - 1.0; // -1..1
            let cy = v * 2.0 - 1.0;

            // Barrel: corners move toward the centre, mid-edges stay put.
            let (dx, dy) = crt_displace(cx, cy, CRT_WARP);

            let px = ox + (dx * 0.5 + 0.5) * dw;
            let py = oy + (dy * 0.5 + 0.5) * dh;

            let r2 = (cx * cx + cy * cy).min(2.0) / 2.0;
            let shade = (1.0 - CRT_VIGNETTE * r2 * r2).clamp(0.0, 1.0);

            verts.push(Vertex {
                position: sdl3::render::FPoint::new(px, py),
                color: FColor::RGBA(shade, shade, shade, alpha),
                tex_coord: sdl3::render::FPoint::new(u, v),
            });
        }
    }

    let stride = (n + 1) as i32;
    let mut indices = Vec::with_capacity(n * n * 6);
    for j in 0..n as i32 {
        for i in 0..n as i32 {
            let a = j * stride + i;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    CrtMesh {
        verts,
        indices,
        w: dst.width(),
        h: dst.height(),
    }
}
