//! Cached subtrees: what an element with `cache` drew is drawn again as it was until something
//! under it changes.
//!
//! Every frame gpui rebuilds the root view and lays the whole element tree out again, which for
//! a retained React tree is most of a frame's cost (taffy measures each flex container several
//! times, from cold, every frame). And a frame is drawn for anything: a terminal's output, a
//! spinner's next sample, a hover. A `cache` element is drawn by a view of its own,
//! [`SubtreeView`], embedded with `Entity::cached`, so gpui reuses its last layout, prepaint and
//! paint while that view is clean.
//!
//! What dirties one:
//! - a change in the retained tree under it: every mutation bumps the `subtree_revision` of the
//!   element and its ancestors, and the root build compares it with the one last built;
//! - what it inherits (selection colour, `userSelect`, a `highlight` above it) changing;
//! - a running motion under it, the keyboard-focus path moving, or any `highlight` in the tree
//!   (search counts matches across the window in paint order);
//! - gpui's own: a view under it notifying (a terminal, an input), a hover or scroll in it, its
//!   bounds, clip or text style changing, and `window.refresh()`.
//!
//! What a reused paint does not do is run the paint closures that fill the per-frame registries
//! (selectable text, start regions, painted text, automation bounds, what is painted over a native
//! page). [`BoundaryElement`] keeps what its subtree registered and registers it again when the
//! subtree is not painted.
//!
//! A compartment (`<div compartment>`, what the app puts around every plugin's component) is the same
//! boundary around its one element, sized as that element would be: by its contents too. gpui
//! lays a cached view out from its style alone, so a compartment measures. On a frame its subtree
//! changed its view is drawn uncached, laid out with everything else, and the size it comes to is
//! kept (and what it drew recorded, `Entity::recorded`); on the frames after, it is a cached view
//! whose open sides are a measured leaf answering with that size, reusing that draw. When the layout asks with constraints it did not ask with before (its box was
//! resized), it is built in place again on the next frame.
//!
//! A compartment also draws on a budget. Each time its subtree renders, what that took is kept;
//! one that takes long (`SLOW`) renders at most once in `SHARE`'s inverse of its cost, and in
//! between it is drawn as it last was: what changes in it (its tree, a hover, a scroll) shows
//! when its turn comes, and the rest of the window keeps every frame. However much a plugin
//! draws, the window's frames are not all spent drawing it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, AppContext as _, AvailableSpace, Bounds, Element, ElementId, Entity, EntityId, GlobalElementId,
    InspectorElementId, IntoElement, LayoutId, Pixels, Render, Size, WeakEntity, Window,
};

use crate::renderer::{GpuixView, Inherited};

/// One `cache` element's state, shared by its view, its element and the root build.
pub(crate) struct Shared {
    /// The retained element whose subtree this is.
    pub(crate) id: u64,
    /// Set when the view renders; the next paint takes it to know whether it painted.
    rendered: Cell<bool>,
    /// The element's `subtree_revision` when its subtree was last built.
    pub(crate) built_revision: Cell<u64>,
    /// What the element inherits, as the build that reached it last saw it.
    pub(crate) inherited: RefCell<Option<Inherited>>,
    /// Its view's id, which the occluders its deferred layers paint are recorded by.
    view: Cell<Option<EntityId>>,
    /// What its last paint registered.
    log: RefCell<(
        crate::text::paint::PaintLog,
        Vec<(u64, crate::automation::ElementBounds)>,
        Vec<crate::occlusion::Painted>,
    )>,
    /// A compartment's: the size it came to when it was last built in place.
    measured: Cell<Option<Size<Pixels>>>,
    /// The constraints the layout asked its size with, on the first frame after it was measured.
    constraints: RefCell<Vec<(Size<Option<Pixels>>, Size<AvailableSpace>)>>,
    /// Whether this frame's questions are the ones `constraints` is made of.
    recording: Cell<bool>,
    /// The layout asked with other constraints: it is built in place again next frame.
    remeasure: Cell<bool>,
    /// A compartment's budget: what its subtree's last render took, and when it ended.
    cost: Cell<Duration>,
    last_render: Cell<Option<Instant>>,
    /// A render it was not allowed is due: the view is notified then.
    wake_pending: Cell<bool>,
    /// The render in progress, as `prepaint` saw it start.
    render_started: Cell<Option<Instant>>,
    /// Whether this boundary draws on a budget (a compartment).
    pub(crate) budgeted: Cell<bool>,
    /// A compartment built in place this frame: its view builds its element as it is, not filling
    /// a box (`GpuixView::build_boundary`).
    pub(crate) in_place: Cell<bool>,
    /// A compartment's name (`compartment="owner/name"`), and what it cost since it was last read.
    pub(crate) name: RefCell<Option<String>>,
    tally: Cell<Tally>,
}

