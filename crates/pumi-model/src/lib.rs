pub mod event;
pub mod objects;
pub mod options;
pub mod state;
use options::{Key, Options};
use pumi_api::{Caller, Error, Result};
pub use pumi_layout::{Dim, Layout, PaneId, Rect, Side};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub type SessionId = u64;
pub type ViewId = u64;
pub type TabId = u64;
pub type ClientId = u64;
pub type ChanId = u64;
/// The channel of calls the server makes itself (startup, terminals' notes, restores).
pub const SERVER: ChanId = 0;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub cwd: Option<String>,
    /// `(index, view)`, by index. A view linked into several sessions has an index in each.
    pub views: Vec<(u32, ViewId)>,
    pub current: Option<ViewId>,
    pub last: Option<ViewId>,
    #[serde(default)]
    pub activity: u64,
    /// When it was created, in seconds since the epoch (`session_created`).
    #[serde(default)]
    pub created: u64,
}
/// What a window shows: a layout of panes, each holding tabs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub id: ViewId,
    /// Its name; without one it shows its active tab's title (`automatic_rename`).
    pub name: Option<String>,
    /// Its panes, split side by side: the layout's leaves are their ids.
    pub layout: Layout,
    /// The panes of its layout, by id.
    pub panes: BTreeMap<PaneId, Pane>,
    /// The tab with the focus: the one in front of its pane.
    pub active: TabId,
    /// The tab that had the focus before it.
    pub last: Option<TabId>,
    /// The active tab's pane fills the view, its tab strip still on top.
    pub zoomed: bool,
}
/// An area of a view's layout: the tabs it holds, in the order its strip shows them, and the one
/// in front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    pub id: PaneId,
    pub tabs: Vec<TabId>,
    /// The tab in front: the one drawn, and the one with the focus while the pane has it.
    pub active: TabId,
    /// The tab in front before it: it comes forward when the one in front goes.
    pub last: Option<TabId>,
}
impl View {
    /// A view of one pane, holding one tab.
    pub fn new(id: ViewId, name: Option<String>, pane: PaneId, tab: TabId) -> View {
        View {
            id,
            name,
            layout: Layout::new(pane),
            panes: BTreeMap::from([(pane, Pane { id: pane, tabs: vec![tab], active: tab, last: None })]),
            active: tab,
            last: None,
            zoomed: false,
        }
    }
    /// The pane holding `tab`.
    pub fn pane_of(&self, tab: TabId) -> Option<PaneId> {
        self.panes.values().find(|p| p.tabs.contains(&tab)).map(|p| p.id)
    }
    /// The pane with the focus: the active tab's.
    pub fn active_pane(&self) -> Option<PaneId> {
        self.pane_of(self.active)
    }
    /// Its panes, in layout order (left to right, top to bottom).
    pub fn pane_order(&self) -> Vec<PaneId> {
        self.layout.panes().into_iter().filter(|p| self.panes.contains_key(p)).collect()
    }
    /// Its tiled tabs: pane by pane in layout order, each pane's in its strip's order.
    pub fn tabs(&self) -> Vec<TabId> {
        self.pane_order().iter().flat_map(|p| self.panes[p].tabs.iter().copied()).collect()
    }
    /// The tabs in front of their panes, in layout order: the ones drawn.
    pub fn shown(&self) -> Vec<TabId> {
        self.pane_order().iter().map(|p| self.panes[p].active).collect()
    }
    /// Whether `tab` is tiled here.
    pub fn holds(&self, tab: TabId) -> bool {
        self.pane_of(tab).is_some()
    }
    /// Every tiled tab with the cells of its pane: a tab behind another is sized as the one in
    /// front, so it doesn't change size as it comes forward.
    pub fn tab_rects(&self, area: Rect) -> Vec<(TabId, Rect)> {
        let mut out = Vec::new();
        for (pane, rect) in self.layout.rects(area) {
            if let Some(p) = self.panes.get(&pane) {
                out.extend(p.tabs.iter().map(|t| (*t, rect)));
            }
        }
        out
    }
    /// The cells of the pane holding `tab`.
    pub fn rect_of(&self, tab: TabId, area: Rect) -> Option<Rect> {
        self.layout.rect_of(self.pane_of(tab)?, area)
    }
    /// Puts `tab` in `pane` at `index` of its strip (after the tab in front by default), and in
    /// front of it when `front`.
    pub fn insert(&mut self, pane: PaneId, tab: TabId, index: Option<usize>, front: bool) -> Result<()> {
        let p = self.panes.get_mut(&pane).ok_or_else(|| Error::not_found(format!("no pane :{pane} in view @{}", self.id)))?;
        let after_front = p.tabs.iter().position(|t| *t == p.active).map_or(p.tabs.len(), |i| i + 1);
        let at = index.map_or(after_front, |i| i.min(p.tabs.len()));
        p.tabs.insert(at, tab);
        if front && p.active != tab {
            p.last = Some(p.active);
            p.active = tab;
        }
        Ok(())
    }
    /// Splits `beside` (or the whole view, `full`) to make `pane`, holding `tab`, on its `side`
    /// with `share` of its space.
    pub fn split(&mut self, beside: PaneId, pane: PaneId, tab: TabId, side: Side, share: f64, full: bool) -> Result<()> {
        self.layout.split_share(beside, pane, side, share, full)?;
        self.panes.insert(pane, Pane { id: pane, tabs: vec![tab], active: tab, last: None });
        Ok(())
    }
    /// Brings `tab` to the front of its pane; whether it wasn't already.
    pub fn bring(&mut self, tab: TabId) -> bool {
        let Some(pane) = self.pane_of(tab) else { return false };
        let p = self.panes.get_mut(&pane).expect("the tab's pane");
        if p.active == tab {
            return false;
        }
        p.last = Some(p.active);
        p.active = tab;
        true
    }
    /// Takes `tab` out of its pane, and the pane out of the layout once it holds nothing (its
    /// last pane stays in the layout, for the view to be pruned). The pane's last tab comes to
    /// the front in its place, else its neighbour; the focus goes to its pane's new front tab,
    /// else to the pane focused before. Whether it was tiled here.
    pub fn remove(&mut self, tab: TabId) -> bool {
        let Some(pane) = self.pane_of(tab) else { return false };
        let p = self.panes.get_mut(&pane).expect("the tab's pane");
        let at = p.tabs.iter().position(|t| *t == tab).expect("a tab of its pane");
        p.tabs.remove(at);
        let left = if p.tabs.is_empty() {
            self.panes.remove(&pane);
            self.layout.remove(pane);
            None
        } else {
            if p.active == tab {
                p.active = p.last.filter(|l| p.tabs.contains(l)).unwrap_or(p.tabs[at.min(p.tabs.len() - 1)]);
            }
            if p.last == Some(tab) || p.last == Some(p.active) {
                p.last = None;
            }
            Some(p.active)
        };
        if self.last == Some(tab) {
            self.last = None;
        }
        if self.active == tab {
            match left {
                // The zoom was its pane's, and stays with it.
                Some(front) => self.active = front,
                None => {
                    self.zoomed = false;
                    let before = self.last.and_then(|l| self.pane_of(l)).map(|p| self.panes[&p].active);
                    if let Some(next) = before.or_else(|| self.shown().first().copied()) {
                        self.active = next;
                    }
                    self.last = None;
                }
            }
        }
        true
    }
}
/// A floating tab's geometry, kept as
/// its caller wrote it: whoever draws it lays it out against the space it has then.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Float {
    /// What it floats over: `view` (its view's tabs) or `client` (the whole window); `pane`,
    /// `cursor` and `mouse` are kept, and drawn as `view`.
    pub relative: String,
    /// The tab `relative = "pane"` is relative to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<TabId>,
    /// The corner or edge it is pinned to (`NW` … `SE`, `center`); `row` and `col` are in from
    /// it.
    #[serde(default = "nw")]
    pub anchor: String,
    pub row: Dim,
    pub col: Dim,
    pub width: Dim,
    pub height: Dim,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    pub focusable: bool,
    /// It takes the pointer; `focusable` when not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mouse: Option<bool>,
    /// What closes it: `escape`, `key`, `press`, `scroll` (the app does it).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dismiss: Vec<String>,
    /// Dims what is under it, 0 to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop: Option<u8>,
    pub zindex: i64,
}
fn nw() -> String {
    "NW".into()
}
impl Float {
    /// The cells it covers in `area` (its view's), for what counts in cells (`pane_width`).
    pub fn rect(&self, area: pumi_layout::Rect) -> pumi_layout::Rect {
        pumi_layout::float::rect(area, &self.anchor, self.row, self.col, self.width, self.height)
    }
}
/// Where a tab is. A new view or a new session is a tab `tiled` in it. A tiled tab's pane is
/// the view's to say ([`View::pane_of`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Place {
    /// In a pane of a view's layout.
    Tiled { view: ViewId },
    /// Over a view.
    Float {
        view: ViewId,
        #[serde(flatten)]
        float: Float,
    },
    /// Alive and in no view: where the core keeps a tab while it moves it. Not a place to ask for.
    Detached,
}
impl Place {
    pub fn view(&self) -> Option<ViewId> {
        match self {
            Place::Tiled { view } | Place::Float { view, .. } => Some(*view),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    pub id: TabId,
    /// What it holds: `file`, or a plugin's block (`owner.name.id`).
    #[serde(alias = "type")]
    pub block: String,
    pub place: Place,
    #[serde(default)]
    pub activity: u64,
}
/// A client: a channel that attached as a UI (`client.attach`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ui {
    pub session: SessionId,
    pub width: u32,
    pub height: u32,
    /// Events up to here were in the state it was sent when it attached.
    #[serde(default)]
    pub from_seq: u64,
}
/// What a channel said about itself (`channel.setInfo`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ClientInfo {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Value,
    /// `ui`, `embedder`, `host`, `plugin`, or `cli`.
    #[serde(default, rename = "type")]
    pub ty: String,
    #[serde(default)]
    pub attributes: Value,
}
/// A connection: a socket client, the CLI, a script, a plugin host.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chan {
    pub id: ChanId,
    pub info: ClientInfo,
    /// The tab calls on it are relative to (`PUMI_TAB`, from its attributes).
    pub tab: Option<TabId>,
    pub ui: Option<Ui>,
}
/// Where a call is: the session, view and tab that `0` means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Current {
    pub session: SessionId,
    pub view: ViewId,
    pub tab: TabId,
}
/// Ids are never reused, and are kept across restarts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Ids {
    pub session: u64,
    pub view: u64,
    pub pane: u64,
    pub tab: u64,
}
/// The objects of the model, the channels connected and every option: what every replica holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub sessions: BTreeMap<SessionId, Session>,
    pub views: BTreeMap<ViewId, View>,
    pub tabs: BTreeMap<TabId, Tab>,
    pub chans: BTreeMap<ChanId, Chan>,
    /// The session used last, for callers with no client and no tab.
    pub last_session: Option<SessionId>,
    pub options: Options,
}
impl Model {
    /// Who set something, for provenance: the caller's source, the channel's client name, or
    /// `chan N`; `own` names calls the process makes itself.
    pub fn source(&self, caller: &Caller, own: &str) -> String {
        if let Some(source) = &caller.source {
            return source.clone();
        }
        if caller.channel == SERVER {
            return own.into();
        }
        match self.chans.get(&caller.channel) {
            Some(chan) if !chan.info.name.is_empty() => chan.info.name.clone(),
            _ => format!("chan {}", caller.channel),
        }
    }
    /// The tab a caller's calls are about: its own (`PUMI_TAB`), its channel's, else the one
    /// its client says it focuses (`client_tab`, the app's).
    fn caller_tab(&self, caller: &Caller) -> Option<TabId> {
        let focused = || {
            self.chans.get(&caller.channel)?.ui.as_ref()?;
            let key = options::Key::Client(caller.channel);
            self.options.get("client_tab", &[key]).and_then(|(v, _)| v.as_u64())
        };
        caller
            .tab
            .or_else(|| self.chans.get(&caller.channel)?.tab)
            .or_else(focused)
            .filter(|t| self.tabs.contains_key(t))
    }
    fn caller_session(&self, caller: &Caller) -> Option<SessionId> {
        let ui = self.chans.get(&caller.channel)?.ui.as_ref()?;
        self.sessions.contains_key(&ui.session).then_some(ui.session)
    }
    /// The caller's session, view and tab: its tab (`PUMI_TAB`), else its client's session,
    /// else the session used last, else the most active.
    pub fn current(&self, caller: &Caller) -> Option<Current> {
        if let Some(tab) = self.caller_tab(caller)
            && let Some(view) = self.tabs[&tab].place.view()
        {
            let session = self
                .caller_session(caller)
                .filter(|s| self.sessions[s].views.iter().any(|(_, v)| *v == view))
                .or_else(|| self.session_of_view(view))?;
            return Some(Current { session, view, tab });
        }
        let session = self
            .caller_session(caller)
            .or(self.last_session.filter(|s| self.sessions.contains_key(s)))
            .or_else(|| self.sessions.values().max_by_key(|s| s.activity).map(|s| s.id))?;
        let view =
            self.sessions[&session].current.or_else(|| self.sessions[&session].views.first().map(|(_, t)| *t))?;
        Some(Current { session, view, tab: self.views.get(&view)?.active })
    }
    pub fn session_id(&self, caller: &Caller, id: SessionId) -> Result<SessionId> {
        if id == 0 {
            return self
                .current(caller)
                .map(|c| c.session)
                .ok_or_else(|| Error::not_found("there is no current session"));
        }
        self.sessions.contains_key(&id).then_some(id).ok_or_else(|| Error::not_found(format!("no session ${id}")))
    }
    pub fn view_id(&self, caller: &Caller, id: ViewId) -> Result<ViewId> {
        if id == 0 {
            return self.current(caller).map(|c| c.view).ok_or_else(|| Error::not_found("there is no current view"));
        }
        self.views.contains_key(&id).then_some(id).ok_or_else(|| Error::not_found(format!("no view @{id}")))
    }
    pub fn tab_id(&self, caller: &Caller, id: TabId) -> Result<TabId> {
        if id == 0 {
            return self.current(caller).map(|c| c.tab).ok_or_else(|| Error::not_found("there is no current tab"));
        }
        self.tabs.contains_key(&id).then_some(id).ok_or_else(|| Error::not_found(format!("no tab %{id}")))
    }
    /// The session a view is in; of several, the one used last.
    pub fn session_of_view(&self, view: ViewId) -> Option<SessionId> {
        self.sessions
            .values()
            .filter(|s| s.views.iter().any(|(_, v)| *v == view))
            .max_by_key(|s| s.activity)
            .map(|s| s.id)
    }
    pub fn view_of_tab(&self, tab: TabId) -> Option<ViewId> {
        self.tabs.get(&tab)?.place.view()
    }
    /// The pane a tiled tab is in, with its view.
    pub fn pane_of_tab(&self, tab: TabId) -> Option<(ViewId, PaneId)> {
        let Place::Tiled { view } = self.tabs.get(&tab)?.place else { return None };
        Some((view, self.views.get(&view)?.pane_of(tab)?))
    }
    /// The view a pane is in.
    pub fn view_of_pane(&self, pane: PaneId) -> Option<ViewId> {
        self.views.values().find(|v| v.panes.contains_key(&pane)).map(|v| v.id)
    }
    /// A pane by its id, `0` being the caller's tab's.
    pub fn pane_id(&self, caller: &Caller, id: PaneId) -> Result<(ViewId, PaneId)> {
        if id == 0 {
            let tab = self.current(caller).map(|c| c.tab).ok_or_else(|| Error::not_found("there is no current pane"))?;
            return self.pane_of_tab(tab).ok_or_else(|| Error::not_found(format!("tab %{tab} is not in a pane")));
        }
        self.view_of_pane(id).map(|v| (v, id)).ok_or_else(|| Error::not_found(format!("no pane :{id}")))
    }
    /// The objects whose options a tab reads, narrowest first.
    pub fn chain(&self, tab: Option<TabId>, view: Option<ViewId>, session: Option<SessionId>) -> Vec<Key> {
        let view = view.or_else(|| tab.and_then(|t| self.view_of_tab(t)));
        let session = session.or_else(|| view.and_then(|v| self.session_of_view(v)));
        let mut chain = Vec::with_capacity(4);
        chain.extend(tab.map(Key::Tab));
        chain.extend(view.map(Key::View));
        chain.extend(session.map(Key::Session));
        chain.push(Key::Global);
        chain
    }
    /// A session as the wire carries it.
    pub fn session_value(&self, id: SessionId) -> Value {
        let s = &self.sessions[&id];
        json!({ "id": s.id, "name": s.name, "cwd": s.cwd, "views": s.views, "current": s.current, "last": s.last, "activity": s.activity, "created": s.created })
    }
    /// A view as the wire carries it.
    pub fn view_value(&self, id: ViewId) -> Value {
        let v = &self.views[&id];
        json!({ "id": v.id, "name": v.name, "layout": v.layout.to_value(), "panes": v.panes, "active": v.active, "last": v.last, "zoomed": v.zoomed })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    /// Two sessions sharing view 1, a second view in session 2, and a client showing session 2.
    fn model() -> Model {
        let session = |id, views: Vec<(u32, ViewId)>, activity| Session {
            id,
            name: format!("s{id}"),
            cwd: None,
            current: views.first().map(|(_, v)| *v),
            views,
            last: None,
            activity,
            created: 0,
        };
        let view = |id, tab| View::new(id, None, tab + 100, tab);
        let tab = |id, view| Tab { id, block: "terminal".into(), place: Place::Tiled { view }, activity: 0 };
        let ui = Ui { session: 2, width: 80, height: 24, from_seq: 0 };
        Model {
            sessions: BTreeMap::from([(1, session(1, vec![(0, 1)], 5)), (2, session(2, vec![(0, 2), (1, 1)], 3))]),
            views: BTreeMap::from([(1, view(1, 10)), (2, view(2, 20))]),
            tabs: BTreeMap::from([(10, tab(10, 1)), (20, tab(20, 2))]),
            chans: BTreeMap::from([(7, Chan { id: 7, ui: Some(ui), ..Chan::default() })]),
            last_session: None,
            options: Options::default(),
        }
    }
    fn caller(channel: ChanId, tab: Option<TabId>) -> Caller {
        Caller { channel, tab, ..Caller::default() }
    }
    #[test]
    fn zero_is_the_callers_tab_in_its_clients_session() {
        let m = model();
        assert_eq!(m.current(&caller(7, Some(10))), Some(Current { session: 2, view: 1, tab: 10 }));
        assert_eq!(m.current(&caller(9, Some(10))), Some(Current { session: 1, view: 1, tab: 10 }));
    }
    #[test]
    fn zero_without_a_tab_is_the_client_then_the_last_then_the_most_active() {
        let mut m = model();
        assert_eq!(m.current(&caller(7, None)).map(|c| c.session), Some(2));
        assert_eq!(m.current(&caller(9, None)).map(|c| c.session), Some(1));
        m.last_session = Some(2);
        assert_eq!(m.current(&caller(9, None)), Some(Current { session: 2, view: 2, tab: 20 }));
    }
    #[test]
    fn ids_resolve_zero_and_refuse_the_unknown() {
        let m = model();
        assert_eq!(m.tab_id(&caller(7, Some(20)), 0).unwrap(), 20);
        assert_eq!(m.view_id(&caller(7, None), 2).unwrap(), 2);
        assert!(m.session_id(&caller(7, None), 9).is_err());
    }
    #[test]
    fn a_pane_is_found_by_its_id_or_by_the_callers_tab() {
        let m = model();
        assert_eq!(m.pane_id(&caller(7, Some(20)), 0).unwrap(), (2, 120));
        assert_eq!(m.pane_id(&caller(7, None), 110).unwrap(), (1, 110));
        assert!(m.pane_id(&caller(7, None), 20).is_err());
    }
    /// A view of one pane holding tabs 1, 2 and 3, with 1 in front, and a second pane holding 4.
    fn stacked() -> View {
        let mut v = View::new(1, None, 10, 1);
        v.insert(10, 2, None, false).unwrap();
        v.insert(10, 3, None, false).unwrap();
        v.split(10, 11, 4, Side::Right, 0.5, false).unwrap();
        v
    }
    #[test]
    fn a_new_tab_goes_after_the_one_in_front_or_where_it_is_put() {
        let mut v = stacked();
        assert_eq!(v.panes[&10].tabs, vec![1, 3, 2]);
        v.insert(10, 5, Some(0), true).unwrap();
        assert_eq!(v.panes[&10].tabs, vec![5, 1, 3, 2]);
        assert_eq!((v.panes[&10].active, v.panes[&10].last), (5, Some(1)));
        assert_eq!(v.tabs(), vec![5, 1, 3, 2, 4]);
        assert_eq!(v.shown(), vec![5, 4]);
    }
    #[test]
    fn closing_the_front_tab_brings_the_last_one_forward_and_keeps_the_focus_in_its_pane() {
        let mut v = stacked();
        v.bring(2);
        v.zoomed = true;
        assert!(v.remove(1));
        assert_eq!(v.panes[&10].active, 2);
        v.bring(3);
        v.active = 3;
        v.remove(3);
        assert_eq!((v.panes[&10].active, v.active, v.zoomed), (2, 2, true));
    }
    #[test]
    fn a_pane_goes_with_its_last_tab_and_the_focus_goes_back_to_the_pane_before() {
        let mut v = stacked();
        v.active = 4;
        v.last = Some(3);
        v.zoomed = true;
        v.remove(4);
        assert!(!v.panes.contains_key(&11));
        assert_eq!(v.layout.panes(), vec![10]);
        assert_eq!((v.active, v.zoomed), (1, false));
    }
    #[test]
    fn every_tab_is_sized_as_its_pane() {
        let v = stacked();
        let rects = v.tab_rects(Rect { x: 0, y: 0, width: 80, height: 24 });
        assert_eq!(rects.len(), 4);
        let left = rects.iter().filter(|(t, _)| *t != 4).map(|(_, r)| *r).collect::<Vec<_>>();
        assert!(left.windows(2).all(|w| w[0] == w[1]));
    }
    #[test]
    fn a_tabs_chain_runs_from_it_to_the_global_options() {
        let m = model();
        assert_eq!(m.chain(Some(20), None, None), vec![Key::Tab(20), Key::View(2), Key::Session(2), Key::Global]);
    }
}
