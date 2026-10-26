use gpui::{App, Bounds, Context, Entity, InputHandler, Pixels, UTF16Selection, Window};
use std::ops::Range;
pub trait PlatformText: Sized + 'static {
    fn marked(&self) -> Option<&str>;
    fn set_marked(&mut self, text: Option<String>);
    fn commit(&mut self, text: &str, cx: &mut Context<Self>);
    fn caret_bounds(&self) -> Bounds<Pixels>;
}
pub struct PlatformInput<T: PlatformText> {
    view: Entity<T>,
}
impl<T: PlatformText> PlatformInput<T> {
    pub fn new(view: Entity<T>) -> Self {
        Self { view }
    }
}
impl<T: PlatformText> InputHandler for PlatformInput<T> {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        let len = utf16_len(self.view.read(cx).marked().unwrap_or(""));
        Some(UTF16Selection { range: len..len, reversed: false })
    }
    fn marked_text_range(&mut self, _: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        let marked = self.view.read(cx).marked()?;
        Some(0..utf16_len(marked))
    }
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        let marked = self.view.read(cx).marked().unwrap_or("").to_string();
        let units: Vec<u16> = marked.encode_utf16().collect();
        let end = range_utf16.end.min(units.len());
        let start = range_utf16.start.min(end);
        *adjusted_range = Some(start..end);
        Some(String::from_utf16_lossy(&units[start..end]))
    }
    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, cx| {
            view.set_marked(None);
            if !text.is_empty() {
                view.commit(text, cx);
            }
            cx.notify();
        });
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, cx| {
            view.set_marked((!new_text.is_empty()).then(|| new_text.to_string()));
            cx.notify();
        });
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.set_marked(None);
            cx.notify();
        });
    }
    fn bounds_for_range(&mut self, _range_utf16: Range<usize>, _: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        Some(self.view.read(cx).caret_bounds())
    }
    fn character_index_for_point(&mut self, _point: gpui::Point<Pixels>, _: &mut Window, _: &mut App) -> Option<usize> {
        None
    }
    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }
}
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}
