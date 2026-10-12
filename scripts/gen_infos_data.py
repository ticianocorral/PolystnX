#!/usr/bin/env python3
"""Gera a tabela única de infos do PolystnX (infos_data.txt), por SERIAL.

Une tudo que o painel mostra num lugar só — nome canônico, lançamento,
editora, região, nota (IGDB) e os tempos do HowLongToBeat — embutido no
binário: o usuário instala o app, solta a ROM e as infos estão lá, sem DAT
e sem rede.

Fontes (arquivos locais em `scripts/dados/`):

1. `gamedb.yaml` — o banco de seriais do DuckStation (nome, data de
   lançamento, editora).
2. `nointro_psx.dat` — o DAT No-Intro em clrmamepro (nome canônico por
   serial, com `serial "SLUS-xxxxx"` nos blocos).
3. `hltb_psx.txt` — os tempos do HowLongToBeat já casados por serial
   (saída do `gen_hltb_data.py`).
4. `igdb_psx.json` — índice {nome normalizado: {nota, ano}} extraído da
   IGDB (plataforma 7, do espelho GameDB do LizardByte).

Casamento com a IGDB: nome normalizado (minúsculas, só alfanuméricos,
artigo reordenado) — exato primeiro, prefixo depois.

Formato de saída — `SERIAL\\tnome\\tano\\teditora\\tregião\\tnota\\tmain\\textra`
(minutos), campo vazio = sem dado.

Uso:

    python3 scripts/gen_infos_data.py > crates/domain/src/infos_data.txt
"""

from __future__ import annotations

import json
import re
import sys
import unicodedata

ARTIGO = re.compile(
    r"^([^,]+), (The|A|An|La|Le|Les|Der|Die|Das|El|Los|Las|O|Os|As)\b[,:]?\s*(.*)$",
    re.IGNORECASE,
)
SERIAL_KEY = re.compile(r"^([A-Z]{4}-\d{5}):\s*$")
YAML_STR = re.compile(r'^\s+(\w+):\s*"?(.*?)"?\s*$')
DAT_BLOCK = re.compile(r"^game \(\s*$")
DAT_QUOTED = re.compile(r'^\s*(name|region|serial)\s+"(.*)"\s*$')
PARENTESE = re.compile(r"\([^)]*\)")

REGIOES = [
    ("SLUS", "USA"), ("SCUS", "USA"), ("LSP-", "USA"),
    ("SLES", "Europa"), ("SCES", "Europa"), ("SLED", "Europa"), ("SCED", "Europa"),
    ("SLPS", "Japão"), ("SLPM", "Japão"), ("SCPS", "Japão"), ("SIPS", "Japão"),
]


def normaliza(s: str) -> str:
    s = unicodedata.normalize("NFKD", s)
    s = "".join(c for c in s if not unicodedata.combining(c))
    s = s.lower().replace("&", " and ")
    return "".join(c for c in s if c.isalnum())


def sem_sufixos(s: str) -> str:
    return PARENTESE.sub(" ", s).strip()


def des_inverte_artigo(s: str) -> str:
    m = ARTIGO.match(s.strip())
    if not m:
        return s
    cabeca, artigo, resto = m.group(1), m.group(2), m.group(3)
    return f"{artigo} {cabeca} {resto}".strip()


def regiao_de(serial: str) -> str:
    prefixo = serial[:4].upper()
    for prefixo_conhecido, regiao in REGIOES:
        if prefixo.startswith(prefixo_conhecido[:3]) or prefixo == prefixo_conhecido:
            return regiao
    return "outra"


def de_gamedb(caminho: str) -> dict[str, dict]:
    """serial → {nome, ano, editora} do banco do DuckStation."""
    jogos: dict[str, dict] = {}
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
            if not m:
                continue
            campo, valor = m.group(1), m.group(2).strip()
            if campo == "name" and valor and serial not in jogos:
                jogos[serial] = {"nome": valor, "ano": "", "editora": ""}
            elif campo == "releaseDate" and valor and len(valor) >= 4:
                jogos.setdefault(serial, {"nome": "", "ano": "", "editora": ""})
                if not jogos[serial]["ano"]:
                    jogos[serial]["ano"] = valor[:4]
            elif campo == "publisher" and valor:
                jogos.setdefault(serial, {"nome": "", "ano": "", "editora": ""})
                if not jogos[serial]["editora"]:
                    jogos[serial]["editora"] = valor
    return jogos


def de_dat(caminho: str) -> dict[str, dict]:
    """serial → {nome canônico, região} do DAT No-Intro (clrmamepro)."""
    jogos: dict[str, dict] = {}
    em_jogo = False
    nome = ""
    regiao = ""
    serial = ""
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            if DAT_BLOCK.match(linha):
                em_jogo, nome, regiao, serial = True, "", "", ""
                continue
            if not em_jogo:
                continue
            if linha.startswith(")"):
                em_jogo = False
                if serial and nome:
                    for s in serial.split(","):
                        s = s.strip()
                        if s and s not in jogos:
                            jogos[s] = {"nome": nome, "regiao": regiao}
                continue
            m = DAT_QUOTED.match(linha)
            if m:
                campo, valor = m.group(1), m.group(2).strip()
                if campo == "name" and not nome:
                    nome = valor
                elif campo == "region" and not regiao:
                    regiao = valor
                elif campo == "serial" and not serial:
                    serial = valor.upper()
    return jogos


