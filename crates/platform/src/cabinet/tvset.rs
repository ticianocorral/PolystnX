//! A TV do gabinete: botões físicos do queixo (POWER, VOL-/VOL+), o estado
//! ligada/desligada com a animação do tubo (aquece, colapsa, vidro morto),
//! o volume e o OSD de barrinhas. Portado do SixteeN — sem o seletor de
//! canal: o PolystnX entra pela entrada AV, então não há CH.

use super::*;

/// O prateado das TVs antigas — a gravação dos botões do queixo.
pub(super) const PRATA_HI: (u8, u8, u8) = (234, 236, 242);
pub(super) const PRATA_INK_DIM: (u8, u8, u8) = (168, 170, 178);
/// O LED verde do POWER, aceso só com a TV ligada.
const LED_TV_ON: (u8, u8, u8) = (64, 220, 96);
const LED_TV_ON_HI: (u8, u8, u8) = (170, 255, 190);
const LED_OFF: (u8, u8, u8) = (56, 26, 26);

/// O menu OSD fecha sozinho depois disso sem mexer.
const MENU_IDLE: Duration = Duration::from_secs(6);
/// Passo de cada toque de VOL-/VOL+ no menu (escala 0..100).
const MENU_STEP: u8 = 5;

/// O comprimento das fases do tubo em frames (60fps: o warm-up leva ~2s
/// como um tubo de verdade, o colapso é rápido — ~0.4s).
const TV_WARM_FRAMES: u32 = 90;
const TV_COLLAPSE_FRAMES: u32 = 30;

/// Um controle físico do queixo da TV (ver `Cabinet::hit_tv_controls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvControl {
    Power,
    VolUp,
    VolDown,
    /// O menu OSD de imagem (brilho, contraste, cor, nitidez).
    Menu,
}

/// Os rects clicáveis do queixo da TV, registrados a cada frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TvChinControls {
    pub vol_down: Rect,
    pub vol_up: Rect,
    pub power: Rect,
    pub menu: Rect,
}

impl TvChinControls {
    /// Todos fora da tela — o "sem controles" (queixo curto demais).
    pub fn offscreen() -> TvChinControls {
        let no = Rect::new(-100, -100, 1, 1);
        TvChinControls {
            vol_down: no,
            vol_up: no,
            power: no,
            menu: no,
        }
    }
}

/// O estado da animação da TV ligando/desligando (`None` = ligada).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvPower {
    /// Ligando: um instante de tubo escuro e a imagem sobe do preto,
    /// ganhando brilho (sem janela abrindo).
    Warming { frames: u32 },
    /// Desligando: a imagem esmaga numa linha clara, a linha encolhe num
    /// ponto e o ponto some devagar (a persistência do fósforo).
    Collapsing { frames: u32 },
    /// Desligada: vidro quase preto com reflexo, controles do queixo vivos.
    Off,
}

/// O que a fase do tubo manda desenhar NO VIDRO neste frame.
pub enum TvPhase {
    /// Ligada: o sinal que o caminho já desenha.
    On,
    /// Desligada: o vidro morto por cima de tudo.
    Dead,
    /// Desligando (`t` de 0 a 1): a imagem esmagada -> linha -> ponto.
    Collapse { t: f32 },
    /// Ligando (`t` de 0 a 1): o tubo escuro e a imagem subindo do preto.
    WarmUp { t: f32 },
}

/// O que o queixo precisa saber do estado da TV para se desenhar.
#[derive(Clone, Copy)]
pub(super) struct ChinLook {
    pub on: bool,
}

/// A geometria da fileira de controles do queixo.
struct ChinRow {
    key_h: i32,
    pw: i32,
    menu_w: i32,
    vol_w: i32,
}

impl ChinRow {
    fn new(chin_h: i32) -> Self {
        let key_h = (chin_h as f32 * 0.34).round() as i32;
        let pw = ((key_h + 12) as f32 * 0.58).round() as i32;
        let key_w = (GLYPH_W as i32 * 3).max(38);
        let menu_w = GLYPH_W as i32 * 4 + 14;
        let vol_w = GLYPH_W as i32 * 3 + 8 + 6 + key_w * 2;
        ChinRow {
            key_h,
            pw,
            menu_w,
            vol_w,
        }
    }
}

/// Até onde os avisos do queixo (versão, RA, OSD) podem ir pela direita:
/// a margem de sempre, ou antes da fileira de controles da TV — o que vier
/// primeiro. Sem controles (queixo curto), só a margem.
pub(super) fn chin_right(screen: Rect, out_h: u32, margin: i32) -> i32 {
    let right = screen.right() - margin;
    let chin_h = out_h as i32 - screen.bottom();
    if chin_h < 40 {
        return right;
    }
    let g = ChinRow::new(chin_h);
    if g.key_h < 14 {
        return right;
    }
    let pcx = screen.right() - g.pw / 2 - 26;
    let controls_left = pcx - g.pw / 2 - 14 - g.menu_w - 18 - g.vol_w;
    right.min(controls_left - 16)
}

