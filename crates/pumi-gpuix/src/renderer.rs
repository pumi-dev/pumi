//! GPUIX retained renderer for napi desktop hosts and GPUI's browser platform.
//!
//! Mutation-based API: React's reconciler sends individual mutations
//! (createElement, appendChild, setStyle, etc.) instead of a full JSON tree.
//! Rust maintains a RetainedTree and rebuilds GPUI elements from it each frame.
//!
//! Desktop lifecycle:
//!   const renderer = new GpuixRenderer(eventCallback)
//!   renderer.init({ title: 'My App', width: 800, height: 600 })
//!   renderer.applyBatch(json)             // one atomic React commit
//!   startFrameLoop(renderer)             // macOS wakes on vsync; Win/Linux polls UI thread
use futures::channel::oneshot;
use gpui::AppContext as _;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::JsDeferred;
use napi_derive::napi;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::accessibility::{apply_a11y_click, apply_accessibility};
use crate::custom_elements::{CustomElementRegistry, CustomRenderContext};
use crate::element_tree::EventPayload;
use crate::retained_tree::{RetainedTree, StyleTable};
use crate::style::StyleDesc;
use crate::text::{selectable_text, selection_frame_reset, SharedSelection};
use crate::theme::Theme;

/// The Window menu items act on the focused window, and the root element is the
/// only place in GPUIX that has one. `crate::app_menu` owns everything else.
fn with_window_menu_actions(root: gpui::Div) -> gpui::Div {
    use crate::app_menu::{CloseWindow, MinimizeWindow, ZoomWindow};
    use gpui::prelude::*;

    root.on_action(|_: &MinimizeWindow, window, _cx| window.minimize_window())
        .on_action(|_: &ZoomWindow, window, _cx| window.zoom_window())
        .on_action(|_: &CloseWindow, window, _cx| window.remove_window())
}

/// Parse a CSS font-weight value (string or number) into a GPUI FontWeight.
/// Accepts named keywords ("bold", "semibold"), numeric strings ("700"),
/// and raw numbers (700). Falls back to 400 (normal) for unrecognized values.
pub(crate) fn parse_font_weight(value: &crate::style::FontWeightValue) -> gpui::FontWeight {
    match value {
        crate::style::FontWeightValue::Num(n) => gpui::FontWeight((*n as f32).clamp(1.0, 1000.0)),
        crate::style::FontWeightValue::Str(s) => {
            let lower = s.trim().to_ascii_lowercase();
            match lower.as_str() {
                "100" | "thin" => gpui::FontWeight(100.0),
                "200" | "extralight" | "extra-light" => gpui::FontWeight(200.0),
                "300" | "light" => gpui::FontWeight(300.0),
                "400" | "normal" => gpui::FontWeight(400.0),
                "500" | "medium" => gpui::FontWeight(500.0),
                "600" | "semibold" | "semi-bold" => gpui::FontWeight(600.0),
                "700" | "bold" => gpui::FontWeight(700.0),
                "800" | "extrabold" | "extra-bold" => gpui::FontWeight(800.0),
                "900" | "black" => gpui::FontWeight(900.0),
                _ => lower
                    .parse::<f32>()
                    .map(|n| gpui::FontWeight(n.clamp(1.0, 1000.0)))
                    .unwrap_or(gpui::FontWeight(400.0)),
            }
        }
    }
}

/// Abstracted event callback shared by desktop, browser, and test renderers.
pub(crate) type EventCallback = Arc<dyn Fn(EventPayload) + Send + Sync>;

fn raw_dimension_u32(value: f64, label: &str) -> std::result::Result<u32, String> {
    if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 || value > u32::MAX as f64 {
        return Err(format!("Image {label} must be a positive integer, got {value}"));
    }
    Ok(value as u32)
}

pub(crate) fn dimension_u32(value: f64, label: &str) -> Result<u32> {
    raw_dimension_u32(value, label).map_err(Error::from_reason)
}

fn raw_element_id(id: f64) -> std::result::Result<u64, String> {
    if !id.is_finite() || id < 0.0 || id.fract() != 0.0 || id > 9_007_199_254_740_991.0 {
        return Err(format!("Invalid element id: {id}"));
    }
    Ok(id as u64)
}

pub(crate) fn to_element_id(id: f64) -> Result<u64> {
    raw_element_id(id).map_err(Error::from_reason)
}

thread_local! {
    /// Shared scroll handles — GpuixView writes here during render(),
    /// platform-local handlers read from here for programmatic scroll control.
    /// ScrollHandle is Rc<RefCell<...>> so its methods (set_offset, offset,
    /// scroll_to_item) work without an App context.
    ///
    /// NOTE: This is a singleton — if multiple renderers/windows coexist,
    /// the last one to render wins. TODO: scope by renderer/window ID.
    static SCROLL_HANDLES: RefCell<HashMap<u64, gpui::ScrollHandle>> = RefCell::new(HashMap::new());
    static VIRTUAL_LIST_STATES: RefCell<HashMap<u64, gpui::ListState>> = RefCell::new(HashMap::new());
    /// Virtual-list scrolls queued for the next `GpuixView::render`, applied
    /// AFTER `VirtualListEntry::sync` splices that frame's child changes.
    ///
    /// Never applied eagerly: JS computes row indices against the child list it
    /// just committed, but that commit only reaches `gpui::ListState` when the
    /// next render splices it in. An eager `scroll_to` would be shifted a
    /// second time by `splice_focusable` and land on the wrong row.
    static PENDING_VIRTUAL_LIST_SCROLLS: RefCell<HashMap<u64, gpui::ListOffset>> =
        RefCell::new(HashMap::new());
}

const SELECTION_SCROLL_TICK_MS: u64 = 24;
const SELECTION_SCROLL_EDGE_PX: f32 = 36.0;
const SELECTION_SCROLL_MAX_STEP_PX: f32 = 24.0;

/// Signed list scroll step for a pointer near a viewport edge.
fn selection_scroll_step(bounds: gpui::Bounds<gpui::Pixels>, position: gpui::Point<gpui::Pixels>) -> f32 {
    let height = f32::from(bounds.size.height);
    if height <= 0.0 {
        return 0.0;
    }
    let edge = SELECTION_SCROLL_EDGE_PX.min(height / 6.0);
    if edge <= 0.0 {
        return 0.0;
    }
    let y = f32::from(position.y);
    let top = f32::from(bounds.top());
    let bottom = f32::from(bounds.bottom());
    let scaled = |penetration: f32| {
        let progress = (penetration / edge).clamp(0.0, 1.0);
        SELECTION_SCROLL_MAX_STEP_PX * progress * progress
    };
    if y < top + edge {
        -scaled(top + edge - y)
    } else if y > bottom - edge {
        scaled(y - (bottom - edge))
    } else {
        0.0
    }
}

/// Queue a virtual-list scroll for the next render. `offset_in_item` may be
/// negative: gpui then anchors the viewport top above the item, which is what
/// keeps a row pixel-stable while unmeasured rows are spliced in above it.
pub(crate) fn queue_virtual_list_scroll(id: u64, index: usize, offset_in_item: f32) {
    PENDING_VIRTUAL_LIST_SCROLLS.with(|cell| {
        cell.borrow_mut().insert(id, gpui::ListOffset { item_ix: index, offset_in_item: gpui::px(offset_in_item) });
    });
}

fn parse_debug_frame_overlay_mode_str(mode: &str) -> std::result::Result<gpui::DebugFrameOverlayMode, String> {
    match mode {
        "hidden" => Ok(gpui::DebugFrameOverlayMode::Hidden),
        "minimal" => Ok(gpui::DebugFrameOverlayMode::Minimal),
        "full" => Ok(gpui::DebugFrameOverlayMode::Full),
        other => Err(format!("Unknown debug frame overlay mode {other:?}. Use hidden, minimal, or full.")),
    }
}

#[cfg(test)]
mod selection_scroll_tests {
    use super::*;

    #[test]
    fn selection_scroll_ramps_at_viewport_edges() {
        let bounds = gpui::Bounds::new(
            gpui::point(gpui::px(10.0), gpui::px(20.0)),
            gpui::size(gpui::px(300.0), gpui::px(200.0)),
        );
        assert_eq!(selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(120.0))), 0.0);
        assert!(selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(20.0))) < 0.0);
        assert!(selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(220.0))) > 0.0);
        assert!(
            selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(220.0)))
                > selection_scroll_step(bounds, gpui::point(gpui::px(20.0), gpui::px(200.0)))
        );
    }
}

pub(crate) fn parse_debug_frame_overlay_mode(mode: &str) -> Result<gpui::DebugFrameOverlayMode> {
    parse_debug_frame_overlay_mode_str(mode).map_err(Error::from_reason)
}

pub(crate) fn debug_frame_overlay_mode_name(mode: gpui::DebugFrameOverlayMode) -> &'static str {
    match mode {
        gpui::DebugFrameOverlayMode::Hidden => "hidden",
        gpui::DebugFrameOverlayMode::Minimal => "minimal",
        gpui::DebugFrameOverlayMode::Full => "full",
    }
}

pub(crate) fn debug_frame_overlay_stats_js(stats: gpui::DebugFrameOverlayStats) -> DebugFrameOverlayStats {
    DebugFrameOverlayStats {
        current_ms: stats.current_ms.map(|ms| ms as f64),
        p90_ms: stats.p90_ms.map(|ms| ms as f64),
        p99_ms: stats.p99_ms.map(|ms| ms as f64),
        max_ms: stats.max_ms.map(|ms| ms as f64),
        frames: stats.frames as f64,
        samples: stats.samples as f64,
    }
}

/// One window and its retained tree. Every renderer opens its window in the process's one app
/// (`crate::app`).
#[napi]
pub struct GpuixRenderer {
    event_callback: Mutex<Option<Arc<ThreadsafeFunction<EventPayload>>>>,
    tree: Arc<Mutex<RetainedTree>>,
    window: Mutex<Option<gpui::WindowHandle<GpuixView>>>,
    /// Shared with GpuixView so napi methods can read the live selection
    /// without an App context. Paint and napi calls can use different threads.
    selection: SharedSelection,
    /// Closing the window hides the app instead (`setHideOnClose`): the app's last window.
    hide_on_close: Arc<std::sync::atomic::AtomicBool>,
}

impl GpuixRenderer {
    /// The window's handle, once `init` opened it.
    pub fn handle(&self) -> Option<gpui::WindowHandle<GpuixView>> {
        *self.window.lock().unwrap()
    }

    fn update_window<R>(
        &self,
        update: impl FnOnce(&mut GpuixView, &mut gpui::Window, &mut gpui::Context<GpuixView>) -> R,
    ) -> Result<R> {
        let window = self.handle().ok_or_else(|| Error::from_reason("The window is not open"))?;
        crate::app::update(|cx| window.update(cx, update).map_err(|error| Error::from_reason(error.to_string())))?
    }

    // Input handlers can update GpuixView, so dispatch without leasing the root view.
    fn update_window_without_view<R>(&self, update: impl FnOnce(&mut gpui::Window, &mut gpui::App) -> R) -> Result<R> {
        let window = self.handle().ok_or_else(|| Error::from_reason("The window is not open"))?;
        crate::app::update(|cx| {
            gpui::AnyWindowHandle::from(window)
                .update(cx, move |_view, window, cx| update(window, cx))
                .map_err(|error| Error::from_reason(error.to_string()))
        })?
    }

    fn invalidate_window(&self) -> Result<()> {
        self.update_window(|_view, window, cx| {
            cx.notify();
            window.refresh();
        })
    }
}

#[napi]
impl GpuixRenderer {
    fn event_callback_for_view(&self) -> Option<EventCallback> {
        self.event_callback.lock().unwrap().clone().map(|tsf| {
            Arc::new(move |payload: EventPayload| {
                tsf.call(Ok(payload), ThreadsafeFunctionCallMode::NonBlocking);
            }) as EventCallback
        })
    }

    fn begin_path_prompt(&self, options: gpui::PathPromptOptions, deferred: PathPromptDeferred) {
        {
            let prompt = crate::app::update(|cx| (cx.background_executor().clone(), cx.prompt_for_paths(options)));
            match prompt {
                Ok((background_executor, receiver)) => settle_path_prompt(background_executor, receiver, deferred),
                Err(error) => deferred.reject(error),
            }
        }
    }

    fn automation_bounds(&self) -> Result<HashMap<u64, crate::automation::ElementBounds>> {
        Ok(crate::automation::all_bounds())
    }

    fn elements_at(&self, x: f64, y: f64) -> Result<Vec<u64>> {
        {
            let tree = self.tree.lock().unwrap();
            Ok(tree.elements_at(x, y, &crate::automation::get_bounds))
        }
    }

    fn element_bounds(&self, id: u64) -> Result<Option<crate::automation::ElementBounds>> {
        Ok(crate::automation::get_bounds(id))
    }

    fn request_invalidate(&self) -> Result<()> {
        self.invalidate_window()
    }

    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        let _ = env_logger::try_init();
        Self {
            event_callback: Mutex::new(event_callback.map(Arc::new)),
            tree: Arc::new(Mutex::new(RetainedTree::new())),
            window: Mutex::new(None),
            selection: SharedSelection::default(),
            hide_on_close: Arc::default(),
        }
    }

    /// Opens this renderer's window, starting the app first if no window has.
    #[napi]
    pub fn init(&self, options: Option<WindowOptions>) -> Result<()> {
        let options = options.unwrap_or_default();
        if self.handle().is_some() {
            return Err(Error::from_reason("The window is already open"));
        }
        let title = options.title.clone().unwrap_or_else(|| "pumi".to_string());
        let app_name = options.app_name.clone().unwrap_or_else(|| title.clone());
        // `focus: false` must also skip `cx.activate`: the window flag only
        // decides key status inside the app, activation is what steals focus.
        let activate = options.focus.unwrap_or(true);
        let width = options.width.unwrap_or(800.0);
        let height = options.height.unwrap_or(600.0);
        crate::app::ensure(&app_name)?;

        let tree = self.tree.clone();
        let callback = self.event_callback_for_view();
        let selection = self.selection.clone();
        let hide_on_close = self.hide_on_close.clone();
        let window = crate::app::update(|cx| {
            let bounds = gpui::Bounds::centered(None, gpui::size(gpui::px(width as f32), gpui::px(height as f32)), cx);
            let opened = cx.open_window(to_gpui_window_options(&options, bounds), |window, cx| {
                // The window's own life reaches JS as `windowActivated` (`hovered`: whether it is
                // the key window now) and `windowClosed`.
                // The last window hides the app instead (its tree stays, so it is back at once),
                // saying `windowHidden`.
                let closing = callback.clone();
                window.on_window_should_close(cx, move |_, cx| {
                    if hide_on_close.load(std::sync::atomic::Ordering::Relaxed) {
                        cx.hide();
                        emit_event_full(&closing, 0, "windowHidden", |_| {});
                        return false;
                    }
                    emit_event_full(&closing, 0, "windowClosed", |_| {});
                    true
                });
                cx.new(|cx| {
                    let activated = callback.clone();
                    // Each change once, so the app knows whether any of its windows is key.
                    let key = Rc::new(std::cell::Cell::new(false));
                    let say = |key: &std::cell::Cell<bool>, active: bool| {
                        if key.replace(active) != active {
                            crate::app::set_active(active);
                        }
                    };
                    say(&key, window.is_window_active());
                    let key_now = key.clone();
                    cx.observe_window_activation(window, move |_, window, _| {
                        let active = window.is_window_active();
                        say(&key_now, active);
                        emit_event_full(&activated, 0, "windowActivated", |p| p.hovered = Some(active));
                    })
                    .detach();
                    // A key window that closes is no longer key.
                    cx.on_release(move |_, _| say(&key, false)).detach();
                    let resized = callback.clone();
                    let was_fullscreen = std::cell::Cell::new(window.is_fullscreen());
                    cx.observe_window_bounds(window, move |_, window, _| {
                        let fullscreen = window.is_fullscreen();
                        if was_fullscreen.replace(fullscreen) != fullscreen {
                            emit_event_full(&resized, 0, "windowFullscreen", |p| p.hovered = Some(fullscreen));
                        }
                    })
                    .detach();
                    GpuixView::new(tree.clone(), callback.clone(), title, selection.clone())
                })
            });
            if opened.is_ok() && activate {
                cx.activate(true);
            }
            opened
        })?
        .map_err(|error| Error::from_reason(format!("Failed to open the window: {error}")))?;
        *self.window.lock().unwrap() = Some(window);
        self.event_callback.lock().unwrap().take();
        Ok(())
    }

    /// Closes the window. The app keeps running.
    #[napi]
    pub fn close(&self) -> Result<()> {
        let Some(window) = self.window.lock().unwrap().take() else {
            return Ok(());
        };
        crate::app::update(|cx| {
            let _ = window.update(cx, |_view, window, _cx| window.remove_window());
        })
    }

    /// Apply a batch of mutations in a single FFI call.
    ///
    /// Accepts a JSON array of mutation tuples. Each tuple is an array where
    /// the first element is the operation name (string) and remaining elements
    /// are the arguments:
    ///
    ///   ["createElement",    id, "type"]
    ///   ["destroyElement",   id]
    ///   ["appendChild",      parentId, childId]
    ///   ["insertBefore",     parentId, childId, beforeId]
    ///   ["setStyle",         id, { ...style }]
    ///   ["setText",          id, "content"]
    ///   ["setEventListener", id, "eventType", true|false]
    ///   ["setRoot",          id]
    ///   ["setKeyboardFocusDim", true|false]
    ///   ["setCustomProp",    id, "key", value]
    ///
    /// Returns accumulated destroyed IDs from all destroyElement ops.
    /// Acquires the tree mutex ONCE for the entire batch.
    #[napi]
    pub fn apply_batch(&self, json: String) -> Result<Vec<f64>> {
        let mut tree = self.tree.lock().unwrap();
        let destroyed = apply_batch_to_tree(&mut tree, json.as_bytes()).map_err(Error::from_reason)?;
        let dirty = tree.take_dirty();
        drop(tree);
        // A re-render that changed nothing sends a batch of identical resends.
        // Waking the window for it makes it rebuild and diff the whole element
        // tree to draw the frame it already has.
        if dirty {
            // Not `window.refresh()`: that draws every cached subtree again. The root render
            // finds the ones whose elements changed (`crate::boundary`).
            self.update_window(|_view, _window, cx| cx.notify())?;
        }
        Ok(destroyed)
    }

    // ── Frame loop ───────────────────────────────────────────────────

    /// Pump the native event loop. Returns false once the app has quit.
    #[napi]
    pub fn tick(&self) -> bool {
        crate::app::tick()
    }

    #[napi]
    pub fn is_initialized(&self) -> bool {
        self.handle().is_some()
    }

    /// Wake JavaScript after the display link queues a frame. Call tick() in the callback; it
    /// never blocks. Wakes coalesce while JavaScript is busy and do not keep the process alive.
    /// Pass null to clear the callback. Already queued callbacks may still arrive. The hook is
    /// the app's: setting it through any window replaces it for all. False where nothing wakes
    /// JavaScript (Windows, Linux): it calls tick() on its own timer then.
    #[napi]
    pub fn set_frame_wake_callback(&self, callback: Option<Function<(), ()>>) -> Result<bool> {
        if !crate::app::FRAME_WAKE {
            return Ok(false);
        }
        let Some(callback) = callback else {
            crate::app::set_wake_hook(None);
            return Ok(true);
        };
        // Coalescing, sending a lost wake again and counting it all are `frame_loop`'s.
        let wake = callback.build_threadsafe_function::<()>().callee_handled::<false>().weak::<true>().build_callback(
            move |_| {
                crate::frame_loop::arrived();
                Ok(())
            },
        )?;
        crate::app::set_wake_hook(Some(Arc::new(move || {
            wake.call((), ThreadsafeFunctionCallMode::NonBlocking) == napi::Status::Ok
        })));
        Ok(true)
    }

    /// How long to wait for a frame wake before calling tick() anyway, in milliseconds: about a
    /// frame while one of the app's windows is key, so a wake lost on its way costs a frame and not
    /// the window's frame rate; longer otherwise.
    #[napi]
    pub fn frame_wake_fallback_ms(&self) -> u32 {
        crate::app::wake_fallback().as_millis() as u32
    }

    /// Whether JavaScript must call tick(): always, the platform's loop (AppKit, Win32's
    /// messages, calloop) is pumped from the JS thread.
    #[napi]
    pub fn requires_tick(&self) -> bool {
        true
    }

    /// The paintable size of the window in logical pixels, excluding any
    /// platform title bar.
    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        self.update_window(|_view, window, _cx| {
            let size = window.viewport_size();
            WindowSize { width: f32::from(size.width) as f64, height: f32::from(size.height) as f64 }
        })
    }

    #[napi]
    pub fn get_window_insets(&self) -> Result<WindowInsets> {
        self.update_window(|_view, window, _cx| WindowInsets::from_gpui(window.insets()))
    }

    /// `"opaque"` | `"transparent"` | `"blurred"`: what shows where the tree paints nothing.
    /// `"blurred"` is the system's vibrancy behind the window.
    #[napi]
    pub fn set_window_background(&self, mode: String) -> Result<()> {
        let appearance = match mode.as_str() {
            "transparent" => gpui::WindowBackgroundAppearance::Transparent,
            "blurred" => gpui::WindowBackgroundAppearance::Blurred,
            "opaque" => gpui::WindowBackgroundAppearance::Opaque,
            other => return Err(Error::from_reason(format!("unknown window background {other:?}"))),
        };
        self.update_window(move |_view, window, _cx| window.set_background_appearance(appearance))
    }

    /// How much what shows through the window is blurred, in points (0 is not at all): the
    /// window server's blur, seen where the tree paints nothing or paints with an alpha while the
    /// background is not `"opaque"`. macOS only; elsewhere it does nothing.
    #[napi]
    pub fn set_window_blur(&self, radius: f64) -> Result<()> {
        self.update_window(move |_view, window, _cx| crate::backdrop::set_blur(window, radius))?
            .map_err(Error::from_reason)
    }

    /// How much of what is behind the window its backgrounds hide, 0–1 (1, the default, is all
    /// of it): every background the tree paints lets the rest through, however many are stacked,
    /// and keeps its own colour, while text, icons and images stay opaque. Seen while the
    /// background is not `"opaque"`. macOS only; elsewhere it does nothing.
    #[napi]
    pub fn set_window_glass(&self, opacity: f64) -> Result<()> {
        self.update_window(move |_view, window, _cx| {
            let done = crate::backdrop::set_glass(window, opacity);
            window.refresh();
            done
        })?
        .map_err(Error::from_reason)
    }

    /// Whether the window is full screen now (the traffic lights are hidden then). Changes
    /// reach the window's handler as `windowFullscreen`, `hovered` saying which.
    #[napi]
    pub fn is_fullscreen(&self) -> Result<bool> {
        self.update_window(|_view, window, _cx| window.is_fullscreen())
    }

    /// The style every `title` tooltip is drawn in (a style object as JSON), or null for the
    /// renderer's own. It applies to every window.
    #[napi]
    pub fn set_tooltip_style(&self, style: Option<String>) -> Result<()> {
        let style = match style {
            Some(json) => Some(
                serde_json::from_str::<StyleDesc>(&json)
                    .map_err(|error| Error::from_reason(format!("tooltip style: {error}")))?,
            ),
            None => None,
        };
        crate::tooltip::set_style(style);
        // No window yet is fine: the style is the renderer's, for whenever one opens.
        let _ = self.invalidate_window();
        Ok(())
    }

    /// `"hidden"` | `"minimal"` | `"full"`. Paints into the scene after layout.
    #[napi]
    pub fn set_debug_frame_overlay(&self, mode: String) -> Result<String> {
        let mode = parse_debug_frame_overlay_mode(&mode)?;
        self.update_window(move |_view, window, _cx| {
            window.set_debug_frame_overlay_mode(mode);
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }

    /// Hidden → minimal → full → hidden.
    #[napi]
    pub fn cycle_debug_frame_overlay(&self) -> Result<String> {
        self.update_window(move |_view, window, _cx| {
            window.cycle_debug_frame_overlay_mode();
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }

    #[napi]
    pub fn get_debug_frame_overlay(&self) -> Result<String> {
        self.debug_frame_overlay_mode()
    }

    fn debug_frame_overlay_mode(&self) -> Result<String> {
        self.update_window(|_view, window, _cx| {
            debug_frame_overlay_mode_name(window.debug_frame_overlay_mode()).to_string()
        })
    }

    /// Clears the last 1000 draw samples. Frame count stays.
    #[napi]
    pub fn reset_debug_frame_overlay_stats(&self) -> Result<()> {
        self.update_window(|_view, window, _cx| {
            window.reset_debug_frame_overlay_stats();
        })
    }

    /// Same numbers as the on-screen overlay: current, p90, p99, max, frames.
    #[napi]
    pub fn get_debug_frame_overlay_stats(&self) -> Result<DebugFrameOverlayStats> {
        self.update_window(|_view, window, _cx| debug_frame_overlay_stats_js(window.debug_frame_overlay_stats()))
    }

    /// What each compartment's renders cost since the last call (a plugin's component, a theme's part:
    /// `compartment="owner/name"`), the compartments with the same name together, and whether
    /// any of them is drawn on a budget now. Reading it starts the counts again.
    #[napi]
    pub fn get_compartment_stats(&self) -> Result<Vec<CompartmentStats>> {
        self.update_window(|view, _window, _cx| view.compartment_stats())
    }

    /// Bring the window forward and give it focus. This is how a window opened
    /// with `show: false` or `focus: false` is revealed later.
    #[napi]
    pub fn activate_window(&self) -> Result<()> {
        self.update_window(|_view, window, cx| {
            cx.activate(true);
            window.activate_window();
        })
    }

    /// Whether closing the window hides the app instead of closing it (the app's last window).
    #[napi]
    pub fn set_hide_on_close(&self, hide: bool) {
        self.hide_on_close.store(hide, std::sync::atomic::Ordering::Relaxed);
    }

    /// Minimize the native window.
    #[napi]
    pub fn minimize_window(&self) -> Result<()> {
        self.update_window(|_view, window, _cx| window.minimize_window())
    }

    /// Run the native zoom or maximize operation.
    #[napi]
    pub fn zoom_window(&self) -> Result<()> {
        self.update_window(|_view, window, _cx| window.zoom_window())
    }

    /// Enter or exit native fullscreen.
    #[napi]
    pub fn toggle_fullscreen(&self) -> Result<()> {
        self.update_window(|_view, window, _cx| window.toggle_fullscreen())
    }

    #[napi]
    pub fn set_window_title(&self, title: String) -> Result<()> {
        self.update_window(move |view, window, cx| {
            view.window_title = title;
            cx.notify();
            window.refresh();
        })
    }

    /// Open the platform path picker. Returns null when the user cancels.
    #[napi(ts_return_type = "Promise<Array<string> | null>")]
    pub fn prompt_for_paths<'env>(&self, env: &'env Env, options: Option<PathPromptOptions>) -> Result<Object<'env>> {
        let (deferred, promise): (PathPromptDeferred, Object<'env>) = env.create_deferred()?;
        match PathPromptOptions::to_gpui(options) {
            Ok(options) => self.begin_path_prompt(options, deferred),
            Err(error) => deferred.reject(error),
        }
        Ok(promise)
    }

    #[napi]
    pub fn focus_element(&self, element_id: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        self.update_window(move |view, window, cx| {
            view.request_focus(id, window, cx);
            window.refresh();
        })
    }

    /// Move focus to the next GPUI tab stop.
    #[napi]
    pub fn focus_next(&self) -> Result<()> {
        self.update_window(|_view, window, cx| window.focus_next(cx))
    }

    /// Move focus to the previous GPUI tab stop.
    #[napi]
    pub fn focus_previous(&self) -> Result<()> {
        self.update_window(|_view, window, cx| window.focus_prev(cx))
    }

    /// Host id of the focused element, or null when nothing is focused.
    #[napi]
    pub fn get_focused_element_id(&self) -> Result<Option<f64>> {
        self.update_window(|view, window, _cx| view.focused_element_id(window).map(|id| id as f64))
    }

    /// Move focus to the next tab stop inside `element_id`, wrapping in that subtree.
    #[napi]
    pub fn focus_next_within(&self, element_id: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        self.update_window(move |view, window, cx| {
            view.focus_next_within(id, window, cx);
            cx.notify();
            window.refresh();
        })
    }

    /// Move focus to the previous tab stop inside `element_id`, wrapping in that subtree.
    #[napi]
    pub fn focus_previous_within(&self, element_id: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        self.update_window(move |view, window, cx| {
            view.focus_previous_within(id, window, cx);
            cx.notify();
            window.refresh();
        })
    }

    /// Enable the window key events requested by the React renderer.
    #[napi]
    pub fn set_window_key_events(&self, key_down: bool, key_up: bool, event_id: f64) -> Result<()> {
        let event_id = to_element_id(event_id)?;
        self.update_window(move |view, window, cx| {
            view.window_key_down = key_down;
            view.window_key_up = key_up;
            view.window_key_event_id = event_id;
            cx.notify();
            window.refresh();
        })
    }

    #[napi]
    pub fn blur(&self) -> Result<()> {
        self.update_window(move |_view, window, _cx| window.blur())
    }

    /// Enable the window selectionChange event requested by the React renderer.
    #[napi]
    pub fn set_window_selection_change(&self, enabled: bool, event_id: f64) -> Result<()> {
        let event_id = to_element_id(event_id)?;
        self.update_window(move |view, window, cx| {
            view.set_selection_change_listener(enabled, event_id);
            cx.notify();
            window.refresh();
        })
    }

    // ── Selection API ────────────────────────────────────────────────

    /// The current text selection joined in document order, or null.
    #[napi]
    pub fn get_selected_text(&self) -> Option<String> {
        self.selection.lock().selected_text()
    }

    /// Drop the current selection and request a repaint.
    #[napi]
    pub fn clear_selection(&self) -> Result<()> {
        self.selection.lock().clear();
        self.request_invalidate()
    }

    // ── Scroll API ───────────────────────────────────────────────────
    // GpuixView syncs scroll handles and virtual list states to thread-local maps.

    /// Set the scroll offset of a scrollable element.
    /// x and y are negative pixel values (scroll down = more negative y).
    #[napi]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        if !VIRTUAL_LIST_STATES.with(|cell| {
            let states = cell.borrow();
            let Some(state) = states.get(&id) else {
                return false;
            };
            state.set_offset_from_scrollbar(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
            true
        }) {
            SCROLL_HANDLES.with(|cell| {
                let handles = cell.borrow();
                if let Some(handle) = handles.get(&id) {
                    handle.set_offset(gpui::point(gpui::px(x as f32), gpui::px(y as f32)));
                }
            });
        }
        self.invalidate_window()
    }

    /// Scroll a child into view by its index in the children list.
    ///
    /// For a `<virtual-list>` the scroll is queued and applied on the next
    /// render, after that frame's child splice, so indices computed against a
    /// just-committed child list are never shifted twice. `offsetInItem` is in
    /// pixels and may be negative, which anchors the viewport top above the
    /// item and resolves against measured heights at layout time.
    #[napi]
    pub fn scroll_to_item(&self, element_id: f64, index: f64, offset_in_item: Option<f64>) -> Result<()> {
        let id = to_element_id(element_id)?;
        let index = index as usize;
        let offset = offset_in_item.unwrap_or(0.0) as f32;
        if !VIRTUAL_LIST_STATES.with(|cell| {
            if !cell.borrow().contains_key(&id) {
                return false;
            }
            queue_virtual_list_scroll(id, index, offset);
            true
        }) {
            SCROLL_HANDLES.with(|cell| {
                let handles = cell.borrow();
                if let Some(handle) = handles.get(&id) {
                    handle.scroll_to_item(index);
                }
            });
        }
        self.invalidate_window()
    }

    /// The logical scroll anchor of a `<virtual-list>`:
    /// `[itemIndex, offsetInItemPx, viewportHeightPx]`, or null for anything
    /// else. `itemIndex == item count` is gpui's at-end sentinel.
    ///
    /// Unlike `getScrollOffset` this is exact even while row heights are still
    /// estimates, because it is the anchor gpui itself scrolls by.
    #[napi]
    pub fn get_list_scroll_top(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(element_id)?;
        Ok(VIRTUAL_LIST_STATES.with(|cell| {
            cell.borrow().get(&id).map(|state| {
                let top = state.logical_scroll_top();
                vec![
                    top.item_ix as f64,
                    f64::from(f32::from(top.offset_in_item)),
                    f64::from(f32::from(state.viewport_bounds().size.height)),
                ]
            })
        }))
    }

    /// Get the current scroll offset of a scrollable element.
    /// Returns [x, y] or null if the element has no scroll handle.
    #[napi]
    pub fn get_scroll_offset(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        let id = to_element_id(element_id)?;
        Ok(VIRTUAL_LIST_STATES
            .with(|cell| {
                cell.borrow().get(&id).map(|state| {
                    let offset = state.scroll_px_offset_for_scrollbar();
                    vec![f64::from(f32::from(offset.x)), f64::from(f32::from(offset.y))]
                })
            })
            .or_else(|| {
                SCROLL_HANDLES.with(|cell| {
                    let handles = cell.borrow();
                    handles.get(&id).map(|handle| {
                        let offset = handle.offset();
                        vec![f64::from(f32::from(offset.x)), f64::from(f32::from(offset.y))]
                    })
                })
            }))
    }

    /// Scroll this element's nearest scroll parent until the element is visible.
    #[napi]
    pub fn scroll_into_view(&self, element_id: f64) -> Result<()> {
        let id = to_element_id(element_id)?;
        {
            self.update_window(move |view, window, cx| {
                if view.scroll_element_into_view(id) {
                    cx.notify();
                    window.refresh();
                }
            })?;
            Ok(())
        }
    }

    /// Paint packed pixels onto an `<img>` host node. `format` is `"rgba"`
    /// (default) or `"bgra"`; BGRA skips the per-pixel swizzle.
    #[napi]
    pub fn set_image_pixels(
        &self,
        element_id: f64,
        width: f64,
        height: f64,
        pixels: Buffer,
        format: Option<String>,
    ) -> Result<()> {
        let id = to_element_id(element_id)?;
        let width = dimension_u32(width, "width")?;
        let height = dimension_u32(height, "height")?;
        let format = crate::custom_elements::img::PixelFormat::parse(format.as_deref()).map_err(Error::from_reason)?;
        let bytes = pixels.to_vec();
        {
            self.update_window(move |view, window, cx| {
                view.set_image_pixels(id, width, height, bytes, format, window, cx)
            })?
            .map_err(Error::from_reason)
        }
    }

    /// Decode PNG, JPEG, WebP, GIF, SVG, BMP, TIFF, ICO, or Netpbm bytes onto
    /// an `<img>` host node. Prefer `setImagePixels` for live waveforms.
    #[napi]
    pub fn set_image(&self, element_id: f64, bytes: Buffer) -> Result<()> {
        let id = to_element_id(element_id)?;
        let bytes = bytes.to_vec();
        {
            self.update_window(move |view, window, cx| view.set_encoded_image(id, bytes, window, cx))?
                .map_err(Error::from_reason)
        }
    }

    /// Make the fonts in a TrueType or OpenType file (or a collection) usable by
    /// `fontFamily`, under the family names the file declares. Call it before
    /// the text that names the family is first laid out: a family that did not
    /// exist then has already fallen back to another font.
    #[napi]
    pub fn register_font(&self, data: Buffer) -> Result<()> {
        let bytes = data.to_vec();
        {
            self.update_window(move |view, window, cx| view.register_font(bytes, window, cx))?
                .map_err(Error::from_reason)
        }
    }

    #[napi]
    pub fn get_automation_tree(&self) -> Result<String> {
        self.request_invalidate()?;
        let bounds = self.automation_bounds()?;
        let tree = self.tree.lock().unwrap();
        let json = tree.to_automation_json(&bounds);
        serde_json::to_string(&json).map_err(|e| Error::from_reason(format!("JSON serialization failed: {}", e)))
    }

    #[napi]
    pub fn get_element_bounds(&self, id: f64) -> Result<Option<ElementBounds>> {
        let id = to_element_id(id)?;
        Ok(self.element_bounds(id)?.map(ElementBounds::from_painted))
    }

    /// The elements whose last painted box holds the window point `(x, y)`,
    /// topmost first, like the DOM's `elementsFromPoint`. An element that did
    /// not paint is never listed, `overflow: hidden | scroll` clips what lies
    /// outside it, and `pointer-events: none` is skipped.
    #[napi]
    pub fn get_elements_at(&self, x: f64, y: f64) -> Result<Vec<f64>> {
        Ok(self.elements_at(x, y)?.into_iter().map(|id| id as f64).collect())
    }

    #[napi]
    pub fn get_all_text(&self) -> Vec<String> {
        let tree = self.tree.lock().unwrap();
        let mut texts = Vec::new();
        if let Some(root_id) = tree.root_id {
            collect_text(root_id, &tree, &mut texts);
        }
        texts
    }

    #[napi]
    pub fn get_painted_text(&self) -> Vec<String> {
        crate::text::painted_text()
    }

    /// Every highlight wash painted in the last frame, in paint order.
    ///
    /// A quad is invisible to `getPaintedText()`, so this is the only way to
    /// assert on `highlight` without a screenshot.
    #[napi]
    pub fn get_painted_highlights(&self) -> Vec<crate::element_tree::HighlightMatch> {
        crate::text::painted_highlights().into_iter().map(Into::into).collect()
    }

    /// Simulate space-separated keystrokes through the focused element's input pipeline.
    #[napi]
    pub fn simulate_keystrokes(&self, keystrokes: String) -> Result<()> {
        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_keystrokes(window, cx, &keystrokes)
        })?
        .map_err(Error::from_reason)
    }

    #[napi]
    pub fn simulate_key_down(&self, keystroke: String, is_held: Option<bool>) -> Result<()> {
        let is_held = is_held.unwrap_or(false);

        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_key_down(window, cx, &keystroke, is_held)
        })?
        .map_err(Error::from_reason)
    }

    #[napi]
    pub fn simulate_key_up(&self, keystroke: String) -> Result<()> {
        self.update_window_without_view(move |window, cx| crate::automation::dispatch_key_up(window, cx, &keystroke))?
            .map_err(Error::from_reason)
    }

    /// `modifiers` uses the `press()` syntax: "cmd", "cmd-shift", "alt".
    #[napi]
    pub fn simulate_click(&self, x: f64, y: f64, button: Option<u32>, modifiers: Option<String>) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_click(window, cx, x, y, button, modifiers);
        })
    }

    #[napi]
    pub fn simulate_mouse_down(&self, x: f64, y: f64, button: Option<u32>, modifiers: Option<String>) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_down(window, cx, x, y, button, modifiers);
        })
    }

    #[napi]
    pub fn simulate_mouse_up(&self, x: f64, y: f64, button: Option<u32>, modifiers: Option<String>) -> Result<()> {
        let button = button.unwrap_or(0);
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_up(window, cx, x, y, button, modifiers);
        })
    }

    #[napi]
    pub fn simulate_mouse_move(
        &self,
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_mouse_move(window, cx, x, y, pressed_button, modifiers);
        })
    }

    /// Dispatch a wheel event through the same GPUI hit test the trackpad uses.
    /// Deltas are pixels: negative `delta_y` scrolls down, negative `delta_x`
    /// pans right, matching `TestGpuixRenderer::simulate_scroll_wheel`.
    #[napi]
    pub fn simulate_scroll_wheel(
        &self,
        x: f64,
        y: f64,
        delta_x: f64,
        delta_y: f64,
        modifiers: Option<String>,
    ) -> Result<()> {
        let modifiers = crate::automation::parse_modifiers(modifiers.as_deref());

        self.update_window_without_view(move |window, cx| {
            crate::automation::dispatch_scroll_wheel(window, cx, x, y, delta_x, delta_y, modifiers);
        })
    }

    #[napi]
    pub fn clock_pause(&self) -> Result<f64> {
        self.update_window(move |view, _window, cx| {
            let now_ms = view.clock.pause();
            cx.notify();
            now_ms
        })
    }

    #[napi]
    pub fn clock_set(&self, now_ms: f64) -> Result<f64> {
        self.update_window(move |view, _window, cx| {
            let now_ms = view.clock.set_ms(now_ms);
            cx.notify();
            now_ms
        })
    }

    #[napi]
    pub fn clock_fast_forward(&self, delta_ms: f64) -> Result<f64> {
        self.update_window(move |view, _window, cx| {
            let now_ms = view.clock.fast_forward_ms(delta_ms);
            cx.notify();
            now_ms
        })
    }

    #[napi]
    pub fn clock_resume(&self) -> Result<f64> {
        self.update_window(move |view, _window, cx| {
            let now_ms = view.clock.resume();
            cx.notify();
            now_ms
        })
    }

    #[napi]
    pub fn capture_screenshot(&self, path: String) -> Result<()> {
        #[cfg(not(feature = "test-support"))]
        return Err(Error::from_reason(format!("{path}: screenshots need an addon built with test-support")));
        #[cfg(feature = "test-support")]
        {
            let image = self
                .update_window(move |_view, window, cx| {
                    cx.notify();
                    window.refresh();
                    window.render_to_image()
                })?
                .map_err(|e| Error::from_reason(format!("Screenshot capture failed: {}", e)))?;
            image.save(&path).map_err(|e| Error::from_reason(format!("Failed to save screenshot: {}", e)))?;
            Ok(())
        }
    }
}

