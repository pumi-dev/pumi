use crate::rpc::Response;
use crate::{Error, ErrorKind, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use pumi_sys::ipc::Stream;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
const TIMEOUT: Duration = Duration::from_secs(30);
enum Wake {
    Answer(std::result::Result<Value, Error>),
    Request(String),
}
type Reply = Sender<Wake>;
pub struct Core {
    writer: Mutex<Stream>,
    pending: Arc<Mutex<HashMap<u64, Reply>>>,
    nest: Arc<Mutex<Vec<Reply>>>,
    lines: Sender<Option<String>>,
    next: AtomicU64,
    closed: Arc<AtomicBool>,
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
fn is_request(line: &str) -> bool {
    line.contains("\"method\"")
        && serde_json::from_str::<Value>(line).is_ok_and(|v| v.get("method").is_some() && v.get("id").is_some())
}
fn gone(why: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::ProviderUnavailable, format!("the core: {why}"))
}
fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PUMI_RPC_TRACE").is_some())
}
impl Core {
    pub fn connect(socket: &Path, lines: Sender<Option<String>>) -> std::io::Result<Arc<Core>> {
        let stream = Stream::connect(socket)?;
        let reader = stream.try_clone()?;
        let pending: Arc<Mutex<HashMap<u64, Reply>>> = Arc::default();
        let nest: Arc<Mutex<Vec<Reply>>> = Arc::default();
        let closed: Arc<AtomicBool> = Arc::default();
        let core = Arc::new(Core {
            writer: Mutex::new(stream),
            pending: pending.clone(),
            nest: nest.clone(),
            lines: lines.clone(),
            next: AtomicU64::new(1),
            closed: closed.clone(),
        });
        std::thread::Builder::new().name("pumi-channel".into()).spawn(move || {
            let reader = BufReader::with_capacity(1 << 16, reader);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                if trace() {
                    eprintln!("core → channel {line}");
                }
                if let Some((id, result)) = answer(&line)
                    && let Some(reply) = lock(&pending).remove(&id)
                {
                    let _ = reply.send(Wake::Answer(result));
                    continue;
                }
                {
                    let nest = lock(&nest);
                    if let Some(waiting) = nest.last()
                        && is_request(&line)
                    {
                        let _ = waiting.send(Wake::Request(line));
                        continue;
                    }
                }
                if lines.send(Some(line)).is_err() {
                    break;
                }
            }
            closed.store(true, Ordering::SeqCst);
            for (_, reply) in lock(&pending).drain() {
                let _ = reply.send(Wake::Answer(Err(gone("the connection ended"))));
            }
            let _ = lines.send(None);
        })?;
        Ok(core)
    }
    pub fn write(&self, line: &str) {
        if trace() {
            eprintln!("channel → core {line}");
        }
        let mut writer = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        let _ = writer.write_all(line.as_bytes()).and_then(|()| writer.write_all(b"\n"));
    }
    pub fn call(&self, method: &str, params: Value, caller: Option<Value>) -> Result<Value> {
        self.wait(method, params, caller, None)
    }
    pub fn call_serving(
        &self,
        method: &str,
        params: Value,
        caller: Option<Value>,
        serve: &mut dyn FnMut(String),
    ) -> Result<Value> {
        self.wait(method, params, caller, Some(serve))
    }
    fn wait(
        &self,
        method: &str,
        params: Value,
        caller: Option<Value>,
        mut serve: Option<&mut dyn FnMut(String)>,
    ) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        lock(&self.pending).insert(id, tx.clone());
        if self.closed.load(Ordering::SeqCst) {
            lock(&self.pending).remove(&id);
            return Err(gone("the connection ended"));
        }
        if serve.is_some() {
            lock(&self.nest).push(tx);
        }
        let mut message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        if let Some(caller) = caller {
            message["rpc.caller"] = caller;
        }
        self.write(&message.to_string());
        let result = loop {
            match rx.recv_timeout(TIMEOUT) {
                Ok(Wake::Answer(result)) => break result,
                Ok(Wake::Request(line)) => match serve.as_mut() {
                    Some(serve) => serve(line),
                    None => {
                        let _ = self.lines.send(Some(line));
                    }
                },
                Err(_) => {
                    lock(&self.pending).remove(&id);
                    break Err(Error::new(
                        ErrorKind::Timeout,
                        format!("{method} did not answer in {}s", TIMEOUT.as_secs()),
                    ));
                }
            }
        };
        if serve.is_some() {
            let mut nest = lock(&self.nest);
            nest.pop();
            while let Ok(wake) = rx.try_recv() {
                if let Wake::Request(line) = wake {
                    match nest.last() {
                        Some(outer) => {
                            let _ = outer.send(Wake::Request(line));
                        }
                        None => {
                            let _ = self.lines.send(Some(line));
                        }
                    }
                }
            }
        }
        result
    }
    pub fn notify(&self, method: &str, params: Value) {
        self.write(&crate::rpc::notification(method, params));
    }
}
fn answer(line: &str) -> Option<(u64, std::result::Result<Value, Error>)> {
    if line.contains("\"method\"") && serde_json::from_str::<Value>(line).ok()?.get("method").is_some() {
        return None;
    }
    let response: Response = serde_json::from_str(line).ok()?;
    let id = match &response.id {
        Some(crate::rpc::Id::Number(n)) => *n as u64,
        _ => return None,
    };
    Some((id, response.into_result()))
}