/// A marca "PolyStnX" prateada (arte pronta, RGBA vazado, gerada por
/// `examples/gen_brand.rs`) — embutida e registrada no mapa de imagens no
/// primeiro frame; o queixo a desenha no meio, sob o vidro.
const BRAND_LOGO_ID: u64 = u64::MAX - 20;
const BRAND_LOGO_W: u32 = 522;
const BRAND_LOGO_H: u32 = 158;
const BRAND_LOGO_RGBA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/assets/brand_polystnx.rgba"
));
/// Espaço que o selo do RA e o OSD do queixo (da direita, até antes dos
/// controles) podem querer: com eles na tela a marca cede.
const BRAND_RESERVE_RA: i32 = 250;

/// Onde a marca prateada mora no queixo: centralizada sob o vidro, ou
/// empurrada para a esquerda quando a fileira de controles (e o espaço
/// reservado à direita) chega até ela — e fora (`None`) se nem assim couber
/// sem pisar na versão impressa no canto esquerdo.
fn brand_logo_rect(screen: Rect, out_h: u32, reserve_right: i32) -> Option<Rect> {
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 40 {
        return None;
    }
    let ratio = BRAND_LOGO_W as f32 / BRAND_LOGO_H as f32;
    let bh = (chin_h as f32 * 0.42).round() as i32;
    let bw = (bh as f32 * ratio).round() as i32;
    let mut x = screen.left() + (screen.width() as i32 - bw) / 2;
    let row = ChinRow::new(chin_h);
    if row.key_h >= 14 {
        let pcx = screen.right() - row.pw / 2 - 26;
        let controls_left = pcx - row.pw / 2 - 14 - row.menu_w - 18 - row.vol_w;
        x = x.min(controls_left - 16 - reserve_right - bw);
    }
    // A versão do app e do núcleo, no canto esquerdo do queixo.
    let version_right = screen.left() + GLYPH_W as i32 * 22;
    (x >= version_right).then(|| Rect::new(x, chin_top + (chin_h - bh) / 2, bw as u32, bh as u32))
}

/// Os controles físicos do queixo: POWER (retângulo vertical com o símbolo
/// ⏻ e o LED verde ao lado) e o par VOL-/VOL+ com o rótulo gravado. Só
/// controles — o plástico do queixo já vem do bezel.
pub(super) fn draw_tv_chin_controls(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    look: ChinLook,
) -> TvChinControls {
    let on = look.on;
    // O queixo é a faixa entre o fim do vidro e o fim do canvas.
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 40 {
        return TvChinControls::offscreen(); // queixo curto demais: sem controles
    }
    let g = ChinRow::new(chin_h);
    let key_h = g.key_h;
    if key_h < 14 {
        return TvChinControls::offscreen();
    }
    let key_w = (GLYPH_W as i32 * 3).max(38);
    let pair_gap = 6;
    let group_gap = 18;
    let cy = chin_top + chin_h / 2;
    let ink = if on { PRATA_HI } else { PRATA_INK_DIM };

    // POWER: retângulo vertical com o símbolo ⏻ (aro grosso com vão no
    // topo + traço com ponta redonda), LED verde do ladinho.
    let ph = key_h + 12;
    let pw = g.pw;
    let pcx = screen.right() - pw / 2 - 26;
    let pcy = cy;
    let prect = Rect::new(pcx - pw / 2, pcy - ph / 2, pw as u32, ph as u32);
    draw_dark_key(canvas, prect, on);
    let r_out = pw as f32 * 0.30;
    let r_in = r_out - 2.5;
    let pc_y = pcy + 2;
    canvas.set_draw_color(Color::RGB(ink.0, ink.1, ink.2));
    let gap = std::f32::consts::PI / 5.0;
    let steps = 40;
    for i in 0..=steps {
        let a = gap + (std::f32::consts::TAU - 2.0 * gap) * (i as f32 / steps as f32);
        for rr in [r_in, (r_in + r_out) / 2.0, r_out] {
            let x = pcx as f32 + a.sin() * rr;
            let y = pc_y as f32 - a.cos() * rr;
            let _ = canvas.fill_rect(Rect::new(x.round() as i32 - 1, y.round() as i32, 2, 2));
        }
    }
    let stem_top = (pc_y as f32 - r_out - 3.0) as i32;
    let stem_h = (r_out + 4.0) as i32;
    let _ = canvas.fill_rect(Rect::new(pcx - 1, stem_top, 2, stem_h as u32));
    let _ = canvas.fill_rect(Rect::new(pcx - 2, stem_top + stem_h - 1, 3, 1));

    // LED verde do ladinho, aceso só com a TV ligada.
    let led_cx = screen.right() - 12;
    if on {
        draw_led_color(canvas, led_cx, cy - 6, 12, true, LED_TV_ON, LED_TV_ON_HI);
    }

    // MENU, colado à esquerda do POWER: a porta para o menu de imagem.
    let menu = Rect::new(
        pcx - pw / 2 - 14 - g.menu_w,
        cy - key_h / 2,
        g.menu_w as u32,
        key_h as u32,
    );
    draw_dark_key(canvas, menu, on);
    draw_text_absolute(
        canvas,
        font,
        menu.x() + 7,
        menu.y() + (menu.height() as i32 - GLYPH_H as i32) / 2,
        TextStyle::new(1.0, ink),
        "MENU",
        usize::MAX,
    );

    // O par do volume, com o rótulo inline — - e + em prata.
    let left_x = menu.x() - group_gap - g.vol_w;
    let label = "VOL";
    let lw = GLYPH_W as i32 * label.chars().count() as i32;
    draw_text_absolute(
        canvas,
        font,
        left_x,
        cy - GLYPH_H as i32 / 2,
        TextStyle::new(1.0, ink),
        label,
        usize::MAX,
    );
    let vol_down = Rect::new(left_x + lw + 8, cy - key_h / 2, key_w as u32, key_h as u32);
    let vol_up = Rect::new(
        vol_down.x() + pair_gap + key_w,
        vol_down.y(),
        key_w as u32,
        key_h as u32,
    );
    for (r, glyph) in [(vol_down, "-"), (vol_up, "+")] {
        draw_dark_key(canvas, r, on);
        let tw = GLYPH_W as i32 * glyph.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            r.x() + (r.width() as i32 - tw) / 2,
            r.y() + (r.height() as i32 - GLYPH_H as i32) / 2,
            TextStyle::new(1.0, ink),
            glyph,
            usize::MAX,
        );
    }

    TvChinControls {
        vol_down,
        vol_up,
        power: prect,
        menu,
    }
}

