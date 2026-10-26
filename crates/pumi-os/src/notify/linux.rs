use super::{Event, Kind, Notification, body_with_subtitle, carried, deliver};
use pumi_api::{Error, Result};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
#[derive(Default)]
struct Shown {
    server: HashMap<String, u32>,
    waiting: HashMap<String, u32>,
}
fn shown() -> &'static Mutex<Shown> {
    static SHOWN: OnceLock<Mutex<Shown>> = OnceLock::new();
    SHOWN.get_or_init(Default::default)
}
fn lock() -> std::sync::MutexGuard<'static, Shown> {
    shown().lock().unwrap_or_else(|p| p.into_inner())
}
pub(super) fn start() {}
fn args(n: &Notification, replace: Option<u32>) -> Vec<String> {
    let mut args = vec![
        "--app-name=pumi".to_string(),
        format!("--urgency={}", n.urgency.name()),
        "--print-id".into(),
        "--wait".into(),
        "--action=default=Open".into(),
    ];
    args.extend(n.actions.iter().map(|a| format!("--action={}={}", a.id, a.label)));
    if let Some(id) = replace {
        args.push(format!("--replace-id={id}"));
    }
    if let Some(image) = &n.image {
        args.push(format!("--icon={image}"));
    }
    match n.sound.as_deref() {
        None | Some("") => {}
        Some("default") => args.push("--hint=string:sound-name:message-new-instant".into()),
        Some(name) if name.contains('/') => args.push(format!("--hint=string:sound-file:{name}")),
        Some(name) => args.push(format!("--hint=string:sound-name:{name}")),
    }
    args.push("--".into());
    args.push(n.title.clone());
    args.push(body_with_subtitle(n));
    args
}
pub(super) fn send(id: &str, n: &Notification) -> Result<()> {
    let replace = lock().server.get(id).copied();
    let mut child = Command::new("notify-send")
        .args(args(n, replace))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| Error::new(pumi_api::ErrorKind::ProviderUnavailable, format!("notify-send: {err}")))?;
    let pid = child.id();
    if let Some(old) = lock().waiting.insert(id.to_string(), pid) {
        unsafe { libc::kill(old as libc::pid_t, libc::SIGTERM) };
    }
    let stdout = child.stdout.take();
    let (id, carried) = (id.to_string(), carried(n));
    std::thread::spawn(move || {
        let mut acted = false;
        for (i, line) in
            stdout.map(BufReader::new).into_iter().flat_map(|r| r.lines()).map_while(|l| l.ok()).enumerate()
        {
            let line = line.trim().to_string();
            if i == 0 {
                if let Ok(server) = line.parse() {
                    lock().server.insert(id.clone(), server);
                }
                continue;
            }
            if line.is_empty() {
                continue;
            }
            acted = true;
            let mut event = Event::new(if line == "default" { Kind::Clicked } else { Kind::Action }, &id, &carried);
            if line != "default" {
                event.action = Some(line);
            }
            deliver(event);
        }
        let _ = child.wait();
        let mut shown = lock();
        if shown.waiting.get(&id) != Some(&pid) {
            return;
        }
        shown.waiting.remove(&id);
        shown.server.remove(&id);
        drop(shown);
        if !acted {
            deliver(Event::new(Kind::Dismissed, &id, &carried));
        }
    });
    Ok(())
}
pub(super) fn withdraw(id: &str) -> bool {
    let Some(server) = lock().server.get(id).copied() else { return false };
    let argument = format!("uint32:{server}");
    let ok = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.Notifications",
            "--object-path",
            "/org/freedesktop/Notifications",
            "--method",
            "org.freedesktop.Notifications.CloseNotification",
            &argument,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    ok
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::{Action, Urgency};
    #[test]
    fn a_notification_is_arguments() {
        let n = Notification {
            actions: vec![Action { id: "yes".into(), label: "Deploy".into() }],
            ..Notification::new("Build").subtitle("main").body("It passed").urgency(Urgency::Critical).sound("default")
        };
        assert_eq!(
            args(&n, Some(7)),
            [
                "--app-name=pumi",
                "--urgency=critical",
                "--print-id",
                "--wait",
                "--action=default=Open",
                "--action=yes=Deploy",
                "--replace-id=7",
                "--hint=string:sound-name:message-new-instant",
                "--",
                "Build",
                "main\nIt passed",
            ]
        );
    }
}
