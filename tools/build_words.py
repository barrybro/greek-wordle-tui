#!/usr/bin/env python3
"""Extract 5-letter Greek words from the Pocket Greek dictionary into words.db.

Source : koine_dictionary_android/app/src/main/assets/pocketGreekEntries.sqlite
Output : data/words.db

Normalization: NFD-decompose, drop combining marks (accents, breathings,
iota subscript, diaeresis), lowercase, and fold final sigma to sigma. The
game is played in those 24 bare letters; the accented lemma is kept in
`display` for showing the answer.
"""
import argparse, bisect, collections, re, sqlite3, sys, unicodedata
from pathlib import Path

ALPHABET = "αβγδεζηθικλμνξοπρστυφχψω"
GREEK = set(ALPHABET)
WORD_LEN = 5
ANSWER_MIN_RANK = 0.5  # answers come from the commoner half of each source

DEFAULT_SRC = Path.home() / "Developer/koine_dictionary_android/app/src/main/assets/pocketGreekEntries.sqlite"
DEFAULT_OUT = Path(__file__).resolve().parent.parent / "data/words.db"

SCHEMA = """
CREATE TABLE word (
  id       INTEGER PRIMARY KEY,
  word     TEXT    NOT NULL UNIQUE,  -- 5 bare Greek letters, the playable form
  display  TEXT    NOT NULL,         -- accented lemma, e.g. λόγος
  rank_pct REAL    NOT NULL,         -- 0..1 commonness, ranked within its source
  is_answer INTEGER NOT NULL         -- 1 = eligible as a puzzle answer
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
CREATE INDEX idx_word_answer ON word(is_answer);
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
    ap.add_argument("--src", type=Path, default=DEFAULT_SRC)
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

    n_answers = 0
    for form in sorted(senses):
        entries = senses[form]
        # Representative = the sense that ranks highest for commonness.
        best = max(entries, key=lambda e: rank(e[3], e[4]))
        rank_pct = rank(best[3], best[4])
        # Proper nouns (Ἰησοῦς, Ἰωάννης, ...) stay guessable but never answers.
        is_answer = int(
            rank_pct >= ANSWER_MIN_RANK
            and not any("proper" in e[2].lower() for e in entries)
        )
        n_answers += is_answer
        cur = out.execute(
            "INSERT INTO word (word, display, rank_pct, is_answer) VALUES (?,?,?,?)",
            (form, best[0], round(rank_pct, 4), is_answer),
        )
        out.executemany(
            "INSERT INTO sense (word_id, display, gloss, pos, source, frequency)"
            " VALUES (?,?,?,?,?,?)",
            [(cur.lastrowid, d, g, p, s, f) for d, g, p, s, f in entries],
        )

    out.execute(f"PRAGMA user_version = 1")
    out.commit()
    out.execute("VACUUM")
    out.close()

    print(f"{len(senses)} words ({n_answers} answers) -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
