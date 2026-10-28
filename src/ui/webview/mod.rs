use crate::theme;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    AnyWindowHandle, App, Bounds, Context, EventEmitter, FocusHandle, Focusable, Pixels, Render, SharedString,
    Subscription, Window, canvas, div, prelude::*,
};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use wry::raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle};
use wry::{NewWindowResponse, PageLoadEvent, WebView, http};
pub const ZOOM_RANGE: (f64, f64) = (0.3, 3.0);
/// Whether pages are built with their inspector (the browser plugin's `webview_devtools` option, as
/// the app says).
pub static DEVTOOLS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Every page's own script: focus and navigation, and the app's keys. A trusted keydown the app's
/// bindings take (`take`, or every key while `all`), and the page doesn't `keep`, goes to the window
/// instead (`{t:"key"}`, with the page's key so another script can't send one). The chord is
/// spelled as `pumi_gpuix::chord` spells GPUI's keystroke: a-z keep shift, a shifted symbol drops
/// it (`cmd+{`), ctrl, alt, shift, cmd in that order.
const SCRIPT: &str = r##"(() => {
  const ipc = window.ipc, send = ipc && ipc.postMessage.bind(ipc), KEY = "__PUMI_KEY__";
  const post = (m) => send && send(JSON.stringify(m));
  addEventListener("pointerdown", () => post({ t: "focus" }), true);
  // The pointer the page's CSS asks for where the mouse is (`cursor: text` over Monaco's lines): the
  // app's view under the page draws the pointer, so it is told. A field or editable text is `text`.
  let pointer = "";
  addEventListener("mousemove", (e) => {
    const el = e.target && e.target.nodeType === 1 ? e.target : null;
    let c = el ? getComputedStyle(el).cursor : "auto";
    if (c === "auto" && el && (el.isContentEditable || /^(INPUT|TEXTAREA)$/.test(el.tagName))) c = "text";
    if (c !== pointer) post({ t: "cursor", c: (pointer = c) });
  }, true);
  // Back in, the pointer is told again whatever it was on the way out.
  document.addEventListener("mouseleave", () => (pointer = ""));
  const keys = { take: new Set(), keep: new Set(), all: false };
  Object.defineProperty(window, "__pumiKeys", { value: (take, all, keep) => {
    keys.take = new Set(take); keys.all = !!all; keys.keep = new Set(keep);
  } });
  const NAMED = { Enter: "enter", Tab: "tab", Backspace: "backspace", Escape: "escape", ArrowUp: "up",
    ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right", Home: "home", End: "end", PageUp: "pageup",
    PageDown: "pagedown", Insert: "insert", Delete: "delete", Space: "space", NumpadEnter: "enter" };
  const CODES = { Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Semicolon: ";", Quote: "'",
    Comma: ",", Period: ".", Slash: "/", Backslash: "\\", Backquote: "`" };
  const SHIFTED = { "[": "{", "]": "}", ";": ":", "'": '"', ",": "<", ".": ">", "/": "?", "\\": "|",
    "-": "_", "=": "+", "`": "~", 1: "!", 2: "@", 3: "#", 4: "$", 5: "%", 6: "^", 7: "&", 8: "*", 9: "(", 0: ")" };
  const SPELLED = { "-": "minus", "=": "equal", "+": "plus", "/": "slash" };
  const stroke = (e) => {
    let key = NAMED[e.code] || NAMED[e.key] || (/^F\d{1,2}$/.test(e.key) ? e.key.toLowerCase() : null);
    let shift = e.shiftKey;
    if (!key) {
      key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3).toLowerCase()
        : /^Digit\d$/.test(e.code) ? e.code.slice(5) : CODES[e.code] || (e.key.length === 1 ? e.key.toLowerCase() : null);
      if (!key) return null;
      if (shift && SHIFTED[key]) [key, shift] = [SHIFTED[key], false];
    }
    let name = SPELLED[key] || key;
    if (name === "~" && !shift) [shift, name] = [true, "`"];
    const mods = [e.ctrlKey && "ctrl", e.altKey && "alt", shift && "shift", e.metaKey && "cmd"].filter(Boolean);
    return { key, shift, chord: mods.length ? mods.join("+") + "+" + name : name };
  };
  addEventListener("keydown", (e) => {
    if (!e.isTrusted || e.isComposing || ["Shift", "Control", "Alt", "Meta"].includes(e.key)) return;
    const s = stroke(e);
    if (!s || keys.keep.has(s.chord) || !(keys.all || keys.take.has(s.chord))) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    post({ t: "key", k: KEY, key: s.key, m: [e.ctrlKey, e.altKey, s.shift, e.metaKey] });
  }, true);
  if (window.top !== window) return;
  // What a script the app runs in the page answers (`Webview::run`'s callers): `{t:"r", id, ok, v}`,
  // the value as JSON says it. The page could call it too, and say no more than its DOM could.
  Object.defineProperty(window, "__pumiReply", { value: (id, ok, v) => {
    let body;
    try { body = JSON.stringify({ t: "r", id: String(id), ok: !!ok, v: v === undefined ? null : v }); }
    catch (e) { body = JSON.stringify({ t: "r", id: String(id), ok: false, v: "the answer is not JSON: " + e }); }
    if (send) send(body);
  } });
  // The page's last console lines and errors, for whoever drives it (the browser's `console`).
  const said = [];
  const show = (v) => {
    if (typeof v === "string") return v;
    if (v instanceof Error) return v.stack || String(v);
    try { return JSON.stringify(v); } catch { return String(v); }
  };
  const keep = (level, args) => {
    let text = Array.prototype.map.call(args, show).join(" ");
    if (text.length > 4000) text = text.slice(0, 4000) + "…";
    said.push({ level, text, at: Date.now() });
    if (said.length > 300) said.shift();
  };
  for (const level of ["log", "info", "warn", "error", "debug"]) {
    const f = console[level];
    if (typeof f === "function") console[level] = function (...a) { try { keep(level, a); } catch {} return f.apply(this, a); };
  }
  addEventListener("error", (e) => keep("error", [`${e.message} (${e.filename}:${e.lineno}:${e.colno})`]));
  addEventListener("unhandledrejection", (e) => keep("error", ["unhandled rejection:", e.reason]));
  Object.defineProperty(window, "__pumiConsole", { value: () => said.slice() });
  // Where the page is in its history, for the app's back and forward buttons: the engine's own
  // count, where it has the Navigation API to ask (WebView2's Chromium). Where it has none
  // (WebKitGTK) the message carries no count, and the app keeps both buttons enabled.
  const nav = () => {
    const n = window.navigation;
    post(n ? { t: "nav", back: n.canGoBack, forward: n.canGoForward } : { t: "nav" });
  };
  for (const k of ["pushState", "replaceState"]) {
    const f = history[k];
    history[k] = function (...a) { const r = f.apply(this, a); nav(); return r; };
  }
  addEventListener("popstate", nav);
  addEventListener("hashchange", nav);
  addEventListener("pageshow", nav);
  nav();
})();"##;
pub const SCHEME: &str = "pumi-plugin";

