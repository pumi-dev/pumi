//! Diagnostics: the app's own window, what makes the app slow, live. A page each for the main
//! thread's frames, the terminals, the app's JS and the processes, drawn natively from
//! `crate::diagnostics` so it keeps drawing when the JS is what is stuck. While it is open the
//! app is sampled once a second; closed, nothing is.
use crate::diagnostics::{self as diag, FRAME_MS, HISTORY, Sample, TITLE};
use crate::theme;
use crate::ui::kit::{Button, ListItem};
use crate::ui::widgets::{self, StyledExt};
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, FocusHandle, Focusable, FontFeatures, FontWeight, Hsla,
    PathBuilder, Pixels, Render, Rgba, ScrollHandle, SharedString, Subscription, TitlebarOptions, Window, WindowBounds,
    WindowOptions, canvas, div, fill, point, prelude::*, px, size,
};
use std::sync::Arc;

const WINDOW_WIDTH: f32 = 980.0;
const WINDOW_HEIGHT: f32 = 720.0;
const TITLEBAR: f32 = 28.0;
const NAV_WIDTH: f32 = 220.0;
const CHART_HEIGHT: f32 = 180.0;
/// Room right of a chart for its scale.
const SCALE_WIDTH: f32 = 44.0;
/// The least a number's column is.
const COLUMN: f32 = 52.0;
/// The seconds the verdict and a copied report cover.
const RECENT: usize = 10;
/// The hangs a page lists.
const HANGS_SHOWN: usize = 12;
/// A hang this long is marked as too much (the verdict's bound).
const HANG_BAD: f32 = 200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Frames,
    Terminals,
    App,
    Processes,
}

impl Page {
    const ALL: [Page; 4] = [Page::Frames, Page::Terminals, Page::App, Page::Processes];
    /// The page a name (`frames`, `processes`) asks for.
    pub fn named(name: &str) -> Option<Page> {
        Some(match name.to_ascii_lowercase().as_str() {
            "frames" => Page::Frames,
            "terminals" => Page::Terminals,
            "app" | "js" => Page::App,
            "processes" | "gpu" | "cpu" => Page::Processes,
            _ => return None,
        })
    }
    fn title(self) -> &'static str {
        match self {
            Page::Frames => "Frames",
            Page::Terminals => "Terminals",
            Page::App => "App",
            Page::Processes => "Processes",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Page::Frames => "zap",
            Page::Terminals => "terminal",
            Page::App => "code",
            Page::Processes => "blocks",
        }
    }
}

thread_local! {
    static OPEN: std::cell::RefCell<Option<gpui::WindowHandle<DiagnosticsWindow>>> = const { std::cell::RefCell::new(None) };
}

/// Opens Diagnostics (or brings it to the front), at `page`.
pub fn open(page: Option<Page>, cx: &mut App) {
    if let Some(handle) = OPEN.with(|open| *open.borrow())
        && handle
            .update(cx, |view, window, cx| {
                window.activate_window();
                if let Some(page) = page {
                    view.goto(page, cx);
                }
            })
            .is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some(TITLE.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(12.), px(12.))),
        }),
        window_min_size: Some(size(px(640.), px(420.))),
        app_id: Some("dev.pumi.app".into()),
        ..Default::default()
    };
    let opened = cx.open_window(options, |window, cx| {
        let view = cx.new(|cx| DiagnosticsWindow::new(page.unwrap_or(Page::Frames), cx));
        window.focus(&view.focus_handle(cx), cx);
        view
    });
    match opened {
        Ok(handle) => {
            cx.activate(true);
            OPEN.with(|open| *open.borrow_mut() = Some(handle));
        }
        Err(err) => eprintln!("pumi: cannot open the diagnostics window: {err}"),
    }
}

pub struct DiagnosticsWindow {
    page: Page,
    focus: FocusHandle,
    scroll: ScrollHandle,
    _watch: diag::Watch,
    _sampled: Subscription,
}

impl Focusable for DiagnosticsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DiagnosticsWindow {
    fn new(page: Page, cx: &mut Context<Self>) -> Self {
        Self {
            page,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            _watch: diag::watch(),
            _sampled: cx.observe_global::<diag::Sampled>(|_, cx| cx.notify()),
        }
    }

