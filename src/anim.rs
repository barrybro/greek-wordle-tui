//! Time-based animation layered over the game.
//!
//! The rules in `game.rs` settle instantly. Everything here only decides how
//! that settled state is *shown* over the following second or so: tiles flip
//! over one by one, a rejected word shakes, a win bounces and throws sparks.
//! Every effect is a pure function of elapsed time, so a slow or dropped frame
//! never knocks anything out of step.

use std::{
    f32::consts::PI,
    time::{Duration, Instant},
};

use crate::game::{MAX_GUESSES, WORD_LEN};

const fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

const FLIP: Duration = ms(440);
const FLIP_STAGGER: Duration = ms(230);
const SHAKE: Duration = ms(440);
const POP: Duration = ms(170);
const KEY_GLOW: Duration = ms(260);
const HOP: Duration = ms(380);
const HOP_STAGGER: Duration = ms(95);
/// Pause between the last tile landing and the first one hopping.
const HOP_DELAY: Duration = ms(80);
const DEAL: Duration = ms(260);
const DEAL_STAGGER: Duration = ms(38);
const SHIMMER: Duration = ms(1200);
const TOAST: Duration = ms(1900);
/// Pause after a game's celebration before the stats panel opens by itself.
const STATS_DELAY: Duration = ms(1300);
const STATS_GROW: Duration = ms(750);
const STATS_STAGGER: Duration = ms(70);
/// Staggered parts of the stats panel: the numbers, then one per bar.
const STATS_PARTS: u32 = 1 + MAX_GUESSES as u32;
const CARET_PERIOD: f32 = 1.4;
/// Stop ambient motion (the caret pulse) after this long without a key.
const IDLE: Duration = Duration::from_secs(30);

const SPARKS: usize = 56;
/// Rows per second squared; rows are about twice as tall as columns are wide.
const GRAVITY: f32 = 20.0;

/// Where one tile of the row being revealed is in its flip.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Flip {
    /// Still waiting its turn; shown as typed.
    Pending,
    /// Turning over, `0.0..1.0`. The scored face shows from halfway.
    Turning(f32),
    Done,
}

pub struct Spark {
    /// Tile of the winning row it bursts from, and its offset from that
    /// tile's centre in cells.
    pub col: usize,
    dx: f32,
    vx: f32,
    vy: f32,
    born: Instant,
    life: f32,
    pub tint: u8,
}

impl Spark {
    /// Offset from its tile's centre and how far through its life it is, or
    /// `None` when it has not been born yet or has burnt out.
    pub fn at(&self, now: Instant) -> Option<(f32, f32, f32)> {
        let t = now.checked_duration_since(self.born)?.as_secs_f32();
        (t < self.life).then(|| {
            (
                self.dx + self.vx * t,
                self.vy * t + 0.5 * GRAVITY * t * t,
                t / self.life,
            )
        })
    }
}

pub struct Anim {
    /// Row whose tiles are flipping, and when the first one started.
    reveal: Option<(usize, Instant)>,
    /// Winning row and when its first tile hops.
    bounce: Option<(usize, Instant)>,
    shake: Option<Instant>,
    /// When each tile of the input row last received a letter.
    typed: [Option<Instant>; WORD_LEN],
    key: Option<(char, Instant)>,
    deal: Instant,
    shimmer: Instant,
    toast: Option<(String, Instant)>,
    /// When the stats panel opened, or will open if that is still ahead.
    stats: Option<Instant>,
    pub sparks: Vec<Spark>,
    epoch: Instant,
    last_input: Instant,
    focused: bool,
    rng: u64,
}

impl Anim {
    pub fn new(now: Instant, seed: u64) -> Self {
        Self {
            reveal: None,
            bounce: None,
            shake: None,
            typed: [None; WORD_LEN],
            key: None,
            deal: now,
            shimmer: now,
            toast: None,
            stats: None,
            sparks: Vec::new(),
            epoch: now,
            last_input: now,
            focused: true,
            rng: seed,
        }
    }

    /// A fresh board: deal the tiles in and run the shimmer across the title.
    pub fn new_game(&mut self, now: Instant) {
        *self = Self {
            epoch: self.epoch,
            focused: self.focused,
            rng: self.rng,
            ..Self::new(now, 0)
        };
    }

    pub fn input(&mut self, now: Instant) {
        self.last_input = now;
    }

