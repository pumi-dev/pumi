//! What `setStyle` carries: an element's style, a subset of CSS written in camelCase.
//!
//! These are the payload's types only. How a style is drawn is the renderer's (`pumi-gpuix`).
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// Font weight value: a CSS keyword or numeric string (`"bold"`, `"700"`) or a number (`700`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FontWeightValue {
    Num(f64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinearGradientStopValue {
    pub color: String,
    #[serde(default)]
    pub position: Option<f64>,
}

/// A radial gradient's size: one radius (a circle), `[x, y]` radii, or omitted for the
/// farthest corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RadiusValue {
    One(f64),
    Pair([f64; 2]),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BackgroundLayerValue {
    Color(String),
    Gradient(Box<GradientValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BackgroundValue {
    Color(String),
    Gradient(Box<GradientValue>),
    /// Painted like CSS: the first layer on top.
    Layers(Vec<BackgroundLayerValue>),
}

/// A length that is pixels (a number), a percentage (`"50%"`) or a keyword.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum LengthValue {
    Px(f64),
    Str(String),
}

/// `backgroundSize`: `"auto"`, `"cover"`, `"contain"`, a width, or `[width, height]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BackgroundSizeValue {
    Width(f64),
    Keyword(String),
    Pair([LengthValue; 2]),
}

/// `backgroundPosition`: a keyword (`"center"`, `"top left"`) or `[x, y]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BackgroundPositionValue {
    Keyword(String),
    Pair([LengthValue; 2]),
}

/// One image layer of `backgroundImage`. What a layer leaves out comes from the element's
/// `backgroundSize`, `backgroundRepeat` and `backgroundPosition`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BackgroundImageItem {
    Src(String),
    Layer(BackgroundImageLayer),
}

/// `backgroundImage`: a src, a layer, or layers (the first on top).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
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

/// Written as a number (pixels), or a string: `"auto"`, `"50%"`, `"12px"` or `"12"`.
impl JsonSchema for DimensionValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DimensionValue".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Pixels as a number, or a string: `\"auto\"`, a percentage (`\"50%\"`) or pixels (`\"12px\"`, `\"12\"`).",
            "type": ["number", "string"]
        })
    }
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

/// An element's style: what `setStyle` sets. Every field is optional; one left out is unset.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
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

impl StyleDesc {
    /// Whether anything paints a background: a colour, a gradient, an image or a frame.
    pub fn has_background(&self) -> bool {
        self.background_color.is_some()
            || self.background.is_some()
            || self.background_image.is_some()
            || self.border_image.is_some()
    }

    /// Whether a side has its own colour.
    pub fn has_side_colors(&self) -> bool {
        self.border_top_color.is_some()
            || self.border_right_color.is_some()
            || self.border_bottom_color.is_some()
            || self.border_left_color.is_some()
    }

    /// `[top, right, bottom, left]` border widths, longhands over the shorthand.
    pub fn border_widths(&self) -> [f64; 4] {
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
    pub fn corner_radii(&self) -> [f64; 4] {
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
