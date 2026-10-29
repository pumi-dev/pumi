//! The mutations a UI sends: one batch per commit, applied all or nothing.
use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::ElementId;

/// One change to the window's tree. On the wire it is a tuple, its name first:
/// `["appendChild", parentId, childId]`.
///
/// A batch is decoded whole before any of it is applied ([`decode_json`]), so a malformed op
/// leaves the tree as it was.
#[derive(Debug, Clone)]
pub enum Op<'a> {
    /// A new element, not yet in the tree: `"div"`, `"text"`, or a custom element's name.
    CreateElement { id: ElementId, element_type: String },
    /// Removes an element and everything under it. The renderer answers with every id it
    /// removed, the descendants too.
    DestroyElement { id: ElementId },
    /// Moves `child_id` to be the last child of `parent_id`.
    AppendChild { parent_id: ElementId, child_id: ElementId },
    /// Moves `child_id` to be just before `before_id` under `parent_id`.
    InsertBefore { parent_id: ElementId, child_id: ElementId, before_id: ElementId },
    /// The element's whole style ([`crate::StyleDesc`]), replacing the last one.
    ///
    /// It stays raw JSON until it is applied. A parsed `StyleDesc` is ~1.4 KB, and a `Vec<Op>`
    /// is as wide as its widest variant, so inlining one made a 220k-op mount reserve over
    /// 300 MB before it parsed a single op. And the renderer shares styles by content, so it
    /// needs the bytes: hashing ~110 bytes is far cheaper than building 80 `Option` fields
    /// and throwing 99.8% of them away.
    SetStyle { id: ElementId, style: &'a RawValue },
    /// A `"text"` element's text.
    SetText { id: ElementId, content: String },
    /// Whether the UI listens for `event_type` ([`crate::vocab::ELEMENT_EVENTS`]) on the
    /// element. The renderer sends only the events something listens for.
    SetEventListener { id: ElementId, event_type: String, has_handler: bool },
    /// The element the window draws.
    SetRoot { id: ElementId },
    /// Whether controls dim while another has keyboard focus.
    SetKeyboardFocusDim { enabled: bool },
    /// Any other prop, as JSON: [`crate::vocab::UNIVERSAL_PROPS`] on built-in elements, a
    /// custom element's own on the rest. `null` unsets it.
    SetCustomProp { id: ElementId, key: String, value: serde_json::Value },
}

/// Ops are equal when they say the same; a style, when its JSON is the same text.
impl PartialEq for Op<'_> {
    fn eq(&self, other: &Self) -> bool {
        use Op::*;
        match (self, other) {
            (SetStyle { id: a, style: x }, SetStyle { id: b, style: y }) => a == b && x.get() == y.get(),
            (CreateElement { id: a, element_type: x }, CreateElement { id: b, element_type: y }) => a == b && x == y,
            (DestroyElement { id: a }, DestroyElement { id: b }) | (SetRoot { id: a }, SetRoot { id: b }) => a == b,
            (AppendChild { parent_id: a, child_id: x }, AppendChild { parent_id: b, child_id: y }) => a == b && x == y,
            (
                InsertBefore { parent_id: a, child_id: x, before_id: m },
                InsertBefore { parent_id: b, child_id: y, before_id: n },
            ) => a == b && x == y && m == n,
            (SetText { id: a, content: x }, SetText { id: b, content: y }) => a == b && x == y,
            (
                SetEventListener { id: a, event_type: x, has_handler: m },
                SetEventListener { id: b, event_type: y, has_handler: n },
            ) => a == b && x == y && m == n,
            (SetKeyboardFocusDim { enabled: a }, SetKeyboardFocusDim { enabled: b }) => a == b,
            (SetCustomProp { id: a, key: x, value: m }, SetCustomProp { id: b, key: y, value: n }) => {
                a == b && x == y && m == n
            }
            _ => false,
        }
    }
}

impl Op<'_> {
    /// The op's name on the wire.
    pub fn name(&self) -> &'static str {
        match self {
            Op::CreateElement { .. } => "createElement",
            Op::DestroyElement { .. } => "destroyElement",
            Op::AppendChild { .. } => "appendChild",
            Op::InsertBefore { .. } => "insertBefore",
            Op::SetStyle { .. } => "setStyle",
            Op::SetText { .. } => "setText",
            Op::SetEventListener { .. } => "setEventListener",
            Op::SetRoot { .. } => "setRoot",
            Op::SetKeyboardFocusDim { .. } => "setKeyboardFocusDim",
            Op::SetCustomProp { .. } => "setCustomProp",
        }
    }
}

/// A batch failure. The message names the op's index in the batch.
pub type DecodeError = String;

