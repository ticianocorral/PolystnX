# Fase 5 — empacotar

Portão da Fase 5 do [`plano-psx-xperience.md`](plano-psx-xperience.md): o
fork empacota nos três sistemas com a mesma esteira herdada.

## O que ficou pronto

- **DMG de macOS construído e provado** (`dist/PSX-Xperience-0.1.0-macos.dmg`,
  6.6 MB): `PSX Xperience.app` assinado ad-hoc (identifier
  `dev.ticianocorral.psx-xperience`), montado, executado (`--help` corre)
  e desmontado. Drag-to-install com o atalho de /Applications, como no
  irmão.
- **AppImage**: estrutura do AppDir validada localmente (AppRun, desktop,
  ícone, binário); o empacotamento binário em si é do **runner Linux no
  CI** — o `appimagetool` é binário x86_64-Linux e não roda no macOS (o
  script está certo, o ambiente é que não).
- **release.yml conferido ponta a ponta**, com dois consertos: o nome
  dobrado do .exe no zip de Windows (`psx-psx-xperience.exe` →
  `psx-xperience.exe`) e o AppDir renomeado (`PSX_Xperience`). Publica os
  três artefatos na release da tag.
- **CI (`ci.yml`) vai passar de primeira**: `cargo fmt` aplicado ao
  workspace inteiro e **clippy zerado** (incluindo os herdados:
  `never_loop` no fallback de default das core options, `while let` do
  lending iterator com `allow` justificado, variante grande do `Handle`
  encaixotada, linha morta do shelf removida).
- **THIRD-PARTY-NOTICES.md** reescrito para o fork: SwanStation (GPL —
  nunca distribuído, baixado do buildbot; BIOS nunca distribuída nem
  baixada), rcheevos MIT com o `rhash` de disco, crate `chd` + `flate2`
  para o parsing de CHD, SDL3 zlib, e a nota do `snes_ntsc` (herdado, LGPL,
  presente sem consumidor — limpeza futura).
- **Ícones placeholder** (`packaging/psx-xperience.png`,
  `packaging/windows/AppIcon.png` gerados; `AppIcon.ico` herado segue
  válido; `AppIcon.icns` do macOS idem) — troque os arquivos, os scripts
  não mudam.
- **Desktop/AppImage/DMG renomeados**: `psx-xperience.desktop` (Name, Exec,
  Icon, Comment de PSX), `build-dmg.sh` produz `PSX Xperience.app` com o
  binário `psx-xperience`, `build-appimage.sh` idem, release.yml publica
  `PSX-Xperience-*.{dmg,zip,AppImage}`.

## O teste final (você, na janela)

```bash
cargo run --bin psx-xperience     # ou abra o DMG de dist/
```

1. Tela inicial: o PSX cinza com a tampa fechada e o botão "Estante de
   games"; slots de memcard no corpo.
2. Estante: o Tekken 3 na lista (serial como título até popular a tabela).
3. Escolher: o disco desce e some na fenda da tampa (com clique de
   encaixe), console desligado à espera do Power.
4. Power: **o boot da Sony na TV**, com som. E o jogo.
5. Painel: LED verde, "Cheats" presente; Salvar/Carregar com carimbo de
   disco; clicar no slot de memcard desligado abre a biblioteca.
6. Sinal off no desligar, ejetar destrava, estante volta.

## Pendências honestas (para os seus ajustes)

- **Ícones e wordmark de arte final** — os atuais são placeholders
  funcionais.
- **Tabela de títulos** (`psx_data.txt`) vazia até o gerador rodar —
  comando em `docs/fase-2.md`.
- **Windows/Linux** só no CI (o fork nasceu no macOS; o AppImage espera o
  runner Linux, ver acima).
- **Devmode/devmenu** aponta o pacote de exemplo para URL vazia — é
  recurso de dev do irmão de SNES; inofensivo (o botão some sem devmode).
