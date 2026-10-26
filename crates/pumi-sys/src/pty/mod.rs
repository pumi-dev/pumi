//! Terminals: a program run on a pseudo-terminal, its output read and its input written.
//!
//! Unix opens a PTY pair and starts the program as a session leader on it (macOS can make it
//! its own responsible process too: [`Spawn::disclaim`]). Windows starts it on a ConPTY pseudo
//! console in a job object, so ending the terminal ends every process it started.
//!
//! A terminal's program hears signals as its OS has them. Where there is no signal, the nearest
//! thing happens: `INT` is a Ctrl+C typed into the terminal, `HUP` closes it, `TERM` and `KILL`
//! end its processes, and `STOP`/`CONT` fail as unsupported (`caps().job_control`).
use std::io;
use std::path::PathBuf;

#[cfg(unix)]
#[path = "unix.rs"]
pub(crate) mod os;
#[cfg(windows)]
#[path = "windows.rs"]
pub(crate) mod os;

pub use crate::process::wait;

/// What to start, and on what size of terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawn {
    pub program: String,
    pub args: Vec<String>,
    /// The name the program sees as its own (`argv[0]`): `-zsh` for a login shell. Unix only.
    pub argv0: Option<String>,
    pub cwd: PathBuf,
    /// The program's whole environment: nothing else is inherited.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// macOS: the program is its own responsible process, so privacy prompts (files, screen
    /// recording, …) name it and pumi's grants do not cover it. Ignored elsewhere.
    pub disclaim: bool,
}

/// A signal, by the name `tab.call(…, "signal")` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Hup,
    Int,
    Quit,
    Term,
    Kill,
    Stop,
    Cont,
    Winch,
}

impl Signal {
    /// `HUP` or `SIGHUP`, any case.
    pub fn parse(name: &str) -> Option<Signal> {
        let name = name.to_ascii_uppercase();
        Some(match name.trim_start_matches("SIG") {
            "HUP" => Signal::Hup,
            "INT" => Signal::Int,
            "QUIT" => Signal::Quit,
            "TERM" => Signal::Term,
            "KILL" => Signal::Kill,
            "STOP" => Signal::Stop,
            "CONT" => Signal::Cont,
            "WINCH" => Signal::Winch,
            _ => return None,
        })
    }
}

/// A running terminal. Dropping it closes the terminal (its program hears a hangup).
pub struct Pty(os::Pty);

/// The terminal's output. A read waits for some, takes what else is ready at once (so a burst
/// is one read), and returns 0 once the terminal closed or [`Stopper::stop`] was called.
pub struct Reader(os::Reader);

/// Ends a [`Reader`]'s reads from another thread, leaving the terminal open.
#[derive(Clone)]
pub struct Stopper(os::Stopper);

/// Starts `spec`'s program on a new terminal.
pub fn spawn(spec: &Spawn) -> io::Result<(Pty, Reader)> {
    os::spawn(spec).map(|(pty, reader)| (Pty(pty), Reader(reader)))
}

impl Pty {
    /// The program's process id.
    pub fn pid(&self) -> u32 {
        self.0.pid()
    }
    /// The terminal device's name (`/dev/ttys004`); `None` on Windows.
    pub fn tty(&self) -> Option<String> {
        self.0.tty()
    }
    /// Types `bytes` into the terminal.
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        self.0.write(bytes)
    }
    /// Gives the terminal a new size, in cells and pixels.
    pub fn resize(&self, cols: u16, rows: u16, width_px: u32, height_px: u32) -> io::Result<()> {
        self.0.resize(cols, rows, width_px, height_px)
    }
    /// The process in the terminal's foreground: its process group on Unix (the group's id),
    /// the newest process in its tree on Windows.
    pub fn foreground(&self) -> Option<u32> {
        self.0.foreground()
    }
    /// Tells the terminal's programs it was resized, leaving out `signaled`, which the resize
    /// itself told (the foreground group, on Unix). Nothing to do on Windows.
    pub fn winch(&self, signaled: Option<u32>) {
        self.0.winch(signaled)
    }
    /// Sends `signal` to every program the terminal holds.
    pub fn signal(&self, signal: Signal) {
        self.0.signal(signal)
    }
    /// Sends `signal` to the terminal's foreground program (what Ctrl+C would reach).
    pub fn signal_foreground(&self, signal: Signal) -> io::Result<()> {
        self.0.signal_foreground(signal)
    }
    /// A second reader of the terminal's output.
    pub fn reader(&self) -> io::Result<Reader> {
        self.0.reader().map(Reader)
    }
}