    fn goto(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.page = page;
            self.scroll.set_offset(point(px(0.), px(0.)));
            cx.notify();
        }
    }

    fn nav(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .vcol()
            .flex_none()
            .w(px(NAV_WIDTH))
            .h_full()
            .border_r_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .child(div().flex_none().h(px(TITLEBAR + 12.)))
            .child(div().vcol().gap(px(1.)).px(px(10.)).children(Page::ALL.into_iter().map(|page| {
                let selected = page == self.page;
                let color = if selected { theme::surface_text() } else { theme::muted() };
                ListItem::new(SharedString::from(format!("diagnostics-nav-{}", page.title())))
                    .start(widgets::icon_file(widgets::named_icon_path(page.icon()), widgets::ICON, color))
                    .label(page.title())
                    .selected(selected)
                    .tab_stop(true)
                    .on_click(cx.listener(move |this, _, _, cx| this.goto(page, cx)))
            })))
            .into_any_element()
    }

    fn header(&self, history: &[Arc<Sample>]) -> AnyElement {
        let copy = Button::new("diagnostics-copy")
            .label("Copy report")
            .icon("icons/copy.svg")
            .disabled(history.is_empty())
            .tooltip("The last ten seconds as `app.profile` answers them, for an agent or an issue")
            .on_click(|_, _, cx| {
                let history = diag::history();
                let recent = &history[history.len().saturating_sub(RECENT)..];
                if let Ok(text) = serde_json::to_string_pretty(&diag::report(recent)) {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            });
        div()
            .hrow()
            .justify_between()
            .flex_none()
            .h(px(64.))
            .pt(px(8.))
            .px(px(28.))
            .child(
                div()
                    .vcol()
                    .child(
                        div()
                            .text(15., theme::surface_text())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.page.title()),
                    )
                    .child(div().text(12., theme::muted()).child(TITLE)),
            )
            .child(
                div()
                    .hrow()
                    .gap(px(12.))
                    .child(div().text(12., theme::placeholder()).child(match history.len() {
                        0 => "Measuring…".to_string(),
                        _ => "Each second".to_string(),
                    }))
                    .child(copy),
            )
            .into_any_element()
    }

    fn body(&self, history: &[Arc<Sample>]) -> Vec<AnyElement> {
        let Some(last) = history.last() else {
            return vec![note("The first second is being measured.")];
        };
        match self.page {
            Page::Frames => frames_page(history, last),
            Page::Terminals => terminals_page(history, last),
            Page::App => app_page(history, last),
            Page::Processes => processes_page(history, last),
        }
    }
}

impl Render for DiagnosticsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let history = diag::history();
        let body = self.body(&history);
        div()
            .id("diagnostics-window")
            .flex()
            .size_full()
            .bg(theme::editor())
            .text_color(theme::text())
            .text_sm()
            .track_focus(&self.focus)
            .child(self.nav(cx))
            .child(
                div().vcol().flex_1().min_w_0().h_full().child(self.header(&history)).child(
                    div()
                        .id(SharedString::from(format!("diagnostics-page-{}", self.page.title())))
                        .track_scroll(&self.scroll)
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(px(28.))
                        .pb(px(28.))
                        .children(body),
                ),
            )
    }
}

// --- Pages.

fn frames_page(history: &[Arc<Sample>], last: &Sample) -> Vec<AnyElement> {
    let mut out = vec![verdict(history)];
    // Judged by the display's frame: 8.3 ms at 120 Hz.
    let budget = last.frame_loop.frame_ms();
    let frame = |ms: f32| Cell::ms(ms).judged(budget, 3.0 * budget);
    let rows = last
        .frames
        .windows
        .iter()
        .map(|w| {
            vec![
                Cell::text(w.title.clone()),
                Cell::num(w.fps, ""),
                frame(w.draw_p50_ms),
                frame(w.draw_p95_ms),
                frame(w.draw_max_ms),
                frame(w.latency_p50_ms),
                frame(w.latency_p95_ms),
                frame(w.latency_max_ms),
            ]
        })
        .collect::<Vec<_>>();
    let headers = ["", "Frames a second", "Draw p50", "Draw p95", "Draw max", "Screen p50", "Screen p95", "Screen max"];
    out.push(table_section("Windows, last second", &headers, rows, "No window drew this second."));
    let main = |s: &Sample| s.processes.threads.iter().find(|t| t.name == "main").map_or(0.0, |t| t.cpu);
    let rows = vec![
        judged(series_row("Busy", history, |s| s.frames.busy, "%"), 50.0, 80.0),
        judged(series_row("Main thread CPU", history, main, "%"), 50.0, 90.0),
        judged(series_row("Input to screen", history, |s| s.frames.input_max_ms.unwrap_or(0.0), " ms"), 50.0, 100.0),
    ];
    out.push(table_section("Main thread", &STATS, rows, ""));
    out.push(frame_loop_section(history, last));
    let theirs =
        |s: &Sample| s.frames.windows.iter().filter(|w| w.title != TITLE).map(|w| w.latency_max_ms).fold(0.0, f32::max);
    let draw =
        |s: &Sample| s.frames.windows.iter().filter(|w| w.title != TITLE).map(|w| w.draw_max_ms).fold(0.0, f32::max);
    out.push(section(
        "Frame times",
        chart(
            vec![
                Series::new("Slowest frame to screen", theme::accent(), history, theirs),
                Series::new("Slowest draw", theme::palette::green(), history, draw),
            ],
            "ms",
            Some(budget),
        ),
    ));
    let hangs = diag::hangs();
    let now = last.at;
    let rows = hangs
        .iter()
        .rev()
        .take(HANGS_SHOWN)
        .map(|w| {
            vec![
                Cell::text(format!("{} · {}", w.kind, w.what)),
                Cell::num((now - w.at).max(0.0), " s ago"),
                Cell::ms(w.ms).judged(0.0, HANG_BAD),
            ]
        })
        .collect();
    let title = format!("Hangs (work of {} ms or more)", diag::frames::HANG.as_millis());
    out.push(table_section(&title, &["", "When", "Took"], rows, "None since this window opened."));
    let rows = last
        .frames
        .slowest
        .iter()
        .map(|w| vec![Cell::text(format!("{} · {}", w.kind, w.what)), Cell::ms(w.ms).judged(FRAME_MS, HANG_BAD)])
        .collect();
    out.push(table_section("Slowest work, last second", &["", "Took"], rows, "Nothing took a tenth of a millisecond."));
    out.push(note(
        "A frame's time to screen runs from the first change that needs it to its submission to the \
         compositor, judged by one frame at the display's rate (8.3 ms at 120 Hz). A window draws only \
         when JS pumps AppKit, so it cannot draw more often than the frame loop's pumps; each the \
         fallback timer started is a display link wake that did not arrive. Draw is GPUI's layout, prepaint and paint of the \
         window. Busy is the share of each second the main thread spent on GPUI's tasks, input, \
         draws and presents. The main thread also runs the bridge that applies the JS's mutations, \
         which Busy leaves out and its CPU counts.",
    ));
    out
}

