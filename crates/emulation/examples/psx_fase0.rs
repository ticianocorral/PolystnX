//! Fase 0, items 2 and 4 (`docs/fase-0.md`): the real boot, headless.
//!
//! Loads the SwanStation core with a real BIOS (`--bios-dir`) and a real CHD
//! (`--rom`), runs N frames, dumps the last one as a BMP and reports what the
//! core exposes through `retro_get_memory_data` — the memory card question:
//! does `RETRO_MEMORY_SAVE_RAM` carry it, or does the plan B (core options)
//! decide?
//!
//!   cargo run -p xperience-emulation --example psx_fase0 -- \
//!       --core core/swanstation_libretro.dylib \
//!       --bios-dir "…" /bios --rom jogo.chd --save-dir "…" /saves \
//!       [--frames N] [--shot out.bmp]

use std::path::PathBuf;

use xperience_emulation::{Core, MEMORY_SAVE_RAM, MEMORY_SYSTEM_RAM};

struct Args {
    core: PathBuf,
    bios_dir: PathBuf,
    rom: PathBuf,
    save_dir: PathBuf,
    frames: u32,
    shot: Option<PathBuf>,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut it = std::env::args().skip(1);
    let mut core = None;
    let mut bios_dir = None;
    let mut rom = None;
    let mut save_dir = None;
    let mut frames = 300;
    let mut shot = None;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--core" => {
                core = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--core precisa de valor"))?
                        .into(),
                )
            }
            "--bios-dir" => {
                bios_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--bios-dir precisa de valor"))?
                        .into(),
                )
            }
            "--rom" => {
                rom = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--rom precisa de valor"))?
                        .into(),
                )
            }
            "--save-dir" => {
                save_dir = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--save-dir precisa de valor"))?
                        .into(),
                )
            }
            "--frames" => {
                frames = it
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--frames precisa de número"))?
                    .parse()?
            }
            "--shot" => {
                shot = Some(
                    it.next()
                        .ok_or_else(|| anyhow::anyhow!("--shot precisa de caminho .bmp"))?
                        .into(),
                )
            }
            other => anyhow::bail!("argumento inesperado: {other}"),
        }
    }
    Ok(Args {
        core: core.ok_or_else(|| anyhow::anyhow!("falta --core"))?,
        bios_dir: bios_dir.ok_or_else(|| anyhow::anyhow!("falta --bios-dir"))?,
        rom: rom.ok_or_else(|| anyhow::anyhow!("falta --rom"))?,
        save_dir: save_dir.ok_or_else(|| anyhow::anyhow!("falta --save-dir"))?,
        frames,
        shot,
    })
}

/// BMP 32bpp — rows already in BMP byte order for XRGB8888 (B,G,R,X);
/// RGB565 gets expanded per pixel first.
fn write_bmp(
    path: &std::path::Path,
    w: u32,
    h: u32,
    format: xperience_emulation::PixelFormat,
    pixels: &[u8],
) -> anyhow::Result<()> {
    let rgba: Vec<u8> = match format {
        xperience_emulation::PixelFormat::Xrgb8888 => pixels.to_vec(),
        xperience_emulation::PixelFormat::Rgb565 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| {
                let v = u16::from_le_bytes(*p) as u32;
                [
                    (((v >> 11) & 0x1f) * 255 / 31) as u8, // R
                    (((v >> 5) & 0x3f) * 255 / 63) as u8,  // G
                    ((v & 0x1f) * 255 / 31) as u8,         // B
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
    bmp[22..26].copy_from_slice(&(-(h as i32)).to_le_bytes()); // top-down
    bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&32u16.to_le_bytes());
    for (y, src_row) in rgba.chunks_exact(w as usize * 4).enumerate() {
        let dst = 54 + y * row;
        bmp[dst..dst + src_row.len()].copy_from_slice(src_row);
    }
    std::fs::write(path, bmp)?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = parse_args()?;
    let mut core = Core::load(&args.core)?;
    core.init();
    core.set_directories(&args.bios_dir, &args.save_dir);
    // The renderer option — the one setting a headless run cannot live
    // without if the core's default wants a GPU context. SwanStation still
    // *reads* legacy GET_VARIABLE per key even though it exposes options via
    // the v2 protocol, so a pre-seeded table reaches it.
    core.set_variable("swanstation_Renderer", "Software");
    core.load_game(&args.rom, &[])?;
    let av = core.av_info();
    println!(
        "av          : {}x{} (max {}x{}) fps={:.3} sr={:.0}",
        av.base_width, av.base_height, av.max_width, av.max_height, av.fps, av.sample_rate
    );

    let mut checksum: u64 = 0;
    let mut last = None;
    for _ in 0..args.frames {
        core.run();
        if let Some(f) = core.take_frame() {
            checksum = checksum.wrapping_add(f.pixels.iter().map(|&b| b as u64).sum::<u64>());
            last = Some(f);
        }
    }
    let frames_live = last.is_some();
    match &last {
        Some(f) => println!(
            "vídeo       : {}x{} {:?}, soma dos bytes = {checksum} (0 seria tela preta)",
            f.width, f.height, f.format
        ),
        None => println!("vídeo       : nenhum frame veio do core"),
    }
    if let (Some(shot), Some(f)) = (&args.shot, &last) {
        write_bmp(shot, f.width, f.height, f.format, &f.pixels)?;
        println!("shot        : {}", shot.display());
    }

    for (name, id) in [
        ("SAVE_RAM (memory card)", MEMORY_SAVE_RAM),
        ("SYSTEM_RAM (conquistas)", MEMORY_SYSTEM_RAM),
    ] {
        match core.memory(id) {
            Some(mem) if !mem.is_empty() => println!(
                "{name:26}: {} bytes, começa com {:02x?}",
                mem.len(),
                &mem[..mem.len().min(8)]
            ),
            Some(_) => println!("{name:26}: exposto, mas vazio (0 bytes)"),
            None => println!("{name:26}: NÃO exposto pelo core"),
        }
    }
    if !frames_live {
        anyhow::bail!("o core não entregou frames");
    }
    Ok(())
}
