//! Runner: cards (extraído de `runner/mod.rs`, sem mudança de comportamento).

use super::*;

/// O arquivo que o PCSX Rearmed usa para o card do SLOT 2 (o core o cria
/// no save dir com memcard2 habilitado e o escreve ao sair).
/// O arquivo do card do SLOT 1 no PCSX Rearmed — a fonte de verdade do
/// core no boot; o SAVE_RAM é espelho e NÃO sobrevive à sessão.
pub(super) const MC1_SHARED_FILE: &str = "pcsx-card1.mcd";

/// Os `.mcr` da pasta, ordenados — a fonte das duas vistas da biblioteca.
pub(super) fn list_cards(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("mcr"))
                .unwrap_or(false)
        })
        .collect()
}

/// A biblioteca com a informação de cada card (plano revision: "lista com
/// todos existentes, com as infos dos blocos salvos neles, inclusive com os
/// ícones dos jogos"): ícone do primeiro save + "N/15 blocos" no rótulo.
/// A linha 0 é sempre "(criar cartão novo)".
pub(super) type CardModalRow = (String, bool, Option<(u32, u32, Vec<u8>)>);

pub(super) fn card_rows_with_icons(
    dir: &Path,
    current: Option<&Path>,
) -> (Vec<CardModalRow>, Vec<PathBuf>) {
    let mut cards = list_cards(dir);
    cards.sort();
    let mut rows = vec![("(criar cartão novo)".to_string(), true, None)];
    for card in &cards {
        let name = card
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let info = crate::memcard::inspect(card);
        let (label, icon) = match info {
            Some(info) => {
                // Um card só vive em um slot — o rótulo diz qual.
                let where_ = if current == Some(card.as_path()) {
                    " - no slot 1"
                } else {
                    ""
                };
                (
                    format!("{name} - {}/15 blocos{}", info.used, where_),
                    info.saves
                        .first()
                        .and_then(|s| s.icon.clone())
                        .map(|d| (16u32, 16u32, d)),
                )
            }
            None => (format!("{name} - inválido"), None),
        };
        rows.push((label, true, icon));
    }
    (rows, cards)
}

// --- core em thread própria (plano revision: "faca tudo") ------------------
//
// O core bloqueia: boot, troca BIOS→jogo e FMV pesada seguram
// `core.run()` por centenas de ms — e com o loop sequencial, nada era
// redesenhado (quadro congelado, disco parado). O worker dono do core
// responde a comandos; a interface continua desenhando a 60 fps o último
// quadro recebido, com o disco girando por relógio.

/// Encaixa `path` num slot de memory card (`mem_id` 0 = slot 1, 1 = slot 2
/// no libretro): o conteúdo vira a memória da sessão e o flush passa a
/// apontar para o arquivo. Card sem arquivo ainda nasce do core na próxima
/// materialização.
pub(super) fn seat_card(
    tx: &std::sync::mpsc::Sender<CoreCmd>,
    current: &mut Option<PathBuf>,
    path: PathBuf,
) {
    // Modo Libretro: o card entra pela MEMÓRIA (WriteMem no SAVE_RAM).
    // Com o console desligado o core está só pausado — o conteúdo fica na
    // memória até o power-on. Com LIGADO, o caminho do modal usa ReseedCards
    // (WriteMem em sessão é sobrescrito pelo core — probe). A persistência
    // no arquivo do card é do flush/saída, não daqui.
    match fs::read(&path) {
        Ok(bytes) => {
            let n = bytes.len();
            let _ = tx.send(CoreCmd::WriteMem {
                id: MEMORY_SAVE_RAM,
                bytes,
            });
            log::info!("card: {} no SAVE_RAM ({n} bytes)", path.display());
        }
        Err(e) => {
            log::info!(
                "card: {} ainda não existe ({e}) — o core formata",
                path.display()
            );
        }
    }
    *current = Some(path);
}
