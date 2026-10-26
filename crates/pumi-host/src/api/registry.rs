//! The registry: one list of what is registered, by kind. The core holds the functions, blocks,
//! traits and options; pumi-host holds the key tables, the menus, the plugins and their themes
//! and viewers, the methods `tab.call` answers and the models, so it answers here and asks the
//! core for the rest.
use super::kernel;
use crate::kernel::Kernel;
use pumi_api::{Cx, Error, Optional, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::borrow::Cow;
/// The kinds `registry.list` lists, in the order the export names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RegistryKind {
    /// Every function, core and provided.
    Fn,
    /// What a tab holds.
    Block,
    /// What a tab is beside its block.
    Trait,
    /// A declared option.
    Option,
    /// A key binding.
    Key,
    /// A menu item.
    Menu,
    /// An installed plugin, or with `available` one in the store.
    Plugin,
    /// A colour scheme a plugin brings.
    Theme,
    /// A plugin that can draw a file, for a `path`.
    Viewer,
    /// What `tab.call` answers: a method, its arguments and the blocks that answer it.
    #[serde(rename = "tabMethod")]
    TabMethod,
    /// What a `get` answers: a model and its fields.
    Model,
    /// A plugin's page or section in Settings (`contributes.settings`).
    Settings,
}
/// What `registry.list` takes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RegistryOpts {
    /// One entry by its name (a function's, a block's, a trait's, an option's, a menu's path, a
    /// tab method's; a model's, in any case).
    #[serde(default)]
    pub name: Optional<String>,
    /// `plugin`: search the store's listings for `name` instead of the installed ones.
    #[serde(default)]
    pub available: Optional<bool>,
    /// `viewer`: the file the viewer is for.
    #[serde(default)]
    pub path: Optional<String>,
    /// `fn`: the palette's entries, with the title a manifest's command gives them.
    #[serde(default)]
    pub palette: Optional<bool>,
}
/// Entries of one kind: an object each, of whatever that kind is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entries(pub Vec<Value>);
impl schemars::JsonSchema for Entries {
    fn schema_name() -> Cow<'static, str> {
        "Entries".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {},
                "required": [],
                "additionalProperties": true,
            }
        })
    }
}
/// Whether `name` names `wanted`: exactly, or by a trailing `*` glob (`pumi_dev.*`).
fn names(wanted: Option<&str>, name: &str) -> bool {
    match wanted {
        None => true,
        Some(wanted) => match wanted.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => name == wanted,
        },
    }
}
/// Registered entries of one kind: functions, blocks, traits and options; the key bindings; the
/// menus; the plugins (or the store's, with `available`); the themes; the viewers of a file
/// (`path`); the methods `tab.call` answers; the models; and the plugins' pages and sections in
/// Settings. `{name}` picks one entry.
#[api(
    name = "registry.list",
    effect = "read",
    example(args = r#"["fn", {"palette": true}]"#, result = r#"[{"name": "owner.name.build", "kind": "fn"}]"#),
    example(args = r#"["plugin", {}]"#, desc = "The installed plugins.")
)]
pub fn pumi_registry_list(cx: &mut Cx, kind: RegistryKind, opts: Optional<RegistryOpts>) -> Result<Entries> {
    let opts = opts.into_option().unwrap_or_default();
    let wanted = opts.name.0.as_deref();
    let (caller, k) = kernel(cx)?;
    let entries = match kind {
        RegistryKind::Fn => functions(&caller, k, wanted, opts.palette.0.unwrap_or(false))?,
        RegistryKind::Block => filter(k.call_core(&caller, "core.blocks", json!([]))?, wanted),
        RegistryKind::Trait => filter(k.call_core(&caller, "core.traits", json!([]))?, wanted),
        RegistryKind::Option => filter(k.call_core(&caller, "core.options", json!([]))?, wanted),
        RegistryKind::Key => keys(k),
        RegistryKind::Menu => crate::api::menu::tree(k, wanted)?,
        RegistryKind::Plugin if opts.available.0.unwrap_or(false) => {
            crate::api::plugin::available(k, wanted.unwrap_or_default())?
        }
        RegistryKind::Plugin => crate::api::plugin::installed(wanted.map(str::to_string).into_iter().collect())?,
        RegistryKind::Theme => themes(wanted),
        RegistryKind::Viewer => {
            let path = opts.path.0.as_deref().unwrap_or_default();
            viewers(path)?
        }
        RegistryKind::TabMethod => tab_methods(&caller, k, wanted)?,
        RegistryKind::Model => models(wanted),
        RegistryKind::Settings => settings(wanted),
    };
    if let Some(wanted) = wanted
        && entries.is_empty()
    {
        return Err(Error::not_found(format!("no {kind:?} `{wanted}`")));
    }
    Ok(Entries(entries))
}
/// The functions the server has: what `api.getInfo` describes of them, or with `palette` the
/// entries of the manifests' commands, which are what the palette lists.
fn functions(caller: &pumi_api::Caller, k: &mut Kernel, wanted: Option<&str>, palette: bool) -> Result<Vec<Value>> {
    let mut functions: Vec<Value> =
        k.call_core(caller, "core.functions", json!([]))?.as_array().cloned().unwrap_or_default();
    if palette {
        let bound = bindings(k);
        let mut entries = Vec::new();
        for plugin in crate::contributions::plugins() {
            for command in &plugin.manifest.contributes.commands {
                let name = plugin.prefixed(&command.id);
                if !names(wanted, &name) {
                    continue;
                }
                let keys: Vec<&str> =
                    bound.iter().filter(|(_, call)| *call == name).map(|(key, _)| key.as_str()).collect();
                entries.push(json!({
                    "name": name,
                    "kind": "fn",
                    "title": command.title,
                    "keys": keys,
                }));
            }
        }
        entries.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        return Ok(entries);
    }
    functions.retain(|f| f["name"].as_str().is_some_and(|name| names(wanted, name)));
    functions.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(functions)
}
/// The server's key bindings, as the `keys` option holds them: `{table: {key: {call, args}}}`.
fn keys(k: &Kernel) -> Vec<Value> {
    let mut out = Vec::new();
    for (table, keys) in k.global_option("keys").as_object().into_iter().flatten() {
        for (key, binding) in keys.as_object().into_iter().flatten() {
            out.push(json!({
                "table": table,
                "key": key,
                "call": binding.get("call").cloned().unwrap_or(Value::Null),
                "args": binding.get("args").cloned().unwrap_or(Value::Null),
            }));
        }
    }
    out
}
/// Every key binding's key and the function it calls, for the palette to show beside a command.
fn bindings(k: &Kernel) -> Vec<(String, String)> {
    keys(k)
        .into_iter()
        .filter_map(|b| Some((b["key"].as_str()?.to_string(), b["call"].as_str()?.to_string())))
        .collect()
}
/// The colour schemes the installed plugins bring: `themes/<name>` in a manifest is `name`.
fn themes(wanted: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    for plugin in crate::contributions::plugins() {
        for path in &plugin.manifest.themes {
            let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
            if names(wanted, name) {
                out.push(json!({ "name": name, "plugin": plugin.id, "path": path }));
            }
        }
    }
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}
/// The plugins' pages and sections in Settings: `{kind: "page" | "section", plugin, id, title,
/// component, keywords}`, a page's `icon` and a section's `page`. A section's `id` is its plugin's
/// prefix and its own (`me.remote.tailscale`), as `openSettings` links to it; so is a page's.
fn settings(wanted: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    for plugin in crate::contributions::plugins() {
        let own = &plugin.manifest.contributes.settings;
        for page in &own.pages {
            let id = plugin.prefixed(&page.id);
            if names(wanted, &id) {
                out.push(json!({ "kind": "page", "plugin": plugin.id, "id": id, "title": page.title,
                    "icon": page.icon, "component": page.component, "keywords": page.keywords }));
            }
        }
        for section in &own.sections {
            let id = plugin.prefixed(&section.id);
            let page = match own.pages.iter().any(|p| p.id == section.page) {
                true => plugin.prefixed(&section.page),
                false => section.page.clone(),
            };
            if names(wanted, &id) {
                out.push(json!({ "kind": "section", "plugin": plugin.id, "id": id, "page": page,
                    "title": section.title, "component": section.component, "keywords": section.keywords }));
            }
        }
    }
    out
}
/// The plugins that can draw the file at `path`.
fn viewers(path: &str) -> Result<Vec<Value>> {
    if path.is_empty() {
        return Err(Error::invalid_params("`registry.list(\"viewer\")` takes a file's `path`"));
    }
    Ok(crate::api::files::renderers(path)
        .into_iter()
        .filter_map(|(plugin, export, fit)| Some(json!({ "plugin": plugin, "export": export, "fit": fit? })))
        .collect())
}
/// The methods `tab.call` answers — `read`, a terminal's `run` — with the schema of each one's
/// arguments and of its answer, and the blocks that answer it (`surface.json`'s `tabMethods`).
fn tab_methods(caller: &pumi_api::Caller, k: &mut Kernel, wanted: Option<&str>) -> Result<Vec<Value>> {
    let blocks: Vec<Value> = k
        .call_core(caller, "core.blocks", json!([]))?
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block.get("id").cloned())
        .collect();
    let mut out = Vec::new();
    for method in &pumi_api::surface::surface().tab_methods {
        if !names(wanted, &method.name) {
            continue;
        }
        let answers: Vec<Value> = if method.blocks.is_empty() {
            blocks.clone()
        } else {
            method.blocks.iter().map(|id| json!(id)).filter(|id| blocks.contains(id)).collect()
        };
        out.push(json!({
            "name": method.name,
            "desc": method.desc,
            "paramsSchema": method.params_schema,
            "returnsSchema": method.returns_schema,
            "blocks": answers,
        }));
    }
    Ok(out)
}

/// The models `session.get`, `view.get`, `tab.get`, `request.get` and `registry.list("plugin")`
/// answer (`surface.json`'s `models`), by name in any case: `pumi help session` describes `Session`.
fn models(wanted: Option<&str>) -> Vec<Value> {
    pumi_api::surface::surface()
        .models
        .iter()
        .filter(|model| wanted.is_none_or(|wanted| model.name.eq_ignore_ascii_case(wanted)))
        .map(|model| json!(model))
        .collect()
}

/// Entries of a kind the core lists, by `name` when one is given.
fn filter(value: Value, wanted: Option<&str>) -> Vec<Value> {
    value
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| {
            ["name", "id"].iter().any(|field| entry[*field].as_str().is_some_and(|name| names(wanted, name)))
        })
        .collect()
}
