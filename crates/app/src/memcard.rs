//! Leitura e gestão de memory cards PSX (`.mcr`, 128 KB — plano §3 e plan
//! revision "crud para memory cards"). O formato: 16 blocos de 8 KB, cada
//! bloco 64 quadros de 128 bytes. O bloco 0 é o cabeçalho ("MC" + strings
//! da Sony) e os seus quadros 1..15 são o DIRETÓRIO: 128 bytes por slot —
//! estado (livre/apagado/em uso), código de produto e título. O bloco de
//! cada save repete o título no seu quadro 0 e traz a paleta (16 cores
//! RGB555) + os ícones 16×16 4bpp nos quadros 1..3.

use std::io;
use std::path::{Path, PathBuf};

/// Tamanho canônico do card: 16 blocos × 8 KB.
pub const CARD_SIZE: usize = 131_072;
const BLOCK: usize = 8_192;
const FRAME: usize = 128;

/// Um save dentro do card (um "bloco" do diretório).
pub struct CardSave {
    /// Título do save (campo SJIS do card — na prática quase sempre ASCII;
    /// bytes inválidos caem no código de produto).
    pub title: String,
    /// Código de produto ("SCUS-94163") ou identificador do save.
    pub product: String,
    /// Ícone 16×16 em RGBA (16×16×4 bytes), primeiro ícone do save.
    pub icon: Option<Vec<u8>>,
}

/// O retrato de um card no disco.
pub struct CardInfo {
    /// Quantos dos 15 slots de save estão em uso.
    pub used: u8,
    /// Os saves, na ordem do diretório.
    pub saves: Vec<CardSave>,
}

/// Lê e interpreta um card. `None` se o arquivo não é um card válido
/// (tamanho errado ou sem o "MC" mágico do bloco 0).
pub fn inspect(path: &Path) -> Option<CardInfo> {
    let bytes = fs_read(path)?;
    if bytes.len() != CARD_SIZE || &bytes[0..2] != b"MC" {
        return None;
    }
    let mut saves = Vec::new();
    for slot in 1..=15usize {
        // O quadro `slot` do bloco 0 é a entrada de diretório do slot.
        let dir = &bytes[slot * FRAME..(slot + 1) * FRAME];
        let (b0, b1) = (dir[0], dir[1]);
        // Livre: (00 00) card fresco, (A0 00) formatado-vazio (o padrão da
        // BIOS do Rearmed ao formatar), (51 51) apagado-fresco, (FF FF)
        // fim-de-cadeia. QUALQUER outro magic = em uso — o Tekken 3, por
        // exemplo, grava os saves com (51 00) e a BIOS lê normalmente (o
        // filtro antigo tratava 0x51 como apagado universal e os saves do
        // Tekken desapareciam do seletor).
        if (b0 == 0 && b1 == 0)
            || b0 == 0xA0
            || (b0 == 0x51 && b1 == 0x51)
            || (b0 == 0xFF && b1 == 0xFF)
        {
            continue;
        }
        let save = &bytes[slot * BLOCK..(slot + 1) * BLOCK];
        let head = &save[0..FRAME];
        // Um slot em uso tem conteúdo no quadro 0 do seu bloco (título,
        // produto, paleta) — guarda contra entradas órfãs.
        if head.iter().skip(8).all(|&b| b == 0) {
            continue;
        }
        // Título/produto em ASCII (jogos ocidentais) OU Shift-JIS (jogos
        // japoneses gravam SJIS mesmo em discos US — o Tekken 3 grava até o
        // CÓDIGO de produto em full-width: "ＥＫＫＥＮ　...").
        let code = readable(trim_ascii(&head[0x08..0x14]));
        let sjis_code = shift_jis_title(&head[0x08..0x14]);
        let code = if !code.is_empty() {
            code
        } else if !sjis_code.is_empty() {
            sjis_code
        } else {
            trim_ascii(&head[0x14..0x1C])
        };
        let ident = trim_ascii(&head[0x14..0x1C]);
        let ascii = readable(trim_ascii(&head[0x1C..0x38]));
        let sjis = shift_jis_title(&head[0x1C..0x38]);
        let title = if !ascii.is_empty() {
            ascii
        } else if !sjis.is_empty() {
            sjis
        } else {
            code.clone()
        };
        saves.push(CardSave {
            title,
            product: if code.is_empty() { ident } else { code },
            icon: decode_icon(head, save),
        });
    }
    Some(CardInfo {
        used: saves.len() as u8,
        saves,
    })
}

