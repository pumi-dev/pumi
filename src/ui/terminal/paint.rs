use super::Metrics;
use super::sprite;
use crate::term::grid::{Cell, Grid, Row, Underline};
use crate::term::selection::{self, Point};
use crate::theme;
use gpui::{
    App, Bounds, Font, Hsla, Pixels, ShapedLine, SharedString, TextRun, UnderlineStyle, Window, fill, point, px, rgb,
    size,
};
use std::collections::HashMap;
use std::sync::Arc;
/// The least contrast (WCAG's ratio) selected text keeps with the selection under it. Text with
/// less, as text in the selection's own colour has, is drawn in the terminal's foreground or
/// background instead, whichever stands out more.
const SELECTED_CONTRAST: f32 = 2.0;
#[derive(Clone, PartialEq)]
pub struct Look {
    pub font_size: Pixels,
    pub fonts: [Font; 4],
    pub metrics: Metrics,
    pub theme: u64,
    /// Settings' Minimum contrast: the least WCAG ratio text keeps with its background.
    pub contrast: f32,
}
impl Look {
    pub fn new(font_size: Pixels, fonts: [Font; 4], metrics: Metrics, settings: &crate::settings::Settings) -> Self {
        let t = &settings.terminal;
        Look { font_size, fonts, metrics, theme: theme::generation(), contrast: t.minimum_contrast }
    }
}
#[derive(Default)]
pub struct Cache {
    look: Option<Look>,
    rows: HashMap<usize, Arc<Painted>>,
}
pub struct Painted {
    _row: Row,
    backgrounds: Vec<Span<Hsla>>,
    /// Each run of text: its column, its shaped line, and the runs it was shaped from.
    text: Vec<(u16, ShapedLine, Runs)>,
    lines: Vec<Span<(Line, Hsla)>>,
    /// The cells drawn as rectangles (`sprite`), not by the font: column, character, colour.
    sprites: Vec<(u16, char, Hsla)>,
    gutter: Option<Hsla>,
    /// The columns the row covers, when its background may be carried into the padding: every
    /// cell has one of its own, and none is a powerline glyph (Ghostty's `extend`).
    extends: Option<u16>,
}
#[derive(Clone, Copy, PartialEq)]
struct Span<T> {
    col: u16,
    cells: u16,
    what: T,
}
#[derive(Clone, Copy, PartialEq)]
enum Line {
    Under(Underline),
    Strike,
    Over,
}
pub struct Frame {
    pub grid: Arc<Grid>,
    pub look: Look,
    pub selection: Option<(Point, Point)>,
    pub block: Option<(u16, u16)>,
}
pub struct Prepared {
    /// The rows laid out anew for this frame (the rest kept theirs).
    pub shaped: usize,
    rows: Vec<Arc<Painted>>,
    images: Vec<crate::term::images::Placed>,
    selection: Vec<(u16, Span<()>)>,
    /// The text runs the selection covers that would not read on it, shaped again in colours
    /// that do: by row, and index in the row's `text`.
    legible: Vec<(u16, usize, ShapedLine)>,
    block: Option<(u16, u16, u16, Option<ShapedLine>)>,
    /// The block cursor's cell when it is a sprite: drawn again over the cursor, in its colour.
    block_sprite: Option<(char, Hsla)>,
    metrics: Metrics,
    cols: u16,
}
impl Cache {
    pub fn prepare(&mut self, frame: &Frame, window: &mut Window) -> Prepared {
        if self.look.as_ref() != Some(&frame.look) {
            self.rows.clear();
            self.look = Some(frame.look.clone());
        }
        let shaper = Shaper { look: &frame.look, window };
        let mut kept = HashMap::with_capacity(frame.grid.rows.len());
        let mut shaped = 0;
        let rows: Vec<Arc<Painted>> = frame
            .grid
            .rows
            .iter()
            .map(|row| {
                let key = Arc::as_ptr(row) as usize;
                // A row may stand in more than one place (blank rows, a line repeated): the
                // second takes what the first already kept.
                let painted = kept.get(&key).cloned().or_else(|| self.rows.remove(&key)).unwrap_or_else(|| {
                    shaped += 1;
                    Arc::new(shaper.row(row))
                });
                kept.insert(key, Arc::clone(&painted));
                painted
            })
            .collect();
        self.rows = kept;
        let selection = frame.selection.map(|bounds| selected(&frame.grid, bounds)).unwrap_or_default();
        let under = theme::terminal::selection();
        let legible = selection
            .iter()
            .filter_map(|(y, span)| Some((*y, span, rows.get(usize::from(*y))?)))
            .flat_map(|(y, span, painted): (u16, &Span<()>, &Arc<Painted>)| {
                painted.text.iter().enumerate().filter_map(move |(i, (col, shaped, runs))| {
                    let cells = shaped.text.chars().count() as u16;
                    let overlaps = *col < span.col + span.cells && span.col < col + cells.max(1);
                    let recoloured: Runs =
                        runs.iter().map(|&(len, font, fg)| (len, font, readable(fg, under))).collect();
                    (overlaps && recoloured != *runs).then(|| (y, i, recoloured))
                })
            })
            .map(|(y, i, runs)| {
                let text = rows[usize::from(y)].text[i].1.text.clone();
                (y, i, shaper.shape(&text, &runs))
            })
            .collect();
        let mut block_sprite = None;
        let block = frame.block.and_then(|(x, y)| {
            let row = frame.grid.rows.get(usize::from(y))?;
            let found = selection::columns(row).find(|(col, cell)| *col == x || (cell.wide && *col + 1 == x));
            let (col, cell) = found.map_or((x, None), |(col, cell)| (col, Some(cell)));
            let cells = if cell.is_some_and(|cell| cell.wide) { 2 } else { 1 };
            let sprite = cell.and_then(sprite_of);
            if let (Some(ch), Some(cell)) = (sprite, cell) {
                let under = cell.bg.unwrap_or(theme::terminal::BG_REF);
                block_sprite = Some((ch, Hsla::from(rgb(theme::terminal::resolve(under)))));
            }
            let glyph = cell.filter(|cell| sprite.is_none() && !cell.text.trim().is_empty()).map(|cell| {
                let under = cell.bg.unwrap_or(theme::terminal::BG_REF);
                shaper.shape(cell.text.as_str(), &[(cell.text.len(), font_index(cell), under)])
            });
            Some((col, y, cells, glyph))
        });
        Prepared {
            shaped,
            rows,
            images: frame.grid.images.clone(),
            selection,
            legible,
            block,
            block_sprite,
            metrics: frame.look.metrics,
            cols: frame.grid.cols,
        }
    }
}
impl Prepared {
    /// Fills the padding between the grid at `origin` and `outer`'s edges with the backgrounds of
    /// the cells next to it, where their rows allow (`Painted::extends`): a program that paints
    /// its whole screen is painted to the pane's edges, as in Ghostty's `window-padding-color =
    /// extend`. Elsewhere the theme's background (under the element) shows.
    pub fn paint_padding(&self, outer: Bounds<Pixels>, origin: gpui::Point<Pixels>, window: &mut Window) {
        let Metrics { cell_width, line_height } = self.metrics;
        let cols = self.cols;
        let Some(last) = self.rows.len().checked_sub(1) else { return };
        let full = |row: &Painted| row.extends == Some(cols) && !row.backgrounds.is_empty();
        let x = |col: u16| origin.x + px(f32::from(col) * cell_width);
        let y = |row: usize| origin.y + px(row as f32 * line_height);
        let (left, right) = (outer.left(), outer.right().max(x(cols)));
        let scale = window.scale_factor();
        let mut quad = |x0: Pixels, y0: Pixels, x1: Pixels, y1: Pixels, color: Hsla| {
            if x1 > x0 && y1 > y0 {
                window.paint_quad(fill(snapped(x0, y0, x1, y1, scale), color));
            }
        };
        for (i, row) in self.rows.iter().enumerate().filter(|(_, row)| full(row)) {
            // Rows at the top and bottom carry their spans up and down too; corners take the
            // corner cell's colour.
            let top = if i == 0 { outer.top() } else { y(i) };
            let bottom = if i == last { outer.bottom().max(y(i + 1)) } else { y(i + 1) };
            for span in &row.backgrounds {
                let x0 = if span.col == 0 { left } else { x(span.col) };
                let x1 = if span.col + span.cells >= cols { right } else { x(span.col + span.cells) };
                if i == 0 {
                    quad(x0, top, x1, y(0), span.what);
                }
                if i == last {
                    quad(x0, y(i + 1), x1, bottom, span.what);
                }
            }
            let (first, end) = (row.backgrounds[0].what, row.backgrounds[row.backgrounds.len() - 1].what);
            quad(left, y(i), x(0), y(i + 1), first);
            quad(x(cols), y(i), right, y(i + 1), end);
        }
    }
    /// Paints the frame, its glyphs smoothed or not as Settings' Font smoothing says.
    pub fn paint(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let smoothing = crate::settings::Settings::get(cx).terminal.font_smoothing;
        window.with_font_smoothing(smoothing, |window| self.paint_smoothed(bounds, window, cx));
    }
    fn paint_smoothed(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let Metrics { cell_width, line_height } = self.metrics;
        let at = |col: u16, row: usize| {
            point(bounds.origin.x + px(f32::from(col) * cell_width), bounds.origin.y + px(row as f32 * line_height))
        };
        let width = |cells: u16| px(f32::from(cells) * cell_width);
        let line = px(line_height);
        let scale = window.scale_factor();
        // Backgrounds and sprites on whole device pixels: a cell's edge between two would
        // otherwise be blended into a hairline where they meet.
        let cell_box = |col: u16, cells: u16, row: usize| {
            let (start, end) = (at(col, row), at(col + cells, row));
            snapped(start.x, start.y, end.x, start.y + line, scale)
        };
        for (y, row) in self.rows.iter().enumerate() {
            for span in &row.backgrounds {
                window.paint_quad(fill(cell_box(span.col, span.cells, y), span.what));
            }
        }
        self.paint_images(bounds, |z| z < 0, window);
        let selected = Hsla::from(theme::terminal::background(theme::terminal::selection()));
        for (y, span) in &self.selection {
            let bounds = Bounds::new(at(span.col, usize::from(*y)), size(width(span.cells), line));
            window.paint_quad(fill(bounds, selected));
        }
        let cursor = Hsla::from(rgb(theme::terminal::cursor()));
        if let Some((col, y, cells, _)) = &self.block {
            window.paint_quad(fill(Bounds::new(at(*col, usize::from(*y)), size(width(*cells), line)), cursor));
        }
        for (y, row) in self.rows.iter().enumerate() {
            if let Some(gutter) = row.gutter {
                let origin = at(0, y) - point(px(4.), px(0.));
                window.paint_quad(fill(Bounds::new(origin, size(px(2.), line)), gutter));
            }
            for (i, (col, shaped, _)) in row.text.iter().enumerate() {
                let hidden = self
                    .block
                    .as_ref()
                    .filter(|(c, by, cells, _)| usize::from(*by) == y && covers(*c, *cells, *col, shaped))
                    .map(|(c, _, cells, _)| (c.saturating_sub(*col), c.saturating_sub(*col) + cells));
                let recoloured = self.legible.iter().find(|(by, bi, _)| usize::from(*by) == y && *bi == i);
                let selected = recoloured.and_then(|_| {
                    let (_, span) = self.selection.iter().find(|(by, _)| usize::from(*by) == y)?;
                    Some((span.col.saturating_sub(*col), (span.col + span.cells).saturating_sub(*col)))
                });
                let origin = at(*col, y);
                if hidden.is_none() && selected.is_none() {
                    let _ = shaped.paint(origin, line, gpui::TextAlign::Left, None, window, cx);
                    continue;
                }
                let mut plain = vec![(0, u16::MAX)];
                if let Some(cut) = hidden {
                    plain = without(&plain, cut);
                }
                if let (Some(cut), Some((_, _, recoloured))) = (selected, recoloured) {
                    let inside = hidden.map_or_else(|| vec![cut], |hidden| without(&[cut], hidden));
                    paint_parts(recoloured, origin, line, &inside, self.metrics, window, cx);
                    plain = without(&plain, cut);
                }
                paint_parts(shaped, origin, line, &plain, self.metrics, window, cx);
            }
            for span in &row.lines {
                paint_line(span, at(span.col, y), width(span.cells), line_height, window);
            }
            for &(col, ch, color) in &row.sprites {
                let covered = self
                    .block
                    .as_ref()
                    .is_some_and(|(c, by, cells, _)| usize::from(*by) == y && col >= *c && col < c + cells);
                if !covered {
                    paint_sprite(ch, at(col, y), self.metrics, color, window);
                }
            }
        }
        if let Some((col, y, _, Some(glyph))) = &self.block {
            let _ = glyph.paint(at(*col, usize::from(*y)), line, gpui::TextAlign::Left, None, window, cx);
        }
        if let (Some((col, y, _, _)), Some((ch, color))) = (&self.block, self.block_sprite) {
            paint_sprite(ch, at(*col, usize::from(*y)), self.metrics, color, window);
        }
        self.paint_images(bounds, |z| z >= 0, window);
    }
    fn paint_images(&self, bounds: Bounds<Pixels>, layer: impl Fn(i32) -> bool, window: &mut Window) {
        let Metrics { cell_width, line_height } = self.metrics;
        for placed in self.images.iter().filter(|p| layer(p.z)) {
            let origin = bounds.origin
                + point(
                    px(placed.col as f32 * cell_width + placed.offset.0 as f32),
                    px(placed.row as f32 * line_height + placed.offset.1 as f32),
                );
            let at = Bounds::new(origin, size(px(placed.size.0 as f32), px(placed.size.1 as f32)));
            let _ = window.paint_image(at, at, Default::default(), Arc::clone(&placed.image), 0, false);
        }
    }
}
fn covers(cursor: u16, cells: u16, col: u16, shaped: &ShapedLine) -> bool {
    let len = shaped.text.chars().count() as u16;
    col < cursor + cells && cursor < col + len.max(1)
}
/// `parts` (column ranges, `u16::MAX` the line's end) without the columns `from..to`.
fn without(parts: &[(u16, u16)], (from, to): (u16, u16)) -> Vec<(u16, u16)> {
    parts
        .iter()
        .flat_map(|&(start, end)| [(start, end.min(from)), (start.max(to), end)])
        .filter(|(start, end)| start < end)
        .collect()
}
/// Paints `shaped` at `origin`, only within `parts`: column ranges relative to it, `u16::MAX` its
/// end.
fn paint_parts(
    shaped: &ShapedLine,
    origin: gpui::Point<Pixels>,
    line: Pixels,
    parts: &[(u16, u16)],
    metrics: Metrics,
    window: &mut Window,
    cx: &mut App,
) {
    let full = Bounds::new(origin, size(shaped.width + px(metrics.cell_width), line));
    let x = |cells: u16| match cells {
        u16::MAX => full.right(),
        cells => origin.x + px(f32::from(cells) * metrics.cell_width),
    };
    for &(from, to) in parts {
        let part = Bounds::from_corners(point(x(from), full.top()), point(x(to).min(full.right()), full.bottom()));
        if part.size.width > px(0.) {
            window.with_content_mask(Some(gpui::ContentMask { bounds: part }), |window| {
                let _ = shaped.paint(origin, line, gpui::TextAlign::Left, None, window, cx);
            });
        }
    }
}
fn paint_line(
    span: &Span<(Line, Hsla)>,
    origin: gpui::Point<Pixels>,
    width: Pixels,
    line_height: f32,
    window: &mut Window,
) {
    let (kind, color) = span.what;
    let thin = px(1.);
    let rule = |window: &mut Window, y: f32| {
        window.paint_quad(fill(Bounds::new(origin + point(px(0.), px(y)), size(width, thin)), color))
    };
    let under = line_height - 3.;
    match kind {
        Line::Under(Underline::None) => {}
        Line::Under(Underline::Single) => rule(window, under),
        Line::Under(Underline::Double) => {
            rule(window, under - 2.);
            rule(window, under);
        }
        Line::Under(Underline::Curly) => {
            let style = UnderlineStyle { thickness: thin, color: Some(color), wavy: true };
            window.paint_underline(origin + point(px(0.), px(under - 2.)), width, &style);
        }
        Line::Under(dashes @ (Underline::Dotted | Underline::Dashed)) => {
            let (on, off): (f32, f32) = if dashes == Underline::Dotted { (1., 1.) } else { (3., 2.) };
            let mut x = 0.;
            while x < f32::from(width) {
                let len = on.min(f32::from(width) - x);
                let bounds = Bounds::new(origin + point(px(x), px(under)), size(px(len), thin));
                window.paint_quad(fill(bounds, color));
                x += on + off;
            }
        }
        Line::Strike => rule(window, (line_height / 2.).round()),
        Line::Over => rule(window, 0.),
    }
}
fn selected(grid: &Grid, bounds: (Point, Point)) -> Vec<(u16, Span<()>)> {
    let (start, end) = bounds;
    (start.row..=end.row.min(grid.rows.len().saturating_sub(1) as u16))
        .filter_map(|row| {
            let first = if row == start.row { start.col } else { 0 };
            let last = if row == end.row { end.col } else { grid.cols.saturating_sub(1) };
            (first <= last).then_some((row, Span { col: first, cells: last - first + 1, what: () }))
        })
        .collect()
}
/// The colour `fg` is drawn in over a selection of `under`: its own, unless that would not read.
fn readable(fg: u32, under: u32) -> u32 {
    if contrast(theme::terminal::resolve(fg), under) >= SELECTED_CONTRAST {
        return fg;
    }
    let (fore, back) = (theme::terminal::fg(), theme::terminal::bg());
    if contrast(fore, under) >= contrast(back, under) { fore } else { back }
}
/// `fg` over `bg`, lightened or darkened as little as it takes to keep a contrast of `least`
/// (Ghostty's `minimum-contrast`); as it is when it has that already, or `least` is 1.
fn legible(fg: u32, bg: u32, least: f32) -> u32 {
    if least <= 1.0 {
        return fg;
    }
    let (fore, back) = (theme::terminal::resolve(fg), theme::terminal::resolve(bg));
    if contrast(fore, back) >= least {
        return fg;
    }
    let mix = |to: u32, t: f32| {
        let channel = |shift: u32| {
            let (a, b) = (((fore >> shift) & 0xff) as f32, ((to >> shift) & 0xff) as f32);
            ((a + (b - a) * t).round() as u32).min(255) << shift
        };
        channel(16) | channel(8) | channel(0)
    };
    // The least blend toward white, and toward black, that reads; the nearer of the two wins.
    let toward = |to: u32| {
        if contrast(mix(to, 1.0), back) < least {
            return None;
        }
        let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
        for _ in 0..12 {
            let mid = (lo + hi) / 2.0;
            if contrast(mix(to, mid), back) >= least { hi = mid } else { lo = mid }
        }
        Some((hi, mix(to, hi)))
    };
    match (toward(0xffffff), toward(0x000000)) {
        (Some(white), Some(black)) => {
            if white.0 <= black.0 {
                white.1
            } else {
                black.1
            }
        }
        (Some((_, c)), None) | (None, Some((_, c))) => c,
        (None, None) => {
            if contrast(0xffffff, back) >= contrast(0x000000, back) {
                0xffffff
            } else {
                0x000000
            }
        }
    }
}
/// WCAG's contrast ratio of two `0xrrggbb` colours, 1 to 21.
fn contrast(a: u32, b: u32) -> f32 {
    let luminance = |c: u32| {
        let channel = |shift: u32| {
            let v = ((c >> shift) & 0xff) as f32 / 255.;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn font_index(cell: &Cell) -> usize {
    usize::from(cell.bold) | (usize::from(cell.italic) << 1)
}
/// A text run's styles: its length in bytes, its font (`font_index`), its colour.
type Runs = Vec<(usize, usize, u32)>;
type Batch = (u16, String, Runs);
struct Shaper<'a> {
    look: &'a Look,
    window: &'a Window,
}
impl Shaper<'_> {
    fn shape(&self, text: &str, runs: &[(usize, usize, u32)]) -> ShapedLine {
        let runs: Vec<TextRun> = runs
            .iter()
            .map(|(len, font, fg)| TextRun {
                len: *len,
                font: self.look.fonts[*font].clone(),
                color: Hsla::from(rgb(theme::terminal::resolve(*fg))),
                background_color: None,
                underline: None,
                strikethrough: None,
            })
            .collect();
        let text = SharedString::from(text.to_string());
        let cell = px(self.look.metrics.cell_width);
        self.window.text_system().shape_line(text, self.look.font_size, &runs, Some(cell))
    }
    fn row(&self, row: &Row) -> Painted {
        let mut painted = Painted {
            _row: Arc::clone(row),
            backgrounds: Vec::new(),
            text: Vec::new(),
            lines: Vec::new(),
            sprites: Vec::new(),
            gutter: None,
            extends: Some(0),
        };
        let mut batch: Option<Batch> = None;
        let flush = |batch: &mut Option<Batch>, painted: &mut Painted| {
            if let Some((col, text, runs)) = batch.take() {
                let trimmed = text.trim_end_matches(' ');
                if !trimmed.is_empty() {
                    let mut runs = runs;
                    let mut cut = text.len() - trimmed.len();
                    while cut > 0 {
                        let Some(last) = runs.last_mut() else { break };
                        let take = last.0.min(cut);
                        last.0 -= take;
                        cut -= take;
                        if last.0 == 0 {
                            runs.pop();
                        }
                    }
                    painted.text.push((col, self.shape(trimmed, &runs), runs));
                }
            }
        };
        for (col, cell) in selection::columns(row) {
            let (fg, bg, bold, underline) = (cell.fg, cell.bg, cell.bold, cell.underline);
            let fg = legible(fg, bg.unwrap_or(theme::terminal::BG_REF), self.look.contrast);
            let cells = if cell.wide { 2 } else { 1 };
            if let Some(bg) = bg {
                push(&mut painted.backgrounds, col, cells, Hsla::from(theme::terminal::background(bg)));
            }
            painted.extends = painted
                .extends
                .filter(|_| bg.is_some() && !cell.text.chars().any(powerline))
                .map(|covered| covered.max(col + cells));
            let colour = |c: Option<u32>| Hsla::from(rgb(theme::terminal::resolve(c.unwrap_or(fg))));
            if underline.is_some() {
                push(&mut painted.lines, col, cells, (Line::Under(underline), colour(cell.underline_color)));
            }
            if cell.strike {
                push(&mut painted.lines, col, cells, (Line::Strike, colour(None)));
            }
            if cell.overline {
                push(&mut painted.lines, col, cells, (Line::Over, colour(None)));
            }
            let font = usize::from(bold) | (usize::from(cell.italic) << 1);
            if let Some(ch) = sprite_of(cell) {
                // Drawn as rectangles, not shaped: the run of text before it ends here.
                flush(&mut batch, &mut painted);
                painted.sprites.push((col, ch, Hsla::from(rgb(theme::terminal::resolve(fg)))));
                continue;
            }
            let alone = cell.wide || cell.text.chars().nth(1).is_some();
            if alone {
                flush(&mut batch, &mut painted);
            }
            let (_, text, runs) = batch.get_or_insert_with(|| (col, String::new(), Vec::new()));
            text.push_str(&cell.text);
            match runs.last_mut() {
                Some(run) if run.1 == font && run.2 == fg => run.0 += cell.text.len(),
                _ => runs.push((cell.text.len(), font, fg)),
            }
            if alone {
                flush(&mut batch, &mut painted);
            }
        }
        flush(&mut batch, &mut painted);
        painted
    }
}
/// The character a cell holds, when it is one `sprite` draws: a lone block element or line.
fn sprite_of(cell: &Cell) -> Option<char> {
    let mut chars = cell.text.chars();
    let ch = chars.next()?;
    (chars.next().is_none() && !cell.wide && sprite::drawn(ch)).then_some(ch)
}
/// Paints `ch` in the cell at `origin`, its rectangles on whole device pixels so neighbouring
/// cells' meet without a seam.
fn paint_sprite(ch: char, origin: gpui::Point<Pixels>, metrics: Metrics, color: Hsla, window: &mut Window) {
    let Some(rects) = sprite::rects(ch, metrics.cell_width, metrics.line_height) else { return };
    let scale = window.scale_factor();
    for r in rects {
        let bounds = snapped(origin.x + px(r.x0), origin.y + px(r.y0), origin.x + px(r.x1), origin.y + px(r.y1), scale);
        window.paint_quad(fill(bounds, color.opacity(r.alpha)));
    }
}
/// The box between two corners, its edges rounded to whole device pixels (at `scale`) and at
/// least one wide and tall.
fn snapped(x0: Pixels, y0: Pixels, x1: Pixels, y1: Pixels, scale: f32) -> Bounds<Pixels> {
    let snap = |v: Pixels| (f32::from(v) * scale).round();
    let (left, top) = (snap(x0), snap(y0));
    let (right, bottom) = (snap(x1).max(left + 1.), snap(y1).max(top + 1.));
    Bounds::from_corners(point(px(left / scale), px(top / scale)), point(px(right / scale), px(bottom / scale)))
}
/// Powerline's separators (and Nerd Fonts' additions to them): a row with one is a prompt or a
/// status line drawn to look shaped, which carrying its colour into the padding would square off.
fn powerline(ch: char) -> bool {
    matches!(ch, '\u{e0b0}'..='\u{e0d7}')
}
fn push<T: PartialEq + Copy>(spans: &mut Vec<Span<T>>, col: u16, cells: u16, what: T) {
    match spans.last_mut() {
        Some(span) if span.what == what && span.col + span.cells == col => span.cells += cells,
        _ => spans.push(Span { col, cells, what }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spans_merge_only_when_they_touch_and_match() {
        let mut spans = Vec::new();
        push(&mut spans, 0, 1, 1);
        push(&mut spans, 1, 2, 1);
        push(&mut spans, 3, 1, 2);
        push(&mut spans, 5, 1, 2);
        let flat: Vec<_> = spans.iter().map(|s| (s.col, s.cells, s.what)).collect();
        assert_eq!(flat, [(0, 3, 1), (3, 1, 2), (5, 1, 2)]);
    }
    #[test]
    fn a_column_range_is_cut_out_of_the_parts_it_falls_in() {
        assert_eq!(without(&[(0, u16::MAX)], (2, 4)), [(0, 2), (4, u16::MAX)]);
        assert_eq!(without(&[(0, 3), (5, 9)], (2, 6)), [(0, 2), (6, 9)]);
        assert_eq!(without(&[(2, 4)], (0, 9)), []);
    }
    #[test]
    fn selected_text_in_the_selections_own_colour_is_drawn_in_one_that_reads() {
        let lavender = 0xa9a0e8;
        assert_eq!(readable(lavender, lavender), theme::terminal::bg(), "dark on a light selection");
        let navy = 0x3a4a63;
        assert_eq!(readable(navy, navy), theme::terminal::fg(), "light on a dark selection");
        assert_eq!(readable(theme::terminal::FG_REF, navy), theme::terminal::FG_REF, "what reads keeps its colour");
    }
    #[test]
    fn minimum_contrast_moves_text_only_as_far_as_it_must() {
        let (navy, black) = (0x000080, 0x000000);
        assert_eq!(legible(navy, black, 1.0), navy, "1 leaves colours alone");
        let lifted = legible(navy, black, 4.5);
        assert!(contrast(lifted, black) >= 4.5, "reads now");
        assert!(contrast(lifted, black) < 5.0, "and no more than it must");
        let white = 0xffffff;
        assert_eq!(legible(white, black, 7.0), white, "what reads keeps its colour");
        assert!(contrast(legible(0xeeeeee, white, 3.0), white) >= 3.0, "darkened on a light background");
    }
    #[test]
    fn selection_spans_cover_whole_middle_rows() {
        let grid = Grid::of_lines(10, &["aaaa", "bbbb", "cccc"]);
        let spans = selected(&grid, (Point::new(2, 0), Point::new(1, 2)));
        let flat: Vec<_> = spans.iter().map(|(row, s)| (*row, s.col, s.cells)).collect();
        assert_eq!(flat, [(0, 2, 8), (1, 0, 10), (2, 0, 2)]);
    }
}
