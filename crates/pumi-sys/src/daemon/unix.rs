//! Unix: fork, a new session, the standard streams to the log.
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

pub fn detach(log: &Path) -> io::Result<()> {
    let out = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
    let null = std::fs::File::open("/dev/null")?;
    match unsafe { libc::fork() } {
        -1 => return Err(io::Error::last_os_error()),
        0 => {}
        _ => std::process::exit(0),
    }
    unsafe {
        libc::setsid();
        libc::dup2(null.as_raw_fd(), 0);
        libc::dup2(out.as_raw_fd(), 1);
        libc::dup2(out.as_raw_fd(), 2);
    }
    std::env::set_current_dir("/")
}
