//! Windows: generic credentials in the Credential Manager, named `<service>/<account>`; the
//! secret is kept as UTF-8.
use pumi_api::{Error, Result};
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW, CredWriteW,
};
use windows::core::{PCWSTR, PWSTR};

/// The credential's name, NUL-terminated UTF-16.
fn target(service: &str, account: &str) -> Vec<u16> {
    format!("{service}/{account}").encode_utf16().chain(Some(0)).collect()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn not_found(err: &windows::core::Error) -> bool {
    err.code() == ERROR_NOT_FOUND.to_hresult()
}

fn failure(what: &str, err: windows::core::Error) -> Error {
    Error::exception(format!("the Credential Manager: {what}: {err}"))
}

pub fn get(service: &str, account: &str) -> Result<Option<String>> {
    let name = target(service, account);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated; a credential read is freed with `CredFree` once copied.
    match unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut credential) } {
        Ok(()) => {}
        Err(err) if not_found(&err) => return Ok(None),
        Err(err) => return Err(failure("reading", err)),
    }
    let secret = unsafe {
        let blob = std::slice::from_raw_parts((*credential).CredentialBlob, (*credential).CredentialBlobSize as usize);
        let secret = String::from_utf8_lossy(blob).into_owned();
        CredFree(credential as *const _);
        secret
    };
    Ok(Some(secret))
}

pub fn set(service: &str, account: &str, secret: &str) -> Result<()> {
    let mut name = target(service, account);
    let mut user = wide(account);
    let mut blob = secret.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        UserName: PWSTR(user.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: every pointer in `credential` is to a buffer that outlives the call.
    unsafe { CredWriteW(&credential, 0) }.map_err(|err| failure("writing", err))
}

pub fn forget(service: &str, account: &str) -> Result<()> {
    let name = target(service, account);
    // SAFETY: `name` is NUL-terminated.
    match unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Err(err) if !not_found(&err) => Err(failure("deleting", err)),
        _ => Ok(()),
    }
}
