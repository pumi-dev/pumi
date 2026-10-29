/// Retained element tree — the Rust-side source of truth for the UI.
///
/// The UI sends mutations (`pumi_ui_proto::Op`: create, append, remove, etc.).
/// This tree stores those mutations. GpuixView builds ephemeral GPUI elements
/// from it, while virtual lists defer offscreen subtrees until layout requests them.
///
/// All IDs are u64, picked by the UI (`pumi_ui_proto::ElementId`); a JS UI counts
/// them up and passes them as numbers (no string allocation).
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock};

use crate::style::StyleDesc;

/// What one `elements_at` walk carries along.
struct HitWalk<'a> {
    x: f64,
    y: f64,
    painted_box: &'a dyn Fn(u64) -> Option<crate::automation::ElementBounds>,
    /// Deferred `anchored` layers met so far, each with its priority and hits.
    layers: Vec<(u64, Vec<u64>)>,
}

pub struct RetainedElement {
    pub id: u64,
    pub element_type: String,
    /// Shared, not owned. `StyleDesc` is ~1.4 KB, and holding it inline made an
    /// unstyled element pay for a style it does not have: `RetainedElement` was
    /// 1624 bytes, and the `HashMap` stores values inline, so the whole table
    /// was that wide. Behind an `Arc` it was 248 bytes; cold metadata makes it 160.
    ///
    /// The pointer is also shared between every element that declared the same
    /// style. A 10k-turn chat has 59 320 `setStyle` ops and 90 distinct styles.
    ///
    /// Read it with `.as_deref()` to get `Option<&StyleDesc>`. Never mutate
    /// through it; `Arc` has no `DerefMut`, so the compiler enforces that.
    pub style: Option<Arc<StyleDesc>>,
    pub content: Option<String>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    /// Most host nodes have neither listeners nor custom props.
    extras: Option<Box<ElementExtras>>,
    /// Take keyboard focus the first time this element gets a focus handle.
    /// Without it an `<input>` is dead until the user clicks it.
    pub auto_focus: bool,
    /// Last mutation applied to this element or one of its descendants.
    pub subtree_revision: u64,
    /// Last change to the searchable TEXT of this subtree: content, structure,
    /// or the set of nested `highlight` declarations.
    ///
    /// Separate from `subtree_revision` because `highlight` is itself a custom
    /// prop: keying the group cache on the general revision means every find-bar
    /// keystroke re-walks and re-folds the whole subtree, which is the one case
    /// the cache exists for. Style, `activeIndex`, colours and a native
    /// element's own props never move this.
    pub search_revision: u64,
    /// Stable locator id from the React `testId` prop.
    pub test_id: Option<String>,
}

#[derive(Default)]
struct ElementExtras {
    events: HashSet<String>,
    custom_props: HashMap<String, serde_json::Value>,
}

static EMPTY_EXTRAS: LazyLock<ElementExtras> = LazyLock::new(ElementExtras::default);

impl RetainedElement {
    /// The element's `tabIndex`, when it has a usable one.
    pub fn tab_index(&self) -> Option<isize> {
        self.custom_props()
            .get("tabIndex")
            .and_then(|value| value.as_i64())
            .and_then(|index| isize::try_from(index).ok())
    }

    /// Whether the element must own a gpui focus handle: it takes text, is in the
    /// tab order, or listens for a key or focus event.
    pub fn needs_focus(&self) -> bool {
        matches!(self.element_type.as_str(), "input" | "textarea")
            || self.tab_index().is_some()
            || self.events().contains("keyDown")
            || self.events().contains("keyUp")
            || self.events().contains("focus")
            || self.events().contains("blur")
    }

    pub fn new(id: u64, element_type: String, revision: u64) -> Self {
        Self {
            id,
            element_type,
            style: None,
            content: None,
            children: Vec::new(),
            parent: None,
            extras: None,
            auto_focus: false,
            subtree_revision: revision,
            search_revision: revision,
            test_id: None,
        }
    }

    pub fn events(&self) -> &HashSet<String> {
        &self.extras.as_deref().unwrap_or(&EMPTY_EXTRAS).events
    }

    pub fn custom_props(&self) -> &HashMap<String, serde_json::Value> {
        &self.extras.as_deref().unwrap_or(&EMPTY_EXTRAS).custom_props
    }

    fn extras_mut(&mut self) -> &mut ElementExtras {
        self.extras.get_or_insert_with(Default::default)
    }

    fn release_empty_extras(&mut self) {
        if self.extras.as_ref().is_some_and(|extras| extras.events.is_empty() && extras.custom_props.is_empty()) {
            self.extras = None;
        }
    }
}

/// The element map, keyed by the u64 counter JS allocates.
///
/// Deliberately not the std hasher. `mark_changed` probes this map once per
/// ancestor hop on every append, insert, style and text mutation, and
/// `build_virtual_list` probes it twice per child on every frame. The keys come
/// from our own counter, never from user input, so SipHash only costs time.
pub type ElementMap = rustc_hash::FxHashMap<u64, RetainedElement>;

/// One hash-consed style: the raw JSON that produced it, and the shared value.
///
/// The raw bytes are kept so a hash hit can be confirmed by comparing content.
/// A 64-bit hash collides eventually, and a collision here would paint one
/// element with another element's style, which is a bug nobody would find.
struct InternedStyle {
    raw: Box<[u8]>,
    style: Arc<StyleDesc>,
}

/// Below this many entries a sweep is not worth thinking about, so the table
/// never sweeps on a small app just because it grew from one style to two.
pub(crate) const STYLE_SWEEP_FLOOR: usize = 64;

