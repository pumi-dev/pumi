use pumi_proto::native::AttachServer;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};
pub const LOCAL_BUDGET: usize = 4 << 20;
pub struct Feed {
    state: Mutex<State>,
    ready: Condvar,
    budget: usize,
}
#[derive(Default)]
struct State {
    queue: VecDeque<AttachServer>,
    bytes: usize,
    behind: bool,
    closed: bool,
}
#[derive(Debug, PartialEq)]
pub enum Take {
    Messages(Vec<AttachServer>),
    Behind,
    Closed,
    Idle,
}
impl Feed {
    pub fn new(budget: usize) -> Self {
        Self { state: Mutex::default(), ready: Condvar::new(), budget }
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    pub fn output(&self, seq: u64, bytes: &[u8]) {
        let mut state = self.lock();
        if state.behind || state.closed {
            return;
        }
        match state.queue.back_mut() {
            Some(AttachServer::Output { seq: last, bytes: into }) => {
                into.extend_from_slice(bytes);
                *last = seq;
            }
            _ => state.queue.push_back(AttachServer::Output { seq, bytes: bytes.to_vec() }),
        }
        state.bytes += bytes.len();
        if state.bytes > self.budget {
            state.queue.retain(|msg| !matches!(msg, AttachServer::Output { .. } | AttachServer::Resized { .. }));
            state.bytes = 0;
            state.behind = true;
        }
        drop(state);
        self.ready.notify_one();
    }
    pub fn push(&self, msg: AttachServer) {
        let mut state = self.lock();
        if state.closed || (state.behind && matches!(msg, AttachServer::Resized { .. })) {
            return;
        }
        state.queue.push_back(msg);
        drop(state);
        self.ready.notify_one();
    }
    pub fn caught_up(&self, snapshot: AttachServer) {
        let mut state = self.lock();
        state.behind = false;
        state.queue.push_front(snapshot);
        drop(state);
        self.ready.notify_one();
    }
    pub fn close(&self) {
        self.lock().closed = true;
        self.ready.notify_all();
    }
    pub fn take(&self, timeout: Option<Duration>) -> Take {
        let deadline = timeout.map(|t| Instant::now() + t);
        let mut state = self.lock();
        loop {
            if state.behind {
                return Take::Behind;
            }
            if !state.queue.is_empty() {
                state.bytes = 0;
                return Take::Messages(state.queue.drain(..).collect());
            }
            if state.closed {
                return Take::Closed;
            }
            state = match deadline {
                None => self.ready.wait(state).unwrap_or_else(|poisoned| poisoned.into_inner()),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Take::Idle;
                    }
                    self.ready.wait_timeout(state, left).unwrap_or_else(|poisoned| poisoned.into_inner()).0
                }
            };
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn out(seq: u64, bytes: &[u8]) -> AttachServer {
        AttachServer::Output { seq, bytes: bytes.to_vec() }
    }
    #[test]
    fn output_merges_and_keeps_its_place_among_messages() {
        let feed = Feed::new(1024);
        feed.output(1, b"a");
        feed.output(2, b"b");
        feed.push(AttachServer::Resized { cols: 10, rows: 2 });
        feed.output(3, b"c");
        let expected = vec![out(2, b"ab"), AttachServer::Resized { cols: 10, rows: 2 }, out(3, b"c")];
        assert_eq!(feed.take(None), Take::Messages(expected));
        assert_eq!(feed.take(Some(Duration::from_millis(1))), Take::Idle);
    }
    #[test]
    fn a_client_over_budget_gets_the_state_not_the_backlog() {
        let feed = Feed::new(4);
        feed.output(3, b"abc");
        feed.push(AttachServer::ControlLost);
        feed.output(6, b"def");
        feed.output(7, b"g");
        assert_eq!(feed.take(None), Take::Behind);
        let snapshot = AttachServer::Snapshot {
            cols: 2,
            rows: 2,
            bytes: b"g".to_vec(),
            seq: 7,
            mode: pumi_proto::native::Mode::Observe,
        };
        feed.caught_up(snapshot.clone());
        feed.output(8, b"h");
        assert_eq!(feed.take(None), Take::Messages(vec![snapshot, AttachServer::ControlLost, out(8, b"h")]));
        feed.close();
        assert_eq!(feed.take(None), Take::Closed);
    }
}
