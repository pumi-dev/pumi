//! Windows: this binary started again, detached, writing to the log.
use crate::win32;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use windows::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows::Win32::System::Threading::{
    CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW, DETACHED_PROCESS, PROCESS_CREATION_FLAGS,
};

/// In the started copy's environment: it is the detached one.
const DETACHED: &str = "PUMI_DETACHED";

pub fn detach(log: &Path) -> io::Result<()> {
    if std::env::var_os(DETACHED).is_some() {
        // SAFETY: a process detaches first thing, before it starts other threads (as forking
        // requires on Unix). Removed, so what this process starts can detach in turn.
        unsafe { std::env::remove_var(DETACHED) };
        let home = std::env::set_current_dir(crate::paths::home());
        return home.or_else(|_| std::env::set_current_dir(std::env::temp_dir()));
    }
    let start = |flags: PROCESS_CREATION_FLAGS| -> io::Result<()> {
        let out = std::fs::OpenOptions::new().create(true).append(true).open(log)?;
        Command::new(std::env::current_exe()?)
            .args(std::env::args_os().skip(1))
            .env(DETACHED, "1")
            .stdin(Stdio::null())
            .stdout(out.try_clone()?)
            .stderr(out)
            .creation_flags(flags.0)
            .spawn()
            .map(drop)
    };
    let detached = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW;
    // Out of this process's job (a terminal's would end the server with it), if the job lets go.
    match start(detached | CREATE_BREAKAWAY_FROM_JOB) {
        Err(err) if win32::is(&err, ERROR_ACCESS_DENIED) => start(detached)?,
        result => result?,
    }
    std::process::exit(0);
}
