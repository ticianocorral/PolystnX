//! Gabinete: chin (extraído de `cabinet.rs`, sem mudança de comportamento).

use super::*;

/// The badge's two rows and their colors — pure so the tests can pin the
/// contract: "RA ATIVADO" always in the OSD green, the mode in amber when
/// hardcore and in the nameplate grey when softcore.
pub(super) fn ra_badge_rows(hardcore: bool) -> [(&'static str, (u8, u8, u8)); 2] {
    [
        ("RA ATIVADO", OSD_GREEN),
        (
            if hardcore { "HARDCORE" } else { "SOFTCORE" },
            if hardcore {
                (240, 180, 60)
            } else {
                (235, 235, 225)
            },
        ),
    ]
}

/// Quanto tempo o aviso de abertura degradado segura o chin (a consequência
/// merece ser lida com calma).
pub(super) const SW_NOTICE_TTL: Duration = Duration::from_secs(10);

/// O aviso residente do chin na abertura — linhas já pareadas com cor e o
/// prazo. Hoje só a degradação (renderer por software); a versão e o nome
/// do renderer saíram do queixo.
pub(super) struct ChinNotice {
    pub(super) lines: Vec<(String, (u8, u8, u8))>,
    pub(super) until: Instant,
}

/// As linhas do aviso degradado — pura para o teste fixar o contrato: o
/// título no âmbar de aviso (o mesmo do hardcore do RA), a consequência no
/// cinza claro do chin. Degradado, não morto.
pub(super) fn sw_notice_rows() -> [(&'static str, (u8, u8, u8)); 2] {
    [
        ("SEM ACELERAÇÃO DE GPU", (240, 180, 60)),
        ("DESEMPENHO REDUZIDO", LIGHT_GREY_ROW),
    ]
}

/// O aviso de abertura quando o renderer caiu em software (nenhum driver de
/// hardware da lista do hint subiu).
pub(super) fn sw_boot_notice() -> ChinNotice {
    ChinNotice {
        lines: sw_notice_rows()
            .iter()
            .map(|(l, c)| (l.to_string(), *c))
            .collect(),
        until: Instant::now() + SW_NOTICE_TTL,
    }
}

/// O aviso residente do chin (abertura) — linhas alinhadas à direita com a
/// sombra de 2px de todo vidro, truncando contra o nameplate como os demais
/// blocos do chin.
pub(super) fn draw_chin_notice(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    nameplate: &str,
    notice: &ChinNotice,
) {
    const MARGIN: i32 = 24;
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 || notice.lines.is_empty() {
        return;
    }
    let cell = GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let gap = 4i32;
    // Mesmo limite esquerdo dos outros blocos: a linha mais larga do
    // nameplate.
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let right = chin_right(screen, out_h, MARGIN);
    let left_limit = screen.left() + nameplate_w + MARGIN * 2;
    let max_cols = ((right - left_limit).max(cell) / cell) as usize;
    let shown: Vec<(String, (u8, u8, u8))> = notice
        .lines
        .iter()
        .map(|(l, c)| (truncate_to_cols(l, max_cols), *c))
        .collect();
    let block_h = shown.len() as i32 * row + (shown.len() as i32 - 1) * gap;
    let mut y = chin_top + (chin_h - block_h) / 2;
    for (line, color) in &shown {
        let x = right - line.chars().count() as i32 * cell;
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(1.0, (12, 14, 12)),
            line,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(1.0, *color),
            line,
            usize::MAX,
        );
        y += row + gap;
    }
}

/// A linha do painel de desempenho — pura para o teste fixar o formato:
/// ASCII de atlas (sem °/·), dois espaços entre segmentos e os segmentos
/// cuja leitura falhou simplesmente omitidos.
pub(super) fn perf_line(fps: u32, cpu: Option<u32>, ram_mb: Option<u32>) -> String {
    let mut parts = vec![format!("{fps} FPS")];
    if let Some(c) = cpu {
        parts.push(format!("CPU {c}C"));
    }
    if let Some(m) = ram_mb {
        parts.push(format!("RAM {m} MB"));
    }
    parts.join("  ")
}

/// O painel de desempenho do modo dev — NA MOLDURA da TV, na faixa acima da
/// tela (o "queixo de cima"): centralizado sobre a TV, fundo escuro
/// translúcido para ler sobre a carcaça. `center` é o rect da tela (para o
/// x); `band` é a faixa da moldura (topo, altura) onde a linha assenta.
pub(super) fn draw_perf_overlay(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    center: Rect,
    band: (i32, i32),
    line: &str,
) {
    const PAD_X: i32 = 10;
    const PAD_Y: i32 = 6;
    if line.is_empty() {
        return;
    }
    let cell = GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let text_w = line.chars().count() as i32 * cell;
    let x = center.left() + (center.width() as i32 - text_w) / 2;
    let (band_top, band_h) = band;
    let y = band_top + ((band_h - row) / 2).max(2);
    fill_rect_rgba(
        canvas,
        (0, 0, 0, 150),
        Rect::new(
            x - PAD_X,
            y - PAD_Y,
            (text_w + PAD_X * 2) as u32,
            (row + PAD_Y * 2) as u32,
        ),
    );
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(1.0, LIGHT_GREY_ROW),
        line,
        usize::MAX,
    );
}

