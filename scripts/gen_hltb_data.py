#!/usr/bin/env python3
"""Gera a tabela serial → tempos do HowLongToBeat embutida no app (hltb_data.txt).

Casa os títulos do HowLongToBeat (HLTB) com os seriais do PSX — o app faz
o lookup pelo serial que já sai do scan do disco, sem rede e sem busca
nenhuma em runtime (mesmo molde de `psx.rs`/`tosec.rs`).

Fontes (arquivos locais, baixados à mão uma vez):

1. Dataset comunitário do HLTB — CSV de ~55 mil linhas com tempos
   (história principal / + extras / completista) de
   https://github.com/johagan94/hltb-dataset-plugin
   (dataset/hltb_dataset_filtered.csv). Só entram as linhas cuja lista de
   plataformas contém "PlayStation" — o nome que o HLTB dá ao PS1, sem
   confundir com PlayStation 2/3/4/5/PSP/Vita.
2. `gamedb.yaml` do DuckStation — o banco de seriais aberto:
   https://github.com/stenzek/duckstation (data/resources/gamedb.yaml).
   Seções `<SERIAL>:` com `name:` (título) e `saveName:` (nome de save,
   estilo No-Intro, com "(USA)"/"(Disc 1)" no fim).

Casamento: nome normalizado (minúsculas, só alfanuméricos — "Tekken 3" ==
"Tekken3", mesma regra do `cheats.rs`), sufixos "(...)" do lado do DuckStation
removidos antes. Sem casamento exato: um é prefixo do outro (título sem
subtítulo de um lado — "Alundra 2" vs "Alundra 2: A New Legend Begins").
Empate entre entradas HLTB: vence a mais votada (`*_polled`).

Formato de saída — uma linha por serial, tempos em MINUTOS inteiros, campo
vazio = o HLTB não tem aquele tempo:

    SERIAL\tmain[\textra[\tcompletista[\ttodos]]]

Uso:

    python3 scripts/gen_hltb_data.py hltb_dataset_filtered.csv gamedb.yaml \
        > crates/domain/src/hltb_data.txt
"""

from __future__ import annotations

import csv
import re
import sys
import unicodedata

# prefixo do serial → região (só pra reportar cobertura por região no fim)
REGIOES = [
    ("SLUS", "USA"), ("SCUS", "USA"), ("LSP-", "USA"),
    ("SLES", "Europa"), ("SCES", "Europa"), ("SLED", "Europa"), ("SCED", "Europa"),
    ("SLPS", "Japão"), ("SLPM", "Japão"), ("SCPS", "Japão"), ("SIPS", "Japão"),
]

SERIAL_KEY = re.compile(r"^([A-Z]{4}-\d{5}):\s*$")
YAML_STR = re.compile(r'^\s+(name|saveName):\s*"?(.*?)"?\s*$')
PARENTESE = re.compile(r"\([^)]*\)")
DISCO_SOLTO = re.compile(r"\s*[-–:]?\s*disc\s*\d+\s*$", re.IGNORECASE)

# casamento por prefixo: o mais curto tem que cobrir quase tudo do longo
# (títulos sem subtítulo de um lado) — abaixo disso é outro jogo
PREFIXO_MIN = 0.75


def normaliza(s: str) -> str:
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = s.lower().replace("&", " and ")
    return "".join(c for c in s if c.isalnum())


def minutos(h: str) -> str:
    """HLTB vem em horas com 2 casas ("10.88") → minutos inteiros ("653")."""
    if not h:
        return ""
    return str(round(float(h) * 60))


def de_hltb(caminho: str) -> dict[str, dict]:
    """Linhas do CSV com "PlayStation" na lista de plataformas, por nome
    normalizado; empate vence a entrada mais votada."""
    jogos: dict[str, dict] = {}
    with open(caminho, encoding="utf-8", newline="") as arq:
        for linha in csv.DictReader(arq):
            plats = [p.strip() for p in linha["platform"].split(",")]
            if "PlayStation" not in plats:
                continue
            votos = max(
                int(linha[c] or 0)
                for c in ("main_story_polled", "main_plus_sides_polled", "completionist_polled")
            )
            reg = normaliza(linha["name"])
            if not reg:
                continue
            atual = jogos.get(reg)
            if atual is None or votos > atual["votos"]:
                jogos[reg] = {
                "nome": linha["name"].strip(),
                "main": minutos(linha["main_story"]),
                "extra": minutos(linha["main_plus_sides"]),
                "comp": minutos(linha["completionist"]),
                "todos": minutos(linha["all_styles"]),
                "votos": votos,
            }
    return jogos


