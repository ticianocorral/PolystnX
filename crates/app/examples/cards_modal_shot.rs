//! Captura o picker de memory cards em jogo (com ícones) headless.
use polystnx_platform::Platform;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v0.2.0\nPCSX Rearmed r26");

    // 6 cards, vários com conteúdo — reproduz a biblioteca cheia.
    let mut pattern = vec![0u8; 131072];
    pattern[0..2].copy_from_slice(b"MC");
    let dir = std::path::Path::new("/tmp/cards-modal-memcards");
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir)?;
    for n in 1..=6 {
        let p = dir.join(format!("card-{n}.mcr"));
        // varia o conteúdo para ícones diferentes (inspect ignora cards
        // sem saves; a lista deve mostrá-los como "0/15 blocos")
        let _ = std::fs::write(&p, &pattern);
    }
    // um card com save de verdade (mock: diretório apontando pro bloco 1)
    let mut with_save = pattern.clone();
    for slot in 1..=3usize {
        let d = &mut with_save[slot * 128..(slot + 1) * 128];
        d[0] = 0x51;
        d[1] = 0x11; // em uso
        let head = &mut with_save[slot * 8192..slot * 8192 + 128];
        head[8..12].copy_from_slice(b"SLUS");
        head[0x1C..0x24].copy_from_slice(b"SAVEslot");
        for (i, b) in head[0x20..].iter_mut().enumerate() {
            *b = (i * 7 % 251) as u8;
        }
    }
    std::fs::write(dir.join("Tekken.mcr"), &with_save)?;

    cab.set_panel(None, None, "Teste", &[]);
    cab.set_powered(true);
    // ícone RGBA 16x16 (quadradinho colorido) em todas as linhas — o
    // picker real tem ícone em card com save
    let mut icon: Vec<u8> = Vec::with_capacity(16 * 16 * 4);
    for i in 0..16 * 16 {
        let x = (i % 16) as u8 * 16;
        let y = (i / 16) as u8 * 16;
        icon.extend_from_slice(&[x, y, 255, 255]);
    }
    let icon = Some((16u32, 16u32, icon));
    type Row = (String, bool, Option<(u32, u32, Vec<u8>)>);
    let mut rows: Vec<Row> = Vec::new();
    // 15 slots: 3 com save, o resto (vazio) — como o novo CardsAction monta
    for slot in 1..=15usize {
        if slot <= 3 {
            rows.push((format!("slot {slot}: Tekken 3 — SLUS-00402"), false, icon.clone()));
        } else {
            rows.push((format!("slot {slot}: (vazio)"), false, None));
        }
    }
    rows.push(("Usar neste slot".into(), true, None));
    rows.push(("Renomear".into(), true, None));
    rows.push(("Apagar".into(), true, None));
    cab.set_modal_with_icons("Tekken - 3/15 blocos", &rows);

    cab.present_modal(polystnx_platform::ModalBackdrop::Frame {
        aspect_ratio: 4.0 / 3.0,
    });
    cab.capture_screen_bmp(std::path::Path::new("/tmp/cards-modal.bmp"))?;
    println!("wrote /tmp/cards-modal.bmp");
    let _ = Duration::from_secs(0);
    Ok(())
}
