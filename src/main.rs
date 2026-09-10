//! Greek Wordle for the terminal.
//!
//! Switch your keyboard layout to Greek and type. Keys that do not produce a
//! Greek letter are ignored, accents are stripped, and every tile is shown
//! capitalized.

mod game;
mod ui;
mod words;

use std::{
    io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

use game::Game;

fn main() -> io::Result<()> {
    let daily = std::env::args().any(|a| a == "--daily");
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!(
            "greek-wordle - Wordle in Greek\n\n\
             USAGE:\n    greek-wordle [--daily]\n\n\
             OPTIONS:\n    \
             --daily    Everyone gets the same word today, one puzzle per day\n    \
             -h, --help Show this help\n\n\
             Set your keyboard layout to Greek to play. Accents are ignored\n\
             and final sigma counts as sigma."
        );
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, daily);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, daily: bool) -> io::Result<()> {
    let mut game = Game::new(pick_answer(daily));

    loop {
        terminal.draw(|f| ui::draw(f, &game))?;

        // Poll so a resize repaints promptly without spinning the CPU.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Kitty's enhanced protocol also reports key releases; act on presses only.
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
        {
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => return Ok(()),
            KeyCode::Backspace => game.backspace(),
            KeyCode::Enter => {
                if game.is_over() {
                    game = Game::new(pick_answer(daily));
                } else {
                    game.submit();
                }
            }
            // Anything that is not a Greek letter is silently ignored.
            KeyCode::Char(c) => {
                if let Some(letter) = words::normalize_char(c) {
                    game.push(letter);
                }
            }
            _ => {}
        }
    }
}

/// In daily mode the answer is a pure function of the date, so the puzzle is
/// stable all day and advances at local midnight. Otherwise pick at random.
fn pick_answer(daily: bool) -> usize {
    let pool: Vec<usize> = words::answers().collect();
    let seed = if daily {
        days_since_epoch()
    } else {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    };
    pool[(mix(seed) % pool.len() as u64) as usize]
}

fn days_since_epoch() -> u64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let offset = local_utc_offset_secs();
    ((secs + offset).max(0) / 86_400) as u64
}

/// Local offset from UTC, read from the `TZ`-aware `date` the shell already
/// knows about via the standard `%z` format. Falls back to UTC.
fn local_utc_offset_secs() -> i64 {
    let Ok(out) = std::process::Command::new("date").arg("+%z").output() else {
        return 0;
    };
    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim();
    if s.len() < 5 {
        return 0;
    }
    let sign = if s.starts_with('-') { -1 } else { 1 };
    let hours: i64 = s[1..3].parse().unwrap_or(0);
    let mins: i64 = s[3..5].parse().unwrap_or(0);
    sign * (hours * 3600 + mins * 60)
}

/// splitmix64 — enough to scatter a timestamp or day number across the pool
/// without pulling in a random-number dependency.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_answer_is_stable_and_valid() {
        let a = pick_answer(true);
        assert_eq!(a, pick_answer(true));
        assert!(words::WORDS[a].is_answer);
    }

    #[test]
    fn random_answers_are_answer_eligible() {
        for _ in 0..50 {
            assert!(words::WORDS[pick_answer(false)].is_answer);
        }
    }

    #[test]
    fn mix_spreads_consecutive_days() {
        let pool = words::answers().count() as u64;
        let picks: std::collections::HashSet<u64> =
            (0..30).map(|d| mix(20000 + d) % pool).collect();
        assert!(picks.len() > 20, "consecutive days should rarely repeat");
    }
}
