//! `<text-input>`: the platform's text input for an editor JS draws itself.
//!
//! A `<div>` with `onKeyDown` hears keystrokes, but the text a keyboard makes does not always come
//! from one keystroke: a dead key (`⌥E` then `e` is `é`), press-and-hold accents, an input method
//! composing Japanese or Chinese, the emoji picker, autocorrect and dictation all reach the window
//! through the platform's text input client, which only a focused element that registered an input
//! handler receives. This element is that handler and nothing else: it draws its children, takes
//! focus (a press in it, `autoFocus`), and reports what the platform types as events. The document,
//! its drawing, the caret and the selection stay in JS; native keeps only the text being composed.
//!
//! Props:
//! - `text`: the text around the caret that the platform may read, any window of the document JS
//!   chooses (the line, the paragraph, all of it). Every range below is in it, in UTF-16 units.
//!   JS hears what is typed after the platform has moved on, and the platform can type twice in
//!   one keystroke (a Korean syllable committed and the next one begun) before JS sends `text`
//!   back, so the element applies each `textInput` to its own copy (the caret after the inserted
//!   text) and the next event's range is in that. An editor that applies each `textInput` as
//!   given and keeps its window where it is has the same text; a `text` or `selection` from JS
//!   that is not one the element made replaces its copy.
//! - `selection`: `[start, end]` in `text`, the selection or the caret (`start == end`).
//! - `caret`: `{x, y, width, height}` in the element's own box, where the input method puts its
//!   window.
//! - `autoFocus`: focuses it each time it turns true, as a terminal's does.
//!
//! Events:
//! - `textInput`: `value` replaces `startIndex..endIndex` of `text` (always given: the selection
//!   when the platform names no range, another range when it replaces a letter just typed or a
//!   word autocorrect fixed).
//! - `composition`: the text an input method is composing (`value`), to draw over
//!   `startIndex..endIndex` of `text`, with the part it has selected in it
//!   (`selectionStart..selectionEnd` of `value`). An empty `value` ends it with nothing typed; a
//!   `textInput` ends it with what was.
//! - `keyDown`/`keyUp`, `focus`/`blur`, `click`, `mouseEnter`/`mouseLeave`, `fileDrop`: as on a
//!   `<div>`. A key that types text arrives as a `keyDown` and then a `textInput` (only the
//!   `textInput` when an input method takes the key), so an editor inserts text from `textInput`
//!   only.
use std::collections::VecDeque;
use std::ops::Range;

use gpui::{
    canvas, div, prelude::*, px, size, Bounds, Context, Entity, EntityInputHandler, Pixels, Point, UTF16Selection,
    Window,
};

use super::{CustomElement, CustomElementFactory, CustomRenderContext};
use crate::renderer::{emit, emit_key_event, EventCallback};
use pumi_ui_proto::{EventKind, Selection};

pub struct TextInputFactory;

impl CustomElementFactory for TextInputFactory {
    fn element_type(&self) -> &str {
        "text-input"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(TextInputElement::default())
    }
}

#[derive(Default)]
struct TextInputElement {
    /// The caret's box in the element's own coordinates, from the `caret` prop.
    caret: Option<Bounds<Pixels>>,
    text: String,
    selection: Range<usize>,
    /// `text` or `selection` changed since the last frame.
    synced: bool,
    auto_focus: bool,
    /// `autoFocus` turned true since the last frame: the next one focuses the element.
    focus_pending: bool,
    state: Option<Entity<TextInputState>>,
}

