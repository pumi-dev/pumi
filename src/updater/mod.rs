//! Updates, as Zed does them: checked and downloaded in the background, then a
//! "Restart to Update" button in the title bar. Settings reads it (`Engine.updateState`, the
//! `update` event). Quitting instead applies the update too.
//!
//! * [`manifest`]: the signed channel manifest, the keys it must be signed with, versions.
//! * [`fetch`]: getting it, from pumi.dev or from GitHub's API (for a release that is not public).
//! * [`install`]: staging a release and swapping it in after the app exits, with rollback.
//! * [`plugins`]: the same checks for the installed plugins.
//!
//! Only release builds update themselves (`PUMI_UPDATER` is set by
//! scripts/release/build.sh): a `cargo build` or a `scripts/install.sh` copy never does.

pub mod fetch;
pub mod install;
pub mod manifest;
pub mod plugins;

use std::time::Duration;

use gpui::{App, BorrowAppContext, Global};

use crate::settings::Settings;
use install::Staged;

/// Whether this build updates itself.
pub const ENABLED: bool = option_env!("PUMI_UPDATER").is_some();

/// The first check, after launch has settled.
const FIRST_CHECK: Duration = Duration::from_secs(30);
/// Checks after that.
const EVERY: Duration = Duration::from_secs(4 * 60 * 60);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Waiting for the next check.
    Idle,
    Checking,
    Downloading(String),
    /// Downloaded and verified: the next restart, or quit, puts it in place.
    Ready(String),
    UpToDate,
    Failed(String),
    /// This copy cannot update itself, and why: a dev build, or an install it cannot write.
    Unavailable(String),
}

pub struct Updater {
    pub status: Status,
    staged: Option<Staged>,
    busy: bool,
    /// The page of the release found last, with its notes (`Manifest::notes_page`).
    notes: Option<String>,
}

impl Global for Updater {}

impl Updater {
    /// How the updater is, in words (Settings → General shows it).
    pub fn describe(&self) -> String {
        let current = env!("CARGO_PKG_VERSION");
        match &self.status {
            Status::Idle => format!("pumi {current}, {} channel.", fetch::CHANNEL),
            Status::Checking => "Checking for updates…".into(),
            Status::Downloading(version) => format!("Downloading pumi {version}…"),
            Status::Ready(version) => {
                format!(
                    "pumi {version} is ready: restart to update (or it is applied when you quit).{}",
                    terminals_restart()
                )
            }
            Status::UpToDate => format!("pumi {current} is the newest version on the {} channel.", fetch::CHANNEL),
            Status::Failed(err) => format!("Could not update: {err}"),
            Status::Unavailable(why) => format!("pumi {current} does not update itself: {why}."),
        }
    }

    pub fn can_check(&self) -> bool {
        !self.busy && !matches!(self.status, Status::Unavailable(_))
    }
}

pub fn start(cx: &mut App) {
    let status = match (ENABLED, install::installed_app()) {
        (false, _) => Status::Unavailable("it is a development build".into()),
        (true, Err(why)) => Status::Unavailable(why),
        (true, Ok(_)) => Status::Idle,
    };
    let enabled = status == Status::Idle;
    cx.set_global(Updater { status, staged: None, busy: false, notes: None });
    if !enabled {
        return;
    }

    // This version works once it has run a while: a waiting swap script stops waiting, and
    // the copy it replaced is let go.
    cx.spawn(async move |cx| {
        cx.background_executor().timer(install::HEALTHY_AFTER).await;
        cx.background_executor()
            .spawn(async {
                if let Some(from) = install::mark_healthy() {
                    after_update(&from);
                }
            })
            .await;
    })
    .detach();

    cx.spawn(async move |cx| {
        cx.background_executor().timer(FIRST_CHECK).await;
        loop {
            if cx.update(|cx| Settings::get(cx).updates.auto) {
                cx.update(|cx| check(false, cx));
            }
            cx.background_executor().timer(EVERY).await;
        }
    })
    .detach();

    // A plain quit applies a downloaded update, without relaunching. Not where terminals don't
    // survive a server restart (`caps().handoff`): applying it stops the server, so there it waits
    // for the person to choose "Restart to update".
    cx.on_app_quit(|cx| {
        // One async block for both paths: two `async {}` are two types.
        if pumi_sys::caps().handoff
            && let Some(staged) = cx.global_mut::<Updater>().staged.take()
            && let Err(err) = install::swap(&staged, false)
        {
            eprintln!("pumi: {err}");
        }
        async {}
    })
    .detach();
}

