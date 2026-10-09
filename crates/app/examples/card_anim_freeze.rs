//! Frames da animação do memory card, headless: renderiza a cena estática
//! (o console desligado do laço do runner) com o card ENCAIXADO e a
//! animação varrendo t 0→1 nos dois sentidos (inserção e ejeção) e dumpa
//! BMPs ao longo do caminho. Nasceu como repro de um travamento que se
//! provou EXTERNO ao app (o processo foi suspenso por fora — o sample
//! anexado mostrou o laço vivo no sleep do pacing); ficou como o dumper
//! de frames do encaixe (o card entra por cima do lábio de baixo e some
//! por baixo da carcaça acima da boca — linha de encaixe em `recess.y`).
use polystnx_platform::Platform;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_panel(None, None, "Teste", &[]);
    cab.set_powered(false);

    let out = std::env::temp_dir().join("card-anim-freeze");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)?;

    // O bay VAZIO (o botão "Inserir card" é o convite do slot sem card).
    cab.set_card_labels(None);
    cab.set_card_motion(None);
    cab.present_static(0.12);
    cab.capture_static_bmp(0.12, &out.join("bay-vazio.bmp"))?;

    cab.set_card_labels(Some("test"));

    // A expulsão também passa por aqui (t 1→0): varre os dois sentidos.
    let t0 = Instant::now();
    for pass in ["insert", "eject"] {
        let inserting = pass == "insert";
        for i in 0..=120u32 {
            let t = i as f32 / 120.0;
            let p = if inserting { t } else { 1.0 - t };
            cab.set_card_motion(Some((p, inserting)));
            cab.present_static(0.12);
            if i % 30 == 0 {
                let path = out.join(format!("{pass}-{i:03}.bmp"));
                cab.capture_static_bmp(0.12, &path)?;
                eprintln!(
                    "t={p:.2} ok ({:.1} ms desde o início)",
                    t0.elapsed().as_millis()
                );
            }
        }
    }
    cab.set_card_motion(None);
    eprintln!("completo: {:?}", t0.elapsed());
    println!("{}", out.display());
    Ok(())
}
