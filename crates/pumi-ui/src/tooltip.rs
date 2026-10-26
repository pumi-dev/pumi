use crate::ThemedBorder;
use gpui::{AnyView, App, Context, Render, SharedString, Window, div, prelude::*, px};

use super::{Kbd, color, popover_shadow};
use crate::StyledExt;

#[derive(Clone)]
pub struct Tooltip {
    text: SharedString,
    shortcut: Option<SharedString>,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into(), shortcut: None }
    }

    pub fn shortcut(mut self, chord: impl Into<SharedString>) -> Self {
        self.shortcut = Some(chord.into());
        self
    }

    pub fn build(self, _: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }

    pub fn text(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |window, cx| Tooltip::new(text.clone()).build(window, cx)
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().pl(px(10.)).pt(px(16.)).child(
            div()
                .hrow()
                .gap(px(8.))
                .py(px(3.))
                .px(px(8.))
                .rounded(px(color::radius()))
                .border_themed()
                .border_color(color::border())
                .bg(color::popover())
                .shadow(popover_shadow())
                .text(12., color::foreground())
                .child(self.text.clone())
                .when_some(self.shortcut.clone(), |el, chord| el.child(Kbd::new(chord).bare())),
        )
    }
}
