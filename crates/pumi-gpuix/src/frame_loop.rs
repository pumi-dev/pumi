//! Waking the JS thread that pumps AppKit, and what it costs.
//!
//! AppKit is pumped from JS (`tick`), and nothing wakes that thread when work arrives for the main
//! thread. Three things ask: the display link once a frame ([`Source::Display`]), input when the
//! system sees some ([`Source::Input`]), anything else through `app::wake` ([`Source::Other`]). A
//! request reaches JS through the hook `setFrameWakeCallback` installs ([`set_sender`]).
//!
//! Requests coalesce while one is on its way: that ends with the wake's arrival, with a pump, or
//! with [`STALE`] passing and the wake sent again. A wake lost on the way (JS never called back)
//! therefore costs a frame or two, not every frame after it.
//!
//! Everything is counted ([`take`]): `app.profile` and the Diagnostics window show how many wakes
//! were asked, sent, coalesced, sent again and arrived, how late, and how often JS pumped, woken or
//! not, with the longest gap between two pumps.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Sends one wake to JS; whether it was queued.
pub type Sender = Arc<dyn Fn() -> bool + Send + Sync>;

/// What asked for a pump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The display link: a frame is due.
    Display,
    /// The system saw a key, a button or the wheel while a window is key.
    Input,
    /// Work queued for the main thread (`app::wake`).
    Other,
}

/// A wake not arrived after this long is sent again: three frames at 120 Hz.
pub const STALE: Duration = Duration::from_millis(25);
/// How long JS waits for a wake before it pumps anyway: while a window is key, about a frame, so
/// even wakes that never arrive leave the window near its frame rate; otherwise a tenth of a
/// second, which only work for hidden windows waits on.
pub const FALLBACK_KEY: Duration = Duration::from_millis(10);
pub const FALLBACK_IDLE: Duration = Duration::from_millis(100);

/// The wake state and its counters, over a clock of nanoseconds (never 0: 0 is "none").
#[derive(Default)]
struct Wakes {
    /// When the wake on its way was sent; 0 when none is.
    pending: AtomicU64,
    asked_display: AtomicU64,
    asked_input: AtomicU64,
    asked_other: AtomicU64,
    coalesced: AtomicU64,
    resent: AtomicU64,
    sent: AtomicU64,
    failed: AtomicU64,
    unhooked: AtomicU64,
    arrived: AtomicU64,
    late_sum: AtomicU64,
    late_max: AtomicU64,
    /// A wake arrived since the last pump: the next pump is a woken one.
    woken: AtomicBool,
    pumps_woken: AtomicU64,
    pumps_unwoken: AtomicU64,
    last_pump: AtomicU64,
    gap_max: AtomicU64,
    pump_max: AtomicU64,
}

