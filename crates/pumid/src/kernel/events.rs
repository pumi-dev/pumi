use super::Kernel;
pub use pumi_model::event::{About, Event, check_event, fnmatch, listens_to};
use serde_json::{Value, json};
#[derive(Clone, Debug)]
pub struct Autocmd {
    pub id: u64,
    pub events: Vec<String>,
    pub patterns: Vec<String>,
    pub desc: String,
    pub once: bool,
    /// The channel its events go to, as `rpc.event` notifications.
    pub chan: u64,
    /// Who subscribed (its channel's name, or the plugin calling through it).
    pub source: String,
}
impl Autocmd {
    fn matches(&self, event: &Event) -> bool {
        self.events.iter().any(|e| listens_to(e, &event.event))
            && (self.patterns.is_empty() || self.patterns.iter().any(|p| fnmatch(p, &event.matched)))
    }
    pub fn info(&self) -> Value {
        json!({
            "id": self.id,
            "event": self.events,
            "pattern": self.patterns,
            "desc": self.desc,
            "once": self.once,
            "chan": self.chan,
            "source": self.source,
        })
    }
}
/// The events pumi fires for everyone, as the export lists them. The core fires others for
/// itself (`core.started`, `client.*`), and a plugin fires its own, `owner.name.x`.
pub const EVENTS: &[&str] = &[
    "session.opened",
    "session.closed",
    "session.changed",
    "view.opened",
    "view.closed",
    "view.changed",
    "tab.opened",
    "tab.closed",
    "tab.changed",
    "tab.alerted",
    "request.opened",
    "request.closed",
    "option.changed",
    "registry.changed",
    "app.changed",
];
#[derive(Default)]
pub struct Events {
    seq: u64,
    pub autocmds: Vec<Autocmd>,
    next_id: u64,
    pending: Vec<Event>,
}
const PENDING_LIMIT: usize = 100_000;
impl Events {
    pub fn seq(&self) -> u64 {
        self.seq
    }
    pub fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
    pub fn push(&mut self, event: &str, matched: &str, about: About, data: Value) -> u64 {
        self.seq += 1;
        if self.pending.len() >= PENDING_LIMIT {
            self.pending.remove(0);
        }
        self.pending.push(Event { seq: self.seq, event: event.into(), matched: matched.into(), about, data });
        self.seq
    }
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub fn chan_closed(&mut self, chan: u64) {
        self.autocmds.retain(|a| a.chan != chan);
    }
}
pub fn flush(kernel: &mut Kernel) {
    while kernel.events.has_pending() {
        let batch = std::mem::take(&mut kernel.events.pending);
        kernel.send_redraw(&batch);
        for event in &batch {
            super::waits::fired(kernel, event);
            let matching: Vec<Autocmd> = kernel.events.autocmds.iter().filter(|a| a.matches(event)).cloned().collect();
            for autocmd in matching {
                let message = pumi_api::rpc::notification("rpc.event", json!([autocmd.id, event]));
                if !kernel.send(autocmd.chan, message) || autocmd.once {
                    kernel.events.autocmds.retain(|a| a.id != autocmd.id);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patterns() {
        assert!(fnmatch("pane_*", "pane_width"));
        assert!(fnmatch("a?c", "abc"));
        assert!(!fnmatch("a?c", "ac"));
        assert!(fnmatch("*", ""));
    }
}
