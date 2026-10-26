//! Windows: named pipes, `\\.\pipe\pumi-<hash>-<name>`, open to their owner only and to local
//! clients only. interprocess makes the listener and connects; reads and writes are this file's
//! own overlapped I/O, so they can time out and a shutdown can cancel them.
use crate::win32::{self, Handle, ToIo};
use interprocess::ConnectWaitMode;
use interprocess::os::windows::named_pipe::pipe_mode::Bytes;
use interprocess::os::windows::named_pipe::{DuplexPipeStream, PipeListener, PipeListenerOptions};
use interprocess::os::windows::security_descriptor::{
    AsSecurityDescriptorExt, BorrowedSecurityDescriptor, SecurityDescriptor,
};
use std::io;
use std::os::windows::io::OwnedHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_IO_PENDING, ERROR_OPERATION_ABORTED, ERROR_PIPE_BUSY,
    ERROR_PIPE_NOT_CONNECTED, HLOCAL, LocalFree, WAIT_TIMEOUT,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::PSECURITY_DESCRIPTOR;
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObject};
use windows::core::PCWSTR;

/// How long a connection waits for the server to have an instance free: it always has one,
/// but for a moment between two accepts.
const CONNECT_WAIT: Duration = Duration::from_secs(5);
/// Each direction's buffer: how much can be written before the other end reads.
const BUFFER: u32 = 64 << 10;

/// The pipe's name: the data directory's hash keeps two data directories (pumi and pumi-dev,
/// a test's) apart, as their socket files are on Unix.
pub fn address(dir: &Path, name: &str) -> PathBuf {
    // FNV-1a over the lower-cased directory: stable across runs and builds.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in dir.to_string_lossy().to_lowercase().bytes() {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    PathBuf::from(format!(r"\\.\pipe\pumi-{hash:016x}-{name}"))
}

/// Full access for this user and SYSTEM, nobody else; not inherited from the pipe's parent.
fn owner_only() -> io::Result<SecurityDescriptor> {
    let sddl = win32::wide(format!("D:P(A;;GA;;;{})(A;;GA;;;SY)", win32::user_sid()?));
    let mut descriptor = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .io()?;
    // interprocess wants an absolute descriptor of its own; the string's is self-relative.
    let owned = unsafe { BorrowedSecurityDescriptor::from_ptr(descriptor.0) }.to_owned_sd();
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    owned
}

pub struct Listener(PipeListener<Bytes, Bytes>);

impl Listener {
    pub fn bind(path: &Path) -> io::Result<Listener> {
        // The first instance is made with FILE_FLAG_FIRST_PIPE_INSTANCE, which fails if the name
        // is served already, by anyone: then a client can connect there. A pipe ends with the
        // process that served it, so none is ever stale.
        PipeListenerOptions::new()
            .path(path)
            .security_descriptor(Some(owner_only()?))
            .accept_remote(false)
            .input_buffer_size_hint(BUFFER)
            .output_buffer_size_hint(BUFFER)
            .create_duplex::<Bytes>()
            .map(Listener)
            .map_err(|err| {
                if win32::is(&err, ERROR_ACCESS_DENIED) || win32::is(&err, ERROR_PIPE_BUSY) {
                    io::Error::new(io::ErrorKind::AddrInUse, format!("{} is in use", path.display()))
                } else {
                    err
                }
            })
    }
    pub fn accept(&self) -> io::Result<Stream> {
        Stream::from_pipe(self.0.accept()?)
    }
}

/// A connection. Its clones share one handle, and with it the timeouts and what was shut down,
/// as a socket's clones share the socket on Unix.
pub struct Stream(Arc<Pipe>);

struct Pipe {
    handle: Handle,
    read_timeout: Mutex<Option<Duration>>,
    write_timeout: Mutex<Option<Duration>>,
    reads_shut: AtomicBool,
    writes_shut: AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Stream {
    fn from_pipe(stream: DuplexPipeStream<Bytes>) -> io::Result<Stream> {
        let handle = OwnedHandle::try_from(stream).map_err(|_| io::Error::other("a pipe stream that is split"))?;
        Ok(Stream(Arc::new(Pipe {
            handle: handle.into(),
            read_timeout: Mutex::new(None),
            write_timeout: Mutex::new(None),
            reads_shut: AtomicBool::new(false),
            writes_shut: AtomicBool::new(false),
        })))
    }
    pub fn connect(path: &Path) -> io::Result<Stream> {
        let wait = ConnectWaitMode::Timeout(CONNECT_WAIT);
        Stream::from_pipe(DuplexPipeStream::<Bytes>::connect_by_path_with_wait_mode(path, wait)?)
    }
    pub fn try_clone(&self) -> io::Result<Stream> {
        Ok(Stream(Arc::clone(&self.0)))
    }
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        *lock(&self.0.read_timeout) = nonzero(timeout)?;
        Ok(())
    }
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        *lock(&self.0.write_timeout) = nonzero(timeout)?;
        Ok(())
    }
    /// Shuts the directions for every clone and cancels what they wait for: a blocked read
    /// returns 0, a blocked write fails `BrokenPipe`. A pipe has no half-close, so the other end
    /// hears the end only when the last clone is dropped and the handle closes.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        use std::net::Shutdown;
        if matches!(how, Shutdown::Read | Shutdown::Both) {
            self.0.reads_shut.store(true, SeqCst);
        }
        if matches!(how, Shutdown::Write | Shutdown::Both) {
            self.0.writes_shut.store(true, SeqCst);
        }
        // Everything pending on the handle; a cancelled operation of a direction still open
        // starts again.
        let _ = unsafe { CancelIoEx(self.0.handle.get(), None) };
        Ok(())
    }
}

