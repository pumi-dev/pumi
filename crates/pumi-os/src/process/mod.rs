//! `process.*`: the machine's processes. `ps` and `lsof` on Unix; on Windows, PowerShell's
//! `Win32_Process`, where a process's directory is not known.
use pumi_api::{Cx, Nullable, Optional, Result, api};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
#[path = "unix.rs"]
mod os;
#[cfg(windows)]
#[path = "windows.rs"]
mod os;

/// What `process.list` takes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListOpts {
    /// The process with this id.
    #[serde(default)]
    pub pid: Optional<u64>,
    /// The processes whose command line contains this.
    #[serde(default)]
    pub command: Optional<String>,
    /// The processes whose directory is this one.
    #[serde(default)]
    pub cwd: Optional<String>,
}
/// One process: its id, its parent's, its processor and memory use, its command line as its
/// arguments, and its directory (when it is known).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u64,
    pub parent_pid: Nullable<u64>,
    /// How much processor it uses: percent of one core, over its whole life (as `ps` reports it).
    pub cpu: f64,
    /// Its resident memory, in bytes.
    pub rss: u64,
    pub command: Vec<String>,
    /// `null` unless a `cwd` was asked for (reading it takes a system call a process), and on
    /// Windows.
    pub cwd: Nullable<String>,
}
/// The processes on this machine, matching the fields given: `pid`, `command` (any part of its
/// command line), and `cwd` (its directory). No fields matches every process.
#[api(
    name = "process.list",
    effect = "read",
    runs_on = "server",
    example(args = "[]"),
    example(args = r#"[{"pid": 1}]"#, desc = "One process; empty when there is none"),
    example(args = r#"[{"cwd": "/src"}]"#, desc = "The ones running in a directory")
)]
pub fn process_list(_cx: &mut Cx, opts: Optional<ListOpts>) -> Result<Vec<ProcessInfo>> {
    let opts = opts.into_option().unwrap_or_default();
    let mut matches = os::processes(opts.cwd.0.is_some())?;
    matches.retain(|process| {
        opts.pid.0.is_none_or(|wanted| wanted == process.pid)
            && opts.command.0.as_ref().is_none_or(|wanted| process.command.join(" ").contains(wanted.as_str()))
            && opts.cwd.0.as_ref().is_none_or(|wanted| process.cwd.0.as_deref() == Some(wanted.as_str()))
    });
    Ok(matches)
}
