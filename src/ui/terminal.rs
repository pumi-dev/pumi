use crate::settings::{CursorBlink, Settings};
use crate::term::selection::{self, Granularity, Point, Selection};
use crate::term::{self, CursorShape, Grid};
use crate::theme;
use crate::ui::ime::{PlatformInput, PlatformText};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Bounds, ClipboardEntry, ClipboardItem, Context, DispatchPhase, EventEmitter, ExternalPaths, FocusHandle,
    Focusable, Font, FontStyle, FontWeight, Hsla, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Render, ScrollDelta, ScrollWheelEvent, Window, canvas, div,
    point, prelude::*, px, rgb, size,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
pub(crate) mod paint;
mod sprite;
const SCROLL_LINES: u16 = 3;
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
const HELD_RECHECK: Duration = Duration::from_millis(100);
/// The least time between two sizes the program is told: a tab that moves or is dragged is laid
/// out every frame, and the program follows it live — told the first size at once, then at most one
/// a frame, and always the last — so it re-lays itself out as the tab moves, not once at the end.
const SETTLE: Duration = Duration::from_millis(16);
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    pub(crate) cell_width: f32,
    pub(crate) line_height: f32,
}
#[derive(Debug, Clone, Copy)]
struct Placement {
    origin_x: f32,
    origin_y: f32,
    pad_x: f32,
    pad_y: f32,
    cols: u16,
    rows: u16,
    metrics: Metrics,
}
impl Default for Placement {
    fn default() -> Self {
        Self {
            origin_x: 0.0,
            origin_y: 0.0,
            pad_x: 0.0,
            pad_y: 0.0,
            cols: 0,
            rows: 0,
            metrics: Metrics { cell_width: 1.0, line_height: 1.0 },
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct Press {
    button: MouseButton,
    forwarded: bool,
    selecting: bool,
}
pub struct TerminalView {
    session: Rc<term::TerminalSession>,
    grid: Arc<Grid>,
    focus: FocusHandle,
    placement: Rc<Cell<Placement>>,
    press: Option<Press>,
    selection: Option<Selection>,
    marked: Option<String>,
    scroll: term::scroll::Accumulator,
    blink_on: bool,
    last_input: Instant,
    focused: bool,
    painter: Rc<RefCell<paint::Cache>>,
    held: gpui::Modifiers,
    recheck: bool,
    /// The size the tab's box asks for, and the one the program was last told. Sizes are sent
    /// as they change, at most one a frame (see [`SETTLE`]).
    wanted: Rc<Cell<Option<term::Size>>>,
    applied: Rc<Cell<Option<term::Size>>>,
    settling: bool,
}
impl Focusable for TerminalView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    OpenPath(Vec<String>),
    OpenUrl { url: String, external: bool },
    AttachFailed(String),
}
impl EventEmitter<TerminalEvent> for TerminalView {}
impl TerminalView {
    pub fn new(session: term::TerminalSession, mut changes: UnboundedReceiver<()>, cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            while changes.next().await.is_some() {
                while changes.try_recv().is_ok() {}
                let updated = this.update(cx, |this, cx| {
                    for text in this.session.take_clipboard_writes() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    if let Some(err) = this.session.take_error() {
                        cx.emit(TerminalEvent::AttachFailed(err));
                    }
                    // Every tab's screen is kept for whoever asks (`tab.cells`, and a theme's
                    // previews), not only the ones drawn: a tab can be attached and unwatched.
                    let grid = this.session.grid();
                    if !std::sync::Arc::ptr_eq(&grid, &this.grid) {
                        crate::term::screens::publish(this.session.tab(), std::sync::Arc::clone(&grid));
                        this.grid = grid;
                    }
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(BLINK_INTERVAL).await;
                let alive = this.update(cx, |this, cx| this.blink(cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
        Self {
            grid: session.grid(),
            session: Rc::new(session),
            focus: cx.focus_handle(),
            placement: Rc::default(),
            press: None,
            selection: None,
            marked: None,
            scroll: Default::default(),
            blink_on: true,
            last_input: Instant::now(),
            focused: false,
            painter: Rc::default(),
            held: gpui::Modifiers::default(),
            recheck: false,
            wanted: Rc::default(),
            applied: Rc::default(),
            settling: false,
        }
    }
    /// Whether the tab's terminal has gone (closed, or its stream ended for good).
    pub(crate) fn closed(&self) -> bool {
        self.session.closed()
    }
    /// Drawn again after a time put away: takes the terminal back if another client took it.
    pub(crate) fn shown_again(&self) {
        self.session.claim();
    }
    /// Tell the program the size its tab has, live: a new size is sent at once, and while the
    /// tab keeps changing size, at most one a frame, ending on the last (see [`SETTLE`]).
    fn settle(&mut self, cx: &mut Context<Self>) {
        if self.settling || self.wanted.get().is_none() || self.wanted.get() == self.applied.get() {
            return;
        }
        self.settling = true;
        let (wanted, applied, session) = (Rc::clone(&self.wanted), Rc::clone(&self.applied), Rc::clone(&self.session));
        let send = move || match wanted.get() {
            Some(size) if wanted.get() != applied.get() => {
                applied.set(Some(size));
                session.resize(size);
                true
            }
            _ => false,
        };
        send();
        cx.spawn(async move |this, cx| {
            // Sizes asked for while we waited are sent one a frame, until the tab holds still.
            loop {
                cx.background_executor().timer(SETTLE).await;
                if !send() {
                    break;
                }
            }
            let _ = this.update(cx, |this, cx| {
                this.settling = false;
                cx.notify();
            });
        })
        .detach();
    }
    fn blink(&mut self, cx: &mut Context<Self>) {
        let blinks =
            self.focused && self.grid.cursor.is_some() && Settings::get(cx).terminal.cursor_blink == CursorBlink::On;
        let next = !blinks || self.last_input.elapsed() < BLINK_INTERVAL || !self.blink_on;
        if next != self.blink_on {
            self.blink_on = next;
            cx.notify();
        }
    }
    pub(crate) fn font(settings: &Settings, bold: bool, italic: bool) -> Font {
        let t = &settings.terminal;
        let mut font = gpui::font(t.font_family.clone());
        // Bold is three steps heavier than the regular weight, and never lighter than bold.
        font.weight = FontWeight(if bold { (t.font_weight + 300.).clamp(700., 900.) } else { t.font_weight });
        font.style = if italic { FontStyle::Italic } else { FontStyle::Normal };
        if !t.font_fallbacks.is_empty() {
            font.fallbacks = Some(gpui::FontFallbacks::from_fonts(t.font_fallbacks.clone()));
        }
        if !t.ligatures {
            let off = ["calt", "liga", "dlig"].map(|feature| (feature.to_string(), 0));
            font.features = gpui::FontFeatures(Arc::new(off.to_vec()));
        }
        font
    }
    pub(crate) fn metrics(window: &Window, settings: &Settings) -> Metrics {
        let size = px(settings.terminal.font_size);
        let text = window.text_system();
        let font_id = text.resolve_font(&Self::font(settings, false, false));
        let cell_width = text
            .advance(font_id, size, 'm')
            .map(|advance| f32::from(advance.width))
            .unwrap_or(settings.terminal.font_size * 0.6);
        let line_height = Self::natural_line_height(window, font_id, size) * Self::line_height_scale(settings);
        Metrics { cell_width, line_height: line_height.round().max(1.) }
    }
    /// The font's own line at `size`: its ascent, descent and line gap, as Ghostty's cell is.
    pub(crate) fn natural_line_height(window: &Window, font_id: gpui::FontId, size: Pixels) -> f32 {
        let text = window.text_system();
        let natural = f32::from(text.ascent(font_id, size))
            + f32::from(text.descent(font_id, size)).abs()
            + f32::from(text.line_gap(font_id, size)).max(0.);
        if natural.is_finite() && natural > 0. { natural } else { f32::from(size) * 1.2 }
    }
    /// Settings' Line height, a percentage added to the font's own line, as a factor of it.
    pub(crate) fn line_height_scale(settings: &Settings) -> f32 {
        (1. + settings.terminal.line_height_adjust / 100.).max(0.1)
    }
    fn cell(&self, position: gpui::Point<Pixels>) -> (u16, u16) {
        let placement = self.placement.get();
        let metrics = placement.metrics;
        let column = ((f32::from(position.x) - placement.origin_x) / metrics.cell_width).floor();
        let row = ((f32::from(position.y) - placement.origin_y) / metrics.line_height).floor();
        (
            column.clamp(0.0, f32::from(placement.cols.saturating_sub(1))) as u16,
            row.clamp(0.0, f32::from(placement.rows.saturating_sub(1))) as u16,
        )
    }
    fn cursor_bounds(&self) -> Bounds<Pixels> {
        let placement = self.placement.get();
        let metrics = placement.metrics;
        let (x, y) = self.grid.cursor.map_or((0, 0), |cursor| (cursor.x, cursor.y));
        Bounds::new(
            point(
                px(placement.origin_x + f32::from(x) * metrics.cell_width),
                px(placement.origin_y + f32::from(y) * metrics.line_height),
            ),
            size(px(metrics.cell_width), px(metrics.line_height)),
        )
    }
    fn modifiers(modifiers: gpui::Modifiers) -> u8 {
        u8::from(modifiers.shift) | (u8::from(modifiers.control) << 1) | (u8::from(modifiers.alt) << 2)
    }
    fn button(button: MouseButton) -> &'static str {
        match button {
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            _ => "left",
        }
    }
    fn send(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.session.send_input(bytes);
        self.typed(cx);
    }
    fn typed(&mut self, cx: &mut Context<Self>) {
        self.selection = None;
        self.last_input = Instant::now();
        self.blink_on = true;
        cx.notify();
    }
    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let has_image = item.entries().iter().any(|entry| matches!(entry, ClipboardEntry::Image(_)));
        match item.text().filter(|_| !has_image) {
            Some(text) => {
                let bytes = term::keys::paste(&text, self.session.bracketed_paste());
                self.send(&bytes, cx)
            }
            None => self.send(&[0x16], cx),
        }
    }
    fn copy(&self, cx: &mut Context<Self>) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let text = selection.text(&self.grid);
        if text.is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }
    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let option_as_alt = Settings::get(cx).terminal.option_as_alt;
        match term::keys::chord(&event.keystroke).as_str() {
            "cmd+v" | "shift+cmd+v" => self.paste(cx),
            "cmd+c" => {
                if !self.copy(cx) {
                    return;
                }
            }
            "cmd+a" => {
                let rows = self.grid.rows.len() as u16;
                self.selection = Some(Selection {
                    anchor: Point::new(0, 0),
                    head: Point::new(self.grid.cols.saturating_sub(1), rows.saturating_sub(1)),
                    granularity: Granularity::Line,
                });
                cx.notify();
            }
            _ if self.marked.is_some() => return,
            _ => match term::keys::typed(&event.keystroke, option_as_alt, self.session.kitty_flags()) {
                term::keys::Typed::Bytes(bytes) => self.send(bytes, cx),
                term::keys::Typed::Key(chord) => {
                    let action = if event.is_held { pumi_keys::Action::Repeat } else { pumi_keys::Action::Press };
                    if !self.session.key(&chord, term::keys::option_as_alt(option_as_alt), action) {
                        return;
                    }
                    self.typed(cx);
                }
                term::keys::Typed::Text | term::keys::Typed::None => return,
            },
        }
        cx.stop_propagation();
    }
    fn on_key_up(&mut self, event: &KeyUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.marked.is_some() {
            return;
        }
        let option_as_alt = Settings::get(cx).terminal.option_as_alt;
        if let Some(chord) = term::keys::released(&event.keystroke, option_as_alt) {
            self.session.key(&chord, term::keys::option_as_alt(option_as_alt), pumi_keys::Action::Release);
        }
    }
    fn on_modifiers(&mut self, event: &ModifiersChangedEvent, _: &mut Window, cx: &mut Context<Self>) {
        let was = std::mem::replace(&mut self.held, event.modifiers);
        if self.session.kitty_flags() & pumi_keys::kitty::ALL_KEYS == 0 {
            return;
        }
        let option_as_alt = term::keys::option_as_alt(Settings::get(cx).terminal.option_as_alt);
        for (chord, pressed) in term::keys::modifier_changes(was, event.modifiers) {
            let action = if pressed { pumi_keys::Action::Press } else { pumi_keys::Action::Release };
            self.session.key(&chord, option_as_alt, action);
        }
    }
    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let (column, row) = self.cell(event.position);
        let at = Point::new(column, row);
        let left = event.button == MouseButton::Left;
        if left && event.modifiers.platform {
            let link = self.session.hyperlink(column, row).or_else(|| selection::url_at(&self.grid, at));
            match link.filter(|url| !url.starts_with("file://")) {
                Some(url) => cx.emit(TerminalEvent::OpenUrl { url, external: event.modifiers.shift }),
                None => {
                    let paths = selection::path_candidates_at(&self.grid, at);
                    if !paths.is_empty() {
                        cx.emit(TerminalEvent::OpenPath(paths));
                    }
                }
            }
            return;
        }
        let local_only = left && event.modifiers.shift;
        // A program that tracks the mouse selects for itself (pi copies what it selects); shift
        // selects here all the same, as in other terminals.
        let selecting = left && (local_only || !self.session.mouse_tracked());
        if selecting {
            match (local_only, self.selection.as_mut()) {
                (true, Some(selection)) => selection.head = at,
                _ => {
                    let granularity = match event.click_count {
                        0 | 1 => Granularity::Cell,
                        2 => Granularity::Word,
                        _ => Granularity::Line,
                    };
                    self.selection = Some(Selection::new(at, granularity));
                }
            }
            cx.notify();
        } else if left && self.selection.take().is_some() {
            cx.notify();
        }
        self.press = Some(Press { button: event.button, forwarded: !local_only, selecting });
        if !local_only {
            self.session.mouse("down", Self::button(event.button), column, row, Self::modifiers(event.modifiers));
        }
    }
    fn on_mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else {
            return;
        };
        if press.selecting && self.selection.is_some_and(|s| s.is_empty()) {
            self.selection = None;
            cx.notify();
        } else if press.selecting && Settings::get(cx).terminal.copy_on_select {
            self.copy(cx);
        }
        if press.forwarded {
            let (column, row) = self.cell(event.position);
            self.session.mouse("up", Self::button(press.button), column, row, Self::modifiers(event.modifiers));
        }
    }
    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &Window, cx: &mut Context<Self>) {
        let Some(press) = self.press else { return };
        let (column, row) = self.cell(event.position);
        if press.selecting
            && let Some(selection) = self.selection.as_mut()
        {
            let head = Point::new(column, row);
            if selection.head != head {
                selection.head = head;
                cx.notify();
            }
        }
        if press.forwarded {
            self.session.mouse("drag", Self::button(press.button), column, row, Self::modifiers(event.modifiers));
        }
    }
    fn on_scroll(&mut self, event: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = match event.delta {
            ScrollDelta::Lines(delta) => term::scroll::Delta::Ticks(delta.y),
            ScrollDelta::Pixels(delta) => term::scroll::Delta::Pixels(f32::from(delta.y)),
        };
        let line_height = self.placement.get().metrics.line_height;
        let steps = self.scroll.feed(delta, line_height);
        if steps == 0 {
            return;
        }
        if self.selection.take().is_some() {
            cx.notify();
        }
        let at = self.cell(event.position);
        self.session.wheel(steps, SCROLL_LINES, at);
    }
    fn on_drop(&mut self, paths: &ExternalPaths, _: &mut Window, cx: &mut Context<Self>) {
        let quoted: Vec<String> = paths.paths().iter().map(|path| shell_quote(&path.to_string_lossy())).collect();
        self.send(format!("{} ", quoted.join(" ")).as_bytes(), cx);
    }
}
impl PlatformText for TerminalView {
    fn marked(&self) -> Option<&str> {
        self.marked.as_deref()
    }
    fn set_marked(&mut self, text: Option<String>) {
        self.marked = text;
    }
    fn commit(&mut self, text: &str, cx: &mut Context<Self>) {
        self.session.type_text(text);
        self.typed(cx);
    }
    fn caret_bounds(&self) -> Bounds<Pixels> {
        self.cursor_bounds()
    }
}
impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let grid = self.session.grid();
        if !std::sync::Arc::ptr_eq(&grid, &self.grid) {
            crate::term::screens::publish(self.session.tab(), std::sync::Arc::clone(&grid));
        }
        self.grid = grid;
        if self.session.held() && !std::mem::replace(&mut self.recheck, true) {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(HELD_RECHECK).await;
                let _ = this.update(cx, |this, cx| {
                    this.recheck = false;
                    cx.notify();
                });
            })
            .detach();
        }
        for image in self.session.take_dropped_images() {
            let _ = window.drop_image(image);
        }
        let focused = self.focus.is_focused(window);
        if std::mem::replace(&mut self.focused, focused) != focused {
            self.session.focus(focused);
        }
        let settings = Settings::get(cx);
        let metrics = Self::metrics(window, settings);
        let font_size = px(settings.terminal.font_size);
        let fonts = [
            Self::font(settings, false, false),
            Self::font(settings, true, false),
            Self::font(settings, false, true),
            Self::font(settings, true, true),
        ];
        let cursor = self.grid.cursor.filter(|_| self.marked.is_none() && (self.blink_on || !self.focused));
        let cursor_shape = cursor.map(|cursor| match cursor.shape {
            _ if !self.focused => CursorShape::Hollow,
            shape => shape,
        });
        let block = cursor.filter(|_| cursor_shape == Some(CursorShape::Block)).map(|cursor| (cursor.x, cursor.y));
        let selected =
            self.selection.filter(|selection| !selection.is_empty()).map(|selection| selection.bounds(&self.grid));
        let default_bg = theme::terminal::BG_REF;
        let cursor_wide = cursor.is_some_and(|cursor| {
            let row = self.grid.rows.get(usize::from(cursor.y));
            row.is_some_and(|row| selection::columns(row).any(|(col, cell)| col == cursor.x && cell.wide))
        });
        let frame = paint::Frame {
            grid: Arc::clone(&self.grid),
            look: paint::Look::new(font_size, fonts, metrics, settings),
            selection: selected,
            block,
        };
        let painter = Rc::clone(&self.painter);
        let diagnostics = self.session.diagnostics().clone();
        let placement = Rc::clone(&self.placement);
        let wanted = Rc::clone(&self.wanted);
        let applied = Rc::clone(&self.applied);
        let view = cx.entity();
        let sizing = view.clone();
        let focus = self.focus.clone();
        let (padding_x, padding_y) = settings.terminal.padding;
        let extend = settings.terminal.padding_color == crate::settings::PaddingColor::Extend;
        let sizer = canvas(
            move |bounds: Bounds<Pixels>, window, cx| {
                let began = Instant::now();
                let prepared = painter.borrow_mut().prepare(&frame, window);
                diagnostics.painted(began.elapsed(), prepared.shaped, !frame.grid.rows.is_empty());
                let width = f32::from(bounds.size.width) - 2.0 * padding_x;
                let cols = (width / metrics.cell_width).floor();
                let rows = ((f32::from(bounds.size.height) - 2.0 * padding_y) / metrics.line_height).floor();
                if cols < 2.0 || rows < 2.0 {
                    return (prepared, (padding_x, padding_y));
                }
                // The pad follows the box, so the text does not slide while a size waits to be
                // sent; the cells the pointer maps to stay the ones the program was laid out for.
                let pad_x = padding_x + ((width - cols * metrics.cell_width) / 2.0).floor().max(0.0);
                let pad_y = padding_y;
                let size = term::Size {
                    cols: cols as u16,
                    rows: rows as u16,
                    cell_width: metrics.cell_width.round() as u32,
                    cell_height: metrics.line_height as u32,
                };
                // Settled from here, where the size is measured, not from the next render: in a
                // cached tab nothing renders the view again, and the session attaches only once
                // it has been told a size, so it would wait for one forever.
                wanted.set(Some(size));
                sizing.update(cx, |this, cx| this.settle(cx));
                let shown = applied.get().unwrap_or(size);
                placement.set(Placement {
                    origin_x: f32::from(bounds.origin.x) + pad_x,
                    origin_y: f32::from(bounds.origin.y) + pad_y,
                    pad_x,
                    pad_y,
                    cols: shown.cols,
                    rows: shown.rows,
                    metrics,
                });
                (prepared, (pad_x, pad_y))
            },
            move |bounds, (prepared, (pad_x, pad_y)), window, cx| {
                let grid = Bounds::new(bounds.origin + point(px(pad_x), px(pad_y)), bounds.size);
                if extend {
                    prepared.paint_padding(bounds, grid.origin, window);
                }
                prepared.paint(grid, window, cx);
                window.handle_input(&focus, PlatformInput::new(view.clone()), cx);
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble {
                        moved.update(cx, |view, cx| view.on_mouse_move(event, window, cx));
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble {
                        view.update(cx, |view, cx| view.on_mouse_up(event, cx));
                    }
                });
            },
        )
        .absolute()
        .size_full();
        let Placement { pad_x, pad_y, .. } = self.placement.get();
        let cell_box = |x: u16, y: u16, cells: f32| {
            div()
                .absolute()
                .left(px(pad_x + f32::from(x) * metrics.cell_width))
                .top(px(pad_y + f32::from(y) * metrics.line_height))
                .w(px(metrics.cell_width * cells))
                .h(px(metrics.line_height))
        };
        let cursor_color = Hsla::from(rgb(theme::terminal::cursor()));
        let cursor_cells = if cursor_wide { 2.0 } else { 1.0 };
        let cursor_overlay = cursor.zip(cursor_shape).and_then(|(cursor, shape)| {
            let (x, y) = (cursor.x, cursor.y);
            match shape {
                CursorShape::Block => None,
                CursorShape::Bar => Some(cell_box(x, y, 1.0).w(px(2.)).bg(cursor_color)),
                CursorShape::Underline => Some(
                    cell_box(x, y, cursor_cells).child(div().absolute().bottom_0().w_full().h(px(2.)).bg(cursor_color)),
                ),
                CursorShape::Hollow => Some(cell_box(x, y, cursor_cells).border_1().border_color(cursor_color)),
            }
        });
        let composition = self.marked.clone().zip(self.grid.cursor).map(|(text, cursor)| {
            cell_box(cursor.x, cursor.y, 1.0)
                .w_auto()
                .whitespace_nowrap()
                .line_height(px(metrics.line_height))
                .bg(theme::terminal::background(default_bg))
                .text_color(Hsla::from(rgb(theme::terminal::fg())))
                .border_b_1()
                .border_color(cursor_color)
                .child(text)
        });
        let status = if self.session.closed() {
            Some("stream closed")
        } else if self.session.observing() {
            Some("viewing · type to take control")
        } else {
            None
        };
        let closed = status.map(|status| {
            div()
                .absolute()
                .bottom(px(2.))
                .right(px(6.))
                .px_1()
                .rounded_md()
                .bg(Hsla::from(rgb(theme::terminal::fg())).opacity(0.12))
                .text_xs()
                .text_color(Hsla::from(rgb(theme::terminal::fg())).opacity(0.6))
                .child(status)
        });
        div()
            .id("terminal")
            .relative()
            .size_full()
            .overflow_hidden()
            .cursor(pointer_style(self.session.pointer().as_deref()))
            .text_color(Hsla::from(rgb(theme::terminal::fg())))
            .font_family(settings.terminal.font_family.clone())
            .text_size(font_size)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_key_up(cx.listener(Self::on_key_up))
            .on_modifiers_changed(cx.listener(Self::on_modifiers))
            .on_drop(cx.listener(Self::on_drop))
            .on_any_mouse_down(cx.listener(Self::on_mouse_down))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(sizer)
            .children(cursor_overlay)
            .children(composition)
            .children(closed)
    }
}
const POINTERS: &[(&str, gpui::CursorStyle)] = &[
    ("default", gpui::CursorStyle::Arrow),
    ("text", gpui::CursorStyle::IBeam),
    ("vertical-text", gpui::CursorStyle::IBeamCursorForVerticalLayout),
    ("pointer", gpui::CursorStyle::PointingHand),
    ("crosshair", gpui::CursorStyle::Crosshair),
    ("grab", gpui::CursorStyle::OpenHand),
    ("grabbing", gpui::CursorStyle::ClosedHand),
    ("move", gpui::CursorStyle::ClosedHand),
    ("not-allowed", gpui::CursorStyle::OperationNotAllowed),
    ("no-drop", gpui::CursorStyle::OperationNotAllowed),
    ("copy", gpui::CursorStyle::DragCopy),
    ("alias", gpui::CursorStyle::DragLink),
    ("context-menu", gpui::CursorStyle::ContextualMenu),
    ("ew-resize", gpui::CursorStyle::ResizeLeftRight),
    ("col-resize", gpui::CursorStyle::ResizeColumn),
    ("ns-resize", gpui::CursorStyle::ResizeUpDown),
    ("row-resize", gpui::CursorStyle::ResizeRow),
    ("e-resize", gpui::CursorStyle::ResizeRight),
    ("w-resize", gpui::CursorStyle::ResizeLeft),
    ("n-resize", gpui::CursorStyle::ResizeUp),
    ("s-resize", gpui::CursorStyle::ResizeDown),
    ("nwse-resize", gpui::CursorStyle::ResizeUpLeftDownRight),
    ("nesw-resize", gpui::CursorStyle::ResizeUpRightDownLeft),
];
fn pointer_style(shape: Option<&str>) -> gpui::CursorStyle {
    let shape = shape.unwrap_or("text");
    POINTERS.iter().find(|(name, _)| *name == shape).map_or(gpui::CursorStyle::IBeam, |(_, style)| *style)
}
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quotes_paths_with_single_quotes() {
        assert_eq!(shell_quote("/tmp/it's here.png"), r"'/tmp/it'\''s here.png'");
    }
}
