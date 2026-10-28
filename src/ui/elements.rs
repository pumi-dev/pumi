//! The app's elements, registered with the renderer before any window opens.
//!
//! `<terminal tab="%3">` (or a tab's number, or a plugin's `{ id, server }`) is the tab's live terminal: it attaches when it mounts, sizes the
//! program to its box, and takes keys while focused (`autoFocus`, or a click). Unmounted (its view
//! hidden), it stays attached for a while, so mounting it again shows it at once. Keys it doesn't
//! use (`cmd+…`) bubble to the window for the app's bindings. `<preview tab="%3">` is the tab's
//! screen scaled into its box, attaching nothing. `<webview handle src>` is a web page (below).
use crate::ui::terminal::{TerminalEvent, TerminalView};
use gpui::{
    AppContext as _, Entity, Focusable as _, InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
};
use pumi_gpuix::GpuixView;
use pumi_gpuix::custom_elements::{CustomElement, CustomElementFactory, CustomRenderContext};
use serde_json::{Value, json};

/// A `tab` prop as the app's wire id (`%3`, `devbox/%3`): the id itself, a tab's number, or a
/// plugin's ref (`{ id: 3, server: "devbox" }`).
fn tab_id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) => Some(id.clone()),
        Value::Number(n) => n.as_u64().map(|n| format!("%{n}")),
        Value::Object(map) => {
            let id = map.get("id")?.as_u64()?;
            Some(match map.get("server").and_then(Value::as_str) {
                Some(server) => format!("{server}/%{id}"),
                None => format!("%{id}"),
            })
        }
        _ => None,
    }
}
pub fn register() {
    pumi_gpuix::custom_elements::register(|| Box::new(Terminal));
    pumi_gpuix::custom_elements::register(|| Box::new(Preview));
    pumi_gpuix::custom_elements::register(|| Box::new(Web));
}

struct Terminal;
impl CustomElementFactory for Terminal {
    fn element_type(&self) -> &str {
        "terminal"
    }
    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(TerminalElement::default())
    }
}

#[derive(Default)]
struct TerminalElement {
    tab: Option<String>,
    auto_focus: bool,
    focused_once: bool,
    /// The window it is drawn in, which its terminal is put away for.
    window: Option<gpui::WindowId>,
    view: Option<(String, Entity<TerminalView>, gpui::Subscription)>,
}

/// The most terminals the app keeps attached while no element draws them; past it, the one put
/// away longest ago is let go, and attaches anew when it is drawn again.
const PUT_AWAY_MAX: usize = 16;

/// A terminal no element draws for now (its view is hidden), still attached, its screen kept, so
/// drawing it again shows it as it is at once: nothing to attach, no screen to replay and no size
/// to tell its program.
struct PutAway {
    window: gpui::WindowId,
    tab: String,
    view: Entity<TerminalView>,
}

