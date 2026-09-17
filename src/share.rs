// Copyright (C) 2026 Barry Brown
// SPDX-License-Identifier: GPL-3.0-or-later

//! The spoiler-free result grid, and copying it to the clipboard.
//!
//! Copying uses OSC 52, the escape sequence that asks the terminal itself to
//! set the system clipboard. Ghostty, Kitty and Alacritty all honour it out of
//! the box, and it works over SSH, with no clipboard tool installed locally.

use crate::game::{Game, MAX_GUESSES, Mark, Status};

/// The result as Wordle players share it: a header, then one row of coloured
/// squares per guess, never the letters themselves.
pub fn result(game: &Game, daily: Option<u64>) -> String {
    let score = match game.status {
        Status::Won => game.guesses.len().to_string(),
        _ => "X".into(),
    };
    let mut out = String::from("Ελληνικό Wordle");
    if let Some(day) = daily {
        let (y, m, d) = civil_date(day);
        out.push_str(&format!(" {d:02}/{m:02}/{y}"));
    }
    out.push_str(&format!(" {score}/{MAX_GUESSES}\n"));
    for guess in &game.guesses {
        out.push('\n');
        out.extend(guess.marks.iter().map(|m| match m {
            Mark::Correct => '🟩',
            Mark::Present => '🟨',
            Mark::Absent => '⬛',
        }));
    }
    out
}

/// The escape sequence that puts `text` on the system clipboard.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Year, month and day for a count of days since 1970-01-01
/// (Howard Hinnant's `civil_from_days`).
fn civil_date(days: u64) -> (i64, u32, u32) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + (month <= 2) as i64;
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::{WORDS, answers};

    #[test]
    fn base64_matches_rfc4648_vectors() {
        let cases = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];
        for (plain, encoded) in cases {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
    }

    #[test]
    fn dates_from_day_numbers() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(11_016), (2000, 2, 29));
        assert_eq!(civil_date(20_712), (2026, 9, 16));
    }

    #[test]
    fn result_grid_hides_the_letters() {
        let idx = answers().next().unwrap();
        let mut g = Game::new(idx);
        let wrong = WORDS.iter().find(|e| e.word != WORDS[idx].word).unwrap();
        for word in [wrong.word, WORDS[idx].word] {
            for c in word.chars() {
                g.push(c);
            }
            g.submit();
        }
        let text = result(&g, Some(20_712));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Ελληνικό Wordle 16/09/2026 2/6");
        assert_eq!(lines[1], "");
        assert_eq!(lines[3], "🟩🟩🟩🟩🟩");
        assert_eq!(lines.len(), 4);
        assert!(!text.contains(WORDS[idx].word));
    }

    #[test]
    fn a_loss_scores_x() {
        let mut g = Game::new(answers().next().unwrap());
        g.status = Status::Lost;
        assert!(result(&g, None).starts_with("Ελληνικό Wordle X/6\n"));
    }
}
