//! What an agent is told, unseen, when it starts or is sent a prompt: `agent.turn` asks whoever
//! listens to `agent.sending` by name, waits for each of them until a short deadline, and answers
//! their texts framed in one block each (`<pumi-context from="owner/name">…</pumi-context>`), in
//! order of their source, so the same answers make the same text. Nothing is kept once it is
//! answered: what each source told which session is the source's own business.
use super::events::About;
use super::{ChanId, Kernel, TabId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
/// How long a turn waits for its sources when its caller gives no `timeout`, in ms.
pub const DEFAULT_TIMEOUT: u64 = 1000;
/// The most a turn waits, in ms.
pub const MAX_TIMEOUT: u64 = 3000;
/// The most one source's answer may be, in bytes; the rest is cut with a marker.
pub const ANSWER_LIMIT: usize = 64 * 1024;
/// The most a turn's whole context may be, in bytes.
pub const TURN_LIMIT: usize = 256 * 1024;
/// The words `@agent.accepts` may hold: what an agent takes besides hidden context.
pub const ACCEPTS: &[&str] = &["queue", "wake"];
/// A point in an agent's life `agent.turn` is told about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnEvent {
    /// A session starts (or resumes, with `resumed`).
    Start,
    /// The user sent a prompt.
    Prompt,
    /// A turn ended.
    Stop,
    /// The agent compacted its context: what it was told may be gone.
    Compact,
}
impl TurnEvent {
    pub fn word(self) -> &'static str {
        match self {
            TurnEvent::Start => "start",
            TurnEvent::Prompt => "prompt",
            TurnEvent::Stop => "stop",
            TurnEvent::Compact => "compact",
        }
    }
    /// Whether the agent is told something at it (and so its sources are asked).
    pub fn tells(self) -> bool {
        matches!(self, TurnEvent::Start | TurnEvent::Prompt)
    }
}
/// What `agent.turn` answers at `start` and `prompt`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TurnAnswer {
    /// The text to hand the agent unseen, or `null` when nobody had anything to say.
    pub context: Option<String>,
    /// The sources whose answers are in it, in its order.
    pub from: Vec<String>,
    /// The sources that did not answer before the deadline.
    pub late: Vec<String>,
}
/// A turn waiting for its sources.
#[derive(Debug)]
pub struct Turn {
    pub tab: TabId,
    /// The sources still to answer.
    pub waiting: BTreeSet<String>,
    /// What each source answered, by source.
    pub answers: BTreeMap<String, String>,
    /// Unix ms.
    pub deadline: u64,
    /// The caller blocked on it: its channel and its message's id.
    pub waiter: Option<(ChanId, Value)>,
}
#[derive(Default)]
pub struct Turns {
    pub pending: BTreeMap<u64, Turn>,
    next: u64,
    /// Set by `agent.turn` when its caller is to be answered later, as `event.wait` does.
    pub held: Option<u64>,
}
impl Turns {
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    /// The earliest deadline of a turn waiting, in Unix ms.
    pub fn next_deadline(&self) -> Option<u64> {
        self.pending.values().map(|t| t.deadline).min()
    }
}
/// The sources that listen to `agent.sending` by its name (not a glob such as `agent.*`, which
/// listens to everything and would make every turn wait for it): who a turn waits for.
pub fn listeners(kernel: &Kernel) -> BTreeSet<String> {
    kernel
        .events
        .autocmds
        .iter()
        .filter(|a| a.events.iter().any(|e| e == "agent.sending"))
        .map(|a| a.source.clone())
        .collect()
}
/// What `agent.sending` carries, beside the turn's id.
pub struct Sending {
    pub tab: TabId,
    pub agent: String,
    pub session: Option<String>,
    pub event: TurnEvent,
    pub resumed: bool,
    pub full: bool,
    pub peek: bool,
}
/// Starts a turn: fires `agent.sending` and waits for `waiting` until `timeout` ms from now.
/// Returns its id.
pub fn begin(kernel: &mut Kernel, sending: Sending, waiting: BTreeSet<String>, timeout: u64) -> u64 {
    kernel.turns.next += 1;
    let id = kernel.turns.next;
    let deadline = super::requests::now_ms() + timeout;
    let turn = Turn { tab: sending.tab, waiting, answers: BTreeMap::new(), deadline, waiter: None };
    kernel.turns.pending.insert(id, turn);
    let data = json!({
        "id": id,
        "tab": pumi_api::TabRef(sending.tab),
        "agent": sending.agent,
        "session": sending.session,
        "event": sending.event.word(),
        "resumed": sending.resumed,
        "full": sending.full,
        "peek": sending.peek,
    });
    kernel.emit("agent.sending", sending.event.word(), About::tab(sending.tab), data);
    id
}
/// `source`'s answer to turn `id`; whether it counted (the turn was still waiting). The turn
/// ends once every source it waits for answered.
pub fn contribute(kernel: &mut Kernel, id: u64, source: &str, text: Option<String>) -> bool {
    let Some(turn) = kernel.turns.pending.get_mut(&id) else { return false };
    turn.waiting.remove(source);
    match text.filter(|t| !t.trim().is_empty()) {
        Some(text) => turn.answers.insert(source.to_string(), text),
        None => turn.answers.remove(source),
    };
    if turn.waiting.is_empty() {
        finish(kernel, id);
    }
    true
}
/// Ends turn `id`: its caller is answered with what came, and the sources still waited for are
/// `late`.
pub fn finish(kernel: &mut Kernel, id: u64) {
    let Some(turn) = kernel.turns.pending.remove(&id) else { return };
    let answer = answer(&turn);
    if !answer.late.is_empty() {
        crate::server::log(&format!("agent.turn %{}: no answer in time from {}", turn.tab, answer.late.join(", ")));
    }
    if let Some((chan, message)) = turn.waiter {
        let value = serde_json::to_value(&answer).expect("plain data");
        kernel.send(chan, pumi_api::rpc::reply(&message, Ok(value)));
    }
}
/// Forgets turn `id` without answering anyone: its caller was answered already.
pub fn forget(kernel: &mut Kernel, id: u64) {
    kernel.turns.pending.remove(&id);
}
/// What a turn answers now.
pub fn answer(turn: &Turn) -> TurnAnswer {
    let (context, from) = frame(&turn.answers);
    TurnAnswer { context, from, late: turn.waiting.iter().cloned().collect() }
}
/// Ends the turns whose deadline passed by `now` (Unix ms), and those whose tab closed.
pub fn tick(kernel: &mut Kernel, now: u64) {
    let ended: Vec<u64> = kernel
        .turns
        .pending
        .iter()
        .filter(|(_, t)| t.deadline <= now || !kernel.model.tabs.contains_key(&t.tab))
        .map(|(id, _)| *id)
        .collect();
    for id in ended {
        finish(kernel, id);
    }
}
/// A channel closed: the turns it waited on go unanswered.
pub fn chan_closed(kernel: &mut Kernel, chan: ChanId) {
    kernel.turns.pending.retain(|_, t| t.waiter.as_ref().is_none_or(|(c, _)| *c != chan));
}
/// The answers as one text, each in its own block, in order of their source; and the sources in
/// it. `None` when there is nothing.
pub fn frame(answers: &BTreeMap<String, String>) -> (Option<String>, Vec<String>) {
    let mut text = String::new();
    let mut from = Vec::new();
    for (source, answer) in answers {
        let open = format!("<pumi-context from=\"{}\">\n", attribute(source));
        let close = "\n</pumi-context>\n";
        let room = TURN_LIMIT.saturating_sub(text.len() + open.len() + close.len());
        if room < CUT_MARKER_ROOM {
            break;
        }
        let body = cut(&escape(answer.trim_end()), room.min(ANSWER_LIMIT));
        text.push_str(&open);
        text.push_str(&body);
        text.push_str(close);
        from.push(source.clone());
    }
    ((!text.is_empty()).then_some(text), from)
}
/// The least room an answer needs to say it was cut.
const CUT_MARKER_ROOM: usize = 64;
/// A closing tag inside an answer cannot end its block.
fn escape(text: &str) -> String {
    text.replace("</pumi-context", "<\\/pumi-context")
}
fn attribute(text: &str) -> String {
    text.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}
