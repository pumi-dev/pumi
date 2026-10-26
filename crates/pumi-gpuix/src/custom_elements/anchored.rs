/// Anchored custom element for deferred, trigger-relative floating layers.
use super::{CustomElement, CustomElementFactory, CustomRenderContext};

pub struct AnchoredFactory;

impl CustomElementFactory for AnchoredFactory {
    fn element_type(&self) -> &str {
        "anchored"
    }

    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(AnchoredElement::default())
    }
}

#[derive(Debug, Clone, Copy)]
enum AnchorPoint {
    TopLeft,
    TopCenter,
    TopRight,
    RightCenter,
    BottomRight,
    BottomCenter,
    BottomLeft,
    LeftCenter,
}

impl AnchorPoint {
    fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "topLeft" => Self::TopLeft,
            "topCenter" => Self::TopCenter,
            "topRight" => Self::TopRight,
            "rightCenter" => Self::RightCenter,
            "bottomRight" => Self::BottomRight,
            "bottomCenter" => Self::BottomCenter,
            "bottomLeft" => Self::BottomLeft,
            "leftCenter" => Self::LeftCenter,
            _ => return None,
        })
    }

    fn as_gpui(self) -> gpui::Anchor {
        match self {
            Self::TopLeft => gpui::Anchor::TopLeft,
            Self::TopCenter => gpui::Anchor::TopCenter,
            Self::TopRight => gpui::Anchor::TopRight,
            Self::RightCenter => gpui::Anchor::RightCenter,
            Self::BottomRight => gpui::Anchor::BottomRight,
            Self::BottomCenter => gpui::Anchor::BottomCenter,
            Self::BottomLeft => gpui::Anchor::BottomLeft,
            Self::LeftCenter => gpui::Anchor::LeftCenter,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum Side {
    Top,
    Right,
    #[default]
    Bottom,
    Left,
}

impl Side {
    fn from_str(value: &str) -> Self {
        match value {
            "top" => Self::Top,
            "right" => Self::Right,
            "left" => Self::Left,
            _ => Self::Bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum Alignment {
    #[default]
    Start,
    Center,
    End,
}

impl Alignment {
    fn from_str(value: &str) -> Self {
        match value {
            "center" => Self::Center,
            "end" => Self::End,
            _ => Self::Start,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
enum FitMode {
    Switch,
    #[default]
    Snap,
}

impl FitMode {
    fn from_str(value: &str) -> Self {
        if value == "switch" {
            Self::Switch
        } else {
            Self::Snap
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnchoredElement {
    position: Option<(f32, f32)>,
    side: Side,
    align: Alignment,
    anchor: Option<AnchorPoint>,
    gap: f32,
    offset: (f32, f32),
    fit: FitMode,
    snap_margin: f32,
    deferred: bool,
    priority: usize,
    occlude: bool,
    /// `fill: "window"`: cover the whole viewport, like a dialog portal.
    fill_window: bool,
}

impl Default for AnchoredElement {
    fn default() -> Self {
        Self {
            position: None,
            side: Side::Bottom,
            align: Alignment::Start,
            anchor: None,
            gap: 0.0,
            offset: (0.0, 0.0),
            fit: FitMode::Snap,
            snap_margin: 8.0,
            deferred: true,
            priority: 1,
            occlude: true,
            fill_window: false,
        }
    }
}

impl AnchoredElement {
    fn resolved_anchor(&self) -> AnchorPoint {
        if let Some(anchor) = self.anchor {
            return anchor;
        }
        match (self.side, self.align) {
            (Side::Top, Alignment::Start) => AnchorPoint::BottomLeft,
            (Side::Top, Alignment::Center) => AnchorPoint::BottomCenter,
            (Side::Top, Alignment::End) => AnchorPoint::BottomRight,
            (Side::Right, Alignment::Start) => AnchorPoint::TopLeft,
            (Side::Right, Alignment::Center) => AnchorPoint::LeftCenter,
            (Side::Right, Alignment::End) => AnchorPoint::BottomLeft,
            (Side::Bottom, Alignment::Start) => AnchorPoint::TopLeft,
            (Side::Bottom, Alignment::Center) => AnchorPoint::TopCenter,
            (Side::Bottom, Alignment::End) => AnchorPoint::TopRight,
            (Side::Left, Alignment::Start) => AnchorPoint::TopRight,
            (Side::Left, Alignment::Center) => AnchorPoint::RightCenter,
            (Side::Left, Alignment::End) => AnchorPoint::BottomRight,
        }
    }

    fn resolved_offset(&self) -> gpui::Point<gpui::Pixels> {
        let (side_x, side_y) = match self.side {
            Side::Top => (0.0, -self.gap),
            Side::Right => (self.gap, 0.0),
            Side::Bottom => (0.0, self.gap),
            Side::Left => (-self.gap, 0.0),
        };
        gpui::point(gpui::px(side_x + self.offset.0), gpui::px(side_y + self.offset.1))
    }

    fn wrap_at_trigger(&self, layer: gpui::AnyElement) -> gpui::AnyElement {
        use gpui::prelude::*;

        match (self.side, self.align) {
            (Side::Top, Alignment::Start) => {
                gpui::div().absolute().top_0().left_0().size_0().child(layer).into_any_element()
            }
            (Side::Top, Alignment::Center) => gpui::div()
                .absolute()
                .top_0()
                .left_0()
                .h_0()
                .w_full()
                .flex()
                .justify_center()
                .child(layer)
                .into_any_element(),
            (Side::Top, Alignment::End) => {
                gpui::div().absolute().top_0().right_0().size_0().child(layer).into_any_element()
            }
            (Side::Right, Alignment::Start) => {
                gpui::div().absolute().top_0().right_0().size_0().child(layer).into_any_element()
            }
            (Side::Right, Alignment::Center) => gpui::div()
                .absolute()
                .top_0()
                .right_0()
                .w_0()
                .h_full()
                .flex()
                .items_center()
                .child(layer)
                .into_any_element(),
            (Side::Right, Alignment::End) => {
                gpui::div().absolute().bottom_0().right_0().size_0().child(layer).into_any_element()
            }
            (Side::Bottom, Alignment::Start) => {
                gpui::div().absolute().bottom_0().left_0().size_0().child(layer).into_any_element()
            }
            (Side::Bottom, Alignment::Center) => gpui::div()
                .absolute()
                .bottom_0()
                .left_0()
                .h_0()
                .w_full()
                .flex()
                .justify_center()
                .child(layer)
                .into_any_element(),
            (Side::Bottom, Alignment::End) => {
                gpui::div().absolute().bottom_0().right_0().size_0().child(layer).into_any_element()
            }
            (Side::Left, Alignment::Start) => {
                gpui::div().absolute().top_0().left_0().size_0().child(layer).into_any_element()
            }
            (Side::Left, Alignment::Center) => gpui::div()
                .absolute()
                .top_0()
                .left_0()
                .w_0()
                .h_full()
                .flex()
                .items_center()
                .child(layer)
                .into_any_element(),
            (Side::Left, Alignment::End) => {
                gpui::div().absolute().bottom_0().left_0().size_0().child(layer).into_any_element()
            }
        }
    }
}

impl CustomElement for AnchoredElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        _cx: &mut gpui::Context<crate::renderer::GpuixView>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        // The layer paints what it is styled with, and nothing of its own: a menu's card is most
        // often its child, rounded, and a fill here would be a square behind it, showing at the
        // card's corners. What it hides of a native page under it is what it paints; its
        // children record their own.
        let cover = ctx.style.and_then(crate::occlusion::cover);

        // The overlay, not the trigger, is what a user clicks, so the recorded
        // box has to be this content div after `anchored` placed it. Only gpui
        // knows where that is, so it reports its own painted box instead of
        // carrying a `bounds_tracker` child.
        let mut content = gpui::div().id(gpui::SharedString::from(format!("__gpuix_anchored_{}", ctx.id))).flex_col();
        content = crate::automation::track_host_bounds(content, ctx.id, None, None, cover);
        if let Some(style) = ctx.style {
            content = crate::renderer::apply_interactive_styles(content, style);
        }
        if self.fill_window {
            // `anchored()` is absolute and sized by its content, so a percentage
            // size has nothing to resolve against. A resize runs gpui's
            // `Window::bounds_changed`, which refreshes the window, so reading
            // the viewport here tracks it in the same frame, with no JS polling.
            // Applied after the JS style so it always wins over width/height.
            let viewport = window.viewport_size();
            content = content.w(viewport.width).h(viewport.height);
        }
        content = crate::accessibility::apply_accessibility(content, ctx.props, None);
        content = super::wire_standard_events(content, &ctx);
        if self.occlude {
            content = content.occlude();
        }
        for child in ctx.children {
            content = content.child(child);
        }

        let mut anchored = gpui::anchored();
        if self.fill_window {
            // Already the window: no offset, no snapping.
            anchored = anchored.anchor(gpui::Anchor::TopLeft).position(gpui::point(gpui::px(0.0), gpui::px(0.0)));
        } else {
            anchored = anchored.anchor(self.resolved_anchor().as_gpui()).offset(self.resolved_offset());
            if let Some((x, y)) = self.position {
                anchored = anchored.position(gpui::point(gpui::px(x), gpui::px(y)));
            }
            if matches!(self.fit, FitMode::Snap) {
                anchored = anchored.snap_to_window_with_margin(gpui::px(self.snap_margin));
            }
        }

        let anchored = anchored.child(content);
        let layer = if self.deferred {
            gpui::deferred(anchored).with_priority(self.priority).into_any_element()
        } else {
            anchored.into_any_element()
        };

        if self.position.is_some() || self.fill_window {
            layer
        } else {
            self.wrap_at_trigger(layer)
        }
    }

    fn set_prop(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "position" => {
                self.position = value.as_object().and_then(|position| {
                    Some((position.get("x")?.as_f64()? as f32, position.get("y")?.as_f64()? as f32))
                });
            }
            "side" => self.side = value.as_str().map(Side::from_str).unwrap_or_default(),
            "align" => self.align = value.as_str().map(Alignment::from_str).unwrap_or_default(),
            "anchor" => self.anchor = value.as_str().and_then(AnchorPoint::from_str),
            "gap" => self.gap = value.as_f64().unwrap_or(0.0) as f32,
            "offset" => {
                self.offset = value
                    .as_object()
                    .map(|offset| {
                        (
                            offset.get("x").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32,
                            offset.get("y").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32,
                        )
                    })
                    .unwrap_or_default();
            }
            "fit" => self.fit = value.as_str().map(FitMode::from_str).unwrap_or_default(),
            "snapMargin" => self.snap_margin = value.as_f64().unwrap_or(8.0) as f32,
            "deferred" => self.deferred = value.as_bool().unwrap_or(true),
            "priority" => {
                self.priority = value.as_u64().and_then(|priority| usize::try_from(priority).ok()).unwrap_or(1);
            }
            "occlude" => self.occlude = value.as_bool().unwrap_or(true),
            "fill" => self.fill_window = value.as_str() == Some("window"),
            _ => {}
        }
    }

    fn supported_props(&self) -> &'static [&'static str] {
        &[
            "position",
            "side",
            "align",
            "anchor",
            "gap",
            "offset",
            "fit",
            "snapMargin",
            "deferred",
            "priority",
            "occlude",
            "fill",
        ]
    }

    fn supported_events(&self) -> &'static [&'static str] {
        &["click", "mouseEnter", "mouseLeave", "mouseDownOutside", "fileDrop"]
    }

    fn destroy(&mut self) {}
}
