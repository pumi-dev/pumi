use crate::ThemedBorder;
use gpui::{App, Rgba, SharedString, Window, div, prelude::*, px};

use super::color;
use crate::{StyledExt, with_alpha};

#[derive(IntoElement)]
pub struct Indicator {
    color: Rgba,
    size: f32,
}

impl Indicator {
    pub fn new(color: Rgba) -> Self {
        Self { color, size: 7. }
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Indicator {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().flex_none().size(px(self.size)).rounded_full().bg(self.color)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TagVariant {
    #[default]
    Secondary,
    Primary,
    Success,
    Warning,
    Danger,
}

#[derive(IntoElement)]
pub struct Tag {
    label: SharedString,
    variant: TagVariant,
}

impl Tag {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self { label: label.into(), variant: TagVariant::default() }
    }

    pub fn variant(mut self, variant: TagVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl RenderOnce for Tag {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let (bg, border, fg) = match self.variant {
            TagVariant::Secondary => (color::accent(), color::border(), color::muted_foreground()),
            TagVariant::Primary => tint(color::primary()),
            TagVariant::Success => tint(color::success()),
            TagVariant::Warning => tint(color::warning()),
            TagVariant::Danger => tint(color::danger()),
        };
        div()
            .flex_none()
            .hrow()
            .h(px(16.))
            .px(px(5.))
            .rounded(px(color::radius_sm()))
            .border_themed()
            .border_color(border)
            .bg(bg)
            .text(10., fg)
            .line_height(gpui::relative(1.25))
            .child(self.label)
    }
}

fn tint(c: Rgba) -> (Rgba, Rgba, Rgba) {
    (with_alpha(c, 0.15), with_alpha(c, 0.4), c)
}
