//! Prova headless da Fase 2: varre a `roms/` de verdade, abre o catálogo e
//! imprime o que a estante mostraria — serial, título, região, tamanho —
//! mais o hash de RA do disco.
//!
//!   cargo run -p polystnx-app --example scan_catalog

use polystnx_domain::{Catalog, Order};

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let catalog = Catalog::open(
        &polystnx_app::dirs::roms_dir(),
        &polystnx_app::dirs::library_path(),
    )?;
    println!("=== estante ({} jogo/s) ===", catalog.counts()?);
    for e in catalog.list(Order::Name)? {
        let hash = polystnx_app::ra::hash_rom(std::path::Path::new(&e.rom.path))
            .unwrap_or_else(|err| format!("<{err}>"));
        println!(
            "título : {}\n  serial  : {}\n  tamanho : {} MiB\n  ra-hash : {}",
            e.title(),
            e.rom.sha1,
            e.rom.size / (1024 * 1024),
            hash,
        );
    }
    Ok(())
}
