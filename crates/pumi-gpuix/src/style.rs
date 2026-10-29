//! How a style ([`StyleDesc`], the UI protocol's) is drawn with GPUI. The style's types are
//! `pumi_ui_proto::style`'s, re-exported here.
pub use pumi_ui_proto::style::*;

pub use crate::color::{parse_color, parse_color_hex};

/// A background as GPUI draws it.
pub(crate) trait BackgroundExt {
    /// The background GPUI paints itself: a colour or a two-stop linear gradient. Anything more
    /// (more stops, radial, conic, layers) is `None` here and painted by `crate::decor`.
    fn resolve(&self) -> Option<gpui::Background>;
    /// Whether this background needs the decoration painter.
    fn is_painted(&self) -> bool;
}

impl BackgroundExt for BackgroundValue {
    fn resolve(&self) -> Option<gpui::Background> {
        match self {
            Self::Color(value) => crate::color::parse_color_rgba(value).map(Into::into),
            Self::Gradient(gradient) => native_gradient(gradient),
            Self::Layers(_) => None,
        }
    }

    fn is_painted(&self) -> bool {
        match self {
            Self::Color(_) => false,
            Self::Gradient(gradient) => native_gradient(gradient).is_none(),
            Self::Layers(_) => true,
        }
    }
}

fn native_gradient(gradient: &GradientValue) -> Option<gpui::Background> {
    let GradientValue::LinearGradient { angle, stops, color_space } = gradient else {
        return None;
    };
    let [from, to] = stops.as_slice() else {
        return None;
    };
    let angle = *angle as f32;
    let from_position = from.position.unwrap_or(0.0) as f32;
    let to_position = to.position.unwrap_or(1.0) as f32;
    if !angle.is_finite() || !(0.0..=1.0).contains(&from_position) || !(0.0..=1.0).contains(&to_position) {
        return None;
    }

    let color_space = match color_space.as_deref() {
        None | Some("srgb") => gpui::ColorSpace::Srgb,
        Some("oklab") => gpui::ColorSpace::Oklab,
        _ => return None,
    };
    Some(
        gpui::linear_gradient(
            angle,
            gpui::linear_color_stop(crate::color::parse_color_rgba(&from.color)?, from_position),
            gpui::linear_color_stop(crate::color::parse_color_rgba(&to.color)?, to_position),
        )
        .color_space(color_space),
    )
}

/// A style as GPUI draws it.
pub(crate) trait StyleExt {
    fn resolved_background(&self) -> Option<gpui::Background>;
    /// Whether anything here is painted by `crate::decor` rather than by GPUI's quad.
    fn has_decor(&self) -> bool;
}

impl StyleExt for StyleDesc {
    fn resolved_background(&self) -> Option<gpui::Background> {
        if let Some(value) = self.background_color.as_deref() {
            return crate::color::parse_color_rgba(value).map(Into::into);
        }
        self.background.as_ref()?.resolve()
    }

    fn has_decor(&self) -> bool {
        self.background.as_ref().is_some_and(BackgroundValue::is_painted)
            || self.background_image.is_some()
            || self.border_image.is_some()
            || self.has_side_colors()
    }
}

/// Whether this style should insert a mouse hitbox.
///
/// GPUI only hit-tests elements that own a hitbox. A painted overlay without
/// one stays visible while clicks fall through. CSS `pointer-events` maps
/// here: `none` never blocks, `auto` always does. Unset follows the painted
/// surface: a fill or an absolute/fixed box blocks.
///
/// In-flow fills use BlockMouseExceptScroll so a parent scroller still gets
/// the wheel. `occlude()` (BlockMouse) is only for overlays that steal it.
pub fn should_occlude(style: &StyleDesc) -> bool {
    match style.pointer_events.as_deref() {
        Some("none") => return false,
        Some("auto") => return true,
        _ => {}
    }
    if matches!(style.position.as_deref(), Some("absolute") | Some("fixed")) {
        return true;
    }
    if !style.has_background() {
        return false;
    }
    match style.resolved_background() {
        Some(background) => !background.is_transparent(),
        None => true,
    }
}

