//! What an agent (or a person) driving pumi from a shell asks most: which tabs there are and
//! what runs in them (`pumi tab list`, `pumi agent list`), where one agent is (`pumi agent
//! status`), waiting until it is done (`pumi agent wait`), and asking another agent something and
//! hearing back (`pumi agent ask`, `pumi agent reply`).
//!
//! These are the CLI's own: they are made of the server's functions (`tab.list`, `tab.get`,
//! `event.on`, `agent.queue`, `agent.unqueue`, `event.emit`), so a script can do the same.
//!
//! A message between agents is a queued message (`agent.queue`): it reaches the agent when its
//! turn ends (or at once, for one that takes messages while idle, like pi), and never by typing
//! into it. `ask` says in the message who asks and how to answer; `reply` queues the answer for
//! the asker and fires `cli.agent.replied` (match: the asker's tab), which `ask --wait` waits for.
//! An answer `ask --wait` heard is taken back out of the asker's queue, so it is not told twice.
use crate::client::{Address, Client};
use crate::usage;
use pumi_api::{Error, ErrorKind};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

/// The event a reply fires, matched by the asker's tab.
pub const REPLIED: &str = "cli.agent.replied";
/// `wait`'s and `ask --wait`'s exit status when their time runs out (as `timeout(1)`'s).
const TIMED_OUT: i32 = 124;
/// What an agent is when it is not working: `agent wait`'s default.
const SETTLED: &[&str] = &["idle", "done", "blocked", "error"];

/// The CLI's own agent verbs, for `pumi help`'s listing.
pub const VERBS: &[(&str, &str)] = &[
    ("agent list", "The agents: tab, name, status, model, messages waiting, title, directory."),
    ("agent status", "One agent's status, name and model: `pumi agent status %3`."),
    ("agent wait", "Until an agent is done working (`--until idle,done,…`, `--for 10m`)."),
    ("agent ask", "Ask another agent something; `--wait` for its answer."),
    ("agent reply", "Answer an agent that asked you: `pumi agent reply %4 'text'`."),
];

/// Whether `words` are one of these commands; runs it if they are.
pub fn run(address: &Address, words: &[String], json: bool) -> Option<Result<i32, Error>> {
    let (noun, verb) = (words.first()?.as_str(), words.get(1).map(String::as_str));
    let rest = words.get(2..).unwrap_or_default();
    let json = json || rest.iter().any(|word| word == "--json");
    let rest: Vec<String> = rest.iter().filter(|word| *word != "--json").cloned().collect();
    let asks_help = rest.iter().any(|word| word == "--help" || word == "-h");
    let help = |form: &str| -> Option<Result<i32, Error>> {
        println!("{form}");
        Some(Ok(0))
    };
    match (noun, verb) {
        // `tab list` is the server's with `--json`: ids, as scripts read them.
        ("tab", Some("list")) if !json && !asks_help && !rest.iter().any(|w| w == "--closed") => {
            Some(tab_list(address, &rest))
        }
        ("agent", Some("list")) if asks_help => help(LIST),
        ("agent", Some("list")) => Some(agent_list(address, &rest, json)),
        ("agent", Some("status")) if asks_help => help(STATUS),
        ("agent", Some("status")) => Some(status(address, &rest, json)),
        ("agent", Some("wait")) if asks_help => help(WAIT),
        ("agent", Some("wait")) => Some(wait(address, &rest, json)),
        ("agent", Some("ask")) if asks_help => help(ASK),
        ("agent", Some("ask")) => Some(ask(address, &rest, json)),
        ("agent", Some("reply")) if asks_help => help(REPLY),
        ("agent", Some("reply")) => Some(reply(address, &rest)),
        _ => None,
    }
}

const LIST: &str = "pumi agent list [--session ID] [--view ID] [--json]
    The tabs that are agents, as a table: tab, agent, status, model, messages waiting, title and
    directory. `--json`: each tab's `tab.get`.";
const STATUS: &str = "pumi agent status [TAB] [--json]
    An agent's status (working, idle, done, blocked, error), name, model and messages waiting:
    `%3 claude working claude-opus-4 (2 waiting)`. No TAB: yours. `--json`: its agent trait.";
