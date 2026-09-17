// Copyright (C) 2026 Barry Brown
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rendering: the tile grid, the on-screen keyboard, and the status lines.
//!
//! Tiles and keys are painted cell by cell rather than with widgets, because
//! the look depends on block glyphs lining up exactly. A scored tile is drawn
//! as `▗▄▄▄▖ / ▐ Α ▌ / ▝▀▀▀▘`, which covers precisely the same area as the
//! rounded outline `╭───╮` of an empty tile, so the two sit flush on the grid
//! and a tile can be squashed or lifted half a row at a time as it animates.

use std::{collections::HashMap, time::Instant};

use ratatui::{
    Frame,
    buffer::{Buffer, Cell},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
};

use crate::{
    anim::{Anim, Flip},
    game::{Game, MAX_GUESSES, Mark, Status, WORD_LEN},
    stats::Stats,
    theme::*,
    words::upper,
};

/// A full tile is 5x3 cells on a 6-column pitch; its half-cell margins leave an
/// even gap of one row between rows and two half-columns between tiles.
const TILE_W: u16 = 5;
const TILE_PITCH: u16 = 6;
const TILE_H: u16 = 3;
/// Short terminals get one-row tiles on a 4-column pitch, with a blank row
/// between rows when there is room for one.
const SMALL_W: u16 = 3;
const SMALL_PITCH: u16 = 4;
const SPACED_H: u16 = 2 * MAX_GUESSES as u16 - 1;

/// The Greek keyboard layout, minus the two keys (`;` and `ς`) that do not
/// add a distinct letter — leaving exactly the 24-letter alphabet in the
/// order the keys physically sit under your fingers.
const KEY_ROWS: [&str; 3] = ["ερτυθιοπ", "ασδφγηξκλ", "ζχψωβνμ"];
const KEY_PITCH: u16 = 4;

const TITLE: &str = "ΕΛΛΗΝΙΚΟ WORDLE";

const KEYBOARD_W: u16 = 35; // widest row: 9 keys of 3 columns plus gaps
/// Everything except the board and subtitle: title, message, flat keyboard and
/// help, plus the gaps between them.
const CHROME_H: u16 = 11;
/// Extra rows the keyboard takes when each key gets a lip beneath it.
const KEYCAP_H: u16 = 3;
pub const MIN_W: u16 = KEYBOARD_W + 2;
pub const MIN_H: u16 = MAX_GUESSES as u16 + CHROME_H;
const TALL_H: u16 = TILE_H * MAX_GUESSES as u16 + CHROME_H;

