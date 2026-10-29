//! The request table: what is put to the user — a notification, a permission an agent waits on,
//! a question — kept by the core so any client (the app, a plugin, a phone) can show it and
//! answer it, and the first answer wins. A request lives until it is closed (`request.close`),
//! it expires, its tab closes, or the channel that opened a permission or a question goes; each
//! end fires `request.closed`. The core never draws one: whoever listens to `request.opened`
//! does.
use super::events::{About, listens_to};
use super::state::{parse_rfc3339, rfc3339};
use super::{ChanId, Kernel, TabId};
use pumi_api::{RequestRef, TabRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
/// How long a permission or a question stays open when its opener gives no `expires`, in
/// seconds.
pub const TIMEOUT: u64 = 300;
/// The most notifications open at once: past it, the oldest is closed.
pub const NOTIFICATION_LIMIT: usize = 256;
/// What a request puts to the user.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RequestKind {
    /// Something to know; it takes no answer.
    #[default]
    Notification,
    /// May an agent do something (run a tool)?
    Permission,
    /// Something asked of the user.
    Question,
}
impl RequestKind {
    pub fn word(self) -> &'static str {
        match self {
            RequestKind::Notification => "notification",
            RequestKind::Permission => "permission",
            RequestKind::Question => "question",
        }
    }
    /// Whether someone waits on its answer: it expires, and it needs someone to show it.
    pub fn waits(self) -> bool {
        !matches!(self, RequestKind::Notification)
    }
}
/// How much a request matters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RequestLevel {
    Info,
    Warn,
    Error,
    /// Only kept: never posted to the OS.
    Debug,
}
impl RequestLevel {
    pub fn word(self) -> &'static str {
        match self {
            RequestLevel::Info => "info",
            RequestLevel::Warn => "warn",
            RequestLevel::Error => "error",
            RequestLevel::Debug => "debug",
        }
    }
}
/// How urgent a notification is to the OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}
/// A button on a notification.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}
/// What a notification does in the OS's notification centre.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationFields {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// `default`, or a sound's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    /// A notification opened with the group of one still open replaces it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<NotificationAction>,
    /// A reply field's placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    /// A file's path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub urgency: Option<NotificationUrgency>,
    /// Carried back with what the user does with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
