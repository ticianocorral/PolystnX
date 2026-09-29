# PSX Xperience

O segundo console da família **Retro Xperience** — emuladores-portáteis
onde o console inteiro vive na tela, com **a mesma interface** do
[SNES Xperience](https://github.com/ticianocorral/snes-xperience): gabinete,
tubo CRT, painel de controle, estante, caderno de pausa. Muda o aparelho
dentro da cena: no PSX, a tampa abre, o disco entra com a etiqueta impressa,
os memory cards ocupam os slots — e o Power mostra o boot real da BIOS.
Projeto pessoal, sem fins comerciais.

**Status: Fase 0** (provar as premissas). O plano completo do fork está em
[`docs/plano-psx-xperience.md`](docs/plano-psx-xperience.md).

## Decisões fechadas

- **Núcleo**: SwanStation via libretro, baixado pelo próprio app (buildbot do
  libretro). Nunca distribuído com o app.
- **Disco**: só `.chd` (conversão de cue/bin com `chdman`, fora do app).
  Multi-disco por `.m3u` apontando os CHDs do jogo.
- **BIOS**: fornecida por quem roda, em `bios/`, validada por MD5 nas
  Configurações. Nunca distribuída nem baixada pelo app.
- **Memory cards**: biblioteca de cards compartilhados (`.mcr` de 128 KB em
  `memcards/`), dois slots físicos na cena — como o console original. Nunca
  um card por jogo.

## Compilar

Precisa de Rust estável e do SDL3.

```bash
brew install sdl3   # macOS
cargo build && cargo test
```

Sem SDL3 no sistema, compile-o junto (precisa de CMake + toolchain C):

```bash
cargo build --features xperience-platform/vendored-sdl
```

`emu-run` roda um disco solto sem o resto do app:

```bash
cargo run --bin emu-run -- --core caminho/swanstation_libretro.dylib --rom jogo.chd
```

Nenhum core, disco ou BIOS é distribuído com o projeto — ver
[`THIRD-PARTY-NOTICES.md`](THIRD-PARTY-NOTICES.md).
