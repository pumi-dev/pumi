use gpui::{AnyElement, App, Div, ElementId, Rgba, Stateful, Window, div, prelude::*, px};

use super::{Layers, color, delegate_div};
use crate::StyledExt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabVariant {
    #[default]
    Underline,
    Pill,
    Segmented,
}

#[derive(IntoElement)]
pub struct Tab {
    base: Stateful<Div>,
    layers: Layers,
    children: Vec<AnyElement>,
    variant: TabVariant,
    selected: bool,
    disabled: bool,
}

delegate_div!(Tab);

impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Tab {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            base: div().id(id.into()),
            layers: Layers::default(),
            children: Vec::new(),
            variant: TabVariant::default(),
            selected: false,
            disabled: false,
        }
    }

    pub fn variant(mut self, variant: TabVariant) -> Self {
        self.variant = variant;
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
}

impl RenderOnce for Tab {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut layers = self.layers;
        let interactive = !self.disabled;
        let on_click = layers.on_click.take();
        let selected = self.selected;
        let (fg, fg_hover): (Rgba, Rgba) =
            if selected { (color::text(), color::text()) } else { (color::chrome_muted(), color::text()) };
        let el = self
            .base
            .flex_none()
            .hrow()
            .gap_1()
            .text_xs()
            .text_color(fg)
            .when(self.disabled, |el| el.opacity(0.5))
            .when(interactive, |el| el.cursor_pointer())
            .map(|el| match self.variant {
                TabVariant::Underline => el.h_full().px_2().border_b_1().border_color(if selected {
                    color::chrome_accent()
                } else {
                    color::transparent()
                }),
                TabVariant::Pill => {
                    el.h(px(22.)).px(px(10.)).rounded_full().when(selected, |el| el.bg(color::accent_active()))
                }
                TabVariant::Segmented => el
                    .h(px(20.))
                    .px(px(10.))
                    .rounded(px(color::radius_sm()))
                    .when(selected, |el| el.bg(color::accent_active()).shadow_sm()),
            })
            .when_some(on_click.filter(|_| interactive), |el, f| el.on_click(move |e, w, cx| f(e, w, cx)))
            .children(self.children);
        let hover = interactive.then(|| {
            let s = gpui::StyleRefinement::default().text_color(fg_hover);
            if !selected && self.variant != TabVariant::Underline { s.bg(color::accent()) } else { s }
        });
        layers.finish(el, hover, None)
    }
}

pub fn segmented(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id.into()).flex_none().hrow().gap(px(2.)).p(px(2.)).rounded(px(color::radius())).bg(color::background())
}
