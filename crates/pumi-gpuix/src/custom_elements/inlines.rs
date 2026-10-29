//! Inline nodes in a text editor, as Lexical's decorator nodes: a run of the value
//! (`@src/main.rs`) that edits as one character and is drawn, in its place in the line, by an
//! element of the page's (the editor's child of the same index as its spec).
//!
//! The value keeps the text a node stands for. The line is laid out from a display text in which
//! each node's text is replaced by a slot: no-break spaces as wide as its element, laid out but
//! not drawn, so the line wraps around the node, never inside it. Until its element has been
//! measured (its first frame), the slot is its label. Offsets map between the two: a buffer offset
//! inside a node has no place of its own, so the caret is snapped to one of its edges, and an edit
//! that reaches into a node takes it whole.

use std::ops::Range;

/// What a slot is made of: a space that glues to its neighbours (gpui wraps around it, not in it).
pub const SLOT: char = '\u{00A0}';

/// One of the editor's `inlines`: the text it stands for, and what stands in for its element
/// until the element is measured (or for a later copy of the same text, which has no element).
#[derive(Clone, Debug, PartialEq)]
pub struct InlineSpec {
    pub text: String,
    pub label: String,
}

/// `inlines` as the prop gives them: `[{ text, label? }]`. Entries without a text, or with a
/// line break in it, are dropped.
pub fn parse_specs(value: &serde_json::Value) -> Vec<InlineSpec> {
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
            Some(InlineSpec { text, label })
        })
        .collect()
}

/// A node found in the value: its spec, where its text is, and where it is in the display text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineAt {
    pub spec: usize,
    pub range: Range<usize>,
    pub display: Range<usize>,
    /// The first node of its spec: its element draws it (a later one is its label, as text).
    pub first: bool,
    /// Its display is a slot (its element measured), not its label.
    pub slot: bool,
}

/// The display text and the nodes in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineLayout {
    pub display: String,
    pub nodes: Vec<InlineAt>,
}

/// A label as it is drawn: its spaces no-break, so it wraps as one word.
pub fn display_label(label: &str) -> String {
    label.chars().map(|c| if c.is_whitespace() { SLOT } else { c }).collect()
}

/// A slot of `chars` no-break spaces (one at least).
pub fn slot(chars: usize) -> String {
    std::iter::repeat_n(SLOT, chars.max(1)).collect()
}

