//! Repro do travamento ao tirar o disco com o console ligado (report do
//! usuário, commit 1f1f1de): carrega o jogo, roda frames, ejeta a bandeja
//! pela disk control interface (fora do `retro_run`, como o worker faz) e
//! continua rodando — depois fecha a bandeja de volta.
//!
//!   cargo run -p polystnx-emulation --example eject_repro -- \
//!       --core core/pcsx_rearmed_libretro.dylib \
//!       --bios-dir .../bios --rom "....chd" [--pre N] [--post N]

use std::path::PathBuf;
use std::time::Instant;

use polystnx_emulation::Core;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let mut core_path = None;
    let mut bios_dir = None;
    let mut rom = None;
    let mut pre = 300u32;
    let mut post = 1200u32;
    let mut cpu = None;
    let mut dump = false;
    let mut close_too = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--core" => core_path = Some(it.next().ok_or(anyhow::anyhow!("falta valor"))?.into()),
            "--bios-dir" => {
                bios_dir = Some(it.next().ok_or(anyhow::anyhow!("falta valor"))?.into())
            }
            "--rom" => rom = Some(it.next().ok_or(anyhow::anyhow!("falta valor"))?.into()),
            "--pre" => pre = it.next().ok_or(anyhow::anyhow!("falta valor"))?.parse()?,
            "--post" => post = it.next().ok_or(anyhow::anyhow!("falta valor"))?.parse()?,
            "--cpu" => cpu = Some(it.next().ok_or(anyhow::anyhow!("falta valor"))?.to_string()),
            "--dump-vars" => dump = true,
            "--close-too" => close_too = true,
            o => anyhow::bail!("argumento inesperado: {o}"),
        }
    }
    let core_path: PathBuf = core_path.ok_or(anyhow::anyhow!("--core obrigatório"))?;
    let bios_dir: PathBuf = bios_dir.ok_or(anyhow::anyhow!("--bios-dir obrigatório"))?;
    let rom: PathBuf = rom.ok_or(anyhow::anyhow!("--rom obrigatório"))?;
    let save_dir = std::env::temp_dir().join("psx-eject-repro");
    std::fs::create_dir_all(&save_dir).ok();

    let mut core = Core::load(&core_path)?;
    core.set_directories(&bios_dir, &save_dir);
    core.init();
    core.set_variable("pcsx_rearmed_nocdaudio", "disabled");
    if let Some(cpu) = &cpu {
        core.set_variable("pcsx_rearmed_frameskip_type", "disabled");
    }
    core.load_game(&rom, &[])?;
    eprintln!("carregado: {}", rom.display());
    if dump {
        for (k, v) in core.variables() {
            eprintln!("opcao: {k} = {v}");
        }
    }
    eprintln!(
        "disk control: get_eject={:?} num_images={:?}",
        core.get_eject_state(),
        core.num_images()
    );

    let t = Instant::now();
    for i in 0..pre {
        core.run();
        core.take_frame();
        if i % 60 == 0 {
            eprintln!("pre {i} ({:.1}s)", t.elapsed().as_secs_f32());
        }
    }
    eprintln!("pré: {pre} frames em {:.1}s", t.elapsed().as_secs_f32());

    eprintln!("EJETANDO a bandeja…");
    let t = Instant::now();
    let ok = core.set_eject_state(true);
    eprintln!(
        "set_eject_state(true) -> {ok:?} ({:.3}s)",
        t.elapsed().as_secs_f32()
    );

    let t = Instant::now();
    for i in 0..post {
        core.run();
        let f = core.take_frame();
        if i % 60 == 0 {
            eprintln!(
                "pós-ejeção {i} frame={:?} ({:.1}s)",
                f.map(|f| (f.width, f.height)),
                t.elapsed().as_secs_f32()
            );
        }
    }
    eprintln!("pós-ejeção ok: {post} frames");

    if close_too {
        eprintln!("FECHANDO a bandeja (mesmo disco)…");
        let t = Instant::now();
        let ok = core.set_eject_state(false);
        eprintln!(
            "set_eject_state(false) -> {ok:?} ({:.3}s)",
            t.elapsed().as_secs_f32()
        );

        let t = Instant::now();
        for i in 0..pre {
            core.run();
            core.take_frame();
            if i % 60 == 0 {
                eprintln!("pós-inserção {i} ({:.1}s)", t.elapsed().as_secs_f32());
            }
        }
    }
    eprintln!("FIM — sem travamento");
    Ok(())
}
