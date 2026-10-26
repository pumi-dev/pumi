use pumi_api::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub mod float;
mod place;
pub use float::{Anchor, Dim};
pub use place::{PlaceKind, PlaceSpec};
pub type PaneId = u64;
/// Where a new pane goes beside an existing one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}
impl Side {
    pub fn axis(self) -> Axis {
        match self {
            Side::Left | Side::Right => Axis::Row,
            Side::Up | Side::Down => Axis::Col,
        }
    }
    pub fn before(self) -> bool {
        matches!(self, Side::Left | Side::Up)
    }
}
/// A `row` lays its children out left to right, a `col` top to bottom (Neovim's words).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Row,
    Col,
}
/// A size: a share of the space (`"30%"`, or `0.3`) or a number of cells (`20`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Share(f64),
    Cells(u32),
}
impl Size {
    pub fn parse(value: &Value) -> Result<Size> {
        let bad = || Error::invalid_params(format!("a size is \"30%\", a share below 1, or cells: got {value}"));
        match value {
            Value::String(text) => match text.strip_suffix('%') {
                Some(percent) => {
                    let percent: f64 = percent.trim().parse().map_err(|_| bad())?;
                    (percent > 0.0 && percent < 100.0).then_some(Size::Share(percent / 100.0)).ok_or_else(bad)
                }
                None => text.trim().parse::<u32>().map(Size::Cells).map_err(|_| bad()),
            },
            Value::Number(n) => match (n.as_u64(), n.as_f64()) {
                (Some(cells), _) if cells >= 1 => Ok(Size::Cells(cells as u32)),
                (_, Some(share)) if share > 0.0 && share < 1.0 => Ok(Size::Share(share)),
                _ => Err(bad()),
            },
            _ => Err(bad()),
        }
    }
    /// The share of `total` cells this size is.
    pub fn share_of(self, total: u32) -> f64 {
        match self {
            Size::Share(share) => share,
            Size::Cells(cells) => (cells as f64 / total.max(1) as f64).clamp(0.05, 0.95),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Leaf(PaneId),
    Split { axis: Axis, children: Vec<Child> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Child {
    /// This child's share of its parent; siblings' shares add up to 1.
    pub size: f64,
    pub node: Node,
}
/// A rectangle of cells within the view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub root: Node,
}
impl Layout {
    pub fn new(pane: PaneId) -> Layout {
        Layout { root: Node::Leaf(pane) }
    }
    /// Every pane, in layout order (depth first, left to right, top to bottom).
    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        collect(&self.root, &mut out);
        out
    }
    pub fn contains(&self, pane: PaneId) -> bool {
        self.panes().contains(&pane)
    }
    /// Puts `new` beside `target`. `size` is the new pane's share of the space `target` had;
    /// `full` splits the whole view instead (tmux's `split-window -f`).
    pub fn split(
        &mut self,
        target: PaneId,
        new: PaneId,
        side: Side,
        size: Option<Size>,
        full: bool,
        area: Rect,
    ) -> Result<()> {
        let span = |rect: Rect| if side.axis() == Axis::Row { rect.width } else { rect.height };
        let rect = if full {
            area
        } else {
            self.rect_of(target, area)
                .ok_or_else(|| Error::not_found(format!("pane {target} is not in this layout")))?
        };
        self.split_share(target, new, side, size.map_or(0.5, |s| s.share_of(span(rect))), full)
    }
    /// [`Layout::split`] with the new pane's share already known: what the core does with a
    /// place the host resolved.
    pub fn split_share(&mut self, target: PaneId, new: PaneId, side: Side, share: f64, full: bool) -> Result<()> {
        if self.contains(new) {
            return Err(Error::invalid_params(format!("pane {new} is already in this layout")));
        }
        if full {
            let old = std::mem::replace(&mut self.root, Node::Leaf(0));
            self.root = pair(old, Node::Leaf(new), side, share);
            normalise(&mut self.root);
            return Ok(());
        }
        if !self.contains(target) {
            return Err(Error::not_found(format!("pane {target} is not in this layout")));
        }
        split_at(&mut self.root, target, new, side, share);
        normalise(&mut self.root);
        Ok(())
    }
    /// Takes a pane out; its space goes to its siblings. `false` when it was the last pane.
    pub fn remove(&mut self, pane: PaneId) -> bool {
        if self.root == Node::Leaf(pane) {
            return false;
        }
        remove_from(&mut self.root, pane);
        normalise(&mut self.root);
        true
    }
    /// `a` where `b` was and `b` where `a` was.
    pub fn swap(&mut self, a: PaneId, b: PaneId) {
        swap_in(&mut self.root, a, b);
    }
    /// `new` takes `old`'s place.
    pub fn replace(&mut self, old: PaneId, new: PaneId) {
        swap_in(&mut self.root, old, new);
    }
    pub fn rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        place(&self.root, area, &mut out);
        out
    }
    pub fn rect_of(&self, pane: PaneId, area: Rect) -> Option<Rect> {
        self.rects(area).into_iter().find(|(p, _)| *p == pane).map(|(_, r)| r)
    }
    /// Neovim's `winlayout()`, with each child's share: `["row", [["leaf", 1, 0.5], ["leaf", 2, 0.5]]]`.
    pub fn to_value(&self) -> Value {
        node_value(&self.root, None)
    }
    pub fn from_value(value: &Value) -> Result<Layout> {
        let mut root = parse_node(value)?.0;
        normalise(&mut root);
        let layout = Layout { root };
        let panes = layout.panes();
        let mut seen = panes.clone();
        seen.sort_unstable();
        seen.dedup();
        if seen.len() != panes.len() {
            return Err(Error::invalid_params("a pane appears twice in the layout"));
        }
        Ok(layout)
    }
}
/// tmux's preset layouts (`select-layout`), for these panes in this order.
pub fn preset(name: &str, panes: &[PaneId], main: Option<Size>, area: Rect) -> Result<Layout> {
    let leaves = |ids: &[PaneId]| -> Vec<Child> {
        ids.iter().map(|p| Child { size: 1.0 / ids.len() as f64, node: Node::Leaf(*p) }).collect()
    };
    let split = |axis: Axis, children: Vec<Child>| -> Node {
        if children.len() == 1 {
            children.into_iter().next().expect("one").node
        } else {
            Node::Split { axis, children }
        }
    };
    let Some((first, rest)) = panes.split_first() else { return Err(Error::invalid_params("no panes to lay out")) };
    let root = match name {
        "even-horizontal" => split(Axis::Row, leaves(panes)),
        "even-vertical" => split(Axis::Col, leaves(panes)),
        "main-horizontal" | "main-horizontal-mirrored" | "main-vertical" | "main-vertical-mirrored" => {
            if rest.is_empty() {
                Node::Leaf(*first)
            } else {
                let vertical = name.starts_with("main-vertical");
                let (axis, inner, span) =
                    if vertical { (Axis::Row, Axis::Col, area.width) } else { (Axis::Col, Axis::Row, area.height) };
                let share = main
                    .map_or(if vertical { 80.0 / span.max(1) as f64 } else { 24.0 / span.max(1) as f64 }, |m| {
                        m.share_of(span)
                    });
                let share = share.clamp(0.05, 0.95);
                let main = Child { size: share, node: Node::Leaf(*first) };
                let others = Child { size: 1.0 - share, node: split(inner, leaves(rest)) };
                let children = if name.ends_with("mirrored") { vec![others, main] } else { vec![main, others] };
                Node::Split { axis, children }
            }
        }
        "tiled" => {
            let n = panes.len();
            let mut columns = 1;
            while columns * columns < n {
                columns += 1;
            }
            let rows = n.div_ceil(columns);
            let lines: Vec<Child> = panes
                .chunks(columns)
                .map(|chunk| Child { size: 1.0 / rows as f64, node: split(Axis::Row, leaves(chunk)) })
                .collect();
            split(Axis::Col, lines)
        }
        other => return Err(Error::invalid_params(format!("unknown layout: {other}"))),
    };
    let mut root = root;
    normalise(&mut root);
    Ok(Layout { root })
}
/// tmux's preset names, in `next-layout` order.
pub const PRESETS: &[&str] = &[
    "even-horizontal",
    "even-vertical",
    "main-horizontal",
    "main-horizontal-mirrored",
    "main-vertical",
    "main-vertical-mirrored",
    "tiled",
];
impl Layout {
    /// tmux's layout string (`window_layout`, `select-layout`): a checksum, then
    /// `WxH,X,Y,ID` for a pane, `WxH,X,Y{…}` for a row and `WxH,X,Y[…]` for a column.
    pub fn to_tmux(&self, area: Rect) -> String {
        let mut body = String::new();
        dump(&self.root, area, &mut body);
        format!("{:04x},{body}", checksum(&body))
    }
    /// Reads a tmux layout string. Its pane ids are tmux's; `panes` are this view's in layout
    /// order and take their places in order, as tmux's `select-layout` does.
    pub fn from_tmux(text: &str, panes: &[PaneId]) -> Result<Layout> {
        let bad = || Error::invalid_params(format!("invalid layout: {text}"));
        let (sum, body) = text.split_once(',').ok_or_else(bad)?;
        if u16::from_str_radix(sum, 16).map_err(|_| bad())? != checksum(body) {
            return Err(bad());
        }
        let mut chars = body.chars().peekable();
        let (node, _) = read_tmux(&mut chars).ok_or_else(bad)?;
        let mut node = node;
        let mut order = Vec::new();
        collect(&node, &mut order);
        if order.len() != panes.len() {
            return Err(Error::invalid_params(format!(
                "the layout has {} panes, the view {}",
                order.len(),
                panes.len()
            )));
        }
        let mut next = panes.iter();
        renumber(&mut node, &mut next);
        normalise(&mut node);
        Ok(Layout { root: node })
    }
    /// Grows or shrinks a pane by `cells` along an axis, from the side named (tmux's
    /// `resize-pane -L/-R/-U/-D`); its neighbour on that side gives or takes the space.
    pub fn resize(&mut self, pane: PaneId, side: Side, cells: i64, area: Rect) {
        let axis = side.axis();
        let span = if axis == Axis::Row { area.width } else { area.height } as f64;
        resize_in(&mut self.root, pane, axis, side.before(), cells as f64 / span.max(1.0), 1.0);
        normalise(&mut self.root);
    }
    /// Sets a pane's width or height in cells, as near as its siblings allow (`resize-pane -x`).
    pub fn set_size(&mut self, pane: PaneId, axis_row: bool, cells: u32, area: Rect) {
        let Some(rect) = self.rect_of(pane, area) else { return };
        let now = if axis_row { rect.width } else { rect.height } as i64;
        let side = if axis_row { Side::Right } else { Side::Down };
        self.resize(pane, side, cells as i64 - now, area);
        if self.rect_of(pane, area).map(|r| if axis_row { r.width } else { r.height } as i64) == Some(now) {
            let side = if axis_row { Side::Left } else { Side::Up };
            self.resize(pane, side, cells as i64 - now, area);
        }
    }
    /// The panes in layout order, rotated by one (tmux's `rotate-window`; `-D` the other way).
    pub fn rotate(&mut self, down: bool) {
        let panes = self.panes();
        if panes.len() < 2 {
            return;
        }
        let mut rotated = panes.clone();
        if down {
            rotated.rotate_right(1);
        } else {
            rotated.rotate_left(1);
        }
        let mut next = rotated.iter();
        renumber(&mut self.root, &mut next);
    }
}
fn renumber<'a>(node: &mut Node, next: &mut impl Iterator<Item = &'a PaneId>) {
    match node {
        Node::Leaf(pane) => *pane = *next.next().expect("as many panes as leaves"),
        Node::Split { children, .. } => children.iter_mut().for_each(|c| renumber(&mut c.node, next)),
    }
}
/// Finds the split along `axis` holding `pane` (or its ancestor), and moves `delta` of the view's
/// span between that child and the sibling before (`before`) or after it.
fn resize_in(node: &mut Node, pane: PaneId, axis: Axis, before: bool, delta: f64, scale: f64) -> bool {
    let Node::Split { axis: here, children } = node else { return false };
    let Some(i) = children.iter().position(|c| contains(&c.node, pane)) else { return false };
    let size = children[i].size;
    if resize_in(&mut children[i].node, pane, axis, before, delta, scale * size) {
        return true;
    }
    if *here != axis {
        return false;
    }
    let j = if before { i.checked_sub(1) } else { (i + 1 < children.len()).then_some(i + 1) };
    let Some(j) = j else { return false };
    let local = delta / scale.max(1e-6);
    let local = local.clamp(-(children[i].size - 0.02), children[j].size - 0.02);
    children[i].size += local;
    children[j].size -= local;
    true
}
fn contains(node: &Node, pane: PaneId) -> bool {
    match node {
        Node::Leaf(p) => *p == pane,
        Node::Split { children, .. } => children.iter().any(|c| contains(&c.node, pane)),
    }
}
fn checksum(body: &str) -> u16 {
    let mut csum: u16 = 0;
    for b in body.bytes() {
        csum = (csum >> 1) + ((csum & 1) << 15);
        csum = csum.wrapping_add(b as u16);
    }
    csum
}
fn dump(node: &Node, area: Rect, out: &mut String) {
    use std::fmt::Write;
    let _ = write!(out, "{}x{},{},{}", area.width, area.height, area.x, area.y);
    match node {
        Node::Leaf(pane) => {
            let _ = write!(out, ",{pane}");
        }
        Node::Split { axis, children } => {
            let (open, close) = if *axis == Axis::Row { ('{', '}') } else { ('[', ']') };
            out.push(open);
            let mut rects = Vec::new();
            place_children(*axis, children, area, &mut rects);
            for (i, (child, rect)) in children.iter().zip(rects).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                dump(&child.node, rect, out);
            }
            out.push(close);
        }
    }
}
fn place_children(axis: Axis, children: &[Child], area: Rect, out: &mut Vec<Rect>) {
    let total = if axis == Axis::Row { area.width } else { area.height };
    let usable = total.saturating_sub(children.len().saturating_sub(1) as u32);
    let mut offset = 0;
    for size in cells(children, usable) {
        out.push(match axis {
            Axis::Row => Rect { x: area.x + offset, y: area.y, width: size, height: area.height },
            Axis::Col => Rect { x: area.x, y: area.y + offset, width: area.width, height: size },
        });
        offset += size + 1;
    }
}
fn read_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<u32> {
    let mut n = String::new();
    while let Some(c) = chars.peek().filter(|c| c.is_ascii_digit()) {
        n.push(*c);
        chars.next();
    }
    n.parse().ok()
}
/// One cell of a tmux layout: the node and its size along both axes.
fn read_tmux(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<(Node, (u32, u32))> {
    let width = read_number(chars)?;
    (chars.next()? == 'x').then_some(())?;
    let height = read_number(chars)?;
    (chars.next()? == ',').then_some(())?;
    read_number(chars)?;
    (chars.next()? == ',').then_some(())?;
    read_number(chars)?;
    match chars.peek() {
        Some(',') => {
            chars.next();
            let id = read_number(chars)?;
            Some((Node::Leaf(id as PaneId), (width, height)))
        }
        Some('{') | Some('[') => {
            let axis = if chars.next()? == '{' { Axis::Row } else { Axis::Col };
            let close = if axis == Axis::Row { '}' } else { ']' };
            let mut kids = Vec::new();
            loop {
                let (node, (w, h)) = read_tmux(chars)?;
                kids.push((node, if axis == Axis::Row { w } else { h }));
                match chars.next()? {
                    ',' => continue,
                    c if c == close => break,
                    _ => return None,
                }
            }
            let total: u32 = kids.iter().map(|(_, s)| *s).sum::<u32>().max(1);
            let children = kids.into_iter().map(|(node, s)| Child { size: s as f64 / total as f64, node }).collect();
            Some((Node::Split { axis, children }, (width, height)))
        }
        _ => None,
    }
}
fn collect(node: &Node, out: &mut Vec<PaneId>) {
    match node {
        Node::Leaf(pane) => out.push(*pane),
        Node::Split { children, .. } => children.iter().for_each(|c| collect(&c.node, out)),
    }
}
/// A split of `old` and `new` along `side`, `new` taking `share`.
fn pair(old: Node, new: Node, side: Side, share: f64) -> Node {
    let share = share.clamp(0.05, 0.95);
    let old = Child { size: 1.0 - share, node: old };
    let new = Child { size: share, node: new };
    let children = if side.before() { vec![new, old] } else { vec![old, new] };
    Node::Split { axis: side.axis(), children }
}
fn split_at(node: &mut Node, target: PaneId, new: PaneId, side: Side, share: f64) -> bool {
    match node {
        Node::Leaf(pane) if *pane == target => {
            *node = pair(Node::Leaf(target), Node::Leaf(new), side, share);
            true
        }
        Node::Leaf(_) => false,
        Node::Split { axis, children } => {
            if *axis == side.axis()
                && let Some(i) = children.iter().position(|c| c.node == Node::Leaf(target))
            {
                let total = children[i].size;
                let share = share.clamp(0.05, 0.95);
                children[i].size = total * (1.0 - share);
                let at = if side.before() { i } else { i + 1 };
                children.insert(at, Child { size: total * share, node: Node::Leaf(new) });
                return true;
            }
            children.iter_mut().any(|c| split_at(&mut c.node, target, new, side, share))
        }
    }
}
fn remove_from(node: &mut Node, pane: PaneId) -> bool {
    let Node::Split { children, .. } = node else { return false };
    if let Some(i) = children.iter().position(|c| c.node == Node::Leaf(pane)) {
        children.remove(i);
        return true;
    }
    children.iter_mut().any(|c| remove_from(&mut c.node, pane))
}
fn swap_in(node: &mut Node, a: PaneId, b: PaneId) {
    match node {
        Node::Leaf(pane) if *pane == a => *pane = b,
        Node::Leaf(pane) if *pane == b => *pane = a,
        Node::Leaf(_) => {}
        Node::Split { children, .. } => children.iter_mut().for_each(|c| swap_in(&mut c.node, a, b)),
    }
}
/// Splits of one child become that child; a split inside a split along the same axis is
/// flattened into it; shares add up to 1.
fn normalise(node: &mut Node) {
    let Node::Split { axis, children } = node else { return };
    let axis = *axis;
    for child in children.iter_mut() {
        normalise(&mut child.node);
    }
    let mut flat = Vec::with_capacity(children.len());
    for child in children.drain(..) {
        match child.node {
            Node::Split { axis: inner, children: grand } if inner == axis => {
                flat.extend(grand.into_iter().map(|g| Child { size: g.size * child.size, node: g.node }));
            }
            node => flat.push(Child { size: child.size, node }),
        }
    }
    let total: f64 = flat.iter().map(|c| c.size.max(0.0)).sum();
    let count = flat.len() as f64;
    for child in &mut flat {
        child.size = if total > 0.0 { child.size.max(0.0) / total } else { 1.0 / count };
    }
    if flat.len() == 1 {
        *node = flat.pop().expect("one child").node;
    } else {
        *children = flat;
    }
}
fn place(node: &Node, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        Node::Leaf(pane) => out.push((*pane, area)),
        Node::Split { axis, children } => {
            let total = if *axis == Axis::Row { area.width } else { area.height };
            let borders = children.len().saturating_sub(1) as u32;
            let usable = total.saturating_sub(borders);
            let sizes = cells(children, usable);
            let mut offset = 0;
            for (child, size) in children.iter().zip(sizes) {
                let rect = match axis {
                    Axis::Row => Rect { x: area.x + offset, y: area.y, width: size, height: area.height },
                    Axis::Col => Rect { x: area.x, y: area.y + offset, width: area.width, height: size },
                };
                place(&child.node, rect, out);
                offset += size + 1;
            }
        }
    }
}
/// Whole cells per child, in proportion to their shares, adding up to `usable`, none below 1:
/// rounded down, then what is left goes to the largest remainders (the first of equals), so a
/// `25%` split of 24 rows is 17 and 6, as tmux's is.
fn cells(children: &[Child], usable: u32) -> Vec<u32> {
    let exact: Vec<f64> = children.iter().map(|c| c.size * usable as f64).collect();
    let mut sizes: Vec<u32> = exact.iter().map(|x| (x.floor() as u32).max(1)).collect();
    let mut sum: u32 = sizes.iter().sum();
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|a, b| (exact[*b] - exact[*b].floor()).total_cmp(&(exact[*a] - exact[*a].floor())).then(a.cmp(b)));
    let mut i = 0;
    while sum < usable {
        sizes[order[i % order.len()]] += 1;
        i += 1;
        sum += 1;
    }
    while sum > usable && sizes.iter().any(|s| *s > 1) {
        let biggest = (0..sizes.len()).max_by_key(|i| sizes[*i]).expect("children");
        sizes[biggest] -= 1;
        sum -= 1;
    }
    sizes
}
fn node_value(node: &Node, size: Option<f64>) -> Value {
    let mut out = match node {
        Node::Leaf(pane) => vec![json!("leaf"), json!(pane)],
        Node::Split { axis, children } => {
            let kind = if *axis == Axis::Row { "row" } else { "col" };
            vec![json!(kind), Value::Array(children.iter().map(|c| node_value(&c.node, Some(c.size))).collect())]
        }
    };
    if let Some(size) = size {
        out.push(json!((size * 1e6).round() / 1e6));
    }
    Value::Array(out)
}
fn parse_node(value: &Value) -> Result<(Node, Option<f64>)> {
    let bad = || {
        Error::invalid_params(format!("a layout node is [\"leaf\", pane] or [\"row\"|\"col\", [nodes]]: got {value}"))
    };
    let list = value.as_array().ok_or_else(bad)?;
    let size = list.get(2).and_then(Value::as_f64);
    match (list.first().and_then(Value::as_str), list.get(1)) {
        (Some("leaf"), Some(pane)) => Ok((Node::Leaf(pane.as_u64().ok_or_else(bad)?), size)),
        (Some(kind @ ("row" | "col")), Some(Value::Array(items))) if !items.is_empty() => {
            let axis = if kind == "row" { Axis::Row } else { Axis::Col };
            let parsed: Vec<(Node, Option<f64>)> = items.iter().map(parse_node).collect::<Result<_>>()?;
            let given: f64 = parsed.iter().filter_map(|(_, s)| *s).sum();
            let missing = parsed.iter().filter(|(_, s)| s.is_none()).count() as f64;
            let fill = if missing > 0.0 { ((1.0 - given).max(0.0) / missing).max(1e-3) } else { 0.0 };
            let children = parsed.into_iter().map(|(node, s)| Child { size: s.unwrap_or(fill), node }).collect();
            Ok((Node::Split { axis, children }, size))
        }
        _ => Err(bad()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const AREA: Rect = Rect { x: 0, y: 0, width: 81, height: 24 };
    #[test]
    fn splits_share_the_space_with_a_border_between() {
        let mut layout = Layout::new(1);
        layout.split(1, 2, Side::Right, None, false, AREA).unwrap();
        assert_eq!(
            layout.rects(AREA),
            vec![(1, Rect { x: 0, y: 0, width: 40, height: 24 }), (2, Rect { x: 41, y: 0, width: 40, height: 24 })]
        );
        layout.split(2, 3, Side::Down, Some(Size::Share(0.25)), false, AREA).unwrap();
        assert_eq!(
            layout.to_value(),
            json!(["row", [["leaf", 1, 0.5], ["col", [["leaf", 2, 0.75], ["leaf", 3, 0.25]], 0.5]]])
        );
        let rects = layout.rects(AREA);
        assert_eq!(rects[2], (3, Rect { x: 41, y: 18, width: 40, height: 6 }));
    }
    #[test]
    fn a_split_along_the_same_axis_does_not_nest() {
        let mut layout = Layout::new(1);
        layout.split(1, 2, Side::Right, None, false, AREA).unwrap();
        layout.split(2, 3, Side::Right, None, false, AREA).unwrap();
        assert_eq!(layout.to_value(), json!(["row", [["leaf", 1, 0.5], ["leaf", 2, 0.25], ["leaf", 3, 0.25]]]));
        layout.split(1, 4, Side::Left, None, true, AREA).unwrap();
        assert_eq!(layout.panes(), vec![4, 1, 2, 3]);
    }
    #[test]
    fn removing_gives_the_space_back_and_collapses() {
        let mut layout = Layout::new(1);
        layout.split(1, 2, Side::Right, None, false, AREA).unwrap();
        layout.split(2, 3, Side::Down, None, false, AREA).unwrap();
        assert!(layout.remove(3));
        assert_eq!(layout.to_value(), json!(["row", [["leaf", 1, 0.5], ["leaf", 2, 0.5]]]));
        assert!(layout.remove(1));
        assert_eq!(layout.to_value(), json!(["leaf", 2]));
        assert!(!layout.remove(2), "the last pane stays");
    }
    #[test]
    fn the_wire_shape_round_trips_and_fills_missing_sizes() {
        let value = json!(["col", [["leaf", 5], ["row", [["leaf", 6], ["leaf", 7]]]]]);
        let layout = Layout::from_value(&value).unwrap();
        assert_eq!(
            layout.to_value(),
            json!(["col", [["leaf", 5, 0.5], ["row", [["leaf", 6, 0.5], ["leaf", 7, 0.5]], 0.5]]])
        );
        assert_eq!(Layout::from_value(&layout.to_value()).unwrap(), layout);
        assert!(Layout::from_value(&json!(["row", [["leaf", 1], ["leaf", 1]]])).is_err());
        assert!(Layout::from_value(&json!(["diagonal", []])).is_err());
    }
    #[test]
    fn sizes_parse_as_tmux_writes_them() {
        assert_eq!(Size::parse(&json!("30%")).unwrap(), Size::Share(0.3));
        assert_eq!(Size::parse(&json!(20)).unwrap(), Size::Cells(20));
        assert_eq!(Size::parse(&json!(0.25)).unwrap(), Size::Share(0.25));
        assert!(Size::parse(&json!("130%")).is_err());
    }
}
