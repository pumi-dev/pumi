//! The addon's renderer: `GpuixRenderer`, the JS class over [`crate::Renderer`]. Ids cross as
//! JS numbers and are checked here; events are flattened to `EventPayload`
//! (`crate::addon::events`).
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::JsDeferred;
use napi_derive::napi;
use pumi_ui_proto::Event;

use super::events::EventPayload;
use super::{dimension_u32, js, to_element_id};
use crate::renderer::{
    CompartmentStats, DebugFrameOverlayStats, ElementBounds, EventCallback, PathPromptOptions, Renderer, WindowInsets,
    WindowOptions, WindowSize,
};

type PathPromptResolver = Box<dyn FnOnce(Env) -> Result<Option<Vec<String>>> + Send>;

type PathPromptDeferred = JsDeferred<Option<Vec<String>>, PathPromptResolver>;

/// One window and its retained tree. Every renderer opens its window in the process's one app
/// (`crate::app`).
#[napi]
pub struct GpuixRenderer {
    inner: Renderer,
}

#[napi]
impl GpuixRenderer {
    #[napi(constructor)]
    pub fn new(event_callback: Option<ThreadsafeFunction<EventPayload>>) -> Self {
        let on_event = event_callback.map(|tsf| {
            Arc::new(move |event: Event| {
                tsf.call(Ok(event.into()), ThreadsafeFunctionCallMode::NonBlocking);
            }) as EventCallback
        });
        Self { inner: Renderer::new(on_event) }
    }

    /// Opens this renderer's window, starting the app first if no window has.
    #[napi]
    pub fn init(&self, options: Option<WindowOptions>) -> Result<()> {
        self.inner.init(options).map_err(js)
    }

    /// Closes the window. The app keeps running.
    #[napi]
    pub fn close(&self) -> Result<()> {
        self.inner.close().map_err(js)
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
        let destroyed = self.inner.apply_batch(json.as_bytes()).map_err(js)?;
        Ok(destroyed.into_iter().map(|id| id as f64).collect())
    }

    // ── Frame loop ───────────────────────────────────────────────────

    /// Pump the native event loop. Returns false once the app has quit.
    #[napi]
    pub fn tick(&self) -> bool {
        self.inner.tick()
    }

    #[napi]
    pub fn is_initialized(&self) -> bool {
        self.inner.is_initialized()
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
        self.inner.frame_wake_fallback_ms()
    }

    /// Whether JavaScript must call tick(): always, the platform's loop (AppKit, Win32's
    /// messages, calloop) is pumped from the JS thread.
    #[napi]
    pub fn requires_tick(&self) -> bool {
        self.inner.requires_tick()
    }

    /// The paintable size of the window in logical pixels, excluding any
    /// platform title bar.
    #[napi]
    pub fn get_window_size(&self) -> Result<WindowSize> {
        self.inner.get_window_size().map_err(js)
    }

    #[napi]
    pub fn get_window_insets(&self) -> Result<WindowInsets> {
        self.inner.get_window_insets().map_err(js)
    }

    /// `"opaque"` | `"transparent"` | `"blurred"`: what shows where the tree paints nothing.
    /// `"blurred"` is the system's vibrancy behind the window.
    #[napi]
    pub fn set_window_background(&self, mode: String) -> Result<()> {
        self.inner.set_window_background(mode).map_err(js)
    }

    /// How much what shows through the window is blurred, in points (0 is not at all): the
    /// window server's blur, seen where the tree paints nothing or paints with an alpha while the
    /// background is not `"opaque"`. macOS only; elsewhere it does nothing.
    #[napi]
    pub fn set_window_blur(&self, radius: f64) -> Result<()> {
        self.inner.set_window_blur(radius).map_err(js)
    }

    /// How much of what is behind the window its backgrounds hide, 0–1 (1, the default, is all
    /// of it): every background the tree paints lets the rest through, however many are stacked,
    /// and keeps its own colour, while text, icons and images stay opaque. Seen while the
    /// background is not `"opaque"`. macOS only; elsewhere it does nothing.
    #[napi]
    pub fn set_window_glass(&self, opacity: f64) -> Result<()> {
        self.inner.set_window_glass(opacity).map_err(js)
    }