/// The persistent RetroAchievements badge — the same chin corner as the
/// unlock block, shown whenever that block isn't on screen: the RA logo
/// (the registered favicon; falls back to the pixel gamepad tile) left of
/// two rows, "RA ATIVADO" in the OSD green and the mode below (amber for
/// hardcore, the nameplate grey for softcore). Same 2px dark shadow as
/// everything else on the glass.
pub(super) fn draw_chin_ra(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    out_h: u32,
    nameplate: &str,
    hardcore: bool,
) {
    const MARGIN: i32 = 24;
    const TILE: i32 = 56;
    const TILE_BG: (u8, u8, u8) = (18, 22, 52);
    const TILE_EDGE: (u8, u8, u8) = (240, 180, 60);
    const GOLD: (u8, u8, u8) = (240, 190, 70);
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 {
        return;
    }
    let cell = GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let rows = ra_badge_rows(hardcore);
    // Same left limit as the unlock block: the nameplate's widest line.
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let right = chin_right(screen, out_h, MARGIN);
    let left_limit = screen.left() + nameplate_w + MARGIN * 2 + TILE + gap;
    let max_cols = ((right - left_limit).max(cell) / cell) as usize;
    let shown: Vec<(String, (u8, u8, u8))> = rows
        .iter()
        .map(|(l, c)| (truncate_to_cols(l, max_cols), *c))
        .collect();
    let block_h = 2 * row + gap;
    let text_w = shown
        .iter()
        .map(|(l, _)| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let mut y = chin_top + (chin_h - block_h) / 2;
    let tile_x = right - text_w - gap - TILE;
    let tile_y = chin_top + (chin_h - TILE) / 2;

    // The mark: the official RA favicon (registered by the app) when it's
    // there; the pixel gamepad tile is the fallback so the badge never
    // depends on the image having been registered.
    if images.contains_key(&RA_LOGO_IMG) {
        draw_image_absolute(
            canvas,
            images,
            RA_LOGO_IMG,
            tile_x,
            tile_y,
            TILE as u32,
            TILE as u32,
        );
    } else {
        canvas.set_draw_color(Color::RGB(TILE_EDGE.0, TILE_EDGE.1, TILE_EDGE.2));
        let _ = canvas.fill_rect(Rect::new(tile_x, tile_y, TILE as u32, TILE as u32));
        canvas.set_draw_color(Color::RGB(TILE_BG.0, TILE_BG.1, TILE_BG.2));
        let _ = canvas.fill_rect(Rect::new(
            tile_x + 2,
            tile_y + 2,
            (TILE - 4) as u32,
            (TILE - 4) as u32,
        ));
        const PAD: [&str; 7] = [
            "..............",
            "##############",
            "#..#......o.o#",
            "#.###........#",
            "#..#.........#",
            "##############",
            "..............",
        ];
        let s = 3;
        let px = tile_x + (TILE - 14 * s) / 2;
        let py = tile_y + (TILE - 7 * s) / 2;
        canvas.set_draw_color(Color::RGB(GOLD.0, GOLD.1, GOLD.2));
        for (gy, line) in PAD.iter().enumerate() {
            for (gx, ch) in line.chars().enumerate() {
                if ch == '.' {
                    continue;
                }
                let _ = canvas.fill_rect(Rect::new(
                    px + gx as i32 * s,
                    py + gy as i32 * s,
                    s as u32,
                    s as u32,
                ));
            }
        }
    }
    for (line, color) in &shown {
        let x = right - line.chars().count() as i32 * cell;
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(1.0, (12, 14, 12)),
            line,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(1.0, *color),
            line,
            usize::MAX,
        );
        y += row + gap;
    }
}

/// The achievements notification (plan fase 4): a right-aligned block in
/// the chin, mirroring `draw_brand` on the left. First line in the OSD
/// green (the "CONQUISTA DESBLOQUEADA" header), the achievement name in
/// near-white, the trailing line (points) in the nameplate's own grey —
/// all flat on the glass with the same 2px dark shadow as the CH 3 banner.
/// Lines longer than the space between the nameplate and the right margin
/// are truncated with "..." so the two blocks never touch. `badge` is the
/// image id registered via `set_image`, drawn left of the text.
#[allow(clippy::too_many_arguments)] // canvas/font/images + 5 layout facts
pub(super) fn draw_chin_osd(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    out_h: u32,
    nameplate: &str,
    lines: &[String],
    badge: Option<u64>,
) {
    const MARGIN: i32 = 24;
    const LIGHT: (u8, u8, u8) = (235, 235, 225);
    const BADGE: u32 = 56;
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 || lines.is_empty() {
        return;
    }
    let cell = GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let has_badge = badge.is_some_and(|id| images.contains_key(&id));
    let badge_room = if has_badge { BADGE as i32 + 12 } else { 0 };
    // The nameplate's widest line sets this block's left limit, so a long
    // achievement name truncates instead of colliding with it.
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    let right = chin_right(screen, out_h, MARGIN);
    let left_limit = screen.left() + nameplate_w + MARGIN * 2 + badge_room;
    let max_cols = ((right - left_limit).max(cell) / cell) as usize;
    let shown: Vec<String> = lines
        .iter()
        .map(|l| truncate_to_cols(l, max_cols))
        .collect();
    let block_h = shown.len() as i32 * row + (shown.len() as i32 - 1) * gap;
    let mut y = chin_top + (chin_h - block_h) / 2;
    let text_w = shown
        .iter()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    if has_badge {
        // Vertically centred in the chin, the same line as the text block's.
        draw_image_absolute(
            canvas,
            images,
            badge.unwrap_or(0),
            right - text_w - badge_room,
            chin_top + (chin_h - BADGE as i32) / 2,
            BADGE,
            BADGE,
        );
    }
    for (i, line) in shown.iter().enumerate() {
        let color = if i == 0 {
            OSD_GREEN
        } else if i + 1 == shown.len() {
            BRAND_TEXT
        } else {
            LIGHT
        };
        let x = right - line.chars().count() as i32 * cell;
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(1.0, (12, 14, 12)),
            line,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(1.0, color),
            line,
            usize::MAX,
        );
        y += row + gap;
    }
}

