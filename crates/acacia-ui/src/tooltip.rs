//! Java's tooltip: lines of text in a dark box with a purple edge, beside the mouse.

use crate::draw::DrawList;
use crate::theme::Theme;

const BACK: [u8; 4] = [0x10, 0x00, 0x10, 0xF0];
const EDGE: [u8; 4] = [0x50, 0x00, 0xFF, 0x50];
const LINE: f32 = 10.0;
/// The box reaches this far past its text.
const PAD: f32 = 4.0;

/// The text's top-left: right of the mouse and above it, moved to its left or up where the
/// screen ends.
fn place([width, height]: [f32; 2], mouse: [f32; 2], size: [f32; 2]) -> [f32; 2] {
    let right = mouse[0] + 12.0;
    let x = if right + width + PAD > size[0] { mouse[0] - 16.0 - width } else { right };
    [x.max(PAD), (mouse[1] - 12.0).min(size[1] - height - PAD).max(PAD)]
}

pub fn draw(list: &mut DrawList, theme: &Theme, lines: &[String], mouse: [f32; 2], size: [f32; 2]) {
    let (Some(font), false) = (&theme.font, lines.is_empty()) else { return };
    let (width, height) = (lines.iter().map(|line| font.width(line)).fold(0.0, f32::max), 8.0 + LINE * (lines.len() - 1) as f32);
    let [x, y] = place([width, height], mouse, size);
    let (white, [l, t, r, b]) = (theme.atlas.white(), [x - PAD, y - PAD, x + width + PAD, y + height + PAD]);
    list.fill(white, [l, t, r, b], BACK);
    for edge in [[l + 1.0, t + 1.0, r - 1.0, t + 2.0], [l + 1.0, b - 2.0, r - 1.0, b - 1.0], [l + 1.0, t + 2.0, l + 2.0, b - 2.0], [r - 2.0, t + 2.0, r - 1.0, b - 2.0]] {
        list.fill(white, edge, EDGE);
    }
    for (i, line) in lines.iter().enumerate() {
        font.draw(list, line, x, y + LINE * i as f32, 0xFFFFFF, 1.0, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tooltip_stays_on_the_screen() {
        let size = [400.0, 240.0];
        assert_eq!(place([50.0, 8.0], [100.0, 100.0], size), [112.0, 88.0]);
        assert_eq!(place([50.0, 8.0], [380.0, 100.0], size), [314.0, 88.0], "left of the mouse at the right edge");
        assert_eq!(place([50.0, 8.0], [100.0, 5.0], size)[1], 4.0);
        assert_eq!(place([50.0, 28.0], [100.0, 239.0], size)[1], 208.0);
    }
}
