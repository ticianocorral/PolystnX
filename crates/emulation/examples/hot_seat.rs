//! Diagnóstico — WriteMem no SAVE_RAM com o jogo rodando gruda no card?
//! (rode: cargo run -p polystnx-emulation --example hot_seat -- <core> <bios-dir> <rom>)

use polystnx_emulation::Core;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios"))?);
    let rom = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("rom"))?);

    let mut core = Core::load(&core_path)?;
    std::fs::create_dir_all("/tmp/hotseat-saves").ok();
    // SEM card: o arquivo não existe (como na sessão de BIOS sem encaixe)
    let _ = std::fs::remove_file("/tmp/hotseat-saves/pcsx-card1.mcd");
    core.set_directories(&bios_dir, &PathBuf::from("/tmp/hotseat-saves"));
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "disabled");
    core.set_variable("pcsx_rearmed_memcard2", "enabled");
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    core.init();
    core.load_game(&rom, &[])?;
    for _ in 0..600 {
        core.run();
        let _ = core.take_frame();
    }
    println!("jogo rodando (600 frames)");

    // card sintético com padrão reconhecível
    let mut card = vec![0u8; 131072];
    card[0..2].copy_from_slice(b"MC");
    for (i, b) in card.iter_mut().enumerate().skip(2) {
        *b = (i % 251) as u8;
    }
    let n = core.write_memory(polystnx_emulation::MEMORY_SAVE_RAM, &card);
    println!("WriteMem do card: {n} bytes");
    for _ in 0..900 {
        core.run();
        let _ = core.take_frame();
    }
    let Some(back) = core.sram() else {
        anyhow::bail!("core não expõe SAVE_RAM")
    };
    println!(
        "lido de volta: {} bytes; idêntico ao escrito: {}",
        back.len(),
        back == card
    );
    let diffs = card
        .iter()
        .zip(back.iter())
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    println!("bytes diferentes: {}", diffs.len());
    if let (Some(f), Some(l)) = (diffs.first(), diffs.last()) {
        println!("faixa: {f}..{l}");
    }
    Ok(())
}
