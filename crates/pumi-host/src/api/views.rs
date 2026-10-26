//! `view.update`: a view's name, its place among its session's views (or another session's), its
//! layout (a tree, one of tmux's presets, or the next preset) and its zoom.
use super::kernel;
use super::select::{Zoom, set_zoom};
use crate::kernel::layout::{self, Size};
use crate::kernel::objects::{LayoutTree, patch_field};
use crate::kernel::options::Key;
use crate::kernel::{Kernel, SessionId, ViewId};
use pumi_api::{Cx, Error, PaneRef, Result, SessionRef, ViewRef, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
/// What `view.update` lays a view out as.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum LayoutSpec {
    /// A preset: `even-horizontal`, `even-vertical`, `main-horizontal`,
    /// `main-horizontal-mirrored`, `main-vertical`, `main-vertical-mirrored` or `tiled`.
    Preset(#[schemars(schema_with = "preset_schema")] String),
    /// The preset so many on from the view's last one (`next-layout` is `{"offset": 1}`).
    Offset(LayoutOffset),
    /// A tree in `view.get`'s `layout` shape, holding the view's panes each once (their `tabs`
    /// may be left out: a pane keeps its own).
    Tree(LayoutTree),
}
/// A preset's name: one of `layout::PRESETS`.
fn preset_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "type": "string", "enum": layout::PRESETS })
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutOffset {
    pub offset: i64,
}
/// The preset each view had last, for `offset`.
#[derive(Default)]
struct Presets(BTreeMap<ViewId, usize>);
/// A patch to a view: what `view.update` changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewPatch {
    /// Its name; `null` gives it back its automatic name (its active tab's title).
    #[serde(default, deserialize_with = "patch_field", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    /// Its index in its session, which must be free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// The session it moves to, at `index` or its first free one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<LayoutSpec>,
    /// The pane zoomed to fill it, which gets its focus; `null` unzooms it.
    #[serde(default, deserialize_with = "patch_field", skip_serializing_if = "Option::is_none")]
    pub zoom: Option<Option<PaneRef>>,
}
/// Change a view: its name, index or session, layout, and zoom. Each change fires `view.changed`
/// with what changed. No id is the caller's view.
#[api(
    name = "view.update",
    example(args = r#"["@1", {"name": "tests", "zoom": ":3"}]"#, result = "null"),
    example(args = r#"["@1", {"index": 5}]"#, result = "null", desc = "To index 5 of its session."),
    example(
        args = r#"["@1", {"session": "$2"}]"#,
        result = "null",
        desc = "Into session $2, at its first free index."
    ),
    example(args = r#"["@1", {"layout": "even-horizontal"}]"#, result = "null"),
    example(args = r#"["@1", {"layout": {"offset": 1}}]"#, result = "null", desc = "The next preset."),
    example(
        args = r#"["@1", {"layout": {"axis": "row", "ratio": 0.7, "first": {"pane": ":1"}, "second": {"pane": ":2"}}}]"#,
        result = "null",
        desc = "A tree."
    )
)]
pub fn pumi_view_update(cx: &mut Cx, view: Option<ViewRef>, patch: ViewPatch) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_ref(&caller, view)?;
    if let Some(name) = patch.name {
        k.call_core(&caller, "core.viewSetName", json!([view, name]))?;
    }
    if patch.index.is_some() || patch.session.is_some() {
        set_place(cx, view, patch.session, patch.index)?;
    }
    if let Some(layout) = patch.layout {
        set_layout(cx, view, layout)?;
    }
    match patch.zoom {
        Some(Some(pane)) => {
            let (caller, k) = kernel(cx)?;
            let (at, pane) = k.model.pane_ref(&caller, Some(pane))?;
            if at != view {
                return Err(Error::invalid_params(format!("pane :{pane} is not in view @{view}")));
            }
            let front = k.model.views[&view].panes[&pane].active;
            set_zoom(cx, front, Zoom::Set(true))?;
        }
        Some(None) => {
            let (caller, k) = kernel(cx)?;
            k.call_core(&caller, "core.viewSetZoom", json!([view, false]))?;
        }
        None => {}
    }
    Ok(())
}
/// Lays a view out again: as a tree, a preset, or the preset so many on.
fn set_layout(cx: &mut Cx, view: ViewId, layout: LayoutSpec) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let at = match layout {
        LayoutSpec::Tree(tree) => {
            return k.call_core(&caller, "core.viewSetLayout", json!([view, tree.to_core()?])).map(|_| ());
        }
        LayoutSpec::Preset(name) => layout::PRESETS
            .iter()
            .position(|n| *n == name.as_str())
            .ok_or_else(|| Error::invalid_params(format!("unknown layout: {name}")))?,
        LayoutSpec::Offset(LayoutOffset { offset: by }) => {
            let n = layout::PRESETS.len() as i64;
            let last =
                k.ext.get_or_default::<Presets>().0.get(&view).map_or(if by > 0 { -1 } else { 0 }, |a| *a as i64);
            (last + by).rem_euclid(n) as usize
        }
    };
    k.ext.get_or_default::<Presets>().0.insert(view, at);
    let chain = k.model.chain(None, Some(view), None);
    let main = k.model.options.get("main_pane_width", &chain).and_then(|(v, _)| Size::parse(&v).ok());
    let panes = k.model.views[&view].layout.panes();
    let laid = layout::preset(layout::PRESETS[at], &panes, main, k.view_area(view))?;
    k.call_core(&caller, "core.viewSetLayout", json!([view, laid.to_value()])).map(|_| ())
}
/// Moves a view to another index or session. A view is in one session.
fn set_place(cx: &mut Cx, view: ViewId, session: Option<SessionRef>, index: Option<u32>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let holding: Vec<SessionId> =
        k.model.sessions.values().filter(|s| s.views.iter().any(|(_, v)| *v == view)).map(|s| s.id).collect();
    let edit = |k: &Kernel, session: SessionId| {
        let s = &k.model.sessions[&session];
        (s.views.clone(), s.current, s.last)
    };
    let session = match session {
        Some(s) => k.model.session_ref(&caller, Some(s))?,
        None => {
            k.model.session_of_view(view).ok_or_else(|| Error::not_found(format!("view @{view} is in no session")))?
        }
    };
    let index = index.unwrap_or_else(|| free_index(k, session));
    match k.model.sessions[&session].views.iter().find(|(i, _)| *i == index).map(|(_, v)| *v) {
        Some(v) if v == view => return Ok(()),
        Some(_) => return Err(Error::invalid_params(format!("index {index} in use"))),
        None => {}
    }
    let mut put: BTreeMap<SessionId, Views> = BTreeMap::new();
    for other in holding.iter().copied().filter(|s| *s != session) {
        let (mut views, mut current, last) = edit(k, other);
        views.retain(|(_, v)| *v != view);
        if current == Some(view) {
            current = last.filter(|l| *l != view).or_else(|| views.first().map(|(_, v)| *v));
        }
        put.insert(other, (views, current, last));
    }
    let (mut views, mut current, last) = edit(k, session);
    views.retain(|(_, v)| *v != view);
    views.push((index, view));
    views.sort_by_key(|(i, _)| *i);
    if current.is_none() {
        current = Some(view);
    }
    put.insert(session, (views, current, last));
    let sessions: Vec<Value> = put
        .into_iter()
        .map(|(session, (views, current, last))| json!({ "session": session, "views": views, "current": current, "last": last }))
        .collect();
    k.call_core(&caller, "core.viewSetPlace", json!([view, sessions])).map(|_| ())
}
/// A session's views as they will be: `[index, view]`, then current and last.
type Views = (Vec<(u32, ViewId)>, Option<ViewId>, Option<ViewId>);
/// The lowest free index at or above `base_index`.
fn free_index(k: &Kernel, session: SessionId) -> u32 {
    let base = k
        .model
        .options
        .get("base_index", &[Key::Session(session), Key::Global])
        .and_then(|(v, _)| v.as_u64())
        .unwrap_or(0) as u32;
    let used: Vec<u32> = k.model.sessions[&session].views.iter().map(|(i, _)| *i).collect();
    (base..).find(|i| !used.contains(i)).expect("a free index")
}
