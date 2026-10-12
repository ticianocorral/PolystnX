//! PROBE: o painel de infos da estante com as linhas do HowLongToBeat, nas
//! resoluções apertadas (Steam Deck 1280×800/720) e no FHD — os rótulos
//! "zerar + extras (HLTB)" são os mais largos que a coluna estreita tem que
//! quebrar. Dados do Tekken 3 (SLUS-00402) como saem do app real.
//!
//! cargo run -p polystnx-app --example shelf_panel_probe -- 1280 800

use std::path::PathBuf;

use polystnx_platform::{PanelSection, Platform, ShelfPanelInfo};

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let w: u32 = it.next().unwrap_or_else(|| "1280".into()).parse().unwrap();
    let h: u32 = it.next().unwrap_or_else(|| "800".into()).parse().unwrap();
    let out = PathBuf::from("/tmp/shelf-probe");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("PolystnX", w, h, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("PolystnX v1.1.1-beta\nPCSX Rearmed r26");

    // As linhas na ordem em que `shelf::run` monta o painel: extras do
    // DAT/RA, fatos do arquivo, e o bloco HLTB do jogo focado (Tekken 3,
    // SLUS-00402 — os tempos reais da tabela).
    let hltb_icon = 0x484C_5442;
    if let Ok(icon) = image::load_from_memory(polystnx_app::HLTB_ICON_PNG) {
        let icon = icon.to_rgba8();
        cab.set_image(hltb_icon, icon.width(), icon.height(), icon.as_raw());
    }
    let barras = |rotulos: [(&str, u64); 4]| -> polystnx_platform::HltbBars {
        let cores = [(52, 124, 246), (86, 88, 204), (198, 60, 72), (44, 162, 156)];
        let maior = rotulos.iter().map(|(_, m)| *m).max().unwrap_or(1) as f32;
        polystnx_platform::HltbBars {
            icon: hltb_icon,
            rows: rotulos
                .into_iter()
                .zip(cores)
                .map(|((label, minutos), color)| polystnx_platform::HltbBar {
                    label: label.to_string(),
                    text: if minutos >= 60 {
                        format!("{}h {}min", minutos / 60, minutos % 60)
                    } else {
                        format!("{minutos}min")
                    },
                    frac: minutos as f32 / maior,
                    color,
                })
                .collect(),
        }
    };
    cab.set_shelf_panel(ShelfPanelInfo {
        title: "Tekken 3".to_string(),
        logo_img: None,
        cartridge_img: None,
        backcover_img: None,
        release: Some("1998".to_string()),
        info: vec![
            ("editora".to_string(), "Namco".to_string()),
            ("desenvolvedor".to_string(), "Namco".to_string()),
            ("gênero".to_string(), "Luta".to_string()),
            ("conquistas".to_string(), "30 de 58 (52%)".to_string()),
            ("disco".to_string(), "438 MB".to_string()),
            ("tempo total".to_string(), "3h 20min".to_string()),
        ],
        section: PanelSection::Informacoes,
        scroll: 0,
        favorite: Some(false),
        achievements: true,
        achievements_reason: None,
        manual_first_page: None,
        award_img: None,
        igdb: None,
        hltb: Some(barras([
            ("história principal", 119),
            ("principal + extras", 701),
            ("completista", 2024),
            ("todos os estilos", 499),
        ])),
    });

    let path = out.join(format!("painel-{w}x{h}.bmp"));
    cab.capture_shelf((28, 26, 24), |_d| {}, &path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", path.display());
    Ok(())
}
