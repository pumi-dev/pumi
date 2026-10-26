//! The engine: pumid's connection and what the native half does for the app's JS.
//!
//! JS starts it once (`Engine.start`) and hears everything through one callback: `state` (the
//! server's sessions, views, tabs, layouts and options), `notice`, `request` (a call to a
//! function the app's JS provides, to `answer`), `notification`
//! (a click on one), `callback` (the server calling a function a call passed it, by `ref`) and
//! `terminal` (a link or path a terminal opened). Calls go out with `call`.
use crate::runtime::{self, Runtime};
use futures::StreamExt;
use futures::channel::mpsc::unbounded;
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod os;
#[cfg(not(target_os = "macos"))]
#[path = "other.rs"]
mod os;

type Events = ThreadsafeFunction<Value, (), Value, Status, false, true>;

struct Shared {
    runtime: Arc<dyn Runtime>,
    events: Events,
    requests: Mutex<HashMap<u32, runtime::pumid::conn::Answer>>,
    next_request: AtomicU32,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

/// Tells the app's JS `kind` happened. Dropped when no engine runs.
pub fn emit(kind: &str, data: Value) {
    if let Some(shared) = SHARED.get() {
        shared.events.call(json!({ "kind": kind, "data": data }), ThreadsafeFunctionCallMode::NonBlocking);
    }
}

/// The URLs macOS asked the app to open (a file or folder dropped on its Dock icon), until the
/// app's JS takes them (`Engine.takeOpened`): one may come before the JS listens, as the app starts.
static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// macOS asked the app to open `urls`: kept for the JS, and an `app` event says so.
pub fn opened(urls: Vec<String>) {
    if urls.is_empty() {
        return;
    }
    OPENED.lock().unwrap_or_else(|p| p.into_inner()).extend(urls);
    emit("app", json!({ "opened": true }));
}

/// The server's connection, once the engine started.
pub fn runtime() -> Option<Arc<dyn Runtime>> {
    SHARED.get().map(|shared| shared.runtime.clone())
}

#[napi(object)]
pub struct StartOptions {
    /// The server's data directory (`--data-dir`); else `$PUMI_DATA_DIR` or the default.
    pub data_dir: Option<String>,
    /// Safe mode (`--safe`): the default theme, and no plugin runs.
    pub safe: Option<bool>,
}

/// What `Engine.fonts` takes.
#[napi(object)]
pub struct FontsOptions {
    /// Only the families a terminal can use: each character as wide as the next.
    pub monospace: Option<bool>,
}

/// A promise of `work`, run off the JS thread.
fn blocking<'env>(
    env: &'env Env,
    work: impl FnOnce() -> std::result::Result<Value, String> + Send + 'static,
) -> Result<Object<'env>> {
    let (deferred, promise) = env.create_deferred::<Value, _>()?;
    std::thread::spawn(move || match work() {
        Ok(value) => deferred.resolve(move |_| Ok(value)),
        Err(err) => deferred.reject(Error::from_reason(err)),
    });
    Ok(promise)
}

#[napi]
pub struct Engine {}