impl Reader {
    /// What ends this reader's reads.
    pub fn stopper(&self) -> Stopper {
        Stopper(self.0.stopper())
    }
}

impl io::Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Stopper {
    pub fn stop(&self) {
        self.0.stop()
    }
}

/// The terminal this process runs in, to write to it directly (an agent's hook telling pumi
/// its state with an escape sequence, past its own redirected output): `tty` if it names one,
/// else `/dev/tty`, opened without becoming the controlling terminal; the console's screen
/// buffer (`CONOUT$`) on Windows.
pub fn open_terminal(tty: Option<&str>) -> io::Result<std::fs::File> {
    os::open_terminal(tty)
}

/// The argument that makes pumid's binary a tab's first process (macOS, disclaimed): it takes
/// the tty as its controlling terminal and execs the program. `posix_spawn` opens the tty before
/// the new session exists, so without this a program that opens its tty with `O_NOCTTY` (zsh)
/// never has one, and Ctrl+C, Ctrl+Z and Ctrl+\ reach it as bytes instead of signals.
pub const TTY_EXEC: &str = "--tty-exec";

/// Says which binary answers [`TTY_EXEC`] (pumid's own, from `main`); unset, a disclaimed
/// program is spawned directly. Nothing on Windows.
pub fn use_trampoline(binary: PathBuf) {
    os::use_trampoline(binary)
}

/// `pumid --tty-exec PROGRAM ARGV0 ARGS…`: what a disclaimed spawn runs first.
pub fn tty_exec(args: impl Iterator<Item = std::ffi::OsString>) -> ! {
    os::tty_exec(args)
}

/// Terminals across a restart (`caps().handoff`): the server writes down each PTY's number,
/// lets it be inherited, `exec`s the new binary, and the new process adopts them. Unix only;
/// on Windows every function fails as unsupported and the server never calls them.
pub mod handoff {
    use super::{Pty, Reader, os};
    use std::io;

    /// A terminal's PTY this process inherited, owned: dropping it closes it.
    pub struct Handle(pub(super) os::Handle);

    impl Handle {
        /// The PTY numbered `raw`, if this process has it open.
        pub fn inherited(raw: i32) -> Option<Handle> {
            os::Handle::inherited(raw).map(Handle)
        }
        /// A second number for the PTY numbered `raw` (tests that adopt a terminal they keep).
        pub fn duplicate(raw: i32) -> io::Result<Handle> {
            os::Handle::duplicate(raw).map(Handle)
        }
    }

    /// The number `pty` is known by in this process.
    pub fn raw(pty: &Pty) -> i32 {
        pty.0.raw()
    }

    /// Whether the PTY numbered `raw` stays open in the program this process `exec`s.
    pub fn inherit(raw: i32, on: bool) -> io::Result<()> {
        os::inherit(raw, on)
    }

    /// The terminal a restart carried: `pid` its program, `fg` its foreground group then.
    pub fn adopt(handle: Handle, pid: u32, tty: Option<String>, fg: u32) -> io::Result<(Pty, Reader)> {
        os::adopt(handle.0, pid, tty, fg).map(|(pty, reader)| (Pty(pty), Reader(reader)))
    }
}
