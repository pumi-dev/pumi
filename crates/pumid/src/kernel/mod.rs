pub mod events;
pub use pumi_layout as layout;
pub mod ops;
pub mod options;
pub mod persist;
pub mod requests;
pub use pumi_model::state;
pub mod types;
pub mod waits;
use events::{About, Event, Events};
use options::{Change, Key, Options};
use pumi_api::{Caller, Error, Kind, Ref, Result};
use pumi_model::objects::{self, LayoutTree};
pub use pumi_model::{
    Chan, ChanId, ClientId, ClientInfo, Current, Float, Ids, Model, Pane, PaneId, Place, SERVER, Session, SessionId,
    Tab, TabId, Ui, View, ViewId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use state::{Blobs, TabState};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use types::{Instance, TabType};
/// A function a channel provides.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub chan: ChanId,
    pub info: pumi_api::FunctionInfo,
    /// The reference a call goes to as a `rpc.callback` notification, answered `null`; without
    /// one the call goes to the channel as a request, and its answer comes back.
    pub callback: Option<String>,
}
/// A tab that closed, kept to be reopened from its state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Closed {
    pub tab: TabId,
    pub state: TabState,
    /// Where it was: its view, its pane, and a tab beside its pane, if they are still there.
    pub view: Option<ViewId>,
    #[serde(default)]
    pub pane: Option<PaneId>,
    pub beside: Option<TabId>,
    pub at: String,
}
/// How many closed tabs are kept to be reopened.
pub const CLOSED_LIMIT: usize = 32;
/// What the server tells the kernel about itself.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The server's name, in states' `origin` and to other servers.
    pub name: String,
    /// Where state and blobs are kept; `None` keeps nothing.
    pub data_dir: Option<PathBuf>,
    /// `$PUMI` for tabs: this server's socket.
    pub socket: PathBuf,
    /// Prepended to every terminal's `PATH` (the `pumi` CLI).
    pub bin_dir: Option<PathBuf>,
    /// The environment terminals start from (the login shell's).
    pub base_env: BTreeMap<String, String>,
}
pub struct Kernel {
    pub config: Config,
    pub ids: Ids,
    /// Sessions, views, tabs, channels and options: what every replica holds.
    pub model: Model,
    instances: HashMap<TabId, Box<dyn Instance>>,
    /// The core's blocks (`terminal`), each with its own engine. One registry with
    /// `plugin_blocks`: `block.list` lists both, and `tab.open` opens either.
    pub blocks: BTreeMap<String, Rc<dyn TabType>>,
    /// The enabled plugins' blocks (`owner.name.id`), from their manifests (`core.setBlocks`):
    /// the generic engine with their own version, strategy and methods. Saved with the state, so a
    /// tab of one restores before pumi-host says them again.
    pub plugin_blocks: BTreeMap<String, Rc<crate::tab::generic::GenericType>>,
    /// The enabled plugins' traits (`core.setTraits`), after the core's in `trait.list`.
    pub plugin_traits: Vec<crate::api::traits::TraitInfo>,
    pub events: Events,
    /// The open requests (`request.open`).
    pub requests: requests::Requests,
    /// The callers blocked in `event.wait`.
    pub waits: waits::Waits,
    pub blobs: Blobs,
    pub closed: VecDeque<Closed>,
    /// Messages for channels, sent by the server after each turn.
    pub out: Vec<(ChanId, String)>,
    /// The model changed since it was last saved.
    pub dirty: bool,
    /// `server.kill` asked the core to stop.
    pub stop: bool,
    /// `server.restart` asked the core to start again as a new process image, keeping every
    /// terminal running.
    pub restart: bool,
    /// Functions channels provide, by name: calls to them are passed on.
    pub routes: BTreeMap<String, Route>,
    /// The functions each app provides for its own client (`runsOn: "client"`), by name and its
    /// channel: a call made for a client goes to that client's app; `routes` has the one every
    /// other call goes to.
    pub client_routes: BTreeMap<String, BTreeMap<ChanId, Route>>,
    /// The channel that gets calls nobody else provides (the host).
    pub fallback: Option<ChanId>,
    activity: u64,
    /// What the server attaches to the kernel: the terminals' inbox and grids.
    pub ext: anymap::Ext,
    /// Each pane as `pane.opened` and `pane.changed` last told of it: its view, its tabs and the
    /// one in front.
    panes_told: BTreeMap<PaneId, (ViewId, Vec<TabId>, TabId)>,
}
/// Values the server and tab types keep beside the model, by type (the terminal type's grid
/// table, its notes channel).
pub mod anymap {
    use std::any::{Any, TypeId};
    use std::collections::HashMap;
    #[derive(Default)]
    pub struct Ext(HashMap<TypeId, Box<dyn Any>>);
    impl Ext {
        pub fn insert<T: Any>(&mut self, value: T) {
            self.0.insert(TypeId::of::<T>(), Box::new(value));
        }
        pub fn get<T: Any>(&self) -> Option<&T> {
            self.0.get(&TypeId::of::<T>())?.downcast_ref()
        }
        pub fn get_mut<T: Any>(&mut self) -> Option<&mut T> {
            self.0.get_mut(&TypeId::of::<T>())?.downcast_mut()
        }
    }
}
impl Kernel {
    pub fn new(config: Config) -> Kernel {
        let blobs = Blobs::new(config.data_dir.as_ref().map(|d| d.join("blobs")));
        Kernel {
            config,
            ids: Ids::default(),
            model: Model {
                chans: BTreeMap::from([(SERVER, Chan { id: SERVER, ..Chan::default() })]),
                options: Options::new(options::builtin()),
                ..Model::default()
            },
            instances: HashMap::new(),
            blocks: BTreeMap::new(),
            plugin_blocks: BTreeMap::new(),
            plugin_traits: Vec::new(),
            events: Events::default(),
            requests: requests::Requests::default(),
            waits: waits::Waits::default(),
            blobs,
            closed: VecDeque::new(),
            out: Vec::new(),
            dirty: false,
            stop: false,
            restart: false,
            routes: BTreeMap::new(),
            client_routes: BTreeMap::new(),
            fallback: None,
            activity: 0,
            ext: anymap::Ext::default(),
            panes_told: BTreeMap::new(),
        }
    }
    /// Keeps state and blobs here from now on (`None`: nothing is kept).
    pub fn set_data_dir(&mut self, dir: Option<PathBuf>) {
        self.blobs = Blobs::new(dir.as_ref().map(|d| d.join("blobs")));
        self.config.data_dir = dir;
    }
    /// Adds one of the core's blocks, and declares its options. Its id is a bare word.
    pub fn register_block(&mut self, block: Rc<dyn TabType>) -> Result<()> {
        let name = block.name().to_string();
        if name.contains('.') || self.blocks.contains_key(&name) {
            return Err(Error::invalid_params(format!("`{name}` is not a new core block's id")));
        }
        for decl in block.options() {
            self.model.options.declare(decl.of_block(&name, &name))?;
        }
        self.blocks.insert(name, block);
        Ok(())
    }
    /// A block by its id: the core's, or an enabled plugin's.
    pub fn block(&self, name: &str) -> Result<Rc<dyn TabType>> {
        if let Some(block) = self.blocks.get(name) {
            return Ok(block.clone());
        }
        if let Some(block) = self.plugin_blocks.get(name) {
            return Ok(block.clone());
        }
        Err(Error::not_found(format!("no block `{name}`: `block.list` lists them")))
    }
    pub fn tick(&mut self) -> u64 {
        self.activity += 1;
        self.activity
    }
    /// Who set something, for provenance.
    pub fn source(&self, caller: &Caller) -> String {
        self.model.source(caller, "server")
    }
    /// An option's value as a tab sees it.
    pub fn tab_option(&self, tab: TabId, name: &str) -> Value {
        self.model.options.get(name, &self.model.chain(Some(tab), None, None)).map(|(v, _)| v).unwrap_or(Value::Null)
    }
    pub fn global_option(&self, name: &str) -> Value {
        self.model.options.get(name, &[Key::Global]).map(|(v, _)| v).unwrap_or(Value::Null)
    }
    pub fn emit(&mut self, event: &str, matched: &str, about: About, data: Value) -> u64 {
        self.dirty = true;
        self.events.push(event, matched, about, data)
    }
    /// Fires `<noun>.changed` for one session, view or tab: `patch` holds the fields that
    /// changed, by the names `<noun>.get` gives them, with their new values.
    pub fn changed(&mut self, kind: Kind, id: u64, patch: Value) {
        let Value::Object(patch) = patch else { return };
        if patch.is_empty() {
            return;
        }
        let text = Ref::local(kind, id).to_string();
        let about = match kind {
            Kind::Session => About::session(id),
            Kind::View => About::view(id),
            Kind::Tab => About::tab(id),
            Kind::Pane => self.model.view_of_pane(id).map(About::view).unwrap_or_default(),
            Kind::Request => About::default(),
        };
        let data = objects::changed(kind.name(), &text, patch);
        self.emit(&format!("{}.changed", kind.name()), &text, about, data);
    }
    /// `view.changed` of a view's layout: its panes, how they split, and each one's tabs; and
    /// what that changed of its panes ([`Kernel::panes_changed`]).
    pub fn layout_changed(&mut self, view: ViewId) {
        let Some(v) = self.model.views.get(&view) else { return };
        let layout = serde_json::to_value(LayoutTree::of(v)).unwrap_or(Value::Null);
        self.changed(Kind::View, view, json!({ "layout": layout }));
        self.panes_changed(view);
    }
    /// Fires `pane.opened`, `pane.changed {tabs, activeTab}` and `pane.closed` for what changed in
    /// a view's panes since they were last told of (a view that is gone has none).
    pub fn panes_changed(&mut self, view: ViewId) {
        let now: Vec<(PaneId, Vec<TabId>, TabId)> = self
            .model
            .views
            .get(&view)
            .map(|v| v.pane_order().into_iter().map(|p| (p, v.panes[&p].tabs.clone(), v.panes[&p].active)).collect())
            .unwrap_or_default();
        let gone: Vec<PaneId> = self
            .panes_told
            .iter()
            .filter(|(p, (v, ..))| *v == view && !now.iter().any(|(n, ..)| n == *p))
            .map(|(p, _)| *p)
            .collect();
        let view_id = Ref::local(Kind::View, view).to_string();
        for pane in gone {
            self.panes_told.remove(&pane);
            let id = Ref::local(Kind::Pane, pane).to_string();
            self.emit("pane.closed", &id, About::view(view), json!({ "pane": id, "view": view_id }));
        }
        let tab_ids = |tabs: &[TabId]| tabs.iter().map(|t| Ref::local(Kind::Tab, *t).to_string()).collect::<Vec<_>>();
        for (pane, tabs, active) in now {
            let id = Ref::local(Kind::Pane, pane).to_string();
            match self.panes_told.get(&pane) {
                None => {
                    self.emit("pane.opened", &id, About::view(view), json!({ "pane": id, "view": view_id }));
                }
                Some((_, was_tabs, was_active)) => {
                    let mut patch = Map::new();
                    if *was_tabs != tabs {
                        patch.insert("tabs".into(), json!(tab_ids(&tabs)));
                    }
                    if *was_active != active {
                        patch.insert("activeTab".into(), json!(Ref::local(Kind::Tab, active).to_string()));
                    }
                    self.changed(Kind::Pane, pane, Value::Object(patch));
                }
            }
            self.panes_told.insert(pane, (view, tabs, active));
        }
    }
    /// `session.changed` of a session's active view.
    pub fn active_view_changed(&mut self, session: SessionId) {
        let Some(s) = self.model.sessions.get(&session) else { return };
        let active = s.current.map(|t| Ref::local(Kind::View, t).to_string());
        self.changed(Kind::Session, session, json!({ "activeView": active }));
    }
    /// The internal `client.changed`: a client shows another session.
    pub fn client_changed(&mut self, client: ChanId, session: SessionId) {
        let session = Ref::local(Kind::Session, session).to_string();
        let data = json!({ "client": client, "keys": ["session"], "patch": { "session": session } });
        self.emit("client.changed", &session, About::default(), data);
    }
    /// Sets an option, fires `option.changed`, and tells a tab's type about its own option.
    /// The provider a call to `name` made for `client` (a channel) goes to: that client's app
    /// when it answers `name` for its own client, else the one everyone's calls go to.
    pub fn route(&self, name: &str, client: ChanId) -> Option<&Route> {
        self.client_routes.get(name).and_then(|apps| apps.get(&client)).or_else(|| self.routes.get(name))
    }
    /// `chan` no longer provides `name` (every name with `None`): its routes go, and a function
    /// another app still provides for its client is everyone's again, the primary app's first.
    pub fn withdraw_routes(&mut self, chan: ChanId, name: Option<&str>) {
        let ours = |n: &str| name.is_none_or(|name| name == n);
        self.routes.retain(|n, route| !ours(n) || route.chan != chan);
        for (n, apps) in self.client_routes.iter_mut() {
            if ours(n) {
                apps.remove(&chan);
            }
        }
        self.client_routes.retain(|_, apps| !apps.is_empty());
        let primary = crate::api::ui::primary(self);
        for (n, apps) in &self.client_routes {
            if !self.routes.contains_key(n) {
                let next = primary.and_then(|p| apps.get(&p)).or_else(|| apps.values().next());
                if let Some(route) = next {
                    self.routes.insert(n.clone(), route.clone());
                }
            }
        }
    }
    pub fn set_option(&mut self, name: &str, key: Key, value: Value, source: &str, internal: bool) -> Result<bool> {
        if let Key::Tab(tab) = key
            && let Some(decl) = self.model.options.decl(name)
            && !decl.blocks.is_empty()
            && let Some(p) = self.model.tabs.get(&tab)
            && !decl.blocks.contains(&p.block)
        {
            return Err(Error::invalid_params(format!("`{name}` is not an option of {} tabs", p.block)));
        }
        let Some(change) = self.model.options.set(name, key, value, source, internal)? else { return Ok(false) };
        self.option_changed(&change, source);
        Ok(true)
    }
    pub fn unset_option(&mut self, name: &str, key: Key, source: &str) {
        if let Some(change) = self.model.options.unset(name, key) {
            self.option_changed(&change, source);
        }
    }
    /// Fires an option's change where it is heard: `option.changed` for the server's own, and
    /// the object's `<noun>.changed` for one of its own options (a trait's field under both its
    /// option and the trait field).
    fn option_changed(&mut self, change: &Change, source: &str) {
        let scope = change.key.scope();
        let about = match change.key {
            Key::Session(id) => About::session(id),
            Key::View(id) => About::view(id),
            Key::Tab(id) => About::tab(id),
            Key::Global | Key::Client(_) => About::default(),
        };
        match change.key {
            Key::Global => {
                self.emit(
                    "option.changed",
                    &change.name,
                    about,
                    json!({ "name": change.name, "scope": "global", "plugin": null, "old": change.old, "new": change.new, "source": source }),
                );
            }
            Key::Session(id) | Key::View(id) | Key::Tab(id) => {
                let kind = match scope {
                    crate::kernel::options::Scope::Session => Kind::Session,
                    crate::kernel::options::Scope::View => Kind::View,
                    _ => Kind::Tab,
                };
                let (noun, text) = (kind.name(), Ref::local(kind, id).to_string());
                let mut keys = vec![format!("options.{}", change.name)];
                let mut options = serde_json::Map::new();
                options.insert(change.name.clone(), change.new.clone());
                let mut patch = serde_json::Map::new();
                patch.insert("options".into(), Value::Object(options));
                if let Some((trait_name, field)) = change.name.strip_prefix('@').and_then(|name| name.split_once('.')) {
                    keys.push(format!("traits.{trait_name}.{field}"));
                    let mut fields = serde_json::Map::new();
                    fields.insert(field.to_string(), change.new.clone());
                    let mut traits = serde_json::Map::new();
                    traits.insert(trait_name.to_string(), Value::Object(fields));
                    patch.insert("traits".into(), Value::Object(traits));
                }
                self.emit(
                    &format!("{noun}.changed"),
                    &text,
                    about,
                    json!({ noun: text, "keys": keys, "patch": patch }),
                );
            }
            // A client's app came to the front or went behind: `app.changed`, matched by its id.
            Key::Client(client) if change.name == "client_active" => {
                let focused = change.new == json!(true);
                if (change.old == json!(true)) != focused {
                    self.emit(
                        "app.changed",
                        &client.to_string(),
                        about,
                        json!({ "client": client, "keys": ["isFocused"], "patch": { "isFocused": focused } }),
                    );
                }
            }
            Key::Client(_) => {}
        }
        if let Key::Tab(tab) = change.key {
            let _ = self.with_instance(tab, |instance, kernel| {
                instance.option_set(kernel, tab, change);
                Ok(())
            });
        }
    }
    /// Runs `f` on a tab's instance, with the kernel beside it.
    pub fn with_instance<R>(
        &mut self,
        tab: TabId,
        f: impl FnOnce(&mut dyn Instance, &mut Kernel) -> Result<R>,
    ) -> Result<R> {
        let mut instance =
            self.instances.remove(&tab).ok_or_else(|| Error::not_found(format!("tab %{tab} is not running")))?;
        let result = f(instance.as_mut(), self);
        if self.model.tabs.contains_key(&tab) {
            self.instances.insert(tab, instance);
        }
        result
    }
    pub fn instance<T: 'static>(&self, tab: TabId) -> Option<&T> {
        self.instances.get(&tab)?.as_any().downcast_ref()
    }
    pub(crate) fn set_instance(&mut self, tab: TabId, instance: Box<dyn Instance>) {
        self.instances.insert(tab, instance);
    }
    pub(crate) fn take_instance(&mut self, tab: TabId) -> Option<Box<dyn Instance>> {
        self.instances.remove(&tab)
    }
    /// The size of a tab nobody sized yet: `default_size`.
    pub fn default_size(&self) -> (u32, u32) {
        let text = self.global_option("default_size");
        let parse = |text: &str| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        };
        text.as_str().and_then(parse).unwrap_or((80, 24))
    }
    /// Gives a tab a size in cells, as whoever sizes it asked (`resize`), and the tabs behind it
    /// in its pane the same: whichever comes forward is the size its pane is.
    pub fn resize_pane(&mut self, tab: TabId, width: u32, height: u32) {
        let mut tabs = vec![tab];
        if let Some((view, pane)) = self.model.pane_of_tab(tab) {
            tabs.extend(self.model.views[&view].panes[&pane].tabs.iter().copied().filter(|t| *t != tab));
        }
        for tab in tabs {
            let _ = self.with_instance(tab, |instance, kernel| {
                instance.resize(kernel, tab, width, height);
                Ok(())
            });
            self.sized(tab, width, height);
        }
    }
    /// A tab has this size now (a terminal's PTY took it): `pane_width`, `pane_height` and
    /// `tab.changed {size}` say so.
    pub fn sized(&mut self, tab: TabId, width: u32, height: u32) {
        if !self.model.tabs.contains_key(&tab)
            || (self.tab_option(tab, "pane_width"), self.tab_option(tab, "pane_height"))
                == (json!(width), json!(height))
        {
            return;
        }
        let _ = self.set_option("pane_width", Key::Tab(tab), json!(width), "server", true);
        let _ = self.set_option("pane_height", Key::Tab(tab), json!(height), "server", true);
        self.changed(Kind::Tab, tab, json!({ "size": { "width": width, "height": height } }));
    }
    pub fn new_session_id(&mut self) -> SessionId {
        self.ids.session += 1;
        self.ids.session
    }
    pub fn new_view_id(&mut self) -> ViewId {
        self.ids.view += 1;
        self.ids.view
    }
    pub fn new_pane_id(&mut self) -> PaneId {
        self.ids.pane += 1;
        self.ids.pane
    }
    pub fn new_tab_id(&mut self) -> TabId {
        self.ids.tab += 1;
        self.ids.tab
    }
    /// A free name for a new session: the one asked for, else its folder's (`:` and `.`
    /// as `_`, as tmux does, and `-2`, `-3`, … while taken), else tmux's numbers from 0.
    /// A session with no folder is named for the home folder its terminals start in.
    pub fn session_name(&self, wanted: Option<String>, cwd: Option<&str>) -> Result<String> {
        let taken = |name: &str| self.model.sessions.values().any(|s| s.name == name);
        let home = pumi_proto::paths::home_dir();
        if wanted.is_none()
            && let Some(folder) = cwd
                .map(std::path::Path::new)
                .unwrap_or(&home)
                .file_name()
                .map(|name| name.to_string_lossy().replace([':', '.'], "_"))
                .filter(|name| !name.trim().is_empty())
        {
            let free = std::iter::once(folder.clone())
                .chain((2..).map(|n: u64| format!("{folder}-{n}")))
                .find(|name| !taken(name))
                .expect("a free name");
            return Ok(free);
        }
        match wanted {
            Some(name) if self.model.sessions.values().any(|s| s.name == name) => {
                Err(Error::invalid_params(format!("duplicate session: {name}")))
            }
            Some(name) if name.is_empty() || name.contains([':', '.']) => {
                Err(Error::invalid_params(format!("a session name cannot be empty or hold `:` or `.`: {name:?}")))
            }
            Some(name) => Ok(name),
            None => Ok((0..)
                .map(|n: u64| n.to_string())
                .find(|n| !self.model.sessions.values().any(|s| &s.name == n))
                .expect("a free name")),
        }
    }
    /// The lowest free index at or above `base_index`.
    pub fn free_index(&self, session: SessionId) -> u32 {
        let base = self
            .model
            .options
            .get("base_index", &[Key::Session(session), Key::Global])
            .and_then(|(v, _)| v.as_u64())
            .unwrap_or(0) as u32;
        let used: Vec<u32> = self.model.sessions[&session].views.iter().map(|(i, _)| *i).collect();
        (base..).find(|i| !used.contains(i)).expect("a free index")
    }
    /// Views whose last tiled tab went, and sessions whose last view went, close.
    /// Returns what closed.
    pub fn prune(&mut self) -> (Vec<ViewId>, Vec<SessionId>) {
        let mut views = Vec::new();
        let live: Vec<ViewId> = self.model.views.keys().copied().collect();
        for view in live {
            let tiled = self.model.views[&view].tabs().into_iter().any(
                |t| matches!(self.model.tabs.get(&t).map(|t| &t.place), Some(Place::Tiled { view: v }) if *v == view),
            );
            if !tiled {
                views.push(view);
            }
        }
        for view in &views {
            let held = self.model.session_of_view(*view);
            self.model.views.remove(view);
            self.model.options.drop_object(Key::View(*view));
            let mut changed = Vec::new();
            for session in self.model.sessions.values_mut() {
                if session.views.iter().any(|(_, t)| t == view) {
                    session.views.retain(|(_, t)| t != view);
                    let was = session.current;
                    if session.current == Some(*view) {
                        session.current = session
                            .last
                            .filter(|l| l != view && session.views.iter().any(|(_, t)| t == l))
                            .or_else(|| session.views.first().map(|(_, t)| *t));
                    }
                    if session.last == Some(*view) || session.last == session.current {
                        session.last = None;
                    }
                    if session.current != was && !session.views.is_empty() {
                        changed.push(session.id);
                    }
                }
            }
            self.panes_changed(*view);
            let id = Ref::local(Kind::View, *view).to_string();
            let session = held.map(|s| Ref::local(Kind::Session, s).to_string());
            self.emit("view.closed", &id, About::view(*view), json!({ "view": id, "session": session }));
            for session in changed {
                self.active_view_changed(session);
            }
        }
        let empty: Vec<SessionId> = self.model.sessions.values().filter(|s| s.views.is_empty()).map(|s| s.id).collect();
        for session in &empty {
            self.model.sessions.remove(session);
            self.model.options.drop_object(Key::Session(*session));
            let id = Ref::local(Kind::Session, *session).to_string();
            self.emit("session.closed", &id, About::session(*session), json!({ "session": id }));
            for chan in self.model.chans.values_mut() {
                if let Some(ui) = &mut chan.ui
                    && ui.session == *session
                {
                    ui.session = 0;
                }
            }
            if self.model.last_session == Some(*session) {
                self.model.last_session = None;
            }
        }
        if let Some(next) = self.model.sessions.values().max_by_key(|s| s.activity).map(|s| s.id) {
            self.adopt_clients(next);
        }
        (views, empty)
    }
    /// Clients showing no session (theirs closed, or there was none when they attached) show
    /// this one. Returns whether any did.
    pub fn adopt_clients(&mut self, session: SessionId) -> bool {
        let mut moved = Vec::new();
        for chan in self.model.chans.values_mut() {
            if let Some(ui) = &mut chan.ui
                && ui.session == 0
            {
                ui.session = session;
                moved.push(chan.id);
            }
        }
        for client in &moved {
            self.client_changed(*client, session);
        }
        !moved.is_empty()
    }
    pub fn touch_session(&mut self, session: SessionId) {
        let tick = self.tick();
        if let Some(s) = self.model.sessions.get_mut(&session) {
            s.activity = tick;
        }
        self.model.last_session = Some(session);
    }
    pub fn touch_tab(&mut self, tab: TabId) {
        let tick = self.tick();
        if let Some(p) = self.model.tabs.get_mut(&tab) {
            p.activity = tick;
        }
    }
    /// Sends a message to a channel; `false` when the channel is gone.
    pub fn send(&mut self, chan: ChanId, message: String) -> bool {
        if chan == SERVER || !self.model.chans.contains_key(&chan) {
            return false;
        }
        self.out.push((chan, message));
        true
    }
    /// The attach stream: every event, in order, to every client attached before it.
    pub fn send_redraw(&mut self, batch: &[Event]) {
        let clients: Vec<(ChanId, u64)> =
            self.model.chans.values().filter_map(|c| c.ui.as_ref().map(|ui| (c.id, ui.from_seq))).collect();
        for (chan, from) in clients {
            let events: Vec<&Event> = batch.iter().filter(|e| e.seq > from).collect();
            if !events.is_empty() {
                self.out.push((chan, pumi_api::rpc::notification("redraw", json!(events))));
            }
        }
    }
    pub fn tab_value(&self, id: TabId) -> Value {
        let p = &self.model.tabs[&id];
        let traits = crate::api::traits::of(self, id);
        json!({ "id": p.id, "block": p.block, "traits": traits, "place": p.place, "activity": p.activity, "options": self.local_options(Key::Tab(id)) })
    }
    pub fn local_options(&self, key: Key) -> Map<String, Value> {
        self.model.options.local(key).into_iter().map(|(name, set)| (name, set.value)).collect()
    }
    /// The whole model, as an attaching client is first sent it.
    pub fn dump(&self) -> Value {
        let mut options = Map::new();
        for key in self.model.options.keys() {
            if matches!(key, Key::Tab(_)) {
                continue;
            }
            let name = match key {
                Key::Global => "global".to_string(),
                Key::Session(id) => format!("session:{id}"),
                Key::View(id) => format!("view:{id}"),
                Key::Client(id) => format!("client:{id}"),
                Key::Tab(_) => unreachable!(),
            };
            options.insert(name, Value::Object(self.local_options(key)));
        }
        json!({
            "sessions": self.model.sessions.keys().map(|id| self.model.session_value(*id)).collect::<Vec<_>>(),
            "views": self.model.views.keys().map(|id| self.model.view_value(*id)).collect::<Vec<_>>(),
            "tabs": self.model.tabs.keys().map(|id| self.tab_value(*id)).collect::<Vec<_>>(),
            "options": options,
            "current": self.model.current(&Caller { channel: SERVER, ..Caller::default() }).map(|c| json!({ "session": c.session, "view": c.view, "tab": c.tab })),
        })
    }
}
impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kernel")
            .field("sessions", &self.model.sessions)
            .field("views", &self.model.views)
            .field("tabs", &self.model.tabs)
            .finish()
    }
}
