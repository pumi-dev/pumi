//! The model's objects as the API shows them: what `session.get`, `view.get`, `pane.get` and
//! `tab.get` answer, where a tab is (`TabPlace`), a view's layout as a tree of halves, and the
//! `<noun>.changed` events' data. Ids here are the prefixed strings of `pumi_api::ids`; the model
//! keeps numbers. It never reads a process or a tab's engine: the core fills those fields in.
use crate::state::TabState;
use crate::{Model, PaneId, Place, SERVER, SessionId, TabId, View, ViewId};
use pumi_api::{Caller, Error, PaneRef, Result, SessionRef, TabRef, ViewRef};
use pumi_layout::{Axis, Child, Node, PlaceKind, PlaceSpec, Rect, Side};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// A field of a patch that may be left out (`None`), cleared (`Some(None)`) or set.
pub fn patch_field<'de, T: Deserialize<'de>, D: Deserializer<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// A session, as `session.get` answers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionInfo {
    pub id: SessionRef,
    pub name: String,
    /// Its directory, where its terminals start without a `cwd` of their own.
    pub cwd: Option<String>,
    /// Its views, in index order.
    pub views: Vec<ViewRef>,
    pub active_view: Option<ViewRef>,
    /// With `deep`: every view, each with its tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tree: Option<Vec<ViewTree>>,
}
/// A view and its tabs, in `session.get {deep}`'s tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ViewTree {
    #[serde(flatten)]
    pub view: ViewInfo,
    pub tabs: Vec<TabInfo>,
}
/// A view, as `view.get` answers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ViewInfo {
    pub id: ViewRef,
    pub session: SessionRef,
    /// The name it was given; `null` while it takes its active tab's title.
    pub name: Option<String>,
    /// Its index in its session.
    pub index: u32,
    pub layout: LayoutTree,
    /// The tab with the focus, in front of its pane.
    pub active_tab: Option<TabRef>,
    /// The pane with the focus.
    pub active_pane: Option<PaneRef>,
    /// The pane zoomed to fill it.
    pub zoom: Option<PaneRef>,
}
/// A pane, as `pane.get` answers it: an area of a view's layout and the tabs it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaneInfo {
    pub id: PaneRef,
    pub view: ViewRef,
    /// Its tabs, in the order its strip shows them.
    pub tabs: Vec<TabRef>,
    /// The tab in front.
    pub active_tab: TabRef,
    /// It has its view's focus.
    pub is_focused: bool,
    /// It fills its view.
    pub is_zoomed: bool,
}
/// A view's panes: a pane, or a split of two halves along an axis.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum LayoutTree {
    Pane(LayoutPane),
    Split(LayoutSplit),
}
/// A pane of a layout. `view.update {layout}` names each pane, and its tabs stay as they are.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutPane {
    pub pane: PaneRef,
    /// Its tabs, in its strip's order.
    #[serde(default)]
    pub tabs: Vec<TabRef>,
    /// The tab in front.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<TabRef>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutSplit {
    pub axis: LayoutAxis,
    /// The first half's share, between 0 and 1.
    pub ratio: f64,
    pub first: Box<LayoutTree>,
    pub second: Box<LayoutTree>,
}
/// A `row` lays its halves out left to right, a `column` top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LayoutAxis {
    Row,
    Column,
}
impl LayoutTree {
    pub fn of(view: &View) -> LayoutTree {
        tree(&view.layout.root, view)
    }
    /// The tree in the core's own shape (`core.viewSetLayout`'s): `["row", [["leaf", 1, 0.5], …]]`,
    /// a leaf being a pane's number.
    pub fn to_core(&self) -> Result<Value> {
        self.core_value(None)
    }
    fn core_value(&self, size: Option<f64>) -> Result<Value> {
        let mut out = match self {
            LayoutTree::Pane(leaf) => vec![json!("leaf"), json!(leaf.pane.0)],
            LayoutTree::Split(split) => {
                if !(split.ratio > 0. && split.ratio < 1.) {
                    return Err(Error::invalid_params(format!("a split's ratio is between 0 and 1: {}", split.ratio)));
                }
                let axis = if split.axis == LayoutAxis::Row { "row" } else { "col" };
                let halves =
                    vec![split.first.core_value(Some(split.ratio))?, split.second.core_value(Some(1. - split.ratio))?];
                vec![json!(axis), Value::Array(halves)]
            }
        };
        if let Some(size) = size {
            out.push(json!(size));
        }
        Ok(Value::Array(out))
    }
}
fn tree(node: &Node, view: &View) -> LayoutTree {
    match node {
        Node::Leaf(pane) => {
            let p = view.panes.get(pane);
            LayoutTree::Pane(LayoutPane {
                pane: PaneRef(*pane),
                tabs: p.map(|p| p.tabs.iter().map(|t| TabRef(*t)).collect()).unwrap_or_default(),
                active: p.map(|p| TabRef(p.active)),
            })
        }
        Node::Split { axis, children } => halves(*axis, children, view),
    }
}
fn halves(axis: Axis, children: &[Child], view: &View) -> LayoutTree {
    let Some((first, rest)) = children.split_first() else { unreachable!("a split has children") };
    if rest.is_empty() {
        return tree(&first.node, view);
    }
    let total: f64 = children.iter().map(|c| c.size).sum();
    let ratio = if total > 0. { first.size / total } else { 1. / children.len() as f64 };
    LayoutTree::Split(LayoutSplit {
        axis: if axis == Axis::Row { LayoutAxis::Row } else { LayoutAxis::Column },
        ratio: (ratio * 1e6).round() / 1e6,
        first: Box::new(tree(&first.node, view)),
        second: Box::new(halves(axis, rest, view)),
    })
}