/// What the platform's input client sees: `text` (from JS, with the `textInput`s since applied)
/// with the text being composed in place of what it covers. Ranges the platform names are in that
/// document; the events JS hears are in `text`.
#[derive(Default)]
struct TextInputState {
    element_id: u64,
    callback: Option<EventCallback>,
    emits_text_input: bool,
    emits_composition: bool,
    text: String,
    /// The selection in `text` (UTF-16), clamped to it.
    selection: Range<usize>,
    caret: Option<Bounds<Pixels>>,
    /// The text being composed, the part of `text` it covers, and the part the input method selected
    /// in it (UTF-16). `covers` is empty while nothing is composed.
    marked: String,
    covers: Range<usize>,
    marked_selection: Range<usize>,
    /// The `text` and `selection` each `textInput` made, oldest first, until JS sends them back.
    pending: VecDeque<(String, Range<usize>)>,
}

/// How many `textInput`s JS may be behind before the oldest is forgotten.
const MAX_PENDING: usize = 32;

/// UTF-16 length of `text`.
fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// The byte offset of UTF-16 offset `offset` in `text`, clamped to its end and to a char boundary.
fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16();
    }
    text.len()
}

fn caret_prop(value: &serde_json::Value) -> Option<Bounds<Pixels>> {
    let get = |key: &str| value.get(key).and_then(serde_json::Value::as_f64).map(|n| n as f32);
    let (x, y) = (get("x")?, get("y")?);
    let width = get("width").unwrap_or(1.0);
    let height = get("height").unwrap_or(16.0);
    Some(Bounds::new(gpui::point(px(x), px(y)), size(px(width), px(height))))
}

/// `[start, end]`, in either order; the caret at 0 for anything else.
fn selection_prop(value: &serde_json::Value) -> Range<usize> {
    let at = |i: usize| value.get(i).and_then(serde_json::Value::as_f64).map(|n| n.max(0.0) as usize);
    match (at(0), at(1)) {
        (Some(a), Some(b)) => a.min(b)..a.max(b),
        (Some(a), None) => a..a,
        _ => 0..0,
    }
}

impl TextInputState {
    fn composing(&self) -> bool {
        !self.marked.is_empty()
    }

    /// What the platform reads: `text` with the composition in place of what it covers.
    fn document(&self) -> String {
        if !self.composing() {
            return self.text.clone();
        }
        let (start, end) = (utf16_to_byte(&self.text, self.covers.start), utf16_to_byte(&self.text, self.covers.end));
        format!("{}{}{}", &self.text[..start], self.marked, &self.text[end..])
    }

    /// Where the composition sits in the document, or the selection when there is none.
    fn marked_range(&self) -> Range<usize> {
        if self.composing() {
            self.covers.start..self.covers.start + utf16_len(&self.marked)
        } else {
            self.selection.clone()
        }
    }

    /// A position in the document as a position in `text`: inside the composition is its start.
    fn to_text(&self, at: usize) -> usize {
        if !self.composing() {
            return at.min(utf16_len(&self.text));
        }
        let marked = self.marked_range();
        if at <= marked.start {
            at
        } else if at < marked.end {
            self.covers.start
        } else {
            at - marked.len() + self.covers.len()
        }
    }

    /// A range the platform names, in `text`; the composition, or the selection, without one.
    fn range_in_text(&self, range: Option<Range<usize>>) -> Range<usize> {
        match range {
            Some(range) => self.to_text(range.start)..self.to_text(range.end).max(self.to_text(range.start)),
            None if self.composing() => self.covers.clone(),
            None => self.selection.clone(),
        }
    }

    fn emit(&self, event: &str, value: &str, range: Range<usize>, selected: Option<Range<usize>>) {
        let wanted = match event {
            "textInput" => self.emits_text_input,
            _ => self.emits_composition,
        };
        if !wanted {
            return;
        }
        let (value, start, end) = (value.to_string(), range.start as u64, range.end as u64);
        let selection = selected.map(|s| Selection { start: s.start as u64, end: s.end as u64 });
        emit(&self.callback, self.element_id, || match event {
            "textInput" => EventKind::TextInput { value, start, end, selection },
            _ => EventKind::Composition { value, start, end, selection },
        });
    }