/// Styles shared by content. Keyed by a hash of the raw payload, with a bucket
/// per key so collisions are resolved by comparing bytes.
///
/// Interning happens here, on arrival, rather than in the protocol. JS cannot
/// do it safely: `commitUpdate` resends the full style on every commit, and a
/// dragged element produces a distinct style every frame, so a JS-owned table
/// would grow without bound and the update path would send a definition plus a
/// reference where it sends one op today.
///
/// Separate from the element tree so `resolve_styles` can borrow it alone. That
/// is what makes a batch atomic: interning is the only fallible step, and the
/// borrow checker proves it cannot have touched an element.
#[derive(Default)]
pub struct StyleTable {
    entries: rustc_hash::FxHashMap<u64, Vec<InternedStyle>>,
    /// Entries currently held. Tracked rather than summed, so `maybe_sweep` is
    /// O(1) on the commits that do not sweep.
    count: usize,
    /// `count` right after the last sweep.
    swept_at: usize,
}

impl StyleTable {
    /// Parse a style payload, reusing the shared value when the same bytes
    /// arrived before. Hashing ~110 bytes is far cheaper than building the
    /// ~80 `Option` fields of a `StyleDesc`.
    pub fn intern(&mut self, raw: &[u8]) -> Result<Arc<StyleDesc>, String> {
        let mut hasher = rustc_hash::FxHasher::default();
        raw.hash(&mut hasher);
        let key = hasher.finish();
        if let Some(bucket) = self.entries.get(&key) {
            if let Some(hit) = bucket.iter().find(|entry| &*entry.raw == raw) {
                return Ok(hit.style.clone());
            }
        }
        let style: StyleDesc = serde_json::from_slice(raw).map_err(|error| error.to_string())?;
        let shared = Arc::new(style);
        self.entries.entry(key).or_default().push(InternedStyle { raw: raw.into(), style: shared.clone() });
        self.count += 1;
        Ok(shared)
    }

    /// Drop interned styles no element references any more.
    ///
    /// This is what makes the table safe on an interactive app. A drag creates
    /// one style per frame; each is released once the element stops referencing
    /// it. `strong_count == 1` means the table holds the last reference, so
    /// nothing can be reading it.
    pub fn sweep(&mut self) {
        let mut live = 0;
        self.entries.retain(|_, bucket| {
            bucket.retain(|entry| Arc::strong_count(&entry.style) > 1);
            live += bucket.len();
            !bucket.is_empty()
        });
        self.count = live;
        self.swept_at = live;
    }

    /// Sweep when the table has grown, or when the tree has shrunk under it.
    ///
    /// Growth alone is not enough. After a large mount `swept_at` is large, so
    /// destroying the root or remounting a smaller app leaves `count` below the
    /// next threshold forever, and the whole high-water table stays resident.
    /// Re-interning an existing style does not raise `count`, so nothing would
    /// ever reclaim it.
    ///
    /// `live_elements` closes that. A live style needs at least one element
    /// holding it, so the live style count can never exceed the element count;
    /// a table far larger than the tree is therefore mostly dead. After a sweep
    /// `count <= live_elements`, so this cannot thrash.
    ///
    /// Both arms are amortized: each scan is O(entries) and follows at least as
    /// many interns or as large a collapse.
    pub fn maybe_sweep(&mut self, live_elements: usize) {
        let grew = self.count >= self.swept_at.saturating_mul(2).max(STYLE_SWEEP_FLOOR);
        let tree_shrank = self.count > live_elements.saturating_mul(2).max(STYLE_SWEEP_FLOOR);
        if grew || tree_shrank {
            self.sweep();
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }
}

pub struct RetainedTree {
    pub elements: ElementMap,
    pub motion_ids: HashSet<u64>,
    /// Elements that `needs_focus`, in id order. Kept as mutations arrive so the
    /// per-frame focus sync visits these few instead of every retained element:
    /// a large tree retains thousands, and nearly none are focusable.
    pub focusable_ids: std::collections::BTreeSet<u64>,
    /// How many `anchored` elements exist. Without any, an `elements_at` walk
    /// can drop a clipped subtree whole.
    anchored_count: usize,
    pub styles: StyleTable,
    /// The root element ID set by appendChildToContainer.
    pub root_id: Option<u64>,
    /// Dim other focusables while a control has keyboard focus. Set per root.
    keyboard_focus_dim: bool,
    next_revision: u64,
    /// Something a frame would draw changed since the last `take_dirty`. A batch
    /// of resends, which is what a re-render with unchanged props produces,
    /// leaves it false, so the window is not asked to rebuild its element tree
    /// for a frame that would come out identical.
    dirty: bool,
}

impl Default for RetainedTree {
    fn default() -> Self {
        Self::new()
    }
}

impl RetainedTree {
    pub fn new() -> Self {
        Self {
            elements: ElementMap::default(),
            motion_ids: HashSet::new(),
            focusable_ids: Default::default(),
            anchored_count: 0,
            styles: StyleTable::default(),
            root_id: None,
            keyboard_focus_dim: true,
            next_revision: 1,
            dirty: false,
        }
    }

    /// Whether a frame drawn now would differ from the last one, clearing the
    /// answer. Call once per applied batch.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn keyboard_focus_dim(&self) -> bool {
        self.keyboard_focus_dim
    }

    pub fn set_keyboard_focus_dim(&mut self, enabled: bool) {
        if self.keyboard_focus_dim != enabled {
            self.keyboard_focus_dim = enabled;
            self.dirty = true;
        }
    }

    pub fn set_root(&mut self, id: u64) {
        if self.root_id != Some(id) {
            self.root_id = Some(id);
            self.dirty = true;
        }
    }

    pub fn create_element(&mut self, id: u64, element_type: String) {
        self.anchored_count += usize::from(element_type == "anchored");
        self.dirty = true;
        let revision = self.take_revision();
        self.elements.insert(id, RetainedElement::new(id, element_type, revision));
        self.track_focusable(id);
    }

    /// Bring `focusable_ids` in line with `id` after anything `needs_focus`
    /// reads has changed.
    fn track_focusable(&mut self, id: u64) {
        if self.elements.get(&id).is_some_and(RetainedElement::needs_focus) {
            self.focusable_ids.insert(id);
        } else {
            self.focusable_ids.remove(&id);
        }
    }

    fn take_revision(&mut self) -> u64 {
        // Every content change takes a revision to invalidate caches, so this is
        // the one place that covers style, text, structure and custom props.
        self.dirty = true;
        let revision = self.next_revision;
        self.next_revision = self.next_revision.wrapping_add(1).max(1);
        revision
    }

