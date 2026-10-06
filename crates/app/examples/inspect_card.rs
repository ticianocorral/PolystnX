//! Mostra o que o inspect lê do card real (títulos SJIS inclusos).
use polystnx_app::memcard;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let card = PathBuf::from(std::env::args().nth(1).ok_or_else(|| anyhow::anyhow!("card"))?);
    let info = memcard::inspect(&card).ok_or_else(|| anyhow::anyhow!("card inválido"))?;
    println!("usado: {}/15", info.used);
    for s in &info.saves {
        println!("  título: '{}' | produto: '{}'", s.title, s.product);
    }
    Ok(())
}
