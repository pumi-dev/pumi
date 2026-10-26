//! Parameters that `Option<T>` alone cannot spell: one that must be given and may be `null`
//! (`fn.set(name, null)` withdraws), and one that may be left out but is never `null` when it is
//! there (an option's `id`). Neither says what `null` does; its function does.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// `T`, or `null`: required, with `anyOf: [T, null]` as its schema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Nullable<T>(pub Option<T>);
impl<T> Nullable<T> {
    pub fn into_option(self) -> Option<T> {
        self.0
    }
}
impl<T: Serialize> Serialize for Nullable<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Nullable<T>, D::Error> {
        Option::<T>::deserialize(d).map(Nullable)
    }
}
/// `T` or absent: `Option<T>`'s convenience, with `T`'s schema alone. Its JSON is the value, or
/// `null` when it is absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Optional<T>(pub Option<T>);
impl<T> Default for Optional<T> {
    fn default() -> Optional<T> {
        Optional(None)
    }
}
impl<T> Optional<T> {
    pub fn into_option(self) -> Option<T> {
        self.0
    }
}
impl<T: Serialize> Serialize for Optional<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Optional<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Optional<T>, D::Error> {
        Option::<T>::deserialize(d).map(Optional)
    }
}
impl<T: schemars::JsonSchema> schemars::JsonSchema for Optional<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Optional_{}", T::schema_name()).into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        generator.subschema_for::<T>()
    }
}
impl<T: schemars::JsonSchema> schemars::JsonSchema for Nullable<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Nullable_{}", T::schema_name()).into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let inner = generator.subschema_for::<T>();
        schemars::json_schema!({ "anyOf": [inner, { "type": "null" }] })
    }
}

#[cfg(test)]
mod tests {
    use super::{Nullable, Optional};
    use serde_json::json;
    #[test]
    fn null_is_none_and_a_value_is_some() {
        assert_eq!(serde_json::from_value::<Nullable<String>>(json!(null)).unwrap(), Nullable(None));
        assert_eq!(serde_json::from_value::<Nullable<String>>(json!("a")).unwrap(), Nullable(Some("a".into())));
        let schema = schemars::SchemaGenerator::default().into_root_schema_for::<Nullable<String>>();
        assert_eq!(schema.get("anyOf"), Some(&json!([{ "type": "string" }, { "type": "null" }])));
    }
    #[test]
    fn absent_is_null_in_json_and_the_schema_is_of_the_value_alone() {
        assert_eq!(serde_json::to_value(Optional::<String>(None)).unwrap(), json!(null));
        assert_eq!(serde_json::from_value::<Optional<String>>(json!("a")).unwrap(), Optional(Some("a".into())));
        let schema = schemars::SchemaGenerator::default().into_root_schema_for::<Optional<String>>();
        assert_eq!(schema.get("type"), Some(&json!("string")));
        assert_eq!(schema.get("anyOf"), None);
    }
}
