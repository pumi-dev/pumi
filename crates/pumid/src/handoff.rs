use crate::host::Sidecars;
use crate::kernel::Kernel;
use crate::server::{Config, log};
use crate::tab::terminal::Terminal;
use crate::term::Carried;
use pumi_sys::pty::{Signal, handoff};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct Adoptable(pub HashMap<u64, (Carried, handoff::Handle, Vec<u8>)>);
pub struct Handoff {
    pub terminals: Adoptable,
    pub sidecars: Sidecars,
}
#[derive(Serialize, Deserialize)]
struct Written {
    terminals: Vec<Carried>,
    sidecars: Sidecars,
}
fn dir(config: &Config) -> PathBuf {
    config.paths.data.join("handoff")
}
pub fn exec(kernel: &mut Kernel, config: &Config, sidecars: Sidecars) -> io::Error {
    let dir = dir(config);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        return err;
    }
    let tabs: Vec<u64> = kernel.model.tabs.keys().copied().collect();
    let terms: Vec<_> =
        tabs.iter().filter_map(|tab| kernel.instance::<Terminal>(*tab).map(|t| t.term.clone())).collect();
    let readers: Vec<_> = terms.iter().filter_map(|term| term.begin_pause()).collect();
    for reader in readers {
        let _ = reader.join();
    }
    let mut terminals = Vec::new();
    for term in terms {
        let tab = term.tab;
        let (carried, ring) = term.carry();
        if let Err(err) = std::fs::write(dir.join(format!("{tab}.ring")), ring) {
            return err;
        }
        if let Err(err) = handoff::inherit(carried.fd, true) {
            return err;
        }
        terminals.push(carried);
    }
    let written = Written { terminals, sidecars };
    let path = dir.join("handoff.json");
    if let Err(err) = std::fs::write(&path, serde_json::to_vec(&written).unwrap_or_default()) {
        return err;
    }
    let exe = match crate::lifecycle::exe() {
        Ok(exe) => exe,
        Err(err) => return err,
    };
    let mut args = args_again();
    args.push("--resume".into());
    args.push(path.display().to_string());
    log(&format!("restarting as {}", exe.display()));
    pumi_sys::process::exec(std::process::Command::new(exe).args(args))
}
/// This process's arguments, for the server that follows it: without `--resume FILE` and
/// `--daemon`.
fn args_again() -> Vec<String> {
    let mut args = Vec::new();
    let mut given = std::env::args().skip(1);
    while let Some(arg) = given.next() {
        match arg.as_str() {
            "--resume" => {
                given.next();
            }
            "--daemon" => {}
            _ => args.push(arg),
        }
    }
    args
}
/// The pid of the server a restarted one follows, in its environment.
pub const RESTART_AFTER: &str = "PUMI_RESTART_AFTER";
/// How long a restarted server waits for the one it follows to end.
const RESTART_WAIT: Duration = Duration::from_secs(10);
/// A restart where terminals cannot be carried (`!caps().handoff`): this binary starts again,
/// detached, and takes over once this process has ended ([`RESTART_AFTER`]). The session was
/// saved; its terminals start again from it.
pub fn relaunch() -> io::Result<()> {
    let exe = crate::lifecycle::exe()?;
    log(&format!("restarting as {} once this process ends", exe.display()));
    std::process::Command::new(exe)
        .args(args_again())
        .arg("--daemon")
        .env(RESTART_AFTER, std::process::id().to_string())
        // Inherited from this detached server, it would tell the next one it is detached already.
        .env_remove("PUMI_DETACHED")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
/// Waits, at most [`RESTART_WAIT`], for the process `pid` to end.
pub fn await_exit(pid: u32) {
    let deadline = Instant::now() + RESTART_WAIT;
    while pumi_sys::process::name(pid).is_some() {
        if Instant::now() > deadline {
            log(&format!("the server before ({pid}) has not ended; starting anyway"));
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
pub fn receive(path: &Path) -> io::Result<Handoff> {
    let written: Written = serde_json::from_slice(&std::fs::read(path)?).map_err(io::Error::other)?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut terminals = Adoptable::default();
    for carried in written.terminals {
        let Some(handle) = handoff::Handle::inherited(carried.fd) else {
            log(&format!("%{}: its PTY did not survive the restart", carried.tab));
            continue;
        };
        let ring = std::fs::read(dir.join(format!("{}.ring", carried.tab))).unwrap_or_default();
        terminals.0.insert(carried.tab, (carried, handle, ring));
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(Handoff { terminals, sidecars: written.sidecars })
}
pub fn forget(kernel: &mut Kernel) {
    let Some(left) = kernel.ext.get_mut::<Adoptable>() else { return };
    for (tab, (carried, handle, _)) in left.0.drain() {
        log(&format!("%{tab} was carried but not restored; hanging up"));
        let _ = pumi_sys::process::signal(carried.pid, Signal::Hup, true);
        drop(handle);
    }
}
