#!/usr/bin/env python3
"""Rename a folder of local art (cover/logo/cartridge/backcover) to the
No-Intro naming convention, by matching titles against a No-Intro dat — so
the file already sits under the exact filename `find_local_art` (in
`xperience-app`) will look for once a matching ROM gets renamed to its own
No-Intro name, even for a game not in the library yet ("renomear... mesmo
dos jogos que ainda nao tem").

Handles two shapes of "not quite right yet" filename:
- A third-party pack's own numbering, "<Title>[<numeric id>].<ext>" (seen
  in a backcover pack) — the id is discarded, only the title is read.
- Already close to No-Intro but missing a trailing tag the game's real
  entry has, e.g. "Donkey Kong Country (USA).png" sitting next to a ROM
  actually named "Donkey Kong Country (USA) (Rev 2).sfc" (seen in a
  cartridge pack) — anything not already an *exact* name from the dat gets
  its title re-derived and re-matched.

Matching is by normalized title (lowercased, punctuation collapsed to single
spaces, "&" as "and", "The X" / "X, The" reconciled even with a subtitle
still trailing after the moved article) against every <game name="..."> in
the dat, stripped of its own (Region)/(Rev N)/(Language) tags. A title can
match several regional releases in the dat (e.g. "(USA)" and "(Japan)");
this picks one with a fixed preference — (USA) first, then (World), then
(Europe), then whatever sorts first — and skips a (Beta)/(Proto)/(Demo)
release when a plain commercial one is also available, since a prototype is
not what "the game" usually means here.

Usage:
    python3 scripts/rename_art_to_nointro.py \
        "/path/to/nointro.dat" \
        "/path/to/assets/backcover"

Renames in place. Never overwrites an existing destination file (skips and
reports it instead), and never touches a file whose name is already an
exact dat entry — safe to re-run after adding more source files.
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

DAT_PATH = sys.argv[1]
ART_DIR = sys.argv[2]

TAG_RE = re.compile(r"\s*[\(\[][^\)\]]*[\)\]]")
NON_ALNUM_RE = re.compile(r"[^a-z0-9]+")
ID_SUFFIX_RE = re.compile(r"\[\d+\]$")


def base_title(name: str) -> str:
    """Strip every trailing (...)/[...] tag, leaving just the title."""
    return TAG_RE.sub("", name).strip()


def normalize(title: str) -> str:
    t = title.strip()
    # No-Intro puts the article after a comma, sometimes with more of the
    # name (a subtitle) still trailing after it — "Legend of Zelda, The -
    # A Link to the Past", not just a bare "X, The" — so this has to move
    # the article and keep whatever comes after it, not just handle the
    # whole-string-ends-in-", The" case.
    m = re.match(r"^(.+?),\s+(The|A|An)\b(.*)$", t, re.IGNORECASE)
    if m:
        t = f"{m.group(2)} {m.group(1)}{m.group(3)}"
    t = re.sub(r"\s*&\s*", " and ", t)
    t = NON_ALNUM_RE.sub(" ", t.lower()).strip()
    t = re.sub(r"\s+", " ", t)
    return t


def region_rank(name: str) -> tuple:
    is_proto = 1 if re.search(r"\((Beta|Proto|Demo|Sample)", name, re.I) else 0
    order = ["(USA)", "(World)", "(Europe)", "(Japan, USA)", "(Japan)"]
    rank = next((i for i, tag in enumerate(order) if tag in name), len(order))
    return (is_proto, rank, name)


tree = ET.parse(DAT_PATH)
root = tree.getroot()
by_title: dict[str, list[str]] = {}
exact_names: set[str] = set()
for game in root.iter("game"):
    name = game.get("name", "")
    exact_names.add(name)
    key = normalize(base_title(name))
    by_title.setdefault(key, []).append(name)

matched = 0
skipped_exists = 0
already_exact = 0
unmatched: list[str] = []
skipped: list[str] = []

for fname in sorted(os.listdir(ART_DIR)):
    if fname.startswith("."):
        continue  # OS clutter (.DS_Store, ...), not art
    stem, ext = os.path.splitext(fname)
    if stem in exact_names:
        already_exact += 1
        continue  # already the real canonical name — don't second-guess it
    title = ID_SUFFIX_RE.sub("", stem).strip()
    # A source pack sometimes disambiguates same-named games with its own
    # trailing "(Publisher)"-shaped annotation that has nothing to do with
    # No-Intro's own (Region)/(Rev N) tags — strip it the same way either
    # side's tags get stripped, so e.g. "Casper (Absolute Entertainment)"
    # still keys on plain "Casper".
    key = normalize(base_title(title))
    candidates = by_title.get(key)
    if not candidates:
        unmatched.append(fname)
        continue
    best = min(candidates, key=region_rank)
    dest = f"{best}{ext}"
    dest_path = os.path.join(ART_DIR, dest)
    src_path = os.path.join(ART_DIR, fname)
    if os.path.exists(dest_path):
        skipped_exists += 1
        skipped.append(f"{fname} -> {dest} (already exists)")
        continue
    os.rename(src_path, dest_path)
    matched += 1

print(f"already exact: {already_exact}")
print(f"renamed: {matched}")
print(f"skipped (destination already exists): {skipped_exists}")
for s in skipped:
    print(f"  {s}")
print(f"unmatched: {len(unmatched)}")
for f in unmatched:
    print(f"  {f}")
