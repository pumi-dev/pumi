use crate::ThemedBorder;
use gpui::{AnyElement, App, Div, ElementId, SharedString, Stateful, Window, div, prelude::*, px};

use super::{Layers, Sizable, Size, color, delegate_div, focus_ring};
use crate::StyledExt;

#[derive(IntoElement)]
pub struct ListItem {
    base: Stateful<Div>,
    layers: Layers,
    start: Option<AnyElement>,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    selected: bool,
    disabled: bool,
    tab_stop: bool,
    size: Size,
}

delegate_div!(ListItem);

impl ParentElement for ListItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Sizable for ListItem {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl ListItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id.into()),
            layers: Layers::default(),
            start: None,
            label: None,
            children: Vec::new(),
            selected: false,
            disabled: false,
            tab_stop: false,
            size: Size::Large,
        }
    }

    pub fn start(mut self, el: impl IntoElement) -> Self {
        self.start = Some(el.into_any_element());
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
}

impl RenderOnce for ListItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut layers = self.layers;
        let interactive = !self.disabled;
        let on_click = layers.on_click.take();
        let (height, text) = match self.size {
            Size::XSmall => (20., 12.),
            Size::Small => (24., 13.),
            Size::Medium => (26., 13.),
            Size::Large => (28., 14.),
        };
        let height = height * crate::theme::metric("control_height") / 28.;
        let text = text * crate::theme::metric("font_size") / 13.;
        let fg = if self.selected { color::foreground() } else { color::muted_foreground() };
        let el = self
            .base
            .hrow()
            .gap(px(8.))
            .h(px(height))
            .px(px(8.))
            .rounded(px(color::radius()))
            .border_themed()
            .border_color(color::transparent())
            .text_size(px(text))
            .text_color(fg)
            .when(self.selected, |el| el.bg(color::list_active()))
            .when(self.disabled, |el| el.opacity(0.5))
            .when(interactive, |el| el.cursor_pointer())
            .when_some(on_click.filter(|_| interactive), |el, f| el.on_click(move |e, w, cx| f(e, w, cx)))
            .when(self.tab_stop && interactive, |el| {
                el.tab_index(0)
                    .focus(focus_ring)
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| window.prevent_default())
            })
            .children(self.start)
            .when_some(self.label, |el, label| el.child(div().flex_1().min_w_0().child(crate::text::text(label))))
            .children(self.children);
        let hover = (interactive && !self.selected)
            .then(|| gpui::StyleRefinement::default().bg(color::list_hover()).text_color(color::foreground()));
        layers.finish(el, hover, None)
    }
}
