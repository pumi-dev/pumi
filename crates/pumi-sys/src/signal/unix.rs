//! Unix: `SIG_IGN` for `SIGPIPE`/`SIGHUP`; `SIGTERM`/`SIGINT` blocked and `sigwait`ed.

pub fn ignore_hangups() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
    }
}

pub fn end_on_closed_pipe() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

pub fn on_shutdown(then: Box<dyn FnOnce() + Send>) {
    let mut set: libc::sigset_t = unsafe { std::mem::zeroed() };
    unsafe {
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
    let _ = std::thread::Builder::new().name("pumi-signals".into()).spawn(move || {
        let mut signal = 0;
        unsafe { libc::sigwait(&set, &mut signal) };
        then();
    });
}