/// Um disco com alpha em coordenadas absolutas — o halo do afterglow do
/// tubo (usado pelo vidro da fase, estático e gameplay).
pub(super) fn disco_alpha(
    canvas: &mut WindowCanvas,
    cx: i32,
    cy: i32,
    r: i32,
    c: (u8, u8, u8, u8),
) {
    canvas.set_draw_color(Color::RGBA(c.0, c.1, c.2, c.3));
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - dx, cy + dy, (dx * 2) as u32, 1));
    }
}

/// O alpha do reflexo diagonal na faixa `e` (0..26) — a mesma escada de
/// [`reflection_alpha`].
fn reflection_alpha(e: i32, base: u8) -> u8 {
    match e {
        0..=3 => base,
        4..=18 => base / 2,
        _ => base / 4,
    }
}

/// O vidro morto (`TvPhase::Dead`) já composto em RGBA32 opaco, `w`x`h`:
/// as linhas do gradiente quase preto e o reflexo diagonal misturado por
/// cima (`src*a + dst*(1-a)`, arredondado como a GPU faz). Pixel a pixel
/// o que o desenho direto fazia com ~600 `fill_rect` + milhares
/// de retângulos 1x1 por quadro — o frame vivo sobe isto UMA vez numa
/// textura ([`DeadGlass`]) e só copia.
pub(super) fn dead_glass_rgba(w: u32, h: u32) -> Vec<u8> {
    let (sw, sh) = (w as i32, h as i32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for row in 0..sh {
        let t = row as f32 / sh as f32;
        let v = 5.0 + 3.0 * t;
        let c = [v as u8, v as u8, (v + 1.0) as u8, 255];
        let base = (row * sw * 4) as usize;
        for p in px[base..base + (sw * 4) as usize].as_chunks_mut::<4>().0 {
            p.copy_from_slice(&c);
        }
    }
    // O reflexo: cada (i, e) cai num pixel distinto (x fixa i, y fixa e),
    // então nenhum pixel recebe duas misturas — como no desenho direto.
    let len = (sw / 3).min(sh / 2);
    let esp = 26i32;
    for i in 0..len {
        for e in 0..esp {
            let (x, y) = (i, sh / 4 + esp - i - e);
            if x < 0 || x >= sw || y < 0 || y >= sh {
                continue;
            }
            let a = reflection_alpha(e, 5) as u32;
            if a == 0 {
                continue;
            }
            let o = ((y * sw + x) * 4) as usize;
            for ch in &mut px[o..o + 3] {
                let d = *ch as u32;
                *ch = ((255 * a + d * (255 - a) + 127) / 255) as u8;
            }
        }
    }
    px
}

/// O vidro morto pronto numa textura, refeita só quando o tamanho do tubo
/// muda — é tudo de que ele depende (cores e reflexo são constantes; a
/// posição entra só no `copy`). Textura estática, não render target:
/// sobrevive a um reset dos alvos do renderer.
pub(super) struct DeadGlass {
    pub size: (u32, u32),
    pub tex: Texture,
}

/// Uma tecla de plástico escuro do queixo — o mesmo plástico da saia do
/// seletor de canal: 1px de assento quase preto em volta, a face caindo
/// do cinza de cima para o quase preto de baixo, um fio de luz no lábio
/// de cima e o de baixo afundado. O rótulo (prateado) é do caller.
pub(super) fn draw_dark_key(canvas: &mut WindowCanvas, rect: Rect, on: bool) {
    let w = rect.width() as i32;
    let h = rect.height() as i32;
    let (x, y) = (rect.x(), rect.y());
    canvas.set_draw_color(Color::RGB(10, 10, 12));
    let _ = canvas.fill_rect(rect);
    let (top, bot, lip) = if on {
        ((88, 89, 96), (30, 30, 34), (128, 130, 138))
    } else {
        ((60, 61, 68), (20, 20, 23), (84, 86, 94))
    };
    for row in 1..h - 1 {
        let inset = 1 + corner_inset(row - 1, h - 2, 2);
        let t = row - 1; // 0..h-2 dentro da face
        let c = if t == 0 {
            lip
        } else if t >= h - 3 {
            (16, 16, 19)
        } else {
            lerp_rgb(top, bot, t as f32 / (h - 2) as f32)
        };
        canvas.set_draw_color(Color::RGB(c.0, c.1, c.2));
        let _ = canvas.fill_rect(Rect::new(x + inset, y + row, (w - 2 * inset) as u32, 1));
    }
}

/// Horizontal inset of scanline `row` in a bar `h` rows tall with corner
/// radius `radius` — quarter-circle steps, the same fake-a-circle trick
/// `draw_led` uses, stacked into whole rows here. Clamped so a bar can never
/// be narrower than its own radius.
pub(super) fn corner_inset(row: i32, h: i32, radius: i32) -> i32 {
    let r = radius.min(h / 2).max(0);
    let from_end = row.min(h - 1 - row);
    if from_end >= r {
        return 0;
    }
    let dx = r - from_end;
    r - ((r * r - dx * dx) as f32).sqrt().floor() as i32
}

/// O mesmo LED com cor própria — o queixo da TV usa o verde da foto do
/// painel real, o console continua no vermelho.
pub(super) fn draw_led_color(
    canvas: &mut WindowCanvas,
    center_x: i32,
    top: i32,
    size: i32,
    lit: bool,
    on: (u8, u8, u8),
    on_hi: (u8, u8, u8),
) {
    let (r, g, b) = if lit { on } else { LED_OFF };
    canvas.set_draw_color(Color::RGB(r, g, b));
    // Four rows, widening then narrowing (roughly 60/90/90/60% of `size`) —
    // a softer step than a plain narrow/wide/narrow, so it reads as a round
    // dot instead of a plus sign at this small a scale.
    let step = (size / 4).max(1);
    let widths = [size * 3 / 5, size * 9 / 10, size * 9 / 10, size * 3 / 5];
    for (i, w) in widths.into_iter().enumerate() {
        let w = w.max(2);
        let h = if i == widths.len() - 1 {
            (size - step * i as i32).max(1)
        } else {
            step
        };
        let _ = canvas.fill_rect(Rect::new(
            center_x - w / 2,
            top + step * i as i32,
            w as u32,
            h as u32,
        ));
    }
    if lit {
        canvas.set_draw_color(Color::RGB(on_hi.0, on_hi.1, on_hi.2));
        let hi = (size / 4).max(1) as u32;
        let hi_x = center_x - (size * 3 / 10);
        let _ = canvas.fill_rect(Rect::new(hi_x, top + step, hi, hi));
    }
}

fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

/// O menu de imagem dos anos 90: letras verdes grandes com sombra, o item
/// em ajuste aceso e os outros apagados, cada um com a barrinha de
/// segmentos e o número.
pub(super) fn draw_picture_menu(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    picture: Picture,
    selected: usize,
) {
    let scale = if screen.height() >= 520 { 2 } else { 1 };
    let cell_w = (GLYPH_W * scale) as i32;
    let row = (GLYPH_H * scale) as i32 + 10;
    let x = screen.x() + 40;
    let mut y = screen.y() + 40;
    let dim = (36, 150, 48);
    let shadow = (12, 14, 12);
    let text = |canvas: &mut WindowCanvas, font: &mut Texture, x: i32, y: i32, c, s: &str| {
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(scale as f32, shadow),
            s,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(scale as f32, c),
            s,
            usize::MAX,
        );
    };
    text(canvas, font, x, y, OSD_GREEN, "IMAGEM");
    y += row + 6;
    let label_cols = 11;
    let segs = 20;
    let seg_w = (6 * scale) as i32;
    let seg_h = (GLYPH_H * scale) as i32 / 2;
    for item in 0..Picture::ITEMS {
        let on = item == selected;
        let c = if on { OSD_GREEN } else { dim };
        let mark = if on { ">" } else { " " };
        text(
            canvas,
            font,
            x,
            y,
            c,
            &format!("{mark}{}", Picture::label(item)),
        );
        let v = picture.get(item);
        let bx = x + cell_w * (label_cols + 1);
        let by = y + ((GLYPH_H * scale) as i32 - seg_h) / 2;
        let lit = (v as i32 * segs + 50) / 100;
        for s in 0..segs {
            let sx = bx + s * (seg_w + 2);
            canvas.set_draw_color(Color::RGB(shadow.0, shadow.1, shadow.2));
            let _ = canvas.fill_rect(Rect::new(sx + 2, by + 2, seg_w as u32, seg_h as u32));
            let col = if s < lit { c } else { (20, 34, 22) };
            canvas.set_draw_color(Color::RGB(col.0, col.1, col.2));
            let _ = canvas.fill_rect(Rect::new(sx, by, seg_w as u32, seg_h as u32));
        }
        text(
            canvas,
            font,
            bx + segs * (seg_w + 2) + 10,
            y,
            c,
            &format!("{v:3}"),
        );
        y += row;
    }
    y += 6;
    let hint = "MENU: próximo   VOL -/+: ajusta";
    draw_text_absolute(
        canvas,
        font,
        x + 1,
        y + 1,
        TextStyle::new(1.0, shadow),
        hint,
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(1.0, dim),
        hint,
        usize::MAX,
    );
}

