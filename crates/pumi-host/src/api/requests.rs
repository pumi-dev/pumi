//! Requests as the host serves them: the core keeps every request (`request.*`), and the host
//! posts each notification-kind one to every app's OS notification centre as it opens, and
//! takes it back as it closes. It never answers one.
use crate::kernel::Kernel;
use crate::kernel::events::Event;
use serde_json::{Map, Value, json};
/// The fields of a request's `notification` the apps post.
const POSTED: &[&str] = &["subtitle", "sound", "group", "actions", "reply", "image", "urgency", "data"];
/// A message from the host itself, as a notification request: kept by `show-messages` and posted
/// by every app, headed by its level. `level` is `error`, `warn`, `info` or `debug` (only kept).
pub fn notify_all(k: &mut Kernel, level: &str, msg: String) {
    match k.core.clone() {
        Some(core) => {
            let request = json!({ "kind": "notification", "title": msg, "level": level });
            core.notify("request.open", json!([request, { "source": "pumi-host" }]));
        }
        None => k.message(level, msg),
    }
}
/// What the host does with an event about a request.
pub fn heard(k: &mut Kernel, event: &Event) {
    match event.event.as_str() {
        "request.opened" if event.data["kind"] == "notification" => post(k, &event.data),
        "request.closed" => {
            let Some(id) = event.data["request"].as_str() else { return };
            for client in ui_clients(k) {
                k.send(client, pumi_api::rpc::notification("message.withdraw", json!([id])));
            }
        }
        _ => {}
    }
}
fn ui_clients(k: &Kernel) -> Vec<u64> {
    k.model.chans.values().filter(|c| c.ui.is_some()).map(|c| c.id).collect()
}
/// Keeps a notification's text and tells every app, which posts it as the request's id: its
/// `detail` under its `title`, or its `title` under a heading of its level.
fn post(k: &mut Kernel, request: &Value) {
    let Some(id) = request["request"].as_str() else { return };
    let level = request["level"].as_str().unwrap_or("info");
    let title = request["title"].as_str().unwrap_or_default();
    let mut opts = Map::new();
    let text = match request["detail"].as_str().filter(|d| !d.is_empty()) {
        Some(detail) => {
            opts.insert("title".into(), json!(title));
            detail
        }
        None => title,
    };
    for key in POSTED {
        if let Some(value) = request["notification"].get(*key).filter(|v| !v.is_null()) {
            opts.insert((*key).into(), value.clone());
        }
    }
    if let Some(tab) = request["tab"].as_str() {
        opts.insert("tab".into(), json!(tab));
    }
    k.message(level, text.to_string());
    for client in ui_clients(k) {
        k.send(client, pumi_api::rpc::notification("message.notify", json!([text, level, opts, id])));
    }
}