    /// Invalidate `id` and its ancestors, including their searchable text.
    fn mark_changed(&mut self, id: u64) {
        self.mark_changed_detail(id, true);
    }

    /// Invalidate for rendering only. Use for changes that cannot move a glyph
    /// into or out of the searchable text: style, and a native element's own
    /// props, whose text is matched at paint and never enters a `GroupList`.
    fn mark_render_changed(&mut self, id: u64) {
        self.mark_changed_detail(id, false);
    }

    fn mark_changed_detail(&mut self, id: u64, search: bool) {
        let revision = self.take_revision();
        let mut current = Some(id);
        while let Some(current_id) = current {
            let Some(element) = self.elements.get_mut(&current_id) else {
                break;
            };
            element.subtree_revision = revision;
            if search {
                element.search_revision = revision;
            }
            current = element.parent;
        }
    }

    /// Recursively destroy an element and all its children.
    /// Returns all destroyed IDs so the caller can clean up JS-side state.
    ///
    /// Unlinks from the parent before removing, then marks the parent chain
    /// changed so caches cannot serve text that is no longer in the tree.
    pub fn destroy_element(&mut self, id: u64) -> Vec<u64> {
        let parent_id = self.elements.get(&id).and_then(|element| element.parent);
        if let Some(parent_id) = parent_id {
            if let Some(parent) = self.elements.get_mut(&parent_id) {
                parent.children.retain(|child| *child != id);
            }
        }
        let mut destroyed = Vec::new();
        self.destroy_element_recursive(id, &mut destroyed);
        if !destroyed.is_empty() {
            self.dirty = true;
        }
        if self.root_id == Some(id) {
            self.root_id = None;
        }
        if let Some(parent_id) = parent_id {
            self.mark_changed(parent_id);
        }
        destroyed
    }

    fn destroy_element_recursive(&mut self, id: u64, destroyed: &mut Vec<u64>) {
        if let Some(element) = self.elements.remove(&id) {
            self.motion_ids.remove(&id);
            self.focusable_ids.remove(&id);
            self.anchored_count -= usize::from(element.element_type == "anchored");
            destroyed.push(id);
            for child_id in element.children {
                self.destroy_element_recursive(child_id, destroyed);
            }
        }
    }

    pub fn append_child(&mut self, parent_id: u64, child_id: u64) {
        // Remove from old parent if any
        let old_parent_id = self.elements.get(&child_id).and_then(|e| e.parent);
        if let Some(old_parent_id) = old_parent_id {
            if let Some(old_parent) = self.elements.get_mut(&old_parent_id) {
                old_parent.children.retain(|c| *c != child_id);
            }
        }
        // Set new parent
        if let Some(child) = self.elements.get_mut(&child_id) {
            child.parent = Some(parent_id);
        }
        // Add to new parent's children
        if let Some(parent) = self.elements.get_mut(&parent_id) {
            parent.children.push(child_id);
        }
        if let Some(old_parent_id) = old_parent_id {
            self.mark_changed(old_parent_id);
        }
        self.mark_changed(parent_id);
    }

    pub fn insert_before(&mut self, parent_id: u64, child_id: u64, before_id: u64) {
        // Remove from old parent if any
        let old_parent_id = self.elements.get(&child_id).and_then(|e| e.parent);
        if let Some(old_parent_id) = old_parent_id {
            if let Some(old_parent) = self.elements.get_mut(&old_parent_id) {
                old_parent.children.retain(|c| *c != child_id);
            }
        }
        // Set new parent
        if let Some(child) = self.elements.get_mut(&child_id) {
            child.parent = Some(parent_id);
        }
        // Insert before the target
        if let Some(parent) = self.elements.get_mut(&parent_id) {
            let pos = parent.children.iter().position(|c| *c == before_id).unwrap_or(parent.children.len());
            parent.children.insert(pos, child_id);
        }
        if let Some(old_parent_id) = old_parent_id {
            self.mark_changed(old_parent_id);
        }
        self.mark_changed(parent_id);
    }

    /// Takes an interned style. Pointer identity is the fast path and covers
    /// every resend, because `StyleTable::intern` returns the same `Arc` for
    /// the same bytes. It is not sufficient on its own: interning keys on raw
    /// bytes, so `{"color":"red","width":10}` and `{"width":10,"color":"red"}`
    /// are two `Arc`s holding the same style. Only a pointer miss pays for the
    /// ~80-field compare, which is what decides whether this repaints.
    pub fn set_style(&mut self, id: u64, style: Arc<StyleDesc>) {
        let mut changed = false;
        if let Some(element) = self.elements.get_mut(&id) {
            let same =
                element.style.as_ref().is_some_and(|current| Arc::ptr_eq(current, &style) || **current == *style);
            if !same {
                element.style = Some(style);
                changed = true;
            }
        }
        if changed {
            self.mark_render_changed(id);
        }
    }

    pub fn set_text(&mut self, id: u64, content: String) {
        let mut changed = false;
        if let Some(element) = self.elements.get_mut(&id) {
            if element.content.as_ref() != Some(&content) {
                element.content = Some(content);
                changed = true;
            }
        }
        if changed {
            self.mark_changed(id);
        }
    }

    pub fn set_event_listener(&mut self, id: u64, event_type: String, has_handler: bool) {
        if let Some(element) = self.elements.get_mut(&id) {
            let changed = if has_handler {
                element.extras_mut().events.insert(event_type)
            } else if let Some(extras) = element.extras.as_mut() {
                let changed = extras.events.remove(&event_type);
                element.release_empty_extras();
                changed
            } else {
                false
            };
            // Which listeners exist decides how the element is built, though it
            // does not touch any cached text.
            if changed {
                self.track_focusable(id);
                self.mark_render_changed(id);
            }
        }
    }