impl Cabinet {
    /// A TV está ligada (ou virando)? `false` no `Off`/`Collapsing`.
    pub fn tv_on(&self) -> bool {
        !matches!(
            self.tv_power,
            Some(TvPower::Off) | Some(TvPower::Collapsing { .. })
        )
    }

    /// Uma transição do tubo em curso (aquecer/colapsar) — contada em
    /// quadros, então quem pace pelo ritmo ocioso segura os 60 Hz até ela
    /// acabar.
    pub(super) fn tv_transitioning(&self) -> bool {
        matches!(
            self.tv_power,
            Some(TvPower::Warming { .. } | TvPower::Collapsing { .. })
        )
    }

    /// Ligar/desligar: dispara o warm-up (ponto -> linha -> raster abrindo,
    /// ~2s) ou o colapso (imagem -> linha -> ponto -> afterglow, ~0.4s).
    /// Chamar de novo no meio de uma animação reinicia a fase oposta.
    pub fn toggle_tv_power(&mut self) {
        self.tv_power = Some(match self.tv_power {
            Some(TvPower::Off) | Some(TvPower::Collapsing { .. }) => TvPower::Warming { frames: 0 },
            None | Some(TvPower::Warming { .. }) => TvPower::Collapsing { frames: 0 },
        });
    }

