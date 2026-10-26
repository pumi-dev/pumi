//! The font families the text system has, for Settings → Appearance's font list.
use gpui::{TextSystem, px};

/// Every family, sorted case-insensitively and without the system's hidden ones (`.SF…`); with
/// `monospace`, only those whose `i` is as wide as their `m`.
pub fn families(text: &TextSystem, monospace: bool) -> Vec<String> {
    let size = px(12.);
    let mut names: Vec<String> = text
        .all_font_names()
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .filter(|name| {
            if !monospace {
                return true;
            }
            let id = text.resolve_font(&gpui::font(name.clone()));
            match (text.advance(id, size, 'i'), text.advance(id, size, 'm')) {
                (Ok(i), Ok(m)) => i.width > px(0.) && (i.width - m.width).abs() < px(0.01),
                _ => false,
            }
        })
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup();
    names
}
