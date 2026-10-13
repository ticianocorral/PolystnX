# Arte para SteamGridDB — PolystnX e SixteeN

Arte original da família **Retro Xperience**, pronta para subir no
[SteamGridDB](https://www.steamgriddb.com/) e usar no Steam Deck (biblioteca
não-Steam via Decky/Steam-ROM-Manager). Estilo "manual do proprietário":
papel creme, tinta e a paleta de cada console, desenhada 100% em vetor pelo
gerador — nenhum asset de terceiros.

## Arquivos por app

| Arquivo | Categoria no SteamGridDB | Observações |
|---|---|---|
| `grid_600x900.png` | **Grid** (vertical) | cápsula alta da biblioteca Steam |
| `grid_920x430.png` | **Grid** (horizontal) | cápsula larga ("panel") |
| `hero_3840x1240.png` | **Hero** | banner do topo da página; no máximo aceito pelo SGDB |
| `logo_660x930.png` | **Logo** | PNG transparente, com contorno creme para fundos escuros |
| `icon_512x512.png` | **Icon** | quadrado, 512 px |

Pastas:

- `polystnx/` — disco roxo, tampa, memory cards e controle com △ ○ ✕ □
  (paleta do site: roxo `#6a5fc9`/`#4a3f8f`, vermelho `#b3382e`)
- `sixteen/` — cartucho entrando no slot, chaves Power/Reset roxas e o
  losango de 4 cores do favicon (vermelho/ouro/verde/roxo de fábrica)

## Como regenerar

```bash
python3 scripts/gen_sgdb_art.py                 # gera tudo em branding/steamgriddb/
python3 scripts/gen_sgdb_art.py --out /tmp/arte # ou outro destino
```

O gerador (`scripts/gen_sgdb_art.py`) precisa só de Pillow. Cores, wordmarks
(fonte pixel própria) e composições ficam no topo do arquivo — ajuste e rode
de novo.

## Dicas de upload

- No SGDB, marque o jogo/app correspondente (PolyStnX e SixteeN são apps
  próprios; se não existirem como jogo, suba como arte de atalho não-Steam
  pelo Steam-ROM-Manager ou use o plugin SteamGridDB do Decky Loader no Deck).
- Logo e ícone exigem transparência — não converter para JPEG.
- Heros: o SGDB aceita de 1920×620 até 3840×1240; o arquivo já está no
  tamanho máximo.
