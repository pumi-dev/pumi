use crate::frame;
use crate::native::{
    ApiError, Attach, AttachClient, AttachServer, ClientMsg, Hello, Mode, PROTOCOL, ServerMsg, Size, Welcome,
};
use serde_json::Value;
use std::io;
use pumi_sys::ipc::Stream;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
#[derive(Debug)]
pub enum Error {
    Unreachable(io::Error),
    Mismatch { server_version: String, protocol: u32, reason: String },
    Io(io::Error),
    Api(ApiError),
    Decode(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unreachable(err) => write!(f, "pumid is not reachable: {err}"),
            Error::Mismatch { server_version, protocol, reason } => {
                write!(f, "pumid {server_version} speaks protocol {protocol}: {reason}")
            }
            Error::Io(err) => write!(f, "pumid: {err}"),
            Error::Api(err) => write!(f, "{err}"),
            Error::Decode(err) => write!(f, "bad response from pumid: {err}"),
        }
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
pub fn connect(socket: &Path, client: &str, version: &str) -> Result<(Stream, Welcome)> {
    let mut stream = Stream::connect(socket).map_err(Error::Unreachable)?;
    stream.set_write_timeout(Some(Duration::from_secs(10))).map_err(Error::Io)?;
    stream.set_read_timeout(Some(Duration::from_secs(10))).map_err(Error::Io)?;
    let hello = Hello { protocol: PROTOCOL, client: client.into(), client_version: version.into() };
    frame::write(&mut stream, &hello).map_err(Error::Io)?;
    let welcome: Welcome =
        frame::read(&mut stream).map_err(Error::Io)?.ok_or_else(|| Error::Decode("closed during handshake".into()))?;
    if let Welcome::Refused { protocol, server_version, reason } = welcome {
        return Err(Error::Mismatch { server_version, protocol, reason });
    }
    stream.set_read_timeout(None).map_err(Error::Io)?;
    Ok((stream, welcome))
}
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub struct Control {
    socket: std::path::PathBuf,
    client: String,
    version: String,
    stream: Mutex<Option<Stream>>,
}
impl Control {
    pub fn new(socket: impl Into<std::path::PathBuf>, client: &str, version: &str) -> Self {
        Self { socket: socket.into(), client: client.into(), version: version.into(), stream: Mutex::new(None) }
    }
    pub fn socket(&self) -> &Path {
        &self.socket
    }
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        let mut slot = self.stream.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for attempt in 0..2 {
            if slot.is_none() {
                *slot = Some(connect(&self.socket, &self.client, &self.version)?.0);
            }
            let Some(stream) = slot.as_mut() else { continue };
            match round_trip(stream, method, &params) {
                Ok(value) => return value,
                Err(err) if attempt == 0 => {
                    let _ = err;
                    *slot = None;
                }
                Err(err) => {
                    *slot = None;
                    return Err(Error::Io(err));
                }
            }
        }
        Err(Error::Io(io::Error::other("unreachable")))
    }
    pub fn subscribe(&self, subscriptions: Value, mut on_event: impl FnMut(Option<Value>) -> bool) -> Result<()> {
        let (mut stream, _) = connect(&self.socket, &self.client, &self.version)?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        frame::write(&mut stream, &ClientMsg::Subscribe { id, subscriptions: subscriptions.to_string() })
            .map_err(Error::Io)?;
        match frame::read::<_, ServerMsg>(&mut stream).map_err(Error::Io)? {
            Some(ServerMsg::Response { result: Err(err), .. }) => return Err(Error::Api(err)),
            Some(ServerMsg::Response { result: Ok(_), .. }) => {}
            other => return Err(Error::Decode(format!("expected a subscription ack, got {other:?}"))),
        }
        if !on_event(None) {
            return Ok(());
        }
        stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(Error::Io)?;
        loop {
            match frame::read::<_, ServerMsg>(&mut stream) {
                Ok(Some(ServerMsg::Event { json })) => {
                    let event = serde_json::from_str(&json).map_err(|err| Error::Decode(err.to_string()))?;
                    if !on_event(Some(event)) {
                        return Ok(());
                    }
                }
                Ok(Some(_)) => {}
                Ok(None) => return Ok(()),
                Err(err) if matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {
                    if !on_event(None) {
                        return Ok(());
                    }
                }
                Err(err) => return Err(Error::Io(err)),
            }
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum Watched {
    State { revision: u64, state: Value },
    Event(Value),
}
impl Control {
    pub fn watch(&self, mut on: impl FnMut(Option<Watched>) -> bool) -> Result<()> {
        let (mut stream, _) = connect(&self.socket, &self.client, &self.version)?;
        frame::write(&mut stream, &ClientMsg::Watch).map_err(Error::Io)?;
        stream.set_read_timeout(Some(Duration::from_secs(3))).map_err(Error::Io)?;
        let decode = |json: &str| serde_json::from_str::<Value>(json).map_err(|err| Error::Decode(err.to_string()));
        loop {
            let watched = match frame::read::<_, ServerMsg>(&mut stream) {
                Ok(Some(ServerMsg::State { revision, json })) => {
                    Some(Watched::State { revision, state: decode(&json)? })
                }
                Ok(Some(ServerMsg::Event { json })) => Some(Watched::Event(decode(&json)?)),
                Ok(Some(_)) => continue,
                Ok(None) => return Ok(()),
                Err(err) if matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => None,
                Err(err) => return Err(Error::Io(err)),
            };
            if !on(watched) {
                return Ok(());
            }
        }
    }
}
fn round_trip(stream: &mut Stream, method: &str, params: &Value) -> io::Result<Result<Value>> {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    frame::write(stream, &ClientMsg::Request { id, method: method.into(), params: params.to_string() })?;
    loop {
        match frame::read::<_, ServerMsg>(stream)? {
            Some(ServerMsg::Response { id: got, result }) if got == id => {
                return Ok(match result {
                    Ok(json) => serde_json::from_str(&json).map_err(|err| Error::Decode(err.to_string())),
                    Err(err) => Err(Error::Api(err)),
                });
            }
            Some(_) => continue,
            None => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "pumid closed the connection")),
        }
    }
}
pub struct AttachStream {
    reader: io::BufReader<Stream>,
}
pub struct AttachWriter {
    writer: Stream,
}
pub fn attach(
    socket: &Path,
    client: &str,
    version: &str,
    tab: &str,
    mode: Mode,
    size: Size,
    budget: Option<u64>,
    resume: Option<u64>,
) -> Result<(AttachStream, AttachWriter)> {
    let (mut stream, _) = connect(socket, client, version)?;
    let attach = Attach { tab: tab.into(), mode, size, budget, resume };
    frame::write(&mut stream, &ClientMsg::Attach(attach)).map_err(Error::Io)?;
    let writer = stream.try_clone().map_err(Error::Io)?;
    writer.set_write_timeout(Some(Duration::from_secs(5))).map_err(Error::Io)?;
    Ok((AttachStream { reader: io::BufReader::with_capacity(1 << 16, stream) }, AttachWriter { writer }))
}
impl AttachStream {
    pub fn set_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.reader.get_ref().set_read_timeout(timeout)
    }
    pub fn pending(&self) -> bool {
        !self.reader.buffer().is_empty()
    }
    pub fn recv(&mut self) -> Option<AttachServer> {
        frame::read(&mut self.reader).ok().flatten()
    }
}
impl Iterator for AttachStream {
    type Item = AttachServer;
    fn next(&mut self) -> Option<AttachServer> {
        self.recv()
    }
}
impl AttachWriter {
    pub fn send(&mut self, msg: &AttachClient) -> io::Result<()> {
        frame::write(&mut self.writer, msg)
    }
    pub fn close(&self) {
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}
