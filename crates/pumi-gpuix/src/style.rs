use serde::{Deserialize, Deserializer, Serialize};

/// Font weight value — accepts both CSS strings ("bold", "700") and numbers (700).
/// JS style objects commonly use both `fontWeight: "bold"` and `fontWeight: 700`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FontWeightValue {
    Num(f64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxShadowValue {
    pub offset_x: f64,
    pub offset_y: f64,
    pub blur_radius: f64,
    pub spread_radius: f64,
    pub color: String,
    /// Draws inside the border box instead of around it, like CSS `inset`.
    #[serde(default)]
    pub inset: bool,
}

/// `boxShadow` is one shadow or a list of them, painted in the order given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoxShadowValues {
    Many(Vec<BoxShadowValue>),
    One(BoxShadowValue),
}

impl BoxShadowValues {
    pub fn as_slice(&self) -> &[BoxShadowValue] {
        match self {
            Self::Many(shadows) => shadows,
            Self::One(shadow) => std::slice::from_ref(shadow),
        }
    }
}

/// One colour stop. A stop without a position sits evenly between its neighbours, like CSS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinearGradientStopValue {
    pub color: String,
    #[serde(default)]
    pub position: Option<f64>,
}

/// A radial gradient's size: one radius (a circle), `[x, y]` radii, or omitted for the
/// farthest corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RadiusValue {
    One(f64),
    Pair([f64; 2]),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GradientValue {
    /// Two stops paint natively; more are rasterised.
    #[serde(rename = "linear-gradient")]
    LinearGradient {
        #[serde(default = "default_linear_angle")]
        angle: f64,
        stops: Vec<LinearGradientStopValue>,
        #[serde(rename = "colorSpace")]
        color_space: Option<String>,
    },
    /// `center` is a fraction of the box (`[0.5, 0.5]` by default).
    #[serde(rename = "radial-gradient")]
    RadialGradient {
        #[serde(default)]
        shape: Option<String>,
        #[serde(default)]
        center: Option<[f64; 2]>,
        #[serde(default)]
        radius: Option<RadiusValue>,
        stops: Vec<LinearGradientStopValue>,
    },
    /// `angle` is where the sweep starts, in degrees clockwise from the top.
    #[serde(rename = "conic-gradient")]
    ConicGradient {
        #[serde(default)]
        angle: f64,
        #[serde(default)]
        center: Option<[f64; 2]>,
        stops: Vec<LinearGradientStopValue>,
    },
}

fn default_linear_angle() -> f64 {
    180.0
}

/// One layer of a layered `background`: a colour or a gradient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundLayerValue {
    Color(String),
    Gradient(Box<GradientValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundValue {
    Color(String),
    Gradient(Box<GradientValue>),
    /// Painted like CSS: the first layer on top.
    Layers(Vec<BackgroundLayerValue>),
}

/// A length that is pixels (a number), a percentage (`"50%"`) or a keyword.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LengthValue {
    Px(f64),
    Str(String),
}

/// `backgroundSize`: `"auto"`, `"cover"`, `"contain"`, a width, or `[width, height]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundSizeValue {
    Width(f64),
    Keyword(String),
    Pair([LengthValue; 2]),
}

/// `backgroundPosition`: a keyword (`"center"`, `"top left"`) or `[x, y]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundPositionValue {
    Keyword(String),
    Pair([LengthValue; 2]),
}

/// One image layer of `backgroundImage`. What a layer leaves out comes from the element's
/// `backgroundSize`, `backgroundRepeat` and `backgroundPosition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImageLayer {
    pub src: String,
    #[serde(default)]
    pub size: Option<BackgroundSizeValue>,
    #[serde(default)]
    pub repeat: Option<String>,
    #[serde(default)]
    pub position: Option<BackgroundPositionValue>,
    #[serde(default)]
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundImageItem {
    Src(String),
    Layer(BackgroundImageLayer),
}

/// `backgroundImage`: a src, a layer, or layers (the first on top).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BackgroundImageValue {
    One(BackgroundImageItem),
    Many(Vec<BackgroundImageItem>),
}