thread_local! {
    static PUT_AWAY: std::cell::RefCell<Vec<PutAway>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn put_away(window: gpui::WindowId, tab: String, view: Entity<TerminalView>) {
    PUT_AWAY.with_borrow_mut(|kept| {
        kept.retain(|k| k.window != window || k.tab != tab);
        kept.push(PutAway { window, tab, view });
        if kept.len() > PUT_AWAY_MAX {
            kept.remove(0);
        }
    });
}

/// `tab`'s terminal put away in `window`, if it is; the ones whose window or terminal has gone
/// are let go.
fn take_back(window: gpui::WindowId, tab: &str, cx: &gpui::App) -> Option<Entity<TerminalView>> {
    let open: Vec<gpui::WindowId> = cx.windows().iter().map(|w| w.window_id()).collect();
    PUT_AWAY.with_borrow_mut(|kept| {
        kept.retain(|k| open.contains(&k.window) && !k.view.read(cx).closed());
        let at = kept.iter().position(|k| k.window == window && k.tab == tab)?;
        Some(kept.remove(at).view)
    })
}

impl TerminalElement {
    fn put_away(&mut self) {
        if let (Some(window), Some((tab, view, _))) = (self.window, self.view.take()) {
            put_away(window, tab, view);
        }
    }
}

impl CustomElement for TerminalElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement {
        let Some(tab) = self.tab.clone() else { return gpui::Empty.into_any_element() };
        let id = window.window_handle().window_id();
        self.window = Some(id);
        if self.view.as_ref().is_none_or(|(shown, ..)| *shown != tab) {
            self.put_away();
            self.focused_once = false;
            let view = match take_back(id, &tab, cx) {
                Some(view) => {
                    view.read(cx).shown_again();
                    Some(view)
                }
                None => crate::engine::runtime().map(|runtime| {
                    let (session, changes) = crate::term::TerminalSession::new(&tab, runtime);
                    cx.new(|cx| TerminalView::new(session, changes, cx))
                }),
            };
            if let Some(view) = view {
                let from = tab.clone();
                let events = cx.subscribe(&view, move |_, _, event: &TerminalEvent, _| {
                    let data = match event {
                        TerminalEvent::OpenPath(paths) => json!({ "tab": from, "openPath": paths }),
                        TerminalEvent::OpenUrl { url, external } => {
                            json!({ "tab": from, "openUrl": url, "external": external })
                        }
                        TerminalEvent::AttachFailed(err) => json!({ "tab": from, "attachFailed": err }),
                    };
                    crate::engine::emit("terminal", data);
                });
                self.view = Some((tab.clone(), view, events));
            }
        }
        let Some((_, view, _)) = &self.view else { return gpui::Empty.into_any_element() };
        if self.auto_focus && !self.focused_once {
            self.focused_once = true;
            let focus = view.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        // The palette's background (at its opacity) under every cell without one of its own, and
        // under the element's padding too, so a theme's padding is the terminal's colour; a
        // theme's own `backgroundColor` goes over it.
        let background = crate::theme::terminal::background(crate::theme::terminal::BG_REF);
        let mut el = ctx.selects_itself(gpui::div().id(("terminal", ctx.id)).size_full().bg(background));
        if let Some(style) = ctx.style {
            el = pumi_gpuix::apply_interactive_styles(el, style);
        }
        // Cached: the view draws again when it notifies (output, the cursor's blink, a selection)
        // or its box changes, not each time the tab around it is built (a spinner in its title,
        // a title change) — a terminal's grid is most of what a tab paints.
        let mut fill = gpui::StyleRefinement::default();
        fill.size.width = Some(gpui::relative(1.).into());
        fill.size.height = Some(gpui::relative(1.).into());
        el.child(view.clone().cached(fill)).into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        match key {
            "tab" => self.tab = tab_id(&value),
            "autoFocus" => {
                // Turned on again (its tab is focused again, or a face over it went): it takes
                // the keys once more.
                let on = value.as_bool().unwrap_or(false);
                if on && !self.auto_focus {
                    self.focused_once = false;
                }
                self.auto_focus = on;
            }
            _ => {}
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["tab", "autoFocus"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {
        self.put_away();
    }
}

struct Preview;
impl CustomElementFactory for Preview {
    fn element_type(&self) -> &str {
        "preview"
    }
    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(PreviewElement { tab: None })
    }
}

struct PreviewElement {
    tab: Option<String>,
}
impl CustomElement for PreviewElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement {
        let Some(tab) = &self.tab else { return gpui::Empty.into_any_element() };
        let mut el = gpui::div().id(("preview", ctx.id)).size_full();
        if let Some(style) = ctx.style {
            el = pumi_gpuix::apply_interactive_styles(el, style);
        }
        el.child(crate::ui::preview::element(tab)).into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        if key == "tab" {
            self.tab = tab_id(&value);
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["tab"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {}
}

/// `<webview handle="b1" src="https://…" zoom={1} autoFocus>`: a web page, the platform's own
/// (WKWebView). `handle` names it, unique in the app: the page says what it does as `webview`
/// events (`{handle, url, title, loading, progress, back, forward, devtools}`, `{handle, open}`
/// for a new window, `{handle, notice}`, `{handle, focused}`, `{handle, bridge}`, `{handle, reply}`,
/// `{handle, snapshot, error}`) and
/// `Engine.webview(handle, method, arg)` drives it. With `plugin` and `dir` it is a plugin's
/// page: `src` is one of its files (served as `pumi-plugin://<plugin>/…`), and its `window.pumi`
/// reaches the API through `bridge` events. Changing `src` loads it.
struct Web;
impl CustomElementFactory for Web {
    fn element_type(&self) -> &str {
        "webview"
    }
    fn create(&self, _id: u64) -> Box<dyn CustomElement> {
        Box::new(WebElement::default())
    }
}

use crate::ui::webview::{Plugin, Webview, WebviewEvent};

thread_local! {
    static PAGES: std::cell::RefCell<std::collections::HashMap<String, gpui::WeakEntity<Webview>>> = Default::default();
}

#[derive(Default)]
struct WebElement {
    handle: Option<String>,
    src: Option<String>,
    plugin: Option<String>,
    dir: Option<String>,
    args: Value,
    theme: Value,
    theme_css: String,
    background: Option<wry::RGBA>,
    keep_keys: Vec<String>,
    zoom: Option<f64>,
    auto_focus: bool,
    focused_once: bool,
    view: Option<(String, Entity<Webview>, gpui::Subscription)>,
    loaded: Option<String>,
}

fn state(handle: &str, view: &Webview) -> Value {
    let frame = view
        .placed_bounds()
        .map(|b| [f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height)]);
    json!({
        "handle": handle, "url": view.url(), "title": view.label(), "loading": view.loading(),
        "progress": view.progress(), "back": view.can_go_back(), "forward": view.can_go_forward(),
        "devtools": view.devtools_open(), "frame": frame,
    })
}

impl CustomElement for WebElement {
    fn render(
        &mut self,
        ctx: CustomRenderContext,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<GpuixView>,
    ) -> gpui::AnyElement {
        let (Some(handle), Some(src)) = (self.handle.clone(), self.src.clone()) else {
            return gpui::Empty.into_any_element();
        };
        if self.view.as_ref().is_none_or(|(shown, ..)| *shown != handle) {
            let plugin = self.plugin.clone().zip(self.dir.clone()).map(|(name, dir)| Plugin {
                name,
                dir: dir.into(),
                component: None,
                tab: None,
                args: self.args.clone(),
                theme: self.theme.clone(),
                theme_css: self.theme_css.clone(),
            });
            let view = cx.new(|cx| Webview::new(&src, plugin, cx));
            let from = handle.clone();
            let events = cx.subscribe(&view, move |_, view, event: &WebviewEvent, cx| {
                let data = match event {
                    WebviewEvent::Changed | WebviewEvent::Loaded => state(&from, view.read(cx)),
                    WebviewEvent::Open(url) => json!({ "handle": from, "open": url }),
                    WebviewEvent::Notice(text) => json!({ "handle": from, "notice": text }),
                    WebviewEvent::Focused => {
                        view.update(cx, |page, cx| page.focus_in_window(cx));
                        json!({ "handle": from, "focused": true })
                    }
                    WebviewEvent::Bridge(body) => json!({ "handle": from, "bridge": body }),
                    WebviewEvent::Reply(body) => json!({ "handle": from, "reply": body }),
                    WebviewEvent::Snapshot(path, error) => json!({ "handle": from, "snapshot": path, "error": error }),
                };
                crate::engine::emit("webview", data);
            });
            PAGES.with(|pages| pages.borrow_mut().insert(handle.clone(), view.downgrade()));
            self.view = Some((handle.clone(), view, events));
            self.loaded = Some(src.clone());
            self.focused_once = false;
        }
        let Some((_, view, _)) = &self.view else { return gpui::Empty.into_any_element() };
        if self.loaded.as_deref() != Some(src.as_str()) {
            self.loaded = Some(src.clone());
            let url = self.plugin.as_ref().map_or(src.clone(), |name| {
                Plugin {
                    name: name.clone(),
                    dir: Default::default(),
                    component: None,
                    tab: None,
                    args: Value::Null,
                    theme: Value::Null,
                    theme_css: String::new(),
                }
                .url(&src)
            });
            view.update(cx, |page, cx| {
                if let Err(err) = page.load(&url, cx) {
                    crate::engine::emit("webview", json!({ "handle": handle, "notice": err }));
                }
            });
        }
        if let Some(zoom) = self.zoom {
            view.update(cx, |page, cx| page.set_zoom(zoom, cx));
            self.zoom = None;
        }
        let (theme, css, background, keep) =
            (self.theme.clone(), self.theme_css.clone(), self.background, self.keep_keys.clone());
        let radius = ctx.style.and_then(|style| style.border_radius).unwrap_or(0.0);
        view.update(cx, |page, _| {
            page.set_shown(true);
            page.set_radius(gpui::px(radius.max(0.0) as f32));
            page.set_theme(theme, css);
            page.set_background(background);
            page.set_keep(keep);
        });
        if self.auto_focus && !self.focused_once {
            self.focused_once = true;
            let focus = view.read(cx).focus_handle(cx);
            window.focus(&focus, cx);
        }
        let mut el = gpui::div().id(("webview", ctx.id)).size_full();
        if let Some(style) = ctx.style {
            el = pumi_gpuix::apply_interactive_styles(el, style);
        }
        el.child(view.clone()).into_any_element()
    }
    fn set_prop(&mut self, key: &str, value: Value) {
        let text = || value.as_str().map(str::to_string);
        match key {
            "handle" => self.handle = text(),
            "src" => self.src = text(),
            "plugin" => self.plugin = text(),
            "dir" => self.dir = text(),
            "args" => self.args = value,
            "theme" => self.theme = value,
            "themeCss" => self.theme_css = text().unwrap_or_default(),
            "background" => self.background = value.as_str().and_then(rgba),
            "keepKeys" => {
                self.keep_keys = value
                    .as_array()
                    .map(|a| a.iter().filter_map(|k| k.as_str().map(str::to_string)).collect())
                    .unwrap_or_default()
            }
            "zoom" => self.zoom = value.as_f64(),
            "autoFocus" => {
                // As a terminal's: turned on again (its tab is focused again), it takes the keys.
                let on = value.as_bool().unwrap_or(false);
                if on && !self.auto_focus {
                    self.focused_once = false;
                }
                self.auto_focus = on;
            }
            _ => {}
        }
    }
    fn supported_props(&self) -> &'static [&'static str] {
        &["handle", "src", "plugin", "dir", "args", "theme", "themeCss", "background", "keepKeys", "zoom", "autoFocus"]
    }
    fn supported_events(&self) -> &'static [&'static str] {
        &[]
    }
    fn destroy(&mut self) {
        if let Some((handle, ..)) = self.view.take() {
            PAGES.with(|pages| pages.borrow_mut().remove(&handle));
        }
    }
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa` as bytes.
fn rgba(color: &str) -> Option<wry::RGBA> {
    let hex = color.strip_prefix('#')?;
    let digit = |i: usize, n: usize| u8::from_str_radix(hex.get(i..i + n)?, 16).ok();
    match hex.len() {
        3 => Some((digit(0, 1)? * 17, digit(1, 1)? * 17, digit(2, 1)? * 17, 255)),
        6 => Some((digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, 255)),
        8 => Some((digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, digit(6, 2)?)),
        _ => None,
    }
}

/// `Engine.webview(handle, method, arg)`: `back`, `forward`, `reload`, `hardReload`, `stop`,
/// `devtools`, `zoom` (a factor), `eval` (a script; what it hands `__pumiReply(id, ok, value)` comes
/// back as a `{handle, reply}` event), `snapshot` (a PNG's path, written as the page shows now, then
/// a `{handle, snapshot, error}` event), `load` (a URL), `focus`, `state`, `edit` (`copy`,
/// `cut`, `paste`, `selectAll`, `undo`, `redo`), `drag` (`{kind, x, y, data}`: one of the app's
/// drags over a plugin's page, in window px; answers `inside` too).
pub fn webview(handle: &str, method: &str, arg: &Value, cx: &mut gpui::App) -> Result<Value, String> {
    let view = PAGES.with(|pages| pages.borrow().get(handle).and_then(|w| w.upgrade()));
    let Some(view) = view else { return Err(format!("no web page {handle}")) };
    view.update(cx, |page, cx| {
        match method {
            "back" => page.go_back(),
            "forward" => page.go_forward(),
            "reload" => page.reload(false),
            "hardReload" => page.reload(true),
            "stop" => page.stop(),
            "devtools" => page.toggle_devtools(cx),
            "zoom" => page.set_zoom(arg.as_f64().ok_or("zoom takes a factor")?, cx),
            "eval" => page.eval(arg.as_str().ok_or("eval takes a script")?),
            "snapshot" => page.snapshot(arg.as_str().ok_or("snapshot takes the PNG's path")?)?,
            "load" => page.load(arg.as_str().ok_or("load takes a URL")?, cx)?,
            "focus" => page.focus_in_window(cx),
            "state" => {}
            "edit" => page.edit(arg.as_str().ok_or("edit takes copy, cut, paste, selectAll, undo or redo")?)?,
            "drag" => {
                let kind = arg.get("kind").and_then(Value::as_str).ok_or("drag takes a kind")?;
                let at = |k: &str| arg.get(k).and_then(Value::as_f64).map(|v| v as f32).ok_or("drag takes x and y");
                let inside = page.drag(kind, at("x")?, at("y")?, arg.get("data").unwrap_or(&Value::Null));
                let mut answer = state(handle, page);
                answer["inside"] = inside.into();
                return Ok(answer);
            }
            other => return Err(format!("a web page has no method {other}")),
        }
        Ok(state(handle, page))
    })
}
