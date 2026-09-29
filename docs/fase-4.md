# Fase 4 — painel, cheats e RA

Portão da Fase 4 do [`plano-psx-xperience.md`](plano-psx-xperience.md): o
painel completo de PSX — cheats com interruptor, bloco **Discos** com troca
na sessão, RA de disco de ponta a ponta. A interface é a herdada; só o
conteúdo é de PSX.

## O que ficou pronto

- **Cheats PSX embutidos** (`crates/domain/src/cheats_data.txt`): gerados
  pelo `scripts/gen_cheats_data.py` (apontado para a pasta
  `cht/Sony - PlayStation` do libretro-database) — **1844 jogos, 17.886
  cheats**. O matcher ganhou a chave **sem separadores**: `tekken3` (nome
  do arquivo do disco) casa com `Tekken 3` (nome do banco) — o PSX chegou
  com nomes-colados. Interruptor, busca, filtro e persistência herdados.
- **Bloco Discos** (plano §6): um jogo `.m3u` com 2+ discos liga a linha
  **"Discos"** nos comandos; o modal lista os discos (o corrente marcado
  "— no drive") e escolher outro executa a troca **com a sessão viva**:
  `core.save_state()` → `retro_load_game` do novo disco (via
  `Core::load_disc`, novo na emulação) → `load_state` por cima — o jogo
  pede o disco seguinte com a memória onde estava. Falhou a troca? O disco
  atual volta ao drive. States continuam carimbados com o disco de origem
  (Fase 1).
- **Biblioteca de memory cards na cena** (fechamento da Fase 3, §3): o
  clique no slot do console abre "Memory Cards" — troca **só desligado**
  (trava com OSD, como o ejetar), criação de "Cartão N" na hora.
- **RA de disco**: `ra::hash_rom` é o `rhash` validado na Fase 0 —
  identificação, cache, OSD de desbloqueio, badges e progresso funcionam
  pelo hash do disco, sem mudança de fluxo (a camada RA já era genérica).

## Provas

- Frame 900 do Tekken 3 (`img/fase3-jogo.png`): logo do PlayStation na TV,
  legenda de comandos com **"Cheats"** (o banco identificou o jogo pelo
  nome do arquivo), gabinete fechado com LED.
- `cargo test`: 49 testes do app (incluindo os dois da biblioteca de
  cards), 5 do cheats sobre os dados PSX, suítes inteiras verdes.

## Como regenerar os cheats

```bash
git clone --depth 1 https://github.com/libretro/libretro-database.git /tmp/lrdb
python3 scripts/gen_cheats_data.py "/tmp/lrdb/cht/Sony - PlayStation" \
    > crates/domain/src/cheats_data.txt
```

## Ruído conhecido (sem ação agora)

- Títulos do banco chegam com sufixos `(GameShark)`/`(Game Buster)` — o
  casamento por base-name sem separadores absorve; uma tabela de títulos
  populada (`psx_data.txt`) refina o título da estante e, com ele, o
  casamento.
- A troca de disco pela indicação do próprio jogo (o jogo pede, o app
  detecta) continua fora de escopo — o bloco Discos cobre o caso real.
