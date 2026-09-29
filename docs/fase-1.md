# Fase 1 — emulador feio que funciona

Portão da Fase 1 do [`plano-psx-xperience.md`](plano-psx-xperience.md): o
`emu-run` de PSX completo — disco, BIOS, memcard, analógico, save states com
carimbo de disco e a medição honesta de desempenho. Sem moldura, sem estante.

## O que ficou pronto

- **Core options v2** (`crates/emulation`): o environment callback agora
  responde `GET_CORE_OPTIONS_VERSION = 2` e parseia `SET_CORE_OPTIONS_V2`
  (e o v1 como cortesia) — structs transcritas do `libretro.h` oficial
  (o `values[128]` inline é o ABI). As definições povoam a tabela de
  variáveis com os defaults do core, e o override por chave
  (`set_variable("swanstation_Renderer", "Software")`) continua valendo —
  é o que mantém o renderer de software no headless.
- **Analógico + L2/R2**: o `Button` fechou as 14 botões digitais do
  DualShock (L2=12, R2=13); o `input_state_cb` responde
  `RETRO_DEVICE_ANALOG` (sticks esquerdo/direito, ±i16). Na plataforma, os
  sticks do gamepad SDL3 são amostrados com deadzone de ~12% e os
  **gatilhos** (axes, não botões, no SDL) viram L2/R2 acima de 25%. No
  teclado: `1` = L2, `2` = R2.
- **Biblioteca de memory cards no osso** (`emu-run --card1 <arquivo.mcr>`):
  o card da biblioteca `memcards/` É o save — o conteúdo entra no
  `SAVE_RAM` ao inserir o disco, o flush periódico (e o de saída) volta
  para o card, e a **primeira inserção materializa o arquivo** (o card
  novo que o core formata, 128 KB com assinatura "MC") — o cartão existe
  antes de o jogo salvar nele. Ciclo provado de ponta a ponta: rodada 1
  cria, rodada 2 recarrega.
- **Carimbo de disco nos save states** (plano §3.1): salvar escreve
  `<slot>.disc` com o nome do disco; carregar um state de outro disco
  avisa no OSD ("STATE DE OUTRO DISCO — salvo no X, drive tem Y"). O aviso
  em UI de verdade (bloco Discos, troca na cena) é Fase 4.
- **Run-ahead 0 por default** (`psx-xperience.cfg`), confirmado por
  medição — abaixo.
- NTSC 59.817 fps / 44.1 kHz e a troca de framebuffer do core
  (`SET_GEOMETRY` → 640×448 com o upscale do SwanStation) atravessam o
  pipeline herdado sem mudanças.

## O portão de desempenho — PASSA

1800 frames (30s de jogo, Tekken 3 bootando, renderer de software,
headless), no macOS arm64 de desenvolvimento:

| Run-ahead | Frames atrasados | CPU | Veredito |
|---|---|---|---|
| 0 | **3 / 1800** | 42% | **60 fps com folga** |
| 1 | 414 / 1800 | 109% | inviável, como previsto |

O default 0 de PSX não é chute: o state grande de MB e o custo do core
dobram o frame com run-ahead 1. Steam Deck/laptop fraco seguem como teste
de campo, mas o portão está atravessado.

## Ruído conhecido (sem ação agora)

- O log de ROM imprime o parse de header SNES sobre o CHD
  (`name=Some("W  0    Ijf     K B") HiRom`) — inofensivo; a identificação
  de discos (serial) chega na Fase 2 e substitui esse caminho.
- Troca de disco por m3u no emu-run fica para a Fase 2 (domínio) — o
  carimbo já cobre o state.
