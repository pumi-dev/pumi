//! What each terminal tab costs the app and how long it took to show: counters its threads and
//! its view add to as they work, read once a second by the sampler. It only adds; it never waits
//! on a terminal or measures one itself.
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// A terminal's counters, the session's to add to.
#[derive(Clone)]
pub struct Handle(Arc<Mutex<Counters>>);

#[derive(Debug, Default)]
struct Counters {
    tab: String,
    opened: Option<Instant>,
    sized: Option<Instant>,
    attached: Option<Instant>,
    first_output: Option<Instant>,
    first_paint: Option<Instant>,
    attaches: u32,
    snapshot_bytes: u64,
    size: (u16, u16),
    observing: bool,
    typed: Option<Instant>,
    second: Second,
}

/// One second of a terminal, taken by the sampler.
#[derive(Debug, Default, Clone)]
struct Second {
    bytes: u64,
    records: u64,
    parse: Duration,
    screen: Duration,
    screens: u32,
    paint: Duration,
    paints: u32,
    shaped: u64,
    echo_max: Option<Duration>,
}

fn registry() -> MutexGuard<'static, HashMap<usize, Handle>> {
    static TABS: OnceLock<Mutex<HashMap<usize, Handle>>> = OnceLock::new();
    lock(TABS.get_or_init(Default::default))
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn ms(duration: Duration) -> f32 {
    (duration.as_secs_f64() * 1000.0) as f32
}

fn between(from: Option<Instant>, to: Option<Instant>) -> Option<f32> {
    Some(ms(to?.checked_duration_since(from?)?))
}

impl Handle {
    /// A terminal of `tab` the app has just opened.
    pub fn open(tab: &str) -> Self {
        let handle = Self(Arc::new(Mutex::new(Counters {
            tab: tab.to_string(),
            opened: Some(Instant::now()),
            ..Counters::default()
        })));
        registry().insert(Arc::as_ptr(&handle.0) as usize, handle.clone());
        handle
    }

    /// The terminal is closed: it is no longer listed.
    pub fn close(&self) {
        registry().remove(&(Arc::as_ptr(&self.0) as usize));
    }

    fn with(&self, change: impl FnOnce(&mut Counters)) {
        change(&mut lock(&self.0));
    }

    /// The tab's box was measured: it has a size to attach with.
    pub fn sized(&self, cols: u16, rows: u16) {
        self.with(|c| {
            c.sized.get_or_insert_with(Instant::now);
            c.size = (cols, rows);
        });
    }

    /// The server took the attach (`observing`: someone else has control).
    pub fn attached(&self, observing: bool) {
        self.with(|c| {
            c.attached.get_or_insert_with(Instant::now);
            c.attaches += 1;
            c.observing = observing;
        });
    }

    /// The screen the server replayed on attaching.
    pub fn snapshot(&self, bytes: usize) {
        self.with(|c| c.snapshot_bytes += bytes as u64);
    }

    /// Output the terminal parsed, and what parsing it took.
    pub fn output(&self, bytes: usize, parse: Duration) {
        self.with(|c| {
            let now = Instant::now();
            if bytes > 0 {
                c.first_output.get_or_insert(now);
            }
            c.second.bytes += bytes as u64;
            c.second.records += 1;
            c.second.parse += parse;
            if bytes > 0
                && let Some(typed) = c.typed.take()
            {
                let echo = now.saturating_duration_since(typed);
                c.second.echo_max = Some(c.second.echo_max.map_or(echo, |max| max.max(echo)));
            }
        });
    }

    /// Something was typed into the program: its next output is its echo.
    pub fn typed(&self) {
        self.with(|c| {
            c.typed.get_or_insert_with(Instant::now);
        });
    }

    /// A screen was read out of the terminal, in `took`.
    pub fn screen(&self, took: Duration) {
        self.with(|c| {
            c.second.screen += took;
            c.second.screens += 1;
        });
    }

    /// The view laid its rows out for a frame: `shaped` of them anew, in `took`. `shown` is whether
    /// the screen had anything on it.
    pub fn painted(&self, took: Duration, shaped: usize, shown: bool) {
        self.with(|c| {
            if shown {
                c.first_paint.get_or_insert_with(Instant::now);
            }
            c.second.paint += took;
            c.second.paints += 1;
            c.second.shaped += shaped as u64;
        });
    }
}

/// One terminal, as one second of the sampler has it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Terminal {
    pub tab: String,
    pub cols: u16,
    pub rows: u16,
    pub observing: bool,
    /// Bytes of output per second, and the records they came in.
    pub bytes: u64,
    pub records: u64,
    /// Milliseconds this second spent parsing output, reading screens and laying rows out.
    pub parse_ms: f32,
    pub screen_ms: f32,
    pub paint_ms: f32,
    pub paints: u32,
    /// Rows shaped anew (a row whose cells did not change keeps its shape).
    pub shaped: u64,
    /// The longest a key took to be echoed (typed to the program's next output), this second.
    pub echo_ms: Option<f32>,
    pub startup: Startup,
}