/// Decodes a batch from its JSON: an array of op tuples.
///
/// It goes straight from the bytes to `Vec<Op>`, with no `Vec<serde_json::Value>` in between.
/// That tree would cost a `String` per key and per value, every payload would be deep-cloned
/// out of it, and `from_value` would parse the clone a second time, so one style would be
/// allocated three times: 1.5M allocations for a 220k-op mount.
///
/// What the decode guarantees is load-bearing:
///
/// * an unknown op is a hard error, not a skipped op. Ignoring one would let a UI and a
///   renderer of different versions desync the tree instead of failing
/// * ids go through [`crate::element_id`], so non-finite, negative, fractional and
///   out-of-range values are rejected
/// * errors name the op's index. `serde_json` reports a byte offset, which is useless when
///   you are chasing a desync
/// * arguments after the ones an op takes are ignored
pub fn decode_json(bytes: &[u8]) -> Result<Vec<Op<'_>>, DecodeError> {
    serde_json::from_slice::<Batch>(bytes)
        .map(|batch| batch.0)
        .map_err(|error| format!("Failed to parse batch: {error}"))
}

struct Batch<'a>(Vec<Op<'a>>);

impl<'de> Deserialize<'de> for Batch<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OpsVisitor;

        impl<'de> Visitor<'de> for OpsVisitor {
            type Value = Batch<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an array of mutation tuples")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Batch<'de>, A::Error> {
                let mut ops = Vec::with_capacity(seq.size_hint().unwrap_or(64));
                loop {
                    // The index is attached here because this is the only place that knows it.
                    let index = ops.len();
                    match seq.next_element::<Op<'de>>() {
                        Ok(Some(op)) => ops.push(op),
                        Ok(None) => break,
                        Err(error) => return Err(de::Error::custom(format!("Batch op {index}: {error}"))),
                    }
                }
                Ok(Batch(ops))
            }
        }

        deserializer.deserialize_seq(OpsVisitor)
    }
}

/// A string argument, borrowed from the input when the JSON has no escapes.
///
/// The owned copy happens exactly once, on the way into the `Op`. Going through a `Value`
/// would allocate twice: into `Value::String`, then into the op.
struct StrArg<'a>(Cow<'a, str>);

impl<'de> Deserialize<'de> for StrArg<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StrArg<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Borrowed(v)))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Owned(v)))
            }
        }
        deserializer.deserialize_str(V)
    }
}

fn next_arg<'de, A, T>(seq: &mut A, what: &str) -> Result<T, A::Error>
where
    A: SeqAccess<'de>,
    T: Deserialize<'de>,
{
    seq.next_element()?.ok_or_else(|| de::Error::custom(format!("missing {what}")))
}

fn next_str<'de, A: SeqAccess<'de>>(seq: &mut A, what: &str) -> Result<String, A::Error> {
    Ok(next_arg::<A, StrArg>(seq, what)?.0.into_owned())
}

/// An element id. It is read as a double, the type a JS UI has, and checked.
fn next_id<'de, A: SeqAccess<'de>>(seq: &mut A, what: &str) -> Result<ElementId, A::Error> {
    let raw: f64 = next_arg(seq, what)?;
    crate::element_id(raw).map_err(de::Error::custom)
}

impl<'de> Deserialize<'de> for Op<'de> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;

        impl<'de> Visitor<'de> for V {
            type Value = Op<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a [opcode, ...args] mutation tuple")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Op<'de>, A::Error> {
                let name: StrArg<'de> = next_arg(&mut seq, "op name")?;
                let op = match name.0.as_ref() {
                    "createElement" => Op::CreateElement {
                        id: next_id(&mut seq, "id")?,
                        element_type: next_str(&mut seq, "element type")?,
                    },
                    "destroyElement" => Op::DestroyElement { id: next_id(&mut seq, "id")? },
                    "appendChild" => Op::AppendChild {
                        parent_id: next_id(&mut seq, "parent id")?,
                        child_id: next_id(&mut seq, "child id")?,
                    },
                    "insertBefore" => Op::InsertBefore {
                        parent_id: next_id(&mut seq, "parent id")?,
                        child_id: next_id(&mut seq, "child id")?,
                        before_id: next_id(&mut seq, "before id")?,
                    },
                    "setStyle" => Op::SetStyle { id: next_id(&mut seq, "id")?, style: next_arg(&mut seq, "style")? },
                    "setText" => Op::SetText { id: next_id(&mut seq, "id")?, content: next_str(&mut seq, "text")? },
                    "setEventListener" => Op::SetEventListener {
                        id: next_id(&mut seq, "id")?,
                        event_type: next_str(&mut seq, "event type")?,
                        has_handler: next_arg(&mut seq, "hasHandler")?,
                    },
                    "setRoot" => Op::SetRoot { id: next_id(&mut seq, "id")? },
                    "setKeyboardFocusDim" => Op::SetKeyboardFocusDim { enabled: next_arg(&mut seq, "enabled")? },
                    "setCustomProp" => Op::SetCustomProp {
                        id: next_id(&mut seq, "id")?,
                        key: next_str(&mut seq, "prop key")?,
                        value: next_arg(&mut seq, "custom prop value")?,
                    },
                    other => return Err(de::Error::custom(format!("unknown operation: {other:?}"))),
                };
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(op)
            }
        }

        deserializer.deserialize_seq(V)
    }
}

