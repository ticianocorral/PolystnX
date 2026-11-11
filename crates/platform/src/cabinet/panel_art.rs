//! Gabinete: panel_art (extraído de `cabinet.rs`, sem mudança de comportamento).

use super::*;

/// Like `Screen::image_fit`, but at absolute window coordinates instead of
/// offset into the 2D screen buffer — for cabinet furniture and panel art.
/// Recorta um blit (célula de origem → retângulo de destino) na banda
/// visível do card (`ymin` = a linha de encaixe na boca, `ymax` = o pé da
/// face): fora da banda não desenha (`None`); o que cruza as linhas copia
/// só a parte de dentro, com a origem cortada na mesma proporção. É o
/// "some atrás da carcaça / não vaza na lista de comandos" do card — sem
/// depender do clip do renderizador (o SDL3.4 do cocoa engole o clip).
pub(super) fn blit_band(src: Rect, dst: Rect, ymin: i32, ymax: i32) -> Option<(Rect, Rect)> {
    if dst.bottom() <= ymin || dst.y() >= ymax {
        return None;
    }
    let (mut src, mut dst) = (src, dst);
    if dst.y() < ymin {
        let cut = ymin - dst.y();
        let src_cut =
            (src.height() as i32 * cut / dst.height() as i32).min(src.height() as i32 - 1);
        src = Rect::new(
            src.x(),
            src.y() + src_cut,
            src.width(),
            (src.height() as i32 - src_cut) as u32,
        );
        dst = Rect::new(
            dst.x(),
            ymin,
            dst.width(),
            (dst.height() as i32 - cut) as u32,
        );
    }
    if dst.bottom() > ymax {
        let cut = dst.bottom() - ymax;
        let src_cut =
            (src.height() as i32 * cut / dst.height() as i32).min(src.height() as i32 - 1);
        src = Rect::new(
            src.x(),
            src.y(),
            src.width(),
            (src.height() as i32 - src_cut) as u32,
        );
        dst = Rect::new(
            dst.x(),
            dst.y(),
            dst.width(),
            (dst.height() as i32 - cut) as u32,
        );
    }
    Some((src, dst))
}

pub(super) fn draw_image_absolute(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    id: u64,
    x: i32,
    y: i32,
    bw: u32,
    bh: u32,
) {
    let Some(img) = images.get(&id) else {
        return;
    };
    let (iwf, ihf) = (img.w as f32, img.h as f32);
    let scale = (bw as f32 / iwf).min(bh as f32 / ihf);
    let dw = (iwf * scale).round() as i32;
    let dh = (ihf * scale).round() as i32;
    let dx = x + (bw as i32 - dw) / 2;
    let dy = y + (bh as i32 - dh) / 2;
    let _ = canvas.copy(
        &img.tex,
        None,
        Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_image_absolute_band(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    id: u64,
    x: i32,
    y: i32,
    bw: u32,
    bh: u32,
    ymin: i32,
    ymax: i32,
) {
    let Some(img) = images.get(&id) else {
        return;
    };
    let (iwf, ihf) = (img.w as f32, img.h as f32);
    let scale = (bw as f32 / iwf).min(bh as f32 / ihf);
    let dw = (iwf * scale).round() as i32;
    let dh = (ihf * scale).round() as i32;
    let dx = x + (bw as i32 - dw) / 2;
    let dy = y + (bh as i32 - dh) / 2;
    let Some((src, dst)) = blit_band(
        Rect::new(0, 0, img.w, img.h),
        Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
        ymin,
        ymax,
    ) else {
        return;
    };
    let _ = canvas.copy(&img.tex, src, dst);
}

/// Círculo preenchido rasterizado linha a linha (o canvas só tem retângulos).
pub(super) fn fill_circle(
    canvas: &mut WindowCanvas,
    color: (u8, u8, u8),
    cx: i32,
    cy: i32,
    r: i32,
) {
    canvas.set_draw_color(Color::RGB(color.0, color.1, color.2));
    for dy in -r..=r {
        let half = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half, cy + dy, (half * 2).max(1) as u32, 1));
    }
}

/// Círculo com ALFA (os brilhos do plástico usam) — requer blend ativo no
/// canvas; liga e desliga sozinho.
pub(super) fn fill_circle_rgba(
    canvas: &mut WindowCanvas,
    color: (u8, u8, u8, u8),
    cx: i32,
    cy: i32,
    r: i32,
) {
    let prev = canvas.blend_mode();
    canvas.set_blend_mode(sdl3::render::BlendMode::Blend);
    canvas.set_draw_color(sdl3::pixels::Color::RGBA(
        color.0, color.1, color.2, color.3,
    ));
    for dy in -r..=r {
        let half = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half, cy + dy, (half * 2).max(1) as u32, 1));
    }
    canvas.set_blend_mode(prev);
}

/// Retângulo com ALFA (os biséis das portas de memory card usam).
pub(super) fn fill_rect_rgba(canvas: &mut WindowCanvas, color: (u8, u8, u8, u8), r: Rect) {
    let prev = canvas.blend_mode();
    canvas.set_blend_mode(sdl3::render::BlendMode::Blend);
    canvas.set_draw_color(sdl3::pixels::Color::RGBA(
        color.0, color.1, color.2, color.3,
    ));
    let _ = canvas.fill_rect(r);
    canvas.set_blend_mode(prev);
}

