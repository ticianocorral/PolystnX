//! Diagnóstico de persistência do card: o que chega ao ARQUIVO pcsx-card1.mcd após o drop (spoiler: nada — o core nunca o escreve).

use polystnx_emulation::Core;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios"))?);
    let rom = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("rom"))?);

    let saves = "/tmp/mcdpersist-saves";
    std::fs::create_dir_all(saves).ok();
    let mcd = format!("{saves}/pcsx-card1.mcd");

    // card base: o card REAL do usuário (formatado pelo app, diretório
    // válido com checksums)
    let base = std::fs::read("/Users/ticiano/Documents/PolystnX/memcards/Fight Games.mcr")?;
    std::fs::write(&mcd, &base)?;

    let mut core = Core::load(&core_path)?;
    core.set_directories(&bios_dir, &PathBuf::from(saves));
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "disabled");
    core.set_variable("pcsx_rearmed_memcard2", "enabled");
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    core.init();
    core.load_game(&rom, &[])?;
    for _ in 0..300 {
        core.run();
        let _ = core.take_frame();
    }

    // "save" sintético: SÓ DADOS (bloco do slot 5), sem tocar o diretório
    // validado — o core não valida dados de bloco, só o diretório.
    let mut saved = base.clone();
    let head = 5 * 8192;
    for (i, b) in saved[head..head + 512].iter_mut().enumerate() {
        *b = (i as u8).wrapping_mul(7).wrapping_add(1);
    }
    let n = core.write_memory(polystnx_emulation::MEMORY_SAVE_RAM, &saved);
    println!("WriteMem (save simulado): {n} bytes");
    for _ in 0..60 {
        core.run();
        let _ = core.take_frame();
    }
    // sram() em vida: reflete o save?
    match core.sram() {
        Some(live) => println!("sram() em vida == save: {}", live == saved),
        None => println!("sram() em vida: None"),
    }
    // drop: o deinit escreve o arquivo com quê?
    drop(core);
    let file = std::fs::read(&mcd)?;
    println!("arquivo pós-drop == save: {}", file == saved);
    println!("arquivo pós-drop == base (sem save): {}", file == base);
    let diff = saved
        .iter()
        .zip(file.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    println!("bytes diferentes (save vs arquivo): {}", diff.len());
    if let (Some(f0), Some(fl)) = (diff.first(), diff.last()) {
        println!("faixa: {f0}..{fl}");
    }
    Ok(())
}
