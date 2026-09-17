// Copyright (C) 2026 Barry Brown
// SPDX-License-Identifier: GPL-3.0-or-later

//! Game rules: scoring a guess and tracking board state.

use std::collections::HashMap;

use crate::words::{self, WORDS};

pub const WORD_LEN: usize = 5;
pub const MAX_GUESSES: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    Correct,
    Present,
    Absent,
}

impl Mark {
    /// Ranking used to keep the best result a letter has ever earned on the
    /// on-screen keyboard: a green key must never fall back to yellow.
    pub fn rank(self) -> u8 {
        match self {
            Mark::Absent => 0,
            Mark::Present => 1,
            Mark::Correct => 2,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Playing,
    Won,
    Lost,
}

pub struct Guess {
    pub letters: [char; WORD_LEN],
    pub marks: [Mark; WORD_LEN],
}

/// Score a guess against the answer.
///
/// Two passes, so repeated letters behave like real Wordle: exact positions
/// claim their letter first, then remaining letters are matched against what
/// is left over. A letter guessed twice against an answer containing it once
/// yields one hit and one miss.
pub fn score(guess: &[char; WORD_LEN], answer: &[char; WORD_LEN]) -> [Mark; WORD_LEN] {
    let mut marks = [Mark::Absent; WORD_LEN];
    let mut claimed = [false; WORD_LEN];

    for i in 0..WORD_LEN {
        if guess[i] == answer[i] {
            marks[i] = Mark::Correct;
            claimed[i] = true;
        }
    }
    for i in 0..WORD_LEN {
        if marks[i] == Mark::Correct {
            continue;
        }
        for j in 0..WORD_LEN {
            if !claimed[j] && answer[j] == guess[i] {
                marks[i] = Mark::Present;
                claimed[j] = true;
                break;
            }
        }
    }
    marks
}

pub struct Game {
    answer_idx: usize,
    answer: [char; WORD_LEN],
    pub guesses: Vec<Guess>,
    pub input: Vec<char>,
    pub status: Status,
    pub keyboard: HashMap<char, Mark>,
    /// Transient note shown under the board ("not in word list", the reveal).
    pub message: Option<String>,
}

impl Game {
    pub fn new(answer_idx: usize) -> Self {
        let mut answer = ['α'; WORD_LEN];
        for (slot, c) in answer.iter_mut().zip(WORDS[answer_idx].word.chars()) {
            *slot = c;
        }
        Self {
            answer_idx,
            answer,
            guesses: Vec::new(),
            input: Vec::new(),
            status: Status::Playing,
            keyboard: HashMap::new(),
            message: None,
        }
    }

    pub fn entry(&self) -> &'static crate::words::Entry {
        &WORDS[self.answer_idx]
    }

    pub fn is_over(&self) -> bool {
        self.status != Status::Playing
    }

    /// Accept a typed character. Non-Greek input is discarded by the caller's
    /// normalization, so anything arriving here is already a bare letter.
    pub fn push(&mut self, c: char) {
        if self.is_over() || self.input.len() >= WORD_LEN {
            return;
        }
        self.input.push(c);
        self.message = None;
    }

    pub fn backspace(&mut self) {
        if self.is_over() {
            return;
        }
        self.input.pop();
        self.message = None;
    }

    pub fn submit(&mut self) {
        if self.is_over() {
            return;
        }
        if self.input.len() < WORD_LEN {
            self.message = Some("Not enough letters".into());
            return;
        }
        let typed: String = self.input.iter().collect();
        if words::lookup(&typed).is_none() {
            self.message = Some(format!(
                "{} is not in the word list",
                typed.chars().map(words::upper).collect::<String>()
            ));
            return;
        }

        let letters: [char; WORD_LEN] = self.input.clone().try_into().unwrap();
        let marks = score(&letters, &self.answer);

        for (c, m) in letters.iter().zip(marks.iter()) {
            self.keyboard
                .entry(*c)
                .and_modify(|best| {
                    if m.rank() > best.rank() {
                        *best = *m;
                    }
                })
                .or_insert(*m);
        }

        self.guesses.push(Guess { letters, marks });
        self.input.clear();

        if marks.iter().all(|m| *m == Mark::Correct) {
            self.status = Status::Won;
        } else if self.guesses.len() >= MAX_GUESSES {
            self.status = Status::Lost;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mark::*;
    use super::*;
    use crate::words::Pool;

    fn w(s: &str) -> [char; WORD_LEN] {
        s.chars().collect::<Vec<_>>().try_into().unwrap()
    }

    #[test]
    fn all_correct() {
        assert_eq!(score(&w("λογοσ"), &w("λογοσ")), [Correct; 5]);
    }

    #[test]
    fn absent_letters() {
        assert_eq!(score(&w("βββββ"), &w("λογοσ")), [Absent; 5]);
    }

    #[test]
    fn present_but_misplaced() {
        // γ and ρ exist in γραφω but sit elsewhere.
        let marks = score(&w("ργαφω"), &w("γραφω"));
        assert_eq!(marks, [Present, Present, Correct, Correct, Correct]);
    }

    #[test]
    fn duplicate_guess_letter_only_matches_once() {
        // λογοσ has a single λ, so the second λ guessed finds nothing left.
        let marks = score(&w("λλβββ"), &w("λογοσ"));
        assert_eq!(marks[0], Correct);
        assert_eq!(marks[1], Absent);

        // Same rule when neither position is exact: one σ in, one σ out.
        let marks = score(&w("σσβββ"), &w("λογοσ"));
        assert_eq!(marks[0], Present);
        assert_eq!(marks[1], Absent);
    }

    #[test]
    fn exact_match_claims_letter_before_misplaced_one() {
        // λογοσ has two ο. The exact position wins first.
        let marks = score(&w("οοοββ"), &w("λογοσ"));
        assert_eq!(marks[1], Correct);
        assert_eq!(
            marks.iter().filter(|m| **m != Absent).count(),
            2,
            "two ο in the answer means exactly two lit tiles"
        );
    }

    #[test]
    fn rejects_word_not_in_list() {
        let mut g = Game::new(crate::words::pool(Pool::All).next().unwrap());
        for c in "ζζζζζ".chars() {
            g.push(c);
        }
        g.submit();
        assert!(g.guesses.is_empty());
        assert!(g.message.as_ref().unwrap().contains("not in the word list"));
    }

    #[test]
    fn winning_and_losing() {
        let idx = crate::words::pool(Pool::All).next().unwrap();
        let mut g = Game::new(idx);
        for c in WORDS[idx].word.chars() {
            g.push(c);
        }
        g.submit();
        assert_eq!(g.status, Status::Won);

        let mut g = Game::new(idx);
        let wrong = WORDS
            .iter()
            .find(|e| e.word != WORDS[idx].word)
            .unwrap()
            .word;
        for _ in 0..MAX_GUESSES {
            for c in wrong.chars() {
                g.push(c);
            }
            g.submit();
        }
        assert_eq!(g.status, Status::Lost);
    }

    #[test]
    fn input_is_capped_and_ignored_after_game_ends() {
        let mut g = Game::new(crate::words::pool(Pool::All).next().unwrap());
        for c in "λογοσλογοσ".chars() {
            g.push(c);
        }
        assert_eq!(g.input.len(), WORD_LEN);
        g.status = Status::Won;
        g.push('β');
        assert_eq!(g.input.len(), WORD_LEN);
    }

    #[test]
    fn keyboard_never_downgrades_a_correct_letter() {
        let mut g = Game::new(crate::words::pool(Pool::All).next().unwrap());
        g.keyboard.insert('λ', Correct);
        g.answer = w("βββββ");
        g.input = "λογοσ".chars().collect();
        g.submit();
        assert_eq!(g.keyboard[&'λ'], Correct);
    }
}
