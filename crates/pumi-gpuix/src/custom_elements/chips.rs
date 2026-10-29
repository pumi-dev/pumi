//! Chips in a text editor: mentions drawn in the line as a name on a rounded wash, after an icon.
//!
//! The editor's value keeps the text a chip stands for (`@src/main.rs`); the line is laid out
//! from a display text in which each chip's text is replaced by its label, padded for the icon.
//! Offsets map between the two: a buffer offset inside a chip has no place of its own, so the
//! caret is snapped to one of its edges, and the chip is one character to it.

use std::ops::Range;
use std::sync::Arc;

use gpui::SharedString;

/// Room before a chip's label for its icon, and after it for the wash's end. No-break spaces, so
/// the line wraps around a chip, never inside it.
pub const LEAD: &str = "\u{00A0}\u{00A0}\u{00A0}";
pub const TRAIL: &str = "\u{00A0}";

/// One of the editor's `chips`: the text it stands for, its label, and its icon's SVG.
#[derive(Clone, Debug, PartialEq)]
pub struct ChipSpec {
    pub text: String,
    pub label: String,
    /// The icon's SVG, and the key gpui caches its raster by.
    pub icon: Option<(SharedString, Arc<[u8]>)>,
}

/// `chips` as the prop gives them: `[{ text, label, icon? }]`, `icon` an SVG's markup or file.
/// Entries without a text are dropped.
pub fn parse_specs(value: &serde_json::Value) -> Vec<ChipSpec> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let text = item.get("text")?.as_str()?.to_string();
            if text.is_empty() || text.contains('\n') {
                return None;
            }
            let label = item.get("label").and_then(|value| value.as_str()).unwrap_or(&text).to_string();
            let icon = item.get("icon").and_then(|value| value.as_str()).and_then(icon_svg);
            Some(ChipSpec { text, label, icon })
        })
        .collect()
}

/// An icon's SVG: markup as it is, else a file or `data:` URL read.
fn icon_svg(source: &str) -> Option<(SharedString, Arc<[u8]>)> {
    let source = source.trim();
    if source.is_empty() {
        return None;
    }
    let bytes: Arc<[u8]> =
        if source.starts_with('<') { Arc::from(source.as_bytes()) } else { Arc::from(super::img::svg_bytes(source)?) };
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    Some((SharedString::from(format!("__gpuix_chip_icon_{}", hasher.finish())), bytes))
}

/// A chip found in the value: its spec, where its text is, and where its label is drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChipAt {
    pub spec: usize,
    pub range: Range<usize>,
    pub display: Range<usize>,
}

/// The display text and the chips in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChipLayout {
    pub display: String,
    pub chips: Vec<ChipAt>,
}

/// A chip's label as it is drawn: padded, its spaces no-break.
pub fn display_label(label: &str) -> String {
    let label: String = label.chars().map(|c| if c.is_whitespace() { '\u{00A0}' } else { c }).collect();
    format!("{LEAD}{label}{TRAIL}")
}

/// Where each spec's text stands whole in `content`: at its start or after whitespace, at its end
/// or before whitespace. Longer texts first, no two overlapping, none across `marked` (the IME's
/// composition). In order.
pub fn find(content: &str, specs: &[ChipSpec], marked: Option<&Range<usize>>) -> Vec<(usize, Range<usize>)> {
    let mut order: Vec<usize> = (0..specs.len()).collect();
    order.sort_by_key(|&index| std::cmp::Reverse(specs[index].text.len()));
    let mut found: Vec<(usize, Range<usize>)> = Vec::new();
    for index in order {
        let text = specs[index].text.as_str();
        if found.iter().any(|(other, _)| specs[*other].text == text) {
            continue;
        }
        for (start, _) in content.match_indices(text) {
            let end = start + text.len();
            let before = content[..start].chars().next_back();
            let after = content[end..].chars().next();
            if before.is_some_and(|c| !c.is_whitespace()) || after.is_some_and(|c| !c.is_whitespace()) {
                continue;
            }
            if found.iter().any(|(_, range)| start < range.end && end > range.start) {
                continue;
            }
            if marked.is_some_and(|marked| start < marked.end && end > marked.start) {
                continue;
            }
            found.push((index, start..end));
        }
    }
    found.sort_by_key(|(_, range)| range.start);
    found
}

/// The display text for `content` and where its chips are in both.
pub fn layout(content: &str, specs: &[ChipSpec], marked: Option<&Range<usize>>) -> ChipLayout {
    let found = find(content, specs, marked);
    if found.is_empty() {
        return ChipLayout { display: content.to_string(), chips: Vec::new() };
    }
    let mut display = String::with_capacity(content.len() + found.len() * 8);
    let mut chips = Vec::with_capacity(found.len());
    let mut from = 0;
    for (spec, range) in found {
        display.push_str(&content[from..range.start]);
        let start = display.len();
        display.push_str(&display_label(&specs[spec].label));
        chips.push(ChipAt { spec, range: range.clone(), display: start..display.len() });
        from = range.end;
    }
    display.push_str(&content[from..]);
    ChipLayout { display, chips }
}

