//! What the hook reads of `settings.json`: whether permissions are asked in pumi, and the user's
//! resume recipes. What agents are told is the plugins' (`agent.onSend`), not the hook's.
use super::options;
use serde_json::{Value, json};
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// `agents.restore`: resume recipes by agent id, over the table's.
    pub restore: Value,
    /// `agents.permissions`: an agent's permission prompts are asked through `request.open`.
    pub permissions: bool,
}
impl Settings {
    pub fn from_value(value: &Value) -> Self {
        let values = &value["options"];
        let plugin = values[options::ENABLED].as_bool().unwrap_or(true);
        Settings {
            restore: values[options::RESTORE].clone(),
            permissions: plugin && values[options::PERMISSIONS].as_bool().unwrap_or(false),
        }
    }
}
pub fn path() -> PathBuf {
    if let Some(path) = std::env::var_os("PUMI_SETTINGS").filter(|p| !p.is_empty()) {
        return PathBuf::from(path);
    }
    pumi_proto::paths::config_dir().join("settings.json")
}
pub fn load() -> Settings {
    let value = std::fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(|| json!({}));
    Settings::from_value(&value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permissions_are_asked_in_pumi_only_when_turned_on() {
        assert!(!Settings::from_value(&json!({})).permissions, "off by default");
        assert!(Settings::from_value(&json!({ "options": { "agents.permissions": true } })).permissions);
        let off = json!({ "options": { "agents.permissions": true, "agents.enabled": false } });
        assert!(!Settings::from_value(&off).permissions, "the master switch wins");
        let restore = json!({ "options": { "agents.restore": { "codex": [] } } });
        assert_eq!(Settings::from_value(&restore).restore, json!({ "codex": [] }));
    }
}