    /// Volume da TV, 0..=16 — o jogo multiplica a amostra dele por isso.
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Ajusta o volume (VOL-/VOL+; cada clique 1 de 16) e acende o OSD de
    /// barrinhas por 2s.
    pub fn nudge_volume(&mut self, delta: i32) {
        let v = self.volume as i32 + delta;
        self.volume = v.clamp(0, 16) as u8;
        self.volume_osd_until = Some(Instant::now() + Duration::from_secs(2));
    }

    /// Um clique num controle do queixo da TV — o mesmo em toda tela.
    pub fn apply_tv_control(&mut self, control: TvControl) {
        match control {
            TvControl::Power => self.toggle_tv_power(),
            // Com o menu de imagem aberto, VOL-/VOL+ ajustam o item, como
            // nas TVs de botão no painel.
            TvControl::VolUp => {
                if !self.menu_adjust(1) {
                    self.nudge_volume(1)
                }
            }
            TvControl::VolDown => {
                if !self.menu_adjust(-1) {
                    self.nudge_volume(-1)
                }
            }
            TvControl::Menu => {
                if self.tv_on() {
                    self.menu_press();
                }
            }
        }
    }

    /// O controle do queixo sob `(ox, oy)` (coordenadas de output), se houver.
    pub fn hit_tv_controls(&self, ox: i32, oy: i32) -> Option<TvControl> {
        let c = self.tv_power_buttons?;
        let inside = |r: Rect| r.contains_point((ox, oy));
        if inside(c.power) {
            Some(TvControl::Power)
        } else if inside(c.menu) {
            Some(TvControl::Menu)
        } else if inside(c.vol_up) {
            Some(TvControl::VolUp)
        } else if inside(c.vol_down) {
            Some(TvControl::VolDown)
        } else {
            None
        }
    }

    /// Restaura o volume (0..=16) sem acender o OSD — o app chama ao abrir.
    pub fn set_volume(&mut self, v: u8) {
        self.volume = v.min(16);
    }

    /// Restaura os ajustes de imagem do menu — o app chama ao abrir.
    pub fn set_picture(&mut self, p: Picture) {
        self.picture = p;
    }

    /// O ajuste de imagem atual (o app não usa; o quadro do jogo é filtrado
    /// no próprio upload).
    pub fn picture(&self) -> Picture {
        self.picture
    }

    /// MENU: abre no primeiro item, passa para o próximo, e depois do
    /// último fecha.
    fn menu_press(&mut self) {
        let next = match self.tv_menu {
            None => Some(0),
            Some((i, _)) if i + 1 < Picture::ITEMS => Some(i + 1),
            Some(_) => None,
        };
        self.tv_menu = next.map(|i| (i, Instant::now()));
    }

    /// VOL-/VOL+ com o menu aberto ajustam o item; `false` = menu fechado
    /// (o toque é volume mesmo).
    fn menu_adjust(&mut self, delta: i32) -> bool {
        let Some((item, _)) = self.menu_open() else {
            return false;
        };
        let v = self.picture.get(item) as i32 + delta * MENU_STEP as i32;
        self.picture.set(item, v.clamp(0, 100) as u8);
        self.tv_menu = Some((item, Instant::now()));
        true
    }

    /// O menu aberto (item e último toque); fecha sozinho parado.
    fn menu_open(&mut self) -> Option<(usize, Instant)> {
        if self.tv_menu.is_some_and(|(_, t)| t.elapsed() >= MENU_IDLE) {
            self.tv_menu = None;
        }
        self.tv_menu
    }

    /// Aplica o clique se ele caiu num controle do queixo; `true` se caiu.
    /// O queixo não tem outro botão, então chamar isto antes dos demais
    /// hit-tests de uma tela é seguro.
    pub fn try_tv_click(&mut self, ox: i32, oy: i32) -> bool {
        match self.hit_tv_controls(ox, oy) {
            Some(c) => {
                self.apply_tv_control(c);
                true
            }
            None => false,
        }
    }