#[napi]
impl Engine {
    /// Connects to pumid (starting it if it isn't running) and starts the app's services.
    #[napi(factory)]
    pub fn start(options: Option<StartOptions>, on_event: Function<Value, ()>) -> Result<Engine> {
        if SHARED.get().is_some() {
            return Err(Error::from_reason("The engine is already running"));
        }
        pumi_proto::paths::export_appname();
        crate::bins::hydrate_path();
        crate::set_safe_mode(options.as_ref().and_then(|o| o.safe).unwrap_or(false));
        let data_dir = options.and_then(|o| o.data_dir).map(std::path::PathBuf::from);
        crate::log_panics(data_dir.clone());
        let events = on_event.build_threadsafe_function::<Value>().callee_handled::<false>().weak::<true>().build()?;
        let server_dir = data_dir.clone().unwrap_or_else(|| pumi_proto::paths::Paths::resolve().data);
        let runtime = runtime::connect(data_dir);
        let _ = SHARED.set(Shared {
            runtime: runtime.clone(),
            events,
            requests: Mutex::new(HashMap::new()),
            next_request: AtomicU32::new(1),
        });
        crate::system::start();
        runtime.on_request(Arc::new(|request| {
            let Some(shared) = SHARED.get() else { return };
            let id = shared.next_request.fetch_add(1, Ordering::Relaxed);
            let (name, args) = crate::system::requested(&request);
            let (name, args) = (name.to_string(), args.clone());
            let caller = request.caller.clone();
            shared.requests.lock().unwrap().insert(id, request.answer);
            emit("request", json!({ "id": id, "name": name, "args": args, "caller": caller }));
        }));
        runtime.on_callback(Arc::new(|reference, args| emit("callback", json!({ "ref": reference, "args": args }))));
        runtime.on_event(Arc::new(|subscription, event| {
            emit("event", json!({ "subscription": subscription, "event": event }))
        }));
        for function in crate::system::provided() {
            if let Err(err) = runtime.provide(function) {
                eprintln!("pumi: providing the app's functions: {err}");
            }
        }
        pumi_gpuix::app::ensure("pumi")?;
        pumi_gpuix::app::update(|cx| {
            cx.set_global(crate::settings::Settings::default());
            crate::system::start_desktop(cx);
            crate::diagnostics::start(server_dir, cx);
            start_reader(runtime.clone(), cx);
            start_setup(runtime.clone(), cx);
            crate::updater::start(cx);
            crate::updater::plugins::start(cx);
        })?;
        Ok(Engine {})
    }

