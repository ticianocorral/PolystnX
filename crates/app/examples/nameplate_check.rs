//! Diagnóstico do nameplate: imprime o texto que o app põe no queixo da TV
//! e o que `Core::load` reporta para o core padrão da máquina.
use polystnx_app::core_update;

fn main() {
    let core_path = core_update::default_core_path();
    println!("core_path: {core_path:?}");
    println!(
        "nameplate: {:?}",
        core_update::nameplate_text(core_path.as_deref())
    );
    if let Some(p) = &core_path {
        match polystnx_emulation::Core::load(p) {
            Ok(c) => println!("system_version: {:?}", c.system_version()),
            Err(e) => println!("Core::load ERRO: {e}"),
        }
    }
}
