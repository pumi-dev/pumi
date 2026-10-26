use gpui::{App, SharedString, Window, div, prelude::*, px};

use super::color;
use crate::StyledExt;

#[derive(IntoElement)]
pub struct Divider {
    vertical: bool,
    label: Option<SharedString>,
}

impl Divider {
    pub fn horizontal() -> Self {
        Self { vertical: false, label: None }
    }

    pub fn vertical() -> Self {
        Self { vertical: true, label: None }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl RenderOnce for Divider {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        if self.vertical {
            return div().flex_none().w(px(1.)).h_full().bg(color::border()).into_any_element();
        }
        match self.label {
            None => div().flex_none().h(px(1.)).w_full().bg(color::border()).into_any_element(),
            Some(label) => div()
                .hrow()
                .w_full()
                .gap(px(8.))
                .child(div().flex_1().h(px(1.)).bg(color::border()))
                .child(div().flex_none().text(11., color::placeholder()).child(label))
                .child(div().flex_1().h(px(1.)).bg(color::border()))
                .into_any_element(),
        }
    }
}
