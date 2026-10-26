use crate::kernel::events::Event;
use crate::kernel::{Config as KernelConfig, Kernel, SERVER};
use pumi_api::{Caller, Registry};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};
#[derive(Debug, Clone)]
pub struct Config {
    pub socket: PathBuf,
    pub data: PathBuf,
    pub startup: crate::defaults::Startup,
}
pub use crate::kernel::events::SUBSCRIPTION;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Notice { msg: String },
}
#[derive(Clone)]
pub struct Notes(pub Sender<Note>);
enum Msg {
    Line(Option<String>),
    Note(Note),
}
pub fn log(message: &str) {
    crate::kernel::log(message);
}
fn base_env(data: &std::path::Path) -> BTreeMap<String, String> {
    std::fs::read(data.join("login-env.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| std::env::vars().collect())
}
fn connect(config: &Config, lines: &Sender<Msg>) -> io::Result<Arc<pumi_api::channel::Core>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let (tx, rx) = channel::<Option<String>>();
        match pumi_api::channel::Core::connect(&config.socket, tx) {
            Ok(core) => {
                let lines = lines.clone();
                let live = Arc::downgrade(&core);
                std::thread::Builder::new().name("pumi-host-lines".into()).spawn(move || {
                    for line in rx {
                        let end = line.is_none();
                        if end && live.upgrade().is_none() {
                            return;
                        }
                        if lines.send(Msg::Line(line)).is_err() || end {
                            return;
                        }
                    }
                })?;
                return Ok(core);
            }
            Err(err) if Instant::now() >= deadline || err.kind() == io::ErrorKind::NotFound => return Err(err),
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}
fn introduce(registry: &Registry, kernel: &mut Kernel) -> pumi_api::Result<()> {
    connect_as_host(kernel)?;
    register(registry, kernel)
}
fn connect_as_host(kernel: &mut Kernel) -> pumi_api::Result<()> {
    let core = kernel.core()?;
    let version = json!({ "string": pumi_proto::PUMI_VERSION });
    core.call("channel.setInfo", json!(["pumi-host", version, "host", {}, { "pid": std::process::id() }]), None)?;
    core.call("event.on", json!(["*", { "desc": SUBSCRIPTION }]), None)?;
    declare(kernel)?;
    kernel.stale = true;
    kernel.sync();
    crate::geometry::size_headless(kernel);
    Ok(())
}
fn register(registry: &Registry, kernel: &mut Kernel) -> pumi_api::Result<()> {
    let core = kernel.core()?;
    let api = core.call("api.getInfo", json!([]), None)?;
    let theirs: BTreeMap<String, Value> = api["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f["name"].as_str()?.to_string(), f.clone())))
        .collect();
    let core_names: std::collections::HashSet<&str> =
        theirs.iter().filter(|(_, f)| f["provider"] == "core").map(|(n, _)| n.as_str()).collect();
    kernel.reads = theirs
        .iter()
        .filter(|(_, f)| f["effect"] == "read")
        .map(|(n, _)| n.clone())
        .chain(
            [
                "api.getInfo",
                "call",
                "core.blocks",
                "core.functions",
                "core.getOptionValue",
                "core.options",
                "core.traits",
            ]
            .map(str::to_string),
        )
        .collect();
    let mine: Vec<Value> = registry
        .functions()
        .into_iter()
        .filter(|f| !core_names.contains(f.name.as_str()))
        .map(|mut f| {
            f.provider = "host".into();
            serde_json::to_value(f).expect("plain data")
        })
        .collect();
    let calls: Vec<Value> = mine
        .iter()
        .map(|f| {
            json!({
                "name": "fn.set",
                "args": [f["name"], {
                    "desc": f["desc"],
                    "effect": if f["effect"] == "read" { "read" } else { "write" },
                    "paramsSchema": f["params_schema"],
                    "returnsSchema": f["returns_schema"],
                }],
            })
        })
        .collect();
    core.call("call", json!([calls]), None)?;
    Ok(())
}
fn be_fallback(kernel: &mut Kernel) -> pumi_api::Result<()> {
    kernel.core()?.call("core.setFallback", json!([]), None)?;
    Ok(())
}
fn serve(registry: &Registry, kernel: &mut Kernel, message: serde_json::Map<String, Value>) -> Option<String> {
    let caller = caller_of(&message);
    kernel.sync();
    kernel.rpc = Some(caller.channel);
    crate::api::tab_call::offer(kernel, caller.channel, message.get("id").cloned());
    let reply = pumi_api::rpc::handle_value(registry, kernel, &caller, Value::Object(message)).map(|r| r.to_string());
    // A function that answers later (`tab.call(…, "wait")`) writes its own reply.
    let held = crate::api::tab_call::taken(kernel);
    kernel.rpc = None;
    if held { None } else { reply }
}
fn declare(kernel: &mut Kernel) -> pumi_api::Result<()> {
    let core = kernel.core()?;
    let known: BTreeMap<String, Value> = serde_json::from_value(core.call("core.options", json!([]), None)?)
        .map(|decls: Vec<Value>| decls.into_iter().filter_map(|d| Some((d["name"].as_str()?.to_string(), d))).collect())
        .unwrap_or_default();
    let rtp = crate::plugins::runtimepath(kernel.ext.get::<crate::defaults::Startup>().is_some_and(|s| s.clean));
    let calls: Vec<Value> = crate::kernel::options::builtin()
        .into_iter()
        .filter(|d| known.get(&d.name).is_none_or(|k| k["source"] != "core"))
        .map(|mut d| {
            if d.name == "runtimepath" {
                d.default = json!(rtp);
            }
            let mut spec = serde_json::to_value(&d).expect("plain data");
            let map = spec.as_object_mut().expect("an object");
            for key in ["name", "source", "pending", "order"] {
                map.remove(key);
            }
            json!({ "name": "option.declare", "args": [d.name, spec] })
        })
        .collect();
    if let Err(err) = core.call("call", json!([calls]), None) {
        log(&format!("declaring the host's options: {}", err.message));
    }
    Ok(())
}
pub fn run(config: Config) -> io::Result<()> {
    pumi_sys::process::parent_gone();
    let (inbox, messages) = channel::<Msg>();
    let mut kernel = Kernel::new(KernelConfig {
        name: pumi_sys::hostname(),
        socket: config.socket.clone(),
        base_env: base_env(&config.data),
        bin_dir: std::env::var_os("PUMI_BIN_DIR").map(PathBuf::from),
        data: config.data.clone(),
    });
    let (notes, note_rx) = channel::<Note>();
    kernel.ext.insert(Notes(notes.clone()));
    kernel.ext.insert(config.startup.clone());
    {
        let inbox = inbox.clone();
        std::thread::Builder::new().name("pumi-host-notes".into()).spawn(move || {
            for note in note_rx {
                if inbox.send(Msg::Note(note)).is_err() {
                    return;
                }
            }
        })?;
    }
    crate::kernel::set_name("pumi-host");
    let registry: &'static Registry = Box::leak(Box::new(Registry::new()));
    registry.set_forward(std::rc::Rc::new(|cx: &mut pumi_api::Cx, name: &str, args: Value| {
        let caller = cx.caller().clone();
        cx.host::<Kernel>()?.call_core(&caller, name, args)
    }));
    kernel.serve = Some(std::rc::Rc::new(move |kernel: &mut Kernel, line: String| {
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(&line) else { return };
        kernel.stale = true;
        let outer = kernel.rpc.take();
        let reply = serve(registry, kernel, message);
        kernel.rpc = outer;
        if let (Some(reply), Some(core)) = (reply, kernel.core.clone()) {
            core.write(&reply);
        }
    }));
    kernel.core = Some(connect(&config, &inbox)?);
    connect_as_host(&mut kernel).map_err(|e| io::Error::other(e.message))?;
    register(registry, &mut kernel).map_err(|e| io::Error::other(e.message))?;
    let mut lp = Loop { registry, kernel: &mut kernel, config: &config, inbox };
    crate::defaults::start(registry, lp.kernel, &config.startup);
    be_fallback(lp.kernel).map_err(|e| io::Error::other(e.message))?;
    log(&format!("pid {} serving the core on {}", std::process::id(), config.socket.display()));
    lp.run(&messages);
    log("stopped");
    Ok(())
}
struct Loop<'a> {
    registry: &'a Registry,
    kernel: &'a mut Kernel,
    config: &'a Config,
    inbox: Sender<Msg>,
}
impl Loop<'_> {
    fn run(&mut self, messages: &Receiver<Msg>) {
        let server = Caller { channel: SERVER, ..Caller::default() };
        self.turn(None);
        let mut last_poll = Instant::now();
        loop {
            let wait = Duration::from_millis(250);
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
                    Msg::Line(Some(text)) => self.line(&text),
                    Msg::Line(None) => {
                        if !self.reconnect(messages) {
                            log("the core is gone");
                            return;
                        }
                    }
                    Msg::Note(Note::Notice { msg }) => {
                        crate::api::requests::notify_all(self.kernel, "info", msg);
                        self.turn(None);
                    }
                }
            }
            if !self.config.startup.clean && !self.config.startup.none && last_poll.elapsed() >= Duration::from_secs(1)
            {
                last_poll = Instant::now();
                self.registry.with_cx(self.kernel, &server, crate::overlay::poll);
            }
            crate::api::tab_call::tick(self.kernel);
            self.turn(None);
            if self.kernel.stop {
                break;
            }
        }
    }
    fn reconnect(&mut self, _messages: &Receiver<Msg>) -> bool {
        if pumi_sys::process::parent_gone() {
            return false;
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.kernel.core = None;
            let Ok(core) = connect(self.config, &self.inbox) else { return false };
            self.kernel.core = Some(core);
            let introduced = introduce(self.registry, self.kernel).and_then(|()| be_fallback(self.kernel));
            match introduced {
                Ok(()) => {
                    log("reconnected to the core");
                    if !self.config.startup.clean && !self.config.startup.none {
                        let server = Caller { channel: SERVER, ..Caller::default() };
                        self.registry.with_cx(self.kernel, &server, crate::contributions::follow);
                    }
                    return true;
                }
                Err(err) if Instant::now() >= deadline => {
                    log(&format!("reconnecting: {}", err.message));
                    return false;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    }
    fn line(&mut self, text: &str) {
        let Ok(Value::Object(message)) = serde_json::from_str::<Value>(text) else { return };
        match message.get("method").and_then(Value::as_str) {
            Some("rpc.event") => {
                let Some(event) = message.get("params").and_then(|p| p.get(1)).cloned() else { return };
                let Ok(event) = serde_json::from_value::<Event>(event) else { return };
                self.kernel.mirror_event(&event);
                self.kernel.sync();
                crate::api::requests::heard(self.kernel, &event);
                crate::api::tab_call::heard(self.kernel, &event);
                crate::program::heard(self.kernel, &event);
                crate::geometry::follow(self.kernel, &event);
                self.turn(None);
            }
            Some(_) => {
                let reply = serve(self.registry, self.kernel, message);
                self.turn(reply);
            }
            None => {}
        }
    }
    /// Sends what a call left for channels, then its reply.
    fn turn(&mut self, reply: Option<String>) {
        let core = self.kernel.core.clone();
        for (chan, message) in self.kernel.out.drain(..) {
            let Some(core) = &core else { continue };
            let parsed: Value = serde_json::from_str(&message).unwrap_or(Value::Null);
            match parsed.get("method").and_then(Value::as_str) {
                Some(method) => core.notify("channel.notify", json!([chan, method, parsed["params"]])),
                None => core.write(&message),
            }
        }
        if let Some(reply) = reply
            && let Some(core) = &self.kernel.core
        {
            core.write(&reply);
        }
    }
}
fn caller_of(message: &serde_json::Map<String, Value>) -> Caller {
    let c = message.get("rpc.caller").cloned().unwrap_or(Value::Null);
    Caller {
        channel: c["chan"].as_u64().unwrap_or(SERVER),
        client: c["client"].as_u64().map(|c| c.to_string()),
        tab: c["tab"].as_u64(),
        source: None,
        api_level: None,
    }
}
