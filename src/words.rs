// Copyright (C) 2026 Barry Brown
// SPDX-License-Identifier: GPL-3.0-or-later

//! The word list and Greek input normalization.

use unicode_normalization::UnicodeNormalization;

include!(concat!(env!("OUT_DIR"), "/words_generated.rs"));

/// The 24 letters, in the order the game plays them.
pub const ALPHABET: &str = "αβγδεζηθικλμνξοπρστυφχψω";

pub struct Entry {
    /// Bare lowercase form, no diacritics — what a guess is compared against.
    pub word: &'static str,
    /// Accented lemma, shown when the answer is revealed.
    pub display: &'static str,
    pub gloss: &'static str,
    /// A proper noun (Ἰησοῦς, Ἰωάννης, ...), which `Pool::NoProperNames` drops.
    pub is_proper: bool,
    /// 0..1 commonness, ranked within this word's own dictionary source, so it
    /// is comparable across sources where the raw frequencies are not.
    pub rank_pct: f32,
}

/// How common a word must be to reach `Pool::Common`: the upper half of its
/// own source, since the two sources count on scales that do not compare.
const COMMON_MIN_RANK: f32 = 0.5;

/// Which words the game may choose an answer from. Guessing is never
/// restricted — every word in the dictionary is always accepted as a guess.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pool {
    /// Every word in the dictionary. The default, because the point is to
    /// learn them all.
    #[default]
    All,
    /// Everything except proper nouns, which teach vocabulary least.
    NoProperNames,
    /// The commoner half of each source, for an easier round.
    Common,
}

impl Pool {
    /// The order the toggle steps through, widest pool first.
    pub const CYCLE: [Pool; 3] = [Pool::All, Pool::NoProperNames, Pool::Common];

    pub fn next(self) -> Self {
        let i = Self::CYCLE.iter().position(|&p| p == self).unwrap_or(0);
        Self::CYCLE[(i + 1) % Self::CYCLE.len()]
    }

    /// Shown under the title, so it reads as a phrase: "random word · all words".
    pub fn label(self) -> &'static str {
        match self {
            Pool::All => "all words",
            Pool::NoProperNames => "no proper names",
            Pool::Common => "common words",
        }
    }

    /// Stable key for the settings file; unknown keys fall back to the default.
    pub fn key(self) -> &'static str {
        match self {
            Pool::All => "all",
            Pool::NoProperNames => "no_proper_names",
            Pool::Common => "common",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::CYCLE.into_iter().find(|p| p.key() == key)
    }

    fn admits(self, e: &Entry) -> bool {
        match self {
            Pool::All => true,
            Pool::NoProperNames => !e.is_proper,
            Pool::Common => e.rank_pct >= COMMON_MIN_RANK,
        }
    }
}

/// Reduce a typed character to a bare lowercase Greek letter.
///
/// Returns `None` for anything that is not a Greek letter, so Latin keys,
/// digits and punctuation are simply ignored. A Greek keyboard layout emits
/// accented vowels through its dead key (ά, ΐ) and has a dedicated final
/// sigma key (ς); both fold to the bare letter the grid is played in, matching
/// the normalization `tools/build_words.py` applied to the dictionary.
pub fn normalize_char(c: char) -> Option<char> {
    // NFD puts the base letter first: ά -> α + U+0301, ΐ -> ι + U+0308 + U+0301.
    let base = c.nfd().next()?;
    let lower = base.to_lowercase().next()?;
    let folded = if lower == 'ς' { 'σ' } else { lower };
    ALPHABET.contains(folded).then_some(folded)
}

/// Uppercase a bare Greek letter for display. Tiles and the keyboard are
/// always capitalized.
pub fn upper(c: char) -> char {
    c.to_uppercase().next().unwrap_or(c)
}

/// A word is guessable if the dictionary has it, whether or not it can be an answer.
pub fn lookup(word: &str) -> Option<&'static Entry> {
    WORDS
        .binary_search_by(|e| e.word.cmp(word))
        .ok()
        .map(|i| &WORDS[i])
}

