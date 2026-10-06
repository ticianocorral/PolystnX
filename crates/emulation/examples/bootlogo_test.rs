//! Controle: show_bios_bootlogo=disabled pula a animação no load inicial?
use polystnx_emulation::{Core, PixelFormat};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios"))?);
    let rom = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("rom"))?);
    let out = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("dir"))?);
    std::fs::create_dir_all(&out)?;

    let mut core = Core::load(&core_path)?;
    std::fs::create_dir_all("/tmp/bltest-saves").ok();
    core.set_directories(&bios_dir, &PathBuf::from("/tmp/bltest-saves"));
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "enabled");
    core.init();
    core.load_game(&rom, &[])?;
    let mut real = 0usize;
    let mut dupes = 0usize;
    for i in 0..2100 {
        core.run();
        if let Some(f) = core.take_frame() {
            real += 1;
            if real.is_multiple_of(30) {
                let p = out.join(format!("bl-{real:03}.bmp"));
                save(&f, &p);
            }
        } else if core.frame_duped() {
            dupes += 1;
        }
        if i == 1199 {
            println!("frames reais: {real}, dupes: {dupes}");
        }
    }
    println!("fim");
    Ok(())
}

fn save(f: &polystnx_emulation::Frame, path: &std::path::Path) {
    let _ = save_bmp(path, f.width, f.height, f.format, &f.pixels);
}

fn save_bmp(
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
        other => anyhow::bail!("formato {other:?}"),
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
        for (x, px) in src.as_chunks::<4>().0.iter().enumerate() {
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
