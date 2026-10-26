use crate::args::json_kind;
use crate::{Caller, Error, ErrorKind, Registry};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::any::Any;
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
impl RpcError {
    pub fn to_error(&self) -> Error {
        let kind = self
            .data
            .as_ref()
            .and_then(|data| data.get("kind"))
            .and_then(Value::as_str)
            .and_then(ErrorKind::from_name)
            .unwrap_or_else(|| ErrorKind::from_rpc_code(self.code));
        let data = self.data.as_ref().and_then(|data| {
            let mut data = data.as_object()?.clone();
            data.remove("kind");
            (!data.is_empty()).then_some(Value::Object(data))
        });
        Error { kind, message: self.message.clone(), data }
    }
}
impl Response {
    pub fn into_result(self) -> Result<Value, Error> {
        match (self.result, self.error) {
            (_, Some(error)) => Err(error.to_error()),
            (Some(result), None) => Ok(result),
            (None, None) => Ok(Value::Null),
        }
    }
}
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
pub fn handle(registry: &Registry, host: &mut dyn Any, caller: &Caller, text: &str) -> Option<String> {
    let reply = match serde_json::from_str::<Value>(text) {
        Err(err) => Some(error_response(Value::Null, PARSE_ERROR, &format!("parse error: {err}"), None)),
        Ok(Value::Array(batch)) if batch.is_empty() => {
            Some(error_response(Value::Null, INVALID_REQUEST, "an empty batch", None))
        }
        Ok(Value::Array(batch)) => {
            let replies: Vec<Value> =
                batch.into_iter().filter_map(|message| handle_value(registry, host, caller, message)).collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        Ok(message) => handle_value(registry, host, caller, message),
    };
    reply.map(|value| value.to_string())
}
pub fn handle_value(registry: &Registry, host: &mut dyn Any, caller: &Caller, message: Value) -> Option<Value> {
    let Value::Object(mut message) = message else {
        return Some(error_response(
            Value::Null,
            INVALID_REQUEST,
            &format!("a request is an object, not {}", json_kind(&message)),
            None,
        ));
    };
    let id = message.remove("id");
    let reply_id = id.clone().unwrap_or(Value::Null);
    if !matches!(id, None | Some(Value::Number(_)) | Some(Value::String(_)) | Some(Value::Null)) {
        return Some(error_response(Value::Null, INVALID_REQUEST, "an id is a number or a string", None));
    }
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error_response(reply_id, INVALID_REQUEST, "`jsonrpc` must be \"2.0\"", None));
    }
    let Some(Value::String(method)) = message.remove("method") else {
        return Some(error_response(reply_id, INVALID_REQUEST, "`method` must be a string", None));
    };
    let params = message.remove("params").unwrap_or(Value::Null);
    if !matches!(params, Value::Array(_) | Value::Object(_) | Value::Null) {
        return Some(error_response(reply_id, INVALID_REQUEST, "`params` is an array or an object", None));
    }
    let result = registry.call(host, caller, &method, params);
    let id = id?;
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(err) => {
            let code = if err.kind == ErrorKind::NotFound
                && Registry::core(&method).is_none()
                && registry.function_info(&method).is_none()
            {
                METHOD_NOT_FOUND
            } else {
                err.kind.rpc_code()
            };
            error_response(id, code, &err.message, Some(error_data(&err)))
        }
    })
}
fn error_data(err: &Error) -> Value {
    let mut data = Map::new();
    data.insert("kind".into(), Value::String(err.kind.name().into()));
    if let Some(Value::Object(extra)) = &err.data {
        for (key, value) in extra {
            data.insert(key.clone(), value.clone());
        }
    } else if let Some(extra) = &err.data {
        data.insert("detail".into(), extra.clone());
    }
    Value::Object(data)
}
fn error_response(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let Some(data) = data {
        error["data"] = data;
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}
pub fn reply(id: &Value, result: crate::Result<Value>) -> String {
    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        Err(err) => error_response(id.clone(), err.kind.rpc_code(), &err.message, Some(error_data(&err))).to_string(),
    }
}
pub fn request(id: Id, method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string()
}
pub fn notification(method: &str, params: Value) -> String {
    json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rpc(text: &str) -> Option<Value> {
        let registry = Registry::new();
        handle(&registry, &mut (), &Caller::default(), text).map(|reply| serde_json::from_str(&reply).unwrap())
    }
    #[test]
    fn calls_a_function() {
        let reply = rpc(r#"{"jsonrpc":"2.0","id":1,"method":"call","params":[[]]}"#).unwrap();
        assert_eq!(reply, json!({"jsonrpc": "2.0", "id": 1, "result": []}));
    }
    #[test]
    fn named_params() {
        let reply = rpc(r#"{"jsonrpc":"2.0","id":"a","method":"call","params":{"name":[]}}"#).unwrap();
        assert_eq!(reply["result"], json!([]));
    }
    #[test]
    fn unknown_method_is_method_not_found() {
        let reply = rpc(r#"{"jsonrpc":"2.0","id":2,"method":"nope"}"#).unwrap();
        assert_eq!(reply["error"]["code"], -32601);
        assert_eq!(reply["error"]["data"]["kind"], "not_found");
    }
    #[test]
    fn unknown_argument_is_invalid_params() {
        let reply = rpc(r#"{"jsonrpc":"2.0","id":3,"method":"call","params":{"name":[],"x":1}}"#).unwrap();
        assert_eq!(reply["error"]["code"], -32602);
        assert!(reply["error"]["message"].as_str().unwrap().contains("`x`"));
    }
    #[test]
    fn too_many_arguments() {
        let reply = rpc(r#"{"jsonrpc":"2.0","id":3,"method":"api.getInfo","params":[1]}"#).unwrap();
        assert_eq!(reply["error"]["data"]["kind"], "invalid_params");
    }
    #[test]
    fn notifications_are_not_answered() {
        assert_eq!(rpc(r#"{"jsonrpc":"2.0","method":"call","params":[[]]}"#), None);
    }
    #[test]
    fn batches() {
        let reply = rpc(r#"[{"jsonrpc":"2.0","id":1,"method":"call","params":[[]]},
                            {"jsonrpc":"2.0","method":"call","params":[[]]},
                            {"jsonrpc":"2.0","id":2,"method":"nope"}]"#)
        .unwrap();
        let replies = reply.as_array().unwrap();
        assert_eq!(replies.len(), 2);
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[1]["error"]["code"], -32601);
    }
    #[test]
    fn malformed() {
        assert_eq!(rpc("{").unwrap()["error"]["code"], PARSE_ERROR);
        assert_eq!(rpc("[]").unwrap()["error"]["code"], INVALID_REQUEST);
        assert_eq!(rpc("3").unwrap()["error"]["code"], INVALID_REQUEST);
        assert_eq!(rpc(r#"{"id":1,"method":"x"}"#).unwrap()["error"]["code"], INVALID_REQUEST);
        assert_eq!(
            rpc(r#"{"jsonrpc":"2.0","id":1,"method":"x","params":3}"#).unwrap()["error"]["code"],
            INVALID_REQUEST
        );
    }
    #[test]
    fn errors_round_trip_to_kinds() {
        let reply = rpc(r#"{"jsonrpc":"2.0","id":1,"method":"nope"}"#).unwrap();
        let response: Response = serde_json::from_value(reply).unwrap();
        let err = response.into_result().unwrap_err();
        assert_eq!(err.kind, ErrorKind::NotFound);
        assert_eq!(err.exit_code(), 3);
    }
}
