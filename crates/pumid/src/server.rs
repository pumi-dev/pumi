use crate::kernel::events::{About, flush};
use crate::kernel::{Chan, Config as KernelConfig, Kernel, SERVER};
use crate::tab::terminal::{Grids, Notes};
use crate::term::Note;
use pumi_api::{ApiInfo, Caller, Registry};
use pumi_proto::paths::{File, Paths};
use pumi_sys::ipc::Listener;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
pub struct Config {
    pub paths: Paths,
    pub bin_dir: Option<PathBuf>,
    pub persist: bool,
    pub idle_exit: Option<Duration>,
    pub name: String,
    pub headless: bool,
    pub host: Option<Vec<String>>,
    pub resume: Option<PathBuf>,
    /// The server this one restarts ([`crate::handoff::RESTART_AFTER`]): its pid, waited for
    /// before this one takes the lock and the sockets.
    pub restart_after: Option<u32>,
}
impl Config {
    pub fn new(paths: Paths) -> Config {
        Config {
            paths,
            bin_dir: None,
            persist: true,
            idle_exit: Some(Duration::from_secs(30)),
            name: pumi_sys::hostname(),
            headless: false,
            host: Some(Vec::new()),
            resume: None,
            restart_after: None,
        }
    }
}
enum Msg {
    Open { chan: u64, out: Sender<String> },
    Line { chan: u64, text: String },
    Close { chan: u64 },
    Note(Note),
    SidecarExited(u32),
}
pub fn log(message: &str) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    eprintln!("[{now}] pumid: {message}");
}
fn lock(paths: &Paths) -> io::Result<Option<std::fs::File>> {
    let path = paths.file(File::ServerLock);
    let file = std::fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(&path)?;
    if file.try_lock().is_err() {
        return Ok(None);
    }
    file.set_len(0)?;
    let mut file = file;
    writeln!(file, "{}", std::process::id())?;
    Ok(Some(file))
}
pub fn run(config: Config, ready: impl FnOnce()) -> io::Result<()> {
    config.paths.ensure()?;
    if let Some(pid) = config.restart_after {
        crate::handoff::await_exit(pid);
    }
    let Some(_lock) = lock(&config.paths)? else {
        log("another core is running on this data directory");
        return Ok(());
    };
    let socket = config.paths.file(File::PumiSocket);
    let grid_socket = config.paths.file(File::GridSocket);
    let follows = config.resume.is_some() || config.restart_after.is_some();
    let rpc = bind(&socket, follows)?;
    let grid = bind(&grid_socket, follows)?;
    let (inbox, messages) = channel::<Msg>();
    let mut kernel = Kernel::new(KernelConfig {
        name: config.name.clone(),
        data_dir: config.persist.then(|| config.paths.data.clone()),
        socket: socket.clone(),
        bin_dir: config.bin_dir.clone(),
        base_env: crate::env::login_env(&config.paths.file(File::LoginEnv), false),
    });
    crate::tab::register(&mut kernel).map_err(|err| io::Error::other(err.message))?;
    let grids = Grids::default();
    kernel.ext.insert(grids.clone());
    let (notes, note_rx) = channel::<Note>();
    kernel.ext.insert(Notes(notes));
    {
        let inbox = inbox.clone();
        thread::Builder::new().name("pumid-notes".into()).spawn(move || {
            for note in note_rx {
                if inbox.send(Msg::Note(note)).is_err() {
                    return;
                }
            }
        })?;
    }
    let resumed = config.resume.as_ref().and_then(|path| match crate::handoff::receive(path) {
        Ok(handoff) => Some(handoff),
        Err(err) => {
            log(&format!("the handoff could not be read ({err}); starting from the saved state"));
            None
        }
    });
    let mut carried_sidecars = None;
    if let Some(handoff) = resumed {
        kernel.ext.insert(handoff.terminals);
        carried_sidecars = Some(handoff.sidecars);
    }
    let registry = Registry::new();
    let server = Caller { channel: SERVER, ..Caller::default() };
    let transient = config.paths.data.join("handoff-state");
    let restarted = config.resume.is_some() || config.restart_after.is_some();
    if !config.persist && restarted {
        kernel.set_data_dir(Some(transient.clone()));
    }
    let restored = registry.with_cx(&mut kernel, &server, crate::kernel::persist::restore).unwrap_or_else(|err| {
        log(&format!("restoring: {}", err.message));
        0
    });
    if !config.persist {
        kernel.set_data_dir(None);
        let _ = std::fs::remove_dir_all(&transient);
    }
    crate::handoff::forget(&mut kernel);
    kernel.emit("core.started", "", About::default(), json!({ "restarted": restarted }));
    log(&format!("pid {} serving {} ({restored} tabs restored)", std::process::id(), socket.display()));
    accept_rpc(rpc, inbox.clone())?;
    crate::attach::serve(grid, grids.clone())?;
    let exited: crate::host::Exited = {
        let inbox = inbox.clone();
        std::sync::Arc::new(move |pid| drop(inbox.send(Msg::SidecarExited(pid))))
    };
    let sidecars = match &config.host {
        Some(_) => crate::host::adopt(carried_sidecars.unwrap_or_default(), &start_of(&config, &exited)),
        None => Vec::new(),
    };
    kernel.ext.insert(crate::lifecycle::Running {
        core: crate::lifecycle::exe().ok().and_then(|exe| crate::lifecycle::Image::of(&exe)),
        host: sidecars.iter().find_map(|s| s.image.clone()),
    });
    ready();
    let mut lp = Loop {
        registry: &registry,
        kernel: &mut kernel,
        config: &config,
        outs: HashMap::new(),
        routed: Routed::default(),
        starting: !sidecars.is_empty(),
        since: Instant::now(),
        waiting: Vec::new(),
        sidecars,
        exited,
        host_due: None,
        host_backoff: Duration::ZERO,
        host_started: Instant::now(),
    };
    let restart = lp.run(&messages);
    let sidecars = lp.sidecars;
    if restart && pumi_sys::caps().handoff {
        drop(_lock);
        let err = crate::handoff::exec(&mut kernel, &config, sidecars);
        log(&format!("restarting failed ({err}); stopping"));
        return Err(err);
    }
    if restart && let Err(err) = crate::handoff::relaunch() {
        log(&format!("restarting failed ({err}); stopping"));
    }
    let _ = std::fs::remove_file(&socket);
    let _ = std::fs::remove_file(&grid_socket);
    log("stopped");
    Ok(())
}
/// How long a server that follows a restart waits for its sockets to be free.
const SOCKETS_FREED: Duration = Duration::from_secs(2);
/// Listens on `path`. A server that `follows` a restart takes over the sockets of the one
/// before, whose listener a child it forked just before can hold for some ms until that child
/// execs: it waits for that, at most [`SOCKETS_FREED`].
fn bind(path: &std::path::Path, follows: bool) -> io::Result<Listener> {
    let deadline = Instant::now() + SOCKETS_FREED;
    loop {
        match Listener::bind(path) {
            Err(err) if follows && err.kind() == io::ErrorKind::AddrInUse && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5));
            }
            bound => return bound,
        }
    }
}
fn start_of<'a>(config: &'a Config, exited: &'a crate::host::Exited) -> crate::host::Start<'a> {
    crate::host::Start {
        paths: &config.paths,
        host_args: config.host.as_deref().unwrap_or_default(),
        headless: config.headless,
        bin_dir: config.bin_dir.as_deref(),
        exited,
    }
}
fn accept_rpc(listener: Listener, inbox: Sender<Msg>) -> io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    thread::Builder::new().name("pumid-rpc".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let chan = NEXT.fetch_add(1, Ordering::Relaxed);
            let (out, outgoing) = channel::<String>();
            if inbox.send(Msg::Open { chan, out }).is_err() {
                return;
            }
            let Ok(mut writer) = stream.try_clone() else { continue };
            let _ = thread::Builder::new().name(format!("pumid-chan-{chan}-out")).spawn(move || {
                let mut batch = String::new();
                while let Ok(line) = outgoing.recv() {
                    batch.clear();
                    batch.push_str(&line);
                    batch.push('\n');
                    while let Ok(line) = outgoing.try_recv() {
                        batch.push_str(&line);
                        batch.push('\n');
                    }
                    if writer.write_all(batch.as_bytes()).is_err() {
                        break;
                    }
                }
                let _ = writer.shutdown(std::net::Shutdown::Both);
            });
            let inbox = inbox.clone();
            let _ = thread::Builder::new().name(format!("pumid-chan-{chan}-in")).spawn(move || {
                let reader = BufReader::with_capacity(1 << 16, stream);
                for line in reader.lines() {
                    let Ok(text) = line else { break };
                    if text.trim().is_empty() {
                        continue;
                    }
                    if inbox.send(Msg::Line { chan, text }).is_err() {
                        return;
                    }
                }
                let _ = inbox.send(Msg::Close { chan });
            });
        }
    })?;
    Ok(())
}
const COALESCE: Duration = Duration::from_millis(40);
const SAVE_AFTER: Duration = Duration::from_millis(50);
#[derive(Default)]
struct Routed {
    next: u64,
    waiting: HashMap<u64, (u64, Value, u64)>,
}
enum Line {
    Answer {
        id: u64,
        message: serde_json::Map<String, Value>,
    },
    /// A call to a function a channel set with a `callback`: notified, and answered `null` here.
    Callback {
        to: u64,
        reference: String,
        args: Value,
        id: Option<Value>,
    },
    Local,
    Route {
        to: u64,
        message: serde_json::Map<String, Value>,
    },
    Wait,
}
struct Loop<'a> {
    registry: &'a Registry,
    kernel: &'a mut Kernel,
    config: &'a Config,
    outs: HashMap<u64, Sender<String>>,
    routed: Routed,
    sidecars: Vec<crate::host::Sidecar>,
    starting: bool,
    since: Instant,
    waiting: Vec<(u64, String)>,
    exited: crate::host::Exited,
    host_due: Option<Instant>,
    host_backoff: Duration,
    host_started: Instant,
}
const AWAIT_HOST: Duration = Duration::from_secs(10);
const HOST_STABLE: Duration = Duration::from_secs(30);
const HOST_BACKOFF_MAX: Duration = Duration::from_secs(30);
impl Loop<'_> {
    fn is_sidecar(&self, chan: u64) -> bool {
        let Some(info) = self.kernel.model.chans.get(&chan).map(|c| &c.info.attributes) else { return false };
        let ours = |key: &str| {
            info.get(key).and_then(Value::as_u64).is_some_and(|p| self.sidecars.iter().any(|s| u64::from(s.pid) == p))
        };
        ours("pid") || ours("ppid")
    }
    fn host_is_up(&self) -> bool {
        self.kernel.fallback.is_some_and(|chan| self.is_sidecar(chan))
    }
    fn others(&self) -> usize {
        self.outs.keys().filter(|chan| !self.is_sidecar(**chan)).count()
    }
    fn run(&mut self, messages: &Receiver<Msg>) -> bool {
        let server = Caller { channel: SERVER, ..Caller::default() };
        let mut changed: HashSet<u64> = HashSet::new();
        let mut last_refresh = Instant::now();
        let mut last_save = Instant::now();
        let mut idle_since: Option<Instant> = None;
        let mut connected = false;
        self.turn(None);
        loop {
            let wait = if changed.is_empty() {
                Duration::from_millis(250)
            } else {
                COALESCE.saturating_sub(last_refresh.elapsed())
            };
            let wait = if self.kernel.dirty { wait.min(SAVE_AFTER) } else { wait };
            // A turn's deadline is short: the loop wakes for it.
            let wait = match self.kernel.turns.next_deadline() {
                Some(at) => {
                    wait.min(Duration::from_millis(at.saturating_sub(crate::kernel::requests::now_ms()).max(1)))
                }
                None => wait,
            };
            let mut batch = Vec::new();
            match messages.recv_timeout(wait) {
                Ok(msg) => {
                    batch.push(msg);
                    while let Ok(msg) = messages.try_recv() {
                        batch.push(msg);
                        if batch.len() >= 256 {
                            break;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            for msg in batch {
                match msg {
                    Msg::Open { chan, out } => {
                        self.kernel.model.chans.insert(chan, Chan { id: chan, ..Chan::default() });
                        self.outs.insert(chan, out);
                    }
                    Msg::Line { chan, text } => {
                        self.line(chan, &text);
                        connected |= self.others() > 0;
                    }
                    Msg::Close { chan } => {
                        self.closed(chan);
                        self.turn(None);
                    }
                    Msg::Note(Note::Changed(tab)) => {
                        changed.insert(tab);
                    }
                    Msg::Note(Note::Exited(tab)) => {
                        crate::tab::terminal::changed(self.kernel, tab);
                        if crate::tab::terminal::exited(self.kernel, tab) {
                            let closed = self
                                .registry
                                .with_cx(self.kernel, &server, |cx| crate::kernel::ops::close_tab(cx, tab));
                            if let Err(err) = closed {
                                log(&format!("closing %{tab} after it exited: {}", err.message));
                            }
                        }
                        self.turn(None);
                    }
                    Msg::Note(Note::Osc { tab, code, data }) => {
                        crate::tab::terminal::osc(self.kernel, tab, code, &data);
                        self.turn(None);
                    }
                    Msg::Note(Note::Bell(tab)) => {
                        crate::tab::terminal::bell(self.kernel, tab);
                        self.turn(None);
                    }
                    Msg::Note(Note::Stream { chan, line }) => self.send(chan, line),
                    Msg::Note(Note::Resized { tab, cols, rows }) => {
                        self.kernel.sized(tab, cols as u32, rows as u32);
                        self.turn(None);
                    }
                    Msg::Note(Note::Wake) => {}
                    Msg::SidecarExited(pid) => self.sidecar_exited(pid),
                }
            }
            self.restart_host();
            self.host_up();
            if !self.kernel.requests.is_empty() {
                crate::kernel::requests::tick(self.kernel, crate::kernel::requests::now_ms());
                self.turn(None);
            }
            if !self.kernel.waits.is_empty() {
                crate::kernel::waits::tick(self.kernel, crate::kernel::requests::now_ms());
                self.turn(None);
            }
            if !self.kernel.turns.is_empty() {
                crate::kernel::turns::tick(self.kernel, crate::kernel::requests::now_ms());
                self.turn(None);
            }
            if !changed.is_empty() && last_refresh.elapsed() >= COALESCE {
                last_refresh = Instant::now();
                for tab in changed.drain() {
                    crate::tab::terminal::changed(self.kernel, tab);
                }
                self.turn(None);
            }
            let blobs_due = last_save.elapsed() >= crate::kernel::persist::BLOBS_EVERY;
            if (self.kernel.dirty && last_save.elapsed() >= SAVE_AFTER) || blobs_due {
                last_save = Instant::now();
                self.save(false);
            }
            if self.kernel.restart {
                self.turn(None);
                if self.kernel.config.data_dir.is_none() {
                    let transient = self.config.paths.data.join("handoff-state");
                    let _ = std::fs::create_dir_all(&transient);
                    self.kernel.set_data_dir(Some(transient));
                }
                // Without a handoff the terminals start again from what is saved: all of it.
                self.save(!pumi_sys::caps().handoff);
                return true;
            }
            if self.kernel.stop {
                break;
            }
            if self.config.headless && connected && self.others() == 0 {
                log("headless: the last connection closed");
                break;
            }
            if let Some(after) = self.config.idle_exit {
                let idle = self.kernel.model.sessions.is_empty()
                    && self.others() == 0
                    && self.kernel.global_option("exit_empty") != json!(false);
                match (idle, idle_since) {
                    (true, None) => idle_since = Some(Instant::now()),
                    (true, Some(since)) if since.elapsed() >= after => {
                        log(&format!("no sessions and no one connected for {after:?}: exiting"));
                        break;
                    }
                    (false, _) => idle_since = None,
                    _ => {}
                }
            }
        }
        self.turn(None);
        self.save(true);
        let tabs: Vec<u64> = self.kernel.model.tabs.keys().copied().collect();
        for tab in tabs {
            if let Some(terminal) = self.kernel.instance::<crate::tab::terminal::Terminal>(tab) {
                terminal.term.drop_clients();
                terminal.term.hang_up();
            }
        }
        false
    }
    fn sidecar_exited(&mut self, pid: u32) {
        let before = self.sidecars.len();
        self.sidecars.retain(|s| s.pid != pid);
        if self.sidecars.len() == before || self.kernel.stop || self.kernel.restart || self.config.host.is_none() {
            return;
        }
        self.host_backoff = match self.host_started.elapsed() < HOST_STABLE {
            true => (self.host_backoff * 2).clamp(Duration::from_secs(1), HOST_BACKOFF_MAX),
            false => Duration::ZERO,
        };
        log(&format!("starting pumi-host again in {:?}", self.host_backoff));
        self.host_due = Some(Instant::now() + self.host_backoff);
        self.starting = true;
        self.since = Instant::now();
    }
    fn restart_host(&mut self) {
        if self.host_due.is_none_or(|due| Instant::now() < due) {
            return;
        }
        self.host_due = None;
        let Some(sidecar) = crate::host::start(&start_of(self.config, &self.exited)) else { return };
        if let Some(running) = self.kernel.ext.get_mut::<crate::lifecycle::Running>() {
            running.host = sidecar.image.clone();
        }
        self.sidecars.push(sidecar);
        self.host_started = Instant::now();
        self.starting = true;
        self.since = Instant::now();
    }
    fn save(&mut self, full: bool) {
        let server = Caller { channel: SERVER, ..Caller::default() };
        if let Err(err) = self.registry.with_cx(self.kernel, &server, |cx| crate::kernel::persist::save(cx, full)) {
            log(&format!("saving: {}", err.message));
        }
    }
    fn closed(&mut self, chan: u64) {
        crate::api::ui::detach(self.kernel, chan);
        crate::kernel::requests::chan_closed(self.kernel, chan);
        crate::attach::chan_closed(self.kernel, chan);
        crate::kernel::turns::chan_closed(self.kernel, chan);
        crate::kernel::queue::chan_closed(self.kernel, chan);
        crate::kernel::waits::chan_closed(self.kernel, chan);
        self.kernel.events.chan_closed(chan);
        self.kernel.withdraw_routes(chan, None);
        if self.kernel.fallback == Some(chan) {
            self.kernel.fallback = None;
        }
        let gone: Vec<u64> =
            self.routed.waiting.iter().filter(|(_, (_, _, to))| *to == chan).map(|(id, _)| *id).collect();
        for id in gone {
            if let Some((from, their_id, _)) = self.routed.waiting.remove(&id) {
                let err = pumi_api::Error::new(pumi_api::ErrorKind::ProviderUnavailable, "its provider went away");
                self.send(from, pumi_api::rpc::reply(&their_id, Err(err)));
            }
        }
        self.routed.waiting.retain(|_, (from, _, _)| *from != chan);
        self.kernel.model.chans.remove(&chan);
        self.outs.remove(&chan);
    }
    fn send(&self, chan: u64, message: String) {
        if let Some(out) = self.outs.get(&chan) {
            let _ = out.send(message);
        }
    }
    fn classify(&self, chan: u64, text: &str) -> Line {
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(text) else { return Line::Local };
        if !message.contains_key("method") {
            return match message.get("id").and_then(Value::as_u64) {
                Some(id) if self.routed.waiting.contains_key(&id) => Line::Answer { id, message },
                _ => Line::Local,
            };
        }
        let method = message["method"].as_str().unwrap_or_default();
        if self.starting && self.kernel.fallback.is_none() && !self.is_sidecar(chan) && method != "channel.setInfo" {
            return Line::Wait;
        }
        let target = match method {
            "call" => message
                .get("params")
                .and_then(|p| p.get(0).or_else(|| p.get("name")))
                .and_then(Value::as_str)
                .unwrap_or(method),
            _ => method,
        };
        if pumi_api::Registry::core(target).is_some() && target != "call.atomic" {
            return Line::Local;
        }
        // The client a call is made for: the one a plugin serving that client's call names, else
        // the caller's own channel. A function each app answers for its own client goes there.
        let given = message.get("rpc.caller").and_then(|c| c["client"].as_u64());
        let client = given.filter(|c| self.kernel.model.chans.get(c).is_some_and(|c| c.ui.is_some())).unwrap_or(chan);
        if let Some(route) = self.kernel.route(target, client)
            && let Some(reference) = &route.callback
        {
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            let given = match method {
                "call" => params.get(1).or_else(|| params.get("args")).cloned().unwrap_or(Value::Null),
                _ => params,
            };
            let args = match given {
                Value::Null => json!([]),
                Value::Array(list) => Value::Array(list),
                Value::Object(mut map) => Value::Array(
                    route.info.param_names().iter().map(|n| map.remove(n).unwrap_or(Value::Null)).collect(),
                ),
                other => json!([other]),
            };
            return Line::Callback {
                to: route.chan,
                reference: reference.clone(),
                args,
                id: message.get("id").cloned(),
            };
        }
        let to = match self.kernel.route(target, client) {
            Some(route) => Some(route.chan),
            None if target == "call.atomic" && !self.atomic_here(&message) => self.kernel.fallback,
            None if target == "call.atomic" => None,
            None if self.registry.function_info(target).is_none() => self.kernel.fallback,
            None => None,
        };
        match to {
            Some(to) if to != chan => Line::Route { to, message },
            _ => Line::Local,
        }
    }
    fn atomic_here(&self, message: &serde_json::Map<String, Value>) -> bool {
        let calls = message.get("params").and_then(|p| p.get(0).or_else(|| p.get("calls")));
        calls.and_then(Value::as_array).is_some_and(|calls| {
            calls
                .iter()
                .all(|c| c.get(0).and_then(Value::as_str).is_some_and(|n| pumi_api::Registry::core(n).is_some()))
        })
    }
    fn line(&mut self, chan: u64, text: &str) {
        match self.classify(chan, text) {
            Line::Wait => self.waiting.push((chan, text.to_string())),
            Line::Answer { id, mut message } => {
                let Some((from, their_id, _)) = self.routed.waiting.remove(&id) else { return };
                message.insert("id".into(), their_id);
                self.turn(Some((from, Value::Object(message).to_string())));
            }
            Line::Callback { to, reference, args, id } => {
                self.send(to, pumi_api::rpc::notification("rpc.callback", json!([reference, args])));
                self.turn(id.map(|id| (chan, pumi_api::rpc::reply(&id, Ok(Value::Null)))));
            }
            Line::Route { to, mut message } => {
                let caller = self.caller(chan, message.get("rpc.caller"));
                if let Some(their_id) = message.remove("id") {
                    self.routed.next += 1;
                    let id = self.routed.next;
                    self.routed.waiting.insert(id, (chan, their_id, to));
                    message.insert("id".into(), json!(id));
                }
                message.insert("rpc.caller".into(), caller);
                self.turn(None);
                self.send(to, Value::Object(message).to_string());
            }
            Line::Local => {
                let parsed = serde_json::from_str::<Value>(text).ok();
                let caller = match parsed.as_ref().and_then(|m| m.get("rpc.caller")) {
                    Some(c) => Caller {
                        channel: c["chan"].as_u64().filter(|c| self.kernel.model.chans.contains_key(c)).unwrap_or(chan),
                        tab: c["tab"].as_u64(),
                        source: c["name"].as_str().map(str::to_string),
                        ..Caller::default()
                    },
                    None => Caller { channel: chan, ..Caller::default() },
                };
                let reply = match &parsed {
                    Some(Value::Object(m)) if m.get("method").and_then(Value::as_str) == Some("api.getInfo") => {
                        let id = m.get("id").cloned();
                        id.map(|id| pumi_api::rpc::reply(&id, Ok(self.api_info())))
                    }
                    _ => pumi_api::rpc::handle(self.registry, self.kernel, &caller, text),
                };
                if let Some(turn) = self.kernel.turns.held.take() {
                    match parsed.as_ref().and_then(|m| waits(m, "agent.turn")) {
                        Some(id) if self.kernel.turns.pending.contains_key(&turn) => {
                            if let Some(t) = self.kernel.turns.pending.get_mut(&turn) {
                                t.waiter = Some((chan, id));
                            }
                            self.turn(None);
                            self.host_up();
                            return;
                        }
                        // Inside `call.atomic`: it answered at once, with no one heard.
                        _ => crate::kernel::turns::forget(self.kernel, turn),
                    }
                }
                if let Some(taker) = self.kernel.queue.held.take() {
                    match parsed.as_ref().and_then(|m| waits(m, "agent.take")) {
                        Some(id) if self.kernel.queue.takers.contains_key(&taker) => {
                            if let Some(t) = self.kernel.queue.takers.get_mut(&taker) {
                                t.waiter = Some((chan, id));
                            }
                            self.turn(None);
                            self.host_up();
                            return;
                        }
                        _ => self.kernel.queue.forget_taker(taker),
                    }
                }
                if let Some(wait) = self.kernel.waits.held.take() {
                    match parsed.as_ref().and_then(|m| waits(m, "event.wait")) {
                        Some(id) if self.kernel.waits.contains(wait) => {
                            self.kernel.waits.waiters.insert(wait, (chan, id));
                            self.turn(None);
                            self.host_up();
                            return;
                        }
                        // Called where it can't be held (inside `call.atomic`): it answered `null`.
                        _ => self.kernel.waits.forget(wait),
                    }
                }
                self.turn(reply.map(|r| (chan, r)));
                self.host_up();
            }
        }
    }
    fn host_up(&mut self) {
        if !self.starting {
            return;
        }
        let up = self.host_is_up();
        if !up && self.since.elapsed() < AWAIT_HOST {
            return;
        }
        if !up {
            log(&format!("the host did not come up in {AWAIT_HOST:?}; calls waiting for it go on"));
        }
        self.starting = false;
        for (chan, text) in std::mem::take(&mut self.waiting) {
            if self.outs.contains_key(&chan) {
                self.line(chan, &text);
            }
        }
    }
    /// Who a call routed from `chan` comes from, as its provider hears it in `rpc.caller`: the
    /// channel; the name, tab and client the call says it is made as (a plugin's, for the call it
    /// serves), else the channel's own.
    fn caller(&self, chan: u64, given: Option<&Value>) -> Value {
        let c = self.kernel.model.chans.get(&chan);
        let tab = given.and_then(|g| g["tab"].as_u64()).filter(|t| self.kernel.model.tabs.contains_key(t));
        let current = self.kernel.model.current(&Caller { channel: chan, tab, ..Caller::default() });
        let name =
            given.and_then(|g| g["name"].as_str()).map(str::to_string).or_else(|| c.map(|c| c.info.name.clone()));
        let is_client = |chan: &u64| self.kernel.model.chans.get(chan).is_some_and(|c| c.ui.is_some());
        let client = given.and_then(|g| g["client"].as_u64()).filter(is_client).or(Some(chan).filter(is_client));
        json!({
            "chan": chan,
            "name": name,
            "tab": current.map(|c| c.tab),
            "client": client,
        })
    }
    fn api_info(&self) -> Value {
        let mut functions = self.registry.functions();
        for f in &mut functions {
            f.provider = "core".into();
        }
        functions.retain(|f| !self.kernel.routes.contains_key(&f.name));
        functions.extend(self.kernel.routes.values().filter(|r| !r.info.internal).map(|r| r.info.clone()));
        functions.sort_by(|a, b| a.name.cmp(&b.name));
        serde_json::to_value(ApiInfo::new(functions)).unwrap_or(Value::Null)
    }
    fn turn(&mut self, reply: Option<(u64, String)>) {
        if self.kernel.events.has_pending() {
            flush(self.kernel);
        }
        for (chan, message) in self.kernel.out.drain(..) {
            if let Some(out) = self.outs.get(&chan) {
                let _ = out.send(message);
            }
        }
        if let Some((chan, reply)) = reply {
            self.send(chan, reply);
        }
    }
}
/// The id of a message that calls `function` itself (`agent.take`, `event.wait`; not in
/// `call.atomic`), which is answered when what it waits on ends rather than at once.
fn waits(message: &Value, function: &str) -> Option<Value> {
    let method = message.get("method")?.as_str()?;
    let named = match method {
        "call" => message.get("params").and_then(|p| p.get(0).or_else(|| p.get("name"))).and_then(Value::as_str),
        other => Some(other),
    };
    (named == Some(function)).then(|| message.get("id").cloned()).flatten()
}
