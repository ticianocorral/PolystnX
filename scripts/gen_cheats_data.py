#!/usr/bin/env python3
"""Convert the libretro-database PlayStation .cht folder into a compact embedded
data file for xperience-domain::cheats (`crates/domain/src/cheats_data.txt`).

Usage:
    git clone --filter=blob:none --sparse --depth 1 \
        https://github.com/libretro/libretro-database.git /tmp/lrdb
    cd /tmp/lrdb && git sparse-checkout set \
        "cht/Sony - PlayStation"
    python3 scripts/gen_cheats_data.py \
        "/tmp/lrdb/cht/Sony - PlayStation" \
        crates/domain/src/cheats_data.txt

Format (one game per "G" line, its cheats as "C" lines right after):
    G\t<display name, e.g. "Aladdin (USA)">
    C\t<description>\t<code>
    C\t<description>\t<code>
    G\t<next game>
    ...

Skips: cheats with an empty code, and cheats whose code contains a
placeholder character ('X', 'x', '?') — those need a user-picked value our
simple on/off toggle UI has no field for, so shipping them would just be a
checkbox that silently does nothing. Also skips (rather: excludes, at the
MAX_CODE_LEN cutoff below) any code long enough to risk the stack-buffer
overflow in this app's SwanStation core build's retro_cheat_set,
reproduced during development with a 602-char, 67-address combo code —
see cheats.rs's own test `no_cheat_code_is_long_enough_to_crash_the_core`.
"""
import html
import os
import re
import sys

SRC_DIR = sys.argv[1]
OUT_PATH = sys.argv[2]

desc_re = re.compile(r'^cheat(\d+)_desc\s*=\s*"(.*)"\s*$')
code_re = re.compile(r'^cheat(\d+)_code\s*=\s*"(.*)"\s*$')

# See the module docstring above and cheats.rs's crash-regression test —
# keep this comfortably under the 602-char length that reproduced the
# core's stack-buffer overflow, with margin in case the real buffer is
# smaller than that (unknown; never bisected further once 96 held up).
MAX_CODE_LEN = 96


def clean(s: str) -> str:
    # A description occasionally comes HTML-escaped in the source (some
    # entries wrap a quoted phrase as `&quot;...&quot;` instead of a literal
    # `"` — the font this app renders with has no glyph for the entity
    # itself, so a game description would show up as literal "&quot;" on
    # screen otherwise). Unescape first, then keep the data file strictly
    # line-based: no tabs/newlines inside a field.
    s = html.unescape(s)
    return s.replace("\t", " ").replace("\n", " ").replace("\r", " ").strip()


def is_placeholder(code: str) -> bool:
    return any(c in code for c in ("X", "x", "?"))


games = 0
cheats_kept = 0
cheats_skipped = 0

with open(OUT_PATH, "w", encoding="utf-8") as out:
    for fname in sorted(os.listdir(SRC_DIR)):
        if not fname.endswith(".cht"):
            continue
        name = fname[: -len(".cht")]
        path = os.path.join(SRC_DIR, fname)
        descs = {}
        codes = {}
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                line = line.rstrip("\n")
                m = desc_re.match(line)
                if m:
                    descs[m.group(1)] = m.group(2)
                    continue
                m = code_re.match(line)
                if m:
                    codes[m.group(1)] = m.group(2)
                    continue
        rows = []
        for idx, desc in descs.items():
            code = codes.get(idx, "")
            if not code or is_placeholder(code) or len(code) > MAX_CODE_LEN:
                cheats_skipped += 1
                continue
            d = clean(desc)
            c = clean(code)
            if not d or not c:
                cheats_skipped += 1
                continue
            rows.append((int(idx), d, c))
        if not rows:
            continue
        rows.sort(key=lambda r: r[0])
        out.write(f"G\t{clean(name)}\n")
        for _, d, c in rows:
            out.write(f"C\t{d}\t{c}\n")
            cheats_kept += 1
        games += 1

print(f"games written: {games}")
print(f"cheats kept: {cheats_kept}")
print(f"cheats skipped: {cheats_skipped}")
print(f"output size: {os.path.getsize(OUT_PATH)} bytes")
