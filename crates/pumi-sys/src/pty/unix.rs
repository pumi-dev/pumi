//! Unix: a PTY pair from `openpty`, the program a session leader on it.
use super::{Signal, Spawn};
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod disclaimed;

static TRAMPOLINE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn use_trampoline(binary: PathBuf) {
    let _ = TRAMPOLINE.set(binary);
}

pub fn tty_exec(mut args: impl Iterator<Item = std::ffi::OsString>) -> ! {
    let (Some(program), Some(argv0)) = (args.next(), args.next()) else {
        eprintln!("pumid {}: needs a program and its argv0", super::TTY_EXEC);
        std::process::exit(127);
    };
    // The session's leader (`POSIX_SPAWN_SETSID`) takes its tty, fd 0.
    unsafe { libc::ioctl(0, libc::TIOCSCTTY as _, 0) };
    let err = Command::new(&program).arg0(argv0).args(args).exec();
    eprintln!("pumid: cannot start {}: {err}", program.to_string_lossy());
    std::process::exit(127);
}

pub fn open_terminal(tty: Option<&str>) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NOCTTY).open(tty.unwrap_or("/dev/tty"))
}

pub struct Pty {
    master: OwnedFd,
    writer: Mutex<File>,
    pid: u32,
    tty: Option<String>,
    /// The foreground process group the tab had when it was carried over a restart: a pty a
    /// new daemon adopts need not report one any more, and a tty with none signals nobody.
    fg: AtomicU32,
}

fn cvt(result: libc::c_int) -> io::Result<libc::c_int> {
    if result == -1 { Err(io::Error::last_os_error()) } else { Ok(result) }
}

fn winsize(cols: u16, rows: u16, width_px: u32, height_px: u32) -> libc::winsize {
    libc::winsize {
        ws_row: rows.max(1),
        ws_col: cols.max(1),
        ws_xpixel: (width_px.min(u16::MAX as u32)) as u16,
        ws_ypixel: (height_px.min(u16::MAX as u32)) as u16,
    }
}

fn set_cloexec(fd: RawFd) -> io::Result<()> {
    inherit(fd, false)
}

pub fn inherit(fd: RawFd, on: bool) -> io::Result<()> {
    let flags = cvt(unsafe { libc::fcntl(fd, libc::F_GETFD) })?;
    let flags = if on { flags & !libc::FD_CLOEXEC } else { flags | libc::FD_CLOEXEC };
    cvt(unsafe { libc::fcntl(fd, libc::F_SETFD, flags) })?;
    Ok(())
}

const RESET_SIGNALS: &[libc::c_int] = &[
    libc::SIGCHLD,
    libc::SIGHUP,
    libc::SIGINT,
    libc::SIGQUIT,
    libc::SIGTERM,
    libc::SIGALRM,
    libc::SIGPIPE,
    libc::SIGTSTP,
    libc::SIGTTIN,
    libc::SIGTTOU,
    libc::SIGWINCH,
];

pub(crate) fn number(signal: Signal) -> libc::c_int {
    match signal {
        Signal::Hup => libc::SIGHUP,
        Signal::Int => libc::SIGINT,
        Signal::Quit => libc::SIGQUIT,
        Signal::Term => libc::SIGTERM,
        Signal::Kill => libc::SIGKILL,
        Signal::Stop => libc::SIGSTOP,
        Signal::Cont => libc::SIGCONT,
        Signal::Winch => libc::SIGWINCH,
    }
}

pub fn spawn(spec: &Spawn) -> io::Result<(Pty, Reader)> {
    let mut master: RawFd = -1;
    let mut slave: RawFd = -1;
    let mut name = [0 as libc::c_char; 128];
    let mut size = winsize(spec.cols, spec.rows, 0, 0);
    cvt(unsafe { libc::openpty(&mut master, &mut slave, name.as_mut_ptr(), std::ptr::null_mut(), &mut size) })?;
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    let slave = unsafe { OwnedFd::from_raw_fd(slave) };
    set_cloexec(master.as_raw_fd())?;
    set_cloexec(slave.as_raw_fd())?;
    let tty =
        unsafe { std::ffi::CStr::from_ptr(name.as_ptr()) }.to_str().ok().map(str::to_string).filter(|s| !s.is_empty());
    #[cfg(target_os = "macos")]
    if spec.disclaim
        && let Some(tty) = &tty
    {
        let pid = disclaimed::spawn(spec, tty)?;
        drop(slave);
        return Pty::with(master, pid, Some(tty.clone()), pid);
    }
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).current_dir(&spec.cwd).env_clear().envs(spec.env.iter().map(|(k, v)| (k, v)));
    if let Some(argv0) = &spec.argv0 {
        command.arg0(argv0);
    }
    let stdio = |fd: &OwnedFd| -> io::Result<Stdio> { Ok(Stdio::from(fd.try_clone()?)) };
    command.stdin(stdio(&slave)?).stdout(stdio(&slave)?).stderr(stdio(&slave)?);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(io::Error::last_os_error());
            }
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            for signal in RESET_SIGNALS {
                libc::signal(*signal, libc::SIG_DFL);
            }
            let mut empty: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut empty);
            libc::sigprocmask(libc::SIG_SETMASK, &empty, std::ptr::null_mut());
            Ok(())
        });
    }
    let child = command.spawn()?;
    drop(slave);
    let pid = child.id();
    Pty::with(master, pid, tty, pid)
}

