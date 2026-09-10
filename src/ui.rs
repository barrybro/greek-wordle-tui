//! Rendering: the tile grid, the on-screen keyboard, and the status lines.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::{
    game::{Game, Mark, Status, MAX_GUESSES, WORD_LEN},
    words::upper,
};

// Wordle's dark palette.
const CORRECT: Color = Color::Rgb(0x53, 0x8d, 0x4e);
const PRESENT: Color = Color::Rgb(0xb5, 0x9f, 0x3b);
const ABSENT: Color = Color::Rgb(0x3a, 0x3a, 0x3c);
const EMPTY: Color = Color::Rgb(0x3a, 0x3a, 0x3c);
const TYPING: Color = Color::Rgb(0x56, 0x57, 0x58);
const SLOT: Color = Color::Rgb(0x2f, 0x31, 0x36);
const UNUSED: Color = Color::Rgb(0x81, 0x83, 0x84);
const TEXT: Color = Color::Rgb(0xf8, 0xf8, 0xf8);
const DIM: Color = Color::Rgb(0x86, 0x88, 0x8a);

const TILE_W: u16 = 5;
/// Tiles are drawn with borders when there is room, and as single coloured
/// cells when the terminal is short — an 80x24 window still gets a full board.
const TILE_H_FULL: u16 = 3;
const TILE_H_COMPACT: u16 = 1;

/// The Greek keyboard layout, minus the two keys (`;` and `ς`) that do not
/// add a distinct letter — leaving exactly the 24-letter alphabet in the
/// order the keys physically sit under your fingers.
const KEY_ROWS: [&str; 3] = ["ερτυθιοπ", "ασδφγηξκλ", "ζχψωβνμ"];

const BOARD_W: u16 = TILE_W * WORD_LEN as u16;
const COMPACT_W: u16 = 4 * WORD_LEN as u16 - 1;
const KEYBOARD_W: u16 = 35; // widest row: 9 keys of 3 columns plus gaps
/// Everything except the board: title, keyboard, message and help, plus gaps.
const CHROME_H: u16 = 11;
pub const MIN_W: u16 = KEYBOARD_W + 2;
pub const MIN_H: u16 = TILE_H_COMPACT * MAX_GUESSES as u16 + CHROME_H;
const FULL_H: u16 = TILE_H_FULL * MAX_GUESSES as u16 + CHROME_H;

fn centered(area: Rect, width: u16) -> Rect {
    let w = width.min(area.width);
    Rect {
        x: area.x + (area.width - w) / 2,
        ..Rect { width: w, ..area }
    }
}

pub fn draw(f: &mut Frame, game: &Game) {
    let area = f.area();
    if area.width < MIN_W || area.height < MIN_H {
        let msg = format!(
            "Terminal too small\nNeed {MIN_W}x{MIN_H}, have {}x{}",
            area.width, area.height
        );
        f.render_widget(Paragraph::new(msg).centered(), area);
        return;
    }

    let tile_h = if area.height >= FULL_H {
        TILE_H_FULL
    } else {
        TILE_H_COMPACT
    };
    let board_h = tile_h * MAX_GUESSES as u16;
    let board_w = if tile_h == TILE_H_FULL { BOARD_W } else { COMPACT_W };

    let [_, title, _, board, _, message, _, keyboard, _, help, _] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(board_h),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .areas(area);

    f.render_widget(
        Paragraph::new(Span::styled(
            "ΕΛΛΗΝΙΚΟ WORDLE",
            Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
        ))
        .centered(),
        title,
    );

    draw_board(f, centered(board, board_w), game, tile_h);
    draw_message(f, message, game);
    draw_keyboard(f, keyboard, game);

    let hint = if game.is_over() {
        "Enter  new game     Esc  quit"
    } else {
        "Enter  guess     Bksp  delete     Esc  quit"
    };
    f.render_widget(
        Paragraph::new(Span::styled(hint, Style::new().fg(DIM))).centered(),
        help,
    );
}

