# Fase 0 — provar as premissas

Portão da Fase 0 do [`plano-psx-xperience.md`](plano-psx-xperience.md):
provar, com código e medição, que as quatro premissas do fork se sustentam
antes de investir na moldura.

## 1. O SwanStation carrega pela API libretro atual — ✅ PROVADO (macOS arm64)

Zero linhas mudadas no wrapper (`crates/emulation`). O core nightly do
buildbot do libretro (`swanstation_libretro.dylib`, 1.0.0 b6c30a7) carrega e
inicializa pela sonda que já existia no projeto:

```bash
curl -LO https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/swanstation_libretro.dylib.zip
cargo run -p xperience-emulation --example probe -- caminho/swanstation_libretro.dylib
```

```
library      : SwanStation 1.0.0 b6c30a7
extensions   : exe, psexe, cue, bin, img, iso, chd, pbp, ecm, mds, psf, m3u
retro_init   : ok
```

A lista de extensões confirma `chd` e `m3u` — o app reforça "só CHD" do lado
dele. Pendente: os outros dois sistemas (trabalho do CI).

## 2. A BIOS boota até o logo — ✅ PROVADO

Com a BIOS real do usuário (`SCPH1001.BIN`, 512 KB, MD5
`924e392ed05558ffdb115408c263dccf` — o checksum canônico do SCPH1001 USA
v2.0, confirmado na lista do [RetroPie](https://github.com/RetroPie/retropie-bios))
em `bios/` e o Tekken 3 (USA) em CHD, a sonda headless
(`crates/emulation/examples/psx_fase0.rs`) boota o console de verdade:

![O logo do PlayStation, renderizado pelo SwanStation com a BIOS real](img/fase0-boot.png)

- `av: 256x239 (max 1024x512) fps=59.817 sr=44100` — NTSC de PSX, 44.1 kHz.
- Vídeo vivo em 640×472 RGB565 (upscale 2× do renderer de software — a core
  option `swanstation_Renderer` = `Software` chega ao core pela tabela de
  variáveis legacy mesmo com o core expondo opções via v2).
- A raiz de dados herda o layout do plano: `~/Documents/PSX Xperience/`
  com `bios/`, `core/`, `roms/`, `saves/`.

## 3. O hash de disco da RA bate — ✅ PROVADO (incluindo CHD)

**O maior risco técnico do plano está dissipado.** O rcheevos vendorizado
compilava com `RC_HASH_NO_DISC`; o fork removeu o define, compilou o `rhash`
(`hash.c`, `hash_disc.c`, `hash_rom.c`, `cdreader.c`) e registrou um
**cdreader customizado em Rust** (`crates/ra/src/hash.rs`) que lê CHD com o
crate puro-Rust `chd` e delega cue/bin/iso ao leitor padrão do rcheevos.

Duas provas, ambas em `cargo test -p xperience-ra`:

1. **Disco sintético** (`tests/hash.rs`): um PSX-ISO mínimo construído
   byte a byte no teste (PVD, diretório raiz, SYSTEM.CNF, exe com header
   PS-X EXE) hasheia exatamente o MD5 que o algoritmo `rc_hash_psx` manda:
   `MD5(exe_name ++ exe_bytes)`. Duas pegadinhas documentadas pelo caminho:
   o campo de tamanho do header PS-X EXE é lido **little-endian** pelo
   rcheevos, e records ISO9660 têm padding par *antes* do byte de tamanho.
2. **CHD real**: `PSX_TEST_CHD=~/Downloads/tekken3.chd cargo test -p
   xperience-ra` — o Tekken 3 (USA) hasheia para
   `5b0009044c8d7724518ff57e35c61af6`, que é o checksum ISO registrado para
   o jogo no próprio site da
   [RetroAchievements](https://retroachievements.org/game/9142). O leitor
   CHD acertou a estrutura de primeira: hunk de 19584 B = 8 frames × 2448 B,
   usuário do MODE2_RAW em `frame[24..2072]`, pregaps fora do espaço de
   hunks (a matemática do `logical_bytes` do disco confirma).

Sem zlib em C, sem libchdr — o CHD é lido em Rust e o algoritmo é o C
vendorizado, o mesmo do RetroArch.

## 4. Memory card via `RETRO_MEMORY_SAVE_RAM` — ✅ PROVADO

A mesma sonda do item 2 respondeu a pergunta da biblioteca de cards:

```
SAVE_RAM (memory card)  : 131072 bytes, começa com [4d, 43, …]  ← "MC"
SYSTEM_RAM (conquistas) : 2097152 bytes                          ← 2 MB
```

- O SwanStation **expõe o memory card por `RETRO_MEMORY_SAVE_RAM`** —
  128 KB exatos, assinatura `"MC"` de card formatado. É o caminho principal
  do plano: a biblioteca de cards grava/lê esse buffer com a tubulação
  `flush_sram` que o app já tem. Plano B (core options de memcard
  compartilhado) não será preciso para o básico.
- `SYSTEM_RAM` = 2 MB — a memória que os `MemAddr` dos sets de PSX avaliam;
  o runner já alimenta `with_memory(MEMORY_SYSTEM_RAM)` genérico.
- `saves/` ficou vazio: quem persiste o card é o **frontend** (flush
  periódico), não o core — exatamente o desenho da biblioteca compartilhada
  (`memcards/*.mcr`) do plano.

## Fechado. Próximo: Fase 1

Emulador feio que funciona, agora com carne: core options v2 (o
`GET_CORE_OPTIONS_VERSION = 0` atual serve o SwanStation só pela tabela
pré-semeada), biblioteca de memcards no osso (`.mcr` em `memcards/`),
analógico (sticks + L2/R2), save states grandes com carimbo de disco,
run-ahead medido — e o jogo a jogo na janela de verdade.