/// What a place is: tiled in a view's layout, floating over a view, hidden while it moves, or
/// tiled in a new view or a new session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TabPlaceKind {
    Tiled,
    Float,
    Hidden,
    View,
    Session,
}
/// A session a place names: one there is, or `new`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum SessionTarget {
    Session(SessionRef),
    New(NewWord),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NewWord {
    New,
}
/// A pane a place names: one there is, or `current`, the caller's tab's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum PaneTarget {
    Pane(PaneRef),
    Current(CurrentWord),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum CurrentWord {
    Current,
}
/// Where a tab is, and where `tab.open` and `tab.update` put one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabPlace {
    pub kind: TabPlaceKind,
    /// Tiled: into this view, beside its active tab. Float: over this view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<ViewRef>,
    /// Tiled: into this pane (`current`: the caller's tab's), as one of its tabs; with `split`,
    /// beside it, in a pane of its own. Where a tiled tab is, its pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<PaneTarget>,
    /// View: the session the new view goes in; `new` makes a session for the tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionTarget>,
    /// Tiled: which side of the pane it splits, making a pane for the tab; `down` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<Side>,
    /// View: the new view's index. Into a pane: where in its strip, after its front tab by
    /// default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// Moving: trade places with this tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap: Option<TabRef>,
    /// Tiled: the new pane's share of what it splits, between 0 and 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ratio: Option<f64>,
    /// Tiled: the tab whose pane to split, the caller's by default. Float: the tab
    /// `relative: "pane"` is relative to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beside: Option<TabRef>,
    /// Tiled: the new pane's size, `"30%"` or cells, instead of `ratio`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "length_schema")]
    pub size: Option<Value>,
    /// Tiled: split the whole view, not one pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    /// View, session: its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Session: its directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Make it the caller's tab; `true` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<bool>,
    /// Opening: in this tab instead of the block it holds, keeping its id, place and options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<TabRef>,
    /// Float: what it floats over, `view` (the default) or `client`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative: Option<String>,
    /// Float: the corner or edge it is pinned to (`NW` … `SE`, `center`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "anchor_schema")]
    pub anchor: Option<String>,
    /// Float: how far in from its anchor: cells, `"24px"` or `"10%"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "length_schema")]
    pub row: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "length_schema")]
    pub col: Option<Value>,
    /// Float: cells, `"480px"` or a share of what it floats over (`"50%"`, the default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "length_schema")]
    pub width: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "length_schema")]
    pub height: Option<Value>,
    /// Float: `none` draws it bare.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    /// Float: it takes the keyboard when it opens (`true` by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focusable: Option<bool>,
    /// Float: it takes the pointer (`focusable` by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse: Option<bool>,
    /// Float: what closes it: `escape`, `key`, `press`, `scroll`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismiss: Option<Vec<String>>,
    /// Float: dims what is under it, 0 to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<u8>,
    /// Float: above floats with a lower one (50 by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_index: Option<i64>,
}
/// A tiled size or a float's length, as `TabPlace` takes it: cells, or a string (`"30%"`,
/// `"480px"`).
fn length_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "anyOf": [{ "type": "number" }, { "type": "string" }, { "type": "null" }] })
}
/// A float's anchor: one of `pumi_layout::Anchor::NAMES`.
fn anchor_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "anyOf": [{ "type": "string", "enum": pumi_layout::Anchor::NAMES }, { "type": "null" }] })
}
impl TabPlace {
    pub fn new(kind: TabPlaceKind) -> TabPlace {
        TabPlace {
            kind,
            view: None,
            pane: None,
            session: None,
            split: None,
            index: None,
            swap: None,
            ratio: None,
            beside: None,
            size: None,
            full: None,
            name: None,
            cwd: None,
            focus: None,
            replace: None,
            relative: None,
            anchor: None,
            row: None,
            col: None,
            width: None,
            height: None,
            border: None,
            focusable: None,
            mouse: None,
            dismiss: None,
            backdrop: None,
            z_index: None,
        }
    }
    /// Where a tab is: `pane` is a tiled tab's.
    pub fn of(place: &Place, pane: Option<PaneId>) -> TabPlace {
        match place {
            Place::Tiled { view } => TabPlace {
                view: Some(ViewRef(*view)),
                pane: pane.map(|p| PaneTarget::Pane(PaneRef(p))),
                ..TabPlace::new(TabPlaceKind::Tiled)
            },
            Place::Float { view, float } => TabPlace {
                view: Some(ViewRef(*view)),
                beside: float.tab.map(TabRef),
                relative: Some(float.relative.clone()),
                anchor: Some(float.anchor.clone()),
                row: Some(float.row.to_value()),
                col: Some(float.col.to_value()),
                width: Some(float.width.to_value()),
                height: Some(float.height.to_value()),
                border: float.border.clone(),
                focusable: Some(float.focusable),
                mouse: float.mouse,
                dismiss: (!float.dismiss.is_empty()).then(|| float.dismiss.clone()),
                backdrop: float.backdrop,
                z_index: Some(float.zindex),
                ..TabPlace::new(TabPlaceKind::Float)
            },
            Place::Detached => TabPlace::new(TabPlaceKind::Hidden),
        }
    }
    /// The place as the core's `PlaceSpec` says it, ids as numbers, before the host resolves it.
    pub fn spec(&self) -> Result<PlaceSpec> {
        let kind = match (self.kind, &self.session) {
            (TabPlaceKind::Hidden, _) => {
                return Err(Error::invalid_params("`hidden` is where a tab is while it moves, not a place to ask for"));
            }
            (TabPlaceKind::Session, _) | (_, Some(SessionTarget::New(_))) => PlaceKind::Session,
            (TabPlaceKind::Tiled, _) => PlaceKind::Tiled,
            (TabPlaceKind::Float, _) => PlaceKind::Float,
            (TabPlaceKind::View, _) => PlaceKind::View,
        };
        let size = match (self.ratio, &self.size) {
            (Some(_), Some(_)) => return Err(Error::invalid_params("say `ratio` or `size`, not both")),
            (Some(ratio), None) if ratio > 0. && ratio < 1. => Some(json!(format!("{}%", ratio * 100.))),
            (Some(ratio), None) => return Err(Error::invalid_params(format!("a ratio is between 0 and 1: {ratio}"))),
            (None, size) => size.clone(),
        };
        Ok(PlaceSpec {
            kind: Some(kind),
            beside: self.beside.map(|p| p.0),
            split: self.split,
            size,
            full: self.full,
            view: self.view.map(|v| v.0),
            pane: self.pane.as_ref().map(|p| match p {
                PaneTarget::Pane(p) => p.0,
                PaneTarget::Current(_) => 0,
            }),
            session: match &self.session {
                Some(SessionTarget::Session(s)) => Some(s.0),
                _ => None,
            },
            index: self.index,
            name: self.name.clone(),
            cwd: self.cwd.clone(),
            relative: self.relative.clone(),
            anchor: self.anchor.clone(),
            row: self.row.clone(),
            col: self.col.clone(),
            width: self.width.clone(),
            height: self.height.clone(),
            border: self.border.clone(),
            focusable: self.focusable,
            mouse: self.mouse,
            dismiss: self.dismiss.clone(),
            backdrop: self.backdrop,
            zindex: self.z_index,
            swap: self.swap.map(|p| p.0),
            focus: self.focus,
            replace: self.replace.map(|p| p.0),
        })
    }
}

