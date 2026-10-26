//! The colours the native half paints with: the terminal's palette, and the app's own chrome.
//! A UI theme (React) sets the terminal's with `terminal.setPalette`; until it does, the built-in
//! dark one draws.
use gpui::{Rgba, rgb, rgba};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Terminal {
    pub background: u32,
    pub foreground: u32,
    pub selection: u32,
    pub cursor: u32,
    pub ansi: [u32; 16],
    /// All 256 colours, `ansi` the first sixteen of them (`Terminal::extend`).
    pub palette: [u32; 256],
    pub opacity: f32,
}
impl Default for Terminal {
    fn default() -> Self {
        Terminal::dark()
    }
}
impl Terminal {
    pub fn dark() -> Self {
        let mut dark = Terminal {
            background: 0x202020,
            foreground: 0xadadad,
            selection: 0x3a4a63,
            cursor: 0xd8d8d8,
            ansi: [
                0x000000, 0xfa5355, 0x126e00, 0xc2c300, 0x4581eb, 0xfa54ff, 0x33c2c1, 0xadadad, 0x555555, 0xfb7172,
                0x67ff4f, 0xffff00, 0x6d9df1, 0xfb82ff, 0x60d3d1, 0xeeeeee,
            ],
            palette: [0; 256],
            opacity: 1.,
        };
        dark.extend(false, false, &[]);
        dark
    }
    /// Fills `palette` from `ansi`: xterm's colour cube and grey ramp above them, or, with
    /// `generate`, a cube made from `ansi` and a ramp from `background` to `foreground`, in
    /// CIELAB (Ghostty's `palette-generate`). `harmonious` keeps a light background's generated
    /// colours running from the background to the foreground, where they would otherwise run dark
    /// to light (`palette-harmonious`). A `pinned` index has its own colour either way.
    pub fn extend(&mut self, generate: bool, harmonious: bool, pinned: &[(u8, u32)]) {
        use libghostty_vt::style::{Palette, PaletteIndex, PaletteMask, RgbColor};
        let rgb = |c: u32| RgbColor { r: (c >> 16) as u8, g: (c >> 8) as u8, b: c as u8 };
        let mut base = Palette::default();
        for (slot, colour) in base.0.iter_mut().zip(self.ansi) {
            *slot = rgb(colour);
        }
        let mut kept = PaletteMask::new();
        for &(index, colour) in pinned {
            base.0[usize::from(index)] = rgb(colour);
            kept.set(PaletteIndex(index));
        }
        let full = if generate {
            Palette::generate(Some(&base), Some(&kept), rgb(self.background), rgb(self.foreground), harmonious)
        } else {
            base
        };
        self.palette = full.0.map(|c| (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b));
        self.ansi.copy_from_slice(&self.palette[..16]);
    }
}