/// How often JS pumped AppKit, what woke it, and whether the display link's wakes arrived: no
/// window draws more often than the pumps come.
fn frame_loop_section(history: &[Arc<Sample>], last: &Sample) -> AnyElement {
    let frame_ms = last.frame_loop.frame_ms();
    let hz = last.frame_loop.refresh_hz;
    let l = |f: fn(&diag::frame_loop::FrameLoop) -> f32| move |s: &Sample| f(&s.frame_loop);
    let rows = vec![
        series_row("Pumps a second", history, l(|l| l.pumps_per_s), ""),
        series_row("Started by a wake", history, l(|l| l.woken_per_s), ""),
        judged(series_row("Started by the fallback timer", history, l(|l| l.fallback_per_s), ""), 5.0, 30.0),
        series_row("Display link wakes asked", history, l(|l| l.display_per_s), ""),
        series_row("Wakes sent", history, l(|l| l.sent_per_s), ""),
        series_row("Wakes arrived", history, l(|l| l.arrived_per_s), ""),
        judged(series_row("Wakes lost, sent again", history, l(|l| l.resent_per_s), ""), 1.0, 10.0),
        judged(series_row("Wake late, at most", history, l(|l| l.late_max_ms), " ms"), 2.0 * frame_ms, 50.0),
        judged(series_row("Longest without a pump", history, l(|l| l.gap_max_ms), " ms"), 3.0 * frame_ms, 50.0),
    ];
    let title = if hz > 0.0 { format!("Frame loop ({hz:.0} Hz display)") } else { "Frame loop".to_string() };
    table_section(&title, &STATS, rows, "")
}

fn terminals_page(history: &[Arc<Sample>], last: &Sample) -> Vec<AnyElement> {
    let cost = |tab: &str| {
        let costs: Vec<f32> =
            history.iter().flat_map(|s| s.terminals.iter().filter(|t| t.tab == tab)).map(|t| t.cost_ms()).collect();
        Stats::of(&costs)
    };
    let rows = last
        .terminals
        .iter()
        .map(|t| {
            vec![
                Cell::text(format!("{}{}", t.tab, if t.observing { " (viewing)" } else { "" })),
                Cell::text(format!("{}×{}", t.cols, t.rows)),
                Cell::num(t.bytes as f32 / 1024.0, " KB"),
                Cell::ms(t.parse_ms),
                Cell::ms(t.screen_ms),
                Cell::ms(t.paint_ms),
                Cell::num(t.shaped as f32, ""),
                t.echo_ms.map_or(Cell::text("–"), |ms| Cell::ms(ms).judged(30.0, 100.0)),
                Cell::ms(cost(&t.tab).p95).judged(50.0, 200.0),
                Cell::ms(cost(&t.tab).max).judged(50.0, 200.0),
            ]
        })
        .collect();
    let headers = ["", "Size", "Output", "Parse", "Screen", "Layout", "Shaped", "Echo", "Cost p95", "Cost max"];
    let mut out = vec![table_section("Last second", &headers, rows, "No terminal is open.")];
    let mut busiest: Vec<(String, f32)> = last.terminals.iter().map(|t| (t.tab.clone(), cost(&t.tab).max)).collect();
    busiest.sort_by(|a, b| b.1.total_cmp(&a.1));
    let colours = [theme::accent(), theme::palette::green(), theme::warning()];
    let mut series =
        vec![Series::new("All terminals", theme::muted(), history, |s| s.terminals.iter().map(|t| t.cost_ms()).sum())];
    for ((tab, _), colour) in busiest.iter().take(colours.len()).zip(colours) {
        let tab = tab.clone();
        series.push(Series::new(&tab.clone(), colour, history, move |s| {
            s.terminals.iter().find(|t| t.tab == tab).map_or(0.0, |t| t.cost_ms())
        }));
    }
    out.push(section("Cost (ms in each second)", chart(series, "ms", None)));
    let rows = last
        .terminals
        .iter()
        .map(|t| {
            let step = |ms: Option<f32>| ms.map_or(Cell::text("–"), |ms| Cell::ms(ms).judged(100.0, 500.0));
            vec![
                Cell::text(t.tab.clone()),
                step(t.startup.size_ms),
                step(t.startup.attach_ms),
                step(t.startup.output_ms),
                step(t.startup.shown_ms),
                Cell::num(t.startup.snapshot_bytes as f32 / 1024.0, " KB"),
                Cell::num(t.startup.attaches as f32, ""),
            ]
        })
        .collect();
    let headers = ["", "Sized", "Attached", "Output", "Shown", "Replayed", "Attaches"];
    out.push(table_section("How each one started", &headers, rows, ""));
    out.push(note(
        "Parse is the terminal's emulator reading the program's output; Screen is reading its cells \
         out for a frame; Layout is shaping the rows that changed (Shaped counts them). Echo is a key \
         to the program's next output: the round trip through pumid and the program. Starting: \
         Sized is opened to the window's layout giving the tab a size (it cannot attach before), \
         Attached is the server taking it, Output its first bytes, Shown opened to its first drawn \
         screen. Replayed is the screen and scrollback the server sent on attaching.",
    ));
    out
}