/// Um botão redondo de plástico, como os do PS1: aro escuro, corpo convexo
/// com a borda de baixo em sombra (o corpo "sai" da carcaça) e o brilho da
/// luz no alto. `pressed` afunda o botão: corpo escurecido e o brilho
/// migrando para baixo.
pub(super) fn draw_round_plastic(
    canvas: &mut WindowCanvas,
    cx: i32,
    cy: i32,
    r: i32,
    pressed: bool,
) {
    // A sombra curta que o botão joga na carcaça.
    let drop = if pressed { 1 } else { 3 };
    fill_circle_rgba(canvas, (0, 0, 0, 70), cx + 1, cy + drop, r);
    // Só um fio escuro de contorno — sem aro claro.
    fill_circle(canvas, PSX_SHELL_EDGE, cx, cy, r);
    // O corpo, um degradê vertical suave de plástico fosco.
    let body = r - 1;
    let k = if pressed { 0.88 } else { 1.0 };
    for dy in -body..=body {
        let t = (dy + body) as f32 / (2 * body).max(1) as f32;
        let lerp = |a: f32, b: f32| ((a + (b - a) * t) * k) as u8;
        canvas.set_draw_color(sdl3::pixels::Color::RGB(
            lerp(192.0, 160.0),
            lerp(188.0, 156.0),
            lerp(178.0, 146.0),
        ));
        let half = ((body * body - dy * dy) as f32).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - half, cy + dy, (half * 2).max(1) as u32, 1));
    }
    if !pressed {
        // O brilho da luz no alto-esquerda do plástico.
        fill_circle_rgba(
            canvas,
            (255, 255, 255, 30),
            cx - r / 4,
            cy - r * 3 / 5,
            (r as f32 * 0.38) as i32,
        );
    }
}

/// Os retângulos clicáveis que a face do console devolve ao painel.
#[derive(Clone, Copy)]
pub(super) struct FaceHits {
    pub(super) power: Rect,
    pub(super) eject: Rect,
    pub(super) reset: Rect,
    pub(super) cards1: Rect,
    /// A label do card encaixado (o adesivo com o nome e o "Trocar/
    /// Ejetar") — clicável igual ao bay: também abre a biblioteca (`None`
    /// com o slot vazio).
    pub(super) card_btn: Option<Rect>,
    /// O botão ANALOG da entrada 1 (clicar alterna o modo).
    pub(super) analog1: Rect,
}

