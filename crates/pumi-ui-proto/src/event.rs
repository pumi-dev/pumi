//! What the renderer sends a UI: an event on an element the UI listens on, or on the window.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ElementId;

/// One event, on `target`: an element that has a listener for it (`setEventListener`), or the
/// window. Window events have target 0, except the window's key and selection events, which have
/// the id the UI gave when it asked for them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    #[schemars(with = "crate::op::ElementIdSchema")]
    pub target: ElementId,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    pub fn new(target: ElementId, kind: EventKind) -> Self {
        Self { target, kind }
    }
}

/// What happened, by its name (`"click"`), with what goes with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EventKind {
    // ── Pointer ──────────────────────────────────────────────────────
    /// A primary-button click, or an assistive technology's press (with no position).
    Click(Press),
    DoubleClick(Press),
    /// A click of another button.
    AuxClick(Press),
    ContextMenu(Press),
    MouseDown(Press),
    MouseUp(Press),
    MouseMove {
        position: Point,
        modifiers: Modifiers,
        /// The button held down, if one is: 0 left, 1 middle, 2 right, 3 back/forward.
        pressed_button: Option<u32>,
    },
    MouseEnter,
    MouseLeave,
    /// A press anywhere but on the element: what closes a menu.
    MouseDownOutside {
        position: Point,
        button: u32,
        modifiers: Modifiers,
    },
    /// The wheel or a trackpad. `delta` is in pixels (a line is 20).
    Scroll {
        position: Point,
        modifiers: Modifiers,
        delta: Point,
        /// Pixel-precise (a trackpad), or by lines (a mouse wheel).
        precise: bool,
        touch_phase: TouchPhase,
    },
    /// Files dropped from the OS on the element: absolute paths.
    FileDrop {
        position: Point,
        paths: Vec<String>,
    },

    // ── Keys and focus ───────────────────────────────────────────────
    KeyDown(Key),
    KeyUp(Key),
    Focus,
    Blur,

    // ── Text fields ──────────────────────────────────────────────────
    /// The field's value changed (typing, a paste, an undo).
    Change {
        value: String,
        selection: Selection,
    },
    /// Its selection or caret moved, and the value did not.
    Select {
        selection: Selection,
    },
    /// Enter, in a field that submits.
    Submit {
        value: String,
    },

    // ── Paint ────────────────────────────────────────────────────────
    /// The element's border box, in window coordinates: after the first paint with the
    /// listener, then each time its size changes.
    Layout {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// The element's scroll offset each time it moved: 0 at the start, negative as the content
    /// moves.
    ScrollChange {
        x: f64,
        y: f64,
    },
    /// A virtual list's visible rows: `start..end`.
    VisibleRange {
        start: u64,
        end: u64,
    },
    /// The matches the element's `highlight` prop found, on screen or not.
    Highlight {
        match_count: u64,
    },
    /// A `motion` target was reached.
    MotionComplete {
        generation: u64,
    },
    /// A looping animation is slowed, over its share of the frame; `compartment` is the
    /// plugin's view it is drawn in.
    MotionThrottled {
        compartment: Option<String>,
    },

    // ── The window ───────────────────────────────────────────────────
    /// The selected text changed: `None` when nothing is selected.
    SelectionChange {
        text: Option<String>,
    },
    /// The window became the key window, or stopped being it.
    WindowActivated {
        active: bool,
    },
    WindowClosed,
    /// Closing the window hid the app instead (it was the last window).
    WindowHidden,
    WindowFullscreen {
        fullscreen: bool,
    },
    WindowKeyDown(Key),
    WindowKeyUp(Key),
    /// A modifier went down or up on its own.
    WindowModifiersChanged {
        modifiers: Modifiers,
    },
}

impl EventKind {
    /// The event's name: what `setEventListener` takes, and `type` on the wire.
    pub fn name(&self) -> &'static str {
        use EventKind::*;
        match self {
            Click(_) => "click",
            DoubleClick(_) => "doubleClick",
            AuxClick(_) => "auxClick",
            ContextMenu(_) => "contextMenu",
            MouseDown(_) => "mouseDown",
            MouseUp(_) => "mouseUp",
            MouseMove { .. } => "mouseMove",
            MouseEnter => "mouseEnter",
            MouseLeave => "mouseLeave",
            MouseDownOutside { .. } => "mouseDownOutside",
            Scroll { .. } => "scroll",
            FileDrop { .. } => "fileDrop",
            KeyDown(_) => "keyDown",
            KeyUp(_) => "keyUp",
            Focus => "focus",
            Blur => "blur",
            Change { .. } => "change",
            Select { .. } => "select",
            Submit { .. } => "submit",
            Layout { .. } => "layout",
            ScrollChange { .. } => "scrollChange",
            VisibleRange { .. } => "visibleRange",
            Highlight { .. } => "highlight",
            MotionComplete { .. } => "motionComplete",
            MotionThrottled { .. } => "motionThrottled",
            SelectionChange { .. } => "selectionChange",
            WindowActivated { .. } => "windowActivated",
            WindowClosed => "windowClosed",
            WindowHidden => "windowHidden",
            WindowFullscreen { .. } => "windowFullscreen",
            WindowKeyDown(_) => "windowKeyDown",
            WindowKeyUp(_) => "windowKeyUp",
            WindowModifiersChanged { .. } => "windowModifiersChanged",
        }
    }
}