fn app_page(history: &[Arc<Sample>], last: &Sample) -> Vec<AnyElement> {
    let mut out = Vec::new();
    if let Some(silent) = last.js_silent_ms {
        out.push(banner(
            "bad",
            &format!("The app's JS has not reported for {:.1} s: it is stuck, or still starting.", silent / 1000.0),
        ));
    }
    let js = |f: fn(&diag::Js) -> f32| move |s: &Sample| s.js.as_ref().map_or(0.0, f);
    let rows = vec![
        judged(series_row("Event loop late, at most", history, js(|j| j.lag_max_ms), " ms"), FRAME_MS, 50.0),
        judged(series_row("Blocked", history, js(|j| j.blocked_ms_per_s), " ms/s"), 50.0, 200.0),
        series_row("React commits", history, js(|j| j.commits_per_s), "/s"),
        series_row("Components rendered", history, js(|j| j.renders_per_s), "/s"),
        series_row("Mutations", history, js(|j| j.ops_per_s), "/s"),
        series_row("Sending them", history, js(|j| j.apply_ms_per_s), " ms/s"),
        series_row("Server calls", history, js(|j| j.calls_per_s), "/s"),
        series_row("Call round trip, mean", history, js(|j| j.call_mean_ms), " ms"),
        judged(series_row("Call round trip, worst", history, js(|j| j.call_max_ms), " ms"), 50.0, 250.0),
        series_row("JS heap", history, js(|j| j.heap_mb), " MB"),
    ];
    out.push(table_section("The app's JS", &STATS, rows, ""));
    out.push(section(
        "Event loop",
        chart(
            vec![
                Series::new("Event loop late", theme::accent(), history, js(|j| j.lag_max_ms)),
                Series::new("Blocked, ms/s", theme::warning(), history, js(|j| j.blocked_ms_per_s)),
            ],
            "ms",
            Some(FRAME_MS),
        ),
    ));
    let rows = last
        .compartments
        .iter()
        .map(|c| {
            vec![
                Cell::text(format!("{}{}", c.name, if c.held { " (held)" } else { "" })),
                Cell::num(c.renders as f32, ""),
                Cell::num(c.deferred as f32, ""),
                Cell::ms(c.total_ms).judged(50.0, 200.0),
                Cell::ms(c.max_ms).judged(FRAME_MS, 3.0 * FRAME_MS),
            ]
        })
        .collect();
    let headers = ["", "Renders", "Put off", "Total", "Slowest"];
    out.push(table_section("Plugin views and theme parts, last second", &headers, rows, "Nothing rendered."));
    if let Some(js) = &last.js {
        let rows = js.hot.iter().map(|h| vec![Cell::text(h.name.clone()), Cell::num(h.per_s, "/s")]).collect();
        out.push(table_section("Most rendered components", &["", "Renders"], rows, "None re-rendered."));
        let rows = js
            .events
            .iter()
            .map(|e| vec![Cell::text(e.name.clone()), Cell::num(e.per_s, "/s"), Cell::ms(e.ms_per_s)])
            .collect();
        out.push(table_section("Server events handled", &["", "Events", "Took"], rows, "No events."));
        if let Some(slowest) = &js.call_slowest {
            out.push(note(&format!("The slowest call last second: {slowest} ({:.0} ms).", js.call_max_ms)));
        }
    }
    let startup = diag::startup();
    let mut steps: Vec<(String, f32)> = startup.marks.clone();
    steps.extend(startup.engine_ms.map(|ms| ("engine (native)".to_string(), ms)));
    steps.extend(startup.first_frame_ms.map(|ms| ("first frame (native)".to_string(), ms)));
    steps.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut before = 0.0;
    let rows = steps
        .into_iter()
        .map(|(name, at)| {
            let step = at - before;
            before = at;
            vec![Cell::text(name), Cell::ms(step), Cell::ms(at)]
        })
        .collect();
    out.push(table_section("How the app started (ms since its process did)", &["", "Step", "At"], rows, ""));
    out.push(note(
        "The app's JS runs in a worker beside the main thread: a late event loop there delays what \
         React draws, not the terminals or the window. A component rendered is one React ran again; \
         put off is a slow view's render its budget moved to a later frame.",
    ));
    out
}

