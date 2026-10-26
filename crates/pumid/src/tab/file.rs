//! The core's `file` block: a tab that shows a file. The core keeps which file (the tab option
//! `path`) and where in it (`at`: a line, a column); what draws it is whichever plugin's renderer
//! claims the file (`contributes.renderers.file.match`), else the app's placeholder. `file.open`
//! (pumi-host) opens one; its own methods (`save`, its viewer's `action`) go to the function
//! `<viewer>.<method>`, where `viewer` is the tab option `@viewer` the app sets.
use super::generic::{Shown, TEXT_LIMIT};
use crate::kernel::Kernel;
use crate::kernel::TabId;
use crate::kernel::options::{Change, Decl, Key, Scope, Type};
use crate::kernel::state::{Need, Strategy};
use crate::kernel::types::{Instance, Restore, SaveOpts, Serialized, TabType};
use pumi_api::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::any::Any;
use std::path::Path;

/// The block's id.
pub const FILE: &str = "file";
/// The tab option naming the file a file tab shows.
pub const PATH: &str = "path";

pub struct FileType;

/// Where in its file a tab is to be: the file, and a line and column (from 1) when one was asked.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct At {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u64>,
}
impl At {
    /// `{path, line, column}` as a caller gives it; the path must be absolute.
    fn parse(args: &Value, path: Option<&str>) -> Result<At> {
        let path = args.get("path").and_then(Value::as_str).or(path).unwrap_or_default();
        if !Path::new(path).is_absolute() {
            return Err(Error::invalid_params(format!(
                "a file tab's `path` is absolute, not {path:?} (`file.open` resolves one)"
            )));
        }
        let number = |key: &str| args.get(key).and_then(Value::as_u64).filter(|n| *n >= 1);
        Ok(At { path: path.to_string(), line: number("line"), column: number("column") })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Data {
    at: At,
    #[serde(flatten)]
    shown: Shown,
}
struct File(Data);

/// Says what a tab showing `path` is: its directory (`tab_current_path`, so a terminal opened
/// beside it starts there) and, until something says otherwise, its title.
fn note_path(kernel: &mut Kernel, tab: TabId, path: &str) {
    let path = Path::new(path);
    if let Some(dir) = path.parent().and_then(Path::to_str) {
        let _ = kernel.set_option("tab_current_path", Key::Tab(tab), json!(dir), FILE, true);
    }
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        let _ = kernel.set_option("title", Key::Tab(tab), json!(name), FILE, true);
    }
}

impl TabType for FileType {
    fn name(&self) -> &str {
        FILE
    }
    fn version(&self) -> u32 {
        1
    }
    fn strategy(&self) -> Strategy {
        Strategy::Reference
    }
    fn options(&self) -> Vec<Decl> {
        vec![Decl::new(
            PATH,
            Type::String,
            json!(""),
            Scope::Tab,
            "The file a file tab shows, by its full path. Setting it shows another file in the same tab.",
        )]
    }
    fn methods(&self) -> Vec<String> {
        ["at", "reveal", "describe"].map(String::from).to_vec()
    }
    fn open(
        &self,
        kernel: &mut Kernel,
        tab: TabId,
        args: &Value,
        restore: Option<Restore>,
    ) -> Result<Box<dyn Instance>> {
        let data = match restore {
            Some(restore) => serde_json::from_value(restore.data.clone())
                .map_err(|err| Error::invalid_params(format!("file state: {err}")))?,
            None => Data { at: At::parse(args, None)?, ..Data::default() },
        };
        kernel.set_option(PATH, Key::Tab(tab), json!(data.at.path), FILE, true)?;
        note_path(kernel, tab, &data.at.path);
        Ok(Box::new(File(data)))
    }
}

impl Instance for File {
    /// What its renderer reported it shows; with no renderer (the app is closed), the file itself.
    fn read(&mut self, _: &mut Kernel, _: TabId, _: &Value) -> Result<Value> {
        if !self.0.shown.text().is_empty() {
            return Ok(json!({ "text": self.0.shown.text() }));
        }
        let bytes =
            std::fs::read(&self.0.at.path).map_err(|err| Error::not_found(format!("{}: {err}", self.0.at.path)))?;
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(TEXT_LIMIT)]);
        Ok(json!({ "text": text }))
    }
    /// Typing in a file is its viewer's: the host sends it to `<viewer>.input`.
    fn input(&mut self, _: &mut Kernel, _: TabId, _: &Value) -> Result<()> {
        Err(Error::not_found("a file tab's input is its viewer's: `<viewer>.input`"))
    }
    fn selection(&mut self, _: &mut Kernel, _: TabId) -> Result<Value> {
        Ok(json!({ "text": self.0.shown.selection().unwrap_or_default() }))
    }
    fn serialize(&mut self, _: &mut Kernel, _: TabId, _: &SaveOpts) -> Result<Serialized> {
        Ok(Serialized {
            data: serde_json::to_value(&self.0).expect("plain data"),
            needs: vec![Need::Path { path: self.0.at.path.clone() }],
            ..Default::default()
        })
    }
    fn deserialize(&mut self, _: &mut Kernel, _: TabId, restore: Restore) -> Result<()> {
        self.0 = serde_json::from_value(restore.data.clone()).map_err(|err| Error::invalid_params(err.to_string()))?;
        Ok(())
    }
    fn call(&mut self, kernel: &mut Kernel, tab: TabId, method: &str, args: Value) -> Result<Value> {
        match method {
            "at" => Ok(json!(self.0.at)),
            "describe" => {
                self.0.shown.describe(kernel, tab, &args)?;
                Ok(Value::Null)
            }
            // Another file, a line in it, or both.
            "reveal" => {
                let at = At::parse(&args, Some(&self.0.at.path))?;
                if at.path != self.0.at.path {
                    self.0.shown = Shown::default();
                    kernel.set_option(PATH, Key::Tab(tab), json!(at.path), FILE, true)?;
                    note_path(kernel, tab, &at.path);
                }
                self.0.at = at;
                kernel.dirty = true;
                Ok(json!(self.0.at))
            }
            // `save`, a viewer's own (Monaco's `action`): the host sends them to `<viewer>`.
            _ => Err(Error::not_found(format!("`{method}` is the file tab's viewer's: `<viewer>.{method}`"))),
        }
    }
    /// `path` set (`set-option -p path`, a file dropped on it): that file, from its top.
    fn option_set(&mut self, kernel: &mut Kernel, tab: TabId, change: &Change) {
        if change.name != PATH {
            return;
        }
        let Some(path) = change.new.as_str() else { return };
        if path == self.0.at.path || path.is_empty() {
            return;
        }
        self.0 = Data { at: At { path: path.to_string(), line: None, column: None }, shown: Shown::default() };
        kernel.dirty = true;
        note_path(kernel, tab, path);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
