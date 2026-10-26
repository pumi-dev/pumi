//! `pumi skill hook AGENT EVENT`: what an agent's hook runs at each point of its life. It knows no
//! agent: it reads the well-known keys of its input (Claude's, Codex's, Cursor's, pi's flags),
//! tells the core (`agent.turn`, which marks the tab an agent), prints what the agent is to be
//! told in the agent's format, and at a turn's end hands the agent what was queued for it. A
//! failure prints one line on stderr and exits 0: an agent never breaks because of pumi.
use crate::client::{Address, Client};
use crate::skill::integrations::{self, Agent, Shape};
use crate::skill::{PERMISSION_WAIT, options, settings};
use serde_json::{Value, json};
use std::io::{IsTerminal, Read, Write};

const USAGE: &str = "usage: pumi skill hook AGENT start|prompt|stop|compact|permission|question|resume [--session ID] [--transcript FILE] [--model NAME] [--resumed] [--full] [--format text|hooks|cursor]";

/// How what the agent is told is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// As it is: an extension or a plugin reads it.
    Text,
    /// Claude's (and Codex's, Gemini's, …) hook output: `hookSpecificOutput`, `decision`.
    Hooks,
    /// Cursor's: `additional_context`.
    Cursor,
}
impl Format {
    fn parse(word: &str) -> Option<Format> {
        match word {
            "text" => Some(Format::Text),
            "hooks" => Some(Format::Hooks),
            "cursor" => Some(Format::Cursor),
            _ => None,
        }
    }
    /// A hook installed without `--format`: its agent's shape says.
    fn of(agent: Option<&Agent>) -> Format {
        match agent.map(|a| a.shape) {
            Some(Shape::Hooks { .. }) => Format::Hooks,
            Some(Shape::Cursor { .. }) => Format::Cursor,
            _ => Format::Text,
        }
    }
}

/// A point the hook is run at, with what an older hook's word meant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Start,
    Prompt {
        full: bool,
    },
    Stop,
    Compact,
    Permission,
    /// Before a tool that waits on the user (a question, a plan): it is `blocked`.
    Question,
    /// After a tool ran: `working` again, if it was `blocked`.
    Resume,
}
impl Event {
    pub fn parse(word: &str) -> Option<Event> {
        match word {
            "start" => Some(Event::Start),
            "prompt" => Some(Event::Prompt { full: false }),
            "full" => Some(Event::Prompt { full: true }),
            "stop" => Some(Event::Stop),
            "compact" | "reset" => Some(Event::Compact),
            "permission" => Some(Event::Permission),
            "question" => Some(Event::Question),
            "resume" => Some(Event::Resume),
            _ => None,
        }
    }
    fn word(self) -> &'static str {
        match self {
            Event::Start => "start",
            Event::Prompt { .. } => "prompt",
            Event::Stop => "stop",
            Event::Compact => "compact",
            Event::Permission => "permission",
            Event::Question => "question",
            Event::Resume => "resume",
        }
    }
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn stdin_json() -> Value {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Value::Null;
    }
    let mut text = String::new();
    let _ = stdin.lock().take(1 << 20).read_to_string(&mut text);
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

/// What a hook is told, from its flags and its input.
#[derive(Debug, Default, PartialEq)]
pub struct Told {
    pub session: Option<String>,
    pub transcript: Option<String>,
    pub model: Option<String>,
    pub resumed: bool,
    /// Claude's `SessionStart` after a compaction (`source: "compact"`).
    pub compacted: bool,
    /// The input's `hook_event_name`, which its output names again.
    pub hook_event: Option<String>,
}
pub fn told(args: &[String], input: &Value) -> Told {
    let session = flag(args, "--session").map(str::to_string).filter(|s| !s.is_empty()).or_else(|| {
        ["session_id", "conversation_id", "sessionId"].iter().find_map(|k| input[*k].as_str().map(str::to_string))
    });
    let transcript = flag(args, "--transcript").map(str::to_string).or_else(|| transcript_of(input));
    let model = flag(args, "--model").map(str::to_string).or_else(|| model_of(input));
    Told {
        session: session.filter(|s| plain_line(s) && !s.is_empty()),
        transcript: transcript.filter(|f| plain_line(f) && f.starts_with('/')),
        model: model.filter(|m| plain_line(m) && !m.is_empty()),
        resumed: args.iter().any(|a| a == "--resumed") || input["source"] == "resume",
        compacted: input["source"] == "compact",
        hook_event: input["hook_event_name"].as_str().map(str::to_string),
    }
}