/// Indices of the words `pool` may use as an answer. Never empty: `build.rs`
/// checks at compile time that every pool has words in it.
pub fn pool(pool: Pool) -> impl Iterator<Item = usize> {
    (0..WORDS.len()).filter(move |&i| pool.admits(&WORDS[i]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_accents_and_folds_final_sigma() {
        assert_eq!(normalize_char('ά'), Some('α'));
        assert_eq!(normalize_char('ΐ'), Some('ι'));
        assert_eq!(normalize_char('ῳ'), Some('ω')); // iota subscript
        assert_eq!(normalize_char('ς'), Some('σ'));
        assert_eq!(normalize_char('Σ'), Some('σ'));
        assert_eq!(normalize_char('Ά'), Some('α'));
    }

    #[test]
    fn rejects_non_greek() {
        for c in ['a', 'Z', '5', ' ', '.', '-', '\t', '£', '्'] {
            assert_eq!(normalize_char(c), None, "should reject {c:?}");
        }
        // A lone tonos dead key press produces no letter.
        assert_eq!(normalize_char('\u{0384}'), None);
    }

    #[test]
    fn every_alphabet_letter_uppercases() {
        for c in ALPHABET.chars() {
            let u = upper(c);
            assert!(u.is_uppercase(), "{c} did not uppercase");
        }
        assert_eq!(upper('σ'), 'Σ');
    }

    #[test]
    fn word_list_is_sorted_and_normalized() {
        assert!(WORDS.windows(2).all(|w| w[0].word < w[1].word));
        for e in WORDS {
            assert_eq!(e.word.chars().count(), 5);
            assert!(e.word.chars().all(|c| ALPHABET.contains(c)), "{}", e.word);
        }
    }

    #[test]
    fn lookup_finds_words() {
        assert!(lookup("λογοσ").is_some());
        assert!(lookup("ζζζζζ").is_none());
    }

    #[test]
    fn every_pool_has_words_and_none_is_wider_than_all() {
        let all = pool(Pool::All).count();
        assert_eq!(all, WORDS.len(), "the default pool holds the dictionary");
        for p in Pool::CYCLE {
            let n = pool(p).count();
            assert!(n > 100, "{} has only {n} words", p.label());
            assert!(n <= all, "{} is wider than all words", p.label());
        }
    }

    #[test]
    fn pools_drop_exactly_what_they_name() {
        assert!(
            pool(Pool::NoProperNames).all(|i| !WORDS[i].is_proper),
            "a proper noun reached the no-proper-names pool"
        );
        assert!(
            pool(Pool::Common).all(|i| WORDS[i].rank_pct >= COMMON_MIN_RANK),
            "a rare word reached the common pool"
        );
        // Proper nouns exist to be dropped, and some are common, so the two
        // narrower pools are genuinely different rather than one nested set.
        assert!(WORDS.iter().any(|e| e.is_proper));
        assert!(
            WORDS
                .iter()
                .any(|e| e.is_proper && e.rank_pct >= COMMON_MIN_RANK)
        );
    }

    #[test]
    fn the_toggle_cycles_through_every_pool_and_returns() {
        let mut seen = vec![Pool::default()];
        let mut p = Pool::default();
        for _ in 0..Pool::CYCLE.len() {
            p = p.next();
            seen.push(p);
        }
        assert_eq!(p, Pool::default(), "cycling all the way round returns");
        for expected in Pool::CYCLE {
            assert!(seen.contains(&expected), "{} unreachable", expected.label());
        }
    }

    #[test]
    fn pool_keys_round_trip_and_junk_is_rejected() {
        for p in Pool::CYCLE {
            assert_eq!(Pool::from_key(p.key()), Some(p));
        }
        assert_eq!(Pool::from_key("nonsense"), None);
        assert_eq!(Pool::default(), Pool::All, "the default plays every word");
    }
}
