use pumi_api::rpc::Response;
use pumi_api::{Error, ErrorKind};
use pumi_sys::ipc::Stream;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
pub type Notes = Box<dyn FnMut(Option<(&str, Value)>) + Send>;
pub type Requests = Box<dyn FnMut(Request) + Send>;
type Reply = Sender<Result<Value, Error>>;
pub struct Request {
    pub method: String,
    pub params: Value,
    pub caller: Value,
    pub answer: Answer,
}
pub struct Answer {
    id: Value,
    writer: Arc<Mutex<Stream>>,
}
impl Answer {
    pub fn send(self, result: Result<Value, Error>) {
        let mut line = pumi_api::rpc::reply(&self.id, result);
        if trace() {
            eprintln!("pumi-rpc -> {line}");
        }
        line.push('\n');
        let _ = self.writer.lock().unwrap_or_else(|p| p.into_inner()).write_all(line.as_bytes());
    }
}
pub struct Conn {
    writer: Arc<Mutex<Stream>>,
    pending: Arc<Mutex<HashMap<u64, Reply>>>,
    next: AtomicU64,
    alive: Arc<AtomicBool>,
}
fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PUMI_RPC_TRACE").is_some())
}
const TIMEOUT: Duration = Duration::from_secs(30);
fn gone(why: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::ProviderUnavailable, why.to_string())
}
impl Conn {
    pub fn open(socket: &Path, mut notes: Notes, mut requests: Requests) -> Result<Arc<Conn>, Error> {
        let stream =
            Stream::connect(socket).map_err(|err| gone(format!("no server on {} ({err})", socket.display())))?;
        let reader = stream.try_clone().map_err(gone)?;
        let pending: Arc<Mutex<HashMap<u64, Reply>>> = Arc::default();
        let alive = Arc::new(AtomicBool::new(true));
        let writer = Arc::new(Mutex::new(stream));
        let answers = writer.clone();
        let conn = Arc::new(Conn { writer, pending: pending.clone(), next: AtomicU64::new(1), alive: alive.clone() });
        std::thread::Builder::new()
            .name("pumi-rpc".into())
            .spawn(move || {
                let mut lines = BufReader::with_capacity(1 << 16, reader);
                let mut line = String::new();
                loop {
                    line.clear();
                    match lines.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    if trace() {
                        eprintln!("pumi-rpc <- {}", line.trim_end());
                    }
                    let Ok(mut message) = serde_json::from_str::<Value>(&line) else { continue };
                    if let (Some(method), Some(id)) =
                        (message.get("method").and_then(Value::as_str), message.get("id").cloned())
                    {
                        let method = method.to_string();
                        let params = message.get_mut("params").map(Value::take).unwrap_or(Value::Null);
                        let caller = message.get_mut("rpc.caller").map(Value::take).unwrap_or(Value::Null);
                        let answer = Answer { id, writer: answers.clone() };
                        requests(Request { method, params, caller, answer });
                        continue;
                    }
                    if let Some(method) = message.get("method").and_then(Value::as_str) {
                        let params = message.get("params").cloned().unwrap_or(Value::Null);
                        notes(Some((method, params)));
                        continue;
                    }
                    let Some(id) = message.get("id").and_then(Value::as_u64) else { continue };
                    let result = serde_json::from_value::<Response>(message)
                        .map_err(|err| Error::exception(format!("a malformed reply: {err}")))
                        .and_then(Response::into_result);
                    if let Some(reply) = pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id) {
                        let _ = reply.send(result);
                    }
                }
                alive.store(false, Ordering::Release);
                for (_, reply) in pending.lock().unwrap_or_else(|p| p.into_inner()).drain() {
                    let _ = reply.send(Err(gone("the server went away")));
                }
                notes(None);
            })
            .map_err(gone)?;
        Ok(conn)
    }
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    fn write(&self, message: &Value) -> Result<(), Error> {
        let mut line = message.to_string();
        if trace() {
            eprintln!("pumi-rpc -> {line}");
        }
        line.push('\n');
        let mut writer = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        writer.write_all(line.as_bytes()).map_err(|err| {
            self.alive.store(false, Ordering::Release);
            gone(err)
        })
    }
    pub fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        self.call_as(method, params, None)
    }
    pub fn call_as(&self, method: &str, params: Value, caller: Option<&Value>) -> Result<Value, Error> {
        if !self.alive() {
            return Err(gone("the server went away"));
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.pending.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);
        let mut message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Some(caller) = caller {
            message["rpc.caller"] = caller.clone();
        }
        if let Err(err) = self.write(&message) {
            self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            return Err(err);
        }
        rx.recv_timeout(TIMEOUT).unwrap_or_else(|_| {
            self.pending.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            Err(Error::new(ErrorKind::Timeout, format!("{method}: no answer in {}s", TIMEOUT.as_secs())))
        })
    }
}
