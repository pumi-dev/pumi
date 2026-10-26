pub mod feed;
pub mod modes;
pub mod ring;
pub mod screen;
use modes::{Modes, Seen, Tracker};
use pumi_proto::native::{AttachServer, Mode, Size};
pub use pumi_sys::pty::Spawn;
use pumi_sys::pty::{self, Pty, Signal, handoff};
use ring::Ring;
use serde::{Deserialize, Serialize};
use std::io::Read as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;
pub const SNAPSHOT_BYTES: usize = 1 << 20;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    Changed(u64),
    Exited(u64),
    Osc {
        tab: u64,
        code: u32,
        data: String,
    },
    Bell(u64),
    Resized {
        tab: u64,
        cols: u16,
        rows: u16,
    },
    /// A line for a channel: a frame of a stream it attached (`tab.call(…, "attach")`).
    Stream {
        chan: u64,
        line: String,
    },
    Wake,
}
struct Client {
    id: u64,
    feed: Arc<feed::Feed>,
    mode: Mode,
}
struct Inner {
    ring: Ring,
    tracker: Tracker,
    size: Size,
    title: Option<String>,
    cwd: Option<String>,
    clients: Vec<Client>,
    controller: Option<u64>,
    exited: Option<Option<i32>>,
}
impl Inner {
    fn tail(&self, n: usize) -> Vec<u8> {
        let tail = self.ring.tail(n);
        if self.ring.end() - tail.len() as u64 == 0 {
            return tail;
        }
        tail[modes::safe_start(&tail)..].to_vec()
    }
}
struct Reader {
    stop: pty::Stopper,
    thread: JoinHandle<()>,
}
pub struct Term {
    pub tab: u64,
    pub shell_pid: u32,
    pub tty: Option<String>,
    pty: Pty,
    reader: Mutex<Option<Reader>>,
    inner: Mutex<Inner>,
    dirty: AtomicBool,
    nudging: AtomicBool,
    notes: Sender<Note>,
    next_client: AtomicU64,
}
const NUDGE_HOLD: (Duration, Duration) = (Duration::from_millis(50), Duration::from_millis(500));
const NUDGE_POLL: Duration = Duration::from_millis(10);
const SHELLS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "fish",
    "dash",
    "ksh",
    "mksh",
    "tcsh",
    "csh",
    "nu",
    "elvish",
    "xonsh",
    "pwsh",
    "powershell",
    "cmd",
];
/// `zsh`, `-zsh` (a login shell), `PowerShell.exe`.
fn is_shell(name: &str) -> bool {
    let name = name.trim_start_matches('-').to_ascii_lowercase();
    SHELLS.contains(&name.strip_suffix(".exe").unwrap_or(&name))
}
const READ_BUF: usize = 64 * 1024;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Carried {
    pub tab: u64,
    /// The PTY's number in the daemon that carried it ([`handoff::raw`]).
    pub fd: i32,
    pub pid: u32,
    pub tty: Option<String>,
    pub cols: u16,
    pub rows: u16,
    /// The tab's foreground process group, so a term adopted by a new daemon can still be
    /// signalled: a pty that outlived a restart need not report one, and a tty without one
    /// sends no SIGWINCH on a resize.
    #[serde(default)]
    pub fg: u32,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub modes: Modes,
    pub end: u64,
    pub capacity: usize,
}
fn lock_reader(reader: &Mutex<Option<Reader>>) -> MutexGuard<'_, Option<Reader>> {
    reader.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
