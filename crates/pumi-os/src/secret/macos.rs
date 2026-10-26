//! macOS: generic passwords in the login keychain, through `security`.
use crate::tool;
use pumi_api::Result;

pub fn get(service: &str, account: &str) -> Result<Option<String>> {
    match tool("security", &["find-generic-password", "-s", service, "-a", account, "-w"]) {
        Ok(secret) => Ok(Some(secret)),
        Err(err) if err.message.contains("could not be found") => Ok(None),
        Err(err) => Err(err),
    }
}

pub fn set(service: &str, account: &str, secret: &str) -> Result<()> {
    tool("security", &["add-generic-password", "-U", "-s", service, "-a", account, "-w", secret]).map(|_| ())
}

pub fn forget(service: &str, account: &str) -> Result<()> {
    match tool("security", &["delete-generic-password", "-s", service, "-a", account]) {
        Err(err) if !err.message.contains("could not be found") => Err(err),
        _ => Ok(()),
    }
}
