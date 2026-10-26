pub use pumi_model::event::{About, Event, check_event};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Events {
    pub groups: BTreeMap<u64, String>,
    next_id: u64,
}
pub const SUBSCRIPTION: &str = "pumi-host mirrors the model";
pub const LOCAL: u64 = 1_000_000;
impl Events {
    pub fn next_id(&mut self) -> u64 {
        self.next_id = self.next_id.max(LOCAL) + 1;
        self.next_id
    }
    pub fn group_id(&self, name: &str) -> Option<u64> {
        self.groups.iter().find(|(_, n)| n.as_str() == name).map(|(id, _)| *id)
    }
}
