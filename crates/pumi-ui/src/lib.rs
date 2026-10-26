mod badge;
mod button;
mod checkbox;
pub mod color;
mod dialog;
mod divider;
pub mod icon;
pub mod ime;
pub mod input;
pub mod kbd;
pub mod keys;
mod list;
pub mod marks;
mod menu;
mod select;
mod slider;
mod stepper;
mod switch;
pub mod tab;
pub mod text;
pub mod textarea;
pub mod theme;
mod tooltip;
pub use badge::{Indicator, Tag, TagVariant};
pub use button::{Button, ButtonGroup};
pub use checkbox::{Check, checkbox};
pub use dialog::modal_layer;
pub use divider::Divider;
use gpui::{BoxShadow, Rgba, Styled, hsla, point, px};
pub use icon::{ICON, ICON_MEDIUM, ICON_SMALL, chevron, icon, icon_file, moving};
pub use input::{InputEvent, LineEdit, Motion, TextInput};
pub use kbd::Kbd;
pub use list::ListItem;
pub use marks::{Marks, state_mark};
pub use menu::{MenuItem, popover};
pub use select::Select;
pub use slider::Slider;
pub use stepper::Stepper;
pub use switch::Switch;
pub use tab::{Tab, TabVariant};
pub use text::{Span, Text};
pub use textarea::TextArea;
pub use tooltip::Tooltip;
pub trait StyledExt: Styled + Sized {
    fn text(self, size: f32, color: Rgba) -> Self {
        self.text_size(px(size)).text_color(color)
    }
    fn fill(self) -> Self {
        self.flex_1().min_w_0().truncate()
    }
    fn hrow(self) -> Self {
        self.flex().items_center()
    }
    fn vcol(self) -> Self {
        self.flex().flex_col()
    }
    fn center(self) -> Self {
        self.flex().items_center().justify_center()
    }
}
impl<T: Styled> StyledExt for T {}
pub fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..color }
}
pub fn h_flex() -> gpui::Div {
    gpui::div().flex().items_center()
}
pub fn v_flex() -> gpui::Div {
    gpui::div().flex().flex_col()
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Size {
    XSmall,
    #[default]
    Small,
    Medium,
    Large,
}
fn tall() -> f32 {
    theme::metric("control_height") / 28.
}
fn big() -> f32 {
    theme::metric("font_size") / 13.
}
impl Size {
    pub fn height(self) -> f32 {
        tall()
            * match self {
                Size::XSmall => 20.,
                Size::Small => 24.,
                Size::Medium => 26.,
                Size::Large => 32.,
            }
    }
    pub fn square(self) -> f32 {
        tall()
            * match self {
                Size::XSmall => 18.,
                Size::Small => 22.,
                Size::Medium => 26.,
                Size::Large => 32.,
            }
    }
    pub fn padding_x(self) -> f32 {
        match self {
            Size::XSmall => 6.,
            Size::Small => 8.,
            Size::Medium => 8.,
            Size::Large => 12.,
        }
    }
    pub fn text(self) -> f32 {
        big()
            * match self {
                Size::XSmall => 11.,
                Size::Small => 12.,
                Size::Medium => 13.,
                Size::Large => 14.,
            }
    }
    pub fn icon(self) -> f32 {
        match self {
            Size::XSmall => 12.,
            Size::Small | Size::Medium => 14.,
            Size::Large => 16.,
        }
    }
    pub fn radius(self) -> f32 {
        match self {
            Size::XSmall | Size::Small | Size::Medium => color::radius_sm(),
            Size::Large => color::radius(),
        }
    }
}
pub trait Sizable: std::marker::Sized {
    fn with_size(self, size: Size) -> Self;
    fn xsmall(self) -> Self {
        self.with_size(Size::XSmall)
    }
    fn small(self) -> Self {
        self.with_size(Size::Small)
    }
    fn medium(self) -> Self {
        self.with_size(Size::Medium)
    }
    fn large(self) -> Self {
        self.with_size(Size::Large)
    }
}
pub trait ThemedBorder: Styled + std::marker::Sized {
    fn border_themed(mut self) -> Self {
        let width: gpui::AbsoluteLength = px(color::border_width()).into();
        let widths = &mut self.style().border_widths;
        (widths.top, widths.right, widths.bottom, widths.left) = (Some(width), Some(width), Some(width), Some(width));
        self
    }
}
impl<S: Styled> ThemedBorder for S {}
pub fn focus_ring<S: Styled>(el: S) -> S {
    el.border_color(color::ring()).shadow(vec![BoxShadow {
        color: Rgba { a: 0.35, ..color::ring() }.into(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(2.),
        inset: false,
    }])
}
pub fn popover_shadow() -> Vec<BoxShadow> {
    [(0., 0., 0., 0.12), (4., 3., -1., 0.18), (2., 2., -2., 0.14), (10., 12., -4., 0.18)]
        .into_iter()
        .map(|(y, blur, spread, alpha)| BoxShadow {
            color: hsla(0., 0., 0., alpha),
            offset: point(px(0.), px(y)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        })
        .collect()
}
pub fn progress(since: Option<std::time::Instant>, ms: u64, window: &gpui::Window) -> f32 {
    let Some(since) = since else { return 1. };
    let t = (since.elapsed().as_secs_f32() * 1000. / ms as f32).clamp(0., 1.);
    if t < 1. {
        window.request_animation_frame();
    }
    1. - (1. - t).powi(3)
}
pub type OnClick = std::rc::Rc<dyn Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static>;
#[derive(Default)]
pub struct Layers {
    pub style: gpui::StyleRefinement,
    pub hover: Option<gpui::StyleRefinement>,
    pub active: Option<gpui::StyleRefinement>,
    pub group: Option<gpui::SharedString>,
    pub on_click: Option<OnClick>,
}
impl Layers {
    pub fn finish(
        self,
        mut el: gpui::Stateful<gpui::Div>,
        hover: Option<gpui::StyleRefinement>,
        active: Option<gpui::StyleRefinement>,
    ) -> gpui::Stateful<gpui::Div> {
        use gpui::{InteractiveElement, IsEmpty, Refineable, StatefulInteractiveElement};
        gpui::Styled::style(&mut el).refine(&self.style);
        let base_text = gpui::Styled::style(&mut el).text.clone();
        let merge = |own: Option<gpui::StyleRefinement>, theirs: Option<gpui::StyleRefinement>| match (own, theirs) {
            (None, None) => None,
            (own, theirs) => {
                let mut s = own.unwrap_or_default();
                let mut text = base_text.clone();
                let mut touched = false;
                for state in [Some(&s), theirs.as_ref()].into_iter().flatten() {
                    if !state.text.is_empty() {
                        text.refine(&state.text);
                        touched = true;
                    }
                }
                if let Some(theirs) = &theirs {
                    s.refine(theirs);
                }
                if touched {
                    s.text = text;
                }
                Some(s)
            }
        };
        if let Some(h) = merge(hover, self.hover) {
            el = el.hover(move |_| h);
        }
        if let Some(a) = merge(active, self.active) {
            el = el.active(move |_| a);
        }
        if let Some(group) = self.group {
            el = el.group(group);
        }
        el
    }
}
macro_rules! delegate_div {
    ($ty:ty) => {
        impl gpui::Styled for $ty {
            fn style(&mut self) -> &mut gpui::StyleRefinement {
                &mut self.layers.style
            }
        }
        impl gpui::InteractiveElement for $ty {
            fn interactivity(&mut self) -> &mut gpui::Interactivity {
                self.base.interactivity()
            }
            fn hover(mut self, f: impl FnOnce(gpui::StyleRefinement) -> gpui::StyleRefinement) -> Self {
                self.layers.hover = Some(f(self.layers.hover.take().unwrap_or_default()));
                self
            }
            fn group(mut self, group: impl Into<gpui::SharedString>) -> Self {
                self.layers.group = Some(group.into());
                self
            }
        }
        impl gpui::StatefulInteractiveElement for $ty {
            fn active(mut self, f: impl FnOnce(gpui::StyleRefinement) -> gpui::StyleRefinement) -> Self
            where
                Self: Sized,
            {
                self.layers.active = Some(f(self.layers.active.take().unwrap_or_default()));
                self
            }
            fn on_click(
                mut self,
                listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
            ) -> Self
            where
                Self: Sized,
            {
                self.layers.on_click = Some(std::rc::Rc::new(listener));
                self
            }
        }
    };
}
pub(crate) use delegate_div;
