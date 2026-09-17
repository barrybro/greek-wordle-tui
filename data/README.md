# words.db

Built by `tools/build_words.py` from the Pocket Greek dictionary
(`~/AndroidStudioProjects/KoineDictionary/app/src/main/assets/pocketGreekEntries.sqlite`),
the author's own work and covered by this repository's license.
Regenerate with `python3 tools/build_words.py`. `PRAGMA user_version` = schema version.

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
  - `is_answer` 1 for the 344 rows eligible as puzzle answers

`sense` — 792 rows, the dictionary entries behind each word (for showing a
definition after the round). Several words carry both a classical and a New
Testament gloss.
  - `word_id`, `display`, `gloss`, `pos`, `source`, `frequency`

## Two things to know

`frequency` is **not comparable across sources**: `GNT-Words` counts New
Testament occurrences (median 4) while `core-greek` uses a classical corpus
scale (median 265). Rank within a source, or just use `rank_pct`.

`is_answer` excludes proper nouns (ἅννας, ἀπφία, ...) and anything below the
median commonness of its source, so answers stay guessable while the full 676
remain acceptable as input.
