use super::events::About;
use super::layout::{Dim, PaneId, Side, Size};
pub use super::layout::{PlaceKind, PlaceSpec};
use super::options::Key;
use super::state::{Origin, STATE_VERSION, TabState, now_rfc3339};
use super::types::{Restore, SaveOpts};
use super::{CLOSED_LIMIT, Closed, Float, Kernel, Place, Session, SessionId, Tab, TabId, View, ViewId};
use pumi_api::{Cx, Error, Kind, PaneRef, Ref, Result, SessionRef, TabRef, ViewRef};
use pumi_model::objects::TabPlace;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
/// A place the core can make: what the caller said, checked against the model.
enum Target {
    /// A new pane, split from `beside` (or the whole view, `full`).
    Split {
        view: ViewId,
        beside: PaneId,
        side: Side,
        share: Option<f64>,
        full: bool,
    },
    /// Into a pane, as one of its tabs.
    Join {
        view: ViewId,
        pane: PaneId,
        index: Option<usize>,
    },
    NewView {
        session: SessionId,
        index: Option<u32>,
        name: Option<String>,
    },
    NewSession {
        name: Option<String>,
        cwd: Option<String>,
    },
    Float {
        view: ViewId,
        float: Float,
    },
}
/// What a place says, with nothing left to choose: the host resolves where a tab goes (beside
/// what, which view, a float's cells, a split's share); the core only checks and makes it.
fn target(cx: &mut Cx, spec: &PlaceSpec, moving: Option<TabId>) -> Result<Target> {
    let caller = cx.caller().clone();
    let kernel = cx.host::<Kernel>()?;
    let needs =
        |what: &str| Error::invalid_params(format!("the core is given every place resolved: `{what}` is missing"));
    let tab = |id: Option<TabId>| id.map(|p| kernel.model.tab_id(&caller, p)).transpose();
    match spec.kind.unwrap_or(PlaceKind::Tiled) {
        PlaceKind::Tiled => {
            let full = spec.full.unwrap_or(false);
            // The pane it goes into, or beside: the one named, else the one `beside` is in.
            let (view, pane) = match spec.pane {
                Some(pane) => kernel.model.pane_id(&caller, pane)?,
                None => {
                    let beside = tab(spec.beside)?.ok_or_else(|| needs("beside or pane"))?;
                    kernel.model.pane_of_tab(beside).ok_or_else(|| {
                        Error::invalid_params(format!("tab %{beside} is not tiled, so nothing can split its pane"))
                    })?
                }
            };
            let own = moving.and_then(|t| kernel.model.pane_of_tab(t)) == Some((view, pane));
            if spec.pane.is_some() && spec.split.is_none() && !full {
                return Ok(Target::Join { view, pane, index: spec.index.map(|i| i as usize) });
            }
            // Its own pane goes as it leaves it: there is nothing left to split, unless the whole
            // view splits and holds another pane.
            let v = &kernel.model.views[&view];
            if own && v.panes[&pane].tabs.len() == 1 && (!full || v.layout.panes().len() == 1) {
                return Err(Error::invalid_params(format!("pane :{pane} holds only the tab, so it cannot split it")));
            }
            let share = match spec.size.as_ref().map(Size::parse).transpose()? {
                None => None,
                Some(Size::Share(share)) => Some(share),
                Some(Size::Cells(_)) => return Err(needs("size as a share")),
            };
            Ok(Target::Split { view, beside: pane, side: spec.split.unwrap_or(Side::Down), share, full })
        }
        PlaceKind::View => {
            let session = kernel.model.session_id(&caller, spec.session.ok_or_else(|| needs("session"))?)?;
            if let Some(index) = spec.index
                && kernel.model.sessions[&session].views.iter().any(|(i, _)| *i == index)
            {
                return Err(Error::invalid_params(format!("index {index} in use")));
            }
            Ok(Target::NewView { session, index: spec.index, name: spec.name.clone() })
        }
        PlaceKind::Session => Ok(Target::NewSession { name: spec.name.clone(), cwd: spec.cwd.clone() }),
        PlaceKind::Float => {
            let view = kernel.model.view_id(&caller, spec.view.ok_or_else(|| needs("view"))?)?;
            let dim = |value: &Option<Value>, what: &str| match value.as_ref() {
                Some(value) => Dim::parse(value),
                None => Err(needs(what)),
            };
            let anchor = spec.anchor.clone().unwrap_or_else(|| "NW".into());
            if pumi_layout::Anchor::parse(&anchor).is_none() {
                let names = pumi_layout::Anchor::NAMES.join(", ");
                return Err(Error::invalid_params(format!("a float's anchor is one of {names}: got {anchor:?}")));
            }
            let float = Float {
                relative: spec.relative.clone().unwrap_or_else(|| "view".into()),
                tab: spec.beside,
                anchor,
                row: dim(&spec.row, "row")?,
                col: dim(&spec.col, "col")?,
                width: dim(&spec.width, "width")?,
                height: dim(&spec.height, "height")?,
                border: spec.border.clone(),
                focusable: spec.focusable.unwrap_or(true),
                mouse: spec.mouse,
                dismiss: spec.dismiss.clone().unwrap_or_default(),
                backdrop: spec.backdrop.map(|b| b.min(100)),
                zindex: spec.zindex.unwrap_or(50),
            };
            Ok(Target::Float { view, float })
        }
    }
}
/// `args.state`, taken out of `args`: a saved state to restore.
fn take_state(args: &mut Value) -> Result<Option<TabState>> {
    match args.as_object_mut().and_then(|a| a.remove("state")) {
        Some(state) => {
            Ok(Some(serde_json::from_value(state).map_err(|err| Error::invalid_params(format!("state: {err}")))?))
        }
        None => Ok(None),
    }
}
/// The bytes of a state's blobs, by name.
fn state_blobs(kernel: &Kernel, state: Option<&TabState>) -> Result<BTreeMap<String, Vec<u8>>> {
    match state {
        Some(state) => state.blobs.iter().map(|(name, blob)| Ok((name.clone(), kernel.blobs.get(blob)?))).collect(),
        None => Ok(BTreeMap::new()),
    }
}
/// Opens a tab of `ty` where `place` says, new from `args` or restored from `args.state`; or,
/// with `place.replace`, in that tab instead of its block ([`replace_block`]).
pub fn open_tab(cx: &mut Cx, ty: &str, mut args: Value, place: PlaceSpec) -> Result<TabId> {
    let ty = ty.to_string();
    cx.host::<Kernel>()?.block(&ty)?;
    let state = take_state(&mut args)?;
    if let Some(tab) = place.replace {
        if place != (PlaceSpec { replace: Some(tab), ..PlaceSpec::default() }) {
            return Err(Error::invalid_params("`replace` is the place; say it alone"));
        }
        let caller = cx.caller().clone();
        let tab = cx.host::<Kernel>()?.model.tab_id(&caller, tab)?;
        return replace_block(cx, tab, &ty, args, state);
    }
    let target = target(cx, &place, None)?;
    let kernel = cx.host::<Kernel>()?;
    let tab_type = kernel.block(&ty)?;
    if let Some(state) = &state {
        state.check(&ty, tab_type.version())?;
    }
    let restore_blobs = state_blobs(kernel, state.as_ref())?;
    let tab = kernel.new_tab_id();
    let focus = place.focus.unwrap_or(true);
    let (place, created) = build(kernel, tab, target, &args, focus)?;
    kernel.model.tabs.insert(tab, Tab { id: tab, block: ty.clone(), place: place.clone(), activity: 0 });
    kernel.touch_tab(tab);
    kernel.set_option("tab_block", Key::Tab(tab), json!(ty), "server", true)?;
    if let Some(state) = &state {
        for (name, value) in &state.options {
            if !kernel.model.options.is_readonly(name) {
                let _ = kernel.set_option(name, Key::Tab(tab), value.clone(), "state", false);
            }
        }
    }
    let restore = state.as_ref().map(|s| Restore { data: &s.data, blobs: restore_blobs, options: &s.options });
    let opened = tab_type.open(kernel, tab, &args, restore);
    let instance = match opened {
        Ok(instance) => instance,
        Err(err) => {
            unplace(kernel, tab);
            kernel.model.tabs.remove(&tab);
            kernel.model.options.drop_object(Key::Tab(tab));
            kernel.prune();
            return Err(err);
        }
    };
    kernel.set_instance(tab, instance);
    announce(kernel, tab, &created, state.is_some());
    if focus {
        match &place {
            Place::Tiled { .. } => set_current_tab_in(kernel, tab),
            Place::Float { float, .. } if float.focusable => set_current_tab_in(kernel, tab),
            _ => {}
        }
    }
    Ok(tab)
}
/// Puts block `ty` in `tab` instead of the one it holds: the old block's engine closes and the
/// new one opens, from `args` or restored from `state`, in the same tab, which keeps its id,
/// place, focus and options. It fires `tab.changed {block}`, not `tab.closed` and `tab.opened`.
/// A state of the block the tab holds is restored into its engine as it runs (Neovim's
/// `:loadview`), and fires the same.
fn replace_block(cx: &mut Cx, tab: TabId, ty: &str, args: Value, state: Option<TabState>) -> Result<TabId> {
    let kernel = cx.host::<Kernel>()?;
    let from = kernel.model.tabs[&tab].block.clone();
    let tab_type = kernel.block(ty)?;
    if let Some(state) = &state {
        state.check(ty, tab_type.version())?;
    }
    let blobs = state_blobs(kernel, state.as_ref())?;
    if from == ty
        && let Some(state) = state
    {
        kernel.with_instance(tab, |instance, kernel| {
            instance.deserialize(kernel, tab, Restore { data: &state.data, blobs, options: &state.options })
        })?;
        for (name, value) in &state.options {
            if !kernel.model.options.is_readonly(name) {
                kernel.set_option(name, Key::Tab(tab), value.clone(), "state", false)?;
            }
        }
        kernel.changed(Kind::Tab, tab, json!({ "block": ty }));
        return Ok(tab);
    }
    if let Some(mut instance) = kernel.take_instance(tab) {
        instance.close(kernel, tab);
    }
    kernel.model.tabs.get_mut(&tab).expect("the tab").block = ty.to_string();
    kernel.set_option("tab_block", Key::Tab(tab), json!(ty), "server", true)?;
    if let Some(state) = &state {
        for (name, value) in &state.options {
            if !kernel.model.options.is_readonly(name) {
                let _ = kernel.set_option(name, Key::Tab(tab), value.clone(), "state", false);
            }
        }
    }
    let restore = state.as_ref().map(|s| Restore { data: &s.data, blobs, options: &s.options });
    match tab_type.open(kernel, tab, &args, restore) {
        Ok(instance) => kernel.set_instance(tab, instance),
        Err(err) => {
            // The old engine is gone: the tab closes rather than stay empty.
            crate::server::log(&format!("%{tab}: {ty} did not open in place of {from}: {}", err.message));
            close_tab_with(cx, tab, false)?;
            return Err(err);
        }
    }
    kernel.changed(Kind::Tab, tab, json!({ "block": ty }));
    Ok(tab)
}
/// Where a block in `tab` starts when it is given no directory: its view's `worktree`, else the
/// folder of the session its view joins, so a session stays in the folder it was opened at (tmux's
/// `session_path`).
pub fn default_cwd(kernel: &Kernel, tab: TabId) -> Option<String> {
    let view = kernel.model.tabs.get(&tab)?.place.view()?;
    if let Some(set) = kernel.model.options.get_local("worktree", Key::View(view))
        && let Some(dir) = set.value.as_str().filter(|dir| !dir.is_empty())
    {
        return Some(dir.to_string());
    }
    let session = kernel.model.session_of_view(view)?;
    kernel.model.sessions.get(&session)?.cwd.clone().filter(|cwd| !cwd.is_empty())
}
/// What a new tab made besides itself, to announce in order.
#[derive(Default)]
struct Created {
    session: Option<SessionId>,
    view: Option<ViewId>,
}
/// Puts a new tab (or a moving one) where `target` says, making the pane, view or session it
/// needs. A tab put into a pane comes to its front when `front`.
fn build(kernel: &mut Kernel, tab: TabId, target: Target, args: &Value, front: bool) -> Result<(Place, Created)> {
    let mut created = Created::default();
    let place = match target {
        Target::Split { view, beside, side, share, full } => {
            let pane = kernel.new_pane_id();
            let v = kernel.model.views.get_mut(&view).expect("a resolved view");
            v.split(beside, pane, tab, side, share.unwrap_or(0.5), full)?;
            if v.zoomed {
                v.zoomed = false;
                kernel.changed(Kind::View, view, json!({ "zoom": null }));
            }
            Place::Tiled { view }
        }
        Target::Join { view, pane, index } => {
            let v = kernel.model.views.get_mut(&view).expect("a resolved view");
            v.insert(pane, tab, index, front)?;
            Place::Tiled { view }
        }
        Target::NewView { session, index, name } => {
            let view = new_view(kernel, session, index, name, tab);
            created.view = Some(view);
            Place::Tiled { view }
        }
        Target::NewSession { name, cwd } => {
            let cwd = cwd.or_else(|| args.get("cwd").and_then(Value::as_str).map(str::to_string));
            let name = kernel.session_name(name, cwd.as_deref())?;
            let id = kernel.new_session_id();
            kernel.model.sessions.insert(
                id,
                Session {
                    id,
                    name,
                    cwd,
                    views: Vec::new(),
                    current: None,
                    last: None,
                    activity: 0,
                    created: super::state::epoch(),
                },
            );
            kernel.touch_session(id);
            let view = new_view(kernel, id, None, None, tab);
            kernel.adopt_clients(id);
            created.session = Some(id);
            created.view = Some(view);
            Place::Tiled { view }
        }
        Target::Float { view, float } => Place::Float { view, float },
    };
    Ok((place, created))
}
fn new_view(kernel: &mut Kernel, session: SessionId, index: Option<u32>, name: Option<String>, tab: TabId) -> ViewId {
    let view = kernel.new_view_id();
    let pane = kernel.new_pane_id();
    kernel.model.views.insert(view, View::new(view, name, pane, tab));
    let index = index.unwrap_or_else(|| kernel.free_index(session));
    let s = kernel.model.sessions.get_mut(&session).expect("a resolved session");
    s.views.push((index, view));
    s.views.sort_by_key(|(i, _)| *i);
    if s.current.is_none() {
        s.current = Some(view);
    }
    view
}
/// Fires `session.opened` and `view.opened` for what a tab's place made.
fn announce_created(kernel: &mut Kernel, created: &Created) {
    if let Some(session) = created.session {
        let s = &kernel.model.sessions[&session];
        let id = Ref::local(Kind::Session, session).to_string();
        let data = json!({ "session": id, "name": s.name, "activeView": s.current.map(ViewRef) });
        kernel.emit("session.opened", &id, About::session(session), data);
    }
    if let Some(view) = created.view {
        let session = kernel.model.session_of_view(view).expect("a new view is in a session");
        let id = Ref::local(Kind::View, view).to_string();
        let data = json!({ "view": id, "session": SessionRef(session) });
        kernel.emit("view.opened", &id, About { session: Some(session), view: Some(view), tab: None }, data);
        kernel.panes_changed(view);
    }
}
/// Fires what a new tab made, `tab.opened` (`restored` when it opened from a saved state), and
/// its view's new layout.
fn announce(kernel: &mut Kernel, tab: TabId, created: &Created, restored: bool) {
    announce_created(kernel, created);
    let ty = kernel.model.tabs[&tab].block.clone();
    let data = json!({ "tab": TabRef(tab), "block": ty, "restored": restored });
    kernel.emit("tab.opened", &ty, About::tab(tab), data);
    if created.view.is_none()
        && let Place::Tiled { view } = kernel.model.tabs[&tab].place
    {
        kernel.layout_changed(view);
    }
}
/// A view's active tab, its pane, and whether it is zoomed, to say what of them changed.
type Focus = (ViewId, TabId, Option<PaneId>, bool);
fn view_focus(kernel: &Kernel, view: Option<ViewId>) -> Option<Focus> {
    let view = view?;
    kernel.model.views.get(&view).map(|v| (view, v.active, v.active_pane(), v.zoomed))
}
/// Fires `view.changed` for what moved in a view's focus since `before` ([`view_focus`]).
fn focus_changed(kernel: &mut Kernel, before: Option<Focus>) {
    let Some((view, active, pane, zoomed)) = before else { return };
    let Some(v) = kernel.model.views.get(&view) else { return };
    let now = v.active_pane();
    let mut patch = Map::new();
    if v.active != active {
        patch.insert("activeTab".into(), json!(TabRef(v.active)));
    }
    if now != pane {
        patch.insert("activePane".into(), json!(now.map(PaneRef)));
    }
    if v.zoomed != zoomed || (v.zoomed && now != pane) {
        patch.insert("zoom".into(), json!(v.zoomed.then_some(now.map(PaneRef)).flatten()));
    }
    kernel.changed(Kind::View, view, Value::Object(patch));
}
/// `tab.changed {place}`.
fn place_changed(kernel: &mut Kernel, tab: TabId) {
    let pane = kernel.model.pane_of_tab(tab).map(|(_, p)| p);
    let place = serde_json::to_value(TabPlace::of(&kernel.model.tabs[&tab].place, pane)).unwrap_or(Value::Null);
    kernel.changed(Kind::Tab, tab, json!({ "place": place }));
}
/// Takes a tab out of where it is, without closing anything: out of its pane, and the pane out
/// of the layout once it is empty. What it leaves empty is pruned by the caller. Returns the
/// view it was tiled in.
fn unplace(kernel: &mut Kernel, tab: TabId) -> Option<ViewId> {
    let place = kernel.model.tabs.get(&tab)?.place.clone();
    let Place::Tiled { view } = place else { return None };
    kernel.model.views.get_mut(&view)?.remove(tab);
    kernel.model.tabs.get_mut(&tab)?.place = Place::Detached;
    Some(view)
}
/// Closes what was left empty; `view.closed` carries the sessions it changed.
fn prune(kernel: &mut Kernel) {
    kernel.prune();
}
/// Closes a tab. Its state is kept to be reopened.
pub fn close_tab(cx: &mut Cx, tab: TabId) -> Result<()> {
    close_tab_with(cx, tab, true)
}
/// [`close_tab`]; `keep` keeps its state to be reopened (not when it moved elsewhere).
pub fn close_tab_with(cx: &mut Cx, tab: TabId, keep: bool) -> Result<()> {
    let (ty, place) = {
        let kernel = cx.host::<Kernel>()?;
        let p = kernel.model.tabs.get(&tab).ok_or_else(|| Error::not_found(format!("no tab %{tab}")))?;
        (p.block.clone(), p.place.clone())
    };
    if keep && matches!(place, Place::Tiled { .. } | Place::Float { .. }) {
        match get_state(cx, tab, &StateOpts { purpose: Some("restore".into()), ..StateOpts::default() }) {
            Ok(state) => {
                let kernel = cx.host::<Kernel>()?;
                let view = place.view();
                let pane = kernel.model.pane_of_tab(tab).map(|(_, p)| p);
                // A tab beside it, in another pane: where it goes back when its pane is gone.
                let beside = view.and_then(|v| {
                    let v = kernel.model.views.get(&v)?;
                    let shown = v.shown();
                    let at = v.pane_order().iter().position(|p| Some(*p) == pane)?;
                    shown.get(at.wrapping_sub(1)).or(shown.get(at + 1)).copied()
                });
                kernel.closed.push_front(Closed { tab, state, view, pane, beside, at: now_rfc3339() });
                kernel.closed.truncate(CLOSED_LIMIT);
            }
            Err(err) => crate::server::log(&format!("closing %{tab}: its state was not kept: {}", err.message)),
        }
    }
    let kernel = cx.host::<Kernel>()?;
    if let Some(mut instance) = kernel.take_instance(tab) {
        instance.close(kernel, tab);
    }
    let before = view_focus(kernel, place.view());
    let view = unplace(kernel, tab);
    let view = view.or(place.view());
    kernel.model.tabs.remove(&tab);
    kernel.model.options.drop_object(Key::Tab(tab));
    kernel.emit("tab.closed", &ty, About::tab(tab), json!({ "tab": TabRef(tab), "block": ty }));
    if let Some(view) = view
        && kernel.model.views.contains_key(&view)
    {
        let alive = kernel.model.views[&view]
            .tabs()
            .into_iter()
            .any(|t| matches!(kernel.model.tabs.get(&t).map(|t| &t.place), Some(Place::Tiled { .. })));
        if alive {
            if matches!(place, Place::Tiled { .. }) {
                kernel.layout_changed(view);
            }
            focus_changed(kernel, before);
        } else {
            let floats: Vec<TabId> = kernel
                .model
                .tabs
                .values()
                .filter(|p| matches!(&p.place, Place::Float { view: t, .. } if *t == view))
                .map(|p| p.id)
                .collect();
            for float in floats {
                close_tab(cx, float)?;
            }
        }
    }
    let kernel = cx.host::<Kernel>()?;
    prune(kernel);
    Ok(())
}
pub fn close_view(cx: &mut Cx, view: ViewId) -> Result<()> {
    let tabs: Vec<TabId> = {
        let kernel = cx.host::<Kernel>()?;
        kernel.model.tabs.values().filter(|p| p.place.view() == Some(view)).map(|p| p.id).collect()
    };
    for tab in tabs {
        if cx.host::<Kernel>()?.model.tabs.contains_key(&tab) {
            close_tab(cx, tab)?;
        }
    }
    Ok(())
}
pub fn close_session(cx: &mut Cx, session: SessionId) -> Result<()> {
    let views: Vec<ViewId> = {
        let kernel = cx.host::<Kernel>()?;
        let s = &kernel.model.sessions[&session];
        s.views
            .iter()
            .map(|(_, t)| *t)
            .filter(|t| kernel.model.sessions.values().filter(|o| o.views.iter().any(|(_, x)| x == t)).count() == 1)
            .collect()
    };
    for view in views {
        close_view(cx, view)?;
    }
    let kernel = cx.host::<Kernel>()?;
    if let Some(s) = kernel.model.sessions.get_mut(&session) {
        s.views.clear();
    }
    prune(kernel);
    Ok(())
}
/// Moves a tab: anywhere a new tab could go, or into another tab's place (`swap`).
pub fn set_place(cx: &mut Cx, tab: TabId, spec: PlaceSpec) -> Result<()> {
    if spec.replace.is_some() {
        return Err(Error::invalid_params("`replace` is for opening a block: `tab.open`"));
    }
    if let Some(other) = spec.swap {
        let caller = cx.caller().clone();
        let kernel = cx.host::<Kernel>()?;
        let other = kernel.model.tab_id(&caller, other)?;
        return swap(kernel, tab, other);
    }
    let target = target(cx, &spec, Some(tab))?;
    let kernel = cx.host::<Kernel>()?;
    let from = kernel.model.tabs[&tab].place.clone();
    let focus = spec.focus.unwrap_or(true);
    if let (Target::Float { view, .. }, Place::Tiled { view: from_view }) = (&target, &from)
        && view == from_view
        && kernel.model.views[view].tabs() == [tab]
    {
        return Err(Error::invalid_params(format!("tab %{tab} is its view's only tab, so it cannot float over it")));
    }
    // Within its own pane, it only moves along the strip.
    if let Target::Join { view, pane, index } = target
        && kernel.model.pane_of_tab(tab) == Some((view, pane))
    {
        let v = kernel.model.views.get_mut(&view).expect("a resolved view");
        let p = v.panes.get_mut(&pane).expect("a resolved pane");
        p.tabs.retain(|t| *t != tab);
        let at = index.map_or(p.tabs.len(), |i| i.min(p.tabs.len()));
        p.tabs.insert(at, tab);
        kernel.layout_changed(view);
        if focus {
            set_current_tab_in(kernel, tab);
        }
        return Ok(());
    }
    let before = view_focus(kernel, from.view());
    let old_view = unplace(kernel, tab);
    let (place, created) = build(kernel, tab, target, &Value::Null, focus)?;
    kernel.model.tabs.get_mut(&tab).expect("the moving tab").place = place.clone();
    prune(kernel);
    announce_created(kernel, &created);
    place_changed(kernel, tab);
    for view in [old_view, place.view().filter(|_| created.view.is_none())].into_iter().flatten() {
        if kernel.model.views.contains_key(&view) {
            kernel.layout_changed(view);
        }
    }
    focus_changed(kernel, before);
    if focus && matches!(place, Place::Tiled { .. }) {
        set_current_tab_in(kernel, tab);
    }
    Ok(())
}
fn swap(kernel: &mut Kernel, a: TabId, b: TabId) -> Result<()> {
    if a == b {
        return Ok(());
    }
    let (pa, pb) = (kernel.model.tabs[&a].place.clone(), kernel.model.tabs[&b].place.clone());
    let before = [view_focus(kernel, pa.view()), view_focus(kernel, pb.view()).filter(|_| pa.view() != pb.view())];
    // Each tiled one takes the other's spot in its pane's strip, in front if the other was.
    let spots = [a, b].map(|t| {
        let (view, pane) = kernel.model.pane_of_tab(t)?;
        let p = &kernel.model.views[&view].panes[&pane];
        Some((view, pane, p.tabs.iter().position(|x| *x == t)?))
    });
    for (spot, gone, came) in [(spots[0], a, b), (spots[1], b, a)] {
        let Some((view, pane, at)) = spot else { continue };
        let v = kernel.model.views.get_mut(&view).expect("a view");
        let p = v.panes.get_mut(&pane).expect("a pane");
        p.tabs[at] = came;
        if p.active == gone {
            p.active = came;
        }
        if p.last == Some(gone) {
            p.last = Some(came);
        }
        if v.active == gone && spots[0].map(|s| s.0) != spots[1].map(|s| s.0) {
            v.active = came;
        }
    }
    kernel.model.tabs.get_mut(&a).expect("a").place = pb.clone();
    kernel.model.tabs.get_mut(&b).expect("b").place = pa.clone();
    let mut views: Vec<ViewId> = [pa.view(), pb.view()].into_iter().flatten().collect();
    views.dedup();
    for tab in [a, b] {
        place_changed(kernel, tab);
    }
    for view in views {
        kernel.layout_changed(view);
    }
    for before in before {
        focus_changed(kernel, before);
    }
    Ok(())
}
/// Makes a tab its pane's front tab and its view's active tab, its view its session's current
/// view, and its session the caller's (its client's, or the server's last).
pub fn set_current_tab_in(kernel: &mut Kernel, tab: TabId) {
    let Some(view) = kernel.model.view_of_tab(tab) else { return };
    let tiled = matches!(kernel.model.tabs[&tab].place, Place::Tiled { .. });
    if tiled && kernel.model.views.get_mut(&view).expect("a view").bring(tab) {
        kernel.layout_changed(view);
    }
    let previous = kernel.model.views[&view].active;
    if tiled && previous != tab {
        let before = view_focus(kernel, Some(view));
        let v = kernel.model.views.get_mut(&view).expect("a view");
        v.last = Some(previous);
        v.active = tab;
        focus_changed(kernel, before);
    }
    kernel.touch_tab(tab);
    if let Some(session) = kernel.model.session_of_view(view) {
        set_current_view_in(kernel, session, view);
    }
}
pub fn set_current_view_in(kernel: &mut Kernel, session: SessionId, view: ViewId) {
    let previous = kernel.model.sessions[&session].current;
    if previous != Some(view) {
        let s = kernel.model.sessions.get_mut(&session).expect("a session");
        s.last = previous;
        s.current = Some(view);
        kernel.active_view_changed(session);
    }
    kernel.touch_session(session);
}
/// Options for [`get_state`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StateOpts {
    /// Overrides the `viewoptions` option for this call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewoptions: Option<String>,
    /// `restore` (the default), `copy`, `move` or `share`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// `ref` keeps blobs on this server (the default); `inline` puts their bytes in the state, for
    /// a state that leaves it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blobs: Option<String>,
}
/// Saves a tab's state. What other channels add to it (`extra`) they add themselves.
pub fn get_state(cx: &mut Cx, tab: TabId, opts: &StateOpts) -> Result<TabState> {
    let purpose = opts.purpose.clone().unwrap_or_else(|| "restore".into());
    if !["restore", "copy", "move", "share"].contains(&purpose.as_str()) {
        return Err(Error::invalid_params(format!("purpose is restore, copy, move or share: not {purpose}")));
    }
    let inline = match opts.blobs.as_deref() {
        None | Some("ref") => false,
        Some("inline") => true,
        Some(other) => return Err(Error::invalid_params(format!("blobs is ref or inline: not {other}"))),
    };
    let state = {
        let kernel = cx.host::<Kernel>()?;
        let ty = kernel.model.tabs.get(&tab).ok_or_else(|| Error::not_found(format!("no tab %{tab}")))?.block.clone();
        let tab_type = kernel.block(&ty)?;
        let viewoptions = opts
            .viewoptions
            .clone()
            .unwrap_or_else(|| kernel.global_option("viewoptions").as_str().unwrap_or("").to_string());
        let save = SaveOpts {
            purpose,
            viewoptions: viewoptions.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        };
        let serialized = kernel.with_instance(tab, |instance, kernel| instance.serialize(kernel, tab, &save))?;
        let blobs = serialized
            .blobs
            .iter()
            .map(|(name, encoding, bytes)| (name.clone(), kernel.blobs.put(encoding, bytes)))
            .collect();
        let options = kernel
            .model
            .options
            .local(Key::Tab(tab))
            .into_iter()
            .filter(|(name, _)| !kernel.model.options.is_readonly(name))
            .map(|(name, set)| (name, set.value))
            .collect();
        TabState {
            pumi_state: STATE_VERSION,
            block: ty.clone(),
            block_version: tab_type.version(),
            options,
            data: serialized.data,
            blobs,
            extra: BTreeMap::new(),
            strategy: tab_type.strategy(),
            needs: serialized.needs,
            origin: Some(Origin { server: kernel.config.name.clone(), tab, at: now_rfc3339() }),
        }
    };
    let mut state = state;
    if inline {
        let kernel = cx.host::<Kernel>()?;
        for blob in state.blobs.values_mut() {
            *blob = kernel.blobs.inline(blob)?;
        }
    }
    Ok(state)
}