/// Map a CSS `cursor` keyword onto a GPUI cursor. Unknown keywords return
/// `None` so the property is ignored, like every other invalid style value.
///
/// `ResizeUpLeftDownRight` is the NorthWest/SouthEast cursor on every backend,
/// so it is `nwse-resize`. GPUI's doc comments and its browser backend named
/// the opposite CSS values until the pinned fork corrected them, so do not
/// "fix" this pair back by reading an older GPUI.
pub fn parse_cursor(name: &str) -> Option<gpui::CursorStyle> {
    use gpui::CursorStyle;
    Some(match name {
        "default" | "auto" => CursorStyle::Arrow,
        "pointer" => CursorStyle::PointingHand,
        "text" => CursorStyle::IBeam,
        "vertical-text" => CursorStyle::IBeamCursorForVerticalLayout,
        "crosshair" => CursorStyle::Crosshair,
        "grab" => CursorStyle::OpenHand,
        "grabbing" | "move" | "all-scroll" => CursorStyle::ClosedHand,
        "col-resize" => CursorStyle::ResizeColumn,
        "row-resize" => CursorStyle::ResizeRow,
        "ew-resize" => CursorStyle::ResizeLeftRight,
        "ns-resize" => CursorStyle::ResizeUpDown,
        "nwse-resize" | "nw-resize" | "se-resize" => CursorStyle::ResizeUpLeftDownRight,
        "nesw-resize" | "ne-resize" | "sw-resize" => CursorStyle::ResizeUpRightDownLeft,
        "w-resize" => CursorStyle::ResizeLeft,
        "e-resize" => CursorStyle::ResizeRight,
        "n-resize" => CursorStyle::ResizeUp,
        "s-resize" => CursorStyle::ResizeDown,
        "not-allowed" | "no-drop" => CursorStyle::OperationNotAllowed,
        "alias" => CursorStyle::DragLink,
        "copy" => CursorStyle::DragCopy,
        "context-menu" => CursorStyle::ContextualMenu,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_fill(fill: &str) -> StyleDesc {
        StyleDesc { background_color: Some(fill.to_owned()), ..Default::default() }
    }

    #[test]
    fn an_offset_is_a_length_of_its_parent() {
        // What a float's anchor and its row/col say: cells, a length in px, or a share of the box
        // it floats over (`runtime/app/app/theme/default.tsx`'s `placement`).
        let style: StyleDesc =
            serde_json::from_str(r##"{"left":"25%","top":20,"right":"24px","bottom":"auto"}"##).unwrap();
        assert_eq!(style.left, Some(DimensionValue::Percentage(0.25)));
        assert_eq!(style.top, Some(DimensionValue::Pixels(20.0)));
        assert_eq!(style.right, Some(DimensionValue::Pixels(24.0)));
        assert_eq!(style.bottom, Some(DimensionValue::Auto));
    }

    #[test]
    fn transparent_function_does_not_occlude() {
        assert!(!should_occlude(&with_fill("transparent")));
        assert!(!should_occlude(&with_fill("oklch(50% 0.2 30 / 0%)")));
    }

    #[test]
    fn invalid_fill_keeps_conservative_occlusion() {
        assert!(should_occlude(&with_fill("not-a-color")));
    }

    #[test]
    fn resolves_a_two_stop_linear_gradient() {
        let style: StyleDesc = serde_json::from_str(
            r##"{"background":{"type":"linear-gradient","angle":90,"stops":[{"color":"#ff0000","position":0},{"color":"#0000ff","position":1}],"colorSpace":"oklab"}}"##,
        )
        .unwrap();
        let expected = gpui::linear_gradient(
            90.0,
            gpui::linear_color_stop(crate::color::parse_color_rgba("#ff0000").unwrap(), 0.0),
            gpui::linear_color_stop(crate::color::parse_color_rgba("#0000ff").unwrap(), 1.0),
        )
        .color_space(gpui::ColorSpace::Oklab);

        assert_eq!(style.resolved_background(), Some(expected));
    }

    #[test]
    fn transparent_gradient_does_not_occlude() {
        let style: StyleDesc = serde_json::from_str(
            r##"{"background":{"type":"linear-gradient","angle":0,"stops":[{"color":"transparent","position":0},{"color":"#00000000","position":1}]}}"##,
        )
        .unwrap();

        assert!(!should_occlude(&style));
    }

    #[test]
    fn box_shadow_takes_one_shadow_or_a_list() {
        let one: StyleDesc = serde_json::from_str(
            r##"{"boxShadow":{"offsetX":0,"offsetY":4,"blurRadius":12,"spreadRadius":0,"color":"#00000033"}}"##,
        )
        .unwrap();
        let shadows = one.box_shadow.as_ref().unwrap().as_slice();
        assert_eq!(shadows.len(), 1);
        assert!(!shadows[0].inset, "a shadow is outside unless it says otherwise");

        let many: StyleDesc = serde_json::from_str(
            r##"{"boxShadow":[
                {"offsetX":0,"offsetY":1,"blurRadius":2,"spreadRadius":0,"color":"#0003"},
                {"offsetX":0,"offsetY":0,"blurRadius":0,"spreadRadius":1,"color":"#fff1","inset":true}
            ]}"##,
        )
        .unwrap();
        let shadows = many.box_shadow.as_ref().unwrap().as_slice();
        assert_eq!(shadows.len(), 2);
        assert!(shadows[1].inset);
        assert_eq!(shadows[1].spread_radius, 1.0);
    }

    #[test]
    fn maps_the_timeline_cursors() {
        assert_eq!(parse_cursor("col-resize"), Some(gpui::CursorStyle::ResizeColumn));
        assert_eq!(parse_cursor("grab"), Some(gpui::CursorStyle::OpenHand));
        assert_eq!(parse_cursor("grabbing"), Some(gpui::CursorStyle::ClosedHand));
        assert_eq!(parse_cursor("pointer"), Some(gpui::CursorStyle::PointingHand));
        assert_eq!(parse_cursor("default"), Some(gpui::CursorStyle::Arrow));
    }

    #[test]
    fn ignores_an_unknown_cursor() {
        assert_eq!(parse_cursor("zoom-in"), None);
        assert_eq!(parse_cursor("POINTER"), None);
    }

    #[test]
    fn deserializes_text_decoration() {
        let style: StyleDesc = serde_json::from_str(r#"{"textDecoration":"underline"}"#).unwrap();
        assert_eq!(style.text_decoration.as_deref(), Some("underline"));

        let style: StyleDesc = serde_json::from_str(r#"{"textDecoration":"line-through"}"#).unwrap();
        assert_eq!(style.text_decoration.as_deref(), Some("line-through"));

        let style: StyleDesc = serde_json::from_str(r#"{"textDecoration":"none"}"#).unwrap();
        assert_eq!(style.text_decoration.as_deref(), Some("none"));
    }
}