pub fn draw(
    f: &mut Frame,
    game: &Game,
    anim: &Anim,
    stats: &Stats,
    counted_win: Option<usize>,
    daily: bool,
    now: Instant,
) {
    let area = f.area();
    if area.width < MIN_W || area.height < MIN_H {
        let msg = format!(
            "Terminal too small\nNeed {MIN_W}x{MIN_H}, have {}x{}",
            area.width, area.height
        );
        f.render_widget(Paragraph::new(msg).centered(), area);
        return;
    }

    // Spend spare height in order of how much it adds: full tiles, then the
    // subtitle, then keycaps.
    let tall = area.height >= TALL_H;
    let board_h = if tall {
        TILE_H * MAX_GUESSES as u16
    } else if area.height >= SPACED_H + CHROME_H {
        SPACED_H
    } else {
        MAX_GUESSES as u16
    };
    // Row pitch of one-row tiles: 2 leaves a gap, 1 packs them tight.
    let small_rows = if board_h == SPACED_H { 2 } else { 1 };
    let spare = area.height - board_h - CHROME_H;
    let subtitle = spare >= 1;
    let keycaps = tall && spare >= 1 + KEYCAP_H;
    let keyboard_h = if keycaps { 6 } else { 3 };

    // A finished game's definition wraps. Its second line takes the empty row
    // under the message, so nothing moves; any further lines push the keyboard
    // down into spare rows at the bottom. The board stays put either way,
    // because the top margin is fixed from the layout without them.
    let base_h = board_h + CHROME_H + u16::from(subtitle) + keyboard_h - 3;
    let top = (area.height - base_h) / 2;
    let bottom = area.height - base_h - top;
    let gloss_lines = if game.is_over() && !anim.busy(now) {
        wrap(game.entry().gloss, gloss_width(area.width)).len() as u16
    } else {
        0
    };
    let pushed = gloss_lines.saturating_sub(2).min(bottom);
    let gap_used = u16::from(gloss_lines >= 2);

    let [_, title, sub, _, board, _, message, _, keyboard, _, help, _] = Layout::vertical([
        Constraint::Length(top),
        Constraint::Length(1),
        Constraint::Length(subtitle as u16),
        Constraint::Length(1),
        Constraint::Length(board_h),
        Constraint::Length(1),
        Constraint::Length(2 + gap_used + pushed),
        Constraint::Length(1 - gap_used),
        Constraint::Length(keyboard_h),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);

    draw_title(f, title, anim, now);
    if subtitle {
        draw_subtitle(f, sub, daily);
    }

    let board_w = if tall {
        TILE_PITCH * WORD_LEN as u16 - 1
    } else {
        SMALL_PITCH * WORD_LEN as u16 - 1
    };
    let board = centered(board, board_w);
    draw_board(f.buffer_mut(), board, game, anim, tall, small_rows, now);
    draw_message(f, message, game, anim, now);
    draw_keyboard(
        f.buffer_mut(),
        keyboard,
        &revealed_keys(game, anim, now),
        anim,
        keycaps,
        now,
    );
    draw_help(f, help, game, anim, now);

    if game.status == Status::Won {
        let row = game.guesses.len() - 1;
        draw_sparks(f.buffer_mut(), anim, now, |col| {
            if tall {
                let x = board.x + col as u16 * TILE_PITCH + TILE_W / 2;
                (x as f32, (board.y + row as u16 * TILE_H + 1) as f32)
            } else {
                let x = board.x + col as u16 * SMALL_PITCH + SMALL_W / 2;
                (x as f32, (board.y + row as u16 * small_rows) as f32)
            }
        });
    }

    if anim.stats_open(now) {
        draw_stats(f, area, game, anim, stats, counted_win, now);
    }
}

fn centered(area: Rect, width: u16) -> Rect {
    let w = width.min(area.width);
    Rect {
        x: area.x + (area.width - w) / 2,
        width: w,
        ..area
    }
}

fn draw_title(f: &mut Frame, area: Rect, anim: &Anim, now: Instant) {
    let letters = TITLE.chars().filter(|c| *c != ' ').count();
    let glyphs = TITLE.chars().count();
    let mut spans = Vec::new();
    let mut lit = 0;
    for (i, c) in TITLE.chars().enumerate() {
        if c == ' ' {
            spans.push(Span::raw("   "));
            continue;
        }
        let base = mix(TITLE_FROM, TITLE_TO, lit as f32 / (letters - 1) as f32);
        let color = mix(base, WHITE, anim.shimmer(i, glyphs, now) * 0.85);
        lit += 1;
        spans.push(Span::styled(
            c.to_string(),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ));
        if i + 1 < glyphs && TITLE.chars().nth(i + 1) != Some(' ') {
            spans.push(Span::raw(" "));
        }
    }
    f.render_widget(Paragraph::new(Line::from(spans)).centered(), area);
}

fn draw_subtitle(f: &mut Frame, area: Rect, daily: bool) {
    let label = if daily { "daily puzzle" } else { "random word" };
    let rule = Span::styled("──────", Style::new().fg(FAINT));
    f.render_widget(
        Paragraph::new(Line::from(vec![
            rule.clone(),
            Span::styled(format!("  {label}  "), Style::new().fg(DIM)),
            rule,
        ]))
        .centered(),
        area,
    );
}

/// What a single tile is showing this frame.
enum Tile {
    Blank { deal: f32, caret: Option<f32> },
    Typed { letter: char, pop: f32, warn: f32 },
    Turning { letter: char, mark: Mark, p: f32 },
    Scored { letter: char, mark: Mark, lift: u8 },
}

fn tile(game: &Game, anim: &Anim, row: usize, col: usize, now: Instant) -> Tile {
    if let Some(g) = game.guesses.get(row) {
        let (letter, mark) = (g.letters[col], g.marks[col]);
        return match anim.flip(row, col, now) {
            Flip::Pending => Tile::Typed {
                letter,
                pop: 0.0,
                warn: 0.0,
            },
            Flip::Turning(p) => Tile::Turning { letter, mark, p },
            Flip::Done => Tile::Scored {
                letter,
                mark,
                lift: anim.hop(row, col, now),
            },
        };
    }
    let input_row = row == game.guesses.len() && !game.is_over();
    match game.input.get(col) {
        Some(&letter) if input_row => Tile::Typed {
            letter,
            pop: anim.pop(col, now),
            warn: anim.warn(now),
        },
        _ => Tile::Blank {
            deal: anim.deal(row, col, now),
            caret: (input_row && col == game.input.len() && !anim.busy(now))
                .then(|| anim.caret(now)),
        },
    }
}

fn mark_color(mark: Mark) -> Color {
    match mark {
        Mark::Correct => CORRECT,
        Mark::Present => PRESENT,
        Mark::Absent => ABSENT,
    }
}

fn draw_board(
    buf: &mut Buffer,
    area: Rect,
    game: &Game,
    anim: &Anim,
    tall: bool,
    small_rows: u16,
    now: Instant,
) {
    let shake = anim.shake(now);
    for row in 0..MAX_GUESSES {
        let dx = if row == game.guesses.len() { shake } else { 0 };
        for col in 0..WORD_LEN {
            let t = tile(game, anim, row, col, now);
            if tall {
                let x = area
                    .x
                    .saturating_add_signed(col as i16 * TILE_PITCH as i16 + dx);
                paint_tile(buf, x, area.y + row as u16 * TILE_H, t);
            } else {
                let x = area
                    .x
                    .saturating_add_signed(col as i16 * SMALL_PITCH as i16 + dx);
                paint_small_tile(buf, x, area.y + row as u16 * small_rows, t, small_rows == 1);
            }
        }
    }
}

fn paint_tile(buf: &mut Buffer, x: u16, y: u16, tile: Tile) {
    match tile {
        Tile::Blank { deal, caret } => {
            // Deal in: nothing, then a pinpoint, then the outline brightening.
            let base = mix(DEAL, EMPTY, (deal - 0.55) / 0.45);
            let color = match caret {
                Some(c) => mix(base, CARET, 0.25 + 0.6 * c),
                None => base,
            };
            if deal >= 0.55 {
                outline(buf, x, y, color, false, None);
            } else if deal >= 0.25 {
                put(buf, x + 2, y + 1, "·", Style::new().fg(EMPTY));
            }
        }
        Tile::Typed { letter, pop, warn } => {
            let color = mix(mix(TYPED, WARN, warn), WHITE, pop * 0.8);
            outline(buf, x, y, color, pop > 0.45, Some(letter));
        }
        Tile::Turning { letter, mark, p } => {
            // Height in half-rows of a card rotating about its horizontal axis:
            // it narrows to an edge, then opens again showing the scored face.
            let h = (4.0 * (std::f32::consts::PI * p).cos().abs()).round() as i16;
            let letter = (h >= 3).then_some(letter);
            let top = 3 - h / 2;
            if p < 0.5 && h == 4 {
                outline(buf, x, y, TYPED, false, letter);
            } else {
                let fill = if p < 0.5 { TYPED } else { mark_color(mark) };
                solid(buf, x, y, top, top + h, fill, letter);
            }
        }
        Tile::Scored { letter, mark, lift } => {
            let lift = lift as i16;
            solid(
                buf,
                x,
                y,
                1 - lift,
                5 - lift,
                mark_color(mark),
                Some(letter),
            );
        }
    }
}

/// `tight` boards have no gap between rows, so blank slots are drawn as dots:
/// filled ones would run together into solid columns.
fn paint_small_tile(buf: &mut Buffer, x: u16, y: u16, tile: Tile, tight: bool) {
    let face = |buf: &mut Buffer, bg: Color, letter: Option<char>| {
        let text = format!(" {} ", letter.map(upper).unwrap_or(' '));
        put(
            buf,
            x,
            y,
            &text,
            Style::new().fg(TEXT).bg(bg).add_modifier(Modifier::BOLD),
        );
    };
    match tile {
        Tile::Blank { deal, caret } => {
            let base = mix(DEAL, SLOT, (deal - 0.55) / 0.45);
            if tight && deal >= 0.25 {
                let fg = caret.map_or(EMPTY, |c| mix(TYPED, CARET, c));
                put(buf, x + 1, y, "·", Style::new().fg(fg));
            } else if deal >= 0.55 {
                let bg = caret.map_or(base, |c| mix(base, SLOT_TYPED, 0.2 + 0.6 * c));
                face(buf, bg, None);
            } else if deal >= 0.25 {
                put(buf, x + 1, y, "·", Style::new().fg(EMPTY));
            }
        }
        Tile::Typed { letter, pop, warn } => {
            face(
                buf,
                mix(mix(SLOT_TYPED, WARN, warn * 0.6), TYPED, pop),
                Some(letter),
            );
        }
        Tile::Turning { letter, mark, p } => {
            let open = (std::f32::consts::PI * p).cos().abs();
            let fill = if p < 0.5 {
                SLOT_TYPED
            } else {
                mark_color(mark)
            };
            if open > 0.6 {
                face(buf, fill, Some(letter));
            } else if open > 0.15 {
                let edge = if p < 0.5 { "▀▀▀" } else { "▄▄▄" };
                put(buf, x, y, edge, Style::new().fg(fill));
            }
        }
        Tile::Scored { letter, mark, lift } => {
            // No half rows to hop into here, so a win flashes instead.
            face(
                buf,
                mix(mark_color(mark), WHITE, lift as f32 * 0.15),
                Some(letter),
            );
        }
    }
}

/// A rounded (or, mid-pop, heavy) box with an optional letter in the middle.
fn outline(buf: &mut Buffer, x: u16, y: u16, color: Color, heavy: bool, letter: Option<char>) {
    let rows = if heavy {
        ["┏━━━┓", "┃   ┃", "┗━━━┛"]
    } else {
        ["╭───╮", "│   │", "╰───╯"]
    };
    for (dy, row) in rows.iter().enumerate() {
        for (dx, ch) in row.chars().enumerate() {
            if ch != ' ' {
                put_char(
                    buf,
                    x + dx as u16,
                    y + dy as u16,
                    ch,
                    Style::new().fg(color),
                );
            }
        }
    }
    if let Some(c) = letter {
        let style = Style::new().fg(TEXT).add_modifier(Modifier::BOLD);
        put_char(buf, x + 2, y + 1, upper(c), style);
    }
}

/// Fill half-rows `lo..hi` of a 5-wide tile whose top row is `y`, where half-row
/// 0 is the upper half of row 0. The outer columns are only half filled, so the
/// tile keeps a half-cell margin on each side, and the ends of the range can
/// fall mid-row or even outside the tile's own three rows.
fn solid(buf: &mut Buffer, x: u16, y: u16, lo: i16, hi: i16, fill: Color, letter: Option<char>) {
    if hi <= lo {
        return;
    }
    for r in lo.div_euclid(2)..=(hi - 1).div_euclid(2) {
        let Some(ty) = y.checked_add_signed(r) else {
            continue;
        };
        let up = (lo..hi).contains(&(2 * r));
        let down = (lo..hi).contains(&(2 * r + 1));
        for c in 0..TILE_W {
            let Some(cell) = buf.cell_mut((x + c, ty)) else {
                continue;
            };
            match c {
                0 => quadrant(cell, up, down, ['▐', '▝', '▗'], fill),
                c if c == TILE_W - 1 => quadrant(cell, up, down, ['▌', '▘', '▖'], fill),
                _ => half(cell, up, down, fill),
            }
        }
    }
    // The letter sits in whichever row is fully covered nearest the middle.
    let row = ((lo + hi) / 2 - 1).div_euclid(2);
    if let (Some(c), Some(ty)) = (letter, y.checked_add_signed(row)) {
        if lo <= 2 * row && 2 * row + 1 < hi {
            let style = Style::new().fg(TEXT).bg(fill).add_modifier(Modifier::BOLD);
            put_char(buf, x + TILE_W / 2, ty, upper(c), style);
        }
    }
}

/// Paint the top and/or bottom half of a cell, keeping any half a neighbouring
/// tile already painted there so a hopping tile can butt up against another.
fn half(cell: &mut Cell, up: bool, down: bool, fill: Color) {
    let bg = (cell.bg != Color::Reset).then_some(cell.bg);
    let (mut top, mut bottom) = match cell.symbol() {
        "▀" => (Some(cell.fg), bg),
        "▄" => (bg, Some(cell.fg)),
        " " => (bg, bg),
        _ => (None, None),
    };
    if up {
        top = Some(fill);
    }
    if down {
        bottom = Some(fill);
    }
    let (symbol, fg, bg) = match (top, bottom) {
        (Some(t), Some(b)) if t == b => (" ", Color::Reset, t),
        (Some(t), Some(b)) => ("▀", t, b),
        (Some(t), None) => ("▀", t, Color::Reset),
        (None, Some(b)) => ("▄", b, Color::Reset),
        (None, None) => return,
    };
    cell.set_symbol(symbol);
    cell.fg = fg;
    cell.bg = bg;
    cell.modifier = Modifier::empty();
}

/// Paint a half-width edge column: `glyphs` is `[both, upper, lower]`.
fn quadrant(cell: &mut Cell, up: bool, down: bool, glyphs: [char; 3], fill: Color) {
    let glyph = match (up, down) {
        (true, true) => glyphs[0],
        (true, false) => glyphs[1],
        (false, true) => glyphs[2],
        (false, false) => return,
    };
    cell.set_char(glyph);
    cell.fg = fill;
    cell.bg = Color::Reset;
    cell.modifier = Modifier::empty();
}

fn put_char(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch).set_style(style);
    }
}

fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style) {
    for (i, ch) in text.chars().enumerate() {
        put_char(buf, x + i as u16, y, ch, style);
    }
}

/// The best mark each letter has earned, counting only tiles that have
/// finished turning over — a key lights up as its tile lands, not before.
fn revealed_keys(game: &Game, anim: &Anim, now: Instant) -> HashMap<char, Mark> {
    let mut keys: HashMap<char, Mark> = HashMap::new();
    for (row, g) in game.guesses.iter().enumerate() {
        for col in 0..WORD_LEN {
            if anim.flip(row, col, now) != Flip::Done {
                continue;
            }
            let (c, m) = (g.letters[col], g.marks[col]);
            let best = keys.entry(c).or_insert(m);
            if m.rank() > best.rank() {
                *best = m;
            }
        }
    }
    keys
}

fn draw_keyboard(
    buf: &mut Buffer,
    area: Rect,
    marks: &HashMap<char, Mark>,
    anim: &Anim,
    keycaps: bool,
    now: Instant,
) {
    for (i, row) in KEY_ROWS.iter().enumerate() {
        let width = row.chars().count() as u16 * KEY_PITCH - 1;
        let x0 = area.x + area.width.saturating_sub(width) / 2;
        let y = area.y + i as u16 * if keycaps { 2 } else { 1 };
        for (j, c) in row.chars().enumerate() {
            let (fg, bg) = match marks.get(&c) {
                Some(Mark::Correct) => (TEXT, CORRECT),
                Some(Mark::Present) => (TEXT, PRESENT),
                Some(Mark::Absent) => (DIM, ABSENT),
                None => (TEXT, KEY),
            };
            let glow = anim.key_glow(c, now);
            let bg = mix(bg, WHITE, glow * 0.5);
            let x = x0 + j as u16 * KEY_PITCH;
            let face = Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD);
            put(buf, x, y, &format!(" {} ", upper(c)), face);
            if keycaps {
                // A darker lip on the lower edge gives each key some depth; a
                // key just pressed loses it, as if pushed down.
                let lip = mix(bg.darker(), bg, glow);
                put(buf, x, y + 1, "▀▀▀", Style::new().fg(lip));
            }
        }
    }
}

