#!/usr/bin/env python3
"""Gera a arte do PolystnX e do SixteeN para o SteamGridDB (Steam Deck).

Estilo: manual do proprietário — papel creme, tinta, paleta da família
Retro Xperience. Tudo desenhado em vetor (Pillow) com supersampling,
fonte pixel própria para os wordmarks.

Saída: branding/steamgriddb/<app>/{grid_600x900,grid_920x430,
hero_3840x1240,logo_660x930,icon_512x512}.png

Uso: python3 scripts/gen_sgdb_art.py [--out DIR]
"""

import argparse
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

REPO = Path(__file__).resolve().parent.parent
OUT_DEFAULT = REPO / "branding" / "steamgriddb"

# Paleta da família (mesmas do site e do favicon)
PAPEL = "#f2ede3"
PAPEL_ALT = "#eae3d4"
PAPEL_SOMBRA = "#ddd5c2"
TINTA = "#211d18"
TINTA_FRACA = "#6b6459"
LINHA = "#c9c0ae"
ROXO = "#4a3f8f"
ROXO_VIVO = "#6a5fc9"
VERMELHO = "#b3382e"
VERDE = "#2e6b3f"
OURO = "#c9a227"
CART = "#d8d2c2"

# Listras-assinatura de cada app
STRIPES_POLY = [VERMELHO, ROXO_VIVO, ROXO, TINTA]
STRIPES_SIX = [VERMELHO, OURO, VERDE, ROXO]

ARIAL_BOLD = "/System/Library/Fonts/Supplemental/Arial Bold.ttf"


def ARIAL(size_px):
    return ImageFont.truetype(ARIAL_BOLD, max(6, int(size_px)))


