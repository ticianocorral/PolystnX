//! Lista as core options declaradas pelo core.
use polystnx_emulation::Core;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let core_path = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("core"))?,
    );
    let mut core = Core::load(&core_path)?;
    core.init();
    for (k, v) in core.variables() {
        println!("{k} = {v}");
    }
    Ok(())
}