/// Runs the hook with `args` (`AGENT EVENT [flags]`); its exit status.
pub fn run(address: &Address, args: &[String]) -> i32 {
    let (Some(name), Some(word)) = (args.first(), args.get(1)) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let Some(event) = Event::parse(word) else {
        eprintln!("pumi: a hook runs at start, prompt, stop, compact, permission, question or resume, not `{word}`");
        return 0;
    };
    let agent = integrations::find(name);
    let format = match flag(args, "--format") {
        Some(word) => match Format::parse(word) {
            Some(format) => format,
            None => {
                eprintln!("pumi: a hook prints text, hooks or cursor, not `{word}`");
                return 0;
            }
        },
        None => Format::of(agent),
    };
    let Some(tab) = std::env::var("PUMI_TAB").ok().and_then(|t| t.trim_start_matches('%').parse::<u64>().ok()) else {
        return 0;
    };
    let input = if flag(args, "--session").is_some() { Value::Null } else { stdin_json() };
    let told = told(args, &input);
    let mut client = match Client::connect(&address.socket()).and_then(|mut c| c.introduce("cli").map(|()| c)) {
        Ok(client) => client,
        Err(err) => {
            eprintln!("pumi: {}", err.message);
            return 0;
        }
    };
    match event {
        Event::Permission => {
            // Claude is about to show its permission dialog: the user is waited on, whether
            // pumi asks it or Claude does. Answered here, the tool runs.
            report(&mut client, tab, agent, name, "blocked", Some("permission"));
            if permission(&mut client, tab, agent, &input) {
                report(&mut client, tab, agent, name, "working", None);
            }
            return 0;
        }
        Event::Question => {
            report(&mut client, tab, agent, name, "blocked", Some("question"));
            return 0;
        }
        Event::Resume => {
            // Run after every tool: only a `blocked` tab is told, so a tool call costs one read.
            let status = client.call("option.get", json!(["@agent.status", { "id": format!("%{tab}") }]));
            if status.ok().as_ref().and_then(Value::as_str) == Some("blocked") {
                report(&mut client, tab, agent, name, "working", None);
            }
            return 0;
        }
        _ => {}
    }
    let accepts: Vec<&str> = agent.map(|a| a.accepts.to_vec()).unwrap_or_default();
    let turn = |event: &str, full: bool| {
        json!({
            "agent": name,
            "event": event,
            "session": told.session,
            "model": told.model,
            "transcript": told.transcript,
            "resumed": told.resumed && event == "start",
            "accepts": accepts,
            "full": full,
        })
    };
    if event == Event::Start && told.compacted {
        let _ = client.call("agent.turn", json!([format!("%{tab}"), turn("compact", false)]));
    }
    let full = matches!(event, Event::Prompt { full: true })
        || (matches!(event, Event::Prompt { .. }) && args.iter().any(|a| a == "--full"));
    let answer = match client.call("agent.turn", json!([format!("%{tab}"), turn(event.word(), full)])) {
        Ok(answer) => answer,
        Err(err) => {
            eprintln!("pumi: agent.turn: {}", err.message);
            Value::Null
        }
    };
    let settings = settings::load();
    if let Some(session) = &told.session
        && let Some(line) = integrations::resume_line(name, session, &settings.restore)
    {
        // The core's own call: it is there with the host down too.
        let _ = client.call("core.setOptionValue", json!([options::RESTORE_COMMAND, line, { "tab": tab }]));
    }
    let late: Vec<&str> = answer["late"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !late.is_empty() {
        eprintln!("pumi: no answer in time from {}: the agent was told without it", late.join(", "));
    }
    let mut out = None;
    let mut state = report_state(event);
    match event {
        Event::Start | Event::Prompt { .. } => {
            if let Some(text) = answer["context"].as_str().filter(|t| !t.is_empty()) {
                out = Some(context_output(format, agent, event, told.hook_event.as_deref(), text));
            }
        }
        Event::Stop if accepts.contains(&"queue") => {
            if let Some(session) = &told.session {
                match client.call("agent.take", json!([format!("%{tab}"), { "session": session }])) {
                    Ok(item) => {
                        if let Some(text) = item["text"].as_str().filter(|t| !t.is_empty()) {
                            out = Some(continuation(format, text));
                            state = Some("working");
                        }
                    }
                    Err(err) => eprintln!("pumi: agent.take: {}", err.message),
                }
            }
        }
        _ => {}
    }
    if let Some(state) = state {
        report(&mut client, tab, agent, name, state, None);
    }
    if let Some(out) = out {
        println!("{out}");
    }
    0
}

/// What the agent is told, in its format.
pub fn context_output(
    format: Format,
    agent: Option<&Agent>,
    event: Event,
    hook_event: Option<&str>,
    text: &str,
) -> String {
    match format {
        Format::Text => text.to_string(),
        Format::Hooks => {
            let named = match agent.map(|a| a.shape) {
                Some(Shape::Hooks { start, .. }) if event == Event::Start => Some(start),
                Some(Shape::Hooks { start, prompt, .. }) => Some(prompt.unwrap_or(start)),
                _ => None,
            };
            let fallback = if event == Event::Start { "SessionStart" } else { "UserPromptSubmit" };
            let name = hook_event.or(named).unwrap_or(fallback);
            json!({ "hookSpecificOutput": { "hookEventName": name, "additionalContext": text } }).to_string()
        }
        Format::Cursor => json!({ "additional_context": text }).to_string(),
    }
}

/// A queued message, as the stop hook hands it on: the turn goes on with it.
pub fn continuation(format: Format, text: &str) -> String {
    match format {
        Format::Text => text.to_string(),
        // Claude's Stop: `block` keeps the agent going, with `reason` as what it is told next.
        Format::Hooks => json!({ "decision": "block", "reason": text }).to_string(),
        Format::Cursor => json!({ "followup_message": text }).to_string(),
    }
}

/// Asks the user in pumi (`agents.permissions`); whether they answered, so Claude does not ask.
fn permission(client: &mut Client, tab: u64, agent: Option<&Agent>, input: &Value) -> bool {
    let settings = settings::load();
    let label = agent.map(|a| a.label).unwrap_or("The agent");
    if !settings.permissions {
        return false;
    }
    let Some(ask) = permission_ask(label, input) else { return false };
    // The request is cancelled when this hook's connection closes, so it ends with the hook.
    let request = match client.call("request.open", json!([ask, { "tab": format!("%{tab}") }])) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("pumi: request.open: {}", err.message);
            return false;
        }
    };
    let wait = json!({ "match": request, "timeout": PERMISSION_WAIT });
    let closed = match client.call("event.wait", json!(["request.closed", wait])) {
        Ok(closed) => closed,
        Err(err) => {
            eprintln!("pumi: event.wait: {}", err.message);
            return false;
        }
    };
    let answer = &closed["data"]["answer"];
    match permission_decision(answer, input) {
        Some(decision) => {
            println!("{decision}");
            true
        }
        None => false,
    }
}