fn processes_page(history: &[Arc<Sample>], last: &Sample) -> Vec<AnyElement> {
    let stat = |pid: i32, f: fn(&diag::processes::Process) -> Option<f32>| {
        let values: Vec<f32> =
            history.iter().filter_map(|s| s.processes.list.iter().find(|p| p.pid == pid).and_then(f)).collect();
        Stats::of(&values)
    };
    let mut rows: Vec<Vec<Cell>> = last
        .processes
        .list
        .iter()
        .map(|p| {
            let cpu = stat(p.pid, |p| p.cpu);
            let gpu = stat(p.pid, |p| p.gpu);
            let shown =
                |known: bool, v: f32| if known { Cell::num(v, "%").judged(50.0, 90.0) } else { Cell::text("–") };
            vec![
                Cell::text(format!("{} ({})", p.name, p.pid)),
                shown(p.cpu.is_some(), cpu.now),
                shown(p.cpu.is_some(), cpu.p50),
                shown(p.cpu.is_some(), cpu.p95),
                shown(p.cpu.is_some(), cpu.max),
                shown(p.gpu.is_some(), gpu.now),
                shown(p.gpu.is_some(), gpu.p50),
                shown(p.gpu.is_some(), gpu.p95),
                shown(p.gpu.is_some(), gpu.max),
                p.rss.map_or(Cell::text("–"), |rss| Cell::num(rss as f32 / 1_048_576.0, " MB")),
            ]
        })
        .collect();
    if last.processes.gpu_total.is_some() {
        let total: Vec<f32> = history.iter().filter_map(|s| s.processes.gpu_total).collect();
        let total = Stats::of(&total);
        let blank = || Cell::text("");
        rows.push(vec![
            Cell::strong("GPU, every process"),
            blank(),
            blank(),
            blank(),
            blank(),
            Cell::num(total.now, "%"),
            Cell::num(total.p50, "%"),
            Cell::num(total.p95, "%"),
            Cell::num(total.max, "%"),
            blank(),
        ]);
    }
    let headers = ["", "CPU", "p50", "p95", "Max", "GPU", "p50", "p95", "Max", "Memory"];
    let mut out = vec![table_section("Usage", &headers, rows, "Nothing to read on this system.")];
    let pid_of = |role: &str| last.processes.list.iter().find(|p| p.role == role).map(|p| p.pid);
    let of = |pid: Option<i32>, f: fn(&diag::processes::Process) -> f32| {
        move |s: &Sample| s.processes.list.iter().find(|p| Some(p.pid) == pid).map_or(0.0, f)
    };
    out.push(section(
        "CPU (100 a core)",
        chart(
            vec![
                Series::new("pumi", theme::accent(), history, of(pid_of("app"), |p| p.cpu.unwrap_or(0.0))),
                Series::new("pumid", theme::palette::green(), history, of(pid_of("server"), |p| p.cpu.unwrap_or(0.0))),
            ],
            "%",
            None,
        ),
    ));
    out.push(section(
        "GPU",
        chart(
            vec![
                Series::new("pumi", theme::accent(), history, of(pid_of("app"), |p| p.gpu.unwrap_or(0.0))),
                Series::new(
                    "WindowServer",
                    theme::palette::green(),
                    history,
                    of(pid_of("compositor"), |p| p.gpu.unwrap_or(0.0)),
                ),
            ],
            "%",
            None,
        ),
    ));
    let rows = last
        .processes
        .threads
        .iter()
        .map(|t| {
            let values: Vec<f32> = history
                .iter()
                .filter_map(|s| s.processes.threads.iter().find(|x| x.name == t.name).map(|x| x.cpu))
                .collect();
            let label = if t.count > 1 { format!("{} ×{}", t.name, t.count) } else { t.name.clone() };
            let stats = Stats::of(&values);
            vec![
                Cell::text(label),
                Cell::num(t.cpu, "%").judged(50.0, 90.0),
                Cell::num(stats.p50, "%").judged(50.0, 90.0),
                Cell::num(stats.p95, "%").judged(50.0, 90.0),
                Cell::num(stats.max, "%").judged(50.0, 90.0),
            ]
        })
        .collect();
    out.push(table_section("The app's threads", &["", "CPU", "p50", "p95", "Max"], rows, ""));
    out.push(note(
        "GPU shares are of one GPU's time over each second, from the driver's own accounting. \
         WindowServer composites pumi's window over everything else, so a window that redraws often \
         shows up there as well as under pumi; other apps add to WindowServer's share too. A thread \
         named terminal %N reads that tab's output; main is AppKit's and GPUI's; the Workers are the \
         app's JS and its plugins.",
    ));
    out
}

// --- Parts.

fn verdict(history: &[Arc<Sample>]) -> AnyElement {
    let recent = &history[history.len().saturating_sub(RECENT)..];
    let verdict = diag::verdict(recent);
    let mut body = div().vcol().gap(px(4.)).child(banner_line(verdict.severity, &verdict.text));
    for finding in verdict.findings.iter().skip(1) {
        body = body.child(div().pl(px(18.)).text(13., theme::muted()).child(finding.clone()));
    }
    div()
        .mb(px(20.))
        .p(px(14.))
        .rounded(px(12.))
        .border_1()
        .border_color(theme::border())
        .bg(theme::surface())
        .child(body)
        .into_any_element()
}

fn severity_colour(severity: &str) -> Rgba {
    match severity {
        "bad" => theme::error(),
        "warn" => theme::warning(),
        _ => theme::palette::green(),
    }
}

