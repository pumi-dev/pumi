pub mod attach;
pub mod conn;
pub mod link;
#[cfg(test)]
pub mod ops;
pub mod wire;
use super::{AttachMode, Attached, Error, Notice, Read, Result, Runtime, Setup, Step, Tried, Watch};
#[cfg(test)]
use super::{Made, Op};
use crate::term::Size;
use conn::Conn;
use futures::channel::mpsc::UnboundedSender;
use link::{Effect, Input, Link};
use pumi_proto::paths::{File, Paths};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
const MAX_WAITS: u32 = 8;
pub type OnCallback = Arc<dyn Fn(&str, Value) + Send + Sync>;
/// Hears an event of a subscription the app made (`event.on`): its handle and the event.
pub type OnEvent = Arc<dyn Fn(u64, Value) + Send + Sync>;
pub type OnRequest = Arc<dyn Fn(conn::Request) + Send + Sync>;
#[derive(Default)]
struct Sinks {
    wake: Option<UnboundedSender<()>>,
    notices: Option<UnboundedSender<Notice>>,
    callback: Option<OnCallback>,
    event: Option<OnEvent>,
    request: Option<OnRequest>,
}
#[derive(Default)]
struct Latest {
    state: Mutex<Option<Value>>,
    stale: AtomicBool,
    menus: AtomicBool,
    keys: AtomicBool,
}
fn gone() -> Error {
    Error::Unreachable("the server went away".into())
}
pub struct Pumid {
    paths: Paths,
    link: Arc<Mutex<Link<Arc<Conn>>>>,
    connecting: Arc<Mutex<()>>,
    sinks: Arc<Mutex<Sinks>>,
    latest: Arc<Latest>,
    provided: Arc<Mutex<BTreeMap<String, Value>>>,
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn error(err: pumi_api::Error) -> Error {
    match err.kind {
        pumi_api::ErrorKind::ProviderUnavailable | pumi_api::ErrorKind::Timeout => Error::Unreachable(err.message),
        _ => Error::Refused(err.message),
    }
}
impl Pumid {
    pub fn new() -> Self {
        Self::at(Paths::resolve())
    }
    pub fn at(paths: Paths) -> Self {
        Self {
            paths,
            link: Arc::new(Mutex::new(Link::Idle)),
            connecting: Arc::default(),
            sinks: Arc::default(),
            latest: Arc::default(),
            provided: Arc::default(),
        }
    }
    fn handle(&self) -> Pumid {
        Pumid {
            paths: self.paths.clone(),
            link: self.link.clone(),
            connecting: self.connecting.clone(),
            sinks: self.sinks.clone(),
            latest: self.latest.clone(),
            provided: self.provided.clone(),
        }
    }
    pub fn socket(&self) -> PathBuf {
        self.paths.file(File::PumiSocket)
    }
    fn set_callback(&self, callback: OnCallback) {
        lock(&self.sinks).callback = Some(callback);
    }
    fn set_event(&self, event: OnEvent) {
        lock(&self.sinks).event = Some(event);
    }
    fn conn(&self) -> Result<Arc<Conn>> {
        if let Some(conn) = lock(&self.link).conn().filter(|c| c.alive()) {
            return Ok(conn.clone());
        }
        let _connecting = lock(&self.connecting);
        let attached = lock(&self.link).conn().map(|c| (c.clone(), c.alive()));
        match attached {
            Some((conn, true)) => return Ok(conn),
            Some((_, false)) => {
                let generation = lock(&self.link).generation();
                self.drive(Input::Lost { generation, reason: gone() }, None);
            }
            None => {}
        }
        self.drive(Input::Connect, None);
        match &*lock(&self.link) {
            Link::Attached { conn, .. } => Ok(conn.clone()),
            Link::Unanswered { reason, .. } | Link::Lost { reason, .. } => Err(reason.clone()),
            _ => Err(gone()),
        }
    }
    fn step(&self, input: Input<Arc<Conn>>) -> Vec<Effect<Arc<Conn>>> {
        let mut link = lock(&self.link);
        let (next, effects) = std::mem::replace(&mut *link, Link::Idle).next(input);
        *link = next;
        effects
    }
    fn drive(&self, input: Input<Arc<Conn>>, lines: Option<&UnboundedSender<String>>) {
        self.perform(self.step(input), lines);
    }
    fn perform(&self, effects: Vec<Effect<Arc<Conn>>>, lines: Option<&UnboundedSender<String>>) {
        let mut queue = std::collections::VecDeque::from(effects);
        while let Some(effect) = queue.pop_front() {
            if let Some(input) = self.effect(effect, lines) {
                queue.extend(self.step(input));
            }
        }
    }
    fn effect(&self, effect: Effect<Arc<Conn>>, lines: Option<&UnboundedSender<String>>) -> Option<Input<Arc<Conn>>> {
        match effect {
            Effect::CheckUpdate => {
                match pumi_proto::server::update(&crate::bins::pumid_bin(), &self.paths) {
                    Ok(true) => eprintln!("pumi: the server ran replaced binaries; it restarted as the new ones"),
                    Ok(false) => {}
                    Err(err) => eprintln!("pumi: updating the server: {err}"),
                }
                Some(Input::Updated)
            }
            Effect::Open { generation } => Some(match self.open(generation) {
                Ok(conn) => Input::Opened(conn),
                Err(err) => Input::OpenFailed(error(err)),
            }),
            Effect::Introduce(conn) => {
                let version = json!({ "string": pumi_proto::PUMI_VERSION });
                let info = json!(["pumi-app", version, "ui", {}, { "pid": std::process::id() }]);
                Some(match conn.call("channel.setInfo", info) {
                    Ok(_) => Input::Introduced,
                    Err(err) => Input::AttachFailed(error(err)),
                })
            }
            Effect::Attach(conn) => Some(match conn.call("client.attach", json!([{ "width": 200, "height": 50 }])) {
                Ok(attached) => Input::Attached {
                    client: attached["client"].as_u64().unwrap_or(0),
                    state: attached.get("state").cloned(),
                },
                Err(err) => Input::AttachFailed(error(err)),
            }),
            Effect::KeepModel(state) => {
                *lock(&self.latest.state) = state;
                self.latest.menus.store(true, Ordering::Release);
                self.latest.keys.store(true, Ordering::Release);
                self.latest.stale.store(false, Ordering::Release);
                None
            }
            Effect::Register(conn) => {
                let provided: Vec<Value> = lock(&self.provided).values().cloned().collect();
                for function in provided {
                    let name = function["name"].as_str().unwrap_or_default().to_string();
                    if let Err(err) = set_function(&name, &function, |args| conn.call("fn.set", args)) {
                        eprintln!("pumi: registering {name}: {}", err.message);
                    }
                }
                Some(Input::Registered)
            }
            Effect::ForgetModel => {
                *lock(&self.latest.state) = None;
                self.latest.stale.store(true, Ordering::Release);
                None
            }
            Effect::Wake => {
                if let Some(wake) = &lock(&self.sinks).wake {
                    let _ = wake.unbounded_send(());
                }
                None
            }
            Effect::StartServer => {
                let bin = crate::bins::pumid_bin();
                let say = |line: String| match lines {
                    Some(lines) => {
                        let _ = lines.unbounded_send(line);
                    }
                    None => eprintln!("pumi: {line}"),
                };
                say(format!(
                    "$ {} --daemon --data-dir {}",
                    bin.display(),
                    pumi_proto::paths::display(&self.paths.data)
                ));
                match pumi_proto::server::start(&bin, &self.paths) {
                    Ok(()) => say("started".into()),
                    Err(err) => say(err.to_string()),
                }
                Some(Input::Started)
            }
        }
    }
    fn open(&self, generation: u64) -> std::result::Result<Arc<Conn>, pumi_api::Error> {
        let (sinks, latest, this) = (self.sinks.clone(), self.latest.clone(), self.handle());
        let asked = self.sinks.clone();
        let requests: conn::Requests = Box::new(move |request: conn::Request| {
            // `app.profile` answers after it has measured: not on the thread every request comes in on.
            if crate::system::takes(crate::system::requested(&request).0) {
                std::thread::spawn(move || {
                    if let Some(result) = crate::system::answer_request(&request) {
                        request.answer.send(result);
                    }
                });
                return;
            }
            if let Some(result) = crate::system::answer_request(&request) {
                request.answer.send(result);
                return;
            }
            let handler = lock(&asked).request.clone();
            match handler {
                Some(handler) => handler(request),
                None => request.answer.send(Err(pumi_api::Error::new(
                    pumi_api::ErrorKind::ProviderUnavailable,
                    format!("the app runs no plugin providing {}", request.method),
                ))),
            }
        });
        Conn::open(
            &self.socket(),
            Box::new(move |note| {
                let Some((method, params)) = note else {
                    this.drive(Input::Lost { generation, reason: gone() }, None);
                    return;
                };
                let sinks = lock(&sinks);
                match (method, params) {
                    ("redraw", events) => {
                        latest.stale.store(true, Ordering::Release);
                        let events: Vec<&Value> =
                            events.as_array().map(|list| list.iter().collect()).unwrap_or_default();
                        // The menus are the registry's: `registry.changed {kind: "menu"}`. The key
                        // tables are the `keys` option, and the other two it is read with.
                        let (mut menus, mut keys) = (false, false);
                        for event in &events {
                            match event["event"].as_str().unwrap_or_default() {
                                "registry.changed" if event["data"]["kind"] == "menu" => menus = true,
                                "option.changed" => {
                                    keys |= ["keys", "prefix", "prefix2"]
                                        .contains(&event["data"]["name"].as_str().unwrap_or(""));
                                }
                                _ => {}
                            }
                        }
                        if menus {
                            latest.menus.store(true, Ordering::Release);
                        }
                        if keys {
                            latest.keys.store(true, Ordering::Release);
                        }
                        let notices = events.iter().filter_map(|e| notice(e));
                        if let Some(tx) = &sinks.notices {
                            for n in notices {
                                let _ = tx.unbounded_send(n);
                            }
                        }
                        if let Some(wake) = &sinks.wake {
                            let _ = wake.unbounded_send(());
                        }
                    }
                    ("message.notify", params) => {
                        crate::system::post_message(&params);
                        if let (Some(n), Some(tx)) = (message(&params), &sinks.notices) {
                            let _ = tx.unbounded_send(n);
                        }
                    }
                    ("message.withdraw", params) => crate::system::withdraw_message(&params),
                    ("rpc.event", params) => {
                        if let (Some(n), Some(tx)) = (params.get(1).and_then(notice), &sinks.notices) {
                            let _ = tx.unbounded_send(n);
                        }
                        let subscription = params.get(0).and_then(Value::as_u64);
                        if let (Some(subscription), Some(event)) = (subscription, sinks.event.clone()) {
                            drop(sinks);
                            event(subscription, params.get(1).cloned().unwrap_or(Value::Null));
                        }
                    }
                    ("rpc.callback", params) => {
                        let reference = params.get(0).and_then(Value::as_str).unwrap_or_default().to_string();
                        let args = params.get(1).cloned().unwrap_or(Value::Null);
                        if let Some(callback) = sinks.callback.clone() {
                            drop(sinks);
                            callback(&reference, args);
                        }
                    }
                    _ => {}
                }
            }),
            requests,
        )
    }
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        let conn = self.conn()?;
        let result = conn.call(method, params).map_err(error);
        if !matches!(method, "server.getState" | "option.get" | "registry.list" | "fs.list") {
            self.latest.stale.store(true, Ordering::Release);
        }
        result
    }
    fn reachable(&self) -> bool {
        self.conn().is_ok()
    }
    fn state(&self) -> Result<Value> {
        if !self.latest.stale.swap(false, Ordering::AcqRel)
            && let Some(state) = lock(&self.latest.state).clone()
        {
            return Ok(state);
        }
        let got = self.request("server.getState", json!([]))?;
        let state = got.get("state").cloned().unwrap_or_default();
        *lock(&self.latest.state) = Some(state.clone());
        Ok(state)
    }
    /// The server's key tables (the `keys` option: a table, then a key, then `{call, args}`) as
    /// the replica and the app's own list of bindings.
    fn replica(&self) -> Result<(pumi_lang::keys::Replica, Vec<Value>)> {
        let keys = self.request("option.get", json!(["keys", { "scope": "global" }]))?;
        let text = |v: Value| v.as_str().unwrap_or_default().to_string();
        let option = |name: &str, opts: Value| self.request("option.get", json!([name, opts])).map(text);
        let prefixes = vec![option("prefix", json!({}))?, option("prefix2", json!({}))?];
        let client = lock(&self.link).client().unwrap_or(0);
        // The keyboard mode is the caller's own option, which `option.get` does not name.
        let mode = self
            .request("core.getOptionValue", json!(["key_table", { "scope": "client", "client": client }]))
            .map(text)
            .unwrap_or_default();
        let mut bindings = Vec::new();
        for (table, keys) in keys.as_object().into_iter().flatten() {
            for (key, _) in keys.as_object().into_iter().flatten() {
                bindings.push(json!({ "table": table, "key": key, "tab": Value::Null }));
            }
        }
        Ok((pumi_lang::keys::Replica::from_bindings(&bindings, prefixes, mode), bindings))
    }
    fn start(&self, lines: UnboundedSender<String>) {
        let effects = {
            let _connecting = lock(&self.connecting);
            self.step(Input::Start)
        };
        self.perform(effects, Some(&lines));
    }
}
impl Default for Pumid {
    fn default() -> Self {
        Self::new()
    }
}
/// Provides `function` as `name` with `set` (a `fn.set` call); a server from before `runsOn`
/// refuses that field, and is given the function without it.
fn set_function<E: std::fmt::Display>(
    name: &str,
    function: &Value,
    mut set: impl FnMut(Value) -> std::result::Result<Value, E>,
) -> std::result::Result<Value, E> {
    let spec = spec_of(function);
    match set(json!([name, spec])) {
        Err(err) if spec.get("runsOn").is_some() && err.to_string().contains("runsOn") => {
            let mut older = spec;
            if let Some(fields) = older.as_object_mut() {
                fields.remove("runsOn");
            }
            set(json!([name, older]))
        }
        done => done,
    }
}
/// A function the app's JS provides, as `fn.set` takes it: what it does, its arguments and its
/// answer. The app describes it as the export does (`params_schema`), the wire spells it camelCase.
fn spec_of(function: &Value) -> Value {
    let field = |name: &str| function.get(name).cloned().filter(|v| !v.is_null());
    let mut spec = json!({ "desc": function["desc"].as_str().unwrap_or_default() });
    if let Some(effect) = field("effect") {
        spec["effect"] = effect;
    }
    // The app's own (its clipboard, its updater) answer its own client's calls; a plugin's are
    // the plugin host's, for everyone.
    if function["runs_on"] == "client" {
        spec["runsOn"] = json!("client");
    }
    for (from, to) in [("params_schema", "paramsSchema"), ("returns_schema", "returnsSchema")] {
        if let Some(schema) = field(from) {
            spec[to] = schema;
        }
    }
    spec
}
/// A tab's program asking for attention (`tab.alerted`): a notice, the `bell` heading one whose
/// program said nothing. Its `message` is the whole text a notification carried (`title: body`
/// for OSC 777), split again where it says so.
fn notice(event: &Value) -> Option<Notice> {
    if event.get("event")?.as_str()? != "tab.alerted" {
        return None;
    }
    let data = event.get("data")?;
    let tab = event.get("tab").and_then(Value::as_str).map(str::to_string);
    let (title, body) = match data.get("message").and_then(Value::as_str).filter(|m| !m.is_empty()) {
        Some(message) => match message.split_once(": ") {
            Some((title, body)) if !title.is_empty() => (title.to_string(), Some(body.to_string())),
            _ => (message.to_string(), None),
        },
        None => ("bell".to_string(), None),
    };
    Some(Notice { tab, title, body, message: None })
}
fn message(params: &Value) -> Option<Notice> {
    let text = params.get(0)?.as_str()?.to_string();
    let level = params.get(1).and_then(Value::as_str).unwrap_or("info").to_string();
    if text.is_empty() || level == "debug" {
        return None;
    }
    let opts = params.get(2).cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let tab = opts.get("tab").and_then(crate::system::tab_of);
    let title = opts.get("title").and_then(Value::as_str).unwrap_or_default().to_string();
    Some(Notice { tab, title, body: Some(text), message: Some((level, opts)) })
}
struct Watcher {
    stop: Arc<AtomicBool>,
}
impl Watch for Watcher {}
impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
#[cfg(test)]
impl ops::Calls for Pumid {
    fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.request(method, params)
    }
    fn state(&self) -> Result<Value> {
        Pumid::state(self)
    }
}
impl Runtime for Pumid {
    fn location(&self) -> String {
        pumi_proto::paths::display(&self.socket())
    }
    fn read(&self, signatures: &BTreeMap<String, String>) -> Result<Read> {
        let mut read = wire::read(&self.state()?, signatures);
        if self.latest.menus.swap(false, Ordering::AcqRel) {
            read.menus = self.request("registry.list", json!(["menu"])).ok().and_then(|v| v.as_array().cloned());
        }
        if self.latest.keys.swap(false, Ordering::AcqRel)
            && let Ok((keys, bindings)) = self.replica()
        {
            (read.keys, read.bindings) = (Some(keys), Some(bindings));
        }
        Ok(read)
    }
    fn watch(&self, wake: UnboundedSender<()>, notices: UnboundedSender<Notice>) -> Box<dyn Watch> {
        {
            let mut sinks = lock(&self.sinks);
            sinks.wake = Some(wake.clone());
            sinks.notices = Some(notices);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let this = self.handle();
        std::thread::Builder::new()
            .name("pumi-watch".into())
            .spawn(move || {
                let mut delay = Duration::from_millis(250);
                while !stopped.load(Ordering::Relaxed) && !wake.is_closed() {
                    match this.conn() {
                        Ok(conn) => {
                            delay = Duration::from_millis(250);
                            let _ = wake.unbounded_send(());
                            while conn.alive() && !stopped.load(Ordering::Relaxed) {
                                std::thread::sleep(Duration::from_millis(200));
                            }
                        }
                        Err(_) => {
                            std::thread::sleep(delay);
                            delay = (delay * 2).min(Duration::from_secs(5));
                        }
                    }
                }
            })
            .ok();
        Box::new(Watcher { stop })
    }
    #[cfg(test)]
    fn apply(&self, op: Op) -> Result<Made> {
        ops::apply(self, op)
    }
    fn attach(
        &self,
        tab: &str,
        mode: AttachMode,
        size: Size,
        resume: Option<u64>,
    ) -> std::result::Result<Attached, String> {
        attach::attach(&self.paths.file(File::GridSocket), tab, mode, size, None, resume)
    }
    fn setup(&self) -> Option<&dyn Setup> {
        Some(self)
    }
    fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.request(method, params)
    }
    fn on_callback(&self, callback: OnCallback) {
        self.set_callback(callback)
    }
    fn on_event(&self, event: OnEvent) {
        self.set_event(event)
    }
    fn call_as(&self, method: &str, params: Value, caller: Option<&Value>) -> Result<Value> {
        let conn = self.conn()?;
        self.latest.stale.store(true, Ordering::Release);
        conn.call_as(method, params, caller).map_err(error)
    }
    fn provide(&self, function: Value) -> Result<()> {
        let name = function["name"].as_str().unwrap_or_default().to_string();
        lock(&self.provided).insert(name.clone(), function.clone());
        match set_function(&name, &function, |args| self.request("fn.set", args)) {
            Err(Error::Unreachable(_)) => Ok(()),
            done => done.map(drop),
        }
    }
    fn on_request(&self, handler: OnRequest) {
        lock(&self.sinks).request = Some(handler);
    }
    fn status(&self) -> Result<Value> {
        let count = |answer: Result<Value>| answer.ok().and_then(|v| v.as_array().map(Vec::len)).unwrap_or(0);
        Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "pid": std::process::id(),
            "tabs": count(self.request("tab.list", json!([{}]))),
            "sessions": count(self.request("session.list", json!([]))),
            "plugins": count(self.request("registry.list", json!(["plugin", {}]))),
        }))
    }
    fn log(&self) -> Option<PathBuf> {
        Some(self.paths.file(File::Log))
    }
    fn restart(&self) -> Result<Value> {
        pumi_proto::server::restart(&crate::bins::pumid_bin(), &self.paths)
            .map(|()| json!({ "said": ["restarted"] }))
            .map_err(|err| Error::Refused(err.to_string()))
    }
}
impl Setup for Pumid {
    fn next(&self, tried: &Tried) -> Step {
        if self.reachable() {
            return Step::Ready;
        }
        if !tried.has("start") {
            let this = self.handle();
            return Step::Run {
                id: "start",
                title: "Starting the pumi server…".into(),
                detail: "Running pumid in the background; terminals live there.".into(),
                work: Box::new(move |lines| this.start(lines)),
            };
        }
        if tried.waits < MAX_WAITS {
            return Step::Wait {
                title: "Waiting for the pumi server…".into(),
                detail: "Knocking on its socket until it answers.".into(),
            };
        }
        let log = pumi_proto::paths::display(&self.paths.file(File::Log));
        Step::GaveUp {
            title: "The pumi server did not start".into(),
            reason: format!("Nothing answered at {}. Its log is {log}.", self.location()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::TerminalSession;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    /// A block the core runs on its terminal engine, as the terminal plugin's manifest declares it.
    const TERM: &str = "test.term.shell";
    struct Server {
        bin: std::path::PathBuf,
        dir: std::path::PathBuf,
        persist: bool,
        child: Option<std::process::Child>,
    }
    impl Server {
        fn spawn(name: &str, persist: bool) -> Option<Server> {
            let bin = std::env::current_exe().ok()?.parent()?.parent()?.join("pumid");
            if !bin.exists() {
                eprintln!("skipped: {} not built", bin.display());
                return None;
            }
            let dir = std::env::temp_dir().join(format!("pumi-app-{name}-{}", std::process::id()));
            let mut server = Server { bin, dir, persist, child: None };
            server.start();
            Some(server)
        }
        fn start(&mut self) {
            let mut command = std::process::Command::new(&self.bin);
            if !self.persist {
                command.arg("--no-persist");
            }
            let child = command
                .args(["--stay", "--data-dir"])
                .arg(&self.dir)
                .env("PUMI_LOGIN_ENV", "0")
                .env("SHELL", "/bin/sh")
                .env("PS1", "$ ")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            self.child = Some(child);
            let runtime = self.runtime();
            until("the server", || runtime.reachable());
            // The host declares the blocks its plugins have: wait for it, then add the terminal's.
            let deadline = Instant::now() + pumi_proto::server::WITHIN;
            while runtime.request("registry.list", json!(["fn"])).is_err() {
                assert!(Instant::now() < deadline, "the host did not start");
                std::thread::sleep(Duration::from_millis(50));
            }
            declare_term(&runtime);
        }
        fn exited(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.wait();
            }
        }
        fn runtime(&self) -> Pumid {
            Pumid::at(Paths::at(&self.dir))
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
    fn until(what: &str, mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + pumi_proto::server::WITHIN;
        while !f() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    /// Declares the terminal engine's block, and what a new tab holds, once the host has declared
    /// its own: a host that starts or reconnects replaces the whole list, so this goes after it.
    fn declare_term(runtime: &Pumid) {
        let deadline = Instant::now() + pumi_proto::server::WITHIN;
        let listed = |runtime: &Pumid| {
            runtime
                .request("core.blocks", json!([]))
                .ok()
                .and_then(|list| list.as_array().map(|list| list.iter().any(|b| b["id"] == TERM)))
                .unwrap_or(false)
        };
        while listed(runtime) {
            assert!(Instant::now() < deadline, "the host never declared its own blocks again");
            std::thread::sleep(Duration::from_millis(20));
        }
        let block = json!({"id": TERM, "provider": "test/term", "engine": "terminal"});
        runtime.request("core.setBlocks", json!([[block]])).expect("the core declares the terminal's block");
        runtime
            .request("core.setOptionValue", json!(["new_tab", TERM, {"scope": "global"}]))
            .expect("a new tab holds the terminal's block");
    }
    #[test]
    fn keys_reach_the_program_in_its_keyboard_mode() {
        let Some(server) = Server::spawn("keys", false) else { return };
        let runtime = Arc::new(server.runtime());
        let made = runtime.apply(Op::NewWorkspace { cwd: Some("/tmp".into()), env: None, server: None }).unwrap();
        let tab = made.tabs.first().cloned().expect("a root tab");
        let (session, _changes) = TerminalSession::new(&tab, runtime);
        session.resize(Size { cols: 60, rows: 10, cell_width: 8, cell_height: 16 });
        let screen = || session.grid().lines().collect::<Vec<_>>().join("\n");
        until("the prompt", || screen().contains('$'));
        session.send_input(b"printf '\\033[>1u'; stty raw -echo; cat -v\r");
        std::thread::sleep(Duration::from_millis(400));
        let key = |name: &str| {
            session.key(&pumi_keys::parse(name).unwrap(), pumi_keys::OptionAsAlt::Both, pumi_keys::Action::Press)
        };
        assert!(key("shift+enter") && key("alt+b") && key("ctrl+c"));
        until("the kitty keys", || screen().contains("^[[13;2u^[[98;3u"));
    }
    #[test]
    fn probes_are_answered_through_the_app() {
        let Some(server) = Server::spawn("probe", false) else { return };
        let runtime = Arc::new(server.runtime());
        let made = runtime.apply(Op::NewWorkspace { cwd: Some("/tmp".into()), env: None, server: None }).unwrap();
        let tab = made.tabs.first().cloned().expect("a root tab");
        let (session, _changes) = TerminalSession::new(&tab, runtime);
        session.resize(Size { cols: 80, rows: 10, cell_width: 8, cell_height: 16 });
        let screen = || session.grid().lines().collect::<Vec<_>>().join("\n");
        until("the prompt", || screen().contains('$'));
        session.send_input(
            br"stty raw -echo; printf '\033_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\033\\\033]7501;?\033\\\033[c'; cat -v",
        );
        session.send_input(b"\r");
        until("the replies", || screen().contains("62;4;22c"));
        let text = screen();
        assert_eq!(text.matches("Gi=31;OK").count(), 1, "{text}");
        assert_eq!(text.matches("7501;?:states=").count(), 1, "{text}");
        assert_eq!(text.matches("62;4;22c").count(), 1, "DA1 says sixel, once: {text}");
    }
    #[test]
    fn a_sixel_reaches_the_apps_screen() {
        let Some(server) = Server::spawn("image", false) else { return };
        let runtime = Arc::new(server.runtime());
        let made = runtime.apply(Op::NewWorkspace { cwd: Some("/tmp".into()), env: None, server: None }).unwrap();
        let tab = made.tabs.first().cloned().expect("a root tab");
        let (session, _changes) = TerminalSession::new(&tab, runtime);
        session.resize(Size { cols: 80, rows: 20, cell_width: 8, cell_height: 16 });
        until("the prompt", || session.grid().lines().any(|line| line.contains('$')));
        session.send_input(b"stty raw -echo; printf '\\033Pq#1;2;100;0;0!8~\\033\\\\'\r");
        until("the image", || !session.grid().images.is_empty());
        let placed = &session.grid().images[0];
        assert_eq!((placed.size.0, placed.size.1), (8, 6), "its own pixels, not scaled");
        assert_eq!(&placed.image.as_bytes(0).unwrap()[..4], &[0, 0, 255, 255], "BGRA red");
    }
    #[test]
    fn the_model_is_pushed_and_notices_arrive() {
        let Some(server) = Server::spawn("watch", false) else { return };
        let runtime = server.runtime();
        let (wake, mut woken) = futures::channel::mpsc::unbounded();
        let (notices, mut heard) = futures::channel::mpsc::unbounded();
        let _watch = runtime.watch(wake, notices);
        until("the first state", || woken.try_recv().is_ok());
        let made = runtime.apply(Op::NewWorkspace { cwd: Some("/tmp".into()), env: None, server: None }).unwrap();
        let view = made.view.clone().expect("a view");
        let read = runtime.read(&BTreeMap::new()).unwrap();
        assert_eq!(read.snapshot.workspaces.len(), 1, "an operation is seen by the next read");
        assert!(read.trees.contains_key(&view));
        until("the pushed change", || woken.try_recv().is_ok());
        let read = runtime.read(&BTreeMap::new()).unwrap();
        let (signature, _) = read.trees[&view].clone();
        let again = runtime.read(&BTreeMap::from([(view.clone(), signature)])).unwrap();
        assert!(again.trees.is_empty(), "unchanged trees are not sent again");
        let args = json!({ "command": ["/usr/bin/printf", "\u{1b}]777;notify;done;it built\u{7}"] });
        let place = json!({ "kind": "view", "session": made.workspace, "focus": false });
        let tab = runtime.request("tab.open", json!([TERM, args, place])).unwrap();
        let tab = tab.as_str().unwrap().to_string();
        let mut notice = None;
        until("the notice", || {
            notice = heard.try_recv().ok();
            notice.is_some()
        });
        assert_eq!(
            notice.unwrap(),
            Notice { tab: Some(tab), title: "done".into(), body: Some("it built".into()), message: None }
        );
    }
    #[test]
    fn a_terminal_goes_on_through_a_restart_and_a_stop() {
        let Some(mut server) = Server::spawn("through", true) else { return };
        let runtime = Arc::new(server.runtime());
        let made = runtime.apply(Op::NewWorkspace { cwd: Some("/tmp".into()), env: None, server: None }).unwrap();
        let tab = made.tabs.first().cloned().expect("a root tab");
        let (session, _changes) = TerminalSession::new(&tab, runtime.clone());
        session.resize(Size { cols: 60, rows: 10, cell_width: 8, cell_height: 16 });
        let screen = || session.grid().lines().collect::<Vec<_>>().join("\n");
        until("the prompt", || screen().contains('$'));
        session.send_input(b"echo one-$((1+1))\r");
        until("the first echo", || screen().contains("one-2"));
        runtime.restart().unwrap();
        declare_term(&runtime);
        session.send_input(b"echo two-$((1+2))\r");
        until("the echo after the restart", || screen().contains("two-3"));
        assert!(screen().contains("one-2"), "the same program and screen: {}", screen());
        runtime.request("server.kill", json!([])).unwrap_or_default();
        server.exited();
        server.start();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !screen().contains("three-4") {
            assert!(Instant::now() < deadline, "the restored tab never answered: {}", screen());
            session.send_input(b"echo three-$((1+3))\r");
            std::thread::sleep(Duration::from_millis(300));
        }
        assert!(!session.closed(), "attached again, not closed");
    }
    fn picture(runtime: &Pumid) -> Value {
        let read = runtime.read(&BTreeMap::new()).unwrap();
        let snap = &read.snapshot;
        let server = json!(
            snap.workspaces
                .iter()
                .map(|w| json!({
                    "space": w.label,
                    "current": w.active_view_id.as_ref().and_then(|t| snap.view(t)).map(|t| t.label.clone()),
                    "views": snap.views.iter().filter(|v| v.workspace_id == w.workspace_id).map(|v| json!({
                        "name": v.label,
                        "tree": format!("{:?}", read.trees.get(&v.view_id).map(|(_, tree)| tree)),
                        "focus": snap.layout(&v.view_id).and_then(|l| l.focused_tab_id.clone()),
                    })).collect::<Vec<_>>(),
                }))
                .collect::<Vec<_>>()
        );
        server
    }
    #[test]
    fn closing_and_opening_the_app_keeps_everything_in_place() {
        let Some(mut server) = Server::spawn("reopen", true) else { return };
        let first = server.runtime();
        let mut spaces = Vec::new();
        for (name, views) in [("alpha", 3), ("beta", 2)] {
            let made = first.apply(Op::NewWorkspace { cwd: Some("/tmp".into()), env: None, server: None }).unwrap();
            let space = made.workspace.clone().unwrap();
            first.apply(Op::RenameWorkspace { workspace: space.clone(), label: name.into() }).unwrap();
            let mut root = made.tabs[0].clone();
            let mut view = made.view.clone().unwrap();
            for n in 0..views {
                if n > 0 {
                    let made = first.apply(Op::NewView { workspace: space.clone(), cwd: None, env: None }).unwrap();
                    (root, view) = (made.tabs[0].clone(), made.view.clone().unwrap());
                }
                first.apply(Op::RenameView { view: view.clone(), label: format!("{name}-{n}") }).unwrap();
                let right = first.apply(Op::Split {
                    target: root.clone(),
                    axis: crate::runtime::tree::Axis::Row,
                    cwd: None,
                    env: None,
                });
                let right = right.unwrap().tabs[0].clone();
                if n % 2 == 0 {
                    first
                        .apply(Op::Split {
                            target: right.clone(),
                            axis: crate::runtime::tree::Axis::Column,
                            cwd: None,
                            env: None,
                        })
                        .unwrap();
                }
                first.request("tab.focus", json!([right])).unwrap();
            }
            spaces.push(space);
        }
        let before = picture(&first);
        assert_eq!(before.as_array().unwrap().len(), 2, "{before}");
        assert!(before.to_string().contains("alpha-2") && before.to_string().contains("Split"), "{before}");
        drop(first);
        let again = server.runtime();
        until("the server, again", || again.reachable());
        assert_eq!(picture(&again), before, "a client that starts again sees the same");
        again.request("server.restart", json!([])).unwrap_or_default();
        drop(again);
        let restarted = server.runtime();
        until("the restarted core", || {
            restarted.reachable() && restarted.read(&BTreeMap::new()).is_ok_and(|r| r.snapshot.workspaces.len() == 2)
        });
        declare_term(&restarted);
        assert_eq!(picture(&restarted), before, "the same after the core restarted in place");
        restarted.request("server.kill", json!([])).unwrap_or_default();
        drop(restarted);
        server.exited();
        server.start();
        let restored = server.runtime();
        until("the restored core", || {
            restored.reachable() && restored.read(&BTreeMap::new()).is_ok_and(|r| r.snapshot.workspaces.len() == 2)
        });
        assert_eq!(picture(&restored), before, "the same after the core stopped and started from its state");
    }
}
