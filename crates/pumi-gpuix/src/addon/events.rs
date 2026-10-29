//! Events as JS gets them: one flat `EventPayload` for every kind, its fields optional, each
//! kind setting the ones it has. The renderer sends typed [`pumi_ui_proto::Event`]s; this is
//! where they are flattened for the addon.
use napi_derive::napi;
use pumi_ui_proto::{Event, EventKind, Key, Modifiers, Point, Press, Selection};

/// Event payload sent back to JS when a user interacts with an element.
#[derive(Debug, Clone)]
#[napi(object)]
pub struct EventPayload {
    /// Numeric element ID (matches the ID assigned in JS via createElement).
    pub element_id: f64,

    /// Event type string — matches the key used in EVENT_PROPS on the JS side.
    /// e.g. "click", "mouseDown", "mouseEnter", "keyDown", "scroll", etc.
    pub event_type: String,

    // ── Mouse position ───────────────────────────────────────────────
    /// Mouse X position in window coordinates (pixels).
    pub x: Option<f64>,
    /// Mouse Y position in window coordinates (pixels).
    pub y: Option<f64>,

    // ── Mouse button ─────────────────────────────────────────────────
    /// Which mouse button: 0=left, 1=middle, 2=right.
    /// Populated for: mouseDown, mouseUp, click, mouseDownOutside, contextMenu.
    pub button: Option<u32>,

    /// Number of consecutive clicks (1=single, 2=double, 3=triple).
    /// Populated for: mouseDown, mouseUp, click.
    pub click_count: Option<u32>,

    /// Whether this is a right-click (convenience for click events).
    /// true when button==2 or ClickEvent::is_right_click().
    pub is_right_click: Option<bool>,

    /// Which mouse button is currently held during a mouseMove.
    /// Same encoding as `button`: 0=left, 1=middle, 2=right.
    /// Populated for: mouseMove.
    pub pressed_button: Option<u32>,

    // ── Keyboard ─────────────────────────────────────────────────────
    /// Key name, e.g. "a", "enter", "escape", "down", "left", "f1".
    /// Populated for: keyDown, keyUp.
    pub key: Option<String>,

    /// The character produced by the key press (e.g. "ß" for option-s).
    /// May differ from `key` when modifiers are active.
    /// Populated for: keyDown, keyUp.
    pub key_char: Option<String>,

    /// Whether this is a key-repeat event (key held down).
    /// Populated for: keyDown.
    pub is_held: Option<bool>,

    /// Id of the GPUI key dispatch that produced this event. Every element
    /// `keyDown` and the trailing `windowKeyDown` of one keystroke share it, so
    /// JS can apply `preventDefault` / `stopPropagation` across them.
    /// Populated for: keyDown, keyUp, windowKeyDown, windowKeyUp.
    pub keystroke_id: Option<f64>,

    /// Id of the GPUI mouse press or release dispatch that produced this event.
    /// Every element's event of one press shares it, innermost first, so JS can
    /// apply `stopPropagation` across them.
    /// Populated for: click, doubleClick, auxClick, contextMenu, mouseDown, mouseUp.
    pub press_id: Option<f64>,

    // ── Scroll ───────────────────────────────────────────────────────
    /// Scroll delta on the X axis (pixels or lines, see `precise`).
    /// Populated for: scroll.
    pub delta_x: Option<f64>,

    /// Scroll delta on the Y axis (pixels or lines, see `precise`).
    /// Populated for: scroll.
    pub delta_y: Option<f64>,

    /// true = pixel-precise (trackpad), false = line-based (mouse wheel).
    /// Populated for: scroll.
    pub precise: Option<bool>,

    /// Touch phase for scroll: "started", "moved", "ended".
    /// Populated for: scroll (trackpad gestures).
    pub touch_phase: Option<String>,

    // ── Hover ────────────────────────────────────────────────────────
    /// true = mouse entered element, false = mouse left element.
    /// Populated for: mouseEnter, mouseLeave.
    pub hovered: Option<bool>,

    // ── Custom element payloads ──────────────────────────────────────
    /// Element-defined string payload.
    /// Populated for: `<diff>` toggleFile (the file path), showMore (the
    /// hidden line count), and lineClick (the line text); `<markdown>`
    /// linkClick (the URL); `selectionChange` (joined selected text, or
    /// absent when the selection is empty); motionThrottled (the name of the
    /// compartment the animation is drawn in, if it has one).
    pub value: Option<String>,

    /// Line number on the pre-change side. Populated for: `<diff>` lineClick.
    pub old_line: Option<f64>,

    /// Line number on the post-change side. Populated for: `<diff>` lineClick.
    pub new_line: Option<f64>,

    /// First visible logical index. Populated for: `<virtual-list>` visibleRange.
    pub start_index: Option<f64>,

    /// Exclusive end of the visible logical range. Populated for: visibleRange.
    pub end_index: Option<f64>,

    /// Border-box size of the element, in pixels. With `x` and `y`, its position
    /// in window coordinates. Populated for: layout.
    pub width: Option<f64>,
    pub height: Option<f64>,

    /// Scroll offset of the element, the pair `getScrollOffset` returns: 0 at the
    /// start and negative as the content moves. Populated for: scrollChange.
    pub scroll_x: Option<f64>,
    pub scroll_y: Option<f64>,

    /// Matches found by this element's `highlight` prop. Counted once per match
    /// even when it is split across several painted runs, and it counts every
    /// retained match, not only the ones currently on screen.
    /// Populated for: highlight.
    pub match_count: Option<f64>,

