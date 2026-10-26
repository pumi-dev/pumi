use super::grid::{Cell, Grid};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Point {
    pub row: u16,
    pub col: u16,
}

impl Point {
    pub fn new(col: u16, row: u16) -> Self {
        Self { row, col }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Cell,
    Word,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Point,
    pub head: Point,
    pub granularity: Granularity,
}

impl Selection {
    pub fn new(at: Point, granularity: Granularity) -> Self {
        Self { anchor: at, head: at, granularity }
    }

    pub fn bounds(&self, grid: &Grid) -> (Point, Point) {
        let (mut start, mut end) =
            if self.anchor <= self.head { (self.anchor, self.head) } else { (self.head, self.anchor) };
        match self.granularity {
            Granularity::Cell => {}
            Granularity::Word => {
                start.col = word_bounds(grid, start).0;
                end.col = word_bounds(grid, end).1;
            }
            Granularity::Line => {
                start.col = 0;
                end.col = grid.cols.saturating_sub(1);
            }
        }
        (start, end)
    }

    pub fn is_empty(&self) -> bool {
        self.granularity == Granularity::Cell && self.anchor == self.head
    }

    pub fn text(&self, grid: &Grid) -> String {
        let (start, end) = self.bounds(grid);
        let mut lines = Vec::new();
        for row in start.row..=end.row {
            let Some(cells) = grid.rows.get(usize::from(row)) else {
                break;
            };
            let first = if row == start.row { start.col } else { 0 };
            let last = if row == end.row { end.col } else { u16::MAX };
            let line: String = columns(cells)
                .filter(|(col, cell)| *col + width(cell) > first && *col <= last)
                .map(|(_, cell)| cell.text.as_str())
                .collect();
            lines.push(line.trim_end().to_string());
        }
        lines.join("\n")
    }
}

fn width(cell: &Cell) -> u16 {
    if cell.wide { 2 } else { 1 }
}

pub fn columns(row: &[Cell]) -> impl Iterator<Item = (u16, &Cell)> {
    let mut col = 0u16;
    row.iter().map(move |cell| {
        let at = col;
        col += width(cell);
        (at, cell)
    })
}

fn cell_at(row: &[Cell], col: u16) -> Option<(u16, &Cell)> {
    columns(row).find(|(at, cell)| *at <= col && col < *at + width(cell))
}

fn is_boundary(text: &str) -> bool {
    text.chars().all(|ch| {
        ch.is_whitespace()
            || matches!(ch, '"' | '\'' | '`' | '|' | ';' | ',' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '│')
    })
}

pub fn word_bounds(grid: &Grid, at: Point) -> (u16, u16) {
    let Some(row) = grid.rows.get(usize::from(at.row)) else {
        return (at.col, at.col);
    };
    let cells: Vec<(u16, &Cell)> = columns(row).collect();
    let Some(index) = cells.iter().position(|(col, cell)| *col <= at.col && at.col < *col + width(cell)) else {
        return (at.col, at.col);
    };
    let blank = |cell: &Cell| cell.text.trim().is_empty();
    let class = |cell: &Cell| (blank(cell), is_boundary(&cell.text));
    let target = class(cells[index].1);
    if target == (false, true) {
        let (col, cell) = cells[index];
        return (col, col + width(cell) - 1);
    }
    let mut first = index;
    while first > 0 && class(cells[first - 1].1) == target {
        first -= 1;
    }
    let mut last = index;
    while last + 1 < cells.len() && class(cells[last + 1].1) == target {
        last += 1;
    }
    let (end_col, end_cell) = cells[last];
    (cells[first].0, end_col + width(end_cell) - 1)
}

fn is_path_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '.' | '/' | '\\' | '~' | '-' | '_' | '+' | '@' | '%' | ':')
}

fn row_chars(row: &[Cell]) -> Vec<(u16, char)> {
    columns(row).flat_map(|(col, cell)| cell.text.chars().map(move |ch| (col, ch))).collect()
}

fn strip_file_scheme(token: &str) -> &str {
    token.strip_prefix("file://").unwrap_or(token)
}