def de_gamedb(caminho: str) -> dict[str, list[str]]:
    """Serial → títulos candidatos (name + saveName, sem sufixos "(...)" e
    "(Disc N)"; discos diferentes do mesmo jogo herdam o mesmo tempo)."""
    jogos: dict[str, list[str]] = {}
    serial: str | None = None
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            m = SERIAL_KEY.match(linha)
            if m:
                serial = m.group(1)
                continue
            if serial is None:
                continue
            m = YAML_STR.match(linha)
            if not m or not m.group(2):
                continue
            titulo = PARENTESE.sub(" ", m.group(2))
            titulo = DISCO_SOLTO.sub("", titulo).replace('\\"', '"').strip()
            if titulo and titulo not in jogos.setdefault(serial, []):
                jogos[serial].append(titulo)
    return jogos


def hltb_do(titulos: list[str], hltb: dict[str, dict]) -> dict | None:
    """Casamento exato primeiro, prefixo depois — o melhor dos candidatos."""
    for titulo in titulos:
        reg = normaliza(titulo)
        if reg in hltb:
            return hltb[reg]
    melhor, nota = None, 0.0
    for titulo in titulos:
        reg = normaliza(titulo)
        if len(reg) < 4:
            continue
        for cand, jogo in hltb.items():
            curto, longo = sorted((len(reg), len(cand)))
            if longo == curto:
                continue
            # prefixo nas duas direções (o início do nome define o jogo;
            # subtítulo de um lado só)
            if reg.startswith(cand) or cand.startswith(reg):
                if curto / longo >= PREFIXO_MIN and curto / longo > nota:
                    melhor, nota = jogo, curto / longo
    return melhor


def regiao_de(serial: str) -> str:
    prefixo = serial[:4].upper()
    for prefixo_conhecido, regiao in REGIOES:
        if prefixo.startswith(prefixo_conhecido[:3]) or prefixo == prefixo_conhecido:
            return regiao
    return "outra"


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    hltb = de_hltb(sys.argv[1])
    seriais = de_gamedb(sys.argv[2])

    achados: dict[str, dict] = {}
    por_regiao: dict[str, list[str]] = {}
    for serial, titulos in sorted(seriais.items()):
        jogo = hltb_do(titulos, hltb)
        if jogo:
            achados[serial] = jogo
            por_regiao.setdefault(regiao_de(serial), []).append(jogo["nome"])

    usados = {j["nome"] for j in achados.values()}
    print("# gerado por scripts/gen_hltb_data.py — não editar à mão")
    print(f"# tempos do HowLongToBeat (minutos): main\\textra\\tcompletista\\ttodos")
    print(f"# {len(achados)} seriais de {len(seriais)}, {len(usados)} títulos HLTB de {len(hltb)}")
    for regiao in ("USA", "Europa", "Japão", "outra"):
        n = len(por_regiao.get(regiao, []))
        total = sum(1 for s in seriais if regiao_de(s) == regiao)
        print(f"# {regiao}: {n}/{total} seriais")
    for serial, jogo in sorted(achados.items()):
        if not (jogo["main"] or jogo["extra"] or jogo["comp"] or jogo["todos"]):
            continue
        print(f"{serial}\t{jogo['main']}\t{jogo['extra']}\t{jogo['comp']}\t{jogo['todos']}")

    # relatório de QA no stderr: o que ficou de fora entre os ocidentais
    sem = sorted(
        (s, " / ".join(ts))
        for s, ts in seriais.items()
        if s not in achados and regiao_de(s) in ("USA", "Europa")
    )
    print(f"\n-- sem casamento (USA/Europa): {len(sem)} de "
          f"{sum(1 for s in seriais if regiao_de(s) in ('USA', 'Europa'))} seriais",
          file=sys.stderr)
    for s, ts in sem[:60]:
        print(f"   {s}  {ts}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
