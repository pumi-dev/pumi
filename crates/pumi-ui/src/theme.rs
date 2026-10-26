use gpui::{Rgba, SharedString};
use std::sync::{Arc, RwLock};
pub trait Tokens: Send + Sync + 'static {
    fn color(&self, name: &str) -> Option<Rgba>;
    fn icon(&self, name: &str) -> Option<SharedString>;
    fn metric(&self, _name: &str) -> Option<f32> {
        None
    }
    fn font(&self, _name: &str) -> Option<SharedString> {
        None
    }
    fn reduce_motion(&self) -> bool {
        false
    }
}
struct Fallback;
impl Tokens for Fallback {
    fn color(&self, _: &str) -> Option<Rgba> {
        None
    }
    fn icon(&self, _: &str) -> Option<SharedString> {
        None
    }
}
static TOKENS: RwLock<Option<Arc<dyn Tokens>>> = RwLock::new(None);
pub fn install(tokens: impl Tokens) {
    *TOKENS.write().unwrap_or_else(|p| p.into_inner()) = Some(Arc::new(tokens));
}
fn tokens() -> Arc<dyn Tokens> {
    TOKENS.read().unwrap_or_else(|p| p.into_inner()).clone().unwrap_or_else(|| Arc::new(Fallback))
}
pub fn color(name: &str) -> Option<Rgba> {
    tokens().color(name)
}
pub fn token(name: &str) -> Rgba {
    tokens().color(name).unwrap_or(Rgba { r: 0., g: 0., b: 0., a: 0. })
}
pub fn icon(name: &str) -> Option<SharedString> {
    tokens().icon(name)
}
pub const METRICS: &[(&str, f32, &str)] = &[
    ("radius", 6., "corners of controls: buttons, fields, list rows"),
    ("radius_sm", 4., "corners of small things: chips, keys, checkboxes, menu rows"),
    ("radius_lg", 8., "corners of containers: menus, popovers, dialogs, cards"),
    ("border", 1., "the width of a control's border"),
    ("font_size", 13., "the controls' text"),
    ("font_size_sm", 11., "small text: captions, badges, hints"),
    ("control_height", 28., "a button's or a field's height"),
    ("spacing", 4., "the unit gaps and padding are multiples of"),
];
pub const FONTS: &[(&str, &str)] = &[("font", "the controls' text"), ("mono", "code and terminals' chrome")];
pub fn metric(name: &str) -> f32 {
    tokens().metric(name).unwrap_or_else(|| METRICS.iter().find(|(n, ..)| *n == name).map_or(0., |m| m.1))
}
pub fn given(name: &str) -> Option<f32> {
    tokens().metric(name)
}
pub fn font(name: &str) -> Option<SharedString> {
    tokens().font(name)
}
pub fn reduce_motion() -> bool {
    tokens().reduce_motion()
}
pub fn spec(spec: &str) -> Option<Rgba> {
    let (choices, alpha) = match spec.rsplit_once('/') {
        Some((choices, alpha)) => match alpha.parse::<f32>() {
            Ok(alpha) => (choices, Some(alpha)),
            Err(_) => (spec, None),
        },
        None => (spec, None),
    };
    let color = choices.split('|').find_map(|choice| match choice.as_bytes().first() {
        Some(b'#') => hex(choice),
        Some(b'$') => tokens().color(&choice[1..]),
        _ => None,
    })?;
    Some(match alpha {
        Some(alpha) => Rgba { a: color.a * alpha, ..color },
        None => color,
    })
}
pub fn hex(text: &str) -> Option<Rgba> {
    let digits = text.strip_prefix('#')?;
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => digits.to_string(),
        _ => return None,
    };
    let value = u32::from_str_radix(&expanded, 16).ok()?;
    Some(match expanded.len() {
        6 => gpui::rgb(value),
        _ => gpui::rgba(value),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn specs_fall_back_and_take_alpha() {
        assert_eq!(hex("#f00"), hex("#ff0000"));
        assert_eq!(hex("#ff000080"), Some(gpui::rgba(0xff000080)));
        assert_eq!(hex("ff0000"), None);
        assert_eq!(spec("$nope|#ff0000"), Some(gpui::rgb(0xff0000)));
        assert_eq!(spec("#ff0000/0.5").map(|c| c.a), Some(0.5));
        assert_eq!(spec("$nope"), None);
    }
}
