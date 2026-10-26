use crate::ThemedBorder;
use gpui::{App, ElementId, SharedString, Window, div, prelude::*, px};

use super::{Button, OnClick, Size, color};
use crate::StyledExt;

const DASH: &str = "icons/dash.svg";
const PLUS: &str = "icons/plus.svg";

#[derive(IntoElement)]
pub struct Stepper {
    id: SharedString,
    value: SharedString,
    on_decrement: Option<OnClick>,
    on_increment: Option<OnClick>,
    can_decrement: bool,
    can_increment: bool,
}

impl Stepper {
    pub fn new(id: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            value: value.into(),
            on_decrement: None,
            on_increment: None,
            can_decrement: true,
            can_increment: true,
        }
    }

    pub fn on_decrement(mut self, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_decrement = Some(std::rc::Rc::new(f));
        self
    }

    pub fn on_increment(mut self, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_increment = Some(std::rc::Rc::new(f));
        self
    }

    pub fn limits(mut self, can_decrement: bool, can_increment: bool) -> Self {
        self.can_decrement = can_decrement;
        self.can_increment = can_increment;
        self
    }
}

impl RenderOnce for Stepper {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let height = Size::Medium.height();
        let side = |suffix: &str, icon: &'static str, on_click: Option<OnClick>, enabled: bool| {
            Button::icon_only(ElementId::Name(format!("{}-{suffix}", self.id).into()), icon)
                .icon_size(12.)
                .disabled(!enabled)
                .tab_stop(true)
                .rounded_none()
                .border_0()
                .w(px(height))
                .h_full()
                .when_some(on_click, |b, f| b.on_click(move |e, w, cx| f(e, w, cx)))
        };
        div()
            .flex_none()
            .hrow()
            .h(px(height))
            .rounded(px(color::radius_sm()))
            .border_themed()
            .border_color(color::input())
            .bg(color::popover())
            .overflow_hidden()
            .child(side("dec", DASH, self.on_decrement, self.can_decrement))
            .child(
                div()
                    .min_w(px(64.))
                    .h_full()
                    .px(px(4.))
                    .center()
                    .border_l_1()
                    .border_r_1()
                    .border_color(color::input())
                    .text(13., color::foreground())
                    .child(self.value),
            )
            .child(side("inc", PLUS, self.on_increment, self.can_increment))
    }
}
