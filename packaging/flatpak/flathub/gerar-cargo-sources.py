#!/usr/bin/env python3
"""Gera o cargo-sources.json para o manifest do Flathub, a partir do Cargo.lock.

O Flathub não permite rede durante o build, então TODAS as dependências do
cargo (incluindo o código C do SDL embutido nos crates sdl3-src,
sdl3-image-src e sdl3-mixer-src, usados pelo feature vendored-sdl) precisam
vir declaradas como fontes do manifest. Este script emite o formato que o
flatpak-builder consome: um "archive" por crate (URL do CDN do crates.io +
sha256 copiado do próprio Cargo.lock), o .cargo-checksum.json de cada vendor
dir e o config.toml que aponta o cargo para o vendor — build 100% offline.

Só usa a stdlib. Saída no mesmo formato do flatpak-cargo-generator.py
oficial. Rode de novo sempre que o Cargo.lock mudar:

    python3 packaging/flatpak/flathub/gerar-cargo-sources.py
"""
import json
import sys
from collections import Counter
from pathlib import Path

AQUI = Path(__file__).resolve().parent
LOCK = AQUI.parent.parent.parent / "Cargo.lock"
SAIDA = AQUI / "cargo-sources.json"
INDEX = "registry+https://github.com/rust-lang/crates.io-index"


def parse_lock(texto):
    """Extrai os blocos [[package]] do Cargo.lock (subconjunto TOML, sem deps)."""
    pacotes, atual = [], None
    for linha in texto.splitlines():
        if linha.startswith("[[package]]"):
            if atual:
                pacotes.append(atual)
            atual = {}
        elif linha.startswith("["):
            atual = None
        elif atual is not None and "=" in linha and not linha[:1].isspace():
            chave, _, valor = linha.partition("=")
            atual[chave.strip()] = valor.strip().strip('"')
    if atual:
        pacotes.append(atual)
    return pacotes


def main():
    pacotes = parse_lock(LOCK.read_text(encoding="utf-8"))

    registry, problemas = [], []
    for p in pacotes:
        origem = p.get("source", "")
        if not origem:
            continue  # path dep do próprio workspace (crates/*) — vem no checkout
        if origem == INDEX:
            if "checksum" in p:
                registry.append((p["name"], p["version"], p["checksum"]))
            else:
                problemas.append(f"{p['name']} {p['version']} sem checksum")
        else:
            problemas.append(f"fonte não suportada ({origem}): {p['name']} {p['version']}")

    if problemas:
        for problema in problemas:
            print(f"ERRO: {problema}", file=sys.stderr)
        sys.exit("cargo-sources.json NÃO gerado — resolva os problemas acima")

    fontes = []
    for nome, versao, checksum in sorted(set(registry)):
        arquivo = f"{nome}-{versao}.crate"
        fontes.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{nome}/{arquivo}",
            "sha256": checksum,
            "dest": f"cargo/vendor/{nome}",
            "dest-filename": arquivo,
        })
        fontes.append({
            "type": "inline",
            "contents": json.dumps({"files": {}, "package": checksum}),
            "dest": f"cargo/vendor/{nome}",
            "dest-filename": ".cargo-checksum.json",
        })
    fontes.append({
        "type": "inline",
        "contents": (
            "[source.crates-io]\n"
            'replace-with = "vendored-sources"\n'
            "\n"
            "[source.vendored-sources]\n"
            'directory = "cargo/vendor"\n'
        ),
        "dest": ".cargo",
        "dest-filename": "config.toml",
    })

    SAIDA.write_text(json.dumps(fontes, indent=4) + "\n", encoding="utf-8")
    dupes = Counter(n for n, _, _ in registry)
    multi = {n: c for n, c in dupes.items() if c > 1}
    print(f"{SAIDA.name}: {len(set(registry))} crates "
          f"({sum(1 for _ in fontes)} fontes; multi-versão: {multi or 'nenhuma'})")


if __name__ == "__main__":
    main()