/// A point in window coordinates, in pixels; or a distance, for a scroll's `delta`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The modifier keys held. `cmd` is ⌘ on macOS and the Windows key elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

/// A mouse press or release. What a press does not know is `None`: an assistive technology's
/// click has no position.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Press {
    pub position: Option<Point>,
    /// 0 left, 1 middle, 2 right, 3 back/forward.
    pub button: Option<u32>,
    /// 1, 2 or 3 for a single, double or triple click.
    pub click_count: Option<u32>,
    pub is_right_click: Option<bool>,
    pub modifiers: Option<Modifiers>,
    /// Shared by every element's event of one press, innermost first, so a UI can stop it
    /// propagating across them.
    pub press_id: Option<u64>,
}

/// A key going down or up.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    /// Its name: `"a"`, `"enter"`, `"escape"`, `"down"`, `"f1"`.
    pub key: String,
    /// The character it types, if one: `"ß"` for ⌥s.
    pub key_char: Option<String>,
    /// Held down and repeating (key down only).
    pub is_held: Option<bool>,
    pub modifiers: Modifiers,
    /// Shared by every element's event of one keystroke and the window's, so a UI can apply
    /// `preventDefault` and `stopPropagation` across them.
    pub keystroke_id: u64,
}

/// A text field's selection as UTF-16 offsets into its value (the units JS strings use):
/// equal when it is only the caret. `end` is the caret unless it was selected backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Selection {
    pub start: u64,
    pub end: u64,
}

/// Where a trackpad scroll is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

impl TouchPhase {
    pub fn name(self) -> &'static str {
        match self {
            TouchPhase::Started => "started",
            TouchPhase::Moved => "moved",
            TouchPhase::Ended => "ended",
            TouchPhase::Cancelled => "cancelled",
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One of each kind, so a test can walk them all.
    pub(crate) fn every_kind() -> Vec<EventKind> {
        use EventKind::*;
        let key = Key { key: "a".into(), ..Default::default() };
        let at = Point::default();
        let mods = Modifiers::default();
        vec![
            Click(Press::default()),
            DoubleClick(Press::default()),
            AuxClick(Press::default()),
            ContextMenu(Press::default()),
            MouseDown(Press::default()),
            MouseUp(Press::default()),
            MouseMove { position: at, modifiers: mods, pressed_button: None },
            MouseEnter,
            MouseLeave,
            MouseDownOutside { position: at, button: 0, modifiers: mods },
            Scroll { position: at, modifiers: mods, delta: at, precise: true, touch_phase: TouchPhase::Moved },
            FileDrop { position: at, paths: vec![] },
            KeyDown(key.clone()),
            KeyUp(key.clone()),
            Focus,
            Blur,
            Change { value: String::new(), selection: Selection::default() },
            Select { selection: Selection::default() },
            Submit { value: String::new() },
            Layout { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            ScrollChange { x: 0.0, y: 0.0 },
            VisibleRange { start: 0, end: 0 },
            Highlight { match_count: 0 },
            MotionComplete { generation: 0 },
            MotionThrottled { compartment: None },
            SelectionChange { text: None },
            WindowActivated { active: true },
            WindowClosed,
            WindowHidden,
            WindowFullscreen { fullscreen: true },
            WindowKeyDown(key.clone()),
            WindowKeyUp(key),
            WindowModifiersChanged { modifiers: mods },
        ]
    }

    #[test]
    fn a_kind_is_its_name_on_the_wire() {
        for kind in every_kind() {
            let json = serde_json::to_value(Event::new(7, kind.clone())).unwrap();
            assert_eq!(json["type"], kind.name(), "{json}");
            assert_eq!(json["target"], 7);
            let back: Event = serde_json::from_value(json).unwrap();
            assert_eq!(back.kind, kind);
        }
    }

    #[test]
    fn a_press_is_flat_beside_its_type() {
        let press = Press { position: Some(Point { x: 1.0, y: 2.0 }), button: Some(0), ..Default::default() };
        let json = serde_json::to_value(Event::new(3, EventKind::Click(press))).unwrap();
        assert_eq!(json["type"], "click");
        assert_eq!(json["button"], 0);
        assert_eq!(json["position"]["y"], 2.0);
    }

    #[test]
    fn fields_are_camel_case() {
        let kind = EventKind::Highlight { match_count: 2 };
        assert_eq!(serde_json::to_value(Event::new(1, kind)).unwrap()["matchCount"], 2);
    }
}