    pub fn focus(&mut self, focused: bool, now: Instant) {
        self.focused = focused;
        self.last_input = now;
    }

    pub fn typed(&mut self, col: usize, letter: char, now: Instant) {
        self.typed[col] = Some(now);
        self.key = Some((letter, now));
        self.toast = None;
    }

    pub fn erased(&mut self, col: usize) {
        if let Some(slot) = self.typed.get_mut(col) {
            *slot = None;
        }
        self.toast = None;
    }

    pub fn reject(&mut self, message: String, now: Instant) {
        self.shake = Some(now);
        self.toast = Some((message, now));
    }

    /// A passing note, like `reject` without the shake.
    pub fn notify(&mut self, message: String, now: Instant) {
        self.toast = Some((message, now));
    }

    pub fn stats_open(&self, now: Instant) -> bool {
        self.stats.is_some_and(|t| t <= now)
    }

    /// Open or close the stats panel; opening one that is only scheduled
    /// brings it forward.
    pub fn toggle_stats(&mut self, now: Instant) {
        self.stats = if self.stats_open(now) {
            None
        } else {
            Some(now)
        };
    }

    pub fn close_stats(&mut self) {
        self.stats = None;
    }

    /// Open the stats panel a moment after the reveal and any bounce finish.
    pub fn open_stats_after_result(&mut self) {
        let settled = self.bounce_end().or(self.reveal_end());
        self.stats = settled.map(|t| t + STATS_DELAY);
    }

    /// Eased growth of part `i` of the stats panel: 0 is the row of numbers,
    /// then 1 to 6 the distribution bars, each starting a little later.
    pub fn stats_grow(&self, i: usize, now: Instant) -> f32 {
        let Some(t0) = self.stats else {
            return 0.0;
        };
        match progress(t0 + STATS_STAGGER * i as u32, STATS_GROW, now) {
            Some(p) => 1.0 - (1.0 - p.min(1.0)).powi(3),
            None => 0.0,
        }
    }

    /// Flip `row` over. On a win, queue the bounce and sparks to follow it.
    pub fn reveal(&mut self, row: usize, won: bool, now: Instant) {
        self.reveal = Some((row, now));
        self.typed = [None; WORD_LEN];
        self.toast = None;
        if won {
            let hop = self.reveal_end().unwrap() + HOP_DELAY;
            self.bounce = Some((row, hop));
            self.shimmer = hop;
            self.sparks = (0..SPARKS).map(|_| self.spark(hop)).collect();
        }
    }

    fn spark(&mut self, from: Instant) -> Spark {
        let col = (self.next() * WORD_LEN as f32) as usize % WORD_LEN;
        Spark {
            col,
            dx: (self.next() - 0.5) * 3.0,
            vx: (self.next() - 0.5) * 30.0,
            vy: -(7.0 + self.next() * 10.0),
            // Each tile's sparks leave as that tile hops.
            born: from + HOP_STAGGER * col as u32 + ms((self.next() * 160.0) as u64),
            life: 0.8 + self.next() * 0.9,
            tint: (self.next() * 3.0) as u8,
        }
    }

