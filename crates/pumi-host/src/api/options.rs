//! Options: the server's, an object's, and a plugin's own store. The object scopes are the core's
//! (it holds the model); a plugin's is pumi-host's, kept in
//! `state/plugins/<owner.name>/storage.json`, so a plugin keeps what it set across a restart, and
//! a `secret` option is kept in the OS keychain instead.
use super::kernel;
use crate::api::plugin::PluginId;
use crate::kernel::Kernel;
use crate::kernel::events::About;
use crate::kernel::options::{coerce, normalise};
use pumi_api::{Cx, Error, Optional, Ref, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
/// Which object an option is read from or set on, by scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OptionScope {
    Global,
    Session,
    View,
    Tab,
    /// One plugin's own store (`plugin` names it).
    Plugin,
}
/// What `option.get` takes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionOpts {
    /// `global`, `session`, `view`, `tab` or `plugin`. Without it: the option's own scope, at the
    /// object `id` names (the caller's own without that).
    #[serde(default)]
    pub scope: Optional<OptionScope>,
    /// The object: `$1`, `@2` or `%3`.
    #[serde(default)]
    pub id: Optional<Ref>,
    /// The plugin whose store it is, for `{scope: "plugin"}`: `owner/name`.
    #[serde(default)]
    pub plugin: Optional<PluginId>,
}
/// What `option.set` takes beside the value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetOptionOpts {
    #[serde(default)]
    pub scope: Optional<OptionScope>,
    #[serde(default)]
    pub id: Optional<Ref>,
    #[serde(default)]
    pub plugin: Optional<PluginId>,
    /// Seconds the value holds for; then it is the one it inherits again.
    #[serde(default)]
    pub ttl: Optional<f64>,
    /// Who set it (`report-agent`'s reporter); the caller's source by default.
    #[serde(default)]
    pub source: Optional<String>,
    /// With `source`: a report older than the last one from it is dropped.
    #[serde(default)]
    pub seq: Optional<i64>,
}
impl SetOptionOpts {
    fn is_plugin(&self) -> bool {
        self.scope.0 == Some(OptionScope::Plugin)
    }
    /// The scope and object as the core's `core.setOptionValue` takes them.
    fn for_core(&self) -> Result<Value> {
        let mut map = Map::new();
        if let Some(scope) = self.scope.0 {
            if scope == OptionScope::Plugin {
                return Err(Error::invalid_params("a plugin's options are pumi-host's"));
            }
            map.insert("scope".into(), json!(scope_name(scope)));
        }
        if let Some(id) = &self.id.0 {
            map.insert("id".into(), json!(id.to_string()));
        }
        if let Some(ttl) = self.ttl.0 {
            map.insert("ttl".into(), json!(ttl));
        }
        if let Some(source) = &self.source.0 {
            map.insert("source".into(), json!(source));
        }
        if let Some(seq) = self.seq.0 {
            map.insert("seq".into(), json!(seq));
        }
        Ok(Value::Object(map))
    }
}
fn scope_name(scope: OptionScope) -> &'static str {
    match scope {
        OptionScope::Global => "global",
        OptionScope::Session => "session",
        OptionScope::View => "view",
        OptionScope::Tab => "tab",
        OptionScope::Plugin => "plugin",
    }
}
/// An option's value in a scope: the object's own, else the one it inherits, else the default.
/// A plugin's is its own store, and a `secret` one is in the OS keychain.
#[api(
    name = "option.get",
    effect = "read",
    example(args = r#"["guifont", {}]"#, result = r#""Berkeley Mono 14""#),
    example(args = r#"["@build", {"scope": "global"}]"#, result = "null", desc = "A user option nobody set."),
    example(
        args = r#"["owner.name.count", {"scope": "plugin", "plugin": "owner/name"}]"#,
        result = "3",
        desc = "What a plugin kept."
    )
)]
pub fn pumi_get_option(cx: &mut Cx, name: String, opts: Optional<OptionOpts>) -> Result<Value> {
    let opts = opts.into_option().unwrap_or_default();
    if opts.scope.0 == Some(OptionScope::Plugin) {
        let plugin = plugin_of(&opts.plugin)?;
        return get_plugin_option(&plugin, &name);
    }
    let (caller, k) = kernel(cx)?;
    let mut map = Map::new();
    if let Some(scope) = opts.scope.0 {
        map.insert("scope".into(), json!(scope_name(scope)));
    }
    if let Some(id) = &opts.id.0 {
        map.insert("id".into(), json!(id.to_string()));
    }
    k.call_core(&caller, "core.getOptionValue", json!([name, Value::Object(map)]))
}
/// Set an option on an object, in a plugin's store, or globally. `@name` options need no
/// declaration. Setting a type's option is acting on the tab: a browser's `url` navigates.
/// `null` unsets it, so it inherits again. `ttl` makes the value expire, and a report whose `seq`
/// is not past the last one from the same `source` is dropped.
#[api(
    name = "option.set",
    example(args = r#"["@build", "ok", {"scope": "global"}]"#, result = "null"),
    example(args = r#"["remain_on_exit", true, {"id": "%1"}]"#, result = "null", desc = "On a tab."),
    example(args = r#"["@build", null, {"scope": "global"}]"#, result = "null", desc = "Unset it."),
    example(
        args = r#"["owner.name.count", 3, {"scope": "plugin", "plugin": "owner/name"}]"#,
        result = "null",
        desc = "What a plugin keeps."
    )
)]
pub fn pumi_set_option_value(cx: &mut Cx, name: String, value: Value, opts: Optional<SetOptionOpts>) -> Result<()> {
    let opts = opts.into_option().unwrap_or_default();
    if opts.is_plugin() {
        let plugin = plugin_of(&opts.plugin)?;
        return set_plugin_option(cx, &plugin, &name, value, &opts);
    }
    let (caller, k) = kernel(cx)?;
    let name = normalise(&name);
    let value = coerce(&k.model.options, &name, value)?;
    k.call_core(&caller, "core.setOptionValue", json!([name, value, opts.for_core()?])).map(|_| ())
}
fn plugin_of(plugin: &Optional<PluginId>) -> Result<String> {
    plugin
        .0
        .as_ref()
        .map(|p| p.0.clone())
        .ok_or_else(|| Error::invalid_params("`{scope: \"plugin\"}` names the plugin"))
}
/// The prefix a plugin's own option names start with (`owner.name.`), made from its id.
fn prefix(plugin: &str) -> String {
    let (owner, name) = plugin.split_once('/').unwrap_or(("", plugin));
    format!("{owner}.{}.", name.replace('-', "_"))
}
/// A plugin's store: `state/plugins/<owner.name>/storage.json`.
fn store_file(plugin: &str) -> PathBuf {
    crate::api::plugin::state_dir(plugin).join("storage.json")
}
/// A plugin's values' expiry and last report, beside its store.
fn reports_file(plugin: &str) -> PathBuf {
    crate::api::plugin::state_dir(plugin).join("option-reports.json")
}
/// One value's expiry and the last `seq` from each reporter.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Report {
    /// Unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires: Option<u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    seq: BTreeMap<String, i64>,
}
fn read<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| Error::exception(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(Error::exception(format!("{}: {e}", path.display()))),
    }
}
fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(|e| Error::exception(e.to_string()))?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    crate::dirs::write_atomic(path, &text).map_err(|e| Error::exception(format!("{}: {e}", path.display())))
}
fn now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}
/// The key a plugin's store keeps `name` under: the option's own name without the plugin's
/// prefix, or `name` as it is.
fn key_of(plugin: &str, name: &str) -> String {
    name.strip_prefix(&prefix(plugin)).map(str::to_string).unwrap_or_else(|| name.to_string())
}
/// Whether the plugin's manifest declares `name` a `secret`, kept in the OS keychain.
fn is_secret(plugin: &str, name: &str) -> bool {
    let lock = crate::plugins::lock_read();
    let Some(entry) = lock.plugins.get(plugin) else { return false };
    pumi_api::manifest::read(&crate::plugins::dir_of(plugin), entry.src.as_deref())
        .is_ok_and(|p| p.secrets().iter().any(|secret| secret == name))
}
fn get_plugin_option(plugin: &str, name: &str) -> Result<Value> {
    if is_secret(plugin, name) {
        return Ok(secret_get(name)?.map_or(Value::Null, Value::String));
    }
    let key = key_of(plugin, name);
    let mut store: Map<String, Value> = read(&store_file(plugin))?;
    let mut reports: BTreeMap<String, Report> = read(&reports_file(plugin))?;
    if reports.get(&key).and_then(|r| r.expires).is_some_and(|at| at <= now()) {
        reports.remove(&key);
        store.remove(&key);
        let _ = write(&reports_file(plugin), &reports);
        let _ = write(&store_file(plugin), &store);
        return Ok(Value::Null);
    }
    Ok(store.remove(&key).unwrap_or(Value::Null))
}
fn set_plugin_option(cx: &mut Cx, plugin: &str, name: &str, value: Value, opts: &SetOptionOpts) -> Result<()> {
    let key = key_of(plugin, name);
    let mut reports: BTreeMap<String, Report> = read(&reports_file(plugin))?;
    let source = match &opts.source.0 {
        Some(source) => source.clone(),
        None => {
            let caller = cx.caller().clone();
            cx.host::<Kernel>().map(|k| k.source(&caller)).unwrap_or_else(|_| "plugin".into())
        }
    };
    if let Some(seq) = opts.seq.0
        && reports.get(&key).and_then(|r| r.seq.get(&source)).is_some_and(|last| seq <= *last)
    {
        return Ok(());
    }
    let secret = is_secret(plugin, name);
    let mut store: Map<String, Value> = read(&store_file(plugin))?;
    let old = if secret { Value::Null } else { store.get(&key).cloned().unwrap_or(Value::Null) };
    if value.is_null() {
        if secret {
            secret_set(name, None)?;
        } else {
            store.remove(&key);
        }
    } else if secret {
        secret_set(name, value.as_str().filter(|text| !text.is_empty()).map(str::to_string))?;
    } else {
        store.insert(key.clone(), value.clone());
    }
    write(&store_file(plugin), &store)?;
    let report = reports.entry(key.clone()).or_default();
    if let Some(seq) = opts.seq.0 {
        report.seq.insert(source.clone(), seq);
    }
    report.expires = opts.ttl.0.filter(|ttl| *ttl > 0.0).map(|ttl| now() + ttl as u64);
    let expired = report.expires.is_none() && report.seq.is_empty();
    if expired {
        reports.remove(&key);
    }
    write(&reports_file(plugin), &reports)?;
    let value = if secret { Value::Null } else { value };
    cx.host::<Kernel>()?.emit(
        "option.changed",
        name,
        About::default(),
        json!({ "name": name, "scope": "plugin", "plugin": plugin, "old": old, "new": value, "source": source }),
    );
    Ok(())
}
/// A plugin's `secret` option, in the OS keychain; none where the platform has no keychain.
#[cfg(feature = "core-system")]
fn secret_set(name: &str, secret: Option<String>) -> Result<()> {
    pumi_os::secret_set(pumi_api::manifest::SECRET_SERVICE, name, secret.as_deref())
}
#[cfg(not(feature = "core-system"))]
fn secret_set(_name: &str, _secret: Option<String>) -> Result<()> {
    Err(Error::new(pumi_api::ErrorKind::ProviderUnavailable, "this system has no keychain"))
}
/// A plugin's `secret` option's value, from the OS keychain.
fn secret_get(name: &str) -> Result<Option<String>> {
    #[cfg(feature = "core-system")]
    {
        pumi_os::secret_get(pumi_api::manifest::SECRET_SERVICE, name)
    }
    #[cfg(not(feature = "core-system"))]
    {
        let _ = name;
        Err(Error::new(pumi_api::ErrorKind::ProviderUnavailable, "this system has no keychain"))
    }
}
