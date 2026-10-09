//! Probe: a BIOS re-escaneia o card com a tela de MC ABERTA quando o
//! conteúdo muda por baixo (WriteMem)? Compara o quadro renderizado.
//! (rode: cargo run -p polystnx-emulation --example card_swap_probe -- <core> <bios> <disc> <card.mcr>)
use polystnx_emulation::{Button, Core};
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let core_path = PathBuf::from(it.next().unwrap());
    let bios_dir = PathBuf::from(it.next().unwrap());
    let disc = PathBuf::from(it.next().unwrap());
    let card = std::fs::read(&it.next().unwrap())?;

    std::fs::create_dir_all("/tmp/csp4").ok();
    let mut core = Core::load(&core_path)?;
    core.set_directories(&bios_dir, &PathBuf::from("/tmp/csp4"));
    core.set_variable("pcsx_rearmed_show_bios_bootlogo", "disabled");
    core.set_variable("pcsx_rearmed_memcard2", "enabled");
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    core.init();
    // sessão de BIOS: disco ejetado
    core.load_game(&disc, &[])?;
    core.set_eject_state(true);

    let mut frames = 0usize;
    macro_rules! run {
        ($core:expr, $n:expr) => {{
            for _ in 0..$n {
                $core.run();
                $core.take_frame();
                frames += 1;
            }
        }};
    }
    macro_rules! tap {
        ($core:expr, $b:expr) => {{
            $core.set_button(0, $b, true);
            $core.run();
            $core.take_frame();
            frames += 1;
            $core.set_button(0, $b, false);
            $core.run();
            $core.take_frame();
            frames += 1;
            $core.run();
            $core.take_frame();
            frames += 1;
        }};
    }

    for i in [1usize, 30, 120, 300, 600] {
        while frames < i {
            core.run();
            core.take_frame();
            frames += 1;
        }
        let f = core.take_frame();
        println!(
            "f{i}: frame = {}",
            f.map(|f| format!("{}x{} {}b", f.width, f.height, f.pixels.len()))
                .unwrap_or("NONE".into())
        );
    }
    // BIOS shell no ar
    tap!(core, Button::Start); // entrar no menu da BIOS
    run!(core, 120);
    // navegar: o cursor começa em "MEMORY CARD" no shell PS1 — abrir com Start
    tap!(core, Button::Start);
    run!(core, 180); // a tela de MC lista os slots (sem card)

    let before = core.take_frame();
    let before_hash: u64 = before
        .as_ref()
        .map(|f| {
            f.pixels
                .iter()
                .step_by(97)
                .map(|&b| b as u64)
                .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
        })
        .unwrap_or(0);
    println!("[MC screen] frame {} hash {:#x}", frames, before_hash);

    // injeta o card POR BAIXO (como o ReseedCards faz)
    core.write_memory(polystnx_emulation::MEMORY_SAVE_RAM, &card);
    run!(core, 300);
    // um Start para a BIOS re-agir caso ela precise de input
    tap!(core, Button::Start);
    run!(core, 300);

    let after = core.take_frame();
    let after_hash: u64 = after
        .as_ref()
        .map(|f| {
            f.pixels
                .iter()
                .step_by(97)
                .map(|&b| b as u64)
                .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
        })
        .unwrap_or(0);
    println!(
        "[pós-injeção] frame {} hash {:#x} (mudou? {})",
        frames,
        after_hash,
        after_hash != before_hash
    );

    if let (Some(b), Some(a)) = (before.as_ref(), after.as_ref()) {
        if b.pixels.len() == a.pixels.len() {
            let diff = b
                .pixels
                .iter()
                .zip(a.pixels.iter())
                .filter(|(x, y)| x != y)
                .count();
            println!(
                "bytes diferentes nos frames: {} de {}",
                diff,
                b.pixels.len()
            );
        }
    }
    // salva os quadros para inspeção visual
    if let Some(f) = before {
        std::fs::write("/tmp/bios-mc-before.rgba", &f.pixels).ok();
        println!("before: {}x{} pitch {}", f.width, f.height, f.pitch);
    }
    if let Some(f) = after {
        std::fs::write("/tmp/bios-mc-after.rgba", &f.pixels).ok();
        println!("after: {}x{} pitch {}", f.width, f.height, f.pitch);
    }
    Ok(())
}