impl BackgroundImageValue {
    pub fn layers(&self) -> Vec<BackgroundImageLayer> {
        let items: &[BackgroundImageItem] = match self {
            Self::One(item) => std::slice::from_ref(item),
            Self::Many(items) => items,
        };
        items
            .iter()
            .map(|item| match item {
                BackgroundImageItem::Src(src) => {
                    BackgroundImageLayer { src: src.clone(), size: None, repeat: None, position: None, opacity: None }
                }
                BackgroundImageItem::Layer(layer) => layer.clone(),
            })
            .collect()
    }
}

/// Four sides in CSS order: one value, `[vertical, horizontal]`, or `[top, right, bottom, left]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SidesValue {
    One(f64),
    Two([f64; 2]),
    Four([f64; 4]),
}

impl SidesValue {
    /// `[top, right, bottom, left]`.
    pub fn sides(&self) -> [f64; 4] {
        match *self {
            Self::One(v) => [v, v, v, v],
            Self::Two([v, h]) => [v, h, v, h],
            Self::Four(s) => s,
        }
    }
}

/// A nine-slice image drawn as the element's frame (CSS `border-image`). `slice` is in the
/// image's pixels, `width` in the window's (the slice by default). The middle is drawn too
/// unless `fill` is false.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BorderImageValue {
    pub src: String,
    pub slice: SidesValue,
    #[serde(default)]
    pub width: Option<SidesValue>,
    /// `"stretch"` (default), `"repeat"` or `"round"`, for the edges and the middle.
    #[serde(default)]
    pub repeat: Option<String>,
    #[serde(default)]
    pub fill: Option<bool>,
    /// Draws the frame this far outside the border box.
    #[serde(default)]
    pub outset: Option<f64>,
}

/// A dimension value that can be a number (pixels) or a string (percentage, auto, etc.)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[derive(Default)]
pub enum DimensionValue {
    Pixels(f64),
    Percentage(f64), // 0.0 to 1.0
    #[default]
    Auto,
}

impl<'de> Deserialize<'de> for DimensionValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct DimensionVisitor;

        impl<'de> Visitor<'de> for DimensionVisitor {
            type Value = DimensionValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or a string like '100%' or 'auto'")
            }

            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DimensionValue::Pixels(v))
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DimensionValue::Pixels(v as f64))
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DimensionValue::Pixels(v as f64))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let written = v.trim();
                if written == "auto" {
                    Ok(DimensionValue::Auto)
                } else if let Some(number) = written.strip_suffix('%') {
                    match number.trim().parse::<f64>() {
                        Ok(n) => Ok(DimensionValue::Percentage(n / 100.0)),
                        Err(_) => Err(de::Error::custom(format!("invalid percentage: {v}"))),
                    }
                } else if let Some(number) = written.strip_suffix("px") {
                    match number.trim().parse::<f64>() {
                        Ok(n) => Ok(DimensionValue::Pixels(n)),
                        Err(_) => Err(de::Error::custom(format!("invalid length: {v}"))),
                    }
                } else {
                    // Try to parse as a number
                    match written.parse::<f64>() {
                        Ok(n) => Ok(DimensionValue::Pixels(n)),
                        Err(_) => Err(de::Error::custom(format!("invalid dimension: {}", v))),
                    }
                }
            }
        }

        deserializer.deserialize_any(DimensionVisitor)
    }
}