trait Darker {
    fn darker(self) -> Color;
}

impl Darker for Color {
    fn darker(self) -> Color {
        scale(self, 0.55)
    }
}

fn pill(text: &str, fill: Color, ink: Color) -> [Span<'static>; 3] {
    [
        Span::styled("▐", Style::new().fg(fill)),
        Span::styled(
            format!(" {text} "),
            Style::new().fg(ink).bg(fill).add_modifier(Modifier::BOLD),
        ),
        Span::styled("▌", Style::new().fg(fill)),
    ]
}

fn draw_message(f: &mut Frame, area: Rect, game: &Game, anim: &Anim, now: Instant) {
    let lines: Vec<Line> = if let Some(msg) = anim.toast(now) {
        vec![Line::from(pill(msg, TOAST, INK).to_vec())]
    } else if game.is_over() && !anim.busy(now) {
        result_lines(game, gloss_width(area.width), area.height - 1)
    } else if !game.is_over() && game.guesses.is_empty() && game.input.is_empty() {
        // Nothing typed yet: remind the player where the letters come from.
        vec![Line::from(Span::styled(
            "Switch your keyboard layout to Greek",
            Style::new().fg(DIM),
        ))]
    } else {
        vec![]
    };
    f.render_widget(Paragraph::new(lines).centered(), area);
}

