//! `pumi` against a server of its own: `--headless --clean` reads and writes none of your
//! directories, and the commands it answers itself — `call`, the API's words (`tab list`),
//! `open`, `wait` — work on whatever server it is pointed at.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// pumid and pumi-host, built beside this crate's binary (the CLI finds them there).
fn binaries() -> PathBuf {
    // Once for every test: cargo links a binary afresh each build, even one it had, and a test
    // starting it meanwhile would find nothing there.
    static BUILT: std::sync::Once = std::sync::Once::new();
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_pumi-cli"));
    BUILT.call_once(|| {
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
        let mut build = Command::new(cargo);
        build.args(["build", "-p", "pumid", "-p", "pumi-host"]);
        if !cfg!(debug_assertions) {
            build.arg("--release");
        }
        assert!(build.status().expect("cargo builds the server").success(), "building the server failed");
    });
    cli
}

fn files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            out.push(path.clone());
            if path.is_dir() && !path.is_symlink() {
                out.extend(files(&path));
            }
        }
    }
    out
}

/// A server of its own, in a directory of its own, that several `pumi` runs talk to.
struct Core {
    child: Child,
    socket: PathBuf,
    dir: tempfile::TempDir,
}
impl Core {
    fn start(pumid: &Path) -> Core {
        let dir = tempfile::tempdir().unwrap();
        let mut command = Command::new(pumid);
        command.arg("--data-dir").arg(dir.path()).args(["--stay", "--no-persist"]).env("PUMI_LOGIN_ENV", "0");
        for sub in ["config", "data", "state", "cache"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
            command.env(format!("XDG_{}_HOME", sub.to_uppercase()), dir.path().join(sub));
        }
        let child = command.stdin(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let socket = pumi_sys::ipc::address(dir.path(), "pumi.sock");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !pumi_sys::ipc::answers(&socket) {
            assert!(Instant::now() < deadline, "the core did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        Core { child, socket, dir }
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `pumi ARGS` against `core`: its exit status, its output and its error.
fn pumi(cli: &Path, core: &Core, args: &[&str]) -> (i32, String, String) {
    let mut command = Command::new(cli);
    command.arg("--server").arg(&core.socket).args(args);
    for sub in ["config", "data", "state", "cache"] {
        command.env(format!("XDG_{}_HOME", sub.to_uppercase()), core.dir.path().join(sub));
    }
    let out = command
        .env("PUMI_SETTINGS", "/nonexistent/settings.json")
        .env_remove("PUMI")
        .env_remove("PUMI_TAB")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `pumi ARGS` as a command that has to work: its output.
fn said(cli: &Path, core: &Core, args: &[&str]) -> String {
    let (status, out, err) = pumi(cli, core, args);
    assert_eq!(status, 0, "pumi {}: {err}", args.join(" "));
    out
}

/// A terminal tab running `command`, in a session of its own: its id, as the API spells it.
fn terminal(cli: &Path, core: &Core, command: &[&str]) -> String {
    let args = json!({ "command": command }).to_string();
    let opened = said(cli, core, &["call", "tab.open", "terminal", args.as_str(), r#"{"kind":"session"}"#]);
    opened.trim().to_string()
}

#[test]
fn a_clean_headless_server_leaves_your_directories_alone() {
    let cli = binaries();
    let yours = tempfile::tempdir().unwrap();
    let plugin = tempfile::tempdir().unwrap();
    std::fs::write(
        plugin.path().join("package.json"),
        r#"{ "name": "x", "pumi": { "name": "x", "owner": "me", "api": 1 } }"#,
    )
    .unwrap();
    let mut command = Command::new(&cli);
    command.args(["--headless", "--clean", "plugin", "add"]).arg(plugin.path());
    for (var, sub) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        command.env(var, yours.path().join(sub));
    }
    command
        .env("PUMID", cli.with_file_name(pumi_sys::paths::exe("pumid")))
        .env_remove("PUMI")
        .env_remove("PUMI_DATA_DIR");
    let out = command.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(files(yours.path()), Vec::<PathBuf>::new(), "nothing of yours was written");
}

#[test]
fn a_call_is_any_function_and_the_api_is_also_its_words() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    assert_eq!(said(&cli, &core, &["call", "tab.list", "--json"]).trim(), "[]");
    assert_eq!(said(&cli, &core, &["tab", "list", "--json"]).trim(), "[]");
    let tab = terminal(&cli, &core, &["sleep", "30"]);
    let listed = said(&cli, &core, &["tab", "list", "--json"]);
    assert_eq!(serde_json::from_str::<Value>(listed.trim()).unwrap(), json!([tab]));
    let got = said(&cli, &core, &["tab", "get", tab.as_str()]);
    let got: Value = serde_json::from_str(got.trim()).unwrap();
    assert_eq!(got["id"], json!(tab), "an id is as the API spells it: `%3`");
    assert_eq!(got["block"], json!("terminal"));
}

#[test]
fn open_shows_a_file_beside_a_tab_and_prints_which() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let beside = terminal(&cli, &core, &["sleep", "30"]);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("note.md");
    std::fs::write(&file, "hi").unwrap();
    let path = file.display().to_string();
    // `pumi open PATH` opens a file tab (`tab open file`): a place's flags say where it goes.
    let printed = said(&cli, &core, &["open", "--beside", beside.as_str(), "--split", "right", path.as_str()]);
    let file_tab = printed.trim().to_string();
    assert!(file_tab.starts_with('%'), "a tab's id: {file_tab}");
    assert_ne!(file_tab, beside, "a tab of its own");
    let own = said(&cli, &core, &["open", "--kind", "view", "--json", path.as_str()]);
    let own: Value = serde_json::from_str(own.trim()).unwrap();
    assert!(own.as_str().is_some_and(|id| id.starts_with('%') && id != file_tab), "a view of its own: {own}");
    assert!(said(&cli, &core, &["tab", "get", file_tab.as_str()]).contains(r#""file""#), "it holds a file");
}

#[test]
fn open_puts_a_relative_path_where_the_caller_stands() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.md"), "hi").unwrap();
    let out = Command::new(&cli)
        .arg("--server")
        .arg(&core.socket)
        .args(["open", "--json", "note.md"])
        .current_dir(dir.path())
        .env_remove("PUMI")
        .env_remove("PUMI_TAB")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let opened: Value = serde_json::from_str(String::from_utf8_lossy(&out.stdout).trim()).unwrap();
    let id = opened.as_str().expect("a tab's id");
    let got: Value = serde_json::from_str(said(&cli, &core, &["tab", "get", id]).trim()).unwrap();
    let given = PathBuf::from(got["options"]["path"].as_str().unwrap()).canonicalize().unwrap();
    assert_eq!(given, dir.path().join("note.md").canonicalize().unwrap());
}

#[test]
fn wait_exits_with_the_program_it_waited_for() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    // Its program outlives the CLI's start: a tab goes when its program ends.
    let seven = terminal(&cli, &core, &["/bin/sh", "-c", "sleep 1; exit 7"]);
    assert_eq!(pumi(&cli, &core, &["wait", seven.as_str()]).0, 7);
    let sleeping = terminal(&cli, &core, &["sleep", "30"]);
    assert_eq!(pumi(&cli, &core, &["wait", sleeping.as_str(), "--for", "100ms"]).0, 124, "timed out");
}

#[test]
fn no_tmux_verb_is_a_command() {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_pumi-cli"));
    let out = Command::new(&cli).args(["split-window", "-h"]).env("PUMI", "/nonexistent/pumi.sock").output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no command `split-window`"), "{err}");
    let help = Command::new(&cli).arg("help").output().unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    for gone in ["split-window", "send-keys", "capture-pane", "new-session", "list-panes"] {
        assert!(!help.contains(gone), "`{gone}` is in the usage");
    }
}

/// A tab's text, once it says `want`: `pumi tab read` and its flags.
fn read_until(cli: &Path, core: &Core, args: &[&str], want: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = said(cli, core, args);
        if text.contains(want) || Instant::now() > deadline {
            return text;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_program_after_dashes_runs_and_its_screen_reads_as_rows() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    // What a full-screen program writes: rows placed by cursor moves, no newline between them.
    let draw = r"printf '\033[2J\033[1;1Hfirst row\033[2;1Hsecond row\033[3;1Hthird row'; sleep 30";
    let tab = said(&cli, &core, &["tab", "open", "terminal", "--kind", "session", "--", "/bin/sh", "-c", draw]);
    let tab = tab.trim();
    assert!(tab.starts_with('%'), "a tab's id: {tab}");
    let text = read_until(&cli, &core, &["tab", "read", tab, "--source", "visible"], "third row");
    assert_eq!(text.trim_end(), "first row\nsecond row\nthird row", "one row a line, as plain text");
    // Without a source or a number of lines: the last lines, as text.
    let text = read_until(&cli, &core, &["tab", "read", tab], "third row");
    assert!(text.trim_end().ends_with("second row\nthird row"), "{text:?}");
    // And as JSON, `{text}`.
    let json: Value = serde_json::from_str(said(&cli, &core, &["tab", "read", tab, "--json"]).trim()).unwrap();
    assert!(json["text"].as_str().is_some_and(|text| text.contains("first row\nsecond row")), "{json}");
}

#[test]
fn a_place_says_its_kind_from_its_flags_and_says_how_when_it_cannot() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let beside = terminal(&cli, &core, &["sleep", "30"]);
    // `--split` and `--beside` are a tiled place's: no `--kind tiled` needed.
    let opened =
        said(&cli, &core, &["tab", "open", "terminal", "--split", "right", "--beside", &beside, "--", "sleep", "30"]);
    let opened = opened.trim();
    let got: Value = serde_json::from_str(said(&cli, &core, &["tab", "get", opened]).trim()).unwrap();
    assert_eq!(got["place"]["kind"], "tiled", "{got}");
    let theirs: Value = serde_json::from_str(said(&cli, &core, &["tab", "get", &beside]).trim()).unwrap();
    assert_eq!(got["place"]["view"], theirs["place"]["view"], "beside it, in its view");
    // Both kinds' flags: which is it?
    let (status, _, err) = pumi(&cli, &core, &["tab", "open", "terminal", "--split", "right", "--anchor", "NE"]);
    assert_eq!(status, 2);
    assert!(err.contains("add --kind tiled or --kind float"), "{err}");
    // Neither: the server's error, and how to fix it.
    let (status, _, err) = pumi(&cli, &core, &["tab", "open", "terminal", "--no-focus"]);
    assert_eq!(status, 1);
    assert!(err.contains("missing field") && err.contains("add --kind tiled"), "{err}");
}

#[test]
fn tab_list_is_a_table_and_its_json_the_ids() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let tab = terminal(&cli, &core, &["sleep", "30"]);
    let table = said(&cli, &core, &["tab", "list"]);
    let mut lines = table.lines();
    let header = lines.next().unwrap_or_default();
    assert!(header.starts_with("TAB") && header.contains("BLOCK") && header.contains("CWD"), "{table}");
    assert!(lines.any(|line| line.starts_with(&format!("{tab} ")) && line.contains("terminal")), "{table}");
    let ids: Value = serde_json::from_str(said(&cli, &core, &["tab", "list", "--json"]).trim()).unwrap();
    assert_eq!(ids, json!([tab]));
    said(&cli, &core, &["call", "option.set", "@status", "working", &json!({ "id": tab }).to_string()]);
    let table = said(&cli, &core, &["tab", "list"]);
    assert!(table.lines().next().is_some_and(|header| header.contains("STATUS")), "{table}");
    assert!(table.lines().any(|line| line.starts_with(&format!("{tab} ")) && line.contains("working")), "{table}");
}

#[test]
fn every_command_is_listed_and_each_has_its_help() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let names = said(&cli, &core, &["registry", "list", "fn", "--names"]);
    assert!(names.lines().any(|line| line.starts_with("tab.open ") && line.contains("Open a tab")), "{names}");
    assert!(names.lines().all(|line| !line.starts_with('{')), "names, not JSON");
    let help = said(&cli, &core, &["help"]);
    for line in ["  tab read ", "  tab open "] {
        assert!(help.contains(line), "`{line}` in the listing: {help}");
    }
    // A tab method is a command, and `help` knows it by either spelling.
    for name in [&["tab.read"][..], &["tab", "read"][..]] {
        let mut args = vec!["help"];
        args.extend(name);
        let help = said(&cli, &core, &args);
        assert!(help.starts_with("pumi tab read [TAB]") && help.contains("tab.call"), "{help}");
    }
}

/// A channel that provides `name` as the app does (`fn.set`), with its schema.
fn provider(
    core: &Core,
    name: &str,
    schema: Value,
) -> (std::io::BufReader<pumi_sys::ipc::Stream>, pumi_sys::ipc::Stream) {
    use std::io::{BufRead, Write};
    let mut writer = pumi_sys::ipc::Stream::connect(&core.socket).unwrap();
    writer.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut lines = std::io::BufReader::new(writer.try_clone().unwrap());
    let mut call = |id: u64, method: &str, params: Value| {
        writeln!(writer, "{}", json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).unwrap();
        loop {
            let mut line = String::new();
            lines.read_line(&mut line).unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id && message.get("method").is_none() {
                assert!(message.get("error").is_none(), "{method}: {message}");
                return;
            }
        }
    };
    call(1, "channel.setInfo", json!(["pumi-app", {}, "ui", {}, {}]));
    call(2, "fn.set", json!([name, { "desc": "A tab's screen.", "effect": "read", "paramsSchema": schema }]));
    (lines, writer)
}

#[test]
fn a_function_whose_one_argument_is_an_object_takes_its_fields_as_words() {
    use std::io::{BufRead, Write};
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    // `tab.cells` as the app provides it: one argument, `opts`, whose `tab` is required.
    let schema = json!({
        "type": "object",
        "properties": { "opts": {
            "type": "object",
            "properties": { "tab": { "type": ["integer", "string"] }, "lines": { "type": "integer" }, "trim": { "type": "boolean" } },
            "required": ["tab"],
        } },
        "required": ["opts"],
    });
    let (mut lines, mut writer) = provider(&core, "tab.cells", schema);
    let help = said(&cli, &core, &["tab", "cells", "--help"]);
    assert!(help.starts_with("pumi tab cells TAB [--lines VALUE] [--trim|--no-trim]"), "{help}");
    assert!(said(&cli, &core, &["help", "tab.cells"]).starts_with("pumi tab cells TAB"), "help knows it too");
    // Each way of saying it is the one object the function takes.
    for (args, sent) in [
        (vec!["tab", "cells", "%3", "--lines", "5"], json!([{ "tab": "%3", "lines": 5 }])),
        (vec!["tab", "cells", "3"], json!([{ "tab": "%3" }])),
        (vec!["tab", "cells", "--tab", "%3", "--no-trim"], json!([{ "tab": "%3", "trim": false }])),
        (vec!["call", "tab.cells", r#"{"tab":"%3"}"#], json!([{ "tab": "%3" }])),
    ] {
        let child = Command::new(&cli)
            .arg("--server")
            .arg(&core.socket)
            .args(&args)
            .env_remove("PUMI_TAB")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut child = child;
        let request = loop {
            let mut line = String::new();
            if lines.read_line(&mut line).is_err() {
                let _ = child.kill();
                let out = child.wait_with_output().unwrap();
                panic!("`pumi {}` asked nothing: {}", args.join(" "), String::from_utf8_lossy(&out.stderr));
            }
            let message: Value = serde_json::from_str(&line).unwrap();
            if message.get("method").is_some() && message.get("id").is_some() {
                break message;
            }
        };
        let params = match request["method"].as_str() {
            Some("call") => request["params"][1].clone(),
            _ => request["params"].clone(),
        };
        assert_eq!(params, sent, "`pumi {}` asks for: {request}", args.join(" "));
        writeln!(writer, "{}", json!({ "jsonrpc": "2.0", "id": request["id"], "result": { "lines": [] } })).unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    }
    // A required field left out says so.
    let (status, _, err) = pumi(&cli, &core, &["tab", "cells"]);
    assert_eq!(status, 2);
    assert!(err.contains("`TAB` is missing"), "{err}");
}

#[test]
fn an_app_function_without_the_app_says_why() {
    let cli = binaries();
    let core = Core::start(&cli.with_file_name(pumi_sys::paths::exe("pumid")));
    let (status, _, err) = pumi(&cli, &core, &["tab", "cells", "%3"]);
    assert_eq!(status, 1);
    assert!(err.contains("the app's") && err.contains("pumi tab read"), "{err}");
}