thread_local! {
    /// The page whose kept chords are in force (`Webview::keep_keys`), and those chords.
    static KEEPER: Cell<Option<gpui::EntityId>> = const { Cell::new(None) };
    static KEPT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Puts `chords` in force as the focused page's own: the window's bindings skip them now, and the
/// menu bar drops them as key equivalents once this update is done.
fn kept_keys(chords: Vec<String>, cx: &mut App) {
    KEPT.with(|k| *k.borrow_mut() = chords.clone());
    pumi_gpuix::set_kept_keys(chords.clone());
    cx.defer(move |cx| crate::menus::keep(&chords, cx));
}
const BRIDGE_MAX: usize = 8 << 20;
const BRIDGE: &str = include_str!("../webview_bridge.js");
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;
#[cfg(target_os = "windows")]
#[path = "windows.rs"]
mod platform;
#[cfg(target_os = "linux")]
#[path = "linux.rs"]
mod platform;
struct Parent(RawWindowHandle);
impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Ok(unsafe { WindowHandle::borrow_raw(self.0) })
    }
}
struct State {
    back: bool,
    forward: bool,
    loading: Option<bool>,
    progress: Option<f64>,
}
#[derive(Clone, Copy)]
enum Step {
    Back,
    Forward,
    Stop,
    HardReload,
}
#[derive(Clone, Copy)]
enum Edit {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,
}
const WIRE: Option<&str> = if cfg!(target_os = "windows") { Some("https://pumi-plugin.") } else { None };
fn wire(url: &str) -> Cow<'_, str> {
    wire_as(WIRE, url)
}
fn canonical(url: &str) -> Cow<'_, str> {
    canonical_as(WIRE, url)
}
fn wire_as<'a>(wire: Option<&str>, url: &'a str) -> Cow<'a, str> {
    match (wire, url.strip_prefix(SCHEME).and_then(|r| r.strip_prefix("://"))) {
        (Some(prefix), Some(rest)) => Cow::Owned(format!("{prefix}{rest}")),
        _ => Cow::Borrowed(url),
    }
}
fn canonical_as<'a>(wire: Option<&str>, url: &'a str) -> Cow<'a, str> {
    match wire.and_then(|prefix| url.strip_prefix(prefix)) {
        Some(rest) => Cow::Owned(format!("{SCHEME}://{rest}")),
        None => Cow::Borrowed(url),
    }
}
fn new_key() -> String {
    use std::hash::{BuildHasher, RandomState};
    let (a, b) = (RandomState::new().hash_one(0u8), RandomState::new().hash_one(1u8));
    format!("{a:016x}{b:016x}")
}
#[derive(Clone, Debug)]
pub struct Plugin {
    pub name: String,
    pub dir: PathBuf,
    pub component: Option<String>,
    pub tab: Option<u64>,
    pub args: Value,
    /// The window's theme tokens, for the page's `pumi.theme`.
    pub theme: Value,
    /// The `--pumi-*` sheet made of them, the page's `<style id="pumi-theme">`.
    pub theme_css: String,
}
impl Plugin {
    /// Its pages' host: `owner.name` for the plugin `owner/name`, so each plugin is an origin of its
    /// own (a URL's host can't hold the `/`).
    pub fn host(&self) -> String {
        host_of(&self.name)
    }
    pub fn url(&self, src: &str) -> String {
        match src.contains("://") || src.starts_with("about:") || src.starts_with("data:") {
            true => src.to_string(),
            false => format!("{SCHEME}://{}/{}", self.host(), src.trim_start_matches(['/', '.'])),
        }
    }
    fn owns(&self, url: &str) -> bool {
        url.strip_prefix(SCHEME)
            .and_then(|r| r.strip_prefix("://"))
            .and_then(|r| r.strip_prefix(self.host().as_str()))
            .is_some_and(|r| r.is_empty() || r.starts_with(['/', '?', '#']))
    }
    fn script(&self, key: &str) -> String {
        let at = match WIRE {
            Some(_) => ["https:".to_string(), format!("{SCHEME}.{}", self.host())],
            None => [format!("{SCHEME}:"), self.host()],
        };
        let init = json!({
            "plugin": self.name, "component": self.component, "tab": self.tab, "args": self.args,
            "at": at, "key": key,
            "theme": self.theme, "theme_css": self.theme_css,
        });
        BRIDGE.replace("__PUMI_INIT__", &init.to_string())
    }
}
/// The host a plugin's pages are served at: its name, `owner/name`, as `owner.name`, lowercased as
/// a URL's host is.
fn host_of(name: &str) -> String {
    name.replace('/', ".").to_ascii_lowercase()
}
/// Serves `uri`, one of the files of the plugin at `dir`; `plugin` is its pages' host.
pub fn serve(plugin: &str, dir: &Path, uri: &str) -> http::Response<Cow<'static, [u8]>> {
    let respond = |status: u16, mime: &str, body: Vec<u8>| {
        http::Response::builder()
            .status(status)
            .header(http::header::CONTENT_TYPE, mime)
            .header(http::header::CACHE_CONTROL, "no-store")
            .body(Cow::Owned(body))
            .unwrap_or_else(|_| http::Response::new(Cow::Borrowed(&[][..])))
    };
    let not_found = || respond(404, "text/plain", b"not found".to_vec());
    let Some(file) = file_of(plugin, dir, uri) else { return not_found() };
    match std::fs::read(&file) {
        Ok(bytes) => respond(200, mime(&file), bytes),
        Err(_) => not_found(),
    }
}
fn file_of(plugin: &str, dir: &Path, uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix(SCHEME)?.strip_prefix("://")?.strip_prefix(plugin)?;
    let path = rest.split(['?', '#']).next().unwrap_or_default().trim_start_matches('/');
    let path = percent_decode(path)?;
    let path = if path.is_empty() || path.ends_with('/') { format!("{path}index.html") } else { path };
    let rel = Path::new(&path);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let root = std::fs::canonicalize(dir).ok()?;
    let file = std::fs::canonicalize(root.join(rel)).ok()?;
    (file.starts_with(&root) && file.is_file()).then_some(file)
}
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok().filter(|p| !p.contains('\0'))
}
fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Allow,
    System,
    Deny,
}
pub fn policy(url: &str, typed: Option<&str>) -> Policy {
    match url.split(':').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "http" | "https" | "about" | "data" | "blob" => Policy::Allow,
        "file" if typed == Some(url) => Policy::Allow,
        "file" | "javascript" | "" => Policy::Deny,
        _ => Policy::System,
    }
}
/// Where a page is in its own history, as its script reports it (`{t:"nav"}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Nav {
    /// The engine's `canGoBack`, when it has a Navigation API to ask: `None` where it hasn't.
    pub back: Option<bool>,
    /// The engine's `canGoForward`, as `back`.
    pub forward: Option<bool>,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ipc {
    Focus,
    Nav(Nav),
    /// The pointer the page's CSS asks for under the mouse.
    Cursor(gpui::CursorStyle),
}
pub fn parse_ipc(body: &str) -> Option<Ipc> {
    if body.len() > 64 {
        return None;
    }
    let v: Value = serde_json::from_str(body).ok()?;
    let o = v.as_object()?;
    match (o.get("t")?.as_str()?, o.len()) {
        ("focus", 1) => Some(Ipc::Focus),
        ("nav", 1 | 3) => Some(Ipc::Nav(Nav {
            back: o.get("back").and_then(Value::as_bool),
            forward: o.get("forward").and_then(Value::as_bool),
        })),
        ("cursor", 2) => Some(Ipc::Cursor(css_cursor(o.get("c")?.as_str()?))),
        _ => None,
    }
}
/// A CSS `cursor` (its first keyword: a `url(…)` falls back to it) as GPUI draws it; the arrow for
/// what GPUI has no pointer for.
pub fn css_cursor(css: &str) -> gpui::CursorStyle {
    use gpui::CursorStyle as C;
    let keyword = css.rsplit(',').next().unwrap_or(css).trim();
    match keyword {
        "text" => C::IBeam,
        "vertical-text" => C::IBeamCursorForVerticalLayout,
        "pointer" => C::PointingHand,
        "crosshair" | "cell" => C::Crosshair,
        "grab" => C::OpenHand,
        "grabbing" => C::ClosedHand,
        "col-resize" => C::ResizeColumn,
        "row-resize" => C::ResizeRow,
        "ew-resize" => C::ResizeLeftRight,
        "ns-resize" => C::ResizeUpDown,
        "e-resize" => C::ResizeRight,
        "w-resize" => C::ResizeLeft,
        "n-resize" => C::ResizeUp,
        "s-resize" => C::ResizeDown,
        "nwse-resize" | "nw-resize" | "se-resize" => C::ResizeUpLeftDownRight,
        "nesw-resize" | "ne-resize" | "sw-resize" => C::ResizeUpRightDownLeft,
        "not-allowed" | "no-drop" => C::OperationNotAllowed,
        "copy" => C::DragCopy,
        "alias" => C::DragLink,
        "context-menu" => C::ContextualMenu,
        _ => C::Arrow,
    }
}
/// A key the page hands back (`{t:"key", k, key, m:[ctrl, alt, shift, cmd]}`), if it carries the
/// page's own `nonce`: the keystroke, as GPUI's platform layer would have made it.
pub fn parse_key(body: &str, nonce: &str) -> Option<gpui::Keystroke> {
    if body.len() > 256 {
        return None;
    }
    let v: Value = serde_json::from_str(body).ok()?;
    let o = v.as_object().filter(|o| o.len() == 4)?;
    if o.get("t")?.as_str()? != "key" || o.get("k")?.as_str()? != nonce {
        return None;
    }
    let key = o.get("key")?.as_str().filter(|k| !k.is_empty() && k.len() <= 16)?.to_string();
    let m: Vec<bool> = o.get("m")?.as_array()?.iter().map(|b| b.as_bool()).collect::<Option<_>>()?;
    let [control, alt, shift, platform] = m[..] else { return None };
    let modifiers = gpui::Modifiers { control, alt, shift, platform, function: false };
    Some(gpui::Keystroke { modifiers, key, key_char: None })
}
fn navigate(url: String, typed: Option<&str>, own: Option<&Plugin>, nav: &impl Fn(Native)) -> bool {
    match policy(&url, typed) {
        _ if url.starts_with(SCHEME) => own.is_some_and(|p| p.owns(&url)),
        Policy::Allow => true,
        Policy::System => {
            nav(Native::System(url));
            false
        }
        Policy::Deny => false,
    }
}
pub fn owns(chord: &str) -> bool {
    matches!(
        chord,
        "cmd+["
            | "cmd+]"
            | "cmd+r"
            | "shift+cmd+r"
            | "cmd+."
            | "alt+cmd+i"
            | "cmd+equal"
            | "cmd+minus"
            | "cmd+0"
            | "cmd+c"
            | "cmd+x"
            | "cmd+v"
            | "cmd+a"
            | "cmd+z"
            | "shift+cmd+z"
    )
}
enum Native {
    Title(String),
    Load(bool),
    Ipc(Ipc),
    Key(gpui::Keystroke),
    Bridge(String),
    /// What a script the app ran answered (`__pumiReply`), as the page sent it.
    Reply(String),
    /// A picture of the page written to the path, or why not.
    Snapshot(String, Result<(), String>),
    System(String),
    NewWindow(String),
    Download(String, Option<String>, bool),
}
pub enum WebviewEvent {
    Focused,
    Changed,
    Open(String),
    Notice(String),
    Loaded,
    Bridge(String),
    /// What a script the app ran answered: `{t:"r", id, ok, v}`.
    Reply(String),
    /// `snapshot`'s picture is at the path, or the error says why not.
    Snapshot(String, Option<String>),
}
pub struct Webview {
    focus: FocusHandle,
    own_focus: bool,
    plugin: Option<Plugin>,
    held: bool,
    webview: Option<WebView>,
    surface: Option<platform::Surface>,
    key: String,
    unsupported: bool,
    host: Option<(AnyWindowHandle, [Subscription; 3])>,
    placed: Option<Bounds<Pixels>>,
    /// Its corners' radius (the element's `borderRadius`).
    radius: Pixels,
    /// The shape the platform was last given: size, radius, and the holes the app draws in.
    clipped: Option<(gpui::Size<Pixels>, Pixels, Vec<(Bounds<Pixels>, Pixels)>)>,
    shown: bool,
    visible: bool,
    url: String,
    title: String,
    loading: bool,
    progress: f64,
    back: bool,
    forward: bool,
    devtools: bool,
    zoom: f64,
    typed: Rc<RefCell<Option<String>>>,
    native: UnboundedSender<Native>,
    polling: bool,
    /// What shows under the page where it paints nothing: see `set_background`.
    background: Option<wry::RGBA>,
    /// The chords the page keeps though the app's bindings take them (`keepKeys`).
    keep: Vec<String>,
    /// The generation of the app's chords the page was last told (`pumi_gpuix::captured_keys`).
    keys_told: u64,
    id: gpui::EntityId,
    /// The pointer the page asks for where the mouse is (`Ipc::Cursor`).
    cursor: gpui::CursorStyle,
}
impl EventEmitter<WebviewEvent> for Webview {}
impl Focusable for Webview {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Webview {
    pub fn new(src: &str, plugin: Option<Plugin>, cx: &mut Context<Self>) -> Self {
        let url = plugin.as_ref().map(|p| p.url(src)).unwrap_or_else(|| src.to_string());
        let mut this = Self::make(cx.focus_handle(), url, 1.0, cx);
        this.own_focus = true;
        this.plugin = plugin;
        this
    }
    fn make(focus: FocusHandle, url: String, zoom: f64, cx: &mut Context<Self>) -> Self {
        let (native, mut events) = unbounded();
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |this, cx| this.native(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        // A page dropped while it had the keyboard leaves its chords to the app again.
        cx.on_release(|this: &mut Self, cx| {
            if KEEPER.with(Cell::get) == Some(this.id) {
                KEEPER.with(|k| k.set(None));
                kept_keys(Vec::new(), cx);
            }
        })
        .detach();
        let typed = url.starts_with("file:").then(|| url.clone());
        Self {
            id: cx.entity_id(),
            cursor: gpui::CursorStyle::Arrow,
            focus,
            own_focus: false,
            plugin: None,
            held: false,
            webview: None,
            surface: None,
            key: new_key(),
            unsupported: false,
            host: None,
            placed: None,
            radius: Pixels::ZERO,
            clipped: None,
            shown: false,
            visible: false,
            url,
            title: String::new(),
            loading: false,
            progress: 0.,
            back: false,
            forward: false,
            devtools: false,
            zoom: zoom.clamp(ZOOM_RANGE.0, ZOOM_RANGE.1),
            typed: Rc::new(RefCell::new(typed)),
            native,
            polling: false,
            background: None,
            keep: Vec::new(),
            keys_told: 0,
        }
    }
    pub fn eval(&self, js: &str) {
        if let Some(w) = &self.webview {
            let _ = w.evaluate_script(js);
        }
    }
    /// Writes a PNG of what the page shows to `path`; `WebviewEvent::Snapshot` says when it is there.
    pub fn snapshot(&self, path: &str) -> Result<(), String> {
        let (Some(w), Some(surface)) = (&self.webview, &self.surface) else {
            return Err("the page is not in a window yet".into());
        };
        let tx = self.native.clone();
        let to = path.to_string();
        surface.snapshot(w, path, Box::new(move |result| drop(tx.unbounded_send(Native::Snapshot(to, result)))));
        Ok(())
    }
    pub fn focus_in_window(&self, cx: &mut App) {
        let Some((handle, _)) = &self.host else { return };
        let focus = self.focus.clone();
        let _ = handle.update(cx, |_, window, cx| window.focus(&focus, cx));
    }
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn loading(&self) -> bool {
        self.loading
    }
    pub fn progress(&self) -> f64 {
        self.progress
    }
    pub fn can_go_back(&self) -> bool {
        self.back
    }
    pub fn can_go_forward(&self) -> bool {
        self.forward
    }
    pub fn devtools_open(&self) -> bool {
        self.devtools
    }
    /// Where the page is in its window, as it was last placed.
    pub fn placed_bounds(&self) -> Option<Bounds<Pixels>> {
        self.placed
    }
    /// An editing command on the page as its own menu would run it: `copy`, `cut`, `paste`,
    /// `selectAll`, `undo`, `redo`.
    pub fn edit(&self, what: &str) -> Result<(), String> {
        let edit = match what {
            "copy" => Edit::Copy,
            "cut" => Edit::Cut,
            "paste" => Edit::Paste,
            "selectAll" => Edit::SelectAll,
            "undo" => Edit::Undo,
            "redo" => Edit::Redo,
            other => return Err(format!("a page has no edit {other}")),
        };
        self.edit_command(edit);
        Ok(())
    }
    /// One of the app's drags over the page (`kind`: `over`, `leave`, `drop`) at `x`, `y` in the
    /// window: the page hears it as DOM drag events carrying `data` (`__pumi.drag`), and says
    /// whether it takes it through its bridge (`dragAccept`). Answers whether the point is on it.
    pub fn drag(&self, kind: &str, x: f32, y: f32, data: &Value) -> bool {
        let Some(b) = self.placed.filter(|_| self.plugin.is_some()) else { return false };
        let (lx, ly) = (x - f32::from(b.origin.x), y - f32::from(b.origin.y));
        let inside = lx >= 0. && ly >= 0. && lx < f32::from(b.size.width) && ly < f32::from(b.size.height);
        if !inside && kind != "leave" {
            return false;
        }
        self.eval(&format!("window.__pumi&&__pumi.drag({},{lx},{ly},{data})", json!(kind)));
        inside
    }
    pub fn label(&self) -> String {
        let host = self.url.split_once("://").map(|(_, r)| r.split(['/', '?', '#']).next().unwrap_or(r));
        [Some(self.title.as_str()), host].into_iter().flatten().find(|t| !t.is_empty()).unwrap_or_default().into()
    }
    pub fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        if !shown {
            self.set_visible(false);
        }
    }
    fn set_visible(&mut self, visible: bool) {
        if self.visible != visible
            && let (Some(w), Some(surface)) = (&self.webview, &self.surface)
        {
            surface.set_visible(w, visible);
            self.visible = visible;
        }
    }
    fn attached(&self) -> bool {
        let (Some(w), Some(surface)) = (&self.webview, &self.surface) else { return false };
        surface.attached(w)
    }
    fn attach(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let handle = Window::window_handle(window);
        if self.host.as_ref().is_none_or(|(h, _)| *h != handle) {
            if self.unsupported && self.webview.is_none() {
                return false;
            }
            let parent = match platform::parent(window).and_then(|p| platform::prepare(cx).map(|()| p)) {
                Ok(parent) => parent,
                Err(err) => {
                    if !std::mem::replace(&mut self.unsupported, true) {
                        cx.emit(WebviewEvent::Notice(format!("web page: {err}")));
                    }
                    return false;
                }
            };
            let moved = match (&self.webview, &mut self.surface) {
                (Some(w), Some(surface)) => surface.moved(w, &parent),
                _ => false,
            };
            if !moved {
                self.surface = None;
                self.webview = None;
                match self.build(&parent, cx) {
                    Ok(w) => {
                        self.surface = Some(platform::Surface::new(&w, &parent));
                        self.webview = Some(w);
                    }
                    Err(err) => {
                        let notice = platform::build_failure(&err);
                        if !std::mem::replace(&mut self.unsupported, true) {
                            cx.emit(WebviewEvent::Notice(notice));
                        }
                        return false;
                    }
                }
                self.visible = false;
            }
            let subs = [
                cx.on_focus(&self.focus, window, |this, window, cx| {
                    this.focus_page();
                    this.keep_keys(window, cx);
                }),
                cx.on_blur(&self.focus, window, |this, window, cx| {
                    this.release_page();
                    this.keep_keys(window, cx);
                }),
                cx.observe_window_activation(window, |this, window, cx| this.keep_keys(window, cx)),
            ];
            self.host = Some((handle, subs));
            self.placed = None;
            self.clipped = None;
        }
        true
    }
    fn place(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.shown {
            self.set_visible(false);
            return self.keep_keys(window, cx);
        }
        if !self.attach(window, cx) {
            return;
        }
        if self.placed != Some(bounds) && self.attached() {
            self.frame(bounds);
            self.placed = Some(bounds);
        }
        self.set_visible(true);
        self.tell_keys();
        if self.focus.is_focused(window) {
            self.focus_page();
        }
        self.keep_keys(window, cx);
    }
    /// While the page has the keyboard (its window in front, it focused and shown), the chords it
    /// keeps are its own: the app's bindings and the menu bar's key equivalents leave them to it.
    /// One page at a time; the one that had them gives them back when it loses the keyboard.
    fn keep_keys(&mut self, window: &Window, cx: &mut Context<Self>) {
        let has = !self.keep.is_empty() && self.visible && window.is_window_active() && self.focus.is_focused(window);
        let mine = KEEPER.with(Cell::get) == Some(self.id);
        if has && !(mine && KEPT.with(|k| *k.borrow() == self.keep)) {
            KEEPER.with(|k| k.set(Some(self.id)));
            kept_keys(self.keep.clone(), cx);
        } else if !has && mine {
            KEEPER.with(|k| k.set(None));
            kept_keys(Vec::new(), cx);
        }
    }
    pub fn set_radius(&mut self, radius: Pixels) {
        self.radius = radius;
    }
    /// Cuts the page to its rounded box minus what the app painted over it this frame (`over`, in
    /// the window), telling the platform only when that changed.
    fn clip(&mut self, over: &[pumi_gpuix::occlusion::Occluder]) {
        let (Some(w), Some(surface), Some(at), true) = (&self.webview, &self.surface, self.placed, self.visible) else {
            return;
        };
        let holes = over
            .iter()
            .map(|o| {
                let c = o.corners;
                let r = [c.top_right, c.bottom_right, c.bottom_left].into_iter().fold(c.top_left, |a, b| a.min(b));
                (Bounds::new(o.bounds.origin - at.origin, o.bounds.size), r)
            })
            .collect();
        let shape = (at.size, self.radius, holes);
        if self.clipped.as_ref() != Some(&shape) {
            surface.clip(w, shape.0, shape.1, &shape.2);
            self.clipped = Some(shape);
        }
    }
    fn frame(&self, bounds: Bounds<Pixels>) {
        if let (Some(w), Some(surface)) = (&self.webview, &self.surface) {
            surface.frame(w, bounds);
        }
    }
    fn build(&mut self, parent: &Parent, _cx: &App) -> wry::Result<WebView> {
        let tx = self.native.clone();
        let send = move || {
            let tx = tx.clone();
            move |event| drop(tx.unbounded_send(event))
        };
        let (title, load, ipc, nav, new, started, done) = (send(), send(), send(), send(), send(), send(), send());
        let typed = self.typed.clone();
        let url = if self.url.is_empty() { "about:blank" } else { &self.url };
        let url = wire(url);
        let (plugin, own) = (self.plugin.clone(), self.plugin.clone());
        let key = format!(r#"{{"t":"pumi","k":"{}","#, self.key);
        let nonce = self.key.clone();
        let mut context = platform::Context::new();
        let mut builder = context.builder();
        if let Some(p) = &self.plugin {
            let (name, dir) = (p.host(), p.dir.clone());
            builder = builder
                .with_custom_protocol(SCHEME.into(), move |_, req| serve(&name, &dir, &req.uri().to_string()))
                .with_initialization_script_for_main_only(p.script(&self.key), true);
            // A plugin's page is part of the window: on macOS it paints nothing where the page
            // paints nothing (what's under it shows, the window's glass too); elsewhere a child
            // window can't blend with the parent's GPU content, so it is the tab's colour.
            builder = match self.page_background() {
                Some(rgba) => builder.with_background_color(rgba),
                None => builder,
            };
        }
        let webview = builder
            .with_url(url.as_ref())
            .with_visible(false)
            .with_devtools(DEVTOOLS.load(std::sync::atomic::Ordering::Relaxed))
            .with_accept_first_mouse(true)
            .with_initialization_script_for_main_only(SCRIPT.replace("__PUMI_KEY__", &self.key), false)
            .with_document_title_changed_handler(move |t| title(Native::Title(t)))
            .with_on_page_load_handler(move |e, _| load(Native::Load(matches!(e, PageLoadEvent::Started))))
            .with_ipc_handler(move |req| {
                let body = req.body();
                if body.starts_with(r#"{"t":"pumi""#) {
                    if body.len() <= BRIDGE_MAX
                        && body.starts_with(&key)
                        && plugin.as_ref().is_some_and(|p| p.owns(&canonical(&req.uri().to_string())))
                    {
                        ipc(Native::Bridge(body.clone()));
                    }
                } else if body.starts_with(r#"{"t":"r","#) {
                    if body.len() <= BRIDGE_MAX {
                        ipc(Native::Reply(body.clone()));
                    }
                } else if let Some(m) = parse_ipc(body) {
                    ipc(Native::Ipc(m))
                } else if let Some(k) = parse_key(body, &nonce) {
                    ipc(Native::Key(k))
                }
            })
            .with_navigation_handler(move |url| {
                let url = canonical(&url).into_owned();
                navigate(url, typed.borrow().as_deref(), own.as_ref(), &nav)
            })
            .with_new_window_req_handler(move |url, _| {
                new(Native::NewWindow(url));
                NewWindowResponse::Deny
            })
            .with_download_started_handler(move |url, path| {
                started(Native::Download(url, Some(crate::display_path(path)), true));
                true
            })
            .with_download_completed_handler(move |url, _, ok| done(Native::Download(url, None, ok)))
            .build_as_child(parent)?;
        let _ = webview.zoom(self.zoom);
        Ok(webview)
    }
    fn native(&mut self, event: Native, cx: &mut Context<Self>) {
        match event {
            Native::Title(t) => self.title = t,
            Native::Load(started) => {
                (self.loading, self.progress) = (started, if started { 0.1 } else { 1. });
                if started {
                    self.poll(cx);
                    if self.plugin.is_some() {
                        cx.emit(WebviewEvent::Loaded);
                    }
                } else {
                    // A new document knows nothing yet: tell it the keys and the theme again.
                    self.keys_told = 0;
                    self.tell_keys();
                    self.tell_theme();
                }
            }
            Native::Bridge(body) => cx.emit(WebviewEvent::Bridge(body)),
            Native::Reply(body) => cx.emit(WebviewEvent::Reply(body)),
            Native::Snapshot(path, result) => cx.emit(WebviewEvent::Snapshot(path, result.err())),
            Native::Ipc(Ipc::Focus) => cx.emit(WebviewEvent::Focused),
            Native::Ipc(Ipc::Nav(nav)) => {
                if let Some(surface) = &self.surface {
                    surface.nav(nav);
                }
            }
            Native::Ipc(Ipc::Cursor(cursor)) => {
                if self.cursor != cursor {
                    self.cursor = cursor;
                    cx.notify();
                }
            }
            Native::Key(keystroke) => self.window_key(keystroke, cx),
            Native::System(url) => cx.open_url(&url),
            Native::NewWindow(url) => cx.emit(WebviewEvent::Open(url)),
            Native::Download(url, path, ok) => {
                let name = url.rsplit('/').next().unwrap_or(&url).split('?').next().unwrap_or_default().to_string();
                cx.emit(WebviewEvent::Notice(match (path, ok) {
                    (Some(path), _) => format!("downloading {path}"),
                    (None, true) => format!("downloaded {name}"),
                    (None, false) => format!("download failed: {name}"),
                }));
            }
        }
        self.refresh(cx);
        cx.notify();
    }
    /// What the page is made with under it: clear on macOS, the tab's colour elsewhere; a page
    /// from the web keeps its engine's own (white), as a site may count on it.
    fn page_background(&self) -> Option<wry::RGBA> {
        self.plugin.as_ref()?;
        if cfg!(target_os = "macos") { Some((0, 0, 0, 0)) } else { self.background.map(|(r, g, b, _)| (r, g, b, 255)) }
    }
    /// The colour under a plugin's page where the system can't show the window through it
    /// (Windows, Linux): the tab's, as `<WebView background>` says.
    pub fn set_background(&mut self, rgba: Option<wry::RGBA>) {
        if self.background == rgba {
            return;
        }
        self.background = rgba;
        if let (Some(w), Some(rgba), false) = (&self.webview, self.page_background(), cfg!(target_os = "macos")) {
            let _ = w.set_background_color(rgba);
        }
    }
    /// The theme a plugin's page reads (`pumi.theme`, the `--pumi-*` sheet), now and after it
    /// navigates.
    pub fn set_theme(&mut self, tokens: Value, css: String) {
        let Some(p) = &mut self.plugin else { return };
        if p.theme == tokens && p.theme_css == css {
            return;
        }
        (p.theme, p.theme_css) = (tokens, css);
        self.tell_theme();
    }
    fn tell_theme(&self) {
        let Some(p) = &self.plugin else { return };
        let (css, tokens) = (Value::String(p.theme_css.clone()), &p.theme);
        self.eval(&format!("window.__pumi&&__pumi.theme({css},{tokens})"));
    }
    /// The chords the page keeps for itself though the app's bindings take them (`keepKeys`).
    pub fn set_keep(&mut self, keep: Vec<String>) {
        if self.keep != keep {
            self.keep = keep;
            self.keys_told = 0;
            self.tell_keys();
        }
    }
    /// Tells the page which chords to hand back (`__pumiKeys`), when the app's have changed.
    fn tell_keys(&mut self) {
        let (generation, all, take) = pumi_gpuix::captured_keys();
        if generation == self.keys_told || self.webview.is_none() {
            return;
        }
        self.keys_told = generation;
        self.eval(&format!("window.__pumiKeys&&__pumiKeys({},{all},{})", json!(take), json!(self.keep)));
    }
    /// A key the page handed back: the window has it as if the app were focused, so the app's
    /// bindings run it. Only a chord they take now; a page sends no other.
    fn window_key(&mut self, keystroke: gpui::Keystroke, cx: &mut Context<Self>) {
        let Some((handle, _)) = &self.host else { return };
        if !pumi_gpuix::is_captured(&keystroke) {
            return;
        }
        let handle = *handle;
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| window.dispatch_keystroke(keystroke, cx));
        });
    }
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (Some(w), Some(surface)) = (&self.webview, &self.surface) else { return };
        let before = (self.url.clone(), self.title.clone(), self.loading, self.back, self.forward);
        let state = surface.state(w);
        (self.back, self.forward) = (state.back, state.forward);
        self.loading = state.loading.unwrap_or(self.loading);
        self.progress = state.progress.unwrap_or(self.progress);
        if let Ok(url) = w.url().map(|u| canonical(&u).into_owned())
            && !url.is_empty()
            && url != "about:blank"
        {
            self.url = url;
        }
        self.devtools = w.is_devtools_open();
        if before != (self.url.clone(), self.title.clone(), self.loading, self.back, self.forward) {
            cx.emit(WebviewEvent::Changed);
        }
    }
    fn poll(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.polling, true) {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let going = this.update(cx, |this, cx| {
                    this.refresh(cx);
                    cx.notify();
                    this.polling = this.loading;
                    this.loading
                });
                if !matches!(going, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }
    pub fn focus_page(&self) {
        if let (false, Some(w), Some(surface)) = (self.held, &self.webview, &self.surface) {
            surface.focus(w);
        }
    }
    fn release_page(&self) {
        if let (Some(w), Some(surface)) = (&self.webview, &self.surface) {
            surface.release(w);
        }
    }
    fn step(&self, step: Step) {
        if let (Some(w), Some(surface)) = (&self.webview, &self.surface) {
            surface.step(w, step);
        }
    }
    fn edit_command(&self, edit: Edit) {
        self.focus_page();
        if let (Some(w), Some(surface)) = (&self.webview, &self.surface) {
            surface.edit(w, edit);
        }
    }
    pub fn load(&mut self, url: &str, cx: &mut Context<Self>) -> Result<(), String> {
        let url = &self.plugin.as_ref().map(|p| p.url(url)).unwrap_or_else(|| url.to_string());
        match policy(url, Some(url)) {
            _ if url.starts_with(SCHEME) => {
                if !self.plugin.as_ref().is_some_and(|p| p.owns(url)) {
                    return Err(format!("a page does not load `{url}`"));
                }
            }
            Policy::Allow => {}
            Policy::System => {
                cx.open_url(url);
                return Ok(());
            }
            Policy::Deny => return Err(format!("a page does not load `{url}`")),
        }
        *self.typed.borrow_mut() = url.starts_with("file:").then(|| url.to_string());
        if let Some(w) = &self.webview {
            let _ = w.load_url(&wire(url));
        }
        self.url = url.to_string();
        cx.emit(WebviewEvent::Changed);
        cx.notify();
        Ok(())
    }
    pub fn go_back(&self) {
        self.step(Step::Back);
    }
    pub fn go_forward(&self) {
        self.step(Step::Forward);
    }
    pub fn set_zoom(&mut self, zoom: f64, cx: &mut Context<Self>) {
        self.zoom = (zoom * 10.).round().clamp(ZOOM_RANGE.0 * 10., ZOOM_RANGE.1 * 10.) / 10.;
        if let Some(w) = &self.webview {
            let _ = w.zoom(self.zoom);
        }
        cx.emit(WebviewEvent::Notice(format!("zoom {:.0}%", self.zoom * 100.)));
        cx.notify();
    }
    pub fn reload(&mut self, hard: bool) {
        match (&self.webview, hard) {
            (Some(_), true) => self.step(Step::HardReload),
            (Some(w), false) => drop(w.reload()),
            _ => {}
        }
    }
    pub fn stop(&self) {
        self.step(Step::Stop);
    }
    pub fn toggle_devtools(&mut self, cx: &mut Context<Self>) {
        let Some(w) = &self.webview else { return };
        let open = !w.is_devtools_open();
        match open {
            true => w.open_devtools(),
            false => w.close_devtools(),
        }
        cx.spawn(async move |this, cx| {
            for _ in 0..20 {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let done = this.update(cx, |this, cx| {
                    let up = open && this.dock_inspector();
                    this.refresh(cx);
                    cx.notify();
                    up || !open
                });
                if !matches!(done, Ok(false)) {
                    break;
                }
            }
        })
        .detach();
    }
    fn dock_inspector(&mut self) -> bool {
        let (Some(w), Some(surface)) = (&self.webview, &self.surface) else { return false };
        if !surface.dock_inspector(w) {
            return false;
        }
        self.placed = None;
        true
    }
    pub fn page_key(&mut self, chord: &str, cx: &mut Context<Self>) -> bool {
        match chord {
            "cmd+r" => self.reload(false),
            "shift+cmd+r" => self.reload(true),
            "cmd+." | "escape" if self.loading => self.stop(),
            "cmd+[" => self.go_back(),
            "cmd+]" => self.go_forward(),
            "alt+cmd+i" => self.toggle_devtools(cx),
            "cmd+equal" | "cmd+plus" | "shift+cmd+equal" => self.set_zoom(self.zoom + 0.1, cx),
            "cmd+minus" => self.set_zoom(self.zoom - 0.1, cx),
            "cmd+0" => self.set_zoom(1., cx),
            "cmd+c" => self.edit_command(Edit::Copy),
            "cmd+x" => self.edit_command(Edit::Cut),
            "cmd+v" => self.edit_command(Edit::Paste),
            "cmd+a" => self.edit_command(Edit::SelectAll),
            "cmd+z" => self.edit_command(Edit::Undo),
            "shift+cmd+z" => self.edit_command(Edit::Redo),
            _ => return false,
        }
        true
    }
}
impl Drop for Webview {
    fn drop(&mut self) {
        self.surface = None;
    }
}
impl Render for Webview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity();
        let page_view = view.downgrade();
        let page = canvas(
            move |b, window, cx| {
                let mut b = b.intersect(&window.content_mask().bounds);
                b.size = b.size.max(&gpui::Size::default());
                view.update(cx, |this, cx| this.place(b, window, cx));
                b
            },
            // Cut once the frame is drawn: what is painted over the page (its deferred layers, the
            // tooltip, a drag) is known only then.
            move |_, b, window, cx| {
                let view = page_view.clone();
                pumi_gpuix::occlusion::page(b, window, cx, move |over, cx| {
                    let _ = view.update(cx, |this, _| this.clip(over));
                });
            },
        )
        .absolute()
        .size_full();
        // A plugin's page is the window's own: nothing of the app's under it but the tab. A page
        // from the web says what it is while it loads.
        let own = self.plugin.is_some();
        let title: SharedString = if own { SharedString::default() } else { self.label().into() };
        let url = if own { String::new() } else { self.url.clone() };
        div()
            .id("webview")
            .when(self.own_focus, |d| {
                d.track_focus(&self.focus).on_key_down(cx.listener(|this, e: &gpui::KeyDownEvent, _, cx| {
                    let chord = pumi_gpuix::chord(&e.keystroke);
                    if owns(&chord) && this.page_key(&chord, cx) {
                        cx.stop_propagation();
                    }
                }))
            })
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_1()
            .when(!own, |d| d.bg(theme::bg()))
            .cursor(self.cursor)
            .text_xs()
            .when(!own, |d| {
                d.child(div().text_color(theme::muted()).child(title))
                    .child(div().text_color(theme::faint()).child(url))
            })
            .child(page)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_policy_keeps_other_schemes_out() {
        assert_eq!(policy("https://a.dev", None), Policy::Allow);
        assert_eq!(policy("about:blank", None), Policy::Allow);
        assert_eq!(policy("mailto:a@b.c", None), Policy::System);
        assert_eq!(policy("slack://open", None), Policy::System);
        assert_eq!(policy("file:///etc/passwd", None), Policy::Deny);
        assert_eq!(policy("file:///tmp/a.html", Some("file:///tmp/a.html")), Policy::Allow);
        assert_eq!(policy("javascript:x", None), Policy::Deny);
        assert_eq!(policy("javascript:x", Some("javascript:x")), Policy::Deny);
    }
    #[test]
    fn ipc_accepts_only_its_two_shapes() {
        assert_eq!(parse_ipc(r#"{"t":"focus"}"#), Some(Ipc::Focus));
        assert_eq!(parse_ipc(r#"{"t":"nav"}"#), Some(Ipc::Nav(Nav::default())));
        assert_eq!(
            parse_ipc(r#"{"t":"nav","back":false,"forward":true}"#),
            Some(Ipc::Nav(Nav { back: Some(false), forward: Some(true) }))
        );
        assert_eq!(parse_ipc(r#"{"t":"nav","back":false}"#), None, "both counts or neither");
        assert_eq!(parse_ipc(r#"{"t":"focus","x":1}"#), None);
        assert_eq!(parse_ipc(r#"{"t":"run"}"#), None);
        assert_eq!(parse_ipc(r#"{"t":"cursor","c":"text"}"#), Some(Ipc::Cursor(gpui::CursorStyle::IBeam)));
        assert_eq!(parse_ipc(r#"{"t":"cursor"}"#), None);
        assert_eq!(css_cursor("pointer"), gpui::CursorStyle::PointingHand);
        assert_eq!(css_cursor("url(a.png) 2 2, col-resize"), gpui::CursorStyle::ResizeColumn);
        assert_eq!(css_cursor("auto"), gpui::CursorStyle::Arrow);
        assert_eq!(parse_ipc("focus"), None);
        assert_eq!(parse_ipc(&format!(r#"{{"t":"nav","pad":"{}"}}"#, "x".repeat(10_000))), None);
    }
    #[test]
    fn a_package_serves_its_own_files_and_no_others() {
        let root = std::env::temp_dir().join(format!("pumi-webview-{}", std::process::id()));
        std::fs::create_dir_all(root.join("pkg/web")).unwrap();
        std::fs::write(root.join("pkg/web/index.html"), "<p>hi</p>").unwrap();
        std::fs::write(root.join("secret"), "no").unwrap();
        let dir = root.join("pkg");
        let ok = file_of("pkg", &dir, "pumi-plugin://pkg/web/index.html?x=1#y");
        assert!(ok.is_some_and(|f| f.ends_with("web/index.html")));
        assert!(file_of("pkg", &dir, "pumi-plugin://pkg/web/").is_some(), "a directory is its index.html");
        assert_eq!(file_of("pkg", &dir, "pumi-plugin://pkg/../secret"), None);
        assert_eq!(file_of("pkg", &dir, "pumi-plugin://pkg/web/%2e%2e/%2e%2e/secret"), None);
        assert_eq!(file_of("pkg", &dir, "pumi-plugin://other/web/index.html"), None);
        assert_eq!(file_of("pkg", &dir, "pumi-plugin://pkgx/web/index.html"), None);
        assert_eq!(serve("pkg", &dir, "pumi-plugin://pkg/nope.html").status(), 404);
        assert_eq!(serve("pkg", &dir, "pumi-plugin://pkg/web/index.html").status(), 200);
        std::fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn a_package_owns_only_its_own_urls() {
        let p = Plugin {
            name: "demo".into(),
            dir: PathBuf::new(),
            component: None,
            tab: None,
            args: Value::Null,
            theme: Value::Null,
            theme_css: String::new(),
        };
        assert_eq!(p.url("web/a.html"), "pumi-plugin://demo/web/a.html");
        assert_eq!(p.url("./web/a.html"), "pumi-plugin://demo/web/a.html");
        assert_eq!(p.url("https://x.dev"), "https://x.dev");
        assert!(p.owns("pumi-plugin://demo/web/a.html") && p.owns("pumi-plugin://demo"));
        assert!(!p.owns("pumi-plugin://demox/a.html") && !p.owns("https://demo/") && !p.owns("pumi-plugin://other/"));
        let owned = Plugin { name: "pumi_dev/Monaco".into(), ..p.clone() };
        assert_eq!(owned.url("web/index.html"), "pumi-plugin://pumi_dev.monaco/web/index.html");
        assert!(owned.owns("pumi-plugin://pumi_dev.monaco/web/index.html"));
        assert!(!owned.owns("pumi-plugin://pumi_dev/monaco/web/index.html"), "the owner alone is not the plugin");
        assert!(owned.script("k").contains(r#""plugin":"pumi_dev/Monaco""#), "the page still names it as it is");
    }
    #[test]
    fn webview2_spells_package_urls_as_https() {
        let wire = Some("https://pumi-plugin.");
        assert_eq!(wire_as(wire, "pumi-plugin://demo/web/a.html?x#y"), "https://pumi-plugin.demo/web/a.html?x#y");
        assert_eq!(wire_as(wire, "https://x.dev/"), "https://x.dev/");
        assert_eq!(canonical_as(wire, "https://pumi-plugin.demo/web/a.html"), "pumi-plugin://demo/web/a.html");
        assert_eq!(
            canonical_as(wire, "https://pumi-plugin.dev/"),
            "pumi-plugin://dev/",
            "every host under the prefix is a plugin's"
        );
        assert_eq!(canonical_as(wire, "https://x.dev/"), "https://x.dev/");
        assert_eq!(wire_as(None, "pumi-plugin://demo/a"), "pumi-plugin://demo/a");
        assert_eq!(canonical_as(None, "https://pumi-plugin.demo/a"), "https://pumi-plugin.demo/a");
    }
    #[test]
    fn a_package_page_is_told_where_it_is_and_its_key() {
        let p = Plugin {
            name: "demo".into(),
            dir: PathBuf::new(),
            component: None,
            tab: None,
            args: Value::Null,
            theme: Value::Null,
            theme_css: String::new(),
        };
        let script = p.script("k1");
        assert!(!script.contains("__PUMI_INIT__"));
        assert!(script.contains(r#""key":"k1""#));
        let at =
            if WIRE.is_some() { r#""at":["https:","pumi-plugin.demo"]"# } else { r#""at":["pumi-plugin:","demo"]"# };
        assert!(script.contains(at), "{at}");
        let (a, b) = (new_key(), new_key());
        assert!(a.len() == 32 && a != b);
    }
    #[test]
    fn the_page_chords_are_its_own() {
        assert!(owns("cmd+c") && owns("cmd+r") && owns("alt+cmd+i"));
        assert!(!owns("cmd+l") && !owns("cmd+t"));
    }
}
