//! Safe wrapper around the vendored blargg `snes_ntsc` 0.2.2 filter.
//!
//! Input and output are both 16-bit RGB565 (the library's compiled-in default,
//! see `vendor/snes_ntsc/snes_ntsc_config.h`). One low-res 256-wide field comes
//! out 602 wide.

use std::ffi::{c_int, c_long, c_void};
use std::os::raw::c_float;

const IN_CHUNK: u32 = 3;
const OUT_CHUNK: u32 = 7;

#[repr(C)]
#[derive(Clone, Copy)]
struct SnesNtscSetup {
    hue: f64,
    saturation: f64,
    contrast: f64,
    brightness: f64,
    sharpness: f64,
    gamma: f64,
    resolution: f64,
    artifacts: f64,
    fringing: f64,
    bleed: f64,
    merge_fields: c_int,
    decoder_matrix: *const c_float,
    bsnes_colortbl: *const c_void,
}

impl SnesNtscSetup {
    const fn base() -> Self {
        SnesNtscSetup {
            hue: 0.0,
            saturation: 0.0,
            contrast: 0.0,
            brightness: 0.0,
            sharpness: 0.0,
            gamma: 0.0,
            resolution: 0.0,
            artifacts: 0.0,
            fringing: 0.0,
            bleed: 0.0,
            merge_fields: 1,
            decoder_matrix: std::ptr::null(),
            bsnes_colortbl: std::ptr::null(),
        }
    }
}

extern "C" {
    fn snes_ntsc_init(ntsc: *mut c_void, setup: *const SnesNtscSetup);
    fn snes_ntsc_blit(
        ntsc: *const c_void,
        input: *const u16,
        in_row_width: c_long,
        burst_phase: c_int,
        in_width: c_int,
        in_height: c_int,
        rgb_out: *mut c_void,
        out_pitch: c_long,
    );
    fn xperience_snes_ntsc_sizeof() -> usize;
}

/// Video-signal presets. `Rf` is our tuning (see the vendor README); the rest
/// mirror the upstream presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Composite,
    SVideo,
    Rgb,
    Monochrome,
    Rf,
}

impl Preset {
    fn setup(self) -> SnesNtscSetup {
        let mut s = SnesNtscSetup::base();
        match self {
            Preset::Composite => {}
            Preset::SVideo => {
                s.sharpness = 0.2;
                s.resolution = 0.2;
                s.artifacts = -1.0;
                s.fringing = -1.0;
            }
            Preset::Rgb => {
                s.sharpness = 0.2;
                s.resolution = 0.7;
                s.artifacts = -1.0;
                s.fringing = -1.0;
                s.bleed = -1.0;
            }
            Preset::Monochrome => {
                s.saturation = -1.0;
                s.sharpness = 0.2;
                s.resolution = 0.2;
                s.artifacts = -0.2;
                s.fringing = -0.2;
                s.bleed = -1.0;
            }
            // Composite, but softer and noisier: the antenna/RF look.
            Preset::Rf => {
                s.resolution = -0.2;
                s.artifacts = 0.35;
                s.fringing = 0.35;
                s.bleed = 0.30;
                s.merge_fields = 0; // let the dot pattern crawl
            }
        }
        s
    }

    pub fn label(self) -> &'static str {
        match self {
            Preset::Composite => "composite",
            Preset::SVideo => "s-video",
            Preset::Rgb => "rgb",
            Preset::Monochrome => "monochrome",
            Preset::Rf => "rf",
        }
    }
}

pub struct NtscFilter {
    /// The ~8 MiB kernel table. `u64`-backed for 8-byte alignment.
    table: Box<[u64]>,
    preset: Preset,
    phase: c_int,
    animate: bool,
    in_buf: Vec<u16>,
    out: Vec<u16>,
}

impl NtscFilter {
    pub fn new(preset: Preset) -> Self {
        let bytes = unsafe { xperience_snes_ntsc_sizeof() };
        let table = vec![0u64; bytes.div_ceil(8)].into_boxed_slice();
        let mut me = Self {
            table,
            preset,
            phase: 0,
            animate: preset == Preset::Rf,
            in_buf: Vec::new(),
            out: Vec::new(),
        };
        me.reinit();
        me
    }

    pub fn preset(&self) -> Preset {
        self.preset
    }

    pub fn set_preset(&mut self, preset: Preset) {
        if preset != self.preset {
            self.preset = preset;
            self.animate = preset == Preset::Rf;
            self.reinit();
        }
    }

    fn reinit(&mut self) {
        let setup = self.preset.setup();
        unsafe { snes_ntsc_init(self.table.as_mut_ptr() as *mut c_void, &setup) };
    }

    /// Output width for a given low-res input width (e.g. 256 -> 602).
    pub fn output_width(in_width: u32) -> u32 {
        ((in_width.saturating_sub(1)) / IN_CHUNK + 1) * OUT_CHUNK
    }

    /// Filter one RGB565 frame. `src` is `src_h` rows of `src_w` pixels with a
    /// `src_pitch_bytes` byte stride. Returns the filtered RGB565 buffer, its
    /// width, and its height (height is unchanged).
    pub fn process(
        &mut self,
        src: &[u8],
        src_w: u32,
        src_h: u32,
        src_pitch_bytes: usize,
    ) -> (&[u16], u32, u32) {
        let (w, h) = (src_w as usize, src_h as usize);
        self.in_buf.resize(w * h, 0);
        for row in 0..h {
            let off = row * src_pitch_bytes;
            for (i, dst) in self.in_buf[row * w..row * w + w].iter_mut().enumerate() {
                let b = off + i * 2;
                *dst = u16::from_le_bytes([src[b], src[b + 1]]);
            }
        }

        let out_w = Self::output_width(src_w) as usize;
        self.out.resize(out_w * h, 0);
        unsafe {
            snes_ntsc_blit(
                self.table.as_ptr() as *const c_void,
                self.in_buf.as_ptr(),
                src_w as c_long,
                self.phase,
                src_w as c_int,
                src_h as c_int,
                self.out.as_mut_ptr() as *mut c_void,
                (out_w * 2) as c_long,
            );
        }
        if self.animate {
            self.phase = (self.phase + 1) % 3;
        }
        (&self.out[..out_w * h], out_w as u32, src_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_width_matches_upstream() {
        assert_eq!(NtscFilter::output_width(256), 602);
    }

    #[test]
    fn filters_a_flat_frame_to_602() {
        let mut f = NtscFilter::new(Preset::Rf);
        let src = vec![0u8; 256 * 224 * 2];
        let (out, w, h) = f.process(&src, 256, 224, 256 * 2);
        assert_eq!((w, h), (602, 224));
        assert_eq!(out.len(), 602 * 224);
    }
}
