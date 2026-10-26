use crate::registry::Handler;
use crate::{Error, Result};
use schemars::{Schema, SchemaGenerator, generate::SchemaSettings};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
/// Where a function runs; the router sends a call there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunsOn {
    Server,
    Client,
    Either,
}
/// What a call does to the server's state. `write` calls go to the op log; `forward` calls
/// (`call`) only pass a call on, so the call they pass is logged instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Read,
    Write,
    Forward,
}
/// One parameter of a core function.
pub struct Param {
    pub name: &'static str,
    /// `Option<T>`: it may be left out, positionally at the end or by name anywhere.
    pub optional: bool,
    pub schema: fn(&mut SchemaGenerator) -> Schema,
}
/// One example, as JSON text: `args` is positional (an array) or named (an object).
pub struct Example {
    pub args: &'static str,
    pub result: Option<&'static str>,
    pub desc: Option<&'static str>,
}
/// A core function, as `#[api]` registers it in [`crate::FUNCTIONS`].
pub struct Function {
    pub name: &'static str,
    pub desc: &'static str,
    pub since: u32,
    pub runs_on: RunsOn,
    pub effect: Effect,
    /// Wire-only: callable over the socket (by the app, the CLI, Settings, and the core's own
    /// `core.*` primitives), left out of listings, the export and the stubs.
    pub internal: bool,
    /// Empty means every platform.
    pub platforms: &'static [&'static str],
    pub params: &'static [Param],
    pub returns: fn(&mut SchemaGenerator) -> Schema,
    pub examples: &'static [Example],
    pub notes: &'static [&'static str],
    pub call: Handler,
    /// Parses arguments (and a result) without calling: how examples are checked.
    pub check: fn(Value, Option<Value>) -> Result<()>,
    pub module: &'static str,
}
impl Function {
    /// The owned description `api.getInfo` returns.
    pub fn info(&self) -> FunctionInfo {
        let mut generator = generator();
        let mut properties = Map::new();
        let mut required = Vec::new();
        let mut parameters = Vec::new();
        for param in self.params {
            let schema = (param.schema)(&mut generator);
            parameters.push((String::new(), param.name.to_string()));
            properties.insert(param.name.to_string(), schema.to_value());
            if !param.optional {
                required.push(Value::String(param.name.to_string()));
            }
        }
        let defs = generator.take_definitions(true);
        let mut recursive = false;
        for schema in properties.values_mut() {
            recursive |= inline_refs(schema, &defs, &mut Vec::new());
        }
        let mut params_schema = json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        });
        if recursive {
            params_schema["$defs"] = Value::Object(defs.clone());
        }
        for (slot, param) in parameters.iter_mut().zip(self.params) {
            slot.0 = nvim_type(&params_schema["properties"][param.name], &defs);
        }
        let mut generator = self::generator();
        let mut returns_schema = (self.returns)(&mut generator).to_value();
        let defs = generator.take_definitions(true);
        let returns = nvim_type(&returns_schema, &defs);
        // As the parameters' are: inlined, with the definitions only for a recursive one.
        if inline_refs(&mut returns_schema, &defs, &mut Vec::new())
            && let Value::Object(map) = &mut returns_schema
        {
            map.insert("$defs".into(), Value::Object(defs));
        }
        FunctionInfo {
            name: self.name.to_string(),
            since: self.since,
            provider: "server".to_string(),
            runs_on: self.runs_on,
            effect: self.effect,
            platforms: self.platforms.iter().map(|p| p.to_string()).collect(),
            desc: self.desc.to_string(),
            parameters,
            params_schema,
            returns,
            returns_schema,
            examples: self
                .examples
                .iter()
                .map(|example| ExampleInfo {
                    args: serde_json::from_str(example.args).unwrap_or(Value::Null),
                    result: example.result.map(|r| serde_json::from_str(r).unwrap_or(Value::Null)),
                    desc: example.desc.map(str::to_string),
                })
                .collect(),
            notes: self.notes.iter().map(|n| n.to_string()).collect(),
            source: "core".to_string(),
            internal: self.internal,
        }
    }
    /// Every example parses, fits the parameters, and its result fits the return type.
    pub fn check_examples(&self) -> Result<()> {
        for (i, example) in self.examples.iter().enumerate() {
            let args: Value = serde_json::from_str(example.args).map_err(|err| {
                Error::invalid_params(format!("{}: example {} args are not JSON: {err}", self.name, i + 1))
            })?;
            let result = match example.result {
                Some(text) => Some(serde_json::from_str(text).map_err(|err| {
                    Error::invalid_params(format!("{}: example {} result is not JSON: {err}", self.name, i + 1))
                })?),
                None => None,
            };
            (self.check)(args, result)
                .map_err(|err| Error::invalid_params(format!("{}: example {}: {}", self.name, i + 1, err.message)))?;
        }
        Ok(())
    }
}
/// A function as data: `name`, `since`, `parameters` and `return_type`, then its
/// schemas, examples and notes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FunctionInfo {
    pub name: String,
    pub since: u32,
    /// `server`, a plugin's name, or `client`.
    pub provider: String,
    pub runs_on: RunsOn,
    pub effect: Effect,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    pub desc: String,
    /// `[[type, name], …]`.
    pub parameters: Vec<(String, String)>,
    pub params_schema: Value,
    pub returns: String,
    pub returns_schema: Value,
    pub examples: Vec<ExampleInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// `core`, or the plugin that created it.
    pub source: String,
    /// Wire-only: callable, and left out of `api.getInfo`, the export, the SDK and the docs (the
    /// app's, the CLI's and Settings' own).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub internal: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ExampleInfo {
    pub args: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
}
impl FunctionInfo {
    /// What every function must have; a function without them is refused.
    pub fn validate(&self) -> Result<()> {
        if !is_function_name(&self.name) {
            return Err(Error::invalid_params(format!(
                "`{}` is not a function name: letters, digits, `_` and `.`, starting with a letter",
                self.name
            )));
        }
        if self.desc.trim().is_empty() {
            return Err(Error::invalid_params(format!("`{}` has no description", self.name)));
        }
        if self.examples.is_empty() {
            return Err(Error::invalid_params(format!("`{}` has no example", self.name)));
        }
        if !self.params_schema.is_object() {
            return Err(Error::invalid_params(format!("`{}` has no parameter schema", self.name)));
        }
        Ok(())
    }
    /// The parameter names, in order.
    pub fn param_names(&self) -> Vec<String> {
        self.parameters.iter().map(|(_, name)| name.clone()).collect()
    }
}
/// `tab.open`, `agents.list`, `git.status`: kernel names and plugin names.
pub fn is_function_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !name.ends_with('.')
        && !name.contains("..")
}
/// The generator every schema comes from: JSON Schema 2020-12, which the SDK's types and the editors
/// that read them expect.
pub fn generator() -> SchemaGenerator {
    SchemaSettings::draft2020_12().into_generator()
}
/// `schema` with each `$ref` into `defs` replaced by what it names, so a parameter's schema says
/// all of itself; one inside the definition it names (a recursive type) stays. Returns whether
/// one stayed.
fn inline_refs(schema: &mut Value, defs: &Map<String, Value>, inside: &mut Vec<String>) -> bool {
    match schema {
        Value::Object(map) => {
            let name =
                map.get("$ref").and_then(Value::as_str).and_then(|r| r.strip_prefix("#/$defs/")).map(String::from);
            let Some(name) = name else {
                return map.values_mut().fold(false, |stayed, value| inline_refs(value, defs, inside) | stayed);
            };
            let Some(def) = defs.get(&name).filter(|_| !inside.contains(&name)) else { return true };
            let mut def = def.clone();
            inside.push(name);
            let stayed = inline_refs(&mut def, defs, inside);
            inside.pop();
            map.remove("$ref");
            match def {
                Value::Object(def) => {
                    for (key, value) in def {
                        map.entry(key).or_insert(value);
                    }
                }
                def => *schema = def,
            }
            stayed
        }
        Value::Array(items) => items.iter_mut().fold(false, |stayed, value| inline_refs(value, defs, inside) | stayed),
        _ => false,
    }
}
/// The type names (`String`, `Integer`, `Dict`, …) for a JSON Schema.
pub fn nvim_type(schema: &Value, defs: &Map<String, Value>) -> String {
    nvim_type_depth(schema, defs, 0)
}
fn nvim_type_depth(schema: &Value, defs: &Map<String, Value>, depth: u32) -> String {
    if depth > 8 {
        return "Object".into();
    }
    if schema == &Value::Bool(true) {
        return "Object".into();
    }
    if let Some(name) = schema.get("x-pumi-type").and_then(Value::as_str) {
        return name.to_string();
    }
    if let Some(target) = schema.get("$ref").and_then(Value::as_str) {
        let key = target.rsplit('/').next().unwrap_or(target);
        return match defs.get(key) {
            Some(def) => nvim_type_depth(def, defs, depth + 1),
            None => "Object".into(),
        };
    }
    for key in ["anyOf", "oneOf"] {
        if let Some(Value::Array(options)) = schema.get(key) {
            let kinds: Vec<String> = options
                .iter()
                .filter(|option| option.get("type").and_then(Value::as_str) != Some("null"))
                .map(|option| nvim_type_depth(option, defs, depth + 1))
                .collect();
            return match kinds.as_slice() {
                [one] => one.clone(),
                _ if !kinds.is_empty() && kinds.iter().all(|k| k == &kinds[0]) => kinds[0].clone(),
                _ => "Object".into(),
            };
        }
    }
    let single = match schema.get("type") {
        Some(Value::String(t)) => Some(t.as_str()),
        Some(Value::Array(types)) => {
            let non_null: Vec<&str> = types.iter().filter_map(Value::as_str).filter(|t| *t != "null").collect();
            if non_null.len() == 1 { Some(non_null[0]) } else { None }
        }
        _ => None,
    };
    match single {
        Some("string") => "String",
        Some("integer") => "Integer",
        Some("number") => "Float",
        Some("boolean") => "Boolean",
        Some("array") => "Array",
        Some("object") => "Dict",
        Some("null") => "nil",
        _ if schema.get("enum").is_some() || schema.get("const").is_some() => "String",
        _ => "Object",
    }
    .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_parameters_schema_names_no_definitions_but_a_recursive_one() {
        let defs = json!({
            "Id": { "type": "string" },
            "Tree": { "type": "object", "properties": { "id": { "$ref": "#/$defs/Id" }, "child": { "$ref": "#/$defs/Tree" } } }
        });
        let defs = defs.as_object().unwrap();
        let mut id = json!({ "anyOf": [{ "$ref": "#/$defs/Id" }, { "type": "null" }] });
        assert!(!inline_refs(&mut id, defs, &mut Vec::new()));
        assert_eq!(id, json!({ "anyOf": [{ "type": "string" }, { "type": "null" }] }));
        let mut tree = json!({ "$ref": "#/$defs/Tree" });
        assert!(inline_refs(&mut tree, defs, &mut Vec::new()), "a tree's child stays a reference");
        assert_eq!(tree["properties"]["id"], json!({ "type": "string" }));
        assert_eq!(tree["properties"]["child"], json!({ "$ref": "#/$defs/Tree" }));
    }
}
