//! Dump a CHD's header and CD track metadata — the Fase 0 ground truth for
//! the custom cdreader (`src/hash.rs`).
//!
//!   cargo run -p xperience-ra --example dump_chd -- <file.chd>

use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: dump_chd <file.chd>")?;
    let mut chd = chd::Chd::open(File::open(&path)?, None)?;
    let mut meta_file = File::open(&path)?;
    let h = chd.header();
    println!("version      : {}", h.version() as u32);
    println!(
        "hunks        : {} x {} bytes",
        h.hunk_count(),
        h.hunk_size()
    );
    println!("logical bytes: {}", h.logical_bytes());
    println!("--- metadata ---");
    for entry in chd.metadata_refs() {
        let md = entry.read(&mut meta_file)?;
        println!(
            "[tag {:?}] {}",
            md.metatag,
            String::from_utf8_lossy(&md.value).trim_end()
        );
    }
    Ok(())
}
