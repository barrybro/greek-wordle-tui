#!/usr/bin/env python3
# Copyright (C) 2026 Barry Brown
# SPDX-License-Identifier: GPL-3.0-or-later

"""Extract 5-letter Greek words from the Pocket Greek dictionary into words.db.

This ran once. `data/words.db` is committed and is what the game builds
against, so nothing here is needed to build or play — the script is kept as the
record of how that file was made, and to re-derive it if the dictionary gains
words. It takes `--src` explicitly because the dictionary lives in its own
project, which this repository does not depend on.

    ./tools/build_words.py --src /path/to/pocketGreekEntries.sqlite

Normalization: NFD-decompose, drop combining marks (accents, breathings,
iota subscript, diaeresis), lowercase, and fold final sigma to sigma. The
game is played in those 24 bare letters; the accented lemma is kept in
`display` for showing the answer.

The output records commonness (`rank_pct`) and proper-noun-ness (`is_proper`)
as separate facts, rather than one precomputed "is this answerable" bit: which
of them to apply is the player's choice at runtime, not this script's.
"""
import argparse, bisect, collections, re, sqlite3, sys, unicodedata
from pathlib import Path

ALPHABET = "αβγδεζηθικλμνξοπρστυφχψω"
GREEK = set(ALPHABET)
WORD_LEN = 5
SCHEMA_VERSION = 2  # rank_pct + is_proper, replacing a single is_answer flag

DEFAULT_OUT = Path(__file__).resolve().parent.parent / "data/words.db"

SCHEMA = """
CREATE TABLE word (
  id        INTEGER PRIMARY KEY,
  word      TEXT    NOT NULL UNIQUE,  -- 5 bare Greek letters, the playable form
  display   TEXT    NOT NULL,         -- accented lemma, e.g. λόγος
  rank_pct  REAL    NOT NULL,         -- 0..1 commonness, ranked within its source
  is_proper INTEGER NOT NULL          -- 1 = proper noun (Ἰησοῦς, Ἰωάννης, ...)
);
CREATE TABLE sense (
  id        INTEGER PRIMARY KEY,
  word_id   INTEGER NOT NULL REFERENCES word(id),
  display   TEXT    NOT NULL,
  gloss     TEXT    NOT NULL,
  pos       TEXT    NOT NULL,
  source    TEXT    NOT NULL,
  frequency INTEGER NOT NULL         -- raw count; only comparable within a source
);
CREATE INDEX idx_sense_word ON sense(word_id);
CREATE INDEX idx_word_proper ON word(is_proper);
"""


def normalize(s: str) -> str:
    """Strip diacritics and fold final sigma."""
    d = unicodedata.normalize("NFD", s)
    d = "".join(c for c in d if not unicodedata.combining(c))
    return unicodedata.normalize("NFC", d).lower().replace("ς", "σ")


def headword(raw: str) -> str:
    """First token of the lemma field.

    GNT-Words rows hold a bare lemma, but core-greek rows hold full principal
    parts ("λαμβάνω, λήψομαι, ἔλαβον, ...") or gendered endings ("ξένος ξένου, ὁ").
    """
    return re.split(r"[\s,;]", raw.strip())[0]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument(
        "--src",
        type=Path,
        required=True,
        help="pocketGreekEntries.sqlite from the Pocket Greek dictionary project",
    )
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    args = ap.parse_args()

    if not args.src.exists():
        print(f"source database not found: {args.src}", file=sys.stderr)
        return 1

    src = sqlite3.connect(f"file:{args.src}?mode=ro", uri=True)
    rows = src.execute(
        "SELECT word, gloss, type, sourceName, frequency FROM greekDictionaryEntry"
    ).fetchall()

    # Collect every sense that normalizes to a 5-letter all-Greek form.
    senses = collections.defaultdict(list)
    for raw, gloss, pos, source, freq in rows:
        form = normalize(headword(raw))
        if len(form) == WORD_LEN and all(c in GREEK for c in form):
            senses[form].append((headword(raw), gloss.strip(), pos, source, freq))

    # `frequency` means different things per source (GNT median 4 vs core-greek
    # median 265), so rank each entry against its own source only.
    scale = collections.defaultdict(list)
    for entries in senses.values():
        for *_, source, freq in entries:
            scale[source].append(freq)
    for v in scale.values():
        v.sort()

    def rank(source: str, freq: int) -> float:
        v = scale[source]
        return bisect.bisect_right(v, freq) / len(v)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    if args.out.exists():
        args.out.unlink()
    out = sqlite3.connect(args.out)
    out.executescript(SCHEMA)

    n_proper = 0
    for form in sorted(senses):
        entries = senses[form]
        # Representative = the sense that ranks highest for commonness.
        best = max(entries, key=lambda e: rank(e[3], e[4]))
        rank_pct = rank(best[3], best[4])
        # A word counts as a proper noun only if every sense of it is one, so a
        # word that doubles as ordinary vocabulary is not lost with the names.
        is_proper = int(all("proper" in e[2].lower() for e in entries))
        n_proper += is_proper
        cur = out.execute(
            "INSERT INTO word (word, display, rank_pct, is_proper) VALUES (?,?,?,?)",
            (form, best[0], round(rank_pct, 4), is_proper),
        )
        out.executemany(
            "INSERT INTO sense (word_id, display, gloss, pos, source, frequency)"
            " VALUES (?,?,?,?,?,?)",
            [(cur.lastrowid, d, g, p, s, f) for d, g, p, s, f in entries],
        )

    out.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
    out.commit()
    out.execute("VACUUM")
    out.close()

    print(f"{len(senses)} words ({n_proper} proper nouns) -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
