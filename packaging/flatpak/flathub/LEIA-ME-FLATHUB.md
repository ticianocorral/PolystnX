# Flathub — kit de submissão do PolystnX

**Nada foi submetido.** Este diretório organiza tudo o que o Flathub exige
para que você revise e dê o ok. Referência:
https://docs.flathub.org/docs/for-app-authors/submission

## ⚠️ Bloqueio atual: versão beta

O Flathub só aceita **release estável** no repositório stable, e o PolystnX
está em `1.2.2-beta`. O kit está pronto e pinado na beta **apenas para
validar o build localmente**. Antes de abrir o PR:

1. Corte a primeira tag estável (ex.: `v1.3.0`).
2. Atualize `tag` + `commit` no `io.github.ticianocorral.PolystnX.yml`.
3. Troque as URLs das screenshots para a tag nova.
4. Rode `gerar-cargo-sources.py` de novo (se o `Cargo.lock` mudou) e
   acrescente a release estável no metainfo.

## Arquivos preparados

| Arquivo | Papel |
| --- | --- |
| `io.github.ticianocorral.PolystnX.yml` (aqui) | Manifest do PR — vai para a raiz do fork do `flathub/flathub` |
| `cargo-sources.json` (aqui) | 238 crates do crates.io com sha256 (gerado do `Cargo.lock`) — vai junto no PR |
| `gerar-cargo-sources.py` (aqui) | Regenera o JSON acima sempre que o `Cargo.lock` mudar |
| `../io.github.ticianocorral.PolystnX.metainfo.xml` | Metainfo **upstream** (instalado pelo build) |
| `../io.github.ticianocorral.PolystnX.desktop` | Desktop file upstream, inglês + pt-BR |
| `../polystnx-256.png` | Ícone 256px novo (redimensionado do de 1024) instalado pelo build |
| `DESCRICAO-PR.md` (aqui) | Rascunho em inglês do corpo do PR |

O flatpak antigo (`dev.ticianocorral.PolystnX.yml`) ficou intocado — mas
note que ele empacota **binário pré-compilado**, o que o Flathub proíbe;
para o Flathub o app compila do fonte.

## Decisões tomadas (confirme)

1. **App ID `io.github.ticianocorral.PolystnX`** — o ID atual
   (`dev.ticianocorral.PolystnX`) pressupõe o domínio `ticianocorral.dev`,
   que **não existe** (verificado: DNS não resolve). O Flathub exige domínio
   próprio em HTTPS; `io.github.` casa com o hosting real
   (`github.com/ticianocorral/PolystnX`) e cumpre o mínimo de 4
   componentes.
2. **Runtime `org.freedesktop.Platform 25.08`** — troquei o `org.kde.Sdk
   6.8` do flatpak antigo por freedesktop: o app é SDL puro, não usa Qt, e
   o runtime freedesktop é o mais leve/convencional. O Flathub exige a
   série mais recente; confirme com `flatpak remote-info flathub
   org.freedesktop.Platform`.
3. **Build 100% offline** — as dependências vêm do `cargo-sources.json`
   (incluindo o SDL embutido nos crates `sdl3-src`/`sdl3-image-src`/
   `sdl3-mixer-src` que o `vendored-sdl` compila; o `Cargo.lock` não tem
   nenhuma dependência de git) e o rustc/cargo da extensão `rust-stable`.
4. **Screenshots fixadas na tag** (regra do Flathub: nunca branch): 2 do
   site + a tela de setup. Revisar se gostou da escolha.
5. **Metainfo bilíngue** (inglês base + pt-BR), com nota de UI apenas em
   português e da necessidade de BIOS e jogos por conta de quem roda.
6. **BIOS e cores nunca distribuídos** — BIOS por MD5 do usuário, core
   SwanStation baixado do buildbot do libretro no primeiro uso: bom ponto
   legal para a review, já documentado na descrição do PR.

## Verificar ANTES de abrir o PR (em uma máquina Linux)

1. **Build local de verdade**:
   ```bash
   flatpak install -y flathub org.flatpak.Builder
   # dentro de uma pasta só com o yml e o cargo-sources.json:
   flatpak run --command=flatpak-builder org.flatpak.Builder \
     --user --install --force-clean builddir io.github.ticianocorral.PolystnX.yml
   flatpak run io.github.ticianocorral.PolystnX
   ```
2. **⚠️ O maior risco técnico: versão do Rust.** O workspace exige
   `rust-version = 1.98`; a extensão `rust-stable` do runtime pode trazer
   um rustc mais velho. Cheque:
   ```bash
   flatpak run --command=bash org.freedesktop.Sdk//25.08 \
     -c '/usr/lib/sdk/rust-stable/bin/rustc --version'
   ```
   Se for mais velho que o código exige: abaixe o `rust-version` (se o
   código permitir) ou suba a série do runtime.
3. **Lint** (erros E warnings são fatais na review):
   ```bash
   flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
     appstream ../io.github.ticianocorral.PolystnX.metainfo.xml
   flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
     manifest io.github.ticianocorral.PolystnX.yml
   ```
4. **Desktop file**: `desktop-file-validate ../io.github.ticianocorral.PolystnX.desktop`.
5. **UI só em pt-BR** — o Flathub pede localização completa em inglês
   (UI incluída); a nota na descrição do metainfo é o caminho previsto,
   mas a review pode pedir tradução. Decida antes de submeter.
6. **`flathub.json`**: não necessário (x86_64 + aarch64 OK).
7. **GitHub com 2FA ativo** — obrigatório; convite de maintainer expira em
   1 semana.

## Passo a passo da submissão (depois do seu ok e do release estável)

1. Fork de `flathub/flathub` **com "Copy the master branch only"
   DESMARCADO** (ou `gh repo fork --clone flathub/flathub`).
2. Na fork, crie uma branch a partir de `new-pr`.
3. Copie para a raiz **apenas** o `io.github.ticianocorral.PolystnX.yml` e
   o `cargo-sources.json`. Commit e push.
4. PR com **base em `new-pr`** (não em `master`!) e título
   `Add io.github.ticianocorral.PolystnX` — corpo em `DESCRICAO-PR.md` +
   template do Flathub.
5. Comente `bot, build` para o build de teste.
6. Responda todos os comentários; nunca feche o PR para refazer; não dê
   merge de `master` na branch.
7. Aprovado → merge num repo novo da org Flathub → convite de maintainer
   (aceitar em 1 semana). Publicação ~1–2h depois.

## Manutenção pós-publicação

- Release nova: atualize `tag` + `commit` no manifest (o `x-checker-data`
  faz o bot propor o PR sozinho), rode `gerar-cargo-sources.py` se o
  `Cargo.lock` mudou e adicione a release no metainfo.
- Se as screenshots mudarem, troque a tag nas URLs das imagens.