const WAIT: &str = "pumi agent wait TAB [--until STATUS[,STATUS…]] [--for DURATION] [--json]
    Until the agent in TAB is done working: its status is one of --until (idle, done, blocked or
    error by default). Prints the status it reached. Exits 124 when --for runs out, 1 when the
    tab closes.";
const ASK: &str = "pumi agent ask TAB TEXT [--wait] [--for DURATION] [--json]
    Send the agent in TAB a message that says it is from you (your tab, your agent, your
    directory) and how to answer: `pumi agent reply YOUR_TAB 'answer'`. It is queued: the agent
    gets it when its turn ends, or at once if it takes messages while idle. Prints the message's
    id (`pumi agent unqueue ID` takes it back). With --wait, waits for the answer and prints it
    instead (exits 124 when --for runs out). An answer you did not wait for comes to you as a
    message, when your turn ends.";
const REPLY: &str = "pumi agent reply TAB TEXT
    Answer the agent in TAB, which asked you something (`pumi agent ask`): it hears at once if it
    waits for it, and as a message when its turn ends if not.";

/// The words of a command line: its positionals, and its `--name value` flags.
struct Words {
    positionals: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

fn words(rest: &[String], boolean: &[&str]) -> Result<Words, Error> {
    let mut out = Words { positionals: Vec::new(), flags: Vec::new() };
    let mut more = rest.iter();
    while let Some(word) = more.next() {
        if word == "--" {
            out.positionals.extend(more.by_ref().cloned());
            break;
        }
        match word.strip_prefix("--") {
            Some(long) => {
                let (name, inline) = long.split_once('=').map_or((long, None), |(n, v)| (n, Some(v.to_string())));
                let value = if boolean.contains(&name) {
                    inline
                } else {
                    Some(
                        inline
                            .or_else(|| more.next().cloned())
                            .ok_or_else(|| usage(format!("--{name} needs a value")))?,
                    )
                };
                out.flags.push((name.to_string(), value));
            }
            None => out.positionals.push(word.clone()),
        }
    }
    Ok(out)
}
impl Words {
    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_deref())
    }
    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(n, _)| n == name)
    }
    fn only(&self, allowed: &[&str], form: &str) -> Result<(), Error> {
        match self.flags.iter().find(|(n, _)| !allowed.contains(&n.as_str())) {
            Some((name, _)) => Err(usage(format!("`--{name}` is not one of its flags\n{form}"))),
            None => Ok(()),
        }
    }
}

/// A tab as its id: `%3`, and `3` will do.
fn tab_id(word: &str) -> String {
    match word.parse::<u64>() {
        Ok(n) if n > 0 => format!("%{n}"),
        _ => word.to_string(),
    }
}

/// The caller's own tab: `$PUMI_TAB`.
fn own_tab() -> Option<String> {
    std::env::var("PUMI_TAB").ok().filter(|tab| tab.starts_with('%'))
}

fn connect(address: &Address) -> Result<Client, Error> {
    let mut client = Client::open(address, true)?;
    client.introduce("cli")?;
    Ok(client)
}

/// `tab.list`'s filters from the command line: `--session`, `--view`, `--block`, `--trait`.
fn filters(words: &Words) -> Value {
    let mut opts = serde_json::Map::new();
    for name in ["session", "view", "block", "trait"] {
        if let Some(value) = words.flag(name) {
            opts.insert(name.into(), json!(value));
        }
    }
    Value::Object(opts)
}