fn banner_line(severity: &str, text: &str) -> AnyElement {
    div()
        .hrow()
        .gap(px(8.))
        .child(div().flex_none().size(px(10.)).rounded_full().bg(severity_colour(severity)))
        .child(div().text(14., theme::surface_text()).child(text.to_string()))
        .into_any_element()
}

fn banner(severity: &str, text: &str) -> AnyElement {
    div()
        .mb(px(20.))
        .p(px(14.))
        .rounded(px(12.))
        .border_1()
        .border_color(severity_colour(severity))
        .bg(theme::surface())
        .child(banner_line(severity, text))
        .into_any_element()
}

fn section(title: &str, body: AnyElement) -> AnyElement {
    div()
        .vcol()
        .pb(px(20.))
        .child(div().pl(px(8.)).pb(px(8.)).text(13., theme::surface_text()).child(title.to_string()))
        .child(
            div().p(px(10.)).rounded(px(12.)).border_1().border_color(theme::border()).bg(theme::surface()).child(body),
        )
        .into_any_element()
}

fn note(text: &str) -> AnyElement {
    div().pt(px(4.)).text(12., theme::muted()).child(text.to_string()).into_any_element()
}

/// How a number reads beside its kind: as is, nothing (a zero), worth a look, or too much.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tone {
    Plain,
    Zero,
    Warn,
    Bad,
}

impl Tone {
    fn colour(self) -> Rgba {
        match self {
            Tone::Plain => theme::surface_text(),
            Tone::Zero => theme::placeholder(),
            Tone::Warn => theme::warning(),
            Tone::Bad => theme::error(),
        }
    }
}

struct Cell {
    text: String,
    unit: String,
    value: Option<f32>,
    strong: bool,
    tone: Tone,
}

impl Cell {
    fn text(text: impl Into<String>) -> Self {
        Self { text: text.into(), unit: String::new(), value: None, strong: false, tone: Tone::Plain }
    }
    fn strong(text: impl Into<String>) -> Self {
        Self { strong: true, ..Self::text(text) }
    }
    fn num(value: f32, unit: &str) -> Self {
        let tone = if value == 0.0 { Tone::Zero } else { Tone::Plain };
        Self { text: number(value), unit: unit.trim().to_string(), value: Some(value), strong: false, tone }
    }
    fn ms(value: f32) -> Self {
        Self::num(value, " ms")
    }
    /// Marked worth a look at `warn` or more, and too much at `bad`.
    fn judged(mut self, warn: f32, bad: f32) -> Self {
        match self.value {
            Some(v) if v >= bad => self.tone = Tone::Bad,
            Some(v) if v >= warn => self.tone = Tone::Warn,
            _ => {}
        }
        self
    }
}

/// A row whose numbers are all judged by the same bounds.
fn judged(row: Vec<Cell>, warn: f32, bad: f32) -> Vec<Cell> {
    row.into_iter().map(|cell| cell.judged(warn, bad)).collect()
}

/// A number as the tables write it: a decimal under ten, none above.
fn number(value: f32) -> String {
    match value.abs() {
        v if v == 0.0 => "0".into(),
        v if v < 10.0 => format!("{value:.1}"),
        _ => format!("{value:.0}"),
    }
}

/// `colour` at `alpha`.
fn tint(colour: Rgba, alpha: f32) -> Rgba {
    Rgba { a: alpha, ..colour }
}

/// Figures of one width, so a column of numbers lines up.
fn tabular() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

/// A number and its unit, the number brightest; a judged one on a tint of its colour.
fn figure(text: String, unit: &str, tone: Tone, strong: bool, size: f32) -> AnyElement {
    let colour = tone.colour();
    let weight =
        if strong || matches!(tone, Tone::Warn | Tone::Bad) { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM };
    let mut out = div()
        .flex()
        .flex_row()
        .items_baseline()
        .gap(px(3.))
        .child(div().text(size, colour).font_weight(weight).font_features(tabular()).child(text));
    if !unit.is_empty() {
        out = out.child(div().text(size - 2.0, theme::placeholder()).child(unit.to_string()));
    }
    if matches!(tone, Tone::Warn | Tone::Bad) {
        out = out.px(px(6.)).rounded(px(5.)).bg(tint(colour, 0.14));
    }
    out.into_any_element()
}

/// A column: the first holds names and takes what is left, the rest hold numbers, right-aligned.
fn column(at: usize) -> gpui::Div {
    let cell = div().flex_basis(px(0.)).flex_shrink(1.0).whitespace_nowrap();
    match at {
        0 => cell.flex_grow(3.0).min_w(px(140.)).overflow_hidden().text_ellipsis(),
        _ => cell.flex_grow(1.0).min_w(px(COLUMN)).flex().justify_end(),
    }
}