    /// JS's `text` and `selection`: one a `textInput` made (JS caught up to it; later ones stand),
    /// or JS's own, which replace the element's.
    fn sync(&mut self, text: &str, selection: Range<usize>) {
        if let Some(i) = self.pending.iter().position(|(t, s)| t == text && *s == selection) {
            self.pending.drain(..=i);
            return;
        }
        self.pending.clear();
        let len = utf16_len(text);
        self.text = text.to_string();
        self.selection = selection.start.min(len)..selection.end.min(len);
        // JS keeps `text` as it is while something is composed (it draws the composition over the
        // range it covers); a `text` that changes anyway keeps the composition inside it.
        self.covers = self.covers.start.min(len)..self.covers.end.min(len);
    }

    /// Applies a `textInput` to the element's `text`, as JS will: `value` over `range`, the caret
    /// after it.
    fn apply(&mut self, range: Range<usize>, value: &str) {
        let (start, end) = (utf16_to_byte(&self.text, range.start), utf16_to_byte(&self.text, range.end));
        self.text.replace_range(start..end, value);
        let caret = range.start + utf16_len(value);
        self.selection = caret..caret;
        self.pending.push_back((self.text.clone(), self.selection.clone()));
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
    }

    fn clear_composition(&mut self) {
        self.marked.clear();
        self.covers = 0..0;
        self.marked_selection = 0..0;
    }

    /// Commits `text`, which replaces `range` of the document (UTF-16) or, without one, the
    /// composition (or the selection, when nothing is composed).
    fn commit(&mut self, range: Option<Range<usize>>, text: &str) {
        let composing = self.composing();
        let replaced = self.range_in_text(range);
        self.clear_composition();
        if text.is_empty() && replaced.is_empty() {
            if composing {
                self.emit("composition", "", replaced, None);
            }
            return;
        }
        self.emit("textInput", text, replaced.clone(), None);
        self.apply(replaced, text);
    }

    /// Starts or changes the composition: `text` over `range` of the document (the composition, or
    /// the selection, without one).
    fn compose(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>) {
        let covers = self.range_in_text(range);
        if text.is_empty() {
            let was = self.composing();
            self.clear_composition();
            if was {
                self.emit("composition", "", covers, None);
            }
            return;
        }
        let len = utf16_len(text);
        self.marked = text.to_string();
        self.covers = covers.clone();
        self.marked_selection = selected.map(|r| r.start.min(len)..r.end.min(len)).unwrap_or(len..len);
        self.emit("composition", text, covers, Some(self.marked_selection.clone()));
    }
}

