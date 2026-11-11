#!/usr/bin/env bash
# Prepara os assets do core PCSX Rearmed para anexar à release do PolystnX no
# GitHub. Este é o passo de release que fecha o ciclo de segurança do
# download do core: o app NÃO baixa mais o core direto do buildbot do
# libretro (nightly sem versão e sem checksum); ele baixa o core anexado à
# release do próprio app e confere o SHA-256 embutido no binário
# (core_update.rs). Este script puxa um build do buildbot UMA vez, extrai o
# binário, re-empacota com o nome que o app espera e imprime o hash para
# colar nas constantes de core_update.rs antes de taggear.
#
# Fluxo de release:
#   1. packaging/prepare-core-asset.sh            # gera packaging/core-asset/
#   2. colar os hashes impressos em crates/app/src/core_update.rs
#   3. subir os zips de packaging/core-asset/ numa release do GitHub
#      (mesmos nomes impressos abaixo) e apontar CORE_ASSET_TAG, no mesmo
#      arquivo, para o tag dessa release — a URL do core usa esse tag, não
#      a versão do app, então releases seguintes não precisam re-anexar
#   4. commit, tag v<X.Y.Z>, push (o workflow de release confere que os
#      zips existem no CORE_ASSET_TAG antes de publicar)
#
# Usage: prepare-core-asset.sh [target ...]
# Targets default: todos os suportados. Bash 3.2 ok (macOS).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT="$HERE/core-asset"
mkdir -p "$OUT"

ALL_TARGETS=(
  x86_64-unknown-linux-gnu
  aarch64-apple-darwin
  x86_64-apple-darwin
  x86_64-pc-windows-msvc
)

buildbot_url() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "https://buildbot.libretro.com/nightly/linux/x86_64/latest/pcsx_rearmed_libretro.so.zip" ;;
    aarch64-apple-darwin)     echo "https://buildbot.libretro.com/nightly/apple/osx/arm64/latest/pcsx_rearmed_libretro.dylib.zip" ;;
    x86_64-apple-darwin)      echo "https://buildbot.libretro.com/nightly/apple/osx/x86_64/latest/pcsx_rearmed_libretro.dylib.zip" ;;
    x86_64-pc-windows-msvc)   echo "https://buildbot.libretro.com/nightly/windows/x86_64/latest/pcsx_rearmed_libretro.dll.zip" ;;
    *) return 1 ;;
  esac
}

# O binário dentro do zip, nome canônico por plataforma (core_file_name()).
core_file() {
  case "$1" in
    *-linux-*)  echo "pcsx_rearmed_libretro.so" ;;
    *-darwin*)  echo "pcsx_rearmed_libretro.dylib" ;;
    *-windows*) echo "pcsx_rearmed_libretro.dll" ;;
    *) return 1 ;;
  esac
}

if [[ $# -gt 0 ]]; then
  TARGETS=("$@")
else
  TARGETS=("${ALL_TARGETS[@]}")
fi

echo "// Cole em crates/app/src/core_update.rs (const CORE_ASSETS):"
for target in "${TARGETS[@]}"; do
  url="$(buildbot_url "$target")" || { echo "aviso: target desconhecido '$target' — pulando" >&2; continue; }
  core="$(core_file "$target")"
  zip="$OUT/pcsx_rearmed_libretro-$target.zip"
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  curl -fsSL "$url" -o "$work/buildbot.zip"
  (cd "$work" && unzip -o -q buildbot.zip)
  bin="$work/$core"
  if [[ ! -f "$bin" ]]; then
    echo "erro: $core não veio dentro do zip do buildbot ($target)" >&2
    exit 1
  fi
  # Re-zipar com o binário no nível raiz e nome canônico — o app procura a
  # entrada por extensão (igual ao zip do buildbot), então qualquer layout
  # serve; este é o mais simples de auditar na mão.
  (cd "$work" && zip -q -9 "$zip" "$core")
  sha="$(shasum -a 256 "$bin" | cut -d' ' -f1)"
  echo "    (\"$target\", \"$sha\"),"
  echo "#   asset: $zip  (suba na release com este nome)" >&2
done