/// What a compartment's renders cost over a while (`Shared::take_tally`).
#[derive(Clone, Copy, Default)]
pub(crate) struct Tally {
    pub renders: u32,
    /// Renders its budget put off.
    pub deferred: u32,
    pub total: Duration,
    pub max: Duration,
}

/// A subtree whose render takes this long is drawn on a budget.
const SLOW: Duration = Duration::from_millis(6);
/// The share of the time a slow subtree may spend rendering.
const SHARE: f64 = 0.25;

impl Shared {
    /// What it cost since the last time, and a fresh count from now.
    pub(crate) fn take_tally(&self) -> Tally {
        self.tally.take()
    }

    /// The budget put a render off.
    pub(crate) fn count_deferred(&self) {
        let mut tally = self.tally.get();
        tally.deferred += 1;
        self.tally.set(tally);
    }

    /// Whether it is drawn on a budget now: slow, and its last render recent.
    pub(crate) fn held(&self) -> bool {
        self.next_turn(Instant::now()).is_some()
    }

    /// What its last render took.
    pub(crate) fn cost(&self) -> Duration {
        self.cost.get()
    }

    /// When a slow subtree may render next; `None` when it may now.
    pub(crate) fn next_turn(&self, now: Instant) -> Option<Instant> {
        let cost = self.cost.get();
        if !self.budgeted.get() || cost < SLOW {
            return None;
        }
        let at = self.last_render.get()? + cost.div_f64(SHARE);
        (at > now).then_some(at)
    }
}

/// A `cache` element: its view, and what the root keeps about it.
pub(crate) struct Boundary {
    pub(crate) view: Entity<SubtreeView>,
    pub(crate) shared: Rc<Shared>,
}

impl Boundary {
    pub(crate) fn new(id: u64, owner: WeakEntity<GpuixView>, cx: &mut App) -> Self {
        let shared = Rc::new(Shared {
            id,
            rendered: Cell::new(false),
            built_revision: Cell::new(0),
            inherited: RefCell::new(None),
            view: Cell::new(None),
            log: RefCell::new(Default::default()),
            measured: Cell::new(None),
            constraints: RefCell::new(Vec::new()),
            recording: Cell::new(false),
            remeasure: Cell::new(false),
            cost: Cell::new(Duration::ZERO),
            last_render: Cell::new(None),
            wake_pending: Cell::new(false),
            render_started: Cell::new(None),
            budgeted: Cell::new(false),
            in_place: Cell::new(false),
            name: RefCell::new(None),
            tally: Cell::new(Tally::default()),
        });
        let view = cx.new(|_| SubtreeView { owner, shared: shared.clone() });
        shared.view.set(Some(view.entity_id()));
        Self { view, shared }
    }

    /// Draw this subtree again in the frame being drawn.
    pub(crate) fn invalidate(&self, window: &mut Window) {
        window.invalidate_cached_view(self.view.entity_id());
    }
}

impl Shared {
    /// Whether a compartment must be built in place this frame, to learn its size.
    pub(crate) fn needs_measure(&self) -> bool {
        self.measured.get().is_none() || self.remeasure.get()
    }
}

/// What sizes a clean compartment's cached view: the size it was measured at, for the sides the
/// layout does not decide. A question with constraints it was not asked with the first time
/// means its box changed, and it is measured again.
pub(crate) fn measure(shared: Rc<Shared>) -> gpui::CachedMeasure {
    Rc::new(move |known: Size<Option<Pixels>>, available: Size<AvailableSpace>, _: &mut Window, _: &mut App| {
        let size = shared.measured.get().unwrap_or_default();
        let asked = (known, available);
        let mut seen = shared.constraints.borrow_mut();
        if !seen.contains(&asked) {
            if shared.recording.get() {
                seen.push(asked);
            } else {
                shared.remeasure.set(true);
            }
        }
        gpui::size(known.width.unwrap_or(size.width), known.height.unwrap_or(size.height))
    })
}

/// Notifies `view` at `at`: its subtree's turn to render, put off by its budget. One wake at a
/// time; the render it brings asks again if it is still early.
pub(crate) fn wake_at(shared: &Rc<Shared>, view: &Entity<SubtreeView>, at: Instant, window: &mut Window, cx: &mut App) {
    if shared.wake_pending.replace(true) {
        return;
    }
    let delay = at.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
    let (shared, view) = (shared.clone(), view.downgrade());
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(delay).await;
            shared.wake_pending.set(false);
            cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()).ok()).ok();
        })
        .detach();
}

