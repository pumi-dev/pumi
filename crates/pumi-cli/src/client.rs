use pumi_api::rpc::Response;
use pumi_api::{Error, ErrorKind};
use pumi_proto::paths::{File, Paths};
use pumi_sys::ipc::Stream;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
pub struct Client {
    reader: BufReader<Stream>,
    writer: Stream,
    next: i64,
    partial: String,
    pub notifications: VecDeque<Value>,
    pub socket: PathBuf,
}
#[derive(Clone, Debug, Default)]
pub struct Address {
    pub socket: Option<PathBuf>,
    pub name: Option<String>,
}
impl Address {
    pub fn socket(&self) -> PathBuf {
        if let Some(socket) = &self.socket {
            return socket.clone();
        }
        if self.name.is_some() {
            return Paths::at(self.data_dir()).file(File::PumiSocket);
        }
        match pumi_env() {
            Some(path) => path,
            None => Paths::resolve().file(File::PumiSocket),
        }
    }
    pub fn data_dir(&self) -> PathBuf {
        if let Some(socket) = &self.socket {
            return data_dir_of(socket).unwrap_or_default();
        }
        let data = Paths::resolve().data;
        match &self.name {
            Some(name) if name != "default" => data.join("servers").join(name),
            _ => match pumi_env() {
                Some(path) => data_dir_of(&path).unwrap_or(data),
                None => data,
            },
        }
    }
}
/// `$PUMI`, when it is a socket's address (a path, or a Windows pipe name).
fn pumi_env() -> Option<PathBuf> {
    std::env::var("PUMI").ok().filter(|path| path.contains(['/', '\\'])).map(PathBuf::from)
}
/// The data directory of the server at `socket`: the socket's own directory where a socket is a
/// file in it, else the one of pumi's (its own, or a named server's) whose address it is.
fn data_dir_of(socket: &Path) -> Option<PathBuf> {
    let (dir, name) = (socket.parent()?, socket.file_name()?.to_str()?);
    if pumi_sys::ipc::address(dir, name) == socket {
        return Some(dir.to_path_buf());
    }
    let data = Paths::resolve().data;
    let named = std::fs::read_dir(data.join("servers")).into_iter().flatten().flatten().map(|entry| entry.path());
    std::iter::once(data.clone()).chain(named).find(|dir| Paths::at(dir).file(File::PumiSocket) == socket)
}
pub fn unreachable(socket: &Path, err: impl std::fmt::Display) -> Error {
    Error::new(ErrorKind::ProviderUnavailable, format!("no server on {} ({err})", socket.display()))
}
impl Client {
    pub fn connect(socket: &Path) -> Result<Client, Error> {
        let stream = Stream::connect(socket).map_err(|err| unreachable(socket, err))?;
        let writer = stream.try_clone().map_err(|err| unreachable(socket, err))?;
        Ok(Client {
            reader: BufReader::new(stream),
            writer,
            next: 1,
            partial: String::new(),
            notifications: VecDeque::new(),
            socket: socket.into(),
        })
    }
    pub fn open(address: &Address, start: bool) -> Result<Client, Error> {
        let socket = address.socket();
        match Client::connect(&socket) {
            Ok(client) => Ok(client),
            Err(err) if !start => Err(err),
            Err(_) => {
                start_server(address)?;
                Client::connect(&socket)
            }
        }
    }
    pub fn send(&mut self, message: &Value) -> Result<(), Error> {
        let line = message.to_string();
        self.writer
            .write_all(line.as_bytes())
            .and_then(|()| self.writer.write_all(b"\n"))
            .map_err(|err| unreachable(&self.socket, err))
    }
    pub fn read(&mut self) -> Result<Value, Error> {
        let mut line = std::mem::take(&mut self.partial);
        match self.reader.read_line(&mut line) {
            Ok(0) => Err(Error::new(ErrorKind::ProviderUnavailable, "the server went away")),
            Ok(_) => serde_json::from_str(&line).map_err(|err| Error::exception(format!("the server sent: {err}"))),
            Err(err) => Err(unreachable(&self.socket, err)),
        }
    }
    pub fn poll(&mut self, timeout: Duration) -> Result<Option<Value>, Error> {
        if let Some(message) = self.notifications.pop_front() {
            return Ok(Some(message));
        }
        let _ = self.reader.get_ref().set_read_timeout(Some(timeout.max(Duration::from_millis(1))));
        let result = self.reader.read_line(&mut self.partial);
        let _ = self.reader.get_ref().set_read_timeout(None);
        match result {
            Ok(0) => Err(Error::new(ErrorKind::ProviderUnavailable, "the server went away")),
            Ok(_) if self.partial.ends_with('\n') => {
                let line = std::mem::take(&mut self.partial);
                serde_json::from_str(&line).map(Some).map_err(|err| Error::exception(format!("the server sent: {err}")))
            }
            Ok(_) => Ok(None),
            Err(err) if matches!(err.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
            Err(err) => Err(unreachable(&self.socket, err)),
        }
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.next;
        self.next += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        loop {
            let message = self.read()?;
            if message.get("id") == Some(&json!(id)) {
                let response: Response = serde_json::from_value(message)
                    .map_err(|err| Error::exception(format!("a malformed reply: {err}")))?;
                return response.into_result();
            }
            self.notifications.push_back(message);
        }
    }
    pub fn notification(&mut self) -> Result<Value, Error> {
        if let Some(message) = self.notifications.pop_front() {
            return Ok(message);
        }
        self.read()
    }
    pub fn introduce(&mut self, kind: &str) -> Result<(), Error> {
        let tab = std::env::var("PUMI_TAB").ok().filter(|t| t.starts_with('%'));
        let attributes = json!({ "tab": tab, "pid": std::process::id() });
        let version = json!({ "string": pumi_proto::PUMI_VERSION });
        self.call("channel.setInfo", json!(["pumi", version, kind, {}, attributes])).map(|_| ())
    }
}
pub fn pumid() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("PUMID") {
        return Some(PathBuf::from(path));
    }
    let exe = std::env::current_exe().ok().and_then(|e| std::fs::canonicalize(e).ok())?;
    let dir = exe.parent()?;
    let pumid = pumi_sys::paths::exe("pumid");
    // Beside it; in the macOS app (`MacOS/`), from `Resources/bin` or `shims`; in the Linux and
    // Windows app (`bin/`), from `Resources/bin`; one folder up.
    for candidate in [
        dir.join(&pumid),
        dir.join("../../MacOS").join(&pumid),
        dir.join("../MacOS").join(&pumid),
        dir.join("../../bin").join(&pumid),
        dir.join("..").join(&pumid),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join(&pumid)).find(|p| p.is_file()))
}
pub fn start_server(address: &Address) -> Result<(), Error> {
    let program =
        pumid().ok_or_else(|| Error::new(ErrorKind::ProviderUnavailable, "cannot find pumid to start a server"))?;
    if let Some(socket) = &address.socket
        && Paths::at(address.data_dir()).file(File::PumiSocket) != *socket
    {
        return Err(Error::new(
            ErrorKind::ProviderUnavailable,
            format!("no server on {}, and pumid only listens on pumi.sock", socket.display()),
        ));
    }
    pumi_proto::server::start(&program, &pumi_proto::paths::Paths::at(address.data_dir()))
        .map_err(|err| Error::new(ErrorKind::ProviderUnavailable, err.to_string()))
}
pub fn io_error(err: io::Error) -> Error {
    Error::exception(err.to_string())
}