/// A tab's foreground process.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Process {
    pub pid: u32,
    /// Its program, as the system names it.
    pub command: Vec<String>,
}
/// How far a tab is scrolled back.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scroll {
    pub offset_from_bottom: u64,
}
/// The tab beside a tab on each side, in its view.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Neighbors {
    pub left: Option<TabRef>,
    pub right: Option<TabRef>,
    pub up: Option<TabRef>,
    pub down: Option<TabRef>,
}
/// A tab, as `tab.get` answers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabInfo {
    pub id: TabRef,
    pub session: SessionRef,
    pub view: Option<ViewRef>,
    /// The block it holds (`terminal`, `owner.name.page`).
    pub block: String,
    /// Each trait it has, with its fields: `{agent: {status, session, …}}`.
    pub traits: BTreeMap<String, Value>,
    pub place: TabPlace,
    pub title: Option<String>,
    /// Its directory: a terminal's program's.
    pub cwd: Option<String>,
    /// The directory of its foreground process.
    pub foreground_cwd: Option<String>,
    pub process: Option<Process>,
    pub scroll: Scroll,
    pub neighbors: Neighbors,
    /// It is the tab a client is in: its view's active tab, in its session's active view.
    pub is_focused: bool,
    /// It fills its view.
    pub is_zoomed: bool,
    /// The options set on it.
    pub options: Map<String, Value>,
    /// With `state`: its saved state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<TabState>,
}

