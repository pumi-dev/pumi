use crate::kernel::{ChanId, Kernel};
use pumi_api::{Cx, Error, ErrorKind, Result, TabRef, api};
use regex::Regex;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
/// Call a method of the block in a tab: the protocol every block answers (`read`, its text;
/// `input`, `{text}`, `{paste}` or `{keys}`; `selection`; `report`, an OSC 7501 body), a
/// terminal tab's `run`, `wait` and `attach` (a block on the terminal engine), and the block's own
/// methods — a terminal's `respawn`, a plugin block's `<block>.<method>`, a file tab's
/// `<viewer>.<method>`. `tab` is the caller's own
/// when it is `null`.
#[api(
    name = "tab.call",
    effect = "forward",
    example(
        args = r#"["%3", "read", {"source": "visible", "lines": 80}]"#,
        result = r#"{"text": "ready\n"}"#,
        desc = "What the tab shows now."
    ),
    example(args = r#"["%3", "input", {"keys": ["ctrl+c"]}]"#, result = "null", desc = "Send Ctrl-C to its program."),
    example(
        args = r#"["%3", "run", {"command": ["make", "test"]}]"#,
        result = "null",
        desc = "A command line pasted and entered, as one write."
    ),
    example(
        args = r#"["%3", "wait", {"exit": true, "timeout": 30}]"#,
        result = r#"{"match": null, "exitCode": 0}"#,
        desc = "Until its program ends, at most 30 seconds."
    ),
    example(
        args = r#"["%3", "attach", {"mode": "observe", "cols": 120, "rows": 40}]"#,
        result = r#"{"stream": "stream/1"}"#,
        desc = "Watch the tab's screen as it is drawn, without writing to it."
    ),
    note = "A block's own method is a function its plugin provides: `tab.call(\"%3\", \"back\")` on an `owner.name.browser` tab calls `owner.name.browser.back` with the tab's id and then the arguments."
)]
pub fn pumi_tab_call(cx: &mut Cx, tab: Option<TabRef>, method: String, args: Option<Value>) -> Result<Value> {
    let args = args.unwrap_or(Value::Null);
    let caller = cx.caller().clone();
    let (tab, block, viewer) = {
        let k = cx.host::<Kernel>()?;
        k.sync();
        let tab = k.model.tab_id(&caller, tab.map_or(0, |tab| tab.0))?;
        let block = k
            .model
            .tabs
            .get(&tab)
            .map(|p| p.block.clone())
            .ok_or_else(|| Error::not_found(format!("%{tab} is not open")))?;
        let viewer = k.tab_option(tab, "@viewer").as_str().unwrap_or_default().to_string();
        (tab, block, viewer)
    };
    match method.as_str() {
        // The program's own report: an OSC 7501 body, whatever the tab holds.
        "report" => report(cx, tab, &args),
        // Key names in pumi's syntax, to the bytes the program's modes ask for; every one is
        // checked before a byte is written. A block's own input is its plugin's function.
        "input" if args.get("keys").is_some() => match cx.call("core.tabCall", json!([tab, "modes", Value::Null])) {
            Ok(modes) => keys(cx, tab, &modes, &args),
            Err(err) if err.kind == ErrorKind::NotFound => forward(cx, tab, &block, &viewer, &method, args, err),
            Err(err) => Err(err),
        },
        "wait" => wait(cx, tab, &args),
        _ => match cx.call("core.tabCall", json!([tab, &method, &args])) {
            Err(err) if err.kind == ErrorKind::NotFound => forward(cx, tab, &block, &viewer, &method, args, err),
            other => other,
        },
    }
}
/// A block's own method: the function `<block>.<method>` of the plugin that provides it, or, in a
/// file tab, `<viewer>.<method>` of the viewer that draws it. The tab's id comes first.
fn forward(cx: &mut Cx, tab: u64, block: &str, viewer: &str, method: &str, args: Value, cause: Error) -> Result<Value> {
    let provider = if block.contains('.') || viewer.is_empty() { block } else { viewer };
    if provider.is_empty() {
        return Err(cause);
    }
    cx.call(&format!("{provider}.{method}"), json!([TabRef(tab).to_string(), args]))
}
/// `{body}`: an OSC 7501 report as the program would send it (`state=working:app=build`), or any
/// other JSON as its own text. Answers whether the tab's status changed.
fn report(cx: &mut Cx, tab: u64, args: &Value) -> Result<Value> {
    let body = match args.get("body") {
        Some(Value::String(body)) => body.clone(),
        Some(other) => other.to_string(),
        None => return Err(Error::invalid_params("report takes `body`, an OSC 7501 report")),
    };
    let k = cx.host::<Kernel>()?;
    k.sync();
    let changed = crate::program::report(k, tab, &body).map_err(Error::invalid_params)?;
    Ok(json!(changed))
}
/// `{keys: ["ctrl+c", "f1", "C-c"]}` to a tab on the terminal engine: pumi's key names, encoded
/// as its program's `modes` ask. Every name is parsed before anything is written to the terminal.
fn keys(cx: &mut Cx, tab: u64, modes: &Value, args: &Value) -> Result<Value> {
    let names: Option<Vec<String>> =
        args["keys"].as_array().into_iter().flatten().map(|name| name.as_str().map(str::to_string)).collect();
    let names = names.filter(|names| !names.is_empty()).ok_or_else(|| {
        Error::invalid_params("input: `keys` is a list of key names (`ctrl+c`, `enter`, `f1`, `C-c`)")
    })?;
    let options = pumi_keys::options_from_modes(modes);
    let bytes = pumi_keys::encode_names_with(&names, &options)
        .map_err(|name| Error::invalid_params(format!("not a key: `{name}`")))?;
    cx.call("core.tabCall", json!([tab, "input", { "bytes": bytes }]))
}
/// What a `wait` waits for.
struct Ask {
    /// The text the tab is to show; with its pattern when the text is a regex.
    text: Option<(String, Option<Regex>)>,
    /// Its program is to end, with the exit code in the answer.
    exit: bool,
}
impl Ask {
    fn parse(args: &Value) -> Result<Ask> {
        let text = match (args.get("match").and_then(Value::as_str), args.get("regex").and_then(Value::as_str)) {
            (Some(needle), None) => Some((needle.to_string(), None)),
            (None, Some(pattern)) => {
                let regex =
                    Regex::new(pattern).map_err(|err| Error::invalid_params(format!("wait: `regex`: {err}")))?;
                Some((pattern.to_string(), Some(regex)))
            }
            (Some(_), Some(_)) => return Err(Error::invalid_params("wait takes `match` or `regex`, not both")),
            (None, None) => None,
        };
        let exit = args.get("exit").and_then(Value::as_bool).unwrap_or(false);
        if text.is_none() && !exit {
            return Err(Error::invalid_params("wait takes `match`, `regex` or `exit: true`"));
        }
        Ok(Ask { text, exit })
    }
    /// What the tab says now, if it already says what this asked for: the text that matched, and
    /// the program's exit code when it ended. `ended` is `None` while its program runs.
    fn met(&self, screen: &str, ended: Option<Option<i32>>) -> Option<Value> {
        let found = self.text.as_ref().and_then(|(text, regex)| match regex {
            Some(regex) => regex.find(screen).map(|m| m.as_str().to_string()),
            None => screen.contains(text.as_str()).then(|| text.clone()),
        });
        let code = ended.filter(|_| self.exit);
        if found.is_none() && code.is_none() {
            return None;
        }
        Some(json!({ "match": found, "exitCode": code.flatten() }))
    }
}
/// The calls the host holds for a `wait`, answered when the tab shows what they asked for, its
/// program ends, or their time is up.
#[derive(Default)]
struct Waiting(Vec<Held>);
struct Held {
    id: Value,
    tab: u64,
    ask: Ask,
    deadline: Option<Instant>,
    /// Its program's end, as `tab.changed` said it: the tab may be gone by the next tick.
    ended: Option<Option<i32>>,
    /// The tab was gone from the model: when, and the last of the core's events the model had
    /// then. Its program's end comes before that in the events, and may still be on its way.
    gone: Option<(Instant, u64)>,
}
/// How long a wait for a gone tab's exit code waits for the event that says it, when the events
/// do not show it is not coming.
const GONE_GRACE: Duration = Duration::from_secs(1);
/// Keeps the exit code a waited-on tab's program ended with (`tab.changed` of `exitCode`).
pub fn heard(kernel: &mut Kernel, event: &crate::kernel::events::Event) {
    let Some(tab) = event.about.tab else { return };
    if event.event != "tab.changed"
        || !event.data["keys"].as_array().is_some_and(|keys| keys.iter().any(|k| k == "exitCode"))
    {
        return;
    }
    let code = event.data["patch"]["exitCode"].as_i64().map(|code| code as i32);
    if let Some(waiting) = kernel.ext.get_mut::<Waiting>() {
        for wait in waiting.0.iter_mut().filter(|wait| wait.tab == tab) {
            wait.ended = Some(code);
        }
    }
}
/// The call being served: `serve` offers it, and a handler that answers later (a `wait`) takes it
/// so that `serve` writes no reply of its own.
#[derive(Default)]
struct InFlight {
    call: Option<(ChanId, Value)>,
    held: bool,
}
pub fn offer(kernel: &mut Kernel, chan: ChanId, id: Option<Value>) {
    let in_flight = kernel.ext.get_or_default::<InFlight>();
    in_flight.call = id.map(|id| (chan, id));
    in_flight.held = false;
}
/// Whether the call just served will be answered later.
pub fn taken(kernel: &mut Kernel) -> bool {
    std::mem::take(&mut kernel.ext.get_or_default::<InFlight>().held)
}
/// `wait {match} | {regex} | {exit: true}, timeout`: what the tab shows first, then the tab
/// waited on. The answer is `{match, exitCode}` — what matched, and its program's code if it
/// ended — with `null`s when the timeout passed first.
fn wait(cx: &mut Cx, tab: u64, args: &Value) -> Result<Value> {
    let ask = Ask::parse(args)?;
    let timeout = args.get("timeout").and_then(Value::as_f64).filter(|secs| *secs > 0.0);
    if let Some(answer) = seen(cx, tab, &ask)? {
        return Ok(answer);
    }
    let deadline = timeout.map(|secs| Instant::now() + Duration::from_secs_f64(secs));
    let k = cx.host::<Kernel>()?;
    let in_flight = k.ext.get_or_default::<InFlight>();
    let Some((_, id)) = in_flight.call.take() else {
        return Err(Error::exception("wait is held until it is answered: it cannot be called inside another function"));
    };
    in_flight.held = true;
    k.ext.get_or_default::<Waiting>().0.push(Held { id, tab, ask, deadline, ended: None, gone: None });
    Ok(Value::Null)
}
/// What the tab says now, when it already says what the wait asked for.
fn seen(cx: &mut Cx, tab: u64, ask: &Ask) -> Result<Option<Value>> {
    let screen = if ask.text.is_some() { screen(cx, tab)? } else { String::new() };
    let k = cx.host::<Kernel>()?;
    k.sync();
    Ok(ask.met(&screen, ended(k, tab)))
}
/// The text the tab shows, for what a `wait` is looking for.
fn screen(cx: &mut Cx, tab: u64) -> Result<String> {
    let read = cx.call("core.tabCall", json!([tab, "read", { "source": "visible" }]))?;
    Ok(read["text"].as_str().unwrap_or_default().to_string())
}
/// A tab's program has ended: with its code when it gave one.
fn ended(kernel: &Kernel, tab: u64) -> Option<Option<i32>> {
    if kernel.tab_option(tab, "tab_dead") != json!(true) {
        return None;
    }
    Some(kernel.tab_option(tab, "tab_dead_status").as_i64().map(|code| code as i32))
}
/// Answers the waits their tabs now satisfy, and ends those whose time is up. The host calls it
/// after every turn of its loop.
pub fn tick(kernel: &mut Kernel) {
    if kernel.ext.get::<Waiting>().is_none_or(|waiting| waiting.0.is_empty()) {
        return;
    }
    kernel.sync();
    let held = std::mem::take(&mut kernel.ext.get_or_default::<Waiting>().0);
    let mut keep = Vec::new();
    for mut wait in held {
        let open = kernel.model.tabs.contains_key(&wait.tab);
        if !open && wait.gone.is_none() {
            wait.gone = Some((Instant::now(), kernel.events_seen));
        }
        let screen = if open && wait.ask.text.is_some() {
            match kernel
                .core()
                .and_then(|core| core.call("core.tabCall", json!([wait.tab, "read", { "source": "visible" }]), None))
            {
                Ok(read) => read["text"].as_str().unwrap_or_default().to_string(),
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };
        let ended = wait.ended.or_else(|| if open { ended(kernel, wait.tab) } else { None });
        let answer = wait.ask.met(&screen, ended).or_else(|| {
            let late = wait.deadline.is_some_and(|at| Instant::now() >= at);
            // A tab read as gone before its `tab.changed` of `exitCode` was heard: that
            // event is still to come, unless every event the read had was heard.
            let told = wait
                .gone
                .is_some_and(|(at, seen)| !wait.ask.exit || kernel.events_heard >= seen || at.elapsed() >= GONE_GRACE);
            ((!open && told) || late).then(|| json!({ "match": null, "exitCode": null }))
        });
        match answer {
            Some(answer) => {
                if let Ok(core) = kernel.core() {
                    core.write(&pumi_api::rpc::reply(&wait.id, Ok(answer)));
                }
            }
            None => keep.push(wait),
        }
    }
    kernel.ext.get_or_default::<Waiting>().0 = keep;
}
