use pumi_api::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
/// The objects an event is about. On the wire each is its prefixed id (`"$1"`, `"@2"`, `"%3"`),
/// as everywhere else.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct About {
    #[serde(default, skip_serializing_if = "Option::is_none", with = "prefixed::session")]
    #[schemars(with = "Option<pumi_api::SessionRef>")]
    pub session: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "prefixed::view")]
    #[schemars(with = "Option<pumi_api::ViewRef>")]
    pub view: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none", with = "prefixed::tab")]
    #[schemars(with = "Option<pumi_api::TabRef>")]
    pub tab: Option<u64>,
}
/// An [`About`]'s ids as their typed refs on the wire.
mod prefixed {
    macro_rules! field {
        ($name:ident, $ref:path) => {
            pub mod $name {
                use serde::{Deserialize, Deserializer, Serialize, Serializer};
                pub fn serialize<S: Serializer>(id: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
                    id.map($ref).serialize(s)
                }
                pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
                    Ok(Option::<$ref>::deserialize(d)?.map(|r| r.0))
                }
            }
        };
    }
    field!(session, pumi_api::SessionRef);
    field!(view, pumi_api::ViewRef);
    field!(tab, pumi_api::TabRef);
}
impl About {
    pub fn session(id: u64) -> About {
        About { session: Some(id), ..About::default() }
    }
    pub fn view(id: u64) -> About {
        About { view: Some(id), ..About::default() }
    }
    pub fn tab(id: u64) -> About {
        About { tab: Some(id), ..About::default() }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Event {
    pub seq: u64,
    pub event: String,
    #[serde(rename = "match")]
    pub matched: String,
    #[serde(flatten)]
    pub about: About,
    pub data: Value,
}
pub fn fnmatch(pattern: &str, text: &str) -> bool {
    fn go(p: &[char], t: &[char]) -> bool {
        match p.split_first() {
            None => t.is_empty(),
            Some(('*', rest)) => (0..=t.len()).any(|i| go(rest, &t[i..])),
            Some(('?', rest)) => !t.is_empty() && go(rest, &t[1..]),
            Some((c, rest)) => t.first() == Some(c) && go(rest, &t[1..]),
        }
    }
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    go(&p, &t)
}
/// An event's name: `tab.opened`, `pumi_dev.queue.done`. Two or more dotted segments, each a
/// lowercase letter then letters, digits and `_` (camelCase: `tab.blockChanged`; a plugin's
/// prefix, `pumi_dev.agent_chat.`). A listener's may be a glob over those (`pumi_dev.queue.*`,
/// `*`); one that fires may not.
pub fn check_event(name: &str, glob: bool) -> Result<()> {
    let segment = |s: &str| {
        let mut chars = s.chars();
        let first = chars.next();
        if glob && s.contains(['*', '?']) {
            return s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '*' | '?'));
        }
        first.is_some_and(|c| c.is_ascii_lowercase()) && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let ok = (glob && name == "*") || name.split('.').count() >= 2 && name.split('.').all(segment);
    if ok { Ok(()) } else { Err(Error::invalid_params(format!("no event `{name}`"))) }
}
/// An event fired before what it names happens (a verb in `-ing`): a `*` listener does not
/// hear it, only one that names it.
pub fn is_before(event: &str) -> bool {
    event.rsplit('.').next().is_some_and(|verb| verb.ends_with("ing"))
}
/// Whether a listener's event (a name or a glob, `*` for every one but the [`is_before`]
/// ones) hears `event`.
pub fn listens_to(listened: &str, event: &str) -> bool {
    match listened {
        "*" => !is_before(event),
        _ if listened.contains(['*', '?']) => fnmatch(listened, event),
        _ => listened == event,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn event_names_are_dotted_and_listeners_may_glob() {
        assert!(check_event("tab.opened", false).is_ok());
        assert!(check_event("pumi_dev.queue.done", false).is_ok());
        assert!(check_event("tab.blockChanged", false).is_ok());
        assert!(check_event("pumi_dev.agent_chat.conversation.changed", false).is_ok(), "a plugin's prefix");
        assert!(check_event("pumi_dev._x.done", false).is_err());
        assert!(check_event("TabNew", false).is_err());
        assert!(check_event("User", false).is_err());
        assert!(check_event("tab", false).is_err());
        assert!(check_event("pumi_dev.queue.*", false).is_err());
        assert!(check_event("pumi_dev.queue.*", true).is_ok());
        assert!(check_event("*", true).is_ok());
        assert!(check_event("*", false).is_err());
        assert!(listens_to("pumi_dev.queue.*", "pumi_dev.queue.done"));
        assert!(!listens_to("pumi_dev.queue.*", "pumi_dev.other.done"));
        assert!(listens_to("*", "tab.opened"));
        assert!(!listens_to("*", "plugin.changing"));
        assert!(listens_to("plugin.changing", "plugin.changing"));
    }
    #[test]
    fn the_listed_events_are_named_by_the_rule_once_each() {
        let events: Vec<_> = pumi_api::events::all().into_iter().chain(pumi_api::events::internal()).collect();
        let mut names: Vec<&str> = events.iter().map(|e| e.name.as_str()).collect();
        for name in &names {
            assert!(check_event(name, false).is_ok(), "{name}");
        }
        names.sort();
        names.dedup();
        assert_eq!(names.len(), events.len(), "an event is listed once");
    }
}