/// The verdict and answer, then its definition wrapped to `width` in at most
/// `rows` lines. Shown under the board and again at the top of the stats panel
/// so the panel never hides the answer.
fn result_lines(game: &Game, width: u16, rows: u16) -> Vec<Line<'static>> {
    let entry = game.entry();
    let first: Vec<Span> = if game.status == Status::Won {
        let verdict = match game.guesses.len() {
            1 => "Θαυμάσιο!",
            2 => "Έξοχο!",
            3 => "Πολύ καλά!",
            4 => "Ωραία!",
            5 => "Καλά!",
            _ => "Μόλις που τα κατάφερες!",
        };
        let mut s = pill(verdict, CORRECT, TEXT).to_vec();
        s.push(Span::raw("  "));
        s.push(Span::styled(
            entry.display,
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ));
        s
    } else {
        let mut s = vec![Span::styled("Η λέξη ήταν  ", Style::new().fg(DIM))];
        s.extend(pill(entry.display, PRESENT, INK));
        s
    };
    let mut lines = vec![Line::from(first)];
    lines.extend(
        fit(wrap(entry.gloss, width), rows as usize, width)
            .into_iter()
            .map(|l| Line::from(Span::styled(l, Style::new().fg(DIM)))),
    );
    lines
}

/// Definitions under the board wrap at a comfortable reading width.
fn gloss_width(screen: u16) -> u16 {
    screen.saturating_sub(4).min(60)
}

