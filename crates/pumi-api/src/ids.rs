//! The ids of the API's objects as every surface spells them: a sigil and a number (`%3`), after
//! the server's name when the object is on another server (`devbox/%3`). The core keeps numbers;
//! this is their spelling at the API's edge. It never resolves an id: `None` is the caller's own,
//! and whether an object exists is the model's to say.
use crate::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// The longest server name an id may carry, in bytes.
pub const MAX_SERVER_NAME: usize = 64;

/// What an id's sigil says it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Session,
    View,
    Pane,
    Tab,
    Request,
}
impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Session, Kind::View, Kind::Pane, Kind::Tab, Kind::Request];
    pub fn sigil(self) -> char {
        match self {
            Kind::Session => '$',
            Kind::View => '@',
            Kind::Pane => ':',
            Kind::Tab => '%',
            Kind::Request => '+',
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Kind::Session => "session",
            Kind::View => "view",
            Kind::Pane => "pane",
            Kind::Tab => "tab",
            Kind::Request => "request",
        }
    }
    pub fn of_sigil(c: char) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.sigil() == c)
    }
}

/// Any object's id: its kind, its number, and its server when it is not this one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ref {
    pub server: Option<String>,
    pub kind: Kind,
    pub id: u64,
}
impl Ref {
    pub fn local(kind: Kind, id: u64) -> Ref {
        Ref { server: None, kind, id }
    }
    pub fn parse(text: &str) -> Result<Ref> {
        let (server, rest) = match text.rsplit_once('/') {
            Some((server, rest)) => (Some(server), rest),
            None => (None, text),
        };
        if let Some(server) = server
            && (server.is_empty()
                || server.len() > MAX_SERVER_NAME
                || !server.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)))
        {
            return Err(Error::invalid_params(format!("`{text}`: `{server}` is not a server's name")));
        }
        let mut chars = rest.chars();
        let kind = chars.next().and_then(Kind::of_sigil);
        let id = chars.as_str().parse::<u64>().ok().filter(|n| *n > 0 && !chars.as_str().starts_with('0'));
        match (kind, id) {
            (Some(kind), Some(id)) => Ok(Ref { server: server.map(str::to_string), kind, id }),
            _ => Err(Error::invalid_params(format!(
                "`{text}` is not an id: `%3` (tab), `:4` (pane), `@2` (view), `$1` (session), `+7` (request)"
            ))),
        }
    }
    /// The number, when it is an id of `kind` on this server.
    pub fn of(&self, kind: Kind) -> Result<u64> {
        if let Some(server) = &self.server {
            return Err(Error::invalid_params(format!("{self} is on server `{server}`, not this one")));
        }
        if self.kind != kind {
            return Err(Error::invalid_params(format!("{self} is a {}, not a {}", self.kind.name(), kind.name())));
        }
        Ok(self.id)
    }
}
impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(server) = &self.server {
            write!(f, "{server}/")?;
        }
        write!(f, "{}{}", self.kind.sigil(), self.id)
    }
}
impl Serialize for Ref {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
impl<'de> Deserialize<'de> for Ref {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Ref, D::Error> {
        let text = String::deserialize(d)?;
        Ref::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// An id's pattern: a server's name and `/` maybe, then `sigil` (a regex: one sigil, or a class of
/// them) and the number.
fn pattern(sigil: &str) -> String {
    format!("^(?:[A-Za-z0-9._-]+/)?{sigil}[1-9][0-9]*$")
}
impl schemars::JsonSchema for Ref {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Ref".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": pattern(r"[$@:%+]") })
    }
}

/// An id of one kind: a `TabRef` is a `Ref` whose sigil is `%`, and refuses any other.
macro_rules! typed {
    ($name:ident, $kind:expr, $sigil:literal) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u64);
        impl $name {
            pub const KIND: Kind = $kind;
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $sigil, self.0)
            }
        }
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<$name, D::Error> {
                let text = String::deserialize(d)?;
                Ref::parse(&text).and_then(|r| r.of($kind)).map($name).map_err(serde::de::Error::custom)
            }
        }
        impl schemars::JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
                schemars::json_schema!({ "type": "string", "pattern": pattern(&regex_escape($sigil)) })
            }
        }
    };
}
fn regex_escape(sigil: &str) -> String {
    match sigil {
        "$" | "+" => format!(r"\{sigil}"),
        _ => sigil.into(),
    }
}
typed!(SessionRef, Kind::Session, "$");
typed!(ViewRef, Kind::View, "@");
typed!(PaneRef, Kind::Pane, ":");
typed!(TabRef, Kind::Tab, "%");
typed!(RequestRef, Kind::Request, "+");

/// An integer handle a call answers and a later call names: a subscription, a queued message, a
/// turn. It is a number from 1, never an object's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Handle(pub u64);
impl<'de> Deserialize<'de> for Handle {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Handle, D::Error> {
        match u64::deserialize(d)? {
            0 => Err(serde::de::Error::custom("a handle is a number from 1")),
            n => Ok(Handle(n)),
        }
    }
}
impl schemars::JsonSchema for Handle {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Handle".into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "integer", "minimum": 1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_id_is_its_sigil_and_number_after_its_server() {
        assert_eq!(Ref::parse("%3").unwrap(), Ref::local(Kind::Tab, 3));
        assert_eq!(Ref::parse("devbox/@2").unwrap().to_string(), "devbox/@2");
        assert_eq!(Ref::parse("+7").unwrap().kind, Kind::Request);
        assert_eq!(Ref::parse(":4").unwrap(), Ref::local(Kind::Pane, 4));
        for bad in ["3", "%0", "%03", "%", "/%3", "dev box/%3", "#3"] {
            assert!(Ref::parse(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn a_typed_id_refuses_another_kind_and_another_server() {
        assert_eq!(serde_json::from_str::<TabRef>("\"%3\"").unwrap(), TabRef(3));
        assert!(serde_json::from_str::<TabRef>("\"@3\"").is_err());
        assert!(serde_json::from_str::<TabRef>("\"devbox/%3\"").is_err());
        assert_eq!(serde_json::to_string(&SessionRef(1)).unwrap(), "\"$1\"");
    }
}
