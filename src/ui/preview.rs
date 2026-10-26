//! A tab's screen, small.
//!
//! What a theme draws for a tab the window is not showing — the office's desks, one per view of
//! every session. It is the app's own screen of the tab, the grid it parsed from the tab's
//! bytes (`tab.cells` reads the same one), so nothing is attached and nothing is resized to take
//! it: it costs a paint, not a terminal. The grid is drawn whole, scaled to fit the box it is
//! given, so a small desk shows all of a tab and a larger one shows it larger.

use crate::settings::Settings;
use crate::term::Grid;
use crate::theme;
use crate::ui::terminal::{Metrics, TerminalView, paint};
use gpui::{AnyElement, App, Bounds, Font, Hsla, Pixels, Window, canvas, div, fill, prelude::*, px};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    /// One shaped-row cache per tab: rows are keyed by the grid's own row pointers, so a tab
    /// that scrolls reshapes only what changed, however many places draw it.
    static CACHES: RefCell<HashMap<String, Rc<RefCell<paint::Cache>>>> = RefCell::new(HashMap::new());
}

struct Shown {
    prepared: paint::Prepared,
    background: Hsla,
}

/// A preview of a tab's screen, filling its box. A tab the app has no screen of yet is blank.
pub fn element(tab: &str) -> AnyElement {
    let key = tab.to_string();
    div()
        .size_full()
        .overflow_hidden()
        .child(
            canvas(
                move |bounds, window, cx| prepare(&key, bounds, window, cx),
                |bounds, shown: Option<Shown>, window, cx| {
                    let Some(shown) = shown else { return };
                    window.paint_quad(fill(bounds, shown.background));
                    shown.prepared.paint(bounds, window, cx);
                },
            )
            .size_full(),
        )
        .into_any_element()
}

fn prepare(tab: &str, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) -> Option<Shown> {
    let grid = crate::term::screens::grid(tab)?;
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if width < 4. || height < 4. {
        return None;
    }
    let settings = Settings::get(cx);
    let (font_size, metrics, fonts) = fit(&grid, (width, height), settings, window);
    let look = paint::Look::new(px(font_size), fonts, metrics, settings);
    let frame = paint::Frame { grid: grid.clone(), look, selection: None, block: None };
    let cache = CACHES.with_borrow_mut(|caches| caches.entry(tab.to_string()).or_default().clone());
    let prepared = cache.borrow_mut().prepare(&frame, window);
    Some(Shown { prepared, background: Hsla::from(theme::terminal::background(grid.default_bg)) })
}

/// The largest type that shows the whole grid in the box, and the cell it makes.
fn fit(grid: &Grid, (width, height): (f32, f32), settings: &Settings, window: &Window) -> (f32, Metrics, [Font; 4]) {
    let cols = f32::from(grid.cols.max(1));
    let rows = grid.rows.len().max(1) as f32;
    let fonts = [
        TerminalView::font(settings, false, false),
        TerminalView::font(settings, true, false),
        TerminalView::font(settings, false, true),
        TerminalView::font(settings, true, true),
    ];
    let text = window.text_system();
    let font_id = text.resolve_font(&fonts[0]);
    let good = settings.terminal.font_size;
    // The line as a multiple of the size: the font's own, metrics scaling with the size.
    let ratio = (TerminalView::natural_line_height(window, font_id, px(good.max(1.))) / good.max(1.)
        * TerminalView::line_height_scale(settings))
    .max(0.5);
    // A cell is about 0.6 of the type wide in a monospace face; then measure it for real.
    let by_width = width / (cols * 0.6);
    let by_height = height / (rows * ratio);
    let font_size = by_width.min(by_height).clamp(1., good.max(1.));
    let size = px(font_size);
    let cell_width =
        text.advance(font_id, size, 'm').map(|advance| f32::from(advance.width)).unwrap_or(font_size * 0.6);
    (font_size, Metrics { cell_width, line_height: (font_size * ratio).round().max(1.) }, fonts)
}
