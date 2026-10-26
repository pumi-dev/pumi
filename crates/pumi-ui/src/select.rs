use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Anchor, App, Bounds, ElementId, FocusHandle, Pixels, SharedString, Window, anchored, deferred, div, point,
    prelude::*, px,
};

use super::{Button, MenuItem, Sizable, Size, StyledExt, color, popover};

type OnSelect = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;
type OnToggle = Rc<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
type OnDismiss = Rc<dyn Fn(&Dismiss, &mut Window, &mut App)>;

pub struct Dismiss;

#[derive(IntoElement)]
pub struct Select {
    id: SharedString,
    options: Vec<(SharedString, SharedString)>,
    selected: SharedString,
    open: bool,
    size: Size,
    searchable: bool,
    menu_width: f32,
    on_toggle: Option<OnToggle>,
    on_dismiss: Option<OnDismiss>,
    on_select: Option<OnSelect>,
}

impl Sizable for Select {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Select {
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            options: Vec::new(),
            selected: SharedString::default(),
            open: false,
            size: Size::Medium,
            searchable: false,
            menu_width: 200.,
            on_toggle: None,
            on_dismiss: None,
            on_select: None,
        }
    }

    pub fn options(
        mut self,
        options: impl IntoIterator<Item = (impl Into<SharedString>, impl Into<SharedString>)>,
    ) -> Self {
        self.options = options.into_iter().map(|(v, l)| (v.into(), l.into())).collect();
        self
    }

    pub fn selected(mut self, value: impl Into<SharedString>) -> Self {
        self.selected = value.into();
        self
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn menu_width(mut self, width: f32) -> Self {
        self.menu_width = width;
        self
    }

    /// A field on top of the menu that filters it as you type: for a list too long to scroll.
    pub fn searchable(mut self, searchable: bool) -> Self {
        self.searchable = searchable;
        self
    }

    pub fn on_toggle(mut self, f: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    pub fn on_dismiss(mut self, f: impl Fn(&Dismiss, &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    pub fn on_select(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

struct State {
    focus: FocusHandle,
    highlighted: Option<usize>,
    /// What was typed into a `searchable` menu: the options it keeps.
    query: String,
    was_open: bool,
    previous: Option<FocusHandle>,
    trigger: Rc<Cell<Bounds<Pixels>>>,
}

impl RenderOnce for Select {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let id = self.id.clone();
        let state = window.use_keyed_state(ElementId::Name(format!("{id}-state").into()), cx, |_, cx| State {
            focus: cx.focus_handle(),
            highlighted: None,
            query: String::new(),
            was_open: false,
            previous: None,
            trigger: Rc::new(Cell::new(Bounds::default())),
        });
        let current = self.options.iter().position(|(v, _)| *v == self.selected);
        let label = current.map(|i| self.options[i].1.clone()).unwrap_or_else(|| self.selected.clone());

        let (was_open, focus) = {
            let s = state.read(cx);
            (s.was_open, s.focus.clone())
        };
        if self.open && !was_open {
            let previous = window.focused(cx);
            focus.focus(window, cx);
            state.update(cx, |s, _| {
                s.was_open = true;
                s.highlighted = current;
                s.query.clear();
                s.previous = previous;
            });
        } else if !self.open && was_open {
            let previous = state.update(cx, |s, _| {
                s.was_open = false;
                s.previous.take()
            });
            if focus.is_focused(window)
                && let Some(previous) = previous
            {
                previous.focus(window, cx);
            }
        }

        let toggle = self.on_toggle.clone();
        let trigger = Button::new(ElementId::Name(format!("{id}-trigger").into()))
            .label(label)
            .suffix("icons/chevron_up_down.svg")
            .icon_size(12.)
            .with_size(self.size)
            .tab_stop(true)
            .bg(color::background())
            .when(self.open, |b| b.border_color(color::ring()))
            .when_some(toggle, |b, f| b.on_click(move |e, w, cx| f(e, w, cx)));

        let trigger_bounds = state.read(cx).trigger.clone();
        let measured = trigger_bounds.clone();
        let trigger = div()
            .relative()
            .child(trigger)
            .child(gpui::canvas(move |b, _, _| measured.set(b), |_, _, _, _| {}).absolute().inset_0());

        let menu = self.open.then(|| {
            let query = state.read(cx).query.clone();
            let keeps = |label: &SharedString| {
                self.searchable && !query.is_empty() && !label.to_lowercase().contains(&query.to_lowercase())
            };
            let shown: Vec<usize> =
                self.options.iter().enumerate().filter(|(_, (_, label))| !keeps(label)).map(|(i, _)| i).collect();
            let highlighted = state.read(cx).highlighted.filter(|i| *i < shown.len());
            let count = shown.len();
            let (on_select, on_dismiss) = (self.on_select.clone(), self.on_dismiss.clone());
            let options = self.options.clone();
            let keys_state = state.clone();
            let keys_options = self.options.clone();
            let searchable = self.searchable;
            let items = shown.iter().enumerate().map(|(at, index)| {
                let (value, label) = options[*index].clone();
                let pick = self.on_select.clone();
                let hover_state = state.clone();
                MenuItem::new(ElementId::Name(format!("{id}-option-{index}").into()), label)
                    .checked(value == self.selected)
                    .highlighted(highlighted == Some(at))
                    .on_hover(move |_, cx| {
                        hover_state.update(cx, |s, cx| {
                            if s.highlighted != Some(at) {
                                s.highlighted = Some(at);
                                cx.notify();
                            }
                        })
                    })
                    .when_some(pick, |item, f| item.on_click(move |_, w, cx| f(&value, w, cx)))
            });
            let empty = shown
                .is_empty()
                .then(|| div().px(px(8.)).py(px(6.)).text(12., color::placeholder()).child("Nothing matches"));
            let dismiss = self.on_dismiss.clone();
            let typed = !query.is_empty();
            deferred(
                anchored()
                    .position(point(trigger_bounds.get().right(), trigger_bounds.get().bottom()))
                    .anchor(Anchor::TopRight)
                    .offset(point(px(0.), px(4.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        popover()
                            .id(ElementId::Name(format!("{id}-menu").into()))
                            .track_focus(&focus)
                            .min_w(px(self.menu_width))
                            .on_key_down(move |e, window, cx| {
                                let key = e.keystroke.key.as_str();
                                let at = keys_state.read(cx).highlighted;
                                let kept = |query: &str| -> Vec<usize> {
                                    (0..keys_options.len())
                                        .filter(|i| {
                                            query.is_empty()
                                                || keys_options[*i].1.to_lowercase().contains(&query.to_lowercase())
                                        })
                                        .collect()
                                };
                                // What the keyboard typed (`key_char` has the layout's character: a
                                // shifted letter, a space), unless a command chord took it.
                                let m = e.keystroke.modifiers;
                                let typed_char = (searchable && !m.control && !m.alt && !m.platform)
                                    .then(|| e.keystroke.key_char.clone())
                                    .flatten();
                                if searchable && key == "backspace" {
                                    keys_state.update(cx, |s, cx| {
                                        s.query.pop();
                                        let shown = kept(&s.query);
                                        s.highlighted = (!shown.is_empty()).then_some(0);
                                        cx.notify();
                                    });
                                } else if let Some(ch) = typed_char.filter(|ch| !ch.is_empty()) {
                                    keys_state.update(cx, |s, cx| {
                                        s.query.push_str(&ch);
                                        let shown = kept(&s.query);
                                        s.highlighted = (!shown.is_empty()).then_some(0);
                                        cx.notify();
                                    });
                                } else {
                                    let next = match key {
                                        "down" => Some(at.map_or(0, |i| (i + 1).min(count.saturating_sub(1)))),
                                        "up" => Some(at.map_or(0, |i| i.saturating_sub(1))),
                                        "home" => Some(0),
                                        "end" => Some(count.saturating_sub(1)),
                                        _ => None,
                                    };
                                    if let Some(next) = next.filter(|_| count > 0) {
                                        keys_state.update(cx, |s, cx| {
                                            s.highlighted = Some(next);
                                            cx.notify();
                                        });
                                    } else if key == "enter" || (key == "space" && !searchable) {
                                        let picked = keys_state.update(cx, |s, _| {
                                            let shown = kept(&s.query);
                                            s.highlighted.and_then(|i| shown.get(i).copied())
                                        });
                                        if let (Some(index), Some(f)) = (picked, on_select.as_ref()) {
                                            f(&keys_options[index].0, window, cx);
                                        }
                                    } else if key == "escape" && typed {
                                        keys_state.update(cx, |s, cx| {
                                            s.query.clear();
                                            s.highlighted = Some(0);
                                            cx.notify();
                                        });
                                    } else if key == "escape" {
                                        if let Some(f) = on_dismiss.as_ref() {
                                            f(&Dismiss, window, cx);
                                        }
                                    } else {
                                        return;
                                    }
                                }
                                cx.stop_propagation();
                            })
                            .on_mouse_down_out(move |e, w, cx| {
                                if trigger_bounds.get().contains(&e.position) {
                                    return;
                                }
                                if let Some(f) = dismiss.as_ref() {
                                    f(&Dismiss, w, cx);
                                }
                            })
                            .when(self.searchable, |panel| {
                                let hint = if typed { query.clone() } else { "Type to filter".to_string() };
                                let color = if typed { color::foreground() } else { color::placeholder() };
                                panel.child(div().px(px(8.)).pb(px(4.)).text(12., color).child(hint))
                            })
                            .child(
                                div()
                                    .id(ElementId::Name(format!("{id}-list").into()))
                                    .flex()
                                    .flex_col()
                                    .max_h(px(300.))
                                    .overflow_y_scroll()
                                    .children(items)
                                    .children(empty),
                            ),
                    ),
            )
            .with_priority(2)
        });
        div().flex().flex_col().items_end().child(trigger).children(menu)
    }
}
