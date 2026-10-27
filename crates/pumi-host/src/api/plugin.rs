use super::kernel;
use crate::kernel::Kernel;
use crate::kernel::events::About;
use crate::plugins::{Entry, checkout_of, dir_of, lock_read, lock_write, root};
use pumi_api::{Cx, Error, Optional, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A plugin's id (`owner/name`), as every function that names a plugin takes it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct PluginId(pub String);
impl<'de> Deserialize<'de> for PluginId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<PluginId, D::Error> {
        let text = String::deserialize(d)?;
        let ok = text.split_once('/').is_some_and(|(owner, name)| {
            let part = |s: &str| {
                !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
            };
            part(owner) && part(name)
        });
        if !ok {
            return Err(serde::de::Error::custom(format!("`{text}` is not a plugin id (`owner/name`)")));
        }
        Ok(PluginId(text))
    }
}
impl schemars::JsonSchema for PluginId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PluginId".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": "^[a-z0-9][a-z0-9_-]*/[a-z0-9][a-z0-9_-]*$" })
    }
}
/// A plugin's state directory, `stdpath('state')/plugins/<owner.name>`: its options' store and
/// what else it keeps here.
pub fn state_dir(plugin: &str) -> PathBuf {
    let state = crate::dirs::stdpath("state").expect("state is a stdpath");
    pumi_api::manifest::data_dir(&state, plugin)
}
/// Runs a program; its standard output, trimmed, or what it said on failing.
fn run(program: &str, args: &[&str], stdin: Option<&str>) -> Result<String> {
    use std::io::Write;
    let mut child = Command::new(program)
        .args(args)
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::exception(format!("{program}: {e}")))?;
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| Error::exception(format!("{program}: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let why = if stderr.is_empty() { stdout } else { stderr };
        return Err(Error::exception(format!("{program} {}: {why}", args.join(" "))));
    }
    Ok(stdout)
}
fn fetch(src: &str) -> Result<String> {
    if src.starts_with("http://") || src.starts_with("https://") {
        return run("curl", &["-fsSL", "--max-time", "30", src], None);
    }
    let path = crate::dirs::expand(src);
    std::fs::read_to_string(&path).map_err(|e| Error::not_found(format!("{}: {e}", path.display())))
}
fn save(lock: &crate::plugins::Lock) -> Result<()> {
    lock_write(lock).map_err(|e| Error::exception(format!("plugin-lock.json: {e}")))
}
/// The store's index (option `pluginindex`), a URL or a file; a URL's answer is kept in
/// `stdpath('cache')/plugin-index.json` for half an hour (the store's own refresh).
fn index(k: &Kernel, refresh: bool) -> Result<Vec<Value>> {
    let src = k.global_option("pluginindex").as_str().unwrap_or_default().to_string();
    let text = if src.starts_with("http://") || src.starts_with("https://") {
        let cache = crate::dirs::stdpath("cache").expect("cache is a stdpath").join("plugin-index.json");
        let fresh = std::fs::metadata(&cache)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() < 1800);
        match std::fs::read_to_string(&cache) {
            Ok(text) if fresh && !refresh => text,
            _ => {
                let text = fetch(&src)?;
                let _ = crate::dirs::write_atomic(&cache, &text);
                text
            }
        }
    } else {
        fetch(&src).map_err(|_| Error::not_found(format!("no store index at {src}")))?
    };
    let index: Value =
        serde_json::from_str(&text).map_err(|e| Error::invalid_params(format!("the store index: {e}")))?;
    Ok(index.get("plugins").unwrap_or(&index).as_array().cloned().unwrap_or_default())
}
/// What a source is: `git`, `link` (a directory here) or `index` (a store id `owner/name`, a
/// name in the store, or GitHub's `owner/repo[/dir]`).
fn kind_of(src: &str) -> &'static str {
    let local = src.starts_with('/') || src.starts_with('~') || src.starts_with('.');
    if src.contains("://") || src.starts_with("git@") {
        "git"
    } else if local {
        "link"
    } else {
        "index"
    }
}
/// A source as `{src, name, version, path, kind}`, a store id or GitHub shorthand resolved.
struct Spec {
    src: String,
    name: Option<String>,
    /// The git ref to follow.
    version: Option<String>,
    /// The plugin's directory in the repository.
    path: Option<String>,
    kind: &'static str,
}
/// A store id as pumi writes owners: lowercase, `-` as `_`.
fn id_key(id: &str) -> String {
    id.to_lowercase().replace('-', "_")
}
/// `owner/repo[/dir…]`, GitHub's shorthand: the repository's URL and the directory in it.
fn github_shorthand(src: &str) -> Option<(String, Option<String>)> {
    let parts: Vec<&str> = src.trim_matches('/').split('/').collect();
    let ok = |p: &str| {
        !p.is_empty() && p != "." && p != ".." && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    if parts.len() < 2 || !parts.iter().all(|p| ok(p)) {
        return None;
    }
    let url = format!("https://github.com/{}/{}", parts[0], parts[1].trim_end_matches(".git"));
    let dir = (parts.len() > 2).then(|| parts[2..].join("/"));
    Some((url, dir))
}
/// The store entry `src` names: its id (`owner/name`, either spelling of the owner), or its name
/// when only one has it.
fn index_entry(index: &[Value], src: &str) -> Option<Value> {
    let key = id_key(src);
    if let Some(e) = index.iter().find(|e| e["id"].as_str().is_some_and(|id| id_key(id) == key)) {
        return Some(e.clone());
    }
    let named: Vec<&Value> = index.iter().filter(|e| e["name"] == src).collect();
    (named.len() == 1).then(|| named[0].clone())
}
fn spec_of(k: &Kernel, source: &str, version: Option<&str>, linked: bool) -> Result<Spec> {
    let mut name: Option<String> = None;
    let mut path: Option<String> = None;
    let version = version.map(str::to_string);
    if source.is_empty() {
        return Err(Error::invalid_params("a plugin needs a source"));
    }
    let mut src = source.to_string();
    let mut kind = if linked { "link" } else { kind_of(&src) };
    if kind == "index" {
        // The store first (an id, or a name only one has), then GitHub's shorthand. Without the
        // store at hand, shorthand still works.
        let index = index(k, false).unwrap_or_default();
        match index_entry(&index, &src) {
            Some(entry) => {
                name = name.or(entry["name"].as_str().map(str::to_string));
                path = path.or(entry["path"].as_str().filter(|p| !p.is_empty()).map(str::to_string));
                src = entry["src"].as_str().unwrap_or_default().to_string();
                kind = kind_of(&src);
            }
            None => match github_shorthand(&src) {
                Some((url, dir)) => {
                    path = path.or(dir);
                    src = url;
                    kind = "git";
                }
                None => {
                    return Err(Error::not_found(format!(
                        "no plugin `{src}` in the store (`registry.list(\"plugin\", {{available: true, name: \"{src}\"}})` lists them); a GitHub repository is `owner/repo` or `owner/repo/dir`"
                    )));
                }
            },
        }
    }
    if kind == "link" {
        let path = crate::dirs::expand(&src);
        let path = std::path::absolute(&path).unwrap_or(path);
        // A relative path is from this server's directory, not the caller's shell: one that is
        // not a directory here would be locked in as a plugin that never loads.
        if !path.is_dir() {
            return Err(Error::not_found(format!(
                "no directory {} (a relative path is from the server's directory: give the whole path)",
                path.display()
            )));
        }
        src = path.display().to_string();
    }
    if let Some(p) = &path
        && (p.split('/').any(|seg| seg == ".." || seg.is_empty()) || p.starts_with('/'))
    {
        return Err(Error::invalid_params(format!("`{p}` is not a directory in the repository")));
    }
    if src.ends_with(".ts") && kind != "index" {
        return Err(Error::invalid_params(format!("`{src}` is one file: a plugin is a directory (`pumi plugin new`)")));
    }
    if name.is_none() {
        let base = match &path {
            Some(p) => p.rsplit('/').next().unwrap_or_default(),
            None => src.trim_end_matches('/').rsplit('/').next().unwrap_or_default(),
        };
        name = Some(base.trim_end_matches(".git").to_string());
    }
    Ok(Spec { src, name, version, path, kind })
}
/// Removes the link `path` (made by `pumi_sys::paths::link`), not what it names: a symbolic link
/// is a file, a Windows junction a directory.
fn unlink(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path).or_else(|_| std::fs::remove_dir(path))
}
/// Installs `spec`; its name and lockfile entry.
fn install(spec: &Spec) -> Result<(String, Entry)> {
    std::fs::create_dir_all(root()).map_err(|e| Error::exception(format!("{}: {e}", root().display())))?;
    let name = spec.name.clone().expect("a spec has a name");
    let dir = dir_of(&name);
    let target = dir.display().to_string();
    if spec.kind == "link" {
        if !Path::new(&spec.src).is_dir() {
            return Err(Error::not_found(format!("no directory {}", spec.src)));
        }
        pumi_sys::paths::link(Path::new(&spec.src), &dir).map_err(|e| Error::exception(format!("{target}: {e}")))?;
        return Ok((name, Entry { src: Some(spec.src.clone()), kind: Some("link".into()), ..Entry::default() }));
    }
    // A plugin in a folder of a repository: the repository is checked out beside the others
    // (`.src/<name>`), and its directory links to the folder.
    let checkout = match &spec.path {
        Some(_) => checkout_of(&name),
        None => dir.clone(),
    };
    let at = checkout.display().to_string();
    let undo = || {
        let _ = std::fs::remove_dir_all(&checkout);
        if spec.path.is_some() {
            let _ = unlink(&dir);
        }
    };
    if let Some(parent) = checkout.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    run("git", &["clone", "--quiet", "--filter=blob:none", &spec.src, &at], None)?;
    if let Some(version) = &spec.version
        && let Err(e) = run("git", &["-C", &at, "checkout", "--quiet", version], None)
    {
        undo();
        return Err(e);
    }
    if let Some(path) = &spec.path {
        let folder = checkout.join(path);
        if !folder.join("package.json").is_file() {
            undo();
            return Err(Error::not_found(format!("{} has no package.json in `{path}`", spec.src)));
        }
        if let Err(e) = pumi_sys::paths::link(&folder, &dir) {
            undo();
            return Err(Error::exception(format!("{target}: {e}")));
        }
    }
    if let Err(e) = check_version(&dir, &spec.src) {
        undo();
        return Err(e);
    }
    // Its npm dependencies: a plugin that cannot load them is not installed.
    if let Err(e) = crate::deps::ensure(&dir) {
        undo();
        return Err(Error::exception(format!("{target}: {e}")));
    }
    let rev = run("git", &["-C", &at, "rev-parse", "HEAD"], None)?;
    let entry = Entry {
        src: Some(spec.src.clone()),
        kind: Some("git".into()),
        version: spec.version.clone(),
        path: spec.path.clone(),
        rev: Some(rev),
        ..Entry::default()
    };
    Ok((name, entry))
}
/// Refuses a plugin whose manifest asks for a newer pumi (`minPumi`) than this one.
fn check_version(dir: &Path, src: &str) -> Result<()> {
    let Ok(plugin) = pumi_api::manifest::read(dir, Some(src)) else { return Ok(()) };
    let Some(min) = &plugin.manifest.min_pumi else { return Ok(()) };
    let parse = |v: &str| -> Vec<u64> { v.split(['.', '-']).take(3).map(|p| p.parse().unwrap_or(0)).collect() };
    if parse(min) > parse(pumi_proto::PUMI_VERSION) {
        return Err(Error::invalid_params(format!(
            "{} needs pumi {min} or later; this is {}",
            plugin.id,
            pumi_proto::PUMI_VERSION
        )));
    }
    Ok(())
}
/// Tells the store an install of the plugin in `dir` happened, so its count goes up (once per
/// address a day, at the server). In the background, never failing the install; only for a
/// plugin from GitHub, and only when the store index is the store's API (`…/v1/index.json`).
fn count_install(k: &Kernel, dir: &Path, src: &str) {
    if !(src.starts_with("https://github.com/") || src.starts_with("git@github.com:")) {
        return;
    }
    let index = k.global_option("pluginindex").as_str().unwrap_or_default().to_string();
    let Some(base) = index.strip_suffix("/v1/index.json").filter(|b| b.starts_with("https://")) else { return };
    let Ok(plugin) = pumi_api::manifest::read(dir, Some(src)) else { return };
    let url = format!("{base}/v1/plugins/{}/install", plugin.id);
    std::thread::spawn(move || {
        let _ = Command::new("curl")
            .args(["-fsS", "-o", "/dev/null", "--max-time", "5", "-X", "POST", "-A"])
            .arg(format!("pumi/{}", pumi_proto::PUMI_VERSION))
            .arg(&url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}
/// The runtimepath again, as the plugins now are (the app finds their files on it), and what
/// their manifests contribute.
fn follow_plugins(cx: &mut Cx) -> Result<()> {
    let clean = cx.host::<Kernel>()?.ext.get::<crate::defaults::Startup>().is_some_and(|s| s.clean);
    let rtp = crate::plugins::runtimepath(clean);
    cx.call("option.set", json!(["runtimepath", rtp, { "scope": "global" }]))?;
    if !clean {
        crate::contributions::follow(cx);
    }
    Ok(())
}
/// The names given, or every installed plugin's; each must be installed.
fn names_or_all(names: Vec<String>, lock: &crate::plugins::Lock) -> Result<Vec<String>> {
    let names = if names.is_empty() { lock.plugins.keys().cloned().collect() } else { names };
    for name in &names {
        if !lock.plugins.contains_key(name) {
            return Err(Error::not_found(format!("plugin `{name}` is not installed")));
        }
    }
    Ok(names)
}
/// How `plugin.add` installs.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddOpts {
    /// The git ref to follow: a tag, a branch, a commit.
    #[serde(default)]
    pub version: Optional<String>,
    /// Link a directory on this machine instead of copying it.
    #[serde(default)]
    pub linked: Optional<bool>,
    /// Start it once installed (`true`, the default); `false` installs it turned off.
    #[serde(default)]
    pub enabled: Optional<bool>,
}
/// Install a plugin: a store id (`owner/name`, or a name only one listing has), a GitHub
/// repository (`owner/repo`, or `owner/repo/dir` for one in a folder), a git URL, or a directory
/// on this machine, which is linked. The commit installed is recorded; a manifest's `minPumi`
/// newer than this pumi is refused. One from git with `dependencies` has them installed (`bun
/// install --production --no-save --ignore-scripts`), and is refused when they fail to install.
/// An install from GitHub is counted by the store
/// (`pluginindex`'s server, once per address a day). One already installed stays as it is.
/// Answers its id (`owner/name`).
#[api(
    name = "plugin.add",
    example(args = r#"["https://github.com/someone/pumi-caffeinate", {}]"#, result = r#""someone/caffeinate""#),
    example(args = r#"["~/code/clock", {"linked": true, "enabled": false}]"#, result = r#""local/clock""#)
)]
pub fn pumi_plugin_add(cx: &mut Cx, source: String, opts: Optional<AddOpts>) -> Result<String> {
    let opts = opts.into_option().unwrap_or_default();
    let spec = {
        let (_, k) = kernel(cx)?;
        spec_of(k, &source, opts.version.0.as_deref(), opts.linked.0.unwrap_or(false))?
    };
    let on = opts.enabled.0.unwrap_or(true);
    if let Some(name) = spec.name.as_deref().filter(|n| crate::plugins::is_bundled(n)) {
        return Err(Error::invalid_params(format!("`{name}` is bundled with pumi; it is installed already")));
    }
    let mut lock = lock_read();
    let known = spec.name.as_ref().filter(|n| std::fs::symlink_metadata(dir_of(n)).is_ok()).cloned();
    let name = match known {
        Some(name) => {
            if let Some(had) = lock.plugins.get(&name)
                && (had.src.as_deref() != Some(spec.src.as_str()) || had.path != spec.path)
            {
                let from = had.src.clone().unwrap_or_default();
                return Err(Error::invalid_params(format!(
                    "a plugin named `{name}` is installed already, from {from}; remove it first (`plugin.remove`)"
                )));
            }
            lock.plugins.entry(name.clone()).or_insert_with(|| Entry {
                src: Some(spec.src.clone()),
                kind: Some(spec.kind.into()),
                version: spec.version.clone(),
                path: spec.path.clone(),
                ..Entry::default()
            });
            save(&lock)?;
            // One installed already may still miss its dependencies (from before pumi installed them).
            if lock.plugins[&name].kind.as_deref() == Some("git") {
                crate::deps::ensure(&dir_of(&name)).map_err(|e| Error::exception(format!("{name}: {e}")))?;
            }
            name
        }
        None => {
            let (name, mut entry) = install(&spec)?;
            if spec.kind == "git" {
                let (_, k) = kernel(cx)?;
                count_install(k, &dir_of(&name), &spec.src);
            }
            entry.disabled = !on;
            lock.plugins.insert(name.clone(), entry);
            save(&lock)?;
            follow_plugins(cx)?;
            registry_changed(cx, &name)?;
            name
        }
    };
    let entry = &lock.plugins[&name];
    pumi_api::manifest::read(&dir_of(&name), entry.src.as_deref()).map(|p| p.id).map_err(Error::exception)
}
/// The lockfile's name for the plugin `id` (`owner/name`): the lockfile names a plugin by its
/// directory, its manifest by its id.
fn lock_name(lock: &crate::plugins::Lock, id: &PluginId) -> Result<String> {
    lock.plugins
        .iter()
        .find(|(name, entry)| pumi_api::manifest::read(&dir_of(name), entry.src.as_deref()).is_ok_and(|p| p.id == id.0))
        .map(|(name, _)| name.clone())
        .ok_or_else(|| Error::not_found(format!("plugin `{}` is not installed", id.0)))
}
/// What would change for one plugin: `{name, from, to, log}`, or none when it follows the
/// commit it has.
fn pending(name: &str, entry: &Entry) -> Result<Option<Value>> {
    let dir = dir_of(name).display().to_string();
    match entry.kind.as_deref() {
        Some("git") => {
            run("git", &["-C", &dir, "fetch", "--quiet", "--tags", "origin"], None)?;
            let target = match &entry.version {
                Some(v) => run("git", &["-C", &dir, "rev-parse", &format!("{v}^{{commit}}")], None)?,
                None => run("git", &["-C", &dir, "rev-parse", "origin/HEAD"], None)?,
            };
            let from = run("git", &["-C", &dir, "rev-parse", "HEAD"], None)?;
            if target == from {
                return Ok(None);
            }
            let log =
                run("git", &["-C", &dir, "log", "--oneline", "--no-decorate", &format!("{from}..{target}")], None)?;
            Ok(Some(json!({ "name": name, "from": from, "to": target, "log": log })))
        }
        _ => Ok(None),
    }
}
/// How `plugin.update` patches one, or refreshes it from its source.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdatePatch {
    /// Turn it on or off; it stays installed.
    #[serde(default)]
    pub enabled: Optional<bool>,
    /// Follow another git ref from now on (a tag, a branch, a commit).
    #[serde(default)]
    pub version: Optional<String>,
}
/// Patch a plugin, or refresh it from its source: a linked one is read again where it is, and a
/// git one is fetched and checked out at the ref it follows (or the one `version` gives), and its
/// dependencies are installed when they changed or are missing. What
/// its manifest contributes is read again, and the app starts it again. `enabled` turns it on or
/// off, and it stays installed.
#[api(
    name = "plugin.update",
    example(args = r#"["owner/name", {"enabled": false}]"#, result = "null"),
    example(args = r#"["owner/name", {}]"#, result = "null", desc = "Refreshed from its source.")
)]
pub fn pumi_plugin_update(cx: &mut Cx, plugin: PluginId, patch: Optional<UpdatePatch>) -> Result<()> {
    let patch = patch.into_option().unwrap_or_default();
    let mut lock = lock_read();
    let name = lock_name(&lock, &plugin)?;
    let entry = lock.plugins[&name].clone();
    if let Some(on) = patch.enabled.0 {
        lock.plugins.get_mut(&name).expect("listed").disabled = !on;
        save(&lock)?;
        follow_plugins(cx)?;
        registry_changed(cx, &name)?;
        return Ok(());
    }
    if entry.kind.as_deref() == Some("bundled") {
        return Err(Error::invalid_params(format!("`{name}` is bundled with pumi; it cannot be refreshed")));
    }
    if let Some(version) = patch.version.0 {
        lock.plugins.get_mut(&name).expect("listed").version = Some(version);
    }
    let entry = lock.plugins.get(&name).cloned().expect("listed");
    match entry.kind.as_deref() {
        Some("git") => {
            if let Some(change) = pending(&name, &entry)? {
                let dir = dir_of(&name);
                let to = change["to"].as_str().unwrap_or_default().to_string();
                run("git", &["-C", &dir.display().to_string(), "checkout", "--quiet", "--detach", &to], None)?;
                lock.plugins.get_mut(&name).expect("listed").rev = Some(to);
                save(&lock)?;
            }
            // Its dependencies as the commit it now has asks for them, or as an install from
            // before pumi ran `bun install` never had them.
            crate::deps::ensure(&dir_of(&name)).map_err(|e| Error::exception(format!("{name}: {e}")))?;
        }
        Some("link") => {
            let target = std::fs::read_link(dir_of(&name))
                .map_err(|e| Error::not_found(format!("{}: {e}", dir_of(&name).display())))?;
            if !target.exists() {
                return Err(Error::not_found(format!("{}: the linked directory is gone", target.display())));
            }
        }
        _ => {}
    }
    save(&lock)?;
    follow_plugins(cx)?;
    registry_changed(cx, &name)?;
    Ok(())
}
/// What `plugin.remove` removes beyond the plugin.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveOpts {
    /// Also what it kept: its own `config`, `state` and `cache` directories
    /// (`stdpath(…)/plugins/<owner.name>`, its options' store among them), and its `secret`
    /// options in the keychain. Without it they stay, for when it is installed again.
    #[serde(default)]
    pub data: Optional<bool>,
}
/// Uninstall a plugin. A linked one's directory stays: only the link goes. What it kept stays
/// too, unless `data` says.
#[api(
    name = "plugin.remove",
    example(args = r#"["owner/name", {}]"#, result = "null"),
    example(args = r#"["owner/name", {"data": true}]"#, result = "null", desc = "And what it kept.")
)]
pub fn pumi_plugin_remove(cx: &mut Cx, plugin: PluginId, opts: Optional<RemoveOpts>) -> Result<()> {
    let data = opts.into_option().unwrap_or_default().data.0.unwrap_or(false);
    let mut lock = lock_read();
    let name = lock_name(&lock, &plugin)?;
    let entry = lock.plugins[&name].clone();
    if entry.kind.as_deref() == Some("bundled") {
        return Err(Error::invalid_params(format!(
            "`{name}` is bundled with pumi and cannot be removed; turn it off with `plugin.update {name} {{enabled: false}}`"
        )));
    }
    let dir = dir_of(&name);
    // Read before its files go: its id names its data, its manifest its secrets.
    let plugin = data.then(|| pumi_api::manifest::read(&dir, entry.src.as_deref()).ok()).flatten();
    let gone = match std::fs::symlink_metadata(&dir) {
        Ok(m) if m.file_type().is_symlink() => unlink(&dir),
        Ok(_) => std::fs::remove_dir_all(&dir),
        Err(_) => Ok(()),
    };
    gone.map_err(|e| Error::exception(format!("{}: {e}", dir.display())))?;
    if entry.path.is_some() && entry.kind.as_deref() == Some("git") {
        let _ = std::fs::remove_dir_all(checkout_of(&name));
    }
    if let Some(plugin) = &plugin {
        forget(plugin)?;
    }
    lock.plugins.remove(&name);
    save(&lock)?;
    follow_plugins(cx)?;
    registry_changed(cx, &name)?;
    Ok(())
}
/// What `plugin` kept: its directories (its storage is in its `state` one) and its secrets.
fn forget(plugin: &pumi_api::manifest::Plugin) -> Result<()> {
    for kind in ["config", "state", "cache"] {
        let root = crate::dirs::stdpath(kind).expect("a stdpath");
        let dir = pumi_api::manifest::data_dir(&root, &plugin.id);
        match std::fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(Error::exception(format!("{}: {e}", dir.display())));
            }
            _ => {}
        }
    }
    forget_secrets(plugin);
    Ok(())
}
#[cfg(feature = "core-system")]
fn forget_secrets(plugin: &pumi_api::manifest::Plugin) {
    for account in plugin.secrets() {
        // A keychain this platform does not have holds none.
        let _ = pumi_os::secret_set(pumi_api::manifest::SECRET_SERVICE, &account, None);
    }
}
#[cfg(not(feature = "core-system"))]
fn forget_secrets(_package: &pumi_api::manifest::Plugin) {}
/// What a plugin brings: from its manifest (`package.json`'s `pumi`), `components`, `commands`,
/// `keys` and `options` (how many of each), `main` (it has code a worker runs) and `themes`;
/// from its files, `icons`. `theme` is any theme it brings.
fn contributes(dir: &Path, plugin: Option<&pumi_api::manifest::Plugin>) -> Value {
    let c = plugin.map(|p| &p.manifest.contributes);
    let themes = plugin.map_or(0, |p| p.manifest.themes.len());
    json!({
        "blocks": c.map_or(0, |c| c.blocks.len()),
        "renderers": c.map_or(0, |c| c.renderers.len()),
        "commands": c.map_or(0, |c| c.commands.len()),
        "keys": c.map_or(0, |c| c.keys.len()),
        "options": c.map_or(0, |c| c.options.len()),
        "main": plugin.is_some_and(|p| p.main.is_some()),
        "themes": themes,
        "theme": themes > 0,
        "icons": dir.join("icons").is_dir(),
    })
}
/// A plugin's `name`, `version` and `description`: its package.json's, else its `pumi.toml`'s, or `{}`.
fn meta(dir: &Path, plugin: Option<&pumi_api::manifest::Plugin>) -> Value {
    if let Some(p) = plugin {
        return json!({ "name": p.manifest.name, "version": p.version, "description": p.description });
    }
    std::fs::read_to_string(dir.join("pumi.toml"))
        .ok()
        .and_then(|t| toml::from_str::<toml::Value>(&t).ok())
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or_else(|| json!({}))
}
/// The installed plugins, or the ones named: `{spec, id, path, rev, kind, bundled, active,
/// disabled, installed, meta, manifest, error, contributes}` each. The plugins bundled with pumi
/// (`pumi/terminal`) are listed too: they can be turned off, not removed. `id` is `owner/name`
/// and `manifest` its package.json's `pumi`, read without running it; `error` is why that could
/// not be read (a plugin without one is not run). `meta` is its name, version and description,
/// and `contributes` what it brings.
pub(crate) fn installed(names: Vec<String>) -> Result<Vec<Value>> {
    let lock = lock_read();
    let mut out = Vec::new();
    for name in names_or_all(names, &lock)? {
        let entry = &lock.plugins[&name];
        let dir = dir_of(&name);
        let installed = dir.exists();
        let bundled = entry.kind.as_deref() == Some("bundled");
        let plugin = pumi_api::manifest::read(&dir, entry.src.as_deref()).and_then(|p| {
            if p.owner == crate::plugins::BUNDLED_OWNER && !bundled {
                return Err(format!("the owner `{}` is pumi's own plugins'", p.owner));
            }
            Ok(p)
        });
        let found = plugin.as_ref().ok();
        out.push(json!({
            "spec": { "name": name, "src": entry.src, "version": entry.version },
            "id": found.map(|p| p.id.clone()),
            "manifest": found.map(|p| &p.manifest),
            "error": plugin.as_ref().err(),
            "path": dir.display().to_string(),
            "rev": entry.rev,
            "kind": entry.kind,
            "bundled": bundled,
            "active": installed && !entry.disabled,
            "disabled": entry.disabled,
            "installed": installed,
            "meta": meta(&dir, found),
            "contributes": contributes(&dir, found),
        }));
    }
    Ok(out)
}
/// The store's listings (option `pluginindex`) matching every word of `query` in their id,
/// description, tags or keywords: `{id, name, owner, description, version, tags, keywords,
/// install, src, path, commit, downloads, downloads7d, updatedAt, createdAt, icon, images,
/// repo: {fullName, url, stars, …}}` each, `tags` holding `plugin` and/or `theme`; `install` is
/// what `plugin.add` takes. An empty query lists them all, most downloaded first.
pub(crate) fn available(k: &Kernel, query: &str) -> Result<Vec<Value>> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(str::to_string).collect();
    Ok(index(k, false)?
        .into_iter()
        .filter(|e| {
            let words_of = |key: &str| -> Vec<&str> {
                e[key].as_array().into_iter().flatten().filter_map(Value::as_str).collect()
            };
            let hay = format!(
                "{} {} {} {} {}",
                e["id"].as_str().unwrap_or_default(),
                e["name"].as_str().unwrap_or_default(),
                e["description"].as_str().unwrap_or_default(),
                words_of("tags").join(" "),
                words_of("keywords").join(" ")
            )
            .to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect())
}
/// A plugin's installed state changed: the app starts, stops or reloads it.
fn registry_changed(cx: &mut Cx, name: &str) -> Result<()> {
    let k = cx.host::<Kernel>()?;
    k.emit("registry.changed", "plugin", About::default(), json!({ "kind": "plugin", "names": [name] }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn github_shorthand_is_a_repository_and_a_folder() {
        assert_eq!(
            github_shorthand("someone/pumi-things"),
            Some(("https://github.com/someone/pumi-things".into(), None))
        );
        assert_eq!(
            github_shorthand("Some-One/pumi-things/plugins/git"),
            Some(("https://github.com/Some-One/pumi-things".into(), Some("plugins/git".into())))
        );
        assert_eq!(github_shorthand("caffeinate"), None);
        assert_eq!(github_shorthand("a/../b"), None);
        assert_eq!(github_shorthand("a/b c"), None);
    }
    #[test]
    fn store_ids_match_either_spelling_of_the_owner() {
        let index = vec![
            json!({ "id": "some_one/git", "name": "git", "src": "https://github.com/Some-One/x", "path": "git" }),
            json!({ "id": "other/git", "name": "git" }),
            json!({ "id": "other/night", "name": "night" }),
        ];
        assert_eq!(index_entry(&index, "Some-One/git").unwrap()["path"], "git");
        assert_eq!(index_entry(&index, "night").unwrap()["id"], "other/night");
        // Two have the name: an id is needed.
        assert!(index_entry(&index, "git").is_none());
        assert!(index_entry(&index, "nope").is_none());
    }
}
