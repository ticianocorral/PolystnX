# Rascunho do PR (corpo, em inglês) — PolystnX

> **Não abrir ainda**: o app está em release beta (`1.2.2-beta`) e o
> Flathub só aceita versão estável. Aguarde a primeira tag estável e
> atualize tag/commit do manifest e as URLs das screenshots antes.

Título do PR: `Add io.github.ticianocorral.PolystnX`
Base: `new-pr` (nunca `master`)

---

**App description**

PolystnX is a free, personal, non-commercial PlayStation 1 emulator that
puts the whole console on screen: the CRT TV, the case with two real
memory card ports, a game shelf with covers — and the real BIOS boot
animation when you flip Power. Games run through the SwanStation libretro
core as .chd discs (multi-disc via .m3u), memory cards work like the
original hardware (two slots sharing a library of .mcr cards,
hot-swappable), and an offline database built into the binary covers
10,638 PS1 titles (canonical name, year, publisher, IGDB ratings,
HowLongToBeat times).

Homepage: https://github.com/ticianocorral/PolystnX

**Build**

- Built from source from the upstream git tag; fully offline. All cargo
  dependencies — including the SDL3/SDL_image/SDL_mixer C sources embedded
  in the `sdl3-src`, `sdl3-image-src` and `sdl3-mixer-src` crates, built
  via cmake by the `vendored-sdl` feature — are vendored through
  `cargo-sources.json`, generated from the committed `Cargo.lock`
  (238 crates). The toolchain comes from
  `org.freedesktop.Sdk.Extension.rust-stable`.
- The SwanStation libretro core is **not** distributed with the app: it is
  downloaded at runtime from the official libretro buildbot on a
  user-triggered action (first-run setup screen), like any libretro
  frontend's core downloader.
- The PS1 BIOS is **never** distributed or downloaded by the app: users
  provide their own dump in `bios/`, validated by MD5 in the setup screen.
- The metainfo, desktop file and icon are installed by the build from the
  upstream source tree (not copied into this repo).

**Requested permissions**

- `--share=network` — downloading the SwanStation libretro core
  (user-triggered, first run).
- `--filesystem=home` — reading game discs (.chd) and the BIOS from any
  folder in the user's home; the app is a portable-style emulator and
  game libraries live anywhere in `~`. The app's own data (memory cards,
  saves, config) stays inside the sandbox (`XDG_DATA_HOME`).
- `--device=all` — gamepads/joysticks via SDL/hidapi.

**Extra notes**

- License: MIT (project), CC0-1.0 (metainfo). OARS 1.1: nothing
  objectionable.
- The user interface is currently Portuguese (Brazil) only; this is noted
  in the metainfo description.
- The desktop file and metainfo include English and pt-BR localisations.