    /// Set a custom prop on an element (for non-div/text elements).
    ///
    /// Custom props never change the searchable text of a subtree: a native
    /// element's strings are matched at paint, not collected into a group. The
    /// one exception is `highlight` itself appearing or disappearing, which
    /// changes which subtrees an ancestor skips.
    pub fn set_custom_prop(&mut self, id: u64, key: String, value: serde_json::Value) {
        let mut changed = false;
        let is_highlight = key == "highlight";
        let is_motion = key == "motion";
        let is_tab_index = key == "tabIndex";
        let was_declaration =
            self.elements.get(&id).is_some_and(|element| element.custom_props().contains_key("highlight"));
        if let Some(element) = self.elements.get_mut(&id) {
            // `autoFocus` applies to every element type, so it is lifted out of
            // the custom-prop map that only custom elements read.
            if key == "autoFocus" || key == "testId" {
                let lifted_changed = if key == "autoFocus" {
                    let auto_focus = value.as_bool().unwrap_or(false);
                    // A custom element (`<terminal>`, `<webview>`) takes the focus itself, from
                    // its own `autoFocus` prop: it holds a gpui view whose handle the renderer
                    // never sees, so the prop is in its props too.
                    if !matches!(element.element_type.as_str(), "div" | "text" | "virtual-list") {
                        element.extras_mut().custom_props.insert(key, serde_json::Value::Bool(auto_focus));
                    }
                    std::mem::replace(&mut element.auto_focus, auto_focus) != auto_focus
                } else {
                    let test_id = value.as_str().map(str::to_string);
                    std::mem::replace(&mut element.test_id, test_id.clone()) != test_id
                };
                // A cached subtree is built again for it (`crate::boundary`): a terminal takes
                // focus from `autoFocus` when it is built.
                if lifted_changed {
                    self.mark_render_changed(id);
                }
                return;
            }
            if value.is_null() {
                changed = element.extras.as_mut().is_some_and(|extras| extras.custom_props.remove(&key).is_some());
                element.release_empty_extras();
            } else if element.custom_props().get(&key) != Some(&value) {
                element.extras_mut().custom_props.insert(key, value);
                changed = true;
            }
        }
        if !changed {
            return;
        }
        if is_tab_index {
            self.track_focusable(id);
        }
        if is_motion {
            if self.elements.get(&id).is_some_and(|element| element.custom_props().contains_key("motion")) {
                self.motion_ids.insert(id);
            } else {
                self.motion_ids.remove(&id);
            }
        }
        self.mark_render_changed(id);
        let is_declaration =
            self.elements.get(&id).is_some_and(|element| element.custom_props().contains_key("highlight"));
        if is_highlight && was_declaration != is_declaration {
            // Only the ancestors: `GroupList::collect` skips a nested
            // declaration's subtree, so which subtrees exist changed for them.
            // This element's own groups are unaffected by its own query.
            if let Some(parent) = self.elements.get(&id).and_then(|element| element.parent) {
                self.mark_changed(parent);
            }
        }
    }

    /// Read a custom prop value from an element.
    pub fn get_custom_prop(&self, id: u64, key: &str) -> Option<&serde_json::Value> {
        self.elements.get(&id)?.custom_props().get(key)
    }

    pub fn to_json(
        &self,
        bounds: &std::collections::HashMap<u64, crate::automation::ElementBounds>,
    ) -> serde_json::Value {
        self.to_json_detail(bounds, true)
    }

    /// Locator tree. Skip style maps so a 5k-row list is not 100ms of JSON.
    pub fn to_automation_json(
        &self,
        bounds: &std::collections::HashMap<u64, crate::automation::ElementBounds>,
    ) -> serde_json::Value {
        self.to_json_detail(bounds, false)
    }

    /// The elements whose painted box holds the point, topmost first: the DOM's
    /// `elementsFromPoint`.
    ///
    /// There is no z-index, so tree order is paint order. Walking each
    /// element's children last to first, and the element after them, yields
    /// the topmost element first. The exception is a deferred `anchored`
    /// layer, which gpui paints after the tree by `priority` and without the
    /// clip of the elements around it: its hits come first.
    ///
    /// `painted_box` is the last frame's box. An element that was not painted
    /// (hidden, or outside a virtual list's window) has none and is never hit,
    /// though its descendants are still visited. What clips matches what is
    /// drawn: `overflow` of `hidden` or `scroll`, per axis, hides whatever lies
    /// outside the clipping box. `pointer-events: none` is inherited, like in
    /// CSS, and `auto` restores it.
    pub fn elements_at(
        &self,
        x: f64,
        y: f64,
        painted_box: &dyn Fn(u64) -> Option<crate::automation::ElementBounds>,
    ) -> Vec<u64> {
        let mut walk = HitWalk { x, y, painted_box, layers: Vec::new() };
        let mut hits = Vec::new();
        if let Some(root_id) = self.root_id {
            self.collect_hits(root_id, false, false, &mut walk, &mut hits);
        }
        // Stable, so layers of one priority stay topmost first.
        walk.layers.sort_by(|a, b| b.0.cmp(&a.0));
        let mut ordered: Vec<u64> = walk.layers.into_iter().flat_map(|(_, layer)| layer).collect();
        ordered.extend(hits);
        ordered
    }

