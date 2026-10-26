//! The machine's functions only the OS answers, as pumi's API has them. Each runs where its
//! `runs_on` says: the server's (`fs.trash`, `process.*`) in the server, the client's (the
//! clipboard, pickers, opening a path) in the app, through the [`Desktop`] it installs.
mod desktop;
pub mod notify;
mod process;
mod secret;
mod trash;
pub use desktop::{Desktop, Pick, PickKind, install_desktop};
pub use secret::{secret_get, secret_set};
use pumi_api::{Error, Result, RunsOn};
use serde_json::Value;
use std::process::{Command, Stdio};
/// Referenced by every binary that links the functions, so the linker keeps them.
pub fn link() {}
/// Whether `name` is one of this crate's functions.
pub fn is_system(name: &str) -> bool {
    pumi_api::Registry::core(name).is_some_and(|f| f.module.starts_with("pumi_os"))
}
/// Whether `name` is one the app's [`Desktop`] answers: it may wait for the app's main thread
/// (a picker, for the user), so a caller answers it off any thread that thread waits for.
pub fn is_desktop(name: &str) -> bool {
    pumi_api::Registry::core(name).is_some_and(|f| f.module.starts_with("pumi_os::desktop"))
}
/// Answers a call to one of this crate's functions that runs on the client (`runs_on =
/// "client"` or `"either"`), here and now; `None` for any other function. The app's plugins and
/// scripts call through it, so a clipboard or a dialog needs no trip to the server.
pub fn answer(name: &str, args: Value) -> Option<Result<Value>> {
    let function = pumi_api::Registry::core(name)?;
    if !function.module.starts_with("pumi_os") || function.runs_on == RunsOn::Server {
        return None;
    }
    let args = match args {
        Value::Null => Value::Array(Vec::new()),
        args => args,
    };
    Some(pumi_api::Registry::new().call(&mut (), &pumi_api::Caller::default(), name, args))
}
/// Runs a tool and answers its stdout, trimmed at the end; a failure is its stderr.
pub(crate) fn tool(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| Error::exception(format!("{program}: {err}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::exception(if err.is_empty() { format!("{program} failed: {}", out.status) } else { err }));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}
#[cfg(test)]
mod tests {
    use serde_json::json;
    #[test]
    fn the_system_functions_are_this_crates() {
        assert!(super::is_system("clipboard.get"));
        assert!(super::is_system("process.list"));
        assert!(!super::is_system("call"), "pumi-api's");
        assert!(!super::is_system("system.clipboard.get"), "no name starts with `system.`");
        assert!(super::is_desktop("fs.pick") && !super::is_desktop("fs.trash"));
    }
    #[test]
    fn the_client_ones_are_answered_here() {
        assert!(super::answer("process.list", json!([])).is_none(), "the server's");
        assert!(super::answer("request.open", json!([])).is_none(), "the server's");
        let refused = super::answer("fs.pick", json!([{ "mode": "save", "multiple": true }])).unwrap();
        assert_eq!(refused.unwrap_err().kind, pumi_api::ErrorKind::InvalidParams, "refused before it asks");
        let refused = super::answer("clipboard.get", json!([])).unwrap();
        assert_eq!(refused.unwrap_err().kind, pumi_api::ErrorKind::ProviderUnavailable, "no desktop installed");
    }
}
