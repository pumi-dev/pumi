//! The protocol as data: `api/ui-protocol.json`, which `cargo xtask api` writes and the app's
//! TS types are generated from.
use schemars::generate::SchemaSettings;
use serde_json::{Value, json};

use crate::vocab::{BUILT_IN_TYPES, ELEMENT_EVENTS, UNIVERSAL_PROPS};
use crate::{Event, LEVEL, Op};

/// The whole protocol: its level, its names, and a JSON Schema (draft 2020-12) of each type
/// under `$defs`: `Op` (what a UI sends), `Event` (what it gets back), `StyleDesc` and the rest.
pub fn schema() -> Value {
    let mut generator = SchemaSettings::draft2020_12().into_generator();
    let op = generator.subschema_for::<Op>();
    let event = generator.subschema_for::<Event>();
    let defs = generator.take_definitions(true);
    json!({
        "level": LEVEL,
        "builtInTypes": BUILT_IN_TYPES,
        "elementEvents": ELEMENT_EVENTS,
        "universalProps": UNIVERSAL_PROPS,
        "op": op,
        "event": event,
        "$defs": defs,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_every_type_it_refers_to() {
        let schema = super::schema();
        let defs = schema["$defs"].as_object().unwrap();
        for name in ["Op", "Event", "StyleDesc", "ElementId", "Press", "Key"] {
            assert!(defs.contains_key(name), "no {name} in $defs");
        }
        let text = schema.to_string();
        for (at, _) in text.match_indices("#/$defs/") {
            let name: String = text[at + 8..].chars().take_while(|c| *c != '"').collect();
            assert!(defs.contains_key(&name), "{name} is referred to and not defined");
        }
    }
}