fn collect_text(id: u64, tree: &RetainedTree, texts: &mut Vec<String>) {
    if let Some(element) = tree.elements.get(&id) {
        if let Some(ref content) = element.content {
            texts.push(content.clone());
        }
        for &child_id in &element.children {
            collect_text(child_id, tree, texts);
        }
    }
}

// ── GPUI View ────────────────────────────────────────────────────────

pub struct GpuixView {
    pub(crate) tree: Arc<Mutex<RetainedTree>>,
    pub(crate) event_callback: Option<EventCallback>,
    pub(crate) window_title: String,
    /// The title last given to the platform window. Setting it is not free on macOS: AppKit
    /// renames the Window menu's item and GPUI lays the traffic lights out again, so it is set
    /// when it changes, not on every frame.
    applied_title: Option<String>,
    pub(crate) window_key_down: bool,
    pub(crate) window_key_up: bool,
    pub(crate) window_key_event_id: u64,
    pub(crate) window_selection_change: bool,
    pub(crate) window_selection_event_id: u64,
    /// Last identity delivered through `selectionChange`. Only written once an
    /// event is really queued, so adding the listener later still reports.
    reported_selection: Option<u64>,
    /// Persistent FocusHandles keyed by element ID.
    /// Created lazily for elements with keyboard or focus/blur listeners.
    /// Handles persist across renders so GPUI maintains focus state.
    pub(crate) focus_handles: HashMap<u64, gpui::FocusHandle>,
    pending_focus_element: Option<PendingFocus>,
    /// Active focus/blur subscriptions keyed by element and event type.
    pub(crate) focus_subscriptions: HashMap<(u64, String), gpui::Subscription>,
    /// Registry for custom element types (input, editor, diff, etc.).
    /// Stores factories (one per type) and live instances (one per element ID).
    pub(crate) custom_registry: CustomElementRegistry,
    /// Persistent ScrollHandles keyed by element ID.
    /// Created lazily for elements with overflow: "scroll" (or per-axis scroll).
    /// Handles persist across renders so GPUI maintains scroll offset state.
    pub(crate) scroll_handles: HashMap<u64, gpui::ScrollHandle>,
    /// Native animation clocks keyed by retained element ID.
    pub(crate) motion_states: HashMap<u64, crate::motion::MotionState>,
    /// When this window's first motion track started: every capped track samples on its grid
    /// (`MotionState::set_grid`).
    pub(crate) motion_grid: Option<web_time::Instant>,
    /// Live text selection, shared with the paint closures and the napi methods.
    pub(crate) selection: SharedSelection,
    /// Persistent measurement and scroll state for React-backed virtual lists.
    virtual_lists: HashMap<u64, VirtualListEntry>,
    /// Latest pointer sample and list during selection edge scrolling.
    selection_drag_position: Option<gpui::Point<gpui::Pixels>>,
    selection_scroll_list: Option<u64>,
    selection_scroll_task: Option<gpui::Task<()>>,
    /// Motion / review clock. Live wall time unless automation freezes it.
    pub(crate) clock: crate::automation::AutomationClock,
    /// Resolved `highlight` state, keyed by the element that declared it.
    /// Empty in every app that does not use search.
    highlights: HashMap<u64, HighlightCacheEntry>,
    /// What `onLayout` and `onScrollChange` last reported, for the elements that
    /// have the listener. Shared with the paint closures that compare against it.
    paint_reports: SharedPaintReports,
    /// The `cache` elements, each drawn by a view of its own (`crate::boundary`).
    boundaries: HashMap<u64, crate::boundary::Boundary>,
    /// What the root render saw, for the cached subtrees it builds later in the frame.
    frame: FrameInputs,
    /// What the last frames an endless animation moved in cost, newest last (`pace_loops`).
    loop_costs: Rc<RefCell<VecDeque<std::time::Duration>>>,
    /// The endless animations told they were slowed (`motionThrottled`).
    slowed: HashSet<u64>,
}

/// What one frame's builds share: the root render's, again in each cached subtree's.
#[derive(Clone)]
struct FrameInputs {
    now: web_time::Instant,
    keyboard_focus: Option<Arc<[u64]>>,
    /// `setKeyboardFocusDim`, which every focusable's look reads.
    focus_dim: bool,
    /// Every cached subtree is drawn again this frame.
    cache_all: bool,
}

#[derive(Default)]
pub(crate) struct PaintReports {
    /// Border-box size last delivered through `onLayout`.
    sizes: HashMap<u64, (f32, f32)>,
    /// Scroll offset last seen for `onScrollChange`. Recorded on the first paint
    /// without an event, so only movement is reported.
    scrolls: HashMap<u64, (f32, f32)>,
}

pub(crate) type SharedPaintReports = Rc<RefCell<PaintReports>>;

/// Two-level cache for one element's `highlight`.
///
/// The group list is keyed by `search_revision`, which a query change does NOT
/// move, so typing in a find bar never re-walks or re-folds text. The matches
/// are additionally keyed by the matcher hash, which excludes `activeIndex` and
/// the colours, so moving the find cursor only re-colours what it already found.
///
/// Do not key the group list on `subtree_revision`: `highlight` is a custom
/// prop, so every keystroke moves that revision and the cache would do nothing.
/// `highlight_cache_tests` at the bottom of this file compares `Arc` identity
/// and fails if either level regresses.
struct HighlightCacheEntry {
    revision: u64,
    groups: Arc<crate::text::GroupList>,
    matcher_hash: u64,
    /// The spec plus the located matches. Ordinals and colours are decided at
    /// paint, so a colour or `activeIndex` change reuses this whole value.
    context: Arc<crate::text::HighlightContext>,
    /// Last identity delivered through `onHighlight`. Only written once an
    /// event is really queued, so adding the listener later still reports.
    reported: Option<u64>,
}

fn emit_motion_settled(
    callback: &Option<EventCallback>,
    tree: &crate::retained_tree::RetainedTree,
    completions: &[(u64, u64)],
) {
    for &(id, generation) in completions {
        let Some(element) = tree.elements.get(&id) else {
            continue;
        };
        if !element.events().contains("motionComplete") {
            continue;
        }
        emit_event_full(callback, id, "motionComplete", |payload| {
            payload.motion_generation = Some(generation as f64);
        });
    }
}

/// Every motion track, brought to one moment (`sync_motion_states`).
struct MotionSync {
    /// A track wants every display frame.
    active: bool,
    /// The soonest a capped track wants its next sample.
    wake: Option<std::time::Duration>,
    /// The tracks that just settled, with their generation.
    settled: Vec<(u64, u64)>,
    /// The tracks that moved or settled.
    moving: Vec<u64>,
    /// The capped tracks that moved, with the time between their samples.
    looping: Vec<(u64, std::time::Duration)>,
}

/// Brings every motion track to `now`.
fn sync_motion_states(
    tree: &crate::retained_tree::RetainedTree,
    states: &mut HashMap<u64, crate::motion::MotionState>,
    grid: &mut Option<web_time::Instant>,
    now: web_time::Instant,
    // The root build's: it asks which tracks moved since the last frame (and the answer is
    // used up); a list row built on its own does not.
    root: bool,
) -> MotionSync {
    states.retain(|id, _| tree.motion_ids.contains(id));
    let mut active = false;
    let mut wake: Option<std::time::Duration> = None;
    let mut settled = Vec::new();
    let mut moving = Vec::new();
    let mut looping = Vec::new();

    for &id in &tree.motion_ids {
        let Some(source) = tree.elements.get(&id).and_then(|element| element.custom_props().get("motion")) else {
            continue;
        };
        let state = match states.entry(id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => match crate::motion::MotionState::new(source, now) {
                Ok(mut state) => {
                    // Every capped track of the window samples on one grid: the first's start.
                    state.set_grid(*grid.get_or_insert(now));
                    entry.insert(state)
                }
                Err(error) => {
                    log::warn!("Invalid motion description for element {id}: {error}");
                    entry.insert(crate::motion::MotionState::invalid(source, now))
                }
            },
        };
        if let Err(error) = state.sync(source, now) {
            log::warn!("Invalid motion update for element {id}: {error}");
        }
        let frame = state.frame(now);
        // Only what shows something new is drawn again: a capped loop between its samples is
        // not, so the cached subtrees around a spinner are not built every display frame.
        let moved = if root { state.moved_since_asked(&frame, now) } else { frame.active };
        if moved || frame.just_settled {
            moving.push(id);
        }
        match frame.wake {
            Some(after) if frame.active => wake = Some(wake.map_or(after, |soonest| soonest.min(after))),
            _ => active |= frame.active,
        }
        if let Some(step) = frame.step {
            looping.push((id, step));
        }
        if frame.just_settled {
            settled.push((id, frame.generation));
        }
    }

    MotionSync { active, wake, settled, moving, looping }
}

/// The share of the time endless animations may spend drawing the window. A frame one moves in
/// costs what is drawn again because it moved; one whose frames cost more is sampled less often,
/// so a spinner in a big uncached tree is a slower spinner, not a slower window.
const LOOP_SHARE: f64 = 0.05;
/// The frames an endless animation's cost is judged by: the median of the last few its own timed
/// repaints drew (not the ones a terminal's output or a hover asked for), so one that was dear for
/// another reason (the tree's first layout) does not count.
const LOOP_FRAMES: usize = 8;

/// A zero-size canvas, last in the window's tree: what the frame cost when it is painted, kept
/// in `costs` when an endless animation's timed repaint drew it.
fn loop_frame_cost(
    started: std::time::Instant,
    looping: bool,
    costs: Rc<RefCell<VecDeque<Duration>>>,
) -> impl gpui::IntoElement {
    use gpui::Styled as _;
    gpui::canvas(
        |_, _, _| (),
        move |_, _, _, _| {
            if !looping {
                return;
            }
            let mut costs = costs.borrow_mut();
            if costs.len() == LOOP_FRAMES {
                costs.pop_front();
            }
            costs.push_back(started.elapsed());
        },
    )
    .absolute()
    .w(gpui::px(0.0))
    .h(gpui::px(0.0))
}

/// The name of the compartment `id` is drawn in: the nearest one around it that has a name.
fn enclosing_compartment(tree: &crate::retained_tree::RetainedTree, id: u64) -> Option<String> {
    let mut current = tree.elements.get(&id).and_then(|element| element.parent);
    while let Some(id) = current {
        let element = tree.elements.get(&id)?;
        if let Some(name) = compartment_name(element).filter(|name| !name.is_empty()) {
            return Some(name);
        }
        current = element.parent;
    }
    None
}

fn emit_highlight_events(callback: &Option<EventCallback>, events: &[(u64, usize)]) {
    for &(id, total) in events {
        emit_event_full(callback, id, "highlight", |payload| {
            payload.match_count = Some(total as f64);
        });
    }
}

thread_local! {
    /// Bumped once per GPUI key dispatch, before any key listener runs.
    static KEYSTROKE_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Emit a key event stamped with the current dispatch's keystroke id.
///
/// JS handlers run after GPUI finished dispatching (the event callback is
/// non-blocking), so they cannot stop GPUI propagation. The id lets the JS host
/// group one keystroke's element events and its trailing window event, and
/// apply `preventDefault` / `stopPropagation` across them.
pub(crate) fn emit_key_event(
    callback: &Option<EventCallback>,
    element_id: u64,
    event_type: &str,
    keystroke: &gpui::Keystroke,
    is_held: Option<bool>,
) {
    let keystroke_id = KEYSTROKE_ID.with(|id| id.get());
    emit_event_full(callback, element_id, event_type, |payload| {
        payload.key = Some(keystroke.key.clone());
        payload.key_char = keystroke.key_char.clone();
        payload.is_held = is_held;
        payload.modifiers = Some(keystroke.modifiers.into());
        payload.keystroke_id = Some(keystroke_id as f64);
    });
}

fn next_keystroke_id() {
    KEYSTROKE_ID.with(|id| id.set(id.get() + 1));
}

thread_local! {
    /// Bumped once per GPUI mouse press and release dispatch, before any element listener runs.
    static PRESS_ID: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Emit a press event (a click, a mouse down or up) stamped with the current dispatch's press id.
///
/// As with keys, JS handlers run after GPUI finished dispatching, so they cannot stop GPUI
/// propagation; the id lets the JS host skip an ancestor's handler of the same event once a
/// descendant's called `stopPropagation`. A click that no mouse produced (an a11y press) has none.
pub(crate) fn emit_press_event(
    callback: &Option<EventCallback>,
    element_id: u64,
    event_type: &str,
    build: impl FnOnce(&mut EventPayload),
) {
    let press_id = PRESS_ID.with(|id| id.get());
    emit_event_full(callback, element_id, event_type, |payload| {
        build(payload);
        payload.press_id = Some(press_id as f64);
    });
}

/// The root's listener that numbers each mouse press and release. Capture runs before every
/// element's bubble listener, so the events of one press share an id.
fn press_dispatch() -> impl gpui::IntoElement {
    use gpui::prelude::*;

    let next = || PRESS_ID.with(|id| id.set(id.get() + 1));
    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            window.on_mouse_event(move |_: &gpui::MouseDownEvent, phase, _, _| {
                if phase == gpui::DispatchPhase::Capture {
                    next();
                }
            });
            window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _, _| {
                if phase == gpui::DispatchPhase::Capture {
                    next();
                }
            });
        },
    )
    .absolute()
    .w(gpui::px(0.0))
    .h(gpui::px(0.0))
}

/// The chords the app's key bindings take before the focused element sees them, and whether it
/// takes every key (a sequence is pending). `setCapturedKeys` sets them.
static CAPTURED: std::sync::RwLock<(bool, Vec<String>)> = std::sync::RwLock::new((false, Vec::new()));