fn strip_position(token: &str) -> &str {
    let mut token = token;
    for _ in 0..2 {
        let Some((head, tail)) = token.rsplit_once(':') else {
            break;
        };
        if tail.is_empty() || tail.len() > 6 || !tail.chars().all(|ch| ch.is_ascii_digit()) {
            break;
        }
        token = head;
    }
    token
}

fn looks_like_path(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if token.contains(['/', '\\']) || token.starts_with(['~', '.']) {
        return true;
    }
    let Some((name, extension)) = token.rsplit_once('.') else {
        return false;
    };
    name.len() >= 2
        && extension.len() <= 8
        && extension.starts_with(|ch: char| ch.is_alphabetic())
        && extension.chars().all(|ch: char| ch.is_alphanumeric())
}

fn run_at(chars: &[(u16, char)], index: usize) -> (usize, usize) {
    let start = chars[..index].iter().rposition(|(_, ch)| !is_path_char(*ch)).map_or(0, |index| index + 1);
    let end = chars[index..].iter().position(|(_, ch)| !is_path_char(*ch)).map_or(chars.len(), |offset| index + offset);
    (start, end)
}

fn as_path(text: &str) -> Option<String> {
    let token = strip_position(text.trim_end_matches(['.', ':']));
    let token = strip_file_scheme(token);
    looks_like_path(token).then(|| token.to_string())
}

const MAX_RUNS: usize = 10;

fn joined_runs(chars: &[(u16, char)], start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut runs = vec![(start, end)];
    let mut edge = start;
    while runs.len() < MAX_RUNS {
        let mut gap = edge;
        while gap > 0 && chars[gap - 1].1 == ' ' {
            gap -= 1;
        }
        if gap == edge || gap == 0 || !is_path_char(chars[gap - 1].1) {
            break;
        }
        let run = run_at(chars, gap - 1);
        edge = run.0;
        runs.insert(0, run);
    }
    let mut edge = end;
    while runs.len() < MAX_RUNS {
        let mut gap = edge;
        while gap < chars.len() && chars[gap].1 == ' ' {
            gap += 1;
        }
        if gap == edge || gap == chars.len() || !is_path_char(chars[gap].1) {
            break;
        }
        let run = run_at(chars, gap);
        edge = run.1;
        runs.push(run);
    }
    runs
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.clone().next().is_some_and(|next| !is_path_char(next)) {
            continue;
        }
        out.push(ch);
    }
    out
}

pub fn path_candidates_at(grid: &Grid, at: Point) -> Vec<String> {
    let Some((chars, index)) = under_pointer(grid, at) else { return Vec::new() };
    let (start, end) = run_at(&chars, index);
    let text = |start, end| chars[start..end].iter().map(|(_, ch)| *ch).collect::<String>();
    let mut found: Vec<String> = as_path(&text(start, end)).into_iter().collect();
    let runs = joined_runs(&chars, start, end);
    let pointer = runs.iter().position(|(start, end)| *start <= index && index < *end).unwrap_or(0);
    let mut wider: Vec<(usize, String)> = Vec::new();
    for left in 0..=pointer {
        for right in pointer..runs.len() {
            if (left, right) == (pointer, pointer) {
                continue;
            }
            let (start, _) = runs[left];
            let (_, end) = runs[right];
            let printed = text(start, end);
            for token in [unescape(&printed), printed] {
                if let Some(token) = as_path(&token) {
                    wider.push((end - start, token));
                }
            }
        }
    }
    wider.sort_by_key(|(width, _)| *width);
    for (_, token) in wider {
        if !found.contains(&token) {
            found.push(token);
        }
    }
    found
}

fn under_pointer(grid: &Grid, at: Point) -> Option<(Vec<(u16, char)>, usize)> {
    let row = grid.rows.get(usize::from(at.row))?;
    cell_at(row, at.col)?;
    let chars = row_chars(row);
    let index = chars.iter().rposition(|(col, _)| *col <= at.col)?;
    is_path_char(chars[index].1).then_some((chars, index))
}

