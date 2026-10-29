use super::kernel;
use crate::kernel::events::{About, Autocmd, check_event};
use pumi_api::{Cx, Error, Result, TabRef, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
/// One name or several.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}
impl OneOrMany {
    pub fn list(self) -> Vec<String> {
        match self {
            OneOrMany::One(one) => vec![one],
            OneOrMany::Many(many) => many,
        }
    }
}
/// The events `event.on` and `event.wait` hear: one name, or a list of at least one.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum EventNames {
    One(String),
    Many(#[schemars(length(min = 1))] Vec<String>),
}
impl EventNames {
    pub fn list(self) -> Vec<String> {
        match self {
            EventNames::One(one) => vec![one],
            EventNames::Many(many) => many,
        }
    }
}
/// Every subscription.
#[api(name = "core.subscriptions", internal, effect = "read", example(args = "[]"))]
pub fn pumi_core_subscriptions(cx: &mut Cx) -> Result<Vec<Value>> {
    let (_, k) = kernel(cx)?;
    Ok(k.events.autocmds.iter().map(Autocmd::info).collect())
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventOnOpts {
    /// Globs matched against the event's `match` (a tab's id for `tab.changed`, a request's
    /// kind for `request.opened`); every match without one.
    #[serde(default, rename = "match")]
    pub matched: Option<OneOrMany>,
    /// End the subscription after its first event.
    #[serde(default)]
    pub once: bool,
    /// What it is for, as a listing shows it.
    #[serde(default)]
    pub desc: Option<String>,
}
/// Subscribe to events, by name or glob (`tab.*`; `*` is every event but those fired before
/// what they name, in `-ing`). Each one that fires goes to the caller's channel
/// as `{"method": "rpc.event", "params": [subscription, event]}`, until `event.off` or the
/// channel closes. Answers the subscription's integer handle.
#[api(
    name = "event.on",
    example(args = r#"["tab.changed", {"match": "%3"}]"#, result = "8", desc = "Hear every change to tab 3."),
    example(args = r#"[["request.opened", "request.closed"], {}]"#, result = "9"),
    example(
        args = r#"["tab.opened", {"match": "terminal", "once": true}]"#,
        result = "10",
        desc = "The next terminal tab."
    )
)]
pub fn pumi_event_on(cx: &mut Cx, event: EventNames, opts: Option<EventOnOpts>) -> Result<u64> {
    let events = event.list();
    if events.is_empty() {
        return Err(Error::invalid_params("a subscription needs an event"));
    }
    for event in &events {
        check_event(event, true)?;
    }
    let opts = opts.unwrap_or_default();
    let (caller, k) = kernel(cx)?;
    let id = k.events.next_id();
    let source = k.source(&caller);
    k.events.autocmds.push(Autocmd {
        id,
        events,
        patterns: opts.matched.map(OneOrMany::list).unwrap_or_default(),
        desc: opts.desc.unwrap_or_default(),
        once: opts.once,
        chan: caller.channel,
        source,
    });
    Ok(id)
}
/// End a subscription (`event.on`'s handle); whether there was one.
#[api(name = "event.off", example(args = "[8]", result = "true"))]
pub fn pumi_event_off(cx: &mut Cx, subscription: u64) -> Result<bool> {
    let (_, k) = kernel(cx)?;
    let before = k.events.autocmds.len();
    k.events.autocmds.retain(|a| a.id != subscription);
    Ok(before != k.events.autocmds.len())
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventEmitOpts {
    #[serde(default)]
    pub data: Value,
    /// The event's `match`: what its listeners' `match` is tested against.
    #[serde(default, rename = "match")]
    pub matched: Option<String>,
    /// The tab it is about; one just closed may be named.
    #[serde(default)]
    pub tab: Option<TabRef>,
}
/// Fire an event to every channel subscribed: mostly a plugin's own (`owner.name.done`), with
/// which plugins and scripts tell each other things, and the events channels have of their own
/// (the CLI's `plugin.reloadRequested`).
#[api(
    name = "event.emit",
    example(args = r#"["owner.name.done", {"data": {"status": 0}, "tab": "%3"}]"#, result = "null")
)]
pub fn pumi_event_emit(cx: &mut Cx, event: String, opts: Option<EventEmitOpts>) -> Result<()> {
    check_event(&event, false)?;
    let (caller, k) = kernel(cx)?;
    let opts = opts.unwrap_or_default();
    let tab = match opts.tab {
        Some(TabRef(p)) if !k.model.tabs.contains_key(&p) && p <= k.ids.tab => Some(p),
        p => p.map(|p| k.model.tab_id(&caller, p.0)).transpose()?,
    };
    let about = About { tab, ..About::default() };
    k.emit(&event, &opts.matched.unwrap_or_default(), about, if opts.data.is_null() { json!({}) } else { opts.data });
    Ok(())
}