impl Pty {
    fn with(master: OwnedFd, pid: u32, tty: Option<String>, fg: u32) -> io::Result<(Pty, Reader)> {
        let reader = Reader::new(File::from(master.try_clone()?))?;
        let writer = Mutex::new(File::from(master.try_clone()?));
        Ok((Pty { master, writer, pid, tty, fg: AtomicU32::new(fg) }, reader))
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn tty(&self) -> Option<String> {
        self.tty.clone()
    }
    pub fn raw(&self) -> i32 {
        self.master.as_raw_fd()
    }
    pub fn reader(&self) -> io::Result<Reader> {
        Reader::new(File::from(self.master.try_clone()?))
    }
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut writer = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        writer.write_all(bytes)?;
        writer.flush()
    }
    pub fn resize(&self, cols: u16, rows: u16, width_px: u32, height_px: u32) -> io::Result<()> {
        let size = winsize(cols, rows, width_px, height_px);
        cvt(unsafe { libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size) })?;
        Ok(())
    }
    pub fn foreground(&self) -> Option<u32> {
        let pgrp = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
        if pgrp > 0 {
            self.fg.store(pgrp as u32, Ordering::Relaxed);
        }
        (pgrp > 0).then_some(pgrp as u32)
    }
    /// SIGWINCH the tab's own groups that the kernel did not just signal: the tab's process
    /// group and the one it had when it was carried over a restart, skipping `signaled` (the
    /// tty's foreground group, which `TIOCSWINSZ` has already woken).
    pub fn winch(&self, signaled: Option<u32>) {
        for group in self.groups(signaled) {
            kill_group(group, libc::SIGWINCH);
        }
    }
    /// The tab's process groups, each once, leaving out `skip`.
    pub(crate) fn groups(&self, skip: Option<u32>) -> Vec<u32> {
        let (carried, live) = (self.fg.load(Ordering::Relaxed), self.foreground().unwrap_or(0));
        let mut groups = vec![self.pid, live, carried];
        groups.sort_unstable();
        groups.dedup();
        groups.into_iter().filter(|group| *group > 0 && Some(*group) != skip).collect()
    }
    pub fn signal(&self, signal: Signal) {
        for group in self.groups(None) {
            kill_group(group, number(signal));
        }
    }
    pub fn signal_foreground(&self, signal: Signal) -> io::Result<()> {
        let group = self.foreground().unwrap_or(self.pid);
        cvt(unsafe { libc::kill(-(group as libc::pid_t), number(signal)) }).map(|_| ())
    }
}

fn kill_group(group: u32, signal: libc::c_int) {
    unsafe { libc::kill(-(group as libc::pid_t), signal) };
}

/// The PTY's output, polled together with a pipe [`Stopper`] writes to.
pub struct Reader {
    file: File,
    wake: OwnedFd,
    stop: Stopper,
}

#[derive(Clone)]
pub struct Stopper(Arc<File>);

impl Stopper {
    pub fn stop(&self) {
        let _ = (&*self.0).write_all(b"x");
    }
}

const POLL_MORE_AT_LEAST: usize = 1024;