/// A zero timeout is refused, as Unix sockets refuse it.
fn nonzero(timeout: Option<Duration>) -> io::Result<Option<Duration>> {
    if timeout == Some(Duration::ZERO) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a zero timeout"));
    }
    Ok(timeout)
}

impl Pipe {
    fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let len = buf.len().min(u32::MAX as usize);
        loop {
            if self.reads_shut.load(SeqCst) {
                return Ok(0);
            }
            let timeout = *lock(&self.read_timeout);
            let buf = &mut buf[..len];
            let read = |overlapped: *mut OVERLAPPED| unsafe { ReadFile(self.handle.get(), Some(buf), None, Some(overlapped)) };
            match self.transfer(timeout, &self.reads_shut, read) {
                // The other end closed.
                Err(err) if win32::is(&err, ERROR_BROKEN_PIPE) || win32::is(&err, ERROR_PIPE_NOT_CONNECTED) => {
                    return Ok(0);
                }
                Err(err) if win32::is(&err, ERROR_OPERATION_ABORTED) => continue,
                result => return result,
            }
        }
    }
    fn write(&self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let buf = &buf[..buf.len().min(u32::MAX as usize)];
        loop {
            if self.writes_shut.load(SeqCst) {
                return Err(io::ErrorKind::BrokenPipe.into());
            }
            let timeout = *lock(&self.write_timeout);
            let write = |overlapped: *mut OVERLAPPED| unsafe { WriteFile(self.handle.get(), Some(buf), None, Some(overlapped)) };
            match self.transfer(timeout, &self.writes_shut, write) {
                Err(err) if win32::is(&err, ERROR_OPERATION_ABORTED) => continue,
                result => return result,
            }
        }
    }
    /// One overlapped read or write, which `start` begins: waited for `timeout` at most (then
    /// cancelled, failing `TimedOut`), and cancelled if `shut` is set, failing
    /// `ERROR_OPERATION_ABORTED`. Returns once the handle is done with the buffer.
    fn transfer(
        &self,
        timeout: Option<Duration>,
        shut: &AtomicBool,
        start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
    ) -> io::Result<usize> {
        let event = Handle::new(unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.io()?);
        let mut overlapped = OVERLAPPED { hEvent: event.get(), ..Default::default() };
        if let Err(err) = start(&raw mut overlapped).io()
            && !win32::is(&err, ERROR_IO_PENDING)
        {
            return Err(err);
        }
        // Checked after the operation began: a shutdown that came before this check has its
        // cancel answered here, one after it finds the operation pending and cancels it.
        if shut.load(SeqCst) {
            let _ = unsafe { CancelIoEx(self.handle.get(), Some(&raw const overlapped)) };
        }
        let millis = timeout.map_or(INFINITE, |t| t.as_millis().clamp(1, (INFINITE - 1) as u128) as u32);
        let timed_out = unsafe { WaitForSingleObject(event.get(), millis) } == WAIT_TIMEOUT;
        if timed_out {
            let _ = unsafe { CancelIoEx(self.handle.get(), Some(&raw const overlapped)) };
        }
        let mut done = 0;
        match unsafe { GetOverlappedResult(self.handle.get(), &raw const overlapped, &mut done, true) }.io() {
            Ok(()) => Ok(done as usize),
            // Cancelled part way: what went through.
            Err(_) if done > 0 => Ok(done as usize),
            Err(err) if timed_out && win32::is(&err, ERROR_OPERATION_ABORTED) => Err(io::ErrorKind::TimedOut.into()),
            Err(err) => Err(err),
        }
    }
}

impl io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}
impl io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    /// Nothing waits in this process: a write returns once the pipe has the bytes.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl io::Read for &Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}
impl io::Write for &Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
