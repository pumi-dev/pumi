//! pumi's UI protocol: how a UI builds the window and hears what happens in it.
//!
//! A UI (React in the app today; any language later) keeps its own tree and sends the renderer
//! the changes to it as [`Op`]s, one batch per commit. The renderer keeps the same tree, draws
//! it, and sends back [`Event`]s for the listeners the UI set. Everything here is plain data
//! (serde, with a JSON Schema): nothing in it knows about JS or GPUI. `cargo xtask api` writes
//! the schema to `api/ui-protocol.json`, and the app's TS types are generated from that.
//!
//! The wire format today is JSON: a batch is an array of op tuples ([`decode_json`]).
pub mod event;
pub mod op;
mod schema;
pub mod style;
pub mod vocab;

pub use event::{Event, EventKind, Key, Modifiers, Point, Press, Selection, TouchPhase};
pub use op::{Op, decode_json};
pub use schema::schema;
pub use style::StyleDesc;

/// The protocol's level. It goes up when an op, an event or a style field changes in a way an
/// older UI or renderer would misread.
pub const LEVEL: u32 = 1;

/// An element's id. The UI picks it, unique within its window; 0 is the window itself (window
/// events). Ids stay within [`MAX_ELEMENT_ID`], so a UI that keeps them as doubles (JS) is exact.
pub type ElementId = u64;

/// The largest element id: 2^53 - 1, the largest integer a double holds exactly.
pub const MAX_ELEMENT_ID: ElementId = 9_007_199_254_740_991;

/// An id as a double (how JS holds one), checked: finite, whole, and within
/// `0..=MAX_ELEMENT_ID`.
pub fn element_id(raw: f64) -> Result<ElementId, String> {
    if !raw.is_finite() || raw < 0.0 || raw.fract() != 0.0 || raw > MAX_ELEMENT_ID as f64 {
        return Err(format!("Invalid element id: {raw}"));
    }
    Ok(raw as ElementId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_ids_are_whole_and_in_range() {
        assert_eq!(element_id(0.0), Ok(0));
        assert_eq!(element_id(MAX_ELEMENT_ID as f64), Ok(MAX_ELEMENT_ID));
        for bad in [-1.0, 1.5, f64::NAN, f64::INFINITY, MAX_ELEMENT_ID as f64 + 2.0] {
            assert!(element_id(bad).is_err(), "{bad} is not an id");
        }
    }
}
