#!/usr/bin/env python3
"""Convert a TOSEC SNES "Games" datfile into a compact embedded data file
for xperience-domain::tosec (`crates/domain/src/tosec_data.txt`).

TOSEC dats have no per-game <year>/<publisher> XML fields (unlike a
Logiqx/No-Intro dat with those as real child elements) — the TOSEC naming
convention instead encodes them as parenthesised groups right in the game's
own name, e.g. "Chrono Trigger (1995)(Square)(US)[tr de]". This script pulls
those two facts back out by CRC32, once, at generation time, rather than
parsing the naming convention at runtime.

Optionally widens CRC32 coverage using a No-Intro dat (third argument): a
ROM revision/region TOSEC never catalogued under its own CRC (e.g. a "(Rev
1)" dump) still gets TOSEC's year/publisher if some *other* dump of the same
game — grouped by the No-Intro dat's own `id`/`cloneofid` linkage, i.e. "this
is a revision of that" — already has it. Only the No-Intro dat's id/cloneofid
*relationships* are used for this grouping; no name, description or other
No-Intro text is read into the output — same reasoning as skipping TOSEC's
own catalogued name/description (see the module doc in `tosec.rs` and
`THIRD-PARTY-NOTICES.md`).

Usage:
    curl -sL "https://tosecdev.org/downloads/category/<date-slug>?download=<id>" \
        -o tosec-pack.zip
    unzip -o tosec-pack.zip \
        "TOSEC/Nintendo Super Famicom & Super Entertainment System - Games (TOSEC-v<ver>_CM).dat" \
        -d /tmp/tosec

    # No-Intro dat: no direct download link (see DAT-o-MATIC's own
    # click-through disclaimer flow at datomatic.no-intro.org/index.php?
    # page=download&op=dat&s=49 for SNES) — fetch it by hand once and pass
    # its path as the third argument below; omit it to skip the widening
    # step and only use TOSEC.

    python3 scripts/gen_tosec_data.py \
        "/tmp/tosec/TOSEC/Nintendo Super Famicom & Super Entertainment System - Games (TOSEC-v<ver>_CM).dat" \
        crates/domain/src/tosec_data.txt \
        "/tmp/Nintendo - Super Nintendo Entertainment System (<version>).dat"

Format (plain TSV, one CRC32 per line — empty fields when not found):
    CRC32\t<year, or empty>\t<publisher, or empty>

Only the year and publisher are kept — not TOSEC's own catalogued name or
description text, which stays out of the app entirely; the game's *name* the
app shows still comes from the ROM's own header or an optional No-Intro DAT,
same as before. A game whose name doesn't fit the naming convention closely
enough to find a 4-digit year (rare — under 1% of the source set, mostly
undated prototypes) is skipped outright rather than guessing.
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

TOSEC_PATH = sys.argv[1]
OUT_PATH = sys.argv[2]
NOINTRO_PATH = sys.argv[3] if len(sys.argv) > 3 else None

GROUP_RE = re.compile(r"\(([^()]*)\)")
YEAR_RE = re.compile(r"^(\d{4})(-\d{2}(-\d{2})?)?$")


def year_and_publisher(name: str):
    groups = GROUP_RE.findall(name)
    for i, g in enumerate(groups):
        m = YEAR_RE.match(g)
        if not m:
            continue
        year = m.group(1)
        publisher = groups[i + 1].strip() if i + 1 < len(groups) else ""
        return year, publisher
    return "", ""


def clean(s: str) -> str:
    return s.replace("\t", " ").replace("\n", " ").replace("\r", " ").strip()


tree = ET.parse(TOSEC_PATH)
root = tree.getroot()

# Keep the most complete entry seen per CRC32 (some hashes repeat across a
# handful of near-identical catalogue entries) — "most complete" meaning
# "has a publisher too", since every entry that has a year at all almost
# always has a publisher right after it in practice.
best: dict[str, tuple[str, str]] = {}
games = 0
skipped = 0

for game in root.iter("game"):
    name = game.get("name", "")
    year, publisher = year_and_publisher(name)
    if not year and not publisher:
        skipped += 1
        continue
    for rom in game.findall("rom"):
        crc = rom.get("crc")
        if not crc:
            continue
        crc = crc.upper()
        prev = best.get(crc)
        if prev is None or (not prev[1] and publisher):
            best[crc] = (year, publisher)
    games += 1

propagated = 0
if NOINTRO_PATH:
    ni_tree = ET.parse(NOINTRO_PATH)
    ni_root = ni_tree.getroot()
    members_of_family: dict[str, list[str]] = {}
    for game in ni_root.iter("game"):
        family = game.get("cloneofid") or game.get("id")
        if not family:
            continue
        for rom in game.findall("rom"):
            crc = rom.get("crc")
            if not crc:
                continue
            members_of_family.setdefault(family, []).append(crc.upper())

    for members in members_of_family.values():
        source = next((best[c] for c in members if c in best), None)
        if source is None:
            continue
        for c in members:
            if c not in best:
                best[c] = source
                propagated += 1

with open(OUT_PATH, "w", encoding="utf-8") as out:
    for crc in sorted(best):
        year, publisher = best[crc]
        out.write(f"{crc}\t{clean(year)}\t{clean(publisher)}\n")

print(f"games matched (TOSEC): {games}")
print(f"games skipped (no year found): {skipped}")
if NOINTRO_PATH:
    print(f"crc32 entries widened via No-Intro clone families: {propagated}")
print(f"crc32 entries written: {len(best)}")
print(f"output size: {os.path.getsize(OUT_PATH)} bytes")
