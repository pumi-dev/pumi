//! Theme tokens for native text editors and document components.
//!
//! Ported from Comet (https://github.com/zeronsh/comet), MIT.
//! Original: `crates/ui/src/theme.rs`.
//!
//! Colours are declared in **oklch** exactly as the Tailwind v4 palette does, then
//! converted to `gpui::Hsla` here. Keeping the source notation means a token can be
//! checked against the Tailwind reference without reverse-engineering an HSL triple.
//!
//! Every token is overridable from JS: elements take a `theme` prop that
//! deserializes into [`ThemeOverride`] and is applied on top of [`Theme::dark`].
//! Unknown keys are ignored so a JS theme object can carry extra fields.

use gpui::{hsla, Hsla};
use serde::Deserialize;

// ── Colour helpers ───────────────────────────────────────────────────

/// Opaque grey from an 8-bit channel value (`grey(0x0e)` is `#0e0e0e`).
pub fn grey(value: u8) -> Hsla {
    hsla(0.0, 0.0, value as f32 / 255.0, 1.0)
}

/// Achromatic colour at an oklch lightness — the Tailwind neutral ramp.
pub fn neutral(lightness: f32) -> Hsla {
    let [v, _, _] = oklch_to_srgb(lightness, 0.0, 0.0);
    hsla(0.0, 0.0, v, 1.0)
}

/// oklch (CSS notation: L 0..1, C, H in degrees) to `gpui::Hsla`.
pub fn oklch(l: f32, c: f32, h_deg: f32) -> Hsla {
    let [r, g, b] = oklch_to_srgb(l, c, h_deg);
    let (h, s, l) = rgb_to_hsl(r, g, b);
    hsla(h, s, l, 1.0)
}

/// oklch to sRGB, each channel 0..1 and gamut-clipped.
/// Reference: Björn Ottosson's OKLab, the same matrices CSS Color 4 uses.
fn oklch_to_srgb(l: f32, c: f32, h_deg: f32) -> [f32; 3] {
    let h = h_deg.to_radians();
    let a = c * h.cos();
    let b = c * h.sin();

    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);

    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_93 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3;
    let b = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;

    [gamma_encode(r), gamma_encode(g), gamma_encode(b)]
}

fn gamma_encode(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

/// sRGB (0..1) to HSL with all components 0..1, gpui's `Hsla` convention.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;
    if delta < f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { delta / (2.0 - max - min) } else { delta / (max + min) };
    let h = if (max - r).abs() < f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (h / 6.0, s, l)
}

// ── Theme ────────────────────────────────────────────────────────────

/// The token set the native text components paint with.
///
/// This is a trimmed Comet theme: only tokens read by the native editors and
/// document elements remain. Surfaces, buttons and chrome tokens are the host
/// app's business and stay in JS as ordinary `style` props.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Content plane behind code and diff bodies.
    pub bg: Hsla,
    /// Hairline border.
    pub border: Hsla,
    /// Primary text.
    pub text: Hsla,
    /// Secondary labels, code-block language tag, copy button.
    pub text_muted: Hsla,
    /// Placeholders and diff gutter numbers.
    pub text_faint: Hsla,
    /// Diff file paths — one notch below `text_muted`, sampled in Comet as #989898.
    pub text_dim: Hsla,
    /// Accent (indigo) — list markers, blockquote rail, selection wash.
    pub accent: Hsla,
    /// Text-editor caret.
    pub caret: Hsla,
    /// Inline-code text, violet-300.
    pub code_text: Hsla,
    /// Rounded wash behind inline code, violet-400 at 12%.
    pub code_wash: Hsla,
    /// Diff added lines, emerald-400.
    pub diff_add: Hsla,
    /// Diff deleted lines, red-400.
    pub diff_del: Hsla,
    /// Diff hunk-header wash.
    pub diff_hunk_bg: Hsla,
    /// UI font family.
    pub font_sans: String,
    /// Monospace family for code, diffs and terminals.
    pub font_mono: String,
}

impl Theme {
    /// Comet's dark theme, token for token.
    pub fn dark() -> Self {
        Self {
            bg: grey(6),
            border: hsla(0.0, 0.0, 1.0, 0.08),
            text: neutral(0.922),
            text_muted: neutral(0.708),
            text_faint: neutral(0.556),
            text_dim: grey(0x98),
            accent: oklch(0.673, 0.182, 276.935),
            caret: oklch(0.673, 0.182, 276.935),
            code_text: oklch(0.811, 0.111, 293.571),
            code_wash: with_alpha(oklch(0.702, 0.183, 293.541), 0.12),
            diff_add: oklch(0.765, 0.177, 163.223),
            diff_del: oklch(0.704, 0.191, 22.216),
            diff_hunk_bg: hsla(0.6, 0.35, 0.6, 0.05),
            font_sans: system_sans().to_string(),
            font_mono: system_mono().to_string(),
        }
    }

