pub mod model;
pub mod pumid;
pub mod tree;
#[cfg(test)]
use crate::runtime::tree::Axis;
use crate::runtime::tree::Node;
use crate::term::Size;
use futures::channel::mpsc::UnboundedSender;
pub use model::Snapshot;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
pub fn connect(data_dir: Option<PathBuf>) -> Arc<dyn Runtime> {
    let local = match data_dir {
        Some(dir) => pumid::Pumid::at(pumi_proto::paths::Paths::at(dir)),
        None => pumid::Pumid::new(),
    };
    Arc::new(local)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Unreachable(String),
    Refused(String),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unreachable(err) | Error::Refused(err) => f.write_str(err),
        }
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
pub struct Read {
    pub snapshot: Snapshot,
    pub trees: BTreeMap<String, (String, Node)>,
    pub menus: Option<Vec<Value>>,
    pub keys: Option<pumi_lang::keys::Replica>,
    /// The `keys` option as a list of bindings (`{table, key, tab}`), read with `keys`.
    pub bindings: Option<Vec<Value>>,
    pub options: Option<serde_json::Map<String, serde_json::Value>>,
}
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Split { target: String, axis: Axis, cwd: Option<String>, env: Option<Value> },
    NewView { workspace: String, cwd: Option<String>, env: Option<Value> },
    NewWorkspace { cwd: Option<String>, env: Option<Value>, server: Option<String> },
    NewWorktree { workspace: String },
    OpenWorktree { workspace: String, path: String },
    MoveTab { tab: String, to: Destination },
    CloseTab { tab: String },
    CloseView { view: String },
    SwapTabs { a: String, b: String },
    ClearTab { tab: String },
    SendText { tab: String, text: String },
    SendKeys { tab: String, keys: Vec<String> },
    Relaunch { tab: String, line: String },
    RenameWorkspace { workspace: String, label: String },
    RenameView { view: String, label: String },
    RenameTab { tab: String, label: Option<String> },
    MoveWorkspace { workspace: String, before: Option<String> },
    SetRatio { view: String, path: Vec<bool>, ratio: f32 },
}
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Beside { view: String, target: String, axis: Axis },
    NewView { workspace: String },
    NewWorkspace,
}
#[cfg(test)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Made {
    pub workspace: Option<String>,
    pub view: Option<String>,
    pub tabs: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachMode {
    Control,
    Observe,
}
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalInput {
    Bytes(Vec<u8>),
    Resize(Size),
}
#[derive(Debug, Clone, PartialEq)]
pub enum TerminalOutput {
    Frame { width: u16, height: u16, bytes: Vec<u8>, more: bool },
    Snapshot { width: u16, height: u16, bytes: Vec<u8>, more: bool },
    Ended { taken_over: bool },
    Lost,
}
pub trait TerminalLink: Send {
    fn send(&mut self, input: TerminalInput);
}
pub struct Attached {
    pub link: Box<dyn TerminalLink>,
    pub output: Box<dyn Iterator<Item = TerminalOutput> + Send>,
    pub seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub tab: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub message: Option<(String, serde_json::Value)>,
}
pub trait Watch: Send {}
pub trait Setup: Send + Sync {
    fn next(&self, tried: &Tried) -> Step;
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tried {
    pub ran: Vec<&'static str>,
    pub waits: u32,
}
impl Tried {
    pub fn has(&self, id: &str) -> bool {
        self.ran.contains(&id)
    }
}
pub type Work = Box<dyn FnOnce(UnboundedSender<String>) + Send>;
pub enum Step {
    Ready,
    Run { id: &'static str, title: String, detail: String, work: Work },
    Wait { title: String, detail: String },
    GaveUp { title: String, reason: String },
}
pub trait Runtime: Send + Sync + 'static {
    fn location(&self) -> String;
    fn read(&self, signatures: &BTreeMap<String, String>) -> Result<Read>;
    /// The integration tests' way to change the server: React changes it through the API.
    #[cfg(test)]
    fn apply(&self, op: Op) -> Result<Made>;
    fn watch(&self, wake: UnboundedSender<()>, notices: UnboundedSender<Notice>) -> Box<dyn Watch>;
    fn attach(
        &self,
        tab: &str,
        mode: AttachMode,
        size: Size,
        resume: Option<u64>,
    ) -> std::result::Result<Attached, String>;
    fn setup(&self) -> Option<&dyn Setup> {
        None
    }
    fn call(&self, _method: &str, _params: Value) -> Result<Value> {
        Err(Error::Unreachable("no server".into()))
    }
    fn on_callback(&self, _callback: pumid::OnCallback) {}
    fn on_event(&self, _event: pumid::OnEvent) {}
    fn call_as(&self, method: &str, params: Value, _caller: Option<&Value>) -> Result<Value> {
        self.call(method, params)
    }
    fn provide(&self, _function: Value) -> Result<()> {
        Err(Error::Unreachable("no server".into()))
    }
    fn on_request(&self, _handler: pumid::OnRequest) {}
    fn status(&self) -> Result<Value> {
        Err(Error::Unreachable("no server".into()))
    }
    fn log(&self) -> Option<PathBuf> {
        None
    }
    fn restart(&self) -> Result<Value> {
        Err(Error::Unreachable("no server".into()))
    }
}
