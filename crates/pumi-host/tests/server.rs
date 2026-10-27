use pumi_sys::ipc::Stream;
use pumi_sys::pty::Signal;
use serde_json::{Map, Value, json};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
/// A plugin block with the generic engine, declared as the browser plugin's manifest would.
const PAGE: &str = "pumi_dev.browser.page";
struct Server {
    dir: PathBuf,
    child: Option<Child>,
}
fn core_binary() -> PathBuf {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let host = PathBuf::from(env!("CARGO_BIN_EXE_pumi-host"));
    let core = host.with_file_name(pumi_sys::paths::exe("pumid"));
    BUILT.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut build = Command::new(cargo);
        build.args(["build", "-p", "pumid"]);
        if !cfg!(debug_assertions) {
            build.arg("--release");
        }
        let status = build.status().expect("cargo builds the core");
        assert!(status.success(), "building the core failed");
    });
    core
}
impl Server {
    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new().prefix("p").tempdir_in("/tmp").unwrap()
    }
    fn start(dir: &Path) -> Server {
        Server::spawn(dir, &["--clean", "-u", "NONE"], false)
    }
    fn start_home(dir: &Path) -> Server {
        Server::spawn(dir, &[], true)
    }
    fn spawn(dir: &Path, extra: &[&str], home: bool) -> Server {
        Server::spawn_from(None, dir, extra, home)
    }
    fn start_installed(bin: &Path, dir: &Path) -> Server {
        Server::spawn_from(Some(bin), dir, &["--clean", "-u", "NONE"], false)
    }
    fn spawn_from(bin: Option<&Path>, dir: &Path, extra: &[&str], home: bool) -> Server {
        Server::spawn_with(bin, dir, extra, home, &[])
    }
    fn spawn_with(bin: Option<&Path>, dir: &Path, extra: &[&str], home: bool, env: &[(&str, &Path)]) -> Server {
        let log = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("test.log")).unwrap();
        let mut command = match bin {
            Some(bin) => {
                let mut command = Command::new(bin.join(pumi_sys::paths::exe("pumid")));
                command.env_remove("PUMI_HOST");
                command
            }
            None => {
                let mut command = Command::new(core_binary());
                command.env("PUMI_HOST", env!("CARGO_BIN_EXE_pumi-host"));
                command
            }
        };
        command.env("PUMI_RUNTIME", Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime"));
        command
            .args(["--data-dir", dir.to_str().unwrap(), "--stay"])
            .args(extra)
            .env("PUMI_LOGIN_ENV", "0")
            .env("PUMI_APPNAME", "pumi")
            .env("SHELL", "/bin/sh")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log);
        if home {
            for (var, sub) in
                [("XDG_CONFIG_HOME", "c"), ("XDG_DATA_HOME", "d"), ("XDG_STATE_HOME", "s"), ("XDG_CACHE_HOME", "k")]
            {
                command.env(var, dir.join(sub));
            }
        }
        command.envs(env.iter().copied());
        let child = command.spawn().unwrap();
        let socket = pumi_sys::ipc::address(dir, "pumi.sock");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pumi_sys::ipc::answers(&socket) {
            assert!(
                Instant::now() < deadline,
                "pumid did not start: {}",
                std::fs::read_to_string(dir.join("test.log")).unwrap_or_default()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        Server { dir: dir.to_path_buf(), child: Some(child) }
    }
    fn socket(&self) -> PathBuf {
        pumi_sys::ipc::address(&self.dir, "pumi.sock")
    }
    fn client(&self) -> Client {
        Client::connect(&self.socket())
    }
    fn signal(&mut self, signal: Signal) {
        let mut child = self.child.take().unwrap();
        pumi_sys::process::signal(child.id(), signal, false).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            assert!(Instant::now() < deadline, "pumid did not exit on signal {signal:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn stop(mut self) {
        self.signal(Signal::Term);
    }
    fn kill(mut self) {
        self.signal(Signal::Kill);
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "--- {}:\n{}",
                self.dir.display(),
                std::fs::read_to_string(self.dir.join("test.log")).unwrap_or_default()
            );
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
struct Client {
    reader: BufReader<Stream>,
    writer: Stream,
    next: i64,
    notifications: VecDeque<Value>,
}
impl Client {
    fn connect(socket: &Path) -> Client {
        let stream = Stream::connect(socket).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
        Client {
            reader: BufReader::new(stream.try_clone().unwrap()),
            writer: stream,
            next: 1,
            notifications: VecDeque::new(),
        }
    }
    fn try_call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.next;
        self.next += 1;
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(self.writer, "{request}").unwrap();
        loop {
            let Some(message) = self.try_read() else { return Err(json!("the server hung up")) };
            if message.get("id") == Some(&json!(id)) {
                return match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message["result"].clone()),
                };
            }
            self.notifications.push_back(message);
        }
    }
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.try_call(method, params.clone()).unwrap_or_else(|err| panic!("{method}({params}): {err}"))
    }
    fn try_read(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(&line).unwrap()),
        }
    }
    /// Declares the browser plugin's page block and its options, as its manifest would.
    fn page(&mut self) {
        self.call("core.setBlocks", json!([[{"id": PAGE, "provider": "pumi_dev/browser"}]]));
        for (name, ty, default) in [("url", "string", json!("about:blank")), ("zoom", "number", json!(1))] {
            let spec = json!({"type": ty, "default": default, "scope": "tab", "blocks": [PAGE], "desc": name});
            self.call("option.declare", json!([name, spec]));
        }
    }
    fn wait_for_text(&mut self, tab: &Value, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let text = self.call("tab.call", json!([tab, "read", {"lines": 200}]));
            let text = text["text"].as_str().unwrap_or("").to_string();
            if text.contains(needle) {
                return text;
            }
            assert!(Instant::now() < deadline, "tab {tab} never showed {needle:?}; it shows:\n{text}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    fn option(&mut self, name: &str, tab: &Value) -> Value {
        self.call("option.get", json!([name, {"id": tab}]))
    }
}
fn build(client: &mut Client, cwd: &Path) -> (Value, Value) {
    let terminal = client.call(
        "tab.open",
        json!([
            "terminal",
            {"command": ["/bin/sh"], "cwd": cwd},
            {"kind": "session", "name": "work", "cwd": cwd}
        ]),
    );
    client.call("tab.call", json!([terminal, "input", {"text": "echo marker-$((6*7))\r"}]));
    client.wait_for_text(&terminal, "marker-42");
    client.page();
    let browser = client.call(
        "tab.open",
        json!([PAGE, {"url": "https://example.com/docs"}, {"kind": "tiled", "beside": terminal, "split": "right"}]),
    );
    client.call("tab.call", json!([browser, "describe", {"title": "Docs", "text": "Example Docs"}]));
    client.call("option.set", json!(["@x", "kept", {"scope": "global"}]));
    (terminal, browser)
}
fn check_back(client: &mut Client, cwd: &Path, scrollback: bool) {
    assert_eq!(client.call("session.list", json!([])).as_array().unwrap().len(), 1);
    let tabs = client.call("tab.list", json!([{}]));
    assert_eq!(tabs.as_array().unwrap().len(), 2, "both tabs came back: {tabs}");
    let types: Vec<Value> = tabs.as_array().unwrap().iter().map(|p| client.option("tab_block", p)).collect();
    assert_eq!(types, [json!("terminal"), json!(PAGE)]);
    let (terminal, browser) = (tabs[0].clone(), tabs[1].clone());
    assert_eq!(client.option("url", &browser), json!("https://example.com/docs"));
    assert_eq!(client.option("title", &browser), json!("Docs"));
    assert_eq!(client.call("option.get", json!(["@x", {"scope": "global"}])), json!("kept"));
    assert_eq!(client.call("session.get", json!([]))["name"], json!("work"), "the session came back as it was");
    let view = client.call("tab.get", json!([&terminal]))["place"]["view"].clone();
    let layout = client.call("view.get", json!([view]))["layout"].clone();
    assert_eq!(layout["axis"], json!("row"), "the split came back: {layout}");
    let path = client.option("tab_current_path", &terminal);
    assert_eq!(std::fs::canonicalize(path.as_str().unwrap()).unwrap(), std::fs::canonicalize(cwd).unwrap());
    if scrollback {
        client.wait_for_text(&terminal, "marker-42");
    }
    client.call("tab.call", json!([terminal, "input", {"text": "echo again-$((1+1))\r"}]));
    client.wait_for_text(&terminal, "again-2");
}
#[test]
fn a_session_survives_the_server_stopping() {
    let dir = Server::dir();
    let cwd = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    build(&mut client, cwd.path());
    drop(client);
    server.stop();
    assert!(dir.path().join("state.json").exists());
    let server = Server::start(dir.path());
    check_back(&mut server.client(), cwd.path(), true);
}
#[test]
fn a_session_survives_the_server_being_killed() {
    let dir = Server::dir();
    let cwd = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    build(&mut client, cwd.path());
    std::thread::sleep(Duration::from_millis(400));
    server.kill();
    let server = Server::start(dir.path());
    check_back(&mut server.client(), cwd.path(), false);
}
#[test]
fn an_attached_client_receives_every_change_in_order() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut watcher = server.client();
    let attached = watcher.call("client.attach", json!([{"width": 100, "height": 30}]));
    let mut seq = attached["seq"].as_u64().unwrap();
    let mut actor = server.client();
    actor.page();
    let p1 = actor.call("tab.open", json!(["terminal", {"command": ["cat"]}, {"kind": "session", "name": "one"}]));
    let s = actor.call("tab.get", json!([&p1]))["session"].clone();
    let p2 = actor
        .call("tab.open", json!(["terminal", {"command": ["cat"]}, {"kind": "tiled", "beside": p1, "split": "right"}]));
    let p3 = actor.call(
        "tab.open",
        json!([PAGE, {"url": "example.com"}, {"kind": "tiled", "beside": p2, "split": "down", "size": "30%"}]),
    );
    actor.call("option.set", json!(["@build", "ok", {"id": p2}]));
    actor.call("option.set", json!(["remain_on_exit", true, {"id": p2}]));
    assert_eq!(actor.call("option.get", json!(["remain_on_exit", {"id": p2}])), json!(true));
    actor.call("option.set", json!(["@theme", "dark", {"scope": "global"}]));
    actor.call("option.set", json!(["@theme", null, {"scope": "global"}]));
    actor.call("session.update", json!([&s, {"name": "renamed"}]));
    actor.call("tab.update", json!([&p1, {"place": {"kind": "tiled", "swap": p3}}]));
    actor.call("tab.update", json!([&p2, {"place": {"kind": "view", "name": "two"}}]));
    let view = actor.call("tab.get", json!([&p2]))["place"]["view"].clone();
    let float = actor.call("tab.open", json!(["terminal", {"command": ["cat"]}, {"kind": "float", "view": view}]));
    actor.call("option.set", json!(["url", "https://example.com/next", {"id": p3}]));
    actor.call("tab.focus", json!([&p1]));
    actor.call("tab.close", json!([float]));
    actor.call("tab.open", json!([PAGE, {"url": "example.org"}, {"kind": "session", "name": "two"}]));
    let view = actor.call("tab.get", json!([&p1]))["place"]["view"].clone();
    let layout = actor.call("view.get", json!([&view]))["layout"].clone();
    actor.call("view.update", json!([&view, {"layout": layout}]));
    actor.call("session.close", json!([&s]));
    watcher.call("session.list", json!([]));
    let events: Vec<Value> = watcher
        .notifications
        .drain(..)
        .filter(|n| n["method"] == "redraw")
        .flat_map(|n| n["params"].as_array().unwrap().clone())
        .collect();
    assert!(events.len() > 40, "the stream carries the changes: {}", events.len());
    for event in &events {
        let next = event["seq"].as_u64().unwrap();
        assert_eq!(next, seq + 1, "in order, none missing: {event}");
        seq = next;
        assert!(event.get("event").is_some() && event.get("match").is_some(), "{event}");
    }
    let names: Vec<&str> = events.iter().map(|e| e["event"].as_str().unwrap()).collect();
    for expected in [
        "session.opened",
        "view.opened",
        "tab.opened",
        "option.changed",
        "session.changed",
        "view.changed",
        "tab.changed",
        "tab.closed",
        "view.closed",
        "session.closed",
    ] {
        assert!(names.contains(&expected), "{expected} is in the stream");
    }
    let now = watcher.call("client.attach", json!([{"width": 100, "height": 30}]));
    assert!(now["seq"].as_u64().unwrap() >= seq, "an attach says where the stream is");
}
/// The share of its view the pane holding a tab has in `layout`, as `view.get` gives it; `None`
/// when the tree does not hold it.
fn share(layout: &Value, tab: &Value) -> Option<f64> {
    if layout["tabs"].as_array().is_some_and(|tabs| tabs.contains(tab)) {
        return Some(1.0);
    }
    let ratio = layout["ratio"].as_f64()?;
    if let Some(inner) = share(&layout["first"], tab) {
        return Some(ratio * inner);
    }
    share(&layout["second"], tab).map(|inner| (1.0 - ratio) * inner)
}
#[test]
fn the_host_places_tabs_and_the_core_makes_the_place() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut c = server.client();
    let cat = json!({"command": ["cat"]});
    let first = c.call("tab.open", json!(["terminal", cat, {"kind": "session", "name": "place"}]));
    let s = c.call("tab.get", json!([&first]))["session"].clone();
    c.call("session.focus", json!([&s]));
    let right = c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "beside": first, "split": "right"}]));
    let view = c.call("tab.get", json!([right]))["place"]["view"].clone();
    let pane_of = |c: &mut Client, tab: &Value| c.call("tab.get", json!([tab]))["place"]["pane"].clone();
    let (left_pane, right_pane) = (pane_of(&mut c, &first), pane_of(&mut c, &right));
    assert_eq!(
        c.call("view.get", json!([&view]))["layout"],
        json!({
            "axis": "row",
            "ratio": 0.5,
            "first": {"pane": left_pane, "tabs": [first], "active": first},
            "second": {"pane": right_pane, "tabs": [right], "active": right},
        })
    );
    let wait_width = |c: &mut Client, tab: &Value, want: u64| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while c.option("pane_width", tab) != json!(want) {
            assert!(Instant::now() < deadline, "{tab} is not {want} wide: {}", c.option("pane_width", tab));
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    wait_width(&mut c, &first, 40);
    wait_width(&mut c, &right, 39);
    let narrow =
        c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "split": "right", "size": 20, "full": true}]));
    let layout = c.call("view.get", json!([&view]))["layout"].clone();
    let narrow_share = share(&layout, &narrow).unwrap_or_else(|| panic!("the narrow tab is in the layout: {layout}"));
    assert!(narrow_share > 0.15 && narrow_share < 0.35, "the size it was given: {layout}");
    let float =
        c.call("tab.open", json!(["terminal", cat, {"kind": "float", "view": view, "width": "50%", "height": 10}]));
    let place = c.call("tab.get", json!([float]))["place"].clone();
    assert_eq!((place["kind"].clone(), place["view"].clone()), (json!("float"), view.clone()));
    assert_eq!((place["width"].clone(), place["height"].clone()), (json!("50%"), json!(10)), "{place}");
    let client = c.call(
        "tab.open",
        json!(["terminal", cat, {
            "kind": "float", "relative": "client", "anchor": "center", "width": "80%", "height": 18,
            "backdrop": 30, "dismiss": ["escape", "press"], "zIndex": 70, "focus": false,
        }]),
    );
    let place = c.call("tab.get", json!([client]))["place"].clone();
    assert_eq!(
        (place["relative"].clone(), place["anchor"].clone(), place["width"].clone(), place["height"].clone()),
        (json!("client"), json!("center"), json!("80%"), json!(18)),
        "{place}"
    );
    assert_eq!(
        (place["backdrop"].clone(), place["dismiss"].clone(), place["zIndex"].clone()),
        (json!(30), json!(["escape", "press"]), json!(70)),
        "{place}"
    );
    c.call("tab.close", json!([client]));
    let alone = c.call("tab.open", json!(["terminal", cat, {"kind": "view"}]));
    assert_eq!(c.call("session.get", json!([&s]))["views"].as_array().unwrap().len(), 2, "into the current session");
    c.call("tab.update", json!([alone, {"place": {"kind": "tiled", "beside": first, "split": "down"}}]));
    assert_eq!(
        c.call("session.get", json!([&s]))["views"].as_array().unwrap().len(),
        1,
        "its view closed when it left"
    );
    let err = c.try_call("core.openTab", json!(["terminal", cat, {"kind": "tiled", "split": "right"}])).unwrap_err();
    assert!(err.to_string().contains("beside"), "the core chooses nothing: {err}");
    c.call("tab.update", json!([right, {"zoom": true}]));
    assert_eq!(c.call("view.get", json!([&view]))["zoom"], right_pane, "its pane fills the view");
    c.call("tab.update", json!([right, {"zoom": false}]));
    assert_eq!(c.call("view.get", json!([&view]))["zoom"], json!(null));
    c.call("pane.update", json!([&right_pane, {"zoom": "toggle"}]));
    assert_eq!(c.call("view.get", json!([&view]))["zoom"], right_pane);
    c.call("pane.update", json!([&right_pane, {"zoom": "toggle"}]));
    assert_eq!(c.call("view.get", json!([&view]))["zoom"], json!(null));
    // Into a pane, as one of its tabs; with `pane_tabs` off, beside it in a pane of its own.
    let joined = c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "pane": &right_pane}]));
    assert_eq!(pane_of(&mut c, &joined), right_pane, "a tab of the pane");
    assert_eq!(c.call("pane.get", json!([&right_pane]))["tabs"], json!([right, joined]));
    c.call("option.set", json!(["pane_tabs", false, {"scope": "global"}]));
    let split = c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "pane": &right_pane}]));
    assert_ne!(pane_of(&mut c, &split), right_pane, "a pane of its own");
    c.call("option.set", json!(["pane_tabs", true, {"scope": "global"}]));
    c.call("tab.close", json!([split]));
    c.call("tab.close", json!([joined]));
    let theirs_tab = c.call("tab.open", json!(["terminal", cat, {"kind": "session", "name": "other"}]));
    let other = c.call("tab.get", json!([theirs_tab]))["session"].clone();
    let theirs = c.call("tab.get", json!([theirs_tab]))["view"].clone();
    assert_eq!(c.call("session.get", json!([&other]))["views"], json!([theirs]), "its own view, at index 0");
    c.call("view.update", json!([&view, {"index": 5}]));
    assert_eq!(c.call("view.get", json!([&view]))["index"], 5);
    assert_eq!(c.call("session.get", json!([&s]))["views"], json!([view]));
    let refused = c.try_call("view.update", json!([&view, {"session": other, "index": 0}]));
    assert!(refused.is_err(), "index 0 is theirs: {refused:?}");
    let gone = c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "beside": first, "split": "down"}]));
    let next = c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "beside": gone, "split": "right"}]));
    c.call("tab.close", json!([next]));
    c.call("tab.close", json!([gone]));
    let closed = c.call("tab.list", json!([{"closed": true}]));
    let entry = closed.as_array().unwrap().iter().find(|e| e["tab"] == next).cloned().unwrap();
    assert_eq!(entry["beside"], gone, "it was beside a tab that closed after it");
    let place = json!({"kind": "tiled", "view": entry["view"], "beside": entry["beside"]});
    let back = c.call("tab.open", json!(["terminal", {"state": entry["state"]}, place]));
    assert_eq!(c.call("tab.get", json!([back]))["place"]["view"], entry["view"], "into its view, beside what is there");
    c.call("option.set", json!(["remain_on_exit", true, {"id": first}]));
    assert_eq!(c.call("option.get", json!(["remain_on_exit", {"id": first}])), json!(true));
    c.call("option.set", json!(["history_bytes", 65536, {"id": first}]));
    assert_eq!(c.call("option.get", json!(["history_bytes", {"id": first}])), json!(65536));
    let raw = c.try_call("core.setOptionValue", json!(["remain_on_exit", "off", {"tab": first}]));
    assert!(raw.is_err(), "the core does not coerce");
}
#[test]
fn tabs_are_sized_by_their_client_and_headless_by_the_host() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut ui = server.client();
    ui.call("client.attach", json!([{"width": 120, "height": 40}]));
    let mut c = server.client();
    let cat = json!({"command": ["cat"]});
    let first = c.call("tab.open", json!(["terminal", cat, {"kind": "session", "name": "sized"}]));
    let right = c.call("tab.open", json!(["terminal", cat, {"kind": "tiled", "beside": first, "split": "right"}]));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(c.option("pane_width", &right), json!(80), "a client is attached: the host leaves sizing to it");
    // As a client sizes a terminal it draws: the core's own call, not the API's.
    let number: u64 = right.as_str().unwrap().trim_start_matches('%').parse().unwrap();
    c.call("core.tabCall", json!([number, "resize", {"width": 59, "height": 40}]));
    assert_eq!(c.option("pane_width", &right), json!(59), "as the client sized it");
    drop(ui);
    let deadline = Instant::now() + Duration::from_secs(10);
    while (c.option("pane_width", &first), c.option("pane_width", &right)) != (json!(40), json!(39)) {
        assert!(Instant::now() < deadline, "the host did not size them: {}", c.option("pane_width", &right));
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn make(client: &mut Client, ty: &str) -> Value {
    match ty {
        "terminal" => {
            let tab = client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session"}]));
            client.call("tab.call", json!([tab, "input", {"text": "echo round-$((20+1))\r"}]));
            client.wait_for_text(&tab, "round-21");
            tab
        }
        PAGE => {
            let tab = client.call("tab.open", json!([PAGE, {"url": "https://example.com/a"}, {"kind": "session"}]));
            client.call("option.set", json!(["url", "https://example.com/b", {"id": tab}]));
            client.call(
                "tab.call",
                json!([tab, "describe", {"text": "B page", "selection": "B", "title": "B", "scroll": [0, 120]}]),
            );
            client.call("option.set", json!(["zoom", 1.25, {"id": tab}]));
            tab
        }
        "file" => {
            let dir = std::env::temp_dir().join(format!("pumi-round-trip-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("notes.txt");
            std::fs::write(&path, "one\ntwo\n").unwrap();
            let tab = client.call("tab.open", json!(["file", {"path": path, "line": 2}, {"kind": "session"}]));
            assert_eq!(client.option("path", &tab), json!(path), "the tab's file");
            client.call("tab.call", json!([tab, "describe", {"text": "one\ntwo\n", "selection": "two"}]));
            tab
        }
        other => panic!("the round-trip test has no recipe for block `{other}`: add one"),
    }
}
#[test]
fn every_block_round_trips_into_a_fresh_server() {
    let a_dir = Server::dir();
    let a = Server::start(a_dir.path());
    let mut from = a.client();
    from.page();
    let types: Vec<String> = from
        .call("registry.list", json!(["block"]))
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect();
    for ty in types {
        let tab = make(&mut from, &ty);
        from.call("option.set", json!(["@role", format!("{ty}-tab"), {"id": tab}]));
        let state = from.call("tab.get", json!([tab, {"state": {"blobs": "inline"}}]))["state"].clone();
        assert_eq!(state["block"], json!(ty));
        let fresh_dir = Server::dir();
        let fresh = Server::start(fresh_dir.path());
        let mut to = fresh.client();
        to.page();
        let copy = to.call("tab.open", json!([ty, {"state": state}]));
        let local = |client: &mut Client, tab: &Value| -> Map<String, Value> {
            let state = client.call("tab.get", json!([tab, {"state": {"viewOptions": ""}}]));
            state["state"]["options"].as_object().unwrap().clone()
        };
        assert_eq!(local(&mut to, &copy), local(&mut from, &tab), "{ty}: options");
        let read = |client: &mut Client, tab: &Value| client.call("tab.call", json!([tab, "read", {"lines": 200}]));
        let selection = |client: &mut Client, tab: &Value| client.call("tab.call", json!([tab, "selection"]));
        match state["strategy"].as_str().unwrap() {
            "relaunch" => {
                let before = read(&mut from, &tab)["text"].as_str().unwrap().trim().to_string();
                let marker = before.lines().rev().find(|l| l.contains("round-21")).unwrap().trim().to_string();
                to.wait_for_text(&copy, &marker);
            }
            "exact" | "reference" => {
                assert_eq!(read(&mut to, &copy), read(&mut from, &tab), "{ty}: read");
                assert_eq!(selection(&mut to, &copy), selection(&mut from, &tab), "{ty}: selection");
            }
            other => panic!("{ty}: strategy {other}"),
        }
        let again = to.call("tab.get", json!([copy, {"state": {"viewOptions": ""}}]));
        let first = from.call("tab.get", json!([tab, {"state": {"viewOptions": ""}}]));
        assert_eq!(again["state"]["data"], first["state"]["data"], "{ty}: its data saves the same");
    }
    let _ = std::fs::remove_dir_all(std::env::temp_dir().join(format!("pumi-round-trip-{}", std::process::id())));
}
#[test]
fn a_linked_package_lists_its_block_types_while_it_is_on() {
    let dir = Server::dir();
    let root = dir.path();
    let pkg = root.join("queue");
    std::fs::create_dir_all(&pkg).unwrap();
    let manifest = r#"{ "name": "pumi-queue", "main": "src/main.ts",
      "pumi": { "name": "queue", "owner": "pumi_dev", "api": 1, "contributes": { "blocks": [{ "id": "itemList" }] } } }"#;
    std::fs::write(pkg.join("package.json"), manifest).unwrap();
    let server = Server::start_home(root);
    let mut client = server.client();
    let listed = |client: &mut Client| {
        let types = client.call("registry.list", json!(["block"]));
        types.as_array().unwrap().iter().any(|t| t["id"] == "pumi_dev.queue.itemList")
    };
    assert!(!listed(&mut client));
    let name = client.call("plugin.add", json!([pkg.to_str().unwrap(), {}]));
    assert_eq!(name, "pumi_dev/queue", "its id");
    assert!(listed(&mut client), "linked");
    client.call("plugin.update", json!([name, {"enabled": false}]));
    assert!(!listed(&mut client), "turned off");
    client.call("plugin.update", json!([name, {"enabled": true}]));
    assert!(listed(&mut client), "on again");
    let tab = client.call("tab.open", json!(["pumi_dev.queue.itemList", {}, {"kind": "session"}]));
    assert_eq!(client.call("tab.get", json!([&tab]))["block"], "pumi_dev.queue.itemList");
    pumi_proto::server::restart(&core_binary(), &pumi_proto::paths::Paths::at(root)).unwrap();
    let mut client = server.client();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !listed(&mut client) {
        assert!(Instant::now() < deadline, "listed again after a restart");
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[test]
fn the_bundled_terminal_plugin_is_listed_turned_off_and_never_removed() {
    let dir = Server::dir();
    let server = Server::start_home(dir.path());
    let mut client = server.client();
    let terminal = |client: &mut Client| {
        let list = client.call("registry.list", json!(["plugin", {"name": "terminal"}]));
        list.as_array().unwrap().iter().find(|p| p["spec"]["name"] == "terminal").cloned().expect("listed")
    };
    let listed = terminal(&mut client);
    assert_eq!((listed["id"].clone(), listed["bundled"].clone()), (json!("pumi/terminal"), json!(true)));
    assert_eq!(listed["manifest"]["contributes"]["renderers"]["terminal"], "Terminal");
    assert_eq!(listed["active"], true);
    let err = client.try_call("plugin.remove", json!(["pumi/terminal", {}])).unwrap_err();
    assert!(err["message"].as_str().unwrap().contains("bundled"), "{err}");
    client.call("plugin.update", json!(["pumi/terminal", {"enabled": false}]));
    assert_eq!(terminal(&mut client)["active"], false, "turned off, and still listed");
    client.call("plugin.update", json!(["pumi/terminal", {"enabled": true}]));
    assert_eq!(terminal(&mut client)["active"], true);
}
/// The browser is a bundled plugin, not the app's: its block and options are there while it is on,
/// and gone when it is turned off.
#[test]
fn the_bundled_browser_s_page_and_options_go_when_it_is_turned_off() {
    let dir = Server::dir();
    let server = Server::start_home(dir.path());
    let mut client = server.client();
    let blocks = |client: &mut Client| -> Vec<Value> {
        client.call("registry.list", json!(["block"])).as_array().unwrap().iter().map(|b| b["id"].clone()).collect()
    };
    let listed = client.call("registry.list", json!(["plugin", {"name": "browser"}]));
    let browser = listed.as_array().unwrap().iter().find(|p| p["spec"]["name"] == "browser").cloned().expect("listed");
    assert_eq!((browser["id"].clone(), browser["bundled"].clone()), (json!("pumi/browser"), json!(true)));
    assert!(blocks(&mut client).contains(&json!("pumi.browser.page")), "its page, while it is on");
    let home = client.call("registry.list", json!(["option", {"name": "pumi.browser.home_url"}]));
    assert_eq!(home.as_array().map(Vec::len), Some(1), "its options are its own: {home}");
    let core = client.try_call("registry.list", json!(["option", {"name": "browser.home_url"}]));
    assert!(core.is_err(), "the core declares none: {core:?}");
    client.call("plugin.update", json!(["pumi/browser", {"enabled": false}]));
    assert!(!blocks(&mut client).contains(&json!("pumi.browser.page")), "off, the page goes");
}
#[test]
fn a_plugin_from_a_repository_installs_at_the_ref_it_follows_and_goes() {
    let dir = Server::dir();
    let root = dir.path();
    let repo = root.join("things");
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    let write = |version: &str, min: &str| {
        std::fs::create_dir_all(&repo).unwrap();
        let manifest = format!(
            r#"{{ "name": "pumi-hello", "version": "{version}", "pumi": {{ "name": "hello", "api": 1{min} }} }}"#
        );
        std::fs::write(repo.join("package.json"), manifest).unwrap();
    };
    std::fs::create_dir_all(&repo).unwrap();
    git(&["init", "-q"]);
    write("1.0.0", "");
    git(&["add", "-A"]);
    git(&["commit", "-qm", "one"]);
    git(&["tag", "v1"]);
    write("2.0.0", "");
    git(&["commit", "-qam", "two"]);
    let server = Server::start_home(root);
    let mut client = server.client();
    let url = format!("file://{}", repo.display());
    let name = client.call("plugin.add", json!([url, {"version": "v1"}]));
    assert_eq!(name, json!("local/hello"), "its id");
    let got = client.call("registry.list", json!(["plugin", {"name": "things"}]));
    assert_eq!(got[0]["meta"]["version"], "1.0.0", "the tag it follows: {got}");
    assert_eq!(got[0]["installed"], true);
    let lock: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("c/pumi/plugin-lock.json")).unwrap()).unwrap();
    assert_eq!(lock["plugins"]["things"]["version"], "v1", "{lock}");
    // It follows the tag, so the commit it has is the one the tag names: nothing to update.
    client.call("plugin.update", json!(["local/hello", {}]));
    assert_eq!(client.call("registry.list", json!(["plugin", {"name": "things"}]))[0]["meta"]["version"], "1.0.0");
    client.call("plugin.remove", json!(["local/hello", {}]));
    assert!(!root.join("d/pumi/installed/things").exists(), "it goes");
    // A plugin for a later pumi is refused, and leaves nothing behind.
    write("3.0.0", r#", "minPumi": "999.0.0""#);
    git(&["commit", "-qam", "three"]);
    let err = client.try_call("plugin.add", json!([url, {}]));
    assert!(err.is_err(), "{err:?}");
    assert!(!root.join("d/pumi/installed/things").exists());
}
#[cfg(unix)]
#[test]
fn a_plugin_from_a_repository_gets_its_dependencies_once_and_again_when_it_lost_them() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Server::dir();
    let root = dir.path();
    let repo = root.join("qr");
    std::fs::create_dir_all(&repo).unwrap();
    let manifest = r#"{ "name": "pumi-qr", "dependencies": { "qrcode": "^1.5.4" },
      "devDependencies": { "typescript": "5.9.3" }, "pumi": { "name": "qr", "api": 1 } }"#;
    std::fs::write(repo.join("package.json"), manifest).unwrap();
    let git = |args: &[&str]| {
        let ok = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"])
            .args(args)
            .current_dir(&repo)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "one"]);
    // A Bun that says how it was run, and fails while `fail` is there.
    let calls = root.join("bun-calls");
    let bun = root.join("fake-bun");
    let script = format!(
        "#!/bin/sh\n[ -e '{fail}' ] && {{ echo 'no network' >&2; exit 1; }}\necho \"$PWD $*\" >> '{calls}'\nmkdir -p node_modules/qrcode\n",
        fail = root.join("fail").display(),
        calls = calls.display()
    );
    std::fs::write(&bun, script).unwrap();
    std::fs::set_permissions(&bun, std::fs::Permissions::from_mode(0o755)).unwrap();
    let ran = || std::fs::read_to_string(&calls).unwrap_or_default().lines().map(String::from).collect::<Vec<_>>();
    let installed = root.join("d/pumi/installed/qr");
    let url = format!("file://{}", repo.display());
    {
        let server = Server::spawn_with(None, root, &[], true, &[("PUMI_BUN", &bun)]);
        let mut client = server.client();
        // Bun failing, the plugin is not installed.
        std::fs::write(root.join("fail"), "").unwrap();
        let err = client.try_call("plugin.add", json!([url, {}]));
        assert!(format!("{err:?}").contains("no network"), "{err:?}");
        assert!(!installed.exists(), "nothing is left behind");
        std::fs::remove_file(root.join("fail")).unwrap();
        assert_eq!(client.call("plugin.add", json!([url, {}])), json!("local/qr"));
        let first = ran();
        assert_eq!(first.len(), 1, "installed once: {first:?}");
        assert!(first[0].ends_with("install --production --no-save --ignore-scripts --no-progress"), "{first:?}");
        assert!(installed.join("node_modules/qrcode").is_dir());
        // Nothing changed: no second install.
        client.call("plugin.update", json!(["local/qr", {}]));
        assert_eq!(ran().len(), 1, "{:?}", ran());
        // Lost, `plugin.update` installs them again.
        std::fs::remove_dir_all(installed.join("node_modules")).unwrap();
        client.call("plugin.update", json!(["local/qr", {}]));
        assert_eq!(ran().len(), 2, "{:?}", ran());
        std::fs::remove_dir_all(installed.join("node_modules")).unwrap();
    }
    // As one installed before pumi ran `bun install`: the next start installs them.
    let _server = Server::spawn_with(None, root, &[], true, &[("PUMI_BUN", &bun)]);
    let deadline = Instant::now() + Duration::from_secs(10);
    while ran().len() < 3 {
        assert!(Instant::now() < deadline, "no install at start: {:?}", ran());
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(ran().len(), 3, "{:?}", ran());
}
#[test]
fn a_linked_package_s_block_is_a_tab_type_with_its_own_methods_and_options() {
    let dir = Server::dir();
    let root = dir.path();
    let pkg = root.join("site");
    std::fs::create_dir_all(&pkg).unwrap();
    let manifest = r#"{ "name": "pumi-site", "main": "src/main.ts",
      "pumi": { "name": "site", "owner": "pumi_dev", "api": 1, "contributes": {
        "options": { "url": { "type": "string", "default": "", "scope": "tab", "blocks": ["page"], "desc": "Its page." } },
        "blocks": [{ "id": "page", "version": 2, "methods": ["navigate"] }] } } }"#;
    std::fs::write(pkg.join("package.json"), manifest).unwrap();
    let server = Server::start_home(root);
    let mut client = server.client();
    client.call("plugin.add", json!([pkg.to_str().unwrap(), {}]));
    let types = client.call("registry.list", json!(["block"]));
    let page = types.as_array().unwrap().iter().find(|t| t["id"] == "pumi_dev.site.page").cloned().expect("listed");
    assert_eq!(page["version"], 2, "{page}");
    assert_eq!(page["methods"], json!(["args", "describe", "navigate"]));
    assert_eq!(page["options"], json!(["pumi_dev.site.url"]));
    let url = "https://example.com/a";
    let tab = client.call("tab.open", json!(["pumi_dev.site.page", {"pumi_dev.site.url": url}, {"kind": "session"}]));
    assert_eq!(client.option("pumi_dev.site.url", &tab), json!(url), "its argument is its option");
    assert_eq!(client.call("tab.get", json!([tab, {"state": true}]))["state"]["block_version"], 2);
    // A method of its own is its plugin's function, with the tab first: nothing provides
    // `pumi_dev.site.page.navigate` in this test, so the call says what it looked for.
    let miss = client.try_call("tab.call", json!([tab, "navigate", {"url": "https://example.com/b"}])).unwrap_err();
    assert!(miss["message"].as_str().unwrap_or_default().contains("pumi_dev.site.page.navigate"), "{miss:?}");
    assert!(client.try_call("tab.call", json!([tab, "back"])).is_err(), "a method it does not declare");
}
/// A plugin's pages and sections in Settings are listed by their prefixed ids, a section on a page
/// of its own by that page's.
#[test]
fn a_linked_plugin_s_settings_pages_and_sections_are_listed() {
    let dir = Server::dir();
    let root = dir.path();
    let pkg = root.join("remote");
    std::fs::create_dir_all(&pkg).unwrap();
    let manifest = r#"{ "name": "remote", "pumi": { "name": "remote", "owner": "me", "api": 1, "contributes": { "settings": {
        "pages": [{ "id": "remote", "title": "Remote", "icon": "globe", "component": "RemotePage", "keywords": ["phone", "qr"] }],
        "sections": [{ "id": "tailscale", "page": "general", "title": "Tailscale", "component": "TailscaleSection", "keywords": ["sign in"] },
                     { "id": "devices", "page": "remote", "title": "Devices", "component": "Devices" }] } } } }"#;
    std::fs::write(pkg.join("package.json"), manifest).unwrap();
    let server = Server::start_home(root);
    let mut client = server.client();
    client.call("plugin.add", json!([pkg.to_str().unwrap(), {}]));
    let list = client.call("registry.list", json!(["settings"]));
    let ids: Vec<(&str, &str, &str)> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["kind"].as_str().unwrap(), e["id"].as_str().unwrap(), e["page"].as_str().unwrap_or("")))
        .collect();
    assert_eq!(
        ids,
        [
            ("page", "me.remote.remote", ""),
            ("section", "me.remote.tailscale", "general"),
            ("section", "me.remote.devices", "me.remote.remote")
        ]
    );
    assert_eq!(list[0]["keywords"], json!(["phone", "qr"]));
    assert_eq!(list[1]["plugin"], "me/remote");
    let one = client.call("registry.list", json!(["settings", {"name": "me.remote.tailscale"}]));
    assert_eq!(one.as_array().unwrap().len(), 1);
}
/// A lockfile under the earlier name (`pack-lock.json`) is read where the current one is missing,
/// and moved over: its plugins stay installed.
#[test]
fn an_older_lockfile_is_read_and_moved_over() {
    let root = Server::dir();
    std::fs::create_dir_all(root.path().join("c/pumi")).unwrap();
    std::fs::create_dir_all(root.path().join("d/pumi/installed/tools")).unwrap();
    let linked = root.path().join("elsewhere/tools");
    std::fs::create_dir_all(&linked).unwrap();
    std::fs::write(
        linked.join("package.json"),
        r#"{ "name": "tools", "pumi": { "name": "tools", "owner": "me", "api": 1 } }"#,
    )
    .unwrap();
    let lock = format!(r#"{{"plugins": {{"tools": {{"kind": "link", "src": "{}"}}}}}}"#, linked.display());
    std::fs::write(root.path().join("c/pumi/pack-lock.json"), lock).unwrap();
    let server = Server::start_home(root.path());
    let mut client = server.client();
    let list = client.call("registry.list", json!(["plugin", {}]));
    assert!(list.as_array().unwrap().iter().any(|p| p["spec"]["name"] == "tools"), "{list}");
    let tools = list.as_array().unwrap().iter().find(|p| p["spec"]["name"] == "tools").cloned().unwrap();
    assert_eq!(tools["path"].as_str(), Some(linked.to_str().unwrap()), "a link is read where it was linked from");
    assert!(root.path().join("c/pumi/plugin-lock.json").exists(), "read where pumi reads it now");
}
#[test]
fn a_program_without_osc_7501_falls_back_to_its_title_and_notifications() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    let tab = client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session", "name": "a"}]));
    let wait = |client: &mut Client, want: Value, why: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while client.option("@agent.status", &tab) != want {
            assert!(Instant::now() < deadline, "{why}: {}", client.option("@program_status", &tab));
            std::thread::sleep(Duration::from_millis(50));
        }
    };
    // A spinner in the title (OSC 2), then a notification (OSC 777) when it is through.
    let toy = "sh -c \"printf '\\033]2;\u{280b} toy\\007'; sleep 2; printf '\\033]2;toy\\007\\033]777;notify;toy;finished\\007'; sleep 2\"\r";
    client.call("tab.call", json!([tab, "input", {"text": toy}]));
    wait(&mut client, json!("working"), "a spinning title never read as working");
    wait(&mut client, json!("done"), "a notification never left a result");
    let records = client.option("@program_status", &tab);
    assert_eq!(records[0]["msg"], json!("toy: finished"), "{records}");
    client.call("tab.update", json!([&tab, {"seen": true}]));
    assert_ne!(client.option("@agent.status", &tab), json!("done"), "seen, it stops asking");
    // A program that reports for itself is heard, not its title.
    let real = "sh -c \"printf '\\033]7501;state=idle:app=real\\007\\033]2;\u{2819} real\\007'; sleep 1\"\r";
    client.call("tab.call", json!([tab, "input", {"text": real}]));
    wait(&mut client, json!("idle"), "the real report never counted");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(client.option("@agent.status", &tab), json!("idle"), "its spinner is not a fallback any more");
}
#[test]
fn a_programs_own_report_is_its_tabs_status() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    let tab = client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session", "name": "a"}]));
    let report = "printf '\\033]7501;state=working:app=toy\\033\\\\'\r";
    client.call("tab.call", json!([tab, "input", {"text": report}]));
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.option("@agent.status", &tab) != json!("working") {
        assert!(Instant::now() < deadline, "the tab never took the report: {:?}", client.option("@agent.status", &tab));
        std::thread::sleep(Duration::from_millis(50));
    }
    let records = client.option("@program_status", &tab);
    assert_eq!((records[0]["state"].clone(), records[0]["app"].clone()), (json!("working"), json!("toy")), "{records}");
    let prompt = "printf '\\033]133;A\\007'\r";
    client.call("tab.call", json!([tab, "input", {"text": prompt}]));
    while client.option("@agent.status", &tab) != json!(null) {
        assert!(Instant::now() < deadline, "the prompt never dropped the running report");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(client.option("@program_status", &tab), json!(null), "and its records went with it");
    let failed = "printf '\\033]7501;state=error\\033\\\\'\r";
    client.call("tab.call", json!([tab, "input", {"text": failed}]));
    while client.option("@agent.status", &tab) != json!("error") {
        assert!(Instant::now() < deadline, "a failed program never said so");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.call("tab.call", json!([tab, "input", {"text": prompt}]));
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(client.option("@agent.status", &tab), json!("error"), "a result stays until it is seen");
    assert_eq!(client.call("tab.update", json!([&tab, {"seen": true}])), json!(null));
    assert_eq!(client.option("@agent.status", &tab), json!("idle"));
    client.call("tab.update", json!([&tab, {"seen": true}]));
    assert_eq!(client.option("@agent.status", &tab), json!("idle"), "nothing more to see");
    let deadline = Instant::now() + Duration::from_secs(10);
    let job = "sh -c \"printf '\\033]7501;state=working:id=job:app=job\\007\\033]133;A\\007'; sleep 2\"\r";
    client.call("tab.call", json!([tab, "input", {"text": job}]));
    while client.option("@agent.status", &tab) != json!("working") {
        assert!(Instant::now() < deadline, "the job never said it was working");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(client.option("@agent.status", &tab), json!("working"), "a program's 133 is not a prompt");
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.option("@agent.status", &tab) != json!("idle") {
        assert!(Instant::now() < deadline, "the job's report outlived it: {}", client.option("@program_status", &tab));
        std::thread::sleep(Duration::from_millis(50));
    }
    let done = "sh -c \"printf '\\033]7501;state=done:id=build\\007'; sleep 1\"\r";
    client.call("tab.call", json!([tab, "input", {"text": done}]));
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.option("@agent.status", &tab) != json!("done") {
        assert!(Instant::now() < deadline, "the job's result never came");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(client.option("@agent.status", &tab), json!("done"), "a result outlives its program");
    assert_eq!(client.call("tab.update", json!([&tab, {"seen": true}])), json!(null));
    let left = client.option("@program_status", &tab);
    assert!(left.as_array().is_some_and(|l| l.iter().all(|r| r["id"] != "build")), "{left}");
    let blocked = client.call("tab.call", json!([tab, "report", {"body": "state=blocked:id=ext:kind=question:app=x"}]));
    assert_eq!(blocked, json!(true));
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.option("@agent.status", &tab) != json!("blocked") {
        assert!(Instant::now() < deadline, "a report from outside never counted");
        std::thread::sleep(Duration::from_millis(50));
    }
    client.call("tab.call", json!([tab, "report", {"body": "state=clear:id=ext"}]));
    while client.option("@agent.status", &tab) != json!("idle") {
        assert!(Instant::now() < deadline, "its clear never counted");
        std::thread::sleep(Duration::from_millis(50));
    }
}
#[test]
fn a_package_tab_is_an_agent_and_its_requests_block_it_until_answered() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    let terminal =
        client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session", "name": "a"}]));
    client.call("core.setBlocks", json!([[{"id": "pumi_dev.acp.chat", "provider": "pumi_dev/acp"}]]));
    let tab = client
        .call("tab.open", json!(["pumi_dev.acp.chat", {}, {"kind": "tiled", "beside": terminal, "split": "right"}]));
    for (name, value) in [("@agent", "codex"), ("@agent.session", "s-1"), ("@agent.model", "gpt-5")] {
        client.call("option.set", json!([name, value, {"id": tab}]));
    }
    assert_eq!(client.call("tab.call", json!([tab, "report", {"body": "state=working:app=codex"}])), json!(true));
    let until = |client: &mut Client, want: &str| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while client.option("@agent.status", &tab) != json!(want) {
            assert!(Instant::now() < deadline, "never {want}: {}", client.option("@program_status", &tab));
            std::thread::sleep(Duration::from_millis(30));
        }
    };
    until(&mut client, "working");
    let ask = json!({"kind": "permission", "title": "Run ls?", "options": [{"id": "yes", "label": "Yes", "kind": "allow_once"}]});
    // Nobody listens to `request.opened`: a permission nobody would see is refused.
    let err = client.try_call("request.open", json!([ask, {"tab": tab}])).unwrap_err();
    assert_eq!(err["data"]["kind"], "provider_unavailable", "{err}");
    let mut ui = server.client();
    ui.call("event.on", json!([["request.opened", "request.closed"], {}]));
    let mut agent = server.client();
    let id = agent.call("request.open", json!([ask, {"tab": tab}]));
    assert!(id.as_str().is_some_and(|id| id.starts_with('+')), "its request's id: {id}");
    let requested = loop {
        let message = ui.try_read().expect("request.opened");
        if message["params"][1]["event"] == "request.opened" {
            break message["params"][1]["data"].clone();
        }
    };
    assert_eq!((requested["request"].clone(), requested["title"].clone()), (id.clone(), json!("Run ls?")));
    until(&mut client, "blocked");
    assert_eq!(agent.call("request.list", json!([{"tab": tab}])), json!([id]), "listed for its tab");
    assert_eq!(agent.call("request.get", json!([&id]))["title"], "Run ls?");
    let bad = agent.try_call("request.close", json!([&id, {"answer": {"option": "no"}}]));
    assert!(bad.is_err(), "an option it does not offer");
    assert_eq!(agent.call("request.close", json!([&id, {"answer": {"option": "yes"}}])), json!(null));
    assert!(agent.try_call("request.close", json!([&id, {"answer": {"option": "yes"}}])).is_err(), "the first wins");
    let closed = loop {
        let message = ui.try_read().expect("request.closed");
        if message["params"][1]["event"] == "request.closed" {
            break message["params"][1]["data"].clone();
        }
    };
    assert_eq!((closed["request"].clone(), closed["reason"].clone()), (id.clone(), json!("answered")));
    assert_eq!(closed["answer"], json!({"option": "yes"}), "the answer it was given");
    until(&mut client, "working");
    // A closed tab's request goes with it.
    let gone = agent.call("request.open", json!([{"kind": "question", "title": "Still there?"}, {"tab": tab}]));
    assert!(gone.as_str().is_some_and(|id| id.starts_with('+')), "{gone}");
    client.call("tab.close", json!([tab]));
    let deadline = Instant::now() + Duration::from_secs(10);
    while agent.call("request.list", json!([])) != json!([]) {
        assert!(Instant::now() < deadline, "a closed tab's request stayed");
        std::thread::sleep(Duration::from_millis(30));
    }
}
#[test]
fn a_notice_is_not_a_tab() {
    let dir = Server::dir();
    let server = Server::spawn(dir.path(), &["--clean"], false);
    let mut client = server.client();
    let before = client.call("tab.list", json!([{}]));
    let id = client.call(
        "request.open",
        json!([{"kind": "notification", "title": "settings.json: skills.tools is not a list", "level": "warn"}]),
    );
    assert!(id.as_str().is_some_and(|id| id.starts_with('+')), "its notification's id: {id}");
    let notice = |title: &str| json!([{"kind": "notification", "title": title, "notification": {"group": "build"}}]);
    let building = client.call("request.open", notice("building"));
    let grouped = client.call("request.open", notice("built"));
    assert_ne!(grouped, building, "a notice of its own");
    assert_eq!(client.call("request.list", json!([])), json!([id, grouped]), "one notice of a group at a time");
    assert_eq!(client.call("request.close", json!([&grouped])), json!(null));
    assert_eq!(client.call("request.list", json!([])), json!([id]));
    assert_eq!(client.call("tab.list", json!([{}])), before, "a notice is no tab");
}
fn install(from: &Path, bin: &Path, name: &str) {
    let new = bin.join(format!("{name}.new"));
    std::fs::copy(from, &new).unwrap();
    // A new binary, as an update writes one: macOS's copy is a clone that keeps the source's
    // times, so a copy of the same build would look like the file already running.
    let file = std::fs::OpenOptions::new().write(true).open(&new).unwrap();
    file.set_modified(std::time::SystemTime::now()).unwrap();
    drop(file);
    std::fs::rename(&new, bin.join(pumi_sys::paths::exe(name))).unwrap();
}
fn host_pid(client: &mut Client) -> Value {
    let chans = client.call("channel.list", json!([]));
    let host = chans.as_array().unwrap().iter().find(|c| c["client"]["name"] == "pumi-host").cloned();
    host.map(|h| h["client"]["attributes"]["pid"].clone()).unwrap_or(Value::Null)
}
#[test]
fn an_update_restarts_the_server_and_its_host_as_the_new_binaries_keeping_its_programs() {
    let dir = Server::dir();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    install(&core_binary(), &bin, "pumid");
    install(Path::new(env!("CARGO_BIN_EXE_pumi-host")), &bin, "pumi-host");
    let server = Server::start_installed(&bin, dir.path());
    let mut client = server.client();
    let image = client.call("server.getImage", json!([]));
    assert_eq!(image["outdated"], false, "{image}");
    assert_eq!(image["host"]["path"], json!(bin.join(pumi_sys::paths::exe("pumi-host")).display().to_string()));
    let tab =
        client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session", "name": "work"}]));
    client.call("tab.call", json!([tab, "input", {"text": "echo before-$((40+2))\r"}]));
    client.wait_for_text(&tab, "before-42");
    let old_host = host_pid(&mut client);
    assert!(old_host.is_u64(), "the host says its pid");
    let paths = pumi_proto::paths::Paths::at(dir.path());
    let ensure = |bin: &Path| pumi_proto::server::ensure(&bin.join(pumi_sys::paths::exe("pumid")), &paths).unwrap();
    assert!(matches!(ensure(&bin), pumi_proto::server::Ensured::Current), "nothing was replaced");
    install(&core_binary(), &bin, "pumid");
    install(Path::new(env!("CARGO_BIN_EXE_pumi-host")), &bin, "pumi-host");
    let image = client.call("server.getImage", json!([]));
    assert_eq!((image["outdated"].clone(), image["host"]["outdated"].clone()), (json!(true), json!(true)), "{image}");
    assert!(matches!(ensure(&bin), pumi_proto::server::Ensured::Updated), "the new binaries took over");
    let mut client = server.client();
    let image = client.call("server.getImage", json!([]));
    assert_eq!(image["outdated"], false, "{image}");
    let new_host = host_pid(&mut client);
    assert!(new_host.is_u64() && new_host != old_host, "a new host: {old_host} then {new_host}");
    client.call("tab.call", json!([tab, "input", {"text": "echo after-$((40+3))\r"}]));
    let text = client.wait_for_text(&tab, "after-43");
    assert!(text.contains("before-42"), "{text}");
}
#[test]
fn a_host_that_dies_is_started_again() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    let old = host_pid(&mut client);
    pumi_sys::process::signal(old.as_u64().unwrap() as u32, Signal::Kill, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut client = server.client();
    loop {
        let new = host_pid(&mut client);
        if new.is_u64() && new != old {
            break;
        }
        assert!(Instant::now() < deadline, "no host came back after {old}");
        std::thread::sleep(Duration::from_millis(50));
    }
    let chans = client.call("channel.list", json!([]));
    let host = chans.as_array().unwrap().iter().find(|c| c["client"]["name"] == "pumi-host").cloned().unwrap();
    assert_eq!(host["client"]["version"]["string"], json!(env!("CARGO_PKG_VERSION")), "the new host is this build");
}
#[test]
fn a_stop_loses_an_attached_stream_rather_than_ending_it() {
    use pumi_proto::native::{AttachServer, Mode, Size};
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    let tab =
        client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session", "name": "work"}]));
    let size = Size { cols: 80, rows: 24, cell_width: 8, cell_height: 16 };
    let target = tab.as_str().unwrap().to_string();
    let (mut stream, _writer) = pumi_proto::client::attach(
        &pumi_sys::ipc::address(dir.path(), "grid.sock"),
        "test",
        "0",
        &target,
        Mode::Control,
        size,
        None,
        None,
    )
    .unwrap();
    assert!(matches!(stream.recv(), Some(AttachServer::Snapshot { .. })));
    drop(client);
    server.stop();
    while let Some(message) = stream.recv() {
        assert!(!matches!(message, AttachServer::Exited { .. } | AttachServer::Error(_)), "{message:?}");
    }
}
#[test]
fn a_restart_keeps_programs_and_returns_once_the_server_is_back() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut client = server.client();
    let tab =
        client.call("tab.open", json!(["terminal", {"command": ["/bin/sh"]}, {"kind": "session", "name": "work"}]));
    client.call("tab.call", json!([tab, "input", {"text": "echo before-$((40+2))\r"}]));
    client.wait_for_text(&tab, "before-42");
    let paths = pumi_proto::paths::Paths::at(dir.path());
    pumi_proto::server::restart(&core_binary(), &paths).unwrap();
    let mut client = server.client();
    assert!(host_pid(&mut client).is_u64(), "the host is back");
    client.call("tab.call", json!([tab, "input", {"text": "echo after-$((40+3))\r"}]));
    let text = client.wait_for_text(&tab, "after-43");
    assert!(text.contains("before-42"), "{text}");
}
#[test]
fn a_menu_item_is_a_function_with_its_arguments() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut c = server.client();
    let cat = json!({"command": ["cat"]});
    let tab = c.call("tab.open", json!(["terminal", cat, {"kind": "session", "name": "points"}]));
    let rhs = json!({"fn": "option.set", "args": ["@clicked", "yes", {"id": tab}], "title": "Mark it"});
    c.call("menu.set", json!(["TabStrip.Mark", rhs]));
    c.call(
        "menu.set",
        json!(["ToolBar.Fork", {"fn": "option.set", "args": ["@forked", "yes", {"id": tab}], "title": "Fork"}]),
    );
    let strip = c.call("registry.list", json!(["menu", {"name": "TabStrip.Mark"}]))[0].clone();
    assert_eq!(
        (strip["label"].clone(), strip["call"]["call"].clone()),
        (json!("Mark it"), json!("option.set")),
        "{strip}"
    );
    assert_eq!(c.call("registry.list", json!(["menu", {"name": "ToolBar.Fork"}]))[0]["label"], "Fork");
    // Choosing it runs the function with the arguments it was set with: its own tab's.
    let (name, args) = (strip["call"]["call"].clone(), strip["call"]["args"].clone());
    c.call("call", json!([name, args]));
    assert_eq!(c.option("@clicked", &tab), json!("yes"));
    // Removing an item takes it out of the menu.
    c.call("menu.set", json!(["TabStrip.Mark", null]));
    assert!(c.try_call("registry.list", json!(["menu", {"name": "TabStrip.Mark"}])).is_err(), "removed");
}
#[test]
fn a_file_opens_on_the_right_and_reads_without_a_renderer() {
    let dir = Server::dir();
    let server = Server::start(dir.path());
    let mut c = server.client();
    let files = Server::dir();
    std::fs::write(files.path().join("a.md"), "# a\n").unwrap();
    let cat = json!({"command": ["cat"], "cwd": files.path()});
    let first = c.call("tab.open", json!(["terminal", cat, {"kind": "session", "name": "files"}]));
    let a = files.path().join("a.md").display().to_string();
    let tab = c.call(
        "tab.open",
        json!(["file", {"path": a, "line": 2}, {"kind": "tiled", "beside": first, "split": "right"}]),
    );
    assert_eq!(c.option("path", &tab), json!(a), "the file it shows");
    let view = c.call("tab.get", json!([&tab]))["place"]["view"].clone();
    let layout = c.call("view.get", json!([view]))["layout"].clone();
    assert_eq!(layout["axis"], json!("row"), "on the right: {layout}");
    assert_eq!(c.call("tab.call", json!([&tab, "at"])), json!({"path": a, "line": 2}));
    let below = c.call("tab.open", json!(["file", {"path": a}, {"kind": "tiled", "beside": first, "split": "down"}]));
    assert_ne!(below, tab, "every open is a tab of its own: a file shown twice is two tabs");
    let relative = c.try_call("tab.open", json!(["file", {"path": "a.md"}, {"kind": "session"}]));
    assert!(relative.unwrap_err().to_string().contains("absolute"), "the caller resolves the path");
    assert_eq!(c.call("tab.call", json!([below, "read"])), json!({"text": "# a\n"}), "no renderer: the file");
}