impl Wakes {
    /// A request at `now`: whether to send a wake (the caller then reports [`Wakes::sent`]).
    fn ask(&self, source: Source, now: u64) -> bool {
        let asked = match source {
            Source::Display => &self.asked_display,
            Source::Input => &self.asked_input,
            Source::Other => &self.asked_other,
        };
        asked.fetch_add(1, Ordering::Relaxed);
        let mut since = self.pending.load(Ordering::Acquire);
        loop {
            if since != 0 && now.saturating_sub(since) < STALE.as_nanos() as u64 {
                self.coalesced.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            match self.pending.compare_exchange_weak(since, now, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => break,
                Err(current) => since = current,
            }
        }
        if since != 0 {
            self.resent.fetch_add(1, Ordering::Relaxed);
        }
        true
    }

    /// What sending the wake asked for at `at` did.
    fn sent(&self, at: u64, queued: bool) {
        if queued {
            self.sent.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed.fetch_add(1, Ordering::Relaxed);
            let _ = self.pending.compare_exchange(at, 0, Ordering::AcqRel, Ordering::Acquire);
        }
    }

    /// A wake reached JS at `now`, just before it pumps.
    fn arrived(&self, now: u64) {
        self.arrived.fetch_add(1, Ordering::Relaxed);
        let since = self.pending.swap(0, Ordering::AcqRel);
        if since != 0 {
            let late = now.saturating_sub(since);
            self.late_sum.fetch_add(late, Ordering::Relaxed);
            self.late_max.fetch_max(late, Ordering::Relaxed);
        }
        self.woken.store(true, Ordering::Release);
    }

    /// JS pumps at `now`: whatever was asked for runs now, so no wake is waited for any more.
    fn pump_began(&self, now: u64) {
        self.pending.store(0, Ordering::Release);
        let counter = if self.woken.swap(false, Ordering::AcqRel) { &self.pumps_woken } else { &self.pumps_unwoken };
        counter.fetch_add(1, Ordering::Relaxed);
        let last = self.last_pump.swap(now, Ordering::AcqRel);
        if last != 0 {
            self.gap_max.fetch_max(now.saturating_sub(last), Ordering::Relaxed);
        }
    }

    fn pump_ended(&self, began: u64, now: u64) {
        self.pump_max.fetch_max(now.saturating_sub(began), Ordering::Relaxed);
    }

    fn take(&self, now: u64) -> Stats {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        let pending = self.pending.load(Ordering::Acquire);
        let last_pump = self.last_pump.load(Ordering::Acquire);
        Stats {
            asked_display: load(&self.asked_display),
            asked_input: load(&self.asked_input),
            asked_other: load(&self.asked_other),
            coalesced: load(&self.coalesced),
            resent: load(&self.resent),
            sent: load(&self.sent),
            failed: load(&self.failed),
            unhooked: load(&self.unhooked),
            arrived: load(&self.arrived),
            late_sum_ns: load(&self.late_sum),
            pumps_woken: load(&self.pumps_woken),
            pumps_unwoken: load(&self.pumps_unwoken),
            late_max_ns: self.late_max.swap(0, Ordering::Relaxed),
            gap_max_ns: self.gap_max.swap(0, Ordering::Relaxed),
            pump_max_ns: self.pump_max.swap(0, Ordering::Relaxed),
            pending_ns: if pending == 0 { 0 } else { now.saturating_sub(pending) },
            since_pump_ns: if last_pump == 0 { 0 } else { now.saturating_sub(last_pump) },
        }
    }
}

/// The counters since the app started, and the worst since the last [`take`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Requests, by what asked.
    pub asked_display: u64,
    pub asked_input: u64,
    pub asked_other: u64,
    /// Requests folded into a wake already on its way.
    pub coalesced: u64,
    /// Wakes sent again because the one before had not arrived after [`STALE`].
    pub resent: u64,
    /// Wakes queued to JS, and the ones that could not be.
    pub sent: u64,
    pub failed: u64,
    /// Requests made with no hook to send them (before JS set one, or after it cleared it).
    pub unhooked: u64,
    /// Wakes that reached JS, and how late they were, summed.
    pub arrived: u64,
    pub late_sum_ns: u64,
    /// Pumps a wake started, and the ones that started without one (JS's fallback timer).
    pub pumps_woken: u64,
    pub pumps_unwoken: u64,
    /// Since the last take: the latest wake, the longest between two pumps, the longest pump.
    pub late_max_ns: u64,
    pub gap_max_ns: u64,
    pub pump_max_ns: u64,
    /// How long the wake on its way has been (0 when none is), and since the last pump.
    pub pending_ns: u64,
    pub since_pump_ns: u64,
}

impl Stats {
    pub fn asked(&self) -> u64 {
        self.asked_display + self.asked_input + self.asked_other
    }
    pub fn pumps(&self) -> u64 {
        self.pumps_woken + self.pumps_unwoken
    }
}

fn wakes() -> &'static Wakes {
    static WAKES: OnceLock<Wakes> = OnceLock::new();
    WAKES.get_or_init(Wakes::default)
}

/// Nanoseconds since the first call, never 0.
fn now() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_nanos() as u64 + 1
}

static SENDER: Mutex<Option<Sender>> = Mutex::new(None);

/// Sets (or, with `None`, clears) how a wake reaches JS. Any wake on its way is forgotten.
pub fn set_sender(sender: Option<Sender>) {
    let previous = std::mem::replace(&mut *SENDER.lock().unwrap_or_else(|p| p.into_inner()), sender);
    wakes().pending.store(0, Ordering::Release);
    drop(previous);
}

