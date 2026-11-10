//! Runner: pace (extraído de `runner/mod.rs`, sem mudança de comportamento).

use super::*;

/// Advance `next` by one frame and wait for it: sleep the bulk of the wait,
/// then spin only the last ~200µs (a plain `thread::sleep` can overshoot;
/// spinning 2 ms inteiros por frame era queimar ~12% de um núcleo — CPU
/// que o core do emulador precisa quando o sistema está estrangulado, como
/// no gaming mode). Falls back to resyncing when the loop fell behind, so
/// it never spirals. Shared by runner/shelf/idle/settings.
pub(crate) fn pace_frame(next: &mut Instant, frame_time: Duration) {
    *next += frame_time;
    let now = Instant::now();
    if *next <= now {
        if polystnx_platform::trace_enabled() {
            let late = now - *next;
            if late > Duration::from_millis(2) {
                log::warn!(
                    "trace: frame atrasado {:.1} ms (loop não fechou o budget)",
                    late.as_secs_f64() * 1e3
                );
            }
        }
        *next = now; // fell behind; resync so we don't spiral
        return;
    }
    let wait = *next - now;
    // O `nanosleep` erra por ~50–100 µs (timer slack); o spin cobre só o
    // erro típico. Antes dormia até faltar 1 ms e girava ~0,9 ms POR QUADRO
    // (o comentário dizia 200 µs) — CPU acordada à toa no Steam Deck.
    #[cfg(target_os = "linux")]
    const SPIN_WINDOW: Duration = Duration::from_micros(150);
    #[cfg(not(target_os = "linux"))]
    const SPIN_WINDOW: Duration = Duration::from_micros(500);
    if wait > SPIN_WINDOW {
        std::thread::sleep(wait - SPIN_WINDOW);
    }
    while Instant::now() < *next {
        std::thread::yield_now();
    }
}

/// O quadro das telas de menu e animações: 60 Hz de verdade. Era 16 ms
/// (62,5 Hz) — quadros a mais do que a tela de 60 Hz mostra.
pub const UI_FRAME: Duration = Duration::from_nanos(16_666_667);

/// Sem input por este tempo (e sem nada animando), as telas paradas caem
/// para meio quadro (~30 Hz); o primeiro input volta aos 60 Hz.
pub(super) const IDLE_AFTER: Duration = Duration::from_secs(2);

/// O quadro de uma tela parada: `frame`, ou múltiplo dele com o jogador
/// parado ([`IDLE_AFTER`]) ou a janela escondida.
pub(crate) fn idle_paced(frame: Duration, plat: &Platform, cab: &Cabinet) -> Duration {
    if plat.window_hidden() {
        // Ninguém vendo: ~10 Hz basta para ouvir os eventos da volta.
        frame * 6
    } else if plat.idle_for() >= IDLE_AFTER && !cab.animating() {
        frame * 2
    } else {
        frame
    }
}

/// O fim de quadro das telas de menu: ritmo ocioso ([`idle_paced`]) e só
/// `sleep` — o spin final do [`pace_frame`] é precisão que menu não usa.
pub fn pace_ui(next: &mut Instant, frame: Duration, plat: &Platform, cab: &Cabinet) {
    *next += idle_paced(frame, plat, cab);
    let now = Instant::now();
    if *next <= now {
        *next = now; // atrasou: ressincroniza, sem espiral
        return;
    }
    std::thread::sleep(*next - now);
}