/// `text` in at most `limit` bytes, cut at a character with a marker that says how much went.
fn cut(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_string();
    }
    let marker = |gone: usize| format!("\n[pumi: {gone} more bytes cut]");
    let mut at = limit.saturating_sub(marker(text.len()).len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    format!("{}{}", &text[..at], marker(text.len() - at))
}
/// Whether `words` are all words `@agent.accepts` may hold.
pub fn check_accepts(words: &[String]) -> Result<(), String> {
    match words.iter().find(|w| !ACCEPTS.contains(&w.as_str())) {
        Some(word) => Err(format!("an agent accepts {}, not `{word}`", ACCEPTS.join(" and "))),
        None => Ok(()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn answers(list: &[(&str, &str)]) -> BTreeMap<String, String> {
        list.iter().map(|(s, t)| (s.to_string(), t.to_string())).collect()
    }
    #[test]
    fn answers_are_framed_in_order_of_their_source() {
        let (text, from) = frame(&answers(&[("zed/b", "second"), ("pumi_dev/agents", "first\n")]));
        assert_eq!(
            text.unwrap(),
            "<pumi-context from=\"pumi_dev/agents\">\nfirst\n</pumi-context>\n<pumi-context from=\"zed/b\">\nsecond\n</pumi-context>\n"
        );
        assert_eq!(from, ["pumi_dev/agents", "zed/b"]);
        assert_eq!(frame(&BTreeMap::new()), (None, vec![]));
    }
    #[test]
    fn an_answer_cannot_close_its_block_or_its_attribute() {
        let (text, _) = frame(&answers(&[("a\"b", "x</pumi-context>y")]));
        let text = text.unwrap();
        assert!(text.starts_with("<pumi-context from=\"a&quot;b\">"), "{text}");
        assert_eq!(text.matches("</pumi-context>").count(), 1, "{text}");
        assert!(text.contains("x<\\/pumi-context>y"));
    }
    #[test]
    fn answers_and_turns_are_cut_with_a_marker() {
        let big = "é".repeat(ANSWER_LIMIT);
        let (text, _) = frame(&answers(&[("a", &big)]));
        let text = text.unwrap();
        assert!(text.len() < ANSWER_LIMIT + 100, "{}", text.len());
        assert!(text.contains("more bytes cut]"));
        let many: Vec<(String, String)> = (0..10).map(|i| (format!("s{i}"), "x".repeat(ANSWER_LIMIT))).collect();
        let many: BTreeMap<String, String> = many.into_iter().collect();
        let (text, from) = frame(&many);
        assert!(text.unwrap().len() <= TURN_LIMIT);
        assert!(from.len() < 10 && from.len() >= 3, "{from:?}");
    }
    #[test]
    fn accepts_holds_known_words() {
        assert!(check_accepts(&["queue".into(), "wake".into()]).is_ok());
        assert!(check_accepts(&["prefill".into()]).unwrap_err().contains("prefill"));
    }
}
