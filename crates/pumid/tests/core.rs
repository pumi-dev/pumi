use pumi_proto::native::{AttachServer, Mode, Size};
use pumi_sys::ipc::{Stream, answers};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
struct Core {
    child: Child,
    socket: PathBuf,
    dir: tempfile::TempDir,
}
impl Core {
    fn start() -> Core {
        Core::start_in(tempfile::tempdir().unwrap(), false)
    }
    fn start_in(dir: tempfile::TempDir, persist: bool) -> Core {
        let child = Command::new(env!("CARGO_BIN_EXE_pumid"))
            .args(["--data-dir"])
            .arg(dir.path())
            .args(["--no-host", "--stay"])
            .args((!persist).then_some("--no-persist"))
            .env("PUMI_LOGIN_ENV", "0")
            .stdin(Stdio::null())
            .stderr(std::fs::File::create(dir.path().join("test.log")).unwrap())
            .spawn()
            .unwrap();
        let socket = pumi_sys::ipc::address(dir.path(), "pumi.sock");
        until("the socket", || answers(&socket));
        Core { child, socket, dir }
    }
    fn grid(&self) -> PathBuf {
        pumi_sys::ipc::address(self.dir.path(), "grid.sock")
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("{}", std::fs::read_to_string(self.dir.path().join("test.log")).unwrap_or_default());
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
struct Conn {
    writer: Stream,
    lines: BufReader<Stream>,
    next: u64,
}
impl Conn {
    fn open(socket: &Path) -> Conn {
        let writer = Stream::connect(socket).expect("the core's socket");
        writer.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let lines = BufReader::new(writer.try_clone().unwrap());
        Conn { writer, lines, next: 0 }
    }
    fn send(&mut self, value: Value) {
        writeln!(self.writer, "{value}").unwrap();
    }
    fn read(&mut self) -> Value {
        self.try_read().expect("a message")
    }
    fn try_read(&mut self) -> Option<Value> {
        let mut line = String::new();
        match self.lines.read_line(&mut line) {
            Ok(n) if n > 0 => serde_json::from_str(&line).ok(),
            _ => None,
        }
    }
    fn call(&mut self, method: &str, params: Value) -> (Result<Value, Value>, Vec<Value>) {
        self.next += 1;
        let id = self.next;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut notes = Vec::new();
        loop {
            let Some(msg) = self.try_read() else { return (Err(json!("the connection ended")), notes) };
            if msg.get("id") == Some(&json!(id)) && msg.get("method").is_none() {
                return match msg.get("error") {
                    Some(err) => (Err(err.clone()), notes),
                    None => (Ok(msg["result"].clone()), notes),
                };
            }
            notes.push(msg);
        }
    }
    fn ok(&mut self, method: &str, params: Value) -> Value {
        match self.call(method, params).0 {
            Ok(v) => v,
            Err(err) => panic!("{method}: {err}"),
        }
    }
}
/// A tab's id, as the API spells it (`%3`), from the number the core's own calls answer.
fn tab_ref(tab: u64) -> Value {
    json!(format!("%{tab}"))
}
fn attach(grid: &Path, tab: u64, resume: Option<u64>) -> pumi_proto::client::AttachStream {
    let size = Size { cols: 80, rows: 24, cell_width: 8, cell_height: 16 };
    let (stream, writer) =
        pumi_proto::client::attach(grid, "test", "0", &format!("%{tab}"), Mode::Observe, size, None, resume).unwrap();
    std::mem::forget(writer);
    stream.set_timeout(Some(Duration::from_secs(5))).unwrap();
    stream
}
fn read_until(stream: &mut pumi_proto::client::AttachStream, text: &str) -> (String, u64) {
    let mut all = Vec::new();
    let mut seq = 0;
    while !String::from_utf8_lossy(&all).contains(text) {
        match stream.recv() {
            Some(AttachServer::Snapshot { bytes, seq: s, .. } | AttachServer::Output { bytes, seq: s }) => {
                all.extend(bytes);
                seq = s;
            }
            Some(_) => {}
            None => panic!("the stream ended before {text:?}: {:?}", String::from_utf8_lossy(&all)),
        }
    }
    (String::from_utf8_lossy(&all).into_owned(), seq)
}
#[test]
fn tabs_options_and_events_over_json_rpc() {
    let core = Core::start();
    let mut c = Conn::open(&core.socket);
    c.ok("event.on", json!(["tab.opened", {}]));
    let (tab, notes) = c.call("core.openTab", json!(["terminal", { "command": ["/bin/cat"] }, { "kind": "session" }]));
    let tab = tab.unwrap().as_u64().expect("a tab id");
    let heard = notes.iter().any(|n| n["method"] == "rpc.event" && n["params"][1]["event"] == "tab.opened");
    assert!(heard, "the subscription heard the tab open, before the answer: {notes:?}");
    c.ok("core.setOptionValue", json!(["@role", "logs", { "tab": tab }]));
    assert_eq!(c.ok("core.getOptionValue", json!(["@role", { "tab": tab }])), json!("logs"));
    assert!(c.call("core.setOptionValue", json!(["no_such", 1, {}])).0.is_err());
    c.ok("core.tabCall", json!([tab, "input", { "text": "hello core\n" }]));
    let mut stream = attach(&core.grid(), tab, None);
    read_until(&mut stream, "hello core");
    let read = c.ok("core.tabCall", json!([tab, "read", { "lines": 5 }]));
    assert!(read["text"].as_str().unwrap().contains("hello core"), "{read}");
    c.ok("core.setBlocks", json!([[{ "id": "pumi_dev.hello.badge", "provider": "pumi_dev/hello" }]]));
    let badge =
        c.ok("core.openTab", json!(["pumi_dev.hello.badge", { "x": 1 }, {"kind": "session"}])).as_u64().unwrap();
    assert_eq!(c.ok("core.tabCall", json!([badge, "args"])), json!({ "x": 1 }));
}
#[test]
fn calls_to_a_function_a_channel_provides_go_to_it() {
    let core = Core::start();
    let mut provider = Conn::open(&core.socket);
    provider.ok("fn.set", json!(["hello_greet", {"desc": "Greet.", "effect": "read"}]));
    // And every function nobody has: the fallback, as the host is.
    provider.ok("core.setFallback", json!([]));
    let mut caller = Conn::open(&core.socket);
    caller.next = 40;
    caller.send(json!({ "jsonrpc": "2.0", "id": 41, "method": "hello_greet", "params": ["you"] }));
    let asked = provider.read();
    assert_eq!(asked["method"], "hello_greet");
    assert_eq!(asked["params"], json!(["you"]));
    assert!(asked["rpc.caller"]["chan"].is_u64());
    provider.send(json!({ "jsonrpc": "2.0", "id": asked["id"], "result": "hi you" }));
    let answer = caller.read();
    assert_eq!(answer, json!({ "jsonrpc": "2.0", "id": 41, "result": "hi you" }));
    caller.send(json!({ "jsonrpc": "2.0", "id": 42, "method": "cmd.run", "params": ["ls"] }));
    let asked = provider.read();
    assert_eq!(asked["method"], "cmd.run");
    provider.send(json!({ "jsonrpc": "2.0", "id": asked["id"], "error": { "code": -32601, "message": "no" } }));
    assert_eq!(caller.read()["error"]["message"], "no");
    let api = caller.ok("api.getInfo", json!([]));
    assert!(api["functions"].as_array().unwrap().iter().any(|f| f["name"] == "hello_greet"));
    drop(provider);
    let (err, _) = caller.call("hello_greet", json!([]));
    assert!(err.is_err(), "it went with its channel");
}
#[test]
fn a_restart_keeps_the_program_and_clients_resume() {
    let core = Core::start();
    let mut c = Conn::open(&core.socket);
    let tab =
        c.ok("core.openTab", json!(["terminal", { "command": ["/bin/cat"] }, { "kind": "session" }])).as_u64().unwrap();
    let pid = c.ok("core.getOptionValue", json!(["tab_pid", { "tab": tab }]));
    c.ok("core.tabCall", json!([tab, "input", { "text": "before\n" }]));
    let mut stream = attach(&core.grid(), tab, None);
    let (_, seq) = read_until(&mut stream, "before\r\nbefore\r\n");
    c.ok("server.restart", json!([]));
    until("the restarted core", || {
        answers(&core.socket) && {
            let mut c = Conn::open(&core.socket);
            c.call("tab.list", json!([])).0.is_ok_and(|p| p.as_array().is_some_and(|p| !p.is_empty()))
        }
    });
    let mut c = Conn::open(&core.socket);
    assert_eq!(c.ok("core.getOptionValue", json!(["tab_pid", { "tab": tab }])), pid, "the same program");
    c.ok("core.tabCall", json!([tab, "input", { "text": "after\n" }]));
    let mut stream = attach(&core.grid(), tab, Some(seq));
    let (text, _) = read_until(&mut stream, "after");
    assert!(!text.contains("before"), "only what it missed: {text:?}");
}
fn agent_runs(c: &mut Conn, stream: &mut pumi_proto::client::AttachStream, tab: u64, line: &str) {
    read_until(stream, "$ ");
    let agent = "sh -c \"sleep 0.2; printf 'w\\141iting\\n'; exec cat\"\n";
    c.ok("core.tabCall", json!([tab, "input", { "text": agent }]));
    read_until(stream, "waiting");
    c.ok("core.setOptionValue", json!(["restore_command", line, { "tab": tab }]));
}
#[test]
fn a_restored_shell_runs_the_line_its_program_left() {
    let core = Core::start();
    let mut c = Conn::open(&core.socket);
    c.ok("core.setOptionValue", json!(["default_shell", "/bin/sh", {}]));
    let tab = c.ok("core.openTab", json!(["terminal", {}, { "kind": "session" }])).as_u64().unwrap();
    let mut stream = attach(&core.grid(), tab, None);
    agent_runs(&mut c, &mut stream, tab, "echo restored-$((6*7))");
    let state = c.ok("tab.get", json!([tab_ref(tab), { "state": true }]))["state"].clone();
    assert_eq!(state["options"]["restore_command"], "echo restored-$((6*7))");
    let again = c.ok("core.openTab", json!(["terminal", { "state": state }, { "kind": "session" }])).as_u64().unwrap();
    let mut restored = attach(&core.grid(), again, None);
    read_until(&mut restored, "restored-42");
    read_until(&mut restored, "$ ");
    c.ok("core.tabCall", json!([again, "input", { "text": "echo again-$((2+2))\n" }]));
    read_until(&mut restored, "again-4");
    assert_eq!(c.ok("tab.get", json!([tab_ref(again), { "state": true }]))["state"]["data"].get("command"), None);
    c.ok("core.tabCall", json!([tab, "input", { "bytes": [4] }]));
    until("the line dropped", || c.ok("core.getOptionValue", json!(["restore_command", { "tab": tab }])) == json!(""));
}
#[test]
fn a_core_that_died_brings_the_agent_back_in_its_tab() {
    let core = Core::start_in(tempfile::tempdir().unwrap(), true);
    let mut c = Conn::open(&core.socket);
    c.ok("core.setOptionValue", json!(["default_shell", "/bin/sh", {}]));
    let tab = c.ok("core.openTab", json!(["terminal", {}, { "kind": "session" }])).as_u64().unwrap();
    let mut stream = attach(&core.grid(), tab, None);
    agent_runs(&mut c, &mut stream, tab, "echo resumed-$((6*7))");
    let saved = core.dir.path().join("state.json");
    until("the model saved", || std::fs::read_to_string(&saved).is_ok_and(|s| s.contains("resumed-")));
    let mut core = core;
    let _ = core.child.kill();
    let _ = core.child.wait();
    let dir = std::mem::replace(&mut core.dir, tempfile::tempdir().unwrap());
    drop(core);
    let core = Core::start_in(dir, true);
    let mut stream = attach(&core.grid(), tab, None);
    read_until(&mut stream, "resumed-42");
}
#[test]
fn a_package_tab_of_its_own_version_comes_back_before_its_package_is_told_again() {
    let core = Core::start_in(tempfile::tempdir().unwrap(), true);
    let mut c = Conn::open(&core.socket);
    let page = json!({"id": "pumi_dev.browser.page", "version": 2, "methods": ["navigate"]});
    c.ok("core.setBlocks", json!([[page]]));
    let args = json!({"url": "https://example.com/kept"});
    let tab = c.ok("core.openTab", json!(["pumi_dev.browser.page", args, { "kind": "session" }])).as_u64().unwrap();
    let saved = core.dir.path().join("state.json");
    until("the model saved", || std::fs::read_to_string(&saved).is_ok_and(|s| s.contains("example.com/kept")));
    let mut core = core;
    let _ = core.child.kill();
    let _ = core.child.wait();
    let dir = std::mem::replace(&mut core.dir, tempfile::tempdir().unwrap());
    drop(core);
    let core = Core::start_in(dir, true);
    let mut c = Conn::open(&core.socket);
    let state = c.ok("tab.get", json!([tab_ref(tab), { "state": true }]))["state"].clone();
    assert_eq!((state["block_version"].clone(), state["data"]["args"].clone()), (json!(2), args));
    let types = c.ok("core.blocks", json!([]));
    assert!(types.as_array().unwrap().iter().any(|t| t["id"] == "pumi_dev.browser.page" && t["version"] == 2));
    let navigate = c.call("core.tabCall", json!([tab, "navigate", {}])).0.unwrap_err();
    assert_eq!(navigate["data"]["kind"], json!("not_found"), "its plugin's function, not the core's");
}
#[test]
fn a_respawned_shell_stays_and_runs_its_restore_command() {
    let core = Core::start();
    let mut c = Conn::open(&core.socket);
    c.ok("core.setOptionValue", json!(["default_shell", "/bin/sh", {}]));
    let tab = c.ok("core.openTab", json!(["terminal", {}, { "kind": "session" }])).as_u64().unwrap();
    let mut stream = attach(&core.grid(), tab, None);
    read_until(&mut stream, "$ ");
    c.ok("core.setOptionValue", json!(["restore_command", "echo back-$((6*7))", { "tab": tab }]));
    c.ok("core.tabCall", json!([tab, "respawn", {}]));
    let mut again = attach(&core.grid(), tab, None);
    read_until(&mut again, "back-42");
    assert_eq!(c.ok("tab.list", json!([])), json!([tab_ref(tab)]), "the tab is still there");
    read_until(&mut again, "$ ");
    c.ok("core.tabCall", json!([tab, "input", { "text": "echo still-$((1+1))\n" }]));
    read_until(&mut again, "still-2");
}
#[test]
fn the_first_app_attached_is_the_primary_app_until_it_goes() {
    let core = Core::start();
    let app = |core: &Core| {
        let mut c = Conn::open(&core.socket);
        c.ok("channel.setInfo", json!(["pumi-app", {}, "ui", {}, {}]));
        c.ok("client.attach", json!([{ "width": 80, "height": 24 }]));
        c
    };
    let mut cli = Conn::open(&core.socket);
    cli.ok("client.attach", json!([{ "width": 80, "height": 24 }]));
    assert_eq!(cli.ok("channel.getInfo", json!([0]))["primary"], json!(false), "only an app is primary");
    let mut first = app(&core);
    let mut second = app(&core);
    assert_eq!(first.ok("channel.getInfo", json!([0]))["primary"], json!(true));
    assert_eq!(second.ok("channel.getInfo", json!([0]))["primary"], json!(false));
    first.ok("client.detach", json!([]));
    assert_eq!(second.ok("channel.getInfo", json!([0]))["primary"], json!(true), "the next app takes over");
    let clients = second.ok("channel.list", json!([{ "ui": true }]));
    assert_eq!(clients.as_array().unwrap().iter().filter(|c| c["primary"] == true).count(), 1);
}
#[test]
fn an_agent_is_told_what_listeners_answer_and_takes_what_waits_for_it() {
    let core = Core::start_in(tempfile::tempdir().unwrap(), true);
    let mut plugin = Conn::open(&core.socket);
    plugin.ok("channel.setInfo", json!(["pumi_dev/agents", {}, "plugin", {}, {}]));
    plugin.ok("event.on", json!(["agent.sending", {}]));
    let mut hook = Conn::open(&core.socket);
    let tab = hook
        .ok("core.openTab", json!(["terminal", { "command": ["/bin/cat"] }, { "kind": "session" }]))
        .as_u64()
        .unwrap();
    let turn = json!({ "agent": "claude", "event": "prompt", "session": "s-1", "accepts": ["queue"] });
    hook.send(json!({ "jsonrpc": "2.0", "id": 900, "method": "agent.turn", "params": [tab_ref(tab), turn] }));
    let sending = plugin.read();
    assert_eq!(sending["params"][1]["event"], "agent.sending");
    let id = sending["params"][1]["data"]["id"].clone();
    plugin.ok("agent.contribute", json!([id, "the map"]));
    let told = hook.read();
    assert_eq!(told["id"], 900);
    assert_eq!(told["result"]["context"], "<pumi-context from=\"pumi_dev/agents\">\nthe map\n</pumi-context>\n");
    // A source that does not answer is late, after the deadline, and the call still answers.
    let started = Instant::now();
    let turn = json!({ "agent": "claude", "event": "prompt", "session": "s-1" });
    let late = hook.ok("agent.turn", json!([tab_ref(tab), turn, { "timeout": 0.15 }]));
    assert!(started.elapsed() >= Duration::from_millis(150) && started.elapsed() < Duration::from_secs(2));
    assert_eq!(late, json!({ "context": null, "from": [], "late": ["pumi_dev/agents"] }));
    let _ = plugin.read();
    // A take waits for an item, and the item is saved until it is taken.
    hook.send(json!({ "jsonrpc": "2.0", "id": 901, "method": "agent.take", "params": [tab_ref(tab), { "session": "s-1", "wait": true }] }));
    let mut cli = Conn::open(&core.socket);
    cli.ok("channel.setInfo", json!(["cli", {}, "cli", {}, {}]));
    let item = cli.ok("agent.queue", json!([tab_ref(tab), "run the tests again"]));
    assert!(item.is_u64(), "its handle: {item}");
    let taken = hook.read();
    assert_eq!((taken["id"].clone(), taken["result"]["text"].clone()), (json!(901), json!("run the tests again")));
    cli.ok("agent.queue", json!([tab_ref(tab), "kept across a restart"]));
    let saved = core.dir.path().join("state.json");
    until("the queue saved", || std::fs::read_to_string(&saved).is_ok_and(|s| s.contains("kept across a restart")));
    let mut core = core;
    let _ = core.child.kill();
    let _ = core.child.wait();
    let dir = std::mem::replace(&mut core.dir, tempfile::tempdir().unwrap());
    drop(core);
    let core = Core::start_in(dir, true);
    let mut c = Conn::open(&core.socket);
    let taken = c.ok("agent.take", json!([tab_ref(tab), { "session": "s-1" }]));
    assert_eq!(taken["text"], "kept across a restart");
}
