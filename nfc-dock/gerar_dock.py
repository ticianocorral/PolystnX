#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Dock de mesa para leitor NFC em pe, com insercao vertical do cartao (estilo cartucho).

Geometria (mm):
  - Leitor: 54.0 (X) x 6.8 (Y) x 84.0 (Z) — em pe na vertical, face virada para +Y (frente).
  - Cartao CR80 de pe (retrato): 54.0 (X) x 0.76-0.84 (Y) x 85.6 (Z), entra por cima,
    desliza na fenda e para cobrindo toda a face do leitor (4 mm a frente dela).

O STL sai como solidos sobrepostos de proposito (uniao feita pelo fatiador:
Cura / PrusaSlicer / Orca / Bambu Studio unem na importacao).
"""
import os
import struct

# ---------------- parâmetros (mm) ----------------
RW, RH, RT = 54.0, 84.0, 6.8      # leitor em pe (vertical): largura X, altura Z, espessura Y
CW, CH, CT = 54.0, 85.6, 0.84     # cartao de pe (retrato): CR80 0.76–0.84
T         = 3.0    # parede geral
FT        = 3.0    # espessura do piso
CLR_R     = 0.3    # folga do leitor por lado
CLR_C     = 0.5    # folga lateral do cartao por lado
SLOT      = 1.4    # espessura da fenda do cartao
GAP_RF    = 4.0    # face do leitor -> face traseira do cartao
POCKET_H  = 65.0   # altura das paredes que seguram o leitor (~22 mm do topo a mostra)
GUIDE_H   = 65.0   # altura das guias laterais/trilhos (cartao de 85.6 fica ~72% guiado)
FRONT_H   = 30.0   # parede frontal BAIXA: a face do cartao fica a mostra acima dela
FLARE_H   = 4.0    # altura do funil de entrada da fenda
FLARE     = 1.0    # abertura do funil por face
FRONT_EXT = 20.0   # piso avanca a frente (estabilidade)
NOTCH_W   = 20.0   # passagem de cabo: sai por tras do leitor, embaixo, bem no meio
NOTCH_H   = 12.0   # (cabo + moldagem do conector, com folga)
RAIL_T    = 2.0    # espessura do trilho traseiro da fenda (so nas pontas, porem de RF limpo)
RAIL_L    = 15.0   # comprimento do trilho (cada lado)
FRONT_W   = 4.0    # espessura da parede frontal da fenda

# ---------------- derivados ----------------
PWI_X, PWI_Y = RW + 2 * CLR_R, RT + 2 * CLR_R   # interior do bolso do leitor
POX     = PWI_X / 2 + T        # meia-largura externa do bolso
RF      = RT                   # Y da face do leitor (nominal, encostado no fundo)
CB      = RF + GAP_RF          # Y da face traseira do cartao apoiado nos trilhos
SLOT_Y1 = CB + SLOT            # fim da fenda
RAIL_Y0 = CB - RAIL_T          # inicio do trilho traseiro
FW_Y1   = SLOT_Y1 + FRONT_W    # fundo da parede frontal
CWI     = CW / 2 + CLR_C       # meia-largura interna do canal do cartao
COX     = CWI + T              # meia-largura externa do corpo
FLOOR_Y1 = FW_Y1 + FRONT_EXT   # frente do piso


def box(x0, x1, y0, y1, z0, z1, tdx=0.0, tdy=0.0):
    """Paralelepipedo; a face do topo (z1) pode ser deslocada (tdx,tdy) -> cunha/funil."""
    b0 = (x0, y0, z0); b1 = (x1, y0, z0); b2 = (x1, y1, z0); b3 = (x0, y1, z0)
    t0 = (x0 + tdx, y0 + tdy, z1); t1 = (x1 + tdx, y0 + tdy, z1)
    t2 = (x1 + tdx, y1 + tdy, z1); t3 = (x0 + tdx, y1 + tdy, z1)
    quads = ((b0, b3, b2, b1), (t0, t1, t2, t3), (b0, b1, t1, t0),
             (b2, b3, t3, t2), (b3, b0, t0, t3), (b1, b2, t2, t1))
    tris = []
    for a, b, c, d in quads:
        tris.append((a, b, c))
        tris.append((a, c, d))
    return tris


def build_parts():
    P = []
    # piso (base inteira)
    P += box(-COX, COX, -T, FLOOR_Y1, 0.0, FT)
    # bolso do leitor: paredes laterais (deixam o topo do leitor a mostra p/ remover)
    P += box(PWI_X / 2, POX, -T, RAIL_Y0 + 0.2, FT, POCKET_H)
    P += box(-POX, -PWI_X / 2, -T, RAIL_Y0 + 0.2, FT, POCKET_H)
    # parede traseira com passagem de cabo embaixo
    P += box(-POX, -NOTCH_W / 2, -T, 0.0, FT, POCKET_H)
    P += box(NOTCH_W / 2, POX, -T, 0.0, FT, POCKET_H)
    P += box(-NOTCH_W / 2, NOTCH_W / 2, -T, 0.0, FT + NOTCH_H, POCKET_H)
    # canal do cartao: guias laterais e trilhos ALTOS (acompanham o cartao de pe)
    for s in (-1.0, 1.0):
        x0, x1 = (CWI, COX) if s > 0 else (-COX, -CWI)
        P += box(x0, x1, RAIL_Y0, FW_Y1, FT, GUIDE_H - FLARE_H)
        P += box(x0, x1, RAIL_Y0, FW_Y1, GUIDE_H - FLARE_H, GUIDE_H, tdx=s * FLARE)
        ra, rb = s * (CWI - RAIL_L), s * (CWI + 1.0)   # trilho entra 1 mm na guia
        rx0, rx1 = min(ra, rb), max(ra, rb)
        P += box(rx0, rx1, RAIL_Y0, CB, FT, GUIDE_H - FLARE_H)
        P += box(rx0, rx1, RAIL_Y0, CB, GUIDE_H - FLARE_H, GUIDE_H, tdy=-FLARE)
        # trilho frontal fino: segue guiando a face do cartao acima da parede baixa
        fa, fb = s * (CWI - 4.5), s * (CWI + 1.0)
        fx0, fx1 = min(fa, fb), max(fa, fb)
        P += box(fx0, fx1, SLOT_Y1, SLOT_Y1 + 1.5, FT, GUIDE_H - FLARE_H)
        P += box(fx0, fx1, SLOT_Y1, SLOT_Y1 + 1.5, GUIDE_H - FLARE_H, GUIDE_H, tdy=FLARE)
    # parede frontal baixa: a maior parte da face do cartao fica a mostra (cartao em pe)
    P += box(-CWI - 1.0, CWI + 1.0, SLOT_Y1, FW_Y1, FT, FRONT_H - FLARE_H)
    P += box(-CWI - 1.0, CWI + 1.0, SLOT_Y1, FW_Y1, FRONT_H - FLARE_H, FRONT_H, tdy=FLARE)
    return P


def ghosts():
    """Leitor e cartao na posicao de uso (so para os previews, nao entram no STL)."""
    reader = box(-RW / 2, RW / 2, 0.0, RT, FT, FT + RH)
    card = box(-CW / 2, CW / 2, CB, CB + CT, FT, FT + CH)
    return [(reader, (0.62, 0.64, 0.68)), (card, (0.22, 0.48, 0.92))]


def export_stl(path, tris):
    with open(path, 'wb') as f:
        f.write(b'dock leitor nfc - cartao tipo cartucho'.ljust(80, b'\0'))
        f.write(struct.pack('<I', len(tris)))
        for t in tris:
            (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
            ux, uy, uz = bx - ax, by - ay, bz - az
            vx, vy, vz = cx - ax, cy - ay, cz - az
            nx, ny, nz = uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx
            l = (nx * nx + ny * ny + nz * nz) ** 0.5 or 1.0
            f.write(struct.pack('<3f', nx / l, ny / l, nz / l))
            f.write(struct.pack('<9f', ax, ay, az, bx, by, bz, cx, cy, cz))
            f.write(struct.pack('<H', 0))


def render(out_png, parts, ghosts_list, titulo):
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    from mpl_toolkits.mplot3d.art3d import Poly3DCollection
    import numpy as np

    def add(ax, grupos):
        # tudo numa unica colecao: a ordenacao por poligono faz a oclusao certa
        tris_all, cols = [], []
        for tris, base, alpha in grupos:
            v = np.array(tris)
            n = np.cross(v[:, 1] - v[:, 0], v[:, 2] - v[:, 0])
            n = n / (np.linalg.norm(n, axis=1, keepdims=True) + 1e-12)
            L = np.array([-0.4, -0.55, 0.73]); L = L / np.linalg.norm(L)
            lam = np.abs(n @ L)
            rgb = np.clip((0.42 + 0.58 * lam)[:, None] * np.array(base)[None, :], 0, 1)
            tris_all.append(v)
            cols.append(np.hstack([rgb, np.full((len(v), 1), alpha)]))
        v = np.vstack(tris_all)
        pc = Poly3DCollection(v, facecolors=np.vstack(cols), edgecolors='none')
        ax.add_collection3d(pc)

    vistas = ((24, 55, 'iso frontal'), (8, 90, 'frontal'),
              (10, 0, 'lateral (fenda)'), (24, 235, 'iso traseira'))
    fig = plt.figure(figsize=(15, 11), dpi=105)
    tudo = [np.array(p).reshape(-1, 3) for p in parts]
    tudo += [np.array(g).reshape(-1, 3) for g, _ in ghosts_list]
    A = np.vstack(tudo)
    lo, hi = A.min(0), A.max(0)
    c = (lo + hi) / 2
    r = (hi - lo).max() * 0.56
    for i, (el, az, lab) in enumerate(vistas, 1):
        ax = fig.add_subplot(2, 2, i, projection='3d')
        grupos = [(parts, (0.70, 0.74, 0.79), 1.0)]
        grupos += [(g, col, 0.92) for g, col in ghosts_list]
        add(ax, grupos)
        ax.set_xlim(c[0] - r, c[0] + r)
        ax.set_ylim(c[1] - r, c[1] + r)
        ax.set_zlim(c[2] - r, c[2] + r)
        ax.set_box_aspect((1, 1, 1))
        ax.view_init(elev=el, azim=az)
        ax.set_axis_off()
        ax.set_title(lab)
    fig.suptitle(titulo)
    fig.savefig(out_png, bbox_inches='tight')
    plt.close(fig)


if __name__ == '__main__':
    aqui = os.path.dirname(os.path.abspath(__file__))
    partes = build_parts()
    stl = os.path.join(aqui, 'dock_leitor_nfc.stl')
    export_stl(stl, partes)
    print('STL:', stl, '-', len(partes), 'triangulos')
    print('Envolvente: X %.1f  Y %.1f  Z %.1f (mm)' % (2 * COX, FLOOR_Y1 + T, POCKET_H))
    print('Fenda do cartao: Y %.1f..%.1f (folga %.2f/lado p/ cartao 0.84), largura %.1f' %
          (CB, SLOT_Y1, (SLOT - CT) / 2, 2 * CWI))
    print('Cartao fica %.1f mm a frente da face do leitor' % GAP_RF)
    try:
        render(os.path.join(aqui, 'preview_com_cartao.png'), partes, ghosts(),
               'Dock NFC - com leitor e cartao em posicao de uso')
        render(os.path.join(aqui, 'preview_vazio.png'), partes, [],
               'Dock NFC - peca impressa')
        print('previews ok')
    except Exception as e:
        print('preview indisponivel:', e)
