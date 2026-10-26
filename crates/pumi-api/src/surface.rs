//! What the API is beside its functions: the models `<noun>.get` answers, the methods `tab.call`
//! answers, the core's traits and the events pumi fires. They are data (`surface.json`), which
//! `api.getInfo`, the export, the registry and the core's trait list all read.
use crate::events::EventInfo;
use crate::meta::ExampleInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;
/// A model: what `<kind>.get` answers, its id's prefix, its fields, the methods its SDK class
/// has and the events about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelInfo {
    pub name: String,
    pub kind: String,
    /// What its id starts with: `%` a tab's; empty for a plugin's `owner/name`.
    #[serde(rename = "idPrefix")]
    pub id_prefix: String,
    pub desc: String,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<ModelMethod>,
    pub events: Vec<String>,
}
/// A model's or a trait's field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FieldInfo {
    pub name: String,
    /// Its type as TypeScript writes it (`ViewId | null`, `Layout`).
    #[serde(rename = "type")]
    pub ty: String,
    pub desc: String,
    /// Whether `<model>.changed` names it in its `keys`.
    pub watch: bool,
    pub schema: Value,
}
/// A model's method: the function it calls with the model's id first.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelMethod {
    pub name: String,
    #[serde(rename = "fn")]
    pub function: String,
    /// For `tab.call`: the tab method it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}
/// A method `tab.call` answers: its arguments, its answer, and the blocks that answer it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TabMethodInfo {
    pub name: String,
    pub desc: String,
    pub params_schema: Value,
    pub returns_schema: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<ExampleInfo>,
    /// The blocks whose own method it is (`run` and `attach`: a terminal's); empty is every block.
    pub blocks: Vec<String>,
}
/// A core trait and its fields: a tab has it while its option `@<name>` is set, and its fields
/// are the options `@<name>.<field>`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TraitInfo {
    pub name: String,
    pub desc: String,
    pub fields: Vec<FieldInfo>,
}
/// Everything `surface.json` holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Surface {
    pub models: Vec<ModelInfo>,
    #[serde(rename = "tabMethods")]
    pub tab_methods: Vec<TabMethodInfo>,
    pub traits: Vec<TraitInfo>,
    /// Every event pumi fires for plugins.
    pub events: Vec<EventInfo>,
}
/// The surface, read once.
pub fn surface() -> &'static Surface {
    static SURFACE: OnceLock<Surface> = OnceLock::new();
    SURFACE.get_or_init(|| serde_json::from_str(include_str!("surface.json")).expect("surface.json is a Surface"))
}
