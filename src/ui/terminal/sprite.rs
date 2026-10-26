//! Block elements (U+2580–U+259F) and box-drawing lines (U+2500–U+254B, U+2574–U+257F) drawn as
//! rectangles that fill their cell, as Ghostty's sprite font does, not as the font's glyphs: a
//! font's `█` or `┃` stops short of the cell (its line is taller than the glyph, its advance not
//! a whole pixel), so a TUI's logo or border drawn in them shows hairlines between the cells.

/// A part of a cell to fill: pixels from its top left, and how much of the colour it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub alpha: f32,
}
const fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
    Rect { x0, y0, x1, y1, alpha: 1. }
}

/// Whether `ch` is drawn here rather than by the font.
pub fn drawn(ch: char) -> bool {
    matches!(ch, '\u{2580}'..='\u{259f}' | '\u{2574}'..='\u{257f}')
        || matches!(ch, '\u{2500}'..='\u{254b}') && arms(ch).is_some()
}

/// The rectangles `ch` fills in a cell `w` by `h` pixels, if it is one drawn here.
pub fn rects(ch: char, w: f32, h: f32) -> Option<Vec<Rect>> {
    let cp = ch as u32;
    let (mx, my) = (w / 2., h / 2.);
    let eighth = |n: u32, of: f32| of * n as f32 / 8.;
    let quads = |ul: bool, ur: bool, ll: bool, lr: bool| {
        [(ul, rect(0., 0., mx, my)), (ur, rect(mx, 0., w, my)), (ll, rect(0., my, mx, h)), (lr, rect(mx, my, w, h))]
            .into_iter()
            .filter_map(|(on, r)| on.then_some(r))
            .collect()
    };
    let shade = |alpha: f32| vec![Rect { alpha, ..rect(0., 0., w, h) }];
    Some(match cp {
        0x2580 => vec![rect(0., 0., w, my)],
        0x2581..=0x2587 => vec![rect(0., h - eighth(cp - 0x2580, h), w, h)],
        0x2588 => vec![rect(0., 0., w, h)],
        0x2589..=0x258f => vec![rect(0., 0., eighth(0x2590 - cp, w), h)],
        0x2590 => vec![rect(mx, 0., w, h)],
        0x2591 => shade(0.25),
        0x2592 => shade(0.5),
        0x2593 => shade(0.75),
        0x2594 => vec![rect(0., 0., w, eighth(1, h))],
        0x2595 => vec![rect(w - eighth(1, w), 0., w, h)],
        0x2596 => quads(false, false, true, false),
        0x2597 => quads(false, false, false, true),
        0x2598 => quads(true, false, false, false),
        0x2599 => quads(true, false, true, true),
        0x259a => quads(true, false, false, true),
        0x259b => quads(true, true, true, false),
        0x259c => quads(true, true, false, true),
        0x259d => quads(false, true, false, false),
        0x259e => quads(false, true, true, false),
        0x259f => quads(false, true, true, true),
        _ => lines(arms(ch)?, w, h),
    })
}

/// A box-drawing character's arms, `[up, right, down, left]`: 0 none, 1 light, 2 heavy. The
/// dashed lines (U+2504–U+250B) are left to the font.
fn arms(ch: char) -> Option<[u8; 4]> {
    // U+2500–U+254B, in order; `X` the dashed ones.
    const X: [u8; 4] = [9; 4];
    #[rustfmt::skip]
    const BOX: [[u8; 4]; 76] = [
        [0,1,0,1], [0,2,0,2], [1,0,1,0], [2,0,2,0], X, X, X, X, X, X, X, X,
        [0,1,1,0], [0,2,1,0], [0,1,2,0], [0,2,2,0],
        [0,0,1,1], [0,0,1,2], [0,0,2,1], [0,0,2,2],
        [1,1,0,0], [1,2,0,0], [2,1,0,0], [2,2,0,0],
        [1,0,0,1], [1,0,0,2], [2,0,0,1], [2,0,0,2],
        [1,1,1,0], [1,2,1,0], [2,1,1,0], [1,1,2,0], [2,1,2,0], [2,2,1,0], [1,2,2,0], [2,2,2,0],
        [1,0,1,1], [1,0,1,2], [2,0,1,1], [1,0,2,1], [2,0,2,1], [2,0,1,2], [1,0,2,2], [2,0,2,2],
        [0,1,1,1], [0,1,1,2], [0,2,1,1], [0,2,1,2], [0,1,2,1], [0,1,2,2], [0,2,2,1], [0,2,2,2],
        [1,1,0,1], [1,1,0,2], [1,2,0,1], [1,2,0,2], [2,1,0,1], [2,1,0,2], [2,2,0,1], [2,2,0,2],
        [1,1,1,1], [1,1,1,2], [1,2,1,1], [1,2,1,2], [2,1,1,1], [1,1,2,1], [2,1,2,1],
        [2,1,1,2], [2,2,1,1], [1,1,2,2], [1,2,2,1], [2,2,1,2], [1,2,2,2], [2,1,2,2], [2,2,2,1],
        [2,2,2,2],
    ];
    #[rustfmt::skip]
    const HALVES: [[u8; 4]; 12] = [
        [0,0,0,1], [1,0,0,0], [0,1,0,0], [0,0,1,0],
        [0,0,0,2], [2,0,0,0], [0,2,0,0], [0,0,2,0],
        [0,2,0,1], [1,0,2,0], [0,1,0,2], [2,0,1,0],
    ];
    let cp = ch as u32;
    let arms = match cp {
        0x2500..=0x254b => BOX[(cp - 0x2500) as usize],
        0x2574..=0x257f => HALVES[(cp - 0x2574) as usize],
        _ => return None,
    };
    (arms != X).then_some(arms)
}

