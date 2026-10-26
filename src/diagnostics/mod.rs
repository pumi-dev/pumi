//! Diagnostics: what makes the app slow, measured while someone looks. Once a second, while the
//! Diagnostics window is open or `app.profile` measures, it reads the main thread's frames
//! (`frames`), every terminal's cost (`terminals`), the processes and threads (`processes`), and
//! what the app's JS says of itself (`report_js`), and keeps the last `HISTORY` seconds. Nobody
//! watching, it samples nothing; the terminals' counters are the only thing always kept.
pub mod frame_loop;
pub mod frames;
pub mod processes;
pub mod terminals;

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(not(target_os = "macos"))]
#[path = "other.rs"]
mod os;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often a sample is taken.
pub const SAMPLE: Duration = Duration::from_secs(1);
/// The seconds kept.
pub const HISTORY: usize = 120;
/// The JS not reporting for this long, while watched, is the JS being stuck.
pub const JS_SILENT: Duration = Duration::from_millis(2500);
/// The window's own name: its frames are left out of the verdict.
pub const TITLE: &str = "Diagnostics";
/// How long the startup's first frame is looked for.
const STARTUP_WAIT: Duration = Duration::from_secs(30);
const STARTUP_POLL: Duration = Duration::from_millis(250);
/// `app.profile`'s seconds when it is not told, and the most it takes.
const PROFILE_DEFAULT: u64 = 3;
const PROFILE_MAX: u64 = 60;
/// A frame's budget at 60 Hz, in milliseconds.
pub const FRAME_MS: f32 = 16.7;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One second of what the app's JS (the worker React runs in) measured of itself.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Js {
    /// The longest its event loop was late, and its lateness summed (ms per second): JS blocked.
    pub lag_max_ms: f32,
    pub blocked_ms_per_s: f32,
    pub heap_mb: f32,
    /// React's commits and the components they re-rendered, the most re-rendered first.
    pub commits_per_s: f32,
    pub renders_per_s: f32,
    pub hot: Vec<Named>,
    /// Mutation batches sent to the native tree, their mutations, and the ms sending them took.
    pub batches_per_s: f32,
    pub ops_per_s: f32,
    pub apply_ms_per_s: f32,
    /// The server's events by kind: how many and the ms handling them took.
    pub events: Vec<Named>,
    /// Calls to the server that came back: how many, their mean and worst round trip.
    pub calls_per_s: f32,
    pub call_mean_ms: f32,
    pub call_max_ms: f32,
    pub call_slowest: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Named {
    pub name: String,
    pub per_s: f32,
    pub ms_per_s: f32,
}

/// What a JS report is: a second, and the startup's marks once.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct JsReport {
    sample: Option<Js>,
    /// `[name, ms since the process started]`, as `trace.ts` marks them.
    startup: Option<Vec<(String, f32)>>,
}

/// The renders of a plugin's component or of a theme's part, one second (`compartment="owner/name"`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Compartment {
    pub name: String,
    pub renders: u32,
    pub deferred: u32,
    pub total_ms: f32,
    pub max_ms: f32,
    pub held: bool,
}

/// One second of the app.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Sample {
    /// Seconds since the app started.
    pub at: f32,
    pub frames: frames::Frames,
    /// How often JS pumped AppKit and whether the display link's wakes reached it.
    pub frame_loop: frame_loop::FrameLoop,
    pub terminals: Vec<terminals::Terminal>,
    pub processes: processes::Processes,
    pub compartments: Vec<Compartment>,
    pub js: Option<Js>,
    /// How long the JS has not reported, once that is longer than `JS_SILENT`.
    pub js_silent_ms: Option<f32>,
}

impl Sample {
    /// The app's own process.
    pub fn app(&self) -> Option<&processes::Process> {
        self.processes.list.iter().find(|p| p.role == "app")
    }
}

