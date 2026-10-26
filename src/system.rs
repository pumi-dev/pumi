use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use pumi_os::notify::Urgency;
pub use pumi_os::notify::{Event, Notification};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
type Events = (UnboundedSender<Event>, Mutex<Option<UnboundedReceiver<Event>>>);
fn events_channel() -> &'static Events {
    static EVENTS: OnceLock<Events> = OnceLock::new();
    EVENTS.get_or_init(|| {
        let (tx, rx) = unbounded();
        (tx, Mutex::new(Some(rx)))
    })
}
pub fn start() {
    pumi_os::link();
    let tx = events_channel().0.clone();
    pumi_os::notify::start(move |event| {
        let _ = tx.unbounded_send(event);
    });
}
/// Work for the app's main thread, from a function asked on another.
type Job = Box<dyn FnOnce(&mut gpui::App) + Send>;
static JOBS: OnceLock<UnboundedSender<Job>> = OnceLock::new();

/// Makes GPUI answer the client's OS functions (the clipboard, pickers, opening a path) on every
/// OS. Called with the app, as the engine starts.
///
/// They are asked on other threads (an `Engine.call`'s own, the server connection's), and GPUI
/// is only reachable from the main thread, which JS pumps: a function's work goes there as a job
/// a task of the app's runs, and the thread that asked waits for its answer. A picker's job only
/// opens the picker, so the main thread runs on while the asking thread waits for the user.
pub fn start_desktop(cx: &mut gpui::App) {
    let (jobs, mut queued) = unbounded::<Job>();
    if JOBS.set(jobs).is_err() {
        return;
    }
    cx.spawn(async move |cx| {
        while let Some(job) = queued.next().await {
            cx.update(job);
        }
    })
    .detach();
    pumi_os::install_desktop(GpuiDesktop);
}

fn unavailable(why: impl Into<String>) -> pumi_api::Error {
    pumi_api::Error::new(pumi_api::ErrorKind::ProviderUnavailable, why)
}

/// Runs `work` with the app on its main thread, and waits for its answer: there and then when
/// this is the main thread.
fn on_main<R: Send + 'static>(work: impl FnOnce(&mut gpui::App) -> R + Send + 'static) -> Result<R, pumi_api::Error> {
    // The app is the main thread's alone: running here says this is it.
    if pumi_gpuix::app::running() {
        return pumi_gpuix::app::update(work).map_err(|err| unavailable(err.reason));
    }
    let jobs = JOBS.get().ok_or_else(|| unavailable("the app has not started"))?;
    let (done, answer) = futures::channel::oneshot::channel();
    jobs.unbounded_send(Box::new(move |cx| {
        let _ = done.send(work(cx));
    }))
    .map_err(|_| unavailable("the app has quit"))?;
    pumi_gpuix::app::wake();
    futures::executor::block_on(answer).map_err(|_| unavailable("the app has quit"))
}

struct GpuiDesktop;

impl pumi_os::Desktop for GpuiDesktop {
    fn clipboard_text(&self) -> pumi_api::Result<Option<String>> {
        on_main(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
    }

    fn set_clipboard_text(&self, text: String) -> pumi_api::Result<()> {
        on_main(move |cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)))
    }

    fn pick(&self, pick: pumi_os::Pick) -> pumi_api::Result<Option<Vec<PathBuf>>> {
        if pumi_gpuix::app::running() {
            return Err(pumi_api::Error::exception("fs.pick waits for the user: it is not asked on the main thread"));
        }
        let options = gpui::PathPromptOptions {
            files: pick.kind == pumi_os::PickKind::File,
            directories: pick.kind == pumi_os::PickKind::Directory,
            multiple: pick.multiple,
            prompt: pick.prompt.map(Into::into),
        };
        let chosen = on_main(move |cx| cx.prompt_for_paths(options))?;
        match futures::executor::block_on(chosen) {
            Ok(Ok(paths)) => Ok(paths),
            Ok(Err(err)) => Err(pumi_api::Error::exception(format!("fs.pick: {err}"))),
            // The picker went away unanswered (its window closed): as cancelled.
            Err(_) => Ok(None),
        }
    }

    fn open(&self, path: &Path) -> pumi_api::Result<()> {
        let path = path.to_path_buf();
        on_main(move |cx| cx.open_with_system(&path))
    }

    fn reveal(&self, path: &Path) -> pumi_api::Result<()> {
        let path = path.to_path_buf();
        on_main(move |cx| cx.reveal_path(&path))
    }
}

