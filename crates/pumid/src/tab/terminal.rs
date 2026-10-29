use crate::kernel::TabId;
use crate::kernel::events::About;
use crate::kernel::options::Key;
use crate::kernel::state::Need;
use crate::kernel::types::{Instance, Restore, SaveOpts, Serialized};
use crate::kernel::{Kernel, Place};
use crate::term::{Note, Spawn, Term};
use pumi_api::{Error, Result, TabRef};
use pumi_proto::native::Size;
use pumi_proto::osc::{Osc, Scanner};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
#[derive(Clone, Default)]
pub struct Grids(pub Arc<Mutex<HashMap<TabId, Arc<Term>>>>);
impl Grids {
    pub fn get(&self, tab: TabId) -> Option<Arc<Term>> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).get(&tab).cloned()
    }
    pub fn insert(&self, tab: TabId, term: Arc<Term>) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).insert(tab, term);
    }
    fn remove(&self, tab: TabId) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(&tab);
    }
}
#[derive(Clone)]
pub struct Notes(pub Sender<Note>);
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}
impl Launch {
    fn from_args(args: &Value) -> Result<Launch> {
        let mut args = match args {
            Value::Null => serde_json::Map::new(),
            Value::Object(map) => map.clone(),
            other => return Err(Error::invalid_params(format!("terminal arguments are an object, not {other}"))),
        };
        if let Some(Value::String(line)) = args.get("command").cloned() {
            args.insert("command".into(), json!(["/bin/sh", "-c", line]));
        }
        serde_json::from_value(Value::Object(args)).map_err(|err| Error::invalid_params(format!("terminal: {err}")))
    }
}
/// What a tab on the terminal engine answers beside the protocol and its block's own methods;
/// `run`, `wait` and `attach` are the host's, over these.
pub const METHODS: [&str; 4] = ["respawn", "signal", "clear_history", "modes"];
/// What the host answers for a tab on the terminal engine: a block's own method may not be one.
pub const HOST_METHODS: [&str; 3] = ["run", "wait", "attach"];
/// A tab of a block on the terminal engine (`engine: "terminal"`), opened with its launch
/// (`command`, `cwd`, `env`, `keep`), or brought back: its PTY adopted when the last server handed
/// it over, else its program started again with its scrollback.
pub fn open(kernel: &mut Kernel, tab: TabId, args: &Value, restore: Option<Restore>) -> Result<Box<dyn Instance>> {
    let (launch, history) = match restore {
        Some(restore) => {
            let launch: Launch = serde_json::from_value(restore.data.clone())
                .map_err(|err| Error::invalid_params(format!("terminal state: {err}")))?;
            let carried = kernel.ext.get_mut::<crate::handoff::Adoptable>().and_then(|a| a.0.remove(&tab));
            if let Some((carried, handle, ring)) = carried {
                let notes = kernel.ext.get::<Notes>().ok_or_else(|| Error::exception("no inbox"))?.0.clone();
                let term = Term::adopt(&carried, handle, &ring, notes)
                    .map_err(|err| Error::exception(format!("adopting %{tab}: {err}")))?;
                let cwd = carried.cwd.clone().or(launch.cwd.clone()).unwrap_or_default();
                started(kernel, tab, &term, &launch, &cwd);
                return Ok(Box::new(Terminal { term, launch }));
            }
            (launch, restore.blobs.get("scrollback").cloned())
        }
        None => {
            let mut args = args.clone();
            if let Some(keep) = args.as_object_mut().and_then(|m| m.remove("keep")) {
                let keep =
                    keep.as_bool().ok_or_else(|| Error::invalid_params("terminal: `keep` is true or false"))?;
                kernel.model.options.set("remain_on_exit", Key::Tab(tab), json!(keep), "tab.open", true)?;
            }
            let mut launch = Launch::from_args(&args)?;
            if launch.cwd.is_none() {
                launch.cwd = crate::kernel::ops::default_cwd(kernel, tab);
            }
            (launch, None)
        }
    };
    let term = spawn(kernel, tab, &launch, history.as_deref())?;
    Ok(Box::new(Terminal { term, launch }))
}
const RESTORE_COMMAND: &str = "restore_command";
pub struct Terminal {
    pub term: Arc<Term>,
    pub launch: Launch,
}
fn base_env(kernel: &Kernel, session: Option<u64>) -> BTreeMap<String, String> {
    let mut base = kernel.config.base_env.clone();
    let chain: Vec<Key> = session.map(Key::Session).into_iter().chain([Key::Global]).collect();
    if let Some((Value::Object(vars), _)) = kernel.model.options.get("environment", &chain) {
        for (name, value) in vars {
            match value {
                Value::String(value) => {
                    base.insert(name, value);
                }
                Value::Bool(false) | Value::Null => {
                    base.remove(&name);
                }
                other => {
                    base.insert(name, other.to_string());
                }
            }
        }
    }
    base
}
fn spawn(kernel: &mut Kernel, tab: TabId, launch: &Launch, history: Option<&[u8]>) -> Result<Arc<Term>> {
    let notes = kernel.ext.get::<Notes>().ok_or_else(|| Error::exception("this core runs no terminals"))?.0.clone();
    let cwd = launch
        .cwd
        .clone()
        .filter(|c| std::path::Path::new(c).is_dir())
        .unwrap_or_else(|| pumi_proto::paths::home_dir().display().to_string());
    let shell = kernel
        .global_option("default_shell")
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(pumi_sys::process::default_shell);
    let restore = kernel.tab_option(tab, RESTORE_COMMAND).as_str().map(str::trim).unwrap_or("").to_string();
    let (program, args, argv0) = match &launch.command {
        Some(words) if !words.is_empty() => (words[0].clone(), words[1..].to_vec(), None),
        _ if !restore.is_empty() => {
            let then = format!("{restore}\nexec {} -l", quote(&shell));
            (shell.clone(), vec!["-c".into(), then], None)
        }
        _ => {
            let name =
                std::path::Path::new(&shell).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            (shell.clone(), Vec::new(), Some(format!("-{name}")))
        }
    };
    let view = kernel.model.view_of_tab(tab);
    let identity = crate::env::Identity { tab, view, session: view.and_then(|t| kernel.model.session_of_view(t)) };
    let context = crate::env::Context { socket: kernel.config.socket.clone(), bin_dir: kernel.config.bin_dir.clone() };
    let env = crate::env::tab_env(&base_env(kernel, identity.session), &context, &identity, &launch.env);
    let (width, height) = kernel.default_size();
    let cols = width.max(2) as u16;
    let rows = height.max(2) as u16;
    let capacity = kernel.tab_option(tab, "history_bytes").as_u64().unwrap_or(2 << 20) as usize;
    let spec = Spawn {
        program,
        args,
        argv0,
        cwd: PathBuf::from(&cwd),
        env,
        cols,
        rows,
        disclaim: kernel.global_option("own_permissions").as_bool().unwrap_or(true),
    };
    let term = Term::spawn(tab, &spec, capacity, history, notes)
        .map_err(|err| Error::exception(format!("cannot start {}: {err}", spec.program)))?;
    started(kernel, tab, &term, launch, &cwd);
    Ok(term)
}
pub fn started(kernel: &mut Kernel, tab: TabId, term: &Arc<Term>, launch: &Launch, cwd: &str) {
    if let Some(grids) = kernel.ext.get::<Grids>() {
        grids.insert(tab, term.clone());
    }
    let size = term.size();
    kernel.sized(tab, size.cols as u32, size.rows as u32);
    let _ = kernel.set_option("tab_pid", Key::Tab(tab), json!(term.shell_pid), "terminal", true);
    let _ = kernel.set_option("tab_tty", Key::Tab(tab), json!(term.tty.clone().unwrap_or_default()), "terminal", true);
    let words: Vec<String> = launch.command.iter().flatten().map(|w| quote(w)).collect();
    let _ = kernel.set_option("tab_start_command", Key::Tab(tab), json!(words.join(" ")), "terminal", true);
    let _ = kernel.set_option("tab_current_path", Key::Tab(tab), json!(cwd), "terminal", true);
    let _ = kernel.set_option("tab_dead", Key::Tab(tab), json!(false), "terminal", true);
}
impl Terminal {
    fn cwd(&self) -> Option<String> {
        self.term.cwd().or_else(|| {
            let pid = self.term.foreground().unwrap_or(self.term.shell_pid);
            pumi_sys::process::cwd(pid)
        })
    }
}
/// The lines `tab.call(…, "read")` answers with when it is not told how many, and how much of the
/// output it reads to find them.
const RECENT_LINES: usize = 200;
const READ_BYTES: usize = 256 * 1024;
fn quote(word: &str) -> String {
    if !word.is_empty() && word.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)) {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}
