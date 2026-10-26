//! What a release says about itself: the channel manifest (`latest.json`, written by
//! `scripts/release/manifest.sh`), and the keys it must be signed with.
//!
//! The manifest's minisign signature is the root of trust. It names each platform's archive
//! with its SHA-256, so an archive is trusted by matching the signed manifest, whichever
//! server it came from.

use std::collections::BTreeMap;

use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

/// The update keys this build trusts: the primary, which CI signs with, and an offline backup
/// for when the primary is lost or leaked (`docs/releasing.md`, Keys).
pub const PUBLIC_KEYS: &[&str] = &[
    // pumi update key: the primary
    "RWQGWJa1jK6fXLhk2t8/zrWDojiRPRykE5DkbUI0/xcdWcU9YfczkJ79",
    // pumi update key: the backup, kept offline
    "RWQe/SSsIy1UiLqkZRlz4FJoUwBTAkRbMcEgsddiFiXHblnemzQvvQ2K",
];

pub const MANIFEST: &str = "latest.json";

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub channel: String,
    pub version: String,
    #[serde(default)]
    pub build: u64,
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub notes_url: String,
    pub assets: BTreeMap<String, Asset>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Asset {
    pub name: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub url: String,
}

/// This machine's key in `assets`: `macos-aarch64`, `linux-x86_64`, `windows-x86_64`, ….
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The manifest's bytes, if `signature` is a valid signature of them by one of `keys` and
/// was made for the manifest (the signed trusted comment names the file, so a signature of
/// some other release file cannot be passed off as one).
pub fn verify(bytes: &[u8], signature: &str, keys: &[&str]) -> Result<Manifest, String> {
    let signature = Signature::decode(signature).map_err(|err| format!("unreadable signature: {err}"))?;
    let trusted = keys
        .iter()
        .filter_map(|key| PublicKey::from_base64(key).ok())
        .any(|key| key.verify(bytes, &signature, false).is_ok());
    if !trusted {
        return Err("the manifest is not signed by a pumi release key".into());
    }
    if !signature.trusted_comment().split_whitespace().any(|word| word == format!("file:{MANIFEST}")) {
        return Err("the signature is not for a manifest".into());
    }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|err| format!("unreadable manifest: {err}"))?;
    if manifest.schema != 1 {
        return Err(format!("manifest schema {} is newer than this pumi understands", manifest.schema));
    }
    Ok(manifest)
}

/// Whether `offered` is a newer version than `current`. Never a downgrade, and nothing that
/// does not parse.
pub fn is_newer(offered: &str, current: &str) -> bool {
    match (semver::Version::parse(offered), semver::Version::parse(current)) {
        (Ok(offered), Ok(current)) => offered > current,
        _ => false,
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made with a throwaway key: `minisign -G -W`, then
    // `minisign -S -m latest.json -t "pumi 9.9.9 preview file:latest.json"`.
    const TEST_KEY: &str = "RWThi/1bZ4j/zUy/X1IwZHPI7PK4tdMIzz5efMjKt5OKT36rxeAVxkjs";

    #[test]
    fn versions_only_move_forward() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("0.2.0", "0.2.0-preview.3"));
        assert!(is_newer("0.2.0-preview.4", "0.2.0-preview.3"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn platform_is_os_and_arch() {
        let platform = platform();
        assert!(["macos-", "linux-", "windows-"].iter().any(|os| platform.starts_with(os)), "{platform}");
    }

    #[test]
    fn rejects_unsigned_and_tampered_manifests() {
        let (bytes, signature) = fixture();
        assert!(verify(&bytes, &signature, &[TEST_KEY]).is_ok());
        assert!(verify(&bytes, &signature, PUBLIC_KEYS).is_err(), "signed by a key this build does not trust");
        let mut tampered = bytes.clone();
        let at = tampered.iter().position(|&b| b == b'9').unwrap();
        tampered[at] = b'8';
        assert!(verify(&tampered, &signature, &[TEST_KEY]).is_err());
        assert!(verify(&bytes, "not a signature", &[TEST_KEY]).is_err());
    }

    fn fixture() -> (Vec<u8>, String) {
        (include_bytes!("testdata/latest.json").to_vec(), include_str!("testdata/latest.json.minisig").to_string())
    }
}
