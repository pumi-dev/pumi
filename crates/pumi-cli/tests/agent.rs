//! An agent's hook against a real core: what a listener says reaches the agent in its format, and
//! a message queued for it continues its turn when the turn ends.
use pumi_sys::ipc::Stream;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn cli() -> PathBuf {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_pumi-cli"));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut build = Command::new(cargo);
    build.args(["build", "-p", "pumid", "-p", "pumi-host"]);
    if !cfg!(debug_assertions) {
        build.arg("--release");
    }
    assert!(build.status().expect("cargo builds the core").success(), "building the core failed");
    cli
}

struct Core {
    child: Child,
    socket: PathBuf,
    _dir: tempfile::TempDir,
}
impl Core {
    fn start(pumid: &Path) -> Core {
        let dir = tempfile::tempdir().unwrap();
        let child = Command::new(pumid)
            .arg("--data-dir")
            .arg(dir.path())
            .args(["--stay", "--no-persist"])
            .env("PUMI_LOGIN_ENV", "0")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let socket = pumi_sys::ipc::address(dir.path(), "pumi.sock");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !pumi_sys::ipc::answers(&socket) {
            assert!(Instant::now() < deadline, "the core did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        // The CLI's commands are the host's registry's: wait for the host to serve it.
        let mut conn = Conn::open(&socket);
        while conn.try_call("registry.list", json!(["fn"])).is_err() {
            assert!(Instant::now() < deadline, "the host did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
        Core { child, socket, _dir: dir }
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Conn {
    writer: Stream,
    lines: BufReader<Stream>,
    next: u64,
}
impl Conn {
    fn open(socket: &Path) -> Conn {
        let writer = Stream::connect(socket).unwrap();
        writer.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Conn { lines: BufReader::new(writer.try_clone().unwrap()), writer, next: 0 }
    }
    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.lines.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    fn try_call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        self.next += 1;
        writeln!(self.writer, "{}", json!({ "jsonrpc": "2.0", "id": self.next, "method": method, "params": params }))
            .unwrap();
        loop {
            let message = self.read();
            if message["id"] == self.next && message.get("method").is_none() {
                return match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message["result"].clone()),
                };
            }
        }
    }
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.try_call(method, params.clone()).unwrap_or_else(|error| panic!("{method}({params}): {error}"))
    }
}

/// Runs the CLI in tab `tab`, with `stdin`; its status and stdout.
fn pumi(cli: &Path, core: &Core, tab: u64, args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(cli)
        .args(args)
        .env("PUMI", &core.socket)
        .env("PUMI_TAB", format!("%{tab}"))
        .env("PUMI_SETTINGS", "/nonexistent/settings.json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn claude_is_told_what_a_plugin_says_and_continues_with_what_was_queued() {
    let cli = cli();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let mut setup = Conn::open(&core.socket);
    let tab = setup.call("core.openTab", json!(["terminal", { "command": ["/bin/cat"] }, { "kind": "session" }]));
    let tab = tab.as_u64().unwrap();
    // A plugin that tells every agent the same thing.
    let mut plugin = Conn::open(&core.socket);
    plugin.call("channel.setInfo", json!(["pumi_dev/agents", {}, "plugin", {}, {}]));
    plugin.call("event.on", json!(["agent.sending", {}]));
    let listener = std::thread::spawn(move || {
        for _ in 0..2 {
            let sending = plugin.read();
            let id = sending["params"][1]["data"]["id"].clone();
            plugin.call("agent.contribute", json!([id, "the tabs around you"]));
        }
    });
    let start = json!({ "session_id": "s-1", "source": "startup", "hook_event_name": "SessionStart", "model": "opus" });
    let (status, out, _) =
        pumi(&cli, &core, tab, &["skill", "hook", "claude", "start", "--format", "hooks"], &start.to_string());
    assert_eq!(status, 0);
    let out: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(out["hookSpecificOutput"]["hookEventName"], "SessionStart");
    let told = out["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert_eq!(told, "<pumi-context from=\"pumi_dev/agents\">\nthe tabs around you\n</pumi-context>\n");
    let option = |c: &mut Conn, name: &str| c.call("option.get", json!([name, { "id": format!("%{tab}") }]));
    assert_eq!(option(&mut setup, "@agent"), "claude");
    assert_eq!(option(&mut setup, "@agent.session"), "s-1");
    assert_eq!(option(&mut setup, "@agent.accepts"), json!(["queue"]));
    assert_eq!(option(&mut setup, "restore_command"), "claude --resume s-1");
    // An older hook: `pumi skill hook`, no `--format`.
    let prompt = json!({ "session_id": "s-1", "hook_event_name": "UserPromptSubmit" });
    let (_, out, _) = pumi(&cli, &core, tab, &["skill", "hook", "claude", "prompt"], &prompt.to_string());
    assert!(out.contains("\"hookEventName\":\"UserPromptSubmit\""), "{out}");
    listener.join().unwrap();
    // A message for it, from another tab's CLI.
    let (status, id, err) = pumi(&cli, &core, tab, &["agent", "queue", &format!("%{tab}"), "run the tests"], "");
    assert_eq!(status, 0, "{err}");
    assert!(id.trim().parse::<u64>().is_ok(), "its handle: {id}");
    let queued = setup.call("tab.get", json!([format!("%{tab}")]))["traits"]["agent"]["queue"].clone();
    assert_eq!(queued[0]["from"], format!("agent %{tab} (claude)"), "queued from an agent's own tab");
    let stop = json!({ "session_id": "s-1", "hook_event_name": "Stop", "stop_hook_active": false });
    let (status, out, _) =
        pumi(&cli, &core, tab, &["skill", "hook", "claude", "stop", "--format", "hooks"], &stop.to_string());
    assert_eq!(status, 0);
    assert_eq!(
        serde_json::from_str::<Value>(out.trim()).unwrap(),
        json!({ "decision": "block", "reason": "run the tests" })
    );
    let (_, out, _) =
        pumi(&cli, &core, tab, &["skill", "hook", "claude", "stop", "--format", "hooks"], &stop.to_string());
    assert_eq!(out.trim(), "", "one message, once");
    // Codex takes none: pumi has no way to send it one.
    let codex = json!({ "session_id": "c-1" });
    pumi(&cli, &core, tab, &["skill", "hook", "codex", "start", "--format", "hooks"], &codex.to_string());
    let (status, _, err) = pumi(&cli, &core, tab, &["agent", "queue", &format!("%{tab}"), "hi"], "");
    assert_ne!(status, 0);
    assert!(err.contains("takes no queued messages"), "{err}");
    // Nothing is listening: the hook says nothing, and still succeeds.
    let (status, out, err) =
        pumi(&cli, &core, tab, &["skill", "hook", "codex", "prompt", "--format", "hooks"], &codex.to_string());
    assert_eq!((status, out.trim(), err.trim()), (0, "", ""));
}

#[test]
fn a_hook_with_no_server_says_why_and_exits_zero() {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_pumi-cli"));
    let out = Command::new(&cli)
        .args(["skill", "hook", "claude", "stop", "--session", "s"])
        .env("PUMI", "/nonexistent/pumi.sock")
        .env("PUMI_TAB", "%1")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert_eq!(String::from_utf8_lossy(&out.stderr).lines().count(), 1, "{}", String::from_utf8_lossy(&out.stderr));
}

/// A Claude Code session in a tab of its own, as its hooks make one: its tab's number.
fn claude(cli: &Path, core: &Core, setup: &mut Conn, session: &str) -> u64 {
    let tab = setup.call("core.openTab", json!(["terminal", { "command": ["/bin/cat"] }, { "kind": "session" }]));
    let tab = tab.as_u64().unwrap();
    let start = json!({ "session_id": session, "source": "startup", "hook_event_name": "SessionStart" });
    let (status, _, err) =
        pumi(cli, core, tab, &["skill", "hook", "claude", "start", "--format", "hooks"], &start.to_string());
    assert_eq!(status, 0, "{err}");
    tab
}

/// What the Stop hook gives the agent in `tab` to go on with: the message queued for it.
fn stop(cli: &Path, core: &Core, tab: u64, session: &str) -> String {
    let stop = json!({ "session_id": session, "hook_event_name": "Stop", "stop_hook_active": false });
    let (_, out, _) =
        pumi(cli, core, tab, &["skill", "hook", "claude", "stop", "--format", "hooks"], &stop.to_string());
    match serde_json::from_str::<Value>(out.trim()) {
        Ok(said) => said["reason"].as_str().unwrap_or_default().to_string(),
        Err(_) => String::new(),
    }
}

#[test]
fn an_agent_asks_another_and_hears_its_answer() {
    let cli = cli();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let mut setup = Conn::open(&core.socket);
    let (a, b) = (claude(&cli, &core, &mut setup, "s-a"), claude(&cli, &core, &mut setup, "s-b"));
    // Where each one is.
    let (status, out, _) = pumi(&cli, &core, a, &["agent", "status", &format!("%{b}")], "");
    assert_eq!(status, 0);
    assert!(out.starts_with(&format!("%{b} claude ")), "{out}");
    let (_, out, _) = pumi(&cli, &core, a, &["agent", "list"], "");
    assert!(out.lines().any(|line| line.starts_with(&format!("%{a}*"))), "the caller's own tab is marked: {out}");
    assert_eq!(pumi(&cli, &core, a, &["agent", "wait", &format!("%{b}"), "--for", "5s"], "").0, 0, "it is not working");
    let (status, _, _) =
        pumi(&cli, &core, a, &["agent", "wait", &format!("%{b}"), "--until", "working", "--for", "200ms"], "");
    assert_eq!(status, 124, "it does not start working");
    // A asks B, and waits for the answer.
    let asking = Command::new(&cli)
        .args(["agent", "ask", &format!("%{b}"), "Will you edit src/x.rs?", "--wait", "--for", "20s"])
        .env("PUMI", &core.socket)
        .env("PUMI_TAB", format!("%{a}"))
        .env("PUMI_SETTINGS", "/nonexistent/settings.json")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // B's turn ends: it is told who asks, how to answer, and what.
    let deadline = Instant::now() + Duration::from_secs(10);
    let told = loop {
        let told = stop(&cli, &core, b, "s-b");
        if !told.is_empty() || Instant::now() > deadline {
            break told;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(told.starts_with(&format!("[pumi] A message from the claude agent in tab %{a}")), "{told}");
    assert!(told.contains(&format!("pumi agent reply %{a} \"your answer\"")), "{told}");
    assert!(told.ends_with("Will you edit src/x.rs?"), "{told}");
    // B answers; A hears it at once.
    let (status, _, err) = pumi(&cli, &core, b, &["agent", "reply", &format!("%{a}"), "No, only tests/."], "");
    assert_eq!(status, 0, "{err}");
    let asked = asking.wait_with_output().unwrap();
    assert!(asked.status.success(), "{}", String::from_utf8_lossy(&asked.stderr));
    assert_eq!(String::from_utf8_lossy(&asked.stdout).trim(), "No, only tests/.");
    // Heard, so not told again when A's turn ends.
    assert_eq!(stop(&cli, &core, a, "s-a"), "", "an answer heard is not queued too");
    // Asked without waiting, the answer comes to A as a message, saying whose it is.
    let (status, id, _) = pumi(&cli, &core, a, &["agent", "ask", &format!("%{b}"), "Done?"], "");
    assert_eq!(status, 0);
    assert!(id.trim().parse::<u64>().is_ok(), "the message's id: {id}");
    pumi(&cli, &core, b, &["agent", "reply", &format!("%{a}"), "Yes."], "");
    let told = stop(&cli, &core, a, "s-a");
    assert!(told.starts_with(&format!("[pumi] An answer from the claude agent in tab %{b} (in ")), "{told}");
    assert!(told.ends_with(":\n\nYes."), "{told}");
}
