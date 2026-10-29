use crate::kernel::Kernel;
use crate::kernel::TabId;
use crate::kernel::options::Key;
use crate::kernel::state::Strategy;
use crate::kernel::types::{Instance, Restore, SaveOpts, Serialized, TabType};
use pumi_api::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::any::Any;
/// A plugin's block, as its manifest declares it (`core.setBlocks`): the generic engine, or the
/// core's terminals (`engine: "terminal"`), with the block's own version, strategy and methods.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenericType {
    /// `owner.name.id`.
    #[serde(alias = "name")]
    pub id: String,
    /// The plugin that declares it (`owner/name`).
    #[serde(default)]
    pub provider: String,
    /// The version of its saved states' `data`.
    #[serde(default = "first")]
    pub version: u32,
    #[serde(default = "exact")]
    pub strategy: Strategy,
    /// Its own methods: `tab.call` of one calls its plugin's function `<block>.<method>`.
    #[serde(default)]
    pub methods: Vec<String>,
    /// What runs its tabs, when the core does: `terminal`, a PTY and its program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<Engine>,
}
/// What the core can run a plugin's block on. Its tabs are the engine's: they open with its
/// arguments, answer its methods, and are saved and brought back as it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// A terminal: opened with `{command, cwd, env, keep}`, relaunched when it comes back, its
    /// PTY carried across a restart.
    Terminal,
}
fn first() -> u32 {
    1
}
fn exact() -> Strategy {
    Strategy::Exact
}
/// What every plugin block answers, beside the protocol: its arguments, and what its renderer
/// says it shows (`describe`).
const OWN: [&str; 2] = ["args", "describe"];
impl GenericType {
    /// Why it cannot be declared, if it cannot.
    pub fn refused(&self) -> Option<String> {
        if !is_plugin_block(&self.id) {
            return Some(format!("`{}` is not a plugin block's id (`owner.name.id`)", self.id));
        }
        if self.version < 1 {
            return Some(format!("{}: `version` is 1 or more", self.id));
        }
        let terminal = self.engine == Some(Engine::Terminal);
        let engine = |m: &str| {
            terminal && (super::terminal::METHODS.contains(&m) || super::terminal::HOST_METHODS.contains(&m))
        };
        let taken = |m: &str| pumi_api::manifest::BLOCK_PROTOCOL.contains(&m) || engine(m);
        let refused = self.methods.iter().find(|m| m.is_empty() || taken(m));
        refused.map(|m| format!("{}: `{m}` is not a method of its own", self.id))
    }
}
/// A plugin block's id: three dotted words, `owner.name.id`, each from a lowercase letter or a
/// digit, as a manifest allows them (`1password.vault.itemList`). A bare word is the core's.
pub fn is_plugin_block(id: &str) -> bool {
    let word = |w: &str| {
        w.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && w.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    };
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() == 3 && parts.into_iter().all(word)
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    args: Value,
    #[serde(flatten)]
    shown: Shown,
}
/// What a block's renderer says its tab shows (`report`): its text, selection and data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Shown {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    data: Value,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selection: Option<String>,
}
impl Shown {
    /// What `describe` says: `text`, `selection`, `data`, `title`; each one given replaces its own.
    pub(crate) fn describe(&mut self, kernel: &mut Kernel, tab: TabId, args: &Value) -> Result<()> {
        if let Some(text) = args.get("text").and_then(Value::as_str) {
            let mut end = text.len().min(TEXT_LIMIT);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            self.text = text[..end].to_string();
        }
        if let Some(selection) = args.get("selection") {
            self.selection = selection.as_str().map(str::to_string);
        }
        if let Some(data) = args.get("data") {
            self.data = data.clone();
        }
        if let Some(title) = args.get("title").and_then(Value::as_str) {
            kernel.set_option("title", Key::Tab(tab), json!(title), "describe", true)?;
        }
        kernel.dirty = true;
        Ok(())
    }
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
    pub(crate) fn selection(&self) -> Option<&str> {
        self.selection.as_deref()
    }
}
/// The longest text a renderer's `describe` keeps, and a file tab's `read` gives of its file.
pub(crate) const TEXT_LIMIT: usize = 256 * 1024;
struct Generic(Data);
impl TabType for GenericType {
    fn name(&self) -> &str {
        &self.id
    }
    fn version(&self) -> u32 {
        self.version
    }
    fn strategy(&self) -> Strategy {
        match self.engine {
            Some(Engine::Terminal) => Strategy::Relaunch,
            None => self.strategy,
        }
    }
    fn methods(&self) -> Vec<String> {
        let base: &[&str] = match self.engine {
            Some(Engine::Terminal) => &super::terminal::METHODS,
            None => &OWN,
        };
        base.iter().map(|m| m.to_string()).chain(self.methods.iter().cloned()).collect()
    }
    fn open(
        &self,
        kernel: &mut Kernel,
        tab: TabId,
        args: &Value,
        restore: Option<Restore>,
    ) -> Result<Box<dyn Instance>> {
        if self.engine == Some(Engine::Terminal) {
            let args = self.own_options(kernel, tab, args, restore.is_none())?;
            return super::terminal::open(kernel, tab, &args, restore);
        }
        if let Some(restore) = restore {
            let data = serde_json::from_value(restore.data.clone())
                .map_err(|err| Error::invalid_params(format!("{} state: {err}", self.id)))?;
            return Ok(Box::new(Generic(data)));
        }
        self.own_options(kernel, tab, args, true)?;
        Ok(Box::new(Generic(Data { args: args.clone(), ..Data::default() })))
    }
}
impl GenericType {
    /// Sets the options of its own that `args` names (`set`: when a tab opens), and answers the
    /// rest: what its engine opens with.
    fn own_options(&self, kernel: &mut Kernel, tab: TabId, args: &Value, set: bool) -> Result<Value> {
        let Some(map) = args.as_object() else { return Ok(args.clone()) };
        let mut rest = serde_json::Map::new();
        for (name, value) in map {
            let owned = kernel.model.options.decl(name).is_some_and(|d| d.blocks.iter().any(|t| t == &self.id));
            if !owned {
                rest.insert(name.clone(), value.clone());
            } else if set {
                kernel.set_option(name, Key::Tab(tab), value.clone(), &self.id, true)?;
            }
        }
        Ok(Value::Object(rest))
    }
}
impl Instance for Generic {
    fn read(&mut self, _: &mut Kernel, _: TabId, _: &Value) -> Result<Value> {
        Ok(json!({ "text": self.0.shown.text }))
    }
    /// A plugin block reads its own input: the host sends it to `<block>.input`.
    fn input(&mut self, _: &mut Kernel, _: TabId, _: &Value) -> Result<()> {
        Err(Error::not_found("a plugin block's input is its own function: `<block>.input`"))
    }
    fn selection(&mut self, _: &mut Kernel, _: TabId) -> Result<Value> {
        Ok(json!({ "text": self.0.shown.selection.clone().unwrap_or_default() }))
    }
    fn serialize(&mut self, _: &mut Kernel, _: TabId, _: &SaveOpts) -> Result<Serialized> {
        Ok(Serialized { data: serde_json::to_value(&self.0).expect("plain data"), ..Default::default() })
    }
    fn deserialize(&mut self, _: &mut Kernel, _: TabId, restore: Restore) -> Result<()> {
        self.0 = serde_json::from_value(restore.data.clone()).map_err(|err| Error::invalid_params(err.to_string()))?;
        Ok(())
    }
    fn call(&mut self, kernel: &mut Kernel, tab: TabId, method: &str, args: Value) -> Result<Value> {
        match method {
            "args" => Ok(self.0.args.clone()),
            "describe" => {
                self.0.shown.describe(kernel, tab, &args)?;
                Ok(Value::Null)
            }
            // Its own methods are its plugin's functions, `owner.name.block.method`.
            _ => Err(Error::not_found(format!("`{method}` is the block's own: its plugin's function"))),
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert!(is_plugin_block("pumi_dev.queue.bar") && is_plugin_block("me.my_pkg.board"));
        assert!(is_plugin_block("1password.vault.itemList"), "a digit-first owner, a camelCase id");
        assert!(!is_plugin_block("terminal") && !is_plugin_block("browser") && !is_plugin_block("hello.badge"));
        assert!(!is_plugin_block("Bad.a.b") && !is_plugin_block("a.b.") && !is_plugin_block(""));
        assert!(!is_plugin_block("a.b.c.d") && !is_plugin_block("a..b") && !is_plugin_block("a.b.Bar"));
        assert!(!is_plugin_block("a.b.c d") && !is_plugin_block(".a.b"));
    }
}