/// How long a terminal took to show, each step from the one before, in milliseconds.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Startup {
    /// Opened until its box had a size (the window's layout).
    pub size_ms: Option<f32>,
    /// Sized until the server took the attach.
    pub attach_ms: Option<f32>,
    /// Attached until the first output.
    pub output_ms: Option<f32>,
    /// Opened until its first screen with something on it was drawn.
    pub shown_ms: Option<f32>,
    pub attaches: u32,
    pub snapshot_bytes: u64,
}

impl Terminal {
    /// Everything this terminal cost the app this second.
    pub fn cost_ms(&self) -> f32 {
        self.parse_ms + self.screen_ms + self.paint_ms
    }
}

/// Every terminal's last second, taken: the counts start again.
pub fn take() -> Vec<Terminal> {
    let handles: Vec<Handle> = registry().values().cloned().collect();
    let mut out: Vec<Terminal> = handles
        .iter()
        .map(|handle| {
            let mut c = lock(&handle.0);
            let second = std::mem::take(&mut c.second);
            Terminal {
                tab: c.tab.clone(),
                cols: c.size.0,
                rows: c.size.1,
                observing: c.observing,
                bytes: second.bytes,
                records: second.records,
                parse_ms: ms(second.parse),
                screen_ms: ms(second.screen),
                paint_ms: ms(second.paint),
                paints: second.paints,
                shaped: second.shaped,
                echo_ms: second.echo_max.map(ms),
                startup: Startup {
                    size_ms: between(c.opened, c.sized),
                    attach_ms: between(c.sized, c.attached),
                    output_ms: between(c.attached, c.first_output),
                    shown_ms: between(c.opened, c.first_paint),
                    attaches: c.attaches,
                    snapshot_bytes: c.snapshot_bytes,
                },
            }
        })
        .collect();
    out.sort_by(|a, b| tab_number(&a.tab).cmp(&tab_number(&b.tab)).then_with(|| a.tab.cmp(&b.tab)));
    out
}

fn tab_number(tab: &str) -> u64 {
    tab.trim_start_matches('%').parse().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(tab: &str) -> Terminal {
        take().into_iter().find(|t| t.tab == tab).expect("listed")
    }

    /// The registry is the process's: a test's `take` empties every terminal's second, so they
    /// take turns.
    fn alone() -> MutexGuard<'static, ()> {
        static TURN: Mutex<()> = Mutex::new(());
        TURN.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn a_second_is_counted_once_and_starts_again() {
        let _turn = alone();
        let handle = Handle::open("%90001");
        handle.output(100, Duration::from_millis(2));
        handle.output(50, Duration::from_millis(1));
        handle.painted(Duration::from_millis(3), 4, true);
        let first = of("%90001");
        assert_eq!((first.bytes, first.records, first.shaped), (150, 2, 4));
        assert!((first.parse_ms - 3.0).abs() < 0.01, "{}", first.parse_ms);
        let second = of("%90001");
        assert_eq!((second.bytes, second.paints), (0, 0), "taken: the next second starts at nothing");
        assert!(second.startup.shown_ms.is_some(), "how it started is kept");
        handle.close();
        assert!(take().iter().all(|t| t.tab != "%90001"), "a closed terminal goes");
    }

    #[test]
    fn an_echo_is_the_first_output_after_a_key() {
        let _turn = alone();
        let handle = Handle::open("%90002");
        handle.output(10, Duration::ZERO);
        assert_eq!(of("%90002").echo_ms, None, "output nobody typed for is no echo");
        handle.typed();
        handle.output(0, Duration::ZERO);
        handle.output(1, Duration::ZERO);
        handle.output(1, Duration::ZERO);
        let echoed = of("%90002");
        assert!(echoed.echo_ms.is_some());
        assert_eq!(of("%90002").echo_ms, None, "one key, one echo");
        handle.close();
    }

    #[test]
    fn startup_steps_follow_each_other() {
        let _turn = alone();
        let handle = Handle::open("%90003");
        assert_eq!(of("%90003").startup.size_ms, None, "not sized yet");
        handle.sized(80, 24);
        handle.attached(false);
        handle.snapshot(4096);
        handle.output(1, Duration::ZERO);
        let terminal = of("%90003");
        let startup = &terminal.startup;
        assert!(startup.size_ms.is_some() && startup.attach_ms.is_some() && startup.output_ms.is_some());
        assert_eq!((startup.attaches, startup.snapshot_bytes, terminal.cols), (1, 4096, 80));
        handle.close();
    }
}
