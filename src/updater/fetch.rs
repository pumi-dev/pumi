//! Getting release files: from the published manifest URL, else from GitHub's API (the
//! fallback when the manifest is unreachable, with a token if the release is not public).
//!
//! Transfers are `curl`'s, which ships with macOS, nearly every Linux and Windows 10 1803+
//! (`curl.exe`, which `Command` finds as `curl`): it brings the system's certificates and
//! proxy settings, and keeps a TLS stack out of the app. The token never reaches argv (where
//! `ps` would show it): curl reads its headers from stdin.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde::Deserialize;

use super::manifest::MANIFEST;

/// The release repository (`PUMI_RELEASE_REPO` at build time; scripts/release/build.sh).
pub const REPO: &str = match option_env!("PUMI_RELEASE_REPO") {
    Some(repo) => repo,
    None => "pumi-dev/pumi",
};

pub const CHANNEL: &str = match option_env!("PUMI_RELEASE_CHANNEL") {
    Some(channel) => channel,
    None => "preview",
};

/// Where the signed manifest is published. If it 404s, the updater asks GitHub instead.
pub fn manifest_url() -> String {
    std::env::var("PUMI_UPDATE_URL").unwrap_or_else(|_| format!("https://pumi.dev/releases/{CHANNEL}/{MANIFEST}"))
}

/// Where a release was found, which decides how its archives are fetched.
#[derive(Debug, Clone)]
pub enum Origin {
    /// The public manifest: archives at their `url`.
    Public,
    /// A GitHub release, through the API: archives by name, authenticated if there is a token.
    GitHub { assets: Vec<ReleaseAsset>, token: Option<String> },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    /// The API URL, which serves the bytes with `Accept: application/octet-stream`.
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    assets: Vec<ReleaseAsset>,
}

/// The verified manifest, and where it came from: the public URL if it serves one that
/// `verify` accepts (anything else there — a 404, a site's HTML fallback — is passed over),
/// else the repository's latest GitHub release.
pub fn manifest<T>(verify: impl Fn(&[u8], &str) -> Result<T, String>) -> Result<(T, Origin), String> {
    let url = manifest_url();
    let public = get(&url, None, None).and_then(|bytes| {
        let signature = get(&format!("{url}.minisig"), None, None)?;
        verify(&bytes, &String::from_utf8_lossy(&signature))
    });
    if let Ok(manifest) = public {
        return Ok((manifest, Origin::Public));
    }
    let token = token();
    let api = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release = get(&api, token.as_deref(), Some("application/vnd.github+json")).map_err(|err| match &token {
        Some(_) => format!("no release found: {err}"),
        None => format!("no release found ({err}); set PUMI_GITHUB_TOKEN for a private mirror"),
    })?;
    let release: Release = serde_json::from_slice(&release).map_err(|err| format!("unreadable release: {err}"))?;
    let origin = Origin::GitHub { assets: release.assets, token };
    let bytes = origin.fetch(MANIFEST, "")?;
    let signature = origin.fetch(&format!("{MANIFEST}.minisig"), "")?;
    Ok((verify(&bytes, &String::from_utf8_lossy(&signature))?, origin))
}

impl Origin {
    /// A release file, by its name (and, for the public origin, its URL).
    pub fn fetch(&self, name: &str, url: &str) -> Result<Vec<u8>, String> {
        let tmp = tempfile(name)?;
        self.download(name, url, &tmp)?;
        let bytes = std::fs::read(&tmp).map_err(|err| err.to_string());
        let _ = std::fs::remove_file(&tmp);
        bytes
    }

    /// A release file, to `dest`.
    pub fn download(&self, name: &str, url: &str, dest: &Path) -> Result<(), String> {
        match self {
            Origin::Public => curl(url, None, None, Some(dest)).map(drop),
            Origin::GitHub { assets, token } => {
                let asset =
                    assets.iter().find(|a| a.name == name).ok_or_else(|| format!("the release has no {name}"))?;
                curl(&asset.url, token.as_deref(), Some("application/octet-stream"), Some(dest)).map(drop)
            }
        }
    }
}

fn tempfile(name: &str) -> Result<std::path::PathBuf, String> {
    let dir = super::install::cache_dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    Ok(dir.join(format!(".{}.{name}", std::process::id())))
}

fn get(url: &str, token: Option<&str>, accept: Option<&str>) -> Result<Vec<u8>, String> {
    curl(url, token, accept, None)
}

/// One transfer. A token is only ever sent to api.github.com: GitHub answers an asset with a
/// redirect to its storage, and curl does not carry `-H` headers across hosts.
fn curl(url: &str, token: Option<&str>, accept: Option<&str>, dest: Option<&Path>) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") && !url.starts_with("file://") {
        return Err(format!("refusing a non-https URL: {url}"));
    }
    let mut config = String::new();
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    config.push_str(&format!("url = \"{}\"\n", quote(url)));
    config.push_str(&format!("header = \"User-Agent: pumi/{}\"\n", env!("CARGO_PKG_VERSION")));
    if let Some(accept) = accept {
        config.push_str(&format!("header = \"Accept: {accept}\"\n"));
    }
    if let Some(token) = token.filter(|_| url.starts_with("https://api.github.com/")) {
        config.push_str(&format!("header = \"Authorization: Bearer {}\"\n", quote(token)));
    }
    if let Some(dest) = dest {
        config.push_str(&format!("output = \"{}\"\n", quote(&dest.to_string_lossy())));
    }
    let mut child = Command::new("curl")
        .args(["--config", "-", "-fsSL", "--proto", "=https,file", "--retry", "2", "--connect-timeout", "20"])
        .args(["--max-time", "900"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("could not run curl: {err}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(config.as_bytes()).map_err(|err| err.to_string())?;
    }
    let output = child.wait_with_output().map_err(|err| err.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().trim_start_matches("curl: ").to_string());
    }
    Ok(output.stdout)
}

/// A GitHub token, for a release that is not public: `PUMI_GITHUB_TOKEN`, else the one
/// the installer stored (the login keychain on macOS, `github-token` in pumi's config directory
/// elsewhere), else the GitHub CLI's.
pub fn token() -> Option<String> {
    let clean = |s: String| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(token) = std::env::var("PUMI_GITHUB_TOKEN").ok().and_then(clean) {
        return Some(token);
    }
    if cfg!(target_os = "macos")
        && let Some(token) =
            run("security", &["find-generic-password", "-s", "dev.pumi.github-token", "-w"]).and_then(clean)
    {
        return Some(token);
    }
    if let Some(token) = std::fs::read_to_string(crate::config_dir().join("github-token")).ok().and_then(clean) {
        return Some(token);
    }
    run("gh", &["auth", "token"]).and_then(clean)
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
