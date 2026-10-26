use super::vt::Emulator;
use libghostty_vt::render::{
    CellIteration, CellIterator, CursorVisualStyle, Dirty, RenderState, RowIterator, Snapshot,
};
use libghostty_vt::screen::CellWide;
use libghostty_vt::style::{RgbColor, StyleColor, Underline as SgrUnderline};
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Text(Repr);
const INLINE: usize = 22;
#[derive(Clone, PartialEq, Eq, Hash)]
enum Repr {
    Inline { len: u8, bytes: [u8; INLINE] },
    Heap(Arc<str>),
}
impl Text {
    pub fn as_str(&self) -> &str {
        match &self.0 {
            Repr::Inline { len, bytes } => unsafe { std::str::from_utf8_unchecked(&bytes[..usize::from(*len)]) },
            Repr::Heap(text) => text,
        }
    }
}
impl Default for Text {
    fn default() -> Self {
        Self::from(" ")
    }
}
impl Deref for Text {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}
impl From<&str> for Text {
    fn from(text: &str) -> Self {
        if text.len() <= INLINE {
            let mut bytes = [0; INLINE];
            bytes[..text.len()].copy_from_slice(text.as_bytes());
            return Self(Repr::Inline { len: text.len() as u8, bytes });
        }
        Self(Repr::Heap(text.into()))
    }
}
impl From<char> for Text {
    fn from(ch: char) -> Self {
        Self::from(ch.encode_utf8(&mut [0; 4]) as &str)
    }
}
impl From<String> for Text {
    fn from(text: String) -> Self {
        Self::from(text.as_str())
    }
}
impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}
impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Underline {
    #[default]
    None,
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}
impl Underline {
    fn from_sgr(style: SgrUnderline) -> Self {
        match style {
            SgrUnderline::None => Self::None,
            SgrUnderline::Double => Self::Double,
            SgrUnderline::Curly => Self::Curly,
            SgrUnderline::Dotted => Self::Dotted,
            SgrUnderline::Dashed => Self::Dashed,
            _ => Self::Single,
        }
    }
    pub fn is_some(self) -> bool {
        self != Self::None
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Cell {
    pub text: Text,
    pub fg: u32,
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: Underline,
    pub underline_color: Option<u32>,
    pub strike: bool,
    pub overline: bool,
    pub link: bool,
    pub wide: bool,
}
impl Cell {
    #[cfg(test)]
    pub fn plain(text: impl Into<Text>) -> Self {
        Self { text: text.into(), fg: crate::theme::terminal::FG_REF, ..Self::default() }
    }
    fn blank(fg: u32, bg: Option<u32>) -> Self {
        Self { fg, bg, ..Self::default() }
    }
}
pub type Row = Arc<Vec<Cell>>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
    Hollow,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub x: u16,
    pub y: u16,
    pub shape: CursorShape,
    pub blinking: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    pub cols: u16,
    pub rows: Vec<Row>,
    pub cursor: Option<Cursor>,
    pub images: Vec<super::images::Placed>,
    pub default_fg: u32,
    pub default_bg: u32,
}
impl Default for Grid {
    fn default() -> Self {
        Self {
            cols: 0,
            rows: Vec::new(),
            cursor: None,
            images: Vec::new(),
            default_fg: crate::theme::terminal::FG_REF,
            default_bg: crate::theme::terminal::BG_REF,
        }
    }
}
impl Grid {
    #[cfg(test)]
    pub fn of_lines(cols: u16, lines: &[&str]) -> Self {
        let row = |line: &&str| -> Row {
            Arc::new(line.chars().map(|ch| Cell { wide: is_wide(ch), ..Cell::plain(ch) }).collect())
        };
        Self { cols, rows: lines.iter().map(row).collect(), ..Self::default() }
    }
    #[cfg(test)]
    pub fn lines(&self) -> impl Iterator<Item = String> + '_ {
        self.rows.iter().map(|row| row.iter().map(|cell| cell.text.as_str()).collect())
    }
}
#[cfg(test)]
fn is_wide(ch: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&ch)
}
pub struct Painter {
    render_state: RenderState<'static>,
    row_iterator: RowIterator<'static>,
    row_cells: CellIterator<'static>,
    text: String,
    rows: Vec<Row>,
    shape: Option<Shape>,
}
#[derive(Clone, PartialEq)]
struct Shape {
    cols: u16,
    rows: u16,
    themed: [u32; 16],
    palette: [RgbColor; 256],
}
impl Painter {
    pub fn new() -> Option<Self> {
        Some(Self {
            render_state: RenderState::new().ok()?,
            row_iterator: RowIterator::new().ok()?,
            row_cells: CellIterator::new().ok()?,
            text: String::new(),
            rows: Vec::new(),
            shape: None,
        })
    }
    pub fn invalidate(&mut self) {
        self.shape = None;
        self.rows.clear();
    }
    pub fn grid(&mut self, terminal: &Emulator, cols: u16, rows: u16, themed: &[u32; 16]) -> Option<Grid> {
        let Self { render_state, row_iterator, row_cells, text, rows: last, shape } = self;
        let snapshot = render_state.update(terminal).ok()?;
        let colors = snapshot.colors().ok()?;
        let now = Shape { cols, rows, themed: *themed, palette: colors.palette };
        let full = shape.as_ref() != Some(&now) || last.len() != usize::from(rows);
        let dirty = snapshot.dirty().ok()?;
        let cursor = cursor(&snapshot);
        if !full && dirty == Dirty::Clean {
            return Some(grid(cols, last.clone(), cursor));
        }
        let colours = Colours { palette: &colors.palette, themed };
        let mut out = Vec::with_capacity(usize::from(rows));
        let mut moved = Moved::default();
        {
            let mut iter = row_iterator.update(&snapshot).ok()?;
            for y in 0..usize::from(rows) {
                let Some(row) = iter.next() else { break };
                let kept = (!full && dirty == Dirty::Partial && matches!(row.dirty(), Ok(false)))
                    .then(|| last.get(y))
                    .flatten();
                let built = match kept {
                    Some(built) => Arc::clone(built),
                    None => {
                        let mut cells = row_cells.update(row).ok()?;
                        let fresh = read_row(&mut cells, cols, &colours, text);
                        moved.find(last, &mut out, y, fresh)
                    }
                };
                let _ = row.set_dirty(false);
                out.push(built);
            }
        }
        let _ = snapshot.set_dirty(Dirty::Clean);
        *last = out.clone();
        *shape = Some(now);
        Some(grid(cols, out, cursor))
    }
}
unsafe impl Send for Painter {}
/// Finds a rebuilt row among the last screen's, so a row that only moved (a scroll, a program
/// redrawing what it already showed) keeps its identity, and with it everything cached by it: a
/// tab's shaped text is kept per row. The row in its own place is looked at first; the rest are
/// indexed by content once a second row has changed, as one changed row is a row typed into.
#[derive(Default)]
struct Moved {
    first: Option<usize>,
    index: Option<std::collections::HashMap<u64, usize>>,
}
impl Moved {
    fn find(&mut self, last: &[Row], out: &mut [Row], y: usize, fresh: Vec<Cell>) -> Row {
        if let Some(row) = last.get(y).filter(|row| ***row == fresh) {
            return Arc::clone(row);
        }
        if self.index.is_none() {
            let Some(first) = self.first else {
                self.first = Some(y);
                return Arc::new(fresh);
            };
            let index = self.index.insert(last.iter().enumerate().map(|(i, row)| (content(row), i)).collect());
            if let Some(row) = out.get_mut(first)
                && let Some(&i) = index.get(&content(row))
                && last[i] == *row
            {
                *row = Arc::clone(&last[i]);
            }
        }
        let index = self.index.as_ref().expect("indexed above");
        match index.get(&content(&fresh)) {
            Some(&i) if *last[i] == fresh => Arc::clone(&last[i]),
            _ => Arc::new(fresh),
        }
    }
}
fn content(cells: &[Cell]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    cells.hash(&mut hasher);
    hasher.finish()
}
fn grid(cols: u16, rows: Vec<Row>, cursor: Option<Cursor>) -> Grid {
    Grid { cols, rows, cursor, ..Grid::default() }
}
struct Colours<'a> {
    palette: &'a [RgbColor; 256],
    themed: &'a [u32; 16],
}
impl Colours<'_> {
    fn resolve(&self, color: StyleColor) -> Option<u32> {
        match color {
            StyleColor::None => None,
            StyleColor::Palette(index) => {
                let index = index.0;
                let packed = pack(self.palette[usize::from(index)]);
                match self.themed.get(usize::from(index)) {
                    Some(theirs) if *theirs == packed => Some(crate::theme::terminal::ansi_ref(index)),
                    _ => Some(packed),
                }
            }
            StyleColor::Rgb(rgb) => Some(pack(rgb)),
        }
    }
}
fn read_row(cells: &mut CellIteration<'_, '_>, cols: u16, colours: &Colours<'_>, text: &mut String) -> Vec<Cell> {
    let default_fg = crate::theme::terminal::FG_REF;
    let default_bg = crate::theme::terminal::BG_REF;
    let mut row = Vec::with_capacity(usize::from(cols));
    for _ in 0..cols {
        let Some(cell) = cells.next() else { break };
        let Ok(raw) = cell.raw_cell() else { continue };
        let wide = raw.wide().unwrap_or(CellWide::Narrow);
        if wide == CellWide::SpacerTail {
            continue;
        }
        let has_hyperlink = raw.has_hyperlink().unwrap_or(false);
        let style = cell.style().unwrap_or_default();
        let underlined = style.underline != SgrUnderline::None;
        let mut fg = colours.resolve(style.fg_color).unwrap_or(default_fg);
        let mut bg = colours.resolve(style.bg_color);
        if style.inverse {
            let inverted_fg = bg.unwrap_or(default_bg);
            bg = Some(fg);
            fg = inverted_fg;
        }
        if style.invisible {
            fg = bg.unwrap_or(default_bg);
        }
        if style.faint {
            fg = crate::theme::terminal::dimmed(fg);
        }
        let has_text = raw.has_text().unwrap_or(false) && cell.graphemes_utf8(text).is_ok() && !text.is_empty();
        if !has_text && !has_hyperlink && !underlined && !style.strikethrough && !style.overline {
            row.push(Cell::blank(fg, bg));
            continue;
        }
        row.push(Cell {
            text: if has_text { Text::from(text.as_str()) } else { Text::default() },
            fg,
            bg,
            bold: style.bold,
            italic: style.italic,
            underline: Underline::from_sgr(style.underline),
            underline_color: colours.resolve(style.underline_color),
            strike: style.strikethrough,
            overline: style.overline,
            link: has_hyperlink,
            wide: wide == CellWide::Wide,
        });
    }
    row
}
fn cursor(snapshot: &Snapshot<'_, '_>) -> Option<Cursor> {
    let cursor = snapshot.cursor().ok().filter(|cursor| cursor.visible)?;
    let viewport = cursor.viewport?;
    Some(Cursor {
        x: viewport.x,
        y: viewport.y,
        shape: match cursor.visual_style {
            CursorVisualStyle::Block => CursorShape::Block,
            CursorVisualStyle::Bar => CursorShape::Bar,
            CursorVisualStyle::Underline => CursorShape::Underline,
            CursorVisualStyle::BlockHollow => CursorShape::Hollow,
            _ => CursorShape::Block,
        },
        blinking: cursor.blinking,
    })
}
fn pack(color: RgbColor) -> u32 {
    (u32::from(color.r) << 16) | (u32::from(color.g) << 8) | u32::from(color.b)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_is_inline_when_short() {
        assert_eq!(Text::from("a").as_str(), "a");
        let family = "👨\u{200d}👩\u{200d}👧\u{200d}👦";
        assert!(family.len() > INLINE);
        assert_eq!(Text::from(family).as_str(), family);
        assert_eq!(std::mem::size_of::<Text>(), 24);
    }
}
