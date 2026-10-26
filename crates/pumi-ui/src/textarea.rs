use crate::ThemedBorder;
use crate::ime::{PlatformInput, PlatformText};
use crate::input::InputEvent;
use crate::{StyledExt, color};
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, EventEmitter, FocusHandle, Focusable, Hsla, KeyDownEvent, Pixels,
    Render, SharedString, TextRun, UnderlineStyle, Window, canvas, div, fill, point, prelude::*, px, size,
};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
const FONT_SIZE: f32 = 12.;
const LINE_HEIGHT: f32 = 18.;
pub struct TextArea {
    focus: FocusHandle,
    edit: AreaEdit,
    marked: Option<String>,
    placeholder: SharedString,
    rows: usize,
    font: SharedString,
    caret: Rc<Cell<Bounds<Pixels>>>,
    top: Rc<Cell<usize>>,
}
impl EventEmitter<InputEvent> for TextArea {}
impl Focusable for TextArea {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl TextArea {
    pub fn new(text: &str, placeholder: impl Into<SharedString>, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            edit: AreaEdit { text: text.to_string(), cursor: 0, anchor: 0 },
            marked: None,
            placeholder: placeholder.into(),
            rows: 16,
            font: "Menlo".into(),
            caret: Rc::new(Cell::new(Bounds::default())),
            top: Rc::new(Cell::new(0)),
        }
    }
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }
    pub fn font(mut self, family: impl Into<SharedString>) -> Self {
        self.font = family.into();
        self
    }
    pub fn text(&self) -> &str {
        &self.edit.text
    }
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.edit.text != text {
            self.edit = AreaEdit { text: text.to_string(), cursor: 0, anchor: 0 };
            self.top.set(0);
            cx.notify();
        }
    }
    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let chord = crate::keys::chord(&event.keystroke);
        let before = self.edit.text.clone();
        if self.edit_key(&chord, cx) {
            cx.stop_propagation();
            if self.edit.text != before {
                cx.emit(InputEvent::Changed(self.edit.text.clone()));
            }
            cx.notify();
        }
    }
    fn edit_key(&mut self, chord: &str, cx: &mut Context<Self>) -> bool {
        let edit = &mut self.edit;
        let (select, key) = match chord.split_once("shift+") {
            Some((mods, key)) => (true, format!("{mods}{key}")),
            None => (false, chord.to_string()),
        };
        let motion = match key.as_str() {
            "left" => Some(Move::Left),
            "right" => Some(Move::Right),
            "alt+left" => Some(Move::WordLeft),
            "alt+right" => Some(Move::WordRight),
            "up" => Some(Move::Up),
            "down" => Some(Move::Down),
            "cmd+left" | "home" => Some(Move::LineStart),
            "cmd+right" | "end" => Some(Move::LineEnd),
            "cmd+up" => Some(Move::Start),
            "cmd+down" => Some(Move::End),
            _ => None,
        };
        if let Some(m) = motion {
            edit.motion(m, select);
            return true;
        }
        match chord {
            "backspace" => edit.delete(Move::Left),
            "delete" => edit.delete(Move::Right),
            "alt+backspace" => edit.delete(Move::WordLeft),
            "cmd+backspace" => edit.delete(Move::LineStart),
            "enter" => {
                let start = edit.line_start(edit.cursor);
                let indent: String = edit.text[start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                edit.insert(&format!("\n{indent}"));
            }
            "tab" => edit.insert("  "),
            "cmd+a" => {
                edit.anchor = 0;
                edit.cursor = edit.text.len();
            }
            "cmd+c" | "cmd+x" => {
                cx.write_to_clipboard(ClipboardItem::new_string(edit.selected().to_string()));
                if chord == "cmd+x" {
                    edit.insert("");
                }
            }
            "cmd+v" => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    edit.insert(&text);
                }
            }
            "cmd+s" => cx.emit(InputEvent::Submitted(edit.text.clone())),
            _ => return false,
        }
        true
    }
    fn field(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let e = &self.edit;
        let placeholder = e.text.is_empty() && self.marked.is_none();
        let mut font = window.text_style().font();
        font.family = self.font.clone();
        let fg: Hsla = color::foreground().into();
        let marked = self.marked.clone().unwrap_or_default();
        let sel = e.selection();
        let mut lines: Vec<(String, Vec<TextRun>, Option<usize>, Range<usize>)> = Vec::new();
        let source = if placeholder { self.placeholder.to_string() } else { e.text.clone() };
        let mut at = 0;
        for line in source.split('\n') {
            let range = at..at + line.len();
            at = range.end + 1;
            let mut text = line.to_string();
            let run = |len: usize, underline: bool, color: Hsla| TextRun {
                len,
                font: font.clone(),
                color,
                background_color: None,
                underline: underline.then(UnderlineStyle::default),
                strikethrough: None,
            };
            if placeholder {
                let runs = if text.is_empty() {
                    Vec::new()
                } else {
                    vec![run(text.len(), false, color::placeholder().into())]
                };
                lines.push((text, runs, (range.start == 0 && focused).then_some(0), 0..0));
                continue;
            }
            let has_caret = e.cursor >= range.start && e.cursor <= range.end;
            let mut caret = None;
            let mut runs = Vec::new();
            if has_caret {
                let c = e.cursor - range.start;
                text.insert_str(c, &marked);
                caret = focused.then_some(c + marked.len());
                for (len, underline) in [(c, false), (marked.len(), true), (line.len() - c, false)] {
                    if len > 0 {
                        runs.push(run(len, underline, fg));
                    }
                }
            } else if !text.is_empty() {
                runs.push(run(text.len(), false, fg));
            }
            let s = sel.start.max(range.start).min(range.end) - range.start;
            let t = sel.end.max(range.start).min(range.end) - range.start;
            let shift = |i: usize| if has_caret && i > e.cursor - range.start { i + marked.len() } else { i };
            let selected = if focused && !sel.is_empty() { shift(s)..shift(t) } else { 0..0 };
            lines.push((text, runs, caret, selected));
        }
        let caret_line = lines.iter().position(|l| l.2.is_some());
        let rows = self.rows;
        let top = self.top.clone();
        if let Some(line) = caret_line {
            let first = top.get();
            if line < first {
                top.set(line);
            } else if line >= first + rows {
                top.set(line + 1 - rows);
            }
        }
        let (caret_cell, focus) = (self.caret.clone(), self.focus.clone());
        let view = cx.entity();
        let first = top.get();
        canvas(
            move |_, window, _| {
                lines
                    .into_iter()
                    .skip(first)
                    .take(rows)
                    .map(|(text, runs, caret, selected)| {
                        let shaped = window.text_system().shape_line(text.into(), px(FONT_SIZE), &runs, None);
                        (shaped, caret, selected)
                    })
                    .collect::<Vec<_>>()
            },
            move |b, shaped, window, cx| {
                window.with_content_mask(Some(ContentMask { bounds: b }), |window| {
                    let line_height = px(LINE_HEIGHT);
                    for (i, (line, caret, selected)) in shaped.into_iter().enumerate() {
                        let origin = point(b.origin.x, b.origin.y + line_height * i as f32);
                        let x = |i: usize| line.x_for_index(i);
                        if !selected.is_empty() {
                            let r = Bounds::new(
                                point(origin.x + x(selected.start), origin.y),
                                size(x(selected.end) - x(selected.start), line_height),
                            );
                            window.paint_quad(fill(r, color::selection()));
                        }
                        let _ = line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx);
                        if let Some(c) = caret {
                            let at = Bounds::new(point(origin.x + x(c), origin.y), size(px(1.5), line_height));
                            caret_cell.set(at);
                            window.paint_quad(fill(at, color::ring()));
                        }
                    }
                    window.handle_input(&focus, PlatformInput::new(view.clone()), cx);
                })
            },
        )
        .w_full()
        .h(px(LINE_HEIGHT * rows as f32))
        .overflow_hidden()
    }
}
impl PlatformText for TextArea {
    fn marked(&self) -> Option<&str> {
        self.marked.as_deref()
    }
    fn set_marked(&mut self, text: Option<String>) {
        self.marked = text;
    }
    fn commit(&mut self, text: &str, cx: &mut Context<Self>) {
        let before = self.edit.text.clone();
        self.edit.insert(text);
        if self.edit.text != before {
            cx.emit(InputEvent::Changed(self.edit.text.clone()));
        }
        cx.notify();
    }
    fn caret_bounds(&self) -> Bounds<Pixels> {
        self.caret.get()
    }
}
impl Render for TextArea {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        div()
            .id("text-area")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .vcol()
            .w_full()
            .px(px(8.))
            .py(px(6.))
            .rounded(px(color::radius()))
            .border_themed()
            .border_color(color::input())
            .bg(color::background())
            .when(!focused, |d| d.hover(|s| s.border_color(color::input_hover())))
            .when(focused, crate::focus_ring)
            .cursor_text()
            .child(self.field(window, cx))
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus, cx);
                cx.notify();
            }))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    LineStart,
    LineEnd,
    Start,
    End,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AreaEdit {
    text: String,
    cursor: usize,
    anchor: usize,
}
impl AreaEdit {
    fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    fn selected(&self) -> &str {
        &self.text[self.selection()]
    }
    fn insert(&mut self, text: &str) {
        let text: String =
            text.replace("\r\n", "\n").chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect();
        let r = self.selection();
        self.text.replace_range(r.clone(), &text);
        self.cursor = r.start + text.len();
        self.anchor = self.cursor;
    }
    fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |i| i + 1)
    }
    fn line_end(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |i| at + i)
    }
    fn column(&self, at: usize) -> usize {
        self.text[self.line_start(at)..at].chars().count()
    }
    fn at_column(&self, start: usize, col: usize) -> usize {
        let end = self.line_end(start);
        self.text[start..end].char_indices().nth(col).map_or(end, |(i, _)| start + i)
    }
    fn target(&self, m: Move) -> usize {
        let (t, at) = (&self.text, self.cursor);
        let prev = |i: usize| t[..i].char_indices().next_back().map_or(0, |(i, _)| i);
        let next = |i: usize| t[i..].chars().next().map_or(i, |c| i + c.len_utf8());
        let word = |c: char| c.is_alphanumeric() || c == '_';
        match m {
            Move::Left => prev(at),
            Move::Right => next(at),
            Move::Start => 0,
            Move::End => t.len(),
            Move::LineStart => self.line_start(at),
            Move::LineEnd => self.line_end(at),
            Move::Up => {
                let start = self.line_start(at);
                if start == 0 {
                    return 0;
                }
                self.at_column(self.line_start(start - 1), self.column(at))
            }
            Move::Down => {
                let end = self.line_end(at);
                if end == t.len() {
                    return end;
                }
                self.at_column(end + 1, self.column(at))
            }
            Move::WordLeft => {
                let mut i = at;
                while i > 0 && !t[prev(i)..].starts_with(word) {
                    i = prev(i);
                }
                while i > 0 && t[prev(i)..].starts_with(word) {
                    i = prev(i);
                }
                i
            }
            Move::WordRight => {
                let mut i = at;
                while i < t.len() && !t[i..].starts_with(word) {
                    i = next(i);
                }
                while i < t.len() && t[i..].starts_with(word) {
                    i = next(i);
                }
                i
            }
        }
    }
    fn motion(&mut self, m: Move, select: bool) {
        let r = self.selection();
        self.cursor = match m {
            Move::Left if !select && !r.is_empty() => r.start,
            Move::Right if !select && !r.is_empty() => r.end,
            _ => self.target(m),
        };
        if !select {
            self.anchor = self.cursor;
        }
    }
    fn delete(&mut self, m: Move) {
        if self.selection().is_empty() {
            self.cursor = self.target(m);
        }
        self.insert("");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn area_edit_keeps_lines_and_moves_between_them() {
        let mut e = AreaEdit::default();
        e.insert("local a = 1\r\nprint(a)\n\tx");
        assert_eq!(e.text, "local a = 1\nprint(a)\n\tx");
        e.cursor = 3;
        e.anchor = 3;
        e.motion(Move::Down, false);
        assert_eq!(e.cursor, 12 + 3);
        e.motion(Move::Down, false);
        assert_eq!(e.cursor, e.text.len());
        e.motion(Move::Up, false);
        e.motion(Move::LineStart, false);
        assert_eq!(e.cursor, 12);
        e.motion(Move::LineEnd, true);
        assert_eq!(e.selected(), "print(a)");
        e.delete(Move::Left);
        assert_eq!(e.text, "local a = 1\n\n\tx");
        e.delete(Move::Left);
        assert_eq!(e.text, "local a = 1\n\tx");
    }
}