    /// Um frame do tubo: avança as fases (Warming/Collapsing contam em
    /// frames; chamar uma vez por frame apresentado) e devolve o que ESTE
    /// frame desenha no vidro.
    fn tv_phase(&mut self) -> TvPhase {
        match self.tv_power {
            Some(TvPower::Warming { frames }) => {
                if frames + 1 >= TV_WARM_FRAMES {
                    self.tv_power = None; // ligada
                } else {
                    self.tv_power = Some(TvPower::Warming { frames: frames + 1 });
                }
            }
            Some(TvPower::Collapsing { frames }) => {
                if frames + 1 >= TV_COLLAPSE_FRAMES {
                    self.tv_power = Some(TvPower::Off);
                    // O hiss morre com a TV (o stream fecha junto).
                    self.hiss = None;
                } else {
                    self.tv_power = Some(TvPower::Collapsing { frames: frames + 1 });
                }
            }
            _ => {}
        }
        self.tv_phase_snapshot()
    }

    /// A fase do frame ATUAL, sem avançar o relógio.
    fn tv_phase_snapshot(&self) -> TvPhase {
        match self.tv_power {
            Some(TvPower::Warming { frames }) => TvPhase::WarmUp {
                t: frames as f32 / TV_WARM_FRAMES as f32,
            },
            Some(TvPower::Collapsing { frames }) => TvPhase::Collapse {
                t: frames as f32 / TV_COLLAPSE_FRAMES as f32,
            },
            Some(TvPower::Off) => TvPhase::Dead,
            None => TvPhase::On,
        }
    }

    /// O vidro morto (a textura pronta, refeita só quando o tamanho do tubo
    /// muda) deformado pela malha do tubo e com `alpha` (0..=255) — 255 cobre
    /// a imagem por inteiro, menos deixa o que está por baixo aparecer.
    fn draw_dead_glass(&mut self, mesh: &CrtMesh, alpha: u8) {
        let screen = self.screen;
        let size = (screen.width(), screen.height());
        if self.dead_glass.as_ref().is_none_or(|g| g.size != size) {
            self.dead_glass = self.build_dead_glass(size);
        }
        let Some(g) = &self.dead_glass else {
            return;
        };
        if alpha == 255 {
            let _ = self
                .canvas
                .render_geometry(&mesh.verts, Some(&g.tex), &mesh.indices[..]);
            return;
        }
        // O alfa vai nas cores dos vértices (a textura entra com blend).
        let a = alpha as f32 / 255.0;
        let mut verts = mesh.verts.clone();
        for v in &mut verts {
            v.color.a = a;
        }
        let _ = self
            .canvas
            .render_geometry(&verts, Some(&g.tex), &mesh.indices[..]);
    }

    /// Fotografa o que o tubo mostra agora (a região do vidro, já com a
    /// imagem desenhada) para esmagá-la no desligar.
    fn snapshot_screen(&mut self) {
        let screen = self.screen;
        let Ok(surf) = self.canvas.read_pixels(Some(screen)) else {
            return;
        };
        let Ok(surf) = surf.convert_format(SdlFormat::RGBA32) else {
            return;
        };
        let (w, h, pitch) = (surf.width(), surf.height(), surf.pitch() as usize);
        let Ok(mut tex) = self.canvas.create_texture_static(SdlFormat::RGBA32, w, h) else {
            return;
        };
        if surf
            .with_lock(|bytes| tex.update(None, bytes, pitch))
            .is_err()
        {
            return;
        }
        tex.set_blend_mode(BlendMode::None);
        tex.set_scale_mode(SdlScaleMode::Linear);
        self.collapse_snap = Some(tex);
    }

