// Copyright (C) 2026 Barry Brown
// SPDX-License-Identifier: GPL-3.0-or-later

//! Lifetime statistics and the one setting that outlives a run, kept in a
//! small text file.
//!
//! The file lives at `$XDG_DATA_HOME/greek-wordle/stats` (falling back to
//! `~/.local/share`) as `key=value` lines, so it is easy to inspect or reset by
//! hand. A missing or damaged file simply starts the counts again from zero.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{game::MAX_GUESSES, words::Pool};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub played: u32,
    pub won: u32,
    /// Wins in a row, broken by any loss.
    pub streak: u32,
    pub best: u32,
    /// Wins by how many guesses they took: `dist[0]` is a first-guess win.
    pub dist: [u32; MAX_GUESSES],
    /// Day number of the last daily puzzle recorded, so replaying today's word
    /// after finishing it does not count twice.
    pub last_daily: Option<u64>,
    /// Which words random games draw answers from. Daily puzzles ignore it so
    /// everyone shares one word.
    pub pool: Pool,
}

impl Stats {
    pub fn path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_DATA_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
        Some(base.join("greek-wordle").join("stats"))
    }

    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    /// Write via a temporary file and rename, so a crash mid-write never leaves
    /// a half-written file behind.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, self.to_text())?;
        fs::rename(tmp, path)
    }

    fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for (key, value) in text.lines().filter_map(|l| l.split_once('=')) {
            let value = value.trim();
            let num = || value.parse().unwrap_or(0);
            match key.trim() {
                "played" => s.played = num(),
                "won" => s.won = num(),
                "streak" => s.streak = num(),
                "best" => s.best = num(),
                "last_daily" => s.last_daily = value.parse().ok(),
                // An unknown pool name falls back to the default rather than
                // failing, so a hand-edited file cannot wedge the game.
                "pool" => s.pool = Pool::from_key(value).unwrap_or_default(),
                "dist" => {
                    for (slot, n) in s.dist.iter_mut().zip(value.split(',')) {
                        *slot = n.trim().parse().unwrap_or(0);
                    }
                }
                _ => {}
            }
        }
        s
    }

    fn to_text(&self) -> String {
        let dist: Vec<String> = self.dist.iter().map(u32::to_string).collect();
        let mut out = format!(
            "played={}\nwon={}\nstreak={}\nbest={}\ndist={}\n",
            self.played,
            self.won,
            self.streak,
            self.best,
            dist.join(",")
        );
        out.push_str(&format!("pool={}\n", self.pool.key()));
        if let Some(day) = self.last_daily {
            out.push_str(&format!("last_daily={day}\n"));
        }
        out
    }

    /// Count a finished game. `daily` is the puzzle's day number in daily mode.
    /// Returns false when this daily puzzle was already recorded.
    pub fn record(&mut self, won: bool, guesses: usize, daily: Option<u64>) -> bool {
        if daily.is_some() && daily == self.last_daily {
            return false;
        }
        if daily.is_some() {
            self.last_daily = daily;
        }
        self.played += 1;
        if won {
            self.won += 1;
            self.streak += 1;
            self.best = self.best.max(self.streak);
            if let Some(slot) = self.dist.get_mut(guesses.wrapping_sub(1)) {
                *slot += 1;
            }
        } else {
            self.streak = 0;
        }
        true
    }

    pub fn win_percent(&self) -> u32 {
        if self.played == 0 {
            0
        } else {
            (self.won as f32 * 100.0 / self.played as f32).round() as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wins_build_a_streak_and_a_loss_breaks_it() {
        let mut s = Stats::default();
        s.record(true, 3, None);
        s.record(true, 4, None);
        s.record(true, 3, None);
        s.record(false, 6, None);
        s.record(true, 1, None);
        assert_eq!((s.played, s.won, s.streak, s.best), (5, 4, 1, 3));
        assert_eq!(s.dist, [1, 0, 2, 1, 0, 0]);
        assert_eq!(s.win_percent(), 80);
    }

    #[test]
    fn a_daily_puzzle_counts_once() {
        let mut s = Stats::default();
        assert!(s.record(true, 2, Some(20_712)));
        assert!(!s.record(true, 2, Some(20_712)), "replay of the same day");
        assert!(s.record(false, 6, Some(20_713)));
        assert!(s.record(true, 5, None), "random games are always counted");
        assert_eq!(s.played, 3);
    }

    #[test]
    fn round_trips_through_text_and_tolerates_junk() {
        let mut s = Stats::default();
        s.record(true, 4, Some(9));
        s.record(false, 6, None);
        assert_eq!(Stats::parse(&s.to_text()), s);

        let damaged = Stats::parse("played=seven\nwon=2\ndist=1,x\ngarbage\n");
        assert_eq!((damaged.played, damaged.won), (0, 2));
        assert_eq!(damaged.dist, [1, 0, 0, 0, 0, 0]);
        assert_eq!(damaged.pool, Pool::default(), "no pool line keeps default");
    }

    #[test]
    fn the_chosen_pool_survives_a_restart() {
        let mut s = Stats {
            pool: Pool::Common,
            ..Default::default()
        };
        s.record(true, 3, None);
        assert_eq!(Stats::parse(&s.to_text()).pool, Pool::Common);
        assert_eq!(
            Stats::parse("pool=made_up\n").pool,
            Pool::default(),
            "an unknown pool name falls back rather than wedging the game"
        );
    }

    #[test]
    fn saves_and_loads_from_disk() {
        let dir = std::env::temp_dir().join(format!("greek-wordle-test-{}", std::process::id()));
        let path = dir.join("nested/stats");
        let mut s = Stats::default();
        s.record(true, 2, None);
        s.save(&path).unwrap();
        assert_eq!(Stats::load(&path), s);
        assert_eq!(Stats::load(&dir.join("missing")), Stats::default());
        fs::remove_dir_all(dir).unwrap();
    }
}