pub fn events() -> Option<UnboundedReceiver<Event>> {
    events_channel().1.lock().unwrap_or_else(|p| p.into_inner()).take()
}
pub fn post(n: &Notification) {
    if let Err(err) = pumi_os::notify::send(n) {
        eprintln!("notify: {}", err.message);
    }
}
/// Posts a message the server tells the apps of (`message.notify`'s `[msg, level, opts, id]`)
/// as its id; a `debug` one, or one without text, is only kept.
pub fn post_message(params: &Value) {
    let Some(text) = params.get(0).and_then(Value::as_str).filter(|t| !t.is_empty()) else { return };
    let level = params.get(1).and_then(Value::as_str).unwrap_or("info");
    let Some(id) = params.get(3).and_then(Value::as_str).filter(|_| level != "debug") else { return };
    let opts = params.get(2).cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
    let tab = opts.get("tab").and_then(tab_of);
    if let Err(err) = pumi_os::notify::send_as(id, &message(text, level, &opts, tab.as_deref())) {
        eprintln!("notify: {}", err.message);
    }
}
/// Takes back the notification of a message the server says is withdrawn (`message.withdraw`).
pub fn withdraw_message(params: &Value) {
    if let Some(id) = params.get(0).and_then(Value::as_str) {
        pumi_os::notify::withdraw(id);
    }
}
/// The functions only the app answers (`command.*`, `dialog.*`, …), as the export has them,
/// wire-only ones included: `cargo xtask api` writes the file. `command.*` is React's:
/// `answer` leaves it to the app's JS.
const APP_FUNCTIONS: &str = include_str!("../api/app-functions.json");
pub fn provided() -> Vec<Value> {
    let mut all: Vec<Value> = serde_json::from_str(APP_FUNCTIONS).expect("api/app-functions.json is JSON");
    for f in &mut all {
        f["source"] = json!("app");
    }
    all.push(crate::term::screens::info());
    all.push(crate::diagnostics::info());
    all
}
pub fn answer(name: &str, args: &Value) -> Option<Result<Value, pumi_api::Error>> {
    if let Some(answer) = crate::term::screens::answer(name, args) {
        return Some(answer);
    }
    if let Some(answer) = crate::diagnostics::answer(name, args) {
        return Some(answer);
    }
    pumi_os::answer(name, args.clone())
}
/// Whether the app answers `name` off the thread that asked: it measures for a while
/// (`app.profile`), or waits for the main thread, which may be waiting for that thread.
pub fn takes(name: &str) -> bool {
    crate::diagnostics::takes(name) || pumi_os::is_desktop(name)
}