/// Sets the chords (`shift+cmd+p`) the window takes for the app's bindings: such a key reaches
/// `windowKeyDown` (with `captured`) and no element. `all` takes every key, for the rest of a
/// sequence.
#[napi]
pub fn set_captured_keys(chords: Vec<String>, all: bool) {
    *CAPTURED.write().unwrap() = (all, chords);
    CAPTURED_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Counts `set_captured_keys` calls, so a web page knows when to be told the chords again.
static CAPTURED_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The chords the app's bindings take, whether every key is taken, and the generation they are of:
/// what a focused web page hands back to the window instead of keeping.
pub fn captured_keys() -> (u64, bool, Vec<String>) {
    let captured = CAPTURED.read().unwrap();
    (CAPTURED_GENERATION.load(std::sync::atomic::Ordering::Relaxed), captured.0, captured.1.clone())
}

/// Whether the app's bindings take `keystroke` now.
pub fn is_captured(keystroke: &gpui::Keystroke) -> bool {
    captured(keystroke)
}

/// A keystroke's one name, as key bindings write it: modifiers in the order ctrl, alt, shift,
/// cmd, then the key (`minus`, `equal`, `plus`, `slash` for those).
pub fn chord(keystroke: &gpui::Keystroke) -> String {
    let gpui::Modifiers { control, alt, mut shift, platform, .. } = keystroke.modifiers;
    let mut key = match keystroke.key.to_ascii_lowercase().as_str() {
        "-" => "minus".to_string(),
        "=" => "equal".to_string(),
        "+" => "plus".to_string(),
        "/" => "slash".to_string(),
        other => other.to_string(),
    };
    if key == "~" && !shift {
        (shift, key) = (true, "`".into());
    }
    let mut parts: Vec<&str> = Vec::new();
    for (pressed, name) in [(control, "ctrl"), (alt, "alt"), (shift, "shift"), (platform, "cmd")] {
        if pressed {
            parts.push(name);
        }
    }
    if parts.is_empty() {
        key
    } else {
        format!("{}+{key}", parts.join("+"))
    }
}

fn captured(keystroke: &gpui::Keystroke) -> bool {
    let captured = CAPTURED.read().unwrap();
    let modifier_only = matches!(keystroke.key.as_str(), "shift" | "control" | "alt" | "platform" | "function");
    if modifier_only {
        return false;
    }
    if captured.0 {
        return true;
    }
    let chord = chord(keystroke);
    captured.1.contains(&chord) && !KEPT.read().unwrap().contains(&chord)
}

/// The chords the web page that has the keyboard keeps for itself (`<WebView keepKeys>`, Monaco's
/// `cmd+d`): the app's bindings leave them to it until it hands the keyboard back. A sequence the
/// app has begun still takes every key.
static KEPT: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Says which chords the focused page keeps (none once no page has the keyboard).
pub fn set_kept_keys(chords: Vec<String>) {
    *KEPT.write().unwrap() = chords;
}

fn window_key_events(
    callback: Option<EventCallback>,
    key_down: bool,
    key_up: bool,
    event_id: u64,
) -> impl gpui::IntoElement {
    use gpui::prelude::*;

    gpui::canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            // Root capture runs first in GPUI's key dispatch, before every
            // element listener, so each keystroke gets a fresh id even when an
            // element later stops propagation.
            let callback_down = callback.clone();
            window.on_root_key_event(move |event: &gpui::KeyDownEvent, phase, _window, _cx| {
                if phase == gpui::DispatchPhase::Capture {
                    next_keystroke_id();
                    // A key the app's bindings take goes to JS alone.
                    if key_down && captured(&event.keystroke) {
                        emit_key_event(
                            &callback_down,
                            event_id,
                            "windowKeyDown",
                            &event.keystroke,
                            Some(event.is_held),
                        );
                        _cx.stop_propagation();
                    }
                    return;
                }
                if key_down {
                    emit_key_event(&callback_down, event_id, "windowKeyDown", &event.keystroke, Some(event.is_held));
                }
                // Tab is a navigation key, never text, like in a browser field.
                // Every element listener already ran; stopping here only keeps
                // the platform from inserting "\t" into the focused <input>
                // and, on the web, keeps focus on GPUI's hidden keyboard
                // element. The JS host decides whether focus moves.
                if event.keystroke.key == "tab" {
                    _cx.stop_propagation();
                }
            });
            let callback_up = callback.clone();
            window.on_root_key_event(move |event: &gpui::KeyUpEvent, phase, _window, _cx| {
                if phase == gpui::DispatchPhase::Capture {
                    next_keystroke_id();
                    return;
                }
                if key_up {
                    emit_key_event(&callback_up, event_id, "windowKeyUp", &event.keystroke, None);
                }
            });
            // A modifier pressed or let go alone (holding ⌘ shows the keys): no keystroke has it,
            // so the window says which are held now. In the capture phase, so a focused element
            // that stops it still lets the window know.
            let callback_mods = callback.clone();
            window.on_root_key_event(move |event: &gpui::ModifiersChangedEvent, phase, _window, _cx| {
                if phase == gpui::DispatchPhase::Capture && (key_down || key_up) {
                    emit_event_full(&callback_mods, event_id, "windowModifiersChanged", |payload| {
                        payload.modifiers = Some(event.modifiers.into());
                    });
                }
            });
        },
    )
    .absolute()
    .w(gpui::px(0.0))
    .h(gpui::px(0.0))
}

/// Resolve one element's `highlight` prop, reusing both cache levels.
///
/// Returns the context, plus the match count when `has_listener` and the result
/// differs from the last one this element reported. Identity, not count:
/// swapping a query for a different one with the same number of hits is still a
/// new result.
fn resolve_highlight(
    cache: &mut HashMap<u64, HighlightCacheEntry>,
    tree: &RetainedTree,
    id: u64,
    value: &serde_json::Value,
    theme: &Theme,
    has_listener: bool,
) -> Option<(Arc<crate::text::HighlightContext>, Option<usize>)> {
    let set = crate::text::HighlightSet::parse(value, theme)?;
    // `search_revision`, NOT `subtree_revision`: `highlight` is a custom prop,
    // so the general revision moves on every keystroke and this cache would
    // never hit for the one case it exists for.
    let revision = tree.elements.get(&id)?.search_revision;
    let matcher_hash = set.matcher_hash();

    let cached = cache.get(&id).filter(|entry| entry.revision == revision && entry.matcher_hash == matcher_hash);
    let context = match cached {
        // Nothing moved at all. Returning the same `Arc` keeps the whole
        // subtree's inherited value identical, which the cache tests assert.
        Some(entry) if entry.context.set == set => entry.context.clone(),
        // Same matches, different colours or find cursor: reuse the located
        // matches and swap only the spec. No text is scanned.
        Some(entry) => {
            let context = Arc::new(crate::text::HighlightContext {
                declaration: id,
                set,
                matches: entry.context.matches.clone(),
            });
            cache.get_mut(&id)?.context = context.clone();
            context
        }
        None => {
            let groups = match cache.get(&id) {
                Some(entry) if entry.revision == revision => entry.groups.clone(),
                _ => Arc::new(crate::text::GroupList::collect(tree, id)),
            };
            let context = Arc::new(crate::text::HighlightContext {
                declaration: id,
                matches: Arc::new(crate::text::search::resolve(&groups, &set)),
                set,
            });
            let reported = cache.get(&id).and_then(|entry| entry.reported);
            cache
                .insert(id, HighlightCacheEntry { revision, groups, matcher_hash, context: context.clone(), reported });
            context
        }
    };

    if !has_listener {
        return Some((context, None));
    }
    let identity = context.matches.identity();
    let entry = cache.get_mut(&id)?;
    if entry.reported == Some(identity) {
        return Some((context, None));
    }
    entry.reported = Some(identity);
    let total = context.matches.total;
    Some((context, Some(total)))
}

/// A view over `tree`, with no event callback, for measuring what a frame of the renderer costs.
/// The app builds its views through `GpuixRenderer`.
#[doc(hidden)]
pub fn perf_view(tree: Arc<Mutex<RetainedTree>>) -> GpuixView {
    GpuixView::new(tree, None, "frame cost".to_string(), SharedSelection::default())
}

impl GpuixView {
    pub(crate) fn new(
        tree: Arc<Mutex<RetainedTree>>,
        event_callback: Option<EventCallback>,
        window_title: String,
        selection: SharedSelection,
    ) -> Self {
        Self {
            tree,
            event_callback,
            window_title,
            applied_title: None,
            window_key_down: false,
            window_key_up: false,
            window_key_event_id: 0,
            window_selection_change: false,
            window_selection_event_id: 0,
            reported_selection: None,
            focus_handles: HashMap::new(),
            pending_focus_element: None,
            focus_subscriptions: HashMap::new(),
            custom_registry: CustomElementRegistry::with_defaults(),
            scroll_handles: HashMap::new(),
            motion_states: HashMap::new(),
            motion_grid: None,
            selection,
            virtual_lists: HashMap::new(),
            selection_drag_position: None,
            selection_scroll_list: None,
            selection_scroll_task: None,
            clock: crate::automation::AutomationClock::new(),
            highlights: HashMap::new(),
            paint_reports: SharedPaintReports::default(),
            boundaries: HashMap::new(),
            frame: FrameInputs {
                now: web_time::Instant::now(),
                keyboard_focus: None,
                focus_dim: true,
                cache_all: true,
            },
            loop_costs: Rc::default(),
            slowed: HashSet::new(),
        }
    }

