//! The `title` prop's tooltip: GPUI's own (it waits, follows the mouse and stays above
//! everything), drawn in the style a theme gives (`setTooltipStyle`).
use std::cell::RefCell;

use crate::style::StyleDesc;

thread_local! {
    static STYLE: RefCell<Option<StyleDesc>> = const { RefCell::new(None) };
}

/// The style every tooltip is drawn in from now on; `None` for the renderer's own.
pub(crate) fn set_style(style: Option<StyleDesc>) {
    STYLE.with(|s| *s.borrow_mut() = style);
}

fn default_style() -> StyleDesc {
    StyleDesc {
        background_color: Some("#1f2023f2".into()),
        color: Some("#e8e9eb".into()),
        border_color: Some("#ffffff1f".into()),
        border_width: Some(1.0),
        border_radius: Some(5.0),
        padding_top: Some(3.0),
        padding_bottom: Some(3.0),
        padding_left: Some(7.0),
        padding_right: Some(7.0),
        font_size: Some(11.0),
        max_width: Some(crate::style::DimensionValue::Pixels(320.0)),
        ..Default::default()
    }
}

struct Tooltip {
    text: gpui::SharedString,
}

impl gpui::Render for Tooltip {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        let style = STYLE.with(|s| s.borrow().clone()).unwrap_or_else(default_style);
        // A tooltip sits a little below and right of the pointer, as Zed's do.
        let mut el = crate::renderer::apply_styles(gpui::div().relative().mt(gpui::px(10.0)).ml(gpui::px(8.0)), &style);
        // Over a native page, the page is cut where the tooltip is.
        if let Some(cover) = crate::occlusion::cover(&style) {
            el = el.on_painted(move |bounds, window, cx| crate::occlusion::record(cover, bounds, window, cx));
        }
        if let Some(decor) = crate::decor::element(&style) {
            el = el.child(decor);
        }
        el.child(self.text.clone())
    }
}

/// Gives `el` the tooltip `text`.
pub(crate) fn attach<E: gpui::StatefulInteractiveElement>(el: E, text: &str) -> E {
    let text = gpui::SharedString::from(text.to_string());
    el.tooltip(move |_window, cx| {
        use gpui::AppContext as _;
        cx.new(|_| Tooltip { text: text.clone() }).into()
    })
}
