#!/usr/bin/env python3
"""Gera a tabela serial → título → região embutida no app (psx_data.txt).

Fontes aceitas (a primeira que existir):

1. ``gamesettings.ini`` do DuckStation — o banco de seriais aberto:
   https://github.com/libretro-shared?? não; baixe de
   https://github.com/stenzek/duckstation (data/resources/gamesettings.ini)
   Seções ``[SLUS-00402]`` com ``Title = Tekken 3``.
2. CSV simples ``serial;título;região`` (uma linha por jogo, com cabeçalho
   opcional) — para quem quiser manter a própria lista.

Região deduzida do prefixo do serial quando a fonte não traz:

    S ___ -U __ → USA         S L _ _        → Japão (SLPS/SLPM/SLPS)
    S ___ -E __ → Europa      _ C _ _ -U/E   → EUA/Europa (licenciado)

Uso:

    python3 scripts/gen_psx_data.py gamesettings.ini \
        > crates/domain/src/psx_data.txt
"""

import re
import sys

# prefixo do serial → rótulo de região (checado na ordem)
REGIOES = [
    ("SLUS", "USA"), ("SCUS", "USA"), ("LSP-", "USA"),
    ("SLES", "Europa"), ("SCES", "Europa"),
    ("SLPS", "Japão"), ("SLPM", "Japão"), ("SCPS", "Japão"),
    ("SIPS", "Japão"), ("SLED", "Europa"), ("SCED", "Europa"),
]

INI_SECTION = re.compile(r"^\[([A-Z]{4}-\d{5})\]\s*$", re.IGNORECASE)
INI_TITLE = re.compile(r"^\s*Title\s*=\s*(.+?)\s*$", re.IGNORECASE)
CSV_LINE = re.compile(r"^([A-Za-z]{4}[-_ ]?\d\w*)[;\t](.+?)(?:[;\t](\S+))?\s*$")


def regiao_de(serial: str, explicita: str | None) -> str | None:
    if explicita:
        return explicita
    prefixo = serial[:4].upper()
    for prefixo_conhecido, regiao in REGIOES:
        if prefixo.startswith(prefixo_conhecido[:3]) or prefixo == prefixo_conhecido:
            return regiao
    return None


def de_gamesettings(caminho: str) -> dict[str, tuple[str, str | None]]:
    jogos: dict[str, tuple[str, str | None]] = {}
    serial_atual: str | None = None
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip()
            m = INI_SECTION.match(linha)
            if m:
                serial_atual = m.group(1).upper()
                continue
            if serial_atual:
                m = INI_TITLE.match(linha)
                if m and m.group(1):
                    jogos[serial_atual] = (m.group(1), None)
    return jogos


def de_csv(caminho: str) -> dict[str, tuple[str, str | None]]:
    jogos: dict[str, tuple[str, str | None]] = {}
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.strip()
            if not linha or linha.startswith("#"):
                continue
            m = CSV_LINE.match(linha)
            if not m:
                continue
            serial = m.group(1).upper().replace("_", "-").replace(".", "")
            jogos[serial] = (m.group(2).strip(), m.group(3))
    return jogos


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    caminho = sys.argv[1]
    try:
        jogos = de_gamesettings(caminho)
    except Exception:
        jogos = {}
    if not jogos:
        jogos = de_csv(caminho)
    if not jogos:
        print(f"{caminho}: nenhum jogo reconhecido", file=sys.stderr)
        return 1

    print("# gerado por scripts/gen_psx_data.py — não editar à mão")
    print(f"# {len(jogos)} jogos")
    for serial in sorted(jogos):
        titulo, regiao = jogos[serial]
        regiao = regiao_de(serial, regiao)
        titulo = titulo.replace("\t", " ").strip()
        print(f"{serial}\t{titulo}\t{regiao or ''}".rstrip("\t"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
