//! The frame loop, one second: how often JS pumped AppKit (the only way the main thread's work,
//! and so every frame, gets done), what woke it, and whether the display link's wakes arrive.
//! A window can draw only when JS pumps, so a window at the display's rate needs a pump a frame;
//! every one the fallback timer started instead is a wake that did not come.
use pumi_gpuix::frame_loop::{self, Stats};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Default, Serialize)]
pub struct FrameLoop {
    /// The main display's most frames a second (120 on ProMotion), 0 when unknown.
    pub refresh_hz: f32,
    /// Pumps a second, and of them the ones a wake started and the ones the fallback timer did.
    pub pumps_per_s: f32,
    pub woken_per_s: f32,
    pub fallback_per_s: f32,
    /// Wakes asked a second: by the display link (about `refresh_hz` while a window shows), by
    /// input, by other work for the main thread.
    pub display_per_s: f32,
    pub input_per_s: f32,
    pub other_per_s: f32,
    /// Wakes sent to JS a second, folded into one already on its way, sent again because one was
    /// late (lost), that could not be queued, and that arrived.
    pub sent_per_s: f32,
    pub coalesced_per_s: f32,
    pub resent_per_s: f32,
    pub failed_per_s: f32,
    pub arrived_per_s: f32,
    /// How late wakes arrived after they were sent: mean and worst.
    pub late_mean_ms: f32,
    pub late_max_ms: f32,
    /// The longest between two pumps, and the longest pump.
    pub gap_max_ms: f32,
    pub pump_max_ms: f32,
    /// How long the wake on its way has waited, when the second ended (0: none is).
    pub pending_ms: f32,
    /// How long JS waits for a wake before it pumps anyway, now.
    pub fallback_ms: f32,
    /// Since the app started: wakes sent, arrived and sent again, pumps the fallback started.
    pub total_sent: u64,
    pub total_arrived: u64,
    pub total_resent: u64,
    pub total_fallback: u64,
}

impl FrameLoop {
    /// One frame at the display's rate, in milliseconds (60 Hz when it is not known).
    pub fn frame_ms(&self) -> f32 {
        if self.refresh_hz > 0.0 { 1000.0 / self.refresh_hz } else { super::FRAME_MS }
    }
}

pub struct Reader {
    last: Stats,
}

impl Reader {
    pub fn new() -> Self {
        Self { last: frame_loop::take() }
    }

    /// Forgets what came before: the next read is from now.
    pub fn skim(&mut self) {
        self.last = frame_loop::take();
    }

    /// Since the last read, over `wall`; `refresh_hz` is the display's.
    pub fn read(&mut self, wall: Duration, refresh_hz: f32, fallback: Duration) -> FrameLoop {
        let now = frame_loop::take();
        let was = std::mem::replace(&mut self.last, now);
        summarize(&was, &now, wall, refresh_hz, fallback)
    }
}

fn ms(ns: u64) -> f32 {
    ns as f32 / 1_000_000.0
}

fn summarize(was: &Stats, now: &Stats, wall: Duration, refresh_hz: f32, fallback: Duration) -> FrameLoop {
    let seconds = wall.as_secs_f32().max(0.001);
    let rate = |f: fn(&Stats) -> u64| f(now).saturating_sub(f(was)) as f32 / seconds;
    let arrived = now.arrived.saturating_sub(was.arrived);
    let late = now.late_sum_ns.saturating_sub(was.late_sum_ns);
    FrameLoop {
        refresh_hz,
        pumps_per_s: rate(Stats::pumps),
        woken_per_s: rate(|s| s.pumps_woken),
        fallback_per_s: rate(|s| s.pumps_unwoken),
        display_per_s: rate(|s| s.asked_display),
        input_per_s: rate(|s| s.asked_input),
        other_per_s: rate(|s| s.asked_other),
        sent_per_s: rate(|s| s.sent),
        coalesced_per_s: rate(|s| s.coalesced),
        resent_per_s: rate(|s| s.resent),
        failed_per_s: rate(|s| s.failed),
        arrived_per_s: rate(|s| s.arrived),
        late_mean_ms: if arrived == 0 { 0.0 } else { ms(late / arrived) },
        late_max_ms: ms(now.late_max_ns),
        // A pump that has not come since counts as the gap it is so far.
        gap_max_ms: ms(now.gap_max_ns.max(now.since_pump_ns)),
        pump_max_ms: ms(now.pump_max_ns),
        pending_ms: ms(now.pending_ns),
        fallback_ms: fallback.as_secs_f32() * 1000.0,
        total_sent: now.sent,
        total_arrived: now.arrived,
        total_resent: now.resent,
        total_fallback: now.pumps_unwoken,
    }
}

/// The main display's most frames a second (`NSScreen.mainScreen.maximumFramesPerSecond` on
/// macOS; 0 where it is not read). The main thread's.
pub fn refresh_hz() -> f32 {
    super::os::refresh_hz()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_is_the_difference_over_the_wall() {
        let was =
            Stats { sent: 100, arrived: 90, pumps_woken: 80, pumps_unwoken: 5, late_sum_ns: 0, ..Default::default() };
        let now = Stats {
            sent: 340,
            arrived: 330,
            pumps_woken: 300,
            pumps_unwoken: 25,
            late_sum_ns: 240 * 2_000_000,
            asked_display: 120,
            gap_max_ns: 12_000_000,
            since_pump_ns: 3_000_000,
            pending_ns: 1_000_000,
            ..Default::default()
        };
        let second = summarize(&was, &now, Duration::from_secs(2), 120.0, Duration::from_millis(10));
        assert_eq!((second.sent_per_s, second.arrived_per_s), (120.0, 120.0));
        assert_eq!((second.pumps_per_s, second.woken_per_s, second.fallback_per_s), (120.0, 110.0, 10.0));
        assert_eq!((second.late_mean_ms, second.gap_max_ms, second.pending_ms), (2.0, 12.0, 1.0));
        assert_eq!((second.fallback_ms, second.total_fallback), (10.0, 25));
        assert!((second.frame_ms() - 8.333).abs() < 0.01);
    }

    #[test]
    fn a_pump_that_has_not_come_is_a_gap_already() {
        let now = Stats { gap_max_ns: 9_000_000, since_pump_ns: 400_000_000, ..Default::default() };
        let second = summarize(&Stats::default(), &now, Duration::from_secs(1), 120.0, Duration::from_millis(10));
        assert_eq!(second.gap_max_ms, 400.0);
    }
}
