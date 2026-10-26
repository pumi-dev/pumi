//! Moving the caller between sessions, views, panes and tabs (`session.focus`, `view.focus`,
//! `pane.focus`, `tab.focus`), and the changes that lay a view out again (`pane.update`'s and
//! `tab.update`'s `size` and `zoom`). Focus is a change to the parent's `activeView`,
//! `activeTab` or `activePane`, so it fires `session.changed` or `view.changed`.
use super::kernel;
use crate::kernel::layout::Side;
use crate::kernel::objects::neighbour;
use crate::kernel::{Kernel, PaneId, TabId, ViewId};
use pumi_api::{Cx, Error, PaneRef, Result, SessionRef, TabRef, ViewRef, api};
use serde::{Deserialize, Serialize};
use serde_json::json;
/// The pane beside `pane` on `side`, the one used last of several (`{left-of}`).
fn beside(k: &Kernel, view: ViewId, pane: PaneId, side: Side) -> Option<PaneId> {
    let v = k.model.views.get(&view)?;
    let activity = |p: PaneId| v.panes.get(&p).and_then(|p| k.model.tabs.get(&p.active)).map_or(0, |t| t.activity);
    neighbour(&k.pane_rects(view), pane, side, activity)
}
/// Makes `target` current, unzooming a zoomed view first when it is in another pane.
fn focus_to(cx: &mut Cx, from: TabId, target: TabId) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let across = k.model.pane_of_tab(from).map(|(_, p)| p) != k.model.pane_of_tab(target).map(|(_, p)| p);
    let zoomed = k.model.view_of_tab(target).is_some_and(|v| k.model.views[&v].zoomed);
    k.call_core(&caller, "core.setCurrentTab", json!([target]))?;
    if zoomed && across {
        set_zoom(cx, target, Zoom::Set(false))?;
    }
    Ok(())
}
/// Where `tab.focus` moves from its tab.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabFocus {
    /// To the tab in front of the pane beside its pane on this side.
    #[serde(default)]
    pub direction: Option<Side>,
    /// To so many tabs on (or back, negative) in its pane's strip, wrapping.
    #[serde(default)]
    pub offset: Option<i64>,
    /// To its view's tab used last.
    #[serde(default)]
    pub last: bool,
}
/// Make a tab its pane's front tab and its view's active tab, and its view and session current;
/// or move from it: to the next tab of its pane (`offset`), to the pane beside it (`direction`),
/// or to its view's tab used last (`last`). No id is the caller's tab. Moving out of a zoomed
/// pane unzooms the view; nothing there is not an error.
#[api(
    name = "tab.focus",
    example(args = r#"["%3"]"#, result = "null"),
    example(args = r#"["%2", {"offset": 1}]"#, result = "null", desc = "The tab after %2 in its pane."),
    example(args = r#"[null, {"direction": "up"}]"#, result = "null", desc = "The pane above the caller's."),
    example(args = r#"[null, {"last": true}]"#, result = "null")
)]
pub fn pumi_tab_focus(cx: &mut Cx, tab: Option<TabRef>, opts: Option<TabFocus>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let how = opts.unwrap_or_default();
    let tab = k.model.tab_ref(&caller, tab)?;
    let ways = usize::from(how.direction.is_some()) + usize::from(how.offset.is_some()) + usize::from(how.last);
    if ways == 0 {
        return focus_to(cx, tab, tab);
    }
    if ways > 1 {
        return Err(Error::invalid_params("say one of `direction`, `offset` and `last`"));
    }
    let view = k.model.view_of_tab(tab).ok_or_else(|| Error::invalid_params(format!("tab %{tab} is in no view")))?;
    let v = &k.model.views[&view];
    let pane = v.pane_of(tab);
    let target = if how.last {
        v.last
    } else if let Some(by) = how.offset {
        let tabs = pane.map(|p| v.panes[&p].tabs.clone()).unwrap_or_default();
        let at = tabs.iter().position(|t| *t == tab).unwrap_or(0) as i64;
        tabs.get((at + by).rem_euclid(tabs.len().max(1) as i64) as usize).copied()
    } else {
        let side = how.direction.expect("one way");
        pane.and_then(|p| beside(k, view, p, side)).map(|p| v.panes[&p].active)
    };
    let Some(target) = target.filter(|t| *t != tab) else { return Ok(()) };
    focus_to(cx, tab, target)
}
/// Where `pane.focus` moves from its pane.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaneFocus {
    /// To the pane beside it on this side.
    #[serde(default)]
    pub direction: Option<Side>,
    /// To so many panes on (or back, negative), in layout order, wrapping.
    #[serde(default)]
    pub offset: Option<i64>,
    /// To the pane of its view's tab used last.
    #[serde(default)]
    pub last: bool,
}
/// Give a pane its view's focus (its front tab becomes the view's active tab), and make its view
/// and session current; or move from it to another: the one beside it (`direction`), so many
/// on (`offset`), or the one used last (`last`). No id is the caller's tab's pane. Moving out of
/// a zoomed pane unzooms the view; nothing there is not an error.
#[api(
    name = "pane.focus",
    example(args = r#"[":2"]"#, result = "null"),
    example(args = r#"[null, {"direction": "left"}]"#, result = "null", desc = "The pane left of the caller's."),
    example(args = r#"[null, {"offset": 1}]"#, result = "null", desc = "The next pane, in layout order."),
    example(args = r#"[null, {"last": true}]"#, result = "null")
)]
pub fn pumi_pane_focus(cx: &mut Cx, pane: Option<PaneRef>, opts: Option<PaneFocus>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let how = opts.unwrap_or_default();
    let (view, pane) = k.model.pane_ref(&caller, pane)?;
    let v = &k.model.views[&view];
    let from = v.panes[&pane].active;
    let ways = usize::from(how.direction.is_some()) + usize::from(how.offset.is_some()) + usize::from(how.last);
    if ways > 1 {
        return Err(Error::invalid_params("say one of `direction`, `offset` and `last`"));
    }
    let target = if ways == 0 {
        Some(pane)
    } else if how.last {
        v.last.and_then(|t| v.pane_of(t))
    } else if let Some(by) = how.offset {
        let panes = v.pane_order();
        let at = panes.iter().position(|p| *p == pane).unwrap_or(0) as i64;
        panes.get((at + by).rem_euclid(panes.len().max(1) as i64) as usize).copied()
    } else {
        how.direction.and_then(|side| beside(k, view, pane, side))
    };
    let Some(target) = target.filter(|p| ways == 0 || *p != pane) else { return Ok(()) };
    let to = v.panes[&target].active;
    focus_to(cx, from, to)
}
/// What `pane.update` changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanePatch {
    /// Fill its view with it, its tab strip on top (`true`), give the view its layout back
    /// (`false`), or flip it (`"toggle"`).
    #[serde(default)]
    pub zoom: Option<Zoom>,
    /// Grow (or, negative, shrink) it by so many cells from each side given.
    #[serde(default)]
    pub resize: Option<PaneResize>,
}
/// Cells to move a pane's borders by, a side at a time.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PaneResize {
    #[serde(default)]
    pub left: Option<i64>,
    #[serde(default)]
    pub right: Option<i64>,
    #[serde(default)]
    pub up: Option<i64>,
    #[serde(default)]
    pub down: Option<i64>,
}
/// Zoom a pane, or move its borders. No id is the caller's tab's pane.
#[api(
    name = "pane.update",
    example(args = r#"[":2", {"zoom": true}]"#, result = "null"),
    example(args = r#"[null, {"zoom": "toggle"}]"#, result = "null"),
    example(args = r#"[null, {"resize": {"right": 5}}]"#, result = "null", desc = "Five cells wider, to the right.")
)]
pub fn pumi_pane_update(cx: &mut Cx, pane: Option<PaneRef>, patch: PanePatch) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let (view, pane) = k.model.pane_ref(&caller, pane)?;
    let front = k.model.views[&view].panes[&pane].active;
    if let Some(cells) = patch.resize {
        for (side, by) in
            [(Side::Left, cells.left), (Side::Right, cells.right), (Side::Up, cells.up), (Side::Down, cells.down)]
        {
            if let Some(by) = by.filter(|by| *by != 0) {
                resize(cx, front, side, by)?;
            }
        }
    }
    if let Some(zoom) = patch.zoom {
        set_zoom(cx, front, zoom)?;
    }
    Ok(())
}
/// Where `view.focus` moves from its view, in its session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ViewFocus {
    /// To so many views on (or back, negative), wrapping.
    #[serde(default)]
    pub offset: Option<i64>,
    /// To the session's view at this index.
    #[serde(default)]
    pub index: Option<u32>,
    /// To the view used before the session's active one.
    #[serde(default)]
    pub last: bool,
}
/// Make a view its session's active view; or move from it to another of its session by offset
/// (`next-window` is `{"offset": 1}`), by index, or the last one. No id is the caller's view.
#[api(
    name = "view.focus",
    example(args = r#"["@1"]"#, result = "null"),
    example(args = r#"[null, {"offset": 1}]"#, result = "null", desc = "The next view."),
    example(args = r#"[null, {"index": 2}]"#, result = "null"),
    example(args = r#"[null, {"last": true}]"#, result = "null")
)]
pub fn pumi_view_focus(cx: &mut Cx, view: Option<ViewRef>, opts: Option<ViewFocus>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let how = opts.unwrap_or_default();
    let origin = k.model.view_ref(&caller, view)?;
    let ways = usize::from(how.offset.is_some()) + usize::from(how.index.is_some()) + usize::from(how.last);
    if ways == 0 {
        return k.call_core(&caller, "core.setCurrentView", json!([origin])).map(|_| ());
    }
    if ways > 1 {
        return Err(Error::invalid_params("say one of `offset`, `index` and `last`"));
    }
    let current = k.model.current(&caller).map(|c| c.session);
    let session = current
        .filter(|s| k.model.sessions[s].views.iter().any(|(_, x)| *x == origin))
        .or_else(|| k.model.session_of_view(origin))
        .ok_or_else(|| Error::not_found(format!("view @{origin} is in no session")))?;
    let s = &k.model.sessions[&session];
    let target = if how.last {
        s.last.ok_or_else(|| Error::not_found("no last view"))?
    } else if let Some(index) = how.index {
        s.views
            .iter()
            .find(|(i, _)| *i == index)
            .map(|(_, v)| *v)
            .ok_or_else(|| Error::not_found(format!("no view {index}")))?
    } else {
        let offset = how.offset.unwrap_or(0);
        let at = s.views.iter().position(|(_, v)| *v == origin).unwrap_or(0) as i64;
        let n = s.views.len().max(1) as i64;
        s.views
            .get((at + offset).rem_euclid(n) as usize)
            .map(|(_, v)| *v)
            .ok_or_else(|| Error::not_found("no views"))?
    };
    k.call_core(&caller, "core.setCurrentView", json!([target])).map(|_| ())
}
/// Where `session.focus` moves from its session.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionFocus {
    /// To so many sessions on (or back), in the order they were made, wrapping.
    #[serde(default)]
    pub offset: Option<i64>,
    /// To the session used most recently before it.
    #[serde(default)]
    pub last: bool,
}
/// Show a session in the caller's client; or move from it to another by offset, or to the last
/// one (`-n`, `-p`, `-l`). No id is the caller's session.
#[api(
    name = "session.focus",
    example(args = r#"["$1"]"#, result = "null"),
    example(args = r#"[null, {"offset": 1}]"#, result = "null", desc = "The next session."),
    example(args = r#"[null, {"last": true}]"#, result = "null")
)]
pub fn pumi_session_focus(cx: &mut Cx, session: Option<SessionRef>, opts: Option<SessionFocus>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let how = opts.unwrap_or_default();
    let origin = k.model.session_ref(&caller, session)?;
    let target = if how.last {
        if how.offset.is_some() {
            return Err(Error::invalid_params("say `offset` or `last`, not both"));
        }
        k.model
            .sessions
            .values()
            .filter(|s| s.id != origin)
            .max_by_key(|s| s.activity)
            .map(|s| s.id)
            .ok_or_else(|| Error::not_found("no last session"))?
    } else if let Some(offset) = how.offset {
        let ids: Vec<u64> = k.model.sessions.keys().copied().collect();
        let at = ids.iter().position(|s| *s == origin).unwrap_or(0) as i64;
        *ids.get((at + offset).rem_euclid(ids.len().max(1) as i64) as usize)
            .ok_or_else(|| Error::not_found("no sessions"))?
    } else {
        origin
    };
    k.call_core(&caller, "core.setCurrentSession", json!([target])).map(|_| ())
}
/// Grows or shrinks a tiled tab's pane from a side: its neighbour there gives or takes the cells.
pub fn resize(cx: &mut Cx, tab: TabId, side: Side, cells: i64) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let Some((view, pane)) = k.model.pane_of_tab(tab) else {
        return Err(Error::invalid_params(format!("tab %{tab} is not tiled, so it has no border to move")));
    };
    let area = k.view_area(view);
    let mut laid = k.model.views[&view].layout.clone();
    laid.resize(pane, side, cells, area);
    k.call_core(&caller, "core.viewSetLayout", json!([view, laid.to_value()])).map(|_| ())
}
/// `tab.update`'s `zoom`: zoom or unzoom, or flip it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Zoom {
    Set(bool),
    Toggle(ToggleWord),
}
/// The word that says to flip a view's zoom.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ToggleWord {
    Toggle,
}
/// Zooms a tab's pane, or unzooms its view.
pub fn set_zoom(cx: &mut Cx, tab: TabId, zoom: Zoom) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let Some((view, pane)) = k.model.pane_of_tab(tab) else {
        return Err(Error::invalid_params(format!("tab %{tab} is not tiled, so it cannot be zoomed")));
    };
    let v = &k.model.views[&view];
    let zoom = match zoom {
        Zoom::Set(zoom) => zoom,
        Zoom::Toggle(_) => !v.zoomed,
    };
    let zoom = zoom && v.layout.panes().len() >= 2;
    if v.zoomed == zoom && (!zoom || v.active_pane() == Some(pane)) {
        return Ok(());
    }
    if zoom {
        k.call_core(&caller, "core.setCurrentTab", json!([tab]))?;
    }
    k.call_core(&caller, "core.viewSetZoom", json!([view, zoom])).map(|_| ())
}
