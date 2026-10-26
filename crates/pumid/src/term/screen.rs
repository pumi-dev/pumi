//! A terminal's text as a person sees it, from the bytes its program wrote: just enough of a VT
//! (cursor moves, erases, inserts and deletes, scrolling and its region, the alternate screen,
//! autowrap) to know which row each character landed on. `tab.call(…, "read")` answers from it,
//! so a full-screen program's rows (an agent's TUI) come back one to a line, and not its redraws
//! run together. Colours and attributes are not kept: `ansi` reads the bytes themselves.
use std::collections::VecDeque;

/// How many rows that scrolled off the top are kept, for `recent`.
const SCROLLBACK: usize = 5000;

#[derive(Clone)]
struct Row {
    /// One char a cell; a wide char's second cell is `WIDE_TAIL`.
    cells: Vec<char>,
    /// The row's text goes on into the next one: it was wrapped at the screen's width.
    wrapped: bool,
}
const WIDE_TAIL: char = '\0';
impl Row {
    fn blank(cols: usize) -> Row {
        Row { cells: vec![' '; cols], wrapped: false }
    }
    fn text(&self) -> String {
        let text: String = self.cells.iter().filter(|c| **c != WIDE_TAIL).collect();
        text.trim_end().to_string()
    }
}

enum State {
    Ground,
    Escape,
    /// `ESC (` and the like: one more char names a charset.
    Charset,
    Csi {
        private: Option<char>,
        params: String,
    },
    /// OSC, DCS, APC, PM, SOS: skipped to BEL or ST.
    Text {
        escape: bool,
    },
}

pub struct Screen {
    cols: usize,
    rows: usize,
    main: Vec<Row>,
    alt: Vec<Row>,
    in_alt: bool,
    scrollback: VecDeque<Row>,
    x: usize,
    y: usize,
    /// The last column was written: the next char wraps first.
    pending_wrap: bool,
    top: usize,
    bottom: usize,
    saved: (usize, usize),
    state: State,
}