    /// O desligar de uma TV de tubo do fim dos anos 90: a imagem é
    /// ESMAGADA na vertical até virar uma linha clara, a linha encolhe até
    /// um ponto e o ponto brilha e some devagar (a persistência do fósforo).
    /// Nada de janela fechando: é a própria imagem que se comprime, com o
    /// brilho estourando à medida que ela afina.
    fn draw_collapse(&mut self, t: f32, mesh: &CrtMesh) {
        if self.collapse_snap.is_none() {
            self.snapshot_screen();
        }
        // O tubo apaga por baixo; o que sobra é só o que a fase desenha.
        self.draw_dead_glass(mesh, 255);
        let screen = self.screen;
        let (sw, sh) = (screen.width() as f32, screen.height() as f32);
        let (cx, cy) = (
            screen.x() + screen.width() as i32 / 2,
            screen.y() + screen.height() as i32 / 2,
        );
        const SQUASH_END: f32 = 0.5; // até aqui: imagem -> linha
        const LINE_END: f32 = 0.82; // até aqui: linha -> ponto; depois, o brilho do ponto
                                    // Fatores de escala da imagem (vertical, horizontal).
        let (fy, fx) = if t < SQUASH_END {
            ((1.0 - t / SQUASH_END).powf(2.4).max(0.012), 1.0)
        } else if t < LINE_END {
            (
                0.012,
                (1.0 - ((t - SQUASH_END) / (LINE_END - SQUASH_END)).powf(1.4)).max(0.0),
            )
        } else {
            (0.0, 0.0)
        };
        if fx > 0.0 {
            let (w, h) = ((sw * fx).max(2.0), (sh * fy).max(2.0));
            let dst = Rect::new(
                cx - (w / 2.0) as i32,
                cy - (h / 2.0) as i32,
                w as u32,
                h as u32,
            );
            if let Some(snap) = self.collapse_snap.as_mut() {
                // A imagem comprimida; quanto mais fina, mais estourada.
                snap.set_blend_mode(BlendMode::None);
                snap.set_alpha_mod(255);
                let _ = self.canvas.copy(snap, None, dst);
                // Só estoura quando já está fina (abaixo de ~1/4 da altura):
                // a imagem grossa ainda aparece com as cores.
                let boost = (1.0 - fy / 0.25).clamp(0.0, 1.0);
                if boost > 0.0 {
                    snap.set_blend_mode(BlendMode::Add);
                    for pass in 0..2 {
                        snap.set_alpha_mod((boost * 190.0 * (1.0 - pass as f32 * 0.4)) as u8);
                        let _ = self.canvas.copy(snap, None, dst);
                    }
                }
            }
            // A linha clara que a imagem vira: núcleo branco e halo azulado.
            let line_a = ((1.0 - fy / 0.25).clamp(0.0, 1.0) * 255.0) as u8;
            self.canvas.set_blend_mode(BlendMode::Add);
            for (grow, a) in [(5, line_a / 5), (3, line_a / 3), (1, line_a)] {
                self.canvas.set_draw_color(Color::RGBA(190, 205, 255, a));
                let _ = self.canvas.fill_rect(Rect::new(
                    dst.x(),
                    dst.y() - grow,
                    dst.width(),
                    dst.height() + (grow * 2) as u32,
                ));
            }
            self.canvas.set_blend_mode(BlendMode::Blend);
        } else {
            // O ponto: brilho quente que apaga sem pressa.
            let k = ((t - LINE_END) / (1.0 - LINE_END)).clamp(0.0, 1.0);
            let glow = (1.0 - k).powf(1.6);
            self.canvas.set_blend_mode(BlendMode::Add);
            for r in (2..=14).rev() {
                let a = (glow * (14 - r) as f32 / 14.0 * 90.0) as u8;
                disco_alpha(
                    &mut self.canvas,
                    cx,
                    cy,
                    (r as f32 * 1.8) as i32,
                    (255, 240, 205, a),
                );
            }
            disco_alpha(
                &mut self.canvas,
                cx,
                cy,
                3,
                (255, 255, 250, (glow * 255.0) as u8),
            );
            self.canvas.set_blend_mode(BlendMode::Blend);
        }
    }

    /// O ligar: um instante de tubo escuro (o filamento esquentando) e a
    /// imagem sobe do preto ganhando brilho — sem janela abrindo. O vidro
    /// morto vai ficando transparente com uma curva que arranca devagar.
    fn draw_warmup(&mut self, t: f32, mesh: &CrtMesh) {
        const DELAY: f32 = 0.14;
        if t < DELAY {
            self.draw_dead_glass(mesh, 255);
            return;
        }
        let p = ((t - DELAY) / (1.0 - DELAY)).clamp(0.0, 1.0);
        let alpha = ((1.0 - p.powf(1.8)) * 255.0) as u8;
        self.draw_dead_glass(mesh, alpha);
    }

    /// O vidro da fase por cima da imagem do tubo (a malha curva as bordas).
    fn draw_tv_phase_glass(&mut self, phase: &TvPhase, mesh: &CrtMesh) {
        if self.screen.width() == 0 || self.screen.height() == 0 {
            return;
        }
        if !matches!(phase, TvPhase::Collapse { .. }) {
            self.collapse_snap = None;
        }
        match *phase {
            TvPhase::On => {}
            TvPhase::Dead => self.draw_dead_glass(mesh, 255),
            TvPhase::Collapse { t } => self.draw_collapse(t, mesh),
            TvPhase::WarmUp { t } => self.draw_warmup(t, mesh),
        }
    }

    /// Sobe o vidro morto (`dead_glass_rgba`) numa textura estática.
    fn build_dead_glass(&mut self, size: (u32, u32)) -> Option<DeadGlass> {
        let (w, h) = size;
        let mut tex = self
            .canvas
            .create_texture_static(SdlFormat::RGBA32, w, h)
            .ok()?;
        tex.set_blend_mode(BlendMode::Blend);
        tex.set_scale_mode(SdlScaleMode::Nearest);
        tex.update(None, &dead_glass_rgba(w, h), (w * 4) as usize)
            .ok()?;
        Some(DeadGlass { size, tex })
    }

    /// O vidro da fase, entre a imagem do tubo e o bezel (que desenha por
    /// cima, curvando as bordas): TV desligada = vidro morto onde estaria a
    /// imagem do jogo — o jogo segue rodando. Avança a fase uma vez por
    /// quadro apresentado.
    pub(super) fn draw_tv_glass_before_bezel(&mut self, mesh: &CrtMesh) {
        let phase = self.tv_phase();
        self.draw_tv_phase_glass(&phase, mesh);
    }

    /// O retângulo (coordenadas de canvas) do botão focado, se foi
    /// desenhado no último quadro.
    fn pad_focus_rect(&self, f: PadFocus) -> Option<Rect> {
        match f {
            PadFocus::Panel(b) => self
                .panel_buttons
                .iter()
                .find(|(p, _)| *p == b)
                .map(|(_, r)| *r),
            PadFocus::Tv(t) => self.tv_power_buttons.map(|c| match t {
                TvControl::Power => c.power,
                TvControl::VolUp => c.vol_up,
                TvControl::VolDown => c.vol_down,
                TvControl::Menu => c.menu,
            }),
            PadFocus::Arrow(UpdateArrow::App) => self.update_arrows.0,
            PadFocus::Arrow(UpdateArrow::Core) => self.update_arrows.1,
        }
    }