    /// Logical native motion target that reached completion.
    /// Populated for: motionComplete.
    pub motion_generation: Option<f64>,

    /// Absolute filesystem paths from a Finder / OS file drop.
    /// Populated for: fileDrop.
    pub paths: Option<Vec<String>>,

    /// A text editor's selection, as UTF-16 offsets into its value: equal when it is only the
    /// caret. `selectionEnd` is where the caret is unless the selection was made backwards.
    /// Populated for: `<input>` and `<textarea>` change and select.
    pub selection_start: Option<f64>,
    pub selection_end: Option<f64>,

    // ── Modifiers ────────────────────────────────────────────────────
    pub modifiers: Option<EventModifiers>,
}

impl Default for EventPayload {
    fn default() -> Self {
        Self {
            element_id: 0.0,
            event_type: String::new(),
            x: None,
            y: None,
            button: None,
            click_count: None,
            is_right_click: None,
            pressed_button: None,
            key: None,
            key_char: None,
            is_held: None,
            keystroke_id: None,
            press_id: None,
            delta_x: None,
            delta_y: None,
            precise: None,
            touch_phase: None,
            hovered: None,
            value: None,
            old_line: None,
            new_line: None,
            start_index: None,
            end_index: None,
            width: None,
            height: None,
            scroll_x: None,
            scroll_y: None,
            match_count: None,
            motion_generation: None,
            paths: None,
            selection_start: None,
            selection_end: None,
            modifiers: None,
        }
    }
}

#[derive(Debug, Clone)]
#[napi(object)]
#[derive(Default)]
pub struct EventModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl From<Modifiers> for EventModifiers {
    fn from(m: Modifiers) -> Self {
        Self { shift: m.shift, ctrl: m.ctrl, alt: m.alt, cmd: m.cmd }
    }
}

impl From<Event> for EventPayload {
    /// The fields each kind sets are the ones it always has; nothing else is set.
    fn from(event: Event) -> Self {
        let mut p = EventPayload {
            element_id: event.target as f64,
            event_type: event.kind.name().to_string(),
            ..Default::default()
        };
        let at = |p: &mut EventPayload, point: Point| {
            p.x = Some(point.x);
            p.y = Some(point.y);
        };
        let press = |p: &mut EventPayload, press: Press| {
            if let Some(position) = press.position {
                p.x = Some(position.x);
                p.y = Some(position.y);
            }
            p.button = press.button;
            p.click_count = press.click_count;
            p.is_right_click = press.is_right_click;
            p.modifiers = press.modifiers.map(Into::into);
            p.press_id = press.press_id.map(|id| id as f64);
        };
        let key = |p: &mut EventPayload, key: Key| {
            p.key = Some(key.key);
            p.key_char = key.key_char;
            p.is_held = key.is_held;
            p.modifiers = Some(key.modifiers.into());
            p.keystroke_id = Some(key.keystroke_id as f64);
        };
        let selection = |p: &mut EventPayload, selection: Selection| {
            p.selection_start = Some(selection.start as f64);
            p.selection_end = Some(selection.end as f64);
        };
        use EventKind::*;
        match event.kind {
            Click(x) | DoubleClick(x) | AuxClick(x) | ContextMenu(x) | MouseDown(x) | MouseUp(x) => press(&mut p, x),
            MouseMove { position, modifiers, pressed_button } => {
                at(&mut p, position);
                p.modifiers = Some(modifiers.into());
                p.pressed_button = pressed_button;
            }
            MouseEnter => p.hovered = Some(true),
            MouseLeave => p.hovered = Some(false),
            MouseDownOutside { position, button, modifiers } => {
                at(&mut p, position);
                p.button = Some(button);
                p.modifiers = Some(modifiers.into());
            }
            Scroll { position, modifiers, delta, precise, touch_phase } => {
                at(&mut p, position);
                p.modifiers = Some(modifiers.into());
                p.precise = Some(precise);
                p.delta_x = Some(delta.x);
                p.delta_y = Some(delta.y);
                p.touch_phase = Some(touch_phase.name().to_string());
            }
            FileDrop { position, paths } => {
                at(&mut p, position);
                p.paths = Some(paths);
            }
            KeyDown(k) | KeyUp(k) | WindowKeyDown(k) | WindowKeyUp(k) => key(&mut p, k),
            Focus | Blur | WindowClosed | WindowHidden => {}
            Change { value, selection: s } => {
                p.value = Some(value);
                selection(&mut p, s);
            }
            Select { selection: s } => selection(&mut p, s),
            Submit { value } => p.value = Some(value),
            Layout { x, y, width, height } => {
                p.x = Some(x);
                p.y = Some(y);
                p.width = Some(width);
                p.height = Some(height);
            }
            ScrollChange { x, y } => {
                p.scroll_x = Some(x);
                p.scroll_y = Some(y);
            }
            VisibleRange { start, end } => {
                p.start_index = Some(start as f64);
                p.end_index = Some(end as f64);
            }
            Highlight { match_count } => p.match_count = Some(match_count as f64),
            MotionComplete { generation } => p.motion_generation = Some(generation as f64),
            MotionThrottled { compartment } => p.value = compartment,
            SelectionChange { text } => p.value = text,
            WindowActivated { active } => p.hovered = Some(active),
            WindowFullscreen { fullscreen } => p.hovered = Some(fullscreen),
            WindowModifiersChanged { modifiers } => p.modifiers = Some(modifiers.into()),
        }
        p
    }
}
