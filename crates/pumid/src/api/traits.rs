//! Traits: what a tab is beside its block (an agent, a file, …). A tab has trait `x` while its
//! option `@x` is set, and a trait's fields are its options `@x.<field>`: there are no verbs, only
//! options, and the tab's `changed` says when a tab gains or loses one. The core defines none
//! (`surface.json`'s `traits`); a plugin's are `owner.name.x`, from its manifest (`core.setTraits`).
use super::kernel;
use crate::kernel::Kernel;
use crate::kernel::TabId;
use crate::kernel::events::About;
use crate::kernel::options::Key;
use pumi_api::{Cx, Error, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
/// A trait, as `registry.list("trait")` describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TraitInfo {
    /// A plugin's `owner.name.x`.
    pub id: String,
    /// Who defines it: `core`, or the plugin (`owner/name`).
    #[serde(default)]
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Its fields: the options `@<id>.<field>`.
    #[serde(default)]
    pub fields: Vec<String>,
}
impl TraitInfo {
    /// The option a tab has it by: `@<id>`.
    pub fn key(&self) -> String {
        format!("@{}", self.id)
    }
}
/// The core's traits, as the export describes them (`pumi_api::surface`).
pub fn core() -> Vec<TraitInfo> {
    pumi_api::surface::surface()
        .traits
        .iter()
        .map(|t| TraitInfo {
            id: t.name.clone(),
            provider: "core".into(),
            desc: Some(t.desc.clone()),
            fields: t.fields.iter().map(|f| f.name.clone()).collect(),
        })
        .collect()
}
/// Whether `tab` has trait `id`: its own `@<id>` is set and not empty.
pub fn has(kernel: &Kernel, tab: TabId, id: &str) -> bool {
    kernel.model.options.get_local(&format!("@{id}"), Key::Tab(tab)).is_some_and(|set| match &set.value {
        Value::Null | Value::Bool(false) => false,
        Value::String(s) => !s.is_empty(),
        _ => true,
    })
}
/// The traits `tab` has, in `registry.list("trait")`'s order.
pub fn of(kernel: &Kernel, tab: TabId) -> Vec<String> {
    core()
        .into_iter()
        .chain(kernel.plugin_traits.iter().cloned())
        .filter(|t| has(kernel, tab, &t.id))
        .map(|t| t.id)
        .collect()
}
/// Every trait there is: the core's, and the enabled plugins' (their manifests'
/// `traits`). A tab has trait `x` while its option `@x` is set; its fields are `@x.<field>`.
#[api(
    name = "core.traits",
    internal,
    effect = "read",
    example(
        args = "[]",
        result = r#"[{"id": "pumi_dev.files.file", "provider": "pumi_dev/files", "fields": ["path"]}]"#
    )
)]
pub fn pumi_trait_list(cx: &mut Cx) -> Result<Vec<TraitInfo>> {
    let (_, k) = kernel(cx)?;
    Ok(core().into_iter().chain(k.plugin_traits.iter().cloned()).collect())
}
/// The enabled plugins' traits, which `registry.list("trait")` lists: pumi-host sends them all
/// again whenever a plugin is added, removed, turned on or off, or reloaded.
#[api(
    name = "core.setTraits",
    internal,
    example(
        args = r#"[[{"id": "pumi_dev.files.file", "provider": "pumi_dev/files", "fields": ["path"]}]]"#,
        result = "null"
    )
)]
pub fn pumi_core_set_traits(cx: &mut Cx, traits: Vec<TraitInfo>) -> Result<()> {
    let (_, k) = kernel(cx)?;
    for t in &traits {
        if t.id.split('.').count() != 3 || !pumi_model::options::valid_name(&t.id) {
            return Err(Error::invalid_params(format!("`{}` is not a plugin trait's id (`owner.name.x`)", t.id)));
        }
    }
    if traits != k.plugin_traits {
        k.plugin_traits = traits;
        k.dirty = true;
        let names: Vec<String> = k.plugin_traits.iter().map(|t| t.id.clone()).collect();
        k.emit("registry.changed", "trait", About::default(), json!({ "kind": "trait", "names": names }));
    }
    Ok(())
}