/// Style description that can be serialized from JS
/// Note: This is only used for JSON deserialization, not direct napi binding
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleDesc {
    // Display
    pub display: Option<String>,
    pub visibility: Option<String>,

    // Flexbox
    pub flex_direction: Option<String>,
    pub flex_wrap: Option<String>,
    pub flex_grow: Option<f64>,
    pub flex_shrink: Option<f64>,
    pub flex_basis: Option<f64>,
    pub align_items: Option<String>,
    pub align_self: Option<String>,
    pub align_content: Option<String>,
    pub justify_content: Option<String>,
    pub gap: Option<f64>,
    pub row_gap: Option<f64>,
    pub column_gap: Option<f64>,
    pub grid_template_columns: Option<f64>,
    pub grid_template_rows: Option<f64>,
    pub grid_column_min: Option<String>,
    pub grid_row_min: Option<String>,

    // Sizing: numbers, or strings like "100%" or "auto"
    pub width: Option<DimensionValue>,
    pub height: Option<DimensionValue>,
    pub min_width: Option<DimensionValue>,
    pub min_height: Option<DimensionValue>,
    pub max_width: Option<DimensionValue>,
    pub max_height: Option<DimensionValue>,

    // Spacing (padding)
    pub padding: Option<f64>,
    pub padding_top: Option<f64>,
    pub padding_right: Option<f64>,
    pub padding_bottom: Option<f64>,
    pub padding_left: Option<f64>,

    // Spacing (margin)
    pub margin: Option<f64>,
    pub margin_top: Option<f64>,
    pub margin_right: Option<f64>,
    pub margin_bottom: Option<f64>,
    pub margin_left: Option<f64>,

    // Position. An offset is a length of its parent, as a size is: `20`, `"20px"` or a share
    // of it (`"25%"`), which is what a float's anchor and row/col are written in
    // (`runtime/app/app/theme/default.tsx`'s `placement`).
    pub position: Option<String>,
    pub top: Option<DimensionValue>,
    pub right: Option<DimensionValue>,
    pub bottom: Option<DimensionValue>,
    pub left: Option<DimensionValue>,

    /// Paint order among siblings: higher paints (and hit-tests) later. Ties keep tree order.
    pub z_index: Option<f64>,

    // Background & Colors
    pub background: Option<BackgroundValue>,
    pub background_color: Option<String>,
    pub background_image: Option<BackgroundImageValue>,
    pub background_size: Option<BackgroundSizeValue>,
    /// `"repeat"` (default), `"repeat-x"`, `"repeat-y"` or `"no-repeat"`.
    pub background_repeat: Option<String>,
    pub background_position: Option<BackgroundPositionValue>,
    /// `"pixelated"` samples this element's images (background, frame, `<img>`) by nearest
    /// neighbour, for pixel art. `"auto"` smooths them.
    pub image_rendering: Option<String>,
    pub color: Option<String>,
    pub opacity: Option<f64>,
    /// An `<svg>`'s turn about its centre, in degrees, clockwise: it moves neither its layout nor
    /// where it is hit.
    pub rotate: Option<f64>,

    // Border
    pub border_width: Option<f64>,
    pub border_top_width: Option<f64>,
    pub border_right_width: Option<f64>,
    pub border_bottom_width: Option<f64>,
    pub border_left_width: Option<f64>,
    pub border_color: Option<String>,
    /// One side's colour, over `borderColor`: bevels, inset and outset frames.
    pub border_top_color: Option<String>,
    pub border_right_color: Option<String>,
    pub border_bottom_color: Option<String>,
    pub border_left_color: Option<String>,
    pub border_image: Option<BorderImageValue>,
    pub border_radius: Option<f64>,
    pub border_top_left_radius: Option<f64>,
    pub border_top_right_radius: Option<f64>,
    pub border_bottom_left_radius: Option<f64>,
    pub border_bottom_right_radius: Option<f64>,
    pub box_shadow: Option<BoxShadowValues>,
    /// `"solid"` or `"dashed"`, for every side.
    pub border_style: Option<String>,
    /// Width divided by height, applied to whichever axis is not definite.
    pub aspect_ratio: Option<f64>,
    /// CSS `outline`: a line outside the border box that takes no layout
    /// space, so it can appear on focus without moving anything.
    pub outline_width: Option<f64>,
    pub outline_color: Option<String>,
    pub outline_offset: Option<f64>,

    // Text
    pub font_size: Option<f64>,
    pub font_family: Option<String>,
    pub font_weight: Option<FontWeightValue>,
    /// `"italic"` or `"normal"`.
    pub font_style: Option<String>,
    pub text_align: Option<String>,
    pub line_height: Option<f64>,
    pub white_space: Option<String>,
    pub text_overflow: Option<String>,
    pub line_clamp: Option<f64>,
    pub text_decoration: Option<String>,

    // Overflow
    pub overflow: Option<String>,
    pub overflow_x: Option<String>,
    pub overflow_y: Option<String>,
    /// Thumb colour for a container that scrolls (`overflow: "scroll"`). Setting
    /// it is what turns the native scrollbar on.
    pub scrollbar_color: Option<String>,
    /// Thickness of that thumb in px. Defaults to 6.
    pub scrollbar_width: Option<f64>,

    // Cursor
    pub cursor: Option<String>,
    /// `"drag"`: the window moves by this element (a title bar of the app's own); `"no-drag"`:
    /// not by this part of one.
    pub app_region: Option<String>,
    /// `"auto"` blocks mouse hits behind this element. `"none"` never does.
    /// Unset: block when this element paints a fill or is absolutely positioned.
    pub pointer_events: Option<String>,

    // Text selection. "none" opts an element and its subtree out of the
    // selection registry, so buttons and toolbars never start a drag.
    // Inherited down the tree like the CSS property of the same name.
    pub user_select: Option<String>,
    /// Selection wash colour for this subtree. Defaults to the theme accent at
    /// 35% opacity, the same tone Comet uses.
    pub selection_color: Option<String>,

    // Pseudo-selector styles — applied by GPUI natively (no JS round-trip).
    // Uses Box to avoid infinite-size struct (StyleDesc contains StyleDesc).
    pub hover: Option<Box<StyleDesc>>,
    pub active: Option<Box<StyleDesc>>,
    /// While focused after keyboard input, like CSS `:focus-visible`.
    pub focus_visible: Option<Box<StyleDesc>>,
    /// `false` keeps this element at full opacity while another control has
    /// keyboard focus. Not inherited.
    pub keyboard_focus_dim: Option<bool>,
}

