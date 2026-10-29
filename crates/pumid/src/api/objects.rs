//! Sessions, views, panes and tabs as the API reads, closes and patches them in the core, and the internal
//! `core.*` functions the host's `focus`, `open` and `update` resolve into. Every id in and out is a
//! prefixed string; a left-out id is the caller's own.
use super::kernel;
use crate::kernel::layout::{Layout, Rect, Side};
use crate::kernel::ops::{self, PlaceSpec, StateOpts};
use crate::kernel::options::Key;
use crate::kernel::{Closed, Kernel, PaneId, Place, SessionId, TabId, ViewId};
use pumi_api::{Cx, Error, Kind, PaneRef, Result, SessionRef, TabRef, ViewRef, api};
use pumi_model::objects::{
    Neighbors, PaneInfo, Process, Scroll, SessionInfo, TabInfo, TabPlace, ViewInfo, ViewTree, neighbour, patch_field,
};
use pumi_model::state::TabState;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// List every session, in creation order.
#[api(name = "session.list", effect = "read", example(args = "[]", result = r#"["$1"]"#))]
pub fn pumi_session_list(cx: &mut Cx) -> Result<Vec<SessionRef>> {
    let (_, k) = kernel(cx)?;
    Ok(k.model.sessions.keys().copied().map(SessionRef).collect())
}
/// What `session.get` adds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionGetOpts {
    /// The whole tree: every view, each with its tabs.
    #[serde(default)]
    pub deep: bool,
}
/// A session: its name, directory, views and active view; with `deep`, the whole tree in one call.
/// No id is the caller's: its tab's session, its client's, or the one used last.
#[api(
    name = "session.get",
    effect = "read",
    example(args = "[]", desc = "The caller's."),
    example(
        args = r#"["$1", {"deep": true}]"#,
        result = r#"{"id": "$1", "name": "work", "cwd": "/src", "views": ["@1"], "activeView": "@1", "tree": []}"#
    )
)]
pub fn pumi_session_get(cx: &mut Cx, session: Option<SessionRef>, opts: Option<SessionGetOpts>) -> Result<SessionInfo> {
    let (caller, k) = kernel(cx)?;
    let id = k.model.session_ref(&caller, session)?;
    let mut info = k.model.session_info(id);
    if opts.unwrap_or_default().deep {
        let mut tree = Vec::new();
        for (_, view) in k.model.sessions[&id].views.clone() {
            let tabs = tabs_of(k, view).into_iter().map(|t| tab_info(k, t)).collect::<Result<_>>()?;
            tree.push(ViewTree { view: k.model.view_info(view)?, tabs });
        }
        info.tree = Some(tree);
    }
    Ok(info)
}
/// A patch to a session: what `session.update` changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionPatch {
    /// Its name; `null` gives it a free one of its own (its folder's, else a number).
    #[serde(default, deserialize_with = "patch_field", skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    /// Its directory, where its terminals start without a `cwd` of their own; `null` has none.
    #[serde(default, deserialize_with = "patch_field", skip_serializing_if = "Option::is_none")]
    pub cwd: Option<Option<String>>,
}
/// Change a session's name or directory. Fires `session.changed` with what changed. No id is the
/// caller's session.
#[api(
    name = "session.update",
    example(args = r#"["$1", {"name": "work"}]"#, result = "null"),
    example(args = r#"["$1", {"cwd": "/src"}]"#, result = "null", desc = "New terminals in it start in /src.")
)]
pub fn pumi_session_update(cx: &mut Cx, session: Option<SessionRef>, patch: SessionPatch) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let session = k.model.session_ref(&caller, session)?;
    let mut changed = Map::new();
    if let Some(cwd) = patch.cwd
        && k.model.sessions[&session].cwd != cwd
    {
        k.model.sessions.get_mut(&session).expect("a session").cwd = cwd.clone();
        changed.insert("cwd".into(), json!(cwd));
    }
    if let Some(name) = patch.name {
        let current = k.model.sessions[&session].name.clone();
        if name.as_ref() != Some(&current) {
            let cwd = k.model.sessions[&session].cwd.clone();
            let name = k.session_name(name, cwd.as_deref())?;
            k.model.sessions.get_mut(&session).expect("a session").name = name.clone();
            changed.insert("name".into(), json!(name));
        }
    }
    k.changed(Kind::Session, session, Value::Object(changed));
    Ok(())
}
/// Close a session and its views and tabs. No id is the caller's.
#[api(name = "session.close", example(args = r#"["$1"]"#, result = "null"))]
pub fn pumi_session_close(cx: &mut Cx, session: Option<SessionRef>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let session = k.model.session_ref(&caller, session)?;
    ops::close_session(cx, session)
}
/// A view: its session, name, index, layout of panes (each with its tabs), active tab and pane,
/// and zoom. No id is the caller's: its session's active view.
#[api(
    name = "view.get",
    effect = "read",
    example(
        args = r#"["@1"]"#,
        result = r#"{"id": "@1", "session": "$1", "name": null, "index": 0, "layout": {"pane": ":1", "tabs": ["%1"], "active": "%1"}, "activeTab": "%1", "activePane": ":1", "zoom": null}"#
    )
)]
pub fn pumi_view_get(cx: &mut Cx, view: Option<ViewRef>) -> Result<ViewInfo> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_ref(&caller, view)?;
    k.model.view_info(view)
}
/// Close a view and every tab in it. No id is the caller's.
#[api(name = "view.close", example(args = r#"["@1"]"#, result = "null"))]
pub fn pumi_view_close(cx: &mut Cx, view: Option<ViewRef>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_ref(&caller, view)?;
    ops::close_view(cx, view)
}
/// Which tabs `tab.list` lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListTabs {
    /// Only this session's.
    #[serde(default)]
    pub session: Option<SessionRef>,
    /// Only this view's.
    #[serde(default)]
    pub view: Option<ViewRef>,
    /// Only tabs that hold this block.
    #[serde(default)]
    pub block: Option<String>,
    /// Only tabs with this trait (`pumi_dev.files.file`): their option `@<trait>` is set.
    #[serde(default, rename = "trait")]
    pub has: Option<String>,
    /// The tabs closed lately instead, newest first, each with its saved state and where it
    /// was. Reopening one is `tab.open(state.block, {state}, {kind: "tiled", pane})` while its
    /// pane is there, else `{kind: "tiled", view, beside}`.
    #[serde(default)]
    pub closed: bool,
}
/// A tab closed lately, to reopen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClosedTab {
    pub tab: TabRef,
    pub state: TabState,
    /// Where it was: its view, its pane, and a tab beside its pane, if they are still there.
    pub view: Option<ViewRef>,
    pub pane: Option<PaneRef>,
    pub beside: Option<TabRef>,
    /// When it closed, in RFC 3339.
    pub at: String,
}
impl ClosedTab {
    fn of(closed: &Closed) -> ClosedTab {
        ClosedTab {
            tab: TabRef(closed.tab),
            state: closed.state.clone(),
            view: closed.view.map(ViewRef),
            pane: closed.pane.map(PaneRef),
            beside: closed.beside.map(TabRef),
            at: closed.at.clone(),
        }
    }
}
/// `tab.list`'s answer: tab ids, or with `closed` the tabs closed lately.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum TabList {
    Tabs(Vec<TabRef>),
    Closed(Vec<ClosedTab>),
}
/// A view's tabs: its panes' in layout order, then its floats.
fn tabs_of(k: &Kernel, view: ViewId) -> Vec<TabId> {
    let mut tabs = k.model.views.get(&view).map(|v| v.tabs()).unwrap_or_default();
    tabs.extend(
        k.model.tabs.values().filter(|t| matches!(&t.place, Place::Float { view: v, .. } if *v == view)).map(|t| t.id),
    );
    tabs
}
/// List tabs, every one or those `opts` asks for: pane by pane in layout order within a view,
/// then floats and hidden tabs. `closed` lists the tabs closed lately instead.
#[api(
    name = "tab.list",
    effect = "read",
    example(args = "[{}]", result = r#"["%1"]"#, desc = "Every tab."),
    example(args = r#"[{"block": "pumi_dev.terminal.shell"}]"#, result = r#"["%1"]"#, desc = "Only the Terminal plugin's shells."),
    example(args = r#"[{"trait": "pumi_dev.files.file"}]"#, result = r#"["%3"]"#, desc = "Only files."),
    example(args = r#"[{"closed": true}]"#, result = "[]", desc = "The tabs closed lately, to reopen one.")
)]
pub fn pumi_tab_list(cx: &mut Cx, opts: Option<ListTabs>) -> Result<TabList> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    if opts.closed {
        return Ok(TabList::Closed(k.closed.iter().map(ClosedTab::of).collect()));
    }
    let session = opts.session.map(|s| k.model.session_ref(&caller, Some(s))).transpose()?;
    let view = opts.view.map(|v| k.model.view_ref(&caller, Some(v))).transpose()?;
    let mut ordered: Vec<TabId> = Vec::new();
    for v in k.model.views.keys() {
        ordered.extend(tabs_of(k, *v));
    }
    ordered.extend(k.model.tabs.keys().filter(|t| !ordered.contains(t)).copied().collect::<Vec<_>>());
    Ok(TabList::Tabs(
        ordered
            .into_iter()
            .filter(|id| {
                let Some(t) = k.model.tabs.get(id) else { return false };
                let in_view = t.place.view();
                opts.block.as_ref().is_none_or(|b| &t.block == b)
                    && opts.has.as_ref().is_none_or(|t| super::traits::has(k, *id, t))
                    && view.is_none_or(|view| in_view == Some(view))
                    && session.is_none_or(|s| {
                        in_view.is_some_and(|t| k.model.sessions[&s].views.iter().any(|(_, x)| *x == t))
                    })
            })
            .map(TabRef)
            .collect(),
    ))
}
/// How `tab.get`'s `state` saves the tab's state.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TabStateOpts {
    /// What the state holds beyond its options and type data, instead of the `viewoptions`
    /// option: `"scrollback,history"`.
    #[serde(default)]
    pub view_options: Option<String>,
    /// `restore` (the default), `copy`, `move` or `share`: a type leaves out what should not travel.
    #[serde(default)]
    #[schemars(schema_with = "purpose_schema")]
    pub purpose: Option<String>,
    /// `ref` keeps blobs on this server (the default); `inline` puts their bytes in the state.
    #[serde(default)]
    #[schemars(schema_with = "blobs_schema")]
    pub blobs: Option<String>,
}
/// The purposes `ops::get_state` saves for.
fn purpose_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "anyOf": [{ "type": "string", "enum": ["restore", "copy", "move", "share"] }, { "type": "null" }] })
}
/// How `ops::get_state` keeps blobs.
fn blobs_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({ "anyOf": [{ "type": "string", "enum": ["ref", "inline"] }, { "type": "null" }] })
}
/// `tab.get`'s `state`: whether to add the saved state, or how to save it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum TabStateWant {
    Add(bool),
    As(TabStateOpts),
}
/// What `tab.get` adds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TabGetOpts {
    /// Its saved state, as persistence saves it (`true`), or saved as these say.
    #[serde(default)]
    pub state: Option<TabStateWant>,
}
/// A tab: its block, traits, place, title, directories, process, neighbours, focus, zoom and
/// options; with `state`, its saved state. No id is the caller's: `PUMI_TAB` for a program in a
/// tab, else its view's active tab.
#[api(
    name = "tab.get",
    effect = "read",
    example(args = "[]", desc = "The caller's."),
    example(args = r#"["%3", {"state": true}]"#, desc = "With its saved state, to reopen it elsewhere."),
    example(
        args = r#"["%3", {"state": {"viewOptions": "", "purpose": "copy"}}]"#,
        desc = "With options and type data only, to duplicate it."
    )
)]
pub fn pumi_tab_get(cx: &mut Cx, tab: Option<TabRef>, opts: Option<TabGetOpts>) -> Result<TabInfo> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_ref(&caller, tab)?;
    let mut info = tab_info(k, tab)?;
    let save = match opts.unwrap_or_default().state {
        None | Some(TabStateWant::Add(false)) => None,
        Some(TabStateWant::Add(true)) => Some(StateOpts::default()),
        Some(TabStateWant::As(o)) => {
            Some(StateOpts { viewoptions: o.view_options, purpose: o.purpose, blobs: o.blobs })
        }
    };
    if let Some(save) = save {
        info.state = Some(ops::get_state(cx, tab, &save)?);
    }
    Ok(info)
}
/// Close a tab. Its state is kept, so it can be reopened (`tab.list({closed: true})`). No id is
/// the caller's.
#[api(name = "tab.close", example(args = r#"["%3"]"#, result = "null"))]
pub fn pumi_tab_close(cx: &mut Cx, tab: Option<TabRef>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_ref(&caller, tab)?;
    ops::close_tab(cx, tab)
}
/// A tab as `tab.get` answers it, without its state.
pub fn tab_info(k: &Kernel, tab: TabId) -> Result<TabInfo> {
    let t = k.model.tabs.get(&tab).ok_or_else(|| Error::not_found(format!("no tab %{tab}")))?;
    let text = |name: &str| k.tab_option(tab, name).as_str().filter(|s| !s.is_empty()).map(str::to_string);
    let number = |name: &str| k.tab_option(tab, name).as_u64().map(|n| n as u32);
    let view = t.place.view();
    let session =
        k.model.session_of_tab(tab).ok_or_else(|| Error::not_found(format!("tab %{tab} is in no session")))?;
    let foreground = number("tab_foreground_pgid");
    let process = foreground
        .or_else(|| number("tab_pid"))
        .map(|pid| Process { pid, command: text("tab_current_command").into_iter().collect() });
    let pane = k.model.pane_of_tab(tab).map(|(_, p)| p);
    let zoomed =
        view.and_then(|v| k.model.views.get(&v)).is_some_and(|v| v.zoomed && pane.is_some() && v.active_pane() == pane);
    Ok(TabInfo {
        id: TabRef(tab),
        session: SessionRef(session),
        view: view.map(ViewRef),
        block: t.block.clone(),
        traits: traits(k, tab),
        place: TabPlace::of(&t.place, pane),
        title: text("title"),
        cwd: text("tab_current_path"),
        foreground_cwd: foreground.and_then(pumi_sys::process::cwd),
        process,
        scroll: Scroll::default(),
        neighbors: neighbors(k, tab),
        is_focused: k.model.is_focused(tab),
        is_zoomed: zoomed,
        options: k.local_options(Key::Tab(tab)),
        state: None,
    })
}
/// Each trait a tab has, with its fields (`@<trait>.<field>`).
fn traits(k: &Kernel, tab: TabId) -> BTreeMap<String, Value> {
    let local =
        |name: &str| k.model.options.get_local(name, Key::Tab(tab)).map_or(Value::Null, |set| set.value.clone());
    let mut out = BTreeMap::new();
    for t in super::traits::core().into_iter().chain(k.plugin_traits.iter().cloned()) {
        if !super::traits::has(k, tab, &t.id) {
            continue;
        }
        let fields: Map<String, Value> =
            t.fields.iter().map(|f| (f.clone(), local(&format!("@{}.{f}", t.id)))).collect();
        out.insert(t.id, Value::Object(fields));
    }
    out
}
/// The tabs in front of the panes beside a tiled tab's pane, laid out in its view's size
/// (`default_size` until a client sizes it).
fn neighbors(k: &Kernel, tab: TabId) -> Neighbors {
    let Some((view, pane)) = k.model.pane_of_tab(tab) else { return Neighbors::default() };
    let (width, height) = k.default_size();
    let v = &k.model.views[&view];
    let rects = v.layout.rects(Rect { x: 0, y: 0, width, height });
    let front = |p: PaneId| v.panes.get(&p).map(|p| p.active);
    let activity = |p: PaneId| front(p).and_then(|t| k.model.tabs.get(&t)).map_or(0, |t| t.activity);
    let side = |side| neighbour(&rects, pane, side, activity).and_then(front).map(TabRef);
    Neighbors { left: side(Side::Left), right: side(Side::Right), up: side(Side::Up), down: side(Side::Down) }
}
/// Which panes `pane.list` lists.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPanes {
    /// Only this view's.
    #[serde(default)]
    pub view: Option<ViewRef>,
}
/// List panes: a view's, or every view's, in layout order.
#[api(
    name = "pane.list",
    effect = "read",
    example(args = "[{}]", result = r#"[":1"]"#, desc = "Every pane."),
    example(args = r#"[{"view": "@1"}]"#, result = r#"[":1", ":2"]"#, desc = "A view's.")
)]
pub fn pumi_pane_list(cx: &mut Cx, opts: Option<ListPanes>) -> Result<Vec<PaneRef>> {
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    let only = opts.view.map(|v| k.model.view_ref(&caller, Some(v))).transpose()?;
    Ok(k.model
        .views
        .values()
        .filter(|v| only.is_none_or(|only| v.id == only))
        .flat_map(|v| v.pane_order())
        .map(PaneRef)
        .collect())
}
/// A pane: its view, its tabs in its strip's order, the one in front, and whether it has the
/// focus or is zoomed. No id is the caller's tab's pane.
#[api(
    name = "pane.get",
    effect = "read",
    example(
        args = r#"[":1"]"#,
        result = r#"{"id": ":1", "view": "@1", "tabs": ["%1", "%2"], "activeTab": "%1", "isFocused": true, "isZoomed": false}"#
    )
)]
pub fn pumi_pane_get(cx: &mut Cx, pane: Option<PaneRef>) -> Result<PaneInfo> {
    let (caller, k) = kernel(cx)?;
    let (view, pane) = k.model.pane_ref(&caller, pane)?;
    k.model.pane_info(view, pane)
}
/// Close a pane and every tab in it. No id is the caller's tab's pane.
#[api(name = "pane.close", example(args = r#"[":1"]"#, result = "null"))]
pub fn pumi_pane_close(cx: &mut Cx, pane: Option<PaneRef>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let (view, pane) = k.model.pane_ref(&caller, pane)?;
    let tabs = k.model.views[&view].panes[&pane].tabs.clone();
    for tab in tabs {
        if cx.host::<Kernel>()?.model.tabs.contains_key(&tab) {
            ops::close_tab(cx, tab)?;
        }
    }
    Ok(())
}
/// Make a session current: the caller's client shows it (`session.focus`).
#[api(name = "core.setCurrentSession", internal, example(args = "[1]"))]
pub fn pumi_core_set_current_session(cx: &mut Cx, session: u64) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let session = k.model.session_id(&caller, session)?;
    let mut entered = false;
    if let Some(ui) = k.model.chans.get_mut(&caller.channel).and_then(|c| c.ui.as_mut())
        && ui.session != session
    {
        ui.session = session;
        entered = true;
    }
    k.touch_session(session);
    if entered {
        k.client_changed(caller.channel, session);
    }
    Ok(())
}
/// Make a view its session's current view (`view.focus`).
#[api(name = "core.setCurrentView", internal, example(args = "[1]"))]
pub fn pumi_core_set_current_view(cx: &mut Cx, view: u64) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_id(&caller, view)?;
    let session = k
        .model
        .current(&caller)
        .map(|c| c.session)
        .filter(|s| k.model.sessions[s].views.iter().any(|(_, t)| *t == view))
        .or_else(|| k.model.session_of_view(view))
        .ok_or_else(|| Error::not_found(format!("view @{view} is in no session")))?;
    ops::set_current_view_in(k, session, view);
    Ok(())
}
/// Make a tab its pane's front tab and its view's active tab, and its view current (`tab.focus`).
#[api(name = "core.setCurrentTab", internal, example(args = "[1]"))]
pub fn pumi_core_set_current_tab(cx: &mut Cx, tab: u64) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_id(&caller, tab)?;
    match &k.model.tabs[&tab].place {
        Place::Tiled { .. } => {}
        Place::Float { float, .. } if float.focusable => {}
        _ => return Err(Error::invalid_params(format!("tab %{tab} is not shown in a view, so it cannot be current"))),
    }
    ops::set_current_tab_in(k, tab);
    Ok(())
}
/// `tab.open` as the core does it, with its place resolved (the host's `tab.open`).
#[api(name = "core.openTab", internal, example(args = r#"["pumi_dev.terminal.shell", {}, {"kind": "session"}]"#, result = "2"))]
pub fn pumi_core_open_tab(cx: &mut Cx, block: String, args: Option<Value>, place: Option<PlaceSpec>) -> Result<u64> {
    ops::open_tab(cx, &block, args.unwrap_or(Value::Null), place.unwrap_or_default())
}
/// `tab.update`'s `place` as the core does it.
#[api(name = "core.tabSetPlace", internal, example(args = r#"[1, {"kind": "view", "session": 1}]"#))]
pub fn pumi_core_tab_set_place(cx: &mut Cx, tab: u64, place: PlaceSpec) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_id(&caller, tab)?;
    ops::set_place(cx, tab, place)
}
/// `tab.update`'s `title` as the core does it: the tab's `title`; `null` leaves it to what its
/// program says next.
#[api(name = "core.tabSetTitle", internal, example(args = r#"[1, "tests"]"#))]
pub fn pumi_core_tab_set_title(cx: &mut Cx, tab: u64, title: Option<String>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_id(&caller, tab)?;
    let source = k.source(&caller);
    let before = k.tab_option(tab, "title");
    match &title {
        Some(title) => {
            k.set_option("title", Key::Tab(tab), json!(title), &source, false)?;
        }
        None => k.unset_option("title", Key::Tab(tab), &source),
    }
    let after = k.tab_option(tab, "title");
    if after != before {
        let title = after.as_str().filter(|s| !s.is_empty()).map(str::to_string);
        k.changed(Kind::Tab, tab, json!({ "title": title }));
    }
    Ok(())
}
/// `view.update`'s `name` as the core does it; `null` gives it back its automatic name.
#[api(name = "core.viewSetName", internal, example(args = r#"[1, "logs"]"#))]
pub fn pumi_core_view_set_name(cx: &mut Cx, view: u64, name: Option<String>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_id(&caller, view)?;
    if k.model.views[&view].name == name {
        return Ok(());
    }
    k.model.views.get_mut(&view).expect("a view").name = name.clone();
    k.changed(Kind::View, view, json!({ "name": name }));
    Ok(())
}
/// `view.update`'s `layout` as the core does it: a tree in the core's shape
/// (`["row", [["leaf", 1, 0.5], …]]`), holding the view's panes each once by number; shares left
/// out are split evenly (the host works out presets). The panes keep their tabs.
#[api(name = "core.viewSetLayout", internal, example(args = r#"[1, ["leaf", 1]]"#))]
pub fn pumi_core_view_set_layout(cx: &mut Cx, view: u64, layout: Value) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_id(&caller, view)?;
    let layout = Layout::from_value(&layout)?;
    let mut want = layout.panes();
    let mut have = k.model.views[&view].layout.panes();
    want.sort_unstable();
    have.sort_unstable();
    if want != have {
        return Err(Error::invalid_params(format!("the layout must hold the view's panes {have:?}, not {want:?}")));
    }
    k.model.views.get_mut(&view).expect("a view").layout = layout;
    k.layout_changed(view);
    Ok(())
}
/// One session's views as `core.viewSetPlace` sets them: every `[index, view]`, and which
/// is current and last.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionViews {
    pub session: u64,
    pub views: Vec<(u32, u64)>,
    #[serde(default)]
    pub current: Option<u64>,
    #[serde(default)]
    pub last: Option<u64>,
}
/// Where each view of these sessions is: its session and index there.
fn positions(k: &Kernel, sessions: &[SessionId]) -> BTreeMap<ViewId, (SessionId, u32)> {
    let mut out = BTreeMap::new();
    for s in sessions {
        for (index, view) in k.model.sessions.get(s).map(|s| s.views.clone()).unwrap_or_default() {
            if k.model.session_of_view(view) == Some(*s) {
                out.insert(view, (*s, index));
            }
        }
    }
    out
}
/// `view.update`'s `index` and `session` as the core does it: the sessions it changed, each with
/// its views as they are now (the host works out moving).
#[api(
    name = "core.viewSetPlace",
    internal,
    example(args = r#"[1, [{"session": 1, "views": [[3, 1]], "current": 1}]]"#)
)]
pub fn pumi_core_view_set_place(cx: &mut Cx, view: u64, sessions: Vec<SessionViews>) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    k.model.view_id(&caller, view)?;
    for s in &sessions {
        let session =
            k.model.sessions.get(&s.session).ok_or_else(|| Error::not_found(format!("no session ${}", s.session)))?;
        let mut indices: Vec<u32> = s.views.iter().map(|(i, _)| *i).collect();
        indices.sort_unstable();
        indices.dedup();
        if indices.len() != s.views.len() {
            return Err(Error::invalid_params(format!("session ${} has an index twice", session.id)));
        }
        if let Some((_, t)) = s.views.iter().find(|(_, t)| !k.model.views.contains_key(t)) {
            return Err(Error::not_found(format!("no view @{t}")));
        }
    }
    let ids: Vec<SessionId> = sessions.iter().map(|s| s.session).collect();
    let before = positions(k, &ids);
    let current: Vec<Option<ViewId>> = ids.iter().map(|s| k.model.sessions[s].current).collect();
    for s in &sessions {
        let session = k.model.sessions.get_mut(&s.session).expect("checked");
        session.views = s.views.clone();
        session.views.sort_by_key(|(i, _)| *i);
        let holds = |t: &u64| s.views.iter().any(|(_, x)| x == t);
        session.current = s.current.filter(holds).or_else(|| session.views.first().map(|(_, t)| *t));
        session.last = s.last.filter(holds);
    }
    let after = positions(k, &ids);
    for (view, (session, index)) in &after {
        let mut patch = Map::new();
        let was = before.get(view);
        if was.map(|(s, _)| s) != Some(session) {
            patch.insert("session".into(), json!(SessionRef(*session)));
        }
        if was.map(|(_, i)| i) != Some(index) {
            patch.insert("index".into(), json!(index));
        }
        k.changed(Kind::View, *view, Value::Object(patch));
    }
    for (session, was) in ids.iter().zip(current) {
        if k.model.sessions.get(session).is_some_and(|s| s.current != was && s.current.is_some()) {
            k.active_view_changed(*session);
        }
    }
    k.prune();
    Ok(())
}
/// `pane.update`'s and `tab.update`'s `zoom` as the core does it: the view's flag, its active
/// pane filling it with its tab strip (the host checks the pane and makes it current first).
#[api(name = "core.viewSetZoom", internal, example(args = "[1, false]"))]
pub fn pumi_core_view_set_zoom(cx: &mut Cx, view: u64, zoom: bool) -> Result<()> {
    let (caller, k) = kernel(cx)?;
    let view = k.model.view_id(&caller, view)?;
    if zoom && k.model.views[&view].layout.panes().len() < 2 {
        return Err(Error::invalid_params(format!("view @{view} has one pane, so nothing zooms")));
    }
    if k.model.views[&view].zoomed == zoom {
        return Ok(());
    }
    let v = k.model.views.get_mut(&view).expect("a view");
    v.zoomed = zoom;
    let zoomed = zoom.then(|| v.active_pane().map(PaneRef)).flatten();
    k.changed(Kind::View, view, json!({ "zoom": zoomed }));
    Ok(())
}