    /// Comet's light theme.
    pub fn light() -> Self {
        Self {
            bg: grey(0xff),
            border: hsla(0.0, 0.0, 0.0, 0.10),
            text: neutral(0.25),
            text_muted: neutral(0.439),
            text_faint: neutral(0.535),
            text_dim: neutral(0.50),
            accent: oklch(0.511, 0.262, 276.966),
            caret: oklch(0.511, 0.262, 276.966),
            code_text: oklch(0.491, 0.27, 292.581),
            code_wash: with_alpha(oklch(0.541, 0.281, 293.009), 0.10),
            diff_add: oklch(0.596, 0.145, 163.225),
            diff_del: oklch(0.577, 0.245, 27.325),
            diff_hunk_bg: hsla(0.6, 0.35, 0.35, 0.07),
            font_sans: system_sans().to_string(),
            font_mono: system_mono().to_string(),
        }
    }

    /// Apply a JS-supplied override on top of this theme.
    pub fn with_override(mut self, o: &ThemeOverride) -> Self {
        if o.appearance.as_deref() == Some("light") {
            self = Theme::light();
        }
        set(&mut self.bg, &o.bg);
        set(&mut self.border, &o.border);
        set(&mut self.text, &o.text);
        set(&mut self.text_muted, &o.text_muted);
        set(&mut self.text_faint, &o.text_faint);
        set(&mut self.text_dim, &o.text_dim);
        set(&mut self.accent, &o.accent);
        set(&mut self.caret, &o.caret);
        set(&mut self.code_text, &o.code_text);
        set(&mut self.code_wash, &o.code_wash);
        set(&mut self.diff_add, &o.diff_add);
        set(&mut self.diff_del, &o.diff_del);
        set(&mut self.diff_hunk_bg, &o.diff_hunk_bg);
        if let Some(font) = &o.font_mono {
            self.font_mono = font.clone();
        }
        if let Some(font) = &o.font_sans {
            self.font_sans = font.clone();
        }
        self
    }

    /// Build from an optional JSON prop value. `null` or a parse failure yields dark.
    pub fn from_prop(value: Option<&serde_json::Value>) -> Self {
        let Some(value) = value.filter(|v| !v.is_null()) else {
            return Theme::dark();
        };
        match serde_json::from_value::<ThemeOverride>(value.clone()) {
            Ok(o) => Theme::dark().with_override(&o),
            Err(e) => {
                log::warn!("invalid theme prop, using dark: {e}");
                Theme::dark()
            }
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

fn set(slot: &mut Hsla, value: &Option<String>) {
    if let Some(color) = value.as_deref().and_then(crate::color::parse_color_rgba) {
        *slot = color.into();
    }
}

fn with_alpha(mut color: Hsla, alpha: f32) -> Hsla {
    color.a = alpha;
    color
}

fn system_sans() -> &'static str {
    if cfg!(target_family = "wasm") {
        "IBM Plex Sans"
    } else if cfg!(target_os = "macos") {
        "Helvetica"
    } else if cfg!(target_os = "windows") {
        "Segoe UI"
    } else {
        "DejaVu Sans"
    }
}

fn system_mono() -> &'static str {
    if cfg!(target_family = "wasm") {
        "Lilex"
    } else if cfg!(target_os = "macos") {
        "Menlo"
    } else if cfg!(target_os = "windows") {
        "Consolas"
    } else {
        "DejaVu Sans Mono"
    }
}

// ── JS-facing override ───────────────────────────────────────────────

/// A `theme` prop from JS. Every field is a CSS colour string and optional.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThemeOverride {
    /// `"dark"` (default) or `"light"` — picks the base before overrides apply.
    pub appearance: Option<String>,
    pub bg: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub text_muted: Option<String>,
    pub text_faint: Option<String>,
    pub text_dim: Option<String>,
    pub accent: Option<String>,
    pub caret: Option<String>,
    pub code_text: Option<String>,
    pub code_wash: Option<String>,
    pub diff_add: Option<String>,
    pub diff_del: Option<String>,
    pub diff_hunk_bg: Option<String>,
    pub font_sans: Option<String>,
    pub font_mono: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srgb_u8(c: [f32; 3]) -> [u8; 3] {
        [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
    }

    /// Reference values from CSS Color 4 matrices — the same assertion Comet makes.
    #[test]
    fn oklch_matches_tailwind_reference() {
        assert_eq!(srgb_u8(oklch_to_srgb(0.673, 0.182, 276.935)), [124, 134, 255]);
        assert_eq!(srgb_u8(oklch_to_srgb(0.704, 0.191, 22.216)), [255, 100, 103]);
        assert_eq!(srgb_u8(oklch_to_srgb(0.828, 0.189, 84.429)), [255, 185, 0]);
    }

    #[test]
    fn light_appearance_switches_the_base() {
        let o: ThemeOverride = serde_json::from_str(r#"{ "appearance": "light" }"#).unwrap();
        let t = Theme::dark().with_override(&o);
        assert_eq!(t.bg, grey(0xff));
    }
}
