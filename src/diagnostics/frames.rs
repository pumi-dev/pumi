//! What the main thread did, from GPUI's foreground journal: each window's frames (how long a
//! draw took, how long from the first change to the screen), how long input took to show, how
//! busy the thread was, and the slowest pieces of work with where they came from. It reads the
//! journal the main thread already keeps; it records nothing of its own.
use gpui::profiler::journal::{
    ForegroundEvent, ForegroundJournal, ForegroundJournalCollector, ForegroundJournalEntry, IntervalBoundary,
    IntervalSealer,
};
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

/// Work at least this long is kept as a hang, with when it was.
pub const HANG: Duration = Duration::from_millis(50);
/// The most hangs kept.
const HANGS: usize = 50;
/// The slowest pieces of work a second lists.
const SLOWEST: usize = 6;

/// A window's frames, one second.
#[derive(Debug, Clone, Default, Serialize)]
pub struct WindowFrames {
    pub id: u64,
    pub title: String,
    pub frames: u32,
    /// Frames a second: `frames` over the second's wall time (the display's rate at most).
    pub fps: f32,
    /// `Window::draw`: layout, prepaint and paint, in milliseconds.
    pub draw_p50_ms: f32,
    pub draw_p95_ms: f32,
    pub draw_max_ms: f32,
    /// From the window's first change to its frame reaching the screen.
    pub latency_p50_ms: f32,
    pub latency_p95_ms: f32,
    pub latency_max_ms: f32,
}

/// One piece of foreground work.
#[derive(Debug, Clone, Serialize)]
pub struct Work {
    /// `draw`, `present`, `task`, `input` or `action`.
    pub kind: &'static str,
    /// The source location of a task, an action's name, an input's kind, a window's title.
    pub what: String,
    pub ms: f32,
    /// Seconds since the app started, when it ended.
    pub at: f32,
}

/// The main thread, one second.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Frames {
    pub windows: Vec<WindowFrames>,
    /// The share of the second the main thread spent on GPUI's work, 100 all of it.
    pub busy: f32,
    /// The longest an input took to reach the screen (dispatched to presented).
    pub input_max_ms: Option<f32>,
    pub inputs: u32,
    pub slowest: Vec<Work>,
}

pub struct Reader {
    collector: ForegroundJournalCollector,
    sealer: IntervalSealer,
    started: Instant,
    /// Inputs that changed a window and are waiting for its frame.
    waiting: Vec<Instant>,
    hangs: VecDeque<Work>,
    first_present: Option<Instant>,
}

#[derive(Default)]
struct Window {
    draws: Vec<f32>,
    latencies: Vec<f32>,
}

impl Reader {
    pub fn new(journal: &ForegroundJournal, started: Instant) -> Self {
        Self {
            collector: journal.collector(),
            sealer: IntervalSealer::new(Instant::now()),
            started,
            waiting: Vec::new(),
            hangs: VecDeque::new(),
            first_present: None,
        }
    }

    /// When a window first reached the screen, if one has since this reader was made.
    pub fn first_present(&self) -> Option<Instant> {
        self.first_present
    }

    /// The hangs kept, the latest last.
    pub fn hangs(&self) -> impl Iterator<Item = &Work> {
        self.hangs.iter()
    }