    /// Calls `method` on the server, off the JS thread; `caller` (`{name, tab}`) says who asks
    /// (a plugin). The app's own functions (`clipboard.*`, `dialog.*`, `server.*`, …) are
    /// answered here.
    #[napi(ts_return_type = "Promise<any>")]
    pub fn call<'env>(
        &self,
        env: &'env Env,
        method: String,
        params: Option<Value>,
        caller: Option<Value>,
    ) -> Result<Object<'env>> {
        let runtime = runtime().ok_or_else(|| Error::from_reason("The engine is not running"))?;
        let (deferred, promise) = env.create_deferred::<Value, _>()?;
        std::thread::spawn(move || {
            let params = params.unwrap_or(Value::Array(Vec::new()));
            let answered = match crate::system::answer(&method, &params) {
                Some(answer) => answer.map_err(|err| err.to_string()),
                None => runtime.call_as(&method, params, caller.as_ref()).map_err(|err| err.to_string()),
            };
            match answered {
                Ok(value) => deferred.resolve(move |_| Ok(value)),
                Err(err) => deferred.reject(Error::from_reason(format!("{method}: {err}"))),
            }
        });
        Ok(promise)
    }

    /// The menu bar: `[{name, items: [{command, title, chord}, {separator: true}, {name, items}]}]`,
    /// the app menu (About, Settings, Hide, Quit) excepted. A click posts a `menu` event with the
    /// command's name; the app's window in front runs it.
    #[napi]
    pub fn set_menus(&self, menus: String) -> Result<()> {
        crate::menus::set(&menus)
    }

    /// The URLs macOS asked the app to open since the last call (`file://` ones for files and
    /// folders dropped on its Dock icon), taken: an `app` event with `opened` says there are some.
    #[napi]
    pub fn take_opened(&self) -> Vec<String> {
        std::mem::take(&mut *OPENED.lock().unwrap_or_else(|p| p.into_inner()))
    }

    /// The About panel: AppKit's, the way an Apple app shows one; elsewhere a prompt on the
    /// window in front.
    #[napi]
    pub fn show_about(&self) -> Result<()> {
        pumi_gpuix::app::update(crate::menus::about)
    }

    /// Quits, puts the downloaded update in place, and opens it; nothing while none is ready.
    #[napi]
    pub fn restart_to_update(&self) -> Result<()> {
        pumi_gpuix::app::update(crate::updater::restart)
    }

    /// Drives the `<webview>` named `handle`: `back`, `forward`, `reload`, `hardReload`, `stop`,
    /// `devtools`, `zoom` (a factor), `eval` (a script), `load` (a URL), `focus`. Answers its state.
    #[napi]
    pub fn webview(&self, handle: String, method: String, arg: Option<Value>) -> Result<Value> {
        let arg = arg.unwrap_or(Value::Null);
        pumi_gpuix::app::update(|cx| crate::ui::elements::webview(&handle, &method, &arg, cx))?
            .map_err(Error::from_reason)
    }

    /// Whether web pages get their inspector (the browser plugin's `webview_devtools` option).
    #[napi]
    pub fn set_web_devtools(&self, on: bool) {
        crate::ui::webview::DEVTOOLS.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether the app's windows are light or dark now: its theme's look (`dark` or `light`).
    /// The webview tabs' chrome, and the About panel, follow it.
    #[napi]
    pub fn set_appearance(&self, appearance: String) -> Result<()> {
        let appearance = match appearance.as_str() {
            "light" => crate::theme::Appearance::Light,
            "dark" => crate::theme::Appearance::Dark,
            other => return Err(Error::from_reason(format!("an appearance is `dark` or `light`, not `{other}`"))),
        };
        if crate::theme::appearance() == appearance {
            return Ok(());
        }
        crate::theme::set_appearance(appearance);
        pumi_gpuix::app::update(|cx| cx.refresh_windows())
    }

    /// The system's appearance, `dark` or `light`: GPUI's once the app runs; before, macOS's
    /// `AppleInterfaceStyle` default (set only when dark), and `light` elsewhere.
    #[napi(ts_return_type = "\"dark\" | \"light\"")]
    pub fn system_appearance() -> String {
        use gpui::WindowAppearance::{Dark, VibrantDark};
        let dark = match pumi_gpuix::app::update(|cx| cx.window_appearance()) {
            Ok(appearance) => matches!(appearance, Dark | VibrantDark),
            Err(_) => os::dark_before_the_app(),
        };
        String::from(if dark { "dark" } else { "light" })
    }

    /// Where `kind` (`config`, `data`, `state`, `cache`) of the app's files are, as `stdpath`.
    #[napi]
    pub fn stdpath(kind: String) -> Option<String> {
        pumi_proto::paths::stdpath(&kind).map(|p| p.display().to_string())
    }

    /// The font families the text system has, sorted, without the system's hidden ones (`.SF…`).
    /// `{monospace: true}` keeps those whose `i` is as wide as their `m`: what a terminal can use.
    /// Off the main thread: telling the monospace ones loads every font, a quarter second with
    /// a couple of hundred installed.
    #[napi(ts_return_type = "Promise<string[]>")]
    pub fn fonts<'env>(&self, env: &'env Env, opts: Option<FontsOptions>) -> Result<Object<'env>> {
        let monospace = opts.and_then(|o| o.monospace).unwrap_or(false);
        let text = pumi_gpuix::app::update(|cx| cx.text_system().clone())?;
        blocking(env, move || Ok(crate::fonts::families(&text, monospace).into()))
    }

    /// The updater's state: `{app: {status, version, current, channel, text, canCheck, busy},
    /// plugins: {checking, found, error}}`. An `update` event carries it each time it changes.
    #[napi]
    pub fn update_state(&self) -> Result<Value> {
        pumi_gpuix::app::update(|cx| crate::updater::state(cx))
    }

    /// Looks for a newer pumi now, and downloads it when there is one; with `plugins`, asks which
    /// installed plugins have a new version instead. The outcome is an `update` event.
    #[napi]
    pub fn check_for_updates(&self, plugins: Option<bool>) -> Result<()> {
        if plugins == Some(true) {
            let runtime = runtime().ok_or_else(|| Error::from_reason("The engine is not running"))?;
            return pumi_gpuix::app::update(|cx| crate::updater::plugins::check(runtime, true, cx));
        }
        pumi_gpuix::app::update(|cx| crate::updater::check(true, cx))
    }

    /// The plugin updates of `names` were applied (`plugin.update`): they are no longer waiting.
    #[napi]
    pub fn plugin_updates_applied(&self, names: Vec<String>) -> Result<()> {
        pumi_gpuix::app::update(|cx| crate::updater::plugins::applied(&names, cx))
    }

    /// What the server says of itself: `{version, pid, tabs, sessions, plugins}`, off the JS thread.
    #[napi(ts_return_type = "Promise<any>")]
    pub fn server_status<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
        let runtime = runtime().ok_or_else(|| Error::from_reason("The engine is not running"))?;
        blocking(env, move || runtime.status().map_err(|err| err.to_string()))
    }

    /// Starts the server again from the binaries on disk; every program keeps running.
    #[napi(ts_return_type = "Promise<any>")]
    pub fn server_restart<'env>(&self, env: &'env Env) -> Result<Object<'env>> {
        let runtime = runtime().ok_or_else(|| Error::from_reason("The engine is not running"))?;
        blocking(env, move || runtime.restart().map_err(|err| err.to_string()))
    }

    /// The server's log file, when it has one.
    #[napi]
    pub fn server_log(&self) -> Option<String> {
        runtime()?.log().map(|path| path.display().to_string())
    }

    /// A haptic on the trackpad (`generic`, `alignment`, `levelChange`): a Force Touch trackpad's
    /// click under a finger, nothing on a mouse or off macOS.
    #[napi]
    pub fn haptic(&self, pattern: Option<String>) {
        crate::haptics::perform(pattern.as_deref().unwrap_or("generic"));
    }

    /// Opens Diagnostics, the app's own native window (or brings it to the front): at `page`
    /// (`frames`, `terminals`, `app`, `processes`). While it is open the app is sampled each second.
    #[napi]
    pub fn open_diagnostics(&self, page: Option<String>) -> Result<()> {
        let page = page.as_deref().and_then(crate::ui::diagnostics::Page::named);
        pumi_gpuix::app::update(|cx| crate::ui::diagnostics::open(page, cx))
    }

    /// What the app's JS measured of itself: `{sample, startup}`, a second while the app is
    /// sampled (a `diagnostics` event says when: `{watching}`), and the startup's marks once.
    #[napi]
    pub fn report_diagnostics(&self, report: Value) -> Result<()> {
        crate::diagnostics::report_js(report).map_err(Error::from_reason)
    }

    /// The log file of plugin `id` (`pumi plugin logs` reads it).
    #[napi]
    pub fn log_path(id: String) -> String {
        pumi_cli::plugin::log_path(&id).display().to_string()
    }

    /// Offers a function of the app's JS on the server (an export's function, `provider: "app"`).
    /// Calls to it arrive as `request` events.
    #[napi]
    pub fn provide(&self, spec: Value) -> Result<()> {
        let runtime = runtime().ok_or_else(|| Error::from_reason("The engine is not running"))?;
        runtime.provide(spec).map_err(|err| Error::from_reason(err.to_string()))
    }

    /// Answers the `request` event `id`: its result, or `error` with a message.
    #[napi]
    pub fn answer(&self, id: u32, result: Option<Value>, error: Option<String>) {
        let Some(shared) = SHARED.get() else { return };
        let Some(answer) = shared.requests.lock().unwrap().remove(&id) else { return };
        answer.send(match error {
            Some(message) => Err(pumi_api::Error::new(pumi_api::ErrorKind::Exception, message)),
            None => Ok(result.unwrap_or(Value::Null)),
        });
    }

    /// Reads the server's state again now.
    #[napi]
    pub fn refresh(&self) {
        if let Some(wake) = WAKE.get() {
            let _ = wake.unbounded_send(());
        }
    }

    /// The terminal's palette, from the UI theme: `background`, `foreground`, `selection`,
    /// `cursor` (`#rrggbb`), `ansi` (sixteen of them) and `opacity`. A background with an alpha
    /// (`#rrggbbaa`) and no `opacity` is drawn at that alpha. The colours above sixteen are
    /// xterm's, or with `generate` made from these (`harmonious`: a light one's keep its order);
    /// `palette` pins any of the 256 by index (`{"196": "#ff3b30"}`).
    #[napi]
    pub fn set_terminal_palette(&self, palette: Value) -> Result<()> {
        let mut terminal = crate::theme::Terminal::default();
        let hex = |v: &Value| v.as_str().and_then(crate::theme::parse_hex).map(crate::theme::to_u32);
        if let Some(alpha) = palette.get("background").and_then(Value::as_str).and_then(crate::theme::parse_hex) {
            terminal.opacity = alpha.a;
        }
        for (name, slot) in [
            ("background", &mut terminal.background),
            ("foreground", &mut terminal.foreground),
            ("selection", &mut terminal.selection),
            ("cursor", &mut terminal.cursor),
        ] {
            if let Some(color) = palette.get(name).and_then(hex) {
                *slot = color;
            }
        }
        if let Some(ansi) = palette.get("ansi").and_then(Value::as_array) {
            for (slot, color) in terminal.ansi.iter_mut().zip(ansi) {
                if let Some(color) = hex(color) {
                    *slot = color;
                }
            }
        }
        if let Some(opacity) = palette.get("opacity").and_then(Value::as_f64) {
            terminal.opacity = opacity as f32;
        }
        let pinned: Vec<(u8, u32)> = palette
            .get("palette")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(index, color)| Some((index.trim().parse().ok()?, hex(color)?)))
            .collect();
        let flag = |name: &str| palette.get(name).and_then(Value::as_bool).unwrap_or(false);
        terminal.extend(flag("generate"), flag("harmonious"), &pinned);
        if crate::theme::set_terminal(terminal) {
            pumi_gpuix::app::update(|cx| cx.refresh_windows())?;
        }
        Ok(())
    }

    /// Runs the command line (`pumi COMMAND …`). Returns its exit code.
    #[napi]
    pub fn cli(argv: Vec<String>) -> i32 {
        let mut cli = vec!["pumi".to_string()];
        cli.extend(argv);
        pumi_cli::main(cli)
    }
}

