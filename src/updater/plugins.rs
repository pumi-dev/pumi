//! Plugin updates: what `plugin.update` would change, asked every few hours while the
//! `pluginautocheck` option is on (and whenever Settings → Plugins asks). Nothing is applied
//! here: the OS says some have a new version, and Settings updates them.
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use gpui::{App, BorrowAppContext, Global};
use serde_json::{Value, json};

use crate::runtime::Runtime;
use crate::settings::Settings;

/// The first check, after launch has settled and the plugins have started.
const FIRST_CHECK: Duration = Duration::from_secs(60);
const EVERY: Duration = Duration::from_secs(4 * 60 * 60);

/// What the last check found: `plugin.update`'s `[{name, from, to, log}]`, or why it failed.
#[derive(Default)]
pub struct PackUpdates {
    pub found: Option<Result<Vec<Value>, String>>,
    pub checking: bool,
    /// The names the OS was last told of, so the same updates are not announced every check.
    told: BTreeSet<String>,
}

impl Global for PackUpdates {}

pub fn start(cx: &mut App) {
    cx.set_global(PackUpdates::default());
    cx.spawn(async move |cx| {
        cx.background_executor().timer(FIRST_CHECK).await;
        loop {
            if cx.update(|cx| Settings::get(cx).updates.plugins && !crate::safe_mode())
                && let Some(runtime) = crate::engine::runtime()
            {
                cx.update(|cx| check(runtime, false, cx));
            }
            cx.background_executor().timer(EVERY).await;
        }
    })
    .detach();
}

/// Ask which installed plugins have a new version. `manual` is a person asking (Settings);
/// a scheduled check tells the OS of updates it has not told of yet, from the plugin host
/// only, so a second app does not say it again.
pub fn check(runtime: Arc<dyn Runtime>, manual: bool, cx: &mut App) {
    if cx.try_global::<PackUpdates>().is_none_or(|u| u.checking) {
        return;
    }
    cx.update_global::<PackUpdates, _>(|u, _| u.checking = true);
    super::changed(cx);
    cx.spawn(async move |cx| {
        let (found, host) = cx
            .background_executor()
            .spawn(async move {
                // `plugin.update` refreshes a plugin and answers `null`, so nothing says what an
                // update would change: there is no call to ask with. Settings' Update buttons
                // apply it.
                let found: Result<Vec<Value>, String> = Ok(Vec::new());
                let host = !manual && runtime.call("channel.getInfo", json!([0])).is_ok_and(|c| c["primary"] == true);
                (found, host)
            })
            .await;
        cx.update(|cx| {
            let names: BTreeSet<String> = match &found {
                Ok(list) => list.iter().filter_map(|u| u["name"].as_str().map(str::to_string)).collect(),
                Err(_) => BTreeSet::new(),
            };
            let new: Vec<String> = cx.update_global::<PackUpdates, _>(|u, _| {
                let new = names.difference(&u.told).cloned().collect();
                u.checking = false;
                // A failed scheduled check keeps what the last one found.
                if manual || found.is_ok() {
                    u.found = Some(found);
                }
                if host || manual {
                    u.told = names;
                }
                new
            });
            if host && !new.is_empty() {
                crate::system::post(&announcement(&new).group("pumi-plugin-updates"));
            }
            super::changed(cx);
        });
    })
    .detach();
}

/// The updates of `names` were applied (all of them when empty): they are no longer waiting.
pub fn applied(names: &[String], cx: &mut App) {
    if cx.try_global::<PackUpdates>().is_none() {
        return;
    }
    cx.update_global::<PackUpdates, _>(|u, _| {
        if names.is_empty() {
            u.told.clear();
        }
        u.told.retain(|t| !names.contains(t));
        if let Some(Ok(list)) = u.found.as_mut() {
            list.retain(|p| !names.is_empty() && !names.iter().any(|n| p["name"] == n.as_str()));
        }
    });
    super::changed(cx);
}

fn announcement(names: &[String]) -> crate::system::Notification {
    let title = match names.len() {
        1 => format!("{} has an update", names[0]),
        n => format!("{n} plugins have updates"),
    };
    crate::system::Notification::new(title).body(format!("Update them in Settings → Plugins: {}.", names.join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_update_is_named_and_several_are_counted() {
        assert_eq!(announcement(&["git".into()]).title, "git has an update");
        let several = announcement(&["git".into(), "dia".into()]);
        assert_eq!(several.title, "2 plugins have updates");
        assert_eq!(several.body.as_deref(), Some("Update them in Settings → Plugins: git, dia."));
    }
}
