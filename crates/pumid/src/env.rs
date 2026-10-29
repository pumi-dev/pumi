use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
pub const LOGIN_ENV_SWITCH: &str = "PUMI_LOGIN_ENV";
const DROP: &[&str] = &[
    "PUMI",
    "PUMI_TAB",
    LEGACY_TAB,
    "PUMI_VIEW",
    "PUMI_SESSION",
    "PUMI_CLIENT",
    crate::handoff::RESTART_AFTER,
    "TMUX",
    "TMUX_PANE",
];
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5);
/// The tab's id under the name `PUMI_PANE` (`%3`) as well, for agents' hooks that read it.
const LEGACY_TAB: &str = "PUMI_PANE";
pub fn login_env(cache: &Path, refresh: bool) -> BTreeMap<String, String> {
    let own: BTreeMap<String, String> = std::env::vars().collect();
    if !pumi_sys::caps().login_env || std::env::var(LOGIN_ENV_SWITCH).as_deref() == Ok("0") {
        return own;
    }
    if !refresh && let Some(cached) = std::fs::read(cache).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
        return cached;
    }
    match read_login_env(&pumi_sys::process::default_shell()) {
        Some(env) => {
            if let Ok(json) = serde_json::to_vec(&env) {
                let _ = std::fs::write(cache, json);
            }
            env
        }
        None => own,
    }
}
fn read_login_env(shell: &str) -> Option<BTreeMap<String, String>> {
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c", "env -0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir(pumi_proto::paths::home_dir())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let out = reader.join().ok()?;
    let env: BTreeMap<String, String> = out
        .split(|b| *b == 0)
        .filter_map(|record| {
            let record = String::from_utf8_lossy(record);
            let (k, v) = record.split_once('=')?;
            (!k.is_empty() && !k.contains('\n')).then(|| (k.to_string(), v.to_string()))
        })
        .collect();
    (env.contains_key("PATH")).then_some(env)
}
#[derive(Debug, Clone, Default)]
pub struct Identity {
    pub tab: u64,
    pub view: Option<u64>,
    pub session: Option<u64>,
}
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub socket: PathBuf,
    pub bin_dir: Option<PathBuf>,
}
type Var = (&'static str, fn(&Context, &Identity) -> Option<String>);
const TAB_VARS: &[Var] = &[
    ("TERM", |_, _| Some("xterm-256color".into())),
    ("COLORTERM", |_, _| Some("truecolor".into())),
    ("TERM_PROGRAM", |_, _| Some("ghostty".into())),
    ("TERM_PROGRAM_VERSION", |_, _| Some(pumi_proto::GHOSTTY_VERSION.into())),
    ("PUMI_VERSION", |_, _| Some(pumi_proto::PUMI_VERSION.into())),
    ("PUMI", |c, _| Some(c.socket.display().to_string())),
    ("PUMI_TAB", |_, i| Some(format!("%{}", i.tab))),
    (LEGACY_TAB, |_, i| Some(format!("%{}", i.tab))),
    ("PUMI_VIEW", |_, i| i.view.map(|t| format!("@{t}"))),
    ("PUMI_SESSION", |_, i| i.session.map(|s| format!("${s}"))),
];
pub fn tab_env(
    base: &BTreeMap<String, String>,
    context: &Context,
    identity: &Identity,
    extra: &BTreeMap<String, String>,
) -> Vec<(String, String)> {
    let mut env = base.clone();
    for name in DROP {
        env.remove(*name);
    }
    for (name, make) in TAB_VARS {
        if let Some(value) = make(context, identity) {
            env.insert((*name).into(), value);
        }
    }
    if let Some(bin) = &context.bin_dir {
        // Windows names it `Path`, and any case is the same variable there.
        let key = match env.keys().find(|k| cfg!(windows) && k.eq_ignore_ascii_case("PATH")) {
            Some(key) => key.clone(),
            None => "PATH".to_string(),
        };
        let path = env.get(&key).cloned().unwrap_or_else(|| "/usr/bin:/bin:/usr/sbin:/sbin".into());
        let (bin, sep) = (bin.display().to_string(), if cfg!(windows) { ';' } else { ':' });
        if !path.split(sep).any(|p| p == bin) {
            env.insert(key, format!("{bin}{sep}{path}"));
        }
    }
    env.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));
    env.into_iter().collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tabs_are_told_the_server_and_where_they_are() {
        let base =
            BTreeMap::from([("PATH".to_string(), "/usr/bin".to_string()), ("TMUX".to_string(), "x".to_string())]);
        let context = Context { socket: "/d/pumi.sock".into(), bin_dir: Some("/app/bin".into()) };
        let identity = Identity { tab: 12, view: Some(3), session: Some(1) };
        let env: BTreeMap<_, _> =
            tab_env(&base, &context, &identity, &BTreeMap::from([("A".into(), "b".into())])).into_iter().collect();
        assert_eq!(env["PUMI"], "/d/pumi.sock");
        assert_eq!(env["PUMI_TAB"], "%12");
        assert_eq!(env["PUMI_PANE"], "%12", "what a hook that reads `PUMI_PANE` sees");
        assert_eq!(env["PUMI_VIEW"], "@3");
        assert_eq!(env["PUMI_SESSION"], "$1");
        let sep = if cfg!(windows) { ';' } else { ':' };
        assert_eq!(env["PATH"], format!("/app/bin{sep}/usr/bin"));
        assert_eq!(env["A"], "b");
        assert!(!env.contains_key("TMUX"));
    }
}
