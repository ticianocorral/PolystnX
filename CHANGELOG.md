# Changelog

Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/);
versionamento por [SemVer 2.0.0](https://semver.org/lang/pt-BR/). O workspace
inteiro compartilha uma versão (`[workspace.package]` em `Cargo.toml`).

## [Não lançado]

### TV
- A imagem do jogo (e da estante) agora preenche os cantos da tela da TV: o
  barril do tubo não deixa mais o recesso arredondado nos cantos; o contorno
  fica reto e a curvatura só aparece dentro da imagem.

### Cheats
- Jogo sem cheats no banco: o Cheater Fish abre mesmo assim,   "No Cheat Codes Available" (antes pulava direto para o jogo).
- O Cheater Fish agora acha os cheats de jogos cujo nome no disco não tem o
  número da sequência que o banco usa (ex.: "Crash Bandicoot - Warped" no
  banco é "Crash Bandicoot 3 - Warped").
- Banco de cheats atualizado com o libretro-database inteiro do PlayStation
  (de 1844 para 2642 jogos, incluindo "Crash Bandicoot - Warped (USA)").

### Console
- **Tampa de acrílico de verdade**: o vidro sobre o disco ganhou reflexos em
  faixas, aro e brilho de domo (a logo do app continua nele); ao abrir a
  tampa (OPEN), o acrílico aparece levantado na dobradiça, com o disco livre.

### Estante
- Removido o rótulo "todos os jogos" acima da grade da estante.
- **Temas de móvel da estante, portados do SixteeN**: além do visual padrão
  do PlayStation, a estante pode ser uma estante de madeira, o expositor de
  metal da locadora, um rack de som preto, o quarto anos 90 (Memphis) ou um
  fliperama com neon. Configurações > estante > "Tema da estante" (clique ou
  setas) e `shelf_theme` no `polystnx.cfg` (`ps1`, `wood`, `metal`, `hifi`,
  `memphis`, `arcade`; valor desconhecido mantém o tema atual). O padrão
  segue idêntico ao de antes.
- **Botões "fósforo" em vermelho**: "histórico", "atualizar" e as setas das
  faixas ficam sem caixa, só o rótulo com sublinhado, como no SixteeN (lá é
  violeta; aqui, vermelho).
  O mesmo vale para "voltar" e "página" do manual/conquistas, o zoom da capa
  traseira e os botões da tela de primeiro uso.

## [1.4.1-beta] - 2026-11-11

Retoques do console e do Cheater Fish, filtro AV e o ANALOG de verdade.

### Controle
- **O botão ANALOG do console agora é o do DualShock de verdade**: ao clicar,
  o core troca a porta 1 para DualShock (os jogos compatíveis passam a ler os
  sticks e a pedir vibração) e o controle vibra ao entrar no modo analógico;
  desligar volta ao controle digital. A troca de memory card com o console
  ligado mantém o modo.

### Vídeo composto (AV)
- **Filtro de cabo AV (amarelo/branco/vermelho) na imagem do jogo**: a cor
  borra e atrasa na horizontal, a luma suaviza um pouco e a saturação cai
  levemente. Sem neve nem fantasma (isso é RF, e o PlayStation usa AV).
  Ligado por padrão; Configurações > jogo > "Video composto (cabo AV)" e
  `av_composto` no cfg.

### Arte padrão (disco no console)
- **Jogo sem arte de disco agora também tem o CD no console**: o disco que
  gira na face do console e entra no slot é um CD prata gerado por código,
  com o nome do jogo impresso (antes só a estante tinha o CD genérico).

### Console
- **Botões POWER, RESET e OPEN** como as peças do PS1: plástico fosco com
  degradê, sem aro claro, e as palavras em **Inter Bold** (fonte vetorial
  com antialias, embutida; atribuição em `THIRD-PARTY-NOTICES.md`). O RESET
  se ajusta para caber no botão e o POWER afunda quando o console liga.
- Toda a escrita da parte do console (ANALOG, controle, memory card, nome no
  adesivo) usa a mesma fonte vetorial.
- Logo do memory card: só o símbolo "P", na proporção da arte, em silhueta.
- As caixas de controle e memory card ficaram centralizadas no painel e 10
  px mais baixas (mais respiro do CD).

### TV
- **Brilho do vidro igual ao do SixteeN**: mancha de luz difusa no alto à
  esquerda, faixa diagonal fina e aro claro rente à borda, sobre a mesma
  malha curva da imagem (malha em cache, sem custo por quadro).
- O "AV 1" ficou mais afastado do canto superior direito.

### Cheater Fish
- **O menu lateral do console não tem mais "Cheats"**: tudo é escolhido no
  Cheater Fish, que aparece a cada vez que o console é ligado. A busca e os
  filtros do modal antigo saíram junto.
- Texto da lista (e das demais telas) com fonte suave, sem o serrilhado.

## [1.4.0-beta] - 2026-11-10

Auditoria de segurança alinhada com o SixteeN (1.3.2 → 1.8.1).

### TV (do SixteeN, sem canal — o PolystnX entra pela AV)
- **Botões físicos no queixo**: POWER (com o LED verde) e VOL-/VOL+,
  prata sobre plástico escuro. POWER liga e desliga o tubo com a animação
  de aquecer e de colapsar (vidro morto curvado pela malha do tubo); com a
  TV desligada o jogo segue rodando, sem imagem nem som.
- **Ligar e desligar como uma TV de tubo do fim dos anos 90** (sem a janela
  que abria e fechava): ao desligar, a PRÓPRIA imagem é esmagada na
  vertical — com as cores ainda aparecendo — até virar uma linha clara que
  estoura o brilho, a linha encolhe num ponto e o ponto some devagar
  (persistência do fósforo); ao ligar, o tubo fica escuro por um instante e
  a imagem sobe do preto ganhando brilho. `examples/tv_shot.rs off|on K`
  fotografa quadro a quadro.
- **Volume** 0..16 com o OSD de barrinhas verdes sobre a imagem; o áudio do
  jogo passa pelo volume (cheio por padrão — o mesmo som de antes).
- **MENU** (à esquerda do POWER): abre o menu de imagem — BRILHO,
  CONTRASTE, COR e NITIDEZ de 0 a 100 (50 = neutro). MENU passa para o
  próximo item e fecha depois do último (ou sozinho, parado por 6 s);
  VOL-/VOL+ ajustam o item enquanto o menu está aberto. Os ajustes
  filtram o quadro do jogo no upload e só custam algo fora do neutro.
- Saíram do queixo a versão/renderer da abertura e o "~N FPS / SOM
  DESCARTADO" a cada 30 s (o aviso de renderer por software, que é
  degradação de verdade, continua).
