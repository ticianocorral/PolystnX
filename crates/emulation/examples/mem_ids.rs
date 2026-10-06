//! O que o core expõe por id de memória? (0=SAVE_RAM, 1=RTC, 2=SYSTEM_RAM)
use polystnx_emulation::{Core, MEMORY_SAVE_RAM, MEMORY_SYSTEM_RAM};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("core"))?);
    let bios_dir = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("bios"))?);
    let rom = PathBuf::from(it.next().ok_or_else(|| anyhow::anyhow!("rom"))?);
    let mut core = Core::load(&core_path)?;
    std::fs::create_dir_all("/tmp/memids-saves").ok();
    core.set_directories(&bios_dir, &PathBuf::from("/tmp/memids-saves"));
    core.set_variable("pcsx_rearmed_memcard2", "enabled");
    core.init();
    core.load_game(&rom, &[])?;
    for id in [MEMORY_SAVE_RAM, MEMORY_SAVE_RAM + 1, MEMORY_SYSTEM_RAM] {
        match core.memory(id) {
            Some(data) => println!(
                "id {id}: {} bytes, head {:02x?}",
                data.len(),
                &data[..8.min(data.len())]
            ),
            None => println!("id {id}: None"),
        }
    }
    Ok(())
}