/// Cria um card formatado e vazio ("Cartão N.mcr", primeiro nome livre) e
/// devolve o caminho. O cabeçalho é o frame canônico "MC" + strings Sony;
/// o diretório nasce todo livre e o corpo é 0xFF, como os cards de fábrica.
pub fn create_blank(dir: &Path) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let n = (1..)
        .find(|n| !dir.join(format!("Cartão {n}.mcr")).exists())
        .unwrap_or(1);
    let path = dir.join(format!("Cartão {n}.mcr"));

    let mut card = vec![0u8; CARD_SIZE];
    card[0..2].copy_from_slice(b"MC");
    let sony1 = b"Sony Electronic Inc.";
    let sony2 = b"Sony Computer Entertainment";
    card[4..4 + sony1.len()].copy_from_slice(sony1);
    card[28..28 + sony2.len()].copy_from_slice(sony2);
    card[0x70..0x72].copy_from_slice(&[0x00, 0xFF]);
    // Corpo livre: a partir do bloco 1, tudo 0xFF — o diretório (quadros
    // 1..15 do bloco 0) já nasce 00 00, as entradas livres.
    for b in card[BLOCK..].iter_mut() {
        *b = 0xFF;
    }
    std::fs::write(&path, card)?;
    Ok(path)
}

/// Apaga o arquivo do card.
pub fn delete(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}

/// Renomeia o card (o stem vira o nome; `.mcr` preservado). Devolve o novo
/// caminho. Nomes de arquivo proibidos viram "_".
pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let clean: String = new_name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    let clean = clean.trim();
    if clean.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nome vazio"));
    }
    let new_path = path.with_file_name(clean).with_extension("mcr");
    std::fs::rename(path, &new_path)?;
    Ok(new_path)
}

/// O nome do card (stem do arquivo) — o que vai no adesivo do slot.
/// `None` para caminho vazio/sem nome.
pub fn card_name(path: Option<&Path>) -> Option<String> {
    path.and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
}

