//! Unix domain sockets, owner-only (0600).
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn address(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

pub struct Listener(UnixListener);

impl Listener {
    pub fn bind(path: &Path) -> io::Result<Listener> {
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(io::Error::new(io::ErrorKind::AddrInUse, format!("{} is in use", path.display())));
            }
            std::fs::remove_file(path)?;
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Listener(listener))
    }
    pub fn accept(&self) -> io::Result<Stream> {
        self.0.accept().map(|(stream, _)| Stream(stream))
    }
}

pub struct Stream(UnixStream);

impl Stream {
    pub fn connect(path: &Path) -> io::Result<Stream> {
        UnixStream::connect(path).map(Stream)
    }
    pub fn try_clone(&self) -> io::Result<Stream> {
        self.0.try_clone().map(Stream)
    }
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(timeout)
    }
    pub fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_write_timeout(timeout)
    }
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