/// How the app started, in milliseconds since its process did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Startup {
    pub engine_ms: Option<f32>,
    pub first_frame_ms: Option<f32>,
    /// The JS's marks (`trace.ts`), in order.
    pub marks: Vec<(String, f32)>,
}

#[derive(Default)]
struct State {
    history: VecDeque<Arc<Sample>>,
    js: Option<(Instant, Js)>,
    startup: Startup,
    hangs: Vec<frames::Work>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    lock(STATE.get_or_init(Default::default))
}

static WATCHERS: AtomicUsize = AtomicUsize::new(0);
static SAMPLES: AtomicU64 = AtomicU64::new(0);
static WAKE: OnceLock<UnboundedSender<()>> = OnceLock::new();

/// Someone looking: while one lives, the app is sampled.
pub struct Watch(());

impl Drop for Watch {
    fn drop(&mut self) {
        WATCHERS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Starts sampling (if nobody was) until the `Watch` is dropped. Any thread.
pub fn watch() -> Watch {
    if WATCHERS.fetch_add(1, Ordering::AcqRel) == 0
        && let Some(wake) = WAKE.get()
    {
        let _ = wake.unbounded_send(());
    }
    Watch(())
}

pub fn watching() -> bool {
    WATCHERS.load(Ordering::Acquire) > 0
}

/// The seconds kept, oldest first.
pub fn history() -> Vec<Arc<Sample>> {
    state().history.iter().cloned().collect()
}

pub fn startup() -> Startup {
    state().startup.clone()
}

/// The main thread's hangs (work of `frames::HANG` or more) since sampling began, the latest last.
pub fn hangs() -> Vec<frames::Work> {
    state().hangs.clone()
}

/// The number of samples taken so far: a window redraws when it moves.
pub fn samples() -> u64 {
    SAMPLES.load(Ordering::Acquire)
}

/// A sample was taken: the Diagnostics window draws it.
pub struct Sampled;
impl gpui::Global for Sampled {}

/// What the app's JS measured (`{sample, startup}`), as it reports each second while watched.
pub fn report_js(report: Value) -> Result<(), String> {
    let report: JsReport = serde_json::from_value(report).map_err(|err| format!("a diagnostics report: {err}"))?;
    let mut state = state();
    if let Some(marks) = report.startup {
        state.startup.marks = marks;
    }
    if let Some(sample) = report.sample {
        state.js = Some((Instant::now(), sample));
    }
    Ok(())
}

/// When the process started, as an `Instant`: now, where the OS doesn't say.
fn process_started() -> Instant {
    let now = Instant::now();
    match os::process_started() {
        Some(started) => {
            let since = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().saturating_sub(started);
            now.checked_sub(since).unwrap_or(now)
        }
        None => now,
    }
}

fn ms_between(from: Instant, to: Instant) -> f32 {
    (to.saturating_duration_since(from).as_secs_f64() * 1000.0) as f32
}

/// Starts the sampler on the main thread; it waits until someone watches. `data_dir` is the
/// server's, where its lock file says its pid.
pub fn start(data_dir: std::path::PathBuf, cx: &mut gpui::App) {
    let started = process_started();
    state().startup.engine_ms = Some(ms_between(started, Instant::now()));
    let (wake, mut woken) = unbounded::<()>();
    if WAKE.set(wake).is_err() {
        return;
    }
    cx.set_global(Sampled);
    let journal = cx.foreground_journal();
    cx.spawn(async move |cx| {
        let mut frames = frames::Reader::new(&journal, started);
        let looking = Instant::now();
        while frames.first_present().is_none() && looking.elapsed() < STARTUP_WAIT && !watching() {
            cx.background_executor().timer(STARTUP_POLL).await;
            frames.skim();
        }
        if let Some(first) = frames.first_present() {
            state().startup.first_frame_ms = Some(ms_between(started, first));
        }
        let mut processes = Some(processes::Reader::new(data_dir));
        let mut pumps = frame_loop::Reader::new();
        let mut was = false;
        let mut at = Instant::now();
        loop {
            let now = watching();
            if now != was {
                was = now;
                crate::engine::emit("diagnostics", json!({ "watching": now }));
                if now {
                    frames.skim();
                    pumps.skim();
                    terminals::take();
                    if let Some(reader) = processes.as_mut() {
                        reader.read();
                    }
                    let mut state = state();
                    state.history.clear();
                    state.hangs.clear();
                    at = Instant::now();
                }
            }
            if !now {
                if woken.next().await.is_none() {
                    break;
                }
                continue;
            }
            cx.background_executor().timer(SAMPLE).await;
            if !watching() {
                continue;
            }
            let wall = at.elapsed();
            at = Instant::now();
            let (titles, compartments, refresh_hz) = cx.update(|cx| {
                let (titles, compartments) = windows(cx);
                (titles, compartments, frame_loop::refresh_hz())
            });
            let frames_second = frames.read(wall, &titles);
            let frame_loop_second = pumps.read(wall, refresh_hz, pumi_gpuix::app::wake_fallback());
            let terminals = terminals::take();
            let Some(mut reader) = processes.take() else { break };
            let (reader, read) = cx
                .background_executor()
                .spawn(async move {
                    let read = reader.read();
                    (reader, read)
                })
                .await;
            processes = Some(reader);
            let mut state = state();
            let js = state.js.clone();
            let silent = js.as_ref().map_or(Duration::MAX, |(heard, _)| heard.elapsed());
            let sample = Sample {
                at: at.saturating_duration_since(started).as_secs_f32(),
                frames: frames_second,
                frame_loop: frame_loop_second,
                terminals,
                processes: read,
                compartments,
                js: js.filter(|_| silent < JS_SILENT).map(|(_, js)| js),
                js_silent_ms: (silent >= JS_SILENT && silent != Duration::MAX).then(|| silent.as_secs_f32() * 1000.0),
            };
            state.hangs = frames.hangs().cloned().collect();
            if state.history.len() >= HISTORY {
                state.history.pop_front();
            }
            state.history.push_back(Arc::new(sample));
            drop(state);
            SAMPLES.fetch_add(1, Ordering::AcqRel);
            cx.update(|cx| cx.set_global(Sampled));
        }
    })
    .detach();
}

/// Each window's title, and what its compartments cost since the last read.
fn windows(cx: &mut gpui::App) -> (BTreeMap<u64, String>, Vec<Compartment>) {
    let mut titles = BTreeMap::new();
    let mut compartments: Vec<Compartment> = Vec::new();
    for handle in cx.windows() {
        let id = handle.window_id().as_u64();
        if let Ok(title) = handle.update(cx, |_, window, _| window.window_title()) {
            titles.insert(id, if title.is_empty() { format!("Window {id}") } else { title });
        }
        let Some(view) = handle.downcast::<pumi_gpuix::GpuixView>() else { continue };
        let Ok(view) = view.read(cx) else { continue };
        for stats in view.compartment_stats() {
            let name = stats.name.unwrap_or_else(|| "(unnamed)".into());
            let at = match compartments.iter().position(|c| c.name == name) {
                Some(at) => at,
                None => {
                    compartments.push(Compartment { name, ..Compartment::default() });
                    compartments.len() - 1
                }
            };
            let c = &mut compartments[at];
            c.renders += stats.renders;
            c.deferred += stats.deferred;
            c.total_ms += stats.total_ms as f32;
            c.max_ms = c.max_ms.max(stats.max_ms as f32);
            c.held |= stats.held;
        }
    }
    compartments.retain(|c| c.renders > 0 || c.held);
    compartments.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
    (titles, compartments)
}

/// What most likely makes the app slow over `samples`, worst first.
#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    /// `ok`, `warn` or `bad`.
    pub severity: &'static str,
    pub text: String,
    pub findings: Vec<String>,
}

fn grade(value: f32, warn: f32, bad: f32) -> Option<&'static str> {
    match value {
        v if v >= bad => Some("bad"),
        v if v >= warn => Some("warn"),
        _ => None,
    }
}

