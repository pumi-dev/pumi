//! Messages for agents: one waits in the core until its agent is ready for it, then goes to the
//! agent as a real message, through whatever runs the agent (its stop hook, its extension, its
//! plugin), which asks for it with `agent.take`. The core never pushes and never types: an agent
//! whose tab does not accept a queue (`@agent.accepts`) takes no item.
//!
//! An item is for one session of its agent: when the tab's `@agent.session` changes, or the
//! tab closes, its items go. `take` answers one item at a time, oldest first, and only while the
//! tab is not `blocked` (a permission or a question open) and shows no `error` not yet seen.
//! What waits is the `queue` field of the tab's agent trait; each change to it fires
//! `tab.changed {keys: ["traits.agent"]}`.
use super::events::About;
use super::options::Key;
use super::state::rfc3339;
use super::{ChanId, Kernel, TabId};
use pumi_api::TabRef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
/// The most one item's text may be, in bytes.
pub const TEXT_LIMIT: usize = 64 * 1024;
/// The most items one tab may have waiting.
pub const TAB_LIMIT: usize = 100;
/// A message waiting for its agent, as a saved state keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueuedItem {
    pub id: u64,
    pub tab: TabId,
    /// The agent's session it is for (`@agent.session` when it was queued); `null`: the tab had
    /// none.
    pub session: Option<String>,
    /// What the agent is sent, as it was given.
    pub text: String,
    /// Who queued it: a plugin's id, `cli`, or `agent %4 (pi)` from an agent's tab.
    pub from: String,
    /// When, in Unix milliseconds.
    pub created: u64,
}
/// A message `agent.take` answers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TakenMessage {
    pub id: u64,
    pub tab: TabRef,
    pub session: Option<String>,
    pub text: String,
    pub from: Option<String>,
    /// RFC 3339.
    pub created: String,
}
impl QueuedItem {
    fn created(&self) -> String {
        rfc3339((self.created / 1000) as i64)
    }
    pub fn taken(&self) -> TakenMessage {
        TakenMessage {
            id: self.id,
            tab: TabRef(self.tab),
            session: self.session.clone(),
            text: self.text.clone(),
            from: Some(self.from.clone()),
            created: self.created(),
        }
    }
}
/// Why an item left the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gone {
    /// Its agent took it.
    Taken,
    /// Someone took it back (`agent.unqueue`).
    Cancelled,
    /// Its tab's session changed.
    Session,
    /// Its tab closed.
    Closed,
}
/// A caller blocked in `agent.take` until an item comes.
#[derive(Debug)]
pub struct Taker {
    pub tab: TabId,
    pub session: String,
    pub waiter: Option<(ChanId, Value)>,
}
#[derive(Default)]
pub struct Queue {
    pub items: BTreeMap<u64, QueuedItem>,
    next: u64,
    pub takers: BTreeMap<u64, Taker>,
    next_taker: u64,
    /// Set by `agent.take` with `wait` when its caller is to be answered later.
    pub held: Option<u64>,
}
impl Queue {
    /// Takes back the items a saved state kept; the ids go on after theirs.
    pub fn restore(&mut self, items: Vec<QueuedItem>) {
        for item in items {
            self.next = self.next.max(item.id);
            self.items.insert(item.id, item);
        }
    }
    /// The `queue` field of `tab`'s agent trait: its messages, oldest first, as `{id, text,
    /// from, created}`.
    pub fn trait_queue(&self, tab: TabId) -> Value {
        let items = self.items.values().filter(|i| i.tab == tab);
        Value::Array(
            items.map(|i| json!({ "id": i.id, "text": i.text, "from": i.from, "created": i.created() })).collect(),
        )
    }
    /// Starts waiting for an item for `tab`'s `session`; the server attaches the caller.
    pub fn wait(&mut self, tab: TabId, session: String) -> u64 {
        self.next_taker += 1;
        self.takers.insert(self.next_taker, Taker { tab, session, waiter: None });
        self.held = Some(self.next_taker);
        self.next_taker
    }
    pub fn forget_taker(&mut self, id: u64) {
        self.takers.remove(&id);
    }
}
/// Whether `tab` takes queued messages: it is an agent, and its `@agent.accepts` says `queue`.
pub fn accepts(kernel: &Kernel, tab: TabId) -> Result<(), String> {
    if !crate::api::traits::has(kernel, tab, "agent") {
        return Err(format!("tab %{tab} is not an agent (it has no `@agent`)"));
    }
    let accepts = kernel.tab_option(tab, "@agent.accepts");
    if !accepts.as_array().is_some_and(|words| words.iter().any(|w| w == "queue")) {
        let agent = kernel.tab_option(tab, "@agent");
        let agent = agent.as_str().unwrap_or("agent");
        return Err(format!(
            "the {agent} in %{tab} takes no queued messages (its `@agent.accepts` has no `queue`): pumi has no way to send it one without typing into it"
        ));
    }
    Ok(())
}
/// Queues `text` for `tab`'s agent, for `session`; returns the item. Checks are the caller's.
pub fn push(kernel: &mut Kernel, tab: TabId, text: String, session: Option<String>, from: String) -> QueuedItem {
    kernel.queue.next += 1;
    let item = QueuedItem { id: kernel.queue.next, tab, session, text, from, created: super::requests::now_ms() };
    kernel.queue.items.insert(item.id, item.clone());
    changed(kernel, tab);
    deliver(kernel);
    item
}
/// `tab`'s queue changed: `tab.changed` patches its agent trait's `queue`.
fn changed(kernel: &mut Kernel, tab: TabId) {
    let id = TabRef(tab);
    let patch = json!({ "traits": { "agent": { "queue": kernel.queue.trait_queue(tab) } } });
    let data = json!({ "tab": id, "keys": ["traits.agent"], "patch": patch });
    kernel.emit("tab.changed", &id.to_string(), About::tab(tab), data);
}
/// Takes item `id` out of the queue; whether it was there. `why` is the caller's to know: the
/// queue only says what is left.
pub fn remove(kernel: &mut Kernel, id: u64, why: Gone) -> Option<QueuedItem> {
    let item = kernel.queue.items.remove(&id)?;
    // A closed tab says so itself.
    if why != Gone::Closed {
        changed(kernel, item.tab);
    }
    Some(item)
}
/// Whether `tab` waits on the user: a permission or a question open, or a result (`error`) not
/// yet seen, or its program says it is `blocked`.
pub fn holds(kernel: &Kernel, tab: TabId) -> bool {
    kernel.requests.holds(tab) || matches!(kernel.tab_option(tab, "@agent.status").as_str(), Some("blocked" | "error"))
}
/// The oldest item for `tab`'s `session`, taken out; none while the tab holds.
pub fn take(kernel: &mut Kernel, tab: TabId, session: &str) -> Option<QueuedItem> {
    if holds(kernel, tab) {
        return None;
    }
    let id = kernel.queue.items.values().find(|i| i.tab == tab && i.session.as_deref() == Some(session))?.id;
    remove(kernel, id, Gone::Taken)
}
/// Answers each blocked `agent.take` an item has come for.
pub fn deliver(kernel: &mut Kernel) {
    if kernel.queue.items.is_empty() || kernel.queue.takers.is_empty() {
        return;
    }
    let ready: Vec<(u64, TabId, String)> = kernel
        .queue
        .takers
        .iter()
        .filter(|(_, t)| t.waiter.is_some())
        .map(|(id, t)| (*id, t.tab, t.session.clone()))
        .collect();
    for (id, tab, session) in ready {
        if !kernel.queue.takers.contains_key(&id) {
            continue;
        }
        let Some(item) = take(kernel, tab, &session) else { continue };
        let Some(taker) = kernel.queue.takers.remove(&id) else { continue };
        if let Some((chan, message)) = taker.waiter {
            let value = serde_json::to_value(item.taken()).expect("plain data");
            kernel.send(chan, pumi_api::rpc::reply(&message, Ok(value)));
        }
    }
}
/// `tab`'s `@agent.session` changed: its items for another session go, and so do the callers
/// waiting for one of the old session's.
pub fn session_changed(kernel: &mut Kernel, tab: TabId) {
    let now = kernel.model.options.get_local("@agent.session", Key::Tab(tab)).map(|s| s.value.clone());
    let now = now.as_ref().and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string);
    let gone: Vec<u64> =
        kernel.queue.items.values().filter(|i| i.tab == tab && i.session != now).map(|i| i.id).collect();
    for id in gone {
        remove(kernel, id, Gone::Session);
    }
    let stale: Vec<u64> = kernel
        .queue
        .takers
        .iter()
        .filter(|(_, t)| t.tab == tab && Some(&t.session) != now.as_ref())
        .map(|(id, _)| *id)
        .collect();
    answer_none(kernel, stale);
}
/// `tab` closed: its items go, and the callers waiting for one are answered `null`.
pub fn tab_closed(kernel: &mut Kernel, tab: TabId) {
    let gone: Vec<u64> = kernel.queue.items.values().filter(|i| i.tab == tab).map(|i| i.id).collect();
    for id in gone {
        remove(kernel, id, Gone::Closed);
    }
    let waiting: Vec<u64> = kernel.queue.takers.iter().filter(|(_, t)| t.tab == tab).map(|(id, _)| *id).collect();
    answer_none(kernel, waiting);
}
fn answer_none(kernel: &mut Kernel, takers: Vec<u64>) {
    for id in takers {
        if let Some(Taker { waiter: Some((chan, message)), .. }) = kernel.queue.takers.remove(&id) {
            kernel.send(chan, pumi_api::rpc::reply(&message, Ok(Value::Null)));
        }
    }
}
/// A channel closed: what it waited for goes, and nothing is taken for it.
pub fn chan_closed(kernel: &mut Kernel, chan: ChanId) {
    kernel.queue.takers.retain(|_, t| t.waiter.as_ref().is_none_or(|(c, _)| *c != chan));
}