/// Whether two inherited states draw a subtree the same.
pub(crate) fn same_inherited(a: &Inherited, b: &Inherited) -> bool {
    a.selectable == b.selectable
        && a.selection_scope == b.selection_scope
        && a.pointer_events_none == b.pointer_events_none
        && a.selection_wash == b.selection_wash
        && a.color == b.color
        && match (&a.highlight, &b.highlight) {
            (None, None) => true,
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            _ => false,
        }
}

/// Draws one `cache` element's subtree, through the root view's build.
pub(crate) struct SubtreeView {
    owner: WeakEntity<GpuixView>,
    shared: Rc<Shared>,
}

impl Render for SubtreeView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.shared.rendered.set(true);
        let shared = self.shared.clone();
        // The root view rendered already this frame (a cached view renders while it is
        // prepainted), so it is not leased.
        self.owner
            .update(cx, |view, cx| view.build_boundary(&shared, window, cx))
            .unwrap_or_else(|_| gpui::Empty.into_any_element())
    }
}

/// The cached view, and the registries' share of its subtree.
pub(crate) struct BoundaryElement {
    child: AnyElement,
    shared: Rc<Shared>,
    /// Its view, for a budgeted render that is put off.
    view: Option<Entity<SubtreeView>>,
    /// A compartment built in place: `child` is its subtree, not its view, and its size is kept.
    measuring: bool,
}

impl BoundaryElement {
    pub(crate) fn new(child: AnyElement, shared: Rc<Shared>) -> Self {
        Self { child, shared, view: None, measuring: false }
    }

    /// A compartment's cached view: drawn on a budget.
    pub(crate) fn budgeted(child: AnyElement, shared: Rc<Shared>, view: Entity<SubtreeView>) -> Self {
        shared.budgeted.set(true);
        Self { child, shared, view: Some(view), measuring: false }
    }

    /// A compartment's view, drawn in place this frame (`Entity::recorded`): what it is laid out
    /// at is its size, and what it draws is reused the frames after.
    pub(crate) fn measuring(child: AnyElement, shared: Rc<Shared>) -> Self {
        shared.in_place.set(true);
        Self { child, shared, view: None, measuring: true }
    }
}

impl IntoElement for BoundaryElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for BoundaryElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        if self.measuring {
            // Its view renders now, laid out with everything else.
            self.shared.render_started.set(Some(Instant::now()));
        }
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        if self.measuring {
            let shared = &self.shared;
            shared.measured.set(Some(bounds.size));
            shared.constraints.borrow_mut().clear();
            shared.recording.set(true);
            shared.remeasure.set(false);
            self.child.prepaint(window, cx);
            return;
        }
        self.shared.in_place.set(false);
        if let Some(view) = &self.view {
            let now = Instant::now();
            if let Some(at) = self.shared.next_turn(now) {
                if window.defer_cached_view(view.entity_id()) {
                    self.shared.count_deferred();
                    wake_at(&self.shared, view, at, window, cx);
                }
            }
            self.shared.render_started.set(Some(now));
        }
        self.child.prepaint(window, cx);
        // The layout is done: what it asked this frame is all a first frame asks.
        self.shared.recording.set(false);
        if self.shared.remeasure.get() {
            window.request_animation_frame();
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text = crate::text::paint::paint_mark();
        let boxes = crate::automation::bounds_mark();
        let covers = crate::occlusion::mark();
        self.child.paint(window, cx);
        let started = self.shared.render_started.take();
        if let (true, Some(started)) = (self.shared.rendered.get(), started) {
            // Its render, layout, prepaint and paint: what drawing it again costs. (Built in
            // place, its share of the window's layout is not in it.)
            let now = Instant::now();
            let cost = now.saturating_duration_since(started);
            self.shared.cost.set(cost);
            self.shared.last_render.set(Some(now));
            let mut tally = self.shared.tally.get();
            tally.renders += 1;
            tally.total += cost;
            tally.max = tally.max.max(cost);
            self.shared.tally.set(tally);
        }
        let fresh = self.shared.rendered.replace(false);
        if fresh {
            *self.shared.log.borrow_mut() = (
                crate::text::paint::paint_since(text),
                crate::automation::bounds_since(boxes),
                crate::occlusion::since(covers),
            );
        } else {
            let log = self.shared.log.borrow();
            crate::text::paint::paint_replay(&log.0);
            crate::automation::bounds_replay(&log.1);
            crate::occlusion::replay(&log.2, window, cx);
        }
        if let Some(view) = self.shared.view.get() {
            crate::occlusion::layer(view, fresh, window, cx);
        }
    }
}
