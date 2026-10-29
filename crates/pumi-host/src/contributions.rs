//! What the enabled plugins contribute, registered from their manifests without running them:
//! their options are declared (`owner.name.<key>`, which Settings shows on the plugin's page),
//! their keys mapped in the `root` table, each calling its command (`owner.name.<command>`),
//! which the app provides for the plugin and starts its worker on, and their blocks
//! (`owner.name.<id>`, with a block's `version`, `strategy` and `methods`) told to the server,
//! which `block.list` lists and `tab.open` opens. A plugin that goes takes its keys and blocks
//! with it.
//!
//! A running plugin can change what it contributes (`plugin.contribute`): what it says is kept
//! here, in place of its manifest's `contributes`, until it gives it back or stops.
use crate::kernel::Kernel;
use crate::plugins::{dir_of, lock_read};
use pumi_api::Cx;
use pumi_api::manifest::{Contributes, Plugin};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Mutex;
/// What each plugin contributes now, by id, where it changed that while it runs.
static LIVE: Mutex<BTreeMap<String, Contributes>> = Mutex::new(BTreeMap::new());
/// `plugin` as it is now: its manifest, with what it contributes while it runs.
pub fn live(mut plugin: Plugin) -> Plugin {
    if let Some(now) = LIVE.lock().unwrap_or_else(|e| e.into_inner()).get(&plugin.id) {
        plugin.manifest.contributes = now.clone();
    }
    plugin
}
/// What plugin `id` contributes from now on; `None` is its manifest's again.
pub fn set_live(id: &str, now: Option<Contributes>) {
    let mut live = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    match now {
        Some(now) => live.insert(id.to_string(), now),
        None => live.remove(id),
    };
}
/// `patch` applied to `target` as a JSON merge patch (RFC 7396): an object merges into an object,
/// `null` takes a key away, anything else (a list too) replaces what was there.
pub fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Default::default());
    }
    let Value::Object(target) = target else { unreachable!() };
    for (key, value) in patch {
        if value.is_null() {
            target.remove(key);
        } else {
            merge_patch(target.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}
/// The keys mapped for each plugin, by id: `(table, key)`.
#[derive(Default)]
struct Mapped(BTreeMap<String, Vec<(String, String)>>);
/// The enabled plugins that have a manifest.
pub fn plugins() -> Vec<Plugin> {
    lock_read()
        .plugins
        .into_iter()
        .filter(|(_, e)| !e.disabled)
        .filter_map(|(name, e)| pumi_api::manifest::read(&dir_of(&name), e.src.as_deref()).ok())
        .map(live)
        .collect()
}
/// Declares every enabled plugin's options, maps its keys and lists its blocks; unmaps the
/// keys of plugins gone.
pub fn follow(cx: &mut Cx) {
    let mut now: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut blocks = Vec::new();
    let mut traits = Vec::new();
    for p in plugins() {
        blocks.extend(p.manifest.contributes.blocks.iter().map(|b| p.block_spec(b)));
        traits.extend(p.manifest.contributes.traits.iter().map(|t| p.trait_spec(t)));
        for (key, spec) in &p.manifest.contributes.options {
            let Some(declaration) = p.declaration(spec) else { continue };
            if let Err(err) = cx.call("option.declare", json!([p.prefixed(key), declaration])) {
                crate::server::log(&format!("{}: option {key}: {}", p.id, err.message));
            }
        }
        let mut mapped = Vec::new();
        for key in &p.manifest.contributes.keys {
            let call = crate::callback::Call { call: p.prefixed(&key.command), args: None };
            match crate::api::keymap::set_binding(cx, "root", &key.key, call, &p.id) {
                Ok(_) => mapped.push(("root".to_string(), key.key.clone())),
                Err(err) => crate::server::log(&format!("{}: key {}: {}", p.id, key.key, err.message)),
            }
        }
        now.insert(p.id.clone(), mapped);
    }
    if let Err(err) = cx.call("core.setBlocks", json!([blocks])) {
        crate::server::log(&format!("blocks: {}", err.message));
    }
    if let Err(err) = cx.call("core.setTraits", json!([traits])) {
        crate::server::log(&format!("traits: {}", err.message));
    }
    let Ok(k) = cx.host::<Kernel>() else { return };
    let before = std::mem::take(&mut k.ext.get_or_default::<Mapped>().0);
    for (id, keys) in before {
        let kept = now.get(&id);
        for (table, key) in keys {
            if kept.is_none_or(|k| !k.contains(&(table.clone(), key.clone()))) {
                let _ = crate::api::keymap::del_binding(cx, &table, &key, "plugin");
            }
        }
    }
    if let Ok(k) = cx.host::<Kernel>() {
        k.ext.get_or_default::<Mapped>().0 = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_merge_patch_merges_objects_removes_nulls_and_replaces_the_rest() {
        let mut now = json!({ "slots": { "tab.toolbar": { "component": "Toolbar" } }, "commands": [{ "id": "a" }] });
        let patch = json!({
            "slots": { "tab.new": { "actions": [{ "id": "pi" }] }, "tab.toolbar": null },
            "commands": [{ "id": "b" }],
        });
        merge_patch(&mut now, &patch);
        assert_eq!(
            now,
            json!({ "slots": { "tab.new": { "actions": [{ "id": "pi" }] } }, "commands": [{ "id": "b" }] })
        );
        merge_patch(&mut now, &json!({ "slots": 3 }));
        assert_eq!(now["slots"], json!(3));
    }
}