impl Screen {
    pub fn new(cols: u16, rows: u16) -> Screen {
        let (cols, rows) = (usize::from(cols.max(2)), usize::from(rows.max(2)));
        Screen {
            cols,
            rows,
            main: vec![Row::blank(cols); rows],
            alt: vec![Row::blank(cols); rows],
            in_alt: false,
            scrollback: VecDeque::new(),
            x: 0,
            y: 0,
            pending_wrap: false,
            top: 0,
            bottom: rows - 1,
            saved: (0, 0),
            state: State::Ground,
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for c in String::from_utf8_lossy(bytes).chars() {
            self.step(c);
        }
    }

    /// The screen as it is: one line a row, blank ones at the end left out.
    pub fn visible(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.grid().iter().map(Row::text).collect();
        trim_blank_end(&mut lines);
        lines
    }

    /// What scrolled off the top and the screen under it, one line a row.
    pub fn recent(&self) -> Vec<String> {
        let mut lines: Vec<String> = self.scrollback.iter().chain(self.grid().iter()).map(Row::text).collect();
        trim_blank_end(&mut lines);
        lines
    }

    /// `recent`, with the rows a long line was wrapped into joined again.
    pub fn unwrapped(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for row in self.scrollback.iter().chain(self.grid().iter()) {
            let text: String = row.cells.iter().filter(|c| **c != WIDE_TAIL).collect();
            if row.wrapped {
                line.push_str(&text);
            } else {
                line.push_str(text.trim_end());
                lines.push(std::mem::take(&mut line));
            }
        }
        if !line.is_empty() {
            lines.push(line.trim_end().to_string());
        }
        trim_blank_end(&mut lines);
        lines
    }

    fn grid(&self) -> &Vec<Row> {
        if self.in_alt { &self.alt } else { &self.main }
    }
    fn grid_mut(&mut self) -> &mut Vec<Row> {
        if self.in_alt { &mut self.alt } else { &mut self.main }
    }

    fn step(&mut self, c: char) {
        match std::mem::replace(&mut self.state, State::Ground) {
            State::Ground => self.ground(c),
            State::Escape => self.escape(c),
            State::Charset => {}
            State::Csi { private, mut params } => match c {
                '0'..='9' | ';' | ':' => {
                    params.push(c);
                    self.state = State::Csi { private, params };
                }
                '?' | '>' | '<' | '=' if params.is_empty() && private.is_none() => {
                    self.state = State::Csi { private: Some(c), params };
                }
                // Intermediates: kept reading, not acted on.
                ' '..='/' => self.state = State::Csi { private: private.or(Some(c)), params },
                '@'..='~' => self.csi(private, &params, c),
                // A control in the middle of a sequence is acted on; the sequence goes on.
                '\x1b' => self.state = State::Escape,
                c if c < ' ' => {
                    self.ground(c);
                    self.state = State::Csi { private, params };
                }
                _ => {}
            },
            State::Text { escape } => match c {
                '\x07' => {}
                '\\' if escape => {}
                '\x1b' => self.state = State::Text { escape: true },
                _ => self.state = State::Text { escape: false },
            },
        }
    }

    fn ground(&mut self, c: char) {
        match c {
            '\x1b' => self.state = State::Escape,
            '\r' => self.carriage_return(),
            '\n' | '\x0b' | '\x0c' => self.index(),
            '\x08' => {
                self.pending_wrap = false;
                self.x = self.x.saturating_sub(1);
            }
            '\t' => {
                self.pending_wrap = false;
                self.x = ((self.x / 8 + 1) * 8).min(self.cols - 1);
            }
            c if (c as u32) < 0x20 || c == '\x7f' => {}
            c => self.print(c),
        }
    }

    fn escape(&mut self, c: char) {
        match c {
            '[' => self.state = State::Csi { private: None, params: String::new() },
            ']' | 'P' | '_' | '^' | 'X' => self.state = State::Text { escape: false },
            '(' | ')' | '*' | '+' | '-' | '.' | '/' | '#' | '%' | ' ' => self.state = State::Charset,
            '7' => self.saved = (self.x, self.y),
            '8' => self.restore(),
            'D' => self.index(),
            'E' => {
                self.carriage_return();
                self.index();
            }
            'M' => self.reverse_index(),
            'c' => *self = Screen::new(self.cols as u16, self.rows as u16),
            _ => {}
        }
    }

    fn print(&mut self, c: char) {
        let width = width(c);
        if width == 0 {
            return;
        }
        if self.pending_wrap || self.x + width > self.cols {
            let y = self.y;
            self.grid_mut()[y].wrapped = true;
            self.carriage_return();
            self.index();
        }
        let (x, y) = (self.x, self.y);
        let row = &mut self.grid_mut()[y];
        row.cells[x] = c;
        if width == 2 && x + 1 < row.cells.len() {
            row.cells[x + 1] = WIDE_TAIL;
        }
        if x + width >= self.cols {
            self.x = self.cols - 1;
            self.pending_wrap = true;
        } else {
            self.x += width;
        }
    }

    fn carriage_return(&mut self) {
        self.pending_wrap = false;
        self.x = 0;
    }

    fn index(&mut self) {
        self.pending_wrap = false;
        if self.y == self.bottom {
            self.scroll_up(1);
        } else if self.y + 1 < self.rows {
            self.y += 1;
        }
    }

    fn reverse_index(&mut self) {
        self.pending_wrap = false;
        if self.y == self.top {
            self.scroll_down(1);
        } else {
            self.y = self.y.saturating_sub(1);
        }
    }

    fn scroll_up(&mut self, n: usize) {
        let (top, bottom, cols) = (self.top, self.bottom, self.cols);
        let to_scrollback = top == 0 && !self.in_alt;
        for _ in 0..n.min(bottom - top + 1) {
            let gone = self.grid_mut().remove(top);
            self.grid_mut().insert(bottom, Row::blank(cols));
            if to_scrollback {
                self.scrollback.push_back(gone);
                if self.scrollback.len() > SCROLLBACK {
                    self.scrollback.pop_front();
                }
            }
        }
    }

    fn scroll_down(&mut self, n: usize) {
        let (top, bottom, cols) = (self.top, self.bottom, self.cols);
        for _ in 0..n.min(bottom - top + 1) {
            self.grid_mut().remove(bottom);
            self.grid_mut().insert(top, Row::blank(cols));
        }
    }

    fn restore(&mut self) {
        self.pending_wrap = false;
        (self.x, self.y) = (self.saved.0.min(self.cols - 1), self.saved.1.min(self.rows - 1));
    }

    fn erase(&mut self, y: usize, from: usize, to: usize) {
        let row = &mut self.grid_mut()[y];
        let to = to.min(row.cells.len());
        for cell in &mut row.cells[from.min(to)..to] {
            *cell = ' ';
        }
        if to == row.cells.len() {
            row.wrapped = false;
        }
    }

    fn set_alt(&mut self, on: bool, save: bool) {
        if on == self.in_alt {
            return;
        }
        if on {
            if save {
                self.saved = (self.x, self.y);
            }
            self.alt = vec![Row::blank(self.cols); self.rows];
            self.in_alt = true;
        } else {
            self.in_alt = false;
            if save {
                self.restore();
            }
        }
    }

    fn csi(&mut self, private: Option<char>, params: &str, op: char) {
        let numbers: Vec<usize> = params.split([';', ':']).map(|p| p.parse::<usize>().unwrap_or(0)).collect::<Vec<_>>();
        let arg = |i: usize| numbers.get(i).copied().unwrap_or(0);
        let n = arg(0).max(1);
        if let Some(private) = private {
            if private == '?' && (op == 'h' || op == 'l') {
                for mode in &numbers {
                    match mode {
                        1049 => self.set_alt(op == 'h', true),
                        47 | 1047 => self.set_alt(op == 'h', false),
                        1048 if op == 'h' => self.saved = (self.x, self.y),
                        1048 => self.restore(),
                        _ => {}
                    }
                }
            }
            return;
        }
        let (cols, rows) = (self.cols, self.rows);
        if !matches!(op, 'm' | 'h' | 'l' | 'n' | 'c' | 'q' | 't') {
            self.pending_wrap = false;
        }
        match op {
            'A' => {
                self.y =
                    if self.y >= self.top { self.y.saturating_sub(n).max(self.top) } else { self.y.saturating_sub(n) }
            }
            'B' | 'e' => {
                self.y = if self.y <= self.bottom { (self.y + n).min(self.bottom) } else { (self.y + n).min(rows - 1) }
            }
            'C' | 'a' => self.x = (self.x + n).min(cols - 1),
            'D' => self.x = self.x.saturating_sub(n),
            'E' => {
                self.x = 0;
                self.y = (self.y + n).min(self.bottom.max(self.y));
            }
            'F' => {
                self.x = 0;
                self.y = self.y.saturating_sub(n);
            }
            'G' | '`' => self.x = (n - 1).min(cols - 1),
            'd' => self.y = (n - 1).min(rows - 1),
            'H' | 'f' => {
                self.y = (arg(0).max(1) - 1).min(rows - 1);
                self.x = (arg(1).max(1) - 1).min(cols - 1);
            }
            'J' => match arg(0) {
                0 => {
                    let y = self.y;
                    self.erase(y, self.x, cols);
                    for y in y + 1..rows {
                        self.erase(y, 0, cols);
                    }
                }
                1 => {
                    for y in 0..self.y {
                        self.erase(y, 0, cols);
                    }
                    let y = self.y;
                    self.erase(y, 0, self.x + 1);
                }
                2 | 3 => {
                    for y in 0..rows {
                        self.erase(y, 0, cols);
                    }
                    if arg(0) == 3 {
                        self.scrollback.clear();
                    }
                }
                _ => {}
            },
            'K' => {
                let y = self.y;
                match arg(0) {
                    0 => self.erase(y, self.x, cols),
                    1 => self.erase(y, 0, self.x + 1),
                    2 => self.erase(y, 0, cols),
                    _ => {}
                }
            }
            'X' => {
                let y = self.y;
                self.erase(y, self.x, self.x + n);
            }
            'P' => {
                let (x, y) = (self.x, self.y);
                let row = &mut self.grid_mut()[y].cells;
                for _ in 0..n.min(cols - x) {
                    row.remove(x);
                    row.push(' ');
                }
            }
            '@' => {
                let (x, y) = (self.x, self.y);
                let row = &mut self.grid_mut()[y].cells;
                for _ in 0..n.min(cols - x) {
                    row.insert(x, ' ');
                    row.pop();
                }
            }
            'L' | 'M' if (self.top..=self.bottom).contains(&self.y) => {
                let (top, y) = (self.top, self.y);
                self.top = y;
                if op == 'L' {
                    self.scroll_down(n);
                } else {
                    // Lines deleted inside the screen are not scrollback.
                    let in_alt = self.in_alt;
                    self.in_alt = true;
                    let (main, alt) = (std::mem::take(&mut self.main), std::mem::take(&mut self.alt));
                    if in_alt {
                        self.alt = alt;
                        self.main = main;
                    } else {
                        self.alt = main;
                        self.main = alt;
                    }
                    self.scroll_up(n);
                    if !in_alt {
                        std::mem::swap(&mut self.main, &mut self.alt);
                    }
                    self.in_alt = in_alt;
                }
                self.top = top;
                self.x = 0;
            }
            'S' => self.scroll_up(n),
            'T' => self.scroll_down(n),
            'r' => {
                let top = arg(0).max(1) - 1;
                let bottom = if arg(1) == 0 { rows } else { arg(1).min(rows) } - 1;
                if top < bottom {
                    (self.top, self.bottom) = (top, bottom);
                    (self.x, self.y) = (0, 0);
                }
            }
            's' => self.saved = (self.x, self.y),
            'u' => self.restore(),
            _ => {}
        }
    }
}

fn trim_blank_end(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
}

/// How many cells a char takes: 0 for one that combines with the one before it, 2 for an East
/// Asian wide one or an emoji, 1 for the rest. Close enough to place text on its row.
fn width(c: char) -> usize {
    let c = c as u32;
    match c {
        0x0300..=0x036F | 0x200B..=0x200F | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF => 0,
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F680..=0x1F6FF
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn screen(cols: u16, rows: u16, bytes: &[u8]) -> Screen {
        let mut screen = Screen::new(cols, rows);
        screen.feed(bytes);
        screen
    }
    #[test]
    fn lines_a_shell_wrote_are_rows() {
        let s = screen(20, 5, b"$ ls\r\na.txt\r\nb.txt\r\n$ ");
        assert_eq!(s.visible(), vec!["$ ls", "a.txt", "b.txt", "$"]);
    }
    #[test]
    fn a_full_screen_program_drawn_with_cursor_moves_is_one_row_a_line() {
        // What a TUI writes: no newlines, every row placed with CUP and cleared with EL.
        let s = screen(20, 4, b"\x1b[2J\x1b[1;1Htitle\x1b[K\x1b[2;1H\xe2\xa0\xa7 Working\x1b[K\x1b[4;1H> prompt");
        assert_eq!(s.visible(), vec!["title", "⠧ Working", "", "> prompt"]);
        // And redrawn: the row says what it says now, not both.
        let mut s = s;
        s.feed(b"\x1b[2;1H\xe2\xa0\x8f Done\x1b[K");
        assert_eq!(s.visible()[1], "⠏ Done");
    }
    #[test]
    fn relative_moves_and_carriage_returns_redraw_in_place() {
        let s = screen(20, 4, b"one\r\ntwo\r\nthree\x1b[2A\rONE!\x1b[2B\r");
        assert_eq!(s.visible(), vec!["ONE!", "two", "three"]);
    }
    #[test]
    fn what_scrolls_off_is_recent_and_the_alt_screen_is_not() {
        let s = screen(10, 2, b"a\r\nb\r\nc\r\n\x1b[?1049hFULL\x1b[?1049l");
        assert_eq!(s.visible(), vec!["c"]);
        assert_eq!(s.recent(), vec!["a", "b", "c"]);
        let full = screen(10, 2, b"a\r\n\x1b[?1049h\x1b[HFULL");
        assert_eq!(full.visible(), vec!["FULL"], "the alternate screen while it is up");
    }
    #[test]
    fn a_wrapped_line_is_rows_and_unwrapped_one_line() {
        let s = screen(4, 4, b"abcdefgh\r\nij");
        assert_eq!(s.visible(), vec!["abcd", "efgh", "ij"]);
        assert_eq!(s.unwrapped(), vec!["abcdefgh", "ij"]);
    }
    #[test]
    fn escapes_that_draw_nothing_leave_nothing() {
        let s = screen(20, 3, b"\x1b]0;title\x07\x1b[1;31mred\x1b[0m \x1bP+q\x1b\\ok\x1b(B");
        assert_eq!(s.visible(), vec!["red ok"]);
    }
    #[test]
    fn a_scroll_region_scrolls_inside_itself() {
        let s = screen(10, 4, b"head\x1b[2;3r\x1b[2;1Hx\r\ny\r\nz\x1b[r\x1b[4;1Hfoot");
        assert_eq!(s.visible(), vec!["head", "y", "z", "foot"]);
        assert!(s.recent().starts_with(&["head".to_string()]), "a region's scrolling is not scrollback");
    }
    #[test]
    fn wide_chars_take_two_cells() {
        let s = screen(6, 2, "日本語ab".as_bytes());
        assert_eq!(s.visible(), vec!["日本語", "ab"]);
    }
}
