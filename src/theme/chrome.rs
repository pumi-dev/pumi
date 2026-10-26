//! The colours of the app's own native windows (Settings): pumi's dark and light, fixed. No UI
//! theme changes them; the window draws the app's look, dark or light (`set_appearance`).
use super::Appearance;
use gpui::{Rgba, rgb, rgba};
use std::sync::atomic::{AtomicBool, Ordering};

static LIGHT: AtomicBool = AtomicBool::new(false);

/// Which of the two the native windows draw with: the app's look, which it tells us
/// (`setAppearance`) when its theme's changes; the system's until it does.
pub fn set_appearance(appearance: Appearance) {
    LIGHT.store(appearance == Appearance::Light, Ordering::Relaxed);
}

pub fn appearance() -> Appearance {
    if LIGHT.load(Ordering::Relaxed) { Appearance::Light } else { Appearance::Dark }
}

macro_rules! palette {
    ($($name:ident),* $(,)?) => {
        pub struct Colors { $(pub $name: Rgba,)* }
        impl Colors {
            pub fn get(&self, name: &str) -> Option<Rgba> {
                match name {
                    $(stringify!($name) => Some(self.$name),)*
                    _ => None,
                }
            }
        }
    };
}

palette! { bg, chrome_surface, chrome_selected, chrome_border, text, chrome_muted, faint, accent, warning, error, surface, hover, active, border, border_variant, element, element_disabled, focus_ring, surface_text, muted, placeholder, editor, selection, row_selected, folder, indent_guide, placeholder_icon, view_bar, view_active, view_inactive, view_hover, view_border, view_active_border, view_text, view_active_text, input_hover, primary, primary_hover, primary_active, primary_foreground, danger_hover, danger_active, danger_foreground, success, overlay, window, green }

fn dark() -> Colors {
    Colors {
        bg: rgb(0x282c34),
        chrome_surface: rgb(0x32363e),
        chrome_selected: rgb(0x4a4d54),
        chrome_border: rgb(0x3d4148),
        text: rgb(0xffffff),
        chrome_muted: rgb(0xbebfc2),
        faint: rgb(0x888a8f),
        accent: rgb(0x81a2be),
        warning: rgb(0xf0c674),
        error: rgb(0xcc6666),
        surface: rgb(0x26282b),
        hover: rgb(0x3c3e41),
        active: rgb(0x494a4d),
        border: rgb(0x393b41),
        border_variant: rgb(0x393b41),
        element: rgb(0x393b41),
        element_disabled: rgb(0x2b2d30),
        focus_ring: rgb(0x3474f0),
        surface_text: rgb(0xdfe1e5),
        muted: rgb(0xb0b1b3),
        placeholder: rgb(0x6f737a),
        editor: rgb(0x1e1f22),
        selection: rgb(0x214283),
        row_selected: rgb(0x293036),
        folder: rgb(0x94a3ad),
        indent_guide: rgb(0x3b3d45),
        placeholder_icon: rgb(0x2e3033),
        view_bar: rgba(0x00000000),
        view_active: rgb(0x4a4d54),
        view_inactive: rgba(0x00000000),
        view_hover: rgb(0x32363e),
        view_border: rgb(0x4a4d54),
        view_active_border: rgb(0x888a8f),
        view_text: rgb(0xbebfc2),
        view_active_text: rgb(0xffffff),
        input_hover: rgb(0x4a4d55),
        primary: rgb(0x3474f0),
        primary_hover: rgb(0x4783f2),
        primary_active: rgb(0x2a65d8),
        primary_foreground: rgb(0xffffff),
        danger_hover: rgb(0xd77a7a),
        danger_active: rgb(0xb85858),
        danger_foreground: rgb(0xffffff),
        success: rgb(0xa6e3a1),
        overlay: rgba(0x26282bcc),
        window: rgb(0x282c34),
        green: rgb(0xa6e3a1),
    }
}

fn light() -> Colors {
    Colors {
        bg: rgb(0xf7f8fa),
        chrome_surface: rgb(0xebecf0),
        chrome_selected: rgb(0xd3d5db),
        chrome_border: rgb(0xd3d5db),
        text: rgb(0x1e1f22),
        chrome_muted: rgb(0x494b57),
        faint: rgb(0x818594),
        accent: rgb(0x3574f0),
        warning: rgb(0xb27200),
        error: rgb(0xdb3b4b),
        surface: rgb(0xf7f8fa),
        hover: rgb(0xe4e5e9),
        active: rgb(0xd8dae0),
        border: rgb(0xdcdee3),
        border_variant: rgb(0xe6e7ea),
        element: rgb(0xd3d5db),
        element_disabled: rgb(0xeeeff2),
        focus_ring: rgb(0x3574f0),
        surface_text: rgb(0x1f2023),
        muted: rgb(0x6c707e),
        placeholder: rgb(0xa8adbd),
        editor: rgb(0xffffff),
        selection: rgb(0xa6d2ff),
        row_selected: rgb(0xdbe6fd),
        folder: rgb(0x6c707e),
        indent_guide: rgb(0xe3e4e8),
        placeholder_icon: rgb(0xd9dbe0),
        view_bar: rgba(0x00000000),
        view_active: rgb(0xd3d5db),
        view_inactive: rgba(0x00000000),
        view_hover: rgb(0xebecf0),
        view_border: rgb(0xd3d5db),
        view_active_border: rgb(0x818594),
        view_text: rgb(0x494b57),
        view_active_text: rgb(0x1e1f22),
        input_hover: rgb(0xc4c7d0),
        primary: rgb(0x3574f0),
        primary_hover: rgb(0x4682fa),
        primary_active: rgb(0x2f63d1),
        primary_foreground: rgb(0xffffff),
        danger_hover: rgb(0xe25a68),
        danger_active: rgb(0xc22d3c),
        danger_foreground: rgb(0xffffff),
        success: rgb(0x40a02b),
        overlay: rgba(0xf7f8facc),
        window: rgb(0xf7f8fa),
        green: rgb(0x40a02b),
    }
}

static DARK: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(dark);
static LIGHT_COLORS: std::sync::LazyLock<Colors> = std::sync::LazyLock::new(light);

pub fn colors() -> &'static Colors {
    if LIGHT.load(Ordering::Relaxed) { &LIGHT_COLORS } else { &DARK }
}

/// What pumi-ui's components draw with.
struct Tokens;
impl pumi_ui::theme::Tokens for Tokens {
    fn color(&self, name: &str) -> Option<Rgba> {
        colors().get(name)
    }
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        crate::ui::widgets::icon_path(name).map(Into::into)
    }
}

/// Gives pumi-ui these colours (once, before a native window draws).
pub fn install() {
    pumi_ui::theme::install(Tokens);
}