/// Look for a newer release now, and download it if there is one. `manual` is a person
/// asking, who is told the outcome; a scheduled check stays quiet unless it finds one.
pub fn check(manual: bool, cx: &mut App) {
    let Some(updater) = cx.try_global::<Updater>() else { return };
    if !updater.can_check() || updater.staged.is_some() {
        return;
    }
    let app = match install::installed_app() {
        Ok(app) => app,
        Err(why) => return set(cx, Status::Unavailable(why), false),
    };
    set(cx, Status::Checking, true);
    cx.spawn(async move |cx| {
        let found = cx
            .background_executor()
            .spawn(async { fetch::manifest(|bytes, sig| manifest::verify(bytes, sig, manifest::PUBLIC_KEYS)) })
            .await;
        let (manifest, origin) = match found {
            Ok(found) => found,
            Err(err) => {
                cx.update(|cx| set(cx, if manual { Status::Failed(err) } else { Status::Idle }, false));
                return;
            }
        };
        let current = env!("CARGO_PKG_VERSION");
        if !manifest::is_newer(&manifest.version, current) || manifest.channel != fetch::CHANNEL {
            cx.update(|cx| set(cx, if manual { Status::UpToDate } else { Status::Idle }, false));
            return;
        }
        let Some(asset) = manifest.assets.get(&manifest::platform()).cloned() else {
            let why = format!("pumi {} has no build for {}", manifest.version, manifest::platform());
            cx.update(|cx| set(cx, if manual { Status::Failed(why) } else { Status::Idle }, false));
            return;
        };
        let version = manifest.version.clone();
        let notes = manifest.notes_page();
        cx.update(|cx| {
            cx.global_mut::<Updater>().notes = Some(notes);
            set(cx, Status::Downloading(version.clone()), true)
        });
        let staged =
            cx.background_executor().spawn(async move { install::stage(&version, &asset, &origin, &app) }).await;
        cx.update(|cx| match staged {
            Ok(staged) => {
                let version = staged.version.clone();
                cx.global_mut::<Updater>().staged = Some(staged);
                // The title bar is the UI theme's, so the OS says it is ready.
                crate::system::post(&crate::system::Notification::new(format!("pumi {version} is ready")).body(
                    format!("Run “Restart to update”, or quit: it is applied either way.{}", terminals_restart()),
                ));
                set(cx, Status::Ready(version), false);
            }
            Err(err) => {
                eprintln!("pumi: update to {} failed: {err}", manifest.version);
                set(cx, Status::Failed(err), false);
            }
        });
    })
    .detach();
}

/// What an update does to the terminals where the server cannot hand them over to its new
/// binary (`pumi_sys::caps().handoff`, Windows): they start again from the saved session.
fn terminals_restart() -> &'static str {
    if pumi_sys::caps().handoff { "" } else { " Terminals restart with it." }
}

/// Runs once, in the first run of a version updated from the older `from`, after it has
/// proven itself ([`install::mark_healthy`]). Nothing to migrate.
fn after_update(from: &semver::Version) {
    let _ = from;
}

/// Quit, put the downloaded version in place, and open it.
pub fn restart(cx: &mut App) {
    let Some(staged) = cx.try_global::<Updater>().and_then(|u| u.staged.clone()) else { return };
    match install::swap(&staged, true) {
        Ok(()) => {
            // The script is waiting for this process to end; the quit hook must not start
            // another one.
            cx.global_mut::<Updater>().staged = None;
            cx.quit();
        }
        Err(err) => set(cx, Status::Failed(err), false),
    }
}

fn set(cx: &mut App, status: Status, busy: bool) {
    cx.update_global::<Updater, _>(|updater, _| {
        updater.status = status;
        updater.busy = busy;
    });
    changed(cx);
}

/// Tells the app's JS the updater's state changed (an `update` event carrying `state`).
pub(crate) fn changed(cx: &App) {
    crate::engine::emit("update", state(cx));
}

/// The updater's state, as Settings shows it: pumi's own (`app`) and the plugins' (`plugins`).
pub fn state(cx: &App) -> serde_json::Value {
    use serde_json::json;
    let current = env!("CARGO_PKG_VERSION");
    let app = match cx.try_global::<Updater>() {
        None => {
            json!({ "status": "unavailable", "current": current, "channel": fetch::CHANNEL, "text": "", "canCheck": false, "busy": false })
        }
        Some(updater) => {
            let (status, version, error) = match &updater.status {
                Status::Idle => ("idle", None, None),
                Status::Checking => ("checking", None, None),
                Status::Downloading(v) => ("downloading", Some(v.clone()), None),
                Status::Ready(v) => ("ready", Some(v.clone()), None),
                Status::UpToDate => ("upToDate", None, None),
                Status::Failed(err) => ("failed", None, Some(err.clone())),
                Status::Unavailable(why) => ("unavailable", None, Some(why.clone())),
            };
            json!({
                "status": status,
                "notes": version.as_ref().and(updater.notes.as_ref()),
                "version": version,
                "error": error,
                "current": current,
                "channel": fetch::CHANNEL,
                "text": updater.describe(),
                "canCheck": updater.can_check(),
                "busy": updater.busy,
            })
        }
    };
    let plugins = match cx.try_global::<plugins::PackUpdates>() {
        None => json!({ "checking": false, "found": null, "error": null }),
        Some(u) => match &u.found {
            None => json!({ "checking": u.checking, "found": null, "error": null }),
            Some(Ok(found)) => json!({ "checking": u.checking, "found": found, "error": null }),
            Some(Err(err)) => json!({ "checking": u.checking, "found": null, "error": err }),
        },
    };
    json!({ "app": app, "plugins": plugins })
}