    /// Desenha o anel do foco do controle (renovado a cada quadro pela tela
    /// dona; some em poucos quadros sem renovar).
    fn draw_pad_focus(&mut self) {
        let Some((f, ttl)) = self.pad_focus else {
            return;
        };
        self.pad_focus = (ttl > 1).then_some((f, ttl - 1));
        if let Some(r) = self.pad_focus_rect(f) {
            draw_focus_ring(&mut self.canvas, r);
        }
    }

    /// O lado TV de toda cena com tubo, logo antes do `present`: o OSD de
    /// volume e os controles do queixo, com os rects registrados a cada
    /// frame para o hit-test.
    pub(super) fn draw_tv_overlay(&mut self) {
        let prev = self.canvas.viewport();
        self.canvas.set_viewport(Some(self.canvas_rect));
        let screen = self.screen;
        self.draw_volume_osd(screen);
        if !self.tv_on() {
            self.tv_menu = None; // o menu morre com a TV
        }
        if let Some((item, _)) = self.menu_open() {
            draw_picture_menu(&mut self.canvas, &mut self.font, screen, self.picture, item);
        }
        let out_h = self.canvas_rect.height();
        // A marca prateada do meio do queixo; cede espaço ao selo do RA e
        // ao OSD quando eles estão na tela.
        if !self.images.contains_key(&BRAND_LOGO_ID) {
            self.set_image(BRAND_LOGO_ID, BRAND_LOGO_W, BRAND_LOGO_H, BRAND_LOGO_RGBA);
        }
        let busy = self.ra_status.is_some() || !self.osd_queue.is_empty();
        let reserve = if busy { BRAND_RESERVE_RA } else { 0 };
        if let Some(r) = brand_logo_rect(screen, out_h, reserve) {
            draw_image_absolute(
                &mut self.canvas,
                &self.images,
                BRAND_LOGO_ID,
                r.x(),
                r.y(),
                r.width(),
                r.height(),
            );
        }
        let look = ChinLook { on: self.tv_on() };
        let chin = draw_tv_chin_controls(&mut self.canvas, &mut self.font, screen, out_h, look);
        self.tv_power_buttons = Some(chin);
        self.draw_pad_focus();
        self.canvas.set_viewport(Some(prev));
    }

    pub(super) fn draw_volume_osd(&mut self, screen: Rect) {
        let Some(until) = self.volume_osd_until else {
            return;
        };
        if Instant::now() >= until {
            self.volume_osd_until = None;
            return;
        }
        // OSD morre com a TV (plan revision): barrinhas sobre o vidro
        // morto é impossível — e o deadline apaga junto, na re-entrada o
        // OSD recomeça do clique, sem resíduo.
        if !self.tv_on() {
            self.volume_osd_until = None;
            return;
        }
        const TOTAL: u8 = 16;
        let canvas = &mut self.canvas;
        let font = &mut self.font;
        let seg_w = 26i32;
        let n = TOTAL as i32;
        let bars_w = n * seg_w;
        let bx = screen.x() + (screen.width() as i32 - bars_w) / 2;
        let by = screen.bottom() - (screen.height() as i32 / 5);
        // O verde do CH 3 (OSD_GREEN) — o fósforo do OSD da TV é um só,
        // canal e volume na mesma cor E na mesma fonte: escala 3 com a
        // sombra 2px deslocada (plan revision: "aquela fonte retro da tv").
        let label = "VOLUME";
        let lw = (GLYPH_W as i32 * 3) * label.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            bx - lw - 18 + 2,
            by + 2 + 2,
            TextStyle::new(3.0, (12, 14, 12)),
            label,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            bx - lw - 18,
            by + 2,
            TextStyle::new(3.0, OSD_GREEN),
            label,
            usize::MAX,
        );
        if self.volume >= TOTAL {
            let mx = bx + bars_w + 18;
            draw_text_absolute(
                canvas,
                font,
                mx + 2,
                by + 2 + 2,
                TextStyle::new(3.0, (12, 14, 12)),
                "MAX",
                usize::MAX,
            );
            draw_text_absolute(
                canvas,
                font,
                mx,
                by + 2,
                TextStyle::new(3.0, OSD_GREEN),
                "MAX",
                usize::MAX,
            );
        }
        for i in 0..n {
            let aceso = (i as u8) < self.volume;
            let x = bx + i * seg_w;
            let cor = if aceso { OSD_GREEN } else { (20, 34, 22) };
            canvas.set_draw_color(Color::RGB(cor.0, cor.1, cor.2));
            let _ = canvas.fill_rect(Rect::new(x, by, 9, 34));
            let _ = canvas.fill_rect(Rect::new(x + 13, by, 9, 26));
        }
        // o filete de brilho sob as barras acesas
        if self.volume > 0 {
            let w = seg_w * self.volume as i32 - 6;
            canvas.set_draw_color(Color::RGBA(OSD_GREEN.0, OSD_GREEN.1, OSD_GREEN.2, 150));
            let _ = canvas.fill_rect(Rect::new(bx, by + 40, w.max(4) as u32, 1));
        }
    }
}