/// A command line as `run` writes it: the line itself, or words quoted for the shell.
fn command_line(args: &Value) -> Result<String> {
    match args.get("command") {
        Some(Value::String(line)) => Ok(line.clone()),
        Some(Value::Array(words)) if !words.is_empty() => {
            let words: Option<Vec<&str>> = words.iter().map(Value::as_str).collect();
            let words = words.ok_or_else(|| Error::invalid_params("run: `command` is a line or an array of words"))?;
            Ok(words.iter().map(|w| quote(w)).collect::<Vec<_>>().join(" "))
        }
        _ => Err(Error::invalid_params("run takes `command`, a line or an array of words")),
    }
}
/// The last `lines` lines of a terminal's output as it wrote them: its escape sequences kept.
pub fn last_lines(bytes: &[u8], lines: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    let all: Vec<&str> = text.trim_end_matches(['\r', '\n']).split('\n').collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}
/// What a source of a terminal's text reads: the screen, its scrollback, or the scrollback with
/// wrapped rows joined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Visible,
    Recent,
    Unwrapped,
}
/// The last `lines` lines of a terminal's text, from its output played onto a screen of its size
/// (`term::screen`): one line a row, so a full-screen program's rows are lines and its redraws
/// are what they drew last, not every frame run together.
pub fn plain(bytes: &[u8], cols: u16, rows: u16, source: Source, lines: usize) -> String {
    let mut screen = crate::term::screen::Screen::new(cols, rows);
    screen.feed(bytes);
    let all = match source {
        Source::Visible => screen.visible(),
        Source::Recent => screen.recent(),
        Source::Unwrapped => screen.unwrapped(),
    };
    all[all.len().saturating_sub(lines)..].join("\n")
}
impl Instance for Terminal {
    /// Text the tab shows: `visible` the screen as it is (the last screenful), `recent` its
    /// scrollback, `unwrapped` the scrollback as lines that were not wrapped at the screen's
    /// width; with `ansi` the escape sequences that wrote them are kept.
    fn read(&mut self, _: &mut Kernel, _: TabId, opts: &Value) -> Result<Value> {
        let size = self.term.size();
        let rows = usize::from(size.rows.max(1));
        let (source, default) = match opts.get("source").and_then(Value::as_str) {
            None | Some("recent") => (Source::Recent, RECENT_LINES),
            Some("unwrapped") => (Source::Unwrapped, RECENT_LINES),
            Some("visible") => (Source::Visible, rows),
            Some(other) => {
                return Err(Error::invalid_params(format!(
                    "read: `source` is visible, recent or unwrapped, not {other:?}"
                )));
            }
        };
        let lines = opts.get("lines").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(default).max(1);
        let ansi = opts.get("ansi").and_then(Value::as_bool).unwrap_or(false);
        let text = if ansi {
            last_lines(&self.term.history(READ_BYTES), lines)
        } else {
            plain(&self.term.written(READ_BYTES), size.cols, size.rows, source, lines)
        };
        Ok(json!({ "text": text }))
    }
    fn input(&mut self, _: &mut Kernel, _: TabId, data: &Value) -> Result<()> {
        let io = |r: std::io::Result<()>| r.map_err(|err| Error::exception(format!("the terminal: {err}")));
        if let Some(text) = data.get("text").and_then(Value::as_str) {
            return io(self.term.write(text.as_bytes()));
        }
        if let Some(text) = data.get("paste").and_then(Value::as_str) {
            return io(self.term.paste(text));
        }
        if let Some(bytes) = data.get("bytes").and_then(Value::as_array) {
            let bytes: Vec<u8> = bytes.iter().filter_map(Value::as_u64).map(|b| b as u8).collect();
            return io(self.term.write(&bytes));
        }
        Err(Error::invalid_params(
            "terminal input is {\"text\": …}, {\"paste\": …} or {\"bytes\": […]}; key names are encoded by the host",
        ))
    }
    fn selection(&mut self, _: &mut Kernel, _: TabId) -> Result<Value> {
        // A terminal's selection is its client's.
        Ok(json!({ "text": "" }))
    }
    fn serialize(&mut self, kernel: &mut Kernel, tab: TabId, opts: &SaveOpts) -> Result<Serialized> {
        let cwd = self.cwd().or_else(|| self.launch.cwd.clone());
        let launch = Launch { command: self.launch.command.clone(), cwd: cwd.clone(), env: self.launch.env.clone() };
        let mut serialized =
            Serialized { data: serde_json::to_value(&launch).expect("plain data"), ..Serialized::default() };
        if opts.wants("scrollback") {
            let bytes = kernel.tab_option(tab, "history_bytes").as_u64().unwrap_or(2 << 20) as usize;
            let history = self.term.history(bytes);
            if history.len() > 8 {
                serialized.blobs.push(("scrollback".into(), "vt".into(), history));
            }
        }
        if let Some(cwd) = cwd {
            serialized.needs.push(Need::Path { path: cwd });
        }
        if let Some(program) = self.launch.command.as_ref().and_then(|c| c.first()) {
            serialized.needs.push(Need::Program { name: program.clone() });
        }
        Ok(serialized)
    }
    fn deserialize(&mut self, kernel: &mut Kernel, tab: TabId, restore: Restore) -> Result<()> {
        let launch: Launch = serde_json::from_value(restore.data.clone())
            .map_err(|err| Error::invalid_params(format!("terminal state: {err}")))?;
        let term = spawn(kernel, tab, &launch, restore.blobs.get("scrollback").map(Vec::as_slice))?;
        std::mem::replace(&mut self.term, term).hang_up();
        self.launch = launch;
        Ok(())
    }
    fn call(&mut self, kernel: &mut Kernel, tab: TabId, method: &str, args: Value) -> Result<Value> {
        match method {
            "respawn" => {
                let mut launch = self.launch.clone();
                if let Some(over) = args.as_object().filter(|o| !o.is_empty()) {
                    launch = Launch::from_args(&Value::Object(over.clone()))?;
                }
                let term = spawn(kernel, tab, &launch, None)?;
                std::mem::replace(&mut self.term, term).kill();
                self.launch = launch;
                Ok(Value::Null)
            }
            "signal" => {
                let name = args.get("signal").and_then(Value::as_str).unwrap_or("TERM");
                let signal = pumi_sys::pty::Signal::parse(name)
                    .ok_or_else(|| Error::invalid_params(format!("no signal {name}")))?;
                match self.term.signal_foreground(signal) {
                    Err(err) if err.kind() == std::io::ErrorKind::Unsupported => {
                        Err(Error::exception(format!("signal {name}: {err}")))
                    }
                    // As `kill` does: a program that has just gone is not an error.
                    _ => Ok(Value::Null),
                }
            }
            "clear_history" => {
                self.term.clear();
                Ok(Value::Null)
            }
            "modes" => Ok(serde_json::to_value(self.term.modes()).expect("plain data")),
            // A command line pasted and entered in one write, so nothing comes between them.
            "run" => {
                let line = command_line(&args)?;
                let mut bytes = crate::term::paste_bytes(&line, self.term.modes().bracketed_paste());
                bytes.push(b'\r');
                self.term.write(&bytes).map_err(|err| Error::exception(format!("the terminal: {err}")))?;
                Ok(Value::Null)
            }
            _ => Err(Error::not_found(format!("terminals have no method `{method}`"))),
        }
    }
    fn resize(&mut self, _: &mut Kernel, _: TabId, cols: u32, rows: u32) {
        let size = self.term.size();
        self.term.resize(Size {
            cols: cols.min(u16::MAX as u32) as u16,
            rows: rows.min(u16::MAX as u32) as u16,
            ..size
        });
    }
    fn close(&mut self, kernel: &mut Kernel, tab: TabId) {
        self.term.hang_up();
        if let Some(alerts) = kernel.ext.get_mut::<Alerts>() {
            alerts.0.remove(&tab);
        }
        if let Some(grids) = kernel.ext.get::<Grids>() {
            grids.remove(tab);
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
pub fn changed(kernel: &mut Kernel, tab: TabId) {
    let Some(terminal) = kernel.instance::<Terminal>(tab) else { return };
    terminal.term.take_dirty();
    let title = terminal.term.title().unwrap_or_default();
    let cwd = terminal.cwd();
    let foreground = terminal.term.foreground();
    let command = foreground.and_then(pumi_sys::process::name).unwrap_or_default();
    let shell = terminal.launch.command.is_none() && foreground == Some(terminal.term.shell_pid);
    if !title.is_empty()
        && kernel.tab_option(tab, "title") != json!(title)
        && kernel.set_option("title", Key::Tab(tab), json!(title), "terminal", true).unwrap_or(false)
    {
        tab_changed(kernel, tab, "title", json!(title));
    }
    if let Some(cwd) = cwd
        && kernel.set_option("tab_current_path", Key::Tab(tab), json!(cwd), "terminal", true).unwrap_or(false)
    {
        tab_changed(kernel, tab, "cwd", json!(cwd));
    }
    if kernel.tab_option(tab, "tab_current_command") != json!(command) {
        let _ = kernel.set_option("tab_current_command", Key::Tab(tab), json!(command), "terminal", true);
    }
    if kernel.tab_option(tab, "tab_foreground_pgid") != json!(foreground) {
        let _ = kernel.set_option("tab_foreground_pgid", Key::Tab(tab), json!(foreground), "terminal", true);
    }
    if shell && kernel.tab_option(tab, RESTORE_COMMAND).as_str().is_some_and(|line| !line.is_empty()) {
        let _ = kernel.set_option(RESTORE_COMMAND, Key::Tab(tab), json!(""), "terminal", false);
    }
}
pub fn exited(kernel: &mut Kernel, tab: TabId) -> bool {
    let Some(terminal) = kernel.instance::<Terminal>(tab) else { return false };
    let Some(status) = terminal.term.exited() else { return false };
    let _ = kernel.set_option("tab_dead", Key::Tab(tab), json!(true), "terminal", true);
    let _ = kernel.set_option("tab_dead_status", Key::Tab(tab), json!(status), "terminal", true);
    tab_changed(kernel, tab, "exitCode", json!(status));
    let tiled =
        matches!(kernel.model.tabs.get(&tab).map(|p| &p.place), Some(Place::Tiled { .. } | Place::Float { .. }));
    !(tiled && kernel.tab_option(tab, "remain_on_exit") == json!(true))
}
/// `tab.changed` of one of a tab's fields, with its new value.
fn tab_changed(kernel: &mut Kernel, tab: TabId, key: &str, value: Value) {
    let id = TabRef(tab).to_string();
    kernel.emit("tab.changed", &id, About::tab(tab), json!({ "tab": id, "keys": [key], "patch": { key: value } }));
}
/// The OSC codes whose reports the host reads (`terminal.osc`): status (7501), progress (9;4)
/// and prompts (133).
const REPORTS: &[u32] = &[9, 133, 7501];
/// The OSC codes a program asks for attention with: OSC 9, a kitty notification (99), OSC 777.
const NOTIFICATIONS: &[u32] = &[9, 99, 777];
/// Each terminal's notification scanner: a kitty notification (99) can come in parts.
#[derive(Default)]
struct Alerts(HashMap<TabId, Scanner>);
/// The program rang the bell.
pub fn bell(kernel: &mut Kernel, tab: TabId) {
    alerted(kernel, tab, "bell", None);
}
/// The program sent an OSC sequence pumi passes on: a notification is `tab.alerted`, a report
/// is the host's (`terminal.osc`).
pub fn osc(kernel: &mut Kernel, tab: TabId, code: u32, data: &str) {
    if NOTIFICATIONS.contains(&code) {
        if kernel.ext.get::<Alerts>().is_none() {
            kernel.ext.insert(Alerts::default());
        }
        let alerts = kernel.ext.get_mut::<Alerts>().expect("just inserted");
        let mut heard = Vec::new();
        alerts.0.entry(tab).or_default().feed(format!("\x1b]{code};{data}\x07").as_bytes(), &mut heard);
        let kind = if code == 777 { "osc777" } else { "osc9" };
        for osc in heard {
            if let Osc::Notify { title, body } = osc {
                let message = match title.filter(|t| !t.is_empty()) {
                    Some(title) => Some(format!("{title}: {body}")),
                    None => Some(body),
                };
                alerted(kernel, tab, kind, message);
            }
        }
    }
    if REPORTS.contains(&code) {
        let fg = kernel.instance::<Terminal>(tab).and_then(|t| t.term.foreground());
        let data = json!({ "code": code, "data": data, "fg": fg });
        kernel.emit("terminal.osc", &code.to_string(), About::tab(tab), data);
    }
}
fn alerted(kernel: &mut Kernel, tab: TabId, kind: &str, message: Option<String>) {
    let data = json!({ "tab": TabRef(tab).to_string(), "kind": kind, "message": message });
    kernel.emit("tab.alerted", kind, About::tab(tab), data);
}
#[cfg(test)]
mod tests {
    use super::{Source, last_lines, plain};
    #[test]
    fn vt_of_output() {
        assert_eq!(last_lines(b"a\r\nb\r\nc\r\n", 2), "b\r\nc");
    }
    #[test]
    fn plain_text_of_output() {
        let out = b"\x1b[31mred\x1b[0m line\r\nprogress 10%\rprogress 99%\r\n\x1b]2;t\x07last";
        assert_eq!(plain(out, 80, 24, Source::Recent, 10), "red line\nprogress 99%\nlast");
        assert_eq!(plain(out, 80, 24, Source::Recent, 1), "last");
    }
    #[test]
    fn a_full_screen_programs_rows_are_lines() {
        // An agent's TUI: rows placed with cursor moves, never a newline between them.
        let out = b"\x1b[2J\x1b[1;1H\xe2\xa0\xa7 Working\x1b[K\x1b[2;1H7.4s\x1b[K\x1b[3;1H> ask me\x1b[1;1H\xe2\xa0\x8f Working\x1b[K";
        let text = plain(out, 40, 10, Source::Visible, 10);
        assert_eq!(text, "\u{280f} Working\n7.4s\n> ask me");
        assert_eq!(plain(out, 40, 10, Source::Visible, 1), "> ask me", "the last lines that hold something");
    }
}
