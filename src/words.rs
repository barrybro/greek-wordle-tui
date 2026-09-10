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
    /// Whether this word may be chosen as an answer (see data/README.md).
    pub is_answer: bool,
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

pub fn answers() -> impl Iterator<Item = usize> {
    (0..WORDS.len()).filter(|&i| WORDS[i].is_answer)
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
        assert!(answers().count() > 100);
    }
}
