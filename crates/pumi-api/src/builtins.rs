use crate::events::EventInfo;
use crate::surface::{ModelInfo, TabMethodInfo, TraitInfo};
use crate::{API_LEVEL, Cx, ErrorKind, FunctionInfo, Result, api};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
/// Everything `api.getInfo` returns: the functions, types and errors, with pumi's metadata, and
/// the events, models, tab methods and traits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApiInfo {
    pub version: ApiVersion,
    /// Each error kind, with its JSON-RPC code and the CLI's exit status.
    pub error_types: BTreeMap<String, ErrorType>,
    pub functions: Vec<FunctionInfo>,
    /// Every event pumi fires itself.
    #[serde(default)]
    pub events: Vec<EventInfo>,
    /// What `<noun>.get` answers.
    #[serde(default)]
    pub models: Vec<ModelInfo>,
    /// What `tab.call` answers.
    #[serde(default, rename = "tabMethods")]
    pub tab_methods: Vec<TabMethodInfo>,
    /// The core's traits.
    #[serde(default)]
    pub traits: Vec<TraitInfo>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ApiVersion {
    pub api_level: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorType {
    pub code: i64,
    pub exit_code: i32,
}
impl ApiInfo {
    pub fn new(functions: Vec<FunctionInfo>) -> ApiInfo {
        let error_types = ErrorKind::ALL
            .into_iter()
            .map(|kind| (kind.name().to_string(), ErrorType { code: kind.rpc_code(), exit_code: kind.exit_code() }))
            .collect();
        let surface = crate::surface::surface();
        ApiInfo {
            version: ApiVersion { api_level: API_LEVEL },
            error_types,
            functions,
            events: surface.events.clone(),
            models: surface.models.clone(),
            tab_methods: surface.tab_methods.clone(),
            traits: surface.traits.clone(),
        }
    }
}
/// One call of a batch: a function and its arguments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchCall {
    pub name: String,
    #[serde(default)]
    pub args: Option<Value>,
}
/// What `call` calls: one function by name, or a batch run in order with nothing else between.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CallName {
    One(String),
    Batch(Vec<BatchCall>),
}
impl schemars::JsonSchema for CallName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CallName".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let call = generator.subschema_for::<BatchCall>();
        schemars::json_schema!({
            "oneOf": [{ "type": "string" }, { "type": "array", "items": call, "minItems": 1 }]
        })
    }
}
/// Describe every function there is: core and created, with parameters, schemas and examples.
/// `pumi --api-info` prints it, and every generator reads it.
#[api(name = "api.getInfo", internal, effect = "read", example(args = "[]", desc = "The whole API, as data."))]
pub fn pumi_get_api_info(cx: &mut Cx) -> Result<ApiInfo> {
    Ok(ApiInfo::new(cx.registry().functions()))
}
/// Call any function by name, as the same caller. `args` is positional (an array) or named (an
/// object); leaving it out calls the function with no arguments. A list of `{name, args}` is a
/// batch: called in order with nothing else running in between, answering every result; it stops
/// at the first error, which names the call, and what earlier calls did is not undone.
#[api(
    name = "call",
    effect = "forward",
    example(args = r#"["api.getInfo", []]"#, desc = "Positional arguments."),
    example(args = r#"{"name": "api.getInfo", "args": {}}"#, desc = "Named arguments, as the CLI sends them."),
    example(
        args = r#"[[{"name": "call", "args": ["api.getInfo"]}, {"name": "call", "args": ["api.getInfo"]}]]"#,
        desc = "A batch: two calls in one round trip."
    )
)]
pub fn pumi_call(cx: &mut Cx, name: CallName, args: Option<Value>) -> Result<Value> {
    match name {
        CallName::One(name) => cx.call(&name, args.unwrap_or(Value::Null)),
        CallName::Batch(calls) => {
            if args.is_some() {
                return Err(crate::Error::invalid_params("a batch's calls carry their own `args`"));
            }
            let mut results = Vec::with_capacity(calls.len());
            for (index, call) in calls.into_iter().enumerate() {
                match cx.call(&call.name, call.args.unwrap_or(Value::Null)) {
                    Ok(value) => results.push(value),
                    Err(err) => {
                        let message = format!("call {index} ({}): {}", call.name, err.message);
                        return Err(crate::Error::new(err.kind, message));
                    }
                }
            }
            Ok(Value::Array(results))
        }
    }
}
