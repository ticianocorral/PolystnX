//! Navegação por gamepad DENTRO do jogo: o jogo é dono de todos os botões,
//! então o menu só abre com um acorde (Select+Start segurados) e, uma vez
//! aberto, os botões físicos viram navegação. Tudo aqui é lógica pura —
//! o `Platform` só entrega o estado cru dos botões (`Platform::pad_bits`).

use std::time::Duration;

/// Botões físicos (SDL, não os remapeados do SNES) como máscara.
pub mod bit {
    pub const UP: u16 = 1 << 0;
    pub const DOWN: u16 = 1 << 1;
    pub const LEFT: u16 = 1 << 2;
    pub const RIGHT: u16 = 1 << 3;
    /// South (A do Xbox/Deck).
    pub const A: u16 = 1 << 4;
    /// East (B).
    pub const B: u16 = 1 << 5;
    /// West (X).
    pub const X: u16 = 1 << 6;
    /// North (Y).
    pub const Y: u16 = 1 << 7;
    pub const LB: u16 = 1 << 8;
    pub const RB: u16 = 1 << 9;
    /// Menu/Start.
    pub const START: u16 = 1 << 10;
    /// View/Back/Select.
    pub const SELECT: u16 = 1 << 11;
}

/// Quanto tempo Select+Start precisam ficar apertados para abrir o menu.
pub const CHORD_HOLD: Duration = Duration::from_millis(800);

/// Temporizador do acorde Select+Start.
#[derive(Debug, Default)]
pub struct Chord {
    held: Duration,
    /// Já disparou nesta pressão: só rearma depois de soltar os dois, e
    /// até lá Select/Start continuam engolidos (o jogo não vê o resto).
    latched: bool,
}

/// O que o acorde decidiu num quadro.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChordOut {
    /// Abrir o menu agora (uma vez por pressão).
    pub fire: bool,
    /// Esconder Select/Start do jogo neste quadro.
    pub swallow: bool,
}

impl Chord {
    pub fn update(&mut self, bits: u16, dt: Duration) -> ChordOut {
        let both = bits & bit::SELECT != 0 && bits & bit::START != 0;
        let any = bits & (bit::SELECT | bit::START) != 0;
        if self.latched {
            if !any {
                self.latched = false;
                self.held = Duration::ZERO;
            }
            return ChordOut {
                fire: false,
                swallow: any,
            };
        }
        if !both {
            // Um só botão apertado não é acorde: o jogo o recebe normal
            // (Start sozinho pausa/segue em muitos jogos).
            self.held = Duration::ZERO;
            return ChordOut::default();
        }
        self.held += dt;
        if self.held >= CHORD_HOLD {
            self.latched = true;
            return ChordOut {
                fire: true,
                swallow: true,
            };
        }
        ChordOut {
            fire: false,
            swallow: true,
        }
    }

    /// Fração 0..=1 do acorde já segurada (para uma barrinha, se quiser).
    pub fn progress(&self) -> f32 {
        (self.held.as_secs_f32() / CHORD_HOLD.as_secs_f32()).min(1.0)
    }

    /// Esquece tudo (ao mudar de tela), exigindo soltar antes de rearmar.
    pub fn reset_latched(&mut self) {
        self.held = Duration::ZERO;
        self.latched = true;
    }
}

const REPEAT_FIRST: Duration = Duration::from_millis(400);
const REPEAT_EVERY: Duration = Duration::from_millis(110);

/// Bordas de descida→subida dos botões, com auto-repetição nas direções
/// e nos ombros (segurar ↓ rola a lista).
#[derive(Debug, Default)]
pub struct EdgeTracker {
    prev: u16,
    since: Duration,
    repeated: Duration,
}

const REPEATING: u16 = bit::UP | bit::DOWN | bit::LEFT | bit::RIGHT | bit::LB | bit::RB;

impl EdgeTracker {
    /// Botões que "dispararam" neste quadro (apertou agora ou repetiu).
    pub fn update(&mut self, bits: u16, dt: Duration) -> u16 {
        let fresh = bits & !self.prev;
        let held = bits & self.prev & REPEATING;
        let mut out = fresh;
        if fresh != 0 || held == 0 {
            // Alvo novo (ou nada segurado): reinicia o relógio da repetição.
            self.since = Duration::ZERO;
            self.repeated = Duration::ZERO;
        } else {
            self.since += dt;
            if self.since >= REPEAT_FIRST {
                self.repeated += dt;
                if self.repeated >= REPEAT_EVERY {
                    self.repeated = Duration::ZERO;
                    out |= held;
                }
            }
        }
        self.prev = bits;
        out
    }

    /// Descarta o que está apertado agora: só conta depois de soltar
    /// (ao abrir/fechar um overlay, para o mesmo toque não vazar).
    pub fn swallow_held(&mut self, bits: u16) {
        self.prev = bits;
        self.since = Duration::ZERO;
        self.repeated = Duration::ZERO;
    }
}