/// Each tab `opts` lists, as `tab.get` has it; one that closed meanwhile is left out.
fn tabs(client: &mut Client, opts: Value) -> Result<Vec<Value>, Error> {
    let ids = client.call("tab.list", json!([opts]))?;
    let mut out = Vec::new();
    for id in ids.as_array().into_iter().flatten() {
        match client.call("tab.get", json!([id])) {
            Ok(tab) => out.push(tab),
            Err(err) if err.kind == ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(out)
}

/// `pumi tab list [--session ID] [--view ID] [--block NAME] [--trait NAME]`: a table of the tabs.
fn tab_list(address: &Address, rest: &[String]) -> Result<i32, Error> {
    let words = words(rest, &[])?;
    let form = "pumi tab list [--session ID] [--view ID] [--block NAME] [--trait NAME] [--closed] [--json]";
    words.only(&["session", "view", "block", "trait"], form)?;
    if !words.positionals.is_empty() {
        return Err(usage(format!("it takes no arguments\n{form}")));
    }
    let mut client = connect(address)?;
    let tabs = tabs(&mut client, filters(&words))?;
    let columns = ["TAB", "VIEW", "PANE", "BLOCK", "AGENT", "STATUS", "MODEL", "TITLE", "CWD"];
    let rows: Vec<Vec<String>> = tabs
        .iter()
        .map(|tab| {
            let agent = &tab["traits"]["agent"];
            vec![
                id_cell(tab),
                text(&tab["view"]),
                place_cell(&tab["place"]),
                text(&tab["block"]),
                text(&agent["name"]),
                text(&agent["status"]),
                text(&agent["model"]),
                text(&tab["title"]),
                home(&text(&tab["cwd"])),
            ]
        })
        .collect();
    print!("{}", table(&columns, &rows));
    Ok(0)
}

/// `pumi agent list`: a table of the agents' tabs.
fn agent_list(address: &Address, rest: &[String], json: bool) -> Result<i32, Error> {
    let words = words(rest, &[])?;
    words.only(&["session", "view"], LIST)?;
    let mut opts = filters(&words);
    opts["trait"] = json!("agent");
    let mut client = connect(address)?;
    let tabs = tabs(&mut client, opts)?;
    if json {
        println!("{}", Value::Array(tabs));
        return Ok(0);
    }
    let columns = ["TAB", "AGENT", "STATUS", "MODEL", "WAITING", "TITLE", "CWD", "PANE"];
    let rows: Vec<Vec<String>> = tabs
        .iter()
        .map(|tab| {
            let agent = &tab["traits"]["agent"];
            let waiting = agent["queue"].as_array().map_or(0, Vec::len);
            vec![
                id_cell(tab),
                text(&agent["name"]),
                text(&agent["status"]),
                text(&agent["model"]),
                if waiting == 0 { String::new() } else { waiting.to_string() },
                text(&tab["title"]),
                home(&text(&tab["cwd"])),
                place_cell(&tab["place"]),
            ]
        })
        .collect();
    print!("{}", table(&columns, &rows));
    Ok(0)
}

/// `pumi agent status [TAB]`: one line.
fn status(address: &Address, rest: &[String], json: bool) -> Result<i32, Error> {
    let words = words(rest, &[])?;
    words.only(&[], STATUS)?;
    let tab = match words.positionals.as_slice() {
        [] => own_tab().ok_or_else(|| usage(format!("which tab? (no $PUMI_TAB here)\n{STATUS}")))?,
        [one] => tab_id(one),
        _ => return Err(usage(format!("one tab at a time\n{STATUS}"))),
    };
    let mut client = connect(address)?;
    let got = client.call("tab.get", json!([tab]))?;
    let agent = &got["traits"]["agent"];
    if json {
        println!("{}", json!({ "tab": got["id"], "agent": agent }));
        return Ok(0);
    }
    if agent.is_null() {
        return Err(Error::not_found(format!(
            "{tab} is not an agent (it has no `@agent`); `pumi tab get {tab}` says what it is"
        )));
    }
    println!("{}", status_line(&got));
    Ok(0)
}

/// `%3 claude working claude-opus-4 (2 waiting)`
fn status_line(tab: &Value) -> String {
    let agent = &tab["traits"]["agent"];
    let mut line = format!("{} {} {}", text(&tab["id"]), text(&agent["name"]), or(&text(&agent["status"]), "unknown"));
    if let Some(model) = agent["model"].as_str().filter(|m| !m.is_empty()) {
        line.push(' ');
        line.push_str(model);
    }
    let waiting = agent["queue"].as_array().map_or(0, Vec::len);
    if waiting > 0 {
        line.push_str(&format!(" ({waiting} waiting)"));
    }
    line
}

/// `pumi agent wait TAB [--until STATUS,…] [--for DURATION]`.
fn wait(address: &Address, rest: &[String], json: bool) -> Result<i32, Error> {
    let words = words(rest, &[])?;
    words.only(&["until", "for", "timeout"], WAIT)?;
    let tab = match words.positionals.as_slice() {
        [one] => tab_id(one),
        [] => return Err(usage(format!("which agent? (`pumi agent list`)\n{WAIT}"))),
        _ => return Err(usage(format!("one tab at a time\n{WAIT}"))),
    };
    let until: Vec<String> = match words.flag("until") {
        Some(list) => list.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
        None => SETTLED.iter().map(|s| s.to_string()).collect(),
    };
    let deadline = deadline(&words)?;
    let mut client = connect(address)?;
    // Listen first, then look: a change between the two is not missed.
    let subscription = client.call("event.on", json!(["tab.*", { "match": tab }]))?;
    loop {
        let got = match client.call("tab.get", json!([tab])) {
            Ok(got) => got,
            Err(err) if err.kind == ErrorKind::NotFound => {
                return Err(Error::not_found(format!("{tab} closed before its agent was {}", until.join(" or "))));
            }
            Err(err) => return Err(err),
        };
        let now = got["traits"]["agent"]["status"].as_str().unwrap_or("").to_string();
        if until.contains(&now) {
            if json {
                println!("{}", json!({ "tab": got["id"], "status": now, "agent": got["traits"]["agent"] }));
            } else {
                println!("{}", status_line(&got));
            }
            return Ok(0);
        }
        if !heard(&mut client, &subscription, deadline)? {
            println!("{}", status_line(&got));
            return Ok(TIMED_OUT);
        }
    }
}

/// When `--for` (or `--timeout`, seconds) runs out; none without one.
fn deadline(words: &Words) -> Result<Option<Instant>, Error> {
    let millis = match (words.flag("for"), words.flag("timeout")) {
        (Some(text), _) => crate::duration(text)?,
        (None, Some(seconds)) => crate::duration(seconds)?,
        (None, None) => return Ok(None),
    };
    Ok(Some(Instant::now() + Duration::from_millis(millis)))
}

/// Waits for an event of `subscription`; `false` when `deadline` passed first.
fn heard(client: &mut Client, subscription: &Value, deadline: Option<Instant>) -> Result<bool, Error> {
    Ok(next_event(client, subscription, deadline)?.is_some())
}

fn next_event(client: &mut Client, subscription: &Value, deadline: Option<Instant>) -> Result<Option<Value>, Error> {
    loop {
        let left = match deadline {
            Some(deadline) => match deadline.checked_duration_since(Instant::now()) {
                Some(left) if !left.is_zero() => left,
                _ => return Ok(None),
            },
            None => Duration::from_secs(3600),
        };
        let Some(message) = client.poll(left)? else { continue };
        if message["method"] == "rpc.event" && message["params"][0] == *subscription {
            return Ok(Some(message["params"][1].clone()));
        }
    }
}

/// Who the caller is, as a message says it: `the pi agent in tab %4 (in ~/repo)`.
fn sender(client: &mut Client, tab: Option<&str>) -> Result<String, Error> {
    let Some(tab) = tab else { return Ok("someone at a command line (no tab of theirs to answer)".into()) };
    let got = client.call("tab.get", json!([tab]))?;
    let agent = got["traits"]["agent"]["name"].as_str().filter(|name| !name.is_empty());
    let cwd = got["cwd"].as_str().map(|cwd| format!(" (in {})", home(cwd))).unwrap_or_default();
    Ok(match agent {
        Some(agent) => format!("the {agent} agent in tab {tab}{cwd}"),
        None => format!("the program in tab {tab}{cwd}"),
    })
}

/// A message's TEXT: the words after the tab, as one text; `-` reads it from stdin.
fn message_text(positionals: &[String], form: &str) -> Result<String, Error> {
    let text = match positionals {
        [only] if only == "-" => {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                .map_err(|err| Error::exception(format!("stdin: {err}")))?;
            text
        }
        words => words.join(" "),
    };
    if text.trim().is_empty() {
        return Err(usage(format!("what to say?\n{form}")));
    }
    Ok(text)
}

/// The message `ask` queues: who asks, how to answer, and the question.
pub fn asked(from: &str, own: Option<&str>, text: &str) -> String {
    let answer = match own {
        Some(tab) => format!(" Answer with: pumi agent reply {tab} \"your answer\""),
        None => String::new(),
    };
    format!("[pumi] A message from {from}.{answer}\n\n{text}")
}

/// The message `reply` queues for the asker.
pub fn answered(from: &str, text: &str) -> String {
    format!("[pumi] An answer from {from}:\n\n{text}")
}

/// `pumi agent ask TAB TEXT [--wait] [--for DURATION]`.
fn ask(address: &Address, rest: &[String], json: bool) -> Result<i32, Error> {
    let words = words(rest, &["wait"])?;
    words.only(&["wait", "for", "timeout"], ASK)?;
    let Some((to, said)) = words.positionals.split_first() else {
        return Err(usage(format!("which agent? (`pumi agent list`)\n{ASK}")));
    };
    let to = tab_id(to);
    let text = message_text(said, ASK)?;
    let own = own_tab();
    let waits = words.has("wait");
    if waits && own.is_none() {
        return Err(usage(format!("--wait needs a tab of yours to answer ($PUMI_TAB)\n{ASK}")));
    }
    if own.as_deref() == Some(to.as_str()) {
        return Err(usage("that is your own tab"));
    }
    let deadline = deadline(&words)?;
    let mut client = connect(address)?;
    let from = sender(&mut client, own.as_deref())?;
    // Listen before asking: an answer cannot come before it is heard for.
    let subscription = match (&own, waits) {
        (Some(own), true) => Some(client.call("event.on", json!([REPLIED, { "match": own }]))?),
        _ => None,
    };
    let id = client.call("agent.queue", json!([to, asked(&from, own.as_deref(), &text), {}]))?;
    let Some(subscription) = subscription else {
        if json {
            println!("{}", json!({ "id": id, "to": to }));
        } else {
            println!("{id}");
        }
        return Ok(0);
    };
    loop {
        let Some(event) = next_event(&mut client, &subscription, deadline)? else {
            // Not answered in time: the question stays queued, and a late answer comes as a message.
            eprintln!(
                "{}",
                json!({ "kind": "timeout", "message": format!("{to} did not answer in time; its answer will come to you as a message"), "data": { "id": id } })
            );
            return Ok(TIMED_OUT);
        };
        let data = &event["data"];
        if data["from"].as_str() != Some(to.as_str()) {
            continue;
        }
        // Heard here: the copy queued for this agent's next turn goes.
        if let Some(queued) = data["queued"].as_u64() {
            let _ = client.call("agent.unqueue", json!([queued]));
        }
        if json {
            println!("{}", json!({ "id": id, "from": to, "text": data["text"] }));
        } else {
            println!("{}", data["text"].as_str().unwrap_or_default());
        }
        return Ok(0);
    }
}

/// `pumi agent reply TAB TEXT`.
fn reply(address: &Address, rest: &[String]) -> Result<i32, Error> {
    let words = words(rest, &[])?;
    words.only(&[], REPLY)?;
    let Some((to, said)) = words.positionals.split_first() else {
        return Err(usage(format!("whom to answer? (the tab in the message)\n{REPLY}")));
    };
    let to = tab_id(to);
    let text = message_text(said, REPLY)?;
    let own = own_tab();
    let mut client = connect(address)?;
    let from = sender(&mut client, own.as_deref())?;
    // Queued for the asker's next turn, if it takes messages; heard at once if it waits.
    let queued = match client.call("agent.queue", json!([to, answered(&from, &text), {}])) {
        Ok(id) => Some(id),
        Err(err) if err.kind == ErrorKind::InvalidParams => None,
        Err(err) => return Err(err),
    };
    let data = json!({ "from": own, "text": text, "queued": queued });
    client.call("event.emit", json!([REPLIED, { "match": to, "tab": to, "data": data }]))?;
    if queued.is_none() {
        eprintln!("{to} takes no queued messages: only an `ask --wait` there hears this");
    }
    Ok(0)
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn or(text: &str, otherwise: &str) -> String {
    if text.is_empty() { otherwise.to_string() } else { text.to_string() }
}

/// A tab's id, `*` after the caller's own.
fn id_cell(tab: &Value) -> String {
    let id = text(&tab["id"]);
    if own_tab().as_deref() == Some(id.as_str()) { format!("{id}*") } else { id }
}

/// Where a tab is: its pane, or its kind of place (`float`, `hidden`).
fn place_cell(place: &Value) -> String {
    match place["pane"].as_str() {
        Some(pane) => pane.to_string(),
        None => text(&place["kind"]),
    }
}

/// A path with the home directory as `~`.
fn home(path: &str) -> String {
    let home = pumi_proto::paths::home_dir().display().to_string();
    match path.strip_prefix(&home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// Rows as columns, each as wide as its widest cell (the last as it is), cut at 48 chars.
pub fn table(columns: &[&str], rows: &[Vec<String>]) -> String {
    const MOST: usize = 48;
    let cut = |cell: &str| -> String {
        let first = cell.lines().next().unwrap_or("");
        if first.chars().count() <= MOST {
            return first.to_string();
        }
        let mut out: String = first.chars().take(MOST - 1).collect();
        out.push('…');
        out
    };
    let rows: Vec<Vec<String>> = rows.iter().map(|row| row.iter().map(|cell| cut(cell)).collect()).collect();
    // Columns no row has anything in are left out.
    let shown: Vec<usize> = match rows.is_empty() {
        true => (0..columns.len()).collect(),
        false => (0..columns.len()).filter(|i| rows.iter().any(|row| !row[*i].is_empty())).collect(),
    };
    let width = |i: usize| rows.iter().map(|row| row[i].chars().count()).chain([columns[i].len()]).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns.len()).map(width).collect();
    let line = |cells: Vec<&str>| -> String {
        let last = shown.len().saturating_sub(1);
        let mut out = String::new();
        for (n, i) in shown.iter().enumerate() {
            let cell = cells[*i];
            if n == last {
                out.push_str(cell);
            } else {
                out.push_str(cell);
                out.push_str(&" ".repeat(widths[*i] - cell.chars().count() + 2));
            }
        }
        out.trim_end().to_string() + "\n"
    };
    let mut out = line(columns.to_vec());
    for row in &rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_table_lines_its_columns_up_and_leaves_empty_ones_out() {
        let rows = vec![
            vec!["%3".to_string(), String::new(), "claude".to_string()],
            vec!["%12".to_string(), String::new(), "pi".to_string()],
        ];
        assert_eq!(table(&["TAB", "VIEW", "AGENT"], &rows), "TAB  AGENT\n%3   claude\n%12  pi\n");
    }
    #[test]
    fn a_question_says_who_asks_and_how_to_answer() {
        let text = asked("the pi agent in tab %4 (in ~/repo)", Some("%4"), "Will you edit src/x.rs?");
        assert!(text.starts_with("[pumi] A message from the pi agent in tab %4"), "{text}");
        assert!(text.contains("pumi agent reply %4 \"your answer\""), "{text}");
        assert!(text.ends_with("\n\nWill you edit src/x.rs?"), "{text}");
        let anonymous = asked("someone at a command line (no tab of theirs to answer)", None, "hi");
        assert!(!anonymous.contains("pumi agent reply"), "nobody to answer: {anonymous}");
        assert!(
            answered("the claude agent in tab %7", "No.")
                .starts_with("[pumi] An answer from the claude agent in tab %7:")
        );
    }
    #[test]
    fn a_tab_is_its_id_and_a_number_will_do() {
        assert_eq!(tab_id("3"), "%3");
        assert_eq!(tab_id("%3"), "%3");
        assert_eq!(tab_id("0"), "0");
    }
    #[test]
    fn words_are_positionals_and_flags() {
        let given: Vec<String> =
            ["%3", "will", "you", "--wait", "--for", "2m", "--", "--literal"].map(String::from).to_vec();
        let words = words(&given, &["wait"]).unwrap();
        assert_eq!(words.positionals, vec!["%3", "will", "you", "--literal"]);
        assert!(words.has("wait"));
        assert_eq!(words.flag("for"), Some("2m"));
        assert!(words.only(&["wait"], "form").is_err(), "--for is not allowed there");
    }
}
