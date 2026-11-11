//! Teste de interação do Cheater Fish: agenda cliques sintéticos e imprime
//! o que ficou ligado. `RUST_LOG=debug cargo run -p polystnx-app --example fish_click`
fn main() -> anyhow::Result<()> {
    env_logger::init();
    let mut plat =
        polystnx_platform::Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let defs = polystnx_domain::cheats_for_title("Tekken 3 (USA)");
    let mut state = vec![false; defs.len()];
    // 3 s: depois do carregamento (2,6 s). Linhas do painel em janela 1280x800.
    for (i, y) in [265, 300, 335].into_iter().enumerate() {
        plat.push_synthetic_click_later(
            450,
            y,
            std::time::Duration::from_millis(3200 + i as u64 * 400),
        );
    }
    // Pula o carregamento e termina escolhendo "Start Game" (1ª linha).
    plat.push_synthetic_click_later(450, 232, std::time::Duration::from_millis(5000));
    let out =
        polystnx_app::cheater_fish::run(&mut plat, &mut cab, "Tekken 3 (USA)", &defs, &mut state)?;
    println!(
        "saiu: {out:?}; ligados: {:?}",
        state
            .iter()
            .enumerate()
            .filter(|(_, &b)| b)
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    );
    Ok(())
}
