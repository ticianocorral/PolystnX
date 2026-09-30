//! Título de fábrica por serial — a tabela embutida do PSX Xperience
//! (equivalente do `tosec.rs` de SNES: "ano e editora de fábrica, sem DAT
//! nenhum"). Gerada por `scripts/gen_psx_data.py` a partir do
//! `gamesettings.ini` do DuckStation (banco de seriais aberto); até ser
//! gerada, a estante mostra o serial e o nome do arquivo, sem falhar.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Um jogo conhecido pelo serial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsxGameInfo {
    /// Nome canônico ("Tekken 3").
    pub title: String,
    /// Região deduzida do prefixo do serial (`USA`, `Europa`, `Japão`…).
    pub region: Option<String>,
}

/// `SERIAL\tTítulo[\tRegião]` por linha, `#` comenta. Estático: entra no
/// binário, não custa re-parsing por chamada.
static TABLE: LazyLock<HashMap<&'static str, (&'static str, Option<&'static str>)>> =
    LazyLock::new(|| parse_table(include_str!("psx_data.txt")));

fn parse_table(text: &'static str) -> HashMap<&'static str, (&'static str, Option<&'static str>)> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split('\t');
        let (Some(serial), Some(title)) = (parts.next(), parts.next()) else {
            continue;
        };
        map.insert(serial, (title, parts.next().filter(|r| !r.is_empty())));
    }
    map
}

/// O jogo do serial, se a tabela embutida o conhece.
pub fn lookup(serial: &str) -> Option<PsxGameInfo> {
    TABLE.get(serial).map(|(title, region)| PsxGameInfo {
        title: (*title).to_string(),
        region: region.map(|r| r.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_looks_up_to_none_without_panicking() {
        // A tabela nasce vazia (comentários só); lookup é sempre None até o
        // gerador rodar. Quando `psx_data.txt` ganhar dados, este teste
        // continua válido — só deixa de ser o caso interessante.
        assert_eq!(lookup("SLUS-00402"), None);
    }

    #[test]
    fn table_parser_tolerates_comments_and_missing_region() {
        let table = parse_table("# comentário\n\nSLUS-00402\tTekken 3\tUSA\nSCES-02105\tGame\t\n");
        assert_eq!(table.get("SLUS-00402").unwrap().0, "Tekken 3");
        assert_eq!(table.get("SLUS-00402").unwrap().1, Some("USA"));
        assert_eq!(table.get("SCES-02105").unwrap().1, None);
        assert!(!table.contains_key("NOPE"));
    }
}
