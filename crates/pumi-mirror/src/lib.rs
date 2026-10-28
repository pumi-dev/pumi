pub mod events;
pub use pumi_layout as layout;
pub mod options;
use events::{About, Events};
use layout::Rect;
use options::{Key, Options};
use pumi_api::{Caller, Error, Result};
/// The channel of calls the host makes itself (startup, events, timers).
pub use pumi_model::SERVER;
pub use pumi_model::objects;
pub use pumi_model::state;
pub use pumi_model::{
    Chan, ChanId, ClientId, ClientInfo, Current, Float, Model, Pane, PaneId, Place, Session, SessionId, Tab, TabId, Ui,
    View, ViewId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
/// The channel Lua's calls are made on, in the host.
pub const LUA: ChanId = u64::MAX;
static NAME: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
/// Names the process in [`log`] lines (`pumi-host`).
pub fn set_name(name: &'static str) {
    let _ = NAME.set(name);
}
/// One line in the log (stderr, which the core points at its own log).
pub fn log(message: &str) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    eprintln!("[{now}] {}: {message}", NAME.get().copied().unwrap_or("pumi"));
}
/// A message for the user (`display-message`, an error in a hook): `show-messages` lists them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Message {
    pub at: u64,
    pub level: String,
    pub text: String,
}
/// How many messages are kept.
pub const MESSAGE_LIMIT: usize = 1000;
/// What the host knows about itself.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The host's name, as other servers know it.
    pub name: String,
    /// The core's socket: `$PUMI`.
    pub socket: PathBuf,
    /// The core's data directory.
    pub data: PathBuf,
    /// The environment jobs (`run-shell`) start from: the login shell's.
    pub base_env: BTreeMap<String, String>,
    /// Where the `pumi` CLI is; prepended to jobs' `PATH`.
    pub bin_dir: Option<PathBuf>,
}
pub struct Kernel {
    pub config: Config,
    /// Sessions, views, tabs, channels and options, as the core has them.
    pub model: Model,
    pub events: Events,
    /// Messages for channels (and for Lua, [`LUA`]), sent after each turn.
    pub out: Vec<(ChanId, String)>,
    /// `server.kill` reached the host: it stops after this turn.
    pub stop: bool,
    /// The connection to the core; `None` in tests that run the host's code alone.
    pub core: Option<Arc<pumi_api::channel::Core>>,
    /// The core's functions that only read: calling one does not move the mirror.
    pub reads: HashSet<String>,
    /// The mirror may be behind the core: it asks for what moved before it is next used.
    pub stale: bool,
    /// The last of the core's events whose change the model has (`core.model`'s `events`): one
    /// up to here moves nothing.
    pub events_seen: u64,
    /// The last of the core's events heard ([`Kernel::mirror_event`]). A read of the model can
    /// be ahead of it: what moved is in the model while its event is still on its way.
    pub events_heard: u64,
    /// Values the server and the host's modules keep beside the model, by type.
    pub ext: anymap::Ext,
    /// Messages for the user, oldest first.
    pub messages: std::collections::VecDeque<Message>,
    /// When the process started (`start_time`).
    pub started: u64,
    /// The channel whose request is being answered.
    pub rpc: Option<ChanId>,
    /// A command queue or a Lua task (pumi-host) that answers that request when it ends, not
    /// now.
    pub defer: Option<u64>,
    pub defer_lua: Option<u64>,
    /// Answers a call that reaches the channel while it waits for one of its own (see
    /// [`pumi_api::channel::Core::call_serving`]); without it such calls wait their turn.
    pub serve: Option<Serve>,
}
/// How a channel answers a call the core passed on: it writes the reply itself.
pub type Serve = std::rc::Rc<dyn Fn(&mut Kernel, String)>;
/// Values kept beside the model, by type.
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
        /// The value of type `T`, made with its default the first time.
        pub fn get_or_default<T: Any + Default>(&mut self) -> &mut T {
            let value = self.0.entry(TypeId::of::<T>()).or_insert_with(|| Box::new(T::default()));
            value.downcast_mut().expect("an Ext entry has its key's type")
        }
    }
}
impl Kernel {
    pub fn new(config: Config) -> Kernel {
        Kernel {
            config,
            model: Model { options: Options::new(options::builtin()), ..Model::default() },
            events: Events::default(),
            out: Vec::new(),
            stop: false,
            core: None,
            reads: HashSet::new(),
            stale: false,
            events_seen: 0,
            events_heard: 0,
            ext: anymap::Ext::default(),
            messages: Default::default(),
            started: state::epoch(),
            rpc: None,
            defer: None,
            defer_lua: None,
            serve: None,
        }
    }
    pub fn message(&mut self, level: &str, text: impl Into<String>) {
        self.messages.push_back(Message { at: state::epoch(), level: level.into(), text: text.into() });
        while self.messages.len() > MESSAGE_LIMIT {
            self.messages.pop_front();
        }
    }
    pub fn core(&self) -> Result<Arc<pumi_api::channel::Core>> {
        self.core.clone().ok_or_else(|| Error::new(pumi_api::ErrorKind::ProviderUnavailable, "no core"))
    }
    /// Who the host acts for in a call it passes to the core.
    pub fn on_behalf(&self, caller: &Caller) -> Option<Value> {
        let tab = caller.tab.or_else(|| self.model.current(caller).map(|c| c.tab));
        let chan = (caller.channel != SERVER && caller.channel != crate::LUA).then_some(caller.channel);
        if chan.is_none() && tab.is_none() && caller.source.is_none() {
            return None;
        }
        Some(json!({ "chan": chan, "tab": tab, "name": caller.source.clone() }))
    }
    /// Calls a function the core has (or routes), as `caller`; a change brings the mirror up
    /// to date before this returns.
    pub fn call_core(&mut self, caller: &Caller, name: &str, args: Value) -> Result<Value> {
        let core = self.core()?;
        let behalf = self.on_behalf(caller);
        let result = match self.serve.clone() {
            Some(serve) => core.call_serving(name, args, behalf, &mut |line| serve(self, line)),
            None => core.call(name, args, behalf),
        };
        if result.is_ok() && !self.reads.contains(name) {
            self.stale = true;
            self.sync();
        }
        result
    }
    /// Brings the mirror up to the core if it may have moved: the core's whole model
    /// (`core.model`), read again.
    pub fn sync(&mut self) {
        if !self.stale {
            return;
        }
        let Some(core) = self.core.clone() else { return };
        let Ok(mut answer) = core.call("core.model", json!([]), None) else { return };
        let Ok(model) = serde_json::from_value::<Model>(answer["model"].take()) else { return };
        self.events_seen = self.events_seen.max(answer["events"].as_u64().unwrap_or(0));
        let lua = self.model.chans.remove(&crate::LUA);
        self.model = model;
        if let Some(lua) = lua {
            self.model.chans.insert(crate::LUA, lua);
        }
        self.stale = false;
    }
    /// An event from the core: what it moved is in the core's model, read again before the
    /// mirror is used, unless the mirror has it already.
    pub fn mirror_event(&mut self, event: &events::Event) {
        self.events_heard = self.events_heard.max(event.seq);
        match event.event.as_str() {
            // Internal, or one of the host's own: it moved nothing in the model.
            "terminal.osc" | "plugin.reloadRequested" => {}
            // A plugin's own event, `<owner>.<name>.<event>`: it moves nothing in the model.
            name if name.split('.').count() > 2 => {}
            _ => self.stale |= event.seq > self.events_seen,
        }
    }
    /// Who set something, for provenance.
    pub fn source(&self, caller: &Caller) -> String {
        self.model.source(caller, "host")
    }
    pub fn tab_option(&self, tab: TabId, name: &str) -> Value {
        self.model.options.get(name, &self.model.chain(Some(tab), None, None)).map(|(v, _)| v).unwrap_or(Value::Null)
    }
    pub fn global_option(&self, name: &str) -> Value {
        self.model.options.get(name, &[Key::Global]).map(|(v, _)| v).unwrap_or(Value::Null)
    }
    /// The cells a view is laid out in: the size of the client that used a session holding it
    /// last, else `default_size` (tmux's `window-size latest`).
    pub fn view_area(&self, view: ViewId) -> Rect {
        let client = self
            .model
            .chans
            .values()
            .filter_map(|c| c.ui.as_ref())
            .filter(|ui| self.model.sessions.get(&ui.session).is_some_and(|s| s.views.iter().any(|(_, t)| *t == view)))
            .max_by_key(|ui| self.model.sessions[&ui.session].activity);
        let parse = |text: &str| {
            let (w, h) = text.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        };
        let (width, height) = match client {
            Some(ui) => (ui.width, ui.height),
            None => self.global_option("default_size").as_str().and_then(parse).unwrap_or((80, 24)),
        };
        Rect { x: 0, y: 0, width: width.max(1), height: height.max(1) }
    }
    /// Fires an event of the host's: the core passes it to every channel subscribed, the host
    /// included.
    pub fn emit(&mut self, event: &str, matched: &str, about: About, data: Value) {
        if let Some(core) = &self.core {
            let opts = json!({ "match": matched, "data": data, "tab": about.tab.map(|t| format!("%{t}")) });
            core.notify("event.emit", json!([event, opts]));
        }
    }
    /// Sets an option through the core (the host as the caller).
    pub fn set_option(&mut self, name: &str, key: Key, value: Value, source: &str, _internal: bool) -> Result<bool> {
        let value = options::coerce(&self.model.options, name, value)?;
        if self.core.is_none() {
            return Ok(self.model.options.set(&options::normalise(name), key, value, source, true)?.is_some());
        }
        let opts = key_opts(key);
        let caller = Caller { channel: SERVER, source: Some(source.to_string()), ..Caller::default() };
        self.call_core(&caller, "core.setOptionValue", json!([name, value, opts])).map(|_| true)
    }
    pub fn unset_option(&mut self, name: &str, key: Key, source: &str) {
        let _ = self.set_option(name, key, Value::Null, source, false);
    }
    /// Where each pane of a view is in its area (`view_area`): its layout's cells, the active
    /// pane alone when zoomed.
    pub fn pane_rects(&self, view: ViewId) -> Vec<(PaneId, Rect)> {
        let Some(v) = self.model.views.get(&view) else { return Vec::new() };
        let area = self.view_area(view);
        match v.active_pane().filter(|_| v.zoomed) {
            Some(pane) => vec![(pane, area)],
            None => v.layout.rects(area),
        }
    }
    /// Where each tab of a view is in its area (`view_area`): its pane's cells, whether it is in
    /// front or behind, and the floats over it.
    pub fn view_rects(&self, view: ViewId) -> Vec<(TabId, Rect)> {
        let Some(v) = self.model.views.get(&view) else { return Vec::new() };
        let area = self.view_area(view);
        let mut rects: Vec<(TabId, Rect)> = self
            .pane_rects(view)
            .into_iter()
            .filter_map(|(pane, rect)| Some((v.panes.get(&pane)?, rect)))
            .flat_map(|(p, rect)| p.tabs.iter().map(move |t| (*t, rect)))
            .collect();
        rects.extend(self.model.tabs.values().filter_map(|t| match &t.place {
            Place::Float { view: at, float } if *at == view => Some((t.id, float.rect(area))),
            _ => None,
        }));
        rects
    }
    /// Sends a message to a channel (through the core), or to Lua.
    pub fn send(&mut self, chan: ChanId, message: String) -> bool {
        if chan == SERVER {
            return false;
        }
        if chan != crate::LUA && !self.model.chans.contains_key(&chan) && self.core.is_some() {
            if !message.contains("\"method\"") {
                self.out.push((chan, message));
                return true;
            }
            return false;
        }
        self.out.push((chan, message));
        true
    }
    pub fn tab_value(&self, id: TabId) -> Value {
        let t = &self.model.tabs[&id];
        json!({ "id": t.id, "block": t.block, "place": t.place, "options": self.local_options(Key::Tab(id)) })
    }
    pub fn local_options(&self, key: Key) -> Map<String, Value> {
        self.model.options.local(key).into_iter().map(|(name, set)| (name, set.value)).collect()
    }
}
/// The `opts` of an option call that names one object.
pub fn key_opts(key: Key) -> Value {
    match key {
        Key::Global => json!({ "scope": "global" }),
        Key::Session(id) => json!({ "scope": "session", "session": id }),
        Key::View(id) => json!({ "scope": "view", "view": id }),
        Key::Tab(id) => json!({ "scope": "tab", "tab": id }),
        Key::Client(id) => json!({ "scope": "client", "client": id }),
    }
}
impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kernel").field("sessions", &self.model.sessions).field("tabs", &self.model.tabs).finish()
    }
}