pub fn url_at(grid: &Grid, at: Point) -> Option<String> {
    let row = grid.rows.get(usize::from(at.row))?;
    cell_at(row, at.col)?;
    let mut text = String::new();
    let mut byte_cols = Vec::new();
    for (col, cell) in columns(row) {
        for _ in 0..cell.text.len() {
            byte_cols.push(col);
        }
        text.push_str(&cell.text);
    }
    let delimiter = |ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '<' | '>');
    let mut start = 0;
    for token in text.split(delimiter) {
        let token_start = start;
        start += token.len() + 1;
        let Some(scheme) = ["https://", "http://", "file://"].iter().filter_map(|scheme| token.find(scheme)).min()
        else {
            continue;
        };
        let mut url = &token[scheme..];
        loop {
            let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
            let trimmed = match trimmed.chars().last() {
                Some(close @ (')' | ']' | '}')) => {
                    let open = match close {
                        ')' => '(',
                        ']' => '[',
                        _ => '{',
                    };
                    if trimmed.matches(open).count() < trimmed.matches(close).count() {
                        &trimmed[..trimmed.len() - 1]
                    } else {
                        trimmed
                    }
                }
                _ => trimmed,
            };
            if trimmed.len() == url.len() {
                break;
            }
            url = trimmed;
        }
        if url.len() <= "file://".len() {
            continue;
        }
        let first = token_start + scheme;
        let last = first + url.len() - 1;
        let (Some(&from), Some(&to)) = (byte_cols.get(first), byte_cols.get(last)) else {
            continue;
        };
        if from <= at.col && at.col <= to {
            return Some(url.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path_at(grid: &Grid, at: Point) -> Option<String> {
        let (chars, index) = under_pointer(grid, at)?;
        let (start, end) = run_at(&chars, index);
        as_path(&chars[start..end].iter().map(|(_, ch)| *ch).collect::<String>())
    }

    fn grid(lines: &[&str]) -> Grid {
        Grid::of_lines(40, lines)
    }

    #[test]
    fn drag_selection_spans_rows_and_trims() {
        let grid = grid(&["hello world   ", "second line   "]);
        let mut selection = Selection::new(Point::new(6, 0), Granularity::Cell);
        selection.head = Point::new(5, 1);
        assert_eq!(selection.text(&grid), "world\nsecond");
        let reversed = Selection { anchor: selection.head, head: selection.anchor, ..selection };
        assert_eq!(reversed.text(&grid), "world\nsecond");
    }

    #[test]
    fn word_and_line_granularity() {
        let grid = grid(&["ls src/main.rs:12 (x)"]);
        let word = Selection::new(Point::new(6, 0), Granularity::Word);
        assert_eq!(word.text(&grid), "src/main.rs:12");
        let paren = Selection::new(Point::new(18, 0), Granularity::Word);
        assert_eq!(paren.text(&grid), "(");
        let line = Selection::new(Point::new(3, 0), Granularity::Line);
        assert_eq!(line.text(&grid), "ls src/main.rs:12 (x)");
    }

    #[test]
    fn wide_glyphs_cover_two_columns() {
        let grid = grid(&["a界b"]);
        let mut selection = Selection::new(Point::new(2, 0), Granularity::Cell);
        selection.head = Point::new(3, 0);
        assert_eq!(selection.text(&grid), "界b");
    }

    #[test]
    fn finds_urls_and_trims_punctuation() {
        let grid = grid(&["see (https://x.dev/a_(b)). ok"]);
        assert_eq!(url_at(&grid, Point::new(10, 0)).as_deref(), Some("https://x.dev/a_(b)"));
        assert_eq!(url_at(&grid, Point::new(1, 0)), None);
        assert_eq!(url_at(&grid, Point::new(26, 0)), None);
    }

    fn pointing(line: &str, needle: &str) -> Point {
        let at = line.find(needle).expect("the needle in the line");
        let col = line[..at].chars().map(|ch| if ch == '界' { 2 } else { 1 }).sum();
        Point::new(col, 0)
    }

    #[test]
    fn finds_a_path_and_drops_its_position() {
        let line = "error[E0308]: src/main.rs:12:5: mismatched types";
        let grid = grid(&[line]);
        assert_eq!(path_at(&grid, pointing(line, "src/main.rs")).as_deref(), Some("src/main.rs"));
        assert_eq!(path_at(&grid, pointing(line, "12:5")).as_deref(), Some("src/main.rs"));
        assert_eq!(path_at(&grid, pointing(line, "mismatched")), None);
        let gap = pointing(line, "src").col - 1;
        assert_eq!(path_at(&grid, Point::new(gap, 0)), None);
    }

    #[test]
    fn a_path_is_read_out_of_the_punctuation_around_it() {
        let line = "see (./src/main.rs), and /tmp/x.log.";
        let grid = grid(&[line]);
        assert_eq!(path_at(&grid, pointing(line, "./src")).as_deref(), Some("./src/main.rs"));
        assert_eq!(path_at(&grid, pointing(line, "x.log")).as_deref(), Some("/tmp/x.log"));
    }

    #[test]
    fn quoted_home_and_file_urls_are_paths() {
        let line = "open \"~/notes/a.md\" then file:///tmp/b.rs";
        let grid = grid(&[line]);
        assert_eq!(path_at(&grid, pointing(line, "~/notes")).as_deref(), Some("~/notes/a.md"));
        assert_eq!(path_at(&grid, pointing(line, "b.rs")).as_deref(), Some("/tmp/b.rs"));
    }

    #[test]
    fn a_wide_glyph_before_a_path_does_not_shift_it() {
        let line = "界 é src/main.rs";
        let grid = grid(&[line]);
        assert_eq!(path_at(&grid, pointing(line, "src/main.rs")).as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn numbers_and_abbreviations_are_not_paths() {
        let line = "took 1.5s at v0.2.2, e.g";
        let grid = grid(&[line]);
        assert_eq!(path_at(&grid, pointing(line, "1.5s")), None);
        assert_eq!(path_at(&grid, pointing(line, "v0.2.2")), None);
        assert_eq!(path_at(&grid, pointing(line, "e.g")), None);
    }

    #[test]
    fn a_path_with_spaces_is_offered_a_word_at_a_time() {
        let line = "cp /home/user/shots/Screen Shot 1 at 5.09 PM.png /tmp/";
        let grid = grid(&[line]);
        let whole = "/home/user/shots/Screen Shot 1 at 5.09 PM.png";
        let on_name = path_candidates_at(&grid, pointing(line, "PM.png"));
        assert_eq!(path_at(&grid, pointing(line, "PM.png")).as_deref(), Some("PM.png"));
        assert!(on_name.contains(&whole.to_string()));
        assert_eq!(path_at(&grid, pointing(line, "Shot")), None);
        assert!(path_candidates_at(&grid, pointing(line, "Shot")).contains(&whole.to_string()));
        assert!(path_candidates_at(&grid, pointing(line, "5.09")).contains(&whole.to_string()));
    }

    #[test]
    fn a_quoted_or_escaped_path_with_spaces_reads_as_one() {
        let quoted = "open '/home/user/My Shot.png' ok";
        let escaped = "cat /home/user/My\\ Shot.png";
        let whole = "/home/user/My Shot.png";
        let tokens = path_candidates_at(&grid(&[quoted]), pointing(quoted, "Shot.png"));
        assert_eq!(tokens.first().map(String::as_str), Some("Shot.png"));
        assert!(tokens.contains(&whole.to_string()));
        let tokens = path_candidates_at(&grid(&[escaped]), pointing(escaped, "Shot.png"));
        assert_eq!(tokens.first().map(String::as_str), Some("Shot.png"));
        assert!(tokens.contains(&whole.to_string()));
    }

    #[test]
    fn the_wider_spans_come_after_the_pointer_s_own() {
        let line = "cp shot.png notes.txt";
        let grid = grid(&[line]);
        let tokens = path_candidates_at(&grid, pointing(line, "shot.png"));
        assert_eq!(tokens.first().map(String::as_str), Some("shot.png"));
        assert!(tokens.contains(&"shot.png notes.txt".to_string()));
        assert_eq!(path_at(&grid, pointing(line, "notes.txt")).as_deref(), Some("notes.txt"));
    }
}
