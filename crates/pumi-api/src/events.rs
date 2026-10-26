use serde::{Deserialize, Serialize};
/// An event pumi fires itself, as `api.getInfo` and the export list it. A plugin's own events
/// (`pumi_dev.queue.done`) are its own and are not listed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct EventInfo {
    pub name: String,
    /// Who fires it: `core` (pumid), `host` (pumi-host), `app` or `cli`.
    pub provider: String,
    pub desc: String,
    /// What a listener's `pattern` is matched against; empty when it is always empty.
    #[serde(rename = "match")]
    pub matched: String,
    /// The fields of its `data`.
    pub data: Vec<String>,
    /// Its `data`'s schema; an internal event has none.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub params_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<crate::meta::ExampleInfo>,
}
/// The events pumi fires for itself: heard by the app, the host and the CLI, and left out of
/// `api.getInfo`, the export and the docs, as wire-only functions are.
const INTERNAL: &[(&str, &str, &str, &[&str], &str)] = &[
    (
        "terminal.osc",
        "core",
        "the OSC code, or `bell`",
        &["code", "data", "fg"],
        "A terminal's program sent an OSC sequence pumi passes on, or rang the bell (the host's notifications and reports).",
    ),
    (
        "core.started",
        "core",
        "",
        &["restarted"],
        "The core started, or started again after a restart (the host lays out again).",
    ),
    ("client.attached", "core", "", &["client", "session"], "A client attached."),
    ("client.detached", "core", "", &["client"], "A client detached: the primary app may change."),
    (
        "client.changed",
        "core",
        "the session's id",
        &["client", "keys", "patch"],
        "A client shows another session (`session.focus`): `patch.session` is it.",
    ),
    (
        "plugin.reloadRequested",
        "cli",
        "",
        &["name", "inspect"],
        "`pumi plugin dev` asked the app to reload a plugin (every one when `name` is empty); `inspect` runs it under Bun's inspector.",
    ),
];
fn info(list: &[(&str, &str, &str, &[&str], &str)]) -> Vec<EventInfo> {
    list.iter()
        .map(|(name, provider, matched, data, desc)| EventInfo {
            name: name.to_string(),
            provider: provider.to_string(),
            desc: desc.to_string(),
            matched: matched.to_string(),
            data: data.iter().map(|d| d.to_string()).collect(),
            params_schema: serde_json::Value::Null,
            examples: Vec::new(),
        })
        .collect()
}
/// Every event pumi fires for plugins: what `api.getInfo` and the export list (`surface.json`).
pub fn all() -> Vec<EventInfo> {
    crate::surface::surface().events.clone()
}
/// The events pumi fires for itself only.
pub fn internal() -> Vec<EventInfo> {
    info(INTERNAL)
}
