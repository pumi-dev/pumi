use super::kernel;
use crate::kernel::events::About;
use crate::kernel::options::Key;
use crate::kernel::queue::{self, TakenMessage};
use crate::kernel::turns::{self, TurnAnswer, TurnEvent};
use pumi_api::{Cx, Error, Result, TabRef, api};
use serde::{Deserialize, Serialize};
use serde_json::json;
/// One line of text short enough to be a trait field: no control characters, at most 4 KiB.
fn plain(field: &str, text: &str) -> Result<()> {
    if text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(Error::invalid_params(format!("`{field}` is one line of at most 4096 bytes")));
    }
    Ok(())
}
/// What `agent.turn` is told about the agent and the moment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentTurn {
    /// Which agent (`claude`, `codex`, `pi`, …): the tab's `@agent`.
    pub agent: String,
    /// `start`, `prompt`, `stop` or `compact`.
    pub event: TurnEvent,
    /// The agent's own session id: `@agent.session`.
    #[serde(default)]
    pub session: Option<String>,
    /// `@agent.model`.
    #[serde(default)]
    pub model: Option<String>,
    /// Its session file's absolute path: `@agent.transcript`.
    #[serde(default)]
    pub transcript: Option<String>,
    /// A `start` that continues a session.
    #[serde(default)]
    pub resumed: bool,
    /// What the agent takes besides hidden text: `@agent.accepts` (`["queue"]`, `["queue",
    /// "wake"]`).
    #[serde(default)]
    #[schemars(schema_with = "accepts_schema")]
    pub accepts: Option<Vec<String>>,
    /// Tell everything again, not only what changed.
    #[serde(default)]
    pub full: bool,
    /// Ask what the agent would be told without counting it as told; the tab's options are
    /// left as they are.
    #[serde(default)]
    pub peek: bool,
}
/// `accepts`: the words `turns::check_accepts` takes.
fn accepts_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "anyOf": [{ "type": "array", "items": { "type": "string", "enum": turns::ACCEPTS } }, { "type": "null" }]
    })
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentTurnOpts {
    /// Seconds to wait for those who listen to `agent.sending`: 1 without, at most 3.
    #[serde(default)]
    pub timeout: Option<f64>,
}
/// Tell the core where the agent in `tab` is: a session starting (`start`), a prompt sent
/// (`prompt`), a turn ended (`stop`), its context compacted (`compact`). It marks the tab an
/// agent (`@agent`, and `@agent.session`, `@agent.model`, `@agent.transcript`, `@agent.accepts`
/// from what it is given), as an agent's hook and a plugin-hosted agent both do. At `start` and
/// `prompt` it fires `agent.sending` and waits for each source listening to it by name to answer
/// (`agent.contribute`), until the deadline; the call answers `{context, from, late}`: their texts,
/// each framed as `<pumi-context from="owner/name">…</pumi-context>` in order of the source, for
/// the agent to be handed unseen (`null` when nobody said anything), who said them, and who did
/// not answer in time. At `stop` and `compact` it fires `agent.turned` and answers `null`.
#[api(
    name = "agent.turn",
    example(
        args = r#"["%3", {"agent": "claude", "event": "prompt", "session": "s-1", "accepts": ["queue"]}]"#,
        result = r#"{"context": null, "from": [], "late": []}"#,
        desc = "A prompt in tab 3's Claude: nobody listens, so it is told nothing."
    ),
    example(
        args = r#"["%3", {"agent": "claude", "event": "stop", "session": "s-1"}]"#,
        result = "null",
        desc = "Its turn ended."
    ),
    note = "Each answer is cut at 64 KiB and the whole at 256 KiB, with a marker; a closing tag inside one is escaped.",
    note = "In `call.atomic` it does not wait: it answers at once, with every listener `late`."
)]
pub fn pumi_agent_turn(
    cx: &mut Cx,
    tab: TabRef,
    turn: AgentTurn,
    opts: Option<AgentTurnOpts>,
) -> Result<Option<TurnAnswer>> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_id(&caller, tab.0)?;
    if turn.agent.trim().is_empty() || turn.agent.len() > 64 || turn.agent.chars().any(char::is_control) {
        return Err(Error::invalid_params("`agent` names the agent in one short word (`claude`)"));
    }
    for (field, value) in [("session", &turn.session), ("model", &turn.model), ("transcript", &turn.transcript)] {
        if let Some(value) = value {
            plain(field, value)?;
        }
    }
    if turn.transcript.as_ref().is_some_and(|t| !t.is_empty() && !t.starts_with('/')) {
        return Err(Error::invalid_params("`transcript` is an absolute path"));
    }
    if let Some(words) = &turn.accepts {
        turns::check_accepts(words).map_err(Error::invalid_params)?;
    }
    let source = k.source(&caller);
    let session = turn
        .session
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| k.tab_option(tab, "@agent.session").as_str().filter(|s| !s.is_empty()).map(str::to_string));
    if !turn.peek {
        let set = [
            ("@agent", Some(json!(turn.agent))),
            ("@agent.session", turn.session.as_ref().filter(|s| !s.is_empty()).map(|s| json!(s))),
            ("@agent.model", turn.model.as_ref().filter(|s| !s.is_empty()).map(|s| json!(s))),
            ("@agent.transcript", turn.transcript.as_ref().filter(|s| !s.is_empty()).map(|s| json!(s))),
            ("@agent.accepts", turn.accepts.as_ref().map(|a| json!(a))),
        ];
        for (name, value) in set {
            if let Some(value) = value {
                k.set_option(name, Key::Tab(tab), value, &source, false)?;
            }
        }
    }
    if !turn.event.tells() {
        let data = json!({ "tab": TabRef(tab), "agent": turn.agent, "session": session, "event": turn.event.word() });
        k.emit("agent.turned", turn.event.word(), About::tab(tab), data);
        return Ok(None);
    }
    let waiting = turns::listeners(k);
    let sending = turns::Sending {
        tab,
        agent: turn.agent,
        session,
        event: turn.event,
        resumed: turn.resumed,
        full: turn.full,
        peek: turn.peek,
    };
    if waiting.is_empty() {
        // Nobody to wait for: it is still said, for whoever listens to everything.
        let id = turns::begin(k, sending, waiting, 0);
        turns::forget(k, id);
        return Ok(Some(TurnAnswer::default()));
    }
    let timeout = match opts.unwrap_or_default().timeout {
        Some(seconds) if seconds.is_finite() && seconds >= 0.0 => ((seconds * 1000.0) as u64).min(turns::MAX_TIMEOUT),
        Some(seconds) => return Err(Error::invalid_params(format!("`timeout` is a number of seconds, not {seconds}"))),
        None => turns::DEFAULT_TIMEOUT,
    };
    let late: Vec<String> = waiting.iter().cloned().collect();
    let id = turns::begin(k, sending, waiting, timeout);
    k.turns.held = Some(id);
    Ok(Some(TurnAnswer { context: None, from: Vec::new(), late }))
}
/// Answer turn `id` (`agent.sending`'s `id`) with what the agent should know, or `null` for
/// nothing. One answer counts per source (the caller's plugin, or its channel's name); the last
/// one in time wins. Returns whether it counted: `false` when the turn had ended (it came too
/// late).
#[api(name = "agent.contribute", example(args = r#"[4, "The tests pass again."]"#, result = "false"))]
pub fn pumi_agent_contribute(cx: &mut Cx, id: u64, text: Option<String>) -> Result<bool> {
    let (caller, k) = kernel(cx)?;
    let source = k.source(&caller);
    Ok(turns::contribute(k, id, &source, text))
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentQueueOpts {
    /// The agent's session it is for; the tab's `@agent.session` now without.
    #[serde(default)]
    pub session: Option<String>,
    /// Who queues it, as the agent is told; the caller without (a plugin's id, `cli`, or
    /// `agent %4 (pi)` for the CLI in an agent's tab).
    #[serde(default)]
    pub from: Option<String>,
}
/// Who queues something: a plugin (its id), `cli`, or `agent %4 (pi)` for the CLI in an agent's
/// tab; else the caller's channel's name.
fn queued_by(k: &crate::kernel::Kernel, caller: &pumi_api::Caller) -> String {
    let chan = k.model.chans.get(&caller.channel);
    if caller.source.is_some() || chan.is_none_or(|c| c.info.ty != "cli") {
        return k.source(caller);
    }
    let own = caller.tab.or_else(|| chan.and_then(|c| c.tab));
    match own.filter(|p| super::traits::has(k, *p, "agent")) {
        Some(tab) => format!("agent %{tab} ({})", k.tab_option(tab, "@agent").as_str().unwrap_or("agent")),
        None => "cli".into(),
    }
}
/// Queue a message for the agent in `tab`: it waits in the core until the agent is done with
/// its turn, then goes to it as a real message, through the channel the agent itself provides
/// (its stop hook, its extension, its plugin). pumi never types into an agent. Refused for a tab
/// that is not an agent, or whose `@agent.accepts` has no `queue`. The message is for one session
/// of the agent: it goes when the tab's session changes or the tab closes. What waits is the
/// `queue` field of the tab's agent trait, and each change to it fires `tab.changed` with
/// `traits.agent`. Answers the message's integer handle, for `agent.unqueue`.
#[api(
    name = "agent.queue",
    example(
        args = r#"["%3", "The migration is in: run the tests again.", {}]"#,
        result = "12",
        desc = "For the agent in tab 3, once its turn ends."
    ),
    note = "A text is at most 64 KiB, and a tab holds at most 100 messages."
)]
pub fn pumi_agent_queue(cx: &mut Cx, tab: TabRef, text: String, opts: Option<AgentQueueOpts>) -> Result<u64> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_id(&caller, tab.0)?;
    let opts = opts.unwrap_or_default();
    queue::accepts(k, tab).map_err(Error::invalid_params)?;
    if text.trim().is_empty() {
        return Err(Error::invalid_params("a queued message has some text"));
    }
    if text.len() > queue::TEXT_LIMIT {
        return Err(Error::invalid_params(format!("a queued message is at most {} KiB", queue::TEXT_LIMIT / 1024)));
    }
    if k.queue.items.values().filter(|i| i.tab == tab).count() >= queue::TAB_LIMIT {
        return Err(Error::invalid_params(format!("%{tab} has {} messages waiting already", queue::TAB_LIMIT)));
    }
    if let Some(from) = &opts.from {
        plain("from", from)?;
    }
    let session = opts
        .session
        .filter(|s| !s.is_empty())
        .or_else(|| k.tab_option(tab, "@agent.session").as_str().filter(|s| !s.is_empty()).map(str::to_string));
    let from = opts.from.filter(|f| !f.trim().is_empty()).unwrap_or_else(|| queued_by(k, &caller));
    Ok(queue::push(k, tab, text, session, from).id)
}
/// Take queued message `id` back; whether it was still waiting.
#[api(name = "agent.unqueue", example(args = "[12]", result = "false"))]
pub fn pumi_agent_unqueue(cx: &mut Cx, id: u64) -> Result<bool> {
    let (_, k) = kernel(cx)?;
    Ok(queue::remove(k, id, queue::Gone::Cancelled).is_some())
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentTakeOpts {
    /// The agent's session asking: only its messages are taken. The tab's `@agent.session`
    /// without.
    #[serde(default)]
    pub session: Option<String>,
    /// Wait for one: the call answers when a message comes (or `null` when the tab closes or
    /// its session changes), and ends when its caller goes.
    #[serde(default)]
    pub wait: bool,
}
/// The oldest message waiting for the agent in `tab`'s `session`, taken out of the queue, or
/// `null`. Called by what runs the agent when it is ready for a message: a stop hook at a turn's
/// end, an extension or a plugin while it is idle (`wait`). Nothing is answered while the tab
/// has a permission or a question open, or shows an `error` not yet seen. A message is answered
/// once at most.
#[api(
    name = "agent.take",
    example(args = r#"["%3", {"wait": false}]"#, result = "null"),
    example(args = r#"["%3", {"session": "s-1", "wait": true}]"#, desc = "Wait for the next one."),
    note = "In `call.atomic` it does not wait."
)]
pub fn pumi_agent_take(cx: &mut Cx, tab: TabRef, opts: Option<AgentTakeOpts>) -> Result<Option<TakenMessage>> {
    let (caller, k) = kernel(cx)?;
    let tab = k.model.tab_id(&caller, tab.0)?;
    let opts = opts.unwrap_or_default();
    let session = opts
        .session
        .filter(|s| !s.is_empty())
        .or_else(|| k.tab_option(tab, "@agent.session").as_str().filter(|s| !s.is_empty()).map(str::to_string))
        .ok_or_else(|| {
            Error::invalid_params(format!("%{tab}'s agent has no session (`@agent.session`): give `session`"))
        })?;
    if let Some(item) = queue::take(k, tab, &session) {
        return Ok(Some(item.taken()));
    }
    if opts.wait {
        k.queue.wait(tab, session);
    }
    Ok(None)
}