/// O bloco do console no painel, sem o corpo: fundo limpo, apenas os três
/// botões redondos do PS1 FAT (Power com LED, Reset menor acima dele, e o
/// Open/Eject do outro lado), o espaço central livre para o disco girar e
/// os dois botões de memory card no padrão das opções do painel. Retorna a
/// área central onde o disco se assenta.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_slot_furniture(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    block: Rect,
    led_on: bool,
    reset_pressed: bool,
    lid_open: bool,
    card_labels: Option<&str>,
    card_motion: Option<(f32, bool)>,
    analog: bool,
    lid: bool,
) -> (Rect, FaceHits) {
    let fill = |canvas: &mut WindowCanvas, color: (u8, u8, u8), r: Rect| {
        canvas.set_draw_color(Color::RGB(color.0, color.1, color.2));
        let _ = canvas.fill_rect(r);
    };

    // A entrada de controle 1 e o bay do memory card lado a lado (plan
    // revision: "coloque o controle 1 na esquerda e o memory card na
    // direita") — e a entrada 2 saiu com o suporte ao segundo controle
    // (removido "no momento").
    let gap = 4;
    // Tela apertada (Steam Deck: painel de jogo com menos de 360 px úteis —
    // 1280×800/720p dão ~280). Lá o console sobe (logo mais baixa), ganha
    // bloco mais alto, slot de controle em duas linhas, botões recolhidos
    // pras bordas, Reset maior e disco maior. FHD+ não muda (plan revision:
    // "na resolução normal nao mexer — está perfeito").
    let tight = block.width() < 360;
    // 56 px de altura para os DOIS boxes (76 nas apertadas: o status do
    // controle quebra em duas linhas e o ANALOG precisa caber no furo); os
    // botões redondos de cima precisam da face.
    let box_h = if tight { 76 } else { 56 };
    let box_w = (block.width() as f32 * 0.46).round() as i32;
    // As caixas descem 5 px (mais respiro do disco); o vão do disco e dos
    // botões continua medido pela posição antiga.
    let box_drop = 10;
    let grid_top = block.bottom() - (box_h + 4) + box_drop;
    let analog_rect;
    // As duas caixas centradas no bloco (margens iguais dos dois lados).
    let pair_x = block.x() + (block.width() as i32 - (box_w * 2 + gap)) / 2;
    let p1 = Rect::new(pair_x, grid_top, box_w as u32, box_h as u32);
    let mc1 = Rect::new(p1.right() + gap, grid_top, box_w as u32, box_h as u32);
    // O bay: painel do adesivo rebaixado (onde a arte do card escolhido
    // vai colar — é também a área clicável da biblioteca de cards) e o
    // rótulo "MEMORY CARD" moldado em preto baixo-relevo, como o SONY da
    // foto. O box do controle usa a MESMA altura (placa contínua na
    // carcaça real) e as plaquinhas ANALOG/RUMBLE.
    // UMA porta de memory card (plan revision: "remova o slot 2 — o console
    // só vai ter um slot"), à direita da entrada de controle 1; a entrada de

    // ---------- BOX: o bay do memory card (à DIREITA) ----------
    fill(canvas, PSX_SHELL_EDGE, mc1);
    fill(
        canvas,
        PSX_SHELL,
        Rect::new(mc1.x() + 1, mc1.y() + 1, mc1.width() - 2, mc1.height() - 2),
    );
    fill_rect_rgba(
        canvas,
        (255, 255, 255, 30),
        Rect::new(mc1.x() + 1, mc1.y() + 1, mc1.width() - 2, 2),
    );
    fill_rect_rgba(
        canvas,
        (0, 0, 0, 40),
        Rect::new(mc1.x() + 1, mc1.bottom() - 3, mc1.width() - 2, 2),
    );
    let seated = card_labels.is_some();
    // A copy do topo do tray, CENTRALIZADA e em CAIXA ALTA (o padrão dos
    // rótulos do painel) — e na escala que couber no bay: o "Memory Card"
    // fixo estourava em janela apertada e a caixa do controle (desenhada
    // depois) pintava por cima, deixando o rótulo cortado (plan revision:
    // "melhore a copy que ficou cortada ali no topo do slot").
    {
        let copy = "MEMORY CARD";
        let px = label::fit_px(copy, 13.0, mc1.width() as f32 - 14.0);
        label::draw_label(
            canvas,
            copy,
            mc1.x() + mc1.width() as i32 / 2,
            mc1.y() + 11,
            px,
            (26, 26, 24),
        );
    }
    // O slot: a BOCA por onde o card entra — baixa (18 px), funda (a sombra
    // interna em degradê dá o efeito de o card estar DENTRO do console, não
    // de um buraco vazado). plan revision: "a abertura nao deveria ser
    // transparente, reduza a altura e faça um sombreamento".
    let recess = Rect::new(mc1.x() + 6, mc1.y() + 27, mc1.width() - 12, 14);
    fill(canvas, (36, 34, 31), recess);
    // degradê de profundidade: escuro no topo (o interior do console),
    // clareando pra baixo (a borda da boca).
    fill_rect_rgba(
        canvas,
        (0, 0, 0, 150),
        Rect::new(recess.x(), recess.y(), recess.width(), 5),
    );
    fill_rect_rgba(
        canvas,
        (0, 0, 0, 70),
        Rect::new(recess.x(), recess.y() + 5, recess.width(), 5),
    );
    fill(
        canvas,
        (12, 11, 10),
        Rect::new(recess.x(), recess.y(), recess.width(), 1),
    );
    fill(
        canvas,
        (120, 116, 107),
        Rect::new(recess.x(), recess.bottom() - 1, recess.width(), 1),
    );
    if !seated {
        // VAZIO: o convite é um BOTÃO de verdade, ocupando o bay abaixo do
        // título (plan revision: "o aviso inserir card tá muito pequeno,
        // ruim de ver e feio, faça tipo um botão"). O clique no bay inteiro
        // já abre a biblioteca — o botão é o alvo visível desse gesto; o
        // plástico escuro com borda clara lê como peça do console. O rótulo
        // desce de escala até caber (o draw_button cortava em "Inserir ...").
        let btn = Rect::new(
            mc1.x() + 6,
            mc1.y() + 20,
            mc1.width() - 12,
            mc1.height() - 22,
        );
        fill(canvas, PANEL_TEXT, btn);
        fill(
            canvas,
            PANEL_BTN_BG,
            Rect::new(btn.x() + 1, btn.y() + 1, btn.width() - 2, btn.height() - 2),
        );
        fill_rect_rgba(
            canvas,
            (255, 255, 255, 25),
            Rect::new(btn.x() + 1, btn.y() + 1, btn.width() - 2, 1),
        );
        fill_rect_rgba(
            canvas,
            (0, 0, 0, 50),
            Rect::new(btn.x(), btn.bottom(), btn.width(), 1),
        );
        let copy = "Inserir card";
        let px = label::fit_px(copy, 15.0, btn.width() as f32 - 12.0);
        label::draw_label(
            canvas,
            copy,
            btn.x() + btn.width() as i32 / 2,
            btn.y() + btn.height() as i32 / 2,
            px,
            PANEL_TEXT,
        );
    }
    // ---------- BOX: a entrada de controle 1 (à ESQUERDA) ----------
    // O pad 1 vive na entrada 1 de fábrica — o teclado/gamepad conecta
    // nela; o drag de troca de entrada saiu junto com o controle 2.
    {
        let plate = p1;
        fill(canvas, PSX_SHELL_EDGE, plate);
        fill(
            canvas,
            PSX_SHELL,
            Rect::new(
                plate.x() + 1,
                plate.y() + 1,
                plate.width() - 2,
                plate.height() - 2,
            ),
        );
        fill_rect_rgba(
            canvas,
            (255, 255, 255, 30),
            Rect::new(plate.x() + 1, plate.y() + 1, plate.width() - 2, 2),
        );
        fill_rect_rgba(
            canvas,
            (0, 0, 0, 40),
            Rect::new(plate.x() + 1, plate.bottom() - 3, plate.width() - 2, 2),
        );

        let hole_h = box_h - 8;
        let hole_y = plate.y() + (box_h - hole_h) / 2;
        let hole = Rect::new(plate.x() + 7, hole_y, plate.width() - 14, hole_h as u32);
        fill(canvas, (30, 28, 26), hole);
        fill(
            canvas,
            (12, 11, 10),
            Rect::new(hole.x(), hole.y(), hole.width(), 2),
        );

        // Conectado: status em verde + o botão ANALOG. Nas telas apertadas o
        // status quebra em DUAS linhas — "CONTROLE 1 CONECTADO" em linha
        // única estourava o furo e saía cortado ("…CONECT"); com o slot mais
        // alto, as duas linhas cabem em escala cheia.
        let (l1, l2) = if tight {
            ("CONTROLE 1", Some("CONECTADO"))
        } else {
            ("CONTROLE 1 CONECTADO", None)
        };
        let sh = (GLYPH_H as f32 * 0.9).round() as i32;
        let px = label::fit_px(l1, sh as f32 * 0.82, hole.width() as f32 - 14.0);
        let l1y = hole.y() + 3;
        let green = (120, 220, 140);
        let at = label::Anchor::Left;
        let band = (i32::MIN, i32::MAX);
        label::draw_text_vec(
            canvas,
            l1,
            (hole.x() + 6, l1y + sh / 2),
            at,
            px,
            green,
            band,
        );
        let status_bottom = if let Some(l2) = l2 {
            let at2 = (hole.x() + 6, l1y + sh + sh / 2);
            label::draw_text_vec(canvas, l2, at2, at, px, green, band);
            l1y + sh * 2
        } else {
            l1y + sh
        };
        let analog_on = analog;
        let mut tag = |x: i32, y: i32, w: i32, h: i32, copy: &str, on: bool| {
            fill(canvas, PSX_SHELL_EDGE, Rect::new(x, y, w as u32, h as u32));
            fill(
                canvas,
                PSX_SHELL,
                Rect::new(x + 1, y + 1, w as u32 - 2, h as u32 - 2),
            );
            let px = label::fit_px(copy, h as f32 * 0.72, (w - 28) as f32);
            label::draw_label(canvas, copy, x + (w - 12) / 2, y + h / 2, px, (40, 38, 35));
            fill_circle(
                canvas,
                if on { (110, 220, 130) } else { (235, 50, 50) },
                x + w - 9,
                y + h / 2,
                4,
            );
        };
        // SÓ o ANALOG (plan revision: "deixar apenas o botão de analog") —
        // ele é o botão do DualShock: liga o analógico E a vibração. O LED
        // fala o modo: vermelho = desligado (digital), verde = ligado.
        let l2y = status_bottom + 2;
        let w_t = hole.width() as i32 - 12;
        tag(hole.x() + 6, l2y, w_t, sh, "ANALOG", analog_on);
        analog_rect = Rect::new(hole.x() + 6, l2y, w_t as u32, sh as u32);
    }

    // Os três botões redondos, centrados na altura útil acima das portas.
    // Nas telas apertadas eles recolhem pras bordas — é o vão central que
    // dá o tamanho do disco.
    let face_top = block.y() + 6;
    let face_bottom = mc1.y() - box_drop - 2;
    let face_h = (face_bottom - face_top).max(1);
    let btn_r =
        ((face_h as f32 * 0.30).round() as i32).min(if block.width() < 640 { 34 } else { 46 });
    let face_inset = if tight { 2 } else { 10 };
    let col_l = block.x() + face_inset + btn_r;
    let col_r = block.right() - face_inset - btn_r;

    // Reset maior nas apertadas: 24 px de raio eram difíceis de mirar.
    let reset_r = (btn_r as f32 * if tight { 0.85 } else { 0.70 }).round() as i32;
    let led_h = 13;
    let power_cy = face_top + face_h * 7 / 10;
    let reset_cy = face_top
        + 6
        + ((power_cy - btn_r) - (face_top + 6) - 14 - reset_r * 2).max(0) / 2
        + reset_r;

    draw_round_plastic(canvas, col_l, reset_cy, reset_r, reset_pressed);
    let reset_hit = Rect::new(
        col_l - reset_r,
        reset_cy - reset_r,
        (reset_r * 2) as u32,
        (reset_r * 2) as u32,
    );
    let label = "RESET";
    let px = label::fit_px(label, reset_r as f32 * 0.62, reset_r as f32 * 1.30);
    label::draw_label(canvas, label, col_l, reset_cy, px, (38, 38, 36));
    // Ligado, o POWER fica afundado; desligado, ele desafunda.
    draw_round_plastic(canvas, col_l, power_cy, btn_r, led_on);
    let led_w = 4;
    fill(
        canvas,
        if led_on { PSX_LED_ON } else { PSX_LED_OFF },
        Rect::new(
            col_l - led_w / 2,
            power_cy + btn_r + 3,
            led_w as u32,
            led_h as u32,
        ),
    );
    let label = "POWER";
    let px = label::fit_px(label, btn_r as f32 * 0.52, btn_r as f32 * 1.55);
    label::draw_label(
        canvas,
        label,
        col_l,
        power_cy + led_on as i32,
        px,
        PSX_BTN_TEXT,
    );
    let power_hit = Rect::new(
        col_l - btn_r,
        power_cy - btn_r,
        (btn_r * 2) as u32,
        (btn_r * 2) as u32,
    );

    draw_round_plastic(canvas, col_r, power_cy, btn_r, false);
    let label = if lid_open { "CLOSE" } else { "OPEN" };
    let px = label::fit_px(label, btn_r as f32 * 0.52, btn_r as f32 * 1.55);
    label::draw_label(canvas, label, col_r, power_cy, px, PSX_BTN_OPEN);
    let eject_hit = Rect::new(
        col_r - btn_r,
        power_cy - btn_r,
        (btn_r * 2) as u32,
        (btn_r * 2) as u32,
    );

    // O vão central do disco — os limites seguem o grid das caixas.
    let cx = block.x() + block.width() as i32 / 2;
    let disc_top_lim = block.y() - 7;
    let disc_bot_lim = mc1.y() - box_drop - 5;
    let cy = (disc_top_lim + disc_bot_lim) / 2;
    let clearance = |bx: i32, by: i32, br: i32| -> f32 {
        let dx = (cx - bx) as f32;
        let dy = (cy - by) as f32;
        (dx * dx + dy * dy).sqrt() - br as f32
    };
    // A folga de 4 px vale nas DUAS larguras, e o divisor 1.13 é o mesmo: ele
    // é o raio do aro do LEITOR (side × 0.565 — a placa de PSX_SHELL em volta
    // do disco, `side_reader` × 1.06 mais o lábio de 0.035), não o do quad do
    // disco. Nas apertadas o vão era medido pelo raio do disco (1.08) e sem
    // margem: o aro do leitor passava dos 4 px calculados e encostava no
    // POWER/OPEN e no RESET (plan revision: "os botões estão encostando no
    // disco, coloque uma leve separação") — agora o vão cede 11 px de lado e
    // a folga de 4 px do FHD fica igual nas telas apertadas.
    let reach = clearance(col_l, reset_cy, reset_r)
        .min(clearance(col_l, power_cy, btn_r))
        .min(clearance(col_r, power_cy, btn_r))
        - 4.0;
    let side_v = ((disc_bot_lim - disc_top_lim) as f32 / 1.145) as i32;
    let side_w = (reach * 2.0 / 1.13) as i32;
    let side = side_v.min(side_w).max(24);
    let mut disc = Rect::new(
        block.x() + (block.width() as i32 - side) / 2,
        cy - side / 2,
        side as u32,
        side as u32,
    );
    if let Some(reader) = images.get(&CD_READER_IMG) {
        let side_reader = (side as f32 * 1.06).round() as i32;
        let rcx = disc.x() + disc.width() as i32 / 2;
        let rcy = disc.y() + disc.height() as i32 / 2;
        let ring_r = (side_reader as f32 / 2.0 + side as f32 * 0.035) as i32;
        fill_circle(canvas, PSX_SHELL_EDGE, rcx, rcy, ring_r);
        fill_circle(canvas, PSX_SHELL, rcx, rcy, ring_r - 2);
        let rr = Rect::new(
            rcx - side_reader / 2,
            rcy - side_reader / 2,
            side_reader as u32,
            side_reader as u32,
        );
        disc = rr;
        let _ = canvas.copy(&reader.tex, None, rr);
        if lid {
            if let Some(glass) = images.get(&LID_GLASS_IMG) {
                let _ = canvas.copy(&glass.tex, None, rr);
            }
        }
    }

    // O CARD ENTRANDO (plan revision: "deve entrar por cima na parte de
    // baixo e na parte de cima entrar por baixo — não pode vazar pra
    // fora, como se encaixasse mesmo"): o card SOBE de baixo da face,
    // desliza POR CIMA do lábio de baixo da boca, e o que alcança a borda
    // de CIMA da boca passa POR BAIXO da carcaça (some atrás dela, como
    // cartucho entrando). A LABEL fica de fora, pendurada abaixo do bay —
    // e é clicável (abre a biblioteca, onde vivem trocar e ejetar). Este
    // é o último bloco da função: o card corre por cima de tudo que já
    // foi desenhado, exceto a sombra da boca, que volta por cima dele.
    let mut card_label_hit: Option<Rect> = None;
    if let Some(name) = card_labels {
        let (t, _) = card_motion.unwrap_or((1.0, false));
        let t = t.clamp(0.0, 1.0);
        let card_w = ((mc1.width() as i32 - 4) as f32 * 0.72).round() as i32;
        let card_x = mc1.x() + (mc1.width() as i32 - card_w) / 2;
        let body_h = 86i32;
        // A label pendurada ABAIXO do pé da face (adesivo com o nome +
        // "Trocar/Ejetar" impresso no plástico): o vão aberto até a lista
        // de comandos é dela — encaixado, só a label fica de fora, com
        // altura de verdade (colada no pé da face, ela colapsava em 1 px
        // e o nome saía cortado numa fileira de pontos). 56 px: adesivo
        // folgado para o nome e o gesto impresso embaixo.
        let label_y = mc1.bottom() + 2;
        let label_h = 56i32;
        let lip_y = recess.y();
        let bottom_lim = label_y + label_h + 2;
        // encaixado (t=1): a boca tem que cortar o corpo ACIMA da copy
        // "MEMORY CARD" moldada (tinta nas linhas 59–76 do corpo) — nas
        // apertadas o box é mais alto (76 px), então o -19 do FHD deixava a
        // copy inteira aparecendo em tarja entre o rebaixo e o adesivo (plan
        // revision: "encaixe mais o memory card, nao deve mostrar apenas a
        // label e o botao de trocar e ejetar"). -42 px põe a linha de corte na
        // linha 81 do corpo: logo e copy somem sob a carcaça; de fora ficam o
        // rebaixo escuro da boca, a língua do plástico e a label inteira
        // (adesivo com o nome + "Trocar/Ejetar" impresso). O FHD fica no -19
        // de sempre (corte na linha 76 — plan revision: "na resolução FHD ou
        // maior nao mexer"). início (t=0): o card nasce DE BAIXO do console —
        // o corpo INTEIRO abaixo da banda visível, escondido — e SOBE, igual
        // ao cartucho do SNES entrando por baixo.
        let seated_y = mc1.bottom() - body_h - if tight { 42 } else { 19 };
        let travel = (bottom_lim - seated_y).max(1);
        let card_y = seated_y + ((1.0 - t) * travel as f32).round() as i32;
        let total_h = body_h + label_h;
        let round_rows = 8i32;
        let edge = (168, 164, 154);
        let face_c = (205, 201, 192);
        // O hit acompanha o card (a label viaja com o corpo na animação).
        card_label_hit = Some(Rect::new(
            card_x,
            card_y + body_h,
            card_w as u32,
            label_h as u32,
        ));

        let draw_card = |canvas: &mut WindowCanvas| {
            for row in 0..total_h {
                let ry = card_y + row;
                if ry < lip_y || ry >= bottom_lim {
                    // acima da linha de encaixe: já "entrou" (por baixo da
                    // carcaça); abaixo do pé da face: ainda não "saiu" —
                    // nenhuma das duas faixas desenha.
                    continue;
                }
                let inset = if row >= total_h - round_rows {
                    (round_rows - (total_h - 1 - row)).min(round_rows)
                } else {
                    0
                };
                let w = card_w - inset * 2;
                fill(canvas, edge, Rect::new(card_x + inset, ry, w as u32, 1));
                if w > 2 {
                    fill(
                        canvas,
                        if row < body_h {
                            face_c
                        } else {
                            (200, 196, 188)
                        },
                        Rect::new(card_x + inset + 1, ry, (w - 2) as u32, 1),
                    );
                }
            }
            if card_y + 2 >= lip_y && card_y + 4 <= bottom_lim {
                fill_rect_rgba(
                    canvas,
                    (255, 255, 255, 60),
                    Rect::new(card_x + 2, card_y + 2, card_w as u32 - 4, 2),
                );
            }
            // as ranhuras do MC real nas laterais do corpo
            for gy in (card_y + 6..card_y + body_h - 6).step_by(3) {
                if gy < lip_y || gy + 2 > bottom_lim {
                    continue;
                }
                for (sx, dir) in [(card_x + 3, 1i32), (card_x + card_w - 10, -1i32)] {
                    fill_rect_rgba(canvas, (0, 0, 0, 55), Rect::new(sx + dir, gy, 1, 2));
                    fill_rect_rgba(
                        canvas,
                        (255, 255, 255, 45),
                        Rect::new(sx + dir + 1, gy, 1, 2),
                    );
                }
            }
            // a logo PolyStn em PRETO no corpo (cortada nas duas linhas),
            // na proporção da arte: 40 px de altura, centrada.
            if let Some(logo) = images.get(&MC_LOGO_IMG) {
                let lh = 40;
                let lw = (lh as f32 * logo.w as f32 / logo.h.max(1) as f32).round() as i32;
                draw_image_absolute_band(
                    canvas,
                    images,
                    MC_LOGO_IMG,
                    card_x + (card_w - lw) / 2,
                    card_y + 12,
                    lw as u32,
                    lh as u32,
                    lip_y,
                    bottom_lim,
                );
            }
            // "MEMORY CARD" — a identificação do corpo: 18 px abaixo da logo
            // (colava nela) e ALTO o bastante para sumir inteira sob a
            // carcaça com o card encaixado (a boca corta na linha 76 do
            // corpo; a copy termina na 75) — só aparece na animação.
            let mc_text = "MEMORY CARD";
            let mc_px = label::fit_px(mc_text, 13.0, card_w as f32 - 12.0);
            label::draw_text_vec(
                canvas,
                mc_text,
                (card_x + card_w / 2, card_y + 62 + 9),
                label::Anchor::Center,
                mc_px,
                (40, 38, 35),
                (lip_y, bottom_lim),
            );
            // a faixa que separa o corpo da label
            let band_y = card_y + body_h - 2;
            if band_y >= lip_y && band_y + 3 <= bottom_lim {
                fill(
                    canvas,
                    (150, 146, 137),
                    Rect::new(card_x + 1, band_y, card_w as u32 - 2, 3),
                );
            }
            // O ADESIVO caseiro com o nome (papel creme tortinho)
            let shown: Cow<str> = Cow::Owned(name.chars().take(16).collect());
            let stick_w = (card_w - 16).max(1);
            let stick_h = (label_h - 26).max(1);
            let stick_x = card_x + 9;
            let stick_y = card_y + body_h + 4;
            // cada pedaço do adesivo cortado na banda visível (a label
            // desce com o card — não pode invadir a lista de comandos)
            let fill_banded = |canvas: &mut WindowCanvas, color: (u8, u8, u8, u8), r: Rect| {
                let y0 = r.y().max(lip_y);
                let y1 = (r.y() + r.height() as i32).min(bottom_lim);
                if y1 > y0 {
                    fill_rect_rgba(
                        canvas,
                        color,
                        Rect::new(r.x(), y0, r.width(), (y1 - y0) as u32),
                    );
                }
            };
            fill_banded(
                canvas,
                (0, 0, 0, 60),
                Rect::new(stick_x + 2, stick_y + 2, stick_w as u32 - 2, stick_h as u32),
            );
            fill_banded(
                canvas,
                (232, 226, 204, 255),
                Rect::new(stick_x, stick_y, stick_w as u32, stick_h as u32),
            );
            fill_banded(
                canvas,
                (205, 201, 192, 255),
                Rect::new(stick_x, stick_y, 3, 3),
            );
            fill_banded(
                canvas,
                (205, 201, 192, 255),
                Rect::new(stick_x + stick_w - 3, stick_y + stick_h - 3, 3, 3),
            );
            fill_banded(
                canvas,
                (120, 112, 90, 160),
                Rect::new(stick_x, stick_y, stick_w as u32, 1),
            );
            fill_banded(
                canvas,
                (120, 112, 90, 120),
                Rect::new(stick_x, stick_y + stick_h - 1, stick_w as u32, 1),
            );
            let name_px = label::fit_px(&shown, 15.0, stick_w as f32 - 12.0);
            label::draw_text_vec(
                canvas,
                &shown,
                (card_x + card_w / 2, stick_y + stick_h / 2),
                label::Anchor::Center,
                name_px,
                (52, 46, 34),
                (lip_y, bottom_lim),
            );
            // "Trocar/Ejetar" IMPRESSO no plástico do card (na faixa entre
            // o adesivo e a base) — em relevo escuro, sem caixa preta.
            let act = "Trocar/Ejetar";
            let act_px = label::fit_px(act, 11.0, card_w as f32 - 14.0);
            let act_cy = card_y + total_h - 20 + 7;
            let band = (lip_y, bottom_lim);
            let ctr = label::Anchor::Center;
            let cx = card_x + card_w / 2;
            label::draw_text_vec(
                canvas,
                act,
                (cx, act_cy + 1),
                ctr,
                act_px,
                (222, 218, 208),
                band,
            );
            label::draw_text_vec(canvas, act, (cx, act_cy), ctr, act_px, (96, 92, 84), band);
        };

        // O card desenha POR CIMA de tudo que já está na cena (o bloco é o
        // último da função) — e o cull no lip_y faz a parte que "entrou"
        // ficar por baixo da carcaça. Falta só a sombra da boca de volta
        // POR CIMA do trecho dentro da abertura — o card está DENTRO do
        // slot, não colado na frente. Os retângulos são a própria boca, já
        // limitados; sem clip de renderizador (o SDL3.4 do cocoa engole).
        draw_card(canvas);
        fill(
            canvas,
            (12, 11, 10),
            Rect::new(recess.x(), recess.y(), recess.width(), 1),
        );
        fill_rect_rgba(
            canvas,
            (0, 0, 0, 150),
            Rect::new(recess.x(), recess.y(), recess.width(), 5),
        );
        fill_rect_rgba(
            canvas,
            (0, 0, 0, 70),
            Rect::new(recess.x(), recess.y() + 5, recess.width(), 5),
        );
    }

    let hits = FaceHits {
        power: power_hit,
        eject: eject_hit,
        reset: reset_hit,
        // O hit do tray é o bay INTEIRO, vazio ou encaixado — e também a
        // label pendurada ("Trocar/Ejetar"): um gesto só, abrir a
        // biblioteca (plan revision: "o botão de trocar e ejetar não abre
        // nada" — com card encaixado nenhum caminho de UI voltava à
        // biblioteca; ejetar é ação do card escolhido, "Ejetar do slot").
        cards1: mc1,
        card_btn: card_label_hit,
        analog1: analog_rect,
    };
    (disc, hits)
}

