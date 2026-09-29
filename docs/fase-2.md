# Fase 2 — domínio de discos + estante

Portão da Fase 2 do [`plano-psx-xperience.md`](plano-psx-xperience.md): a
varredura de `roms/` vira disco — só CHD, serial como chave, tabela embutida
de títulos — e a estante herda inteira, porque a interface é a mesma.

## O que ficou pronto

- **Serial como identidade** (`xperience-ra::hash::psx_serial`): o mesmo
  caminho do `rc_hash_psx` — PVD no setor 16 → diretório raiz → `SYSTEM.CNF`
  → linha `BOOT` — lendo poucos setores do CHD, nunca o disco inteiro.
  Provado com o Tekken 3 (USA): **`SLUS-00402`**, o serial registrado no
  site da RA. Detalhe descoberto: jogos com o executável em subdiretório
  (`BOOT = cdrom:\TEKKEN3\SLUS_004.02;1`) — o serial é o nome de arquivo
  depois da última barra (o hash da RA usa o caminho inteiro; o serial, só
  o arquivo). Normalização para o formato canônico: `slus_004.02` →
  `SLUS-00402`.
- **Varredura só-CHD + m3u** (`xperience_domain::library`): `.chd` e `.m3u`
  entram; **um `.m3u` é um jogo com N discos** (identidade pelo primeiro
  disco). `.zip` saiu do mundo (não existe "extrair transparentemente" um
  disco), e cue/bin/iso/img/pbp/ecm/mds recebem aviso com a ponte:
  *"converta para .chd (chdman createcd -i arquivo.cue -o arquivo.chd)"*.
  **Symlinks agora entram** — discos moram em outro volume e chegam por
  link (`fs::metadata` segue; o do `DirEntry` não).
- **Catálogo por serial** (`catalog.rs`): a chave estável do jogo no
  sidecar (`library.json`) era o sha1 da ROM; para disco é o **serial** —
  playtime, favoritos, títulos customizados e o cache de RA ficam amarrados
  ao que o disco é, não ao nome do arquivo. O campo herdado `sha1` carrega
  o serial (nome de tempo de cartucho, papel novo).
- **Título de fábrica pela tabela embutida** (`xperience_domain::psx` +
  `scripts/gen_psx_data.py`): a tabela nasce vazia e o gerador a popula a
  partir do `gamesettings.ini` do DuckStation (ou de um CSV próprio) —
  serial → título → região deduzida do prefixo (SCUS/SLUS → USA, SCES →
  Europa, SLPS → Japão…). Sem tabela, a estante mostra o serial e o nome de
  arquivo, sem falhar. O fluxo de DAT No-Intro por CRC32 saiu do caminho do
  PSX (casamento por CRC de bins não existe para CHD).
- **Ids de textura da estante em FNV/splitmix**: cover/wheel/backcover/
  cartridge fatiavam o sha1 em hex — serial não é hex e colidiria todos os
  jogos no id 0. Agora é splitmix64 sobre a chave com um sal por uso
  (mesma fórmula do `manual_page_id`, sem o bit de família badge).
- **Renomeação canônica** (`rom_rename.rs`): mesma ação da tela de
  configurações, agora pela tabela embutida (e movendo `assets/disc/`
  junto, ao lado de cover/logo/cartridge).
- **RA por disco**: `ra::hash_rom` é o `rhash` de disco validado na Fase 0
  — a estante identifica o jogo na RetroAchievements pelo hash do disco.

## A prova

`cargo run -p xperience-app --example scan_catalog`, com o Tekken 3 na
`roms/` (por symlink) e um `.cue` intruso:

```
scan: …/tekken3-antigo.cue: .cue não é aceito — converta para .chd
      (chdman createcd -i arquivo.cue -o arquivo.chd)
=== estante (1 jogo/s) ===
título : SLUS-00402        ← tabela vazia: serial como título
  serial  : SLUS-00402
  tamanho : 438 MiB
  ra-hash : 5b0009044c8d7724518ff57e35c61af6   ← o checksum registrado na RA
```

A interface visual é a herdada, idêntica — a estante lista, busca, ordena,
favorita e abre o manual exatamente como no SNES Xperience.

## Como popular a tabela

```bash
curl -LO https://raw.githubusercontent.com/stenzek/duckstation/master/data/resources/gamesettings.ini
python3 scripts/gen_psx_data.py gamesettings.ini > crates/domain/src/psx_data.txt
cargo build   # a tabela entra no binário, portátil como no SNES
```

## Ruído conhecido (sem ação agora)

- Troca de disco por m3u dentro da sessão: o m3u já é um jogo com N discos
  no catálogo, mas o bloco "Discos" no caderno de pausa é Fase 4 (com o
  carimbo de disco dos states já esperando por ele, da Fase 1).
- `rom.rs` (identificação de cartucho SNES) sobra na árvore sem
  consumidor — remoção de limpeza, sem pressa.