/// The pane beside `pane` on `side` among `rects` (its view's), the one used last of several.
pub fn neighbour(
    rects: &[(PaneId, Rect)],
    pane: PaneId,
    side: Side,
    activity: impl Fn(PaneId) -> u64,
) -> Option<PaneId> {
    let (_, me) = *rects.iter().find(|(p, _)| *p == pane)?;
    let overlap = |a0: u32, a1: u32, b0: u32, b1: u32| a0 < b1 && b0 < a1;
    rects
        .iter()
        .filter(|(p, r)| {
            *p != pane
                && match side {
                    Side::Left => r.x + r.width + 1 == me.x && overlap(r.y, r.y + r.height, me.y, me.y + me.height),
                    Side::Right => me.x + me.width + 1 == r.x && overlap(r.y, r.y + r.height, me.y, me.y + me.height),
                    Side::Up => r.y + r.height + 1 == me.y && overlap(r.x, r.x + r.width, me.x, me.x + me.width),
                    Side::Down => me.y + me.height + 1 == r.y && overlap(r.x, r.x + r.width, me.x, me.x + me.width),
                }
        })
        .max_by_key(|(p, _)| activity(*p))
        .map(|(p, _)| *p)
}

/// A `<noun>.changed` event's data: the object's id, the fields that changed, and their values.
pub fn changed(noun: &str, id: &str, patch: Map<String, Value>) -> Value {
    let keys: Vec<&String> = patch.keys().collect();
    json!({ noun: id, "keys": keys, "patch": patch })
}

