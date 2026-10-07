//! A BIOS do console (plano revision: "menu de escolher bios padrão" +
//! "mostrar informação sobre a bios"): lista os arquivos de `bios/`,
//! extrai a informação impressa na própria ROM ("System ROM Version 4.5
//! 05/25/00 A" → versão, data e região) e materializa a escolhida como a
//! BIOS ativa — um diretório de *staging* com um symlink só dela, que é o
//! `system_dir` que o core recebe (assim a escolha vale sem tocar nos
//! arquivos do usuário).

use std::fs;
use std::path::{Path, PathBuf};

/// Uma BIOS encontrada em `bios/`.
#[derive(Clone)]
pub struct BiosInfo {
    pub file: String,
    /// "4.5" (a que a ROM imprime; vazia quando não imprime versão).
    pub version: String,
    /// "05/25/00" (idem).
    pub date: String,
    /// Região impressa na ROM: EUA, Europa, Japão ou "?".
    pub region: String,
    /// Tamanho em KB (512 para as oficiais).
    pub kb: u64,
}

impl BiosInfo {
    /// Rótulo para a linha de menu: "SCPH-101.BIN — v4.5 05/25/00 — EUA".
    pub fn label(&self) -> String {
        let mut s = self.file.clone();
        if !self.version.is_empty() {
            s.push_str(&format!(" — v{}", self.version));
        }
        if !self.date.is_empty() {
            s.push_str(&format!(" {}", self.date));
        }
        s.push_str(&format!(" — {} ({} KB)", self.region, self.kb));
        s
    }
}

/// Há uma BIOS utilizável em `dir`? (.bin presente com tamanho de ROM —
/// as oficiais têm 512 KB; qualquer coisa abaixo de 128 KB não é BIOS.)
/// (plan revision: "liberar continuar somente com a bios".)
pub fn any_installed(dir: &Path) -> bool {
    list(dir).iter().any(|b| b.kb >= 128)
}

/// Lista as BIOS de `dir` (`.bin`/`.BIN`), com a informação da ROM.
pub fn list(dir: &Path) -> Vec<BiosInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let ext = path
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| x.to_ascii_lowercase())
            .unwrap_or_default();
        if ext != "bin" {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let (version, date, region) = parse_rom_string(&bytes);
        out.push(BiosInfo {
            file: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            version,
            date,
            region,
            kb: bytes.len() as u64 / 1024,
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

/// A string "System ROM Version X.Y MM/DD/YY R" gravada na própria ROM.
fn parse_rom_string(bytes: &[u8]) -> (String, String, String) {
    let Some(start) = bytes.windows(19).position(|w| w == b"System ROM Version ") else {
        return (String::new(), String::new(), "?".into());
    };
    let text: String = bytes[start..start + 64]
        .iter()
        .take_while(|&&b| b != 0 && b != b'\n' && b != b'\r')
        .map(|&b| b as char)
        .collect();
    let mut version = String::new();
    let mut date = String::new();
    let mut region = "?".to_string();
    let mut next_is_version = false;
    for tok in text.split_whitespace() {
        if tok == "Version" {
            next_is_version = true;
        } else if next_is_version {
            version = tok.to_string();
            next_is_version = false;
        } else if tok.contains('/') && tok.len() == 8 {
            date = tok.to_string();
        }
    }
    if let Some(last) = text.chars().last() {
        region = match last {
            'A' => "EUA".into(),
            'E' => "Europa".into(),
            'J' => "Japão".into(),
            _ => "?".into(),
        };
    }
    (version, date, region)
}

/// O `system_dir` que o core deve receber: o diretório de staging com o
/// symlink da BIOS escolhida (ou `dir` raiz quando nenhuma escolha/erro —
/// o core varre a pasta como sempre fez).
pub fn selected_system_dir(dir: &Path, chosen: Option<&str>) -> PathBuf {
    let Some(chosen) = chosen else {
        return dir.to_path_buf();
    };
    let staging = dir.join(".selected");
    if fs::create_dir_all(&staging).is_err() {
        return dir.to_path_buf();
    }
    // limpa links antigos (qualquer arquivo no staging)
    if let Ok(entries) = fs::read_dir(&staging) {
        for e in entries.flatten() {
            let _ = fs::remove_file(e.path());
        }
    }
    let target = dir.join(chosen);
    if !target.is_file() {
        return dir.to_path_buf();
    }
    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(&target, staging.join("SCPH1001.BIN")).is_err() {
            return dir.to_path_buf();
        }
    }
    #[cfg(not(unix))]
    {
        if fs::copy(&target, staging.join("SCPH1001.BIN")).is_err() {
            return dir.to_path_buf();
        }
    }
    staging
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_rom_string_with_region() {
        let mut rom = vec![0u8; 1024];
        let s = b"System ROM Version 4.5 05/25/00 A";
        rom[..s.len()].copy_from_slice(s);
        let (v, d, r) = parse_rom_string(&rom);
        assert_eq!(v, "4.5");
        assert_eq!(d, "05/25/00");
        assert_eq!(r, "EUA");
    }

    #[test]
    fn parses_a_bios_without_region_letter() {
        let mut rom = vec![0u8; 1024];
        let s = b"System ROM Version 1.0";
        rom[..s.len()].copy_from_slice(s);
        let (v, d, r) = parse_rom_string(&rom);
        assert_eq!(v, "1.0");
        assert_eq!(d, "");
        assert_eq!(r, "?");
    }
}