pub fn verdict(samples: &[Arc<Sample>]) -> Verdict {
    let mut found: Vec<(f32, &'static str, String)> = Vec::new();
    let mut add = |weight: f32, severity: Option<&'static str>, text: String| {
        if let Some(severity) = severity {
            found.push((weight, severity, text));
        }
    };
    let max = |f: &dyn Fn(&Sample) -> f32| samples.iter().map(|s| f(s)).fold(0.0, f32::max);
    let theirs = |w: &&frames::WindowFrames| w.title != TITLE;
    // Frames are judged by the display's: 8.3 ms at 120 Hz.
    let frame = samples.iter().map(|s| s.frame_loop.frame_ms()).reduce(f32::min).unwrap_or(FRAME_MS);
    if let Some((title, latency)) = samples
        .iter()
        .flat_map(|s| s.frames.windows.iter().filter(theirs))
        .map(|w| (w.title.clone(), w.latency_max_ms))
        .max_by(|a, b| a.1.total_cmp(&b.1))
    {
        add(
            latency,
            grade(latency, 2.0 * frame, 6.0 * frame),
            format!("A frame of {title} took {latency:.0} ms to reach the screen"),
        );
    }
    frame_loop_findings(samples, frame, &mut add);
    if let Some(work) = samples.iter().flat_map(|s| s.frames.slowest.first()).max_by(|a, b| a.ms.total_cmp(&b.ms)) {
        let text = format!("The main thread was blocked {:.0} ms by a {} ({})", work.ms, work.kind, work.what);
        add(work.ms, grade(work.ms, frames::HANG.as_secs_f32() * 1000.0, 200.0), text);
    }
    let busy = max(&|s| s.frames.busy);
    add(busy, grade(busy, 60.0, 90.0), format!("The main thread was busy {busy:.0}% of a second"));
    let input = max(&|s| s.frames.input_max_ms.unwrap_or(0.0));
    add(input, grade(input, 3.0 * frame, 100.0), format!("Input took {input:.0} ms to show"));
    if let Some(silent) = samples.iter().filter_map(|s| s.js_silent_ms).reduce(f32::max) {
        add(silent, Some("bad"), format!("The app's JS did not answer for {:.1} s", silent / 1000.0));
    }
    let lag = max(&|s| s.js.as_ref().map_or(0.0, |js| js.lag_max_ms));
    add(lag, grade(lag, 50.0, 250.0), format!("The app's JS was blocked {lag:.0} ms"));
    let mut by_tab: BTreeMap<&str, (f32, f32, f32, f32)> = BTreeMap::new();
    for t in samples.iter().flat_map(|s| s.terminals.iter()) {
        let entry = by_tab.entry(&t.tab).or_default();
        if t.cost_ms() > entry.0 {
            *entry = (t.cost_ms(), t.parse_ms, t.screen_ms, entry.3);
        }
        entry.3 = entry.3.max(t.echo_ms.unwrap_or(0.0));
    }
    for (tab, (cost, parse, screen, echo)) in by_tab {
        let paint = cost - parse - screen;
        let text = format!(
            "Terminal {tab} cost {cost:.0} ms in a second (parse {parse:.0}, screen {screen:.0}, layout {paint:.0})"
        );
        add(cost / 4.0, grade(cost, 100.0, 300.0), text);
        add(echo, grade(echo, 50.0, 150.0), format!("Typing in {tab} echoed after {echo:.0} ms"));
    }
    let cpu = max(&|s| s.app().and_then(|p| p.cpu).unwrap_or(0.0));
    add(cpu / 4.0, grade(cpu, 80.0, 150.0), format!("The app used {cpu:.0}% CPU"));
    let gpu = max(&|s| {
        s.processes.list.iter().filter(|p| p.role == "app" || p.role == "compositor").filter_map(|p| p.gpu).sum()
    });
    add(gpu / 2.0, grade(gpu, 40.0, 70.0), format!("The app and WindowServer used {gpu:.0}% of the GPU"));
    for c in samples.iter().flat_map(|s| s.compartments.iter()).filter(|c| c.held) {
        add(c.max_ms / 2.0, Some("warn"), format!("{} draws slowly ({:.0} ms), held to its budget", c.name, c.max_ms));
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.dedup_by(|a, b| a.2 == b.2);
    let severity = if found.iter().any(|f| f.1 == "bad") {
        "bad"
    } else if found.is_empty() {
        "ok"
    } else {
        "warn"
    };
    let text = found.first().map_or_else(|| "Nothing measured lags".to_string(), |f| f.2.clone());
    Verdict { severity, text, findings: found.into_iter().map(|f| f.2).collect() }
}

/// What the frame loop says of `samples` (`frame` is the display's, in ms): whether JS pumped
/// AppKit once a frame while a window showed, whether the display link's wakes reached it, and
/// whether the windows drew as often as their terminals changed.
fn frame_loop_findings(samples: &[Arc<Sample>], frame: f32, add: &mut impl FnMut(f32, Option<&'static str>, String)) {
    // Seconds the display link ran (a window showed): they should have a pump a frame.
    let showing: Vec<&Sample> = samples
        .iter()
        .map(|s| s.as_ref())
        .filter(|s| s.frame_loop.refresh_hz > 0.0 && s.frame_loop.display_per_s >= 0.5 * s.frame_loop.refresh_hz)
        .collect();
    if let Some(worst) = showing.iter().map(|s| &s.frame_loop).min_by(|a, b| a.pumps_per_s.total_cmp(&b.pumps_per_s)) {
        let share = worst.pumps_per_s / worst.refresh_hz;
        let severity = if share < 0.5 {
            Some("bad")
        } else if share < 0.85 {
            Some("warn")
        } else {
            None
        };
        add(
            (1.0 - share) * 1000.0,
            severity,
            format!(
                "JS pumped AppKit {:.0} times a second for a {:.0} Hz display, so no window could draw more often: \
                 the display link asked {:.0} wakes, {:.0} were sent, {:.0} arrived ({:.0} ms late at worst), \
                 and {:.0} pumps were the {:.0} ms fallback timer's",
                worst.pumps_per_s,
                worst.refresh_hz,
                worst.display_per_s,
                worst.sent_per_s,
                worst.arrived_per_s,
                worst.late_max_ms,
                worst.fallback_per_s,
                worst.fallback_ms,
            ),
        );
    }
    let gap = showing.iter().map(|s| s.frame_loop.gap_max_ms).fold(0.0, f32::max);
    add(gap, grade(gap, 3.0 * frame, 50.0), format!("The main thread went {gap:.0} ms without being pumped"));
    let late = samples.iter().map(|s| s.frame_loop.late_max_ms).fold(0.0, f32::max);
    add(late, grade(late, 2.0 * frame, 50.0), format!("A frame wake took {late:.0} ms to reach JS"));
    let pending = samples.iter().map(|s| s.frame_loop.pending_ms).fold(0.0, f32::max);
    let stale = pumi_gpuix::frame_loop::STALE.as_secs_f32() * 1000.0;
    add(
        pending,
        grade(pending, 2.0 * stale, 100.0),
        format!("A frame wake has waited {pending:.0} ms and not reached JS"),
    );
    let sum = |f: fn(&frame_loop::FrameLoop) -> f32| samples.iter().map(|s| f(&s.frame_loop)).sum::<f32>();
    let (sent, resent, failed) = (sum(|l| l.sent_per_s), sum(|l| l.resent_per_s), sum(|l| l.failed_per_s));
    if sent > 0.0 && resent + failed > 0.0 {
        let share = (resent + failed) / sent;
        add(
            share * 200.0,
            grade(share, 0.02, 0.2),
            format!(
                "{:.0} of {sent:.0} frame wakes never reached JS ({failed:.0} could not be queued)",
                resent + failed
            ),
        );
    }
    // The window that drew least often for what its terminals changed and the display could show.
    let starved = showing
        .iter()
        .filter_map(|s| {
            let hz = s.frame_loop.refresh_hz;
            let busiest = s.terminals.iter().max_by_key(|t| t.records)?;
            let wanted = (busiest.records as f32).min(hz);
            (wanted >= 0.25 * hz).then_some(())?;
            let w = s.frames.windows.iter().filter(|w| w.title != TITLE).max_by(|a, b| a.fps.total_cmp(&b.fps))?;
            Some((w.fps / wanted, w, busiest, hz))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((share, w, busiest, hz)) = starved
        && share < 0.5
    {
        add(
            (1.0 - share) * 200.0,
            Some(if share < 0.25 { "bad" } else { "warn" }),
            format!(
                "{} drew {:.0} frames a second while terminal {} changed {} times ({hz:.0} Hz display)",
                w.title, w.fps, busiest.tab, busiest.records
            ),
        );
    }
}

/// `samples` as `app.profile` answers them: the verdict, the hangs, the startup and each second.
pub fn report(samples: &[Arc<Sample>]) -> Value {
    json!({
        "seconds": samples.len(),
        "verdict": verdict(samples),
        "startup": startup(),
        "hangs": hangs(),
        "samples": samples.iter().map(|s| s.as_ref()).collect::<Vec<_>>(),
    })
}

pub const FUNCTION: &str = "app.profile";

pub fn info() -> Value {
    json!({
        "name": FUNCTION,
        "since": 1,
        "provider": "app",
        "runs_on": "client",
        "effect": "read",
        "desc": "Measures the app for `seconds` (3 by default, at most 60) and says what makes it slow: \
                 `verdict` (`severity` ok, warn or bad; `text`; every `findings`), the main thread's \
                 `hangs`, how it started (`startup`), and each second in `samples`. A second has each \
                 window's frames (`frames.windows`: frames a second, draw and first-change-to-screen \
                 ms, judged by a frame at the display's rate), the frame loop (`frame_loop`: the \
                 display's Hz, how often JS pumped AppKit and what woke it, the display link's wakes \
                 asked, sent, lost and arrived and how late, the longest without a pump), how busy the \
                 main thread was and its slowest work with where it came from (`frames.slowest`), \
                 every terminal's cost (parse, screen and layout ms, bytes, key-to-echo ms, how long it \
                 took to show), the app's JS (event loop lag, React's commits, server events and \
                 calls), compartments (a plugin's component, a theme's part), and the processes (the app, \
                 pumid, pumi-host, WindowServer: CPU, GPU, memory) and the app's threads. Start it, \
                 then do what is slow: what runs meanwhile is measured. Window ▸ Diagnostics shows \
                 the same numbers live.",
        "parameters": [["Integer", "seconds"]],
        "params_schema": {
            "type": "object",
            "properties": { "seconds": { "type": "integer", "minimum": 1, "maximum": PROFILE_MAX } },
            "additionalProperties": false
        },
        "returns": "Object",
        "returns_schema": {
            "type": "object",
            "properties": {
                "seconds": { "type": "integer" },
                "verdict": { "type": "object" },
                "startup": { "type": "object" },
                "hangs": { "type": "array" },
                "samples": { "type": "array" }
            }
        },
        "examples": [{ "args": [5], "desc": "Five seconds" }],
        "notes": ["Durations are milliseconds; CPU is 100 a core, GPU 100 the whole GPU."],
        "source": "app"
    })
}

fn seconds_of(args: &Value) -> Result<u64, pumi_api::Error> {
    let given = match args {
        Value::Array(list) => list.first().cloned().unwrap_or(Value::Null),
        other => other.clone(),
    };
    let given = match given {
        Value::Object(map) => map.get("seconds").cloned().unwrap_or(Value::Null),
        other => other,
    };
    match given {
        Value::Null => Ok(PROFILE_DEFAULT),
        Value::Number(n) => match n.as_u64() {
            Some(n @ 1..=PROFILE_MAX) => Ok(n),
            _ => Err(pumi_api::Error::invalid_params(format!("{FUNCTION}: `seconds` is 1 to {PROFILE_MAX}"))),
        },
        _ => Err(pumi_api::Error::invalid_params(format!("{FUNCTION} takes `seconds`"))),
    }
}

/// Whether `name` is this module's: it answers after measuring, so off the thread that asked.
pub fn takes(name: &str) -> bool {
    name == FUNCTION
}

/// `app.profile`: blocks for the seconds it measures.
pub fn answer(name: &str, args: &Value) -> Option<Result<Value, pumi_api::Error>> {
    if name != FUNCTION {
        return None;
    }
    let seconds = match seconds_of(args) {
        Ok(seconds) => seconds,
        Err(err) => return Some(Err(err)),
    };
    let _watch = watch();
    let from = samples();
    let deadline = Instant::now() + Duration::from_secs(seconds) + SAMPLE * 3;
    while samples() < from + seconds && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let taken = (samples() - from).min(seconds) as usize;
    let history = history();
    let samples = &history[history.len().saturating_sub(taken)..];
    Some(Ok(report(samples)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn second(edit: impl FnOnce(&mut Sample)) -> Arc<Sample> {
        let mut sample = Sample::default();
        edit(&mut sample);
        Arc::new(sample)
    }

    #[test]
    fn a_quiet_app_has_nothing_to_say() {
        let verdict = verdict(&[second(|_| {})]);
        assert_eq!(verdict.severity, "ok");
        assert!(verdict.findings.is_empty());
    }

    #[test]
    fn the_worst_finding_leads() {
        let samples = [
            second(|s| s.frames.busy = 70.0),
            second(|s| {
                s.frames.windows.push(frames::WindowFrames {
                    title: "pumi".into(),
                    latency_max_ms: 180.0,
                    ..Default::default()
                });
                s.js_silent_ms = Some(3000.0);
            }),
        ];
        let verdict = verdict(&samples);
        assert_eq!(verdict.severity, "bad");
        assert!(verdict.text.contains("JS did not answer"), "{}", verdict.text);
        assert!(verdict.findings.iter().any(|f| f.contains("pumi took 180 ms")), "{:?}", verdict.findings);
        assert!(verdict.findings.iter().any(|f| f.contains("busy 70%")));
    }

    #[test]
    fn the_diagnostics_window_does_not_judge_itself() {
        let samples = [second(|s| {
            s.frames.windows.push(frames::WindowFrames {
                title: TITLE.into(),
                latency_max_ms: 500.0,
                ..Default::default()
            })
        })];
        assert_eq!(verdict(&samples).severity, "ok");
    }

    #[test]
    fn a_profile_takes_one_to_sixty_seconds() {
        assert_eq!(seconds_of(&json!([])).ok(), Some(PROFILE_DEFAULT));
        assert_eq!(seconds_of(&json!([5])).ok(), Some(5));
        assert_eq!(seconds_of(&json!({ "seconds": 2 })).ok(), Some(2));
        assert!(seconds_of(&json!([0])).is_err());
        assert!(seconds_of(&json!([61])).is_err());
        assert!(seconds_of(&json!(["soon"])).is_err());
    }

    #[test]
    fn a_js_report_keeps_its_second_and_its_marks() {
        report_js(
            json!({ "sample": { "lagMaxMs": 12.5, "hot": [{ "name": "Tab", "perS": 3 }] }, "startup": [["bun", 4.0]] }),
        )
        .unwrap();
        let state = state();
        assert_eq!(state.js.as_ref().map(|(_, js)| js.lag_max_ms), Some(12.5));
        assert_eq!(state.startup.marks.first().map(|m| m.0.as_str()), Some("bun"));
        drop(state);
        assert!(report_js(json!("not a report")).is_err());
    }

    fn at_120_hz(edit: impl FnOnce(&mut Sample)) -> Arc<Sample> {
        second(|s| {
            s.frame_loop.refresh_hz = 120.0;
            s.frame_loop.display_per_s = 120.0;
            s.frame_loop.pumps_per_s = 120.0;
            s.frame_loop.woken_per_s = 120.0;
            s.frame_loop.sent_per_s = 120.0;
            s.frame_loop.arrived_per_s = 120.0;
            s.frame_loop.gap_max_ms = 9.0;
            edit(s);
        })
    }

    #[test]
    fn a_frame_loop_at_the_display_rate_has_nothing_to_say() {
        let verdict = verdict(&[at_120_hz(|_| {})]);
        assert_eq!(verdict.severity, "ok", "{:?}", verdict.findings);
    }

    #[test]
    fn wakes_that_do_not_arrive_are_the_frame_rate() {
        let samples = [at_120_hz(|s| {
            let l = &mut s.frame_loop;
            (l.pumps_per_s, l.woken_per_s, l.fallback_per_s, l.fallback_ms) = (12.0, 2.0, 10.0, 100.0);
            (l.sent_per_s, l.arrived_per_s, l.resent_per_s) = (40.0, 2.0, 38.0);
            (l.gap_max_ms, l.pending_ms) = (110.0, 400.0);
            s.frames.windows.push(frames::WindowFrames {
                title: "pumi".into(),
                frames: 12,
                fps: 12.0,
                ..Default::default()
            });
            s.terminals.push(terminals::Terminal { tab: "%3".into(), records: 150, ..Default::default() });
        })];
        let verdict = verdict(&samples);
        assert_eq!(verdict.severity, "bad");
        assert!(verdict.text.contains("pumped AppKit 12 times a second for a 120 Hz display"), "{}", verdict.text);
        let has = |needle: &str| verdict.findings.iter().any(|f| f.contains(needle));
        assert!(has("went 110 ms without being pumped"), "{:?}", verdict.findings);
        assert!(has("has waited 400 ms"), "{:?}", verdict.findings);
        assert!(has("38 of 40 frame wakes never reached JS"), "{:?}", verdict.findings);
        assert!(has("pumi drew 12 frames a second while terminal %3 changed 150 times"), "{:?}", verdict.findings);
    }

    #[test]
    fn a_hidden_window_is_not_held_to_the_display_rate() {
        let samples = [second(|s| {
            let l = &mut s.frame_loop;
            (l.refresh_hz, l.display_per_s, l.pumps_per_s, l.fallback_per_s, l.gap_max_ms) =
                (120.0, 0.0, 10.0, 10.0, 100.0);
        })];
        assert_eq!(verdict(&samples).severity, "ok");
    }

    #[test]
    fn frames_are_judged_by_the_display_rate() {
        let samples = [at_120_hz(|s| {
            s.frames.windows.push(frames::WindowFrames {
                title: "pumi".into(),
                latency_max_ms: 30.0,
                ..Default::default()
            })
        })];
        assert_eq!(verdict(&samples).severity, "warn", "30 ms is over two frames at 120 Hz");
    }

    #[test]
    fn the_function_info_is_one_the_registry_takes() {
        let info: pumi_api::FunctionInfo = serde_json::from_value(info()).expect("the registry reads this");
        assert_eq!(info.name, FUNCTION);
        assert_eq!((info.runs_on, info.effect), (pumi_api::RunsOn::Client, pumi_api::Effect::Read));
        assert!(info.validate().is_ok());
    }
}