    /// `none` is the inherited `pointer-events: none`. `clipped` means an
    /// ancestor's clip excludes the point, so only deferred layers below can
    /// still be hit.
    fn collect_hits(&self, id: u64, none: bool, clipped: bool, walk: &mut HitWalk, hits: &mut Vec<u64>) {
        let Some(element) = self.elements.get(&id) else {
            return;
        };
        let style = element.style.as_deref();
        let own_box = (walk.painted_box)(id);
        let (x, y) = (walk.x, walk.y);
        let inside = |bounds: &crate::automation::ElementBounds, horizontal: bool, vertical: bool| {
            (!horizontal || (x >= bounds.x && x < bounds.x + bounds.width))
                && (!vertical || (y >= bounds.y && y < bounds.y + bounds.height))
        };

        let mut clipped = clipped;
        if let (Some(style), Some(bounds)) = (style, &own_box) {
            let clips = |axis: Option<&str>| matches!(axis, Some("hidden" | "scroll"));
            let clip_x = clips(style.overflow_x.as_deref().or(style.overflow.as_deref()));
            let clip_y = clips(style.overflow_y.as_deref().or(style.overflow.as_deref()));
            if (clip_x || clip_y) && !inside(bounds, clip_x, clip_y) {
                if self.anchored_count == 0 {
                    return;
                }
                clipped = true;
            }
        }

        let none = match style.and_then(|style| style.pointer_events.as_deref()) {
            Some("none") => true,
            Some("auto") => false,
            _ => none,
        };

        if element.element_type == "anchored" {
            let deferred = element.custom_props().get("deferred").and_then(|value| value.as_bool()).unwrap_or(true);
            if deferred {
                let priority = element.custom_props().get("priority").and_then(|value| value.as_u64()).unwrap_or(1);
                let mut layer = Vec::new();
                for &child in element.children.iter().rev() {
                    self.collect_hits(child, none, false, walk, &mut layer);
                }
                walk.layers.push((priority, layer));
                return;
            }
        }

        for &child in element.children.iter().rev() {
            self.collect_hits(child, none, clipped, walk, hits);
        }
        if !clipped && !none && own_box.is_some_and(|bounds| inside(&bounds, true, true)) {
            hits.push(id);
        }
    }

    fn to_json_detail(
        &self,
        bounds: &std::collections::HashMap<u64, crate::automation::ElementBounds>,
        include_details: bool,
    ) -> serde_json::Value {
        match self.root_id {
            Some(root_id) => element_to_json(root_id, self, bounds, include_details),
            None => serde_json::Value::Null,
        }
    }
}

