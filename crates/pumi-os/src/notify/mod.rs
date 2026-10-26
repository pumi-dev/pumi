use pumi_api::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod backend;
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod backend;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod backend;

/// One notification.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Notification {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    /// A line between the title and the body (on Linux and Windows, the body's first line).
    #[serde(default)]
    pub subtitle: Option<String>,
    /// `default` for the system's sound, or a sound's name (`Glass`); silent without.
    #[serde(default)]
    pub sound: Option<String>,
    /// One sent with the group of one still shown replaces it, and the group is its id.
    #[serde(default)]
    pub group: Option<String>,
    /// Buttons; the one pressed comes back in `notification_action`.
    #[serde(default)]
    pub actions: Vec<Action>,
    /// A reply field, with this placeholder; what is typed comes back in `notification_reply`
    /// (macOS and Windows).
    #[serde(default)]
    pub reply: Option<String>,
    /// A picture: a file's path.
    #[serde(default)]
    pub image: Option<String>,
    /// How much it interrupts: `low`, `normal` or `critical`.
    #[serde(default)]
    pub urgency: Urgency,
    /// The tab it is about (`3` or `"%3"`): clicking it shows that tab.
    #[serde(default)]
    pub tab: Option<Value>,
    /// Anything, handed back with every event about it.
    #[serde(default)]
    pub data: Option<Value>,
}
impl Notification {
    pub fn new(title: impl Into<String>) -> Self {
        Notification { title: title.into(), ..Notification::default() }
    }
    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into()).filter(|b: &String| !b.is_empty());
        self
    }
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into()).filter(|s: &String| !s.is_empty());
        self
    }
    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }
    pub fn sound(mut self, sound: impl Into<String>) -> Self {
        self.sound = Some(sound.into());
        self
    }
    pub fn urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }
    pub fn tab(mut self, tab: impl Into<Value>) -> Self {
        self.tab = Some(tab.into());
        self
    }
}
/// A button on a notification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Action {
    /// What `notification_action` says was pressed.
    pub id: String,
    pub label: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}
impl Urgency {
    /// A message's level (`message.notify`'s) as an urgency.
    pub fn of_level(level: &str) -> Urgency {
        match level {
            "error" => Urgency::Critical,
            "debug" | "trace" => Urgency::Low,
            _ => Urgency::Normal,
        }
    }
    /// Its name, as `notify-send` takes it (Linux's only).
    #[allow(dead_code)]
    fn name(self) -> &'static str {
        match self {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        }
    }
}
/// What the user did with a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Clicked,
    Action,
    Reply,
    Dismissed,
}
/// What the user did with a notification: an answer whose kind, and
/// `data.kind`, is [`Event::pattern`].
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Event {
    #[serde(skip)]
    pub kind: Kind,
    pub id: String,
    /// The button's id, for `notification_action`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// What was typed, for `notification_reply`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
impl Event {
    /// The answer's kind: `notification_clicked`, …
    pub fn pattern(&self) -> &'static str {
        match self.kind {
            Kind::Clicked => "notification_clicked",
            Kind::Action => "notification_action",
            Kind::Reply => "notification_reply",
            Kind::Dismissed => "notification_dismissed",
        }
    }
    /// The event's `data`.
    pub fn data(&self) -> Value {
        serde_json::to_value(self).unwrap_or_default()
    }
    fn new(kind: Kind, id: &str, carried: &str) -> Event {
        let carried: Value = serde_json::from_str(carried).unwrap_or_default();
        let field = |key: &str| carried.get(key).filter(|v| !v.is_null()).cloned();
        Event { kind, id: id.to_string(), action: None, reply: None, tab: field("tab"), data: field("data") }
    }
}
type Handler = Box<dyn Fn(Event) + Send + Sync>;
static HANDLER: OnceLock<Handler> = OnceLock::new();
/// Where what the user does with a notification goes, from now on (once: later calls are
/// ignored). The app calls it as it starts, before it finishes launching, so the click that
/// launched it is heard too.
pub fn start(handler: impl Fn(Event) + Send + Sync + 'static) {
    if HANDLER.set(Box::new(handler)).is_ok() {
        backend::start();
    }
}
/// Tells the handler; from whatever thread the OS said it on.
fn deliver(event: Event) {
    if let Some(handler) = HANDLER.get() {
        handler(event);
    }
}
/// What a notification carries back in its events, as JSON text.
fn carried(n: &Notification) -> String {
    json!({ "tab": n.tab, "data": n.data }).to_string()
}
/// A new notification's id: its group, or one of this process's.
fn id_of(n: &Notification) -> String {
    n.group.clone().unwrap_or_else(new_id)
}
/// An id no other notification of this process has.
fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("pumi-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}
/// The text a platform without a subtitle shows as the body (macOS has one: unused there).
#[allow(dead_code)]
fn body_with_subtitle(n: &Notification) -> String {
    match (&n.subtitle, &n.body) {
        (Some(s), Some(b)) => format!("{s}\n{b}"),
        (Some(s), None) => s.clone(),
        (None, Some(b)) => b.clone(),
        (None, None) => String::new(),
    }
}
/// Posts `n`; answers its id, for [`withdraw`] and the events about it.
pub fn send(n: &Notification) -> Result<String> {
    let id = id_of(n);
    send_as(&id, n)?;
    Ok(id)
}
/// Posts `n` as `id` (a message's, `message.notify`'s answer), replacing one of that id.
pub fn send_as(id: &str, n: &Notification) -> Result<()> {
    if n.title.is_empty() && n.body.as_deref().unwrap_or_default().is_empty() {
        return Err(pumi_api::Error::invalid_params("a notification needs a title or a body"));
    }
    backend::send(id, n)
}
/// Takes a notification back from the screen and the notification centre; `false` when this
/// platform cannot, or it was not one of ours.
pub fn withdraw(id: &str) -> bool {
    backend::withdraw(id)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_group_is_the_id() {
        assert_eq!(id_of(&Notification::new("a").group("build")), "build");
        let (a, b) = (id_of(&Notification::new("a")), id_of(&Notification::new("a")));
        assert_ne!(a, b);
    }
    #[test]
    fn an_event_carries_what_it_was_sent_with() {
        let n = Notification { tab: Some(json!("%3")), data: Some(json!({"n": 1})), ..Notification::new("t") };
        let ev = Event::new(Kind::Clicked, "x", &carried(&n));
        assert_eq!(ev.pattern(), "notification_clicked");
        assert_eq!(ev.data(), json!({ "id": "x", "tab": "%3", "data": {"n": 1} }));
        assert_eq!(Event::new(Kind::Dismissed, "y", "{}").data(), json!({ "id": "y" }));
    }
    #[test]
    fn a_level_is_an_urgency() {
        assert_eq!(Urgency::of_level("error"), Urgency::Critical);
        assert_eq!(Urgency::of_level("warn").name(), "normal");
    }
    #[test]
    fn a_subtitle_without_its_own_line_leads_the_body() {
        let n = Notification::new("t").subtitle("s").body("b");
        assert_eq!(body_with_subtitle(&n), "s\nb");
        assert_eq!(body_with_subtitle(&Notification::new("t")), "");
    }
    #[test]
    fn unknown_fields_are_refused() {
        assert!(serde_json::from_value::<Notification>(json!({ "title": "t", "colour": 1 })).is_err());
        let n: Notification = serde_json::from_value(json!({ "title": "t", "urgency": "critical" })).unwrap();
        assert_eq!(n.urgency, Urgency::Critical);
    }
}
