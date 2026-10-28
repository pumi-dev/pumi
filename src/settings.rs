//! What the native half reads of the user's settings. They are pumid's global options (Settings
//! writes them, from React); this is the typed view the terminals and the updater follow.
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub terminal: TerminalSettings,
    pub updates: UpdateSettings,
}
/// The updater (`crate::updater`). Off, it never checks on its own; "Check for updates"
/// still does. `plugins` is the same for the installed plugins (`updater::plugins`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub auto: bool,
    pub plugins: bool,
}
impl Default for UpdateSettings {
    fn default() -> Self {
        Self { auto: false, plugins: false }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: f32,
    /// How much taller than the font's natural line (its ascent, descent and line gap) a row
    /// is, in percent: Ghostty's `adjust-cell-height`. 0 is the font's own.
    pub line_height_adjust: f32,
    /// Thicken the glyphs as macOS's font smoothing does; off draws them thin.
    pub font_smoothing: bool,
    /// The families tried, in order, for a character the font hasn't.
    pub font_fallbacks: Vec<String>,
    /// The regular text's weight (CSS's, 100 to 900); bold is heavier.
    pub font_weight: f32,
    pub ligatures: bool,
    /// The least WCAG contrast text keeps with what is under it; 1 leaves colours alone.
    pub minimum_contrast: f32,
    /// The space round the text, in pixels: left and right, top and bottom.
    pub padding: (f32, f32),
    /// What fills the padding (`terminal_padding_color`, Ghostty's `window-padding-color`).
    pub padding_color: PaddingColor,
    pub cursor_style: CursorStyle,
    pub cursor_blink: CursorBlink,
    pub copy_on_select: bool,
    pub option_as_alt: bool,
}
/// What fills a terminal's padding (`terminal_padding_color`): the theme's background, or the
/// colour of the cells at the grid's edge, so a program that paints its whole screen (a TUI with
/// its own background) is painted up to the pane's edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaddingColor {
    Background,
    #[default]
    Extend,
}
/// The cursor's shape while the program hasn't asked for another (`terminal_cursor_style`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
    BlockHollow,
}
impl CursorStyle {
    pub fn shape(self) -> crate::term::CursorShape {
        use crate::term::CursorShape;
        match self {
            CursorStyle::Block => CursorShape::Block,
            CursorStyle::Bar => CursorShape::Bar,
            CursorStyle::Underline => CursorShape::Underline,
            CursorStyle::BlockHollow => CursorShape::Hollow,
        }
    }
}
/// Whether the cursor blinks (`terminal_cursor_blink`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CursorBlink {
    #[default]
    Off,
    On,
}
pub const FONT_SIZE_RANGE: (f32, f32) = (8.0, 32.0);
pub const LINE_HEIGHT_ADJUST_RANGE: (f32, f32) = (-50.0, 100.0);
pub const CONTRAST_RANGE: (f32, f32) = (1.0, 21.0);
pub const PADDING_RANGE: (f32, f32) = (0.0, 40.0);
impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            font_family: "Menlo".into(),
            font_size: 12.0,
            line_height_adjust: 0.0,
            font_smoothing: false,
            font_fallbacks: Vec::new(),
            font_weight: 400.0,
            ligatures: true,
            minimum_contrast: 1.0,
            padding: (4.0, 2.0),
            padding_color: PaddingColor::Extend,
            cursor_style: CursorStyle::Block,
            cursor_blink: CursorBlink::Off,
            copy_on_select: false,
            option_as_alt: true,
        }
    }
}
impl Global for Settings {}
pub mod names {
    pub const GUIFONT: &str = "guifont";
    pub const LINESPACE: &str = "linespace";
    pub const FONT_SMOOTHING: &str = "terminal_font_smoothing";
    pub const FONT_FALLBACKS: &str = "terminal_font_fallbacks";
    pub const FONT_STYLE: &str = "terminal_font_style";
    pub const LIGATURES: &str = "terminal_ligatures";
    pub const MINIMUM_CONTRAST: &str = "terminal_minimum_contrast";
    pub const PADDING_X: &str = "terminal_padding_x";
    pub const PADDING_Y: &str = "terminal_padding_y";
    pub const PADDING_COLOR: &str = "terminal_padding_color";
    pub const CURSOR_STYLE: &str = "terminal_cursor_style";
    pub const CURSOR_BLINK: &str = "terminal_cursor_blink";
    pub const COPY_ON_SELECT: &str = "terminal_copy_on_select";
    pub const OPTION_AS_ALT: &str = "macos_option_as_alt";
    pub const AUTOUPDATE: &str = "autoupdate";
    pub const PLUGINAUTOCHECK: &str = "pluginautocheck";
}
fn parse_guifont(text: &str, defaults: &TerminalSettings) -> (String, f32) {
    let mut family = defaults.font_family.clone();
    let mut size = defaults.font_size;
    let mut parts = text.split(':');
    if let Some(name) = parts.next().map(str::trim).filter(|n| !n.is_empty()) {
        family = name.replace('_', " ");
    }
    for part in parts {
        if let Some(h) = part.strip_prefix('h').and_then(|h| h.parse::<f32>().ok()) {
            size = h;
        }
    }
    (family, size)
}
/// `terminal_font_style`'s weights, by name.
fn font_weight(style: &str) -> Option<f32> {
    Some(match style.trim().to_ascii_lowercase().replace([' ', '-', '_'], "").as_str() {
        "thin" | "hairline" => 100.,
        "extralight" | "ultralight" => 200.,
        "light" => 300.,
        "regular" | "normal" | "book" | "default" | "" => 400.,
        "medium" => 500.,
        "semibold" | "demibold" => 600.,
        "bold" => 700.,
        "extrabold" | "ultrabold" => 800.,
        "black" | "heavy" => 900.,
        _ => return None,
    })
}
fn choice<T: serde::de::DeserializeOwned>(value: Option<&Value>) -> Option<T> {
    serde_json::from_value(value?.clone()).ok()
}
impl Settings {
    pub fn get(cx: &App) -> &Self {
        cx.global::<Self>()
    }
    pub fn from_options(global: &serde_json::Map<String, Value>) -> Self {
        let d = Settings::default();
        let s = |name: &str, default: &str| global.get(name).and_then(Value::as_str).unwrap_or(default).to_string();
        let b = |name: &str, default: bool| global.get(name).and_then(Value::as_bool).unwrap_or(default);
        let f = |name: &str, default: f32| global.get(name).and_then(Value::as_f64).map_or(default, |n| n as f32);
        let (font_family, font_size) = parse_guifont(&s(names::GUIFONT, ""), &d.terminal);
        Settings {
            terminal: TerminalSettings {
                font_family,
                font_size,
                line_height_adjust: f(names::LINESPACE, d.terminal.line_height_adjust),
                font_smoothing: b(names::FONT_SMOOTHING, d.terminal.font_smoothing),
                font_fallbacks: s(names::FONT_FALLBACKS, "")
                    .split(',')
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(String::from)
                    .collect(),
                font_weight: font_weight(&s(names::FONT_STYLE, "")).unwrap_or(d.terminal.font_weight),
                ligatures: b(names::LIGATURES, d.terminal.ligatures),
                minimum_contrast: f(names::MINIMUM_CONTRAST, d.terminal.minimum_contrast),
                padding: (f(names::PADDING_X, d.terminal.padding.0), f(names::PADDING_Y, d.terminal.padding.1)),
                padding_color: choice(global.get(names::PADDING_COLOR)).unwrap_or(d.terminal.padding_color),
                cursor_style: choice(global.get(names::CURSOR_STYLE)).unwrap_or(d.terminal.cursor_style),
                cursor_blink: choice(global.get(names::CURSOR_BLINK)).unwrap_or(d.terminal.cursor_blink),
                copy_on_select: b(names::COPY_ON_SELECT, d.terminal.copy_on_select),
                option_as_alt: b(names::OPTION_AS_ALT, d.terminal.option_as_alt),
            },
            updates: UpdateSettings {
                auto: b(names::AUTOUPDATE, d.updates.auto),
                plugins: b(names::PLUGINAUTOCHECK, d.updates.plugins),
            },
        }
        .clamped()
    }
    /// Follows pumid's options as they change.
    pub fn follow(global: &serde_json::Map<String, Value>, cx: &mut App) {
        let next = Settings::from_options(global);
        if cx.try_global::<Self>() == Some(&next) {
            return;
        }
        let blink = Some(next.terminal.cursor_blink == CursorBlink::On);
        crate::term::cursor_defaults::set(next.terminal.cursor_style.shape(), blink);
        cx.set_global(next);
        cx.refresh_windows();
    }
    fn clamped(mut self) -> Self {
        let clamp = |value: f32, (lo, hi): (f32, f32), default: f32| {
            if value.is_finite() { value.clamp(lo, hi) } else { default }
        };
        let defaults = Settings::default();
        self.terminal.font_size = clamp(self.terminal.font_size, FONT_SIZE_RANGE, defaults.terminal.font_size);
        self.terminal.line_height_adjust =
            clamp(self.terminal.line_height_adjust, LINE_HEIGHT_ADJUST_RANGE, defaults.terminal.line_height_adjust);
        let t = &mut self.terminal;
        t.minimum_contrast = clamp(t.minimum_contrast, CONTRAST_RANGE, defaults.terminal.minimum_contrast);
        t.padding = (
            clamp(t.padding.0, PADDING_RANGE, defaults.terminal.padding.0),
            clamp(t.padding.1, PADDING_RANGE, defaults.terminal.padding.1),
        );
        if self.terminal.font_family.trim().is_empty() {
            self.terminal.font_family = defaults.terminal.font_family;
        }
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn global(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().cloned().unwrap()
    }
    #[test]
    fn nothing_set_is_the_defaults() {
        assert_eq!(Settings::from_options(&global(json!({}))), Settings::default().clamped());
    }
    #[test]
    fn options_become_settings() {
        let s = Settings::from_options(&global(json!({ "guifont": "JetBrains_Mono:h14", "linespace": 1.5 })));
        assert_eq!((s.terminal.font_family.as_str(), s.terminal.font_size), ("JetBrains Mono", 14.0));
        assert_eq!(s.terminal.line_height_adjust, 1.5);
        let s = Settings::from_options(&global(json!({ "linespace": 15 })));
        assert_eq!(s.terminal.line_height_adjust, 15.0);
        let s = Settings::from_options(&global(json!({ "linespace": -500 })));
        assert_eq!(s.terminal.line_height_adjust, LINE_HEIGHT_ADJUST_RANGE.0);
    }
    #[test]
    fn font_smoothing_is_off_unless_turned_on() {
        assert!(!Settings::from_options(&global(json!({}))).terminal.font_smoothing);
        assert!(Settings::from_options(&global(json!({ "terminal_font_smoothing": true }))).terminal.font_smoothing);
    }
    #[test]
    fn terminal_options_become_settings() {
        let s = Settings::from_options(&global(json!({
            "terminal_font_fallbacks": " Symbols Nerd Font , ,Apple Color Emoji",
            "terminal_font_style": "Semi Bold",
            "terminal_cursor_style": "bar",
            "terminal_cursor_blink": "on",
            "terminal_minimum_contrast": 99,
        })))
        .terminal;
        assert_eq!(s.font_fallbacks, ["Symbols Nerd Font", "Apple Color Emoji"]);
        assert_eq!(s.font_weight, 600.);
        assert_eq!((s.cursor_style, s.cursor_blink), (CursorStyle::Bar, CursorBlink::On));
        assert_eq!(s.minimum_contrast, CONTRAST_RANGE.1);
        let unknown = Settings::from_options(&global(json!({ "terminal_cursor_style": "triangle" }))).terminal;
        assert_eq!(unknown.cursor_style, CursorStyle::Block);
    }
    #[test]
    fn out_of_range_values_are_clamped() {
        let s = Settings::from_options(&global(json!({ "guifont": "Menlo:h400" })));
        assert_eq!(s.terminal.font_size, FONT_SIZE_RANGE.1);
    }
}