/// `(rect da seta do app, rect da seta do core)` — `None` quando a linha
/// não tem seta neste frame. Saída de [`draw_brand`] para o hit-test do
/// idle (plan revision: "ao clicar no icone verde de atualizar").
pub(super) type UpdateArrowRects = (Option<Rect>, Option<Rect>);

pub(super) fn draw_brand(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    label: &str,
    updates: (bool, bool),
) -> UpdateArrowRects {
    let mut arrows: UpdateArrowRects = (None, None);
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 {
        return arrows;
    }
    // A '\n' in the label stacks lines (plan revision: "no nameplate colocar
    // a versão do PCSX Rearmed abaixo do PolystnX") — the block centred in
    // the chin. When the chin can't fit them all, keep only the first (the
    // app's own name/version) rather than spilling over the bezel.
    let lines: Vec<&str> = label.split('\n').collect();
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let fits = chin_h >= (lines.len() as i32 * row + (lines.len() as i32 - 1) * gap).max(row);
    let shown = if fits { &lines[..] } else { &lines[..1] };
    let block_h = shown.len() as i32 * row + (shown.len() as i32 - 1) * gap;
    let mut y = chin_top + (chin_h - block_h) / 2;
    for (i, line) in shown.iter().enumerate() {
        draw_text_absolute(
            canvas,
            font,
            screen.left(),
            y,
            TextStyle::new(1.0, BRAND_TEXT),
            line,
            usize::MAX,
        );
        // The "tem update" arrow (plan revision: "um icone verde no
        // nameplate ... uma seta verde pra cima com update") — right after
        // the line's own text, vertically centred on it. Line 0 is the
        // app's version, line 1 the core's.
        let wants_arrow = i == 0 && updates.0 || i == 1 && updates.1;
        if wants_arrow {
            let ax = screen.left() + line.chars().count() as i32 * GLYPH_W as i32 + 10;
            draw_update_arrow(canvas, ax, y + row / 2);
            // Área de clique generosa (o ícone tem 11×12 px) — uma faixa
            // da altura da linha, um pouco mais larga que a seta.
            let hit = Rect::new(ax - 4, y, 19u32, row as u32);
            if i == 0 {
                arrows.0 = Some(hit);
            } else {
                arrows.1 = Some(hit);
            }
        }
        y += row + gap;
    }
    arrows
}

/// The nameplate's "tem update" marker (plan revision: "uma seta verde pra
/// cima com update") — a chunky pixel arrow pointing up, in the same green
/// as the CH 3 banner / OSD header, with the furniture's usual 1px dark
/// drop shadow. 11×12 px: a stepped triangle head (1-3-5-7-9 px rows,
/// matching the bitmap font's chunkiness) over a 3px stem.
pub(super) fn draw_update_arrow(canvas: &mut WindowCanvas, x: i32, y_center: i32) {
    const HEAD_ROWS: [i32; 5] = [1, 3, 5, 7, 9];
    const STEM_W: i32 = 3;
    const STEM_H: i32 = 3;
    let total_h = HEAD_ROWS.len() as i32 + STEM_H;
    let top = y_center - total_h / 2;
    let cx = x + 5; // centre line of the widest head row
    let mut draw = |color: Color, ox: i32, oy: i32| {
        canvas.set_draw_color(color);
        for (r, &w) in HEAD_ROWS.iter().enumerate() {
            let _ = canvas.fill_rect(Rect::new(cx - w / 2 + ox, top + r as i32 + oy, w as u32, 1));
        }
        let _ = canvas.fill_rect(Rect::new(
            cx - STEM_W / 2 + ox,
            top + HEAD_ROWS.len() as i32 + oy,
            STEM_W as u32,
            STEM_H as u32,
        ));
    };
    draw(Color::RGB(12, 14, 12), 1, 1); // shadow
    draw(Color::RGB(OSD_GREEN.0, OSD_GREEN.1, OSD_GREEN.2), 0, 0);
}
