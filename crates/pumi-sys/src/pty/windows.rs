//! Windows: a ConPTY pseudo console, the program in a job object of its own.
use super::{Signal, Spawn};
use crate::win32::{self, ToIo};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use windows::Win32::Foundation::{
    ERROR_BROKEN_PIPE, ERROR_OPERATION_ABORTED, ERROR_PIPE_NOT_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{COORD, ClosePseudoConsole, CreatePseudoConsole, HPCON, ResizePseudoConsole};
use windows::Win32::System::IO::CancelIoEx;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    TerminateJobObject,
};
use windows::Win32::System::Pipes::{CreatePipe, PeekNamedPipe};
use windows::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute,
};
use windows::core::{PCWSTR, PWSTR};

/// The exit code of a process `TERM` or `KILL` ended.
const KILLED: u32 = 1;
/// Ctrl+C, typed: what the console makes an interrupt of.
const CTRL_C: &[u8] = b"\x03";
/// Each pipe's buffer.
const PIPE_BUFFER: u32 = 64 << 10;
/// Read at least this much more in the same read, if it is there already.
const MORE_AT_LEAST: usize = 1024;
/// How many times [`Stopper::stop`] cancels a read that has not begun yet, a millisecond apart.
const STOP_TRIES: u32 = 200;
/// The processes ConPTY starts to host a console: never a terminal's foreground.
const CONSOLE_HOSTS: [&str; 2] = ["conhost.exe", "OpenConsole.exe"];

pub fn use_trampoline(_binary: PathBuf) {}

pub fn tty_exec(_args: impl Iterator<Item = std::ffi::OsString>) -> ! {
    eprintln!("pumid {}: not on Windows", super::TTY_EXEC);
    std::process::exit(127);
}

pub fn open_terminal(_tty: Option<&str>) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).open("CONOUT$")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn spawn(spec: &Spawn) -> io::Result<(Pty, Reader)> {
    let (console_in, input) = pipe()?;
    let (output, console_out) = pipe()?;
    let console = unsafe { CreatePseudoConsole(coord(spec.cols, spec.rows), console_in.get(), console_out.get(), 0) };
    let console = Console(console.io()?);
    // The console holds its own ends; with ours closed, the output ends when the console does.
    drop((console_in, console_out));
    let job = job()?;
    let (process, pid) = start(spec, &console, &job)?;
    let reader = Reader::new(output.duplicate()?);
    let pty = Pty { pid, console: Mutex::new(Some(console)), input: Mutex::new(input), output, job, process };
    Ok((pty, reader))
}

/// An anonymous pipe, (read end, write end), inherited by nobody.
fn pipe() -> io::Result<(win32::Handle, win32::Handle)> {
    let (mut read, mut write) = (HANDLE(std::ptr::null_mut()), HANDLE(std::ptr::null_mut()));
    unsafe { CreatePipe(&mut read, &mut write, None, PIPE_BUFFER) }.io()?;
    Ok((win32::Handle::new(read), win32::Handle::new(write)))
}

fn coord(cols: u16, rows: u16) -> COORD {
    let cells = |n: u16| n.clamp(1, i16::MAX as u16) as i16;
    COORD { X: cells(cols), Y: cells(rows) }
}

/// A job whose processes all end when its last handle closes.
fn job() -> io::Result<win32::Handle> {
    let job = win32::Handle::new(unsafe { CreateJobObjectW(None, PCWSTR::null()) }.io()?);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let size = size_of_val(&limits) as u32;
    unsafe { SetInformationJobObject(job.get(), JobObjectExtendedLimitInformation, (&raw const limits).cast(), size) }
        .io()?;
    Ok(job)
}