impl Reader {
    fn new(file: File) -> io::Result<Reader> {
        let mut fds = [0; 2];
        cvt(unsafe { libc::pipe(fds.as_mut_ptr()) })?;
        let (wake, stop) = unsafe { (OwnedFd::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) };
        set_cloexec(wake.as_raw_fd())?;
        set_cloexec(stop.as_raw_fd())?;
        Ok(Reader { file, wake, stop: Stopper(Arc::new(stop)) })
    }
    pub fn stopper(&self) -> Stopper {
        self.stop.clone()
    }
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut fds = [
            libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: self.wake.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        loop {
            if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } == -1 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }
            if fds[1].revents != 0 {
                return Ok(0);
            }
            if fds[0].revents != 0 {
                break;
            }
        }
        let n = match self.file.read(buf) {
            Ok(n) => n,
            // A PTY whose last program exited reads EIO on Linux: that is its end.
            Err(err) if err.raw_os_error() == Some(libc::EIO) => 0,
            Err(err) => return Err(err),
        };
        Ok(if n == 0 { 0 } else { n + self.read_more(&mut buf[n..]) })
    }
    /// What else is ready now, so one burst of output is one read.
    fn read_more(&mut self, buf: &mut [u8]) -> usize {
        let mut got = 0;
        while buf.len() - got >= POLL_MORE_AT_LEAST {
            let mut fd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            if unsafe { libc::poll(&mut fd, 1, 0) } != 1 || fd.revents & libc::POLLIN == 0 {
                break;
            }
            match self.file.read(&mut buf[got..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => got += n,
            }
        }
        got
    }
}

pub struct Handle(OwnedFd);

impl Handle {
    pub fn inherited(raw: i32) -> Option<Handle> {
        if unsafe { libc::fcntl(raw, libc::F_GETFD) } == -1 {
            return None;
        }
        set_cloexec(raw).ok()?;
        Some(Handle(unsafe { OwnedFd::from_raw_fd(raw) }))
    }
    pub fn duplicate(raw: i32) -> io::Result<Handle> {
        let fd = cvt(unsafe { libc::dup(raw) })?;
        set_cloexec(fd)?;
        Ok(Handle(unsafe { OwnedFd::from_raw_fd(fd) }))
    }
}

pub fn adopt(handle: Handle, pid: u32, tty: Option<String>, fg: u32) -> io::Result<(Pty, Reader)> {
    let master = handle.0;
    set_cloexec(master.as_raw_fd())?;
    // Best effort: hand the terminal its foreground group back, so the kernel signals it
    // again on the next resize. A daemon is not in the tab's session, so this can be
    // refused; `signal` still reaches that group from what was carried.
    if fg > 0 {
        unsafe { libc::tcsetpgrp(master.as_raw_fd(), fg as libc::pid_t) };
    }
    Pty::with(master, pid, tty, fg)
}

#[cfg(test)]
mod tests {
    use super::super::{Spawn, spawn, wait};
    use std::io::Read;

    fn run(spec: &Spawn) -> (u32, String) {
        let (pty, mut reader) = spawn(spec).unwrap();
        assert!(pty.tty().is_some_and(|t| t.starts_with("/dev/")));
        let pid = pty.pid();
        let drain = std::thread::spawn(move || {
            let mut out = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                out.extend_from_slice(&buf[..n]);
            }
            out
        });
        assert_eq!(wait(pid), Some(0));
        drop(pty);
        (pid, String::from_utf8_lossy(&drain.join().unwrap()).into_owned())
    }
    fn spec(script: &str, disclaim: bool) -> Spawn {
        Spawn {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            argv0: Some("my-sh".into()),
            cwd: "/usr".into(),
            env: vec![("PATH".into(), "/usr/bin:/bin".into()), ("X".into(), "y".into())],
            cols: 77,
            rows: 13,
            disclaim,
        }
    }
    const SCRIPT: &str =
        "stty size; test -t 0 && echo tty; echo ok > /dev/tty && echo ctty; ps -o args= -p $$; pwd; echo \"x=$X\"";
    fn check(out: &str) {
        for want in ["13 77", "tty", "ctty", "/usr", "x=y", "my-sh"] {
            assert!(out.contains(want), "{want:?} in {out:?}");
        }
    }
    #[test]
    fn a_child_sees_a_terminal_of_the_given_size() {
        check(&run(&spec(SCRIPT, false)).1);
    }
    #[test]
    fn a_disclaimed_child_gets_the_same_terminal() {
        check(&run(&spec(SCRIPT, true)).1);
    }
    #[test]
    fn a_missing_program_is_an_error() {
        for disclaim in [false, true] {
            let mut spec = spec("", disclaim);
            spec.program = "no-such-program-here".into();
            assert!(spawn(&spec).is_err());
        }
    }
    #[test]
    fn a_stopped_reader_returns_and_the_terminal_stays() {
        let (pty, mut reader) = spawn(&spec("sleep 5", false)).unwrap();
        let stopper = reader.stopper();
        let read = std::thread::spawn(move || reader.read(&mut [0u8; 64]).unwrap());
        stopper.stop();
        assert_eq!(read.join().unwrap(), 0);
        pty.signal(super::Signal::Kill);
        wait(pty.pid());
    }
}
