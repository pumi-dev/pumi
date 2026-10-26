use super::kernel;
use super::keymap::Binding;
use crate::kernel::options::Key;
use crate::kernel::{ChanId, Kernel};
use pumi_api::{Cx, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
/// Keys a client has typed of a sequence not finished yet, by channel.
#[derive(Default)]
struct Pending(BTreeMap<ChanId, Vec<String>>);
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputOpts {
    /// The tab the keys are typed in; the client's current tab by default.
    #[serde(default)]
    pub tab: Option<u64>,
    /// Send a key bound nowhere to the tab here, rather than leaving it to the client (a
    /// client that encodes keys itself, as the app does, leaves this off).
    #[serde(default)]
    pub send: bool,
}
/// What became of the keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Input {
    /// `command` (a mapping ran), `pending` (a longer mapping may follow), `table` (a mode was
    /// entered: the prefix), `tab` (bound nowhere: the tab's), `none` (swallowed by a mode).
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// The keys that are the tab's, for `tab`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
}
fn input(action: &str) -> Input {
    Input { action: action.into(), command: None, keys: Vec::new() }
}
fn pending(k: &mut Kernel) -> &mut BTreeMap<ChanId, Vec<String>> {
    if k.ext.get::<Pending>().is_none() {
        k.ext.insert(Pending::default());
    }
    &mut k.ext.get_mut::<Pending>().expect("just inserted").0
}
fn key_table(k: &Kernel, chan: ChanId) -> String {
    k.model
        .options
        .get("key_table", &[Key::Client(chan), Key::Global])
        .and_then(|(v, _)| v.as_str().map(str::to_string))
        .filter(|t| !t.is_empty() && t != "root")
        .unwrap_or_default()
}
fn set_table(k: &mut Kernel, chan: ChanId, table: &str) {
    let _ = k.set_option("key_table", Key::Client(chan), json!(table), "server", true);
}
/// The tables a key is looked up in for a tab, in order.
fn tables(k: &Kernel, tab: Option<u64>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(ty) = tab.and_then(|t| k.model.tabs.get(&t)).map(|t| t.block.clone()) {
        out.push(ty);
    }
    out.extend(["global".to_string(), "root".to_string()]);
    out
}
fn lookup(k: &Kernel, tables: &[String], seq: &str, tab: Option<u64>) -> (Option<Binding>, bool) {
    let mut longer = false;
    let all = super::keymap::tables(k);
    for table in tables {
        let Some(keys) = all.get(table) else { continue };
        if let Some(b) = keys.get(seq).filter(|b| b.tab.is_none() || b.tab == tab) {
            return (Some(b.clone()), false);
        }
        let prefix = format!("{seq} ");
        longer |= keys.iter().any(|(key, b)| key.starts_with(&prefix) && (b.tab.is_none() || b.tab == tab));
    }
    (None, longer)
}
fn run(cx: &mut Cx, binding: &Binding, tab: Option<u64>) -> Result<Input> {
    let done = Input { action: "command".into(), command: None, keys: Vec::new() };
    if let Some(callback) = &binding.callback {
        callback.call(cx, json!([{ "key": binding.key, "tab": tab }]))?;
        return Ok(done);
    }
    if let Some(call) = &binding.call {
        call.run(cx, tab)?;
        return Ok(done);
    }
    crate::callback::run_command(cx, &binding.command, json!({ "tab": tab }))?;
    Ok(Input { command: Some(binding.command.clone()), ..done })
}
/// Keys a user typed in a client: resolved by the server against the
/// key tables, as the client's mode and its tab decide. Returns what became of them; a key
/// bound nowhere is the tab's (`action = "tab"`), sent there when `opts.send` is set.
#[api(
    name = "input.keys",
    internal,
    example(args = r#"["<C-b>"]"#, result = r#"{"action": "table"}"#, desc = "The prefix enters `prefix`."),
    example(args = r#"["x", {"send": false}]"#, result = r#"{"action": "tab", "keys": ["x"]}"#)
)]
pub fn pumi_input(cx: &mut Cx, keys: String, opts: Option<InputOpts>) -> Result<Input> {
    let opts = opts.unwrap_or_default();
    let mut last = input("none");
    for key in pumi_lang::keys::parse(&keys) {
        last = one(cx, key, &opts)?;
    }
    Ok(last)
}
fn one(cx: &mut Cx, key: String, opts: &InputOpts) -> Result<Input> {
    let (caller, k) = kernel(cx)?;
    let chan = caller.channel;
    let tab = match opts.tab {
        Some(t) => Some(k.model.tab_id(&caller, t)?),
        None => k.model.current(&caller).map(|c| c.tab),
    };
    let mode = key_table(k, chan);
    if !mode.is_empty() {
        let (binding, _) = lookup(k, std::slice::from_ref(&mode), &key, tab);
        let repeat = binding.as_ref().is_some_and(|b| b.repeat);
        if !repeat {
            set_table(k, chan, "root");
        }
        return match binding {
            Some(binding) => run(cx, &binding, tab),
            None => Ok(input("none")),
        };
    }
    let mut seq = pending(k).remove(&chan).unwrap_or_default();
    seq.push(key.clone());
    let joined = seq.join(" ");
    let tables = tables(k, tab);
    match lookup(k, &tables, &joined, tab) {
        (Some(binding), _) => return run(cx, &binding, tab),
        (None, true) => {
            pending(k).insert(chan, seq);
            return Ok(input("pending"));
        }
        (None, false) => {}
    }
    let prefixes = [k.global_option("prefix"), k.global_option("prefix2")];
    if seq.len() == 1 && prefixes.iter().any(|p| p.as_str().map(pumi_lang::keys::normalise).as_deref() == Some(&key)) {
        set_table(k, chan, "prefix");
        return Ok(input("table"));
    }
    if opts.send
        && let Some(tab) = tab
    {
        cx.call("tab.call", json!([tab, "input", { "keys": seq }]))?;
    }
    Ok(Input { action: "tab".into(), command: None, keys: seq })
}
