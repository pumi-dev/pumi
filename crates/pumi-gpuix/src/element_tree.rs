//! What the renderer reports about its tree beside events: painted highlights. Events are
//! `pumi_ui_proto`'s.

/// One highlight wash painted in the last frame, with the boxes it drew.
///
/// The rects matter: a quad never lands in `getPaintedText()`, and a match that
/// soft-wraps must produce one box per visual row. Without the geometry the only
/// way to assert either is a screenshot.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "napi", napi_derive::napi(object))]
pub struct HighlightMatch {
    /// Numeric id of the element that painted the run.
    pub element_id: f64,
    /// Index of the run within that element. 0 for a plain `<text>`.
    pub sub: f64,
    /// The full string of the run, so `text.slice(start, end)` is the match.
    pub text: String,
    /// UTF-16 code-unit offsets into `text`, the units JS strings use.
    pub start: f64,
    pub end: f64,
    pub active: bool,
    pub rects: Vec<HighlightRect>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "napi", napi_derive::napi(object))]
pub struct HighlightRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl From<crate::text::PaintedHighlight> for HighlightMatch {
    fn from(painted: crate::text::PaintedHighlight) -> Self {
        Self {
            element_id: painted.element_id as f64,
            sub: painted.sub as f64,
            text: painted.text.to_string(),
            start: painted.start as f64,
            end: painted.end as f64,
            active: painted.active,
            rects: painted
                .rects
                .into_iter()
                .map(|(x, y, width, height)| HighlightRect {
                    x: x as f64,
                    y: y as f64,
                    width: width as f64,
                    height: height as f64,
                })
                .collect(),
        }
    }
}

/// GPUI's modifiers as the protocol's.
pub(crate) fn modifiers(m: gpui::Modifiers) -> pumi_ui_proto::Modifiers {
    pumi_ui_proto::Modifiers {
        shift: m.shift,
        ctrl: m.control,
        alt: m.alt,
        cmd: m.platform, // platform = Cmd on macOS, Win on Windows
    }
}