fn element_to_json(
    id: u64,
    tree: &RetainedTree,
    bounds: &std::collections::HashMap<u64, crate::automation::ElementBounds>,
    include_details: bool,
) -> serde_json::Value {
    let Some(element) = tree.elements.get(&id) else {
        return serde_json::Value::Null;
    };

    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), serde_json::Value::String(element.element_type.clone()));
    obj.insert("id".to_string(), serde_json::json!(element.id));

    if let Some(ref test_id) = element.test_id {
        obj.insert("testId".to_string(), serde_json::Value::String(test_id.clone()));
    }

    if let Some(ref content) = element.content {
        obj.insert("text".to_string(), serde_json::Value::String(content.clone()));
    }

    if let Some(rect) = bounds.get(&id) {
        obj.insert(
            "bounds".to_string(),
            serde_json::json!({
                "x": rect.x,
                "y": rect.y,
                "width": rect.width,
                "height": rect.height,
            }),
        );
    }

    if include_details {
        if let Some(ref style) = element.style {
            // `as_ref`, not the `Arc`. Serializing the pointer needs serde's
            // `rc` feature, which we never asked for; it only compiles because
            // gpui happens to enable it, and would break when gpui stops.
            if let Ok(style_json) = serde_json::to_value(style.as_ref()) {
                if let serde_json::Value::Object(ref map) = style_json {
                    let filtered: serde_json::Map<String, serde_json::Value> =
                        map.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), v.clone())).collect();
                    if !filtered.is_empty() {
                        obj.insert("style".to_string(), serde_json::Value::Object(filtered));
                    }
                }
            }
        }

        if !element.events().is_empty() {
            let mut events: Vec<String> = element.events().iter().cloned().collect();
            events.sort();
            obj.insert("events".to_string(), serde_json::json!(events));
        }

        if !element.custom_props().is_empty() {
            let custom: serde_json::Map<String, serde_json::Value> =
                element.custom_props().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            obj.insert("customProps".to_string(), serde_json::Value::Object(custom));
        }
    }

    if !element.children.is_empty() {
        let children: Vec<serde_json::Value> = element
            .children
            .iter()
            .map(|&cid| element_to_json(cid, tree, bounds, include_details))
            .filter(|v| !v.is_null())
            .collect();
        if !children.is_empty() {
            obj.insert("children".to_string(), serde_json::Value::Array(children));
        }
    }

    serde_json::Value::Object(obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(tree: &mut RetainedTree, json: &str) {
        crate::renderer::apply_batch_to_tree(tree, json.as_bytes()).expect("valid batch");
    }

    fn tree_with_child() -> RetainedTree {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.create_element(2, "text".to_string());
        tree.create_element(3, "text".to_string());
        tree.append_child(1, 2);
        tree.append_child(2, 3);
        tree.set_text(3, "hello".to_string());
        tree
    }

    #[test]
    fn destroy_unlinks_from_parent() {
        let mut tree = tree_with_child();
        let destroyed = tree.destroy_element(2);
        assert_eq!(destroyed, vec![2, 3]);
        assert_eq!(tree.elements[&1].children, Vec::<u64>::new());
        assert!(!tree.elements.contains_key(&3));
    }

    #[test]
    fn destroy_bumps_the_parent_chain_revision() {
        let mut tree = tree_with_child();
        let before = tree.elements[&1].subtree_revision;
        tree.destroy_element(2);
        assert!(
            tree.elements[&1].subtree_revision > before,
            "destroying a child must invalidate a subtree_revision cache on the parent"
        );
    }

    #[test]
    fn destroying_the_root_clears_it() {
        let mut tree = tree_with_child();
        tree.root_id = Some(1);
        tree.destroy_element(1);
        assert_eq!(tree.root_id, None);
        assert!(tree.elements.is_empty());
    }

    /// The whole point of `search_revision`: `highlight` is a custom prop, so
    /// keying the group cache on `subtree_revision` means every find-bar
    /// keystroke re-walks and re-folds the subtree it exists to avoid.
    #[test]
    fn a_query_change_does_not_move_search_revision() {
        let mut tree = tree_with_child();
        tree.set_custom_prop(1, "highlight".to_string(), serde_json::json!({"query": "a"}));
        let search = tree.elements[&1].search_revision;
        let subtree = tree.elements[&1].subtree_revision;

        tree.set_custom_prop(1, "highlight".to_string(), serde_json::json!({"query": "ab"}));
        assert_eq!(tree.elements[&1].search_revision, search, "same text");
        assert!(tree.elements[&1].subtree_revision > subtree, "still repaints");
    }

    #[test]
    fn style_does_not_move_search_revision() {
        let mut tree = tree_with_child();
        let search = tree.elements[&1].search_revision;
        let style = tree.styles.intern(br##"{"color":"#fff"}"##).unwrap();
        tree.set_style(3, style);
        assert_eq!(tree.elements[&1].search_revision, search);
        assert!(tree.elements[&1].subtree_revision > 0);
    }

    // ── Style interning ──────────────────────────────────────────────

    #[test]
    fn equal_style_bytes_share_one_allocation() {
        let mut tree = tree_with_child();
        let first = tree.styles.intern(br#"{"color":"red"}"#).unwrap();
        let second = tree.styles.intern(br#"{"color":"red"}"#).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(tree.styles.len(), 1);

        let other = tree.styles.intern(br#"{"color":"blue"}"#).unwrap();
        assert!(!Arc::ptr_eq(&first, &other));
        assert_eq!(tree.styles.len(), 2);
    }

    /// Re-sending the same style must not repaint. `commitUpdate` resends the
    /// full style on every commit, so without this every commit would dirty
    /// every element it touched.
    #[test]
    fn resending_the_same_style_is_not_a_change() {
        let mut tree = tree_with_child();
        let style = tree.styles.intern(br#"{"color":"red"}"#).unwrap();
        tree.set_style(3, style);
        let revision = tree.elements[&3].subtree_revision;

        let same = tree.styles.intern(br#"{"color":"red"}"#).unwrap();
        tree.set_style(3, same);
        assert_eq!(tree.elements[&3].subtree_revision, revision);

        let different = tree.styles.intern(br#"{"color":"blue"}"#).unwrap();
        tree.set_style(3, different);
        assert!(tree.elements[&3].subtree_revision > revision);
    }

    /// The table must not grow without bound. A dragged element produces a
    /// distinct style every frame, so the sweep is what keeps this safe.
    #[test]
    fn sweep_releases_styles_no_element_references() {
        let mut tree = tree_with_child();
        for frame in 0..50 {
            let payload = format!(r#"{{"left":{frame}}}"#);
            let style = tree.styles.intern(payload.as_bytes()).unwrap();
            tree.set_style(3, style);
            tree.styles.sweep();
            assert_eq!(tree.styles.len(), 1, "frame {frame} leaked a style");
        }
    }

    /// A drag through the batch entry point must keep the style table bounded.
    #[test]
    fn a_drag_through_apply_batch_stays_bounded() {
        let mut tree = tree_with_child();
        for frame in 0..1_000 {
            apply(&mut tree, &format!(r#"[["setStyle",3,{{"left":{frame}}}]]"#));
            assert!(
                tree.styles.len() <= STYLE_SWEEP_FLOOR * 2,
                "frame {frame} grew the table to {}",
                tree.styles.len(),
            );
        }
    }

    /// Growth alone never fires again once the tree collapses: `swept_at` stays
    /// at the high-water mark and `count` cannot climb past it, because
    /// re-interning an existing style does not add an entry. Destroying the
    /// tree must reclaim the table anyway.
    #[test]
    fn destroying_the_tree_releases_its_styles() {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.root_id = Some(1);

        let wide = STYLE_SWEEP_FLOOR * 4;
        for index in 0..wide {
            let child = 100 + index as u64;
            tree.create_element(child, "div".to_string());
            tree.append_child(1, child);
            let style = tree.styles.intern(format!(r#"{{"left":{index}}}"#).as_bytes()).unwrap();
            tree.set_style(child, style);
        }
        assert_eq!(tree.styles.len(), wide, "every style is still live");

        tree.destroy_element(1);
        assert!(tree.elements.is_empty(), "the tree is gone");

        // Exactly what `apply_batch_to_tree` does after a batch that destroyed
        // the root. Nothing new is interned, so only the element count can
        // trigger this.
        let live_elements = tree.elements.len();
        tree.styles.maybe_sweep(live_elements);
        assert_eq!(tree.styles.len(), 0, "the destroyed tree kept its styles");
    }

    /// A style table larger than the tree must not re-sweep on every commit.
    #[test]
    fn the_shrink_sweep_does_not_thrash() {
        let mut tree = RetainedTree::new();
        for index in 0..(STYLE_SWEEP_FLOOR * 4) {
            let id = index as u64 + 1;
            tree.create_element(id, "div".to_string());
            let style = tree.styles.intern(format!(r#"{{"left":{index}}}"#).as_bytes()).unwrap();
            tree.set_style(id, style);
        }
        let live = tree.styles.len();
        let live_elements = tree.elements.len();
        tree.styles.maybe_sweep(live_elements);
        assert_eq!(tree.styles.len(), live, "a full table must survive a sweep");
    }

    #[test]
    fn sweep_keeps_a_style_two_elements_still_use() {
        let mut tree = tree_with_child();
        let style = tree.styles.intern(br#"{"color":"red"}"#).unwrap();
        tree.set_style(2, style.clone());
        tree.set_style(3, style);
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1);

        let replacement = tree.styles.intern(br#"{"color":"blue"}"#).unwrap();
        tree.set_style(3, replacement);
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 2, "element 2 still holds red");

        let replacement = tree.styles.intern(br#"{"color":"blue"}"#).unwrap();
        tree.set_style(2, replacement);
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1, "red is now unreferenced");
    }

    /// Sharing must never let one element's animation restyle another.
    #[test]
    fn a_shared_style_is_never_mutated_through() {
        let mut tree = tree_with_child();
        let style = tree.styles.intern(br#"{"color":"red"}"#).unwrap();
        tree.set_style(2, style.clone());
        tree.set_style(3, style);

        // This is what the motion path does: copy out, then mutate the copy.
        let mut animated = tree.elements[&3].style.as_deref().cloned().unwrap();
        animated.color = Some("green".to_string());

        assert_eq!(tree.elements[&2].style.as_ref().unwrap().color.as_deref(), Some("red"));
        assert_eq!(tree.elements[&3].style.as_ref().unwrap().color.as_deref(), Some("red"));
    }

    /// A nested declaration appearing changes which subtrees an ancestor skips,
    /// so the ancestor's collected groups really did change.
    #[test]
    fn a_nested_declaration_appearing_moves_the_ancestor() {
        let mut tree = tree_with_child();
        let search = tree.elements[&1].search_revision;
        tree.set_custom_prop(2, "highlight".to_string(), serde_json::json!({"query": "a"}));
        assert!(tree.elements[&1].search_revision > search);

        // Changing that nested query does not.
        let after = tree.elements[&1].search_revision;
        tree.set_custom_prop(2, "highlight".to_string(), serde_json::json!({"query": "b"}));
        assert_eq!(tree.elements[&1].search_revision, after);
    }

    #[test]
    fn text_and_structure_move_search_revision() {
        let mut tree = tree_with_child();
        let search = tree.elements[&1].search_revision;
        tree.set_text(3, "changed".to_string());
        assert!(tree.elements[&1].search_revision > search);

        let after = tree.elements[&1].search_revision;
        tree.destroy_element(3);
        assert!(tree.elements[&1].search_revision > after);
    }

    /// `focusable_ids` stands in for a scan of every element, so it has to
    /// agree with `needs_focus` after any mutation that can change the answer.
    #[test]
    fn focusable_ids_follow_what_needs_focus_reads() {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.create_element(2, "input".to_string());
        tree.create_element(3, "div".to_string());
        tree.create_element(4, "div".to_string());
        tree.append_child(1, 2);
        tree.append_child(1, 3);
        tree.append_child(1, 4);
        let focusable = |tree: &RetainedTree| tree.focusable_ids.iter().copied().collect::<Vec<_>>();
        assert_eq!(focusable(&tree), vec![2], "inputs always take focus");

        tree.set_event_listener(3, "keyDown".to_string(), true);
        tree.set_custom_prop(4, "tabIndex".to_string(), serde_json::json!(0));
        assert_eq!(focusable(&tree), vec![2, 3, 4]);

        // A click listener has nothing to do with focus.
        tree.set_event_listener(1, "click".to_string(), true);
        assert_eq!(focusable(&tree), vec![2, 3, 4]);

        // Other listeners keep an element in the set until the last one goes.
        tree.set_event_listener(3, "blur".to_string(), true);
        tree.set_event_listener(3, "keyDown".to_string(), false);
        assert_eq!(focusable(&tree), vec![2, 3, 4]);
        tree.set_event_listener(3, "blur".to_string(), false);
        assert_eq!(focusable(&tree), vec![2, 4]);

        tree.set_custom_prop(4, "tabIndex".to_string(), serde_json::Value::Null);
        assert_eq!(focusable(&tree), vec![2]);

        tree.destroy_element(1);
        assert!(tree.focusable_ids.is_empty(), "destroying a subtree forgets its ids");
    }

    /// A page with a clipping scroller, overlapping siblings, and an overlay that
    /// does not take the pointer:
    ///
    /// ```text
    /// 1 root      0,0   100x100
    ///   2 scroller  0,0   50x50   overflow: hidden
    ///     3 row       0,40  50x30   (sticks out of the scroller's bottom)
    ///   4 sibling   40,0  50x50   (paints over the scroller)
    ///   5 overlay   0,0   100x100 pointer-events: none
    ///     6 button    60,60 20x20   pointer-events: auto
    /// ```
    fn hit_test_tree() -> (RetainedTree, impl Fn(u64) -> Option<crate::automation::ElementBounds>) {
        let mut tree = RetainedTree::new();
        for id in 1..=6 {
            tree.create_element(id, "div".to_string());
        }
        tree.set_root(1);
        for (parent, child) in [(1, 2), (2, 3), (1, 4), (1, 5), (5, 6)] {
            tree.append_child(parent, child);
        }
        let styled = |overflow: Option<&str>, pointer_events: Option<&str>| {
            Arc::new(StyleDesc {
                overflow: overflow.map(str::to_string),
                pointer_events: pointer_events.map(str::to_string),
                ..Default::default()
            })
        };
        tree.set_style(2, styled(Some("hidden"), None));
        tree.set_style(5, styled(None, Some("none")));
        tree.set_style(6, styled(None, Some("auto")));
        let boxes = [
            (1, (0.0, 0.0, 100.0, 100.0)),
            (2, (0.0, 0.0, 50.0, 50.0)),
            (3, (0.0, 40.0, 50.0, 30.0)),
            (4, (40.0, 0.0, 50.0, 50.0)),
            (5, (0.0, 0.0, 100.0, 100.0)),
            (6, (60.0, 60.0, 20.0, 20.0)),
        ];
        let lookup = move |id: u64| {
            boxes.iter().find(|(candidate, _)| *candidate == id).map(|(_, (x, y, width, height))| {
                crate::automation::ElementBounds { x: *x, y: *y, width: *width, height: *height }
            })
        };
        (tree, lookup)
    }

    #[test]
    fn elements_at_lists_what_is_under_the_point_topmost_first() {
        let (tree, painted) = hit_test_tree();
        // The later sibling paints over the scroller, and both sit in the root.
        assert_eq!(tree.elements_at(45.0, 10.0, &painted), vec![4, 2, 1]);
        // A descendant is above its ancestor.
        assert_eq!(tree.elements_at(10.0, 45.0, &painted), vec![3, 2, 1]);
        assert_eq!(tree.elements_at(500.0, 500.0, &painted), Vec::<u64>::new());
    }

    #[test]
    fn elements_at_stops_at_the_clip_edge() {
        let (tree, painted) = hit_test_tree();
        // y 60 is inside the row but below the scroller that clips it.
        assert_eq!(tree.elements_at(10.0, 60.0, &painted), vec![1]);
    }

    #[test]
    fn elements_at_skips_what_does_not_take_the_pointer() {
        let (tree, painted) = hit_test_tree();
        // The overlay covers everything yet is never hit...
        assert_eq!(tree.elements_at(10.0, 90.0, &painted), vec![1]);
        // ...while its child opts back in.
        assert_eq!(tree.elements_at(70.0, 70.0, &painted), vec![6, 1]);
    }

    #[test]
    fn elements_at_never_hits_what_was_not_painted() {
        let (tree, painted) = hit_test_tree();
        let without_the_sibling = |id: u64| if id == 4 { None } else { painted(id) };
        assert_eq!(tree.elements_at(45.0, 10.0, &without_the_sibling), vec![2, 1]);
    }

    fn boxed(boxes: &[(u64, (f64, f64, f64, f64))]) -> impl Fn(u64) -> Option<crate::automation::ElementBounds> + '_ {
        move |id| {
            boxes.iter().find(|(candidate, _)| *candidate == id).map(|(_, (x, y, width, height))| {
                crate::automation::ElementBounds { x: *x, y: *y, width: *width, height: *height }
            })
        }
    }

    /// A popover lives inside a clipping scroller, early in the tree, yet gpui
    /// paints it after everything else and outside that clip.
    #[test]
    fn elements_at_puts_a_deferred_layer_above_the_tree_and_outside_its_clip() {
        let mut tree = RetainedTree::new();
        for (id, kind) in [(1, "div"), (2, "div"), (3, "anchored"), (4, "div"), (5, "div")] {
            tree.create_element(id, kind.to_string());
        }
        tree.set_root(1);
        for (parent, child) in [(1, 2), (2, 3), (3, 4), (1, 5)] {
            tree.append_child(parent, child);
        }
        tree.set_style(2, Arc::new(StyleDesc { overflow: Some("hidden".to_string()), ..Default::default() }));
        let boxes = [
            (1, (0.0, 0.0, 100.0, 100.0)),
            (2, (0.0, 0.0, 20.0, 20.0)),
            (4, (50.0, 50.0, 30.0, 30.0)),
            (5, (40.0, 40.0, 60.0, 60.0)),
        ];
        // The popover content is outside its scroller's clip and under a later
        // sibling, and is still what the point lands on.
        assert_eq!(tree.elements_at(60.0, 60.0, &boxed(&boxes)), vec![4, 5, 1]);

        // Ordinary clipping is untouched.
        tree.destroy_element(3);
        tree.create_element(6, "div".to_string());
        tree.append_child(2, 6);
        let with_clipped_child =
            [(1, (0.0, 0.0, 100.0, 100.0)), (2, (0.0, 0.0, 20.0, 20.0)), (6, (0.0, 0.0, 90.0, 90.0))];
        assert_eq!(tree.elements_at(60.0, 60.0, &boxed(&with_clipped_child)), vec![1]);
    }

    #[test]
    fn elements_at_orders_deferred_layers_by_priority() {
        let mut tree = RetainedTree::new();
        for (id, kind) in [(1, "div"), (2, "anchored"), (3, "div"), (4, "anchored"), (5, "div")] {
            tree.create_element(id, kind.to_string());
        }
        tree.set_root(1);
        for (parent, child) in [(1, 2), (2, 3), (1, 4), (4, 5)] {
            tree.append_child(parent, child);
        }
        let boxes = [(1, (0.0, 0.0, 100.0, 100.0)), (3, (0.0, 0.0, 100.0, 100.0)), (5, (0.0, 0.0, 100.0, 100.0))];
        // Equal priority: the layer later in the tree paints over the earlier.
        assert_eq!(tree.elements_at(10.0, 10.0, &boxed(&boxes)), vec![5, 3, 1]);

        tree.set_custom_prop(2, "priority".to_string(), serde_json::json!(5));
        assert_eq!(tree.elements_at(10.0, 10.0, &boxed(&boxes)), vec![3, 5, 1]);

        // Not deferred: painted in place, in tree order, under the clip.
        tree.set_custom_prop(2, "deferred".to_string(), serde_json::json!(false));
        assert_eq!(tree.elements_at(10.0, 10.0, &boxed(&boxes)), vec![5, 3, 1]);
    }

    #[test]
    fn a_custom_element_reads_its_own_auto_focus() {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.create_element(2, "terminal".to_string());
        tree.set_root(1);
        tree.append_child(1, 2);
        tree.set_custom_prop(1, "autoFocus".to_string(), serde_json::json!(true));
        tree.set_custom_prop(2, "autoFocus".to_string(), serde_json::json!(true));
        assert!(tree.elements[&1].auto_focus && tree.elements[&2].auto_focus);
        // A div's focus is the renderer's; a terminal takes its own, from its props.
        assert_eq!(tree.get_custom_prop(1, "autoFocus"), None);
        assert_eq!(tree.get_custom_prop(2, "autoFocus"), Some(&serde_json::json!(true)));
        tree.set_custom_prop(2, "autoFocus".to_string(), serde_json::json!(false));
        assert_eq!(tree.get_custom_prop(2, "autoFocus"), Some(&serde_json::json!(false)));
    }

    #[test]
    fn cold_event_and_prop_storage_is_released_when_empty() {
        let mut tree = tree_with_child();
        assert!(tree.elements[&1].extras.is_none());

        tree.set_event_listener(1, "click".to_string(), true);
        tree.set_custom_prop(1, "aria-label".to_string(), serde_json::json!("button"));
        assert!(tree.elements[&1].events().contains("click"));
        assert_eq!(tree.get_custom_prop(1, "aria-label"), Some(&serde_json::json!("button")));

        tree.set_event_listener(1, "click".to_string(), false);
        assert!(tree.elements[&1].extras.is_some());
        tree.set_custom_prop(1, "aria-label".to_string(), serde_json::Value::Null);
        assert!(tree.elements[&1].extras.is_none());
    }

    #[test]
    fn set_text_bumps_every_ancestor() {
        let mut tree = tree_with_child();
        let before = tree.elements[&1].subtree_revision;
        tree.set_text(3, "changed".to_string());
        assert!(tree.elements[&1].subtree_revision > before);
        // An unchanged value must not invalidate anything.
        let after = tree.elements[&1].subtree_revision;
        tree.set_text(3, "changed".to_string());
        assert_eq!(tree.elements[&1].subtree_revision, after);
    }
}