/// The session's file, as an agent's hook names it: Claude's and Codex's `transcript_path`,
/// pi's `session_file`.
fn transcript_of(input: &Value) -> Option<String> {
    ["transcript_path", "transcriptPath", "session_file"].iter().find_map(|k| input[*k].as_str().map(str::to_string))
}
/// The model, as an agent's hook names it: a name (`claude-sonnet-4-5`), or `{provider, id}`.
fn model_of(input: &Value) -> Option<String> {
    match &input["model"] {
        Value::String(model) => Some(model.clone()),
        Value::Object(model) => {
            match (model.get("provider").and_then(Value::as_str), model.get("id").and_then(Value::as_str)) {
                (Some(provider), Some(id)) => Some(format!("{provider}/{id}")),
                (None, Some(id)) => Some(id.to_string()),
                _ => model.get("display_name").and_then(Value::as_str).map(str::to_string),
            }
        }
        _ => None,
    }
}
/// One line of text, short enough to be an option's value.
fn plain_line(text: &str) -> bool {
    text.len() <= 4096 && !text.chars().any(char::is_control)
}
/// What a permission hook asks (`request.open`), from its input: Claude's `tool_name` and
/// `tool_input`, and an "always" option when it suggests a rule.
fn permission_ask(agent: &str, input: &Value) -> Option<Value> {
    let tool = input["tool_name"].as_str().filter(|t| !t.is_empty())?;
    let what = ["command", "file_path", "path", "url", "pattern"]
        .iter()
        .find_map(|k| input["tool_input"][*k].as_str())
        .map(|w| w.lines().next().unwrap_or("").chars().take(160).collect::<String>());
    let title = match what {
        Some(what) if !what.is_empty() => format!("{agent} wants to use {tool}: {what}"),
        _ => format!("{agent} wants to use {tool}"),
    };
    let mut options = vec![json!({ "id": "allow", "label": "Allow", "kind": "allow_once" })];
    if input["permission_suggestions"].as_array().is_some_and(|s| !s.is_empty()) {
        options.push(json!({ "id": "always", "label": "Always allow", "kind": "allow_always" }));
    }
    options.push(json!({ "id": "deny", "label": "Deny", "kind": "reject_once" }));
    let detail = input["tool_input"]["description"].as_str();
    Some(json!({
        "kind": "permission",
        "title": title,
        "detail": detail,
        "tool": tool,
        "options": options,
    }))
}
/// Claude's `PermissionRequest` decision for an answer; none (its own prompt asks) for `null` or
/// an answer it does not know.
fn permission_decision(answer: &Value, input: &Value) -> Option<Value> {
    let decision = match answer["option"].as_str()? {
        "allow" => json!({ "behavior": "allow" }),
        "always" => json!({ "behavior": "allow", "updatedPermissions": input["permission_suggestions"] }),
        "deny" => {
            let message = answer["text"].as_str().filter(|t| !t.is_empty()).unwrap_or("Denied in pumi");
            json!({ "behavior": "deny", "message": message })
        }
        _ => return None,
    };
    Some(json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } }))
}
fn report_state(event: Event) -> Option<&'static str> {
    match event {
        Event::Start => Some("idle"),
        Event::Prompt { .. } => Some("working"),
        Event::Stop => Some("done"),
        Event::Compact | Event::Permission | Event::Question | Event::Resume => None,
    }
}
/// Says `state` (and why it is blocked, `kind`) in the tab's terminal, for an agent that does
/// not report itself.
fn report(client: &mut Client, tab: u64, agent: Option<&Agent>, name: &str, state: &str, kind: Option<&str>) {
    if agent.is_some_and(|a| a.reports) {
        return;
    }
    let tty = client.call("option.get", json!(["tab_tty", { "id": format!("%{tab}") }])).unwrap_or(Value::Null);
    say(tty.as_str().filter(|tty| !tty.is_empty()), &sequence(name, state, kind));
}
/// The OSC 7501 report of `state`.
fn sequence(agent: &str, state: &str, kind: Option<&str>) -> String {
    let kind = kind.map(|k| format!(":kind={k}")).unwrap_or_default();
    format!("\x1b]7501;state={state}{kind}:app={agent}\x1b\\")
}
fn say(tty: Option<&str>, body: &str) {
    let Ok(mut terminal) = pumi_sys::pty::open_terminal(tty) else {
        return;
    };
    let _ = terminal.write_all(body.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_hook_says_what_the_agent_is_doing() {
        assert_eq!(report_state(Event::Start), Some("idle"));
        assert_eq!(report_state(Event::Prompt { full: false }), Some("working"));
        assert_eq!(report_state(Event::Stop), Some("done"), "the turn ended: a result for the user");
        assert_eq!(report_state(Event::Compact), None, "a compaction is about what it was told, not its state");
        assert_eq!(report_state(Event::Permission), None, "a permission is a request, not a state");
        assert_eq!((Event::parse("question"), Event::parse("resume")), (Some(Event::Question), Some(Event::Resume)));
        let blocked = sequence("claude", "blocked", Some("question"));
        assert_eq!(blocked, "\x1b]7501;state=blocked:kind=question:app=claude\x1b\\");
        let body = blocked.trim_start_matches("\x1b]7501;").trim_end_matches("\x1b\\");
        match pumi_proto::status::parse(body) {
            Some(pumi_proto::status::Sequence::Report(r)) => {
                assert_eq!(
                    (r.state, r.kind),
                    (pumi_proto::status::State::Blocked, Some(pumi_proto::status::Kind::Question))
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(sequence("claude", "working", None), "\x1b]7501;state=working:app=claude\x1b\\");
    }
    #[test]
    fn older_hooks_words_still_mean_what_they_meant() {
        assert_eq!(Event::parse("full"), Some(Event::Prompt { full: true }));
        assert_eq!(Event::parse("reset"), Some(Event::Compact));
        assert_eq!(Event::parse("show"), None);
        assert_eq!(Format::of(integrations::find("claude")), Format::Hooks);
        assert_eq!(Format::of(integrations::find("cursor")), Format::Cursor);
        assert_eq!(Format::of(integrations::find("pi")), Format::Text);
        assert_eq!(Format::of(None), Format::Text);
    }
    #[test]
    fn a_hook_reads_the_well_known_keys() {
        let claude = json!({
            "session_id": "s", "transcript_path": "/home/u/.claude/projects/p/s.jsonl", "model": "claude-x",
            "source": "resume", "hook_event_name": "SessionStart",
        });
        let got = told(&[], &claude);
        assert_eq!(got.session.as_deref(), Some("s"));
        assert_eq!(got.transcript.as_deref(), Some("/home/u/.claude/projects/p/s.jsonl"));
        assert_eq!(got.model.as_deref(), Some("claude-x"));
        assert!(got.resumed && !got.compacted);
        assert_eq!(got.hook_event.as_deref(), Some("SessionStart"));
        let pi = json!({ "session_file": "/s.jsonl", "model": { "provider": "anthropic", "id": "m" } });
        assert_eq!((transcript_of(&pi).as_deref(), model_of(&pi).as_deref()), (Some("/s.jsonl"), Some("anthropic/m")));
        assert_eq!(model_of(&json!({ "model": { "display_name": "Opus" } })).as_deref(), Some("Opus"));
        let flags: Vec<String> =
            ["--session", "p-1", "--transcript", "rel.jsonl", "--resumed"].map(String::from).to_vec();
        let got = told(&flags, &json!({ "conversation_id": "ignored" }));
        assert_eq!((got.session.as_deref(), got.transcript, got.resumed), (Some("p-1"), None, true));
        assert!(told(&[], &json!({ "source": "compact" })).compacted);
        assert!(!plain_line("a\nb") && plain_line("/a b/c.jsonl"));
    }
    #[test]
    fn what_an_agent_is_told_is_in_its_format() {
        let claude = integrations::find("claude");
        let start = context_output(Format::Hooks, claude, Event::Start, None, "x");
        assert_eq!(
            start,
            json!({ "hookSpecificOutput": { "hookEventName": "SessionStart", "additionalContext": "x" } }).to_string()
        );
        let gemini = integrations::find("gemini");
        let prompt = context_output(Format::Hooks, gemini, Event::Prompt { full: false }, None, "x");
        assert!(prompt.contains("BeforeAgent"), "{prompt}");
        let named = context_output(Format::Hooks, None, Event::Prompt { full: false }, Some("UserPromptSubmit"), "x");
        assert!(named.contains("UserPromptSubmit"), "the input's name wins: {named}");
        assert_eq!(context_output(Format::Cursor, None, Event::Start, None, "x"), r#"{"additional_context":"x"}"#);
        assert_eq!(context_output(Format::Text, claude, Event::Start, None, "x"), "x");
        assert_eq!(continuation(Format::Hooks, "go on"), r#"{"decision":"block","reason":"go on"}"#);
        assert_eq!(continuation(Format::Text, "go on"), "go on");
    }
    #[test]
    fn a_permission_is_asked_and_its_answer_is_claudes_decision() {
        let input = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "make deploy\necho done", "description": "Deploy" },
            "permission_suggestions": [{ "type": "addRules" }],
        });
        let ask = permission_ask("Claude Code", &input).unwrap();
        assert_eq!(ask["title"], "Claude Code wants to use Bash: make deploy");
        assert_eq!(ask["tool"], "Bash");
        let ids: Vec<&str> = ask["options"].as_array().unwrap().iter().filter_map(|o| o["id"].as_str()).collect();
        assert_eq!(ids, ["allow", "always", "deny"]);
        assert!(permission_ask("x", &json!({})).is_none(), "no tool, nothing to ask");
        let allow = permission_decision(&json!({ "option": "allow" }), &input).unwrap();
        assert_eq!(allow["hookSpecificOutput"]["decision"], json!({ "behavior": "allow" }));
        let always = permission_decision(&json!({ "option": "always" }), &input).unwrap();
        assert_eq!(always["hookSpecificOutput"]["decision"]["updatedPermissions"], input["permission_suggestions"]);
        let deny = permission_decision(&json!({ "option": "deny", "text": "not now" }), &input).unwrap();
        assert_eq!(deny["hookSpecificOutput"]["decision"], json!({ "behavior": "deny", "message": "not now" }));
        assert_eq!(permission_decision(&Value::Null, &input), None, "unanswered: Claude asks itself");
    }
}
