# words.db

This file is the word list, committed and built into the binary. It was
extracted once by `tools/build_words.py` from the Pocket Greek dictionary — the
author's own work, covered by this repository's license — and nothing in the
build reads that dictionary, so this repository stands alone. `PRAGMA
user_version` = schema version.

To re-derive it if the dictionary gains words, point the script at a copy:

```sh
./tools/build_words.py --src /path/to/pocketGreekEntries.sqlite
```

## Normalization

Words are stored in the form the game is played in: NFD-decomposed, combining
marks dropped (accents, breathings, iota subscript, diaeresis), lowercased, and
final sigma folded to sigma. So ᾳ→α, ϋ→υ, and λόγος→λογοσ. That leaves exactly
the 24 bare letters, all of which appear in both the word list and the answer
pool. The accented lemma is preserved in `display` for revealing the answer.

## Tables

`word` — 676 rows, one per playable form. All are valid guesses.
  - `word`      the 5-letter normalized form (UNIQUE)
  - `display`   accented lemma of the representative sense, e.g. λόγος
  - `rank_pct`  0..1 commonness, ranked *within the entry's own source*
  - `is_proper` 1 for the 91 proper nouns (ἅννας, ἀπφία, ...)

`sense` — 792 rows, the dictionary entries behind each word (for showing a
definition after the round). Several words carry both a classical and a New
Testament gloss.
  - `word_id`, `display`, `gloss`, `pos`, `source`, `frequency`

## Two things to know

`frequency` is **not comparable across sources**: `GNT-Words` counts New
Testament occurrences (median 4) while `core-greek` uses a classical corpus
scale (median 265). Rank within a source, or just use `rank_pct`.

`rank_pct` and `is_proper` are kept as two independent facts rather than one
precomputed "answerable" flag, because which of them to apply is the player's
choice at runtime: the game plays all 676 by default and can narrow to the 585
that are not proper names, or the 373 in the commoner half of their source. All
676 are always accepted as guesses whichever pool is active. A word is marked
`is_proper` only when *every* sense of it is a proper noun, so a name that
doubles as ordinary vocabulary is not lost along with the names.