/// Draw the panel's console block: o PSX com o disco do jogo entrando pela
/// fenda da tampa. `t`/`ejecting` come from `PanelInfo::cartridge_motion`
/// (`None` outside the animation means seated — tampa fechada, disco
/// inteiro escondido dentro do console, `SEAT_HIDDEN_FRAC` > 1 garante o
/// clip cem por cento opaco). Requires `PANEL_CARTRIDGE_IMG` to be loaded —
/// `draw_panel` only calls this under `has_cartridge`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_panel_slot(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    block: Rect,
    t: f32,
    ejecting: bool,
    led_on: bool,
    reset_pressed: bool,
    spin_deg: f32,
    lid_open: bool,
    card_labels: Option<&str>,
    card_motion: Option<(f32, bool)>,
    analog: bool,
) -> FaceHits {
    let (disc, hits) = draw_slot_furniture(
        canvas,
        images,
        block,
        led_on,
        reset_pressed,
        lid_open,
        card_labels,
        card_motion,
        analog,
        false,
    );

    // A mesa do spindle: onde o CD assenta. GIRA com o MESMO ângulo do
    // disco (mesmo eixo, mesmo quad rotacionado) — aparece pelo furo
    // central do CD e durante a animação de ejeção; sem disco, fica parada
    // no último ângulo do giro.
    if let Some(seat) = images.get(&CD_SEAT_IMG) {
        // A mesa é o mesmo recorte da arte do leitor (mesma escala: placa a
        // 0.112 do lado do vão) — girada ela cobre a versão estática que veio
        // na textura do leitor, sem emenda.
        let side_seat = (disc.width().min(disc.height()) as f32) * 1.08 * 0.45;
        let scx = disc.x() as f32 + disc.width() as f32 / 2.0;
        let scy = disc.y() as f32 + disc.height() as f32 / 2.0;
        let shw = side_seat / 2.0;
        let sa = spin_deg.to_radians();
        let (ss, sc) = (sa.sin(), sa.cos());
        let white = sdl3::pixels::FColor::WHITE;
        let seat_corner = |sx: f32, sy: f32| -> Vertex {
            let (lx, ly) = (sx * shw, sy * shw);
            Vertex {
                position: sdl3::render::FPoint::new(
                    scx + lx * sc - ly * ss,
                    scy + lx * ss + ly * sc,
                ),
                color: white,
                tex_coord: sdl3::render::FPoint::new((sx + 1.0) / 2.0, (sy + 1.0) / 2.0),
            }
        };
        let seat_verts = [
            seat_corner(-1.0, -1.0),
            seat_corner(1.0, -1.0),
            seat_corner(1.0, 1.0),
            seat_corner(-1.0, 1.0),
        ];
        let _ = canvas.render_geometry(&seat_verts, Some(&seat.tex), &[0, 1, 2, 0, 2, 3]);
    }

    // O disco: desce de cima, ENCAIXA no centro da área livre (entre os
    // botões) e, com o console ligado, GIRA (a arte rotaciona em torno do
    // eixo). Menor que o bloco: cabe no espaço entre Power/Reset e Eject.
    if let Some(art) = images.get(&PANEL_CARTRIDGE_IMG) {
        let t = t.clamp(0.0, 1.0);
        let e = if ejecting {
            1.0 - (1.0 - t) * (1.0 - t)
        } else {
            t * t * (3.0 - 2.0 * t)
        };
        let p = if ejecting { 1.0 - e } else { e };
        // Assentado: centrado no poço do leitor.
        // O disco é redondo dentro do quad da textura: girado, ele varre o
        // MESMO círculo. E é maior que o poço (1.08×): a borda cobre o aro
        // e a beira da foto — o disco preenche o poço inteiro, sem aro
        // cinza sobrando em volta. O tamanho do vão já nasce com folga dos
        // botões e das portas — vê draw_slot_furniture.
        let side = (disc.width().min(disc.height()) as f32) * 1.08;
        let hub_cx = disc.x() as f32 + disc.width() as f32 / 2.0;
        let hub_cy = disc.y() as f32 + disc.height() as f32 / 2.0;
        let hw = side / 2.0;
        let enter_cy = block.y() as f32 - side / 2.0;
        // O centro do disco desce do topo até o eixo e ENCAIXA nele.
        let ccx = hub_cx;
        let ccy = enter_cy + (hub_cy - enter_cy) * p;
        // O ângulo é SEMPRE o acumulado pelo relógio — o disco para onde
        // parou (a rampa do motor é quem manda; ver o tick de spin) e não
        // volta para a posição original quando o leitor desliga.
        let angle = spin_deg;
        let s = angle.to_radians().sin();
        let c = angle.to_radians().cos();
        let white = sdl3::pixels::FColor::WHITE;
        let corner = |sx: f32, sy: f32| -> Vertex {
            let (lx, ly) = (sx * hw, sy * hw);
            Vertex {
                position: sdl3::render::FPoint::new(ccx + lx * c - ly * s, ccy + lx * s + ly * c),
                color: white,
                tex_coord: sdl3::render::FPoint::new((sx + 1.0) / 2.0, (sy + 1.0) / 2.0),
            }
        };
        let verts = [
            corner(-1.0, -1.0),
            corner(1.0, -1.0),
            corner(1.0, 1.0),
            corner(-1.0, 1.0),
        ];
        // Totalmente fora (p=0, remoção animada): o quad ficaria com o
        // centro acima do topo do bloco, invadindo o que tem ali — não
        // desenha. A remoção deixa o estado em (1.0, ejecting) de propósito.
        if p > 0.0 {
            let _ = canvas.render_geometry(&verts, Some(&art.tex), &[0, 1, 2, 0, 2, 3]);
        }

        // A tampa translúcida (a "tampa de acrílico" por cima do disco
        // inserido, como no console): só quando assentado e com a tampa
        // FECHADA — abrir o OPEN remove o vidro. Reflexos pré-renderizados
        // com gradiente (textura RGBA com blend) — os círculos chapados
        // antigos ficavam estranhos sobre a arte.
        if p >= 1.0 && !ejecting && !lid_open {
            if let Some(glass) = images.get(&LID_GLASS_IMG) {
                let _ = canvas.copy(&glass.tex, None, disc);
            }
        }
        // Tampa ABERTA: o acrílico gira para cima na dobradiça (a borda de
        // cima do vão) e aparece de perfil, achatado e espelhado, com o
        // disco liberado embaixo — a mesma textura do vidro, sem o disco
        // coberto.
        if p >= 1.0 && !ejecting && lid_open {
            if let Some(glass) = images.get(&LID_GLASS_IMG) {
                let fw = (disc.width() as f32 * 0.9) as u32;
                let fh = (disc.height() as f32 * 0.34) as u32;
                let fx = disc.x() + (disc.width() as i32 - fw as i32) / 2;
                let fy = disc.y() + 2 - fh as i32;
                let flap = Rect::new(fx, fy, fw, fh);
                let _ = canvas.copy_ex(&glass.tex, None, flap, 0.0, None, false, true);
                // A dobradiça: um traço escuro na base da tampa levantada.
                fill_rect_rgba(
                    canvas,
                    (40, 42, 48, 220),
                    Rect::new(fx + fw as i32 / 6, disc.y() + 1, fw * 2 / 3, 3),
                );
            }
        }
    }
    hits
}
