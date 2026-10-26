//! Blocks: what a tab holds. One registry for the core's (`terminal`) and the enabled plugins'
//! (`owner.name.id`): `registry.list("block")` lists both, and `tab.open` opens either.
use super::kernel;
use crate::kernel::events::About;
use crate::kernel::types::TabType;
use crate::tab::generic::GenericType;
use pumi_api::{Cx, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::rc::Rc;
/// A block, as `registry.list("block")` describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BlockInfo {
    /// `terminal`, or a plugin's `owner.name.id`.
    pub id: String,
    /// Who declares it: `core`, or the plugin (`owner/name`).
    pub provider: String,
    /// The version of its saved states' `data`.
    pub version: u32,
    pub strategy: crate::kernel::state::Strategy,
    /// Its own methods, beside the protocol's `read`, `input` and `selection`.
    pub methods: Vec<String>,
    /// The options it owns.
    pub options: Vec<String>,
}
/// Every block there is: the core's, and the enabled plugins' (their manifests' `blocks`), each
/// with the version, strategy, methods and capabilities it has.
#[api(
    name = "core.blocks",
    internal,
    effect = "read",
    example(
        args = "[]",
        result = r#"[{"id": "file", "provider": "core", "version": 1, "strategy": "reference", "methods": ["at", "reveal", "report", "save"], "options": ["path"]}, {"id": "terminal", "provider": "core", "version": 1, "strategy": "relaunch", "methods": ["respawn", "signal", "clear_history", "modes"], "options": []}]"#
    )
)]
pub fn pumi_block_list(cx: &mut Cx) -> Result<Vec<BlockInfo>> {
    let (_, k) = kernel(cx)?;
    let core = k.blocks.values().map(|b| (b.clone(), "core".to_string()));
    let plugins = k
        .plugin_blocks
        .values()
        .filter(|b| !k.blocks.contains_key(&b.id))
        .map(|b| (b.clone() as Rc<dyn TabType>, b.provider.clone()));
    Ok(core
        .chain(plugins)
        .map(|(block, provider)| BlockInfo {
            id: block.name().to_string(),
            provider,
            version: block.version(),
            strategy: block.strategy(),
            methods: block.methods(),
            options: k
                .model
                .options
                .decls()
                .filter(|d| d.blocks.iter().any(|b| b == block.name()))
                .map(|d| d.name.clone())
                .collect(),
        })
        .collect())
}
/// The enabled plugins' blocks, which `registry.list("block")` lists and `tab.open` opens with
/// their version, strategy and methods: pumi-host sends them all again whenever a plugin is
/// added, removed, turned on or off, or reloaded.
#[api(
    name = "core.setBlocks",
    internal,
    example(
        args = r#"[[{"id": "pumi_dev.browser.page", "provider": "pumi_dev/browser", "version": 1, "strategy": "exact", "methods": ["navigate"]}]]"#,
        result = "null"
    )
)]
pub fn pumi_core_set_blocks(cx: &mut Cx, blocks: Vec<GenericType>) -> Result<()> {
    let (_, k) = kernel(cx)?;
    let mut plugin_blocks = BTreeMap::new();
    for block in blocks {
        if let Some(why) = block.refused() {
            return Err(pumi_api::Error::invalid_params(why));
        }
        plugin_blocks.insert(block.id.clone(), Rc::new(block));
    }
    if plugin_blocks != k.plugin_blocks {
        k.plugin_blocks = plugin_blocks;
        k.dirty = true;
        let names: Vec<String> = k.plugin_blocks.keys().cloned().collect();
        k.emit("registry.changed", "block", About::default(), json!({ "kind": "block", "names": names }));
    }
    Ok(())
}
