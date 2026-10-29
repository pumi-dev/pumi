//! The names the protocol's ops use: element types, event names and props.

/// The element types the renderer always has. Every other `createElement` type is a custom
/// element (`input`, `img`, `terminal`, …), registered by what embeds the renderer.
pub const BUILT_IN_TYPES: &[&str] = &["div", "text"];

/// The events `setEventListener` can listen for on an element: the [`crate::EventKind`] names
/// that are not the window's own.
pub const ELEMENT_EVENTS: &[&str] = &[
    "click",
    "doubleClick",
    "auxClick",
    "contextMenu",
    "mouseDown",
    "mouseUp",
    "mouseMove",
    "mouseEnter",
    "mouseLeave",
    "mouseDownOutside",
    "scroll",
    "fileDrop",
    "keyDown",
    "keyUp",
    "focus",
    "blur",
    "change",
    "select",
    "submit",
    "layout",
    "scrollChange",
    "visibleRange",
    "highlight",
    "motionComplete",
    "motionThrottled",
];

/// The custom props (`setCustomProp`) a built-in element reads. A UI need not send it others.
pub const UNIVERSAL_PROPS: &[&str] = &[
    "autoFocus",
    "tabIndex",
    "motion",
    "cache",
    "compartment",
    "testId",
    "title",
    "highlight",
    "textFlow",
    "role",
    "aria-label",
    "aria-description",
    "aria-id",
    "aria-expanded",
    "aria-selected",
    "aria-valuetext",
    "aria-level",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_element_event_is_an_event() {
        let names: Vec<&str> = crate::event::tests::every_kind().iter().map(|k| k.name()).collect();
        for event in ELEMENT_EVENTS {
            assert!(names.contains(event), "{event} is no EventKind");
            assert!(!event.starts_with("window"), "{event} is the window's");
        }
    }
}
