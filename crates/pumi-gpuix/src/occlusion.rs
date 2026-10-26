//! What a frame paints over a native page: its *occluders*.
//!
//! A web page is a native child view, above everything the window paints, so what the app draws
//! over it (a menu, a dialog, a tooltip, a drag's ghost, an opaque later sibling) would be hidden
//! beneath it. Each frame records, in paint order, every box that paints something hiding what is
//! under it: a fill, a shadow, a border's strips, an `<anchored>` layer's card, a tooltip. A page
//! marks where it was painted ([`page`]); once the window's frame is drawn it is told what was
//! painted after it over its box, and cuts itself there.
//!
//! Recorded in paint, where `on_painted` fires before a box's children, so a page's ancestors come
//! before it and what is drawn over it after. A cached subtree reused as it was paints nothing:
//! `crate::boundary` keeps its share and records it again ([`since`], [`replay`]), and what its
//! deferred layers painted, which paint at the end of the frame outside its own paint, is kept by
//! view and added on a frame it is reused ([`layer`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{canvas, px, App, Bounds, Corners, EntityId, IntoElement, Pixels, Styled, Window, WindowId};

use crate::style::StyleDesc;

/// A box the app painted over a page, in the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occluder {
    pub bounds: Bounds<Pixels>,
    pub corners: Corners<Pixels>,
}

/// What a box paints that hides what is under it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Cover {
    /// All of it: a fill or a shadow.
    Box(Corners<Pixels>),
    /// Only its border, `[top, right, bottom, left]`: a ring around a page cuts no hole in it.
    Border([Pixels; 4]),
}

type Settle = Rc<dyn Fn(&[Occluder], &mut App)>;

/// One thing a frame painted, in the order it was.
#[derive(Clone)]
pub(crate) enum Painted {
    /// With the view that painted it, when it was painted after the window's tree (a deferred layer).
    Cover(Occluder, Option<EntityId>),
    Page(Bounds<Pixels>, Settle),
    /// A cached view's subtree: painted afresh, or reused.
    Layer(EntityId, bool),
}

#[derive(Default)]
struct Frame {
    window: Option<WindowId>,
    generation: u64,
    painted: Vec<Painted>,
    /// The window's tree is painted: what comes now is its deferred layers, the tooltip, a drag.
    deferred: bool,
    /// What the deferred layers of the cached views reused this frame painted when they last did.
    late: Vec<Occluder>,
    /// Each cached view's deferred layers' occluders, as last painted, and its window.
    kept: HashMap<EntityId, (WindowId, Vec<Occluder>)>,
    settling: bool,
}

thread_local! {
    static FRAME: RefCell<Frame> = RefCell::new(Frame::default());
}

impl Frame {
    /// A new frame of `window`: what the last one's fresh cached views' deferred layers painted
    /// is kept, and what its views no longer painted is dropped.
    fn start(&mut self, window: WindowId) {
        if let Some(last) = self.window {
            let mut deferred: HashMap<EntityId, Vec<Occluder>> = HashMap::new();
            for painted in &self.painted {
                if let Painted::Cover(occluder, Some(view)) = painted {
                    deferred.entry(*view).or_default().push(*occluder);
                }
            }
            let mut seen = Vec::new();
            for painted in &self.painted {
                if let Painted::Layer(view, fresh) = painted {
                    seen.push(*view);
                    if *fresh {
                        match deferred.remove(view) {
                            Some(covers) => drop(self.kept.insert(*view, (last, covers))),
                            None => drop(self.kept.remove(view)),
                        }
                    }
                }
            }
            self.kept.retain(|view, (window, _)| *window != last || seen.contains(view));
        }
        self.window = Some(window);
        self.generation += 1;
        self.painted.clear();
        self.late.clear();
        self.deferred = false;
        self.settling = false;
    }

    fn push(&mut self, painted: Painted, window: &mut Window, cx: &mut App) {
        match &painted {
            Painted::Page(..) if !std::mem::replace(&mut self.settling, true) => {
                let generation = self.generation;
                window.defer(cx, move |_, cx| settle(generation, cx));
            }
            Painted::Layer(view, false) => {
                if let Some((_, covers)) = self.kept.get(view) {
                    self.late.extend_from_slice(covers);
                }
            }
            _ => {}
        }
        self.painted.push(painted);
    }
}

/// Tells each page of the frame `generation` what was painted after it over its box.
fn settle(generation: u64, cx: &mut App) {
    let pages: Vec<(Settle, Vec<Occluder>)> = FRAME.with(|frame| {
        let frame = frame.borrow();
        if frame.generation != generation {
            return Vec::new();
        }
        let painted = &frame.painted;
        (0..painted.len())
            .filter_map(|i| {
                let Painted::Page(bounds, settle) = &painted[i] else { return None };
                let after = painted[i + 1..].iter().filter_map(|p| match p {
                    Painted::Cover(occluder, _) => Some(*occluder),
                    _ => None,
                });
                let over = after.chain(frame.late.iter().copied()).filter(|o| o.bounds.intersects(bounds)).collect();
                Some((settle.clone(), over))
            })
            .collect()
    });
    for (settle, over) in pages {
        settle(&over, cx);
    }
}

