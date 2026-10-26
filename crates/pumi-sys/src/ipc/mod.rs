//! Local sockets: how pumi's processes talk (the server's RPC and grid sockets, the host's, the
//! CLI's and the app's connections to them).
//!
//! A socket is named by an *address*, a path: [`address`] makes one from the data directory and
//! a file name. On Unix it is that file, a Unix domain socket. On Windows it is a named pipe,
//! `\\.\pipe\pumi-<hash of the directory>-<name>`, open to its owner only. The address is what
//! goes into `$PUMI` and `--socket`, so anything that can connect to a path connects to it as it
//! is (Bun's `node:net` takes a pipe name too).
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
#[path = "unix.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

/// The address of the socket `name` (`pumi.sock`) in the data directory `dir`.
pub fn address(dir: &Path, name: &str) -> PathBuf {
    os::address(dir, name)
}

/// Whether something answers at `address`.
pub fn answers(address: &Path) -> bool {
    Stream::connect(address).is_ok()
}

/// A listening socket. Dropping it stops listening; on Unix it leaves the file, which the next
/// [`Listener::bind`] replaces.
pub struct Listener(os::Listener);

impl Listener {
    /// Listens at `address`, for this user only. Fails with `AddrInUse` when something already
    /// answers there; a stale socket left by a process that died is replaced.
    pub fn bind(address: &Path) -> io::Result<Listener> {
        os::Listener::bind(address).map(Listener)
    }
    /// Waits for the next connection.
    pub fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(Stream)
    }
    /// Every connection, as `accept` gives them; never ends.
    pub fn incoming(&self) -> impl Iterator<Item = io::Result<Stream>> + '_ {
        std::iter::repeat_with(|| self.accept())
    }
}

/// One connection, both ways. `try_clone` gives a second handle to it (a reader thread's).
pub struct Stream(os::Stream);

impl Stream {
    pub fn connect(address: &Path) -> io::Result<Stream> {
        os::Stream::connect(address).map(Stream)
    }
    pub fn try_clone(&self) -> io::Result<Stream> {
        self.0.try_clone().map(Stream)
    }
    /// How long a read waits before it fails with `WouldBlock` or `TimedOut`; `None` forever.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(timeout)
    }
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(timeout)
    }
    /// Ends the connection for both handles: a read blocked on the other one returns.
    pub fn shutdown(&self, how: std::net::Shutdown) -> io::Result<()> {
        self.0.shutdown(how)
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
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
impl io::Read for &Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&self.0).read(buf)
    }
}
impl io::Write for &Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&self.0).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        (&self.0).flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    #[test]
    fn a_line_goes_there_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let at = address(dir.path(), "t.sock");
        let listener = Listener::bind(&at).unwrap();
        assert!(matches!(Listener::bind(&at), Err(e) if e.kind() == io::ErrorKind::AddrInUse));
        let server = std::thread::spawn(move || {
            let stream = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap()).read_line(&mut line).unwrap();
            (&stream).write_all(line.to_uppercase().as_bytes()).unwrap();
        });
        let mut client = Stream::connect(&at).unwrap();
        client.write_all(b"hello\n").unwrap();
        let mut back = String::new();
        BufReader::new(client.try_clone().unwrap()).read_line(&mut back).unwrap();
        assert_eq!(back, "HELLO\n");
        server.join().unwrap();
    }
}
