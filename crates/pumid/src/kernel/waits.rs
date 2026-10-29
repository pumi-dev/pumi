//! `event.wait`: callers blocked until an event fires. A wait is answered with the first event
//! that matches it, or `null` when its time is up or its caller goes.
use super::events::{Event, fnmatch, listens_to};
use super::{ChanId, Kernel};
use pumi_api::{SessionRef, TabRef, ViewRef};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
/// What a caller waits for.
pub struct Wait {
    /// Names or globs.
    pub events: Vec<String>,
    /// Globs over the event's `match`; any without.
    pub patterns: Vec<String>,
    /// Unix ms; `0`: no end.
    pub deadline: u64,
}
/// An event as `event.wait` answers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaitedEvent {
    pub event: String,
    #[serde(rename = "match")]
    pub matched: String,
    pub session: Option<SessionRef>,
    pub view: Option<ViewRef>,
    pub tab: Option<TabRef>,
    pub data: Value,
}
impl WaitedEvent {
    pub fn of(event: &Event) -> WaitedEvent {
        WaitedEvent {
            event: event.event.clone(),
            matched: event.matched.clone(),
            session: event.about.session.map(SessionRef),
            view: event.about.view.map(ViewRef),
            tab: event.about.tab.map(TabRef),
            data: event.data.clone(),
        }
    }
}
#[derive(Default)]
pub struct Waits {
    pending: BTreeMap<u64, Wait>,
    next: u64,
    /// Set by `event.wait`: the server takes it after the call and keeps the caller's message id
    /// in `waiters` instead of replying.
    pub held: Option<u64>,
    pub waiters: BTreeMap<u64, (ChanId, Value)>,
}
impl Waits {
    pub fn add(&mut self, wait: Wait) -> u64 {
        self.next += 1;
        self.pending.insert(self.next, wait);
        self.held = Some(self.next);
        self.next
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn contains(&self, id: u64) -> bool {
        self.pending.contains_key(&id)
    }
    pub fn forget(&mut self, id: u64) {
        self.pending.remove(&id);
        self.waiters.remove(&id);
    }
}
fn answer(kernel: &mut Kernel, id: u64, value: Value) {
    kernel.waits.pending.remove(&id);
    if let Some((chan, message)) = kernel.waits.waiters.remove(&id) {
        kernel.send(chan, pumi_api::rpc::reply(&message, Ok(value)));
    }
}
/// `event` fired: every wait it matches is answered with it.
pub fn fired(kernel: &mut Kernel, event: &Event) {
    if kernel.waits.pending.is_empty() {
        return;
    }
    let matched: Vec<u64> = kernel
        .waits
        .pending
        .iter()
        .filter(|(_, w)| {
            w.events.iter().any(|e| listens_to(e, &event.event))
                && (w.patterns.is_empty() || w.patterns.iter().any(|p| fnmatch(p, &event.matched)))
        })
        .map(|(id, _)| *id)
        .collect();
    let value = serde_json::to_value(WaitedEvent::of(event)).unwrap_or(Value::Null);
    for id in matched {
        answer(kernel, id, value.clone());
    }
}
/// Ends the waits whose time is up by `now` (Unix ms), with `null`.
pub fn tick(kernel: &mut Kernel, now: u64) {
    let ended: Vec<u64> =
        kernel.waits.pending.iter().filter(|(_, w)| w.deadline != 0 && w.deadline <= now).map(|(id, _)| *id).collect();
    for id in ended {
        answer(kernel, id, Value::Null);
    }
}
/// A channel closed: what it waited on goes.
pub fn chan_closed(kernel: &mut Kernel, chan: ChanId) {
    let gone: Vec<u64> = kernel.waits.waiters.iter().filter(|(_, (c, _))| *c == chan).map(|(id, _)| *id).collect();
    for id in gone {
        kernel.waits.waiters.remove(&id);
        kernel.waits.pending.remove(&id);
    }
}