# ---------------------------------------------------------------------------
# Fonte pixel própria (5 colunas; maiúsculas 7 linhas, minúsculas 5 linhas
# na linha de base — visual de console 8/16 bits)
# ---------------------------------------------------------------------------
GLYPHS = {
    "A": [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "B": ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."],
    "C": [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
    "D": ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "F": ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
    "G": [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".###."],
    "H": ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "I": ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####"],
    "J": ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."],
    "K": ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
    "L": ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
    "M": ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"],
    "N": ["#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#"],
    "O": [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "P": ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
    "Q": [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"],
    "R": ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
    "S": [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
    "T": ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
    "U": ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "V": ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "W": ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"],
    "X": ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
    "Y": ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
    "Z": ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
    "0": [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
    "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", "#####"],
    "2": [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    "3": ["####.", "....#", "....#", ".###.", "....#", "....#", "####."],
    "4": ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    "5": ["#####", "#....", "#....", "####.", "....#", "....#", "####."],
    "6": [".###.", "#...#", "#....", "####.", "#...#", "#...#", ".###."],
    "7": ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    "8": [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    "9": [".###.", "#...#", "#...#", ".####", "....#", "#...#", ".###."],
    # minúsculas: caixa-baixa de 5 linhas, na linha de base
    "o": [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###."],
    "l": ["..#..", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
    "y": [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "t": [".....", "..#..", "..#..", "#####", "..#..", "..#..", "..#.."],
    "n": [".....", ".....", "#...#", "##..#", "#.#.#", "#..##", "#...#"],
    "i": [".....", "..#..", ".....", "..#..", "..#..", "..#..", "..#.."],
    "x": [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#"],
    "e": [".....", ".....", ".###.", "#...#", "####.", "#....", ".###."],
    "s": [".....", ".....", ".####", "#....", ".###.", "....#", "####."],
    " ": [".....", ".....", ".....", ".....", ".....", ".....", "....."],
}

CELL = 6  # 5 colunas + 1 de espaço entre letras


def pixel_width(text, scale):
    return (len(text) * CELL - 1) * scale


def fit_scale(text, target_w, max_scale=200):
    """Escala da fonte pixel para o texto caber em target_w."""
    cells = len(text) * CELL - 1
    return max(2, min(max_scale, int(target_w / cells)))


def pixel_text(draw, xy, text, scale, fill, align="left"):
    """Desenha texto na fonte pixel. xy é o canto superior esquerdo (ou
    centro/topo, com align='center'). Devolve a largura desenhada."""
    width = pixel_width(text, scale)
    x, y = xy
    if align == "center":
        x -= width / 2
    for ch in text:
        for ry, row in enumerate(GLYPHS[ch]):
            for rx, cellch in enumerate(row):
                if cellch == "#":
                    draw.rectangle(
                        [x + rx * scale, y + ry * scale,
                         x + (rx + 1) * scale - 1, y + (ry + 1) * scale - 1],
                        fill=fill,
                    )
        x += CELL * scale
    return width


def pixel_text_outlined(draw, xy, text, scale, fill, outline, stroke,
                        align="left"):
    """Texto pixel com contorno (cópias deslocadas em 8 direções)."""
    width = pixel_width(text, scale)
    x, y = xy
    if align == "center":
        x -= width / 2
    for dx in (-stroke, 0, stroke):
        for dy in (-stroke, 0, stroke):
            if dx or dy:
                pixel_text(draw, (x + dx, y + dy), text, scale, outline)
    pixel_text(draw, (x, y), text, scale, fill)
    return width


def tracked_text(draw, xy, text, font, fill, tracking=0, align="left"):
    """Texto com espaçamento extra entre caracteres (Arial etc.)."""
    widths = [draw.textlength(ch, font=font) for ch in text]
    total = sum(widths) + tracking * (len(text) - 1)
    x, y = xy
    if align == "center":
        x -= total / 2
    elif align == "right":
        x -= total
    for ch, w in zip(text, widths):
        draw.text((x, y), ch, font=font, fill=fill)
        x += w + tracking
    return total


# ---------------------------------------------------------------------------
# Primitivas de cena
# ---------------------------------------------------------------------------
def margins(w, h):
    """Margens da moldura, relativas ao menor lado (banners 3:1 precisam
    disso — relativo à largura engoliria a altura)."""
    m = min(w, h)
    return 0.055 * m, 0.085 * m


def paper(draw, w, h, frame=True, fold=False):
    draw.rectangle([0, 0, w - 1, h - 1], fill=PAPEL)
    m1, m2 = margins(w, h)
    if frame:
        draw.rectangle([m1, m1, w - m1, h - m1], outline=TINTA,
                       width=max(2, int(0.008 * m1)))
        draw.rectangle([m2, m2, w - m2, h - m2], outline=TINTA_FRACA,
                       width=max(1, int(0.0025 * m1)))
    if fold:  # canto de página dobrado, canto superior direito
        f = 0.055 * w
        draw.polygon([(w - m1 - f, m1), (w - m1, m1), (w - m1, m1 + f)],
                     fill=PAPEL_SOMBRA, outline=TINTA)


def stripe(draw, cx, y, width, height, colors):
    n = len(colors)
    seg = width / n
    x = cx - width / 2
    for c in colors:
        draw.rectangle([x, y, x + seg - height * 0.18, y + height], fill=c)
        x += seg


def diamond(draw, cx, cy, size, colors=(VERMELHO, OURO, VERDE, ROXO)):
    """Losango 4 cores (marca do SixteeN): quadrado grande a 45°,
    dividido em quatro — como no favicon. Os quartos se tocam no centro."""
    r45 = 0.70710678
    s = size / 2 * r45          # meia-lado do quadrado grande
    hs = s / 2
    quads = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
    lw = max(1, int(size * 0.006))
    for (qx, qy), color in zip(quads, colors):
        lcx, lcy = qx * hs, qy * hs
        corners = [(lcx - hs, lcy - hs), (lcx + hs, lcy - hs),
                   (lcx + hs, lcy + hs), (lcx - hs, lcy + hs)]
        rot = [(cx + x * r45 - y * r45, cy + x * r45 + y * r45)
               for (x, y) in corners]
        draw.polygon(rot, fill=color, outline=TINTA, width=lw)


def disc(draw, cx, cy, r, label=ROXO_VIVO):
    """Disco com etiqueta roxa (marca do PolystnX)."""
    draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=TINTA)
    rl = r * 0.84
    draw.ellipse([cx - rl, cy - rl, cx + rl, cy + rl], fill=label,
                 outline=TINTA, width=max(2, int(r * 0.03)))
    # brilho do label
    rh = r * 0.60
    draw.arc([cx - rh, cy - rh, cx + rh, cy + rh], 200, 320,
             fill=PAPEL, width=max(2, int(r * 0.045)))
    hub = r * 0.30
    draw.ellipse([cx - hub, cy - hub, cx + hub, cy + hub], fill=PAPEL,
                 outline=TINTA, width=max(2, int(r * 0.03)))
    hole = r * 0.11
    draw.ellipse([cx - hole, cy - hole, cx + hole, cy + hole], fill=TINTA)


def glyph_button(draw, cx, cy, r, kind, color, lw):
    """Botão de face do controle de PSX: △ ○ ✕ □."""
    draw.ellipse([cx - r, cy - r, cx + r, cy + r], fill=PAPEL,
                 outline=TINTA, width=lw)
    g = r * 0.60
    if kind == "tri":
        draw.polygon([(cx, cy - g), (cx + g * 0.95, cy + g * 0.75),
                      (cx - g * 0.95, cy + g * 0.75)], outline=color, width=lw)
    elif kind == "cir":
        draw.ellipse([cx - g, cy - g, cx + g, cy + g], outline=color, width=lw)
    elif kind == "cross":
        d = g * 0.8
        draw.line([(cx - d, cy - d), (cx + d, cy + d)], fill=color, width=lw)
        draw.line([(cx - d, cy + d), (cx + d, cy - d)], fill=color, width=lw)
    elif kind == "sq":
        draw.rectangle([cx - g * 0.85, cy - g * 0.85, cx + g * 0.85,
                        cy + g * 0.85], outline=color, width=lw)


def dpad(draw, cx, cy, arm, th, color=TINTA):
    """Cruceta: arm = meio-comprimento do braço, th = espessura."""
    draw.rounded_rectangle([cx - arm, cy - th, cx + arm, cy + th],
                           radius=th * 0.35, fill=color)
    draw.rounded_rectangle([cx - th, cy - arm, cx + th, cy + arm],
                           radius=th * 0.35, fill=color)


# ---------------------------------------------------------------------------
# Consoles
# ---------------------------------------------------------------------------
def draw_psx(draw, x, y, w, S):
    """PSX visto de cima: tampa com disco, power e faixa frontal com
    memory cards e portas de controle (MC, PORT, MC, PORT)."""
    lw = max(2, int(0.012 * w))
    h_top = 0.62 * w
    band = 0.17 * w
    r = 0.09 * w

    # faixa frontal + tampa superior
    draw.rounded_rectangle([x, y, x + w, y + h_top + band], radius=r,
                           fill=PAPEL_SOMBRA, outline=TINTA, width=lw)
    draw.rounded_rectangle([x, y, x + w, y + h_top], radius=r,
                           fill=PAPEL_ALT, outline=TINTA, width=lw)
    draw.rounded_rectangle(
        [x + 0.035 * w, y + 0.05 * h_top, x + w - 0.035 * w,
         y + h_top - 0.05 * h_top], radius=r * 0.7,
        outline=LINHA, width=max(1, lw // 3))

    # botão power
    pcx, pcy = x + 0.125 * w, y + 0.30 * h_top
    pr = 0.036 * w
    draw.ellipse([pcx - pr, pcy - pr, pcx + pr, pcy + pr], fill=TINTA)
    draw.ellipse([pcx - pr * 0.45, pcy - pr * 0.45, pcx + pr * 0.45,
                  pcy + pr * 0.45], fill=PAPEL_ALT)
    tracked_text(draw, (pcx, pcy + pr + 0.030 * w), "POWER",
                 ARIAL(0.026 * w), TINTA_FRACA, tracking=0.006 * w,
                 align="center")

    # tampa do disco + disco
    lcx, lcy, lr = x + 0.645 * w, y + 0.47 * h_top, 0.27 * w
    draw.ellipse([lcx - lr, lcy - lr, lcx + lr, lcy + lr], fill=PAPEL,
                 outline=TINTA, width=lw)
    disc(draw, lcx, lcy, lr * 0.74)
    fr = lr * 0.90
    draw.arc([lcx - fr, lcy - fr, lcx + fr, lcy + fr], 115, 175,
             fill=TINTA_FRACA, width=max(2, lw // 2))

    # faixa frontal: memory card + porta, memory card + porta
    by = y + h_top + band / 2
    slots = [(0.20, "mc", VERMELHO), (0.36, "port", None),
             (0.64, "mc", ROXO), (0.80, "port", None)]
    for frac, kind, color in slots:
        cxc = x + frac * w
        if kind == "mc":
            sw, sh = 0.070 * w, 0.42 * band
            draw.rounded_rectangle(
                [cxc - sw / 2, by - sh / 2, cxc + sw / 2, by + sh / 2],
                radius=sh * 0.25, fill=color, outline=TINTA,
                width=max(1, lw // 2))
        else:
            pr2 = 0.058 * w
            draw.rounded_rectangle(
                [cxc - pr2, by - pr2 * 0.85, cxc + pr2, by + pr2 * 0.85],
                radius=pr2 * 0.3, fill=TINTA)
            hr = pr2 * 0.40
            draw.ellipse([cxc - hr, by - hr * 0.85, cxc + hr, by + hr * 0.85],
                         fill=PAPEL_ALT)


def draw_dualshock(draw, x, y, w, S):
    """Controle de PSX (digital, sem analógicos) visto de cima."""
    lw = max(2, int(0.013 * w))
    h = 0.58 * w
    gr = 0.185 * w
    body = [x + 0.10 * w, y, x + 0.90 * w, y + 0.60 * h]

    # silhueta: camada tinta expandida + preenchimento por cima (união limpa)
    exp = lw
    draw.rounded_rectangle([body[0] - exp, body[1] - exp, body[2] + exp,
                            body[3] + exp * 2], radius=0.15 * w + exp,
                           fill=TINTA)
    for gx in (x + 0.27 * w, x + 0.73 * w):
        gy = y + 0.58 * h
        draw.ellipse([gx - gr - exp, gy - gr - exp, gx + gr + exp,
                      gy + gr + exp], fill=TINTA)
    draw.rounded_rectangle(body, radius=0.15 * w, fill=PAPEL_ALT)
    for gx in (x + 0.27 * w, x + 0.73 * w):
        gy = y + 0.58 * h
        draw.ellipse([gx - gr, gy - gr, gx + gr, gy + gr], fill=PAPEL_ALT)

    # cruceta
    dpad(draw, x + 0.27 * w, y + 0.42 * h, 0.085 * w, 0.032 * w)

    # botões de face △ ○ ✕ □
    br, blw = 0.058 * w, max(2, int(0.010 * w))
    bcx, bcy, off = x + 0.73 * w, y + 0.42 * h, 0.105 * w
    glyph_button(draw, bcx, bcy - off, br, "tri", VERDE, blw)
    glyph_button(draw, bcx + off, bcy, br, "cir", VERMELHO, blw)
    glyph_button(draw, bcx, bcy + off, br, "cross", ROXO, blw)
    glyph_button(draw, bcx - off, bcy, br, "sq", OURO, blw)

    # select / start
    for cxc in (0.455 * w, 0.545 * w):
        draw.rounded_rectangle(
            [x + cxc - 0.026 * w, y + 0.30 * h, x + cxc + 0.026 * w,
             y + 0.42 * h], radius=0.010 * w, outline=TINTA,
            width=max(1, lw // 2))


def draw_snes(draw, x, y, w, S):
    """Console 16-bit visto de cima: slot com cartucho entrando (a
    assinatura do SixteeN), chaves roxas e badge de 4 cores."""
    lw = max(2, int(0.012 * w))
    h_top = 0.60 * w
    band = 0.17 * w
    r = 0.09 * w

    draw.rounded_rectangle([x, y, x + w, y + h_top + band], radius=r,
                           fill=PAPEL_SOMBRA, outline=TINTA, width=lw)
    draw.rounded_rectangle([x, y, x + w, y + h_top], radius=r,
                           fill=PAPEL_ALT, outline=TINTA, width=lw)
    draw.rounded_rectangle(
        [x + 0.035 * w, y + 0.05 * h_top, x + w - 0.035 * w,
         y + h_top - 0.05 * h_top], radius=r * 0.7,
        outline=LINHA, width=max(1, lw // 3))

    # cartucho entrando no slot
    cart_w, cart_h = 0.32 * w, 0.32 * w
    scx, scy = x + 0.40 * w, y + 0.58 * h_top
    ccy = scy - cart_h * 0.36
    draw.rounded_rectangle(
        [scx - cart_w / 2, ccy - cart_h / 2, scx + cart_w / 2,
         ccy + cart_h / 2], radius=0.018 * w, fill=CART, outline=TINTA,
        width=lw)
    for i in range(3):  # linhas de pressão no topo
        lx = scx - cart_w * 0.28 + i * cart_w * 0.28
        draw.line([(lx, ccy - cart_h / 2 + 0.020 * w),
                   (lx, ccy - cart_h / 2 + 0.055 * w)],
                  fill=TINTA_FRACA, width=max(1, lw // 2))
    lab_w, lab_h = cart_w * 0.82, cart_h * 0.46
    draw.rectangle(
        [scx - lab_w / 2, ccy - lab_h / 2, scx + lab_w / 2, ccy + lab_h / 2],
        fill=PAPEL, outline=TINTA, width=max(1, lw // 2))
    diamond(draw, scx, ccy - lab_h * 0.14, lab_w * 0.46)
    tracked_text(draw, (scx, ccy + lab_h * 0.16), "SixteeN",
                 ARIAL(0.030 * w), TINTA, tracking=0.004 * w, align="center")
    # slot por cima da base do cartucho
    draw.rounded_rectangle(
        [scx - (cart_w * 0.5 + 0.025 * w), scy - 0.030 * w,
         scx + (cart_w * 0.5 + 0.025 * w), scy + 0.030 * w],
        radius=0.018 * w, fill=TINTA)
    # linhas de movimento, acima do cartucho (dentro da face)
    cart_top = ccy - cart_h / 2
    for i in range(2):
        ly = cart_top - 0.028 * w - i * 0.030 * w
        draw.line([(scx - cart_w * 0.72, ly), (scx - cart_w * 0.55, ly)],
                  fill=LINHA, width=max(1, lw // 2))
        draw.line([(scx + cart_w * 0.55, ly), (scx + cart_w * 0.72, ly)],
                  fill=LINHA, width=max(1, lw // 2))

    # chaves Power / Reset (roxas)
    for i, name in enumerate(("POWER", "RESET")):
        kx, ky = x + 0.80 * w, y + (0.28 + 0.28 * i) * h_top
        draw.rounded_rectangle(
            [kx - 0.080 * w, ky - 0.026 * w, kx + 0.080 * w, ky + 0.026 * w],
            radius=0.018 * w, outline=TINTA, width=max(1, lw // 2), fill=PAPEL)
        off = -0.032 * w if i == 0 else 0.032 * w
        draw.rounded_rectangle(
            [kx + off - 0.026 * w, ky - 0.022 * w, kx + off + 0.026 * w,
             ky + 0.022 * w], radius=0.010 * w, fill=ROXO_VIVO,
            outline=TINTA, width=max(1, lw // 2))
        tracked_text(draw, (kx, ky + 0.045 * w), name, ARIAL(0.022 * w),
                     TINTA_FRACA, tracking=0.005 * w, align="center")

    # badge de 4 cores
    diamond(draw, x + 0.13 * w, y + 0.30 * h_top, 0.14 * w)

    # faixa frontal: portas de controle
    by = y + h_top + band / 2
    for frac in (0.32, 0.68):
        cxc = x + frac * w
        pr2 = 0.058 * w
        draw.rounded_rectangle(
            [cxc - pr2, by - pr2 * 0.85, cxc + pr2, by + pr2 * 0.85],
            radius=pr2 * 0.3, fill=TINTA)
        hr = pr2 * 0.40
        draw.ellipse([cxc - hr, by - hr * 0.85, cxc + hr, by + hr * 0.85],
                     fill=PAPEL_ALT)


def draw_snespad(draw, x, y, w, S):
    """Controle de SNES visto de cima."""
    lw = max(2, int(0.013 * w))
    h = 0.52 * w
    draw.rounded_rectangle([x + 0.06 * w, y, x + 0.94 * w, y + h],
                           radius=0.20 * w, fill=PAPEL_ALT,
                           outline=TINTA, width=lw)

    # cruceta
    dpad(draw, x + 0.24 * w, y + 0.50 * h, 0.075 * w, 0.028 * w)

    # botões coloridos em losango, com letra
    br = 0.058 * w
    blw = max(2, int(0.009 * w))
    fnt = ARIAL(0.032 * w)
    buttons = [
        (0.76, 0.24, "X", ROXO),
        (0.63, 0.50, "Y", VERDE),
        (0.89, 0.50, "A", VERMELHO),
        (0.76, 0.76, "B", OURO),
    ]
    for fx, fy, letter, color in buttons:
        cx_, cy_ = x + fx * w, y + fy * h
        draw.ellipse([cx_ - br, cy_ - br, cx_ + br, cy_ + br], fill=color,
                     outline=TINTA, width=blw)
        tw = draw.textlength(letter, font=fnt)
        thh = fnt.size * 0.72
        draw.text((cx_ - tw / 2, cy_ - thh / 2 - fnt.size * 0.12), letter,
                  font=fnt, fill=PAPEL)

    # select / start
    for cxc in (0.455 * w, 0.545 * w):
        draw.rounded_rectangle(
            [x + cxc - 0.024 * w, y + 0.40 * h, x + cxc + 0.024 * w,
             y + 0.60 * h], radius=0.008 * w, outline=TINTA,
            width=max(1, lw // 2))


# ---------------------------------------------------------------------------
# Composição por tipo de asset
# ---------------------------------------------------------------------------
APPS = {
    "polystnx": {
        "wordmark": "PolyStnX",
        "subtitle": "PSX XPERIENCE",
        "stripes": STRIPES_POLY,
        "tag": "Nº 02 — PSX",
        "footer": "leitor de disco · memory card · controle de verdade",
        "console": draw_psx,
        "pad": draw_dualshock,
        "pad_ratio": 0.60,   # altura do controle = ratio × largura
        "accent": ROXO,
    },
    "sixteen": {
        "wordmark": "SixteeN",
        "subtitle": "SNES XPERIENCE",
        "stripes": STRIPES_SIX,
        "tag": "Nº 01 — SNES",
        "footer": "gabinete · tubo crt · cartucho de verdade",
        "console": draw_snes,
        "pad": draw_snespad,
        "pad_ratio": 0.52,
        "accent": VERMELHO,
    },
}


def header_footer(draw, w, h, cfg, small_size, tag_right=None, footer_cx=None,
                  header_left=True):
    fnt = ARIAL(small_size)
    hy = 0.085 * h
    if header_left:
        tracked_text(draw, (0.095 * w, hy), "RETRO XPERIENCE", fnt,
                     TINTA_FRACA, tracking=0.010 * w)
    tracked_text(draw, (tag_right if tag_right else w - 0.095 * w, hy),
                 cfg["tag"], fnt, TINTA_FRACA, tracking=0.006 * w,
                 align="right")
    _, m2 = margins(w, h)
    tracked_text(draw, (footer_cx if footer_cx else w / 2,
                        h - m2 - 1.9 * small_size),
                 cfg["footer"], fnt, TINTA_FRACA, tracking=0.004 * w,
                 align="center")


def render(w, h, S):
    img = Image.new("RGB", (w * S, h * S), PAPEL)
    return img, ImageDraw.Draw(img)


def grain(img, w, h, step=7):
    """Grão de papel: pontinhos quase invisíveis, como no site."""
    ov = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(ov)
    c = (33, 29, 24, 10)
    for yy in range(4, h, step):
        for xx in range(4, w, step):
            d.point((xx, yy), fill=c)
    return Image.alpha_composite(img.convert("RGBA"), ov).convert("RGB")


def make_grid_portrait(app, out, S=3):
    w, h = 600, 900
    cfg = APPS[app]
    img, d = render(w, h, S)
    W, H = w * S, h * S
    paper(d, W, H, fold=True)
    header_footer(d, W, H, cfg, 0.020 * H)

    stripe(d, W / 2, 0.115 * H, 0.60 * W, 0.014 * H, cfg["stripes"])

    # console
    cw = 0.76 * W
    cy = 0.155 * H
    cfg["console"](d, (W - cw) / 2, cy, cw, S)
    console_bottom = cy + cw * 0.79

    # controle
    pw = 0.38 * W
    py = console_bottom + 0.028 * H
    cfg["pad"](d, (W - pw) / 2, py, pw, S)
    pad_bottom = py + pw * cfg["pad_ratio"]

    # wordmark + subtítulo
    max_w = 0.72 * W
    y0 = max(pad_bottom + 0.030 * H, 0.74 * H)
    s1 = fit_scale(cfg["wordmark"], max_w)
    pixel_text(d, (W / 2, y0), cfg["wordmark"], s1, TINTA, align="center")
    y1 = y0 + 7 * s1 + 0.028 * H
    s2 = fit_scale(cfg["subtitle"], max_w * 0.80)
    pixel_text(d, (W / 2, y1), cfg["subtitle"], s2, cfg["accent"],
               align="center")

    img = img.resize((w, h), Image.LANCZOS)
    img = grain(img, w, h)
    img.save(out / app / "grid_600x900.png")


def make_grid_landscape(app, out, S=3):
    w, h = 920, 430
    cfg = APPS[app]
    img, d = render(w, h, S)
    W, H = w * S, h * S
    m1, m2 = margins(W, H)
    paper(d, W, H)

    # console à direita, dimensionado pela altura
    ch = 0.66 * H
    cw = ch / 0.79
    cx = W - cw - 0.055 * W
    cy = (H - ch) / 2
    cfg["console"](d, cx, cy, cw, S)

    # bloco de texto à esquerda
    tx = 0.095 * W
    max_w = cx - 0.025 * W - tx
    s1 = fit_scale(cfg["wordmark"], max_w)
    ty = 0.30 * H
    pixel_text(d, (tx, ty), cfg["wordmark"], s1, TINTA)
    s2 = fit_scale(cfg["subtitle"], max_w)
    ty2 = ty + 7 * s1 + 0.075 * H
    pixel_text(d, (tx, ty2), cfg["subtitle"], s2, cfg["accent"])
    block_bottom = ty2 + 7 * s2

    # cabeçalho/rodapé confinados à coluna de texto
    header_footer(d, W, H, cfg, 0.032 * H, tag_right=cx - 0.025 * W,
                  footer_cx=tx + max_w / 2, header_left=False)
    footer_y = H - m2 - 1.9 * 0.032 * H

    # controle sob o texto, sem invadir rodapé nem moldura
    pw = 0.165 * W
    pad_h = pw * cfg["pad_ratio"]
    py = min(block_bottom + 0.030 * H, footer_y - pad_h - 0.015 * H)
    cfg["pad"](d, tx, py, pw, S)

    img = img.resize((w, h), Image.LANCZOS)
    img = grain(img, w, h)
    img.save(out / app / "grid_920x430.png")


def make_hero(app, out, S=2):
    w, h = 3840, 1240
    cfg = APPS[app]
    img, d = render(w, h, S)
    W, H = w * S, h * S
    m1, m2 = margins(W, H)
    paper(d, W, H)

    # console à direita
    ch = 0.72 * H
    cw = ch / 0.79
    cx = W - cw - 0.05 * W
    cy = (H - ch) / 2
    cfg["console"](d, cx, cy, cw, S)

    # bloco de texto à esquerda
    tx = 0.10 * W
    max_w = cx - 0.02 * W - tx
    s1 = min(fit_scale(cfg["wordmark"], max_w), int(0.20 * H / 7))
    ty = 0.28 * H
    pixel_text(d, (tx, ty), cfg["wordmark"], s1, TINTA)
    s2 = min(fit_scale(cfg["subtitle"], max_w), int(0.085 * H / 7))
    ty2 = ty + 7 * s1 + 0.040 * H
    pixel_text(d, (tx, ty2), cfg["subtitle"], s2, cfg["accent"])
    block_bottom = ty2 + 7 * s2

    header_footer(d, W, H, cfg, 0.042 * H, tag_right=cx - 0.02 * W,
                  footer_cx=tx + max_w / 2)
    footer_y = H - m2 - 1.9 * 0.042 * H

    # controle sob o texto
    pw = 0.10 * W
    pad_h = pw * cfg["pad_ratio"]
    py = min(block_bottom + 0.035 * H, footer_y - pad_h - 0.02 * H)
    cfg["pad"](d, tx + 0.005 * W, py, pw, S)

    img = img.resize((w, h), Image.LANCZOS)
    img = grain(img, w, h, step=6)
    img.save(out / app / "hero_3840x1240.png")


def make_logo(app, out, S=3):
    w, h = 660, 930
    cfg = APPS[app]
    img = Image.new("RGBA", (w * S, h * S), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    W, H = w * S, h * S
    stroke = 3 * S

    # motivo acima do wordmark
    motif_cy = 0.24 * H
    if app == "polystnx":
        disc(d, W / 2, motif_cy, 0.21 * W)
    else:
        diamond(d, W / 2, motif_cy, 0.46 * W)

    wm = cfg["wordmark"]
    s1 = fit_scale(wm, 0.84 * W)
    y0 = 0.52 * H
    pixel_text_outlined(d, (W / 2, y0), wm, s1, TINTA, PAPEL, stroke,
                        align="center")
    sub = cfg["subtitle"]
    s2 = fit_scale(sub, 0.52 * W)
    y1 = y0 + 7 * s1 + 0.055 * H
    pixel_text_outlined(d, (W / 2, y1), sub, s2, cfg["accent"], PAPEL,
                        stroke, align="center")
    stripe(d, W / 2, y1 + 7 * s2 + 0.065 * H, 0.42 * W, 0.014 * H,
           cfg["stripes"])

    img = img.resize((w, h), Image.LANCZOS)
    img.save(out / app / "logo_660x930.png")


def make_icon(app, out, S=4):
    w = 512
    img, d = render(w, w, S)
    W = w * S
    # moldura pesada, como no favicon
    m = lw_ = 0.0625 * W
    d.rectangle([m, m, W - m, W - m], outline=TINTA, width=int(lw_))

    if app == "polystnx":
        # fiel ao favicon: disco roxo + dois pontos de memory card
        disc(d, W * 0.5, W * 0.45, W * 0.265)
        for cx_, color in ((0.3125 * W, VERMELHO), (0.6875 * W, ROXO)):
            r = 0.0625 * W
            d.ellipse([cx_ - r, W * 0.8125 - r, cx_ + r, W * 0.8125 + r],
                      fill=color)
    else:
        diamond(d, W * 0.5, W * 0.46, 0.58 * W)
        s = fit_scale("N", 0.24 * W)
        pixel_text(d, (W * 0.5, W * 0.46 - 3.5 * s), "N", s, PAPEL,
                   align="center")

    img = img.resize((w, w), Image.LANCZOS)
    grain(img, w, w).save(out / app / "icon_512x512.png")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", type=Path, default=OUT_DEFAULT)
    args = ap.parse_args()
    out = args.out
    for name in ("polystnx", "sixteen"):
        (out / name).mkdir(parents=True, exist_ok=True)

    for app in ("polystnx", "sixteen"):
        make_grid_portrait(app, out)
        make_grid_landscape(app, out)
        make_hero(app, out)
        make_logo(app, out)
        make_icon(app, out)
        print(f"[ok] {app}: 5 imagens em {out / app}")


if __name__ == "__main__":
    main()
