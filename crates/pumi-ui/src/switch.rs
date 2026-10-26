use std::time::Instant;

use gpui::{App, ElementId, Rgba, Window, div, prelude::*, px};

use super::{OnClick, Sizable, Size, color, focus_ring, progress};
use crate::with_alpha;

#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    size: Size,
    tab_stop: bool,
    on_click: Option<OnClick>,
}

impl Sizable for Switch {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), checked: false, disabled: false, size: Size::default(), tab_stop: true, on_click: None }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    pub fn on_click(mut self, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(std::rc::Rc::new(f));
        self
    }
}

struct Motion {
    checked: bool,
    since: Option<Instant>,
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (w, h, knob) = match self.size {
            Size::XSmall => (24., 14., 10.),
            Size::Small | Size::Medium => (32., 20., 14.),
            Size::Large => (40., 24., 18.),
        };
        let inset = (h - 2. - knob) / 2.;
        let checked = self.checked;
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| Motion { checked, since: None });
        if state.read(cx).checked != checked {
            state.update(cx, |m, _| *m = Motion { checked, since: Some(Instant::now()) });
        }
        let t = progress(state.read(cx).since, 160, window);
        let t = if checked { t } else { 1. - t };
        let travel = w - 2. - knob - inset * 2.;
        let (track, edge, edge_hover): (Rgba, Rgba, Rgba) = if checked {
            (with_alpha(color::ring(), 0.45), with_alpha(color::ring(), 0.6), color::ring())
        } else {
            (color::background(), color::input(), color::input_hover())
        };
        let interactive = !self.disabled;
        div()
            .id(self.id)
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(h))
            .rounded_full()
            .border_1()
            .border_color(edge)
            .bg(track)
            .when(self.disabled, |el| el.opacity(0.5))
            .when(interactive, |el| el.cursor_pointer().hover(move |s| s.border_color(edge_hover)))
            .when(interactive && self.tab_stop, |el| {
                el.tab_index(0)
                    .focus(focus_ring)
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| window.prevent_default())
            })
            .when_some(self.on_click.filter(|_| interactive), |el, f| el.on_click(move |e, w, cx| f(e, w, cx)))
            .child(
                div()
                    .absolute()
                    .top(px(inset))
                    .left(px(inset + travel * t))
                    .size(px(knob))
                    .rounded_full()
                    .bg(color::thumb())
                    .opacity(0.55 + 0.45 * t)
                    .shadow_sm(),
            )
    }
}
