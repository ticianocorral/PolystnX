//! Gabinete: audio (extraído de `cabinet/mod.rs`, sem mudança de comportamento).

use super::*;

impl Cabinet {
    /// Show the green "CH 3" channel banner over the picture for `dur`
    /// (plan revision: "quando ligar o console, mostrar por 3 segundos e
    /// remover da tela") — the runner calls this on power-on, the way an old
    /// TV flashes the channel number when you tune it. While the tube is on
    /// static instead, `present_static` draws the banner constantly, no
    /// timer involved.
    pub fn flash_ch3(&mut self, dur: Duration) {
        self.ch3_until = Some(Instant::now() + dur);
    }

    /// Settings gate for the ambient static hiss (plan revision: "colocar
    /// na configuração para tocar ou não. por padrão vem desligado") — off
    /// by default; `present_static` queues the white noise only while this
    /// is set. Turning it off drops the stream immediately.
    pub fn set_static_hiss(&mut self, on: bool) {
        self.static_hiss = on;
        if !on {
            self.hiss = None;
        }
    }

    /// Queue a one-shot sound (mono `samples` at `rate`) on the cabinet's
    /// persistent foley stream — the stream outlives the call, so the queued
    /// audio actually plays (a stream dropped immediately after `queue`
    /// takes its audio with it). Returns `false` when no audio device is
    /// available; callers keep their fallback in that case.
    pub fn play_foley(&mut self, samples: &[i16], rate: u32) -> bool {
        if self.foley.is_none() {
            match AudioOut::new(&self.audio, rate) {
                Ok(a) => self.foley = Some(a),
                Err(e) => {
                    log::warn!("foley stream: {e}");
                    return false;
                }
            }
        }
        // Mono → the interleaved stereo `AudioOut` wants. Non-blocking: SDL
        // buffers and plays on its own; overlapping sounds queue in order.
        let mut buf = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            buf.push(s);
            buf.push(s);
        }
        if let Some(a) = &self.foley {
            a.queue(&buf);
            self.foley_busy_at = Instant::now();
        }
        true
    }

    /// Um tick da máquina de som do leitor de CD: `on` = o core está
    /// hesitando (boot, load, FMV) e o drive "lê". O loop entra com fade de
    /// ~150 ms e sai com fade igual — nenhum corte seco no meio do som.
    /// Chame uma vez por tick de interface; a fila anda sozinha.
    pub fn tick_cd_noise(&mut self, on: bool, samples: &'static [i16], rate: u32) {
        const FADE_TICKS: f32 = 9.0; // ~150 ms a 60 fps
        const AMBIENT: f32 = 0.20; // bem abaixo do áudio do jogo
        const CHUNK: usize = 368; // ~1/60 s de 22 050 Hz
        if on {
            if self.cd_noise.is_none() {
                match AudioOut::new(&self.audio, rate) {
                    Ok(stream) => {
                        self.cd_noise = Some(CdNoise {
                            stream,
                            pos: 0,
                            level: 0.0,
                        });
                    }
                    Err(e) => {
                        log::warn!("cd noise stream: {e}");
                        return;
                    }
                }
            }
            if let Some(cd) = &mut self.cd_noise {
                cd.level = (cd.level + 1.0 / FADE_TICKS).min(1.0);
                let mut buf = Vec::with_capacity(CHUNK * 2);
                for i in 0..CHUNK {
                    let s = samples[(cd.pos + i) % samples.len()] as f32 * cd.level * AMBIENT;
                    let v = s.clamp(-32000.0, 32000.0) as i16;
                    buf.push(v);
                    buf.push(v);
                }
                cd.pos = (cd.pos + CHUNK) % samples.len();
                cd.stream.queue(&buf);
            }
        } else if let Some(mut cd) = self.cd_noise.take() {
            // Saída com fade: fila o que falta do fade e encerra o stream.
            while cd.level > 0.0 {
                cd.level = (cd.level - 1.0 / FADE_TICKS).max(0.0);
                let mut buf = Vec::with_capacity(CHUNK * 2);
                for i in 0..CHUNK {
                    let s = samples[(cd.pos + i) % samples.len()] as f32 * cd.level * AMBIENT;
                    let v = s.clamp(-32000.0, 32000.0) as i16;
                    buf.push(v);
                    buf.push(v);
                }
                cd.pos = (cd.pos + CHUNK) % samples.len();
                cd.stream.queue(&buf);
            }
        }
    }

    /// Fecha o stream de foley depois de 3 s sem nada tocando: um stream
    /// aberto mixando silêncio mantém a thread de áudio acordando e impede o
    /// PipeWire de suspender a saída (Steam Deck: codec e amplificador
    /// ligados à toa). Reabre sozinho no próximo som.
    pub(super) fn reap_idle_audio(&mut self) {
        const IDLE: Duration = Duration::from_secs(3);
        if let Some(f) = &self.foley {
            if f.queued_frames() > 0 {
                self.foley_busy_at = Instant::now();
            } else if self.foley_busy_at.elapsed() >= IDLE {
                self.foley = None;
            }
        }
    }

    /// Queue one frame's worth of hiss at `level` (the same 0..1 the visual
    /// snow uses, so blizzards hiss louder than the resting hiss). Opens the
    /// stream lazily on the first hissing frame.
    pub(super) fn queue_static_hiss(&mut self, level: f32) {
        const RATE: u32 = 22_050;
        if self.hiss.is_none() {
            match AudioOut::new(&self.audio, RATE) {
                Ok(a) => self.hiss = Some(a),
                Err(e) => {
                    log::warn!("static hiss: {e}");
                    self.static_hiss = false;
                    return;
                }
            }
        }
        // Enche a fila até ~50 ms, em vez de um bloco fixo por quadro: com o
        // ritmo ocioso (30/20/10 Hz) um bloco de 1/60 s deixaria buracos.
        let target = (RATE / 20) as usize;
        let n = target.saturating_sub(a_queued(&self.hiss));
        if n == 0 {
            return;
        }
        let amp = (level.clamp(0.0, 1.0) * 1800.0) as i32;
        let mut buf = Vec::with_capacity(n * 2);
        for _ in 0..n {
            self.hiss_rng ^= self.hiss_rng << 13;
            self.hiss_rng ^= self.hiss_rng >> 17;
            self.hiss_rng ^= self.hiss_rng << 5;
            let s = (((self.hiss_rng >> 8) & 0xFFFF) as i32 - 0x8000) * amp / 0x8000;
            let v = s.clamp(-32000, 32000) as i16;
            buf.push(v);
            buf.push(v);
        }
        if let Some(a) = &self.hiss {
            a.queue(&buf);
        }
    }
}
