use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{App, Bounds, ElementId, Pixels, SharedString, Window, div, prelude::*, px, relative};

use super::{color, focus_ring};
use crate::with_alpha;

pub type OnSlide = Rc<dyn Fn(f32, &mut Window, &mut App)>;

thread_local! {
    static SLIDING: RefCell<Option<SharedString>> = const { RefCell::new(None) };
}

fn fraction_at(bounds: Bounds<Pixels>, x: Pixels) -> f32 {
    let width = f32::from(bounds.size.width).max(1.);
    (f32::from(x - bounds.origin.x) / width).clamp(0., 1.)
}

#[derive(IntoElement)]
pub struct Slider {
    id: SharedString,
    fraction: f32,
    width: f32,
    disabled: bool,
    on_change: Option<OnSlide>,
}

impl Slider {
    pub fn new(id: impl Into<SharedString>, fraction: f32) -> Self {
        Self { id: id.into(), fraction: fraction.clamp(0., 1.), width: 140., disabled: false, on_change: None }
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, f: OnSlide) -> Self {
        self.on_change = Some(f);
        self
    }
}

impl RenderOnce for Slider {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        const KNOB: f32 = 14.;
        const TRACK: f32 = 4.;
        let (id, fraction, width) = (self.id, self.fraction, self.width);
        let held = SLIDING.with(|s| s.borrow().as_ref() == Some(&id));
        let on_change = self.on_change.filter(|_| !self.disabled);
        let bounds = Rc::new(Cell::new(Bounds::default()));
        let group = SharedString::from(format!("kit-slider-{id}"));
        let tracker = {
            let (paint_id, paint_bounds, bounds, on_change) =
                (id.clone(), bounds.clone(), bounds.clone(), on_change.clone());
            gpui::canvas(
                move |b, _, _| paint_bounds.set(b),
                move |_, _, window, _| {
                    let Some(change) = on_change.clone() else { return };
                    if SLIDING.with(|s| s.borrow().as_ref() != Some(&paint_id)) {
                        return;
                    }
                    let moved = bounds.clone();
                    window.on_mouse_event(move |e: &gpui::MouseMoveEvent, phase, window, cx| {
                        if phase == gpui::DispatchPhase::Bubble && e.pressed_button == Some(gpui::MouseButton::Left) {
                            change(fraction_at(moved.get(), e.position.x), window, cx);
                        }
                    });
                    window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, window, _| {
                        if phase == gpui::DispatchPhase::Bubble {
                            SLIDING.with(|s| s.borrow_mut().take());
                            window.refresh();
                        }
                    });
                },
            )
            .absolute()
            .inset_0()
        };
        let halo = |spread: f32| {
            vec![
                gpui::BoxShadow {
                    color: with_alpha(color::ring(), 0.35).into(),
                    offset: gpui::point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(spread),
                    inset: false,
                },
                gpui::BoxShadow {
                    color: gpui::hsla(0., 0., 0., 0.4),
                    offset: gpui::point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                    inset: false,
                },
            ]
        };
        let knob = div()
            .absolute()
            .top(px(3.))
            .left(px((width - KNOB) * fraction))
            .size(px(KNOB))
            .rounded_full()
            .bg(color::thumb())
            .shadow(if held { halo(4.) } else { halo(0.) })
            .when(!self.disabled && !held, |k| k.group_hover(group.clone(), move |s| s.shadow(halo(3.))));
        let mut el = div()
            .id(ElementId::Name(id.clone()))
            .group(group)
            .relative()
            .flex_none()
            .w(px(width))
            .h(px(KNOB + 6.))
            .rounded_full()
            .when(self.disabled, |el| el.opacity(0.5));
        if let Some(change) = on_change {
            let (press_bounds, press_id) = (bounds.clone(), id.clone());
            let press = change.clone();
            el = el
                .cursor_pointer()
                .tab_index(0)
                .focus(focus_ring)
                .on_mouse_down(gpui::MouseButton::Left, move |e, window, cx| {
                    SLIDING.with(|s| *s.borrow_mut() = Some(press_id.clone()));
                    press(fraction_at(press_bounds.get(), e.position.x), window, cx);
                    window.prevent_default();
                    cx.stop_propagation();
                    window.refresh();
                })
                .on_key_down(move |e, window, cx| {
                    let to = match e.keystroke.key.as_str() {
                        "left" | "down" => fraction - 0.05,
                        "right" | "up" => fraction + 0.05,
                        "home" => 0.,
                        "end" => 1.,
                        _ => return,
                    };
                    change(to.clamp(0., 1.), window, cx);
                    cx.stop_propagation();
                });
        }
        el.child(tracker)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px((KNOB + 6. - TRACK) / 2.))
                    .h(px(TRACK))
                    .rounded_full()
                    .bg(color::track())
                    .child(div().h_full().w(relative(fraction)).rounded_full().bg(color::primary())),
            )
            .child(knob)
    }
}