/// What `request.open` is asked to put to the user.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RequestAsk {
    #[serde(default)]
    pub kind: RequestKind,
    /// One line: what is asked or said (`Run "rm -rf build"?`).
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
    /// The tool a permission is for (`Bash`).
    #[serde(default)]
    pub tool: Option<String>,
    /// The answers it offers: a permission's are `{id, label, kind}`; a question may offer none
    /// and take text; a notification has none.
    #[serde(default)]
    pub options: Vec<Value>,
    #[serde(default)]
    pub level: Option<RequestLevel>,
    #[serde(default)]
    pub notification: Option<NotificationFields>,
}
/// An open request, as `request.get` answers it and `request.opened` carries it (with `id`
/// spelled `request`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Request {
    pub id: RequestRef,
    pub tab: Option<TabRef>,
    pub kind: RequestKind,
    pub title: String,
    pub detail: Option<String>,
    pub tool: Option<String>,
    pub options: Vec<Value>,
    pub level: Option<RequestLevel>,
    pub notification: Option<NotificationFields>,
    /// Who opened it: a plugin's id, or its channel's name.
    pub source: Option<String>,
    /// RFC 3339.
    pub created: String,
    /// RFC 3339; `null`: it stays until it is closed.
    pub expires: Option<String>,
}
/// Why a request closed, in `request.closed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// Someone answered it.
    Answered,
    /// The channel that opened it went.
    Cancelled,
    /// It expired.
    Timeout,
    /// Someone closed it without an answer, its tab closed, or a newer one replaced it.
    Closed,
}
impl Reason {
    fn word(self) -> &'static str {
        match self {
            Reason::Answered => "answered",
            Reason::Cancelled => "cancelled",
            Reason::Timeout => "timeout",
            Reason::Closed => "closed",
        }
    }
}
/// The core's open requests.
#[derive(Default)]
pub struct Requests {
    pub open: BTreeMap<u64, Request>,
    next: u64,
    /// When each request that expires does, in Unix ms.
    deadlines: BTreeMap<u64, u64>,
    /// The channel that opened each permission and question.
    openers: BTreeMap<u64, ChanId>,
}
impl Requests {
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }
    /// Takes back the requests a saved state kept (those it cannot read are dropped); the ids go
    /// on after theirs.
    pub fn restore(&mut self, saved: Vec<Value>) {
        for request in saved.into_iter().filter_map(|r| serde_json::from_value::<Request>(r).ok()) {
            let id = request.id.0;
            self.next = self.next.max(id);
            if let Some(at) = request.expires.as_deref().and_then(parse_rfc3339) {
                self.deadlines.insert(id, (at.max(0) as u64) * 1000);
            }
            self.open.insert(id, request);
        }
    }
    /// The requests as a saved state keeps them.
    pub fn saved(&self) -> Vec<Value> {
        self.open.values().map(|r| serde_json::to_value(r).expect("plain data")).collect()
    }
}
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
/// Whether any channel listens to `request.opened` by name (or a glob such as `request.*`): the
/// host's `*` is not someone who can show it.
pub fn heard(kernel: &Kernel) -> bool {
    kernel.events.autocmds.iter().any(|a| a.events.iter().any(|e| e != "*" && listens_to(e, "request.opened")))
}
/// What `request.opened` carries: the request's fields, its id spelled `request`.
fn opened_data(request: &Request) -> Value {
    let mut data = serde_json::to_value(request).expect("plain data");
    if let Some(map) = data.as_object_mut()
        && let Some(id) = map.remove("id")
    {
        map.insert("request".into(), id);
    }
    data
}
/// Opens `ask` for the user; returns its id. `expires` is in Unix ms. A permission or a question
/// is cancelled when `opener` closes. Checks are the caller's.
pub fn open(
    kernel: &mut Kernel,
    ask: RequestAsk,
    tab: Option<TabId>,
    expires: Option<u64>,
    source: Option<String>,
    opener: Option<ChanId>,
) -> u64 {
    if let Some(group) = ask.notification.as_ref().and_then(|n| n.group.clone()) {
        let replaced: Vec<u64> = kernel
            .requests
            .open
            .values()
            .filter(|r| r.notification.as_ref().and_then(|n| n.group.as_deref()) == Some(group.as_str()))
            .map(|r| r.id.0)
            .collect();
        for id in replaced {
            close(kernel, id, Value::Null, Reason::Closed);
        }
    }
    if ask.kind == RequestKind::Notification {
        let notifications: Vec<u64> =
            kernel.requests.open.values().filter(|r| r.kind == RequestKind::Notification).map(|r| r.id.0).collect();
        let over = (notifications.len() + 1).saturating_sub(NOTIFICATION_LIMIT);
        for id in notifications.into_iter().take(over) {
            close(kernel, id, Value::Null, Reason::Closed);
        }
    }
    kernel.requests.next += 1;
    let id = kernel.requests.next;
    let request = Request {
        id: RequestRef(id),
        tab: tab.map(TabRef),
        kind: ask.kind,
        title: ask.title,
        detail: ask.detail,
        tool: ask.tool,
        options: ask.options,
        level: ask.level,
        notification: ask.notification,
        source,
        created: rfc3339((now_ms() / 1000) as i64),
        expires: expires.map(|ms| rfc3339((ms / 1000) as i64)),
    };
    if let Some(at) = expires {
        kernel.requests.deadlines.insert(id, at);
    }
    if let Some(chan) = opener.filter(|_| request.kind.waits()) {
        kernel.requests.openers.insert(id, chan);
    }
    let data = opened_data(&request);
    let about = About { tab, ..About::default() };
    kernel.requests.open.insert(id, request.clone());
    kernel.emit("request.opened", request.kind.word(), about, data);
    id
}
/// Closes request `id` with `answer`; whether it was open (the first answer wins).
pub fn close(kernel: &mut Kernel, id: u64, answer: Value, reason: Reason) -> bool {
    let Some(request) = kernel.requests.open.remove(&id) else { return false };
    kernel.requests.deadlines.remove(&id);
    kernel.requests.openers.remove(&id);
    let data = json!({ "request": request.id, "tab": request.tab, "answer": answer, "reason": reason.word() });
    let about = About { tab: request.tab.map(|p| p.0), ..About::default() };
    kernel.emit("request.closed", &request.id.to_string(), about, data);
    true
}
/// Closes the requests that expired by `now` (Unix ms), and those whose tab closed.
pub fn tick(kernel: &mut Kernel, now: u64) {
    let ended: Vec<(u64, Reason)> = kernel
        .requests
        .open
        .values()
        .filter_map(|r| {
            let id = r.id.0;
            if r.tab.is_some_and(|p| !kernel.model.tabs.contains_key(&p.0)) {
                Some((id, Reason::Closed))
            } else if kernel.requests.deadlines.get(&id).is_some_and(|at| *at <= now) {
                Some((id, Reason::Timeout))
            } else {
                None
            }
        })
        .collect();
    for (id, reason) in ended {
        close(kernel, id, Value::Null, reason);
    }
}
/// A channel closed: the permissions and questions it opened are cancelled.
pub fn chan_closed(kernel: &mut Kernel, chan: ChanId) {
    let gone: Vec<u64> = kernel.requests.openers.iter().filter(|(_, c)| **c == chan).map(|(id, _)| *id).collect();
    for id in gone {
        close(kernel, id, Value::Null, Reason::Cancelled);
    }
}
/// The id an option offers, when it is an object with one.
fn option_id(option: &Value) -> Option<&str> {
    option.get("id").and_then(Value::as_str)
}
/// Whether `ask`'s options suit its kind: a notification has none, a permission's each have an
/// `id` and a `label`, and no two share an id.
pub fn check_options(ask: &RequestAsk) -> Result<(), String> {
    match ask.kind {
        RequestKind::Notification if !ask.options.is_empty() => {
            return Err("a notification has no `options`: it takes no answer".into());
        }
        RequestKind::Permission if ask.options.is_empty() => {
            return Err("a permission offers `options` (`{id, label, kind}`) to allow or reject it".into());
        }
        _ => {}
    }
    let mut seen = std::collections::BTreeSet::new();
    for option in &ask.options {
        let labelled = option.get("label").and_then(Value::as_str).is_some_and(|l| !l.is_empty());
        let id = option_id(option).filter(|id| !id.is_empty());
        match id {
            Some(id) if labelled && seen.insert(id) => {}
            Some(id) if labelled => return Err(format!("two options have the id `{id}`")),
            _ if ask.kind == RequestKind::Permission => {
                return Err(format!("a permission's option has an `id` and a `label`: {option}"));
            }
            _ => {}
        }
    }
    Ok(())
}
/// Whether `answer` is one `request` takes: an object whose `option`, if it names one, is one of
/// the request's options. A notification takes no answer.
pub fn check_answer(request: &Request, answer: &Value) -> Result<(), String> {
    if request.kind == RequestKind::Notification {
        return Err(format!("{} is a notification: it takes no answer", request.id));
    }
    let Value::Object(map) = answer else {
        return Err(format!("an answer is an object (`{{\"option\": …}}`, `{{\"text\": …}}`), not {answer}"));
    };
    match map.get("option") {
        None => Ok(()),
        Some(Value::String(option)) if request.options.iter().any(|o| option_id(o) == Some(option.as_str())) => Ok(()),
        Some(other) => Err(format!("{} has no option {other}", request.id)),
    }
}