impl EntityInputHandler for TextInputState {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let document = self.document();
        let len = utf16_len(&document);
        let range = range.start.min(len)..range.end.min(len).max(range.start.min(len));
        *adjusted_range = Some(range.clone());
        Some(document[utf16_to_byte(&document, range.start)..utf16_to_byte(&document, range.end)].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = if self.composing() {
            let at = self.covers.start;
            at + self.marked_selection.start..at + self.marked_selection.end
        } else {
            self.selection.clone()
        };
        Some(UTF16Selection { range, reversed: false })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.composing().then(|| self.marked_range())
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // The platform accepts the composition as it stands.
        if self.composing() {
            let text = self.marked.clone();
            self.commit(None, &text);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.commit(range, text);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.compose(range, new_text, new_selected_range);
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let caret = self.caret.unwrap_or(Bounds::new(Point::default(), size(px(1.0), px(16.0))));
        Some(Bounds::new(element_bounds.origin + caret.origin, caret.size))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn text_length_utf16(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<usize> {
        Some(utf16_len(&self.document()))
    }
}

impl CustomElement for TextInputElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut Window,
        cx: &mut Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        let focus_handle = ctx.focus_handle.cloned().unwrap_or_else(|| cx.focus_handle());
        if std::mem::take(&mut self.focus_pending) {
            window.focus(&focus_handle, cx);
        }
        let state = self.state.get_or_insert_with(|| cx.new(|_| TextInputState::default())).clone();
        let (callback, caret) = (ctx.event_callback.clone(), self.caret);
        let synced = std::mem::take(&mut self.synced);
        let (text, selection) = (&self.text, self.selection.clone());
        state.update(cx, |state, _| {
            state.element_id = ctx.id;
            state.callback = callback;
            state.emits_text_input = ctx.events.contains("textInput");
            state.emits_composition = ctx.events.contains("composition");
            if synced {
                state.sync(text, selection);
            }
            state.caret = caret;
        });

        let id = gpui::SharedString::from(format!("__gpuix_text_input_{}", ctx.id));
        let mut el = div().id(id).track_focus(&focus_handle);
        if let Some(style) = ctx.style {
            el = crate::renderer::apply_interactive_styles(el, style);
        }
        if ctx.style.and_then(|style| style.position.as_deref()).is_none() {
            el = el.relative();
        }
        el = crate::renderer::apply_focus_visible(el, ctx.id, ctx.style, ctx.keyboard_focus.as_deref());
        // A press in it moves the caret, which JS draws: it starts no window text selection.
        el = ctx.selects_itself(el);
        el = crate::accessibility::apply_accessibility(el, ctx.props, Some(gpui::Role::MultilineTextInput));
        el = super::wire_standard_events(el, &ctx);
        for (event, down) in [("keyDown", true), ("keyUp", false)] {
            if !ctx.events.contains(event) {
                continue;
            }
            let (callback, id) = (ctx.event_callback.clone(), ctx.id);
            el = if down {
                el.on_key_down(move |e, _, _| emit_key_event(&callback, id, EventKind::KeyDown, &e.keystroke, Some(e.is_held)))
            } else {
                el.on_key_up(move |e, _, _| emit_key_event(&callback, id, EventKind::KeyUp, &e.keystroke, None))
            };
        }
        // The input handler is registered for the element's box each frame it paints, and the
        // platform asks it only while the element has focus.
        let handler = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                window.handle_input(&focus_handle, gpui::ElementInputHandler::new(bounds, state), cx);
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        el.children(ctx.children).child(handler).into_any_element()
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "caret" => self.caret = caret_prop(&value),
            "text" => {
                self.text = value.as_str().unwrap_or_default().to_string();
                self.synced = true;
            }
            "selection" => {
                self.selection = selection_prop(&value);
                self.synced = true;
            }
            "autoFocus" => {
                let on = value.as_bool().unwrap_or(false);
                self.focus_pending |= on && !self.auto_focus;
                self.auto_focus = on;
            }
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &["text", "selection", "caret", "autoFocus"]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &[
            "textInput",
            "composition",
            "keyDown",
            "keyUp",
            "focus",
            "blur",
            "click",
            "mouseEnter",
            "mouseLeave",
            "fileDrop",
            "mouseDownOutside",
        ]
    }

    fn destroy(&mut self) {
        self.state = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets_land_on_char_boundaries() {
        let text = "a😀é";
        assert_eq!(utf16_len(text), 4);
        assert_eq!(utf16_to_byte(text, 1), 1);
        assert_eq!(utf16_to_byte(text, 3), 5);
        assert_eq!(utf16_to_byte(text, 9), text.len());
    }

    type Seen = std::sync::Arc<std::sync::Mutex<Vec<(String, Option<String>, Option<f64>, Option<f64>)>>>;

    fn state(text: &str, selection: Range<usize>) -> (TextInputState, Seen) {
        let seen: Seen = Default::default();
        let sink = seen.clone();
        let state = TextInputState {
            emits_text_input: true,
            emits_composition: true,
            text: text.into(),
            selection,
            callback: Some(std::sync::Arc::new(move |event: pumi_ui_proto::Event| {
                let (kind, value, start, end) = match event.kind {
                    pumi_ui_proto::EventKind::TextInput { value, start, end, .. } => ("textInput", value, start, end),
                    pumi_ui_proto::EventKind::Composition { value, start, end, .. } => ("composition", value, start, end),
                    other => (other.name(), String::new(), 0, 0),
                };
                sink.lock().unwrap().push((kind.to_string(), Some(value), Some(start as f64), Some(end as f64)));
            })),
            ..Default::default()
        };
        (state, seen)
    }

    fn event(kind: &str, value: &str, start: usize, end: usize) -> (String, Option<String>, Option<f64>, Option<f64>) {
        (kind.into(), Some(value.into()), Some(start as f64), Some(end as f64))
    }

    #[test]
    fn typed_text_replaces_the_selection_and_says_where() {
        let (mut s, seen) = state("hello world", 6..11);
        s.commit(None, "there");
        assert_eq!(seen.lock().unwrap()[0], event("textInput", "there", 6, 11));
    }

    #[test]
    fn an_accent_replaces_the_letter_before_the_caret() {
        let (mut s, seen) = state("cafe and more", 4..4);
        s.commit(Some(3..4), "é");
        assert_eq!(seen.lock().unwrap()[0], event("textInput", "é", 3, 4));
    }

    #[test]
    fn a_composition_covers_the_selection_and_commits_over_it() {
        let (mut s, seen) = state("ab cd", 3..5);
        s.compose(None, "に", Some(1..1));
        // The platform sees the composition in place of what it covers.
        assert_eq!(s.document(), "ab に");
        assert_eq!(s.marked_range(), 3..4);
        // It commits over its own range in the document, which is 3..5 of `text`.
        s.commit(Some(3..4), "日");
        assert!(!s.composing());
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], event("composition", "に", 3, 5));
        assert_eq!(seen[1], event("textInput", "日", 3, 5));
    }