    /// Whether the window is full screen now (the traffic lights are hidden then). Changes
    /// reach the window's handler as `windowFullscreen`, `hovered` saying which.
    #[napi]
    pub fn is_fullscreen(&self) -> Result<bool> {
        self.inner.is_fullscreen().map_err(js)
    }

    /// The style every `title` tooltip is drawn in (a style object as JSON), or null for the
    /// renderer's own. It applies to every window.
    #[napi]
    pub fn set_tooltip_style(&self, style: Option<String>) -> Result<()> {
        self.inner.set_tooltip_style(style).map_err(js)
    }

    /// `"hidden"` | `"minimal"` | `"full"`. Paints into the scene after layout.
    #[napi]
    pub fn set_debug_frame_overlay(&self, mode: String) -> Result<String> {
        self.inner.set_debug_frame_overlay(mode).map_err(js)
    }

    /// Hidden → minimal → full → hidden.
    #[napi]
    pub fn cycle_debug_frame_overlay(&self) -> Result<String> {
        self.inner.cycle_debug_frame_overlay().map_err(js)
    }

    #[napi]
    pub fn get_debug_frame_overlay(&self) -> Result<String> {
        self.inner.get_debug_frame_overlay().map_err(js)
    }

    /// Clears the last 1000 draw samples. Frame count stays.
    #[napi]
    pub fn reset_debug_frame_overlay_stats(&self) -> Result<()> {
        self.inner.reset_debug_frame_overlay_stats().map_err(js)
    }

    /// Same numbers as the on-screen overlay: current, p90, p99, max, frames.
    #[napi]
    pub fn get_debug_frame_overlay_stats(&self) -> Result<DebugFrameOverlayStats> {
        self.inner.get_debug_frame_overlay_stats().map_err(js)
    }

    /// What each compartment's renders cost since the last call (a plugin's component, a theme's part:
    /// `compartment="owner/name"`), the compartments with the same name together, and whether
    /// any of them is drawn on a budget now. Reading it starts the counts again.
    #[napi]
    pub fn get_compartment_stats(&self) -> Result<Vec<CompartmentStats>> {
        self.inner.get_compartment_stats().map_err(js)
    }

    /// Bring the window forward and give it focus. This is how a window opened
    /// with `show: false` or `focus: false` is revealed later.
    #[napi]
    pub fn activate_window(&self) -> Result<()> {
        self.inner.activate_window().map_err(js)
    }

    /// Whether closing the window hides the app instead of closing it (the app's last window).
    #[napi]
    pub fn set_hide_on_close(&self, hide: bool) {
        self.inner.set_hide_on_close(hide)
    }

    /// Minimize the native window.
    #[napi]
    pub fn minimize_window(&self) -> Result<()> {
        self.inner.minimize_window().map_err(js)
    }

    /// Run the native zoom or maximize operation.
    #[napi]
    pub fn zoom_window(&self) -> Result<()> {
        self.inner.zoom_window().map_err(js)
    }

    /// Enter or exit native fullscreen.
    #[napi]
    pub fn toggle_fullscreen(&self) -> Result<()> {
        self.inner.toggle_fullscreen().map_err(js)
    }

    #[napi]
    pub fn set_window_title(&self, title: String) -> Result<()> {
        self.inner.set_window_title(title).map_err(js)
    }

