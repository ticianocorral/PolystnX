//! Diagnóstico: BIOS viva na tela "insira um disco" + load_disc de um jogo
//! + fechar a tampa — o jogo bootA sem reset?
//!   cargo run -p polystnx-emulation --example bios_insert -- <core> <bios-dir> <boot.chd> <jogo.chd> <dir>

use polystnx_emulation::{Core, PixelFormat};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios"))?);
    let boot = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("boot"))?);
    let game = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("game"))?);
    let out = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("dir"))?);
    std::fs::create_dir_all(&out)?;

    let mut core = Core::load(&core_path)?;
    std::fs::create_dir_all("/tmp/biosinsert-saves").ok();
    core.set_directories(&bios_dir, &PathBuf::from("/tmp/biosinsert-saves"));
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "enabled");
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    core.set_variable("pcsx_rearmed_memcard2", "enabled");
    core.init();

    // boot com disco real ejetado (igual ao runner)
    core.load_game(&boot, &[])?;
    let ok = core.set_eject_state(true);
    println!("eject no boot: {ok:?}");
    for i in 0..900 {
        core.run();
        if let Some(f) = core.take_frame() {
            if i == 899 {
                save(&f, &out.join("a-bios.bmp"));
            }
        }
    }
    println!("tela da BIOS após 900 frames");

    // inserção: carrega OUTRO disco e fecha a tampa — SEM reset
    // Fecha a tampa ANTES do load: o reset interno do load_disc bootA a
    // BIOS com a tampa fechada e o jogo dentro — a BIOS lê e bootA o jogo
    // direto (o menu não volta).
    let ok = core.set_eject_state(false);
    println!("fecha tampa (antes do load): {ok:?}");
    // ===== REPRODUÇÃO EXATA DO BOOTFRESH DO APP =====
    // 1. o velho sai PRIMEIRO: drop → dlclose → refcount 0 → a lib
    //    DESCARREGA e os globals morrem com ela (criar o novo antes
    //    mantinha a lib viva com estado sujo = tela preta eterna)
    drop(core);
    // 2. core novo: dlopen limpo
    let mut fresh = polystnx_emulation::Core::load(&core_path)?;
    // 3. dirs + opções NA MESA antes do init
    fresh.set_directories(&bios_dir, &PathBuf::from("/tmp/biosinsert-saves"));
    fresh.set_variable("pcsx_rearmed_show_bios_bootlogo", "disabled");
    fresh.set_variable("pcsx_rearmed_memcard2", "enabled");
    fresh.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    // 4. init + load do jogo (o skip entra direto no exe)
    fresh.init();
    match fresh.load_game(&game, &[]) {
        Ok(()) => println!("boot fresh: load ok"),
        Err(e) => println!("boot fresh: load FALHOU: {e}"),
    }
    for i in 0..1350 {
        fresh.run();
        if let Some(f) = fresh.take_frame() {
            if i % 90 == 0 {
                let p = out.join(format!("bf-{i:04}.bmp"));
                save(&f, &p);
            }
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