/// Movimento do cursor numa lista de `n` linhas em `cols` colunas
/// (preenchidas coluna a coluna, como o `draw_modal`). `dx`/`dy` em passos;
/// ↑/↓ dão a volta na lista, ←/→ pulam de coluna sem sair da grade.
pub fn move_cursor(cur: usize, n: usize, cols: usize, dx: i32, dy: i32) -> usize {
    if n == 0 {
        return 0;
    }
    let cur = cur.min(n - 1);
    let rows_per_col = n.div_ceil(cols.max(1)).max(1);
    let mut c = cur as i32;
    if dy != 0 {
        c = (c + dy).rem_euclid(n as i32);
    }
    if dx != 0 && cols > 1 {
        let t = c + dx * rows_per_col as i32;
        if (0..n as i32).contains(&t) {
            c = t;
        }
    }
    c as usize
}

/// Salto de página (LB/RB): `step` linhas, sem dar a volta.
pub fn page_cursor(cur: usize, n: usize, step: i32) -> usize {
    if n == 0 {
        return 0;
    }
    (cur as i32 + step).clamp(0, n as i32 - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const BOTH: u16 = bit::SELECT | bit::START;

    #[test]
    fn chord_fires_once_after_the_hold() {
        let mut c = Chord::default();
        let mut fired = 0;
        for _ in 0..60 {
            if c.update(BOTH, MS(16)).fire {
                fired += 1;
            }
        }
        assert_eq!(fired, 1);
    }

    #[test]
    fn chord_short_press_never_fires_and_releases_clean() {
        let mut c = Chord::default();
        for _ in 0..20 {
            assert!(!c.update(BOTH, MS(16)).fire); // 320 ms
        }
        assert_eq!(c.update(0, MS(16)), ChordOut::default());
        // Recomeça do zero: mais 20 quadros também não bastam.
        for _ in 0..20 {
            assert!(!c.update(BOTH, MS(16)).fire);
        }
    }

    #[test]
    fn chord_swallows_until_both_are_released() {
        let mut c = Chord::default();
        while !c.update(BOTH, MS(16)).fire {}
        // Soltou só o Select: Start ainda engolido, sem novo disparo.
        let o = c.update(bit::START, MS(16));
        assert!(o.swallow && !o.fire);
        let o = c.update(0, MS(16));
        assert!(!o.swallow && !o.fire);
    }

    #[test]
    fn a_lone_button_is_not_swallowed() {
        let mut c = Chord::default();
        assert_eq!(c.update(bit::START, MS(16)), ChordOut::default());
    }

    #[test]
    fn edges_fire_on_press_and_repeat_when_held() {
        let mut e = EdgeTracker::default();
        assert_eq!(e.update(bit::DOWN, MS(16)), bit::DOWN);
        assert_eq!(e.update(bit::DOWN, MS(16)), 0);
        let mut n = 0;
        for _ in 0..60 {
            if e.update(bit::DOWN, MS(16)) & bit::DOWN != 0 {
                n += 1;
            }
        }
        assert!(n >= 5, "repetiu {n}x");
        // Botão de ação não repete.
        let mut e = EdgeTracker::default();
        assert_eq!(e.update(bit::A, MS(16)), bit::A);
        for _ in 0..60 {
            assert_eq!(e.update(bit::A, MS(16)), 0);
        }
    }

    #[test]
    fn swallow_held_ignores_the_press_that_opened_the_overlay() {
        let mut e = EdgeTracker::default();
        e.swallow_held(bit::A);
        assert_eq!(e.update(bit::A, MS(16)), 0);
        assert_eq!(e.update(0, MS(16)), 0);
        assert_eq!(e.update(bit::A, MS(16)), bit::A);
    }

    #[test]
    fn cursor_wraps_on_vertical_and_hops_columns() {
        assert_eq!(move_cursor(0, 5, 1, 0, -1), 4);
        assert_eq!(move_cursor(4, 5, 1, 0, 1), 0);
        // 15 linhas em 2 colunas: 8 por coluna.
        assert_eq!(move_cursor(2, 15, 2, 1, 0), 10);
        assert_eq!(move_cursor(10, 15, 2, -1, 0), 2);
        // Coluna da direita mais curta: 7 não tem par à direita (15 fora).
        assert_eq!(move_cursor(7, 15, 2, 1, 0), 7);
        assert_eq!(move_cursor(0, 0, 1, 0, 1), 0);
    }

    #[test]
    fn page_cursor_clamps() {
        assert_eq!(page_cursor(2, 30, 5), 7);
        assert_eq!(page_cursor(2, 30, -5), 0);
        assert_eq!(page_cursor(28, 30, 5), 29);
    }
}
