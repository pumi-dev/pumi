use pumi_api::{Caller, Registry};
use pumid::kernel::events::{Autocmd, flush};
use pumid::kernel::{Chan, Config, Kernel};
use pumid::tab::terminal::{Grids, Notes};
use serde_json::{Value, json};
const PAGE: &str = "pumi_dev.browser.page";
use std::sync::mpsc::channel;
const CHAN: u64 = 7;
const LISTEN: u64 = 8;
/// A tab's, view's or session's id, as the API spells it.
fn tab(n: u64) -> Value {
    json!(format!("%{n}"))
}
/// `core.openTab`'s answer, a number, as the API spells a tab.
fn id(opened: &Value) -> Value {
    tab(opened.as_u64().expect("a tab's number"))
}
fn view(n: u64) -> Value {
    json!(format!("@{n}"))
}
fn session(n: u64) -> Value {
    json!(format!("${n}"))
}
fn pane(n: u64) -> Value {
    json!(format!(":{n}"))
}
/// A layout's leaf, as `view.get` gives it: pane `n`, holding `tabs`, with the first in front.
fn leaf(n: u64, tabs: &[u64]) -> Value {
    json!({"pane": pane(n), "tabs": tabs.iter().map(|t| tab(*t)).collect::<Vec<_>>(), "active": tab(tabs[0])})
}
struct Fixture {
    registry: Registry,
    kernel: Kernel,
    caller: Caller,
}
impl Fixture {
    fn new() -> Fixture {
        let mut fixture = Fixture::empty();
        fixture.call("client.attach", json!([{"width": 80, "height": 24}]));
        fixture.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"kind": "session", "name": "main"}]));
        fixture.call("event.on", json!(["tab.closed", {}]));
        fixture
    }
    fn empty() -> Fixture {
        pumid::api::link();
        let mut kernel = Kernel::new(Config {
            name: "test".into(),
            data_dir: None,
            socket: "/nonexistent/pumi.sock".into(),
            bin_dir: None,
            base_env: std::env::vars().collect(),
        });
        pumid::tab::register(&mut kernel).unwrap();
        kernel.ext.insert(Grids::default());
        let (notes, receiver) = channel();
        std::mem::forget(receiver);
        kernel.ext.insert(Notes(notes));
        for id in [CHAN, LISTEN] {
            kernel.model.chans.insert(id, Chan { id, ..Chan::default() });
        }
        Fixture { registry: Registry::new(), kernel, caller: Caller { channel: CHAN, ..Caller::default() } }
    }
    fn try_call(&mut self, name: &str, args: Value) -> pumi_api::Result<Value> {
        let result = self.registry.call(&mut self.kernel, &self.caller, name, args);
        flush(&mut self.kernel);
        result
    }
    fn call(&mut self, name: &str, args: Value) -> Value {
        self.try_call(name, args.clone()).unwrap_or_else(|err| panic!("{name}({args}): {err:?}"))
    }
    fn listen(&mut self, events: &[&str]) -> u64 {
        let id = self.kernel.events.next_id();
        self.kernel.events.autocmds.push(Autocmd {
            id,
            events: events.iter().map(|e| e.to_string()).collect(),
            patterns: Vec::new(),
            desc: "test".into(),
            once: false,
            chan: LISTEN,
            source: "test".into(),
        });
        id
    }
    /// Makes `tab` an agent, as its hook would.
    fn agent(&mut self, tab: u64, agent: &str, session: &str, accepts: &[&str]) {
        self.call("core.setOptionValue", json!(["@agent", agent, {"tab": tab}]));
        self.call("core.setOptionValue", json!(["@agent.session", session, {"tab": tab}]));
        self.call("core.setOptionValue", json!(["@agent.accepts", accepts, {"tab": tab}]));
    }
    /// A source (a plugin) listening to `event` by name.
    fn listen_as(&mut self, source: &str, event: &str) -> u64 {
        let id = self.kernel.events.next_id();
        self.kernel.events.autocmds.push(Autocmd {
            id,
            events: vec![event.into()],
            patterns: Vec::new(),
            desc: "test".into(),
            once: false,
            chan: LISTEN,
            source: source.into(),
        });
        id
    }
    /// Calls as `source` (a plugin's calls name it).
    fn call_as(&mut self, source: &str, name: &str, args: Value) -> Value {
        let mut caller = self.caller.clone();
        caller.source = Some(source.into());
        let result = self.registry.call(&mut self.kernel, &caller, name, args.clone());
        flush(&mut self.kernel);
        result.unwrap_or_else(|err| panic!("{name}({args}): {err:?}"))
    }
    /// The replies sent to `chan`'s held call `id`.
    fn replies(&self, chan: u64, id: u64) -> Vec<Value> {
        self.kernel
            .out
            .iter()
            .filter(|(c, _)| *c == chan)
            .map(|(_, m)| serde_json::from_str::<Value>(m).unwrap())
            .filter(|m| m["id"] == id && m.get("method").is_none())
            .map(|m| m["result"].clone())
            .collect()
    }
    fn heard(&self, id: u64) -> Vec<Value> {
        self.kernel
            .out
            .iter()
            .filter(|(chan, _)| *chan == LISTEN)
            .map(|(_, m)| serde_json::from_str::<Value>(m).unwrap())
            .filter(|m| m["method"] == "rpc.event" && m["params"][0] == id)
            .map(|m| m["params"][1].clone())
            .collect()
    }
    fn option(&mut self, name: &str, tab: impl Into<Value>) -> Value {
        self.call("core.getOptionValue", json!([name, {"tab": tab.into()}]))
    }
    /// The browser plugin's page block, and its options.
    fn browser(&mut self) {
        self.call("core.setBlocks", json!([[{"id": PAGE, "provider": "pumi_dev/browser"}]]));
        for (name, ty, default) in [("url", "string", json!("about:blank")), ("zoom", "number", json!(1))] {
            let spec = json!({"type": ty, "default": default, "scope": "tab", "blocks": [PAGE], "desc": name});
            self.call("option.declare", json!([name, spec]));
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let tabs: Vec<u64> = self.kernel.model.tabs.keys().copied().collect();
        for tab in tabs {
            if let Some(t) = self.kernel.instance::<pumid::tab::terminal::Terminal>(tab) {
                t.term.kill();
            }
        }
    }
}
#[test]
fn every_core_function_is_described_and_its_examples_fit() {
    pumid::api::link();
    let functions = Registry::core_functions();
    assert!(functions.len() > 40, "the core's functions are linked: {}", functions.len());
    for function in functions {
        let info = function.info();
        info.validate().unwrap();
        function.check_examples().unwrap();
        assert!(
            info.name.split('.').all(
                |s| s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
            ),
            "{}: `noun.verb`, camelCase after the dot",
            info.name
        );
        assert!(!info.desc.contains("[`"), "{}: a description is plain text, not rustdoc links", info.name);
        let verb = info.name.rsplit('.').next().unwrap_or_default();
        if ["get", "list"].iter().any(|prefix| verb.starts_with(prefix)) || verb == "current" {
            assert_eq!(info.effect, pumi_api::Effect::Read, "{}: get and list functions only read", info.name);
        }
        assert_eq!(
            info.params_schema["additionalProperties"],
            json!(false),
            "{}: unknown arguments are errors",
            info.name
        );
        // An example says how a function is called, and fits its schema. What it answers is the
        // export's business (`cargo xtask api` calls every one): an example names the tabs, views
        // and requests a session has (`%3`, `+7`), which a bare fixture has not got.
        assert!(!info.examples.is_empty(), "{}: an example says how it is called", info.name);
    }
}
#[test]
fn a_session_without_a_name_is_named_for_its_folder() {
    let mut f = Fixture::empty();
    f.call("client.attach", json!([{"width": 80, "height": 24}]));
    let mut name = |place: Value| {
        let tab = f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, place]));
        let session = f.kernel.model.session_of_tab(tab.as_u64().unwrap()).unwrap();
        f.kernel.model.sessions[&session].name.clone()
    };
    assert_eq!(name(json!({"kind": "session", "cwd": "/tmp/my.app"})), "my_app");
    assert_eq!(name(json!({"kind": "session", "cwd": "/tmp/my.app/"})), "my_app-2");
    assert_eq!(name(json!({"kind": "session", "cwd": "/"})), "0");
    let home = pumi_proto::paths::home_dir();
    if let Some(home) = home.file_name() {
        let home = home.to_string_lossy().replace([':', '.'], "_");
        assert_eq!(name(json!({"kind": "session"})), home, "no folder is the home folder's name");
        assert_eq!(name(json!({"kind": "session"})), format!("{home}-2"));
    }
    assert_eq!(name(json!({"kind": "session", "name": "work", "cwd": "/tmp"})), "work");
}
#[test]
fn a_terminal_given_no_folder_starts_in_its_sessions() {
    let mut f = Fixture::empty();
    f.call("client.attach", json!([{"width": 80, "height": 24}]));
    let dir = std::env::temp_dir().canonicalize().unwrap().display().to_string();
    let session = f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"kind": "session", "cwd": dir}]));
    let session = f.kernel.model.session_of_tab(session.as_u64().unwrap()).unwrap();
    let path = |f: &mut Fixture, tab: Value| f.option("tab_current_path", tab);
    let split = f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": 1}]));
    assert_eq!(path(&mut f, split), json!(dir), "a split starts in the session's folder");
    let opened =
        f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"kind": "view", "session": session}]));
    assert_eq!(path(&mut f, opened.clone()), json!(dir), "and so does a new view");
    let home = pumi_proto::paths::home_dir().display().to_string();
    let given =
        f.call("core.openTab", json!(["terminal", {"command": ["cat"], "cwd": home}, {"split": "down", "beside": 1}]));
    assert_eq!(path(&mut f, given), json!(home), "a folder given wins");
    let in_view = f.kernel.model.tabs[&opened.as_u64().unwrap()].place.view().unwrap();
    f.call("core.setOptionValue", json!(["worktree", home, {"view": in_view}]));
    let worked =
        f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": opened}]));
    assert_eq!(path(&mut f, worked), json!(home), "a tab in a worktree keeps to it");
    let split = f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": 1}]));
    assert_eq!(path(&mut f, split), json!(dir), "and the session's other views to its folder");
}
#[test]
fn splitting_keeps_the_tree_and_whoever_shows_a_pane_sizes_it() {
    let mut f = Fixture::new();
    let right = f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": 1}]));
    assert_eq!(right, 2);
    assert_eq!(f.call("tab.get", json!([]))["id"], tab(2), "a new tab is current, as tmux's are");
    assert_eq!(
        f.call("view.get", json!([view(1)]))["layout"],
        json!({"axis": "row", "ratio": 0.5, "first": leaf(1, &[1]), "second": leaf(2, &[2])})
    );
    let width = |f: &mut Fixture, tab| f.option("pane_width", tab);
    assert_eq!(width(&mut f, 2), json!(80), "`default_size` until someone sizes it: the core lays nothing out");
    // A tab's size is `tab.update {size}` (the host): not a method of `tab.call`.
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "down", "size": "25%", "beside": 2}]));
    let layout = f.call("view.get", json!([view(1)]))["layout"].clone();
    assert_eq!(layout["second"]["axis"], json!("column"), "{layout}");
    assert_eq!(layout["second"]["second"], leaf(3, &[3]), "{layout}");
    assert_eq!(f.call("tab.list", json!([{"view": view(1)}])), json!([tab(1), tab(2), tab(3)]));
}
#[test]
fn closing_the_zoomed_panes_last_tab_gives_its_view_its_layout_back() {
    let mut f = Fixture::new();
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": 1}]));
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "down", "beside": 2}]));
    f.call("core.viewSetZoom", json!([1, true]));
    assert_eq!(f.call("view.get", json!([view(1)]))["zoom"], pane(3));
    f.call("tab.close", json!([tab(3)]));
    assert_eq!(f.call("view.get", json!([view(1)]))["zoom"], json!(null), "the next pane is not zoomed in its place");
}
#[test]
fn a_tab_opened_into_a_pane_comes_to_its_front_after_the_tab_there() {
    let mut f = Fixture::new();
    let seen = f.listen(&["pane.opened", "pane.changed"]);
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"pane": 1}]));
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"pane": 1, "focus": false}]));
    assert_eq!(
        f.call("view.get", json!([view(1)]))["layout"],
        json!({"pane": pane(1), "tabs": [tab(1), tab(2), tab(3)], "active": tab(2)}),
        "each goes after the tab in front; the one opened without the focus stays behind"
    );
    assert_eq!(f.call("view.get", json!([view(1)]))["activeTab"], tab(2), "the one opened behind it has no focus");
    let info = f.call("pane.get", json!([pane(1)]));
    assert_eq!(info["tabs"], json!([tab(1), tab(2), tab(3)]));
    assert_eq!((info["activeTab"].clone(), info["isFocused"].clone()), (tab(2), json!(true)));
    assert_eq!(f.call("tab.get", json!([tab(3)]))["place"]["pane"], pane(1));
    let events: Vec<Value> = f.heard(seen).iter().map(|e| e["event"].clone()).collect();
    assert!(events.iter().all(|e| e == "pane.changed"), "no pane opened: {events:?}");
}
#[test]
fn closing_the_front_tab_brings_the_one_before_it_forward() {
    let mut f = Fixture::new();
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"pane": 1}]));
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"pane": 1}]));
    f.call("tab.close", json!([tab(3)]));
    let info = f.call("pane.get", json!([pane(1)]));
    assert_eq!((info["tabs"].clone(), info["activeTab"].clone()), (json!([tab(1), tab(2)]), tab(2)));
    assert_eq!(f.call("view.get", json!([view(1)]))["activeTab"], tab(2), "the focus stays in the pane");
}
#[test]
fn a_tab_moves_along_its_strip_into_another_pane_and_out_into_its_own() {
    let mut f = Fixture::new();
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"pane": 1}]));
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": 2}]));
    f.call("core.tabSetPlace", json!([2, {"pane": 1, "index": 0}]));
    assert_eq!(f.call("pane.get", json!([pane(1)]))["tabs"], json!([tab(2), tab(1)]), "along its strip");
    f.call("core.tabSetPlace", json!([1, {"pane": 2}]));
    assert_eq!(f.call("pane.get", json!([pane(2)]))["tabs"], json!([tab(3), tab(1)]), "into the other pane");
    f.call("core.tabSetPlace", json!([2, {"pane": 2, "split": "left"}]));
    assert!(f.try_call("pane.get", json!([pane(1)])).is_err(), "its pane went with its last tab");
    let alone = f.try_call("core.tabSetPlace", json!([2, {"pane": 3, "split": "down"}]));
    assert!(alone.is_err(), "a tab alone in its pane cannot split it");
    assert_eq!(f.call("pane.list", json!([{"view": view(1)}])), json!([pane(3), pane(2)]));
}
#[test]
fn every_tab_of_a_pane_takes_its_size() {
    let mut f = Fixture::new();
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"pane": 1, "focus": false}]));
    f.call("core.tabCall", json!([1, "resize", {"width": 100, "height": 30}]));
    assert_eq!(f.option("pane_width", 2), json!(100), "the tab behind is the front tab's size");
    assert_eq!(f.option("pane_height", 2), json!(30));
}
#[test]
fn closing_the_last_tab_closes_its_view_and_session_and_keeps_its_state() {
    let mut f = Fixture::new();
    let seen = f.listen(&["tab.closed", "view.closed", "session.closed"]);
    f.call("tab.close", json!([tab(1)]));
    let names: Vec<Value> = f.heard(seen).iter().map(|e| e["event"].clone()).collect();
    assert_eq!(names, ["tab.closed", "view.closed", "session.closed"]);
    assert_eq!(f.call("session.list", json!([])), json!([]));
    let closed = f.call("tab.list", json!([{"closed": true}]));
    assert_eq!(closed[0]["state"]["block"], "terminal");
    assert_eq!(closed[0]["state"]["data"]["command"], json!(["cat"]));
    let state = closed[0]["state"].clone();
    let tab = f.call("core.openTab", json!(["terminal", {"state": state}, {"kind": "session"}]));
    assert_eq!(tab, 2, "ids are never reused");
    assert_eq!(f.call("session.list", json!([])), json!([session(2)]));
}
#[test]
fn options_inherit_and_name_the_option_that_changed() {
    let mut f = Fixture::new();
    f.browser();
    let seen = f.listen(&["session.changed", "tab.changed"]);
    f.call("channel.setInfo", json!(["init.lua", {}, "plugin", {}, {}]));
    f.call("core.setOptionValue", json!(["remain_on_exit", true, {"scope": "session"}]));
    assert_eq!(f.option("remain_on_exit", 1), json!(true), "a tab takes the session's");
    let decl = f
        .call("core.options", json!([]))
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "remain_on_exit")
        .cloned()
        .unwrap();
    assert_eq!(decl["scope"], "tab", "set higher, a tab takes the nearest one: {decl}");
    f.call("core.setOptionValue", json!(["@x", 42, {"tab": 1}]));
    assert_eq!(f.option("@x", 1), json!(42));
    assert!(f.try_call("core.setOptionValue", json!(["tab_pid", 1, {"tab": 1}])).is_err(), "read-only");
    assert!(f.try_call("core.setOptionValue", json!(["url", "x", {"tab": 1}])).is_err(), "a browser's option");
    let keys: Vec<Value> =
        f.heard(seen).iter().flat_map(|e| e["data"]["keys"].as_array().cloned().unwrap_or_default()).collect();
    assert!(keys.contains(&json!("options.remain_on_exit")) && keys.contains(&json!("options.@x")), "{keys:?}");
}
#[test]
fn moving_swapping_and_floating() {
    let mut f = Fixture::new();
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"split": "right", "beside": 1}]));
    f.call("core.tabSetPlace", json!([1, {"swap": 2}]));
    assert_eq!(
        f.call("view.get", json!([view(1)]))["layout"],
        json!({"axis": "row", "ratio": 0.5, "first": leaf(1, &[2]), "second": leaf(2, &[1])})
    );
    f.call("core.tabSetPlace", json!([2, {"kind": "view", "session": 1}]));
    assert_eq!(f.call("session.get", json!([session(1)]))["views"], json!([view(1), view(2)]));
    let float = f.call(
        "core.openTab",
        json!(["terminal", {"command": ["cat"]}, {"kind": "float", "view": 1, "width": 40, "height": 10, "row": 7, "col": 20}]),
    );
    assert_eq!(f.call("tab.get", json!([tab(float.as_u64().unwrap())]))["place"]["kind"], "float");
    assert!(f.call("tab.list", json!([{"view": view(1)}])).as_array().unwrap().contains(&tab(3)), "a float is listed");
    let refused = f.try_call("core.tabSetPlace", json!([float, {"kind": "detached"}]));
    assert!(refused.is_err(), "detached is not a place to ask for");
}
#[test]
fn a_view_is_named_and_its_name_is_in_the_state() {
    let mut f = Fixture::new();
    f.call("core.setOptionValue", json!(["title", "logs", {"tab": 1}]));
    assert_eq!(f.option("title", 1), json!("logs"));
    f.call("core.viewSetName", json!([1, "build"]));
    let state = f.call("server.getState", json!([]));
    assert_eq!(state["state"]["views"][0]["name"], "build", "{state:#}");
}
#[test]
fn a_plugin_block_opens_and_is_listed() {
    let mut f = Fixture::new();
    let names = |f: &mut Fixture| -> Vec<String> {
        let types = f.call("core.blocks", json!([]));
        types.as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect()
    };
    let spec =
        json!({"type": "number", "default": 3, "scope": "tab", "blocks": ["pumi_dev.queue.board"], "desc": "Rows."});
    f.call("option.declare", json!(["pumi_dev.queue.rows", spec]));
    let blocks =
        json!([{"id": "pumi_dev.queue.board", "provider": "pumi_dev/queue"}, {"id": "1password.vault.itemList"}]);
    f.call("core.setBlocks", json!([blocks]));
    let types = f.call("core.blocks", json!([]));
    let board = types.as_array().unwrap().iter().find(|t| t["id"] == "pumi_dev.queue.board").expect("listed");
    assert_eq!(board["options"], json!(["pumi_dev.queue.rows"]));
    assert_eq!((board["version"].clone(), board["strategy"].clone()), (json!(1), json!("exact")));
    assert!(names(&mut f).contains(&"1password.vault.itemList".to_string()));
    let tab = f
        .call("core.openTab", json!(["pumi_dev.queue.board", {"pumi_dev.queue.rows": 5, "x": 1}, {"kind": "session"}]));
    assert_eq!(f.call("tab.get", json!([id(&tab), {"state": true}]))["state"]["block"], "pumi_dev.queue.board");
    assert_eq!(f.option("pumi_dev.queue.rows", tab.clone()), json!(5), "an argument named like its option");
    assert_eq!(f.call("core.tabCall", json!([tab, "args"]))["x"], 1);
    let other = f.try_call("core.openTab", json!(["pumi_dev.hello.badge", {}, {"kind": "session"}]));
    assert!(other.is_err(), "a block no manifest named does not open");
    assert_eq!(board["provider"], "pumi_dev/queue");
    f.call("core.setBlocks", json!([[]]));
    assert!(!names(&mut f).iter().any(|n| n.contains('.')), "the plugins went, and their types");
    for bad in ["terminal", "hello.badge", "a.b.c.d", "Bad.a.view"] {
        assert!(f.try_call("core.setBlocks", json!([[{"id": bad}]])).is_err(), "{bad}");
    }
}
#[test]
fn a_tab_has_a_trait_while_its_option_is_set() {
    let mut f = Fixture::new();
    let ids = |f: &mut Fixture| -> Vec<String> {
        let list = f.call("core.traits", json!([]));
        list.as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(ids(&mut f), ["agent"]);
    f.call(
        "core.setTraits",
        json!([[{"id": "pumi_dev.files.file", "provider": "pumi_dev/files", "fields": ["path"]}]]),
    );
    assert_eq!(ids(&mut f), ["agent", "pumi_dev.files.file"]);
    assert_eq!(f.call("tab.list", json!([{"trait": "agent"}])), json!([]));
    f.call("core.setOptionValue", json!(["@agent", "codex", {"tab": 1}]));
    assert_eq!(f.call("tab.list", json!([{"trait": "agent"}])), json!([tab(1)]));
    let state = f.call("server.getState", json!([]));
    assert_eq!(state["state"]["tabs"][0]["traits"], json!(["agent"]));
    f.call("core.setOptionValue", json!(["@agent", "", {"tab": 1}]));
    assert_eq!(f.call("tab.list", json!([{"trait": "agent"}])), json!([]), "an empty value is none");
    assert!(f.try_call("core.setTraits", json!([[{"id": "file"}]])).is_err(), "a bare word is the core's");
    f.call("core.setTraits", json!([[]]));
    assert_eq!(ids(&mut f), ["agent"]);
}
#[test]
fn a_plugin_block_has_its_version_strategy_and_methods() {
    let mut f = Fixture::new();
    let page = json!({"id": "pumi_dev.browser.page", "version": 2, "strategy": "reference", "methods": ["navigate"]});
    f.call("core.setBlocks", json!([[page]]));
    let types = f.call("core.blocks", json!([]));
    let listed = types.as_array().unwrap().iter().find(|t| t["id"] == "pumi_dev.browser.page").expect("listed");
    assert_eq!((listed["version"].clone(), listed["strategy"].clone()), (json!(2), json!("reference")));
    assert_eq!(listed["methods"], json!(["args", "describe", "navigate"]));
    let tab = f.call("core.openTab", json!(["pumi_dev.browser.page", {"url": "a"}, {"kind": "session"}]));
    let state = f.call("tab.get", json!([id(&tab), {"state": true}]))["state"].clone();
    assert_eq!((state["block_version"].clone(), state["strategy"].clone()), (json!(2), json!("reference")));
    // Its own methods are its plugin's functions: the host calls `pumi_dev.browser.page.navigate`.
    let undeclared = f.try_call("core.tabCall", json!([tab, "navigate", {"url": "a"}])).unwrap_err();
    assert_eq!(undeclared.kind, pumi_api::ErrorKind::NotFound, "{undeclared:?}");
    let mut older = state.clone();
    older["block_version"] = json!(1);
    assert!(
        f.try_call("core.openTab", json!(["pumi_dev.browser.page", {"state": older}, {"kind": "session"}])).is_err()
    );
    f.call("core.setBlocks", json!([[]]));
    assert!(f.try_call("core.tabCall", json!([tab, "navigate"])).is_err(), "the plugin went, and its methods");
    for bad in
        [json!({"version": 0}), json!({"methods": ["read"]}), json!({"methods": ["report"]}), json!({"strategy": "x"})]
    {
        let mut ty = bad.clone();
        ty["id"] = json!("pumi_dev.browser.page");
        assert!(f.try_call("core.setBlocks", json!([[ty]])).is_err(), "{bad}");
    }
}
#[test]
fn a_file_tab_keeps_its_file_and_where_in_it_and_reads_it_without_a_renderer() {
    let mut f = Fixture::new();
    let dir = std::env::temp_dir().canonicalize().unwrap().join(format!("pumi-file-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.txt"), dir.join("b.rs"));
    std::fs::write(&a, "one\ntwo\n").unwrap();
    std::fs::write(&b, "fn main() {}\n").unwrap();
    let (a, b) = (a.display().to_string(), b.display().to_string());
    let relative = f.try_call("core.openTab", json!(["file", {"path": "a.txt"}, {"split": "right", "beside": 1}]));
    assert!(relative.unwrap_err().message.contains("absolute"), "the host resolves a relative path");
    let tab = f.call("core.openTab", json!(["file", {"path": a, "line": 2}, {"split": "right", "beside": 1}]));
    assert_eq!(f.option("path", tab.clone()), json!(a));
    assert_eq!(f.option("title", tab.clone()), json!("a.txt"));
    assert_eq!(f.option("tab_current_path", tab.clone()), json!(dir.display().to_string()));
    assert_eq!(f.call("core.tabCall", json!([tab, "at"])), json!({"path": a, "line": 2}));
    assert_eq!(f.call("core.tabCall", json!([tab, "read"])), json!({"text": "one\ntwo\n"}), "no renderer: the file");
    f.call("core.tabCall", json!([tab, "describe", {"text": "drawn"}]));
    assert_eq!(f.call("core.tabCall", json!([tab, "read"])), json!({"text": "drawn"}), "what its renderer says");
    f.call("core.tabCall", json!([tab, "reveal", {"path": b, "line": 1, "column": 4}]));
    // Its viewer's own (`save`, `action`) are functions of `<viewer>.<method>`.
    assert_eq!(f.try_call("core.tabCall", json!([tab, "save"])).unwrap_err().kind, pumi_api::ErrorKind::NotFound);
    assert_eq!(
        f.try_call("core.tabCall", json!([tab, "action", {"id": "x"}])).unwrap_err().kind,
        pumi_api::ErrorKind::NotFound
    );
    assert_eq!(f.option("path", tab.clone()), json!(b));
    assert_eq!(f.option("title", tab.clone()), json!("b.rs"));
    assert_eq!(f.call("core.tabCall", json!([tab, "read"])), json!({"text": "fn main() {}\n"}), "another file's text");
    f.call("core.setOptionValue", json!(["path", a, {"tab": tab}]));
    assert_eq!(f.call("core.tabCall", json!([tab, "at"])), json!({"path": a}), "`path` set: that file, from its top");
    let state = f.call("tab.get", json!([id(&tab), {"state": true}]))["state"].clone();
    assert_eq!(state["strategy"], "reference");
    let copy = f.call("core.openTab", json!(["file", {"state": state}, {"kind": "session"}]));
    assert_eq!(f.option("path", copy.clone()), json!(a), "restored");
    assert!(f.try_call("core.setOptionValue", json!(["path", a, {"tab": 1}])).is_err(), "a file tab's option");
    std::fs::remove_dir_all(&dir).unwrap();
}
#[test]
fn a_client_drawn_tab_keeps_what_it_reported_and_restores_exactly() {
    let mut f = Fixture::new();
    f.browser();
    let tab = f.call("core.openTab", json!([PAGE, {"url": "https://example.com"}, {"kind": "view", "session": 1}]));
    assert_eq!(f.option("url", tab.clone()), json!("https://example.com"), "an argument named like its option");
    f.call("core.setOptionValue", json!(["url", "https://example.com/b", {"tab": tab}]));
    let data = json!({"back": ["https://example.com"]});
    f.call("core.tabCall", json!([tab, "describe", {"text": "Page B", "selection": "B", "title": "B", "data": data}]));
    f.call("core.setOptionValue", json!(["zoom", 1.5, {"tab": tab}]));
    let state = f.call("tab.get", json!([id(&tab), {"state": true}]))["state"].clone();
    assert_eq!(state["strategy"], "exact");
    let copy = f.call("core.openTab", json!([PAGE, {"state": state}, {"kind": "view", "session": 1}]));
    for name in ["url", "zoom", "title"] {
        let a = f.option(name, tab.clone());
        assert_eq!(f.option(name, copy.clone()), a, "{name}");
    }
    assert_eq!(f.call("core.tabCall", json!([copy, "read"])), json!({"text": "Page B"}));
    assert_eq!(f.call("core.tabCall", json!([copy, "selection"])), json!({"text": "B"}));
    assert_eq!(f.call("tab.get", json!([id(&copy), {"state": true}]))["state"]["data"]["data"], data);
}
#[test]
fn an_older_state_is_refused() {
    let mut f = Fixture::new();
    let mut state = f.call("tab.get", json!([tab(1), {"state": true}]))["state"].clone();
    state["block_version"] = json!(0);
    let err = f.try_call("core.openTab", json!(["terminal", {"state": state}, {"kind": "session"}])).unwrap_err();
    assert!(err.message.contains("version 0"), "{}", err.message);
}
#[test]
fn chan_autocmds_queue_notifications_for_their_channel() {
    let mut f = Fixture::new();
    f.call("event.on", json!(["session.changed", {}]));
    f.kernel.out.clear();
    f.call("session.update", json!([session(1), {"name": "work"}]));
    let sent: Vec<Value> =
        f.kernel.out.iter().filter(|(chan, _)| *chan == CHAN).map(|(_, m)| serde_json::from_str(m).unwrap()).collect();
    let methods: Vec<&str> = sent.iter().map(|m| m["method"].as_str().unwrap()).collect();
    assert_eq!(methods, ["redraw", "rpc.event"], "an attached client gets the change in its stream too");
    assert_eq!(sent[1]["params"][1]["event"], "session.changed");
    assert_eq!(sent[1]["params"][1]["match"], session(1), "matched by the session's id");
    assert_eq!(sent[1]["params"][1]["data"]["patch"]["name"], "work");
}
#[test]
fn a_turn_marks_the_agent_and_with_nobody_listening_tells_it_nothing() {
    let mut f = Fixture::new();
    let turned = f.listen(&["agent.turned"]);
    let turn = json!({"agent": "claude", "event": "start", "session": "s-1", "model": "opus", "transcript": "/t.jsonl", "accepts": ["queue"]});
    assert_eq!(f.call("agent.turn", json!([tab(1), turn])), json!({"context": null, "from": [], "late": []}));
    assert_eq!(f.option("@agent", 1), "claude");
    assert_eq!(f.option("@agent.session", 1), "s-1");
    assert_eq!(f.option("@agent.model", 1), "opus");
    assert_eq!(f.option("@agent.transcript", 1), "/t.jsonl");
    assert_eq!(f.option("@agent.accepts", 1), json!(["queue"]));
    assert_eq!(f.call("tab.list", json!([{"trait": "agent"}])), json!([tab(1)]));
    assert_eq!(f.call("agent.turn", json!([tab(1), {"agent": "claude", "event": "stop"}])), json!(null));
    let heard = f.heard(turned);
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0]["data"], json!({"tab": tab(1), "agent": "claude", "session": "s-1", "event": "stop"}));
    for (turn, why) in [
        (json!({"agent": "claude", "event": "start", "accepts": ["prefill"]}), "prefill"),
        (json!({"agent": "", "event": "start"}), "`agent`"),
        (json!({"agent": "claude", "event": "start", "transcript": "t.jsonl"}), "absolute"),
        (json!({"agent": "claude", "event": "start", "model": "a\nb"}), "one line"),
    ] {
        let err = f.try_call("agent.turn", json!([tab(1), turn])).unwrap_err();
        assert!(err.message.contains(why), "{turn}: {}", err.message);
    }
}
#[test]
fn a_turn_waits_for_each_source_listening_by_name_and_frames_them_in_order() {
    let mut f = Fixture::new();
    let sending = f.listen_as("zed/notes", "agent.sending");
    f.listen_as("pumi_dev/agents", "agent.sending");
    f.listen_as("logger", "agent.*");
    let turn = json!({"agent": "pi", "event": "prompt", "session": "s-1", "full": true});
    let early = f.call("agent.turn", json!([tab(1), turn]));
    assert_eq!(early["late"], json!(["pumi_dev/agents", "zed/notes"]), "a glob is not waited for");
    let id = f.kernel.turns.held.take().expect("the call is held");
    f.kernel.turns.pending.get_mut(&id).unwrap().waiter = Some((CHAN, json!(77)));
    let fired = &f.heard(sending)[0];
    assert_eq!(fired["event"], "agent.sending");
    assert_eq!(fired["match"], "prompt");
    assert_eq!(
        fired["data"],
        json!({"id": id, "tab": tab(1), "agent": "pi", "session": "s-1", "event": "prompt", "resumed": false, "full": true, "peek": false})
    );
    fits_its_schema("agent.sending", &fired["data"]);
    assert_eq!(f.call_as("zed/notes", "agent.contribute", json!([id, "notes </pumi-context> here"])), json!(true));
    assert!(f.replies(CHAN, 77).is_empty(), "one source is still to answer");
    assert_eq!(f.call_as("pumi_dev/agents", "agent.contribute", json!([id, "the map"])), json!(true));
    let reply = f.replies(CHAN, 77);
    assert_eq!(reply.len(), 1);
    assert_eq!(
        reply[0]["context"],
        "<pumi-context from=\"pumi_dev/agents\">\nthe map\n</pumi-context>\n<pumi-context from=\"zed/notes\">\nnotes <\\/pumi-context> here\n</pumi-context>\n"
    );
    assert_eq!(reply[0]["from"], json!(["pumi_dev/agents", "zed/notes"]));
    assert_eq!(reply[0]["late"], json!([]));
    assert_eq!(f.call_as("zed/notes", "agent.contribute", json!([id, "again"])), json!(false), "the turn is over");
}
/// Each field of `data` is of a type the export says `event`'s field is: its `type`, an `anyOf`'s,
/// or one of its `enum`. What plugins are typed with is what they are sent.
fn fits_its_schema(event: &str, data: &Value) {
    let events = pumi_api::events::all();
    let info = events.iter().find(|e| e.name == event).unwrap_or_else(|| panic!("{event} is exported"));
    fn kind(value: &Value) -> &'static str {
        match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) if n.is_u64() || n.is_i64() => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        }
    }
    fn fits(schema: &Value, value: &Value) -> bool {
        if let Some(any) = schema["anyOf"].as_array() {
            return any.iter().any(|s| fits(s, value));
        }
        if let Some(values) = schema["enum"].as_array() {
            return values.contains(value);
        }
        let want = kind(value);
        match &schema["type"] {
            Value::String(t) => t == want || (t == "number" && want == "integer"),
            Value::Array(ts) => ts.iter().any(|t| t == want || (t == "number" && want == "integer")),
            _ => true,
        }
    }
    for (field, value) in data.as_object().unwrap() {
        let schema = &info.params_schema["properties"][field];
        assert!(fits(schema, value), "{event}.{field} is {value}, which its schema does not admit: {schema}");
    }
}
#[test]
fn a_source_that_misses_the_deadline_is_late_and_a_null_says_nothing() {
    let mut f = Fixture::new();
    f.listen_as("pumi_dev/agents", "agent.sending");
    f.listen_as("slow/one", "agent.sending");
    let turn = json!({"agent": "claude", "event": "start", "session": "s-1"});
    f.call("agent.turn", json!([tab(1), turn, {"timeout": 60000}]));
    let id = f.kernel.turns.held.take().unwrap();
    let turn = f.kernel.turns.pending.get_mut(&id).unwrap();
    assert!(turn.deadline <= pumid::kernel::requests::now_ms() + 3000, "at most three seconds");
    turn.waiter = Some((CHAN, json!(5)));
    f.call_as("pumi_dev/agents", "agent.contribute", json!([id, null]));
    pumid::kernel::turns::tick(&mut f.kernel, pumid::kernel::requests::now_ms() + 3001);
    assert_eq!(f.replies(CHAN, 5), [json!({"context": null, "from": [], "late": ["slow/one"]})]);
    assert!(f.kernel.turns.is_empty());
}
#[test]
fn a_peek_asks_without_changing_the_tab() {
    let mut f = Fixture::new();
    let sending = f.listen(&["agent.sending"]);
    f.call("agent.turn", json!([tab(1), {"agent": "claude", "event": "prompt", "session": "s-1", "peek": true}]));
    assert_eq!(f.option("@agent", 1), json!(null), "a peek marks nothing");
    let fired = f.heard(sending);
    assert_eq!(fired[0]["data"]["peek"], json!(true));
}
#[test]
fn a_queue_is_refused_where_it_cannot_be_delivered() {
    let mut f = Fixture::new();
    let err = f.try_call("agent.queue", json!([tab(1), "hi"])).unwrap_err();
    assert!(err.message.contains("not an agent"), "{}", err.message);
    f.agent(1, "codex", "s-1", &[]);
    let err = f.try_call("agent.queue", json!([tab(1), "hi"])).unwrap_err();
    assert!(err.message.contains("no `queue`"), "{}", err.message);
    f.agent(1, "claude", "s-1", &["queue"]);
    for (args, why) in [(json!([tab(1), "  "]), "some text"), (json!([tab(1), "x".repeat(64 * 1024 + 1)]), "64 KiB")] {
        let err = f.try_call("agent.queue", args).unwrap_err();
        assert!(err.message.contains(why), "{}", err.message);
    }
    for _ in 0..100 {
        f.call("agent.queue", json!([tab(1), "hi"]));
    }
    assert!(f.try_call("agent.queue", json!([tab(1), "hi"])).unwrap_err().message.contains("100"));
}
#[test]
fn a_queued_message_is_taken_once_oldest_first_by_its_session() {
    let mut f = Fixture::new();
    let changes = f.listen(&["tab.changed"]);
    f.agent(1, "claude", "s-1", &["queue"]);
    f.call("agent.queue", json!([tab(1), "first"]));
    f.call_as("pumi_dev/marmaide", "agent.queue", json!([tab(1), "second"]));
    let queue = |f: &mut Fixture| f.call("tab.get", json!([tab(1)]))["traits"]["agent"]["queue"].clone();
    let listed = queue(&mut f);
    assert_eq!(listed.as_array().unwrap().iter().map(|i| i["text"].clone()).collect::<Vec<_>>(), ["first", "second"]);
    assert_eq!(listed[0]["from"], "chan 7");
    assert_eq!(listed[1]["from"], "pumi_dev/marmaide");
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-2"}])), json!(null), "another session's");
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-1"}]))["text"], "first");
    let second = f.call("agent.take", json!([tab(1), {"session": "s-1"}]));
    assert_eq!(second["text"], "second");
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-1"}])), json!(null), "at most once");
    let third = f.call("agent.queue", json!([tab(1), "third"]));
    assert_eq!(f.call("agent.unqueue", json!([third])), json!(true));
    assert_eq!(f.call("agent.unqueue", json!([third])), json!(false));
    assert_eq!(queue(&mut f), json!([]));
    let keys: Vec<Value> =
        f.heard(changes).iter().flat_map(|e| e["data"]["keys"].as_array().cloned().unwrap_or_default()).collect();
    assert!(
        keys.iter().filter(|k| *k == "traits.agent").count() >= 6,
        "each queue and take says the agent's trait changed: {keys:?}"
    );
}
#[test]
fn a_new_session_or_a_closed_tab_takes_its_messages_away() {
    let mut f = Fixture::new();
    f.agent(1, "claude", "s-1", &["queue"]);
    f.call("agent.queue", json!([tab(1), "old"]));
    f.call("agent.queue", json!([tab(1), "for the next", {"session": "s-2"}]));
    f.call("agent.turn", json!([tab(1), {"agent": "claude", "event": "start", "session": "s-2"}]));
    let left = f.call("tab.get", json!([tab(1)]))["traits"]["agent"]["queue"].clone();
    assert_eq!(left.as_array().unwrap().len(), 1, "the other session's went with it");
    assert_eq!(left[0]["text"], "for the next");
    f.call("core.openTab", json!(["terminal", {"command": ["cat"]}, {"kind": "view", "session": 1}]));
    f.call("tab.close", json!([tab(1)]));
    assert!(f.kernel.queue.items.is_empty(), "a closed tab's messages went with it");
}
#[test]
fn a_tab_waiting_on_the_user_holds_its_messages() {
    let mut f = Fixture::new();
    f.listen(&["request.opened"]);
    f.agent(1, "claude", "s-1", &["queue"]);
    f.call("agent.queue", json!([tab(1), "later"]));
    let request = f.call("request.open", json!([{"kind": "question", "title": "Which?"}, {"tab": tab(1)}]));
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-1"}])), json!(null), "a request is pending");
    f.call("request.close", json!([request, {}]));
    f.call("core.setOptionValue", json!(["@agent.status", "error", {"tab": 1}]));
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-1"}])), json!(null), "an error not yet seen");
    f.call("core.setOptionValue", json!(["@agent.status", "idle", {"tab": 1}]));
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-1"}]))["text"], "later");
}
#[test]
fn a_waiting_take_is_answered_when_a_message_comes_and_goes_with_its_caller() {
    let mut f = Fixture::new();
    f.agent(1, "pi", "s-1", &["queue", "wake"]);
    assert_eq!(f.call("agent.take", json!([tab(1), {"session": "s-1", "wait": true}])), json!(null));
    let taker = f.kernel.queue.held.take().expect("held");
    f.kernel.queue.takers.get_mut(&taker).unwrap().waiter = Some((CHAN, json!(9)));
    f.call("core.setOptionValue", json!(["@agent.status", "blocked", {"tab": 1}]));
    f.call("agent.queue", json!([tab(1), "wake up"]));
    assert!(f.replies(CHAN, 9).is_empty(), "held while blocked");
    f.call("core.setOptionValue", json!(["@agent.status", "idle", {"tab": 1}]));
    let reply = f.replies(CHAN, 9);
    assert_eq!(reply.len(), 1);
    assert_eq!(reply[0]["text"], "wake up");
    let queue = |f: &mut Fixture| f.call("tab.get", json!([tab(1)]))["traits"]["agent"]["queue"].clone();
    assert_eq!(queue(&mut f), json!([]));
    f.call("agent.take", json!([tab(1), {"session": "s-1", "wait": true}]));
    let taker = f.kernel.queue.held.take().unwrap();
    f.kernel.queue.takers.get_mut(&taker).unwrap().waiter = Some((LISTEN, json!(10)));
    pumid::kernel::queue::chan_closed(&mut f.kernel, LISTEN);
    f.call("agent.queue", json!([tab(1), "kept"]));
    assert_eq!(queue(&mut f)[0]["text"], "kept", "nothing is taken for a caller that went");
}
