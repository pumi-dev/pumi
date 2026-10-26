//! Linux and Windows: nothing is read yet.
use super::{Task, Thread};
use std::collections::HashMap;

/// No compositor is looked for.
pub const COMPOSITOR: &str = "";

pub fn name(_: i32) -> Option<String> {
    None
}
pub fn children(_: i32) -> Vec<i32> {
    Vec::new()
}
pub fn find(_: &str) -> Option<i32> {
    None
}
pub fn task(_: i32) -> Option<Task> {
    None
}
pub fn threads() -> Vec<Thread> {
    Vec::new()
}
pub fn gpu_utilisation() -> Option<f32> {
    None
}
pub fn gpu_times() -> Option<HashMap<i32, u64>> {
    None
}
