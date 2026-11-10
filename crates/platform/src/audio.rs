//! Push-model audio: the frontend hands over the S16 stereo the core produced
//! each frame and SDL's stream does the resampling to the device rate.

use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::AudioSubsystem;

use crate::PlatformError;

pub struct AudioOut {
    /// `None` = saída muda ([`AudioOut::muted`]): sem dispositivo de áudio o
    /// jogo ainda roda, só sem som.
    stream: Option<AudioStreamOwner>,
    /// Source rate the core declared; kept for diagnostics.
    pub source_rate: u32,
    /// Dispositivo pausado por [`AudioOut::set_paused`].
    paused: std::cell::Cell<bool>,
}

impl AudioOut {
    pub(crate) fn new(audio: &AudioSubsystem, sample_rate: u32) -> Result<Self, PlatformError> {
        let device = audio.default_playback_device();
        let spec = AudioSpec::new(Some(sample_rate as i32), Some(2), Some(AudioFormat::S16LE));
        let stream = device
            .open_device_stream(Some(&spec))
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        stream
            .resume()
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        Ok(Self {
            stream: Some(stream),
            source_rate: sample_rate,
            paused: std::cell::Cell::new(false),
        })
    }

    /// Uma saída que engole tudo — o caminho de quando o dispositivo de
    /// áudio não abre (PipeWire caído, saída HDMI/Bluetooth que sumiu).
    pub fn muted(sample_rate: u32) -> Self {
        Self {
            stream: None,
            source_rate: sample_rate,
            paused: std::cell::Cell::new(false),
        }
    }

    /// Interleaved L/R. Non-blocking; SDL buffers and resamples.
    pub fn queue(&self, interleaved_stereo: &[i16]) {
        if interleaved_stereo.is_empty() {
            return;
        }
        if let Some(s) = &self.stream {
            let _ = s.put_data_i16(interleaved_stereo);
        }
    }

    /// Stereo sample-frames still waiting to play. Used to pace the loop so the
    /// buffer neither starves nor grows without bound.
    pub fn queued_frames(&self) -> usize {
        self.stream
            .as_ref()
            .and_then(|s| s.queued_bytes().ok())
            .map(|b| (b.max(0) as usize) / 4)
            .unwrap_or(0)
    }

    /// Pausa/retoma o DISPOSITIVO deste stream. Parado (jogo pausado, menu
    /// aberto, console desligado), um stream aberto mixando silêncio mantém
    /// a thread de áudio do SDL acordando ~100×/s e impede o PipeWire de
    /// suspender a saída — no Steam Deck, codec e amplificador ligados à toa.
    /// Ao pausar, a fila é descartada (nada de áudio velho na volta). Só
    /// chama o SDL quando o estado muda.
    pub fn set_paused(&self, paused: bool) {
        if self.paused.get() == paused {
            return;
        }
        self.paused.set(paused);
        let Some(s) = &self.stream else {
            return;
        };
        let r = if paused {
            let _ = s.clear();
            s.pause()
        } else {
            s.resume()
        };
        if let Err(e) = r {
            log::warn!(
                "áudio: {} falhou: {e}",
                if paused { "pausar" } else { "retomar" }
            );
        }
    }

    pub fn clear(&self) {
        if let Some(s) = &self.stream {
            let _ = s.clear();
        }
    }
}
