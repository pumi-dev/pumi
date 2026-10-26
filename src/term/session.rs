use super::grid::Grid;
use super::local::{Local, Modes, Wheel};
use crate::runtime::{AttachMode as Mode, Runtime, TerminalInput, TerminalLink, TerminalOutput};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
type Diagnostics = crate::diagnostics::terminals::Handle;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum End {
    TakenOver,
    Closed,
    Lost,
}
const RETRY: (Duration, Duration) = (Duration::from_millis(50), Duration::from_secs(1));
const TELL_AFTER: Duration = Duration::from_secs(5);
const QUEUE: usize = 1024;
struct Shared {
    local: Mutex<Option<Local>>,
    grid: Mutex<Arc<Grid>>,
    cell_px: Mutex<(u32, u32)>,
    clipboard: Mutex<Vec<String>>,
    generation: AtomicU64,
    stale: AtomicBool,
    held: AtomicBool,
    changed: UnboundedSender<()>,
    diagnostics: Diagnostics,
    /// The terminal's modes as of its last batch of output; `None` until a stream starts.
    modes: Mutex<Option<Modes>>,
    /// The mouse's report format as of the same batch (see [`Modes`]).
    mouse: Mutex<Option<pumi_keys::mouse::Encoder>>,
    dropped: Mutex<Vec<Arc<gpui::RenderImage>>>,
    /// Rows the user moved the scrollback by, and whether a key sent it to the bottom, not yet
    /// applied: whoever holds the terminal next applies them ([`Shared::publish`]).
    scroll: AtomicIsize,
    bottom: AtomicBool,
}
impl Shared {
    fn touch(&self) -> bool {
        self.stale.swap(true, Ordering::AcqRel) || self.changed.unbounded_send(()).is_ok()
    }
    /// The screen as last published. Never waits on the parsing thread: a screen held by a
    /// synchronized update is looked at again only if the terminal is free.
    fn read(&self) -> Arc<Grid> {
        self.stale.store(false, Ordering::Release);
        if self.held.load(Ordering::Acquire) {
            self.refresh();
        }
        lock(&self.grid).clone()
    }
    /// Brings what the app's thread reads up to date with `local`: the scrollback moves asked
    /// for, the screen (unless a synchronized update holds it), the modes, the images let go.
    /// Runs with the terminal locked, on whichever thread holds it.
    fn publish(&self, local: &mut Local) {
        if self.bottom.swap(false, Ordering::SeqCst) {
            local.terminal.scroll_to_bottom();
        }
        let delta = self.scroll.swap(0, Ordering::SeqCst);
        if delta != 0 {
            local.terminal.scroll(delta);
        }
        let began = Instant::now();
        match local.settled_screen() {
            Some(screen) => {
                self.diagnostics.screen(began.elapsed());
                *lock(&self.grid) = Arc::new(screen);
                self.held.store(false, Ordering::Release);
            }
            None => self.held.store(true, Ordering::Release),
        }
        self.follow(local);
        let dropped = local.take_dropped_images();
        if !dropped.is_empty() {
            lock(&self.dropped).extend(dropped);
        }
    }
    fn follow(&self, local: &Local) {
        *lock(&self.modes) = Some(local.modes());
        if let Some(mouse) = lock(&self.mouse).as_mut() {
            mouse.follow(&local.terminal);
        }
    }
    /// Publishes now if the terminal is free; if it is held, its holder publishes once it lets
    /// go ([`Shared::settle`]). Whether it published.
    fn refresh(&self) -> bool {
        let generation = self.generation.load(Ordering::Relaxed);
        let Ok(mut slot) = self.local.try_lock() else { return false };
        let Some(local) = slot.as_mut().filter(|local| local.generation == generation) else { return false };
        self.publish(local);
        true
    }
    /// Run after letting the terminal go: what the app's thread asked for while it was held.
    fn settle(&self) {
        std::sync::atomic::fence(Ordering::SeqCst);
        let pending = self.bottom.load(Ordering::SeqCst) || self.scroll.load(Ordering::SeqCst) != 0;
        if pending && self.refresh() {
            self.touch();
        }
    }
}
struct Stream {
    mode: Mode,
    generation: u64,
    link: Box<dyn TerminalLink>,
    seq: Arc<AtomicU64>,
}
enum Link {
    Pending,
    Opening { mode: Mode, ticket: u64, queued: Vec<TerminalInput> },
    Live(Stream),
    Closed,
}
struct Inner {
    tab_id: String,
    runtime: Arc<dyn Runtime>,
    shared: Shared,
    size: Mutex<Option<Size>>,
    link: Mutex<Link>,
    tickets: AtomicU64,
    error: Mutex<Option<String>>,
}
pub struct TerminalSession {
    inner: Arc<Inner>,
    encoder: Mutex<Option<pumi_keys::Encoder>>,
}
impl TerminalSession {
    pub fn new(tab_id: &str, runtime: Arc<dyn Runtime>) -> (Self, UnboundedReceiver<()>) {
        let (changed, changes) = unbounded();
        let inner = Arc::new(Inner {
            tab_id: tab_id.to_owned(),
            runtime,
            shared: Shared {
                local: Mutex::new(None),
                grid: Mutex::new(Arc::new(Grid::default())),
                cell_px: Mutex::new((0, 0)),
                clipboard: Mutex::new(Vec::new()),
                generation: AtomicU64::new(0),
                stale: AtomicBool::new(false),
                held: AtomicBool::new(false),
                changed,
                diagnostics: Diagnostics::open(tab_id),
                modes: Mutex::new(None),
                mouse: Mutex::new(pumi_keys::mouse::Encoder::new()),
                dropped: Mutex::new(Vec::new()),
                scroll: AtomicIsize::new(0),
                bottom: AtomicBool::new(false),
            },
            size: Mutex::new(None),
            link: Mutex::new(Link::Pending),
            tickets: AtomicU64::new(0),
            error: Mutex::new(None),
        });
        (Self { inner, encoder: Mutex::new(pumi_keys::Encoder::new().ok()) }, changes)
    }
    pub fn take_error(&self) -> Option<String> {
        lock(&self.inner.error).take()
    }
    pub fn closed(&self) -> bool {
        matches!(*lock(&self.inner.link), Link::Closed)
    }
    pub fn observing(&self) -> bool {
        matches!(&*lock(&self.inner.link), Link::Live(stream) if stream.mode == Mode::Observe)
    }
    pub fn grid(&self) -> Arc<Grid> {
        self.inner.shared.read()
    }
    /// Takes control back from a client that took it while this one was only watching, as a
    /// terminal attaching anew does.
    pub fn claim(&self) {
        let mut link = lock(&self.inner.link);
        if matches!(&*link, Link::Live(stream) if stream.mode == Mode::Observe) {
            connect(&self.inner, &mut link, Mode::Control, None);
        }
    }
    pub fn tab(&self) -> &str {
        &self.inner.tab_id
    }
    /// What this terminal costs and how long it took to show, for Diagnostics.
    pub fn diagnostics(&self) -> &Diagnostics {
        &self.inner.shared.diagnostics
    }
    pub fn held(&self) -> bool {
        self.inner.shared.held.load(Ordering::Acquire)
    }
    pub fn take_clipboard_writes(&self) -> Vec<String> {
        std::mem::take(&mut *lock(&self.inner.shared.clipboard))
    }
    pub fn send_input(&self, bytes: &[u8]) {
        self.inner.shared.diagnostics.typed();
        self.write(TerminalInput::Bytes(bytes.to_vec()), true);
    }
    pub fn wheel(&self, steps: i32, lines_per_step: u16, at: (u16, u16)) {
        if steps == 0 {
            return;
        }
        let shared = &self.inner.shared;
        let cell = *lock(&shared.cell_px);
        let wheel = {
            let modes = lock(&shared.modes);
            let Some(modes) = modes.as_ref() else { return };
            let (mut mouse, mut keys) = (lock(&shared.mouse), lock(&self.encoder));
            modes.wheel(mouse.as_mut(), keys.as_mut(), steps, lines_per_step.max(1), at, cell)
        };
        match wheel {
            Wheel::Send(bytes) if !bytes.is_empty() => self.write(TerminalInput::Bytes(bytes), true),
            Wheel::Send(_) => {}
            Wheel::Scroll(delta) => {
                shared.scroll.fetch_add(delta, Ordering::SeqCst);
                if shared.refresh() {
                    shared.touch();
                }
            }
        }
    }
    pub fn mouse(&self, action: &str, button: &str, column: u16, row: u16, modifiers: u8) {
        use pumi_keys::mouse;
        let shared = &self.inner.shared;
        let screen = match lock(&shared.modes).as_ref() {
            Some(modes) if modes.mouse_tracked => modes.screen,
            _ => return,
        };
        let Some(kind) = mouse::ACTIONS.iter().find(|(n, _)| *n == action).map(|(_, a)| *a) else { return };
        let button = mouse::BUTTONS.iter().find(|(n, _)| *n == button).map(|(_, b)| *b);
        let mods = super::local::mods(modifiers);
        let event = mouse::Mouse { action: kind, button, column, row, mods };
        let cell = *lock(&shared.cell_px);
        let bytes = lock(&shared.mouse).as_mut().map(|mouse| mouse.encode(cell, screen, &event)).unwrap_or_default();
        if !bytes.is_empty() {
            self.write(TerminalInput::Bytes(bytes), action == "down");
        }
    }
    pub fn key(
        &self,
        chord: &pumi_keys::Chord,
        option_as_alt: pumi_keys::OptionAsAlt,
        action: pumi_keys::Action,
    ) -> bool {
        let shared = &self.inner.shared;
        let Some((keys, scrolled_back)) = lock(&shared.modes).as_ref().map(|m| (m.keys, m.scrolled_back)) else {
            return false;
        };
        let options = pumi_keys::Options { option_as_alt, ..keys };
        let encode = |encoder: &mut pumi_keys::Encoder| encoder.encode_action(chord, &options, action);
        let bytes = lock(&self.encoder).as_mut().map(encode).unwrap_or_default();
        if bytes.is_empty() {
            return false;
        }
        let typing = action != pumi_keys::Action::Release && !pumi_keys::is_modifier(chord.key);
        if typing && (scrolled_back || shared.scroll.load(Ordering::SeqCst) != 0) {
            shared.scroll.store(0, Ordering::SeqCst);
            shared.bottom.store(true, Ordering::SeqCst);
            if shared.refresh() {
                shared.touch();
            }
        }
        shared.diagnostics.typed();
        self.write(TerminalInput::Bytes(bytes), true);
        true
    }
    fn modes<T: Default>(&self, read: impl FnOnce(&Modes) -> T) -> T {
        lock(&self.inner.shared.modes).as_ref().map(read).unwrap_or_default()
    }
    pub fn kitty_flags(&self) -> u8 {
        self.modes(|modes| modes.keys.kitty_flags)
    }
    pub fn focus(&self, gained: bool) {
        let bytes = self.modes(|modes| modes.focus(gained));
        if !bytes.is_empty() {
            self.write(TerminalInput::Bytes(bytes), false);
        }
    }
    pub fn type_text(&self, text: &str) {
        self.send_input(text.as_bytes());
    }
    pub fn take_dropped_images(&self) -> Vec<Arc<gpui::RenderImage>> {
        std::mem::take(&mut *lock(&self.inner.shared.dropped))
    }
    pub fn hyperlink(&self, col: u16, row: u16) -> Option<String> {
        self.local(|local| local.hyperlink(col, row)).flatten()
    }
    pub fn pointer(&self) -> Option<String> {
        self.modes(|modes| modes.pointer.clone())
    }
    pub fn bracketed_paste(&self) -> bool {
        self.modes(|modes| modes.bracketed_paste)
    }
    /// Whether the program asked to be told of the mouse (it selects for itself, as pi does).
    pub fn mouse_tracked(&self) -> bool {
        self.modes(|modes| modes.mouse_tracked)
    }
    fn local<T>(&self, f: impl FnOnce(&mut Local) -> T) -> Option<T> {
        let generation = self.inner.shared.generation.load(Ordering::Relaxed);
        let mut slot = lock(&self.inner.shared.local);
        slot.as_mut().filter(|local| local.generation == generation).map(f)
    }
    pub fn resize(&self, size: Size) {
        let size = Size { cols: size.cols.max(2), rows: size.rows.max(2), ..size };
        {
            let mut current = lock(&self.inner.size);
            if *current == Some(size) {
                return;
            }
            *current = Some(size);
        }
        self.inner.shared.diagnostics.sized(size.cols, size.rows);
        *lock(&self.inner.shared.cell_px) = (size.cell_width, size.cell_height);
        let mut link = lock(&self.inner.link);
        match &mut *link {
            Link::Pending => connect(&self.inner, &mut link, Mode::Control, None),
            Link::Live(stream) if stream.mode == Mode::Control => stream.link.send(TerminalInput::Resize(size)),
            Link::Live(_) => connect(&self.inner, &mut link, Mode::Observe, None),
            Link::Opening { .. } | Link::Closed => {}
        }
    }
    fn write(&self, input: TerminalInput, claim: bool) {
        let mut link = lock(&self.inner.link);
        let observing = match &mut *link {
            Link::Live(stream) if stream.mode == Mode::Control => return stream.link.send(input),
            Link::Live(_) => true,
            Link::Opening { mode, .. } => *mode == Mode::Observe,
            Link::Pending | Link::Closed => return,
        };
        if observing && !claim {
            return;
        }
        if observing {
            connect(&self.inner, &mut link, Mode::Control, None);
        }
        if let Link::Opening { queued, .. } = &mut *link
            && queued.len() < QUEUE
        {
            queued.push(input);
        }
    }
}
impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.inner.shared.diagnostics.close();
        self.inner.shared.generation.fetch_add(1, Ordering::Relaxed);
        let stream = std::mem::replace(&mut *lock(&self.inner.link), Link::Closed);
        drop(stream);
    }
}
fn connect(inner: &Arc<Inner>, link: &mut Link, mode: Mode, resume: Option<u64>) {
    let ticket = inner.tickets.fetch_add(1, Ordering::Relaxed) + 1;
    let queued = match std::mem::replace(link, Link::Closed) {
        Link::Opening { queued, .. } => queued,
        _ => Vec::new(),
    };
    *link = Link::Opening { mode, ticket, queued };
    let inner = Arc::clone(inner);
    let named = thread::Builder::new().name(format!("terminal attach {}", inner.tab_id));
    let spawned = named.spawn(move || {
        let (began, mut delay, mut told) = (Instant::now(), RETRY.0, false);
        loop {
            let Some(size) = *lock(&inner.size) else { return };
            let attached = inner.runtime.attach(&inner.tab_id, mode, size, resume);
            let mut link = lock(&inner.link);
            let Link::Opening { ticket: current, queued, .. } = &mut *link else { return };
            if *current != ticket {
                return;
            }
            match attached {
                Ok(attached) => {
                    inner.shared.diagnostics.attached(mode == Mode::Observe);
                    let queued = std::mem::take(queued);
                    let mut stream = start(&inner, attached, mode, size, resume);
                    if mode == Mode::Control {
                        queued.into_iter().for_each(|input| stream.link.send(input));
                        if let Some(now) = *lock(&inner.size)
                            && now != size
                        {
                            stream.link.send(TerminalInput::Resize(now));
                        }
                    }
                    *link = Link::Live(stream);
                    return;
                }
                Err(err) if !told && began.elapsed() >= TELL_AFTER => {
                    told = true;
                    *lock(&inner.error) = Some(err);
                    let _ = inner.shared.changed.unbounded_send(());
                }
                Err(_) => {}
            }
            drop(link);
            thread::sleep(delay);
            delay = (delay * 2).min(RETRY.1);
        }
    });
    if let Err(err) = spawned {
        eprintln!("pumi: a terminal's attach: {err}");
    }
}
fn start(
    inner: &Arc<Inner>,
    attached: crate::runtime::Attached,
    mode: Mode,
    size: Size,
    resume: Option<u64>,
) -> Stream {
    let generation = inner.shared.generation.fetch_add(1, Ordering::Relaxed) + 1;
    *lock(&inner.shared.cell_px) = (size.cell_width, size.cell_height);
    let reader = Arc::clone(inner);
    let output = attached.output;
    let named = thread::Builder::new().name(format!("terminal {}", inner.tab_id));
    let spawned = named.spawn(move || {
        let reply = |bytes: Vec<u8>| {
            if let Link::Live(stream) = &mut *lock(&reader.link)
                && stream.generation == generation
                && stream.mode == Mode::Control
            {
                stream.link.send(TerminalInput::Bytes(bytes));
            }
        };
        let end = pump(output, size, &reader.shared, generation, resume.is_some(), &reply);
        stream_ended(&reader, generation, end);
    });
    if let Err(err) = spawned {
        eprintln!("pumi: a terminal's output: {err}");
    }
    Stream { mode, generation, link: attached.link, seq: attached.seq }
}
fn stream_ended(inner: &Arc<Inner>, generation: u64, end: End) {
    let mut link = lock(&inner.link);
    let (mode, seq) = match &*link {
        Link::Live(stream) if stream.generation == generation => (stream.mode, stream.seq.load(Ordering::Acquire)),
        _ => return,
    };
    match end {
        End::TakenOver if mode == Mode::Control => connect(inner, &mut link, Mode::Observe, None),
        End::Lost => connect(inner, &mut link, mode, (seq > 0).then_some(seq)),
        End::TakenOver | End::Closed => *link = Link::Closed,
    }
    drop(link);
    let _ = inner.shared.changed.unbounded_send(());
}
fn pump(
    output: impl IntoIterator<Item = TerminalOutput>,
    size: Size,
    shared: &Shared,
    generation: u64,
    resume: bool,
    reply: &dyn Fn(Vec<u8>),
) -> End {
    let current = || shared.generation.load(Ordering::Relaxed) == generation;
    {
        let mut slot = lock(&shared.local);
        match slot.as_mut().filter(|_| resume) {
            Some(local) => local.generation = generation,
            None => {
                let Some(local) = Local::new(generation, size.cols, size.rows) else {
                    return End::Closed;
                };
                *slot = Some(local);
            }
        }
        if let Some(local) = slot.as_ref() {
            shared.follow(local);
        }
    }
    for record in output {
        let mut slot = lock(&shared.local);
        let Some(local) = slot.as_mut().filter(|local| local.generation == generation) else {
            return End::Closed;
        };
        let more = match record {
            TerminalOutput::Snapshot { width, height, bytes, more } => {
                local.reset(width.max(2), height.max(2), *lock(&shared.cell_px));
                let began = Instant::now();
                local.write(&bytes);
                shared.diagnostics.snapshot(bytes.len());
                shared.diagnostics.output(bytes.len(), began.elapsed());
                local.take_replies();
                more
            }
            TerminalOutput::Frame { width, height, bytes, more } => {
                if width > 0 && height > 0 {
                    local.resize(width, height, *lock(&shared.cell_px));
                }
                let began = Instant::now();
                let copied = local.write(&bytes);
                shared.diagnostics.output(bytes.len(), began.elapsed());
                if !copied.is_empty() && current() {
                    lock(&shared.clipboard).extend(copied);
                }
                more
            }
            TerminalOutput::Ended { taken_over: true } => return End::TakenOver,
            TerminalOutput::Ended { taken_over: false } => return End::Closed,
            TerminalOutput::Lost => return End::Lost,
        };
        let replies = local.take_replies();
        // The screen is built here, at the end of a batch, so the app's thread only ever takes a
        // finished one and never waits for a parse.
        if !more && current() {
            shared.publish(local);
        }
        drop(slot);
        shared.settle();
        if !replies.is_empty() && current() {
            reply(replies);
        }
        if more {
            continue;
        }
        if !current() || !shared.touch() {
            return End::Closed;
        }
    }
    End::Closed
}
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
#[cfg(test)]
mod tests {
    use super::*;
    const SIZE: Size = Size { cols: 10, rows: 3, cell_width: 8, cell_height: 16 };
    fn shared(generation: u64) -> (Shared, UnboundedReceiver<()>) {
        let (changed, changes) = unbounded();
        let shared = Shared {
            local: Mutex::new(None),
            grid: Mutex::new(Arc::new(Grid::default())),
            cell_px: Mutex::new((SIZE.cell_width, SIZE.cell_height)),
            clipboard: Mutex::new(Vec::new()),
            generation: AtomicU64::new(generation),
            stale: AtomicBool::new(false),
            held: AtomicBool::new(false),
            changed,
            diagnostics: Diagnostics::open("test"),
            modes: Mutex::new(None),
            mouse: Mutex::new(pumi_keys::mouse::Encoder::new()),
            dropped: Mutex::new(Vec::new()),
            scroll: AtomicIsize::new(0),
            bottom: AtomicBool::new(false),
        };
        (shared, changes)
    }
    fn frame(width: u16, height: u16, bytes: &str) -> TerminalOutput {
        TerminalOutput::Frame { width, height, bytes: bytes.as_bytes().to_vec(), more: false }
    }
    fn text(cells: &[super::super::grid::Cell]) -> String {
        cells.iter().map(|c| c.text.as_str()).collect()
    }
    #[test]
    fn frame_is_emulated_at_its_own_size() {
        let row = "x".repeat(20);
        let (shared, _changes) = shared(1);
        assert_eq!(
            pump([frame(20, 2, &format!("\x1b[1;1H{row}\x1b[2;1Hdone"))], SIZE, &shared, 1, false, &|_| {}),
            End::Closed
        );
        let grid = shared.read();
        assert_eq!(grid.cols, 20);
        assert_eq!(grid.rows.len(), 2);
        assert_eq!(text(&grid.rows[0]), row);
        assert!(text(&grid.rows[1]).starts_with("done"));
    }
    #[test]
    fn session_waits_for_its_first_size() {
        let runtime = Arc::new(crate::runtime::pumid::Pumid::at(pumi_proto::paths::Paths::at("/nonexistent")));
        let (session, _changes) = TerminalSession::new("t1", runtime);
        assert!(matches!(*lock(&session.inner.link), Link::Pending));
        session.send_input(b"ls\n");
        session.wheel(3, 1, (0, 0));
        session.mouse("down", "left", 0, 0, 0);
        assert!(matches!(*lock(&session.inner.link), Link::Pending));
        assert!(!session.closed() && !session.observing());
        assert_eq!(session.take_error(), None);
        assert_eq!(*lock(&session.inner.size), None);
    }
    #[test]
    fn claiming_a_terminal_not_yet_sized_attaches_nothing() {
        let runtime = Arc::new(crate::runtime::pumid::Pumid::at(pumi_proto::paths::Paths::at("/nonexistent")));
        let (session, _changes) = TerminalSession::new("t1", runtime);
        session.claim();
        assert!(matches!(*lock(&session.inner.link), Link::Pending));
    }
    #[test]
    fn takeover_is_told_apart_from_the_terminal_closing() {
        let (shared, _changes) = shared(1);
        assert_eq!(
            pump([TerminalOutput::Ended { taken_over: true }], SIZE, &shared, 1, false, &|_| {}),
            End::TakenOver
        );
        assert_eq!(pump([TerminalOutput::Ended { taken_over: false }], SIZE, &shared, 1, false, &|_| {}), End::Closed);
        assert_eq!(pump([], SIZE, &shared, 1, false, &|_| {}), End::Closed);
    }
    #[test]
    fn stale_stream_publishes_nothing() {
        let (shared, _changes) = shared(2);
        pump([frame(10, 3, "stale")], SIZE, &shared, 1, false, &|_| {});
        assert!(shared.read().rows.is_empty());
    }
    #[test]
    fn a_snapshot_starts_over_and_raw_bytes_follow() {
        let (shared, _changes) = shared(1);
        let stream = [
            frame(10, 3, "old"),
            TerminalOutput::Snapshot { width: 12, height: 2, bytes: b"\x1b[?1hfresh".to_vec(), more: false },
            TerminalOutput::Frame { width: 0, height: 0, bytes: b"!".to_vec(), more: false },
        ];
        pump(stream, SIZE, &shared, 1, false, &|_| {});
        let grid = shared.read();
        assert_eq!(grid.cols, 12);
        assert!(text(&grid.rows[0]).starts_with("fresh!"), "{:?}", text(&grid.rows[0]));
        let local = lock(&shared.local);
        let local = local.as_ref().unwrap();
        assert!(local.terminal.dec_mode(1), "the program's DECCKM is ours");
    }
    #[test]
    fn the_local_terminal_answers_queries_but_not_in_a_snapshot() {
        let (shared, _changes) = shared(1);
        let answered = std::cell::RefCell::new(Vec::new());
        let stream = [
            TerminalOutput::Snapshot { width: 10, height: 3, bytes: b"\x1b[6n".to_vec(), more: false },
            frame(10, 3, "ab\x1b[6n"),
        ];
        pump(stream, SIZE, &shared, 1, false, &|bytes| answered.borrow_mut().extend(bytes));
        assert_eq!(answered.into_inner(), b"\x1b[1;3R", "DSR answered once, for the live query");
    }
    #[test]
    fn a_held_screen_is_looked_at_again_without_new_output() {
        let (shared, _changes) = shared(1);
        pump([frame(10, 3, "before")], SIZE, &shared, 1, false, &|_| {});
        assert!(text(&shared.read().rows[0]).starts_with("before"));
        pump([frame(10, 3, "\x1b[?2026h\rafter!")], SIZE, &shared, 1, true, &|_| {});
        assert!(text(&shared.read().rows[0]).starts_with("before"), "held");
        assert!(shared.held.load(Ordering::Acquire));
        pump([frame(10, 3, "\x1b[?2026l")], SIZE, &shared, 1, true, &|_| {});
        shared.stale.store(false, Ordering::Release);
        assert!(text(&shared.read().rows[0]).starts_with("after!"), "read again though nothing said stale");
        assert!(!shared.held.load(Ordering::Acquire));
    }
    #[test]
    fn a_scroll_asked_for_while_the_terminal_is_busy_is_applied_once_it_is_let_go() {
        let (shared, mut changes) = shared(1);
        let lines: String = (0..30).map(|i| format!("line {i}\r\n")).collect();
        pump([frame(10, 3, &lines)], SIZE, &shared, 1, false, &|_| {});
        while changes.try_recv().is_ok() {}
        let bottom = shared.read();
        assert!(shared.modes.lock().unwrap().as_ref().is_some_and(|modes| !modes.scrolled_back));
        let busy = lock(&shared.local);
        shared.scroll.fetch_add(-2, Ordering::SeqCst);
        assert!(!shared.refresh(), "busy: left to whoever holds it");
        drop(busy);
        shared.settle();
        assert!(changes.try_recv().is_ok(), "told");
        assert_ne!(shared.read().rows, bottom.rows, "scrolled");
        assert!(shared.modes.lock().unwrap().as_ref().is_some_and(|modes| modes.scrolled_back));
    }
    #[test]
    fn a_burst_draws_once() {
        let (shared, mut changes) = shared(1);
        let burst = [
            TerminalOutput::Frame { width: 10, height: 3, bytes: b"a".to_vec(), more: true },
            TerminalOutput::Frame { width: 10, height: 3, bytes: b"b".to_vec(), more: false },
        ];
        pump(burst, SIZE, &shared, 1, false, &|_| {});
        assert!(changes.try_recv().is_ok());
        assert!(changes.try_recv().is_err());
        assert!(text(&shared.read().rows[0]).starts_with("ab"));
    }
}
