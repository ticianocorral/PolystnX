# Plano de projeto — PSX Xperience (fork do SNES Xperience)

Projeto pessoal, sem fins comerciais. Fork direto do
[SNES Xperience](https://github.com/ticianocorral/snes-xperience): em vez de
reimplementar a moldura, reaproveita-se o app inteiro e troca-se o console —
o Super Nintendo por um PlayStation, com o cartucho virando disco.

Este plano é um **plano de delta**: parte do princípio de que a base existe,
está madura (v1.1.1) e roda nos três sistemas, e mapeia só o que muda, onde
muda e em que ordem. Referências de código apontam para a árvore do
`SNES Xperience`.

Decisões fechadas de antemão (não reabrir): **SwanStation** como núcleo,
**CHD como único formato de disco**, e **biblioteca de memory cards
compartilhados** — cards de verdade, que existem independentes do jogo, como
no console original. Nunca um memory card por jogo.

**E a mais importante: a interface é a mesma do SNES Xperience.** Os dois
apps são membros do ecossistema **Retro Xperience** — a família de
emuladores-portáteis onde o console inteiro vive na tela. TV de tubo, bezel,
painel lateral, estante, caderno de pausa, modais, sinal off: tudo idêntico,
porque é o mesmo código herdado e **nenhuma interface nova será inventada
para o PSX**. O que muda é só o console desenhado dentro da cena — bandeja
de CD no lugar do slot de cartucho, disco com etiqueta, slots de memory
card — desenhado com a mesma linguagem visual do irmão. Um novo console,
não um novo app.

---

## 0. Resumo executivo — o que herda, o que adapta, o que nasce

| Camada / recurso | Destino | Trabalho |
|---|---|---|
| Workspace de 6 crates, dependências só para baixo | **Herda** | Renomear, só |
| `xperience-emulation` (core libretro dinâmico) | **Adapta** | Já é genérico; faltam analógico, L2/R2 e core options v2 (obrigatório — o SwanStation só fala v2) |
| `xperience-platform` SDL3 (Cabinet, CRT, áudio, gamepad) | **Adapta** | Mesma interface do SNES Xperience (família Retro Xperience); só o console desenhado na cena muda |
| `xperience-domain` (identificação, catálogo, DAT) | **Reescreve o miolo** | Disco = **CHD somente** (+ m3u como lista de discos), serial de fábrica lido de dentro do CHD |
| `xperience-ra` (rcheevos vendorizado) | **Adapta com risco real** | Hash de ROM vira hash de **disco CHD** via rhash — obrigatório, sem plano B de formato |
| **Biblioteca de memory cards** | **Novo** | Cards `.mcr` compartilhados em `memcards/`, dois slots físicos na cena, troca só desligado |
| `xperience-ntsc` (blargg snes_ntsc) | **Adia** | Não há psx_ntsc; começa sem filtro composto |
| Estante, busca, recentes, playtime, notas com 15 capturas, save states em 10 slots, cheats com interruptor, manuais em PDF, sinal off, pausa | **Herda** | Zero a quase zero |
| Download de core do buildbot, renomeação No-Intro, devmode com pacote | **Adapta** | Apontar para o SwanStation e para o DAT "Sony - PlayStation" |
| Packaging (DMG/zip/AppImage com SDL3), CI de 3 runners, site | **Herda** | Renomear tudo |

A leitura rápida: **o app já é 75% do caminho**. O que falta concentra-se em
quatro lugares — disco CHD, hashing de disco para a RA, a biblioteca de
memory cards (o único recurso novo de verdade) e o console desenhado na tela.

---

## 1. Núcleo de emulação

**SwanStation via libretro.**

Motivos da escolha, em ordem:

1. **Herdeiro direto do DuckStation** — o emulador de PSX de referência hoje:
   precisão alta, manutenção ativa, compatibilidade de ponta. O core libretro
   é o mesmo motor.
2. **CHD de primeira.** O SwanStation nasceu com CHD como formato principal —
   e como CHD é o único formato aceito (§2), isso não é detalhe.
3. **Renderizador de software embutido.** O pipeline do app é software: o
   core entrega um framebuffer 2D que o Cabinet escala e deforma no tubo.
   O SwanStation roda com a core option de renderer = Software e encaixa sem
   callback de contexto novo. Renderização de hardware (upscaling interno,
   PGXP) fica para um dia em que o pipeline ganhe contexto de GPU — fora de
   escopo agora.
4. **BIOS real.** O HLE do SwanStation existe mas é parcial; o app **exige**
   BIOS de verdade e valida (§1.1). O momento mais bonito do PSX — o boot
   autêntico no Power — depende disso (§6).
5. **`need_fullpath`.** Cores de disco recusam bytes na RAM — querem o caminho
   do arquivo. O wrapper já implementa esse ramo
   (`crates/emulation/src/core.rs`, `CoreError::NeedsFullPath`); pago.
6. **Cheats.** Códigos GameShark/Action Replay aplicam pelo `retro_cheat_set`
   padrão — a mesma válvula do snes9x.

### 1.1 BIOS

- Nunca distribuída com o app, nunca baixada por ele — a pasta `bios/` (nova,
  ao lado de `core/`) é povoada pelo usuário, e as Configurações ganham uma
  seção "BIOS" que valida o arquivo por tamanho/MD5 contra os modelos SCPH
  conhecidos e diz, em português claro, qual falta ou qual está errado.
- O `system_directory` que o environment callback devolve passa a apontar para
  `bios/` (hoje aponta para a raiz — ver `Core::set_directories`).
- Sem BIOS, o console **liga e mostra "sem disco no drive"** na TV, igual ao
  hardware sem mídia: erro em cena, não diálogo de sistema.

Duas ressalvas de licença, ambas leves: o SwanStation (DuckStation) é GPL —
confirme a variante exata antes de publicar binário, o fluxo do
`THIRD-PARTY-NOTICES.md` já existe; e o app continua nunca tocando em disco
nem BIOS.

**Plano B de core (só se o portão de desempenho da Fase 1 falhar):**
PCSX-ReARMed, o core leve por excelência. A troca é barata pelo wrapper — mas
é decisão de Fase 1, com medição na mão, não antecipada.

---

## 2. Arquitetura — mapa do delta por crate

As quatro camadas e a regra de dependências ficam de pé como estão. O detalhe:

### `xperience-emulation` — pequeno, mas cirúrgico

- **Core options v2 é obrigatório, não opcional.** O callback hoje devolve
  `GET_CORE_OPTIONS_VERSION = 0` e só sabe o protocolo legacy `SET_VARIABLES`.
  O SwanStation expõe opções só pelo protocolo novo
  (`SET_CORE_OPTIONS_V2`) — sem estender o `environment_cb`, não há como
  configurar renderer, memcard nem controle. É a primeira tarefa de código.
- **Botões**: o `enum Button` tem os 12 do pad de SNES. O pad de PSX tem 14
  digitais (acrescem **L2/R2**) e analógico. `MAX_PORTS = 2` fica.
- **Analog sticks**: o `input_state_cb` só responde `RETRO_DEVICE_JOYPAD`.
  Precisa responder `RETRO_DEVICE_ANALOG` (eixos esquerdo/direito) e a camada
  de plataforma mapear os sticks/triggers do gamepad SDL3. O controle vira
  DualShock por core option.
- **Defaults**: `base 256×224`, `sample_rate 32040` são de SNES; viram
  `320×240` / `44100` (e a taxa real sempre vem do core via `AV_INFO` — a
  estrutura já é dinâmica, os defaults é que são SNES).
- **Multi-resolução**: o PSX troca de resolução em voo (256/320/368/512 de
  largura, 480i interlaced em alguns). O caminho
  `SET_SYSTEM_AV_INFO | SET_GEOMETRY → av_info_dirty → refresh` **já existe**;
  falta conferir que o tubo (mesh warp) aceita texturas de largura/altura
  variáveis sem reagarrar as UVs.

### `xperience-domain` — é aqui que o disco mora

O cartucho de SNES é o melhor caso possível para um identificador: um arquivo,
hash estável, header fixo. O disco de PSX é o oposto — e a decisão de aceitar
**só CHD** simplifica o problema de volta para um arquivo só:

- **Formatos aceitos**: `.chd`. Ponto. Nada de cue/bin, iso, pbp — e `.zip`
  sai por completo.
- **`.m3u` fica, mas só como lista de discos**: um jogo multi-disco é um
  `.m3u` cujas linhas apontam para os `.chd` dele. Não é "formato de disco" —
  é o mesmo papel do cartucho com vários volumes. A estante indexa o m3u como
  **um** jogo com N discos; a troca acontece no app (§6).
- **Identificação**: o PSX tem uma dádiva que o SNES não tinha — o **serial de
  fábrica** (`SCUS-94163`, `SCES-02105`…), gravado no volume do disco. Com
  CHD, ele se lê de dentro do arquivo (SYSTEM.CNF / descritor de volume,
  pelo mesmo leitor ISO9660+CHD que o rhash já traz — §4). O serial vira a
  chave primária, com hash de disco como verificação. Fallback por nome de
  arquivo, como hoje.
- **Tabela embutida**: o `gen_tosec_data.py` ganha um irmão que gera a tabela
  a partir de um DAT Redump/No-Intro de "Sony - PlayStation" — serial → nome
  canônico, ano, região. O DAT local opcional (`config/nointro.dat`) mantém
  prioridade, mesmo fluxo de hoje.
- **Coleções em cue/bin**: a varredura encontra `.cue` solto em `roms/` e
  avisa na estante — "converta para CHD" — apontando o `chdman` no README.
  A conversão em si fica fora do app (§11).

### `xperience-platform` — o console muda de casco

O Cabinet desenha o SNES proceduralmente (meshes, rocker switches, slot de
cartucho). O tubo CRT, bezel, sinal off, OSD de conquistas, painel, modais,
caderno de pausa e estante **herdam inteiros** — são independentes de console.
O que se redesenha: o gabinete (§6) — agora com dois slots de memory card
funcionais.

### `xperience-ra` — o risco técnico nº 1

Hoje o rcheevos vendorizado compila com `RC_HASH_NO_DISC` e o app porta na mão
o `rc_hash_snes` (MD5 do arquivo sem header). Para PSX o hash da RA **é um
hash de disco**: o `rhash` do rcheevos parseia o ISO9660 dentro do CHD e
devolve a string que o servidor espera.

Com CHD como único formato, **não existe plano B de formato** — o caminho é
um só: compilar o rhash vendorizado com hashing de disco (remover
`RC_HASH_NO_DISC`) e registrar um **cdreader customizado** para o CHD, o
mesmo hook que o RetroArch usa para plugá-lo. É o mesmo C que o RetroArch
usa, então o hash certo de um lado é o hash certo do outro.

*Revisão (Fase 0): este parágrafo previa um `chd.c` no rcheevos que precisava
de zlib — o rcheevos atual não o tem mais. O que se implementou foi melhor:
o `rhash` C roda o algoritmo e um cdreader em Rust (`crates/ra/src/hash.rs`,
crate puro-Rust `chd`) lê o CHD — zero zlib, zero libchdr. Provado contra um
disco sintético e contra um CHD real (Tekken 3 USA), cujo hash
`5b0009044c8d7724518ff57e35c61af6` é o checksum registrado no site da RA.
Ver `docs/fase-0.md`.*

A avaliação de conquistas em si **herda sem mudanças**: `tick` já lê uma slice
de memória que o caller entrega, e o runner já alimenta
`with_memory(MEMORY_SYSTEM_RAM)` — para PSX isso é a RAM de 2 MB, o que os
`MemAddr` dos sets de PSX usam. O console id para a API é o
`RC_CONSOLE_PLAYSTATION = 12` (confirmado no `rc_consoles.h` vendorizado); os
endpoints usados (`achievementsets`, `awardachievement`, `login2`,
`API_GetGameExtended`…) não mudam.

### `xperience-ntsc` — adia, não mata

O `snes_ntsc` do blargg é um codificador composto NTSC, mas sua entrada assume
o formato de cor 15-bit do SNES. PSX é 15-bit também, com ordem de canais
diferente — **pode** funcionar com uma tabela de conversão, mas é hipótese,
não dado. Decisão: a Fase 1 sai com **Pixel perfect / Sharp bilinear + o
mesmo warp de tubo** (que herda), e o filtro composto vira experimento isolado
depois, com desligue rápido se o resultado for feio. Nada do app depende dele.

---

## 3. Biblioteca de memory cards — o recurso novo

O PSX original não tinha save por jogo: tinha **cartões físicos** encaixados
nos slots, e cada jogo gravava no que estivesse lá. É exatamente isso que o
app modela — e é a decisão que muda o "saves/<título>/" de lugar:

- **Formato**: arquivos `.mcr` de 128 KB (formato VMC/MCR padrão) na pasta
  `memcards/` da raiz do app. Formato universal: o mesmo arquivo abre no
  DuckStation, no RetroArch, no PCSX — o card do usuário é dele, não do app.
- **Compartilhados por natureza**: um card não pertence a jogo nenhum. O jogo
  A grava o save dele; o jogo B grava o dele **no mesmo card**, lado a lado,
  como os 15 blocos do cartão físico sempre permitiram. O app **nunca** cria
  card por jogo, nunca sugere card por jogo, nunca esconde um card porque
  "não é deste jogo".
- **Dois slots, como o console**: o slot 1 e o slot 2 da cena seguram um card
  cada. Trocar card é gesto físico: abre a **biblioteca** (a "estante de
  cards"), escolhe um card existente ou cria um novo com nome, e o card
  encaixa no slot com animação — a etiqueta do cartão aparece na cena.
- **Regra da trava**: trocar card só com o console **desligado**, igual à
  trava de ejetar o disco. Um card, uma etiqueta, um lugar.
- **Primeira execução**: o app cria um "Cartão 1" e o deixa encaixado no
  slot 1; slot 2 vazio — como quem comprou o console com um cartão na caixa.
  Quem quiser mais cards cria os seus.
- **Plumbing (Fase 1)**: o caminho principal é o `RETRO_MEMORY_SAVE_RAM` —
  escrever o conteúdo do card encaixado antes do `load_game` e fazer o flush
  periódico para o arquivo (a tubulação do `flush_sram` **já existe** e faz
  exatamente isso). A verificar na Fase 0/1: se o SwanStation expõe o memcard
  por `SAVE_RAM` no libretro. Se não expuser, o plano B é o modo de memcard
  compartilhado das core options, apontando os dois slots para os arquivos da
  biblioteca — por isso o `SET_CORE_OPTIONS_V2` (§2) vem antes.
- **Save state não carimba card**: o card é físico e independente do estado —
  restaurar um state não devolve o conteúdo do card de então, igual ao
  console real. O state carimba só o disco (§3.1).
- **Fora da biblioteca, de propósito**: apagar save individual dentro do card
  (o **jogo** faz isso, no gerenciador dele) e formatar card (idem). A
  biblioteca cuida de arquivos e slots, não de blocos.

### 3.1 Save states

`retro_serialize` nos 10 slots, igualzinho ao SNES. Duas pegadinhas de PSX:

- O estado é **grande** (MB, não KB) e amarra o disco carregado — um state do
  disco 2 restaurado com o disco 1 no drive é lixo. Regra: o slot carimba o
  índice do disco e o app avisa ("este state é do disco 2 — troque antes de
  carregar") em vez de restaurar às cegas.
- **Run-ahead**: com estados desse tamanho e emulação mais pesada, o default
  do `xperience.cfg` vira `runahead = 0` (o SNES herdava 1). Teste honesto na
  Fase 1 decide se 1 volta.

---

## 4. Entrada: gamepad analógico

O PSX é o primeiro console do projeto com sticks — e muitos jogos **exigem**
analog (e alguns exigem DualShock para vibrar; vibração fica fora de escopo).

- O `PAD` map do runner e o `PadButton` da plataforma ganham L2/R2 como
  botões; os sticks do gamepad SDL3 mapeiam para eixos, e L1/R1 de trigger
  continuam botões.
- Tela de configurações: um teste de sticks ao vivo (cruz que se move) para o
  usuário conferir deadzone — herda o espírito das telas utilitárias que já
  existem.
- No console desenhado, os pads na cena continuam decorativos; o DualShock na
  moldura é decisão de arte da Fase 3, não bloqueia nada.

---

## 5. Cheats

A tubulação herda: `libretro-database` tem pasta de cheats de PSX em formato
GameShark/Action Replay, o `gen_cheats_data.py` aponta para ela, e o
`retro_cheat_set` aplica sem conversão — o SwanStation aceita os códigos
crus. O interruptor por cheat, a UI de checklist e o `cheats.txt` por jogo
são exatamente os de hoje. O único ajuste real é o gerador filtrar PSX e as
descrições serem reescritas com o mesmo critério de antes (a lista de
endereços é fato bruto; o texto tem autor).

---

## 6. A moldura: de cartucho a disco (e o card no slot)

**A interface é a do SNES Xperience — a mesma TV, o mesmo painel, a mesma
estante, os mesmos rituais — porque é o mesmo código.** A Fase 3 não desenha
uma interface nova: desenha o gabinete do segundo console da família Retro
Xperience, com a mesma linguagem visual (mesmas proporções de cena, mesma
tipografia, o bezel mais escuro que a tela, os comandos nos mesmos lugares).
O que muda na cena é o aparelho:

O ritual é a alma do app, e o PSX tem um ritual próprio — melhor que o de
cartucho em um ponto: **o boot de verdade**.

- **Tela inicial**: o console cinza com a **tampa fechada**, os **dois slots
  de memory card com os cards encaixados** (etiqueta visível), botão "Inserir
  disco" abrindo a estante.
- **Inserir disco**: a estante abre, o disco com a **arte impressa do jogo**
  (`assets/disc/<rom>.png` — substitui o `assets/cartridge/`) desce para a
  bandeja, a tampa fecha. O disco tem etiqueta de verdade: sem arte local, um
  template neutro com o nome — nunca disco cinza liso.
- **Power**: liga, e aí acontece o que nenhum cartucho podia dar — **o boot
  real da BIOS** na TV, com o som original, porque é o core rodando o BIOS de
  verdade. Zero trabalho de app, efeito máximo. O "CH 3" e o sinal off herdam.
- **Reset**: momentâneo como hoje (reinicia pela BIOS — e olha que bonito,
  o boot de novo).
- **Ejetar**: só desligado, mesma trava do SNES (regra do app; o hardware real
  permitia abrir rodando, mas a trava única ensina a sequência e evita estados
  estranhos). Abrir a tampa com disco dentro ejetando o disco para cima é o
  gesto físico do PSX — é esse que a animação copia.
- **Trocar disco (m3u)**: no caderno de pausa ganha um bloco "Discos" com os
  discos do m3u; escolher outro executa a troca na cena (tampa abre, disco
  sai, disco entra) e recarrega no core. Save states carimbam o disco
  (§3.1).
- **Slots de memory card funcionais**: clicar num slot da cena abre a
  biblioteca de cards (§3) — lista de cards, "Cartão novo", e o card
  encaixando no slot com animação quando escolhido. É a única peça da moldura
  que não existia no SNES Xperience em forma nenhuma.
- **Marca**: "PSX Xperience", wordmark novo (`console-tag.png`), ícones e site
  novos. O console desenhado é **inspirado no** PlayStation, não uma cópia de
  trade dress da Sony — mesma regra do plano original com a Nintendo. A raiz
  de dados vira `~/Documents/PSX Xperience` (macOS), o XDG e o
  ao-lado-do-executável seguem o mesmo padrão (`dirs.rs` tem três constantes
  de nome para trocar).

Os três cuidados obrigatórios do plano original continuam valendo sem
modificação: sem flash de tela cheia, chuvisco com corte, pular no primeiro
botão.

---

## 7. Cena, escala e filtros

- **Proporção**: 320×240 em 4:3, pixel não quadrado — o mesmo caso do SNES
  (§4.7 do plano original) com outros números. O costume continua: escala
  inteira na vertical, fração aceita na horizontal. Sharp bilinear é o padrão;
  Pixel perfect e o modo CRT herdam.
- **480i**: jogos entrelaçados (alguns 3D tardios) tremem em warp de tubo por
  natureza. Botão de escapa: opção "sem warp em interlaced" ligada por
  detecção (altura dobrada) — decide-se com jogo real na mão, Fase 3.
- **PAL**: 50 Hz vem do core e o pacing segue (`pace_frame` já usa o fps do
  `AV_INFO`). A ficção de TV (RF, CH 3) é NTSC-branded e fica assim — decisor
  de PAL provavelmente tem TV capaz, e consertar a ficção para 50 Hz é
  cosmética de último dia.

---

## 8. Renomeação, empacotamento e CI

Trabalho mecânico, mas extenso — listar para não subestimar:

- Workspace `psx-xperience`; binário principal `psx-xperience` (os binários
  `emu-run` e `selector` continuam como ferramentas de dev).
- `BRAND` da plataforma, nameplate ("PSX Xperience v1.0 / SwanStation x.y.z"),
  ícones (`packaging/{macos,windows}`), `Info.plist`, `.desktop`, site
  (`site/`), README, CHANGELOG zerado com entrada 0.1.0.
- Pastas da raiz de dados: `roms/`, `core/`, **`bios/` (novo)**,
  **`memcards/` (novo)**, `assets/`, `saves/`, `notes/`, `retroachievements/`,
  `config/`. O `saves/<título>/` fica só com states, cheats, playtime e notas
  — save de jogo mora no card (§3).
- CI de 3 runners herda; release publica DMG/zip/AppImage com SDL3 embutido —
  os scripts de packaging não mudam em nada além de nomes.
- **A família no site**: o `site/` é a porta do ecossistema Retro
  Xperience — um card por console (SNES lançado, PSX em desenvolvimento,
  N64 no papel), cada um apontando para o seu repo. O nome próprio de cada
  app não muda; Retro Xperience é o guarda-chuva.
- **Crates gêmeos, sincronização na mão**: enquanto forem dois, os crates
  compartilhados de fato (`platform`, `domain`, `ra`) permanecem duplicados
  entre os dois repos — unificá-los em crates de família é decisão para
  quando o terceiro membro (N64?) nascer, não agora. O compromisso é outro:
  mudança de interface no irmão entra aqui também, para os dois nunca
  divergirem no visual.
- Repo novo (`psx-xperience`) com remote próprio; o histórico do fork fica
  citado no README, não importado.
- `THIRD-PARTY-NOTICES.md` ganha: licença GPL do SwanStation, zlib (linkada
  pelo hashing de CHD), e a nota de que BIOS e disco nunca acompanham o app.

---

*Revisão (release 0.1.0): "PR, merge" não se aplica ao primeiro commit de
um repositório novo — não existe base para um diff. O ciclo de release
nasce aqui no formato canônico: commit direto na `main`, tag `v0.1.0`, e a
esteira `release.yml` empacota os três sistemas. Dos próximos lançamentos
em diante, vale o fluxo branch → PR → merge → tag.*

## 9. Fases

### Fase 0 — Provar as premissas (1 semana)

1. O SwanStation baixa do buildbot e roda pela API libretro atual, sem tocar
   no wrapper (prova de que `need_fullpath`, XRGB8888 e SET_GEOMETRY já
   funcionam) — CHD e m3u de CHDs, nos três sistemas.
2. BIOS em `bios/` valida e boota até a tela de logo.
3. O rhash de disco bate com o que o site da RA espera, para cinco jogos de
   teste — **todos em CHD**.
4. O memcard aparece por `RETRO_MEMORY_SAVE_RAM` (ou o plano B de core option
   fica decidido aqui).

*Revisão (Fase 0): os quatro itens estão provados no macOS — o portão mais
arriscado (o hash de disco da RA) fechou com um CHD real cujo checksum bate
com o registrado no site da RA; a BIOS real boota até o logo do PS; e o
memory card aparece por `RETRO_MEMORY_SAVE_RAM` (128 KB, assinatura "MC"),
que confirma o desenho da biblioteca compartilhada sem plano B. Ver
`docs/fase-0.md`, com a imagem do boot. Falta só a prova nos outros dois
sistemas — trabalho do CI herdado.*

**Sem plano B de formato: se o item 3 falhasse, a RA ficaria bloqueada até
consertar — não se inventa hash próprio.**

### Fase 1 — Emulador feio que funciona (3 a 4 semanas)

`emu-run` de PSX: CHD, BIOS, core options v2, biblioteca de cards no osso
(arquivo `.mcr` encaixa via SAVE_RAM, flush periódico), save states grandes
com aviso de disco, analógico (sticks + L2/R2), multi-resolução, PAL, 60 fps
no pacing, run-ahead medido (provavelmente 0). Sem moldura, sem estante.

**Portão de desempenho**: se o renderer de software não sustenta o fps no
hardware-alvo (Steam Deck / laptop), é aqui que se decide — contexto de GPU
para o renderer de hardware do core, ou plano B de core — antes de investir
na cena.

### Fase 2 — Domínio de discos + estante (2 a 3 semanas)

Varredura de `roms/` aceitando só `.chd` (+ m3u), serial lido de dentro do
CHD como chave, tabela Redump embutida, DAT opcional, aviso amigável para
cue/bin com ponte para o `chdman`, renomeação No-Intro, estante completa com
capas e m3u como um jogo com N discos.

### Fase 3 — A moldura PSX (4 semanas)

Console novo com tampa e slots de card, disco com etiqueta, animação de
inserir/ejetar, boot real da BIOS no Power, sinal off, trava de ejeção, troca
de disco na cena, **biblioteca de cards com UI completa** (picker, criação com
nome, animação de encaixe), 480i sem warp se necessário, wordmark e ícones.

### Fase 4 — Painel, cheats e RA (3 a 4 semanas)

Cheats PSX com interruptor, RA de ponta a ponta (identificação por hash de
disco, cache, OSD, badges, progresso), bloco de Discos no caderno de pausa,
manuais em PDF herdam.

### Fase 5 — Empacotar (1 semana)

DMG/zip/AppImage, CI verde nos três runners, site novo, release 0.1.0.

**Total realista para uma pessoa: três a quatro meses até a Fase 5** — menos
que os quatro a cinco do projeto original, porque estante, painel, pausa,
notas, sinal off, packaging e CI já existem e o fork herda todos. A
biblioteca de cards, o único recurso novo, cabe dentro desse bônus.

---

## 10. Riscos

| Risco | Impacto | Mitigação |
|---|---|---|
| Hash de disco CHD da RA divergir | ~~Alto~~ **Resolvido na Fase 0** | Provado com CHD real: o hash do Tekken 3 USA bate com o checksum registrado no site da RA (`docs/fase-0.md`) |
| SAVE_RAM não expor o memcard no SwanStation | Médio | Verificado na Fase 0/1; plano B: core options de memcard compartilhado apontando para os arquivos da biblioteca |
| Desempenho do renderer de software | Alto | Portão na Fase 1, antes da moldura; contexto de GPU ou core leve são as saídas — decididas com medição |
| Coleção do usuário em cue/bin | Médio | Aviso na estante + `chdman` documentado no README; conversão dentro do app é fora de escopo |
| BIOS ausente/errada na primeira execução | Alto — o console não liga | Seção BIOS nas Configurações com validação por MD5 e instrução clara; erro em cena, não diálogo |
| Usuário troca card com console ligado | Baixo | Mesma trava do disco — o gesto resiste com um clunk, igual ao hardware |
| blargg NTSC não servir para PSX | Baixo — já é o plano | Começa sem filtro composto; warp herda |
| Save state restaurado com disco errado | Médio | Carimbo de índice de disco no slot + aviso no carregar |
| 480i tremendo no warp de tubo | Baixo | Opção de desligar warp em interlaced |
| Marca Sony / trade dress | Médio | Console "inspirado em", nunca idêntico; sem logo PlayStation; vale em projeto pessoal publicado |

---

## 11. Fora de escopo

- PlayStation 2 e qualquer console além do PSX.
- Netplay, rede, qualquer coisa online além da RA que já existe.
- Multitap (mais de 2 controles), light gun, PocketStation, link cable.
- Vibração do DualShock.
- Renderização de hardware (upscaling interno, PGXP) — pede contexto de GPU
  no pipeline; o software do SwanStation cobre o projeto.
- Conversor cue/bin → CHD dentro do app (documenta-se o `chdman`; o app só
  aponta).
- Gerenciar blocos de save dentro do card e formatar card — trabalho do jogo,
  no gerenciador dele.
- Baixar BIOS ou disco por dentro do app — nunca.
- Disco trocado no meio do jogo **por indicação do próprio jogo** (a troca por
  m3u na pausa cobre o caso real; o fluxo "o jogo pede o disco 2 e o app
  detecta" é refinamento futuro).

Cada um é um projeto. Nenhum é necessário para descobrir se o fork se sustenta.

---

## 12. O teste que decide tudo

Duas horas de um RPG longo do **disco 1** (o clássico de FF VII serve), com a
moldura na tela — incluindo pelo menos um boot pela BIOS, um save gravado no
"Cartão 1" pelo jogo, a criação de um segundo card na biblioteca e o mesmo
jogo rodando nele depois de uma troca, um save state com aviso de disco e uma
troca de disco na pausa.

Se você parar de enxergar a moldura, a cerimônia de disco não cansar na
terceira troca de jogo e o card da biblioteca parecer um cartão de verdade na
prateleira, o fork está certo — e o boot da BIOS no Power, sozinho, paga o
ingresso. Se incomodar, é contraste ou saturação competindo com o jogo, ou
cerimônia demais para o tamanho da biblioteca; o conserto é o mesmo do
projeto original, e a Fase 3 é o lugar de medir antes de investir no console
definitivo.
