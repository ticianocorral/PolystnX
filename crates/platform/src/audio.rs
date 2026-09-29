//! Push-model audio: the frontend hands over the S16 stereo the core produced
//! each frame and SDL's stream does the resampling to the device rate.

use sdl3::audio::{AudioFormat, AudioSpec, AudioStreamOwner};
use sdl3::AudioSubsystem;

use crate::PlatformError;

pub struct AudioOut {
    stream: AudioStreamOwner,
    /// Source rate the core declared; kept for diagnostics.
    pub source_rate: u32,
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
            stream,
            source_rate: sample_rate,
        })
    }

    /// Interleaved L/R. Non-blocking; SDL buffers and resamples.
    pub fn queue(&self, interleaved_stereo: &[i16]) {
        if interleaved_stereo.is_empty() {
            return;
        }
        let _ = self.stream.put_data_i16(interleaved_stereo);
    }

    /// Stereo sample-frames still waiting to play. Used to pace the loop so the
    /// buffer neither starves nor grows without bound.
    pub fn queued_frames(&self) -> usize {
        self.stream
            .queued_bytes()
            .map(|b| (b.max(0) as usize) / 4)
            .unwrap_or(0)
    }

    pub fn clear(&self) {
        let _ = self.stream.clear();
    }
}
