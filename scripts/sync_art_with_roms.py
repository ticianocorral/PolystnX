#!/usr/bin/env python3
"""Keep `assets/<kind>/` (the folder the app actually scans for local art)
limited to games currently in `roms/`, moving everything else out to
`assets/<kind>-extra/` — so a big third-party art pack doesn't clutter what
the app looks through, but nothing is lost either. Two-way and idempotent:
a file already in `<kind>-extra/` that matches a ROM added since the last
run moves *back* into `<kind>/` automatically ("mova... tudo sobre os jogos
que eu nao tenho, pra ter facil acesso quando for adicionar mais jogos" —
re-running this after adding a ROM is that "easy access").

`<kind>-extra/` sits next to `<kind>/` under the same `assets/` folder, but
with a name the app's own fixed list of art folders (cover/logo/cartridge/
backcover) never matches — so it's invisible to `find_local_art` at
runtime, just a shelf to pull from by hand (or by re-running this script).

Usage:
    python3 scripts/sync_art_with_roms.py "/path/to/app/root" [kind ...]

`kind` defaults to cover logo cartridge backcover if none are given. Skips
a `kind` whose `assets/<kind>/` folder doesn't exist. Never overwrites an
existing file at the destination on either side.
"""
import os
import sys

APP_ROOT = sys.argv[1]
KINDS = sys.argv[2:] or ["cover", "logo", "cartridge", "backcover"]
ROM_EXTS = (".sfc", ".smc")

roms_dir = os.path.join(APP_ROOT, "roms")
owned = {
    os.path.splitext(f)[0]
    for f in os.listdir(roms_dir)
    if f.lower().endswith(ROM_EXTS)
}
# macOS's default filesystem is case-insensitive, so a file whose name
# differs from a ROM's stem *only* in case still resolves as the same path
# at runtime — but `stem not in owned` below is a case-sensitive Python
# string comparison, and would otherwise move a perfectly working file out
# to `-extra/` (a different directory this time, not just a case variant of
# the same one — that genuinely breaks the match). Rename it to the exact
# casing instead, once, before the real move logic runs.
owned_by_lower = {s.lower(): s for s in owned}

assets_dir = os.path.join(APP_ROOT, "assets")
for kind in KINDS:
    live = os.path.join(assets_dir, kind)
    extra = os.path.join(assets_dir, f"{kind}-extra")
    if not os.path.isdir(live):
        print(f"{kind}: no assets/{kind}/ folder, skipping")
        continue
    os.makedirs(extra, exist_ok=True)

    fixed_case = 0
    moved_out = 0
    for fname in os.listdir(live):
        if fname.startswith("."):
            continue
        stem, ext = os.path.splitext(fname)
        if stem not in owned and stem.lower() in owned_by_lower:
            correct = owned_by_lower[stem.lower()]
            os.rename(
                os.path.join(live, fname), os.path.join(live, f"{correct}{ext}")
            )
            fixed_case += 1
            continue
        if stem not in owned:
            dest = os.path.join(extra, fname)
            if not os.path.exists(dest):
                os.rename(os.path.join(live, fname), dest)
                moved_out += 1

    moved_in = 0
    for fname in os.listdir(extra):
        if fname.startswith("."):
            continue
        stem, ext = os.path.splitext(fname)
        if stem not in owned and stem.lower() in owned_by_lower:
            stem = owned_by_lower[stem.lower()]
            fname_corrected = f"{stem}{ext}"
        else:
            fname_corrected = fname
        if stem in owned:
            dest = os.path.join(live, fname_corrected)
            if not os.path.exists(dest):
                os.rename(os.path.join(extra, fname), dest)
                moved_in += 1

    print(
        f"{kind}: {fixed_case} fixed to the ROM's exact casing, "
        f"{moved_out} moved out to {kind}-extra/, {moved_in} moved back into {kind}/"
    )