- O upload do quadro não derruba mais o app (perda do dispositivo de
  vídeo pula o quadro) e perdeu o código de debug.
- **Marca prateada "PolyStnX" no meio do queixo**, cromado escovado como a
  do SixteeN (arte gerada por `examples/gen_brand.rs` a partir do nome da
  logo do console, 330 KB embutidos). Cede ao selo do RA e ao OSD quando
  eles estão na tela e some se não couber entre a versão e os controles.
- O volume e o ajuste de imagem da TV são lembrados entre sessões
  (`tv_volume` e `tv_picture` no `polystnx.cfg`, gravados ao sair).
- Os avisos do queixo (RA, OSD) param antes dos controles.
- `crates/app/examples/tv_shot.rs`: fotos do queixo e do vidro pelo caminho
  vivo do `present`.

### Manual do console
- **Botão "Manual do console" na tela inicial** (acima de Configurações),
  que abre no tubo o manual do PolystnX — 12 páginas: primeiros passos,
  estante, ligar o console, TV, Cheater Fish, controles, configurações,
  conquistas e cards, e problemas comuns. O mesmo leitor dos manuais da
  estante (‹ ›, voltar); alcançável pelo controle.
- O PDF é embutido no binário (`assets/console-manual-pt.pdf`) e gerado por
  `cargo run -p polystnx-app --release --example gen_console_manual`, a
  partir de capturas em `packaging/manual-console/shots/`.

### Arte padrão
- **Jogo sem arte vira um CD-R de prata numa caixa de CD transparente**, com
  o nome e o ano escritos à caneta no disco (preta, azul ou vermelha, tremida
  e inclinada — o sorteio vem do próprio jogo, então é sempre igual). Vale
  para a grade e para a faixa de recentes; sai o modo "lista" que aparecia
  quando nenhum jogo tinha capa.

