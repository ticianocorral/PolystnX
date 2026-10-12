# Dock "cartucho" para leitor NFC

Dock de mesa no estilo console: o leitor fica **em pé na vertical** e o cartão entra **por cima,
de pé (retrato)**, como um cartucho — desliza até o piso e para cobrindo toda a face do leitor,
4 mm à frente dela.

## Arquivos

| Arquivo | O que é |
|---|---|
| `dock_leitor_nfc.stl` | A peça para fatiar (unidades em mm) |
| `gerar_dock.py` | Gerador paramétrico — todos os números estão no topo do arquivo |
| `preview_com_cartao.png` | Renders com leitor e cartão na posição de uso (fantasmas azul/escuro não fazem parte da peça) |
| `preview_vazio.png` | Renders da peça impressa |

## Como funciona

- **Cartão de pé (retrato)**: 54 de largura × 85,6 de altura. A fenda tem 1,4 mm de espessura ×
  55 mm de largura, com funil de entrada de 4 mm no topo das guias. O cartão desliza até o piso e
  cobre a face inteira do leitor. A parede frontal é **baixa** (30 mm) — acima dela a face do
  cartão fica exposta, com o cartão segurado pelas bordas por guias laterais e trilhos finos que
  sobem até 65 mm. ~2/3 do cartão ficam à mostra, claramente em pé.
- **Leitor em pé, na vertical**: o corpo de 84 × 54 × 6,8 mm fica apoiado no piso, com a face
  (54 × 84) virada para a frente. Folga de 0,3 mm por lado no bolso; as paredes sobem até 65 mm
  e os ~22 mm do topo ficam à mostra para agarrar e remover.
- **Passagem de cabo**: vão de 20 × 12 mm na parede traseira, embaixo e centrado — alinhado com
  a saída do cabo que sai de trás do leitor, na base. O cabo atravessa e desce por trás do dock.
  Se precisar de outro tamanho: `NOTCH_W`/`NOTCH_H`.
- **Base**: 61 × 39,2 mm, com extensão frontal para não tombar ao inserir o cartão.

## Impressão

- Sem suportes, base no prato (como modelado), 0,2 mm, PLA ou PETG.
- 2 paredes + 15% de infil bastam (~35 g).

## Ajustes rápidos (topo do `gerar_dock.py`)

| Parâmetro | O que muda |
|---|---|
| `CLR_R` | Folga do leitor no bolso (0,3 mm/lado) |
| `CLR_C`, `SLOT` | Folgas do cartão na fenda |
| `GAP_RF` | Distância cartão ↔ face do leitor (4 mm) |
| `CHAN_H` → `GUIDE_H` | Altura das guias laterais/trilhos do cartão (65 mm) |
| `FRONT_H` | Altura da parede frontal baixa (30 mm) — quanto menor, mais cartão à mostra |
| `POCKET_H` | Altura das paredes do leitor (65 mm) |
| `NOTCH_W/H` | Passagem de cabo |

Para regenerar: `python3 gerar_dock.py`
