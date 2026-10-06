//! Diagnóstico: frames iniciais do boot — o logo do PlayStation toca?
//!   cargo run -p polystnx-emulation --example boot_frames -- <core> <bios-dir> <rom> <dir>

use polystnx_emulation::{Core, PixelFormat};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios-dir"))?);
    let rom = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("rom"))?);
    let out = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("dir"))?);
    std::fs::create_dir_all(&out)?;

    let mut core = Core::load(&core_path)?;
    core.set_directories(&bios_dir, &PathBuf::from("/tmp/bootframes-saves"));
    std::fs::create_dir_all("/tmp/bootframes-saves").ok();
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "enabled");
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    core.init();
    core.load_game(&rom, &[])
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let mut frame_no = 0u32;
    loop {
        core.run();
        if let Some(frame) = core.take_frame() {
            frame_no += 1;
            if frame_no % 60 == 0 && frame_no <= 600 {
                let path = out.join(format!("boot-{frame_no:04}.bmp"));
                write_bmp(
                    &path,
                    frame.width,
                    frame.height,
                    frame.format,
                    &frame.pixels,
                )?;
            }
            if frame_no >= 600 {
                break;
            }
        }
    }
    println!("frames: {frame_no}");
    Ok(())
}

fn write_bmp(
    path: &std::path::Path,
    w: u32,
    h: u32,
    format: PixelFormat,
    pixels: &[u8],
) -> anyhow::Result<()> {
    let rgba: Vec<u8> = match format {
        PixelFormat::Xrgb8888 => pixels.to_vec(),
        PixelFormat::Rgb565 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| {
                let v = u16::from_le_bytes(*p) as u32;
                [
                    (((v >> 11) & 0x1f) * 255 / 31) as u8,
                    (((v >> 5) & 0x3f) * 255 / 63) as u8,
                    ((v & 0x1f) * 255 / 31) as u8,
                    255,
                ]
            })
            .collect(),
        other => anyhow::bail!("formato {other:?} sem conversor no exemplo"),
    };
    let row = w as usize * 4;
    let size = 54 + row * h as usize;
    let mut bmp = vec![0u8; size];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&(size as u32).to_le_bytes());
    bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
    bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&(w as i32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(h as i32).to_le_bytes());
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
    for y in 0..h as usize {
        let src = &rgba[y * row..(y + 1) * row];
        let dst = h as usize - 1 - y;
        for (x, px) in src.chunks_exact(4).enumerate() {
            let o = 54 + dst * row + x * 4;
            if o + 3 < size {
                bmp[o] = px[2];
                bmp[o + 1] = px[1];
                bmp[o + 2] = px[0];
                bmp[o + 3] = 255;
            }
        }
    }
    std::fs::write(path, bmp)?;
    Ok(())
}
