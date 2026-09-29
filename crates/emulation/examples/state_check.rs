//! Sanity-check save states and SRAM against a real core + ROM.
//!
//!   cargo run -p xperience-emulation --example state_check -- <core> <rom.sfc>
//!
//! Runs to frame 600 twice — once straight, once via a save at 300 and a
//! reload — and checks the frame hashes match. Then reports SRAM size.

use std::hash::{Hash, Hasher};

use xperience_emulation::{Button, Core};

fn frame_hash(core: &mut Core) -> u64 {
    let f = core.take_frame().expect("a frame");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    f.width.hash(&mut h);
    f.height.hash(&mut h);
    f.pixels.hash(&mut h);
    h.finish()
}

fn run_n(core: &mut Core, n: u32) {
    for _ in 0..n {
        // Hold nothing; deterministic attract mode.
        for b in Button::ALL {
            core.set_button(0, b, false);
        }
        core.run();
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let core_path = args.next().ok_or_else(|| anyhow::anyhow!("need <core>"))?;
    let rom_path = args.next().ok_or_else(|| anyhow::anyhow!("need <rom>"))?;
    let rom = std::fs::read(&rom_path)?;

    let mut core = Core::load(&core_path)?;
    core.init();
    core.load_game(std::path::Path::new(&rom_path), &rom)?;

    println!("serialize size: {} bytes", core.serialize_size());
    match core.sram() {
        Some(s) => println!("SRAM: {} bytes", s.len()),
        None => println!("SRAM: none for this game"),
    }

    // Reference: straight run to 600.
    run_n(&mut core, 599);
    let reference = frame_hash(&mut core);

    // Rewound run: save at 300, roll to 600, reload, roll to 600 again.
    core.reset();
    run_n(&mut core, 300);
    let state = core.save_state().expect("save_state");
    run_n(&mut core, 299);
    let _ = frame_hash(&mut core);
    assert!(core.load_state(&state), "load_state rejected");
    run_n(&mut core, 299);
    let after_reload = frame_hash(&mut core);

    println!("frame@600 straight  = {reference:016x}");
    println!("frame@600 reloaded  = {after_reload:016x}");
    if reference == after_reload {
        println!("OK — save/load is deterministic");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "mismatch — save state is not round-tripping"
        ))
    }
}