### Cheater Fish
- **Tela de cheats antes do jogo, no estilo GameShark** (o equivalente do
  Cheat Genius do SixteeN): ao ligar o console com um disco que tem cheats,
  aparece primeiro um carregamento com o peixe de óculos (logo em
  pixel-art) e, direto, a lista dos cheats disponíveis para aquele jogo —
  liga/desliga cada um (A ou clique), "Start Game" ou B começam o jogo com o
  que ficou ligado. Fundo azul manchado, molduras chanfradas de prata, linha
  amarela de seleção, caixa de informação ("N Cheat Codes Available / M Are
  On") e o texto de ajuda rolando embaixo.
- **Visual de PS1**: as telas são renderizadas num quadro pequeno (metade
  do tubo) por um rasterizador de software (`psx_gpu.rs`) com cor de 15
  bits, dither 4x4 da GPU do PlayStation nos degradês, fonte bitmap de
  limiar duro com sombra seca, mapa de textura afim ondulando no fundo e
  molduras que tremem um pixel, e depois ampliadas sem filtro
  (`Cabinet::update_live_image`) — parecem as de uma ROM de 32 bits.
- Só no primeiro ligar da sessão (desligar e ligar de novo retoma), e nunca
  com o RetroAchievements em hardcore. Opção `cheater_fish` no cfg e em
  Configurações > jogo (ligada por padrão; jogo sem cheats não mostra nada).
- Diferente do SixteeN, não é um programa emulado: o PS1 não tem um disco de
  trapaça para rodar antes, então a tela é interface do app; as escolhas
  valem para o jogo que vai começar. `examples/fish_shot.rs` fotografa as
  telas.

### Configurações (organização do SixteeN)
- As seções seguem a ordem do SixteeN — **controles, jogo, imagem e som,
  estante, conquistas, sistema** — mais **bios**, que só existe no PS1.
  Abre em "controles"; **LB/RB** trocam de seção; B sai das configurações
  de qualquer seção (antes "controles" voltava para "jogo").
- **imagem e som** reúne tela cheia, silenciar o jogo e a TV: volume e
  brilho, contraste, cor e nitidez (setas ajustam, o clique avança e volta
  ao começo; as linhas acompanham os botões do queixo).
- **estante** ficou com o "Renomear ROMs"; **sistema** com o núcleo e a
  checagem de updates; **jogo** com o Cheater Fish.
- Só entram opções que fazem sentido no PlayStation: nada de mau contato,
  tema da estante, sugestão da noite, flyback, burn-in ou idioma.

### Corrigido
- **Cheater Fish: clique no console, teclado e controle.** Durante a tela do
  acessório o painel lateral (POWER e OPEN/CLOSE) não respondia a cliques, e o
  teclado não navegava (nos menus só mouse e controle valem). Agora: POWER
  desliga sem começar o jogo, OPEN/CLOSE ejeta e volta à estante; setas/W/S,
  PageUp/PageDown, Home/End, Enter/Espaço/Z (liga/desliga), Backspace/X/Esc
  (começa o jogo) e os botões do controle — d-pad, A e B — fazem tudo.
- **"O jogo não abriu" depois de "Renomear ROMs"**: a estante seguia
  apontando para o nome antigo do arquivo (o catálogo só era relido no
  "Atualizar"). Agora o catálogo é relido sempre que se volta das
  configurações; e, se o arquivo do disco sumiu, o aviso diz isso e manda
  usar "Atualizar" em vez de culpar o núcleo.

### Controle
- **Botões da TV alcançáveis pelo controle na tela inicial** (como no
  SixteeN): o d-pad passeia pelos botões do painel, pelas setas de update e
  pelos botões da TV (VOL-, VOL+, MENU, POWER), com o anel lavanda de foco;
  A ativa e B sai. O anel e a legenda só aparecem para quem usa o controle.

### Código
- Revisão: o início do download do núcleo (copiado em três pontos da tela
  inicial) virou `Download::start_core`; saíram campos mortos
  (`hiss_on_static`, `rng`), o hash de ROM de SNES do RA, o log de debug
  por quadro do upload e o desvio de debug de bissecção da estante;
  comentários que ainda falavam de SNES foram corrigidos.
- `platform/src/cabinet.rs` (7,9 mil linhas) virou o módulo `cabinet/`:
  `mod.rs` (tipos, construtor, testes) mais `setters`, `scenes`,
  `shelf_scenes`, `audio`, `chin`, `chrome`, `geometry`, `text`,
  `panel`, `panel_art` e `pause` — só mover, sem mudar comportamento.
- `app/src/runner.rs` perdeu `paths`, `cards`, `cheats`, `notes` e
  `pace` para `runner/`.
- `cargo clippy --workspace --all-targets -- -D warnings` limpo (inclui os
  exemplos de dev que já falhavam).

### Segurança
- **Núcleo PCSX Rearmed verificado**: o download sai do buildbot (nightly
  sem versão nem checksum) e passa a vir de uma release do próprio
  PolystnX no GitHub, aceito só se o SHA-256 bater com o hash embutido
  (`CORE_ASSETS`). Download com teto, descompactação com teto e instalação
  atômica (sobrescrever o dylib carregado matava o app no macOS).
  `packaging/prepare-core-asset.sh` gera os zips e os hashes.
- **Self-update verificado**: só assets de
  `github.com/ticianocorral/PolystnX/releases`, URL validada contra `..` e
  escapes, SHA-256 publicado pelo GitHub conferido no download, `.part`
  com tamanho e fsync antes do rename, pacote reconferido pelo `.sha256`
  no arranque, `.part` órfão apagado, 3 falhas e o pacote é descartado.
  Desligado no Flatpak; não oferecido no Windows; DMG montado na pasta do
  app (não no `/tmp`); AppImage com fsync.
- **RetroAchievements**: senha e token no corpo do POST (não na URL),
  erros de rede sem a URL no log, nome do badge sanitizado (path
  traversal) e com teto de tamanho/dimensão, JSON da API com teto.
  Desbloqueio gravado como pendente antes do envio e reenviado no login;
  401/429/5xx são falhas transitórias (só um `Success:false` conta como
  recusa) e um pendente recusado 3 vezes sai da fila.
- **Config com 0600** (leva o token do RA), gravado de forma atômica e
  apertado para 0600 já na leitura; erro de TOML não cita o conteúdo.
- **Imagens com teto** de dimensão e alocação (`imgutil`) em capas, artes,
  badges e páginas de manual; PDF com teto de expansão no FlateDecode e
  dimensão validada.
- **Dependências**: zip 8 → 9, lopdf 0.36 → 0.42, rustls 0.23.46,
  rustls-webpki 0.103.16 (as mesmas versões com que o SixteeN zerou o
  `cargo audit`).

### Energia e estabilidade (do SixteeN)
- **Telas paradas a 30 Hz** (10 Hz com a janela escondida) depois de 2 s
  sem input e sem nada animando (`pace_ui`, `Platform::idle_for`,
  `Cabinet::animating`); quadro de UI em 60 Hz exatos (era 16 ms).
- **`pace_frame` girava ~0,9 ms por quadro** (o comentário dizia 200 µs):
  agora dorme até 150 µs (Linux) / 500 µs do prazo.
- **Descanso de tela liberado** fora do jogo (o SDL bloqueava sempre);
  bloqueado só com jogo correndo ou input nos últimos 2 min.
- **Janela minimizada pausa o jogo.** Áudio do jogo pausa o dispositivo
  com o jogo pausado, menu aberto ou console desligado; o stream de foley
  fecha após 3 s ocioso; o chiado enche a fila por nível.
- **Jogo que não abre não derruba o app**: aviso na tela e volta à
  inicial; sem dispositivo de áudio o jogo roda mudo; panic grava
  `crash.log`.
- **Portas de controle estáveis na partida**: um pad que cai e volta (id
  novo) reocupa a sua porta e um 3º pad não rouba a vaga de quem caiu.
- **Select+Start** abre/fecha o livro de pausa pelo controle.
- Falha ao salvar/carregar state aparece no OSD; badge do RA baixado em
  thread; painel da estante só refeito quando muda; variável do núcleo com
  `value` nulo é pulada.
- Buffers de pixel do vídeo reaproveitados (sem alocar ~1,2 MB por
  quadro); `getenv` de debug fora do caminho por quadro.

### Corrigido
- **Card de memória não é mais sobrescrito** quando não lê ou não tem
  128 KB (DexDrive `.gme`, `.mcd` com cabeçalho, erro de I/O): o slot fica
  vazio E desencaixado, e todo flush passa por `memcard::overwrite_check`.
- **Renomear card não apaga outro**: "A" → "B" com um `B.mcr` existente
  agora recusa (antes o `rename` substituía o B em silêncio).
- **Gravação atômica** (tmp + fsync + rename) de cards, savestates,
  notas, tempo de jogo, cheats, biblioteca, catálogo, DAT e caches do RA.
- `polystnx.cfg` quebrado (TOML ou UTF-8 inválido) vai para `.cfg.bad` e
  o app abre com os padrões; uma tecla/botão inválido perde só ela.
- `library.json` corrompido vai para `.json.bad` em vez de ser zerado
  pelo próximo save (favoritos e contagens).
- Nome de pasta por jogo recusa `.`/`..`, corta pontos/espaços finais e
  tem teto de 200 bytes.
- Pasta de BIOS não lê inteiro um `.bin` maior que 1 MB (um disco largado
  lá ia para a RAM); `.m3u` com teto.
- DAT: download com teto e conferência do Content-Length; pacote de
  exemplo do devmenu com teto de download e de expansão.

## [1.3.9-beta] - 2026-10-13

### Corrigido
- **O vidro da tampa voltou a desenhar** — o clip da 1.3.8 usava espaço de
  coordenadas que não é o do desenho: recorte vazio, vidro invisível. O
  deslize agora recorta na FONTE (a parte de baixo do vidro segue
  cobrindo o poço conforme ele sobe por trás da carcaça), com a origem
  proporcional aos pixels da textura — src fora dos limites falhava em
  silêncio (o let _ = engolia).

### Modificado
- **emu-run carrega as artes do console** (leitor de CD, vidro da tampa,
  mesa do spindle) — o dev tool mentia sobre a tela: sem elas o poço fica
  pelado e o vidro da tampa nunca desenha (foi assim que o bug passou
  pela bissecção).
- **POLYSTNX_PERF=1** liga o painel de desempenho sem menu (repro/testes
  headless).

## [1.3.8-beta] - 2026-10-13

### Corrigido
- **A tampa do CD agora tem efeito de tampa** — o vidro de acrílico
  aparecia/desaparecia num cross-fade imperceptível (vidro translúcido
  desvanecendo nele mesmo não se vê). Agora ela DESLIZA: ao fechar, entra
  de cima assentando no poço; ao abrir, sobe e some por trás da carcaça
  (clip na abertura fixa, smoothstep de ~0,45 s), com o disco parando e
  retomando por baixo como já fazia.

## [1.3.7-beta] - 2026-10-13

### Corrigido
- **macOS: o app não faz mais spawn de processo NENHUM** — a leitura de
  RAM via `ps` era o gatilho dos congelamentos: o fps aparecia no painel
  (primeira janela de 1 s fecha) e o jogo congelava no instante seguinte,
  no primeiro spawn. Todos os laudos de zumbi tinham a thread ram-readout
  presa no wait4, e o `ps` engasgava com os zumbis anteriores — o ciclo
  que forçava encerrar a sessão. No macOS o painel de desempenho mostra
  só o FPS (RAM e temperatura ficam de fora até haver leitura sem fork —
  sysctl). No Linux nada muda (RAM lê /proc, sem spawn, e o painel nunca
  travou lá).

## [1.3.6-beta] - 2026-10-13

### Adicionado
- **Bússola do loop no log** — a cada 10 s, uma linha `estado:` diz em qual
  ramo o tick está (jogo/pausa/modal), com in_flight, frames e estado da
  tampa. O congelamento do macOS deixa a main thread VIVA no ritmo de 60
  fps com o core parado no recv — ou seja, o loop saiu do ramo de jogo por
  alguma porta; agora o log nomeia a porta.

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