impl Model {
    pub fn session_ref(&self, caller: &Caller, id: Option<SessionRef>) -> Result<SessionId> {
        self.session_id(caller, id.map_or(0, |s| s.0))
    }
    pub fn view_ref(&self, caller: &Caller, id: Option<ViewRef>) -> Result<ViewId> {
        self.view_id(caller, id.map_or(0, |v| v.0))
    }
    pub fn tab_ref(&self, caller: &Caller, id: Option<TabRef>) -> Result<TabId> {
        self.tab_id(caller, id.map_or(0, |t| t.0))
    }
    pub fn pane_ref(&self, caller: &Caller, id: Option<PaneRef>) -> Result<(ViewId, PaneId)> {
        self.pane_id(caller, id.map_or(0, |p| p.0))
    }
    pub fn pane_info(&self, view: ViewId, pane: PaneId) -> Result<PaneInfo> {
        let v = self.views.get(&view).ok_or_else(|| Error::not_found(format!("no view @{view}")))?;
        let p = v.panes.get(&pane).ok_or_else(|| Error::not_found(format!("no pane :{pane}")))?;
        let focused = v.active_pane() == Some(pane);
        Ok(PaneInfo {
            id: PaneRef(pane),
            view: ViewRef(view),
            tabs: p.tabs.iter().map(|t| TabRef(*t)).collect(),
            active_tab: TabRef(p.active),
            is_focused: focused,
            is_zoomed: focused && v.zoomed,
        })
    }
    pub fn session_info(&self, id: SessionId) -> SessionInfo {
        let s = &self.sessions[&id];
        SessionInfo {
            id: SessionRef(id),
            name: s.name.clone(),
            cwd: s.cwd.clone(),
            views: s.views.iter().map(|(_, v)| ViewRef(*v)).collect(),
            active_view: s.current.map(ViewRef),
            tree: None,
        }
    }
    pub fn view_info(&self, id: ViewId) -> Result<ViewInfo> {
        let v = self.views.get(&id).ok_or_else(|| Error::not_found(format!("no view @{id}")))?;
        let session =
            self.session_of_view(id).ok_or_else(|| Error::not_found(format!("view @{id} is in no session")))?;
        let index = self.sessions[&session].views.iter().find(|(_, x)| *x == id).map_or(0, |(i, _)| *i);
        Ok(ViewInfo {
            id: ViewRef(id),
            session: SessionRef(session),
            name: v.name.clone(),
            index,
            layout: LayoutTree::of(v),
            active_tab: self.tabs.contains_key(&v.active).then_some(TabRef(v.active)),
            active_pane: v.active_pane().map(PaneRef),
            zoom: v.zoomed.then(|| v.active_pane().map(PaneRef)).flatten(),
        })
    }
    /// The session a tab's calls are in: its view's, else the one used last.
    pub fn session_of_tab(&self, tab: TabId) -> Option<SessionId> {
        match self.view_of_tab(tab) {
            Some(view) => self.session_of_view(view),
            None => self.current(&Caller { channel: SERVER, ..Caller::default() }).map(|c| c.session),
        }
    }
    /// Whether a client is in `tab`: its view's active tab, in its session's active view, in a
    /// session a client shows (with no client, the one used last).
    pub fn is_focused(&self, tab: TabId) -> bool {
        let Some(view) = self.view_of_tab(tab) else { return false };
        let Some(session) = self.session_of_view(view) else { return false };
        let shown = |s: SessionId| {
            let mut clients = self.chans.values().filter_map(|c| c.ui.as_ref()).peekable();
            if clients.peek().is_none() {
                return self.current(&Caller { channel: SERVER, ..Caller::default() }).is_some_and(|c| c.session == s);
            }
            clients.any(|ui| ui.session == s)
        };
        self.views[&view].active == tab && self.sessions[&session].current == Some(view) && shown(session)
    }
}
