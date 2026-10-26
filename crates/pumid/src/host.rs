use crate::lifecycle::Image;
use crate::server::log;
use pumi_proto::paths::{File, Paths};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sidecar {
    pub name: String,
    pub pid: u32,
    #[serde(default)]
    pub image: Option<Image>,
}
pub type Sidecars = Vec<Sidecar>;
pub type Exited = std::sync::Arc<dyn Fn(u32) + Send + Sync>;
pub struct Start<'a> {
    pub paths: &'a Paths,
    pub host_args: &'a [String],
    pub headless: bool,
    pub bin_dir: Option<&'a std::path::Path>,
    pub exited: &'a Exited,
}
pub fn start(how: &Start) -> Option<Sidecar> {
    let Some(binary) = crate::lifecycle::host_binary() else {
        log("no pumi-host beside the core");
        return None;
    };
    let mut command = Command::new(&binary);
    command.arg("--socket").arg(how.paths.file(File::PumiSocket)).arg("--data-dir").arg(&how.paths.data);
    command.stdin(Stdio::null()).args(how.host_args);
    command.env("PUMI", how.paths.file(File::PumiSocket)).env_remove(crate::handoff::RESTART_AFTER);
    if let Some(bin) = how.bin_dir {
        command.env("PUMI_BIN_DIR", bin);
    }
    pumi_sys::process::reset_signals(&mut command);
    if !how.headless {
        command.stdout(Stdio::null());
    }
    match command.spawn() {
        Ok(child) => {
            let pid = child.id();
            reap(pid, how.exited.clone());
            Some(Sidecar { name: "host".into(), pid, image: Image::of(&binary) })
        }
        Err(err) => {
            log(&format!("cannot start {}: {err}", binary.display()));
            None
        }
    }
}
pub fn adopt(sidecars: Sidecars, how: &Start) -> Sidecars {
    let mut kept = Vec::new();
    for sidecar in sidecars {
        if sidecar.image.as_ref().is_some_and(|image| !image.outdated()) {
            reap(sidecar.pid, how.exited.clone());
            kept.push(sidecar);
            continue;
        }
        log(&format!("pumi-host ({}) is not the binary on disk: starting it again", sidecar.pid));
        pumi_sys::process::stop(sidecar.pid, std::time::Duration::from_secs(3));
    }
    if kept.is_empty() {
        kept.extend(start(how));
    }
    kept
}
fn reap(pid: u32, exited: Exited) {
    let _ = std::thread::Builder::new().name("pumid-sidecar".into()).spawn(move || {
        let status = pumi_sys::process::wait(pid);
        log(&format!("pumi-host ({pid}) exited ({status:?})"));
        exited(pid);
    });
}
