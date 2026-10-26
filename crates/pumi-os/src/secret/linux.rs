//! Linux: the Secret Service (GNOME Keyring, KWallet), through libsecret's `secret-tool`. An
//! item's attributes are `service` and `account`.
use pumi_api::{Error, ErrorKind, Result};
use std::io::Write;
use std::process::{Command, Output, Stdio};

/// Runs `secret-tool` with `args`, `stdin` written to it.
fn secret_tool(args: &[&str], stdin: Option<&str>) -> Result<Output> {
    let mut command = Command::new("secret-tool");
    command.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    command.stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() });
    let mut child = command.spawn().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => Error::new(
            ErrorKind::ProviderUnavailable,
            "secrets need `secret-tool` (libsecret-tools on Debian and Ubuntu, libsecret on Fedora and Arch)",
        ),
        _ => Error::exception(format!("secret-tool: {err}")),
    })?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        pipe.write_all(text.as_bytes()).map_err(|err| Error::exception(format!("secret-tool: {err}")))?;
    }
    child.wait_with_output().map_err(|err| Error::exception(format!("secret-tool: {err}")))
}

/// What a failed run said, or its status.
fn failure(out: &Output) -> Error {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Error::exception(if err.is_empty() { format!("secret-tool failed: {}", out.status) } else { err })
}

pub fn get(service: &str, account: &str) -> Result<Option<String>> {
    let out = secret_tool(&["lookup", "service", service, "account", account], None)?;
    // Nothing kept is a failure that says nothing.
    if !out.status.success() {
        return if out.stderr.trim_ascii().is_empty() { Ok(None) } else { Err(failure(&out)) };
    }
    Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
}

pub fn set(service: &str, account: &str, secret: &str) -> Result<()> {
    let label = format!("{service} ({account})");
    let out = secret_tool(&["store", "--label", &label, "service", service, "account", account], Some(secret))?;
    if out.status.success() { Ok(()) } else { Err(failure(&out)) }
}

pub fn forget(service: &str, account: &str) -> Result<()> {
    let out = secret_tool(&["clear", "service", service, "account", account], None)?;
    if out.status.success() || out.stderr.trim_ascii().is_empty() { Ok(()) } else { Err(failure(&out)) }
}