/// The tuples, each with its name as a constant first item.
impl JsonSchema for Op<'_> {
    fn schema_name() -> Cow<'static, str> {
        "Op".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let id = generator.subschema_for::<ElementIdSchema>();
        let style = generator.subschema_for::<crate::StyleDesc>();
        let string = json_schema!({ "type": "string" });
        let boolean = json_schema!({ "type": "boolean" });
        let tuple = |name: &str, desc: &str, args: Vec<(&str, Schema)>| {
            let mut items = vec![json_schema!({ "const": name })];
            let mut names = Vec::new();
            for (arg, schema) in args {
                names.push(arg.to_string());
                items.push(schema);
            }
            json_schema!({
                "description": format!("`[\"{name}\", {}]`: {desc}", names.join(", ")),
                "type": "array",
                "prefixItems": items,
                "minItems": items.len(),
            })
        };
        let variants = vec![
            tuple(
                "createElement",
                "a new element, not yet in the tree.",
                vec![("id", id.clone()), ("type", string.clone())],
            ),
            tuple("destroyElement", "removes the element and everything under it.", vec![("id", id.clone())]),
            tuple(
                "appendChild",
                "moves the child to be the parent's last.",
                vec![("parentId", id.clone()), ("childId", id.clone())],
            ),
            tuple(
                "insertBefore",
                "moves the child to be just before `beforeId`.",
                vec![("parentId", id.clone()), ("childId", id.clone()), ("beforeId", id.clone())],
            ),
            tuple("setStyle", "the element's whole style.", vec![("id", id.clone()), ("style", style)]),
            tuple("setText", "a text element's text.", vec![("id", id.clone()), ("content", string.clone())]),
            tuple(
                "setEventListener",
                "whether the UI listens for an event on the element.",
                vec![("id", id.clone()), ("eventType", string.clone()), ("hasHandler", boolean.clone())],
            ),
            tuple("setRoot", "the element the window draws.", vec![("id", id.clone())]),
            tuple(
                "setKeyboardFocusDim",
                "whether controls dim while another has keyboard focus.",
                vec![("enabled", boolean)],
            ),
            tuple(
                "setCustomProp",
                "any other prop, as JSON; `null` unsets it.",
                vec![("id", id), ("key", string), ("value", json_schema!(true))],
            ),
        ];
        json_schema!({
            "description": "One change to the window's tree, as a tuple with its name first.",
            "oneOf": variants,
        })
    }
}

/// [`ElementId`]'s schema: an integer within what a double holds exactly.
pub(crate) struct ElementIdSchema;

impl JsonSchema for ElementIdSchema {
    fn schema_name() -> Cow<'static, str> {
        "ElementId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An element's id: unique within its window, picked by the UI. 0 is the window.",
            "type": "integer",
            "minimum": 0,
            "maximum": crate::MAX_ELEMENT_ID,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_every_op() {
        let json = r#"[
            ["createElement", 1, "div"],
            ["destroyElement", 2],
            ["appendChild", 1, 3],
            ["insertBefore", 1, 4, 3],
            ["setStyle", 1, {"width": 10}],
            ["setText", 3, "hi"],
            ["setEventListener", 1, "click", true],
            ["setRoot", 1],
            ["setKeyboardFocusDim", false],
            ["setCustomProp", 1, "title", "Close"]
        ]"#;
        let ops = decode_json(json.as_bytes()).unwrap();
        let names: Vec<&str> = ops.iter().map(Op::name).collect();
        assert_eq!(
            names,
            [
                "createElement",
                "destroyElement",
                "appendChild",
                "insertBefore",
                "setStyle",
                "setText",
                "setEventListener",
                "setRoot",
                "setKeyboardFocusDim",
                "setCustomProp"
            ]
        );
        assert_eq!(ops[3], Op::InsertBefore { parent_id: 1, child_id: 4, before_id: 3 });
        let Op::SetStyle { style, .. } = &ops[4] else { panic!() };
        assert_eq!(style.get(), r#"{"width": 10}"#);
    }

    #[test]
    fn an_unknown_op_names_its_index() {
        let err = decode_json(br#"[["setRoot", 1], ["explode", 2]]"#).unwrap_err();
        assert!(err.contains("Batch op 1") && err.contains("explode"), "{err}");
    }

    #[test]
    fn a_bad_id_fails_the_batch() {
        let err = decode_json(br#"[["setRoot", 1.5]]"#).unwrap_err();
        assert!(err.contains("Invalid element id"), "{err}");
    }

    #[test]
    fn trailing_arguments_are_ignored() {
        assert_eq!(decode_json(br#"[["setRoot", 1, "extra"]]"#).unwrap(), [Op::SetRoot { id: 1 }]);
    }
}