    /// Reads what the journal took since the last read, without measuring it: the first present only.
    pub fn skim(&mut self) {
        for entry in self.collector.collect_unseen().entries {
            if let ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(presented)) = entry {
                self.first_present.get_or_insert(presented.presentation.present_end);
            }
        }
    }

    /// The main thread since the last read; `titles` names the windows.
    pub fn read(&mut self, wall: Duration, titles: &BTreeMap<u64, String>) -> Frames {
        let drained = self.collector.collect_unseen();
        let mut windows: BTreeMap<u64, Window> = BTreeMap::new();
        let mut inputs = Vec::new();
        for entry in &drained.entries {
            match entry {
                ForegroundJournalEntry::Event(ForegroundEvent::Input(input)) if input.caused_invalidation => {
                    self.waiting.push(input.start);
                }
                ForegroundJournalEntry::Event(ForegroundEvent::Draw(draw)) => {
                    let window = windows.entry(draw.window_id.as_u64()).or_default();
                    window.draws.push(ms(draw.draw_duration()));
                }
                ForegroundJournalEntry::Boundary(IntervalBoundary::Presented(presented)) => {
                    let end = presented.presentation.present_end;
                    self.first_present.get_or_insert(end);
                    let window = windows.entry(presented.frame.window_id.as_u64()).or_default();
                    if let Some(latency) = presented.dirty_to_present_duration() {
                        window.latencies.push(ms(latency));
                    }
                    self.waiting.retain(|start| match end.checked_duration_since(*start) {
                        Some(took) => {
                            inputs.push(ms(took));
                            false
                        }
                        None => true,
                    });
                }
                _ => {}
            }
        }
        let snapshots = self.sealer.push_entries(drained.entries);
        let busy: Duration = snapshots.iter().map(|s| s.occupancy()).sum();
        let events: Vec<&ForegroundEvent> = snapshots.iter().flat_map(|s| s.events.iter()).collect();
        let frames: Vec<(Instant, Instant, Duration)> = events
            .iter()
            .filter(|e| matches!(e, ForegroundEvent::Draw(_) | ForegroundEvent::Present(_)))
            .map(|e| (e.start_time(), e.end_time(), e.duration()))
            .collect();
        // A task that is mostly the frame it drew is the frame loop: the draw says more.
        let mostly_a_frame = |event: &ForegroundEvent| {
            let ForegroundEvent::TaskPoll(_) = event else { return false };
            let (start, end, took) = (event.start_time(), event.end_time(), event.duration());
            let inside: Duration = frames.iter().filter(|f| f.0 >= start && f.1 <= end).map(|f| f.2).sum();
            inside * 2 >= took
        };
        let mut work: Vec<Work> =
            events.iter().filter(|event| !mostly_a_frame(event)).filter_map(|event| self.work(event, titles)).collect();
        work.sort_by(|a, b| b.ms.total_cmp(&a.ms));
        for hang in work.iter().filter(|w| w.ms >= ms(HANG)) {
            if self.hangs.len() >= HANGS {
                self.hangs.pop_front();
            }
            self.hangs.push_back(hang.clone());
        }
        work.truncate(SLOWEST);
        Frames {
            windows: windows
                .into_iter()
                .map(|(id, w)| {
                    let frames = w.latencies.len().max(w.draws.len()) as u32;
                    (id, w, frames)
                })
                .map(|(id, w, frames)| WindowFrames {
                    id,
                    title: titles.get(&id).cloned().unwrap_or_else(|| format!("Window {id}")),
                    frames,
                    fps: frames as f32 / wall.as_secs_f32().max(0.001),
                    draw_p50_ms: percentile(&w.draws, 0.5),
                    draw_p95_ms: percentile(&w.draws, 0.95),
                    draw_max_ms: max(&w.draws),
                    latency_p50_ms: percentile(&w.latencies, 0.5),
                    latency_p95_ms: percentile(&w.latencies, 0.95),
                    latency_max_ms: max(&w.latencies),
                })
                .collect(),
            busy: (100.0 * busy.as_secs_f64() / wall.as_secs_f64().max(0.001)).min(100.0) as f32,
            input_max_ms: (!inputs.is_empty()).then(|| max(&inputs)),
            inputs: inputs.len() as u32,
            slowest: work,
        }
    }

    fn work(&self, event: &ForegroundEvent, titles: &BTreeMap<u64, String>) -> Option<Work> {
        let title =
            |id: gpui::WindowId| titles.get(&id.as_u64()).cloned().unwrap_or_else(|| format!("Window {}", id.as_u64()));
        let (kind, what) = match event {
            ForegroundEvent::TaskPoll(task) => ("task", format!("{}:{}", task.location.file(), task.location.line())),
            ForegroundEvent::Action(action) => ("action", action.name.to_string()),
            ForegroundEvent::Input(input) => ("input", input.kind.to_string()),
            ForegroundEvent::Draw(draw) => ("draw", title(draw.window_id)),
            ForegroundEvent::Present(present) => ("present", title(present.window_id)),
            ForegroundEvent::SmallPolls(_) => return None,
        };
        let end = event.end_time();
        Some(Work {
            kind,
            what,
            ms: ms(event.duration()),
            at: end.saturating_duration_since(self.started).as_secs_f32(),
        })
    }
}

fn ms(duration: Duration) -> f32 {
    (duration.as_secs_f64() * 1000.0) as f32
}

fn max(values: &[f32]) -> f32 {
    values.iter().copied().fold(0.0, f32::max)
}

/// The least value that a share `q` (0 to 1) of `values` are at or under; 0 for none.
pub fn percentile(values: &[f32], q: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() as f32 * q).ceil() as usize).clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_percentile_is_the_least_value_that_share_are_at_or_under() {
        let values: Vec<f32> = (1..=20).map(|n| n as f32).collect();
        assert_eq!(percentile(&values, 0.5), 10.0);
        assert_eq!(percentile(&values, 0.95), 19.0);
        assert_eq!(percentile(&[4.0], 0.95), 4.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
        assert_eq!(max(&[1.0, 7.0, 3.0]), 7.0);
    }
}