/// The function a request from the server calls, and its arguments.
pub fn requested(request: &crate::runtime::pumid::conn::Request) -> (&str, &Value) {
    static NONE: Value = Value::Array(Vec::new());
    match request.method.as_str() {
        "call" => (request.params[0].as_str().unwrap_or_default(), request.params.get(1).unwrap_or(&NONE)),
        name => (name, &request.params),
    }
}
pub fn answer_request(request: &crate::runtime::pumid::conn::Request) -> Option<Result<Value, pumi_api::Error>> {
    let (name, args) = requested(request);
    answer(name, args)
}
/// A message (`message.notify`'s text, level and `opts`) as the notification the app posts.
pub fn message(text: &str, level: &str, opts: &Value, tab: Option<&str>) -> Notification {
    let heading = match level {
        "error" => "Error",
        "warn" => "Warning",
        _ => "pumi",
    };
    let mut fields = serde_json::Map::new();
    for key in ["subtitle", "sound", "group", "actions", "reply", "image", "data", "urgency"] {
        if let Some(value) = opts.get(key).filter(|v| !v.is_null()) {
            fields.insert(key.into(), value.clone());
        }
    }
    let title = opts.get("title").and_then(Value::as_str).unwrap_or(heading);
    fields.insert("title".into(), json!(title));
    fields.insert("body".into(), json!(text));
    if !fields.contains_key("urgency") {
        fields.insert("urgency".into(), serde_json::to_value(Urgency::of_level(level)).unwrap_or_default());
    }
    if let Some(tab) = tab {
        fields.insert("tab".into(), json!(tab));
    }
    serde_json::from_value(Value::Object(fields)).unwrap_or_else(|err| {
        eprintln!("message.notify: opts: {err}");
        Notification::new(title).body(text).urgency(Urgency::of_level(level))
    })
}
pub fn tab_of(tab: &Value) -> Option<String> {
    match tab {
        Value::Number(n) => n.as_u64().map(|n| format!("%{n}")),
        Value::String(s) if s.starts_with('%') => Some(s.clone()),
        Value::String(s) => s.parse::<u64>().ok().map(|n| format!("%{n}")).or_else(|| Some(s.clone())),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_message_is_a_notification() {
        let n = message("built", "error", &json!({ "subtitle": "main", "colour": "red" }), Some("%3"));
        assert_eq!(
            (n.title.as_str(), n.body.as_deref(), n.subtitle.as_deref()),
            ("Error", Some("built"), Some("main"))
        );
        assert_eq!(n.urgency, Urgency::Critical);
        assert_eq!(n.tab, Some(json!("%3")));
        let titled = message("hi", "info", &json!({ "title": "Build", "actions": "not a list" }), None);
        assert_eq!((titled.title.as_str(), titled.body.as_deref()), ("Build", Some("hi")), "bad opts fall back");
    }
    #[test]
    fn a_tab_is_named_as_the_app_names_it() {
        assert_eq!(tab_of(&json!(3)).as_deref(), Some("%3"));
        assert_eq!(tab_of(&json!("%3")).as_deref(), Some("%3"));
        assert_eq!(tab_of(&json!("3")).as_deref(), Some("%3"));
        assert_eq!(tab_of(&json!(null)), None);
    }
    #[test]
    fn the_apps_functions_are_provided_and_answered_here() {
        let names: Vec<String> = provided().iter().filter_map(|f| f["name"].as_str().map(str::to_string)).collect();
        assert!(names.contains(&"tab.cells".to_string()));
        assert!(names.contains(&"app.profile".to_string()));
        assert!(provided().iter().all(|f| f["provider"] == "app" && f["source"] == "app"));
        assert!(answer("tab.list", &json!([])).is_none(), "the server's");
        let bad = answer("fs.pick", &json!([{ "kind": "nonsense" }])).unwrap();
        assert!(bad.is_err(), "a system function the app answers itself");
    }
    #[test]
    fn each_provided_functions_schema_names_its_arguments() {
        // The server's `fn.set` takes a function's arguments from its schema's properties, in
        // order: a schema that lists an object's fields there instead makes each one an argument
        // (`tab cells TAB [LINES] [TRIM]`), and a call the function cannot read.
        for function in provided() {
            let name = function["name"].as_str().unwrap_or_default();
            let parameters: Vec<&str> = function["parameters"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p.get(1).and_then(Value::as_str))
                .collect();
            let properties: Vec<&str> = function["params_schema"]["properties"]
                .as_object()
                .map(|p| p.keys().map(String::as_str).collect())
                .unwrap_or_default();
            assert_eq!(parameters, properties, "{name}: its parameters are its schema's properties");
        }
    }
}