impl ChipLayout {
    /// The display offset of buffer offset `offset`; one inside a chip is its start.
    pub fn to_display(&self, offset: usize) -> usize {
        let mut shift: isize = 0;
        for chip in &self.chips {
            if offset <= chip.range.start {
                break;
            }
            if offset < chip.range.end {
                return chip.display.start;
            }
            shift = chip.display.end as isize - chip.range.end as isize;
        }
        (offset as isize + shift).max(0) as usize
    }

    /// The buffer offset of display offset `offset`; one in a chip's label is its nearer edge.
    pub fn to_buffer(&self, offset: usize) -> usize {
        let mut shift: isize = 0;
        for chip in &self.chips {
            if offset <= chip.display.start {
                break;
            }
            if offset < chip.display.end {
                let nearer_start = offset - chip.display.start <= chip.display.end - offset;
                return if nearer_start { chip.range.start } else { chip.range.end };
            }
            shift = chip.range.end as isize - chip.display.end as isize;
        }
        (offset as isize + shift).max(0) as usize
    }

    /// The chip buffer offset `offset` is strictly inside of.
    pub fn inside(&self, offset: usize) -> Option<&ChipAt> {
        self.chips.iter().find(|chip| offset > chip.range.start && offset < chip.range.end)
    }

    /// `offset`, or the edge of the chip it is inside: its end `toward_end`, else its start.
    pub fn snap(&self, offset: usize, toward_end: bool) -> usize {
        match self.inside(offset) {
            Some(chip) if toward_end => chip.range.end,
            Some(chip) => chip.range.start,
            None => offset,
        }
    }

    /// The chip whose label display offset `offset` is in.
    pub fn at_display(&self, offset: usize) -> Option<&ChipAt> {
        self.chips.iter().find(|chip| offset >= chip.display.start && offset < chip.display.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str, label: &str) -> ChipSpec {
        ChipSpec { text: text.to_string(), label: label.to_string(), icon: None }
    }

    #[test]
    fn a_chip_stands_whole_between_spaces() {
        let specs = [spec("@a.rs", "a.rs")];
        assert_eq!(find("see @a.rs now", &specs, None), vec![(0, 4..9)]);
        assert_eq!(find("@a.rs", &specs, None), vec![(0, 0..5)]);
        assert!(find("see @a.rsx", &specs, None).is_empty());
        assert!(find("x@a.rs", &specs, None).is_empty());
        assert_eq!(find("@a.rs @a.rs", &specs, None), vec![(0, 0..5), (0, 6..11)]);
    }

    #[test]
    fn the_longer_text_wins_an_overlap_and_the_composition_has_none() {
        let specs = [spec("@a", "a"), spec("@a b", "a b")];
        assert_eq!(find("@a b", &specs, None), vec![(1, 0..4)]);
        assert!(find("@a b", &specs, Some(&(1..2))).is_empty());
    }

    #[test]
    fn offsets_map_both_ways_and_snap_to_a_chip_edge() {
        let specs = [spec("@src/a.rs", "a.rs")];
        let layout = layout("x @src/a.rs y", &specs, None);
        let label = display_label("a.rs");
        assert_eq!(layout.display, format!("x {label} y"));
        let chip = &layout.chips[0];
        assert_eq!(chip.range, 2..11);
        assert_eq!(chip.display, 2..2 + label.len());
        assert_eq!(layout.to_display(0), 0);
        assert_eq!(layout.to_display(2), 2);
        assert_eq!(layout.to_display(5), 2);
        assert_eq!(layout.to_display(11), chip.display.end);
        assert_eq!(layout.to_display(13), chip.display.end + 2);
        assert_eq!(layout.to_buffer(chip.display.start + 1), 2);
        assert_eq!(layout.to_buffer(chip.display.end - 1), 11);
        assert_eq!(layout.to_buffer(chip.display.end + 2), 13);
        assert_eq!(layout.snap(5, false), 2);
        assert_eq!(layout.snap(5, true), 11);
        assert_eq!(layout.snap(11, false), 11);
        assert!(layout.at_display(chip.display.start).is_some());
        assert!(layout.at_display(chip.display.end).is_none());
    }

    #[test]
    fn a_label_wraps_as_one_word() {
        assert_eq!(display_label("a b"), format!("{LEAD}a\u{00A0}b{TRAIL}"));
    }

    #[test]
    fn specs_come_from_the_prop() {
        let specs = parse_specs(&serde_json::json!([
            { "text": "@a", "label": "a", "icon": "<svg/>" },
            { "text": "" },
            { "label": "no text" },
            { "text": "@b" }
        ]));
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].icon.as_ref().map(|(_, bytes)| bytes.as_ref()), Some(&b"<svg/>"[..]));
        assert_eq!(specs[1].label, "@b");
    }
}
