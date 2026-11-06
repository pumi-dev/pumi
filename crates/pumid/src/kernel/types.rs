use super::Kernel;
use super::TabId;
use super::options::{Change, Decl};
use super::state::{Need, Strategy};
use pumi_api::manifest::Audience;
use pumi_api::{Error, Result};
use serde_json::Value;
use std::any::Any;
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default)]
pub struct SaveOpts {
    pub purpose: String,
    pub viewoptions: Vec<String>,
}
impl SaveOpts {
    pub fn wants(&self, item: &str) -> bool {
        self.viewoptions.iter().any(|v| v == item)
    }
}
#[derive(Clone, Debug, Default)]
pub struct Serialized {
    pub data: Value,
    pub blobs: Vec<(String, String, Vec<u8>)>,
    pub needs: Vec<Need>,
}
pub struct Restore<'a> {
    pub data: &'a Value,
    pub blobs: BTreeMap<String, Vec<u8>>,
    pub options: &'a BTreeMap<String, Value>,
}
pub trait TabType: 'static {
    fn name(&self) -> &str;
    fn version(&self) -> u32;
    fn strategy(&self) -> Strategy;
    fn options(&self) -> Vec<Decl> {
        Vec::new()
    }
    fn methods(&self) -> Vec<String> {
        Vec::new()
    }
    /// Who its tabs are for: anyone's (`any`), a person's (`user`), an agent's (`agent`), or its
    /// plugin's alone (`internal`) — what `tab.open` reads before it starts one.
    fn audience(&self) -> Audience {
        Audience::Any
    }
    /// The plugin that declares it (`owner/name`): the one that opens an `internal` block.
    fn provider(&self) -> Option<&str> {
        None
    }
    fn open(
        &self,
        kernel: &mut Kernel,
        tab: TabId,
        args: &Value,
        restore: Option<Restore>,
    ) -> Result<Box<dyn Instance>>;
}
pub trait Instance: 'static {
    fn read(&mut self, kernel: &mut Kernel, tab: TabId, opts: &Value) -> Result<Value>;
    fn input(&mut self, kernel: &mut Kernel, tab: TabId, data: &Value) -> Result<()>;
    fn selection(&mut self, kernel: &mut Kernel, tab: TabId) -> Result<Value>;
    fn serialize(&mut self, kernel: &mut Kernel, tab: TabId, opts: &SaveOpts) -> Result<Serialized>;
    fn deserialize(&mut self, kernel: &mut Kernel, tab: TabId, restore: Restore) -> Result<()>;
    fn call(&mut self, _kernel: &mut Kernel, _tab: TabId, method: &str, _args: Value) -> Result<Value> {
        Err(Error::not_found(format!("no method `{method}`")))
    }
    fn resize(&mut self, _kernel: &mut Kernel, _tab: TabId, _cols: u32, _rows: u32) {}
    fn option_set(&mut self, _kernel: &mut Kernel, _tab: TabId, _change: &Change) {}
    fn close(&mut self, _kernel: &mut Kernel, _tab: TabId) {}
    fn as_any(&self) -> &dyn Any;
}
