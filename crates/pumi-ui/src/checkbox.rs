use crate::ThemedBorder;
use crate::{ICON, ICON_SMALL, color, icon};
use gpui::{Div, ElementId, Stateful, div, prelude::*, px};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Check {
    #[default]
    Off,
    On,
    Partial,
}
impl Check {
    pub fn named(name: &str) -> Self {
        match name {
            "on" => Self::On,
            "partial" => Self::Partial,
            _ => Self::Off,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Partial => "partial",
        }
    }
}
pub fn checkbox(id: impl Into<ElementId>, check: Check) -> Stateful<Div> {
    let set = check != Check::Off;
    let mark = match check {
        Check::On => Some("check"),
        Check::Partial => Some("minus"),
        Check::Off => None,
    };
    div()
        .id(id)
        .flex_none()
        .size(px(ICON))
        .rounded(px(color::radius_sm()))
        .border_themed()
        .border_color(if set { color::ring() } else { color::border() })
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .children(mark.map(|name| icon(name, ICON_SMALL, color::ring())))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_state_is_read_by_its_name() {
        for check in [Check::Off, Check::On, Check::Partial] {
            assert_eq!(Check::named(check.name()), check);
        }
        assert_eq!(Check::named(""), Check::Off);
    }
}
