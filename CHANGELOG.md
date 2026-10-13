# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
versionamento por [SemVer 2.0.0](https://semver.org/lang/pt-BR/). O workspace
inteiro compartilha uma versão (`[workspace.package]` em `Cargo.toml`).

## [1.3.5-beta] - 2026-10-13

### Corrigido
- **A RAIZ DOS DADOS SAI DE ~/Documents NO macOS** — a causa de TODOS os
  congelamentos no Mac: `~/Documents` é sincronizado pelo iCloud Drive, e
  o file provider trava de forma intermitente. Um stall no meio do
  streaming do .chd congela o jogo; a thread presa em leitura
  ininterruptível torna o processo imortal em TX (nem kill -9 colhe — o
  laudo do `sample` pegou um processo com 2 minutos preso no dyld lendo o
  próprio binário de Documents). A raiz nova é
  `~/Library/Application Support/PolystnX` (nunca sincroniza) e a pasta
  antiga migra sozinha no primeiro boot — roms, saves, memcards e config
  continuam no lugar, sem cópia. Quem tiver processos zumbis em TX:
  reiniciar o Mac (o kernel não os colhe).

## [1.3.4-beta] - 2026-10-13

### Corrigido
- **macOS: remove a leitura do SMC e dá OpenGL antes do Metal** — o laudo
  do `sample` mostrou a thread do SMC presa DENTRO de uma chamada IOKit
  no kernel (processo virava imortal em TX — nem kill -9 colhia) e a main
  thread congelada no update da textura do jogo pelo caminho SDL_GPU/
  Metal. O FFI artesanal do SMC sai (temperatura no macOS fica de fora
  até adotar um crate consagrado) e o macOS passa a abrir em OpenGL como
  o Linux — o mesmo caminho GL já provado no handheld. Quem tiver os
  processos zumbis em TX precisa reiniciar o Mac para limpar (o kernel
  não os colhe).

## [1.3.3-beta] - 2026-10-13

### Corrigido
- **O congelamento era a LEITURA DE RAM, não o SMC** — `sample` nos
  processos travados cravou a main thread dentro do spawn do `ps`
  (1.3.1: `wait4`; 1.3.2: `pipe`) — o spawn de processo a cada 2 s na
  thread de UI era o verdugo. A RAM foi para o mesmo modelo da
  temperatura: worker de fundo + cache atômico, a UI lê número pronto e
  não faz spawn nunca. Nenhuma syscall de processo resta na main thread
  com o monitoramento ligado.

## [1.3.2-beta] - 2026-10-13

### Corrigido
- **App congelava ao abrir o jogo com o monitoramento ligado (macOS)** — a
  leitura do SMC rodava na thread de UI: uma chamada IOKit que decidisse
  segurar travava o app inteiro. A leitura foi para um worker de fundo (a
  cada 5 s, publicando em cache atômico) — a UI só lê o valor pronto; se a
  chamada segurar, o pior caso é o segmento CPU congelar no último valor
  ou ficar ausente, o app segue. O dicionário do IOServiceMatching agora é
  solto (CFRelease) em vez de vazar por leitura.

## [1.3.1-beta] - 2026-10-13

### Adicionado
- **Temperatura e RAM no painel de desempenho do macOS** — CPU lida
  direto do SMC (user client do IOKit, chaves Intel TCxx e Apple Silicon
  Tpxx; chave inexistente no modelo é pulada) e RAM residente via `ps`;
  cada falha vira segmento ausente, nunca erro.
- **Tampa do CD com movimento** — o vidro de acrílico aparecia/desaparecia
  a seco ao abrir e fechar; agora tem tween de opacidade (~0,35 s) nos
  dois sentidos, com o disco parando/retomando por baixo como já fazia.

### Modificado
- **A entrada da TV é AV** — o rótulo "RF NTSC" saía no log e na ajuda,
  mas nunca houve filtro RF no sinal: o vídeo é composite limpo + tubo
  CRT, e o banner de ligar já dizia "AV 1". Rótulos agora dizem a verdade
  ("entrada AV (composite) + tubo crt"). O filtro NTSC de verdade
  (crates/ntsc) continua disponível para um futuro "modo CRT pesado".

### Corrigido
- **Painel de desempenho na moldura** — saiu de DENTRO da tela da TV para
  a faixa da moldura acima dela (o "queixo de cima"), centralizado sobre
  o televisor.

## [1.3.0-beta] - 2026-10-13

### Adicionado
- **Painel de desempenho no modo dev** — nova opção "infos de
  desempenho" no menu dev (Konami code na tela inicial → Dev): liga um
  painel no TOPO CENTRAL da TV, na borda dela, com **fps do jogo**,
  **temperatura da CPU** (zonas térmicas do sysfs, preferindo zonas
  cpu/soc) e **RAM usada pelo app** (VmRSS de /proc) — atualizados a cada
  1 s (temperatura/RAM no máximo a cada 2 s). Fundo escuro translúcido
  para ler sobre qualquer cena; segmento com leitura indisponível sai da
  linha (macOS/Windows mostram só o fps). O estado vive na sessão, como
  o modo dev: não é persistido. O botão tem seleção por setas (Up/Down)
  além do clique, e o menu ganhou o segundo botão.

## [1.2.9-beta] - 2026-10-13

### Corrigido
- **Vigia confirmou: core estrangulado no gaming mode (~34 fps vs 60 no
  desktop)** — o remanescente de CPU do app sai: o `pace_frame` esperava
  girando (spin) os últimos 2 ms de CADA frame (~12% de um núcleo queimados
  a 60 fps) — agora dorme até 1 ms do fim e gira só 200 µs; e o caminho
  RGB565 (o nativo do Rearmed) convertia o quadro DUAS vezes por frame — a
  segunda cópia saiu. Não é o conserto do estrangulamento (ele é externo:
  clock/agendador do gamescope-session), mas o app deixa de ser cúmplice.

## [1.2.8-beta] - 2026-10-13

### Adicionado
- **Retrato de desempenho NA TV** — a cada 30 s de jogo, 2 s de
  "~N FPS / SOM DESCARTADO M" no chin. O número que decide o diagnóstico
  (core a 60 = problema na exibição; a 30 = core estrangulado) fica
  visível na TV, sem terminal nem arquivo de log.

### Corrigido
- **`emu-run` não gravava arquivo de log** — só stderr, que um atalho do
  Steam engole: sessões via `emu-run` ficavam sem rastro nenhum (nem o
  retrato "perf:" do runner). Agora grava `config/logs/emu-run-*.log`,
  igual ao app principal.

## [1.2.7-beta] - 2026-10-13

### Corrigido
- **Linux: vídeo x11 sempre** — a detecção do gamescope por
  `GAMESCOPE_WAYLAND_DISPLAY` (1.2.5) não é confiável no SteamOS: o env nem
  sempre chega ao app, e o gaming mode seguia no caminho wayland do SDL
  (loop interno nominal, exibição desabando). Agora o hint
  `SDL_VIDEO_DRIVER=x11` vale em todo Linux — XWayland no Plasma e no
  gamescope, o caminho batido de todo emulador Steam. Um
  `SDL_VIDEODRIVER` no ambiente vence.

## [1.2.6-beta] - 2026-10-13

### Adicionado
- **Aviso de abertura no chin: versão + renderer** — toda inicialização
  mostra "{Marca} vX.Y.Z" no verde do OSD e o renderer ativo abaixo, por
  5 s, em todas as telas e capturas (o aviso degradado de software toma o
  lugar e os 10 s quando for o caso). "Em qual binário eu estou?" tem
  resposta na TV, não no log — relatos de "continuou lento" começam
  sabendo com o que.

## [1.2.5-beta] - 2026-10-13

### Corrigido
- **O atualizador in-app nunca aplicava no Linux/AppImage** — `current_exe()`
  dentro de um AppImage em execução aponta para o monte efêmero somente
  leitura (`/tmp/.mount_XXXX/...`): a cópia do update falhava, o erro era
  engolido (melhor-esforço) e a versão velha seguia rodando calada — o app
  "atualizava", reiniciava, e nada mudava. Agora o alvo é o arquivo real do
  AppImage (`$APPIMAGE`, exportado pelo runtime; binário solto continua no
  `current_exe`), gravado via temp + rename atômico.
- **Gaming mode do Steam Deck: XWayland** — sob o gamescope o app agora pede
  o driver de vídeo `x11` (detectando o gamescope pelo env
  `GAMESCOPE_WAYLAND_DISPLAY` que ele exporta): o caminho wayland do SDL
  apresenta mal ali — o loop interno segue nominal e a TELA desaba em
  judder/câmera lenta. XWayland é o caminho batido de todo emulador Steam.
  Fora do gamescope nada muda; `SDL_VIDEODRIVER` no ambiente vence.

### Adicionado
- **Retrato de desempenho no log a cada 10 s** durante o jogo — runs/s,
  renderer ativo, fila e descartes de áudio: qualquer log de sessão passa a
  carregar o diagnóstico sem reproduzir com TRACE ligado.

## [1.2.4-beta] - 2026-10-13

### Corrigido
- **Jogo em câmera lenta com som engasgado no gaming mode do Steam Deck**
  (1.2.3-beta não resolveu) — a velocidade do jogo É o tick do loop de UI
  (um Run do core por tick): se a apresentação desaba no gamescope, tudo
  desaba junto. O OpenGL passa na frente do SDL_GPU/Vulkan na lista de
  renderer no Linux (XWayland/GLX é o caminho batido do gamescope; o
  Vulkan 2D ali é o suspeito) e o vsync sai de vez, explícito
  (`SDL_RENDER_VSYNC=0`) — um `present` bloqueando em FIFO derruba o tick
  à metade sozinho. Nada muda com um `SDL_RENDER_DRIVER`/`SDL_RENDER_VSYNC`
  no ambiente (prioridade DEFAULT).

### Adicionado
- **Vigia de câmera lenta** — o core entregando menos de ~55 quadros/s por
  3 s seguidos (fora do boot e de janelas furadas por pausa/modal) mostra
  "DESEMPENHO REDUZIDO / JOGO EM CÂMERA LENTA (~N FPS)" no chin por 6 s e
  registra WARN no log com o nome do renderer ativo e os quadros de áudio
  descartados no período, no máximo uma vez por minuto. Acabou a era do
  "não resolveu" sem dado: o aviso diz o que está acontecendo e com qual
  renderer.

## [1.2.3-beta] - 2026-10-13

### Adicionado
- **Renderer de hardware por padrão + aviso de modo degradado** — o app pede
  ao SDL a lista `gpu,opengl,opengles2,software` (hint `SDL_RENDER_DRIVER`,
  prioridade DEFAULT: o que o ambiente definir vence). Em software o gabinete
  CRT (warp em malha + véus de alpha por frame) satura a CPU e compete com o
  core do emulador — era o travamento do app no gaming mode do Steam Deck,
  onde o SDL não achava driver de hardware sozinho. Se ainda assim o renderer
  subir em software (último nome da lista), o app registra WARN no log e
  mostra "SEM ACELERAÇÃO DE GPU / DESEMPENHO REDUZIDO" no chin por 10 s
  depois de abrir, em todas as telas e nas capturas.

## [1.2.2-beta] - 2026-10-12

### Removido
- **Run-ahead** — o recurso rodava o core um frame especulativo à frente
  (serializando um savestate de 2 MB por frame) para esconder ~16 ms de
  lag de input. Custo de CPU/energia alto e benefício imperceptível: nas
  máquinas lentas derrubava o FPS (o travamento do AppImage no Steam
  Deck). O jogo agora roda sempre à velocidade cheia, com o menor gasto
  possível; a opção `runahead` sai do `polystnx.cfg`, da CLI e da tela
  de configurações.

## [1.2.1-beta] - 2026-10-12

### Corrigido
- **Jogo rodando lento no Steam Deck (AppImage)** — o run-ahead (default 1)
  serializa um savestate de 2 MB por frame a mais; no APU do Deck o core
  estourava o budget de 16,7 ms e o FPS despencava. O run-ahead agora é
  **auto-ajustável**: a main cronometra o serviço do core (Run → quadro
  pronto) e, após 30 frames seguidos acima do budget, desliga o run-ahead
  na hora (registro INFO no log). Em máquinas rápidas nada muda.

## [1.2.0-beta] - 2026-10-11

### Adicionado
- **Infos unificadas embutidas, offline** — nome canônico, lançamento,
  editora, região, **nota (IGDB)** e os tempos do HowLongToBeat numa
  tabela única por serial (10.638 jogos de PS1), gerada do DAT No-Intro +
  DuckStation + IGDB e compilada dentro do binário: instala, solta a ROM
  e as infos estão lá — **sem DAT e sem rede**. Os módulos `psx`, `tosec`
  e `hltb` saem, absorvidos pela tabela (`infos.rs`).
- **Seções no painel de infos** — **Informações Gerais**, **IGDB** (com a
  logo do press kit) e **HowLongToBeat** (com o ícone do site), títulos em
  negrito e todo o conteúdo centralizado; ano como campo normal e as notas
  onde existem (a IGDB só as tem para os jogos com votos da comunidade).
- **Botão de play no hover** — ícone de CD com seta no canto inferior
  direito da capa sob o cursor; o contorno amarelo de seleção sai.
- **Cobertura opcional pela API da IGDB** — com credenciais gratuitas da
  Twitch (`IGDB_CLIENT_ID`/`IGDB_CLIENT_SECRET`), o gerador baixa a
  plataforma 7 inteira e completa as notas que o espelho não tem.

### Modificado
- **A estante virou estante de verdade** — prateleiras de madeira com fita
  LED e mão francesa sob a grade "todos os jogos", capas 3D transparentes
  pousadas com um suporte de acrílico na base; as faixas de favoritos e
  jogados recentemente ficam planas, sem prateleira.
- **3 caixas grandes por prateleira no Steam Deck** (720p/800p) e 5 nas
  telas maiores, com a largura do item mandando no desenho (e não mais as
  constantes de tile).
- Linha de conquistas com a medalha centralizada junto ao valor.

### Corrigido
- Capa 3D transparente: a matte cinza do render não vira mais franja
  alrededor da caixa (limpeza no upload + sangria de borda), e a
  transparência sobrevive a voltar de um jogo.
- Ruído do tubo não atravessa mais a estante (assenta opaca) — com capas
  vazadas, a estática aparecia como "borda cinza" sob as capas.

## [1.1.1-beta] - 2026-10-10

### Adicionado
- **Site oficial no GitHub Pages**, no estilo do manual do proprietário do
  SixteeN: <https://ticianocorral.github.io/PolystnX/> — capa, sumário,
  seções numeradas e contracapa, com telas 100% sintéticas (o homebrew
  fictício *Mundo do Tomate* rendido dentro do app real), o GIF do disco
  assentando no spindle e o selo que puxa a versão da última release.
  Nenhum jogo comercial, nenhuma ROM.

### Removido
- **O som do Reset e do open/close da tampa**: as duas ações ficam mudas —
  seguem como feedback o movimento real da tampa (o disco para e retoma
  pelo disk control), o OSD "TAMPA ABERTA/FECHADA" e o flash do botão. O
  foley `reset.wav` sai do binário com a variante `Sfx::Reset`; os cliques
  sintetizados do abrir/fechar (`eject_clunk`/`tone_click`) saem órfãos.

### Corrigido
- **Painel de jogo na Steam Deck (1280×800/720p)**: o console sobe (a logo
  reserva menos) e o bloco ganha altura; o slot do controle quebra o status
  em duas linhas — "CONTROLE 1" / "CONECTADO" em escala cheia (a linha
  única saía cortada no "…CONECT"); os botões recolhem para as bordas ao
  redor de um **disco ~20% maior**; o **Reset cresce** (raio 24→29); a
  lista de comandos sobe. FHD+ não muda nada.

## [1.1.0-beta] - 2026-10-09

### Adicionado
- **Vibração DualShock**: o core pede a interface de rumble do libretro e
  o gamepad da entrada 1 responde (motores forte/fraco, drenados a cada
  quadro, zerados na troca de core/jogo). No painel, o botão ANALOG liga
  as duas coisas juntas — analógico + vibração (o LED vermelho vira
  verde).
- **Log em arquivo**: todo lançamento escreve `polystnx-<timestamp>.log`
  em `logs/` na pasta de config — lançado pelo Finder o stderr se perde,
  e o rastro do fluxo de card/boot precisava sobreviver a isso.

### Modificado
- **O memory card é decisão do jogador**: os slots nascem VAZIOS em todo
  jogo — nada de auto-encaixe pelo `config.toml`, o mecanismo do apagão
  de saves (`card_slot1/2` ficam no arquivo por compatibilidade, sem
  efeito). Inserir card é manual, pelo modal de MC no painel.
- **O console tem UMA porta de card** (o slot 2 saiu do painel) e o card
  encaixado ganhou corpo de verdade: logo em silhueta preta, adesivo
  caseiro com o nome e o gesto "Trocar/Ejetar" impresso no plástico.
- **"Ligar sem disco" saiu do setup** — junto com o toggle "Controle 2"
  das configurações e as flags `--bios`/`--card2` do emu-run: o suporte
  ao segundo controle foi removido.
- **Leitor**: no core leve (PCSX Rearmed) a leitura do disco se deduz do
  conteúdo — tela congelada (grade de luma 48×36 idêntica) + áudio mudo
  por 1 s ligam o loop; hesitação (250 ms) e vão lento (70 ms) ficam como
  rede para boot/FMV/máquina fraca.
- **RetroAchievements**: jogo identificado SEM set publicado (caso
  Spawn: The Eternal) não é "versão sem suporte" — sem botão Conquistas
  e sem tally "0 de 0".
- Setup: os botões se agrupam em telas apertadas — rótulos descritivos
  só quando cabem (Steam Deck: ficavam por cima do texto).
- Ícones e logo com a arte nova (P + PolyStnX, fundo transparente) em
  todos os formatos — macOS, Windows, Linux/Flatpak, cabeçalho e tampa.

### Corrigido
- **O sync do card não perde mais saves**: slot com o magic (51 00) —
  como o Tekken 3 grava os saves — é EM USO; o contador antigo o tratava
  como livre-apagado. Títulos de card em Shift-JIS full-width
  (ＴＥＫＫＥＮ) são normalizados para ASCII.

## [0.1.3] - 2026-09-30

### Adicionado
- Nomes canônicos No-Intro via DAT Redump "Sony - PlayStation" (espelho
  libretro): o disco casa pelo **serial de fábrica** (`SLUS-00402` →
  "Tekken 3 (USA)"), na estante, no painel do jogo e na busca de arte
  (`assets/*/Tekken 3 (USA).png`). Casamento também pela **base** do
  serial (variantes `…D`/`…GH`) e com preferência determinística pela
  entrada **retail** sobre Beta/Demo/Proto.
- "Renomear ROMs para o padrão No-Intro" resolve o nome pelo DAT e move
  saves, notas e arte junto.
- Memory card do **slot 2** funcional (os botões "MC slot 1 / MC slot 2"
  do painel abrem a biblioteca do slot correspondente).

### Modificado
- Painel sem o corpo do console: só Power/Reset/Eject; disco menor
  girando entre os botões (~1 volta/s).
- Pasta de updates do app na raiz (`<raiz>/update/`), não dentro de
  `saves/` — o mesmo conserto do SNES Xperience 1.1.5, com migração no
  arranque.
- Corrigido: abrir jogo com arte de disco não fecha mais o app (pânico
  no dissolve quando o tubo não tinha quadro da estante).

## [0.1.2] - 2026-09-30

### Corrigido (revisão)
- A pasta de updates do app saiu de `saves/update/` e agora é
  `<raiz>/update/` — o mesmo conserto do SNES Xperience 1.1.5 (saves
  guardam progresso de jogo, não pacote do app). Um pacote pendente na
  pasta antiga migra sozinho no arranque, antes do apply; a pasta antiga
  sai se restar vazia.

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
