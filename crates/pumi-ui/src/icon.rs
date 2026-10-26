use gpui::{
    Animation, AnimationExt as _, AnyElement, ElementId, Rgba, SharedString, Transformation, percentage, prelude::*,
    px, svg,
};
use std::time::Duration;
pub const ICON: f32 = 14.0;
pub const ICON_SMALL: f32 = 12.0;
pub const ICON_MEDIUM: f32 = 16.0;
pub fn icon_file(path: &str, size: f32, color: Rgba) -> AnyElement {
    svg().path(path.to_string()).flex_none().size(px(size)).text_color(color).into_any_element()
}
pub fn icon(name: &str, size: f32, color: Rgba) -> AnyElement {
    match crate::theme::icon(name) {
        Some(path) => icon_file(&path, size, color),
        None => gpui::div().flex_none().size(px(size)).into_any_element(),
    }
}
pub fn moving(icon: gpui::Svg, motion: &str, id: impl Into<SharedString>) -> AnyElement {
    let motion = motion.split_whitespace().find(|word| matches!(*word, "spin" | "pulse"));
    let Some(motion) = motion.filter(|_| !crate::theme::reduce_motion()) else {
        return icon.into_any_element();
    };
    let id = ElementId::Name(id.into());
    match motion {
        "spin" => icon
            .with_animation(id, Animation::new(Duration::from_millis(900)).repeat(), |icon, delta| {
                icon.with_transformation(Transformation::rotate(percentage(delta)))
            })
            .into_any_element(),
        _ => icon
            .with_animation(
                id,
                Animation::new(Duration::from_millis(1600)).repeat().with_easing(gpui::pulsating_between(0.55, 1.)),
                |icon, delta| icon.opacity(delta),
            )
            .into_any_element(),
    }
}
pub fn chevron(open: bool, color: Rgba) -> AnyElement {
    icon(if open { "chevron-down" } else { "chevron-right" }, ICON_SMALL, color)
}
