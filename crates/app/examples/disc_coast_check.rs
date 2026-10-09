//! Sonda headless da inércia do disco no DESLIGAR (dev/QA): liga o console
//! com disco assentado, deixa o giro assentar, clica o Power e captura a
//! estática em quatro instantes. O esperado: o disco SEGUE girando após o
//! clique (coast da inércia, ~2 s até parar) e congela PARADO NO ÂNGULO —
//! quadros idênticos depois da parada, diferentes durante o coast.
//! `cargo run --example disc_coast_check -- [dir]`

use std::path::{Path, PathBuf};
use std::time::Duration;

use polystnx_platform::{Cabinet, Platform};

/// Arte de disco de mentira — CD prateado com furo central e UMA marca
/// escura fora do centro: sem ela o disco é radialmente simétrico e girar
/// não muda um pixel (a sonda precisava enxergar o ângulo).
fn disc_art() -> (u32, u32, Vec<u8>) {
    let (w, h) = (512u32, 512u32);
    let mut img = image::RgbaImage::new(w, h);
    let (cx, cy) = (256.0f32, 256.0f32);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
        let mut v = if d < 26.0 {
            40
        } else if d < 60.0 {
            150
        } else {
            215
        };
        if d >= 60.0 && x > cx as u32 && (y as f32 - cy).abs() < 12.0 {
            v = 60; // marca de fase: fende o disco em dois arcos distintos
        }
        let a = if d > 250.0 { 0 } else { 255 };
        *px = image::Rgba([v, v, v, a]);
    }
    (w, h, img.into_raw())
}

/// Estática de nível 0 é azul chapado (update_noise_tex) — a ÚNICA coisa
/// que muda entre dois quadros é o ângulo do disco no painel. O BMP do SDL
/// não carrega timestamp: bytes iguais ⇔ quadro congelado (a contagem aqui
/// nem precisa decodificar pixel — basta a diferença bruta).
fn diff_bytes(a: &Path, b: &Path) -> anyhow::Result<usize> {
    let a = std::fs::read(a)?;
    let b = std::fs::read(b)?;
    anyhow::ensure!(a.len() == b.len(), "tamanhos divergem");
    Ok(a.iter().zip(&b).filter(|(p, q)| p != q).count())
}

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp".into()));
    std::fs::create_dir_all(&dir)?;
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab: Cabinet = plat
        .create_cabinet("PolystnX", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let (w, h, rgba) = disc_art();
    cab.set_panel(None, Some((w, h, &rgba)), "Sonda do Coast", &[]);

    // Liga: o giro só tem alvo 420 °/s com o console LIGADO (panel.powered).
    cab.set_powered(true);

    // Arrancada: ~1.5 s de rampa até o giro cheio (280 °/s²).
    let shot = |cab: &mut Cabinet, name: &str| -> anyhow::Result<PathBuf> {
        let p = dir.join(name);
        cab.capture_static_bmp(0.0, Path::new(&p))?;
        println!("captura {}", p.display());
        Ok(p)
    };
    let pace = |cab: &mut Cabinet, frames: usize| {
        for _ in 0..frames {
            cab.present_static(0.0);
            std::thread::sleep(Duration::from_millis(16));
        }
    };

    pace(&mut cab, 90);
    let a = shot(&mut cab, "coast-a-girando.bmp")?;

    // O clique do Power: desliga e o coast tem de continuar na estática.
    cab.set_powered(false);
    pace(&mut cab, 30); // ~0.5 s depois do clique
    let b = shot(&mut cab, "coast-b-coastando.bmp")?;
    pace(&mut cab, 150); // ~2.4 s a mais — coast de 420 °/s a 210 °/s² já acabou
    let c = shot(&mut cab, "coast-c-parado.bmp")?;
    std::thread::sleep(Duration::from_millis(300));
    let d = shot(&mut cab, "coast-d-congelado.bmp")?;

    let (ab, bc, cd) = (
        diff_bytes(&a, &b)?,
        diff_bytes(&b, &c)?,
        diff_bytes(&c, &d)?,
    );
    println!("diff A→B (coast logo após o clique): {ab}");
    println!("diff B→C (ainda freando):            {bc}");
    println!("diff C→D (parado no ângulo):         {cd}");

    let ok = ab > 0 && bc > 0 && cd == 0;
    println!(
        "{}",
        if ok {
            "OK: coast com inércia no desligar"
        } else {
            "FALHOU"
        }
    );
    anyhow::ensure!(ok, "o disco não coasta como esperado");
    Ok(())
}
