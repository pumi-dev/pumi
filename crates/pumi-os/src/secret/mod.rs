//! Secrets: how a plugin's `secret` options are kept. The login keychain on macOS (`security`),
//! the Secret Service on Linux (`secret-tool`), the Credential Manager on Windows.
use pumi_api::Result;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod os;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod os;

/// The secret kept for `account` of `service`, or `None`.
pub fn secret_get(service: &str, account: &str) -> Result<Option<String>> {
    os::get(service, account)
}

/// Keeps a secret for `account` of `service`, replacing one kept; `None` forgets it.
pub fn secret_set(service: &str, account: &str, secret: Option<&str>) -> Result<()> {
    match secret {
        Some(secret) => os::set(service, account, secret),
        None => os::forget(service, account),
    }
}