/// Starts `spec`'s program on `console`, suspended until it is in `job`, so that everything it
/// starts is in the job too. Its process handle and id.
fn start(spec: &Spawn, console: &Console, job: &win32::Handle) -> io::Result<(win32::Handle, u32)> {
    let attributes = Attributes::new(console)?;
    let mut info = STARTUPINFOEXW::default();
    info.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    // None of this process's standard handles (a daemon's are its log): the console's.
    info.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    info.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    info.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    info.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    info.lpAttributeList = attributes.list();
    let mut command_line = command_line(&program(spec), &spec.args);
    let environment = environment(&spec.env);
    let cwd_wide = win32::wide(&spec.cwd);
    let cwd = if spec.cwd.as_os_str().is_empty() { PCWSTR::null() } else { PCWSTR(cwd_wide.as_ptr()) };
    let mut created = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED,
            Some(environment.as_ptr().cast()),
            cwd,
            (&raw const info).cast(),
            &mut created,
        )
    }
    .io()?;
    let (process, thread) = (win32::Handle::new(created.hProcess), win32::Handle::new(created.hThread));
    let started = unsafe { AssignProcessToJobObject(job.get(), process.get()) }.io().and_then(|()| {
        if unsafe { ResumeThread(thread.get()) } == u32::MAX { Err(io::Error::last_os_error()) } else { Ok(()) }
    });
    if let Err(err) = started {
        let _ = unsafe { TerminateProcess(process.get(), KILLED) };
        return Err(err);
    }
    Ok((process, created.dwProcessId))
}

/// A thread attribute list naming the pseudo console a process starts on.
struct Attributes(Vec<usize>);

impl Attributes {
    fn new(console: &Console) -> io::Result<Attributes> {
        let mut size = 0;
        // Asked with no list, it fails and says how big one must be.
        let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
        let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(buffer.as_mut_ptr().cast());
        unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut size) }.io()?;
        let attributes = Attributes(buffer);
        // The value is the console's handle itself, not a pointer to it.
        unsafe {
            UpdateProcThreadAttribute(
                attributes.list(),
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(console.0.0 as *const c_void),
                size_of::<HPCON>(),
                None,
                None,
            )
        }
        .io()?;
        Ok(attributes)
    }
    fn list(&self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.0.as_ptr().cast_mut().cast())
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.list()) };
    }
}

/// The program's path: as given if it has a directory; else the first match on the `PATH` of
/// the program's own environment, trying each `PATHEXT` extension if it has none; else as given
/// (CreateProcessW then searches this process's `PATH`).
fn program(spec: &Spawn) -> PathBuf {
    let program = Path::new(&spec.program);
    if program.is_absolute() || program.components().count() > 1 {
        return program.into();
    }
    let var = |name: &str| spec.env.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str());
    let extensions: Vec<&str> = match program.extension() {
        Some(_) => vec![""],
        None => var("PATHEXT").unwrap_or(".COM;.EXE;.BAT;.CMD").split(';').filter(|ext| !ext.is_empty()).collect(),
    };
    std::env::split_paths(var("PATH").unwrap_or_default())
        .flat_map(|dir| extensions.iter().map(move |ext| dir.join(format!("{}{ext}", spec.program))))
        .find(|path| path.is_file())
        .unwrap_or_else(|| program.into())
}

/// `program` and `args` as one command line, quoted so CommandLineToArgvW (and the C runtime)
/// splits it into them again. The program is quoted whole: argv[0] takes no escapes, and a path
/// has no `"`.
fn command_line(program: &Path, args: &[String]) -> Vec<u16> {
    let mut line = format!("\"{}\"", program.display());
    for arg in args {
        line.push(' ');
        quote(arg, &mut line);
    }
    win32::wide(line)
}

/// `arg`, in quotes if it has a space, a tab or a quote (or is empty): within them a `"` is
/// `\"`, and the backslashes before a `"` (or the closing one) are doubled.
fn quote(arg: &str, line: &mut String) {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        line.push_str(arg);
        return;
    }
    line.push('"');
    let mut backslashes = 0;
    for c in arg.chars() {
        match c {
            '\\' => {
                backslashes += 1;
                continue;
            }
            '"' => line.extend(std::iter::repeat_n('\\', backslashes * 2 + 1)),
            _ => line.extend(std::iter::repeat_n('\\', backslashes)),
        }
        backslashes = 0;
        line.push(c);
    }
    line.extend(std::iter::repeat_n('\\', backslashes * 2));
    line.push('"');
}