fn table(headers: &[&str], rows: Vec<Vec<Cell>>, empty: &str) -> AnyElement {
    let line = || div().hrow().px(px(14.)).gap(px(12.));
    let header = line().h(px(30.)).bg(tint(theme::text(), 0.04)).border_b_1().border_color(theme::border()).children(
        headers
            .iter()
            .enumerate()
            .map(|(at, h)| column(at).text(11., theme::muted()).font_weight(FontWeight::SEMIBOLD).child(h.to_string())),
    );
    let mut out = div().vcol().child(header);
    if rows.is_empty() && !empty.is_empty() {
        out = out.child(line().h(px(34.)).text(13., theme::muted()).child(empty.to_string()));
    }
    let count = rows.len();
    for (index, row) in rows.into_iter().enumerate() {
        let mut el = line().h(px(32.)).hover(|style| style.bg(tint(theme::accent(), 0.08))).children(
            row.into_iter().enumerate().map(|(at, cell)| {
                let el = column(at);
                if cell.value.is_some() {
                    el.child(figure(cell.text, &cell.unit, cell.tone, cell.strong, 13.))
                } else {
                    let colour = if at == 0 || cell.strong { theme::surface_text() } else { theme::muted() };
                    let el = el.text(13., colour).child(cell.text);
                    if cell.strong { el.font_weight(FontWeight::SEMIBOLD) } else { el }
                }
                .into_any_element()
            }),
        );
        if index % 2 == 1 {
            el = el.bg(tint(theme::text(), 0.02));
        }
        if index + 1 < count {
            el = el.border_b_1().border_color(tint(theme::border(), 0.5));
        }
        out = out.child(el);
    }
    out.into_any_element()
}

/// A section that is a table: the table runs to the frame's edges.
fn table_section(title: &str, headers: &[&str], rows: Vec<Vec<Cell>>, empty: &str) -> AnyElement {
    div()
        .vcol()
        .pb(px(20.))
        .child(div().pl(px(8.)).pb(px(8.)).text(13., theme::surface_text()).child(title.to_string()))
        .child(
            div()
                .rounded(px(12.))
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .overflow_hidden()
                .child(table(headers, rows, empty)),
        )
        .into_any_element()
}

/// A table's columns for a value kept each second.
const STATS: [&str; 5] = ["", "Now", "p50", "p95", "Max"];

/// A value kept each second: the latest, its percentiles and the most.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Stats {
    now: f32,
    p50: f32,
    p95: f32,
    max: f32,
}

impl Stats {
    fn of(values: &[f32]) -> Self {
        let Some(&now) = values.last() else { return Self::default() };
        Self {
            now,
            p50: diag::frames::percentile(values, 0.5),
            p95: diag::frames::percentile(values, 0.95),
            max: values.iter().copied().fold(0.0, f32::max),
        }
    }
}

fn series_row(label: &str, history: &[Arc<Sample>], value: impl Fn(&Sample) -> f32, unit: &str) -> Vec<Cell> {
    let values: Vec<f32> = history.iter().map(|s| value(s)).collect();
    let stats = Stats::of(&values);
    vec![
        Cell::text(label),
        Cell::num(stats.now, unit),
        Cell::num(stats.p50, unit),
        Cell::num(stats.p95, unit),
        Cell::num(stats.max, unit),
    ]
}

struct Series {
    label: String,
    colour: Rgba,
    values: Vec<f32>,
}

impl Series {
    fn new(label: &str, colour: Rgba, history: &[Arc<Sample>], value: impl Fn(&Sample) -> f32) -> Self {
        Self { label: label.to_string(), colour, values: history.iter().map(|s| value(s)).collect() }
    }
}

/// The scale's top: three steps of 1, 2 or 5 times a power of ten that reach `peak`.
fn scale(peak: f32) -> f32 {
    let step = (peak / 3.0).max(0.1);
    let magnitude = 10f32.powf(step.log10().floor());
    let nice =
        [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * magnitude).find(|n| *n >= step).unwrap_or(10.0 * magnitude);
    nice * 3.0
}

