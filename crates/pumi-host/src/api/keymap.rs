use super::kernel;
use crate::callback::{Call, ChanRef, Rhs};
use crate::kernel::Kernel;
use crate::kernel::options::Key;
use pumi_api::{Cx, Error, Result};
use serde_json::{Map, Value, json};
/// A key binding: what runs when the key is pressed in
/// its table.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct Binding {
    pub table: String,
    pub key: String,
    /// The command line it runs. For a `call`, the same through the `call` command, so a
    /// listing shows one; empty for a `callback`.
    pub command: String,
    /// Or the function it calls (`{call, args}`), the host's to make.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<Call>,
    /// Or the channel's reference it is sent to, with `{key, tab}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback: Option<ChanRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// The key repeats without the prefix for `repeat-time`.
    #[serde(default)]
    pub repeat: bool,
    /// Only in this tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<u64>,
    /// Who mapped it (`pumi`, `settings.json`, `cli`), which Settings shows.
    #[serde(default)]
    pub source: String,
}
impl Binding {
    /// What it runs, as `key.set` takes it.
    pub fn rhs(&self) -> Rhs {
        match (&self.call, &self.callback) {
            (Some(call), _) => Rhs::Call(call.clone()),
            (None, Some(r)) => Rhs::Chan(r.clone()),
            (None, None) => Rhs::Command(self.command.clone()),
        }
    }
}
/// The server's key bindings, as the `keys` option holds them: a table, then a key, then
/// `{call, args}`, called at the tab the key was pressed in.
pub fn tables(k: &Kernel) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, Binding>> {
    let mut out: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Binding>> =
        std::collections::BTreeMap::new();
    for (table, keys) in k.global_option("keys").as_object().into_iter().flatten() {
        for (key, value) in keys.as_object().into_iter().flatten() {
            let Ok(call) = serde_json::from_value::<Call>(value.clone()) else { continue };
            out.entry(table.clone()).or_default().insert(
                key.clone(),
                Binding {
                    table: table.clone(),
                    key: key.clone(),
                    command: call.as_command(),
                    call: Some(call),
                    callback: None,
                    desc: None,
                    repeat: false,
                    tab: None,
                    source: "keys".into(),
                },
            );
        }
    }
    out
}
/// The `keys` option's value, read as a map of tables.
fn store(k: &Kernel) -> Map<String, Value> {
    k.global_option("keys").as_object().cloned().unwrap_or_default()
}
/// Map a key in a table to a function: the `keys` option holds it from now on, and the app and
/// the server's fast path hear `option.changed`.
pub fn set_binding(cx: &mut Cx, table: &str, key: &str, call: Call, source: &str) -> Result<()> {
    call.check()?;
    if table.is_empty() || key.is_empty() {
        return Err(Error::invalid_params("a binding needs a table and a key"));
    }
    let (caller, k) = kernel(cx)?;
    let _ = caller;
    let mut root = store(k);
    let entry = root.entry(table.to_string()).or_insert_with(|| json!({}));
    let Some(keys) = entry.as_object_mut() else {
        return Err(Error::invalid_params(format!("`{table}` is not a key table")));
    };
    keys.insert(key.to_string(), json!({ "call": call.call, "args": call.args }));
    k.set_option("keys", Key::Global, Value::Object(root), source, false).map(|_| ())
}
/// Unmap a key.
pub fn del_binding(cx: &mut Cx, table: &str, key: &str, source: &str) -> Result<()> {
    let (_, k) = kernel(cx)?;
    let mut root = store(k);
    let Some(keys) = root.get_mut(table).and_then(Value::as_object_mut) else {
        return Err(Error::not_found(format!("no key table {table}")));
    };
    if keys.remove(key).is_none() {
        return Err(Error::not_found(format!("{key} is not bound in {table}")));
    }
    k.set_option("keys", Key::Global, Value::Object(root), source, false).map(|_| ())
}