/// `env` as CreateProcessW takes it: `KEY=value\0` for each, sorted by key regardless of case
/// as Windows keeps it (the last of the same key wins), then one more `\0`.
fn environment(env: &[(String, String)]) -> Vec<u16> {
    let valid = |key: &str, value: &str| {
        !key.is_empty() && !key.chars().skip(1).any(|c| c == '=') && !key.contains('\0') && !value.contains('\0')
    };
    let mut vars = BTreeMap::new();
    for (key, value) in env.iter().rev().filter(|(key, value)| valid(key, value)) {
        vars.entry(key.to_uppercase()).or_insert((key, value));
    }
    let mut block: Vec<u16> = Vec::new();
    for (key, value) in vars.into_values() {
        block.extend(format!("{key}={value}\0").encode_utf16());
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// A pseudo console, closed when dropped.
struct Console(HPCON);

impl Drop for Console {
    /// On a thread of its own: closing can wait until the console's last output is read
    /// (Windows 10), and nobody may be reading.
    fn drop(&mut self) {
        let console = self.0;
        let close = move || unsafe { ClosePseudoConsole(console) };
        if std::thread::Builder::new().name("pumi-conpty-close".into()).spawn(close).is_err() {
            unsafe { ClosePseudoConsole(console) };
        }
    }
}

/// A terminal. Dropped, its console closes (its programs hear `CTRL_CLOSE_EVENT`) and then its
/// job does, ending every process in it.
pub struct Pty {
    pid: u32,
    /// `None` once a hangup closed it.
    console: Mutex<Option<Console>>,
    input: Mutex<win32::Handle>,
    /// The output's read end, which [`Pty::reader`] duplicates.
    output: win32::Handle,
    job: win32::Handle,
    /// Held so the program's id is not another's while the terminal lives.
    process: win32::Handle,
}

impl Pty {
    pub fn pid(&self) -> u32 {
        self.pid
    }
    pub fn tty(&self) -> Option<String> {
        None
    }
    pub fn raw(&self) -> i32 {
        -1
    }
    pub fn reader(&self) -> io::Result<Reader> {
        Ok(Reader::new(self.output.duplicate()?))
    }
    pub fn write(&self, bytes: &[u8]) -> io::Result<()> {
        let input = lock(&self.input);
        let mut rest = bytes;
        while !rest.is_empty() {
            let chunk = &rest[..rest.len().min(u32::MAX as usize)];
            let mut written = 0;
            unsafe { WriteFile(input.get(), Some(chunk), Some(&raw mut written), None) }.io()?;
            rest = &rest[written as usize..];
        }
        Ok(())
    }
    pub fn resize(&self, cols: u16, rows: u16, _width_px: u32, _height_px: u32) -> io::Result<()> {
        match &*lock(&self.console) {
            Some(console) => unsafe { ResizePseudoConsole(console.0, coord(cols, rows)) }.io(),
            None => Ok(()),
        }
    }
    pub fn foreground(&self) -> Option<u32> {
        Some(newest_descendant(self.pid, &self.process).unwrap_or(self.pid))
    }
    pub fn winch(&self, _signaled: Option<u32>) {}
    pub fn signal(&self, signal: Signal) {
        match signal {
            Signal::Hup => drop(lock(&self.console).take()),
            Signal::Term | Signal::Kill | Signal::Quit => {
                let _ = unsafe { TerminateJobObject(self.job.get(), KILLED) };
            }
            Signal::Int => {
                let _ = self.write(CTRL_C);
            }
            Signal::Stop | Signal::Cont | Signal::Winch => {}
        }
    }
    pub fn signal_foreground(&self, signal: Signal) -> io::Result<()> {
        match signal {
            Signal::Int => self.write(CTRL_C),
            Signal::Term | Signal::Kill => {
                let foreground = self.foreground().unwrap_or(self.pid);
                let process = win32::open_process(foreground, PROCESS_TERMINATE)?;
                unsafe { TerminateProcess(process.get(), KILLED) }.io()
            }
            Signal::Stop | Signal::Cont => Err(crate::unsupported("job control")),
            Signal::Hup | Signal::Quit | Signal::Winch => Err(crate::unsupported("this signal to one process")),
        }
    }
}

/// The newest live process started under `root` (its program, held open as `process`) that is
/// not a console host: the one most likely reading what is typed.
fn newest_descendant(root: u32, process: &win32::Handle) -> Option<u32> {
    let processes = win32::processes().ok()?;
    let mut newest: Option<(u64, u32)> = None;
    let mut parents = vec![(root, win32::created(process.get())?)];
    while let Some((parent, parent_created)) = parents.pop() {
        for child in processes.iter().filter(|p| p.parent == parent && p.pid != parent) {
            let opened = win32::open_process(child.pid, PROCESS_QUERY_LIMITED_INFORMATION);
            let Some(created) = opened.ok().and_then(|handle| win32::created(handle.get())) else { continue };
            // Started before its parent: its parent's id was a process's that has ended.
            if created < parent_created {
                continue;
            }
            parents.push((child.pid, created));
            let host = CONSOLE_HOSTS.iter().any(|name| child.exe.eq_ignore_ascii_case(name));
            if !host && newest.is_none_or(|(at, _)| created >= at) {
                newest = Some((created, child.pid));
            }
        }
    }
    newest.map(|(_, pid)| pid)
}

/// The output, read with blocking ReadFiles on a handle of its own.
pub struct Reader(Arc<Output>);

struct Output {
    handle: win32::Handle,
    stopped: AtomicBool,
    /// Set from just before a read looks at `stopped` until its ReadFile returned.
    reading: AtomicBool,
}

/// Stops a [`Reader`]: sets its flag and cancels its read (CancelIoEx reaches a blocking one on
/// another thread). Handles duplicated from one pipe end share its reads, so a cancel can hit
/// another reader's too, which then reads again.
#[derive(Clone)]
pub struct Stopper(Arc<Output>);

impl Stopper {
    /// A read about to block cannot be cancelled before it has: cancel again until it returned.
    pub fn stop(&self) {
        self.0.stopped.store(true, SeqCst);
        for _ in 0..STOP_TRIES {
            if !self.0.reading.load(SeqCst) {
                return;
            }
            let _ = unsafe { CancelIoEx(self.0.handle.get(), None) };
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Reader {
    fn new(handle: win32::Handle) -> Reader {
        Reader(Arc::new(Output { handle, stopped: AtomicBool::new(false), reading: AtomicBool::new(false) }))
    }
    pub fn stopper(&self) -> Stopper {
        Stopper(Arc::clone(&self.0))
    }
    pub fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let n = loop {
            match self.read_once(buf) {
                Err(err) if win32::is(&err, ERROR_OPERATION_ABORTED) => continue,
                // The console closed.
                Err(err) if win32::is(&err, ERROR_BROKEN_PIPE) || win32::is(&err, ERROR_PIPE_NOT_CONNECTED) => {
                    return Ok(0);
                }
                result => break result?,
            }
        };
        Ok(if n == 0 { 0 } else { n + self.read_more(&mut buf[n..]) })
    }
    /// One ReadFile, unless stopped (then 0).
    fn read_once(&self, buf: &mut [u8]) -> io::Result<usize> {
        let output = &self.0;
        output.reading.store(true, SeqCst);
        let result = if output.stopped.load(SeqCst) {
            Ok(0)
        } else {
            let mut read = 0;
            let len = buf.len().min(u32::MAX as usize);
            unsafe { ReadFile(output.handle.get(), Some(&mut buf[..len]), Some(&raw mut read), None) }
                .io()
                .map(|()| read as usize)
        };
        output.reading.store(false, SeqCst);
        result
    }
    /// What else is in the pipe now, so one burst of output is one read.
    fn read_more(&self, buf: &mut [u8]) -> usize {
        let mut got = 0;
        while buf.len() - got >= MORE_AT_LEAST {
            let mut ready = 0;
            let peek = unsafe { PeekNamedPipe(self.0.handle.get(), None, 0, None, Some(&raw mut ready), None) };
            if peek.is_err() || ready == 0 {
                break;
            }
            let want = (ready as usize).min(buf.len() - got);
            match self.read_once(&mut buf[got..got + want]) {
                Ok(0) | Err(_) => break,
                Ok(n) => got += n,
            }
        }
        got
    }
}

pub struct Handle(());

impl Handle {
    pub fn inherited(_raw: i32) -> Option<Handle> {
        None
    }
    pub fn duplicate(_raw: i32) -> io::Result<Handle> {
        Err(crate::unsupported("terminal handoff"))
    }
}

pub fn inherit(_raw: i32, _on: bool) -> io::Result<()> {
    Err(crate::unsupported("terminal handoff"))
}

pub fn adopt(_handle: Handle, _pid: u32, _tty: Option<String>, _fg: u32) -> io::Result<(Pty, Reader)> {
    Err(crate::unsupported("terminal handoff"))
}