fn chart(series: Vec<Series>, unit: &str, reference: Option<f32>) -> AnyElement {
    let peak = series.iter().flat_map(|s| s.values.iter().copied()).fold(reference.unwrap_or(0.0), f32::max);
    let top = scale(peak);
    let grid = Hsla::from(theme::border());
    let lines: Vec<(Rgba, Vec<f32>)> = series.iter().map(|s| (s.colour, s.values.clone())).collect();
    let reference_line = reference.filter(|r| *r <= top);
    let stats: Vec<Stats> = series.iter().map(|s| Stats::of(&s.values)).collect();
    // The first series' p50 and p95, drawn across the chart in its colour.
    let marks: Vec<(&str, f32)> = match (series.first(), stats.first()) {
        (Some(first), Some(st)) if !first.values.is_empty() => vec![("p50", st.p50), ("p95", st.p95)],
        _ => Vec::new(),
    };
    let first_colour = series.first().map_or(theme::accent(), |s| s.colour);
    let mark_lines: Vec<(f32, [f32; 2])> =
        marks.iter().map(|(name, v)| (*v, if *name == "p50" { [2.0, 3.0] } else { [6.0, 3.0] })).collect();
    let plot = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, (), window, _| {
            let (left, width) = (f32::from(bounds.origin.x), f32::from(bounds.size.width) - SCALE_WIDTH);
            let (top_y, height) = (f32::from(bounds.origin.y) + 6.0, f32::from(bounds.size.height) - 12.0);
            let y = |v: f32| top_y + height * (1.0 - (v / top).clamp(0.0, 1.0));
            for step in 0..=3 {
                let at = y(top * step as f32 / 3.0);
                window.paint_quad(fill(Bounds::new(point(px(left), px(at)), size(px(width), px(1.))), grid));
            }
            if let Some(r) = reference_line {
                let mut dashed = PathBuilder::stroke(px(1.)).dash_array(&[px(4.), px(4.)]);
                dashed.move_to(point(px(left), px(y(r))));
                dashed.line_to(point(px(left + width), px(y(r))));
                if let Ok(path) = dashed.build() {
                    window.paint_path(path, Hsla::from(theme::placeholder()));
                }
            }
            for (value, dash) in &mark_lines {
                let mut dashed = PathBuilder::stroke(px(1.)).dash_array(&[px(dash[0]), px(dash[1])]);
                dashed.move_to(point(px(left), px(y(*value))));
                dashed.line_to(point(px(left + width), px(y(*value))));
                if let Ok(path) = dashed.build() {
                    window.paint_path(path, Hsla::from(tint(first_colour, 0.7)));
                }
            }
            for (colour, values) in &lines {
                if values.is_empty() {
                    continue;
                }
                let x = |at: usize| left + width * (at + HISTORY - values.len()) as f32 / (HISTORY - 1) as f32;
                let mut path = PathBuilder::stroke(px(1.5));
                path.move_to(point(px(x(0)), px(y(values[0]))));
                for (at, value) in values.iter().enumerate().skip(1) {
                    path.line_to(point(px(x(at)), px(y(*value))));
                }
                if values.len() == 1 {
                    path.line_to(point(px(x(0) + 1.0), px(y(values[0]))));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, Hsla::from(*colour));
                }
            }
        },
    )
    .size_full();
    let labels = (0..=3).map(|step| {
        let value = top * step as f32 / 3.0;
        let at = 6.0 + (CHART_HEIGHT - 12.0) * (1.0 - step as f32 / 3.0) - 8.0;
        div()
            .absolute()
            .right_0()
            .top(px(at))
            .w(px(SCALE_WIDTH - 6.0))
            .flex()
            .justify_end()
            .text(11., theme::muted())
            .child(number(value))
    });
    // Each tag sits at its line, inside the plot's left edge; one too close to the last is dropped.
    let mut placed: Vec<f32> = Vec::new();
    let tags: Vec<AnyElement> = marks
        .iter()
        .filter_map(|(name, value)| {
            let at = 6.0 + (CHART_HEIGHT - 12.0) * (1.0 - (value / top).clamp(0.0, 1.0)) - 16.0;
            if placed.iter().any(|p| (p - at).abs() < 14.0) {
                return None;
            }
            placed.push(at);
            Some(
                div()
                    .absolute()
                    .left(px(4.))
                    .top(px(at.max(0.0)))
                    .hrow()
                    .gap(px(4.))
                    .px(px(5.))
                    .rounded(px(4.))
                    .bg(tint(theme::surface(), 0.85))
                    .child(div().text(10., first_colour).font_weight(FontWeight::SEMIBOLD).child(name.to_string()))
                    .child(figure(number(*value), unit, Tone::Plain, true, 11.))
                    .into_any_element(),
            )
        })
        .collect();
    let legend = div().vcol().gap(px(4.)).pt(px(10.)).children(series.iter().zip(&stats).map(|(s, st)| {
        let stat =
            |name: &str, value: f32| {
                div()
                    .hrow()
                    .gap(px(4.))
                    .child(div().text(11., theme::placeholder()).child(name.to_string()))
                    .child(figure(number(value), unit, Tone::Plain, false, 12.))
            };
        div()
            .hrow()
            .gap(px(14.))
            .child(
                div()
                    .hrow()
                    .gap(px(6.))
                    .flex_1()
                    .min_w_0()
                    .child(div().flex_none().size(px(8.)).rounded_full().bg(s.colour))
                    .child(
                        div()
                            .text(12., theme::muted())
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(s.label.clone()),
                    ),
            )
            .child(stat("now", st.now))
            .child(stat("p50", st.p50))
            .child(stat("p95", st.p95))
            .child(stat("max", st.max))
    }));
    div()
        .vcol()
        .child(
            div().relative().h(px(CHART_HEIGHT)).child(plot).children(labels).children(tags).child(
                div().absolute().right_0().top(px(-14.)).text(11., theme::placeholder()).child(unit.to_string()),
            ),
        )
        .child(legend)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_is_three_round_steps_over_the_peak() {
        assert_eq!(scale(13.6), 15.0);
        assert_eq!(scale(16.7), 30.0);
        assert_eq!(scale(0.0), 0.3);
        assert_eq!(scale(250.0), 300.0);
    }

    #[test]
    fn numbers_keep_a_decimal_only_when_small() {
        assert_eq!(number(0.0), "0");
        assert_eq!(number(4.25), "4.2");
        assert_eq!(number(13.6), "14");
    }

    #[test]
    fn stats_are_the_last_the_percentiles_and_the_most() {
        assert_eq!(Stats::of(&[1.0, 3.0, 2.0]), Stats { now: 2.0, p50: 2.0, p95: 3.0, max: 3.0 });
        assert_eq!(Stats::of(&[]), Stats::default());
    }

    #[test]
    fn a_page_is_found_by_its_name() {
        assert_eq!(Page::named("Processes"), Some(Page::Processes));
        assert_eq!(Page::named("gpu"), Some(Page::Processes));
        assert_eq!(Page::named("nope"), None);
    }
}