    /// Open the platform path picker. Returns null when the user cancels.
    #[napi(ts_return_type = "Promise<Array<string> | null>")]
    pub fn prompt_for_paths<'env>(&self, env: &'env Env, options: Option<PathPromptOptions>) -> Result<Object<'env>> {
        let (deferred, promise): (PathPromptDeferred, Object<'env>) = env.create_deferred()?;
        match PathPromptOptions::to_gpui(options) {
            Ok(options) => self.inner.prompt_for_paths(options, move |result| match result {
                Ok(paths) => deferred.resolve(Box::new(move |_env| Ok(paths))),
                Err(error) => deferred.reject(js(error)),
            }),
            Err(error) => deferred.reject(js(error)),
        }
        Ok(promise)
    }

    #[napi]
    pub fn focus_element(&self, element_id: f64) -> Result<()> {
        self.inner.focus_element(to_element_id(element_id)?).map_err(js)
    }

    /// Move focus to the next GPUI tab stop.
    #[napi]
    pub fn focus_next(&self) -> Result<()> {
        self.inner.focus_next().map_err(js)
    }

    /// Move focus to the previous GPUI tab stop.
    #[napi]
    pub fn focus_previous(&self) -> Result<()> {
        self.inner.focus_previous().map_err(js)
    }

    /// Host id of the focused element, or null when nothing is focused.
    #[napi]
    pub fn get_focused_element_id(&self) -> Result<Option<f64>> {
        self.inner.get_focused_element_id().map(|id| id.map(|id| id as f64)).map_err(js)
    }

    /// Move focus to the next tab stop inside `element_id`, wrapping in that subtree.
    #[napi]
    pub fn focus_next_within(&self, element_id: f64) -> Result<()> {
        self.inner.focus_next_within(to_element_id(element_id)?).map_err(js)
    }

    /// Move focus to the previous tab stop inside `element_id`, wrapping in that subtree.
    #[napi]
    pub fn focus_previous_within(&self, element_id: f64) -> Result<()> {
        self.inner.focus_previous_within(to_element_id(element_id)?).map_err(js)
    }

    /// Enable the window key events requested by the React renderer.
    #[napi]
    pub fn set_window_key_events(&self, key_down: bool, key_up: bool, event_id: f64) -> Result<()> {
        self.inner.set_window_key_events(key_down, key_up, to_element_id(event_id)?).map_err(js)
    }

    #[napi]
    pub fn blur(&self) -> Result<()> {
        self.inner.blur().map_err(js)
    }

    /// Enable the window selectionChange event requested by the React renderer.
    #[napi]
    pub fn set_window_selection_change(&self, enabled: bool, event_id: f64) -> Result<()> {
        self.inner.set_window_selection_change(enabled, to_element_id(event_id)?).map_err(js)
    }

    /// The current text selection joined in document order, or null.
    #[napi]
    pub fn get_selected_text(&self) -> Option<String> {
        self.inner.get_selected_text()
    }

    /// Drop the current selection and request a repaint.
    #[napi]
    pub fn clear_selection(&self) -> Result<()> {
        self.inner.clear_selection().map_err(js)
    }

    /// Set the scroll offset of a scrollable element.
    /// x and y are negative pixel values (scroll down = more negative y).
    #[napi]
    pub fn scroll_to(&self, element_id: f64, x: f64, y: f64) -> Result<()> {
        self.inner.scroll_to(to_element_id(element_id)?, x, y).map_err(js)
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
        self.inner.scroll_to_item(to_element_id(element_id)?, index as usize, offset_in_item).map_err(js)
    }

    /// The logical scroll anchor of a `<virtual-list>`:
    /// `[itemIndex, offsetInItemPx, viewportHeightPx]`, or null for anything
    /// else. `itemIndex == item count` is gpui's at-end sentinel.
    ///
    /// Unlike `getScrollOffset` this is exact even while row heights are still
    /// estimates, because it is the anchor gpui itself scrolls by.
    #[napi]
    pub fn get_list_scroll_top(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        self.inner.get_list_scroll_top(to_element_id(element_id)?).map_err(js)
    }

    /// Get the current scroll offset of a scrollable element.
    /// Returns [x, y] or null if the element has no scroll handle.
    #[napi]
    pub fn get_scroll_offset(&self, element_id: f64) -> Result<Option<Vec<f64>>> {
        self.inner.get_scroll_offset(to_element_id(element_id)?).map_err(js)
    }

    /// Scroll this element's nearest scroll parent until the element is visible.
    #[napi]
    pub fn scroll_into_view(&self, element_id: f64) -> Result<()> {
        self.inner.scroll_into_view(to_element_id(element_id)?).map_err(js)
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
        self.inner
            .set_image_pixels(
                to_element_id(element_id)?,
                dimension_u32(width, "width")?,
                dimension_u32(height, "height")?,
                pixels.to_vec(),
                format,
            )
            .map_err(js)
    }

    /// Decode PNG, JPEG, WebP, GIF, SVG, BMP, TIFF, ICO, or Netpbm bytes onto
    /// an `<img>` host node. Prefer `setImagePixels` for live waveforms.
    #[napi]
    pub fn set_image(&self, element_id: f64, bytes: Buffer) -> Result<()> {
        self.inner.set_image(to_element_id(element_id)?, bytes.to_vec()).map_err(js)
    }

    /// Make the fonts in a TrueType or OpenType file (or a collection) usable by
    /// `fontFamily`, under the family names the file declares. Call it before
    /// the text that names the family is first laid out: a family that did not
    /// exist then has already fallen back to another font.
    #[napi]
    pub fn register_font(&self, data: Buffer) -> Result<()> {
        self.inner.register_font(data.to_vec()).map_err(js)
    }

    #[napi]
    pub fn get_automation_tree(&self) -> Result<String> {
        self.inner.get_automation_tree().map_err(js)
    }

    #[napi]
    pub fn get_element_bounds(&self, id: f64) -> Result<Option<ElementBounds>> {
        self.inner.get_element_bounds(to_element_id(id)?).map_err(js)
    }

    /// The elements whose last painted box holds the window point `(x, y)`,
    /// topmost first, like the DOM's `elementsFromPoint`. An element that did
    /// not paint is never listed, `overflow: hidden | scroll` clips what lies
    /// outside it, and `pointer-events: none` is skipped.
    #[napi]
    pub fn get_elements_at(&self, x: f64, y: f64) -> Result<Vec<f64>> {
        self.inner.get_elements_at(x, y).map(|ids| ids.into_iter().map(|id| id as f64).collect()).map_err(js)
    }

    #[napi]
    pub fn get_all_text(&self) -> Vec<String> {
        self.inner.get_all_text()
    }

    #[napi]
    pub fn get_painted_text(&self) -> Vec<String> {
        self.inner.get_painted_text()
    }

    /// Every highlight wash painted in the last frame, in paint order.
    ///
    /// A quad is invisible to `getPaintedText()`, so this is the only way to
    /// assert on `highlight` without a screenshot.
    #[napi]
    pub fn get_painted_highlights(&self) -> Vec<crate::element_tree::HighlightMatch> {
        self.inner.get_painted_highlights()
    }

    /// Simulate space-separated keystrokes through the focused element's input pipeline.
    #[napi]
    pub fn simulate_keystrokes(&self, keystrokes: String) -> Result<()> {
        self.inner.simulate_keystrokes(keystrokes).map_err(js)
    }

    #[napi]
    pub fn simulate_key_down(&self, keystroke: String, is_held: Option<bool>) -> Result<()> {
        self.inner.simulate_key_down(keystroke, is_held).map_err(js)
    }

    #[napi]
    pub fn simulate_key_up(&self, keystroke: String) -> Result<()> {
        self.inner.simulate_key_up(keystroke).map_err(js)
    }

    /// `modifiers` uses the `press()` syntax: "cmd", "cmd-shift", "alt".
    #[napi]
    pub fn simulate_click(&self, x: f64, y: f64, button: Option<u32>, modifiers: Option<String>) -> Result<()> {
        self.inner.simulate_click(x, y, button, modifiers).map_err(js)
    }

    #[napi]
    pub fn simulate_mouse_down(&self, x: f64, y: f64, button: Option<u32>, modifiers: Option<String>) -> Result<()> {
        self.inner.simulate_mouse_down(x, y, button, modifiers).map_err(js)
    }

    #[napi]
    pub fn simulate_mouse_up(&self, x: f64, y: f64, button: Option<u32>, modifiers: Option<String>) -> Result<()> {
        self.inner.simulate_mouse_up(x, y, button, modifiers).map_err(js)
    }

    #[napi]
    pub fn simulate_mouse_move(
        &self,
        x: f64,
        y: f64,
        pressed_button: Option<u32>,
        modifiers: Option<String>,
    ) -> Result<()> {
        self.inner.simulate_mouse_move(x, y, pressed_button, modifiers).map_err(js)
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
        self.inner.simulate_scroll_wheel(x, y, delta_x, delta_y, modifiers).map_err(js)
    }

    #[napi]
    pub fn clock_pause(&self) -> Result<f64> {
        self.inner.clock_pause().map_err(js)
    }

    #[napi]
    pub fn clock_set(&self, now_ms: f64) -> Result<f64> {
        self.inner.clock_set(now_ms).map_err(js)
    }

    #[napi]
    pub fn clock_fast_forward(&self, delta_ms: f64) -> Result<f64> {
        self.inner.clock_fast_forward(delta_ms).map_err(js)
    }

    #[napi]
    pub fn clock_resume(&self) -> Result<f64> {
        self.inner.clock_resume().map_err(js)
    }

    #[napi]
    pub fn capture_screenshot(&self, path: String) -> Result<()> {
        self.inner.capture_screenshot(path).map_err(js)
    }
}