fn draw_board(f: &mut Frame, area: Rect, game: &Game, tile_h: u16) {
    for row in 0..MAX_GUESSES {
        let cells: Vec<(Option<char>, Option<Mark>)> = (0..WORD_LEN)
            .map(|col| match game.guesses.get(row) {
                Some(g) => (Some(g.letters[col]), Some(g.marks[col])),
                // The row currently being typed.
                None if row == game.guesses.len() => (game.input.get(col).copied(), None),
                None => (None, None),
            })
            .collect();

        if tile_h == TILE_H_FULL {
            for (col, (ch, mark)) in cells.into_iter().enumerate() {
                let cell = Rect {
                    x: area.x + col as u16 * TILE_W,
                    y: area.y + row as u16 * TILE_H_FULL,
                    width: TILE_W,
                    height: TILE_H_FULL,
                };
                draw_tile(f, cell, ch, mark);
            }
        } else {
            let spans: Vec<Span> = cells
                .into_iter()
                .flat_map(|(ch, mark)| {
                    let (fg, bg) = match mark {
                        Some(Mark::Correct) => (TEXT, CORRECT),
                        Some(Mark::Present) => (TEXT, PRESENT),
                        Some(Mark::Absent) => (TEXT, ABSENT),
                        None if ch.is_some() => (TEXT, SLOT),
                        None => (SLOT, SLOT),
                    };
                    [
                        Span::styled(
                            format!(" {} ", ch.map(upper).unwrap_or(' ')),
                            Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                    ]
                })
                .collect();
            let line = Rect {
                y: area.y + row as u16,
                height: 1,
                ..area
            };
            f.render_widget(Paragraph::new(Line::from(spans)).centered(), line);
        }
    }
}

fn draw_tile(f: &mut Frame, area: Rect, ch: Option<char>, mark: Option<Mark>) {
    let (bg, border) = match mark {
        Some(Mark::Correct) => (CORRECT, CORRECT),
        Some(Mark::Present) => (PRESENT, PRESENT),
        Some(Mark::Absent) => (ABSENT, ABSENT),
        None if ch.is_some() => (Color::Reset, TYPING),
        None => (Color::Reset, EMPTY),
    };

    let block = Block::bordered().border_style(Style::new().fg(border).bg(bg));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Tiles are always capitalized, whatever the player typed.
    let text = ch.map(|c| upper(c).to_string()).unwrap_or_else(|| " ".into());
    f.render_widget(
        Paragraph::new(Span::styled(
            text,
            Style::new().fg(TEXT).bg(bg).add_modifier(Modifier::BOLD),
        ))
        .centered(),
        inner,
    );
}

fn draw_message(f: &mut Frame, area: Rect, game: &Game) {
    let entry = game.entry();
    let lines: Vec<Line> = match game.status {
        Status::Won | Status::Lost => {
            let (verdict, color) = if game.status == Status::Won {
                (
                    match game.guesses.len() {
                        1 => "Θαυμάσιο!",
                        2 => "Έξοχο!",
                        3 => "Πολύ καλά!",
                        4 => "Ωραία!",
                        5 => "Καλά!",
                        _ => "Μόλις που τα κατάφερες!",
                    },
                    CORRECT,
                )
            } else {
                ("Η λέξη ήταν", PRESENT)
            };
            vec![
                Line::from(vec![
                    Span::styled(verdict, Style::new().fg(color).add_modifier(Modifier::BOLD)),
                    Span::raw("  "),
                    Span::styled(
                        entry.display,
                        Style::new().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(Span::styled(truncate(entry.gloss, 60), Style::new().fg(DIM))),
            ]
        }
        Status::Playing => match &game.message {
            Some(m) => vec![Line::from(Span::styled(
                m.clone(),
                Style::new().fg(PRESENT).add_modifier(Modifier::BOLD),
            ))],
            // Nothing typed yet: remind the player where the letters come from.
            None if game.guesses.is_empty() && game.input.is_empty() => {
                vec![Line::from(Span::styled(
                    "Switch your keyboard layout to Greek",
                    Style::new().fg(DIM),
                ))]
            }
            None => vec![],
        },
    };
    f.render_widget(Paragraph::new(lines).centered(), area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

fn draw_keyboard(f: &mut Frame, area: Rect, game: &Game) {
    for (i, row) in KEY_ROWS.iter().enumerate() {
        let spans: Vec<Span> = row
            .chars()
            .flat_map(|c| {
                let (fg, bg) = match game.keyboard.get(&c) {
                    Some(Mark::Correct) => (TEXT, CORRECT),
                    Some(Mark::Present) => (TEXT, PRESENT),
                    Some(Mark::Absent) => (DIM, ABSENT),
                    None => (Color::Black, UNUSED),
                };
                [
                    Span::styled(
                        format!(" {} ", upper(c)),
                        Style::new().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                ]
            })
            .collect();
        let line = Rect {
            y: area.y + i as u16,
            height: 1,
            ..area
        };
        f.render_widget(Paragraph::new(Line::from(spans)).centered(), line);
    }
}
