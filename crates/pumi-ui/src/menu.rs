use crate::ThemedBorder;
use gpui::{AnyElement, App, Div, ElementId, Rgba, SharedString, Window, div, prelude::*, px};

use super::{Kbd, OnClick, color, popover_shadow};
use crate::StyledExt;
use crate::icon::icon_file;

type OnHover = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

pub fn popover() -> Div {
    div()
        .occlude()
        .p(px(4.))
        .rounded(px(color::radius_lg()))
        .border_themed()
        .border_color(color::border())
        .bg(color::popover())
        .shadow(popover_shadow())
        .text_color(color::foreground())
}

#[derive(IntoElement)]
pub struct MenuItem {
    kind: Kind,
}

enum Kind {
    Item {
        id: ElementId,
        label: SharedString,
        icon: Option<SharedString>,
        checked: Option<bool>,
        shortcut: Option<SharedString>,
        disabled: bool,
        danger: bool,
        highlighted: bool,
        on_click: Option<OnClick>,
        on_hover: Option<OnHover>,
    },
    Separator,
    Label(SharedString),
}

impl MenuItem {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            kind: Kind::Item {
                id: id.into(),
                label: label.into(),
                icon: None,
                checked: None,
                shortcut: None,
                disabled: false,
                danger: false,
                highlighted: false,
                on_click: None,
                on_hover: None,
            },
        }
    }

    pub fn separator() -> Self {
        Self { kind: Kind::Separator }
    }

    pub fn heading(label: impl Into<SharedString>) -> Self {
        Self { kind: Kind::Label(label.into()) }
    }

    fn with(mut self, f: impl FnOnce(&mut Kind)) -> Self {
        f(&mut self.kind);
        self
    }

    pub fn icon(self, path: impl Into<SharedString>) -> Self {
        let path = path.into();
        self.with(|k| {
            if let Kind::Item { icon, .. } = k {
                *icon = Some(path)
            }
        })
    }

    pub fn checked(self, on: bool) -> Self {
        self.with(|k| {
            if let Kind::Item { checked, .. } = k {
                *checked = Some(on)
            }
        })
    }

    pub fn shortcut(self, chord: impl Into<SharedString>) -> Self {
        let chord = chord.into();
        self.with(|k| {
            if let Kind::Item { shortcut, .. } = k {
                *shortcut = Some(chord)
            }
        })
    }

    pub fn disabled(self, on: bool) -> Self {
        self.with(|k| {
            if let Kind::Item { disabled, .. } = k {
                *disabled = on
            }
        })
    }

    pub fn danger(self, on: bool) -> Self {
        self.with(|k| {
            if let Kind::Item { danger, .. } = k {
                *danger = on
            }
        })
    }

    pub fn highlighted(self, on: bool) -> Self {
        self.with(|k| {
            if let Kind::Item { highlighted, .. } = k {
                *highlighted = on
            }
        })
    }

    pub fn on_click(self, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        let f: OnClick = std::rc::Rc::new(f);
        self.with(|k| {
            if let Kind::Item { on_click, .. } = k {
                *on_click = Some(f)
            }
        })
    }

    pub fn on_hover(self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        let f: OnHover = std::rc::Rc::new(f);
        self.with(|k| {
            if let Kind::Item { on_hover, .. } = k {
                *on_hover = Some(f)
            }
        })
    }
}

impl RenderOnce for MenuItem {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        match self.kind {
            Kind::Separator => div().mx(px(-4.)).my(px(4.)).h(px(1.)).bg(color::border()).into_any_element(),
            Kind::Label(label) => {
                div().h(px(24.)).px(px(8.)).hrow().text(11., color::placeholder()).child(label).into_any_element()
            }
            Kind::Item { id, label, icon, checked, shortcut, disabled, danger, highlighted, on_click, on_hover } => {
                let fg: Rgba = if danger { color::danger() } else { color::foreground() };
                let lead: Option<AnyElement> = match (checked, icon) {
                    (Some(on), _) => Some(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .children(on.then(|| icon_file("icons/check.svg", 12., color::ring())))
                            .into_any_element(),
                    ),
                    (None, Some(path)) => Some(icon_file(&path, 14., color::muted_foreground())),
                    (None, None) => None,
                };
                div()
                    .id(id)
                    .hrow()
                    .gap(px(6.))
                    .h(px(26.))
                    .px(px(6.))
                    .rounded(px(color::radius_sm()))
                    .text(13., fg)
                    .when(disabled, |el| el.opacity(0.5))
                    .when(highlighted && !disabled, |el| el.bg(color::accent_active()))
                    .when(!disabled, |el| {
                        el.cursor_pointer()
                            .hover(|s| s.bg(color::accent_active()))
                            .when_some(on_click, |el, f| el.on_click(move |e, w, cx| f(e, w, cx)))
                            .when_some(on_hover, |el, f| {
                                el.on_hover(move |hovered, w, cx| {
                                    if *hovered {
                                        f(w, cx)
                                    }
                                })
                            })
                    })
                    .children(lead)
                    .child(div().flex_1().min_w_0().child(crate::text::text(label)))
                    .when_some(shortcut, |el, chord| el.child(Kbd::new(chord).bare()))
                    .into_any_element()
            }
        }
    }
}
