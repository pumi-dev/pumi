use crate::{Error, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
pub struct Args {
    function: &'static str,
    kind: Kind,
}
enum Kind {
    Positional(Vec<Value>),
    Named(Map<String, Value>),
}
impl Args {
    pub fn new(function: &'static str, args: Value, names: &[&str]) -> Result<Args> {
        let kind = match args {
            Value::Null => Kind::Positional(Vec::new()),
            Value::Array(list) => {
                if list.len() > names.len() {
                    return Err(Error::invalid_params(format!(
                        "{function} takes {} argument{}, got {}",
                        names.len(),
                        if names.len() == 1 { "" } else { "s" },
                        list.len()
                    )));
                }
                Kind::Positional(list)
            }
            Value::Object(map) => {
                if let Some(unknown) = map.keys().find(|key| !names.contains(&key.as_str())) {
                    let expected = if names.is_empty() { "none".to_string() } else { names.join(", ") };
                    return Err(Error::invalid_params(format!(
                        "{function} has no argument `{unknown}`; its arguments are: {expected}"
                    )));
                }
                Kind::Named(map)
            }
            other => {
                return Err(Error::invalid_params(format!(
                    "{function}: arguments are an array or an object, not {}",
                    json_kind(&other)
                )));
            }
        };
        Ok(Args { function, kind })
    }
    pub fn take<T: DeserializeOwned>(&mut self, index: usize, name: &str, optional: bool) -> Result<T> {
        let value = match &mut self.kind {
            Kind::Positional(list) => list.get_mut(index).map(Value::take),
            Kind::Named(map) => map.remove(name),
        };
        let value = match value {
            Some(value) => value,
            None if optional => Value::Null,
            None => return Err(Error::invalid_params(format!("{}: missing argument `{name}`", self.function))),
        };
        serde_json::from_value(value)
            .map_err(|err| Error::invalid_params(format!("{}: argument `{name}`: {err}", self.function)))
    }
}
pub fn to_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|err| Error::exception(format!("the result is not JSON: {err}")))
}
pub fn from_value<T: DeserializeOwned>(value: Value, what: &str) -> Result<T> {
    serde_json::from_value(value).map_err(|err| Error::invalid_params(format!("{what}: {err}")))
}
pub(crate) fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}
