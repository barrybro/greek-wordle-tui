//! Palette and colour arithmetic.
//!
//! Wordle's dark-mode colours plus the few helpers the animations need to blend
//! them. The game never paints its own background, so a themed or translucent
//! terminal window shows through; the palette assumes that background is dark.

use ratatui::style::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub const CORRECT: Color = rgb(0x53, 0x8d, 0x4e);
pub const PRESENT: Color = rgb(0xb5, 0x9f, 0x3b);
pub const ABSENT: Color = rgb(0x3a, 0x3a, 0x3c);

/// Outline of a tile nobody has typed into.
pub const EMPTY: Color = rgb(0x3a, 0x3a, 0x3c);
/// Outline of a tile holding a letter, and its fill while it flips.
pub const TYPED: Color = rgb(0x62, 0x63, 0x65);
/// Brightest point of the pulsing outline on the next free tile.
pub const CARET: Color = rgb(0x9a, 0x9c, 0x9e);
/// Compact-mode tile backgrounds, where there is no room for an outline.
pub const SLOT: Color = rgb(0x2a, 0x2b, 0x2e);
pub const SLOT_TYPED: Color = rgb(0x48, 0x49, 0x4c);
/// Where a tile starts when the board deals in.
pub const DEAL: Color = rgb(0x1c, 0x1c, 0x1e);
/// Outline tint while a rejected row shakes.
pub const WARN: Color = rgb(0xc2, 0x4e, 0x4e);

pub const KEY: Color = rgb(0x81, 0x83, 0x84);
pub const TEXT: Color = rgb(0xf8, 0xf8, 0xf8);
pub const SOFT: Color = rgb(0xc4, 0xc6, 0xc8);
pub const DIM: Color = rgb(0x86, 0x88, 0x8a);
pub const FAINT: Color = rgb(0x3a, 0x3a, 0x3c);

/// Stats panel ground, its outline, and a distribution bar that is not today's.
pub const PANEL: Color = rgb(0x17, 0x18, 0x1b);
pub const PANEL_EDGE: Color = rgb(0x4a, 0x4b, 0x4e);
pub const BAR: Color = rgb(0x4a, 0x4b, 0x4e);

pub const TOAST: Color = rgb(0xf0, 0xf0, 0xf0);
pub const INK: Color = rgb(0x12, 0x12, 0x13);

/// Title gradient ends: the two scoring colours, lifted to glow as text.
pub const TITLE_FROM: Color = rgb(0x6a, 0xb8, 0x62);
pub const TITLE_TO: Color = rgb(0xd8, 0xbf, 0x4f);

pub const WHITE: Color = rgb(0xff, 0xff, 0xff);

/// Blend `a` toward `b`. Only RGB colours blend; anything else snaps halfway.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    match (a, b) {
        (Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
            let ch = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
            Color::Rgb(ch(ar, br), ch(ag, bg), ch(ab, bb))
        }
        _ if t < 0.5 => a,
        _ => b,
    }
}

/// Darken (`k < 1`) or brighten (`k > 1`) a colour.
pub fn scale(c: Color, k: f32) -> Color {
    match c {
        Color::Rgb(r, g, b) => {
            let ch = |x: u8| (x as f32 * k).round().clamp(0.0, 255.0) as u8;
            Color::Rgb(ch(r), ch(g), ch(b))
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_hits_both_ends_and_clamps() {
        assert_eq!(mix(CORRECT, WHITE, 0.0), CORRECT);
        assert_eq!(mix(CORRECT, WHITE, 1.0), WHITE);
        assert_eq!(mix(CORRECT, WHITE, 7.0), WHITE);
        assert_eq!(mix(rgb(0, 0, 0), rgb(200, 100, 50), 0.5), rgb(100, 50, 25));
    }

    #[test]
    fn scale_saturates() {
        assert_eq!(scale(rgb(200, 10, 0), 2.0), rgb(255, 20, 0));
        assert_eq!(scale(Color::Reset, 0.5), Color::Reset);
    }
}