    /// splitmix64, mapped to `0.0..1.0`.
    fn next(&mut self) -> f32 {
        self.rng = self.rng.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn reveal_end(&self) -> Option<Instant> {
        self.reveal
            .map(|(_, t)| t + FLIP_STAGGER * (WORD_LEN as u32 - 1) + FLIP)
    }

    fn bounce_end(&self) -> Option<Instant> {
        self.bounce
            .map(|(_, t)| t + HOP_STAGGER * (WORD_LEN as u32 - 1) + HOP)
    }

    pub fn flip(&self, row: usize, col: usize, now: Instant) -> Flip {
        let Some((r, t0)) = self.reveal else {
            return Flip::Done;
        };
        if r != row {
            return Flip::Done;
        }
        match progress(t0 + FLIP_STAGGER * col as u32, FLIP, now) {
            None => Flip::Pending,
            Some(p) if p >= 1.0 => Flip::Done,
            Some(p) => Flip::Turning(p),
        }
    }

    /// True while the guess is still being revealed or celebrated: input waits,
    /// and the result is held back so it does not spoil the last tiles.
    pub fn busy(&self, now: Instant) -> bool {
        [self.reveal_end(), self.bounce_end()]
            .into_iter()
            .flatten()
            .any(|end| now < end)
    }

    /// Half-rows a tile of the winning row is lifted by, `0..=2`.
    pub fn hop(&self, row: usize, col: usize, now: Instant) -> u8 {
        match self.bounce {
            Some((r, t0)) if r == row => match progress(t0 + HOP_STAGGER * col as u32, HOP, now) {
                Some(p) if p < 1.0 => (2.0 * (PI * p).sin()).round() as u8,
                _ => 0,
            },
            _ => 0,
        }
    }

    /// How far a tile has faded in after a deal, `0.0..=1.0`.
    pub fn deal(&self, row: usize, col: usize, now: Instant) -> f32 {
        let wave = (row + col) as u32;
        progress(self.deal + DEAL_STAGGER * wave, DEAL, now).map_or(0.0, |p| p.min(1.0))
    }

    /// Horizontal jolt of the input row after a rejected guess, in cells.
    pub fn shake(&self, now: Instant) -> i16 {
        match self.shake.and_then(|t| progress(t, SHAKE, now)) {
            Some(p) if p < 1.0 => (2.4 * (1.0 - p) * (p * PI * 6.0).sin()).round() as i16,
            _ => 0,
        }
    }

    /// Red tint left on the input row by a rejected guess.
    pub fn warn(&self, now: Instant) -> f32 {
        fade(self.shake, SHAKE, now)
    }

    /// How strongly the outline of a tile still flashes from being typed.
    pub fn pop(&self, col: usize, now: Instant) -> f32 {
        fade(self.typed[col], POP, now)
    }

    pub fn key_glow(&self, letter: char, now: Instant) -> f32 {
        match self.key {
            Some((k, t)) if k == letter => fade(Some(t), KEY_GLOW, now),
            _ => 0.0,
        }
    }

    /// Brightness of the caret outline, `0.0..=1.0`. Holds still once the
    /// window loses focus or the player walks away.
    pub fn caret(&self, now: Instant) -> f32 {
        if !self.ambient(now) {
            return 0.6;
        }
        let t = now.duration_since(self.epoch).as_secs_f32();
        0.5 - 0.5 * (2.0 * PI * t / CARET_PERIOD).cos()
    }

    /// Highlight on letter `i` of `n` as a band of light sweeps the title.
    pub fn shimmer(&self, i: usize, n: usize, now: Instant) -> f32 {
        match progress(self.shimmer, SHIMMER, now) {
            Some(p) if p < 1.0 => {
                let head = -4.0 + (n as f32 + 8.0) * p;
                let d = i as f32 - head;
                (-d * d / 3.0).exp()
            }
            _ => 0.0,
        }
    }

    pub fn toast(&self, now: Instant) -> Option<&str> {
        match &self.toast {
            Some((msg, t)) if now < *t + TOAST => Some(msg),
            _ => None,
        }
    }

    fn ambient(&self, now: Instant) -> bool {
        self.focused && now.duration_since(self.last_input) < IDLE
    }

    /// How long the event loop may sleep before the picture next changes.
    pub fn frame(&self, now: Instant) -> Duration {
        let ends = [
            self.reveal_end(),
            self.bounce_end(),
            self.shake.map(|t| t + SHAKE),
            self.key.map(|(_, t)| t + KEY_GLOW),
            self.typed.iter().flatten().max().map(|t| *t + POP),
            Some(self.deal + DEAL_STAGGER * (MAX_GUESSES + WORD_LEN) as u32 + DEAL),
            Some(self.shimmer + SHIMMER),
            self.toast.as_ref().map(|(_, t)| *t + TOAST),
            self.stats
                .map(|t| t + STATS_STAGGER * STATS_PARTS + STATS_GROW),
            self.sparks
                .iter()
                .map(|s| s.born + Duration::from_secs_f32(s.life))
                .max(),
        ];
        if ends.into_iter().flatten().any(|end| now < end) {
            ms(16)
        } else if self.ambient(now) {
            ms(50)
        } else {
            Duration::from_secs(1)
        }
    }
}

/// Fraction of `dur` elapsed since `start`, or `None` before it begins.
fn progress(start: Instant, dur: Duration, now: Instant) -> Option<f32> {
    now.checked_duration_since(start)
        .map(|e| e.as_secs_f32() / dur.as_secs_f32())
}

/// `1.0` at `start`, easing out to `0.0` over `dur`.
fn fade(start: Option<Instant>, dur: Duration, now: Instant) -> f32 {
    match start.and_then(|t| progress(t, dur, now)) {
        Some(p) if p < 1.0 => (1.0 - p) * (1.0 - p),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_flip_left_to_right_then_settle() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 1);
        a.reveal(2, false, t0);

        let mid = t0 + FLIP / 2;
        assert!(matches!(a.flip(2, 0, mid), Flip::Turning(_)));
        assert_eq!(a.flip(2, 4, mid), Flip::Pending);
        assert_eq!(a.flip(1, 4, mid), Flip::Done, "earlier rows stay revealed");
        assert!(a.busy(mid));

        let end = t0 + FLIP_STAGGER * 4 + FLIP;
        assert!((0..WORD_LEN).all(|c| a.flip(2, c, end) == Flip::Done));
        assert!(!a.busy(end));
        assert!(a.sparks.is_empty(), "no celebration without a win");
    }