def de_hltb(caminho: str) -> dict[str, tuple[str, str]]:
    """serial → (principal, extras) em minutos."""
    jogos: dict[str, tuple[str, str]] = {}
    with open(caminho, encoding="utf-8") as arq:
        for linha in arq:
            linha = linha.rstrip("\n")
            if linha.startswith("#"):
                continue
            p = linha.split("\t")
            if len(p) >= 3:
                jogos[p[0]] = (p[1], p[2])
    return jogos



def igdb_api_atualiza_indice(caminho_indice: str, platform_id: int) -> dict:
    """Com credenciais da IGDB/Twitch no ambiente (`IGDB_CLIENT_ID` e
    `IGDB_CLIENT_SECRET` — gratuitas em dev.twitch.tv), baixa TODOS os jogos
    da plataforma direto da API oficial e atualiza o índice — cobertura
    completa, inclusive os jogos que o espelho do GameDB não tem. Sem
    credenciais, usa o índice embarcado como está."""
    import os
    import urllib.request

    idx = json.load(open(caminho_indice, encoding="utf-8"))
    client_id = os.environ.get("IGDB_CLIENT_ID")
    client_secret = os.environ.get("IGDB_CLIENT_SECRET")
    if not (client_id and client_secret):
        return idx

    req = urllib.request.Request(
        "https://id.twitch.tv/oauth2/token?client_id={}&client_secret={}"
        "&grant_type=client_credentials".format(client_id, client_secret),
        data=b"",
    )
    token = json.load(urllib.request.urlopen(req))["access_token"]

    offset = 0
    while True:
        pedido = (
            "fields name,total_rating,rating,aggregated_rating,"
            "first_release_date; where platforms = ({}); limit 500; offset {};"
        ).format(platform_id, offset)
        req = urllib.request.Request(
            "https://api.igdb.com/v4/games",
            data=pedido.encode(),
            headers={
                "Client-ID": client_id,
                "Authorization": "Bearer " + token,
                "Accept": "application/json",
            },
        )
        lote = json.load(urllib.request.urlopen(req))
        if not lote:
            break
        for g in lote:
            nota = g.get("total_rating") or g.get("rating") or g.get("aggregated_rating")
            key = normaliza(g.get("name") or "")
            if key and key not in idx:
                idx[key] = {"nota": round(nota) if nota else None, "ano": None}
        offset += 500
        print(f"  igdb api: {offset} jogos baixados...", file=sys.stderr)

    json.dump(idx, open(caminho_indice, "w"))
    print(f"  índice atualizado pela API: {len(idx)} jogos", file=sys.stderr)
    return idx

def nota_igdb(titulos: list[str], igdb: dict) -> tuple[str, str]:
    """(nota, ano) da IGDB pelo nome — exato primeiro, prefixo depois."""
    for titulo in titulos:
        reg = normaliza(sem_sufixos(des_inverte_artigo(titulo)))
        if reg in igdb:
            return str(igdb[reg]["nota"] or ""), igdb[reg]["ano"] or ""
    for titulo in titulos:
        reg = normaliza(sem_sufixos(des_inverte_artigo(titulo)))
        if len(reg) < 4:
            continue
        for cand, info in igdb.items():
            curto, longo = sorted((len(reg), len(cand)))
            if longo == curto:
                continue
            if (reg.startswith(cand) or cand.startswith(reg)) and curto / longo >= 0.75:
                return str(info["nota"] or ""), info["ano"] or ""
    return "", ""


def main() -> int:
    igdb = igdb_api_atualiza_indice("scripts/dados/igdb_psx.json", 7)
    gamedb = de_gamedb("scripts/dados/gamedb.yaml")
    dat = de_dat("scripts/dados/nointro_psx.dat")
    hltb = de_hltb("scripts/dados/hltb_psx.txt")

    linhas = []
    for serial in sorted(gamedb):
        jogo = gamedb[serial]
        dat_nome = dat.get(serial, {}).get("nome", "")
        titulos = [t for t in (dat_nome, jogo["nome"]) if t]
        nota, ano_igdb = nota_igdb(titulos, igdb)
        nome = dat_nome or jogo["nome"]
        ano = jogo["ano"] or ano_igdb
        editora = jogo["editora"]
        regiao = regiao_de(serial)
        main, extra = hltb.get(serial, ("", ""))
        if not (nome or ano or editora or nota or main or extra):
            continue
        linhas.append(
            f"{serial}\t{nome}\t{ano}\t{editora}\t{regiao}\t{nota}\t{main}\t{extra}"
        )

    print("# gerado por scripts/gen_infos_data.py — não editar à mão")
    print("# infos unificadas por serial: nome\\tano\\teditora\\tregião\\tnota (IGDB)\\tHLTB main\\tHLTB extra (minutos)")
    print(f"# {len(linhas)} seriais de {len(gamedb)}")
    for linha in linhas:
        print(linha)
    return 0


if __name__ == "__main__":
    sys_exit = main()
    raise SystemExit(sys_exit)
