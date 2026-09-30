# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
versionamento por [SemVer 2.0.0](https://semver.org/lang/pt-BR/). O workspace
inteiro compartilha uma versão (`[workspace.package]` em `Cargo.toml`).

## [Não lançado]

### Fase 0 — provar as premissas

- Fork do SNES Xperience (v1.1.1): a base do app (moldura, estante, pausa,
  notas, save states, RA, packaging, CI) herda como está, renomeada para
  **PSX Xperience**.
- Núcleo trocado: snes9x → **SwanStation** (download pelo buildbot do
  libretro no menu de configurações). O nightly carrega e inicializa pela
  API libretro herdada sem nenhuma mudança no wrapper.
- Disco: **CHD como único formato** (cue/bin/iso/pbp/zip não são aceitos);
  `.m3u` fica como lista de discos de um jogo multi-disco.
- **Hash de disco da RA funcionando de ponta a ponta**: o rcheevos
  vendorizado compila agora com o `rhash` completo (sem `RC_HASH_NO_DISC`),
  e um cdreader customizado em Rust (`xperience-ra::hash`, crate `chd`) lê
  CHDs. O hash do Tekken 3 (USA) em CHD bate com o checksum registrado no
  site da RetroAchievements.
- **BIOS real boota**: com `SCPH1001.BIN` validada por MD5 em `bios/` e o
  Tekken 3 em CHD, a sonda headless (`psx_fase0`) rodou o boot de verdade —
  59.817 fps, 44.1 kHz, e o logo do PS na tela (`docs/img/fase0-boot.png`).
- **Memory card confirmado por `RETRO_MEMORY_SAVE_RAM`**: 128 KB com
  assinatura "MC", exposto pelo core e persistido pelo frontend — o desenho
  da biblioteca de cards compartilhados do plano funciona sem plano B.
  Ver `docs/fase-0.md`.
### Fase 4 — painel, cheats e RA

- **Cheats PSX embutidos**: 1844 jogos / 17.886 cheats do
  `cht/Sony - PlayStation` (libretro-database), com o matcher casando
  nomes-colados (`tekken3` ↔ `Tekken 3`). Interruptor, busca, filtro e
  persistência herdados.
- **Bloco Discos** (m3u com 2+ discos): linha "Discos" nos comandos, troca
  com a sessão viva (save_state → load_disc → load_state), fallback frio
  em caso de recusa do core.
- **Biblioteca de cards na cena**: clique no slot do console abre o modal
  "Memory Cards" (encaixado marcado, criação de "Cartão N"); trava
  só-desligado com OSD.
- RA identifica pelo hash de disco (Fase 0) sem mudança de fluxo.

### Correções do estado 0.1.1 (pós-fases)

- **A causa raiz do "quadrado verde girando"**: o default de --system-dir
  apontava saves/ (herança de SNES) — a BIOS nunca era encontrada e o boot
  travava na tela escura. Corrigido: dirs::bios_dir() no app e no emu-run.
  Prova: emu-run com --system-dir saves/ reproduziu byte a byte o frame do
  app; com bios/, o jogo. (O renderizador, a Metal e o filtro NTSC foram
  exonerados no caminho — Renderer=Software setado no runner e o filtro
  composto do blargg removido do PSX.)
- **O jogo roda no mesmo processo e janela do SNES Xperience** — o caminho
  do irmão, sem spawn nem piscada de processo.
- **TV de entrada composta**: sem sinal = tela azul escura com o selo
  **AV 1** (nada de RF/chuvisco); a estante dissolve para o AV mantendo o
  painel e o chrome contínuos (só o conteúdo do tubo troca); ligar/desligar
  colapsam para o preto, como uma TV trocando de entrada.
- Frames 565→XRGB8888 no CPU (nativo da Metal); matcher de cheats casa
  nomes-colados (tekken3 ↔ Tekken 3).

### Fase 5 — empacotar

- Release build verde; THIRD-PARTY-NOTICES reescrito para o fork
  (SwanStation GPL, rcheevos+rhash, chd/flate2, snes_ntsc herdado sem
  consumidor); ícones placeholder; desktop/DMG/AppImage/release.yml
  renomeados; CI de 3 runners herdado. `docs/fase-5.md` traz o roteiro de
  teste na janela.

### Fase 3 — a moldura PSX

- **Gabinete PSX no painel**: o console de frente em cinza SCPH-1001 —
  corpo, tampa com hub (círculo rasterizado), fenda tampa/corpo, LED de
  power (verde ligado), dois slots de memory card com portas de controle, e
  wordmark "PSX XPERIENCE" novo (pixel-font itálico) no lugar do tag SNES.
- **Disco pela fenda**: mesma matemática de animação do cartucho, semântica
  nova — o disco entra de cima e em repouso fica **inteiro escondido**
  (tampa fechada, `SEAT_HIDDEN_FRAC` > 1; fórmula de fit sem o bound de
  "sala em pé", que invertia com fração > 1). Arte do disco em
  `assets/disc/<rom>.png`.
- **Boot real da BIOS em cena**: Power mostra o boot da Sony na TV, com
  som — o core rodando a BIOS (prova visual em `docs/img/fase3-boot.png`).
- Interface visual intocada fora do bloco do console — mesma TV, mesmo
  painel, mesmos widgets (fase-3.md).

### Fase 2 — domínio de discos + estante

- **Serial como identidade**: `psx_serial` lê o serial de fábrica de dentro
  do CHD (PVD → raiz → SYSTEM.CNF; poucos setores, nunca o disco inteiro) —
  provado com o Tekken 3 (USA) = `SLUS-00402`, o serial registrado na RA.
  Subdiretórios no `BOOT` (cdrom:\TEKKEN3\SLUS_004.02) tratados.
- **Varredura só-CHD**: `.chd` e `.m3u` entram (m3u = um jogo, N discos);
  zip saiu; cue/bin/iso/etc. recebem aviso com a ponte do `chdman`;
  **symlinks entraram** (discos em outro volume).
- **Catálogo por serial**: a chave estável no sidecar (`library.json`) é o
  serial — playtime, favoritos e títulos custom amarrados ao disco, não ao
  nome do arquivo. Título pela **tabela embutida** (nascida vazia; gerador
  novo `scripts/gen_psx_data.py` lê o `gamesettings.ini` do DuckStation).
- **Estante com discos**: ids de textura em splitmix (serial não é hex),
  RA identifica pelo hash de disco validado na Fase 0, renomeação canônica
  pela tabela embutida (e move `assets/disc/` junto). Interface visual
  idêntica à do irmão de SNES.
- Prova headless: `cargo run -p xperience-app --example scan_catalog` —
  `docs/fase-2.md`.

### Fase 1 — emulador feio que funciona

- **Core options v2**: o frontend responde `GET_CORE_OPTIONS_VERSION = 2`,
  parseia `SET_CORE_OPTIONS_V2`/v1 (structs do `libretro.h` oficial) e
  povoa a tabela de opções com os defaults; override por chave intacto.
- **Analógico**: os 14 botões do DualShock (`L2`/`R2` novos), sticks via
  `RETRO_DEVICE_ANALOG` com deadzone na plataforma, gatilhos SDL como
  L2/R2, teclas `1`/`2` no teclado.
- **Biblioteca de memory cards no osso** (`emu-run --card1`): card
  `.mcr` de `memcards/` entra no `SAVE_RAM`, flush volta para o card, e a
  primeira inserção materializa o arquivo (128 KB "MC" formatado pelo
  core). Ciclo criar→recarregar provado headless.
- **Carimbo de disco nos save states**: `<slot>.disc` registra o disco;
  carregar state de outro disco avisa no OSD.
- **Run-ahead 0 por default**, confirmado por medição: 3 frames atrasados
  em 1800 com RA 0 (60 fps com folga); RA 1 inviável (414). Portão de
  desempenho da Fase 1 atravessado — `docs/fase-1.md`.

- A remover da árvore herdada conforme as fases avançam: core option
  `snes9x_blargg` (removida no fork), tabela TOSEC de SNES, geradores de
  dados de SNES, paciência.