/// The arms as rectangles meeting at the cell's centre; a light line is about a twelfth of the
/// cell's height (a pixel at most sizes), a heavy one twice that.
fn lines([up, right, down, left]: [u8; 4], w: f32, h: f32) -> Vec<Rect> {
    let light = (h / 12.).floor().max(1.);
    let thick = |weight: u8| f32::from(weight.min(2)) * light;
    let (cx, cy) = ((w / 2.).floor(), (h / 2.).floor());
    // Each arm reaches past the centre by half the widest arm across it, so a corner is closed.
    let across_v = thick(up).max(thick(down));
    let across_h = thick(left).max(thick(right));
    let mut out = Vec::with_capacity(4);
    let mut arm = |weight: u8, vertical: bool, toward_start: bool| {
        if weight == 0 {
            return;
        }
        let t = thick(weight);
        if vertical {
            let reach = across_h.max(t) / 2.;
            let (y0, y1) = if toward_start { (0., cy + reach) } else { (cy - reach, h) };
            out.push(rect(cx - t / 2., y0, cx + t / 2., y1));
        } else {
            let reach = across_v.max(t) / 2.;
            let (x0, x1) = if toward_start { (0., cx + reach) } else { (cx - reach, w) };
            out.push(rect(x0, cy - t / 2., x1, cy + t / 2.));
        }
    };
    arm(up, true, true);
    arm(down, true, false);
    arm(left, false, true);
    arm(right, false, false);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_fill_their_part_of_the_cell_to_its_edges() {
        assert_eq!(rects('█', 7.2, 14.), Some(vec![rect(0., 0., 7.2, 14.)]));
        assert_eq!(rects('▀', 7.2, 14.), Some(vec![rect(0., 0., 7.2, 7.)]));
        assert_eq!(rects('▄', 8., 16.), Some(vec![rect(0., 8., 8., 16.)]));
        assert_eq!(rects('▌', 8., 16.), Some(vec![rect(0., 0., 4., 16.)]));
        assert_eq!(rects('▐', 8., 16.), Some(vec![rect(4., 0., 8., 16.)]));
        assert_eq!(rects('▚', 8., 16.).map(|r| r.len()), Some(2));
        assert_eq!(rects('▒', 8., 16.).map(|r| r[0].alpha), Some(0.5));
    }
    #[test]
    fn a_vertical_line_runs_the_cells_whole_height() {
        let heavy = rects('┃', 8., 16.).unwrap();
        let (top, bottom) =
            (heavy.iter().map(|r| r.y0).fold(f32::MAX, f32::min), heavy.iter().map(|r| r.y1).fold(0., f32::max));
        assert_eq!((top, bottom), (0., 16.), "row to row without a gap");
        let light = rects('│', 8., 16.).unwrap();
        assert!(heavy[0].x1 - heavy[0].x0 > light[0].x1 - light[0].x0, "heavy is thicker");
    }
    #[test]
    fn a_corner_closes_and_dashes_are_the_fonts() {
        let corner = rects('┌', 8., 16.).unwrap();
        assert_eq!(corner.len(), 2);
        let (down, right) = (corner[0], corner[1]);
        assert!(down.y0 <= right.y0 && right.x0 <= down.x0, "the arms overlap at the centre");
        assert!(drawn('─') && drawn('╋') && drawn('╹') && drawn('▓'));
        assert!(!drawn('┄') && !drawn('═') && !drawn('a'));
        assert_eq!(rects('┄', 8., 16.), None);
    }
}