/// Word-wrap `text` to lines of at most `width` characters. A sense separator
/// `·` stays on the end of a line rather than starting the next one, and a
/// word too long for a line is split.
fn wrap(text: &str, width: u16) -> Vec<String> {
    let width = width.max(1) as usize;
    let mut words: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match words.last_mut() {
            Some(prev) if word == "·" => prev.push_str(" ·"),
            _ => words.push(word.to_string()),
        }
    }
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in words {
        let fits = line.chars().count() + 1 + word.chars().count() <= width;
        if !line.is_empty() && !fits {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
        while line.chars().count() > width {
            let rest: String = line.chars().skip(width).collect();
            lines.push(line.chars().take(width).collect());
            line = rest;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Keep at most `rows` wrapped lines; when some are cut, the last line kept is
/// refilled with the remaining text and ends in an ellipsis.
fn fit(mut lines: Vec<String>, rows: usize, width: u16) -> Vec<String> {
    if lines.len() > rows && rows > 0 {
        let rest = lines.split_off(rows - 1).join(" ");
        lines.push(truncate(&rest, width as usize));
    }
    lines.truncate(rows);
    lines
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

/// Key hints, most important first. Narrow windows drop them from the end
/// instead of letting the line clip.
fn draw_help(f: &mut Frame, area: Rect, game: &Game, anim: &Anim, now: Instant) {
    let keys: &[(&str, &str)] = if game.is_over() && !anim.busy(now) {
        &[
            ("Enter", "new game"),
            ("C", "share"),
            ("Tab", "stats"),
            ("Esc", "quit"),
        ]
    } else {
        &[
            ("Enter", "guess"),
            ("Esc", "quit"),
            ("Tab", "stats"),
            ("Bksp", "delete"),
        ]
    };
    f.render_widget(Paragraph::new(hints(keys, area.width)).centered(), area);
}

fn hints(keys: &[(&str, &str)], width: u16) -> Line<'static> {
    let width_of = |n: usize| -> usize {
        keys[..n]
            .iter()
            .map(|(k, a)| k.chars().count() + 1 + a.chars().count())
            .sum::<usize>()
            + 3 * n.saturating_sub(1)
    };
    let n = (1..=keys.len())
        .rev()
        .find(|&n| width_of(n) <= width as usize)
        .unwrap_or(1);
    let mut spans = Vec::new();
    for (i, (key, action)) in keys[..n].iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(FAINT)));
        }
        spans.push(Span::styled(
            key.to_string(),
            Style::new().fg(SOFT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(format!(" {action}"), Style::new().fg(DIM)));
    }
    Line::from(spans)
}

const STATS_W: u16 = 42;
const STATS_H: u16 = 15;

/// Modal panel: four headline numbers that count up, then the distribution of
/// winning guess counts as bars that grow in, today's win picked out in green.
fn draw_stats(
    f: &mut Frame,
    area: Rect,
    game: &Game,
    anim: &Anim,
    stats: &Stats,
    counted_win: Option<usize>,
    now: Instant,
) {
    let w = STATS_W.min(area.width.saturating_sub(2));
    let result = game.is_over() && !anim.busy(now);
    // Inner width, inside the border and a two-column margin each side.
    let text_w = w.saturating_sub(6);
    let wanted = if result {
        wrap(game.entry().gloss, text_w).len() as u16
    } else {
        0
    };
    // Grow the panel to fit the whole definition. When the window is too short,
    // give up the spacer under the answer, then the one above it, and only
    // then shorten the definition, never below two lines.
    let (mut gloss, mut gap_after, mut top_gap) = (wanted, u16::from(result), 1);
    let height = |g: u16, a: u16, t: u16| STATS_H - 1 + t + u16::from(result) * (1 + g) + a;
    while height(gloss, gap_after, top_gap) > area.height {
        if gap_after > 0 {
            gap_after = 0;
        } else if top_gap > 0 {
            top_gap = 0;
        } else if gloss > 2 {
            gloss -= 1;
        } else {
            break;
        }
    }
    let h = height(gloss, gap_after, top_gap).min(area.height);
    let panel = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(PANEL_EDGE))
        .style(Style::new().bg(PANEL))
        .title(
            Line::from(Span::styled(
                " Statistics ",
                Style::new().fg(SOFT).add_modifier(Modifier::BOLD),
            ))
            .centered(),
        );
    let inner = block.inner(panel);
    f.render_widget(Clear, panel);
    f.render_widget(block, panel);
    let inner = Rect {
        x: inner.x + 2,
        width: inner.width.saturating_sub(4),
        ..inner
    };

    let [_, answer, _, numbers, labels, _, heading, bars, _, footer] = Layout::vertical([
        Constraint::Length(top_gap),
        Constraint::Length(if result { 1 + gloss } else { 0 }),
        Constraint::Length(gap_after),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(MAX_GUESSES as u16),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    if result {
        f.render_widget(
            Paragraph::new(result_lines(game, text_w, gloss)).centered(),
            answer,
        );
    }

    let grow = anim.stats_grow(0, now);
    let headline = [
        (stats.played, "Played"),
        (stats.win_percent(), "Win %"),
        (stats.streak, "Streak"),
        (stats.best, "Best"),
    ];
    let columns = Layout::horizontal([Constraint::Fill(1); 4]);
    for ((value, label), (num, lab)) in headline.iter().zip(
        columns
            .split(numbers)
            .iter()
            .zip(columns.split(labels).iter()),
    ) {
        let shown = (*value as f32 * grow).round() as u32;
        f.render_widget(
            Paragraph::new(Span::styled(
                shown.to_string(),
                Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
            ))
            .centered(),
            *num,
        );
        f.render_widget(
            Paragraph::new(Span::styled(*label, Style::new().fg(DIM))).centered(),
            *lab,
        );
    }

    f.render_widget(
        Paragraph::new(Span::styled(
            "Guess distribution",
            Style::new().fg(SOFT).add_modifier(Modifier::BOLD),
        )),
        heading,
    );
    let today = counted_win.filter(|_| !anim.busy(now));
    let most = stats.dist.iter().copied().max().unwrap_or(0);
    let buf = f.buffer_mut();
    for (i, &count) in stats.dist.iter().enumerate() {
        let y = bars.y + i as u16;
        put(
            buf,
            bars.x,
            y,
            &(i + 1).to_string(),
            Style::new().fg(SOFT).bg(PANEL),
        );
        let fill = if today == Some(i + 1) { CORRECT } else { BAR };
        let label = count.to_string();
        let room = bars.width.saturating_sub(2) as f32;
        let min = label.len() as f32 + 2.0;
        let full_len = if most == 0 {
            min
        } else {
            min + (room - min).max(0.0) * count as f32 / most as f32
        };
        let len = min + (full_len - min) * anim.stats_grow(1 + i, now);
        paint_bar(buf, bars.x + 2, y, len, fill, &label);
    }

    // The panel hides the usual message line, so notes such as "copied" take
    // the place of the key hints while they last.
    let line = match anim.toast(now) {
        Some(msg) => Line::from(pill(msg, TOAST, INK).to_vec()),
        None if game.is_over() && !anim.busy(now) => hints(
            &[("Enter", "new game"), ("C", "share"), ("Tab", "close")],
            footer.width,
        ),
        None => hints(&[("Tab", "close")], footer.width),
    };
    f.render_widget(Paragraph::new(line).centered(), footer);
}

/// A horizontal bar `len` cells long, its ragged end drawn in eighths of a
/// cell so it grows smoothly, with `label` right-aligned inside it.
fn paint_bar(buf: &mut Buffer, x: u16, y: u16, len: f32, fill: Color, label: &str) {
    const EIGHTHS: [&str; 8] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let total = (len.max(0.0) * 8.0).round() as u16;
    let (full, eighths) = (total / 8, (total % 8) as usize);
    put(buf, x, y, &" ".repeat(full as usize), Style::new().bg(fill));
    if eighths > 0 {
        if let Some(cell) = buf.cell_mut((x + full, y)) {
            cell.set_symbol(EIGHTHS[eighths])
                .set_style(Style::new().fg(fill).bg(PANEL));
        }
    }
    let lx = (x + full).saturating_sub(label.len() as u16 + 1);
    put(
        buf,
        lx,
        y,
        label,
        Style::new().fg(TEXT).bg(fill).add_modifier(Modifier::BOLD),
    );
}

/// Sparks from a win arc over the board and pass *behind* everything: they only
/// land on empty cells with empty neighbours, so they never wedge themselves
/// between the letters of the title or the keys.
fn draw_sparks(buf: &mut Buffer, anim: &Anim, now: Instant, origin: impl Fn(usize) -> (f32, f32)) {
    let bounds = buf.area;
    let blank = |buf: &Buffer, x: u16, y: u16| {
        buf.cell((x, y))
            .is_none_or(|c| c.symbol() == " " && c.bg == Color::Reset)
    };
    for spark in &anim.sparks {
        let Some((dx, dy, age)) = spark.at(now) else {
            continue;
        };
        let (ox, oy) = origin(spark.col);
        let (x, y) = ((ox + dx).round(), (oy + dy).round());
        if x < bounds.left() as f32 || y < bounds.top() as f32 {
            continue;
        }
        let (x, y) = (x as u16, y as u16);
        if !blank(buf, x, y) || !blank(buf, x.wrapping_sub(1), y) || !blank(buf, x + 1, y) {
            continue;
        }
        let Some(cell) = buf.cell_mut((x, y)) else {
            continue;
        };
        let glyph = match age {
            a if a < 0.3 => '✦',
            a if a < 0.6 => '✧',
            a if a < 0.85 => '⋆',
            _ => '·',
        };
        let base = [TITLE_FROM, TITLE_TO, WHITE][spark.tint as usize % 3];
        cell.set_char(glyph);
        cell.fg = mix(base, DEAL, age * age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(buf: &Buffer) -> Vec<String> {
        (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    fn tile_buf(height: u16) -> Buffer {
        Buffer::empty(Rect::new(0, 0, TILE_W, height))
    }

    #[test]
    fn scored_tile_matches_the_outline_it_replaces() {
        let mut buf = tile_buf(3);
        solid(&mut buf, 0, 0, 1, 5, CORRECT, Some('λ'));
        assert_eq!(rows(&buf), ["▗▄▄▄▖", "▐ Λ ▌", "▝▀▀▀▘"]);
        assert_eq!(buf[(2, 1)].bg, CORRECT);
    }

    #[test]
    fn flipping_tile_narrows_to_an_edge() {
        let mut buf = tile_buf(3);
        solid(&mut buf, 0, 0, 3, 4, PRESENT, None);
        assert_eq!(rows(&buf), ["     ", "▗▄▄▄▖", "     "]);
    }

    #[test]
    fn hopping_tile_rises_half_a_row_and_merges_with_the_one_above() {
        // Tile above occupies rows 0..3, the hopping tile starts at row 3 and
        // lifts two half-rows, sharing row 2 with the other's bottom edge.
        let mut buf = tile_buf(6);
        solid(&mut buf, 0, 0, 1, 5, ABSENT, Some('α'));
        solid(&mut buf, 0, 3, -1, 3, CORRECT, Some('β'));
        let r = rows(&buf);
        assert_eq!(r[3], "▐ Β ▌", "letter rides up with the tile");
        assert_eq!(r[2], "▗▀▀▀▖");
        assert_eq!((buf[(2, 2)].fg, buf[(2, 2)].bg), (ABSENT, CORRECT));
        assert_eq!(r[5], "     ", "the vacated row is left empty");
    }

    #[test]
    fn hints_drop_from_the_end_to_fit() {
        let keys = [("Enter", "guess"), ("Esc", "quit"), ("Tab", "stats")];
        let text = |w| hints(&keys, w).to_string();
        assert_eq!(text(80), "Enter guess · Esc quit · Tab stats");
        assert_eq!(text(30), "Enter guess · Esc quit");
        assert_eq!(text(3), "Enter guess", "the first hint always shows");
    }

    #[test]
    fn bars_grow_in_eighths_with_the_count_inside() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        paint_bar(&mut buf, 0, 0, 5.5, BAR, "12");
        assert_eq!(rows(&buf)[0], "  12 ▌    ");
        assert_eq!(buf[(4, 0)].bg, BAR);
        assert_eq!((buf[(5, 0)].fg, buf[(5, 0)].bg), (BAR, PANEL));

        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        paint_bar(&mut buf, 0, 0, 3.97, BAR, "0");
        assert_eq!(
            rows(&buf)[0],
            "  0       ",
            "rounds up to a whole cell, no stub"
        );
        assert_eq!(buf[(3, 0)].bg, BAR);
        assert_eq!(buf[(4, 0)].bg, Color::Reset);
    }

    #[test]
    fn the_stats_panel_never_hides_a_lost_answer() {
        use crate::words::WORDS;
        use ratatui::{Terminal, backend::TestBackend};

        let idx = crate::words::answers().next().unwrap();
        let mut game = Game::new(idx);
        let wrong = WORDS.iter().find(|e| e.word != WORDS[idx].word).unwrap();
        for _ in 0..MAX_GUESSES {
            for c in wrong.word.chars() {
                game.push(c);
            }
            game.submit();
        }
        assert_eq!(game.status, Status::Lost);

        let t0 = Instant::now();
        let mut anim = Anim::new(t0, 1);
        anim.reveal(MAX_GUESSES - 1, false, t0);
        anim.open_stats_after_result();
        let later = t0 + std::time::Duration::from_secs(10);
        assert!(anim.stats_open(later));

        for (w, h) in [(80, 40), (MIN_W, MIN_H)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| draw(f, &game, &anim, &Stats::default(), None, false, later))
                .unwrap();
            let screen = rows(term.backend().buffer()).join("\n");
            assert!(
                screen.contains(game.entry().display),
                "answer hidden at {w}x{h}:\n{screen}"
            );
            assert!(screen.contains("Statistics") && screen.contains("Played"));
            assert!(
                screen.contains("new game"),
                "panel footer clipped at {w}x{h}"
            );
        }
    }

    #[test]
    fn definitions_wrap_between_words_and_keep_separators_on_the_line() {
        assert_eq!(wrap("to be strong, able", 60), ["to be strong, able"]);
        assert_eq!(
            wrap("think, suppose · to think; seem", 16),
            ["think, suppose ·", "to think; seem"]
        );
        assert_eq!(wrap("a verylongword", 5), ["a", "veryl", "ongwo", "rd"]);
        assert!(wrap("", 10).is_empty());
    }

    #[test]
    fn fitting_cuts_the_last_line_with_an_ellipsis() {
        let lines = wrap("one two three four five six seven", 9);
        assert_eq!(lines, ["one two", "three", "four five", "six seven"]);
        assert_eq!(fit(lines.clone(), 9, 9), lines, "room for all");
        assert_eq!(fit(lines, 2, 9), ["one two", "three fo…"]);
    }

    /// Win the answer with the longest definition and look at the screen.
    fn won_longest_gloss() -> (Game, Anim, Instant) {
        let idx = crate::words::answers()
            .max_by_key(|&i| crate::words::WORDS[i].gloss.len())
            .unwrap();
        let mut game = Game::new(idx);
        for c in game.entry().word.chars() {
            game.push(c);
        }
        game.submit();
        let t0 = Instant::now();
        let mut anim = Anim::new(t0, 1);
        anim.reveal(0, true, t0);
        (game, anim, t0 + std::time::Duration::from_secs(3))
    }

    fn screen(game: &Game, anim: &Anim, now: Instant, w: u16, h: u16) -> String {
        use ratatui::{Terminal, backend::TestBackend};
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, game, anim, &Stats::default(), None, false, now))
            .unwrap();
        rows(term.backend().buffer()).join("\n")
    }

    #[test]
    fn a_long_definition_fits_under_the_board_or_shows_two_lines() {
        let (game, anim, now) = won_longest_gloss();
        let gloss = game.entry().gloss;
        assert!(gloss.len() > 120, "picked a genuinely long definition");

        let roomy = screen(&game, &anim, now, 100, 40);
        for line in wrap(gloss, gloss_width(100)) {
            assert!(roomy.contains(&line), "missing {line:?} in\n{roomy}");
        }
        assert!(!roomy.contains('…'));

        let tight = screen(&game, &anim, now, MIN_W, MIN_H);
        let lines = wrap(gloss, gloss_width(MIN_W));
        assert!(tight.contains(&lines[0]), "first line in\n{tight}");
        let second: String = lines[1]
            .chars()
            .take(gloss_width(MIN_W) as usize - 1)
            .collect();
        assert!(tight.contains(&second), "second line in\n{tight}");
    }

    #[test]
    fn the_board_does_not_move_when_a_definition_appears() {
        let (game, _, now) = won_longest_gloss();
        let t0 = now - std::time::Duration::from_secs(3);
        let mut busy = Anim::new(t0, 1);
        busy.reveal(0, true, now - std::time::Duration::from_millis(200));
        let settled = Anim::new(t0, 1);
        for (w, h) in [(100, 40), (80, 24), (MIN_W, MIN_H)] {
            let during = screen(&game, &busy, now, w, h);
            let after = screen(&game, &settled, now, w, h);
            let title = |s: &str| s.lines().position(|l| l.contains("W O R D L E"));
            assert_eq!(title(&during), title(&after), "shifted at {w}x{h}");
        }
    }

    #[test]
    fn the_stats_panel_shows_a_long_definition() {
        let (game, mut anim, now) = won_longest_gloss();
        anim.open_stats_after_result();
        let later = now + std::time::Duration::from_secs(10);
        let roomy = screen(&game, &anim, later, 100, 40);
        let text_w = STATS_W - 6;
        for line in wrap(game.entry().gloss, text_w) {
            assert!(roomy.contains(&line), "missing {line:?} in\n{roomy}");
        }
        let tight = screen(&game, &anim, later, MIN_W, MIN_H);
        let lines = wrap(game.entry().gloss, MIN_W - 2 - 6);
        assert!(tight.contains(&lines[0]), "first line in\n{tight}");
        assert!(tight.contains("Played") && tight.contains("new game"));
    }

    #[test]
    fn keys_wait_for_their_tile_to_land() {
        let now = Instant::now();
        let mut game = Game::new(crate::words::answers().next().unwrap());
        for c in game.entry().word.chars() {
            game.push(c);
        }
        game.submit();
        let mut anim = Anim::new(now, 1);
        anim.reveal(0, true, now);
        assert!(revealed_keys(&game, &anim, now).is_empty());
        let later = now + std::time::Duration::from_secs(3);
        assert_eq!(
            revealed_keys(&game, &anim, later).len(),
            game.keyboard.len()
        );
    }
}