    #[test]
    fn a_win_bounces_after_the_reveal_and_holds_input() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 7);
        a.reveal(0, true, t0);

        let landed = a.reveal_end().unwrap();
        assert_eq!(a.hop(0, 0, landed), 0);
        assert!(a.busy(landed), "input waits for the bounce too");

        let peak = landed + HOP_DELAY + HOP / 2;
        assert_eq!(a.hop(0, 0, peak), 2);
        assert_eq!(a.hop(1, 0, peak), 0, "only the winning row hops");
        assert!(!a.busy(a.bounce_end().unwrap()));
        assert_eq!(a.sparks.len(), SPARKS);
    }

    #[test]
    fn sparks_arc_up_then_fall_and_burn_out() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 3);
        a.reveal(0, true, t0);
        let s = &a.sparks[0];
        assert!(s.at(s.born - ms(1)).is_none());
        let (_, y, _) = s.at(s.born + ms(100)).unwrap();
        assert!(y < 0.0, "sparks start upward");
        assert!(s.at(s.born + Duration::from_secs(2)).is_none());
    }

    #[test]
    fn shake_and_pops_die_away() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 1);
        a.reject("nope".into(), t0);
        a.typed(3, 'λ', t0);
        assert!((0..SHAKE.as_millis() as u64).any(|m| a.shake(t0 + ms(m)) != 0));
        assert_eq!(a.shake(t0 + SHAKE), 0);
        assert_eq!(a.pop(3, t0), 1.0);
        assert_eq!(a.pop(3, t0 + POP), 0.0);
        assert_eq!(a.toast(t0), None, "typing dismisses the toast");
    }

    #[test]
    fn stats_open_after_the_celebration_and_grow_in() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 1);
        a.reveal(0, true, t0);
        a.open_stats_after_result();
        let settled = a.bounce_end().unwrap();
        assert!(!a.stats_open(settled), "the bounce plays out first");
        let open = settled + STATS_DELAY;
        assert!(a.stats_open(open));
        assert_eq!(a.stats_grow(0, open), 0.0);
        assert!(a.stats_grow(0, open + STATS_GROW / 2) > a.stats_grow(6, open + STATS_GROW / 2));
        assert_eq!(a.stats_grow(6, open + Duration::from_secs(2)), 1.0);

        a.toggle_stats(open);
        assert!(!a.stats_open(open));
        a.toggle_stats(open);
        assert!(a.stats_open(open));
        a.new_game(open);
        assert!(!a.stats_open(open + Duration::from_secs(9)));
    }

    #[test]
    fn toggling_a_scheduled_panel_opens_it_now() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 1);
        a.reveal(5, false, t0);
        a.open_stats_after_result();
        assert!(!a.stats_open(t0));
        a.toggle_stats(t0);
        assert!(a.stats_open(t0));
    }

    #[test]
    fn frames_slow_down_when_nothing_moves() {
        let t0 = Instant::now();
        let mut a = Anim::new(t0, 1);
        assert_eq!(a.frame(t0), ms(16), "the opening deal animates");
        let later = t0 + Duration::from_secs(5);
        assert_eq!(a.frame(later), ms(50), "only the caret pulses");
        a.focus(false, later);
        assert_eq!(a.frame(later), Duration::from_secs(1));
    }
}