/// Asks JS to pump soon, from any thread.
pub fn request(source: Source) {
    let sender = SENDER.lock().unwrap_or_else(|p| p.into_inner()).clone();
    let wakes = wakes();
    let Some(sender) = sender else {
        wakes.unhooked.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let at = now();
    if wakes.ask(source, at) {
        wakes.sent(at, sender());
    }
}

/// A wake reached JS (the hook's callback, on the JS thread, before it pumps).
pub fn arrived() {
    wakes().arrived(now());
}

/// Pumps with `pump`, counting it.
pub fn pump<R>(pump: impl FnOnce() -> R) -> R {
    let wakes = wakes();
    let began = now();
    wakes.pump_began(began);
    let result = pump();
    wakes.pump_ended(began, now());
    result
}

/// The counters; the worst-since values start again.
pub fn take() -> Stats {
    wakes().take(now())
}

/// How long JS's frame loop waits for a wake before it pumps anyway.
pub fn fallback(any_key_window: bool) -> Duration {
    if any_key_window {
        FALLBACK_KEY
    } else {
        FALLBACK_IDLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    #[test]
    fn requests_coalesce_while_a_wake_is_on_its_way() {
        let w = Wakes::default();
        assert!(w.ask(Source::Display, 10 * MS));
        w.sent(10 * MS, true);
        assert!(!w.ask(Source::Display, 18 * MS));
        assert!(!w.ask(Source::Input, 19 * MS));
        w.arrived(20 * MS);
        assert!(w.ask(Source::Display, 27 * MS), "the wait ended when it arrived");
        let stats = w.take(30 * MS);
        assert_eq!((stats.asked_display, stats.asked_input, stats.coalesced, stats.sent), (3, 1, 2, 1));
        assert_eq!((stats.arrived, stats.late_max_ns), (1, 10 * MS));
    }

    #[test]
    fn a_lost_wake_does_not_swallow_every_one_after_it() {
        let w = Wakes::default();
        assert!(w.ask(Source::Display, MS));
        w.sent(MS, true);
        // It never arrives; JS pumps on its fallback timer.
        w.pump_began(100 * MS);
        assert!(w.ask(Source::Display, 101 * MS), "a pump ends the wait");
        w.sent(101 * MS, true);
        // That one is lost too, and no pump comes: after STALE it is sent again.
        assert!(!w.ask(Source::Display, 110 * MS));
        assert!(w.ask(Source::Display, 101 * MS + STALE.as_nanos() as u64));
        let stats = w.take(130 * MS);
        assert_eq!((stats.resent, stats.pumps_unwoken, stats.arrived), (1, 1, 0));
    }

    #[test]
    fn a_wake_that_cannot_be_queued_is_not_waited_for() {
        let w = Wakes::default();
        assert!(w.ask(Source::Other, MS));
        w.sent(MS, false);
        assert!(w.ask(Source::Other, 2 * MS));
        assert_eq!(w.take(3 * MS).failed, 1);
    }

    #[test]
    fn pumps_say_whether_a_wake_started_them_and_the_longest_gap() {
        let w = Wakes::default();
        w.pump_began(10 * MS);
        w.pump_ended(10 * MS, 11 * MS);
        assert!(w.ask(Source::Display, 12 * MS));
        w.arrived(13 * MS);
        w.pump_began(13 * MS);
        w.pump_began(113 * MS);
        let stats = w.take(120 * MS);
        assert_eq!((stats.pumps_woken, stats.pumps_unwoken), (1, 2));
        assert_eq!((stats.gap_max_ns, stats.pump_max_ns, stats.since_pump_ns), (100 * MS, MS, 7 * MS));
        let again = w.take(121 * MS);
        assert_eq!((again.gap_max_ns, again.pumps()), (0, 3), "the worst starts again, the counts do not");
    }

    #[test]
    fn a_pending_wake_says_how_long_it_has_waited() {
        let w = Wakes::default();
        assert!(w.ask(Source::Display, 5 * MS));
        w.sent(5 * MS, true);
        assert_eq!(w.take(45 * MS).pending_ns, 40 * MS);
    }
}
