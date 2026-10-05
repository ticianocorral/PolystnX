//! Identificação de disco PSX (plano §2): o **serial de fábrica** é a chave —
//! `SLUS-00402` e companhia, gravados no volume do disco, legíveis de dentro
//! do CHD sem ler o disco inteiro. O cartucho de SNES se identificava por
//! hash do arquivo; o disco se identifica pelo que a fábrica imprimiu nele.

use std::path::{Path, PathBuf};

pub use polystnx_ra::hash::normalize_serial;
use polystnx_ra::hash::psx_serial;

/// Um jogo identificado: o serial canônico do primeiro disco (a identidade
/// que sustenta estante, playtime, favoritos e RA) e quantos discos são.
#[derive(Debug, Clone)]
pub struct DiscId {
    /// Serial canônico (`SLUS-00402`) do primeiro disco.
    pub serial: String,
    /// Número de discos (`.chd` solto = 1; `.m3u` = linhas `.chd`).
    pub discs: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscError {
    #[error("{0}")]
    Read(String),
}

impl DiscId {
    pub fn from_path(path: &Path) -> Result<Self, DiscError> {
        match ext(path).as_str() {
            "chd" => Ok(DiscId {
                serial: psx_serial(path).map_err(DiscError::Read)?,
                discs: 1,
            }),
            "m3u" => {
                // Um `.m3u` é UM jogo com N discos (plano §2): a identidade
                // vem do primeiro disco, que é o que o core boota.
                let discs = playlist(path).map_err(DiscError::Read)?;
                let first = discs.first().ok_or_else(|| {
                    DiscError::Read(format!("{}: m3u sem linha .chd", path.display()))
                })?;
                Ok(DiscId {
                    serial: psx_serial(first).map_err(DiscError::Read)?,
                    discs: discs.len(),
                })
            }
            other => Err(DiscError::Read(format!(
                "extensão .{other} não é disco aceito (.chd/.m3u)"
            ))),
        }
    }
}

fn ext(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// As linhas `.chd` de um `.m3u`, em ordem, resolvidas contra a pasta do
/// m3u — comentários (`#`) e linhas vazias fora.
pub fn playlist(path: &Path) -> Result<Vec<PathBuf>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if ext(Path::new(line)) != "chd" {
            continue;
        }
        out.push(dir.join(line));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_normalizes_to_the_canonical_form() {
        assert_eq!(normalize_serial("slus_004.02"), "SLUS-00402");
        assert_eq!(normalize_serial("SCUS_941.63"), "SCUS-94163");
        assert_eq!(normalize_serial(" SCES-02105 "), "SCES-02105");
        assert_eq!(normalize_serial("slps_035.82"), "SLPS-03582");
    }

    #[test]
    fn playlist_keeps_only_chd_lines_in_order() {
        let dir = std::env::temp_dir().join(format!("psx-m3u-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m3u = dir.join("Game.m3u");
        std::fs::write(
            &m3u,
            "# Comment\nGame (Disc 1).chd\n\nhttp://nope/\nnote.txt\nGame (Disc 2).chd\n",
        )
        .unwrap();
        let discs = playlist(&m3u).unwrap();
        assert_eq!(
            discs,
            vec![dir.join("Game (Disc 1).chd"), dir.join("Game (Disc 2).chd"),]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m3u_without_chd_lines_is_rejected() {
        let dir = std::env::temp_dir().join(format!("psx-m3u-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m3u = dir.join("Empty.m3u");
        std::fs::write(&m3u, "# nada\n").unwrap();
        let err = DiscId::from_path(&m3u).unwrap_err().to_string();
        assert!(err.contains("sem linha .chd"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cue_is_not_a_disc_we_accept() {
        let dir = std::env::temp_dir().join(format!("psx-cue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cue = dir.join("Game.cue");
        std::fs::write(&cue, "FILE \"g.bin\" BINARY\n").unwrap();
        let err = DiscId::from_path(&cue).unwrap_err().to_string();
        assert!(err.contains(".cue"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
