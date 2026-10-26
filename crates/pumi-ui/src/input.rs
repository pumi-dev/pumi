use crate::ThemedBorder;
use crate::ime::{PlatformInput, PlatformText};
use crate::{StyledExt, color, icon_file};
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, EventEmitter, FocusHandle, Focusable, Hsla, KeyDownEvent, Pixels,
    Render, SharedString, TextRun, UnderlineStyle, Window, canvas, div, fill, point, prelude::*, px, size,
};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
const FONT_SIZE: f32 = 13.;
const COMPACT_FONT_SIZE: f32 = 12.;
pub enum InputEvent {
    Changed(String),
    Submitted(String),
}
pub struct TextInput {
    focus: FocusHandle,
    edit: LineEdit,
    marked: Option<String>,
    placeholder: SharedString,
    search: bool,
    compact: bool,
    submitted: String,
    caret: Rc<Cell<Bounds<Pixels>>>,
    scroll: Rc<Cell<f32>>,
    _blur: Option<gpui::Subscription>,
}
impl EventEmitter<InputEvent> for TextInput {}
impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl TextInput {
    pub fn new(text: &str, placeholder: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let blur = cx.on_blur(&focus, window, |this, _, cx| {
            this.marked = None;
            this.submit(cx);
            cx.notify();
        });
        Self {
            focus,
            edit: LineEdit { text: text.to_string(), cursor: text.len(), anchor: text.len() },
            marked: None,
            placeholder: placeholder.into(),
            search: false,
            compact: false,
            submitted: text.to_string(),
            caret: Rc::new(Cell::new(Bounds::default())),
            scroll: Rc::new(Cell::new(0.)),
            _blur: Some(blur),
        }
    }
    pub fn search(mut self) -> Self {
        self.search = true;
        self
    }
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }
    pub fn text(&self) -> &str {
        &self.edit.text
    }
    pub fn select_all(&mut self) {
        self.edit = LineEdit::new(&self.edit.text);
    }
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.edit.text != text {
            self.edit = LineEdit { text: text.to_string(), cursor: text.len(), anchor: text.len() };
            self.submitted = text.to_string();
            cx.notify();
        }
    }
    fn changed(&mut self, before: &str, cx: &mut Context<Self>) {
        if self.edit.text != before {
            cx.emit(InputEvent::Changed(self.edit.text.clone()));
        }
        cx.notify();
    }
    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.edit.text != self.submitted {
            self.submitted = self.edit.text.clone();
            cx.emit(InputEvent::Submitted(self.submitted.clone()));
        }
    }
    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let chord = crate::keys::chord(&event.keystroke);
        let before = self.edit.text.clone();
        if self.edit_key(&chord, cx) {
            cx.stop_propagation();
            self.changed(&before, cx);
        }
    }
    fn edit_key(&mut self, chord: &str, cx: &mut Context<Self>) -> bool {
        let edit = &mut self.edit;
        let (select, key) = match chord.split_once("shift+") {
            Some((mods, key)) => (true, format!("{mods}{key}")),
            None => (false, chord.to_string()),
        };
        let motion = match key.as_str() {
            "left" => Some(Motion::Left),
            "right" => Some(Motion::Right),
            "alt+left" => Some(Motion::WordLeft),
            "alt+right" => Some(Motion::WordRight),
            "cmd+left" | "home" | "up" => Some(Motion::Home),
            "cmd+right" | "end" | "down" => Some(Motion::End),
            _ => None,
        };
        if let Some(m) = motion {
            edit.motion(m, select);
            return true;
        }
        match chord {
            "backspace" => edit.delete(Motion::Left),
            "delete" => edit.delete(Motion::Right),
            "alt+backspace" => edit.delete(Motion::WordLeft),
            "cmd+backspace" => edit.delete(Motion::Home),
            "cmd+a" => *edit = LineEdit::new(&edit.text),
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
            // Enter always says so, changed or not: a field that closes on it closes.
            "enter" => {
                self.submitted = self.edit.text.clone();
                cx.emit(InputEvent::Submitted(self.submitted.clone()));
            }
            _ => return false,
        }
        true
    }
    fn field(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        let e = &self.edit;
        let mut text = e.text.clone();
        let m = self.marked.clone().unwrap_or_default();
        text.insert_str(e.cursor, &m);
        let marked = e.cursor..e.cursor + m.len();
        let shift = |i: usize| if i > e.cursor { i + m.len() } else { i };
        let sel = if focused { shift(e.selection().start)..shift(e.selection().end) } else { 0..0 };
        let caret = focused.then_some(marked.end);
        let placeholder = text.is_empty();
        if placeholder {
            text = self.placeholder.to_string();
        }
        let color: Hsla = if placeholder { color::placeholder().into() } else { color::foreground().into() };
        let font = window.text_style().font();
        let run = |len: usize, underline: bool| TextRun {
            len,
            font: font.clone(),
            color,
            background_color: None,
            underline: underline.then(UnderlineStyle::default),
            strikethrough: None,
        };
        let runs: Vec<TextRun> = if placeholder {
            vec![run(text.len(), false)]
        } else {
            [(0..marked.start, false), (marked.clone(), true), (marked.end..text.len(), false)]
                .into_iter()
                .filter(|(r, _)| !r.is_empty())
                .map(|(r, underline)| run(r.len(), underline))
                .collect()
        };
        let (scroll, caret_cell, focus) = (self.scroll.clone(), self.caret.clone(), self.focus.clone());
        let view = cx.entity();
        let font_size =
            crate::theme::metric("font_size") / 13. * if self.compact { COMPACT_FONT_SIZE } else { FONT_SIZE };
        canvas(
            move |_, window, _| window.text_system().shape_line(text.into(), px(font_size), &runs, None),
            move |b, line, window, cx| {
                window.with_content_mask(Some(ContentMask { bounds: b }), |window| {
                    let x = |i: usize| f32::from(line.x_for_index(i));
                    let width = f32::from(b.size.width);
                    let offset = match caret {
                        Some(c) if !placeholder => scroll.get().min(x(c)).max(x(c) - width + 2.).max(0.),
                        _ => 0.,
                    };
                    scroll.set(offset);
                    let origin = point(b.origin.x - px(offset), b.origin.y);
                    let line_height = b.size.height;
                    if !sel.is_empty() {
                        let r = Bounds::new(
                            point(origin.x + px(x(sel.start)), origin.y),
                            size(px(x(sel.end) - x(sel.start)), line_height),
                        );
                        window.paint_quad(fill(r, color::selection()));
                    }
                    let _ = line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx);
                    if let Some(c) = caret {
                        let c = if placeholder { 0 } else { c };
                        let at = Bounds::new(point(origin.x + px(x(c)), origin.y), size(px(1.5), line_height));
                        caret_cell.set(at);
                        if sel.is_empty() {
                            window.paint_quad(fill(at, color::ring()));
                        }
                        window.handle_input(&focus, PlatformInput::new(view.clone()), cx);
                    }
                })
            },
        )
        .flex_1()
        .min_w_0()
        .h(px(font_size + 5.))
        .overflow_hidden()
    }
}
impl PlatformText for TextInput {
    fn marked(&self) -> Option<&str> {
        self.marked.as_deref()
    }
    fn set_marked(&mut self, text: Option<String>) {
        self.marked = text;
    }
    fn commit(&mut self, text: &str, cx: &mut Context<Self>) {
        let before = self.edit.text.clone();
        self.edit.insert(text);
        self.changed(&before, cx);
    }
    fn caret_bounds(&self) -> Bounds<Pixels> {
        self.caret.get()
    }
}
impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus.is_focused(window);
        div()
            .id("text-input")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .hrow()
            .w_full()
            .gap(px(6.))
            .h(px(if self.compact { 20. } else { crate::theme::metric("control_height") }))
            .px(px(if self.compact { 4. } else { 8. }))
            .rounded(px(color::radius()))
            .border_themed()
            .border_color(color::input())
            .bg(color::background())
            .when(!focused, |d| d.hover(|s| s.border_color(color::input_hover())))
            .when(focused, crate::focus_ring)
            .cursor_text()
            .when(self.search, |d| d.child(icon_file("icons/magnifying_glass.svg", crate::ICON, color::placeholder())))
            .child(self.field(window, cx))
            .on_click(cx.listener(|this, _, window, cx| {
                window.focus(&this.focus, cx);
                cx.notify();
            }))
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineEdit {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
}
impl LineEdit {
    pub fn new(text: &str) -> Self {
        Self { text: text.into(), cursor: text.len(), anchor: 0 }
    }
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }
    pub fn selected(&self) -> &str {
        &self.text[self.selection()]
    }
    pub fn insert(&mut self, text: &str) {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        let r = self.selection();
        self.text.replace_range(r.clone(), &text);
        self.cursor = r.start + text.len();
        self.anchor = self.cursor;
    }
    fn target(&self, m: Motion) -> usize {
        let (t, at) = (&self.text, self.cursor);
        let prev = |i: usize| t[..i].char_indices().next_back().map_or(0, |(i, _)| i);
        let next = |i: usize| t[i..].chars().next().map_or(i, |c| i + c.len_utf8());
        let word = |c: char| c.is_alphanumeric();
        match m {
            Motion::Left => prev(at),
            Motion::Right => next(at),
            Motion::Home => 0,
            Motion::End => t.len(),
            Motion::WordLeft => {
                let mut i = at;
                while i > 0 && !t[prev(i)..].starts_with(word) {
                    i = prev(i);
                }
                while i > 0 && t[prev(i)..].starts_with(word) {
                    i = prev(i);
                }
                i
            }
            Motion::WordRight => {
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
    pub fn motion(&mut self, m: Motion, select: bool) {
        let r = self.selection();
        self.cursor = match m {
            Motion::Left if !select && !r.is_empty() => r.start,
            Motion::Right if !select && !r.is_empty() => r.end,
            _ => self.target(m),
        };
        if !select {
            self.anchor = self.cursor;
        }
    }
    pub fn delete(&mut self, m: Motion) {
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
    fn line_edit_moves_and_deletes() {
        let mut e = LineEdit::new("https://a.dev/path");
        assert_eq!(e.selected(), "https://a.dev/path");
        e.insert("ab cd-ef");
        assert_eq!((e.text.as_str(), e.cursor), ("ab cd-ef", 8));
        e.motion(Motion::WordLeft, false);
        assert_eq!(e.cursor, 6);
        e.motion(Motion::WordLeft, true);
        assert_eq!(e.selected(), "cd-");
        e.motion(Motion::Left, false);
        assert_eq!((e.cursor, e.selection().is_empty()), (3, true));
        e.delete(Motion::WordRight);
        assert_eq!(e.text, "ab -ef");
        e.motion(Motion::End, false);
        e.delete(Motion::Left);
        e.delete(Motion::WordLeft);
        assert_eq!(e.text, "ab -");
        e.motion(Motion::Home, true);
        assert_eq!(e.selected(), "ab -");
        e.delete(Motion::Left);
        assert_eq!(e.text, "");
        let mut e = LineEdit::default();
        e.insert("é\nx");
        e.motion(Motion::Left, false);
        e.motion(Motion::Left, false);
        assert_eq!(e.cursor, 0);
        assert_eq!(e.text, "éx");
    }
}