pub use crate::color::{parse_color, parse_color_hex};

impl BackgroundValue {
    /// The background GPUI paints itself: a colour or a two-stop linear gradient. Anything more
    /// (more stops, radial, conic, layers) is `None` here and painted by `crate::decor`.
    fn resolve(&self) -> Option<gpui::Background> {
        match self {
            Self::Color(value) => crate::color::parse_color_rgba(value).map(Into::into),
            Self::Gradient(gradient) => native_gradient(gradient),
            Self::Layers(_) => None,
        }
    }

    /// Whether this background needs the decoration painter.
    pub(crate) fn is_painted(&self) -> bool {
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

impl StyleDesc {
    pub(crate) fn has_background(&self) -> bool {
        self.background_color.is_some()
            || self.background.is_some()
            || self.background_image.is_some()
            || self.border_image.is_some()
    }

    pub(crate) fn resolved_background(&self) -> Option<gpui::Background> {
        if let Some(value) = self.background_color.as_deref() {
            return crate::color::parse_color_rgba(value).map(Into::into);
        }
        self.background.as_ref()?.resolve()
    }

    /// Whether a side has its own colour, so `crate::decor` paints the border.
    pub(crate) fn has_side_colors(&self) -> bool {
        self.border_top_color.is_some()
            || self.border_right_color.is_some()
            || self.border_bottom_color.is_some()
            || self.border_left_color.is_some()
    }

    /// Whether anything here is painted by `crate::decor` rather than by GPUI's quad.
    pub(crate) fn has_decor(&self) -> bool {
        self.background.as_ref().is_some_and(BackgroundValue::is_painted)
            || self.background_image.is_some()
            || self.border_image.is_some()
            || self.has_side_colors()
    }

    /// `[top, right, bottom, left]` border widths, longhands over the shorthand.
    pub(crate) fn border_widths(&self) -> [f64; 4] {
        let all = self.border_width.unwrap_or(0.0);
        [
            self.border_top_width.unwrap_or(all),
            self.border_right_width.unwrap_or(all),
            self.border_bottom_width.unwrap_or(all),
            self.border_left_width.unwrap_or(all),
        ]
        .map(|w| w.max(0.0))
    }

    /// `[top-left, top-right, bottom-right, bottom-left]` radii.
    pub(crate) fn corner_radii(&self) -> [f64; 4] {
        let all = self.border_radius.unwrap_or(0.0);
        [
            self.border_top_left_radius.unwrap_or(all),
            self.border_top_right_radius.unwrap_or(all),
            self.border_bottom_right_radius.unwrap_or(all),
            self.border_bottom_left_radius.unwrap_or(all),
        ]
        .map(|r| r.max(0.0))
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