/// Where each spec's text is in `content`, wherever it is (text typed against a node leaves it a
/// node). Longer texts first, no two overlapping, none across `marked` (the IME's composition).
/// In order.
pub fn find(content: &str, specs: &[InlineSpec], marked: Option<&Range<usize>>) -> Vec<(usize, Range<usize>)> {
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

/// The display text for `content` and where its nodes are in both. `slots[spec]` is how many
/// no-break spaces the spec's element is wide, once measured; a node without one shows its label.
pub fn layout(
    content: &str,
    specs: &[InlineSpec],
    slots: &[Option<usize>],
    marked: Option<&Range<usize>>,
) -> InlineLayout {
    let found = find(content, specs, marked);
    if found.is_empty() {
        return InlineLayout { display: content.to_string(), nodes: Vec::new() };
    }
    let mut display = String::with_capacity(content.len() + found.len() * 8);
    let mut nodes = Vec::with_capacity(found.len());
    let mut seen = vec![false; specs.len()];
    let mut from = 0;
    for (spec, range) in found {
        display.push_str(&content[from..range.start]);
        let first = !std::mem::replace(&mut seen[spec], true);
        let width = slots.get(spec).copied().flatten().filter(|_| first);
        let start = display.len();
        match width {
            Some(chars) => display.push_str(&slot(chars)),
            None => display.push_str(&display_label(&specs[spec].label)),
        }
        nodes.push(InlineAt {
            spec,
            range: range.clone(),
            display: start..display.len(),
            first,
            slot: width.is_some(),
        });
        from = range.end;
    }
    display.push_str(&content[from..]);
    InlineLayout { display, nodes }
}

impl InlineLayout {
    /// The display offset of buffer offset `offset`; one inside a node is its start.
    pub fn to_display(&self, offset: usize) -> usize {
        let mut shift: isize = 0;
        for node in &self.nodes {
            if offset <= node.range.start {
                break;
            }
            if offset < node.range.end {
                return node.display.start;
            }
            shift = node.display.end as isize - node.range.end as isize;
        }
        (offset as isize + shift).max(0) as usize
    }

    /// The buffer offset of display offset `offset`; one in a node's display is its nearer edge.
    pub fn to_buffer(&self, offset: usize) -> usize {
        let mut shift: isize = 0;
        for node in &self.nodes {
            if offset <= node.display.start {
                break;
            }
            if offset < node.display.end {
                let nearer_start = offset - node.display.start <= node.display.end - offset;
                return if nearer_start { node.range.start } else { node.range.end };
            }
            shift = node.range.end as isize - node.display.end as isize;
        }
        (offset as isize + shift).max(0) as usize
    }

    /// The node buffer offset `offset` is strictly inside of.
    pub fn inside(&self, offset: usize) -> Option<&InlineAt> {
        self.nodes.iter().find(|node| offset > node.range.start && offset < node.range.end)
    }

    /// `offset`, or the edge of the node it is inside: its end `toward_end`, else its start.
    pub fn snap(&self, offset: usize, toward_end: bool) -> usize {
        match self.inside(offset) {
            Some(node) if toward_end => node.range.end,
            Some(node) => node.range.start,
            None => offset,
        }
    }

    /// `range` grown to take whole every node it reaches into: an edit never leaves part of one.
    pub fn widen(&self, range: Range<usize>) -> Range<usize> {
        self.snap(range.start, false)..self.snap(range.end, true).max(range.start)
    }

    /// The node whose display offset `offset` is in.
    pub fn at_display(&self, offset: usize) -> Option<&InlineAt> {
        self.nodes.iter().find(|node| offset >= node.display.start && offset < node.display.end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(text: &str, label: &str) -> InlineSpec {
        InlineSpec { text: text.to_string(), label: label.to_string() }
    }

    #[test]
    fn a_node_is_found_wherever_its_text_is() {
        let specs = [spec("@a.rs", "a.rs")];
        assert_eq!(find("see @a.rs now", &specs, None), vec![(0, 4..9)]);
        assert_eq!(find("@a.rs", &specs, None), vec![(0, 0..5)]);
        // Typed against it, it stays a node.
        assert_eq!(find("see @a.rsx", &specs, None), vec![(0, 4..9)]);
        assert_eq!(find("x@a.rs", &specs, None), vec![(0, 1..6)]);
        assert_eq!(find("@a.rs @a.rs", &specs, None), vec![(0, 0..5), (0, 6..11)]);
    }

    #[test]
    fn the_longer_text_wins_an_overlap_and_the_composition_has_none() {
        let specs = [spec("@a", "a"), spec("@a b", "a b")];
        assert_eq!(find("@a b", &specs, None), vec![(1, 0..4)]);
        assert!(find("@a b", &specs, Some(&(1..2))).is_empty());
    }

    #[test]
    fn a_measured_node_is_a_slot_and_its_later_copies_their_label() {
        let specs = [spec("@src/a.rs", "a.rs")];
        let layout = layout("x @src/a.rs @src/a.rs", &specs, &[Some(4)], None);
        assert_eq!(layout.display, format!("x {} a.rs", slot(4)));
        assert!(layout.nodes[0].first && layout.nodes[0].slot);
        assert!(!layout.nodes[1].first && !layout.nodes[1].slot);
        let unmeasured = super::layout("@src/a.rs", &specs, &[None], None);
        assert_eq!(unmeasured.display, "a.rs");
        assert!(!unmeasured.nodes[0].slot);
    }

    #[test]
    fn offsets_map_both_ways_and_snap_to_a_node_edge() {
        let specs = [spec("@src/a.rs", "a.rs")];
        let layout = layout("x @src/a.rs y", &specs, &[Some(3)], None);
        let node = &layout.nodes[0];
        assert_eq!(node.range, 2..11);
        assert_eq!(node.display, 2..2 + slot(3).len());
        assert_eq!(layout.to_display(0), 0);
        assert_eq!(layout.to_display(5), 2);
        assert_eq!(layout.to_display(11), node.display.end);
        assert_eq!(layout.to_display(13), node.display.end + 2);
        assert_eq!(layout.to_buffer(node.display.start + 1), 2);
        assert_eq!(layout.to_buffer(node.display.end - 1), 11);
        assert_eq!(layout.to_buffer(node.display.end + 2), 13);
        assert_eq!(layout.snap(5, false), 2);
        assert_eq!(layout.snap(5, true), 11);
        assert_eq!(layout.snap(11, false), 11);
        assert!(layout.at_display(node.display.start).is_some());
        assert!(layout.at_display(node.display.end).is_none());
    }

    #[test]
    fn an_edit_that_reaches_into_a_node_takes_it_whole() {
        let specs = [spec("@src/a.rs", "a.rs")];
        let layout = layout("x @src/a.rs y", &specs, &[None], None);
        // The last letter before it and the node's first part: both, and the node whole.
        assert_eq!(layout.widen(1..5), 1..11);
        assert_eq!(layout.widen(9..12), 2..12);
        assert_eq!(layout.widen(5..6), 2..11);
        assert_eq!(layout.widen(0..1), 0..1);
        assert_eq!(layout.widen(11..11), 11..11);
    }

    #[test]
    fn a_label_wraps_as_one_word() {
        assert_eq!(display_label("a b"), "a\u{00A0}b");
    }

    #[test]
    fn specs_come_from_the_prop() {
        let specs = parse_specs(&serde_json::json!([
            { "text": "@a", "label": "a" },
            { "text": "" },
            { "label": "no text" },
            { "text": "@b" }
        ]));
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[1].label, "@b");
    }
}
