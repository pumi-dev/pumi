use crate::ThemedBorder;
use gpui::{App, Div, MouseButton, MouseDownEvent, Stateful, Window, div, prelude::*, px};

use super::{color, popover_shadow};

pub fn modal_layer(
    width: f32,
    height: f32,
    surface: Stateful<Div>,
    dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id("modal-backdrop")
        .absolute()
        .inset_0()
        .occlude()
        .bg(color::overlay())
        .on_mouse_down(MouseButton::Left, dismiss)
        .child(
            div().absolute().top(px(80.)).left_0().right_0().flex().justify_center().child(
                surface
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .flex()
                    .w(px(width))
                    .h(px(height))
                    .rounded(px(color::radius_lg()))
                    .border_themed()
                    .border_color(color::border())
                    .bg(color::popover())
                    .shadow(popover_shadow())
                    .overflow_hidden(),
            ),
        )
}