    #[test]
    fn positions_after_a_composition_map_back_into_the_text() {
        let (mut s, _) = state("ab cd ef", 3..5);
        s.compose(None, "xyz", None);
        // `ab xyz ef`: 7 in the document is 6 in the text.
        assert_eq!(s.to_text(7), 6);
        assert_eq!(s.to_text(4), 3);
    }

    #[test]
    fn text_typed_before_js_answers_lands_after_the_text_before_it() {
        // A Korean input method commits a syllable and starts the next in one keystroke.
        let (mut s, seen) = state("ab", 2..2);
        s.compose(None, "하", None);
        s.commit(None, "한");
        s.compose(None, "ㄱ", None);
        assert_eq!(s.document(), "ab한ㄱ");
        s.commit(None, "각");
        let seen = seen.lock().unwrap();
        assert_eq!(seen[1], event("textInput", "한", 2, 2));
        assert_eq!(seen[2], event("composition", "ㄱ", 3, 3));
        assert_eq!(seen[3], event("textInput", "각", 3, 3));
    }

    #[test]
    fn js_catching_up_keeps_what_was_typed_since_and_its_own_text_replaces_it() {
        let (mut s, _) = state("", 0..0);
        s.commit(None, "a");
        s.commit(None, "b");
        // JS sends back the text after `a`: `b` stands.
        s.sync("a", 1..1);
        assert_eq!((s.text.as_str(), s.selection.clone()), ("ab", 2..2));
        s.sync("ab", 2..2);
        assert!(s.pending.is_empty());
        // JS moved the caret itself.
        s.sync("ab", 0..0);
        s.commit(None, "c");
        assert_eq!(s.text, "cab");
        // JS refused `c`, and says so with its own text.
        s.sync("xy", 1..1);
        assert_eq!((s.text.as_str(), s.selection.clone()), ("xy", 1..1));
    }

    #[test]
    fn a_cancelled_composition_says_so_with_nothing_typed() {
        let (mut s, seen) = state("ab", 2..2);
        s.compose(None, "x", None);
        s.commit(None, "");
        let seen = seen.lock().unwrap();
        assert_eq!(seen[1], event("composition", "", 2, 2));
    }
}
