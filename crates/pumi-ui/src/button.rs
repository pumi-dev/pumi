use crate::ThemedBorder;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Div, ElementId, Rgba, SharedString, Stateful, StyleRefinement,
    Transformation, Window, div, percentage, prelude::*, px, svg,
};

use super::{Layers, Sizable, Size, Tooltip, color, delegate_div, focus_ring};
use crate::StyledExt;

pub const SPINNER: &str = "icons/loader_circle.svg";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonVariant {
    #[default]
    Secondary,
    Primary,
    Ghost,
    Danger,
}

#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    base: Stateful<Div>,
    layers: Layers,
    children: Vec<AnyElement>,
    label: Option<SharedString>,
    icon: Option<SharedString>,
    icon_color: Option<Rgba>,
    icon_size: Option<f32>,
    suffix: Option<SharedString>,
    variant: ButtonVariant,
    size: Size,
    selected: bool,
    disabled: bool,
    loading: bool,
    tab_stop: bool,
    tooltip: Option<Tooltip>,
}

delegate_div!(Button);

impl ParentElement for Button {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            layers: Layers::default(),
            children: Vec::new(),
            label: None,
            icon: None,
            icon_color: None,
            icon_size: None,
            suffix: None,
            variant: ButtonVariant::default(),
            size: Size::default(),
            selected: false,
            disabled: false,
            loading: false,
            tab_stop: false,
            tooltip: None,
        }
    }

    pub fn icon_only(id: impl Into<ElementId>, icon: impl Into<SharedString>) -> Self {
        Self::new(id).icon(icon).ghost()
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn icon_color(mut self, color: Rgba) -> Self {
        self.icon_color = Some(color);
        self
    }

    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = Some(size);
        self
    }

    pub fn suffix(mut self, icon: impl Into<SharedString>) -> Self {
        self.suffix = Some(icon.into());
        self
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn primary(self) -> Self {
        self.variant(ButtonVariant::Primary)
    }

    pub fn ghost(self) -> Self {
        self.variant(ButtonVariant::Ghost)
    }

    pub fn danger(self) -> Self {
        self.variant(ButtonVariant::Danger)
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    pub fn tooltip(mut self, text: impl Into<SharedString>) -> Self {
        self.tooltip = Some(Tooltip::new(text));
        self
    }

    pub fn tooltip_with_shortcut(mut self, text: impl Into<SharedString>, chord: impl Into<SharedString>) -> Self {
        self.tooltip = Some(Tooltip::new(text).shortcut(chord));
        self
    }

    fn icon_only_square(&self) -> bool {
        self.label.is_none() && self.children.is_empty() && self.icon.is_some()
    }

    fn colors(&self) -> [(Rgba, Rgba, Rgba); 3] {
        let clear = color::transparent();
        match self.variant {
            ButtonVariant::Secondary => [
                (clear, color::input(), color::foreground()),
                (color::accent(), color::input_hover(), color::foreground()),
                (color::accent_active(), color::input_hover(), color::foreground()),
            ],
            ButtonVariant::Primary => [
                (color::primary(), color::primary(), color::primary_foreground()),
                (color::primary_hover(), color::primary_hover(), color::primary_foreground()),
                (color::primary_active(), color::primary_active(), color::primary_foreground()),
            ],
            ButtonVariant::Ghost => [
                (clear, clear, color::muted_foreground()),
                (color::accent(), clear, color::foreground()),
                (color::accent_active(), clear, color::foreground()),
            ],
            ButtonVariant::Danger => [
                (color::danger(), color::danger(), color::danger_foreground()),
                (color::danger_hover(), color::danger_hover(), color::danger_foreground()),
                (color::danger_active(), color::danger_active(), color::danger_foreground()),
            ],
        }
    }
}

pub(super) fn glyph(
    id: &ElementId,
    path: SharedString,
    size: f32,
    color: Rgba,
    spinning: bool,
    hover: Option<(SharedString, Rgba)>,
) -> AnyElement {
    let icon = svg()
        .flex_none()
        .size(px(size))
        .text_color(color)
        .when_some(hover, |icon, (group, color)| icon.group_hover(group, move |s| s.text_color(color)));
    if spinning {
        icon.path(SPINNER)
            .with_animation(
                ElementId::from((id.clone(), "spin")),
                Animation::new(Duration::from_millis(900)).repeat(),
                |icon, delta| icon.with_transformation(Transformation::rotate(percentage(delta))),
            )
            .into_any_element()
    } else {
        icon.path(path).into_any_element()
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let [rest, hovered, pressed] = self.colors();
        let interactive = !self.disabled && !self.loading;
        let (bg, border, fg) = if self.selected { pressed } else { rest };
        let square = self.icon_only_square();
        let size = self.size;
        let icon_size = self.icon_size.unwrap_or(size.icon());
        let group = self.layers.group.clone().unwrap_or_else(|| SharedString::from(format!("kit-button-{}", self.id)));
        let icon_color = self.icon_color;
        let part = |path: SharedString, spinning: bool| {
            let hover = (icon_color.is_none() && interactive && !self.selected).then(|| (group.clone(), hovered.2));
            glyph(&self.id, path, icon_size, icon_color.unwrap_or(fg), spinning, hover)
        };
        let icon = match (&self.icon, self.loading) {
            (_, true) => Some(part(SPINNER.into(), true)),
            (Some(path), false) => Some(part(path.clone(), false)),
            (None, false) => None,
        };
        let suffix = self.suffix.clone().map(|path| part(path, false));

        let mut layers = self.layers;
        layers.group = Some(group);
        let on_click = layers.on_click.take();
        let el = self
            .base
            .flex_none()
            .hrow()
            .justify_center()
            .gap(px(if size == Size::Large { 6. } else { 4. }))
            .rounded(px(size.radius()))
            .border_themed()
            .border_color(border)
            .bg(bg)
            .text_size(px(size.text()))
            .text_color(fg)
            .line_height(gpui::relative(1.25))
            .map(
                |el| if square { el.size(px(size.square())) } else { el.h(px(size.height())).px(px(size.padding_x())) },
            )
            .when(self.disabled, |el| el.opacity(0.5).cursor_default())
            .when(self.loading && !self.disabled, |el| el.opacity(0.8))
            .when(interactive, |el| el.cursor_pointer())
            .when_some(on_click.filter(|_| interactive), |el, on_click| {
                el.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .when(self.tab_stop && interactive, |el| {
                let ghost = self.variant == ButtonVariant::Ghost;
                el.tab_index(0)
                    .focus(move |s| if ghost { focus_ring(s).bg(color::accent()) } else { focus_ring(s) })
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| window.prevent_default())
            })
            .when_some(self.tooltip, |el, tooltip| el.tooltip(move |window, cx| tooltip.clone().build(window, cx)))
            .children(icon)
            .when_some(self.label, |el, label| el.child(div().min_w_0().child(crate::text::text(label))))
            .children(self.children)
            .children(suffix);
        let state = |(bg, border, fg): (Rgba, Rgba, Rgba)| {
            interactive.then(|| StyleRefinement::default().bg(bg).border_color(border).text_color(fg))
        };
        let hover = state(if self.selected { pressed } else { hovered });
        layers.finish(el, hover, state(pressed))
    }
}

#[derive(IntoElement)]
pub struct ButtonGroup {
    id: ElementId,
    buttons: Vec<Button>,
    size: Size,
    disabled: bool,
}

impl Sizable for ButtonGroup {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl ButtonGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self { id: id.into(), buttons: Vec::new(), size: Size::default(), disabled: false }
    }

    pub fn button(mut self, button: Button) -> Self {
        self.buttons.push(button);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for ButtonGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let last = self.buttons.len().saturating_sub(1);
        let (size, disabled) = (self.size, self.disabled);
        let radius = px(size.radius());
        div().id(self.id).flex_none().hrow().children(self.buttons.into_iter().enumerate().map(move |(i, b)| {
            let b = b.with_size(size).when(disabled, |b| b.disabled(true)).rounded_none();
            let b = if i > 0 { b.border_l_0() } else { b };
            let b = if i == 0 { b.rounded_l(radius) } else { b };
            if i == last { b.rounded_r(radius) } else { b }
        }))
    }
}