fn fs_read(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn trim_ascii(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let trimmed: &[u8] = bytes[..end].strip_suffix(b" ").unwrap_or(&bytes[..end]);
    String::from_utf8_lossy(trimmed).into_owned()
}

/// Título gravado em Shift-JIS (jogos japoneses/europeus com saves JP),
/// decodificado para a UI. Vazio se os bytes não formarem SJIS razoável
/// (títulos ASCII caem no `readable` antes de chegarem aqui).
fn shift_jis_title(bytes: &[u8]) -> String {
    let trimmed: &[u8] = {
        let end = bytes
            .iter()
            .rposition(|b| *b != 0 && *b != 0x20 && *b != 0x81 && *b != 0x40)
            .map_or(0, |p| p + 1);
        &bytes[..end]
    };
    let decoded = encoding_rs::SHIFT_JIS.decode(trimmed).0.into_owned();
    let cleaned = decoded.trim().to_string();
    // SJIS decodifica quase qualquer coisa: aceita só se ficou razoável
    // (sem os losangos de substituição em excesso).
    let bad = cleaned.matches('\u{FFFD}').count();
    if !cleaned.is_empty() && bad * 4 <= cleaned.chars().count() {
        cleaned
    } else {
        String::new()
    }
}

/// SJIS cai mal em UTF-8: só aceita título ASCII imprimível; o resto vira
/// vazio e o chamador usa o código de produto no lugar.
fn readable(s: String) -> String {
    let ok = !s.is_empty() && s.chars().all(|c| c.is_ascii_graphic() || c == ' ');
    if ok {
        s
    } else {
        String::new()
    }
}

/// Decodifica o primeiro ícone do save: paleta RGB555 de 16 cores no quadro
/// 0 (0x60..0x80) e o bitmap 4bpp 16×16 no quadro 1. Índice 0 com o bit
/// STP ligado é transparente; o resto é opaco.
fn decode_icon(head: &[u8], save: &[u8]) -> Option<Vec<u8>> {
    let mut palette = [0u16; 16];
    for (i, c) in palette.iter_mut().enumerate() {
        let at = 0x60 + i * 2;
        if at + 1 >= head.len() {
            return None;
        }
        *c = u16::from_le_bytes([head[at], head[at + 1]]);
    }
    if palette.iter().all(|&c| c == 0) {
        return None; // save sem ícone
    }
    let bitmap = &save[FRAME..FRAME + 128];
    let mut rgba = vec![0u8; 16 * 16 * 4];
    for px in 0..256usize {
        let v = palette[((bitmap[px / 2] >> if px % 2 == 0 { 4 } else { 0 }) & 0xF) as usize];
        let (r, g, b) = (
            ((v >> 10) & 0x1F) as u32,
            ((v >> 5) & 0x1F) as u32,
            (v & 0x1F) as u32,
        );
        let a = if (v & 0x8000) != 0 && v & 0x7FFF == 0 {
            0
        } else {
            255
        };
        let at = px * 4;
        rgba[at] = (r * 255 / 31) as u8;
        rgba[at + 1] = (g * 255 / 31) as u8;
        rgba[at + 2] = (b * 255 / 31) as u8;
        rgba[at + 3] = a;
    }
    Some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um card sintético com um save (título, produto, paleta e ícone) —
    /// a base da validação do `inspect`.
    fn card_with_one_save() -> Vec<u8> {
        let mut card = vec![0u8; CARD_SIZE];
        card[0..2].copy_from_slice(b"MC");
        // diretório do slot 1: 54 51 = em uso
        card[FRAME] = 0x54;
        card[FRAME + 1] = 0x51;
        let block_at = BLOCK;
        card[block_at + 0x08..block_at + 0x08 + 10].copy_from_slice(b"SLUS-00879");
        card[block_at + 0x1C..block_at + 0x1C + 17].copy_from_slice(b"FINAL FANTASY VII");
        // paleta: 16 cores RGB555 (0 = preto opaco; sem STP nenhuma)
        for i in 0..16u16 {
            let v = (i << 10) | (i << 5) | i;
            card[block_at + 0x60 + (i as usize) * 2] = (v & 0xFF) as u8;
            card[block_at + 0x61 + (i as usize) * 2] = (v >> 8) as u8;
        }
        // ícone: índice crescente 1..16
        for (px, b) in card[block_at + FRAME..block_at + FRAME + 128]
            .iter_mut()
            .enumerate()
        {
            *b = ((px % 8) as u8) << 4 | ((px + 4) % 8) as u8;
        }
        card
    }

    #[test]
    fn inspect_extracts_title_product_and_icon() {
        let dir = std::env::temp_dir().join(format!("mc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Teste.mcr");
        std::fs::write(&path, card_with_one_save()).unwrap();

        let info = inspect(&path).expect("card válido");
        assert_eq!(info.used, 1);
        let save = &info.saves[0];
        assert_eq!(save.title, "FINAL FANTASY VII");
        assert_eq!(save.product, "SLUS-00879");
        let icon = save.icon.as_ref().expect("ícone decodificado");
        assert_eq!(icon.len(), 16 * 16 * 4);
        // opaco em todo lugar (paleta sem índice 0+STP)
        assert!(icon.chunks(4).all(|c| c[3] == 255));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn inspect_rejects_garbage() {
        let dir = std::env::temp_dir().join(format!("mc-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lixo.mcr");
        std::fs::write(&path, b"a").unwrap();
        assert!(inspect(&path).is_none());
        // 128 KB sem o "MC" mágico: rejeitado também
        let path = dir.join("sem-mc.mcr");
        std::fs::write(&path, vec![0u8; CARD_SIZE]).unwrap();
        assert!(inspect(&path).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rename_sanitizes_and_keeps_extension() {
        let dir = std::env::temp_dir().join(format!("mc-test3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Velho.mcr");
        std::fs::write(&path, card_with_one_save()).unwrap();
        let new = rename(&path, "Meu/Card:2").expect("renomeia");
        assert_eq!(new.file_name().unwrap(), "Meu_Card_2.mcr");
        assert!(new.exists());
        assert!(!path.exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