/// Marks a native page painted at `bounds` here, in paint order. Once the window's frame is drawn,
/// `settle` gets what was painted after it over `bounds`.
///
/// After the frame, not in a deferred layer of the highest priority: GPUI paints the tooltip, a
/// drag and a prompt after every deferred layer. `Window::defer` runs at the end of the update the
/// frame is drawn in, before anything else draws.
pub fn page(
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
    settle: impl Fn(&[Occluder], &mut App) + 'static,
) {
    FRAME.with(|frame| frame.borrow_mut().push(Painted::Page(bounds, Rc::new(settle)), window, cx));
}

/// What a box styled `style` paints that hides what is under it, if anything.
pub(crate) fn cover(style: &StyleDesc) -> Option<Cover> {
    if style.opacity.is_some_and(|opacity| opacity <= 0.0) {
        return None;
    }
    let fill = style.has_background() && style.resolved_background().is_none_or(|fill| !fill.is_transparent());
    let shadow = style.box_shadow.as_ref().is_some_and(|shadows| !shadows.as_slice().is_empty());
    if fill || shadow {
        return Some(Cover::Box(corners(style)));
    }
    let seen = |color: &Option<String>| {
        color.as_deref().and_then(crate::color::parse_color_rgba).is_some_and(|color| color.a > 0.0)
    };
    let colored = seen(&style.border_color)
        || [&style.border_top_color, &style.border_right_color, &style.border_bottom_color, &style.border_left_color]
            .into_iter()
            .any(seen);
    let widths = style.border_widths();
    (colored && widths.iter().any(|width| *width > 0.0)).then(|| Cover::Border(widths.map(|w| px(w as f32))))
}

/// `style`'s corner radii.
pub(crate) fn corners(style: &StyleDesc) -> Corners<Pixels> {
    let [top_left, top_right, bottom_right, bottom_left] = style.corner_radii().map(|r| px(r as f32));
    Corners { top_left, top_right, bottom_right, bottom_left }
}

/// Records what a box painted at `bounds` covers, as far as what clips it lets it show.
pub(crate) fn record(cover: Cover, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let mask = window.content_mask().bounds;
    let boxes: [(Bounds<Pixels>, Corners<Pixels>); 4] = match cover {
        Cover::Box(corners) => [(bounds, corners), Default::default(), Default::default(), Default::default()],
        Cover::Border([top, right, bottom, left]) => {
            let (o, s) = (bounds.origin, bounds.size);
            let strip =
                |x, y, width, height| (Bounds::new(gpui::point(x, y), gpui::size(width, height)), Corners::default());
            [
                strip(o.x, o.y, s.width, top),
                strip(o.x + s.width - right, o.y, right, s.height),
                strip(o.x, o.y + s.height - bottom, s.width, bottom),
                strip(o.x, o.y, left, s.height),
            ]
        }
    };
    FRAME.with(|frame| {
        let mut frame = frame.borrow_mut();
        let view = frame.deferred.then(|| window.current_view());
        for (bounds, corners) in boxes {
            let bounds = bounds.intersect(&mask);
            if !bounds.is_empty() {
                frame.push(Painted::Cover(Occluder { bounds, corners }, view), window, cx);
            }
        }
    });
}

/// Where this frame's record stands now.
pub(crate) fn mark() -> usize {
    FRAME.with(|frame| frame.borrow().painted.len())
}

/// What was painted since `mark`.
pub(crate) fn since(mark: usize) -> Vec<Painted> {
    FRAME.with(|frame| frame.borrow().painted.get(mark..).map(<[_]>::to_vec).unwrap_or_default())
}

/// Records `painted` again: a reused subtree's share, its pages and cached views too.
pub(crate) fn replay(painted: &[Painted], window: &mut Window, cx: &mut App) {
    FRAME.with(|frame| {
        let mut frame = frame.borrow_mut();
        for painted in painted {
            let painted = match painted {
                Painted::Layer(view, _) => Painted::Layer(*view, false),
                other => other.clone(),
            };
            frame.push(painted, window, cx);
        }
    });
}

/// A cached view's subtree was painted here, `fresh` or reused.
pub(crate) fn layer(view: EntityId, fresh: bool, window: &mut Window, cx: &mut App) {
    FRAME.with(|frame| frame.borrow_mut().push(Painted::Layer(view, fresh), window, cx));
}

/// Zero-size canvas that starts a frame's record. Keep it ahead of the app subtree under the root.
pub(crate) fn frame_start() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |_, _, window, _| {
            let id = window.window_handle().window_id();
            FRAME.with(|frame| frame.borrow_mut().start(id));
        },
    )
    .absolute()
    .w(px(0.0))
    .h(px(0.0))
}

/// Zero-size canvas that ends the window's tree: what paints after it is deferred. Keep it after
/// the app subtree under the root.
pub(crate) fn tree_end() -> impl IntoElement {
    canvas(|_, _, _| (), |_, _, _, _| FRAME.with(|frame| frame.borrow_mut().deferred = true))
        .absolute()
        .w(px(0.0))
        .h(px(0.0))
}
