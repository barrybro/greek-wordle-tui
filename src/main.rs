// Copyright (C) 2026 Barry Brown
// SPDX-License-Identifier: GPL-3.0-or-later

//! Greek Wordle for the terminal.
//!
//! Switch your keyboard layout to Greek and type. Keys that do not produce a
//! Greek letter are ignored, accents are stripped, and every tile is shown
//! capitalized.

mod anim;
mod game;
mod share;
mod stats;
mod theme;
mod ui;
mod words;

use std::{
    io::{self, Write},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crossterm::{
    event::{
        self, DisableFocusChange, EnableFocusChange, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate},
};

use anim::Anim;
use game::{Game, Status};
use stats::Stats;

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
    // Focus reports let the pulsing caret rest while the window is in the
    // background. ratatui's panic hook restores the terminal; ours runs first to
    // undo what ratatui does not know about.
    execute!(io::stdout(), EnableFocusChange)?;
    let restore = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(io::stdout(), EndSynchronizedUpdate, DisableFocusChange);
        restore(info);
    }));

    let result = run(&mut terminal, daily);
    let _ = execute!(io::stdout(), DisableFocusChange);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, daily: bool) -> io::Result<()> {
    let mut app = App {
        game: Game::new(pick_answer(daily)),
        anim: Anim::new(Instant::now(), clock_nanos()),
        stats_path: Stats::path(),
        stats: Stats::default(),
        counted_win: None,
        daily,
    };
    if let Some(path) = &app.stats_path {
        app.stats = Stats::load(path);
    }

    loop {
        let now = Instant::now();
        // Synchronized output: the terminal presents each frame whole, so a
        // tile turning over at 60 fps never tears halfway down the board.
        execute!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
        terminal.draw(|f| {
            ui::draw(
                f,
                &app.game,
                &app.anim,
                &app.stats,
                app.counted_win,
                daily,
                now,
            )
        })?;
        execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;

        // Sleep until the next animation frame is due, or much longer when
        // nothing is moving. A resize or keypress wakes us either way.
        if !event::poll(app.anim.frame(now))? {
            continue;
        }
        // Drain everything queued so fast typing lands in a single frame.
        loop {
            match app.handle(event::read()?) {
                Action::Quit => return Ok(()),
                Action::Copy(text) => {
                    let out = terminal.backend_mut();
                    out.write_all(share::osc52(&text).as_bytes())?;
                    out.flush()?;
                }
                Action::None => {}
            }
            if !event::poll(Duration::ZERO)? {
                break;
            }
        }
    }
}

struct App {
    game: Game,
    anim: Anim,
    stats: Stats,
    /// Where stats persist; `None` when there is no home directory to use.
    stats_path: Option<std::path::PathBuf>,
    /// Guesses taken by the game just won, if it went into the stats; the
    /// panel picks out that bar. A replayed daily puzzle is not counted.
    counted_win: Option<usize>,
    daily: bool,
}

enum Action {
    None,
    Quit,
    /// Put this text on the system clipboard.
    Copy(String),
}

impl App {
    /// Apply one terminal event.
    fn handle(&mut self, event: Event) -> Action {
        let now = Instant::now();
        let (game, anim) = (&mut self.game, &mut self.anim);
        let key = match event {
            Event::FocusGained | Event::FocusLost => {
                anim.focus(event == Event::FocusGained, now);
                return Action::None;
            }
            Event::Key(key) => key,
            _ => return Action::None,
        };
        // Kitty's enhanced protocol also reports key releases; act on presses only.
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return Action::None;
        }
        anim.input(now);

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d'))
        {
            return Action::Quit;
        }

        let settled = game.is_over() && !anim.busy(now);
        let stats_open = anim.stats_open(now);
        match key.code {
            KeyCode::Esc if stats_open => anim.close_stats(),
            KeyCode::Esc => return Action::Quit,
            KeyCode::Tab => anim.toggle_stats(now),
            // Share once the game is decided. The key is the one marked C, which
            // types ψ on a Greek layout.
            KeyCode::Char('c' | 'C' | 'ψ' | 'Ψ') if settled => {
                anim.notify("Result copied to clipboard".into(), now);
                return Action::Copy(share::result(game, self.daily.then(days_since_epoch)));
            }
            KeyCode::Enter if settled => {
                *game = Game::new(pick_answer(self.daily));
                anim.new_game(now);
                self.counted_win = None;
            }
            // The stats panel takes no other keys, and nothing else is taken
            // while tiles are still turning over.
            _ if stats_open || anim.busy(now) => {}
            KeyCode::Backspace => {
                game.backspace();
                anim.erased(game.input.len());
            }
            KeyCode::Enter => {
                let row = game.guesses.len();
                game.submit();
                if game.guesses.len() > row {
                    anim.reveal(row, game.status == Status::Won, now);
                    if game.is_over() {
                        self.finish(now);
                    }
                } else if let Some(msg) = game.message.take() {
                    anim.reject(msg, now);
                }
            }
            // Anything that is not a Greek letter is silently ignored.
            KeyCode::Char(c) => {
                if let Some(letter) = words::normalize_char(c) {
                    let col = game.input.len();
                    game.push(letter);
                    if game.input.len() > col {
                        anim.typed(col, letter, now);
                    }
                }
            }
            _ => {}
        }
        Action::None
    }

    /// Record a finished game and queue the stats panel to follow the reveal.
    fn finish(&mut self, now: Instant) {
        let won = self.game.status == Status::Won;
        let day = self.daily.then(days_since_epoch);
        let guesses = self.game.guesses.len();
        if self.stats.record(won, guesses, day) {
            self.counted_win = won.then_some(guesses);
            let saved = match &self.stats_path {
                Some(path) => self.stats.save(path),
                None => Err(io::Error::other("no home directory")),
            };
            if let Err(e) = saved {
                self.anim.notify(format!("Could not save stats: {e}"), now);
            }
        }
        self.anim.open_stats_after_result();
    }
}

/// In daily mode the answer is a pure function of the date, so the puzzle is
/// stable all day and advances at local midnight. Otherwise pick at random.
fn pick_answer(daily: bool) -> usize {
    let pool: Vec<usize> = words::answers().collect();
    let seed = if daily {
        days_since_epoch()
    } else {
        clock_nanos()
    };
    pool[(mix(seed) % pool.len() as u64) as usize]
}

fn clock_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
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