impl Term {
    pub fn spawn(
        tab: u64,
        spec: &Spawn,
        capacity: usize,
        history: Option<&[u8]>,
        notes: Sender<Note>,
    ) -> std::io::Result<Arc<Self>> {
        let (pty, reader) = pty::spawn(spec)?;
        let size = Size { cols: spec.cols.max(2), rows: spec.rows.max(2), cell_width: 0, cell_height: 0 };
        let mut ring = Ring::new(capacity);
        if let Some(history) = history {
            ring.push(history);
        }
        let term = Self::start(tab, pty, size, ring, Tracker::default(), notes)?;
        term.start_reader(reader)?;
        Ok(term)
    }
    pub fn adopt(
        carried: &Carried,
        handle: handoff::Handle,
        ring: &[u8],
        notes: Sender<Note>,
    ) -> std::io::Result<Arc<Self>> {
        let (pty, reader) = handoff::adopt(handle, carried.pid, carried.tty.clone(), carried.fg)?;
        let size = Size { cols: carried.cols, rows: carried.rows, cell_width: 0, cell_height: 0 };
        let tracker = Tracker::with(carried.modes.clone());
        let ring = Ring::with(carried.capacity, ring, carried.end);
        let term = Self::start(carried.tab, pty, size, ring, tracker, notes)?;
        {
            let mut inner = lock(&term.inner);
            inner.title = carried.title.clone();
            inner.cwd = carried.cwd.clone();
        }
        term.start_reader(reader)?;
        Ok(term)
    }
    fn start(
        tab: u64,
        pty: Pty,
        size: Size,
        ring: Ring,
        tracker: Tracker,
        notes: Sender<Note>,
    ) -> std::io::Result<Arc<Self>> {
        let term = Arc::new(Term {
            tab,
            shell_pid: pty.pid(),
            tty: pty.tty(),
            pty,
            reader: Mutex::new(None),
            inner: Mutex::new(Inner {
                ring,
                tracker,
                size,
                title: None,
                cwd: None,
                clients: Vec::new(),
                controller: None,
                exited: None,
            }),
            dirty: AtomicBool::new(false),
            nudging: AtomicBool::new(false),
            notes,
            next_client: AtomicU64::new(1),
        });
        let waiter = term.clone();
        let pid = term.shell_pid;
        thread::Builder::new().name(format!("pumid-wait-{tab}")).spawn(move || waiter.exit(pty::wait(pid)))?;
        Ok(term)
    }
    fn start_reader(self: &Arc<Self>, mut reader: pty::Reader) -> std::io::Result<()> {
        let stop = reader.stopper();
        let output = self.clone();
        let thread = thread::Builder::new().name(format!("pumid-read-{}", self.tab)).spawn(move || {
            let mut buf = vec![0u8; READ_BUF];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => output.output(&buf[..n]),
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => return,
                }
            }
        })?;
        *lock_reader(&self.reader) = Some(Reader { stop, thread });
        Ok(())
    }
    pub fn pause_reader(&self) {
        if let Some(reader) = self.begin_pause() {
            let _ = reader.join();
        }
    }
    pub fn begin_pause(&self) -> Option<JoinHandle<()>> {
        let reader = lock_reader(&self.reader).take()?;
        reader.stop.stop();
        Some(reader.thread)
    }
    pub fn carry(&self) -> (Carried, Vec<u8>) {
        let inner = lock(&self.inner);
        let carried = Carried {
            tab: self.tab,
            fd: handoff::raw(&self.pty),
            pid: self.shell_pid,
            tty: self.tty.clone(),
            cols: inner.size.cols,
            rows: inner.size.rows,
            fg: self.pty.foreground().unwrap_or(0),
            title: inner.title.clone(),
            cwd: inner.cwd.clone(),
            modes: inner.tracker.modes.clone(),
            end: inner.ring.end(),
            capacity: inner.ring.capacity(),
        };
        (carried, inner.ring.tail(inner.ring.len()))
    }
    fn output(&self, bytes: &[u8]) {
        let mut seen = Vec::new();
        {
            let mut inner = lock(&self.inner);
            inner.tracker.feed(bytes, &mut seen);
            inner.ring.push(bytes);
            let seq = inner.ring.end();
            for client in &inner.clients {
                client.feed.output(seq, bytes);
            }
            let answers = inner.controller.is_none();
            for item in &seen {
                match item {
                    Seen::Title(title) => inner.title = Some(title.clone()),
                    Seen::Cwd(raw) => {
                        if let Some(path) = modes::pwd_path(raw) {
                            inner.cwd = Some(path);
                        }
                    }
                    Seen::Query(reply) if answers => {
                        let _ = self.pty.write(reply);
                    }
                    _ => {}
                }
            }
        }
        for item in seen {
            match item {
                Seen::Osc { code, data } => {
                    let _ = self.notes.send(Note::Osc { tab: self.tab, code, data });
                }
                Seen::Bell => {
                    let _ = self.notes.send(Note::Bell(self.tab));
                }
                _ => {}
            }
        }
        if !self.dirty.swap(true, Ordering::AcqRel) {
            let _ = self.notes.send(Note::Changed(self.tab));
        }
    }
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }
    fn exit(&self, status: Option<i32>) {
        {
            let mut inner = lock(&self.inner);
            inner.exited = Some(status);
            broadcast(&inner, AttachServer::Exited { status });
        }
        let _ = self.notes.send(Note::Exited(self.tab));
    }
    pub fn exited(&self) -> Option<Option<i32>> {
        lock(&self.inner).exited
    }
    pub fn write(&self, bytes: &[u8]) -> std::io::Result<()> {
        self.pty.write(bytes)
    }
    pub fn paste(&self, text: &str) -> std::io::Result<()> {
        let bracketed = lock(&self.inner).tracker.modes.bracketed_paste();
        self.write(&paste_bytes(text, bracketed))
    }
    pub fn modes(&self) -> Modes {
        lock(&self.inner).tracker.modes.clone()
    }
    pub fn title(&self) -> Option<String> {
        lock(&self.inner).title.clone()
    }
    pub fn cwd(&self) -> Option<String> {
        lock(&self.inner).cwd.clone()
    }
    pub fn size(&self) -> Size {
        lock(&self.inner).size
    }
    /// The process in the terminal's foreground: its process group, where there are groups
    /// (`caps().process_groups`), else the newest process in its tree.
    pub fn foreground(&self) -> Option<u32> {
        self.pty.foreground()
    }
    /// Sends `signal` to the terminal's foreground program.
    pub fn signal_foreground(&self, signal: Signal) -> std::io::Result<()> {
        self.pty.signal_foreground(signal)
    }
    /// The terminal's last `bytes` of output as its program wrote them, from a line's start
    /// where it can, and with nothing added to end it (`history` adds what resets its modes).
    pub fn written(&self, bytes: usize) -> Vec<u8> {
        lock(&self.inner).tail(bytes)
    }
    pub fn history(&self, bytes: usize) -> Vec<u8> {
        let inner = lock(&self.inner);
        let tail = inner.tail(bytes);
        let mut read = Tracker::default();
        read.feed(&tail, &mut Vec::new());
        let mut out = tail;
        out.extend_from_slice(&read.modes.diff(&Modes::default()));
        out.extend_from_slice(b"\x1b[0m\r\n");
        out
    }
    fn snapshot(&self, inner: &Inner, mode: Mode) -> AttachServer {
        let tail = inner.tail(SNAPSHOT_BYTES);
        let mut read = Tracker::default();
        read.feed(&tail, &mut Vec::new());
        let mut bytes = Vec::with_capacity(tail.len() + 64);
        if inner.tracker.modes.alt && !read.modes.alt {
            bytes.extend_from_slice(b"\x1b[?1049h");
            read.modes.alt = true;
        }
        bytes.extend_from_slice(&tail);
        bytes.extend_from_slice(&read.modes.diff(&inner.tracker.modes));
        if let Some(title) = &inner.title {
            bytes.extend_from_slice(format!("\x1b]2;{title}\x07").as_bytes());
        }
        AttachServer::Snapshot { cols: inner.size.cols, rows: inner.size.rows, bytes, seq: inner.ring.end(), mode }
    }
    pub fn resize(&self, size: Size) {
        let size = Size { cols: size.cols.max(2), rows: size.rows.max(2), ..size };
        let mut inner = lock(&self.inner);
        if inner.size == size {
            return;
        }
        inner.size = size;
        self.set_winsize(size);
        broadcast(&inner, AttachServer::Resized { cols: size.cols, rows: size.rows });
        let _ = self.notes.send(Note::Resized { tab: self.tab, cols: size.cols, rows: size.rows });
    }
    fn set_winsize(&self, size: Size) {
        // `TIOCSWINSZ` signals the tty's foreground process group itself, and a pty adopted from
        // an earlier daemon can have none: then nobody hears the resize, and a program that reads
        // its size once and waits for SIGWINCH (every Node/Bun TUI: pi, opencode) keeps the size
        // it started with while the tab moves on. Tell the groups the kernel will not.
        let signaled = self.pty.foreground();
        let _ = self.pty.resize(
            size.cols,
            size.rows,
            size.cols as u32 * size.cell_width,
            size.rows as u32 * size.cell_height,
        );
        self.pty.winch(signaled);
    }
    fn draws_its_screen(&self, inner: &Inner) -> bool {
        inner.tracker.modes.alt
            || self.pty.foreground().and_then(pumi_sys::process::name).is_some_and(|name| !is_shell(&name))
    }
    fn nudge(self: &Arc<Self>) {
        if self.nudging.swap(true, Ordering::AcqRel) {
            return;
        }
        let term = Arc::clone(self);
        let spawned = thread::Builder::new().name(format!("pumid-nudge-{}", self.tab)).spawn(move || {
            let (size, start) = {
                let inner = lock(&term.inner);
                (inner.size, inner.ring.end())
            };
            term.set_winsize(Size { rows: size.rows.saturating_sub(1).max(1), ..size });
            let began = std::time::Instant::now();
            let mut seen = start;
            loop {
                thread::sleep(NUDGE_POLL);
                let end = lock(&term.inner).ring.end();
                let answered = end != start && end == seen && began.elapsed() >= NUDGE_HOLD.0;
                if answered || began.elapsed() >= NUDGE_HOLD.1 {
                    break;
                }
                seen = end;
            }
            let inner = lock(&term.inner);
            term.set_winsize(inner.size);
            drop(inner);
            term.nudging.store(false, Ordering::Release);
        });
        if spawned.is_err() {
            self.nudging.store(false, Ordering::Release);
        }
    }
    pub fn drop_clients(&self) {
        let mut inner = lock(&self.inner);
        for client in inner.clients.drain(..) {
            client.feed.close();
        }
        inner.controller = None;
    }
    pub fn hang_up(&self) {
        self.pty.signal(Signal::Hup);
    }
    pub fn kill(&self) {
        self.pty.signal(Signal::Kill);
    }
    pub fn attach(
        self: &Arc<Self>,
        mode: Mode,
        size: Size,
        budget: usize,
        resume: Option<u64>,
    ) -> (u64, Arc<feed::Feed>) {
        if mode == Mode::Control {
            self.resize(size);
        }
        let id = self.next_client.fetch_add(1, Ordering::Relaxed);
        let feed = Arc::new(feed::Feed::new(budget));
        let mut inner = lock(&self.inner);
        let missed = resume.and_then(|seq| inner.ring.since(seq));
        let fresh = missed.is_none();
        match missed {
            Some(bytes) => feed.push(AttachServer::Output { seq: inner.ring.end(), bytes }),
            None => feed.push(self.snapshot(&inner, mode)),
        }
        if let Some(status) = inner.exited {
            feed.push(AttachServer::Exited { status });
        }
        inner.clients.push(Client { id, feed: Arc::clone(&feed), mode });
        if mode == Mode::Control {
            self.give_control(&mut inner, id);
        }
        if fresh && inner.exited.is_none() && self.draws_its_screen(&inner) {
            self.nudge();
        }
        (id, feed)
    }
    pub fn resync(&self, id: u64) {
        let inner = lock(&self.inner);
        if let Some(client) = inner.clients.iter().find(|c| c.id == id) {
            client.feed.caught_up(self.snapshot(&inner, client.mode));
        }
    }
    fn give_control(&self, inner: &mut Inner, id: u64) {
        if inner.controller == Some(id) {
            return;
        }
        let previous = inner.controller.replace(id);
        for client in inner.clients.iter_mut() {
            if client.id == id {
                client.mode = Mode::Control;
            } else if Some(client.id) == previous {
                client.mode = Mode::Observe;
                client.feed.push(AttachServer::ControlLost);
            }
        }
    }
    pub fn claim(&self, id: u64) {
        let mut inner = lock(&self.inner);
        if !inner.clients.iter().any(|c| c.id == id) {
            return;
        }
        self.give_control(&mut inner, id);
        if let Some(client) = inner.clients.iter().find(|c| c.id == id) {
            client.feed.push(AttachServer::ControlGained);
        }
    }
    pub fn release(&self, id: u64) {
        let mut inner = lock(&self.inner);
        if inner.controller == Some(id) {
            inner.controller = None;
        }
        if let Some(client) = inner.clients.iter_mut().find(|c| c.id == id) {
            client.mode = Mode::Observe;
        }
    }
    pub fn detach(&self, id: u64) {
        let mut inner = lock(&self.inner);
        for client in inner.clients.iter().filter(|c| c.id == id) {
            client.feed.close();
        }
        inner.clients.retain(|c| c.id != id);
        if inner.controller == Some(id) {
            inner.controller = None;
        }
    }
    pub fn is_controller(&self, id: u64) -> bool {
        lock(&self.inner).controller == Some(id)
    }
    pub fn controller(&self) -> Option<u64> {
        lock(&self.inner).controller
    }
    pub fn client_resize(&self, id: u64, size: Size) {
        if self.is_controller(id) {
            self.resize(size);
        }
    }
    pub fn clients(&self) -> usize {
        lock(&self.inner).clients.len()
    }
    pub fn clear(&self) {
        let mut inner = lock(&self.inner);
        let clear = b"\x1b[H\x1b[2J\x1b[3J";
        inner.ring.clear();
        inner.ring.push(clear);
        let seq = inner.ring.end();
        for client in &inner.clients {
            client.feed.output(seq, clear);
        }
    }
}
fn broadcast(inner: &Inner, msg: AttachServer) {
    for client in &inner.clients {
        client.feed.push(msg.clone());
    }
}
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let clean: String = text.replace("\r\n", "\n").chars().filter(|&c| c != '\x1b').collect();
    if bracketed { format!("\x1b[200~{clean}\x1b[201~").into_bytes() } else { clean.replace('\n', "\r").into_bytes() }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{Receiver, channel};
    use std::time::{Duration, Instant};
    fn shell(script: &str) -> (Arc<Term>, Receiver<Note>) {
        shell_with(script, 1 << 20)
    }
    fn shell_with(script: &str, capacity: usize) -> (Arc<Term>, Receiver<Note>) {
        let (tx, rx) = channel();
        let spec = Spawn {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            argv0: None,
            cwd: "/".into(),
            env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("TERM".into(), "xterm-256color".into())],
            cols: 40,
            rows: 10,
            disclaim: true,
        };
        (Term::spawn(7, &spec, capacity, None, tx).unwrap(), rx)
    }
    fn until(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !ok() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn drain(feed: &feed::Feed, raw: &mut Vec<u8>) {
        if let feed::Take::Messages(msgs) = feed.take(Some(Duration::from_millis(10))) {
            for msg in msgs {
                match msg {
                    AttachServer::Output { bytes, .. } | AttachServer::Snapshot { bytes, .. } => raw.extend(bytes),
                    _ => {}
                }
            }
        }
    }
    #[test]
    fn output_reaches_clients_raw_and_the_title_is_heard() {
        let (term, notes) = shell("printf '\\033]2;hi\\007\\033[>1u'; read x; echo got:$x; sleep 5");
        let size = Size { cols: 40, rows: 10, cell_width: 8, cell_height: 16 };
        let (id, feed) = term.attach(Mode::Control, size, feed::LOCAL_BUDGET, None);
        let mut raw = Vec::new();
        until("the kitty push", || {
            drain(&feed, &mut raw);
            raw.windows(5).any(|w| w == b"\x1b[>1u")
        });
        assert_eq!(term.title().as_deref(), Some("hi"));
        assert_eq!(term.modes().kitty, vec![1]);
        assert!(term.is_controller(id));
        term.write(b"pumi\n").unwrap();
        until("the echo", || {
            drain(&feed, &mut raw);
            String::from_utf8_lossy(&raw).contains("got:pumi")
        });
        let note = std::iter::from_fn(|| notes.recv_timeout(Duration::from_secs(1)).ok())
            .find(|n| !matches!(n, Note::Resized { .. }));
        assert!(matches!(note, Some(Note::Changed(7))), "its controller's size is heard first, then its output");
        term.hang_up();
        until("the exit", || term.exited().is_some());
    }
    #[test]
    fn a_late_client_gets_the_tail_and_the_modes_and_a_returning_one_what_it_missed() {
        let (term, _notes) = shell_with(
            "printf '\\033[?2004h'; i=0; while [ $i -lt 100 ]; do echo filler $i; i=$((i+1)); done; echo second; read x; echo third; sleep 5",
            1024,
        );
        until("the output", || term.history(1 << 20).windows(6).any(|w| w == b"second"));
        let size = Size { cols: 40, rows: 10, cell_width: 8, cell_height: 16 };
        let (id, feed) = term.attach(Mode::Observe, size, feed::LOCAL_BUDGET, None);
        let feed::Take::Messages(msgs) = feed.take(Some(Duration::from_secs(2))) else { panic!("a snapshot") };
        let AttachServer::Snapshot { bytes, seq, .. } = &msgs[0] else { panic!("a snapshot first") };
        let text = String::from_utf8_lossy(bytes);
        assert!(text.contains("second") && !text.contains("filler 1\r") && text.ends_with("\x1b[?2004h"), "{text:?}");
        term.detach(id);
        term.write(b"x\n").unwrap();
        until("third", || term.history(1 << 20).windows(5).any(|w| w == b"third"));
        let (_, feed) = term.attach(Mode::Observe, size, feed::LOCAL_BUDGET, Some(*seq));
        let feed::Take::Messages(msgs) = feed.take(Some(Duration::from_secs(2))) else { panic!("output") };
        let AttachServer::Output { bytes, .. } = &msgs[0] else { panic!("what it missed: {msgs:?}") };
        let text = String::from_utf8_lossy(bytes);
        assert!(text.contains("third") && !text.contains("second"), "{text:?}");
        term.kill();
    }
    #[test]
    fn a_second_controller_takes_over_and_the_first_is_told() {
        let (term, _notes) = shell("sleep 5");
        let size = Size { cols: 40, rows: 10, cell_width: 1, cell_height: 1 };
        let (first, first_rx) = term.attach(Mode::Control, size, feed::LOCAL_BUDGET, None);
        let (second, _second_rx) = term.attach(Mode::Control, Size { cols: 50, ..size }, feed::LOCAL_BUDGET, None);
        assert!(term.is_controller(second) && !term.is_controller(first));
        let lost = std::iter::from_fn(|| match first_rx.take(Some(Duration::from_millis(500))) {
            feed::Take::Messages(msgs) => Some(msgs),
            _ => None,
        })
        .flatten()
        .any(|msg| msg == AttachServer::ControlLost);
        assert!(lost);
        assert_eq!(term.size().cols, 50);
        term.client_resize(first, Size { cols: 60, ..size });
        assert_eq!(term.size().cols, 50, "an observer's size does not apply");
        term.kill();
    }
    #[test]
    fn no_controller_means_the_core_answers_device_attributes() {
        let (term, _notes) =
            shell("stty raw -echo; printf '\\033[c'; dd bs=1 count=9 2>/dev/null | od -c | head -1; sleep 5");
        until("the reply read back", || String::from_utf8_lossy(&term.history(1 << 20)).contains("?   6   2"));
        term.kill();
    }
    #[test]
    fn a_tail_drawn_on_the_alternate_screen_is_read_there() {
        let (term, _notes) = shell_with(
            "printf '\\033[?1049h'; i=0; while [ $i -lt 100 ]; do echo drawn $i; i=$((i+1)); done; sleep 5",
            1024,
        );
        until("the output", || term.history(1 << 20).windows(8).any(|w| w == b"drawn 99"));
        let size = Size { cols: 40, rows: 10, cell_width: 8, cell_height: 16 };
        let (_, feed) = term.attach(Mode::Observe, size, feed::LOCAL_BUDGET, None);
        let feed::Take::Messages(msgs) = feed.take(Some(Duration::from_secs(2))) else { panic!("a snapshot") };
        let AttachServer::Snapshot { bytes, .. } = &msgs[0] else { panic!("a snapshot first") };
        assert!(bytes.starts_with(b"\x1b[?1049h"), "{:?}", String::from_utf8_lossy(bytes));
        assert_eq!(bytes.windows(8).filter(|w| *w == b"\x1b[?1049h").count(), 1, "switched once, first");
        term.kill();
    }
    #[test]
    fn a_nudge_holds_the_smaller_size_until_the_program_sees_it() {
        let (term, _notes) =
            shell("trap 'echo winch $(stty size)' WINCH; printf '\\033[?1049hready\\n'; while :; do sleep 0.03; done");
        until("ready", || term.history(1 << 20).windows(5).any(|w| w == b"ready"));
        let size = Size { cols: 40, rows: 10, cell_width: 8, cell_height: 16 };
        let (_, _feed) = term.attach(Mode::Control, size, feed::LOCAL_BUDGET, None);
        until("both sizes", || {
            let text = String::from_utf8_lossy(&term.history(1 << 20)).into_owned();
            text.find("winch 9 40").is_some_and(|small| text[small..].contains("winch 10 40"))
        });
        term.kill();
    }
    #[test]
    fn shells_are_told_from_programs() {
        assert!(is_shell("zsh") && is_shell("-zsh") && is_shell("bash"));
        assert!(is_shell("pwsh") && is_shell("PowerShell.exe") && is_shell("cmd.exe"));
        assert!(!is_shell("node") && !is_shell("claude") && !is_shell("nvim"));
    }
    #[test]
    fn pastes() {
        assert_eq!(paste_bytes("a\r\nb\x1b[201~", true), b"\x1b[200~a\nb[201~\x1b[201~");
        assert_eq!(paste_bytes("a\nb", false), b"a\rb");
    }
    /// The pty a new daemon adopts can report no foreground group at all, so `TIOCSWINSZ` wakes
    /// nobody. The groups the tab had are carried with it, and the resize is said out loud.
    #[test]
    fn an_adopted_term_still_wakes_the_program_on_a_resize() {
        let (term, _notes) =
            shell("trap 'echo winch $(stty size)' WINCH; printf '\\033[?1049hready\\n'; while :; do sleep 0.03; done");
        until("ready", || term.history(1 << 20).windows(5).any(|w| w == b"ready"));
        let (carried, ring) = term.carry();
        assert!(carried.fg > 0, "the foreground group is carried: {carried:?}");
        let handle = handoff::Handle::duplicate(carried.fd).unwrap();
        let _paused = term.begin_pause();
        let (tx, _rx) = channel();
        let adopted = Term::adopt(&carried, handle, &ring, tx).unwrap();
        adopted.resize(Size { cols: 50, rows: 12, cell_width: 8, cell_height: 16 });
        until("the resize after the adoption", || {
            String::from_utf8_lossy(&adopted.history(1 << 20)).contains("winch 12 50")
        });
        adopted.kill();
    }
    /// The fallback `set_winsize` takes when the tty reports no foreground group: the groups the
    /// tab kept are told directly, so a program that waits for SIGWINCH still hears the resize.
    #[test]
    fn a_resize_tells_the_groups_the_kernel_did_not() {
        let (term, _notes) =
            shell("trap 'echo winch $(stty size)' WINCH; printf 'ready\\n'; while :; do sleep 0.03; done");
        until("ready", || term.history(1 << 20).windows(5).any(|w| w == b"ready"));
        term.pty.winch(None);
        until("the resize said out loud", || term.history(1 << 20).windows(5).any(|w| w == b"winch"));
        term.kill();
    }
    /// A handoff written without `fg` still reads.
    #[test]
    fn a_carried_terminal_from_an_older_daemon_still_reads() {
        let (term, _notes) = shell("sleep 5");
        let (carried, _ring) = term.carry();
        let mut value = serde_json::to_value(&carried).unwrap();
        assert!(value.as_object_mut().is_some_and(|object| object.remove("fg").is_some()));
        let older: Carried = serde_json::from_value(value).unwrap();
        assert_eq!(older.fg, 0, "no group to tell, so the tab's own process group is used");
        term.kill();
    }
}