/// Brings the server up when it isn't (starts pumid, waits for its socket), saying how as `setup`
/// events: `{title, detail, gaveUp}` for each step, `{line}` for what a step prints, and `{ready}`.
fn start_setup(runtime: Arc<dyn Runtime>, cx: &mut gpui::App) {
    use runtime::Step;
    use std::time::Duration;
    let (lines, mut incoming) = unbounded::<String>();
    cx.spawn(async move |_| {
        while let Some(line) = incoming.next().await {
            emit("setup", json!({ "line": line }));
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        let mut tried = runtime::Tried::default();
        let mut delay = Duration::from_millis(50);
        loop {
            let (runtime, asked) = (runtime.clone(), tried.clone());
            let step =
                cx.background_executor().spawn(async move { runtime.setup().map(|setup| setup.next(&asked)) }).await;
            match step {
                None | Some(Step::Ready) => {
                    if !tried.ran.is_empty() || tried.waits > 0 {
                        emit("setup", json!({ "ready": true }));
                        if let Some(wake) = WAKE.get() {
                            let _ = wake.unbounded_send(());
                        }
                    }
                    break;
                }
                Some(Step::Run { id, title, detail, work }) => {
                    tried.ran.push(id);
                    emit("setup", json!({ "title": title, "detail": detail, "gaveUp": false }));
                    let lines = lines.clone();
                    cx.background_executor().spawn(async move { work(lines) }).await;
                }
                Some(Step::Wait { title, detail }) => {
                    tried.waits += 1;
                    emit("setup", json!({ "title": title, "detail": detail, "gaveUp": false }));
                    cx.background_executor().timer(delay).await;
                    delay = (delay * 2).min(Duration::from_secs(1));
                }
                Some(Step::GaveUp { title, reason }) => {
                    emit("setup", json!({ "title": title, "detail": reason, "gaveUp": true }));
                    break;
                }
            }
        }
    })
    .detach();
}

static WAKE: OnceLock<futures::channel::mpsc::UnboundedSender<()>> = OnceLock::new();

/// Reads the server's state whenever it says it changed, follows its options, and tells JS.
fn start_reader(runtime: Arc<dyn Runtime>, cx: &mut gpui::App) {
    let (wake, mut requests) = unbounded::<()>();
    let (notices, mut heard) = unbounded::<runtime::Notice>();
    let _ = WAKE.set(wake.clone());
    let watch = runtime.watch(wake, notices);
    cx.spawn(async move |cx| {
        let _watch = watch;
        let mut signatures: BTreeMap<String, String> = BTreeMap::new();
        loop {
            let reading = runtime.clone();
            let asked = signatures.clone();
            let read = cx.background_executor().spawn(async move { reading.read(&asked) }).await;
            match read {
                Ok(read) => {
                    for (view, (signature, _)) in &read.trees {
                        signatures.insert(view.clone(), signature.clone());
                    }
                    signatures.retain(|view, _| read.snapshot.layout(view).is_some());
                    if let Some(options) = &read.options {
                        cx.update(|cx| crate::settings::Settings::follow(options, cx));
                    }
                    emit("state", state(&read));
                }
                Err(err) => emit(
                    "state",
                    json!({ "error": err.to_string(), "unreachable": matches!(err, runtime::Error::Unreachable(_)) }),
                ),
            }
            if requests.next().await.is_none() {
                break;
            }
            while requests.try_recv().is_ok() {}
        }
    })
    .detach();
    cx.spawn(async move |_| {
        while let Some(notice) = heard.next().await {
            emit(
                "notice",
                json!({ "tab": notice.tab, "title": notice.title, "body": notice.body,
                "message": notice.message.map(|(level, opts)| json!({ "level": level, "opts": opts })) }),
            );
        }
    })
    .detach();
    if let Some(mut answers) = crate::system::events() {
        cx.spawn(async move |_| {
            while let Some(event) = answers.next().await {
                let mut data = event.data();
                data["kind"] = json!(event.pattern());
                emit("notification", json!({ "pattern": event.pattern(), "tab": event.tab, "data": data }));
            }
        })
        .detach();
    }
}

/// What `state` carries: the snapshot, the layout trees that changed, and the server's menus,
/// key bindings and options when they changed.
fn state(read: &runtime::Read) -> Value {
    json!({
        "snapshot": read.snapshot,
        "trees": read.trees.iter().map(|(view, (_, tree))| (view.clone(), tree_json(tree))).collect::<serde_json::Map<_, _>>(),
        "menus": read.menus,
        "bindings": read.bindings,
        "options": read.options,
    })
}

fn tree_json(node: &runtime::tree::Node) -> Value {
    use runtime::tree::{Axis, Leaf, Node};
    match node {
        Node::Leaf(Leaf::Pane(pane)) => json!({ "pane": pane.id, "tabs": pane.tabs, "active": pane.active }),
        Node::Leaf(Leaf::App(id)) => json!({ "app": id }),
        Node::Split { axis, ratio, first, second } => json!({
            "axis": match axis { Axis::Row => "row", Axis::Column => "column" },
            "ratio": ratio,
            "first": tree_json(first),
            "second": tree_json(second),
        }),
    }
}