    pub(crate) fn set_live_image(
        &mut self,
        id: u64,
        image: std::sync::Arc<gpui::RenderImage>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> std::result::Result<(), String> {
        {
            let tree = self.tree.lock().unwrap();
            let element = tree.elements.get(&id).ok_or_else(|| format!("Unknown element id {id}"))?;
            if element.element_type != "img" {
                return Err(format!("setImage is only valid on <img>, got <{}>", element.element_type));
            }
        }
        let previous = self.custom_registry.set_live_image(id, image)?;
        if let Some(previous) = previous {
            window.drop_image(previous).ok();
        }
        cx.notify();
        window.refresh();
        Ok(())
    }

    pub(crate) fn set_image_pixels(
        &mut self,
        id: u64,
        width: u32,
        height: u32,
        bytes: Vec<u8>,
        format: crate::custom_elements::img::PixelFormat,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> std::result::Result<(), String> {
        let image = crate::custom_elements::img::render_image_from_pixels(width, height, bytes, format)?;
        self.set_live_image(id, image, window, cx)
    }

    pub(crate) fn set_encoded_image(
        &mut self,
        id: u64,
        bytes: Vec<u8>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> std::result::Result<(), String> {
        let image = crate::custom_elements::img::render_image_from_encoded(bytes, cx.svg_renderer())?;
        self.set_live_image(id, image, window, cx)
    }

    /// Adds the fonts in `bytes` to the text system and repaints, so text that
    /// names one of them picks it up.
    pub(crate) fn register_font(
        &mut self,
        bytes: Vec<u8>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> std::result::Result<(), String> {
        check_font(&bytes)?;
        window.text_system().add_fonts(vec![std::borrow::Cow::Owned(bytes)]).map_err(|error| error.to_string())?;
        cx.notify();
        window.refresh();
        Ok(())
    }

    pub(crate) fn scroll_element_into_view(&self, id: u64) -> bool {
        let Some((scroller_id, child_index)) = self.scroll_target(id) else {
            return false;
        };
        if self.scroll_virtual_list_to_item(scroller_id, child_index, 0.0) {
            return true;
        }
        if let Some(handle) = self.scroll_handles.get(&scroller_id) {
            handle.scroll_to_item(child_index);
            return true;
        }
        false
    }

    fn scroll_target(&self, id: u64) -> Option<(u64, usize)> {
        let tree = self.tree.lock().unwrap();
        let mut current = id;
        loop {
            let element = tree.elements.get(&current)?;
            let parent_id = element.parent?;
            let parent = tree.elements.get(&parent_id)?;
            let index = parent.children.iter().position(|child| *child == current)?;
            if let Some(entry) = self.virtual_lists.get(&parent_id) {
                return Some((parent_id, entry.logical_index_of(current)?));
            }
            if self.scroll_handles.contains_key(&parent_id) {
                return Some((parent_id, index));
            }
            current = parent_id;
        }
    }

    pub(crate) fn set_selection_change_listener(&mut self, enabled: bool, event_id: u64) {
        self.window_selection_change = enabled;
        self.window_selection_event_id = event_id;
        self.reported_selection = None;
    }

    /// Emit `selectionChange` when the selected range set changed this frame.
    ///
    /// Reads the same `SelectionState` as `getSelectedText`. Keyed on identity
    /// so an unchanged frame does not emit. Empty on mount is not a change.
    /// `reported_selection` is written only when an event is queued, so adding
    /// `onSelectionChange` later still reports a live selection.
    fn emit_selection_change(&mut self, callback: &Option<EventCallback>) {
        if !self.window_selection_change {
            return;
        }
        let selection = self.selection.lock();
        let identity = selection.identity();
        if self.reported_selection == Some(identity) {
            return;
        }
        if identity == 0 && self.reported_selection.is_none() {
            return;
        }
        let value = selection.selected_text();
        drop(selection);
        self.reported_selection = Some(identity);
        emit_event_full(callback, self.window_selection_event_id, "selectionChange", |payload| {
            payload.value = value;
        });
    }

    /// A cached subtree's build (`crate::boundary`), from its view's render: after the root's,
    /// within the same frame, so it builds with what the root render saw.
    /// `GpuixRenderer::get_compartment_stats`.
    pub fn compartment_stats(&self) -> Vec<CompartmentStats> {
        let mut by_name: Vec<CompartmentStats> = Vec::new();
        for boundary in self.boundaries.values() {
            let shared = &boundary.shared;
            if !shared.budgeted.get() {
                continue;
            }
            let name = shared.name.borrow().clone();
            let tally = shared.take_tally();
            let at = match by_name.iter().position(|s| s.name == name) {
                Some(at) => at,
                None => {
                    by_name.push(CompartmentStats {
                        name,
                        count: 0,
                        renders: 0,
                        deferred: 0,
                        total_ms: 0.0,
                        max_ms: 0.0,
                        last_ms: 0.0,
                        held: false,
                    });
                    by_name.len() - 1
                }
            };
            let stats = &mut by_name[at];
            let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
            stats.count += 1;
            stats.renders += tally.renders;
            stats.deferred += tally.deferred;
            stats.total_ms += ms(tally.total);
            stats.max_ms = stats.max_ms.max(ms(tally.max));
            stats.last_ms = stats.last_ms.max(ms(shared.cost()));
            stats.held |= shared.held();
        }
        by_name.sort_by(|a, b| b.total_ms.total_cmp(&a.total_ms));
        by_name
    }

    /// When the endless animations that moved (`looping`) are sampled next: at `after`, or later
    /// if their frames cost more than `LOOP_SHARE` of the time. Each one held to half its rate or
    /// less is told so once (`motionThrottled`, with the compartment it is in).
    fn pace_loops(
        &mut self,
        tree: &RetainedTree,
        looping: &[(u64, Duration)],
        after: Duration,
        callback: &Option<EventCallback>,
    ) -> Duration {
        let mut costs: Vec<Duration> = self.loop_costs.borrow().iter().copied().collect();
        costs.sort();
        let cost = costs.get(costs.len() / 2).copied().unwrap_or_default();
        let budget = cost.div_f64(LOOP_SHARE);
        for &(id, step) in looping {
            if budget < step * 2 || !self.slowed.insert(id) {
                continue;
            }
            let Some(element) = tree.elements.get(&id) else { continue };
            if !element.events().contains("motionThrottled") {
                continue;
            }
            let name = enclosing_compartment(tree, id);
            emit_event_full(callback, id, "motionThrottled", |payload| payload.value = name);
        }
        after.max(budget)
    }

    pub(crate) fn build_boundary(
        &mut self,
        shared: &crate::boundary::Shared,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use gpui::IntoElement;

        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let Some(element) = tree.elements.get(&shared.id) else {
            return gpui::Empty.into_any_element();
        };
        shared.built_revision.set(element.subtree_revision);
        let inherited = shared.inherited.borrow().clone().unwrap_or_else(|| Inherited::root(&Theme::dark()));
        let callback = self.event_callback.clone();
        let frame = self.frame.clone();
        let mut highlight_events = Vec::new();
        // A compartment drawn in place is built as it is (sized with everything else); a cached
        // subtree fills the box its view was laid out at.
        let in_place = shared.in_place.get();
        let mut ctx = BuildCtx {
            tree: &tree,
            event_callback: &callback,
            focus_handles: &self.focus_handles,
            scroll_handles: &mut self.scroll_handles,
            custom_registry: &mut self.custom_registry,
            virtual_lists: &mut self.virtual_lists,
            motion_states: &mut self.motion_states,
            now: frame.now,
            selection: self.selection.clone(),
            inherited,
            highlights: &mut self.highlights,
            paint_reports: &self.paint_reports,
            highlight_events: &mut highlight_events,
            keyboard_focus: frame.keyboard_focus,
            boundaries: &mut self.boundaries,
            cache_root: (!in_place).then_some(shared.id),
            measuring: in_place.then_some(shared.id),
            cache_all: frame.cache_all,
        };
        let built = build_element(shared.id, &mut ctx, window, cx);
        drop(tree);
        emit_highlight_events(&callback, &highlight_events);
        built
    }

    fn build_virtual_child(
        &mut self,
        list_id: u64,
        index: usize,
        expected_child_id: u64,
        inherited: Inherited,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use gpui::prelude::*;

        let row_focus_handle = self.virtual_lists.get_mut(&list_id).and_then(|entry| {
            entry.seen_rows.insert(expected_child_id);
            (entry.child_at(index) == Some(expected_child_id))
                .then(|| {
                    index
                        .checked_sub(entry.window_start)
                        .and_then(|offset| entry.row_focus_handles.get(offset).cloned())
                })
                .flatten()
                .flatten()
        });

        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let window_start = self.virtual_lists.get(&list_id).map(|entry| entry.window_start).unwrap_or(0);
        let child_matches = tree
            .elements
            .get(&list_id)
            .and_then(|list| index.checked_sub(window_start).and_then(|offset| list.children.get(offset)))
            == Some(&expected_child_id);
        if !child_matches {
            let height =
                self.virtual_lists.get(&list_id).and_then(|entry| entry.config.estimated_item_height).unwrap_or(1.0);
            return unmounted_virtual_row(height);
        }

        let callback = self.event_callback.clone();
        let now = self.clock.now();
        let motion = sync_motion_states(&tree, &mut self.motion_states, &mut self.motion_grid, now, false);
        let mut highlight_events = Vec::new();

        // Re-resolve against the tree as it is NOW. gpui calls this during
        // layout and prepaint, after the root render returned, and on Windows
        // and Linux the Node thread can commit new text in between. Reusing the
        // captured ranges would paint a wash over the wrong glyphs, or at a byte
        // offset that is no longer a character boundary.
        let mut inherited = inherited;
        if let Some(declaration) = inherited.highlight.as_ref().map(|ctx| ctx.declaration) {
            inherited.highlight = tree
                .elements
                .get(&declaration)
                .and_then(|element| element.custom_props().get("highlight"))
                .and_then(|value| {
                    resolve_highlight(&mut self.highlights, &tree, declaration, value, &Theme::dark(), false)
                })
                .map(|(context, _)| context);
        }

        let keyboard_focus = self.keyboard_focus_path(&tree, window, cx);
        let mut build_ctx = BuildCtx {
            tree: &tree,
            event_callback: &callback,
            focus_handles: &self.focus_handles,
            scroll_handles: &mut self.scroll_handles,
            custom_registry: &mut self.custom_registry,
            virtual_lists: &mut self.virtual_lists,
            motion_states: &mut self.motion_states,
            now,
            selection: self.selection.clone(),
            inherited,
            highlights: &mut self.highlights,
            paint_reports: &self.paint_reports,
            highlight_events: &mut highlight_events,
            keyboard_focus,
            boundaries: &mut self.boundaries,
            cache_root: None,
            measuring: None,
            cache_all: self.frame.cache_all,
        };
        let child = build_element(expected_child_id, &mut build_ctx, window, cx);
        emit_highlight_events(&callback, &highlight_events);
        emit_motion_settled(&callback, &tree, &motion.settled);
        if motion.active {
            window.request_animation_frame();
        } else if let Some(after) = motion.wake {
            // Only capped tracks move: a timed repaint at their next sample, not every frame.
            let after = self.pace_loops(&tree, &motion.looping, after, &callback);
            crate::schedule::repaint_in(after, window, cx);
        }
        let Some(focus_handle) = row_focus_handle else {
            return child;
        };
        gpui::div()
            .id(gpui::SharedString::from(format!("__gpuix_virtual_row_{}_{}", list_id, expected_child_id)))
            .w_full()
            .track_focus(&focus_handle)
            .child(child)
            .into_any_element()
    }

    pub(crate) fn scroll_virtual_list_to_item(&self, id: u64, index: usize, offset_in_item: f32) -> bool {
        if !self.virtual_lists.contains_key(&id) {
            return false;
        }
        queue_virtual_list_scroll(id, index, offset_in_item);
        emit_event_full(&self.event_callback, id, "visibleRange", |payload| {
            payload.start_index = Some(index as f64);
            payload.end_index = Some((index + 1) as f64);
        });
        true
    }

    /// The list's logical scroll anchor as
    /// `[item_ix, offset_in_item_px, viewport_height_px]`.
    ///
    /// `item_ix == item count` is gpui's at-end sentinel (a bottom-aligned
    /// list resting at its very end); the viewport height is what lets a
    /// caller convert that into a position relative to the trailing rows.
    #[cfg(feature = "test-support")]
    pub(crate) fn virtual_list_scroll_top(&self, id: u64) -> Option<[f64; 3]> {
        let state = &self.virtual_lists.get(&id)?.state;
        let top = state.logical_scroll_top();
        Some([
            top.item_ix as f64,
            f64::from(f32::from(top.offset_in_item)),
            f64::from(f32::from(state.viewport_bounds().size.height)),
        ])
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn set_virtual_list_offset(&self, id: u64, x: f32, y: f32) -> bool {
        let Some(entry) = self.virtual_lists.get(&id) else {
            return false;
        };
        entry.state.set_offset_from_scrollbar(gpui::point(gpui::px(x), gpui::px(y)));
        true
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn virtual_list_offset(&self, id: u64) -> Option<[f64; 2]> {
        let offset = self.virtual_lists.get(&id)?.state.scroll_px_offset_for_scrollbar();
        Some([f64::from(f32::from(offset.x)), f64::from(f32::from(offset.y))])
    }

    pub(crate) fn reveal_virtual_list_ancestor(&self, id: u64) -> bool {
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let mut current = id;
        let location = loop {
            let Some(parent_id) = tree.elements.get(&current).and_then(|element| element.parent) else {
                break None;
            };
            if let Some(entry) = self.virtual_lists.get(&parent_id) {
                break entry.logical_index_of(current).map(|index| (parent_id, index));
            }
            current = parent_id;
        };
        drop(tree);

        let Some((list_id, index)) = location else {
            return false;
        };
        self.scroll_virtual_list_to_item(list_id, index, 0.0)
    }
}

/// Everything `build_element` threads through the tree.
///
/// Split into a struct because the recursion needs eight-plus shared references
/// and adding one more to every call site is how this file rots. `window` and
/// `cx` stay separate parameters: they are `&mut` and gpui reborrows them.
pub(crate) struct BuildCtx<'a> {
    pub tree: &'a RetainedTree,
    pub event_callback: &'a Option<EventCallback>,
    pub focus_handles: &'a HashMap<u64, gpui::FocusHandle>,
    pub scroll_handles: &'a mut HashMap<u64, gpui::ScrollHandle>,
    pub custom_registry: &'a mut CustomElementRegistry,
    virtual_lists: &'a mut HashMap<u64, VirtualListEntry>,
    pub motion_states: &'a mut HashMap<u64, crate::motion::MotionState>,
    pub now: web_time::Instant,
    pub selection: SharedSelection,
    /// Inherited text state, resolved the way CSS inherits it. The renderer's
    /// own theme only seeds the root selection wash; custom elements resolve
    /// their own theme from their `theme` prop.
    pub inherited: Inherited,
    /// Persistent `highlight` caches, keyed by the declaring element.
    highlights: &'a mut HashMap<u64, HighlightCacheEntry>,
    paint_reports: &'a SharedPaintReports,
    /// `onHighlight` payloads queued during the build.
    ///
    /// Never emitted inline: a handler that calls `setState` repaints, which
    /// would re-enter the build and emit again. They are flushed once the root
    /// build has returned.
    highlight_events: &'a mut Vec<(u64, usize)>,
    /// See `GpuixView::keyboard_focus_path`.
    pub keyboard_focus: Option<Arc<[u64]>>,
    /// The `cache` elements (`crate::boundary`).
    boundaries: &'a mut HashMap<u64, crate::boundary::Boundary>,
    /// The `cache` element this build is the subtree of: built here, not embedded again.
    cache_root: Option<u64>,
    /// The compartment's element being built in place to measure it (`crate::boundary`): built
    /// as it is, not as a boundary.
    measuring: Option<u64>,
    /// Every cached subtree met is drawn again.
    cache_all: bool,
}

/// Style properties that cascade into descendants.
///
/// Not `Copy`: `highlight` holds an `Arc`. Every call site must clone
/// explicitly, including the deferred `build_virtual_child` callback, which gpui
/// may run more than once per frame.
#[derive(Clone)]
pub(crate) struct Inherited {
    /// False once an ancestor sets `userSelect: "none"`.
    pub selectable: bool,
    /// Selection wash colour for this subtree.
    pub selection_wash: gpui::Hsla,
    /// The nearest ancestor's `highlight`, resolved. `None` in every app that
    /// does not use search. It carries the declaring element id, which is what
    /// a virtual-list row re-resolves against: that row is built after the root
    /// render returns, and on Windows and Linux the Node thread can edit text
    /// in between, so a stale range would paint over the wrong glyphs.
    pub highlight: Option<Arc<crate::text::HighlightContext>>,
    /// True below a `pointerEvents: "none"` until a `"auto"` turns it back, as CSS inherits it:
    /// a box drawn there with a fill blocks no click to what is behind it. Not inherited, the
    /// preview inside a card's `"none"` picture took the click the card was waiting for.
    pub pointer_events_none: bool,
    /// The nearest `color` at or above this element, as CSS inherits it. gpui cascades it to text
    /// on its own while painting, but an `<svg>` tints with its own style only: without this an
    /// icon inside a coloured button drew the fixed fallback, white on a light theme.
    pub color: Option<gpui::Rgba>,
}

/// Opacity multiplier for every other focusable element while a control has
/// keyboard focus. Paint only, so it moves nothing.
const KEYBOARD_FOCUS_DIM: f32 = 0.4;

/// Keyboard focus look for a focusable element, applied right after
/// `track_focus` (gpui applies focus refinements only to a tracked element).
///
/// Two independent states. `focusVisible` styles the element while it has
/// keyboard focus, like CSS `:focus-visible`. While another control has
/// keyboard focus, the element dims unless its style sets
/// `keyboardFocusDim: false`; that also shows at a glance where Tab can go.
pub(crate) fn apply_focus_visible<E: gpui::InteractiveElement + gpui::Styled>(
    mut el: E,
    id: u64,
    style: Option<&StyleDesc>,
    keyboard_focus: Option<&[u64]>,
) -> E {
    // gpui runs the refinement eagerly, so borrowing the style is fine.
    if let Some(declared) = style.and_then(|style| style.focus_visible.as_deref()) {
        el = el.focus_visible(|refinement| apply_styles(refinement, declared));
    }
    let opted_out = style.and_then(|style| style.keyboard_focus_dim) == Some(false);
    match keyboard_focus {
        // Opacity applies to the subtree, so the focused element's own
        // focusable ancestors must stay at full opacity too.
        Some(path) if !opted_out && !path.contains(&id) => {
            let base = style.and_then(|style| style.opacity).unwrap_or(1.0) as f32;
            el.opacity(base * KEYBOARD_FOCUS_DIM)
        }
        _ => el,
    }
}

impl Inherited {
    fn root(theme: &Theme) -> Self {
        let mut wash = theme.accent;
        wash.a = 0.35;
        Self { selectable: true, selection_wash: wash, highlight: None, pointer_events_none: false, color: None }
    }

    /// Apply the inheritable parts of `style` for the subtree below it.
    fn descend(mut self, style: Option<&StyleDesc>) -> Self {
        let Some(style) = style else { return self };
        match style.user_select.as_deref() {
            Some("none") => self.selectable = false,
            Some("text") | Some("auto") => self.selectable = true,
            _ => {}
        }
        match style.pointer_events.as_deref() {
            Some("none") => self.pointer_events_none = true,
            Some("auto") => self.pointer_events_none = false,
            _ => {}
        }
        if let Some(color) = style.selection_color.as_deref().and_then(crate::color::parse_color_rgba) {
            self.selection_wash = color.into();
        }
        if let Some(color) = style.color.as_deref().and_then(crate::color::parse_color_rgba) {
            self.color = Some(color);
        }
        self
    }
}

fn json_usize(value: &serde_json::Value) -> Option<usize> {
    value
        .as_u64()
        .map(|n| n as usize)
        .or_else(|| value.as_f64().filter(|n| *n >= 0.0 && n.is_finite()).map(|n| n as usize))
        .or_else(|| value.as_i64().filter(|n| *n >= 0).map(|n| n as usize))
}

fn window_start_from_element(element: &crate::retained_tree::RetainedElement) -> usize {
    element.custom_props().get("windowStart").and_then(json_usize).unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq)]
struct VirtualListConfig {
    alignment: gpui::ListAlignment,
    follow_tail: bool,
    overdraw: f32,
    estimated_item_height: Option<f32>,
    item_count: Option<usize>,
}

impl VirtualListConfig {
    fn from_element(element: &crate::retained_tree::RetainedElement) -> Self {
        let prop = |key: &str| element.custom_props().get(key);
        let alignment = match prop("alignment").and_then(serde_json::Value::as_str) {
            Some("bottom") => gpui::ListAlignment::Bottom,
            _ => gpui::ListAlignment::Top,
        };
        let follow_tail = prop("followTail").and_then(serde_json::Value::as_bool).unwrap_or(false);
        let overdraw = prop("overdraw").and_then(serde_json::Value::as_f64).unwrap_or(512.0).max(0.0) as f32;
        let estimated_item_height = prop("estimatedItemHeight")
            .and_then(serde_json::Value::as_f64)
            .filter(|height| *height > 0.0)
            .map(|height| height as f32);
        let item_count = estimated_item_height.and_then(|_| prop("itemCount").and_then(json_usize));
        Self { alignment, follow_tail, overdraw, estimated_item_height, item_count }
    }

    fn logical_count(self, child_len: usize) -> usize {
        self.item_count.unwrap_or(child_len)
    }

    fn make_state(self, item_count: usize, focus_handles: &[Option<gpui::FocusHandle>]) -> gpui::ListState {
        let mut state = gpui::ListState::new(item_count, self.alignment, gpui::px(self.overdraw));
        if focus_handles.len() == item_count {
            state.splice_focusable(0..item_count, focus_handles.iter().cloned());
        } else {
            state.splice_focusable(0..item_count, (0..item_count).map(|_| None));
        }
        if let Some(height) = self.estimated_item_height {
            state = state.with_uniform_item_height(gpui::px(height));
        }
        if self.follow_tail {
            state.set_follow_mode(gpui::FollowMode::Tail);
        }
        state
    }
}

struct VirtualListEntry {
    state: gpui::ListState,
    config: VirtualListConfig,
    window_start: usize,
    child_ids: Vec<u64>,
    child_revisions: Vec<u64>,
    row_focus_handles: Vec<Option<gpui::FocusHandle>>,
    seen_rows: HashSet<u64>,
}

impl VirtualListEntry {
    fn new(
        config: VirtualListConfig,
        window_start: usize,
        child_ids: Vec<u64>,
        child_revisions: Vec<u64>,
        row_focus_handles: Vec<Option<gpui::FocusHandle>>,
    ) -> Self {
        let item_count = config.logical_count(child_ids.len());
        let state = config.make_state(item_count, &row_focus_handles);
        if row_focus_handles.len() != item_count {
            for (offset, handle) in row_focus_handles.iter().enumerate() {
                if handle.is_some() {
                    let logical = window_start + offset;
                    if logical < item_count {
                        state.splice_focusable(logical..logical + 1, std::iter::once(handle.clone()));
                    }
                }
            }
        }
        Self { state, config, window_start, child_ids, child_revisions, row_focus_handles, seen_rows: HashSet::new() }
    }

    fn child_at(&self, logical_index: usize) -> Option<u64> {
        logical_index.checked_sub(self.window_start).and_then(|offset| self.child_ids.get(offset).copied())
    }

    fn logical_index_of(&self, child_id: u64) -> Option<usize> {
        self.child_ids.iter().position(|id| *id == child_id).map(|offset| self.window_start + offset)
    }

    fn sync(
        &mut self,
        config: VirtualListConfig,
        window_start: usize,
        child_ids: Vec<u64>,
        child_revisions: Vec<u64>,
        focusable_rows: &HashSet<u64>,
        cx: &mut gpui::Context<GpuixView>,
    ) {
        let focus_unchanged = self.child_ids == child_ids
            && self.row_focus_handles.len() == child_ids.len()
            && self
                .child_ids
                .iter()
                .zip(&self.row_focus_handles)
                .all(|(id, handle)| handle.is_some() == focusable_rows.contains(id));
        if self.config == config
            && self.window_start == window_start
            && focus_unchanged
            && self.child_revisions == child_revisions
        {
            return;
        }

        let old_rows: HashMap<u64, (u64, Option<gpui::FocusHandle>)> = self
            .child_ids
            .iter()
            .copied()
            .zip(self.child_revisions.iter().copied())
            .zip(self.row_focus_handles.iter().cloned())
            .map(|((id, revision), focus_handle)| (id, (revision, focus_handle)))
            .collect();
        let row_focus_handles: Vec<Option<gpui::FocusHandle>> = child_ids
            .iter()
            .map(|id| {
                focusable_rows.contains(id).then(|| {
                    old_rows
                        .get(id)
                        .and_then(|(_, focus_handle)| focus_handle.clone())
                        .unwrap_or_else(|| cx.focus_handle())
                })
            })
            .collect();
        if self.config != config {
            let scroll_top = self.state.logical_scroll_top();
            let should_follow = config.follow_tail && (!self.config.follow_tail || self.state.is_following_tail());
            let mut replacement = Self::new(config, window_start, child_ids, child_revisions, row_focus_handles);
            replacement.seen_rows = std::mem::take(&mut self.seen_rows);
            replacement.seen_rows.retain(|id| replacement.child_ids.contains(id));
            if !should_follow {
                replacement.state.scroll_to(scroll_top);
            }
            *self = replacement;
            return;
        }

        // gpui anchors a list on a logical item, so splicing rows in at the
        // front keeps the rows already on screen and pushes the new ones above
        // the viewport. A browser anchors too, but suppresses it at scrollTop 0,
        // so a prepend is visible. Match the browser: remember a list pinned to
        // the top and put it back after the splice.
        //
        // While the content is shorter than the viewport gpui re-anchors to
        // item 0 every layout, so the drift only appears once the list
        // overflows.
        //
        // The guard is `is_following_tail()`, not `config.follow_tail`: a
        // following list that does not fill its viewport also ends layout
        // anchored at {0, 0}, and `scroll_to` would call `stop_following` on it.
        // Once the user scrolls up to the top, following is already stopped, so
        // a top-aligned `followTail` list still gets the browser behaviour.
        let top = self.state.logical_scroll_top();
        let was_pinned_to_top = matches!(config.alignment, gpui::ListAlignment::Top)
            && !self.state.is_following_tail()
            && top.item_ix == 0
            && top.offset_in_item <= gpui::px(0.0);

        // A windowed list's children are a sliding viewport. Splicing by
        // child position would treat a scroll as a rewrite of items 0..N.
        if config.item_count.is_none() && self.child_ids != child_ids {
            let prefix = self.child_ids.iter().zip(&child_ids).take_while(|(old, new)| old == new).count();
            let suffix = self.child_ids[prefix..]
                .iter()
                .rev()
                .zip(child_ids[prefix..].iter().rev())
                .take_while(|(old, new)| old == new)
                .count();
            self.state.splice_focusable(
                prefix..self.child_ids.len().saturating_sub(suffix),
                row_focus_handles[prefix..row_focus_handles.len().saturating_sub(suffix)].iter().cloned(),
            );
            if let Some(height) = config.estimated_item_height {
                self.state = self.state.clone().with_uniform_item_height(gpui::px(height));
            }
        }

        for (offset, (&id, focus_handle)) in child_ids.iter().zip(&row_focus_handles).enumerate() {
            let logical = window_start + offset;
            let focusability_changed =
                old_rows.get(&id).is_some_and(|(_, old_handle)| old_handle.is_some() != focus_handle.is_some());
            if focusability_changed {
                self.state.splice_focusable(logical..logical + 1, std::iter::once(focus_handle.clone()));
            }
        }

        let mut changed_start = None;
        for (offset, (&id, &revision)) in child_ids.iter().zip(&child_revisions).enumerate() {
            let logical = window_start + offset;
            let changed = old_rows.get(&id).is_some_and(|(old_revision, _)| *old_revision != revision);
            match (changed_start, changed) {
                (None, true) => changed_start = Some(logical),
                (Some(start), false) => {
                    self.state.remeasure_items(start..logical);
                    changed_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = changed_start {
            self.state.remeasure_items(start..window_start + child_ids.len());
        }
        self.remeasure_unknown_rows(window_start, &child_ids, &old_rows);
        if was_pinned_to_top {
            self.state.scroll_to(gpui::ListOffset::default());
        }

        self.window_start = window_start;
        self.child_ids = child_ids;
        self.child_revisions = child_revisions;
        self.row_focus_handles = row_focus_handles;
    }

    fn remeasure_unknown_rows(
        &mut self,
        window_start: usize,
        child_ids: &[u64],
        known: &HashMap<u64, (u64, Option<gpui::FocusHandle>)>,
    ) {
        let mut range_start = None;
        for (offset, id) in child_ids.iter().enumerate() {
            let logical = window_start + offset;
            let is_new = !known.contains_key(id);
            match (range_start, is_new) {
                (None, true) => range_start = Some(logical),
                (Some(start), false) => {
                    self.state.remeasure_items(start..logical);
                    range_start = None;
                }
                _ => {}
            }
        }
        if let Some(start) = range_start {
            self.state.remeasure_items(start..window_start + child_ids.len());
        }
    }
}

impl GpuixView {
    pub(crate) fn request_focus(&mut self, id: u64, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        self.reveal_virtual_list_ancestor(id);
        if let Some(handle) = self.focus_handles.get(&id) {
            self.pending_focus_element = None;
            handle.focus(window, cx);
        } else {
            self.pending_focus_element = Some(PendingFocus::new(id, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn focused_element_id(&self, window: &gpui::Window) -> Option<u64> {
        self.focus_handles.iter().find_map(|(id, handle)| handle.is_focused(window).then_some(*id))
    }

    fn descendant_ids(&self, ancestor: u64) -> HashSet<u64> {
        let tree = self.tree.lock().unwrap();
        let mut ids = HashSet::new();
        fn walk(tree: &RetainedTree, id: u64, ids: &mut HashSet<u64>) {
            ids.insert(id);
            let Some(element) = tree.elements.get(&id) else {
                return;
            };
            for child in &element.children {
                walk(tree, *child, ids);
            }
        }
        walk(&tree, ancestor, &mut ids);
        ids
    }

    fn focus_among(&self, ancestor: u64, forward: bool, window: &mut gpui::Window, cx: &mut gpui::App) {
        let ids = self.descendant_ids(ancestor);
        let allowed: HashSet<gpui::FocusId> =
            self.focus_handles.iter().filter_map(|(id, handle)| ids.contains(id).then_some(handle.id())).collect();
        if forward {
            window.focus_next_among(|id| allowed.contains(id), cx);
        } else {
            window.focus_prev_among(|id| allowed.contains(id), cx);
        }
    }

    pub(crate) fn focus_next_within(&self, ancestor: u64, window: &mut gpui::Window, cx: &mut gpui::App) {
        self.focus_among(ancestor, true, window, cx);
    }

    pub(crate) fn focus_previous_within(&self, ancestor: u64, window: &mut gpui::Window, cx: &mut gpui::App) {
        self.focus_among(ancestor, false, window, cx);
    }

    fn on_selection_mouse_move(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut gpui::Context<Self>) {
        if !self.selection.lock().is_dragging() {
            self.stop_selection_scroll();
            return;
        }
        let list_id = self
            .selection_scroll_list
            .filter(|id| {
                self.virtual_lists.get(id).is_some_and(|entry| {
                    let bounds = entry.state.viewport_bounds();
                    position.x >= bounds.left() && position.x <= bounds.right()
                })
            })
            .or_else(|| {
                self.virtual_lists
                    .iter()
                    .find(|(_, entry)| entry.state.viewport_bounds().contains(&position))
                    .map(|(id, _)| *id)
            });
        let Some(list_id) = list_id else {
            self.stop_selection_scroll();
            return;
        };
        self.selection_drag_position = Some(position);
        self.selection_scroll_list = Some(list_id);
        self.schedule_selection_scroll(cx);
    }

    fn stop_selection_scroll(&mut self) {
        self.selection_drag_position = None;
        self.selection_scroll_list = None;
        self.selection_scroll_task = None;
    }

    fn schedule_selection_scroll(&mut self, cx: &mut gpui::Context<Self>) {
        if self.selection_scroll_task.is_some() || !self.selection.lock().is_dragging() {
            return;
        }
        let (Some(position), Some(list_id)) = (self.selection_drag_position, self.selection_scroll_list) else {
            return;
        };
        let Some(entry) = self.virtual_lists.get(&list_id) else {
            return;
        };
        if selection_scroll_step(entry.state.viewport_bounds(), position) == 0.0 {
            return;
        }
        self.selection_scroll_task = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(Duration::from_millis(SELECTION_SCROLL_TICK_MS)).await;
            if let Err(error) = view.update(cx, |view, cx| {
                view.selection_scroll_task = None;
                view.step_selection_scroll(cx);
            }) {
                log::debug!("selection scroll stopped after view teardown: {error}");
            }
        }));
    }

    fn step_selection_scroll(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.selection.lock().is_dragging() {
            self.stop_selection_scroll();
            return;
        }
        let (Some(position), Some(list_id)) = (self.selection_drag_position, self.selection_scroll_list) else {
            return;
        };
        let Some(entry) = self.virtual_lists.get(&list_id) else {
            self.stop_selection_scroll();
            return;
        };
        let step = selection_scroll_step(entry.state.viewport_bounds(), position);
        if step == 0.0 {
            return;
        }

        let before = entry.state.logical_scroll_top();
        let selection_moved = crate::text::paint::update_drag_at(&self.selection, position);
        entry.state.scroll_by(gpui::px(step));
        let after = entry.state.logical_scroll_top();
        let list_moved = after.item_ix != before.item_ix || after.offset_in_item != before.offset_in_item;
        if !selection_moved && !list_moved {
            self.stop_selection_scroll();
            return;
        }
        cx.notify();
        self.schedule_selection_scroll(cx);
    }

    /// Sync focus handles with the current element tree.
    /// Creates handles for new focusable elements, subscribes on_focus/on_blur,
    /// and cleans up handles for destroyed elements.
    /// The control holding focus while the user navigates with the keyboard,
    /// and its ancestors, or None. Text fields do not count: typing is keyboard
    /// input too, and dimming the window while someone types is noise. A mouse
    /// move or a focus change refreshes the window, so it is recomputed each
    /// frame.
    fn keyboard_focus_path(&self, tree: &RetainedTree, window: &gpui::Window, cx: &gpui::App) -> Option<Arc<[u64]>> {
        if !tree.keyboard_focus_dim() || !window.last_input_was_keyboard() {
            return None;
        }
        let focused = window.focused(cx)?.id();
        let (&id, _) = self.focus_handles.iter().find(|(_, handle)| handle.id() == focused)?;
        let element = tree.elements.get(&id)?;
        if matches!(element.element_type.as_str(), "input" | "textarea") {
            return None;
        }
        let path: Vec<u64> =
            std::iter::successors(Some(id), |id| tree.elements.get(id).and_then(|element| element.parent)).collect();
        Some(path.into())
    }

    fn sync_focus_handles(
        &mut self,
        tree: &RetainedTree,
        callback: &Option<EventCallback>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // Checked before this frame's autoFocus runs: an explicit request
        // outranks autoFocus, but not a Tab or click that came after it.
        if self
            .pending_focus_element
            .is_some_and(|pending| window.focused(cx).map(|handle| handle.id()) != pending.focused_at_request)
        {
            self.pending_focus_element = None;
        }
        // Only the focusable elements are visited. This runs every frame, and
        // the rest of the tree is orders of magnitude larger.
        //
        // Create handles for elements that need focus but don't have one yet.
        for &id in &tree.focusable_ids {
            let Some(element) = tree.elements.get(&id) else {
                continue;
            };
            let tab_index = element
                .tab_index()
                .or_else(|| matches!(element.element_type.as_str(), "input" | "textarea").then_some(0));

            if !self.focus_handles.contains_key(&id) {
                let handle = match tab_index {
                    Some(index) => cx.focus_handle().tab_index(index).tab_stop(index >= 0),
                    None => cx.focus_handle(),
                };
                // Focus once, at creation. Re-focusing every frame would
                // steal focus back from whatever the user clicked next.
                if element.auto_focus {
                    handle.focus(window, cx);
                }
                self.focus_handles.insert(id, handle);
            } else if let (Some(handle), Some(index)) = (self.focus_handles.get(&id).cloned(), tab_index) {
                self.focus_handles.insert(id, handle.tab_index(index).tab_stop(index >= 0));
            } else if let Some(handle) = self.focus_handles.get(&id).cloned() {
                self.focus_handles.insert(id, handle.tab_stop(false));
            }
        }

        // Keep the request until the handle exists. A frame can render before
        // the batch that creates the element arrives (the client's update lands
        // on a later tick; Windows and Linux render on their own thread).
        if let Some(handle) =
            self.pending_focus_element.and_then(|pending| self.focus_handles.get(&pending.id).cloned())
        {
            handle.focus(window, cx);
            self.pending_focus_element = None;
        }

        self.focus_subscriptions
            .retain(|(id, event), _| tree.elements.get(id).is_some_and(|element| element.events().contains(event)));
        for &id in &tree.focusable_ids {
            let (Some(element), Some(handle)) = (tree.elements.get(&id), self.focus_handles.get(&id).cloned()) else {
                continue;
            };
            let focus_key = (id, "focus".to_string());
            if element.events().contains("focus") && !self.focus_subscriptions.contains_key(&focus_key) {
                let callback = callback.clone();
                let subscription = cx.on_focus(&handle, window, move |_this, _window, _cx| {
                    emit_event_full(&callback, id, "focus", |_| {});
                });
                self.focus_subscriptions.insert(focus_key, subscription);
            }
            let blur_key = (id, "blur".to_string());
            if element.events().contains("blur") && !self.focus_subscriptions.contains_key(&blur_key) {
                let callback = callback.clone();
                let subscription = cx.on_blur(&handle, window, move |_this, _window, _cx| {
                    emit_event_full(&callback, id, "blur", |_| {});
                });
                self.focus_subscriptions.insert(blur_key, subscription);
            }
        }

        // Clean up handles for elements that no longer exist.
        self.focus_handles.retain(|id, _| tree.focusable_ids.contains(id));
    }
}

/// A `focusElement` request for an element whose focus handle does not exist
/// yet. It is dropped when focus moves any other way first (Tab, a click, a
/// newer request), so a late element never steals focus from the user.
#[derive(Clone, Copy)]
struct PendingFocus {
    id: u64,
    focused_at_request: Option<gpui::FocusId>,
}

impl PendingFocus {
    fn new(id: u64, window: &gpui::Window, cx: &gpui::App) -> Self {
        Self { id, focused_at_request: window.focused(cx).map(|handle| handle.id()) }
    }
}

/// `PUMI_TRACE=1`: when the first window first draws a tree (epoch ms, read beside the JS marks).
fn trace_first_render(elements: usize) {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if elements == 0
        || std::env::var_os("PUMI_TRACE").is_none()
        || DONE.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        return;
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0);
    eprintln!("pumi-trace render {ms:.1} {elements}");
}

impl gpui::Render for GpuixView {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::IntoElement;

        let started = std::time::Instant::now();
        let woken = crate::schedule::take_woken(window);
        if self.applied_title.as_deref() != Some(self.window_title.as_str()) {
            window.set_window_title(&self.window_title);
            self.applied_title = Some(self.window_title.clone());
        }
        trace_first_render(self.tree.lock().unwrap().elements.len());

        // Clone Arc so we don't borrow self.tree — frees self for focus_handles access.
        let tree_arc = self.tree.clone();
        let tree = tree_arc.lock().unwrap();
        let callback = self.event_callback.clone();

        // Sync focus handles before building elements.
        self.sync_focus_handles(&tree, &callback, window, cx);

        // Ensure custom element instances are destroyed when their IDs disappear.
        self.custom_registry.prune_missing(|id| tree.elements.contains_key(&id), window);

        // Clean up scroll handles for destroyed elements (IDs removed from tree).
        // Scrollability-based cleanup (element still exists but style changed
        // from scroll to non-scroll) is handled inside build_host_container().
        self.scroll_handles.retain(|id, _| tree.elements.contains_key(id));
        self.virtual_lists.retain(|id, _| tree.elements.contains_key(id));
        // Pruned by listener, not existence: dropping `onLayout` and adding it
        // back reports the size again, like a fresh observer.
        {
            let mut reports = self.paint_reports.borrow_mut();
            if !reports.sizes.is_empty() {
                reports
                    .sizes
                    .retain(|id, _| tree.elements.get(id).is_some_and(|element| element.events().contains("layout")));
            }
            if !reports.scrolls.is_empty() {
                reports.scrolls.retain(|id, _| {
                    tree.elements.get(id).is_some_and(|element| element.events().contains("scrollChange"))
                });
            }
        }
        // Build the element tree. custom_registry, focus_handles, and scroll_handles
        // are different fields of self, so Rust allows borrowing all simultaneously.
        let theme = Theme::dark();
        let now = self.clock.now();
        let motion = sync_motion_states(&tree, &mut self.motion_states, &mut self.motion_grid, now, true);
        self.slowed.retain(|id| tree.motion_ids.contains(id));
        // Pruned by DECLARATION, not existence: an element that drops its
        // `highlight` prop keeps living, and its cached group list holds a copy
        // of every string in its subtree.
        self.highlights.retain(|id, _| {
            tree.elements.get(id).is_some_and(|element| element.custom_props().contains_key("highlight"))
        });
        let mut highlight_events = Vec::new();
        let keyboard_focus = self.keyboard_focus_path(&tree, window, cx);

        // The cached subtrees (`crate::boundary`). One that is no longer `cache` is built in
        // place again; one under a moving element samples it, and every one is drawn again
        // when the keyboard focus moves (its rings and dimming are anywhere) or anything is
        // highlighted (matches are counted across the window, in paint order).
        self.boundaries.retain(|id, _| {
            tree.elements
                .get(id)
                .is_some_and(|element| is_cache_boundary(element) || is_compartment_root(&tree, element))
        });
        let focus_dim = tree.keyboard_focus_dim();
        let cache_all = !self.highlights.is_empty()
            || self.frame.keyboard_focus != keyboard_focus
            || self.frame.focus_dim != focus_dim;
        self.frame = FrameInputs { now, keyboard_focus: keyboard_focus.clone(), focus_dim, cache_all };
        if !cache_all && !self.boundaries.is_empty() {
            for &id in &motion.moving {
                let mut current = Some(id);
                while let Some(id) = current {
                    if let Some(boundary) = self.boundaries.get(&id) {
                        boundary.invalidate(window);
                    }
                    current = tree.elements.get(&id).and_then(|element| element.parent);
                }
            }
        }

        let result = match tree.root_id {
            Some(root_id) => {
                let mut ctx = BuildCtx {
                    tree: &tree,
                    event_callback: &callback,
                    focus_handles: &self.focus_handles,
                    scroll_handles: &mut self.scroll_handles,
                    custom_registry: &mut self.custom_registry,
                    virtual_lists: &mut self.virtual_lists,
                    motion_states: &mut self.motion_states,
                    now,
                    selection: self.selection.clone(),
                    inherited: Inherited::root(&theme),
                    highlights: &mut self.highlights,
                    paint_reports: &self.paint_reports,
                    highlight_events: &mut highlight_events,
                    keyboard_focus,
                    boundaries: &mut self.boundaries,
                    cache_root: None,
                    measuring: None,
                    cache_all,
                };
                build_element(root_id, &mut ctx, window, cx)
            }
            None => gpui::Empty.into_any_element(),
        };
        // Flushed after the root build so a `setState` in the handler cannot
        // re-enter this build.
        emit_highlight_events(&callback, &highlight_events);
        emit_motion_settled(&callback, &tree, &motion.settled);
        self.emit_selection_change(&callback);

        // The frame reset must paint BEFORE any text, so it is the first child of
        // the root wrapper. Without it the selection registry accumulates stale
        // entries across frames and a drag resolves against elements that are no
        // longer on screen.
        let result = {
            use gpui::prelude::*;
            let drag_move_view = cx.weak_entity();
            let drag_end_view = drag_move_view.clone();
            let root = gpui::div().size_full();
            with_window_menu_actions(root)
                .child(window_key_events(
                    callback.clone(),
                    self.window_key_down,
                    self.window_key_up,
                    self.window_key_event_id,
                ))
                .child(press_dispatch())
                .child(selection_frame_reset(
                    self.selection.clone(),
                    move |position, app| {
                        // AppKit (and a11y clicks) dispatch with GpuixView leased.
                        // Defer until that lease returns or a nested update panics.
                        let drag_move_view = drag_move_view.clone();
                        app.defer(move |app| {
                            drag_move_view.update(app, |view, cx| view.on_selection_mouse_move(position, cx)).ok();
                        });
                    },
                    move |app| {
                        let drag_end_view = drag_end_view.clone();
                        app.defer(move |app| {
                            drag_end_view.update(app, |view, _cx| view.stop_selection_scroll()).ok();
                        });
                    },
                ))
                .child(crate::automation::bounds_frame_reset())
                .child(crate::occlusion::frame_start())
                .child(result)
                .child(crate::occlusion::tree_end())
                .child(loop_frame_cost(started, woken && !motion.looping.is_empty(), self.loop_costs.clone()))
                .into_any_element()
        };

        // Sync scroll handles to thread_local so napi methods (scrollTo,
        // getScrollOffset) can access them without an App context.
        SCROLL_HANDLES.with(|cell| {
            let mut handles = cell.borrow_mut();
            handles.clear();
            for (&id, handle) in &self.scroll_handles {
                handles.insert(id, handle.clone());
            }
        });
        VIRTUAL_LIST_STATES.with(|cell| {
            let mut states = cell.borrow_mut();
            states.clear();
            for (&id, entry) in &self.virtual_lists {
                states.insert(id, entry.state.clone());
            }
        });
        // One-shot: a queued scroll for a list that did not build this frame
        // would otherwise fire on some later frame, against child indices that
        // no longer match what JS meant.
        PENDING_VIRTUAL_LIST_SCROLLS.with(|cell| cell.borrow_mut().clear());

        if motion.active {
            window.request_animation_frame();
        } else if let Some(after) = motion.wake {
            // Only capped tracks move: a timed repaint at their next sample, not every frame.
            let after = self.pace_loops(&tree, &motion.looping, after, &callback);
            crate::schedule::repaint_in(after, window, cx);
        }

        result
    }
}

// ── Element builders ─────────────────────────────────────────────────

pub(crate) fn build_element(
    id: u64,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::IntoElement;

    // Copy the tree reference out of `ctx`, so `element` borrows the tree and
    // not the build context, which the children below need mutably.
    let tree = ctx.tree;
    let Some(element) = tree.elements.get(&id) else {
        return gpui::Empty.into_any_element();
    };

    let animated_style = if element.custom_props().contains_key("motion") {
        ctx.motion_states.get(&id).and_then(|state| state.visible_style(ctx.now)).map(|style| {
            // `Arc<StyleDesc>` is shared, so the animated frame is applied to a
            // copy. Mutating through the pointer would restyle every element
            // that declared the same style.
            let mut resolved = element.style.as_deref().cloned().unwrap_or_default();
            style.apply_to(&mut resolved);
            resolved
        })
    } else {
        None
    };
    let style = animated_style.as_ref().or(element.style.as_deref());

    if ctx.cache_root != Some(id) && ctx.measuring != Some(id) {
        if is_compartment(element) {
            match compartment_child(tree, element) {
                Some(child) if is_compartment_root(tree, child) => {
                    return build_compartment(child, compartment_name(element), ctx, window, cx);
                }
                // Not a box it can be: it is not there, and what it holds is drawn in its place.
                Some(child) => return build_element(child.id, ctx, window, cx),
                None if element.children.is_empty() => return gpui::Empty.into_any_element(),
                // Several: drawn as the `<div>` it is.
                None => {}
            }
        }
        if is_cache_boundary(element) {
            return build_cache_boundary(element, style, ctx, window, cx);
        }
    }

    // Inheritable style resolves once here so both built-ins and custom
    // elements see the same cascade.
    let parent_inherited = ctx.inherited.clone();
    ctx.inherited = parent_inherited.clone().descend(style);

    // A `highlight` here replaces any ancestor's: the nearest declaration wins,
    // and `GroupList::collect` skips nested declarations so an ancestor never
    // resolves or counts matches that will not paint.
    if let Some(value) = element.custom_props().get("highlight") {
        let has_listener = element.events().contains("highlight");
        let resolved = resolve_highlight(ctx.highlights, ctx.tree, id, value, &Theme::dark(), has_listener);
        if let Some((_, Some(total))) = &resolved {
            ctx.highlight_events.push((id, *total));
        }
        ctx.inherited.highlight = resolved.map(|(context, _)| context);
    }

    let built = match element.element_type.as_str() {
        // `<text>` is a `<div>` that happens to carry a string. Its own builder
        // would make every interaction prop on the shared `Props` type
        // (onClick, hover, focus, tabIndex) type-check, register a JS listener,
        // and then silently do nothing.
        "div" | "text" => {
            ctx.custom_registry.destroy(id);
            build_host_container(element, style, ctx, window, cx)
        }
        "virtual-list" => {
            ctx.custom_registry.destroy(id);
            build_virtual_list(element, ctx, window, cx)
        }

        // Polymorphic dispatch for all custom elements.
        custom_type => {
            let custom_children: Vec<gpui::AnyElement> = element
                .children
                .iter()
                .copied()
                .filter(|child_id| ctx.tree.elements.contains_key(child_id))
                .map(|child_id| build_element(child_id, ctx, window, cx))
                .collect();
            let inherited = ctx.inherited.clone();
            let render_ctx = CustomRenderContext {
                id,
                events: element.events(),
                event_callback: ctx.event_callback,
                focus_handle: ctx.focus_handles.get(&id),
                keyboard_focus: ctx.keyboard_focus.clone(),
                style,
                children: custom_children,
                selection: ctx.selection.clone(),
                selectable: inherited.selectable,
                selection_wash: inherited.selection_wash,
                color: inherited.color,
                highlight_set: inherited.highlight.clone(),
                props: element.custom_props(),
            };
            ctx.custom_registry.render(custom_type, element.custom_props(), render_ctx, window, cx)
        }
    };

    ctx.inherited = parent_inherited;
    built
}

/// What a compartment is called (`compartment="owner/name"`), for its cost in the overlay.
fn compartment_name(element: &crate::retained_tree::RetainedElement) -> Option<String> {
    element.custom_props().get("compartment").and_then(|value| value.as_str()).map(str::to_owned)
}

/// Whether `element` is a `<div compartment>` (`true`, or its name: whose view it holds, for its
/// cost in the overlay). It draws nothing of its own; its one element is a boundary sized by its
/// contents (`crate::boundary`).
fn is_compartment(element: &crate::retained_tree::RetainedElement) -> bool {
    element.element_type == "div"
        && element
            .custom_props()
            .get("compartment")
            .is_some_and(|value| value.as_bool() == Some(true) || value.as_str().is_some_and(|name| !name.is_empty()))
}

/// A compartment's one element, when it holds exactly one.
fn compartment_child<'t>(
    tree: &'t crate::retained_tree::RetainedTree,
    element: &crate::retained_tree::RetainedElement,
) -> Option<&'t crate::retained_tree::RetainedElement> {
    let mut live = element.children.iter().filter_map(|id| tree.elements.get(id));
    let first = live.next()?;
    live.next().is_none().then_some(first)
}

/// Whether `element` is the one element of a compartment, and a box that can be one: a `<div>`
/// or `<text>` that does not move (a motion samples its style each frame).
fn is_compartment_root(
    tree: &crate::retained_tree::RetainedTree,
    element: &crate::retained_tree::RetainedElement,
) -> bool {
    matches!(element.element_type.as_str(), "div" | "text")
        && !element.custom_props().contains_key("motion")
        && element.parent.and_then(|parent| tree.elements.get(&parent)).is_some_and(|parent| {
            is_compartment(parent) && compartment_child(tree, parent).is_some_and(|child| child.id == element.id)
        })
}

/// A compartment's element (`crate::boundary`): built in place on a frame something under it
/// changed (or its box did), and measured; on the frames after, a cached view of the size it
/// measured, laid out from its own style otherwise.
fn build_compartment(
    element: &crate::retained_tree::RetainedElement,
    name: Option<String>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let owner = cx.weak_entity();
    let boundary =
        ctx.boundaries.entry(element.id).or_insert_with(|| crate::boundary::Boundary::new(element.id, owner, cx));
    let shared = boundary.shared.clone();
    let view = boundary.view.clone();
    if *shared.name.borrow() != name {
        *shared.name.borrow_mut() = name;
    }
    let inherited_changed =
        shared.inherited.borrow().as_ref().is_none_or(|last| !crate::boundary::same_inherited(last, &ctx.inherited));
    *shared.inherited.borrow_mut() = Some(ctx.inherited.clone());
    let changed = ctx.cache_all || inherited_changed || shared.built_revision.get() != element.subtree_revision;
    // A slow compartment whose tree changed before its turn is drawn as it was until then (its
    // view's wake brings it back here); one that must learn its size is built at once.
    let turn = if changed && !shared.needs_measure() { shared.next_turn(std::time::Instant::now()) } else { None };
    let early = turn.is_some();
    if let Some(at) = turn {
        shared.count_deferred();
        crate::boundary::wake_at(&shared, &view, at, window, cx);
    }
    if (changed && !early) || shared.needs_measure() {
        // Its view draws it in place, uncached, and what it draws is reused the frames after.
        return crate::boundary::BoundaryElement::measuring(view.recorded().into_any_element(), shared)
            .into_any_element();
    }

    let mut outer = gpui::div();
    if let Some(style) = element.style.as_deref() {
        outer = apply_styles(outer, style);
    }
    if element.style.as_deref().and_then(|style| style.position.as_deref()).is_none() {
        outer = outer.relative();
    }
    let layout = outer.style().clone();
    crate::boundary::BoundaryElement::budgeted(
        view.clone().cached_measured(layout, crate::boundary::measure(shared.clone())).into_any_element(),
        shared,
        view,
    )
    .into_any_element()
}

/// Whether `element` is drawn by a view of its own (`crate::boundary`): a `<div>` with `cache`.
fn is_cache_boundary(element: &crate::retained_tree::RetainedElement) -> bool {
    element.element_type == "div" && element.custom_props().get("cache").and_then(|value| value.as_bool()) == Some(true)
}

/// A `cache` element, as the build of its parent meets it: its view, laid out from its own style
/// alone (gpui does not look inside a cached view to size it), and drawn again this frame when
/// something under it changed since it last was.
fn build_cache_boundary(
    element: &crate::retained_tree::RetainedElement,
    style: Option<&StyleDesc>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let owner = cx.weak_entity();
    let boundary =
        ctx.boundaries.entry(element.id).or_insert_with(|| crate::boundary::Boundary::new(element.id, owner, cx));
    let inherited_changed = boundary
        .shared
        .inherited
        .borrow()
        .as_ref()
        .is_none_or(|last| !crate::boundary::same_inherited(last, &ctx.inherited));
    if ctx.cache_all || inherited_changed || boundary.shared.built_revision.get() != element.subtree_revision {
        boundary.invalidate(window);
    }
    *boundary.shared.inherited.borrow_mut() = Some(ctx.inherited.clone());

    let mut outer = gpui::div();
    if let Some(style) = style {
        outer = apply_styles(outer, style);
    }
    if style.and_then(|style| style.position.as_deref()).is_none() {
        outer = outer.relative();
    }
    let layout = outer.style().clone();
    crate::boundary::BoundaryElement::new(
        boundary.view.clone().cached(layout).into_any_element(),
        boundary.shared.clone(),
    )
    .into_any_element()
}

fn build_virtual_list(
    element: &crate::retained_tree::RetainedElement,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    let child_ids: Vec<u64> =
        element.children.iter().copied().filter(|child_id| ctx.tree.elements.contains_key(child_id)).collect();
    let child_revisions: Vec<u64> = child_ids
        .iter()
        .filter_map(|child_id| ctx.tree.elements.get(child_id).map(|child| child.subtree_revision))
        .collect();
    let focusable_rows: HashSet<u64> = ctx
        .focus_handles
        .keys()
        .filter_map(|element_id| virtual_row_ancestor(ctx.tree, element.id, *element_id))
        .collect();
    let focused_row = ctx
        .focus_handles
        .iter()
        .find_map(|(element_id, handle)| {
            handle.is_focused(window).then(|| virtual_row_ancestor(ctx.tree, element.id, *element_id)).flatten()
        })
        .or_else(|| {
            ctx.focus_handles.keys().find_map(|element_id| {
                ctx.tree
                    .elements
                    .get(element_id)
                    .is_some_and(|element| element.auto_focus)
                    .then(|| virtual_row_ancestor(ctx.tree, element.id, *element_id))
                    .flatten()
            })
        });
    let config = VirtualListConfig::from_element(element);
    let window_start = if config.item_count.is_some() { window_start_from_element(element) } else { 0 };
    let list_state = match ctx.virtual_lists.entry(element.id) {
        std::collections::hash_map::Entry::Occupied(mut entry) => {
            entry.get_mut().sync(config, window_start, child_ids.clone(), child_revisions, &focusable_rows, cx);
            let entry = entry.into_mut();
            if let Some(row_id) = focused_row.filter(|row_id| !entry.seen_rows.contains(row_id)) {
                if let Some(index) = entry.logical_index_of(row_id) {
                    entry.state.scroll_to(gpui::ListOffset { item_ix: index, offset_in_item: gpui::px(0.0) });
                }
            }
            entry.state.clone()
        }
        std::collections::hash_map::Entry::Vacant(entry) => {
            let row_focus_handles =
                child_ids.iter().map(|id| focusable_rows.contains(id).then(|| cx.focus_handle())).collect();
            let entry = entry.insert(VirtualListEntry::new(
                config,
                window_start,
                child_ids.clone(),
                child_revisions,
                row_focus_handles,
            ));
            if let Some(row_id) = focused_row {
                if let Some(index) = entry.logical_index_of(row_id) {
                    entry.state.scroll_to(gpui::ListOffset { item_ix: index, offset_in_item: gpui::px(0.0) });
                }
            }
            entry.state.clone()
        }
    };

    // Queued scrolls apply here, after `sync` spliced this frame's child
    // changes, so the indices JS computed against its committed child list are
    // the indices the splice-adjusted ListState sees.
    if let Some(offset) = PENDING_VIRTUAL_LIST_SCROLLS.with(|cell| cell.borrow_mut().remove(&element.id)) {
        list_state.scroll_to(offset);
    }

    if element.events().contains("visibleRange") {
        let callback = ctx.event_callback.clone();
        let list_id = element.id;
        list_state.set_scroll_handler(move |event, _window, _cx| {
            emit_event_full(&callback, list_id, "visibleRange", |payload| {
                payload.start_index = Some(event.visible_range.start as f64);
                payload.end_index = Some(event.visible_range.end as f64);
            });
        });
    }

    let list_id = element.id;
    // Cloned, not copied: gpui runs this processor once per requested row, so
    // the captured value must survive every call.
    let inherited = ctx.inherited.clone();
    let render_item = cx.processor(move |view, index: usize, window, cx| {
        let Some(entry) = view.virtual_lists.get(&list_id) else {
            return unmounted_virtual_row(1.0);
        };
        let Some(child_id) = entry.child_at(index) else {
            // Empty measures as 0 and poisons ListState. Keep the estimate.
            return unmounted_virtual_row(entry.config.estimated_item_height.unwrap_or(1.0));
        };
        view.build_virtual_child(list_id, index, child_id, inherited.clone(), window, cx)
    });
    let scroll_state = list_state.clone();
    let mut list = gpui::list(list_state, render_item)
        .with_sizing_behavior(gpui::ListSizingBehavior::Auto)
        .id(gpui::ElementId::Name(gpui::SharedString::from(format!("__gpuix_virtual_list_{}", element.id))));
    if let Some(style) = element.style.as_deref() {
        list = apply_styles(list, style);
    }
    let list = apply_accessibility(list, element.custom_props(), None).into_any_element();
    match element.style.as_deref().and_then(|style| ThumbStyle::of(style, false, true)) {
        Some(thumbs) => ListWithScrollbar { list, state: scroll_state, thumbs }.into_any_element(),
        None => list,
    }
}

fn unmounted_virtual_row(height: f32) -> gpui::AnyElement {
    use gpui::prelude::*;
    gpui::div().h(gpui::px(height.max(1.0))).w_full().into_any()
}

fn virtual_row_ancestor(tree: &RetainedTree, list_id: u64, element_id: u64) -> Option<u64> {
    let mut current = element_id;
    loop {
        let parent = tree.elements.get(&current)?.parent?;
        if parent == list_id {
            return Some(current);
        }
        current = parent;
    }
}

fn joined_text_content(tree: &RetainedTree, element: &crate::retained_tree::RetainedElement) -> Option<String> {
    if let Some(content) = element.content.as_deref().filter(|value| !value.is_empty()) {
        return Some(content.to_string());
    }
    let mut joined = String::new();
    for child_id in &element.children {
        let Some(child) = tree.elements.get(child_id) else {
            continue;
        };
        if child.element_type == "text" {
            if let Some(content) = child.content.as_deref() {
                joined.push_str(content);
            }
        }
    }
    (!joined.is_empty()).then_some(joined)
}

thread_local! {
    /// A primary press landed on something of its own (a button, a draggable, `appRegion:
    /// "no-drag"`): the drag region around it leaves the window where it is.
    static PRESS_CLAIMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A press in a drag region that moves the window once the mouse moves.
    static MOVE_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `appRegion` (as Electron's `-webkit-app-region`): `"drag"` moves the window by this element
/// (a press and a move, as macOS's title bar does; a double press zooms or minimizes it, as the
/// system's setting says), and `"no-drag"` keeps a part of one from it. Anything inside one that
/// takes presses itself (`onClick`, `onMouseDown`) is left out by itself.
fn apply_app_region(
    mut el: gpui::Stateful<gpui::Div>,
    element: &crate::retained_tree::RetainedElement,
    style: Option<&StyleDesc>,
) -> gpui::Stateful<gpui::Div> {
    use gpui::prelude::*;
    let region = style.and_then(|s| s.app_region.as_deref());
    let events = element.events();
    let takes_presses = events.contains("click") || events.contains("mouseDown") || events.contains("doubleClick");
    if region == Some("no-drag") || (takes_presses && region != Some("drag")) {
        // Bubble listeners run innermost first: this says so before the region's runs.
        el = el.on_mouse_down(gpui::MouseButton::Left, |_, _, _| PRESS_CLAIMED.with(|c| c.set(true)));
    }
    if region != Some("drag") {
        return el;
    }
    el.capture_any_mouse_down(|_, _, _| PRESS_CLAIMED.with(|c| c.set(false)))
        .on_mouse_down(gpui::MouseButton::Left, |event, window, _| {
            // Claimed by what is inside (or by a region inside this one, which has taken it): left be.
            if PRESS_CLAIMED.with(|c| c.replace(true)) {
                return;
            }
            if event.click_count == 2 {
                MOVE_ARMED.with(|c| c.set(false));
                window.titlebar_double_click();
            } else {
                MOVE_ARMED.with(|c| c.set(true));
            }
        })
        .on_mouse_move(|event, window, _| {
            if event.pressed_button == Some(gpui::MouseButton::Left) && MOVE_ARMED.with(|c| c.replace(false)) {
                window.start_window_move();
            }
        })
        .on_mouse_up(gpui::MouseButton::Left, |_, _, _| MOVE_ARMED.with(|c| c.set(false)))
}

/// The one builder for `<div>` and `<text>`.
///
/// Both get the same stable GPUI id, so gpui keeps their interactive element
/// state (hover, active, pointer capture, scroll, accessibility node) across
/// frames, and both wire the whole shared `Props` surface.
pub(crate) fn build_host_container(
    element: &crate::retained_tree::RetainedElement,
    style: Option<&StyleDesc>,
    ctx: &mut BuildCtx,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<GpuixView>,
) -> gpui::AnyElement {
    use gpui::prelude::*;

    // `ElementId::Integer` rather than a formatted name: host ids are already
    // unique per renderer, and every `<div>` and `<text>` builds one of these on
    // every frame, so the string allocation was pure overhead. Custom elements
    // use `ElementId::Name`, which is a different variant and cannot collide.
    let mut el = gpui::div().id(gpui::ElementId::Integer(element.id));

    if let Some(style) = style {
        el = apply_interactive_styles(el, style);

        // `ctx.inherited` has this element's own `pointerEvents` in it already.
        if crate::style::should_occlude(style) && !ctx.inherited.pointer_events_none {
            // BlockMouse (occlude) stops the hit test, so the parent scroller
            // never sees the wheel. HTML does not work that way: a wheel over
            // an absolutely positioned card still scrolls the ancestor. Only
            // `pointerEvents: "auto"` opts into stealing it. Everything else
            // uses BlockMouseExceptScroll.
            //
            // Absolute must not steal it: that would make a pannable canvas
            // impossible, as every absolutely placed item (a timeline clip, a
            // graph node) would end the hit test before the pan listener ran.
            // `<anchored>` still occludes through its own `occlude` prop, so
            // menus and tooltips are unaffected.
            el = if style.pointer_events.as_deref() == Some("auto") {
                el.occlude()
            } else {
                el.block_mouse_except_scroll()
            };
        }
    }

    // ── Overflow: scroll ─────────────────────────────────────────────
    // overflow_scroll() requires StatefulInteractiveElement (only on Stateful<Div>),
    // so we handle it here rather than in apply_styles (which takes E: Styled).
    //
    // CSS precedence: axis-specific props (overflowX/Y) override the shorthand
    // (overflow). E.g. { overflow: "scroll", overflowY: "hidden" } → scroll X only.
    //
    // overflow-x only works as a flex viewport. Default display is Block, so a
    // wide child fills the parent instead of overflowing. Zed's code-block path:
    // flex + min_w_0 on the scroller, flex_none on the child.
    let mut overflow_x_only = false;
    if let Some(style) = style {
        // Resolve each axis: axis-specific overrides shorthand.
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());

        let needs_scroll_x = resolved_x == Some("scroll");
        let needs_scroll_y = resolved_y == Some("scroll");

        if needs_scroll_x && needs_scroll_y {
            el = el.overflow_scroll();
            // GPUI zeroes the smaller of the two deltas by default, so one
            // diagonal wheel moves one axis. A browser moves both, and a
            // two-axis container is exactly where a user expects that.
            el.style().allow_concurrent_scroll = Some(true);
        } else if needs_scroll_x {
            overflow_x_only = true;
            el = el.flex().min_w_0().overflow_x_scroll().restrict_scroll_to_axis();
        } else if needs_scroll_y {
            // Restricted as the X-only case is: unrestricted, GPUI turns a
            // sideways swipe into a vertical scroll, so swiping a horizontal
            // strip inside a vertical page scrolled the page too. A browser
            // scrolls a Y-only box with vertical deltas only.
            el = el.overflow_y_scroll().restrict_scroll_to_axis();
        }

        // Attach a persistent ScrollHandle when scrolling is enabled.
        // The handle persists across renders (stored in GpuixView::scroll_handles)
        // so GPUI maintains the scroll offset between frames.
        if needs_scroll_x || needs_scroll_y {
            let handle = ctx.scroll_handles.entry(element.id).or_default();
            el = el.track_scroll(handle);
        } else {
            // Element is no longer scrollable — remove stale handle.
            ctx.scroll_handles.remove(&element.id);
        }
    } else {
        // No style at all — remove stale handle if it existed.
        ctx.scroll_handles.remove(&element.id);
    }

    // If a FocusHandle was pre-created for this element (by sync_focus_handles),
    // attach it via track_focus. This makes the element focusable — clicking it
    // or tabbing to it gives it keyboard focus. The handle persists across renders
    // because it's stored in GpuixView::focus_handles.
    if style.and_then(|style| style.position.as_deref()).is_none() {
        el = el.relative();
    }
    el = crate::automation::track_host_bounds(
        el,
        element.id,
        selection_start_flag(style),
        paint_reporter(element.id, element.events(), ctx),
        style.and_then(crate::occlusion::cover),
    );

    if let Some(handle) = ctx.focus_handles.get(&element.id) {
        el = el.track_focus(handle);
        el = apply_focus_visible(el, element.id, style, ctx.keyboard_focus.as_deref());
    }
    if let Some(tab_index) = element
        .custom_props()
        .get("tabIndex")
        .and_then(|value| value.as_i64())
        .and_then(|index| isize::try_from(index).ok())
    {
        el = el.tab_index(tab_index).tab_stop(tab_index >= 0);
    }

    // React text instances (`createTextInstance`) are also type `text` and
    // hold `content`. Only the host `<text>` (no content of its own) gets
    // Label. The inner nodes stay out of the AX tree so VoiceOver does not
    // hear the same string twice.
    let is_text_host = element.element_type == "text" && element.content.is_none();
    let default_role = is_text_host.then_some(gpui::Role::Label);
    el = apply_accessibility(el, element.custom_props(), default_role);
    if is_text_host && element.custom_props().get("aria-valuetext").is_none() {
        if let Some(content) = joined_text_content(ctx.tree, element) {
            el = el.aria_value(content);
        }
    }

    // Wire up events.
    // Some events (on_hover, on_aux_click) require a stateful element (.id()),
    // which we already set above. Others (on_mouse_down, on_key_down) work
    // on any InteractiveElement.
    if element.events().contains("click") {
        let id = element.id;
        let callback = ctx.event_callback.clone();
        // GPUI's higher-level on_click gesture is not finalized by the
        // embedded macOS pump. Bubble listeners run in reverse registration
        // order, so attach click first to keep onMouseUp ahead of onClick.
        el = el.on_mouse_up(gpui::MouseButton::Left, move |mouse_event, _window, _cx| {
            emit_press_event(&callback, id, "click", |p| {
                let (x, y) = point_to_xy(mouse_event.position);
                p.x = Some(x);
                p.y = Some(y);
                p.button = Some(0);
                p.modifiers = Some(mouse_event.modifiers.into());
                p.click_count = Some(mouse_event.click_count as u32);
                p.is_right_click = Some(false);
            });
        });
        el = apply_a11y_click(el, element.events(), id, ctx.event_callback);
    }

    for event_type in element.events() {
        let id = element.id;
        let callback = ctx.event_callback.clone();
        match event_type.as_str() {
            // ── Aux click (non-primary), like the DOM `auxclick` ──
            "auxClick" => {
                el = el.on_aux_click(move |click_event, _window, _cx| {
                    emit_press_event(&callback, id, "auxClick", |p| {
                        let (x, y) = point_to_xy(click_event.position());
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(click_event.modifiers().into());
                        p.click_count = Some(click_event.click_count() as u32);
                        p.is_right_click = Some(click_event.is_right_click());
                    });
                });
            }

            // ── Context menu: a right press, or a control-click as macOS has it ──
            "contextMenu" => {
                for &button in &[gpui::MouseButton::Right, gpui::MouseButton::Left] {
                    let callback = callback.clone();
                    el = el.on_mouse_down(button, move |mouse_event, _window, _cx| {
                        if button == gpui::MouseButton::Left && !mouse_event.modifiers.control {
                            return;
                        }
                        emit_press_event(&callback, id, "contextMenu", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.modifiers = Some(mouse_event.modifiers.into());
                            p.is_right_click = Some(true);
                        });
                    });
                }
            }

            // ── Double click: the second primary release of a quick pair ──
            "doubleClick" => {
                el = el.on_mouse_up(gpui::MouseButton::Left, move |mouse_event, _window, _cx| {
                    if mouse_event.click_count != 2 {
                        return;
                    }
                    emit_press_event(&callback, id, "doubleClick", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.button = Some(0);
                        p.click_count = Some(2);
                        p.modifiers = Some(mouse_event.modifiers.into());
                    });
                });
            }

            // ── Mouse down (all buttons) ─────────────────────────
            "mouseDown" => {
                // Wire all three buttons so JS gets right-click, middle-click, etc.
                for &button in &[gpui::MouseButton::Left, gpui::MouseButton::Middle, gpui::MouseButton::Right] {
                    let callback = callback.clone();
                    el = el.on_mouse_down(button, move |mouse_event, _window, _cx| {
                        emit_press_event(&callback, id, "mouseDown", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse up (all buttons) ───────────────────────────
            "mouseUp" => {
                for &button in &[gpui::MouseButton::Left, gpui::MouseButton::Middle, gpui::MouseButton::Right] {
                    let callback = callback.clone();
                    el = el.on_mouse_up(button, move |mouse_event, _window, _cx| {
                        emit_press_event(&callback, id, "mouseUp", |p| {
                            let (x, y) = point_to_xy(mouse_event.position);
                            p.x = Some(x);
                            p.y = Some(y);
                            p.button = Some(mouse_button_to_u32(mouse_event.button));
                            p.click_count = Some(mouse_event.click_count as u32);
                            p.modifiers = Some(mouse_event.modifiers.into());
                        });
                    });
                }
            }

            // ── Mouse move ───────────────────────────────────────
            "mouseMove" => {
                el = el.on_mouse_move(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseMove", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(mouse_event.modifiers.into());
                        p.pressed_button = mouse_event.pressed_button.map(mouse_button_to_u32);
                    });
                });
            }

            // ── Hover (mouseEnter + mouseLeave) ──────────────────
            // GPUI's on_hover fires with true on enter, false on leave.
            // We split into two distinct event types for the React side.
            "mouseEnter" | "mouseLeave" => {
                // Only wire once even if both mouseEnter and mouseLeave are registered.
                // Check if we already wired on_hover via the other event.
                let has_enter = element.events().contains("mouseEnter");
                let has_leave = element.events().contains("mouseLeave");
                // Wire on first encounter (mouseEnter sorts before mouseLeave).
                if event_type.as_str() == "mouseEnter" || !has_enter {
                    let callback_enter = if has_enter { ctx.event_callback.clone() } else { None };
                    let callback_leave = if has_leave { ctx.event_callback.clone() } else { None };
                    el = el.on_hover(move |&is_hovered, _window, _cx| {
                        if is_hovered {
                            emit_event_full(&callback_enter, id, "mouseEnter", |p| {
                                p.hovered = Some(true);
                            });
                        } else {
                            emit_event_full(&callback_leave, id, "mouseLeave", |p| {
                                p.hovered = Some(false);
                            });
                        }
                    });
                }
            }

            // ── Mouse down outside ───────────────────────────────
            // Fires when the user clicks OUTSIDE this element.
            // Critical for "click outside to close" pattern (dropdowns, modals).
            "mouseDownOutside" => {
                el = el.on_mouse_down_out(move |mouse_event, _window, _cx| {
                    emit_event_full(&callback, id, "mouseDownOutside", |p| {
                        let (x, y) = point_to_xy(mouse_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.button = Some(mouse_button_to_u32(mouse_event.button));
                        p.modifiers = Some(mouse_event.modifiers.into());
                    });
                });
            }

            // ── File drop (Finder / OS paths) ────────────────────
            "fileDrop" => {
                el = el.on_drop(move |dropped: &gpui::ExternalPaths, window, _cx| {
                    emit_file_drop(&callback, id, dropped, window.mouse_position());
                });
            }

            // ── Scroll wheel ─────────────────────────────────────
            "scroll" => {
                el = el.on_scroll_wheel(move |scroll_event, _window, _cx| {
                    emit_event_full(&callback, id, "scroll", |p| {
                        let (x, y) = point_to_xy(scroll_event.position);
                        p.x = Some(x);
                        p.y = Some(y);
                        p.modifiers = Some(scroll_event.modifiers.into());
                        p.precise = Some(scroll_event.delta.precise());

                        // Convert ScrollDelta to pixel values.
                        // For Lines delta, we use a default line height of 20px.
                        let line_height = gpui::px(20.0);
                        let pixel_delta = scroll_event.delta.pixel_delta(line_height);
                        p.delta_x = Some(f64::from(f32::from(pixel_delta.x)));
                        p.delta_y = Some(f64::from(f32::from(pixel_delta.y)));

                        p.touch_phase = Some(match scroll_event.touch_phase {
                            gpui::TouchPhase::Started => "started".to_string(),
                            gpui::TouchPhase::Moved => "moved".to_string(),
                            gpui::TouchPhase::Ended => "ended".to_string(),
                            gpui::TouchPhase::Cancelled => "cancelled".to_string(),
                        });
                    });
                });
            }

            // ── Key down ─────────────────────────────────────────
            // Requires .focusable() (set above). Element must be focused
            // (clicked or tabbed to) for these to fire.
            "keyDown" => {
                el = el.on_key_down(move |key_event, _window, _cx| {
                    emit_key_event(&callback, id, "keyDown", &key_event.keystroke, Some(key_event.is_held));
                });
            }

            // ── Key up ───────────────────────────────────────────
            "keyUp" => {
                el = el.on_key_up(move |key_event, _window, _cx| {
                    emit_key_event(&callback, id, "keyUp", &key_event.keystroke, None);
                });
            }

            // ── Focus / Blur ─────────────────────────────────────
            // Event emission is handled by FocusHandle subscriptions
            // set up in GpuixView::sync_focus_handles(). The handle is
            // attached to this element via .track_focus() above.
            "focus" | "blur" => {}

            _ => {}
        }
    }

    if element.events().contains("mouseDown") && element.events().contains("mouseMove") {
        el = el.capture_pointer();
    }

    el = apply_app_region(el, element, style);

    // `title`: GPUI's tooltip, in the theme's tooltip style.
    if let Some(title) = element.custom_props().get("title").and_then(|value| value.as_str()).filter(|t| !t.is_empty())
    {
        el = crate::tooltip::attach(el, title);
    }

    // Text content — selectable, same as a <text> leaf.
    // An empty string paints no glyphs, so it must take no space: gpui's text
    // layout still reserves a full line for "". React never sends empty text;
    // the filter keeps one from taking a full-height line.
    // What the quad cannot paint (gradients past two stops, images, nine-slice frames, a colour
    // a side) paints first, under the text and the children.
    if let Some(decor) = style.and_then(crate::decor::element) {
        el = el.child(decor);
    }

    if let Some(content) = element.content.as_deref().filter(|value| !value.is_empty()) {
        let text = text_content(element, content, ctx);
        // A node with no style of its own (every text React makes) is already `relative` and
        // exactly the text's box: its runs go straight in, without a wrapper per string.
        el = if style.is_none() {
            el.children(crate::text::selectable_text_parts(text))
        } else {
            el.child(selectable_text(text))
        };
    }

    // Children, in `zIndex` order when any sets one (a stable sort, so ties keep tree order).
    let mut children: std::borrow::Cow<'_, [u64]> = std::borrow::Cow::Borrowed(&element.children);
    let z_of = |id: &u64| -> f64 {
        ctx.tree
            .elements
            .get(id)
            .and_then(|child| child.style.as_deref())
            .and_then(|style| style.z_index)
            .unwrap_or(0.0)
    };
    if element.children.iter().any(|id| z_of(id) != 0.0) {
        let mut sorted = element.children.clone();
        sorted.sort_by(|a, b| z_of(a).partial_cmp(&z_of(b)).unwrap_or(std::cmp::Ordering::Equal));
        children = std::borrow::Cow::Owned(sorted);
    }
    for &child_id in children.iter() {
        let child = build_element(child_id, ctx, window, cx);
        el = if overflow_x_only { el.child(gpui::div().flex_none().child(child)) } else { el.child(child) };
    }

    // Last, so the thumb paints over the content.
    if let (Some(style), Some(handle)) = (style, ctx.scroll_handles.get(&element.id)) {
        if let Some(thumbs) = ThumbStyle::of(
            style,
            style.overflow_x.as_deref().or(style.overflow.as_deref()) == Some("scroll"),
            style.overflow_y.as_deref().or(style.overflow.as_deref()) == Some("scroll"),
        ) {
            el = el.child(scrollbar_overlay(handle.clone(), thumbs));
        }
    }

    if ctx.cache_root == Some(element.id) {
        // The root of a cached subtree fills the box its view was laid out at: where it sits
        // and how big it is were decided outside, from the same style (`build_cache_boundary`).
        let style = el.style();
        style.position = Some(gpui::Position::Relative);
        style.inset = Default::default();
        style.margin = Default::default();
        style.size.width = Some(gpui::relative(1.).into());
        style.size.height = Some(gpui::relative(1.).into());
        style.min_size = Default::default();
        style.max_size = Default::default();
    }

    el.into_any_element()
}

/// A classic scrollbar's default thickness.
const SCROLLBAR_WIDTH: f32 = 6.0;
/// The shortest a thumb gets, however long the content.
const MIN_SCROLLBAR_THUMB: f32 = 20.0;

/// Where a thumb starts and how long it is along one axis, in the track's own
/// coordinates. `track` is the viewport's extent, `max_scroll` how far the
/// content can move, and `offset` how far it has (zero or negative). `None`
/// when the content fits, so there is nothing to show.
fn scrollbar_thumb(track: f32, max_scroll: f32, offset: f32) -> Option<(f32, f32)> {
    if track <= 0.0 || max_scroll <= 1.0 {
        return None;
    }
    let length = (track * track / (track + max_scroll)).clamp(MIN_SCROLLBAR_THUMB.min(track), track);
    let progress = (-offset / max_scroll).clamp(0.0, 1.0);
    Some((progress * (track - length), length))
}

/// What a scroller asks its thumbs to look like, and along which axes it has any.
#[derive(Clone, Copy)]
struct ThumbStyle {
    color: gpui::Hsla,
    thickness: f32,
    horizontal: bool,
    vertical: bool,
}

impl ThumbStyle {
    /// `None` unless the style names a `scrollbarColor` that parses.
    fn of(style: &StyleDesc, horizontal: bool, vertical: bool) -> Option<Self> {
        let color = style.scrollbar_color.as_deref().and_then(crate::color::parse_color_rgba)?;
        Some(Self {
            color: color.into(),
            thickness: style.scrollbar_width.unwrap_or(SCROLLBAR_WIDTH as f64) as f32,
            horizontal,
            vertical,
        })
    }
}

/// Paints the thumbs of a viewport at `origin` with `size`. `max` is how far
/// its content can move and `offset` how far it has (zero or negative).
fn paint_scrollbars(
    window: &mut gpui::Window,
    origin: gpui::Point<gpui::Pixels>,
    size: gpui::Size<gpui::Pixels>,
    max: gpui::Point<gpui::Pixels>,
    offset: gpui::Point<gpui::Pixels>,
    thumbs: ThumbStyle,
) {
    let ThumbStyle { color, thickness, horizontal, vertical } = thumbs;
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let along = |show: bool, track: f32, max_scroll: gpui::Pixels, offset: gpui::Pixels| {
        show.then(|| scrollbar_thumb(track, f32::from(max_scroll), f32::from(offset))).flatten()
    };
    let mut across = along(vertical, height, max.y, offset.y);
    let mut sideways = along(horizontal, width, max.x, offset.x);
    if across.is_some() && sideways.is_some() {
        // Leave the corner to whichever thumb reaches it first.
        across = along(vertical, height - thickness, max.y, offset.y);
        sideways = along(horizontal, width - thickness, max.x, offset.x);
    }
    let paint = |window: &mut gpui::Window, x: f32, y: f32, w: f32, h: f32| {
        // Whole pixels, so a thumb is as crisp as a browser's.
        let rect = gpui::Bounds::new(
            gpui::point(origin.x + gpui::px(x.round()), origin.y + gpui::px(y.round())),
            gpui::size(gpui::px(w.round()), gpui::px(h.round())),
        );
        window.paint_quad(gpui::quad(
            rect,
            gpui::px(thickness / 2.0),
            color,
            gpui::px(0.0),
            gpui::transparent_black(),
            gpui::BorderStyle::default(),
        ));
    };
    if let Some((start, length)) = across {
        paint(window, width - thickness, start, thickness, length);
    }
    if let Some((start, length)) = sideways {
        paint(window, start, height - thickness, length, thickness);
    }
}

/// The thumbs of a scroll container, painted from its own scroll offset in the
/// frame the content moves in, so a wheel tick costs nothing beyond that frame.
///
/// It is the container's last child, absolutely placed over its padding box.
/// Children of a scroller are painted shifted by the scroll offset, so the
/// viewport is this box moved back by it. It has no hitbox and takes no input.
fn scrollbar_overlay(handle: gpui::ScrollHandle, thumbs: ThumbStyle) -> gpui::AnyElement {
    use gpui::prelude::*;

    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let offset = handle.offset();
            paint_scrollbars(window, bounds.origin - offset, bounds.size, handle.max_offset(), offset, thumbs);
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
    .into_any_element()
}

/// How far a virtual list can scroll and has, for `paint_scrollbars`. Rows the
/// list has not measured count as the average measured row, not as the single
/// `estimatedItemHeight`, which a list of tall rows would otherwise miss by
/// a few times. Before any row is measured the list's own totals stand.
fn list_scroll_extent(state: &gpui::ListState) -> (gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>) {
    match state.averaged_scroll_extent() {
        Some((max, offset)) => (gpui::point(gpui::px(0.0), max), gpui::point(gpui::px(0.0), -offset)),
        None => (state.max_offset_for_scrollbar(), state.scroll_px_offset_for_scrollbar()),
    }
}

/// A virtual list with its thumb painted over it.
///
/// A list element takes no children, and wrapping it in a box would change the
/// layout its style asks for. This lays out, prepaints and paints as the list
/// does (it answers with the list's own layout node), then draws the thumb in
/// the list's bounds from what its `ListState` has measured: the same totals
/// the list scrolls by, so the thumb and the scroll position cannot disagree.
struct ListWithScrollbar {
    list: gpui::AnyElement,
    state: gpui::ListState,
    thumbs: ThumbStyle,
}

impl gpui::IntoElement for ListWithScrollbar {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for ListWithScrollbar {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, ()) {
        (self.list.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        _: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        self.list.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) {
        self.list.paint(window, cx);
        let (max, offset) = list_scroll_extent(&self.state);
        paint_scrollbars(window, bounds.origin, bounds.size, max, offset, self.thumbs);
    }
}

#[cfg(test)]
mod scrollbar_tests {
    use super::scrollbar_thumb;

    #[test]
    fn a_thumb_is_the_viewports_share_of_the_content() {
        // 100 of 300: a third of the track.
        let (start, length) = scrollbar_thumb(100.0, 200.0, 0.0).unwrap();
        assert_eq!(start, 0.0);
        assert!((length - 100.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn a_thumb_runs_the_whole_track_as_the_content_runs_its_distance() {
        let (_, length) = scrollbar_thumb(100.0, 200.0, 0.0).unwrap();
        let (start, same_length) = scrollbar_thumb(100.0, 200.0, -200.0).unwrap();
        assert_eq!(length, same_length);
        assert!((start + length - 100.0).abs() < 0.01, "ends at the end of the track");
        let (halfway, _) = scrollbar_thumb(100.0, 200.0, -100.0).unwrap();
        assert!((halfway - (100.0 - length) / 2.0).abs() < 0.01);
    }

    #[test]
    fn a_thumb_stays_inside_the_track_when_the_offset_overshoots() {
        let (start, length) = scrollbar_thumb(100.0, 200.0, -450.0).unwrap();
        assert!((start + length - 100.0).abs() < 0.01);
        assert_eq!(scrollbar_thumb(100.0, 200.0, 30.0).unwrap().0, 0.0);
    }

    #[test]
    fn a_thumb_is_never_shorter_than_a_grabbable_length_nor_longer_than_the_track() {
        assert_eq!(scrollbar_thumb(100.0, 100_000.0, 0.0).unwrap().1, 20.0);
        assert_eq!(scrollbar_thumb(10.0, 100_000.0, 0.0).unwrap().1, 10.0);
    }

    #[test]
    fn content_that_fits_has_no_thumb() {
        assert_eq!(scrollbar_thumb(100.0, 0.0, 0.0), None);
        assert_eq!(scrollbar_thumb(100.0, 0.5, 0.0), None, "a fraction of a pixel is rounding");
        assert_eq!(scrollbar_thumb(0.0, 200.0, 0.0), None);
    }
}

/// A selectable text run owned by `element`. Runs are left to gpui so the
/// text keeps inheriting colour, weight and family from ancestor styles.
///
/// The run's group is its parent host element, because React makes a separate
/// host node for every interpolated string. `<text>Hello {name}!</text>` is one
/// logical line painted as three runs that all share the parent's id.
/// A `userSelect: "none"` run still paints highlight washes, because a browser
/// still finds that text with Ctrl+F. Element chrome that must never be found,
/// such as a code gutter, uses `chrome_text` instead.
fn text_content(
    element: &crate::retained_tree::RetainedElement,
    content: &str,
    ctx: &BuildCtx,
) -> crate::text::SelectableText {
    crate::text::SelectableText {
        group: crate::text::search::group_id(ctx.tree, element.id),
        selectable: ctx.inherited.selectable,
        highlight: ctx.inherited.highlight.clone().map(crate::text::HighlightSource::Resolved),
        ..crate::text::SelectableText::new(
            element.id,
            0,
            gpui::SharedString::from(content.to_string()),
            None,
            ctx.selection.clone(),
            ctx.inherited.selection_wash,
        )
    }
}

/// Explicit `userSelect` on this node. `None` means inherit; the ancestor
/// that set the value already owns the start region.
fn selection_start_flag(style: Option<&StyleDesc>) -> Option<bool> {
    match style.and_then(|style| style.user_select.as_deref()) {
        Some("none") => Some(false),
        Some("text") | Some("auto") => Some(true),
        _ => None,
    }
}

// ── Style application ────────────────────────────────────────────────

pub(crate) fn apply_width<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.w(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.w_full(),
        crate::style::DimensionValue::Percentage(v) => el.w(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

pub(crate) fn apply_height<E: gpui::Styled>(el: E, dim: &crate::style::DimensionValue) -> E {
    match dim {
        crate::style::DimensionValue::Pixels(v) => el.h(gpui::px(*v as f32)),
        crate::style::DimensionValue::Percentage(v) if *v >= 0.999 => el.h_full(),
        crate::style::DimensionValue::Percentage(v) => el.h(gpui::relative(*v as f32)),
        crate::style::DimensionValue::Auto => el,
    }
}

/// Base styles plus gpui's `hover` and `active` refinements.
///
/// Every stateful GPUI root must go through this, never `apply_styles` alone.
/// `StyleDesc` carries `hover` and `active` for every element type, so a custom
/// element that only applied the base styles accepted the prop, serialized it,
/// and dropped it. gpui reads both refinements from the element state behind the
/// element's `ElementId`, so the caller must have called `.id(..)` first.
pub fn apply_interactive_styles<E>(mut el: E, style: &StyleDesc) -> E
where
    E: gpui::Styled + gpui::StatefulInteractiveElement,
{
    el = apply_styles(el, style);
    if let Some(hover_style) = style.hover.as_deref() {
        el = el.hover(|refinement| apply_styles(refinement, hover_style));
    }
    if let Some(active_style) = style.active.as_deref() {
        el = el.active(|refinement| apply_styles(refinement, active_style));
    }
    // `focusVisible` needs the focus handle, so `apply_focus_visible` applies
    // it where the element calls track_focus.
    el
}

pub(crate) fn apply_styles<E: gpui::Styled>(mut el: E, style: &StyleDesc) -> E {
    match style.display.as_deref() {
        Some("flex") => el = el.flex(),
        Some("grid") => el = el.grid(),
        _ => {}
    }
    if let Some(cols) = style.grid_template_columns {
        let count = cols.round().clamp(1.0, 64.0) as u16;
        el = match style.grid_column_min.as_deref() {
            Some("min-content") => el.grid_cols_min_content(count),
            Some("max-content") => el.grid_cols_max_content(count),
            _ => el.grid_cols(count),
        };
    }
    if let Some(rows) = style.grid_template_rows {
        let count = rows.round().clamp(1.0, 64.0) as u16;
        el = match style.grid_row_min.as_deref() {
            Some("min-content") => el.grid_rows_min_content(count),
            Some("max-content") => el.grid_rows_max_content(count),
            _ => el.grid_rows(count),
        };
    }
    if style.flex_direction.as_deref() == Some("column") {
        el = el.flex_col();
    }
    if style.flex_direction.as_deref() == Some("row") {
        el = el.flex_row();
    }
    match style.flex_wrap.as_deref() {
        Some("wrap") => el = el.flex_wrap(),
        Some("wrap-reverse") => el = el.flex_wrap_reverse(),
        Some("nowrap") => el = el.flex_nowrap(),
        _ => {}
    }
    if let Some(grow) = style.flex_grow {
        el.style().flex_grow = Some(grow as f32);
    }
    if let Some(shrink) = style.flex_shrink {
        el.style().flex_shrink = Some(shrink as f32);
    }
    if let Some(basis) = style.flex_basis {
        el = el.flex_basis(gpui::px(basis as f32));
    }
    match style.align_items.as_deref() {
        Some("center") => el = el.items_center(),
        Some("start") | Some("flex-start") => el = el.items_start(),
        Some("end") | Some("flex-end") => el = el.items_end(),
        Some("stretch") => el = el.items_stretch(),
        Some("baseline") => el = el.items_baseline(),
        _ => {}
    }
    match style.align_content.as_deref() {
        Some("center") => el = el.content_center(),
        Some("start") | Some("flex-start") => el = el.content_start(),
        Some("end") | Some("flex-end") => el = el.content_end(),
        Some("between") | Some("space-between") => el = el.content_between(),
        Some("around") | Some("space-around") => el = el.content_around(),
        Some("evenly") | Some("space-evenly") => el = el.content_evenly(),
        Some("stretch") => el = el.content_stretch(),
        Some("normal") => el = el.content_normal(),
        _ => {}
    }
    match style.justify_content.as_deref() {
        Some("center") => el = el.justify_center(),
        Some("start") | Some("flex-start") => el = el.justify_start(),
        Some("end") | Some("flex-end") => el = el.justify_end(),
        Some("between") | Some("space-between") => el = el.justify_between(),
        Some("around") | Some("space-around") => el = el.justify_around(),
        Some("evenly") | Some("space-evenly") => el = el.justify_evenly(),
        _ => {}
    }
    match style.align_self.as_deref() {
        Some("center") => {
            el.style().align_self = Some(gpui::AlignItems::Center);
        }
        Some("start") | Some("flex-start") => {
            el.style().align_self = Some(gpui::AlignItems::FlexStart);
        }
        Some("end") | Some("flex-end") => {
            el.style().align_self = Some(gpui::AlignItems::FlexEnd);
        }
        Some("stretch") => {
            el.style().align_self = Some(gpui::AlignItems::Stretch);
        }
        Some("baseline") => {
            el.style().align_self = Some(gpui::AlignItems::Baseline);
        }
        _ => {}
    }
    if let Some(gap) = style.gap {
        el = el.gap(gpui::px(gap as f32));
    }
    // Per-axis gaps come after `gap` so the axis value wins, matching CSS
    // shorthand order.
    if let Some(gap) = style.row_gap {
        el = el.gap_y(gpui::px(gap as f32));
    }
    if let Some(gap) = style.column_gap {
        el = el.gap_x(gpui::px(gap as f32));
    }
    if let Some(ref w) = style.width {
        el = apply_width(el, w);
    }
    if let Some(ref h) = style.height {
        el = apply_height(el, h);
    }
    if let Some(ref min_w) = style.min_width {
        match min_w {
            crate::style::DimensionValue::Pixels(v) => el = el.min_w(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.min_w(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref min_h) = style.min_height {
        match min_h {
            crate::style::DimensionValue::Pixels(v) => el = el.min_h(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.min_h(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref max_w) = style.max_width {
        match max_w {
            crate::style::DimensionValue::Pixels(v) => el = el.max_w(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.max_w(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(ref max_h) = style.max_height {
        match max_h {
            crate::style::DimensionValue::Pixels(v) => el = el.max_h(gpui::px(*v as f32)),
            crate::style::DimensionValue::Percentage(v) => el = el.max_h(gpui::relative(*v as f32)),
            crate::style::DimensionValue::Auto => {}
        }
    }
    if let Some(p) = style.padding {
        el = el.p(gpui::px(p as f32));
    }
    if let Some(pt) = style.padding_top {
        el = el.pt(gpui::px(pt as f32));
    }
    if let Some(pr) = style.padding_right {
        el = el.pr(gpui::px(pr as f32));
    }
    if let Some(pb) = style.padding_bottom {
        el = el.pb(gpui::px(pb as f32));
    }
    if let Some(pl) = style.padding_left {
        el = el.pl(gpui::px(pl as f32));
    }
    if let Some(m) = style.margin {
        el = el.m(gpui::px(m as f32));
    }
    if let Some(mt) = style.margin_top {
        el = el.mt(gpui::px(mt as f32));
    }
    if let Some(mr) = style.margin_right {
        el = el.mr(gpui::px(mr as f32));
    }
    if let Some(mb) = style.margin_bottom {
        el = el.mb(gpui::px(mb as f32));
    }
    if let Some(ml) = style.margin_left {
        el = el.ml(gpui::px(ml as f32));
    }
    // Taffy has no viewport-fixed position, and GPUI has no scrolling document,
    // so "fixed" lays out exactly like "absolute", as `should_occlude` treats
    // the two; without this arm a "fixed" box would stay in flow.
    match style.position.as_deref() {
        Some("absolute") | Some("fixed") => el = el.absolute(),
        Some("relative") => el = el.relative(),
        _ => {}
    }
    if let Some(top) = style.top {
        el = el.top(gpui::px(top as f32));
    }
    if let Some(right) = style.right {
        el = el.right(gpui::px(right as f32));
    }
    if let Some(bottom) = style.bottom {
        el = el.bottom(gpui::px(bottom as f32));
    }
    if let Some(left) = style.left {
        el = el.left(gpui::px(left as f32));
    }
    if let Some(ratio) = style.aspect_ratio {
        if ratio > 0.0 && ratio.is_finite() {
            el = el.aspect_ratio(ratio as f32);
        }
    }
    if let Some(background) = style.resolved_background() {
        el = el.bg(background);
    }
    if let Some(ref color) = style.color {
        if let Some(color) = crate::color::parse_color_rgba(color) {
            el = el.text_color(color);
        }
    }
    if let Some(size) = style.font_size {
        el = el.text_size(gpui::px(size as f32));
    }
    if let Some(ref family) = style.font_family {
        el = el.font_family(family.clone());
    }
    if let Some(ref weight) = style.font_weight {
        el = el.font_weight(parse_font_weight(weight));
    }
    match style.font_style.as_deref() {
        Some("italic") | Some("oblique") => el = el.italic(),
        Some("normal") => el.text_style().font_style = Some(gpui::FontStyle::Normal),
        _ => {}
    }
    match style.text_align.as_deref() {
        Some("center") => el = el.text_center(),
        Some("right") => el = el.text_right(),
        Some("left") | Some("start") => el = el.text_left(),
        _ => {}
    }
    match style.white_space.as_deref() {
        Some("nowrap") => el = el.whitespace_nowrap(),
        Some("normal") => el = el.whitespace_normal(),
        _ => {}
    }
    match style.text_overflow.as_deref() {
        Some("ellipsis") => el = el.text_ellipsis(),
        Some("ellipsis-start") => el = el.text_ellipsis_start(),
        _ => {}
    }
    if let Some(clamp) = style.line_clamp {
        if clamp >= 1.0 {
            el = el.line_clamp(clamp as usize);
        }
    }
    match style.text_decoration.as_deref() {
        Some("underline") => el = el.underline(),
        Some("line-through") => el = el.line_through(),
        Some("none") => el = el.text_decoration_none(),
        _ => {}
    }
    if let Some(line_height) = style.line_height {
        if line_height > 0.0 {
            el = el.line_height(gpui::px(line_height as f32));
        }
    }
    if let Some(radius) = style.border_radius {
        el = el.rounded(gpui::px(radius as f32));
    }
    // Apply corner longhands after the shorthand so the explicit corner wins.
    if let Some(radius) = style.border_top_left_radius {
        el = el.rounded_tl(gpui::px(radius as f32));
    }
    if let Some(radius) = style.border_top_right_radius {
        el = el.rounded_tr(gpui::px(radius as f32));
    }
    if let Some(radius) = style.border_bottom_left_radius {
        el = el.rounded_bl(gpui::px(radius as f32));
    }
    if let Some(radius) = style.border_bottom_right_radius {
        el = el.rounded_br(gpui::px(radius as f32));
    }
    // `borderWidth: 0` must clear a border, not be ignored: an element that
    // draws its own border needs a way for the caller to remove it.
    if let Some(width) = style.border_width {
        el = el.border(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = style.border_top_width {
        el = el.border_t(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = style.border_right_width {
        el = el.border_r(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = style.border_bottom_width {
        el = el.border_b(gpui::px(width.max(0.0) as f32));
    }
    if let Some(width) = style.border_left_width {
        el = el.border_l(gpui::px(width.max(0.0) as f32));
    }
    if let Some(ref color) = style.border_color {
        if let Some(color) = crate::color::parse_color_rgba(color) {
            el = el.border_color(color);
        }
    }
    // A colour a side, or a nine-slice frame, is `crate::decor`'s to paint: the quad keeps the
    // widths (they are layout) and draws no line.
    if style.has_side_colors() || style.border_image.is_some() {
        el = el.border_color(gpui::transparent_black());
    }
    match style.border_style.as_deref() {
        Some("dashed") => el = el.border_dashed(),
        Some("solid") => el.style().border_style = Some(gpui::BorderStyle::Solid),
        _ => {}
    }
    if let Some(ref shadows) = style.box_shadow {
        // A shadow whose colour does not parse is dropped on its own, so one
        // bad entry does not take the rest of a list with it.
        let shadows: Vec<gpui::BoxShadow> = shadows
            .as_slice()
            .iter()
            .filter_map(|shadow| {
                let color = crate::color::parse_color_rgba(&shadow.color)?;
                let built = gpui::BoxShadow::new(
                    gpui::px(shadow.offset_x as f32),
                    gpui::px(shadow.offset_y as f32),
                    color.into(),
                )
                .blur_radius(gpui::px(shadow.blur_radius.max(0.0) as f32))
                .spread_radius(gpui::px(shadow.spread_radius as f32));
                Some(if shadow.inset { built.inset() } else { built })
            })
            .collect();
        if !shadows.is_empty() {
            el = el.shadow(shadows);
        }
    }
    if style.outline_width.is_some() || style.outline_color.is_some() {
        let color = style
            .outline_color
            .as_deref()
            .and_then(crate::color::parse_color_rgba)
            .unwrap_or_else(|| gpui::transparent_black().into());
        el = el.outline(gpui::Outline {
            width: gpui::px(style.outline_width.unwrap_or(1.0).max(0.0) as f32),
            color: color.into(),
            offset: gpui::px(style.outline_offset.unwrap_or(0.0) as f32),
        });
    }
    if style.visibility.as_deref() == Some("hidden") {
        el = el.invisible();
    }
    if let Some(opacity) = style.opacity {
        el = el.opacity(opacity as f32);
    }
    if let Some(cursor) = style.cursor.as_deref().and_then(crate::style::parse_cursor) {
        el = el.cursor(cursor);
    }
    // Overflow: hidden is on the Styled trait, so we handle it here.
    // overflow: "scroll" requires StatefulInteractiveElement — handled in build_host_container().
    // CSS precedence: axis-specific (overflowX/Y) overrides the shorthand (overflow).
    {
        let resolved_x = style.overflow_x.as_deref().or(style.overflow.as_deref());
        let resolved_y = style.overflow_y.as_deref().or(style.overflow.as_deref());
        // Only apply hidden here — scroll is handled in build_host_container.
        if resolved_x == Some("hidden") && resolved_y == Some("hidden") {
            el = el.overflow_hidden();
        } else if resolved_x == Some("hidden") {
            el = el.overflow_x_hidden();
        } else if resolved_y == Some("hidden") {
            el = el.overflow_y_hidden();
        }
    }

    el
}

// ── Event emission ───────────────────────────────────────────────────

/// Helper to convert a GPUI Point<Pixels> to (f64, f64).
pub(crate) fn point_to_xy(p: gpui::Point<gpui::Pixels>) -> (f64, f64) {
    (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)))
}

/// Paths that are valid UTF-8, or None when the drop is empty or not Unicode.
/// `to_string_lossy` would invent a path that does not exist on disk.
fn file_drop_paths(dropped: &gpui::ExternalPaths) -> Option<Vec<String>> {
    let paths = dropped.paths();
    if paths.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        out.push(path.to_str()?.to_string());
    }
    Some(out)
}

pub(crate) fn emit_file_drop(
    callback: &Option<EventCallback>,
    element_id: u64,
    dropped: &gpui::ExternalPaths,
    position: gpui::Point<gpui::Pixels>,
) {
    let Some(paths) = file_drop_paths(dropped) else {
        return;
    };
    emit_event_full(callback, element_id, "fileDrop", |payload| {
        let (x, y) = point_to_xy(position);
        payload.x = Some(x);
        payload.y = Some(y);
        payload.paths = Some(paths);
    });
}

/// Convert GPUI MouseButton to our u32 encoding: 0=left, 1=middle, 2=right.
pub(crate) fn mouse_button_to_u32(button: gpui::MouseButton) -> u32 {
    match button {
        gpui::MouseButton::Left => 0,
        gpui::MouseButton::Middle => 1,
        gpui::MouseButton::Right => 2,
        gpui::MouseButton::Navigate(_) => 3,
    }
}

/// The paint-driven events of one element, as one reporter for `on_painted`.
///
/// `onLayout`: its border box after the first paint with the listener, then again
/// each time its size changes. A move without a resize reports nothing, so
/// scrolling a list does not queue an event per row.
///
/// `onScrollChange`: its scroll offset each time it differs from the last paint,
/// whatever moved it. The wheel is only one cause; a scrollbar drag or
/// `scrollTo` moves it too, and gpui raises no event for those.
///
/// Events are queued after the paint that saw the change, never from inside it.
/// The browser build calls the listener synchronously, and a handler that sets
/// state would re-enter the frame that is being painted.
fn paint_reporter(id: u64, events: &HashSet<String>, ctx: &BuildCtx) -> Option<crate::automation::PaintReporter> {
    let layout = events.contains("layout");
    let scroll = events.contains("scrollChange").then(|| ctx.scroll_handles.get(&id).cloned()).flatten();
    if !layout && scroll.is_none() {
        return None;
    }
    let callback = ctx.event_callback.clone()?;
    let reports = ctx.paint_reports.clone();
    Some(Rc::new(move |bounds, cx| {
        if layout {
            report_layout(id, bounds, &reports, &callback, cx);
        }
        if let Some(handle) = &scroll {
            report_scroll(id, handle.offset(), &reports, &callback, cx);
        }
    }))
}

fn report_layout(
    id: u64,
    bounds: gpui::Bounds<gpui::Pixels>,
    reports: &SharedPaintReports,
    callback: &EventCallback,
    cx: &mut gpui::App,
) {
    let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    {
        let mut reports = reports.borrow_mut();
        let unchanged = reports.sizes.get(&id).is_some_and(|last| {
            (last.0 - size.0).abs() < PAINT_REPORT_EPSILON && (last.1 - size.1).abs() < PAINT_REPORT_EPSILON
        });
        if unchanged {
            return;
        }
        reports.sizes.insert(id, size);
    }
    let callback = Some(callback.clone());
    cx.defer(move |_| {
        emit_event_full(&callback, id, "layout", |payload| {
            payload.x = Some(f64::from(f32::from(bounds.origin.x)));
            payload.y = Some(f64::from(f32::from(bounds.origin.y)));
            payload.width = Some(f64::from(size.0));
            payload.height = Some(f64::from(size.1));
        });
    });
}

fn report_scroll(
    id: u64,
    offset: gpui::Point<gpui::Pixels>,
    reports: &SharedPaintReports,
    callback: &EventCallback,
    cx: &mut gpui::App,
) {
    let offset = (f32::from(offset.x), f32::from(offset.y));
    {
        let mut reports = reports.borrow_mut();
        match reports.scrolls.get(&id).copied() {
            // The first paint only sets the baseline.
            None => {
                reports.scrolls.insert(id, offset);
                return;
            }
            Some(last)
                if (last.0 - offset.0).abs() < PAINT_REPORT_EPSILON
                    && (last.1 - offset.1).abs() < PAINT_REPORT_EPSILON =>
            {
                return;
            }
            Some(_) => {
                reports.scrolls.insert(id, offset);
            }
        }
    }
    let callback = Some(callback.clone());
    cx.defer(move |_| {
        emit_event_full(&callback, id, "scrollChange", |payload| {
            payload.scroll_x = Some(f64::from(offset.0));
            payload.scroll_y = Some(f64::from(offset.1));
        });
    });
}

/// Values closer than this are the same value. Layout is deterministic, so this
/// only absorbs float noise from fractional scale factors.
const PAINT_REPORT_EPSILON: f32 = 0.01;

/// General-purpose event emitter. Builds a default EventPayload, lets the
/// caller customize it via a closure, then sends it through the callback.
/// Production: queues on Node.js event loop via ThreadsafeFunction.
/// Tests: pushes to a synchronous Vec for drainEvents().
pub(crate) fn emit_event_full(
    callback: &Option<EventCallback>,
    element_id: u64,
    event_type: &str,
    build: impl FnOnce(&mut EventPayload),
) {
    if let Some(cb) = callback {
        let mut payload =
            EventPayload { element_id: element_id as f64, event_type: event_type.to_string(), ..Default::default() };
        build(&mut payload);
        cb(payload);
    }
}

// ── Batch processing ─────────────────────────────────────────────

/// Parsed batch operation — typed enum for atomic validation.
/// All ops are parsed and validated BEFORE any tree mutation occurs.
/// This prevents partial application on malformed batches.
enum BatchOp<'a> {
    CreateElement {
        id: u64,
        element_type: String,
    },
    DestroyElement {
        id: u64,
    },
    AppendChild {
        parent_id: u64,
        child_id: u64,
    },
    InsertBefore {
        parent_id: u64,
        child_id: u64,
        before_id: u64,
    },
    /// The payload stays as raw JSON until apply time.
    ///
    /// Two reasons. A parsed `StyleDesc` is ~1.4 KB, and a `Vec<BatchOp>` is as
    /// wide as its widest variant, so inlining one made a 220k-op mount reserve
    /// over 300 MB before it parsed a single op. And the tree hash-conses
    /// styles by content, so it needs the bytes: hashing ~110 bytes is far
    /// cheaper than building 80 `Option` fields and throwing 99.8% of them away.
    SetStyle {
        id: u64,
        style: &'a serde_json::value::RawValue,
    },
    SetText {
        id: u64,
        content: String,
    },
    SetEventListener {
        id: u64,
        event_type: String,
        has_handler: bool,
    },
    SetRoot {
        id: u64,
    },
    SetKeyboardFocusDim {
        enabled: bool,
    },
    SetCustomProp {
        id: u64,
        key: String,
        value: serde_json::Value,
    },
}

/// A batch failure. The message names the op index, so it survives the trip
/// back to JS as a plain `Error`.
pub type BatchResult<T> = std::result::Result<T, String>;

/// Decode the batch straight from its JSON bytes into `Vec<BatchOp>`.
///
/// There is deliberately no `Vec<serde_json::Value>` in between. That tree would
/// cost a `String` per key and per value, every payload would be deep-cloned out
/// of it, and `from_value` would parse the clone a second time, so one style
/// would be allocated three times: 1.5M allocations for a 220k-op mount.
///
/// What the decode guarantees is load-bearing:
///
/// * an unknown opcode is a hard error, not a skipped op. Silently ignoring one
///   would let a JS/Rust version skew desync the tree instead of throwing
/// * ids go through `raw_element_id`, so non-finite, negative, fractional and
///   out-of-safe-range values are still rejected
/// * `hasHandler` is accepted as a bool or a number
/// * errors still name the op index. `serde_json` reports a byte offset, which
///   is useless when you are chasing a desync
fn parse_batch_ops(bytes: &[u8]) -> BatchResult<Vec<BatchOp<'_>>> {
    serde_json::from_slice::<BatchOps>(bytes)
        .map(|batch| batch.0)
        .map_err(|error| format!("Failed to parse batch: {error}"))
}

struct BatchOps<'a>(Vec<BatchOp<'a>>);

impl<'de> serde::Deserialize<'de> for BatchOps<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct OpsVisitor;

        impl<'de> serde::de::Visitor<'de> for OpsVisitor {
            type Value = BatchOps<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an array of mutation tuples")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<BatchOps<'de>, A::Error> {
                let mut ops = Vec::with_capacity(seq.size_hint().unwrap_or(64));
                loop {
                    // The index is attached here because this is the only place
                    // that knows it.
                    let index = ops.len();
                    match seq.next_element::<BatchOp<'de>>() {
                        Ok(Some(op)) => ops.push(op),
                        Ok(None) => break,
                        Err(error) => return Err(serde::de::Error::custom(format!("Batch op {index}: {error}"))),
                    }
                }
                Ok(BatchOps(ops))
            }
        }

        deserializer.deserialize_seq(OpsVisitor)
    }
}

/// A string argument, borrowed from the input when the JSON has no escapes.
///
/// The owned copy happens exactly once, on the way into the `BatchOp`. Going
/// through a `Value` would allocate twice: into `Value::String`, then into the op.
struct StrArg<'a>(std::borrow::Cow<'a, str>);

impl<'de> serde::Deserialize<'de> for StrArg<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        use std::borrow::Cow;
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = StrArg<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string")
            }
            fn visit_borrowed_str<E: serde::de::Error>(self, v: &'de str) -> std::result::Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Borrowed(v)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Owned(v.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> std::result::Result<StrArg<'de>, E> {
                Ok(StrArg(Cow::Owned(v)))
            }
        }
        deserializer.deserialize_str(V)
    }
}

fn next_arg<'de, A, T>(seq: &mut A, what: &str) -> std::result::Result<T, A::Error>
where
    A: serde::de::SeqAccess<'de>,
    T: serde::Deserialize<'de>,
{
    seq.next_element()?.ok_or_else(|| serde::de::Error::custom(format!("missing {what}")))
}

/// Read an element id. Ids cross napi as JS numbers, so they are read as `f64`
/// and validated.
fn next_id<'de, A: serde::de::SeqAccess<'de>>(seq: &mut A, what: &str) -> std::result::Result<u64, A::Error> {
    let raw: f64 = next_arg(seq, what)?;
    raw_element_id(raw).map_err(serde::de::Error::custom)
}

impl<'de> serde::Deserialize<'de> for BatchOp<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct V;

        impl<'de> serde::de::Visitor<'de> for V {
            type Value = BatchOp<'de>;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a [opcode, ...args] mutation tuple")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<BatchOp<'de>, A::Error> {
                let name: StrArg<'de> = next_arg(&mut seq, "op name")?;
                let op = match name.0.as_ref() {
                    "createElement" => BatchOp::CreateElement {
                        id: next_id(&mut seq, "id")?,
                        element_type: next_arg::<A, StrArg>(&mut seq, "element type")?.0.into_owned(),
                    },
                    "destroyElement" => BatchOp::DestroyElement { id: next_id(&mut seq, "id")? },
                    "appendChild" => BatchOp::AppendChild {
                        parent_id: next_id(&mut seq, "parent id")?,
                        child_id: next_id(&mut seq, "child id")?,
                    },
                    "insertBefore" => BatchOp::InsertBefore {
                        parent_id: next_id(&mut seq, "parent id")?,
                        child_id: next_id(&mut seq, "child id")?,
                        before_id: next_id(&mut seq, "before id")?,
                    },
                    "setStyle" => {
                        BatchOp::SetStyle { id: next_id(&mut seq, "id")?, style: next_arg(&mut seq, "style")? }
                    }
                    "setText" => BatchOp::SetText {
                        id: next_id(&mut seq, "id")?,
                        content: next_arg::<A, StrArg>(&mut seq, "text")?.0.into_owned(),
                    },
                    "setEventListener" => BatchOp::SetEventListener {
                        id: next_id(&mut seq, "id")?,
                        event_type: next_arg::<A, StrArg>(&mut seq, "event type")?.0.into_owned(),
                        has_handler: next_arg(&mut seq, "hasHandler")?,
                    },
                    "setRoot" => BatchOp::SetRoot { id: next_id(&mut seq, "id")? },
                    "setKeyboardFocusDim" => BatchOp::SetKeyboardFocusDim { enabled: next_arg(&mut seq, "enabled")? },
                    "setCustomProp" => BatchOp::SetCustomProp {
                        id: next_id(&mut seq, "id")?,
                        key: next_arg::<A, StrArg>(&mut seq, "prop key")?.0.into_owned(),
                        value: next_arg(&mut seq, "custom prop value")?,
                    },
                    other => return Err(serde::de::Error::custom(format!("unknown operation: {other:?}"))),
                };
                // Trailing arguments are tolerated and ignored.
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(op)
            }
        }

        deserializer.deserialize_seq(V)
    }
}

/// Turn one raw `setStyle` payload into a shared style.
///
fn intern_style_payload(styles: &mut StyleTable, payload: &serde_json::value::RawValue) -> BatchResult<Arc<StyleDesc>> {
    let raw = payload.get().trim();
    styles.intern(raw.as_bytes())
}

/// Resolve every `setStyle` payload in the batch, in op order.
///
/// This is the last fallible step, so it runs before the apply loop and borrows
/// only the style table. The borrow checker then proves no element was touched
/// when it returns `Err`, which is what makes a batch atomic.
fn resolve_styles(styles: &mut StyleTable, ops: &[BatchOp<'_>]) -> BatchResult<Vec<Arc<StyleDesc>>> {
    let mut resolved = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        if let BatchOp::SetStyle { style, .. } = op {
            let shared = intern_style_payload(styles, style)
                .map_err(|error| format!("Batch op {index} setStyle parse error: {error}"))?;
            resolved.push(shared);
        }
    }
    Ok(resolved)
}

/// Apply a batch of mutation tuples to a RetainedTree.
/// Shared between GpuixRenderer::apply_batch and TestGpuixRenderer::apply_batch.
/// Returns accumulated destroyed IDs (as f64) from all destroyElement ops.
///
/// ATOMIC: the batch is decoded and every style is resolved before a single
/// element is touched. If any op is malformed the tree is left unchanged and an
/// error is returned. Nothing after that point can fail, so JS and Rust cannot
/// desync when a batch is retried.
///
/// Batch format: JSON array of tuples [opcode, ...args].
/// See GpuixRenderer::apply_batch for opcode documentation.
///
/// Public so a benchmark times this exact function. A replica in the bench would
/// drift, and the numbers would then describe code nobody runs.
pub fn apply_batch_to_tree(tree: &mut RetainedTree, bytes: &[u8]) -> BatchResult<Vec<f64>> {
    // Phase 1: decode. No mutation.
    let parsed = parse_batch_ops(bytes)?;

    // Phase 2: resolve styles. Touches the style table only; a failure here
    // sweeps back out whatever this call interned.
    let styles = resolve_styles(&mut tree.styles, &parsed).inspect_err(|_| tree.styles.sweep())?;
    let mut styles = styles.into_iter();

    // Phase 3: apply. Cannot fail.
    let mut destroyed_ids: Vec<f64> = Vec::new();
    for batch_op in parsed {
        match batch_op {
            BatchOp::CreateElement { id, element_type } => {
                tree.create_element(id, element_type);
            }
            BatchOp::DestroyElement { id } => {
                let destroyed = tree.destroy_element(id);
                destroyed_ids.extend(destroyed.iter().map(|&id| id as f64));
            }
            BatchOp::AppendChild { parent_id, child_id } => {
                tree.append_child(parent_id, child_id);
            }
            BatchOp::InsertBefore { parent_id, child_id, before_id } => {
                tree.insert_before(parent_id, child_id, before_id);
            }
            BatchOp::SetStyle { id, .. } => {
                let shared = styles.next().expect("one resolved style per setStyle op");
                tree.set_style(id, shared);
            }
            BatchOp::SetText { id, content } => {
                tree.set_text(id, content);
            }
            BatchOp::SetEventListener { id, event_type, has_handler } => {
                tree.set_event_listener(id, event_type, has_handler);
            }
            BatchOp::SetRoot { id } => {
                tree.set_root(id);
            }
            BatchOp::SetKeyboardFocusDim { enabled } => {
                tree.set_keyboard_focus_dim(enabled);
            }
            BatchOp::SetCustomProp { id, key, value } => {
                tree.set_custom_prop(id, key, value);
            }
        }
    }

    // Release styles nothing references any more. Without this a dragged
    // element, which produces a distinct style every frame, would grow the
    // table for as long as the app runs. The element count is what catches the
    // opposite case, a batch that destroyed most of the tree.
    let live_elements = tree.elements.len();
    tree.styles.maybe_sweep(live_elements);

    Ok(destroyed_ids)
}

// ── Types ────────────────────────────────────────────────────────────

type PathPromptReceiver = oneshot::Receiver<anyhow::Result<Option<Vec<std::path::PathBuf>>>>;

type PathPromptResolver = Box<dyn FnOnce(Env) -> Result<Option<Vec<String>>> + Send>;

type PathPromptDeferred = JsDeferred<Option<Vec<String>>, PathPromptResolver>;

fn path_prompt_result(result: anyhow::Result<Option<Vec<std::path::PathBuf>>>) -> Result<Option<Vec<String>>> {
    result
        .map_err(|error| Error::from_reason(error.to_string()))?
        .filter(|paths| !paths.is_empty())
        .map(|paths| {
            paths
                .into_iter()
                .map(|path| {
                    path.into_os_string()
                        .into_string()
                        .map_err(|_| Error::from_reason("The selected path is not valid Unicode"))
                })
                .collect()
        })
        .transpose()
}

fn settle_path_prompt(executor: gpui::BackgroundExecutor, receiver: PathPromptReceiver, deferred: PathPromptDeferred) {
    executor
        .spawn(async move {
            let result = receiver
                .await
                .map_err(|_| Error::from_reason("The path prompt closed without a result"))
                .and_then(path_prompt_result);
            match result {
                Ok(paths) => deferred.resolve(Box::new(move |_env| Ok(paths))),
                Err(error) => deferred.reject(error),
            }
        })
        .detach();
}

#[derive(Debug, Clone, Default)]
#[napi(object)]
pub struct PathPromptOptions {
    /// Select files. Defaults to true unless `directories` is true.
    pub files: Option<bool>,
    /// Select directories. Defaults to false.
    pub directories: Option<bool>,
    /// Allow several paths. Defaults to false.
    pub multiple: Option<bool>,
    /// Label for the picker confirmation button.
    pub prompt: Option<String>,
}

impl PathPromptOptions {
    pub(crate) fn to_gpui(options: Option<Self>) -> Result<gpui::PathPromptOptions> {
        let options = options.unwrap_or_default();
        let directories = options.directories.unwrap_or(false);
        let files = options.files.unwrap_or(!directories);
        if !files && !directories {
            return Err(Error::from_reason("promptForPaths requires files or directories"));
        }
        if files && directories && !cfg!(target_os = "macos") {
            return Err(Error::from_reason("Selecting files and directories together is only supported on macOS"));
        }
        Ok(gpui::PathPromptOptions {
            files,
            directories,
            multiple: options.multiple.unwrap_or(false),
            prompt: options.prompt.map(Into::into),
        })
    }
}

pub struct PromptForPathsTask {
    receiver: Option<Result<PathPromptReceiver>>,
}

impl PromptForPathsTask {
    #[cfg(feature = "test-support")]
    pub(crate) fn new(receiver: Result<PathPromptReceiver>) -> Self {
        Self { receiver: Some(receiver) }
    }
}

#[napi]
impl Task for PromptForPathsTask {
    type Output = Option<Vec<String>>;
    type JsValue = Option<Vec<String>>;

    fn compute(&mut self) -> Result<Self::Output> {
        let receiver = self.receiver.take().ok_or_else(|| Error::from_reason("Path prompt was already awaited"))??;
        let result = futures::executor::block_on(receiver)
            .map_err(|_| Error::from_reason("The path prompt closed without a result"))?;
        path_prompt_result(result)
    }

    fn resolve(&mut self, _env: Env, output: Self::Output) -> Result<Self::JsValue> {
        Ok(output)
    }
}

#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default)]
#[napi(object)]
pub struct EdgeInsets {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

#[derive(Debug, Clone, Default)]
#[napi(object)]
pub struct WindowInsets {
    pub safe_area: EdgeInsets,
    pub ime: EdgeInsets,
    pub effective: EdgeInsets,
}

impl WindowInsets {
    fn from_gpui(insets: gpui::WindowInsets) -> Self {
        let effective = insets.effective();
        Self {
            safe_area: EdgeInsets::from_gpui(insets.safe_area),
            ime: EdgeInsets::from_gpui(insets.ime),
            effective: EdgeInsets::from_gpui(effective),
        }
    }
}

impl EdgeInsets {
    fn from_gpui(insets: gpui::Edges<gpui::Pixels>) -> Self {
        Self {
            top: f32::from(insets.top) as f64,
            right: f32::from(insets.right) as f64,
            bottom: f32::from(insets.bottom) as f64,
            left: f32::from(insets.left) as f64,
        }
    }
}

/// Last painted box for a host element.
#[derive(Debug, Clone)]
#[napi(object)]
pub struct ElementBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ElementBounds {
    pub(crate) fn from_painted(bounds: crate::automation::ElementBounds) -> Self {
        Self { x: bounds.x, y: bounds.y, width: bounds.width, height: bounds.height }
    }
}

/// What one compartment (or the ones of one name) cost since it was last read.
#[derive(Debug, Clone)]
#[napi(object)]
pub struct CompartmentStats {
    /// Whose it is: `owner/name`, a theme's part (`theme.TabBar`), or none.
    pub name: Option<String>,
    /// How many there are of it.
    pub count: u32,
    /// Its renders, and the ones its budget put off.
    pub renders: u32,
    pub deferred: u32,
    /// What its renders took, in all and the longest; what its last one took.
    pub total_ms: f64,
    pub max_ms: f64,
    pub last_ms: f64,
    /// Whether it is drawn on a budget now (slow, and drawn recently).
    pub held: bool,
}

/// Recorded draw times from the debug frame overlay.
#[derive(Debug, Clone)]
#[napi(object)]
pub struct DebugFrameOverlayStats {
    pub current_ms: Option<f64>,
    pub p90_ms: Option<f64>,
    pub p99_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub frames: f64,
    pub samples: f64,
}

/// Wayland `wlr-layer-shell` surface options. Linux/Wayland only; ignored on
/// every other platform. When present on `WindowOptions`, the window is opened
/// as a compositor-anchored surface (a bar, dock, notification overlay or
/// wallpaper) with no native titlebar instead of a normal floating window.
/// `width` / `height` then only constrain the axis the surface is not
/// stretched along by its `anchor`.
#[derive(Debug, Clone, Default)]
#[napi(object)]
pub struct LayerShellOptions {
    /// Compositor surface namespace, used for window rules. Cannot be changed
    /// after the surface is created. Defaults to the empty string.
    pub namespace: Option<String>,
    /// `"background"` | `"bottom"` | `"top"` | `"overlay"`. Defaults to `"top"`.
    pub layer: Option<String>,
    /// Screen edges to anchor to: any combination of `"top"`, `"bottom"`,
    /// `"left"`, `"right"`. Anchoring two opposite edges stretches the surface
    /// across that axis. Defaults to `["top", "left", "right"]` (a top bar).
    pub anchor: Option<Vec<String>>,
    /// Logical pixels to reserve along the anchored edge so other windows do
    /// not overlap the surface. `0` lets the compositor decide, a negative
    /// value asks it not to reserve any space.
    pub exclusive_zone: Option<f64>,
    /// Which edge the exclusive zone applies to when it cannot be inferred from
    /// a single-edge `anchor`. Same values as one `anchor` entry.
    pub exclusive_edge: Option<String>,
    /// Gap between the surface and its anchor edge(s), in CSS order:
    /// `[top, right, bottom, left]`. Must have exactly four entries or it is
    /// ignored.
    pub margin: Option<Vec<f64>>,
    /// `"none"` | `"on-demand"` | `"exclusive"`. Defaults to `"none"`: a bar or
    /// overlay that never takes keyboard focus.
    pub keyboard_interactivity: Option<String>,
}

#[derive(Debug, Clone)]
#[napi(object)]
pub struct WindowOptions {
    pub title: Option<String>,
    /// The name used inside the macOS "Hide" and "Quit" menu items. Defaults to
    /// `title`. It does NOT set the title of the application menu itself: macOS
    /// takes that from the executable, and only a `.app` bundle changes it.
    pub app_name: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub resizable: Option<bool>,
    pub fullscreen: Option<bool>,
    /// Plain alpha transparency. Prefer `window_background` when you need blur.
    pub transparent: Option<bool>,
    /// Hide the native titlebar so the app can draw chrome under the traffic lights. macOS only:
    /// Windows keeps its native title bar, Linux the window manager's.
    pub titlebar_transparent: Option<bool>,
    /// `"opaque"` | `"transparent"` | `"blurred"`. `transparent: true` is the
    /// same as `"transparent"` when this is unset.
    pub window_background: Option<String>,
    /// Where the traffic lights sit (macOS only).
    pub traffic_light_x: Option<f64>,
    pub traffic_light_y: Option<f64>,
    /// Give the window key focus when it opens. `false` opens it behind the
    /// active app, like `open -g`. Ignored on Linux.
    pub focus: Option<bool>,
    /// Show the window when it opens. `false` opens it hidden; call
    /// `activateWindow()` to reveal it. Ignored on Linux.
    pub show: Option<bool>,
    /// Application id: Wayland `app_id` / X11 `WM_CLASS`. Desktop environments
    /// use it to group windows and match window rules. Required in practice for
    /// a `layerShell` surface a compositor is meant to target by rule.
    pub app_id: Option<String>,
    /// Open the window as a Wayland `wlr-layer-shell` surface instead of a
    /// normal window. Linux/Wayland only; ignored elsewhere.
    pub layer_shell: Option<LayerShellOptions>,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: Some("GPUIX".to_string()),
            app_name: None,
            width: Some(800.0),
            height: Some(600.0),
            min_width: None,
            min_height: None,
            resizable: Some(true),
            fullscreen: Some(false),
            transparent: Some(false),
            titlebar_transparent: Some(false),
            window_background: None,
            traffic_light_x: None,
            traffic_light_y: None,
            focus: Some(true),
            show: Some(true),
            app_id: None,
            layer_shell: None,
        }
    }
}

fn to_gpui_window_options(options: &WindowOptions, bounds: gpui::Bounds<gpui::Pixels>) -> gpui::WindowOptions {
    let title = options.title.clone().unwrap_or_else(|| "GPUIX".to_string());
    // A transparent title bar with the app's chrome under the traffic lights is macOS's only:
    // Windows keeps its native title bar (GPUI would hide it and leave the app to draw one), and
    // Linux asks for server-side decorations (X11's window manager, or KDE's Wayland, draws them).
    let macos = cfg!(target_os = "macos");
    let titlebar_transparent = macos && options.titlebar_transparent.unwrap_or(false);
    let traffic_light_position = match (options.traffic_light_x, options.traffic_light_y) {
        (Some(x), Some(y)) if macos => Some(gpui::point(gpui::px(x as f32), gpui::px(y as f32))),
        _ => None,
    };
    let window_decorations = cfg!(target_os = "linux").then_some(gpui::WindowDecorations::Server);
    let window_background = match options.window_background.as_deref() {
        Some("transparent") => gpui::WindowBackgroundAppearance::Transparent,
        Some("blurred") => gpui::WindowBackgroundAppearance::Blurred,
        Some("opaque") => gpui::WindowBackgroundAppearance::Opaque,
        _ if options.transparent.unwrap_or(false) => gpui::WindowBackgroundAppearance::Transparent,
        _ => gpui::WindowBackgroundAppearance::Opaque,
    };
    let window_min_size = match (options.min_width, options.min_height) {
        (Some(width), Some(height)) => Some(gpui::size(gpui::px(width as f32), gpui::px(height as f32))),
        _ => None,
    };
    let window_bounds = if options.fullscreen.unwrap_or(false) {
        gpui::WindowBounds::Fullscreen(bounds)
    } else {
        gpui::WindowBounds::Windowed(bounds)
    };
    #[allow(unused_mut)]
    let mut gpui_options = gpui::WindowOptions {
        window_bounds: Some(window_bounds),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: titlebar_transparent,
            traffic_light_position,
        }),
        is_resizable: options.resizable.unwrap_or(true),
        window_background,
        window_min_size,
        focus: options.focus.unwrap_or(true),
        show: options.show.unwrap_or(true),
        app_id: options.app_id.clone(),
        window_decorations,
        // The app draws its own title bar under the traffic lights, so it moves the window too:
        // only by `appRegion: "drag"` (`apply_app_region`). Left to AppKit, a press anywhere in
        // the top strip moved it, a tab being dragged out of a pane's strip included.
        app_owns_titlebar_drag: titlebar_transparent,
        ..Default::default()
    };

    // A layer-shell surface has no titlebar and is placed by the compositor
    // from its anchor, not by `window_bounds.origin`. The `WindowKind` variant
    // only exists on a Wayland-enabled Linux gpui build.

    gpui_options
}

#[cfg(test)]
mod highlight_cache_tests {
    use super::*;

    fn tree_with_text() -> RetainedTree {
        let mut tree = RetainedTree::new();
        tree.create_element(1, "div".to_string());
        tree.create_element(2, "text".to_string());
        tree.append_child(1, 2);
        tree.set_text(2, "a fox and a fox".to_string());
        tree
    }

    fn query(text: &str) -> serde_json::Value {
        serde_json::json!({ "query": text })
    }

    fn declare(tree: &mut RetainedTree, value: &serde_json::Value) {
        tree.set_custom_prop(1, "highlight".to_string(), value.clone());
    }

    /// The whole reason `search_revision` exists. `highlight` is a custom prop,
    /// so keying the group list on `subtree_revision` means every keystroke
    /// re-walks and re-folds the subtree. The pointer comparison is the proof;
    /// a timing budget over a realistic app is far too coarse to catch it.
    #[test]
    fn a_query_change_reuses_the_group_list() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("f"));
        resolve_highlight(&mut cache, &tree, 1, &query("f"), &theme, false).expect("resolves");
        let first = Arc::as_ptr(&cache[&1].groups);

        declare(&mut tree, &query("fo"));
        resolve_highlight(&mut cache, &tree, 1, &query("fo"), &theme, false).expect("resolves");
        assert_eq!(Arc::as_ptr(&cache[&1].groups), first, "a query change must not rebuild the group list");
    }

    /// Moving a find cursor changes no text and no matcher, so it must re-use
    /// the located matches. Colours and ordinals are decided at paint.
    #[test]
    fn a_cursor_move_reuses_the_located_matches() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();
        let spec = |active: u64| serde_json::json!({ "query": "fox", "activeIndex": active });

        declare(&mut tree, &spec(0));
        resolve_highlight(&mut cache, &tree, 1, &spec(0), &theme, true).expect("resolves");
        let matches = Arc::as_ptr(&cache[&1].context.matches);

        declare(&mut tree, &spec(1));
        let (context, changed) = resolve_highlight(&mut cache, &tree, 1, &spec(1), &theme, true).expect("resolves");
        assert_eq!(Arc::as_ptr(&context.matches), matches, "no rescan");
        assert_eq!(changed, None, "a cursor move is not a new result");
        assert_eq!(context.set.specs[0].active_index, Some(1), "spec still swapped");
    }

    /// Editing the text must invalidate, or the wash paints over stale offsets.
    #[test]
    fn a_text_change_rebuilds_the_group_list() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("fox"));
        resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        let first = Arc::as_ptr(&cache[&1].groups);

        tree.set_text(2, "one fox only".to_string());
        let (_, changed) = resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_ne!(Arc::as_ptr(&cache[&1].groups), first);
        assert_eq!(changed, Some(1), "two matches became one");
    }

    /// `reported` is written only once an event is queued, so mounting without
    /// `onHighlight` and adding it later still reports.
    #[test]
    fn adding_the_listener_later_still_reports() {
        let theme = Theme::dark();
        let mut tree = tree_with_text();
        let mut cache = HashMap::new();

        declare(&mut tree, &query("fox"));
        let (_, changed) = resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, false).expect("resolves");
        assert_eq!(changed, None, "nothing to report without a listener");

        let (_, changed) = resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_eq!(changed, Some(2), "the listener gets the current count");

        let (_, changed) = resolve_highlight(&mut cache, &tree, 1, &query("fox"), &theme, true).expect("resolves");
        assert_eq!(changed, None, "and only once");
    }
}

/// The `applyBatch` protocol. This is the surface JS talks to, so every rule it
/// relies on is asserted here against real JSON bytes rather than through a
/// hand-built `Vec<BatchOp>`.
#[cfg(test)]
mod batch_tests {
    use super::*;

    fn apply(tree: &mut RetainedTree, json: &str) -> BatchResult<Vec<f64>> {
        apply_batch_to_tree(tree, json.as_bytes())
    }

    /// Everything a mutation can reach, so an unwanted partial apply shows up
    /// as a diff instead of hiding in a field the test forgot to read.
    fn describe(tree: &RetainedTree) -> String {
        let mut ids: Vec<_> = tree.elements.keys().copied().collect();
        ids.sort_unstable();
        let mut out = format!("root={:?}\n", tree.root_id);
        for id in ids {
            let element = &tree.elements[&id];
            let mut events: Vec<_> = element.events().iter().cloned().collect();
            events.sort();
            let mut props: Vec<_> = element.custom_props().iter().collect();
            props.sort_by(|(a, _), (b, _)| a.cmp(b));
            out += &format!(
                "{id} type={} text={:?} style={:?} children={:?} parent={:?} events={events:?} props={props:?} rev={}/{}\n",
                element.element_type,
                element.content,
                element.style.as_deref(),
                element.children,
                element.parent,
                element.subtree_revision,
                element.search_revision,
            );
        }
        out
    }

    /// Whether the batch left something a frame would draw differently. The
    /// window is only woken for these, so a wrong `false` is a stale screen and
    /// a wrong `true` is a wasted frame.
    fn dirty_after(tree: &mut RetainedTree, json: &str) -> bool {
        apply(tree, json).expect("valid batch");
        tree.take_dirty()
    }

    const MOUNT: &str = r##"[
        ["createElement",1,"div"],["createElement",2,"text"],["createElement",3,"div"],
        ["appendChild",1,2],["appendChild",1,3],["setRoot",1],
        ["setStyle",1,{"width":100,"backgroundColor":"#112233"}],
        ["setStyle",3,{"height":10}],
        ["setText",2,"hello"],
        ["setEventListener",3,"click",true],
        ["setCustomProp",3,"aria-label","row"],
        ["setCustomProp",3,"testId","row"],
        ["setCustomProp",3,"autoFocus",true]
    ]"##;

    #[test]
    fn mounting_marks_the_tree_dirty() {
        let mut tree = RetainedTree::new();
        assert!(dirty_after(&mut tree, MOUNT));
    }

    #[test]
    fn resending_everything_unchanged_is_not_dirty() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, MOUNT).expect("valid batch");
        tree.take_dirty();

        // The same ops again, with the style keys reordered: interning keys on
        // raw bytes, so this reaches `set_style` as a different `Arc`.
        let resend = r##"[
            ["setRoot",1],
            ["setStyle",1,{"backgroundColor":"#112233","width":100}],
            ["setStyle",3,{"height":10}],
            ["setText",2,"hello"],
            ["setEventListener",3,"click",true],
            ["setCustomProp",3,"aria-label","row"],
            ["setCustomProp",3,"testId","row"],
            ["setCustomProp",3,"autoFocus",true],
            ["setKeyboardFocusDim",true],
            ["setCustomProp",3,"missing",null]
        ]"##;
        assert!(!dirty_after(&mut tree, resend));
    }

    #[test]
    fn each_kind_of_change_is_dirty() {
        let changes = [
            r#"[["setStyle",3,{"height":11}]]"#,
            r#"[["setText",2,"changed"]]"#,
            r#"[["setEventListener",3,"click",false]]"#,
            r#"[["setEventListener",3,"mouseDown",true]]"#,
            r#"[["setCustomProp",3,"aria-label","other"]]"#,
            r#"[["setCustomProp",3,"aria-label",null]]"#,
            r#"[["setCustomProp",3,"testId","other"]]"#,
            r#"[["setCustomProp",3,"autoFocus",false]]"#,
            r#"[["setKeyboardFocusDim",false]]"#,
            r#"[["createElement",4,"div"]]"#,
            r#"[["destroyElement",3]]"#,
            r#"[["insertBefore",1,3,2]]"#,
            r#"[["setRoot",3]]"#,
        ];
        for change in changes {
            let mut tree = RetainedTree::new();
            apply(&mut tree, MOUNT).expect("valid batch");
            tree.take_dirty();
            assert!(dirty_after(&mut tree, change), "{change} must repaint");
        }
    }

    #[test]
    fn a_rejected_batch_does_not_leave_the_tree_dirty() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, MOUNT).expect("valid batch");
        tree.take_dirty();

        apply(&mut tree, r#"[["setText",2,"changed"],["setStyle",2,123]]"#).expect_err("malformed style");

        assert!(!tree.take_dirty());
    }

    /// The regression test for batch atomicity: a malformed style late in a
    /// batch leaves nothing applied, so JS can retry against a tree that has
    /// not moved.
    #[test]
    fn a_malformed_style_applies_nothing_at_all() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"],["setRoot",1]]"#).expect("valid batch");
        let before = describe(&tree);
        let styles_before = tree.styles.len();

        let error = apply(&mut tree, r#"[["createElement",2,"div"],["setText",2,"changed"],["setStyle",2,123]]"#)
            .expect_err("a malformed style must reject the batch");

        assert_eq!(describe(&tree), before, "the tree must be untouched");
        assert_eq!(tree.styles.len(), styles_before, "the failed batch must not leave styles interned");
        assert!(error.contains("setStyle"), "{error}");
    }

    /// A style that fails halfway through a long batch is unfindable without
    /// its index; serde reports a byte offset, which names nothing.
    #[test]
    fn a_style_error_names_its_op_index() {
        let mut tree = RetainedTree::new();
        let error = apply(
            &mut tree,
            r#"[["createElement",1,"div"],["setStyle",1,{"color":"red"}],["setStyle",1,{"color":5}]]"#,
        )
        .expect_err("a bad style rejects the batch");
        assert!(error.starts_with("Batch op 2 setStyle parse error:"), "{error}");
    }

    /// `null` is not "no style". Treating it as `{}` would silently clear every
    /// declared property instead of telling JS it sent something wrong.
    #[test]
    fn a_null_style_is_an_error() {
        let mut tree = RetainedTree::new();
        let error =
            apply(&mut tree, r#"[["createElement",1,"div"],["setStyle",1,null]]"#).expect_err("null is not a style");
        assert!(error.contains("Batch op 1 setStyle parse error:"), "{error}");
        assert!(tree.elements.is_empty(), "and the batch stays atomic");
    }

    /// Skipping an unknown opcode would let a JS/Rust version skew desync the
    /// tree quietly. It has to throw.
    #[test]
    fn an_unknown_opcode_is_an_error() {
        let mut tree = RetainedTree::new();
        let error = apply(&mut tree, r#"[["teleportElement",1]]"#).expect_err("unknown opcode");
        assert!(error.contains("unknown operation"), "{error}");
        assert!(tree.elements.is_empty());
    }

    /// Every op that takes an id must validate it. A fractional or oversized id
    /// would truncate into a *different* element, which is a silent desync.
    #[test]
    fn an_invalid_id_is_rejected_in_every_id_position() {
        let templates = [
            r#"[["createElement",ID,"div"]]"#,
            r#"[["destroyElement",ID]]"#,
            r#"[["appendChild",ID,2]]"#,
            r#"[["appendChild",1,ID]]"#,
            r#"[["insertBefore",ID,2,3]]"#,
            r#"[["insertBefore",1,ID,3]]"#,
            r#"[["insertBefore",1,2,ID]]"#,
            r#"[["setStyle",ID,{}]]"#,
            r#"[["setText",ID,"x"]]"#,
            r#"[["setEventListener",ID,"click",true]]"#,
            r#"[["setRoot",ID]]"#,
            r#"[["setCustomProp",ID,"k",1]]"#,
        ];
        // 1e999 overflows f64, 9007199254740992 is Number.MAX_SAFE_INTEGER + 1.
        let bad_ids = ["-1", "1.5", "9007199254740992", "1e999"];

        for template in templates {
            for bad in bad_ids {
                let json = template.replace("ID", bad);
                let mut tree = RetainedTree::new();
                let error = apply(&mut tree, &json).expect_err(&format!("{json} must be rejected"));
                assert!(error.contains("Batch op 0"), "{json}: {error}");
                assert!(tree.elements.is_empty(), "{json} mutated the tree");
                assert_eq!(tree.root_id, None, "{json} mutated the root");
            }
        }
    }

    #[test]
    fn has_handler_requires_a_bool() {
        for (payload, expected) in [("true", true), ("false", false)] {
            let mut tree = RetainedTree::new();
            let json = format!(r#"[["createElement",1,"div"],["setEventListener",1,"click",{payload}]]"#);
            apply(&mut tree, &json).expect("boolean handler state");
            assert_eq!(tree.elements[&1].events().contains("click"), expected, "hasHandler {payload}");
        }

        for payload in ["0", "1", "0.5", r#""true""#] {
            let mut tree = RetainedTree::new();
            let json = format!(r#"[["createElement",1,"div"],["setEventListener",1,"click",{payload}]]"#);
            apply(&mut tree, &json).expect_err(&format!("hasHandler {payload} is not a boolean"));
        }
    }

    #[test]
    fn a_malformed_op_tuple_is_an_error() {
        let cases = [
            (r#"[42]"#, "a non-array op"),
            (r#"[["createElement",1]]"#, "a missing argument"),
            (r#"[[7,1,"div"]]"#, "a non-string op name"),
        ];
        for (json, what) in cases {
            let mut tree = RetainedTree::new();
            let error = apply(&mut tree, json).expect_err(what);
            assert!(error.starts_with("Failed to parse batch:"), "{what}: {error}");
            assert!(tree.elements.is_empty(), "{what} mutated the tree");
        }
    }

    /// Interning keys on raw bytes, so re-ordered keys are two `Arc`s. They are
    /// still the same style, and a repaint per key order would be a real cost
    /// on any app that builds style objects conditionally.
    #[test]
    fn a_reordered_style_does_not_repaint() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"],["setStyle",1,{"color":"red","left":10}]]"#)
            .expect("valid batch");
        let revision = tree.elements[&1].subtree_revision;

        apply(&mut tree, r#"[["setStyle",1,{"left":10,"color":"red"}]]"#).expect("valid batch");
        assert_eq!(tree.elements[&1].subtree_revision, revision, "the same style in another key order is not a change");
    }

    /// Three ways an interned style loses its last element reference.
    #[test]
    fn a_style_is_released_when_nothing_references_it() {
        let mut tree = RetainedTree::new();
        apply(&mut tree, r#"[["createElement",1,"div"]]"#).expect("valid batch");

        // Set on an id that does not exist: nothing keeps the style alive.
        apply(&mut tree, r#"[["setStyle",99,{"color":"red"}]]"#).expect("missing ids are ignored");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 0, "a style nobody took must be released");

        apply(&mut tree, r#"[["setStyle",1,{"color":"red"}]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1);

        // Replaced.
        apply(&mut tree, r#"[["setStyle",1,{"color":"blue"}]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 1, "the replaced style must be released");

        // Destroyed.
        apply(&mut tree, r#"[["destroyElement",1]]"#).expect("valid batch");
        tree.styles.sweep();
        assert_eq!(tree.styles.len(), 0);
    }
}

#[cfg(test)]
mod path_prompt_options_tests {
    use super::*;

    #[test]
    fn defaults_to_one_file_and_maps_directory_prompts() {
        let defaults = PathPromptOptions::to_gpui(None).expect("default prompt");
        assert!(defaults.files);
        assert!(!defaults.directories);
        assert!(!defaults.multiple);
        assert!(defaults.prompt.is_none());

        let directories = PathPromptOptions::to_gpui(Some(PathPromptOptions {
            directories: Some(true),
            multiple: Some(true),
            prompt: Some("Choose folder".to_string()),
            ..PathPromptOptions::default()
        }))
        .expect("directory prompt");
        assert!(!directories.files);
        assert!(directories.directories);
        assert!(directories.multiple);
        assert_eq!(directories.prompt.as_deref(), Some("Choose folder"));
    }

    #[test]
    fn rejects_a_prompt_that_selects_nothing() {
        let error = PathPromptOptions::to_gpui(Some(PathPromptOptions {
            files: Some(false),
            directories: Some(false),
            ..PathPromptOptions::default()
        }))
        .expect_err("an empty path prompt must fail");
        assert!(error.reason.contains("requires files or directories"));
    }

    #[test]
    fn mixed_selection_matches_platform_support() {
        let result = PathPromptOptions::to_gpui(Some(PathPromptOptions {
            files: Some(true),
            directories: Some(true),
            ..PathPromptOptions::default()
        }));
        if cfg!(target_os = "macos") {
            assert!(result.is_ok());
        } else {
            assert!(result.expect_err("mixed selection must fail").reason.contains("only supported on macOS"));
        }
    }

    #[test]
    fn an_empty_platform_selection_is_cancellation() {
        assert_eq!(path_prompt_result(Ok(Some(Vec::new()))).unwrap(), None);
    }
}

#[cfg(test)]
mod window_options_tests {
    use super::*;

    fn mapped(options: WindowOptions) -> gpui::WindowOptions {
        let bounds = gpui::Bounds {
            origin: gpui::point(gpui::px(0.0), gpui::px(0.0)),
            size: gpui::size(gpui::px(800.0), gpui::px(600.0)),
        };
        to_gpui_window_options(&options, bounds)
    }

    #[test]
    fn defaults_open_a_focused_visible_window() {
        let gpui_options = mapped(WindowOptions::default());
        assert!(gpui_options.focus);
        assert!(gpui_options.show);
    }

    #[test]
    fn unset_focus_and_show_still_default_to_true() {
        let gpui_options = mapped(WindowOptions { focus: None, show: None, ..WindowOptions::default() });
        assert!(gpui_options.focus);
        assert!(gpui_options.show);
    }

    #[test]
    fn focus_false_leaves_the_window_visible() {
        let gpui_options = mapped(WindowOptions { focus: Some(false), ..WindowOptions::default() });
        assert!(!gpui_options.focus);
        assert!(gpui_options.show);
    }

    #[test]
    fn show_false_keeps_focus_independent() {
        let gpui_options = mapped(WindowOptions { show: Some(false), ..WindowOptions::default() });
        assert!(!gpui_options.show);
        assert!(gpui_options.focus);
    }

    #[test]
    fn existing_options_are_still_mapped() {
        let gpui_options = mapped(WindowOptions {
            title: Some("Background".to_string()),
            resizable: Some(false),
            window_background: Some("blurred".to_string()),
            min_width: Some(320.0),
            min_height: Some(240.0),
            focus: Some(false),
            ..WindowOptions::default()
        });
        let titlebar = gpui_options.titlebar.expect("titlebar options");
        assert_eq!(titlebar.title.as_deref(), Some("Background"));
        assert!(!gpui_options.is_resizable);
        assert_eq!(gpui_options.window_background, gpui::WindowBackgroundAppearance::Blurred);
        assert_eq!(gpui_options.window_min_size, Some(gpui::size(gpui::px(320.0), gpui::px(240.0))));
    }

    #[test]
    fn app_id_is_forwarded() {
        let gpui_options = mapped(WindowOptions { app_id: Some("kite-panel".to_string()), ..WindowOptions::default() });
        assert_eq!(gpui_options.app_id.as_deref(), Some("kite-panel"));
    }

    #[test]
    fn the_title_bar_is_transparent_on_macos_only_and_linux_asks_for_server_decorations() {
        let gpui_options = mapped(WindowOptions {
            titlebar_transparent: Some(true),
            traffic_light_x: Some(12.0),
            traffic_light_y: Some(12.0),
            ..WindowOptions::default()
        });
        let titlebar = gpui_options.titlebar.expect("titlebar options");
        let macos = cfg!(target_os = "macos");
        assert_eq!(titlebar.appears_transparent, macos);
        assert_eq!(titlebar.traffic_light_position.is_some(), macos);
        // The app's own title bar moves the window (`appRegion`), not AppKit's.
        assert_eq!(gpui_options.app_owns_titlebar_drag, macos);
        let server = matches!(gpui_options.window_decorations, Some(gpui::WindowDecorations::Server));
        assert_eq!(server, cfg!(target_os = "linux"));
    }

    #[test]
    fn without_layer_shell_the_window_is_normal_with_a_titlebar() {
        let gpui_options = mapped(WindowOptions::default());
        assert_eq!(gpui_options.kind, gpui::WindowKind::Normal);
        assert!(gpui_options.titlebar.is_some());
        assert!(!gpui_options.app_owns_titlebar_drag);
    }
}

/// Rejects fonts GPUI's text system cannot load. It unwraps the family, full and
/// PostScript name records and aborts the whole process when one is missing, and
/// skips a font without an "m" glyph, which would leave the family unresolvable.
fn check_font(bytes: &[u8]) -> std::result::Result<(), String> {
    use ttf_parser::name_id::{FAMILY, FULL_NAME, POST_SCRIPT_NAME};

    let face =
        ttf_parser::Face::parse(bytes, 0).map_err(|error| format!("not a TrueType or OpenType font: {error}"))?;
    for (id, what) in [(FAMILY, "family"), (FULL_NAME, "full"), (POST_SCRIPT_NAME, "PostScript")] {
        let named = face.names().into_iter().any(|name| name.name_id == id && name.to_string().is_some());
        if !named {
            return Err(format!("font has no {what} name"));
        }
    }
    if face.glyph_index('m').is_none() {
        return Err("font has no glyph for \"m\"".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod inherited_tests {
    use super::*;

    fn coloured(color: &str) -> StyleDesc {
        StyleDesc { color: Some(color.to_owned()), ..Default::default() }
    }

    #[test]
    fn color_cascades_to_descendants_that_set_none() {
        let button = Inherited::root(&Theme::dark()).descend(Some(&coloured("#3a2d44")));
        // An icon inside the button, with no `color` of its own, tints with the button's.
        let icon = button.descend(Some(&StyleDesc::default()));
        assert_eq!(icon.color, crate::color::parse_color_rgba("#3a2d44"));
    }

    #[test]
    fn an_own_color_wins_over_the_inherited_one() {
        let icon =
            Inherited::root(&Theme::dark()).descend(Some(&coloured("#ffffff"))).descend(Some(&coloured("#000000")));
        assert_eq!(icon.color, crate::color::parse_color_rgba("#000000"));
    }

    #[test]
    fn nothing_coloured_above_leaves_no_color() {
        assert_eq!(Inherited::root(&Theme::dark()).descend(None).color, None);
    }
}
