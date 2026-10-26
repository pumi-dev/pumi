//! What this OS can do, for callers that pick another way when it can't.

/// What works on this OS. Everything not named here works everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// The server restarts as a new binary keeping its terminals running (`pumid` `exec`s itself
    /// and the new process adopts each terminal's open PTY). Without it a restart saves the
    /// session, and the terminals start again from it.
    pub handoff: bool,
    /// Job control: a terminal's program can be stopped and continued (`STOP`, `CONT`).
    pub job_control: bool,
    /// A terminal reports the process in its foreground by itself (a process group), and a
    /// resize reaches it as `SIGWINCH`. Without it the foreground is the newest process in the
    /// terminal's tree.
    pub process_groups: bool,
    /// A running binary can be replaced on disk (Unix renames over it; Windows keeps it locked).
    pub replace_running: bool,
    /// The user's login environment can be read from their shell (`$SHELL -l -i -c 'env -0'`).
    /// Without it, a terminal starts from this process's environment.
    pub login_env: bool,
}

/// What this OS can do.
pub const fn caps() -> Capabilities {
    if cfg!(windows) {
        Capabilities {
            handoff: false,
            job_control: false,
            process_groups: false,
            replace_running: false,
            login_env: false,
        }
    } else {
        Capabilities { handoff: true, job_control: true, process_groups: true, replace_running: true, login_env: true }
    }
}