static CURRENT: LazyLock<RwLock<Arc<Terminal>>> = LazyLock::new(|| RwLock::new(Arc::new(Terminal::dark())));
static GENERATION: AtomicU64 = AtomicU64::new(1);
thread_local! {
    static LOCAL: RefCell<(u64, Option<Arc<Terminal>>)> = const { RefCell::new((0, None)) };
}
/// Sets the palette. False when it was already this one.
pub fn set_terminal(palette: Terminal) -> bool {
    let mut current = CURRENT.write().unwrap_or_else(|p| p.into_inner());
    if **current == palette {
        return false;
    }
    *current = Arc::new(palette);
    GENERATION.fetch_add(1, Ordering::AcqRel);
    true
}
/// Bumped whenever the palette changes, so painted rows know to repaint.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}
fn read<R>(read: impl FnOnce(&Terminal) -> R) -> R {
    let generation = GENERATION.load(Ordering::Acquire);
    LOCAL.with(|local| {
        let mut local = local.borrow_mut();
        if local.0 != generation || local.1.is_none() {
            *local = (generation, Some(CURRENT.read().unwrap_or_else(|p| p.into_inner()).clone()));
        }
        read(local.1.as_ref().expect("set above"))
    })
}
pub fn parse_hex(spec: &str) -> Option<Rgba> {
    let hex = spec.trim().strip_prefix('#')?;
    if !hex.is_ascii() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    let spread = |n: u32| (n & 0xf) * 0x11;
    Some(match hex.len() {
        3 => rgb((spread(value >> 8) << 16) | (spread(value >> 4) << 8) | spread(value)),
        4 => rgba((spread(value >> 12) << 24) | (spread(value >> 8) << 16) | (spread(value >> 4) << 8) | spread(value)),
        6 => rgb(value),
        8 => rgba(value),
        _ => return None,
    })
}
pub fn to_u32(color: Rgba) -> u32 {
    let byte = |channel: f32| u32::from((channel.clamp(0., 1.) * 255.).round() as u8);
    (byte(color.r) << 16) | (byte(color.g) << 8) | byte(color.b)
}
pub mod terminal {
    pub fn bg() -> u32 {
        super::read(|t| t.background)
    }
    pub fn fg() -> u32 {
        super::read(|t| t.foreground)
    }
    pub fn selection() -> u32 {
        super::read(|t| t.selection)
    }
    pub fn cursor() -> u32 {
        super::read(|t| t.cursor)
    }
    pub fn ansi() -> [u32; 16] {
        super::read(|t| t.ansi)
    }
    pub fn palette() -> [u32; 256] {
        super::read(|t| t.palette)
    }
    pub fn opacity() -> f32 {
        super::read(|t| t.opacity.clamp(0., 1.))
    }
    pub fn background(color: u32) -> gpui::Rgba {
        let rgba = gpui::rgb(resolve(color));
        let alpha = if color & !DIM == BG_REF { opacity() } else { 1. };
        gpui::Rgba { a: alpha, ..rgba }
    }
    pub fn resolve(color: u32) -> u32 {
        let dim = color & DIM != 0;
        let color = color & !DIM;
        let resolved = match color >> 24 {
            ANSI_TAG => super::read(|t| t.ansi[(color & 0xf) as usize]),
            FG_TAG => fg(),
            BG_TAG => bg(),
            _ => color & 0xff_ffff,
        };
        if dim { faint(resolved) } else { resolved }
    }
    pub const fn ansi_ref(index: u8) -> u32 {
        (ANSI_TAG << 24) | (index as u32 & 0xf)
    }
    pub const FG_REF: u32 = FG_TAG << 24;
    pub const BG_REF: u32 = BG_TAG << 24;
    pub const fn dimmed(color: u32) -> u32 {
        color | DIM
    }
    const ANSI_TAG: u32 = 0x01;
    const FG_TAG: u32 = 0x02;
    const BG_TAG: u32 = 0x03;
    const DIM: u32 = 0x8000_0000;
    fn faint(color: u32) -> u32 {
        let scale = |channel: u32| ((channel * 6) / 10).min(255);
        (scale((color >> 16) & 0xff) << 16) | (scale((color >> 8) & 0xff) << 8) | scale(color & 0xff)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hex_round_trips() {
        assert_eq!(parse_hex("#abc"), Some(rgb(0xaabbcc)));
        assert_eq!(parse_hex("#12345678"), Some(rgba(0x12345678)));
        assert_eq!(parse_hex("123456"), None);
        assert_eq!(to_u32(rgba(0x12345680)), 0x123456);
    }
    #[test]
    fn terminal_cells_follow_the_palette_by_reference() {
        let ansi = terminal::ansi();
        assert_eq!(terminal::resolve(terminal::ansi_ref(1)), ansi[1]);
        assert_eq!(terminal::resolve(terminal::FG_REF), terminal::fg());
        assert_eq!(terminal::resolve(terminal::BG_REF), terminal::bg());
        assert_eq!(terminal::resolve(0x123456), 0x123456);
        assert_eq!(terminal::resolve(terminal::dimmed(0x646464)), 0x3c3c3c);
    }
    fn light() -> Terminal {
        let mut light = Terminal::dark();
        (light.background, light.foreground) = (0xffffff, 0x000000);
        light
    }
    #[test]
    fn without_generating_the_colours_above_sixteen_are_xterms() {
        let dark = Terminal::dark();
        assert_eq!(dark.palette[..16], dark.ansi);
        assert_eq!(dark.palette[16], 0x000000, "the cube's first corner");
        assert_eq!(dark.palette[196], 0xff0000);
        assert_eq!(dark.palette[232], 0x080808, "the grey ramp's first step");
    }
    #[test]
    fn generating_makes_the_cube_and_ramp_from_the_scheme_and_keeps_its_sixteen() {
        let mut t = Terminal::dark();
        let xterm = t.palette;
        t.extend(true, false, &[]);
        assert_eq!(t.palette[..16], xterm[..16]);
        assert_ne!(t.palette[16..], xterm[16..]);
        assert_eq!(t.palette[16], t.background, "the cube starts at the background");
    }
    #[test]
    fn a_pinned_index_keeps_its_colour_whether_generated_or_not() {
        let mut t = Terminal::dark();
        t.extend(true, false, &[(1, 0x123456), (196, 0xabcdef)]);
        assert_eq!((t.ansi[1], t.palette[1], t.palette[196]), (0x123456, 0x123456, 0xabcdef));
        t.extend(false, false, &[(196, 0xabcdef)]);
        assert_eq!(t.palette[196], 0xabcdef);
    }
    #[test]
    fn harmonious_keeps_a_light_ramp_running_from_the_background() {
        let mut t = light();
        t.extend(true, true, &[]);
        assert_eq!(t.palette[16], 0xffffff, "light first");
        t.extend(true, false, &[]);
        assert_ne!(t.palette[16], 0xffffff, "dark first");
    }
}

pub mod chrome;

pub use chrome::{appearance, set_appearance};

macro_rules! accessors {
    ($($name:ident),* $(,)?) => {
        $(pub fn $name() -> Rgba { chrome::colors().$name })*
    };
}
// The native windows' colours, by name (`theme::muted()`).
accessors! { bg, text, faint, accent, warning, error, surface, border, surface_text, muted, placeholder, editor }
pub mod palette {
    pub fn green() -> gpui::Rgba {
        super::chrome::colors().green
    }
}
