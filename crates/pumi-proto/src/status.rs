use serde::{Deserialize, Serialize};
pub const OSC: &str = "7501";
pub const MAX_SEQUENCE: usize = 4096;
pub const MAX_RECORDS: usize = 256;
const MAX_KEY: usize = 16;
const MAX_MSG: (usize, usize) = (2732, 2048);
const MAX_TITLE: (usize, usize) = (256, 192);
const MAX_APP: usize = 32;
const MAX_ID: usize = 128;
const MAX_SEGMENT: usize = 32;
const MAX_DEPTH: usize = 8;
pub const QUERY_REPLY: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth\x1b\\";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Idle,
    Working,
    Done,
    Blocked,
    Error,
}
impl State {
    pub const ALL: [State; 5] = [State::Idle, State::Working, State::Done, State::Error, State::Blocked];
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "idle" => State::Idle,
            "working" => State::Working,
            "done" => State::Done,
            "blocked" => State::Blocked,
            "error" => State::Error,
            _ => return None,
        })
    }
    pub fn is_running(self) -> bool {
        matches!(self, State::Working | State::Blocked)
    }
    pub fn is_result(self) -> bool {
        matches!(self, State::Done | State::Error)
    }
    pub fn word(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::Done => "done",
            State::Blocked => "blocked",
            State::Error => "error",
        }
    }
    pub fn urgency(self) -> u8 {
        match self {
            State::Idle => 0,
            State::Working => 1,
            State::Done => 2,
            State::Error => 3,
            State::Blocked => 4,
        }
    }
}
pub fn most_urgent<'a>(words: impl IntoIterator<Item = &'a str>) -> &'a str {
    words
        .into_iter()
        .filter_map(|word| Some((State::parse(word)?.urgency(), word)))
        .max_by_key(|(urgency, _)| *urgency)
        .map_or("", |(_, word)| word)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Permission,
    Question,
    Auth,
}
impl Kind {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "permission" => Kind::Permission,
            "question" => Kind::Question,
            "auth" => Kind::Auth,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sequence {
    Query,
    Report(Record),
    Clear(Option<String>),
}
pub fn parse(body: &str) -> Option<Sequence> {
    if body.len() + OSC.len() + 4 > MAX_SEQUENCE {
        return None;
    }
    if body.trim() == "?" {
        return Some(Sequence::Query);
    }
    let mut pairs: Vec<(&str, &str)> = Vec::new();
    for pair in body.split(':') {
        let Some((key, value)) = pair.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        let key_ok = !key.is_empty() && key.len() <= MAX_KEY && key.bytes().all(|b| b.is_ascii_lowercase());
        let value_ok = value.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.,+/=-".contains(&b));
        if key_ok && value_ok {
            pairs.retain(|(k, _)| *k != key);
            pairs.push((key, value));
        }
    }
    let get = |key: &str| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    let id = match get("id") {
        Some(id) => Some(valid_id(id)?.to_string()),
        None => None,
    };
    let state = get("state")?;
    if state == "clear" {
        return Some(Sequence::Clear(id));
    }
    let state = State::parse(state)?;
    let title = match get("title") {
        Some(t) => Some(text(t, MAX_TITLE)?),
        None => None,
    };
    let msg = match get("msg") {
        Some(m) => Some(text(m, MAX_MSG)?),
        None => None,
    };
    let kind = get("kind").filter(|_| state == State::Blocked).and_then(Kind::parse);
    let progress = get("progress")
        .filter(|_| state.is_running())
        .filter(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|p| p.parse::<u8>().ok())
        .filter(|p| *p <= 100);
    let app = get("app").filter(|a| (1..=MAX_APP).contains(&a.len()) && a.bytes().all(id_byte)).map(str::to_string);
    Some(Sequence::Report(Record { id: id.unwrap_or_default(), state, kind, progress, app, title, msg }))
}
fn id_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"_.+-".contains(&b)
}
fn valid_id(id: &str) -> Option<&str> {
    let segments: Vec<&str> = id.split('/').collect();
    let ok = id.len() <= MAX_ID
        && segments.len() <= MAX_DEPTH
        && segments.iter().all(|s| (1..=MAX_SEGMENT).contains(&s.len()) && s.bytes().all(id_byte));
    ok.then_some(id)
}
fn text(encoded: &str, (max_encoded, max_decoded): (usize, usize)) -> Option<String> {
    if encoded.len() > max_encoded {
        return None;
    }
    let bytes = base64(encoded)?;
    if bytes.len() > max_decoded {
        return None;
    }
    let text = String::from_utf8(bytes).ok()?;
    if text.chars().any(|c| c.is_control()) {
        return None;
    }
    Some(text.chars().filter(|c| !invisible(*c)).collect())
}
fn invisible(c: char) -> bool {
    matches!(c, '\u{061C}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}
fn base64(text: &str) -> Option<Vec<u8>> {
    let digits = text.trim_end_matches('=');
    if text.len() - digits.len() > 2
        || digits.len() % 4 == 1
        || (text.len() != digits.len() && !text.len().is_multiple_of(4))
    {
        return None;
    }
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.as_bytes().chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= value(*c)? << (18 - 6 * i);
        }
        out.extend_from_slice(&acc.to_be_bytes()[1..chunk.len()]);
    }
    Some(out)
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Remove,
    Set(u8),
    Error(Option<u8>),
    Indeterminate,
    Paused(Option<u8>),
}
pub fn parse_progress(rest: &str) -> Option<Progress> {
    let mut fields = rest.strip_prefix(';').unwrap_or(rest).split(';').map(str::trim);
    let state = fields.next().filter(|s| !s.is_empty()).unwrap_or("0");
    let percent = fields.next().and_then(|p| p.parse::<u32>().ok()).map(|p| p.min(100) as u8);
    Some(match state {
        "0" => Progress::Remove,
        "1" => Progress::Set(percent.unwrap_or(0)),
        "2" => Progress::Error(percent),
        "3" => Progress::Indeterminate,
        "4" => Progress::Paused(percent),
        _ => return None,
    })
}
/// What a program says without OSC 7501, mapped to the root record while it has not reported:
/// OSC 9;4 progress, a spinner in its title (OSC 0/2), a desktop notification (OSC 9, 99, 777).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Fallback {
    Progress,
    Title,
    Notification,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Records {
    records: Vec<Stored>,
    clock: u64,
    #[serde(default)]
    reported: bool,
    /// The process group whose report stopped the fallbacks: they count again once it is gone.
    #[serde(default)]
    reporter: Option<i32>,
    /// Which fallback the root record is, while one is.
    #[serde(default)]
    fallback: Option<Fallback>,
    /// Whether the title spins now: a fallback goes on a change, not on each frame.
    #[serde(default)]
    spinning: bool,
}
/// A title that spins: a braille frame first (`⠋ Working`), as most CLIs and agents draw one.
/// The blank pattern (U+2800) is not a frame.
pub fn spins(title: &str) -> bool {
    title.trim_start().chars().next().is_some_and(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    record: Record,
    updated: u64,
    owner: Option<i32>,
}
impl Records {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
    pub fn apply(&mut self, sequence: Sequence, owner: Option<i32>) -> bool {
        if !matches!(sequence, Sequence::Query) {
            // A real report replaces whatever a fallback said (the spec: a terminal stops mapping
            // OSC 9;4 once it has an OSC 7501 report).
            self.reported = true;
            self.reporter = owner;
            self.fallback = None;
        }
        self.put(sequence, owner)
    }
    /// The terminal's foreground is `now`: a program that reported and is gone no longer stops
    /// the fallbacks of the next.
    pub fn foreground(&mut self, now: Option<i32>) {
        if self.reported && self.reporter.is_some() && self.reporter != now {
            self.reported = false;
            self.reporter = None;
        }
    }
    /// The root record as a fallback says it (its callers check that no real report has the say).
    fn fallback_root(
        &mut self,
        fallback: Fallback,
        state: State,
        progress: Option<u8>,
        msg: Option<String>,
        owner: Option<i32>,
    ) -> bool {
        let record = Record { id: String::new(), state, kind: None, progress, app: None, title: None, msg };
        self.fallback = Some(fallback);
        self.put(Sequence::Report(record), owner)
    }
    /// The title changed (OSC 0/2): one that starts spinning is `working`; one that stops is
    /// `idle` again, unless something since (a notification) left a result.
    pub fn title(&mut self, title: &str, owner: Option<i32>) -> bool {
        let spinning = spins(title);
        if spinning == self.spinning {
            return false;
        }
        self.spinning = spinning;
        if self.reported {
            return false;
        }
        let root = self.records.iter().find(|s| s.record.id.is_empty()).map(|s| s.record.state);
        if spinning {
            // OSC 9;4 says more (a percent) than a spinner does.
            if self.fallback == Some(Fallback::Progress) && root == Some(State::Working) {
                return false;
            }
            return self.fallback_root(Fallback::Title, State::Working, None, None, owner);
        }
        if self.fallback == Some(Fallback::Title) && root == Some(State::Working) {
            return self.fallback_root(Fallback::Title, State::Idle, None, None, owner);
        }
        false
    }
    /// The program sent a desktop notification (OSC 9, 99, 777): it has something for the user,
    /// `done` until they see it. Its text is the record's message.
    pub fn notified(&mut self, message: Option<&str>, owner: Option<i32>) -> bool {
        if self.reported {
            return false;
        }
        let msg = message
            .map(|m| m.chars().filter(|c| !c.is_control() && !invisible(*c)).take(MAX_MSG.1).collect::<String>())
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty());
        self.fallback_root(Fallback::Notification, State::Done, None, msg, owner)
    }
    /// Says something for the program that it did not report itself (an agent request pumid
    /// keeps): it goes as a report does, but OSC 9;4 progress still counts after it.
    pub fn set(&mut self, sequence: Sequence, owner: Option<i32>) -> bool {
        self.put(sequence, owner)
    }
    pub fn progress(&mut self, progress: Progress, owner: Option<i32>) -> bool {
        if self.reported {
            return false;
        }
        let last = self.records.iter().find(|s| s.record.id.is_empty()).and_then(|s| s.record.progress);
        let (state, percent) = match progress {
            Progress::Remove => {
                // A result another fallback left (a notification) is the user's to see.
                let result = self.records.iter().any(|s| s.record.id.is_empty() && s.record.state.is_result());
                if result && self.fallback != Some(Fallback::Progress) {
                    return false;
                }
                self.fallback = None;
                return self.drop_where(|r| r.id.is_empty());
            }
            Progress::Set(percent) => (State::Working, Some(percent)),
            Progress::Indeterminate => (State::Working, None),
            Progress::Paused(percent) => (State::Working, percent.or(last)),
            Progress::Error(_) => (State::Error, None),
        };
        self.fallback_root(Fallback::Progress, state, percent, None, owner)
    }
    fn put(&mut self, sequence: Sequence, owner: Option<i32>) -> bool {
        match sequence {
            Sequence::Query => false,
            Sequence::Clear(None) => self.drop_where(|_| true),
            Sequence::Clear(Some(id)) => self.drop_where(|r| beneath(&r.id, &id)),
            Sequence::Report(record) => {
                self.clock += 1;
                let stored = Stored { record, updated: self.clock, owner };
                if let Some(slot) = self.records.iter_mut().find(|s| s.record.id == stored.record.id) {
                    let changed = slot.record != stored.record;
                    *slot = stored;
                    return changed;
                }
                if self.records.len() >= MAX_RECORDS
                    && let Some(oldest) = self.records.iter().enumerate().min_by_key(|(_, s)| s.updated).map(|(i, _)| i)
                {
                    self.records.remove(oldest);
                }
                self.records.push(stored);
                true
            }
        }
    }
    /// The shell drew its prompt (OSC 133;A): its program is gone, and with it what it reported.
    pub fn prompt(&mut self) -> bool {
        self.reported = false;
        self.reporter = None;
        self.spinning = false;
        self.drop_where(|r| r.state.is_running())
    }
    pub fn exited(&mut self, owner: i32) -> bool {
        let before = self.records.len();
        self.records.retain(|s| s.owner != Some(owner) || s.record.state.is_result());
        self.records.len() != before
    }
    pub fn ended(&mut self) -> bool {
        self.drop_where(|r| !r.state.is_result())
    }
    pub fn owners(&self) -> Vec<i32> {
        let mut owners: Vec<i32> =
            self.records.iter().filter(|s| !s.record.state.is_result()).filter_map(|s| s.owner).collect();
        owners.sort_unstable();
        owners.dedup();
        owners
    }
    pub fn reset(&mut self) -> bool {
        self.reported = false;
        self.reporter = None;
        self.fallback = None;
        self.spinning = false;
        self.drop_where(|_| true)
    }
    pub fn seen(&mut self, present: Option<i32>) -> bool {
        let mut changed = false;
        self.records.retain_mut(|s| {
            if !s.record.state.is_result() {
                return true;
            }
            changed = true;
            if present.is_none() || s.owner != present {
                return false;
            }
            s.record = Record { state: State::Idle, kind: None, progress: None, msg: None, ..s.record.clone() };
            true
        });
        changed
    }
    pub fn restore(list: Vec<Record>, owner: Option<i32>) -> Self {
        let mut records = Records::default();
        for record in list.into_iter().take(MAX_RECORDS) {
            records.put(Sequence::Report(record), owner);
        }
        records
    }
    pub fn root(&self) -> Option<Record> {
        self.list().into_iter().find(|r| r.id.is_empty())
    }
    pub fn list(&self) -> Vec<Record> {
        self.records
            .iter()
            .map(|s| {
                let mut record = s.record.clone();
                if record.app.is_none() {
                    record.app = self.inherited_app(&record.id);
                }
                record
            })
            .collect()
    }
    pub fn most_demanding(&self) -> Option<Record> {
        self.list().into_iter().max_by_key(|r| (r.state.urgency(), r.id.is_empty()))
    }
    fn inherited_app(&self, id: &str) -> Option<String> {
        let mut at = id;
        while !at.is_empty() {
            at = at.rsplit_once('/').map_or("", |(parent, _)| parent);
            if let Some(app) = self.records.iter().find(|s| s.record.id == at).and_then(|s| s.record.app.clone()) {
                return Some(app);
            }
        }
        None
    }
    fn drop_where(&mut self, gone: impl Fn(&Record) -> bool) -> bool {
        let before = self.records.len();
        self.records.retain(|s| !gone(&s.record));
        self.records.len() != before
    }
}
fn beneath(id: &str, of: &str) -> bool {
    id == of || id.strip_prefix(of).is_some_and(|rest| rest.starts_with('/'))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn report(body: &str) -> Record {
        match parse(body) {
            Some(Sequence::Report(record)) => record,
            other => panic!("{body}: {other:?}"),
        }
    }
    #[test]
    fn the_specs_examples_parse() {
        let r = report(
            "state=blocked:kind=permission:app=terraform:msg=QXBwbHkgMyB0byBhZGQsIDEgdG8gY2hhbmdlLCAwIHRvIGRlc3Ryb3k/",
        );
        assert_eq!((r.state, r.kind, r.app.as_deref()), (State::Blocked, Some(Kind::Permission), Some("terraform")));
        assert_eq!(r.msg.as_deref(), Some("Apply 3 to add, 1 to change, 0 to destroy?"));
        let r = report("state=working:id=us-east:title=VVMgRWFzdA==:progress=40:msg=UHVzaGluZyBpbWFnZQ==");
        assert_eq!((r.id.as_str(), r.title.as_deref(), r.progress), ("us-east", Some("US East"), Some(40)));
        assert_eq!(r.msg.as_deref(), Some("Pushing image"));
        assert_eq!(parse("?"), Some(Sequence::Query));
        assert_eq!(parse("state=clear:id=us-east"), Some(Sequence::Clear(Some("us-east".into()))));
        assert_eq!(parse("state=clear"), Some(Sequence::Clear(None)));
    }
    #[test]
    fn pairs_are_forgiving_and_reports_are_not() {
        let r = report(" state = working :junk: Bad=x : state=done ");
        assert_eq!(r.state, State::Done);
        assert_eq!(parse("app=x"), None);
        assert_eq!(parse("state=sleeping"), None);
        assert_eq!(parse("state=idle:id=a//b"), None);
        assert_eq!(parse(&format!("state=idle:id={}", "a/".repeat(8) + "a")), None);
        assert_eq!(parse("state=idle:msg=QQ=A"), None);
        assert_eq!(parse("state=idle:msg=aGkKdGhlcmU="), None, "a newline");
        assert_eq!(parse(&format!("state=idle:title={}", "QUFB".repeat(65))), None);
        let r = report("state=idle:kind=permission:progress=50:app=bad!app");
        assert_eq!((r.kind, r.progress, r.app), (None, None, None));
        assert_eq!(report("state=working:progress=101").progress, None);
        assert_eq!(report("state=working:progress=7").progress, Some(7));
        assert_eq!(report("state=blocked:kind=whatever").kind, None);
        assert_eq!(report("state=done:msg=aGk").msg.as_deref(), Some("hi"));
        assert_eq!(report("state=done:msg=4oCuaGk").msg.as_deref(), Some("hi"));
    }
    #[test]
    fn records_follow_the_lifetimes() {
        let mut records = Records::default();
        let apply = |records: &mut Records, body: &str, owner| records.apply(parse(body).unwrap(), owner);
        apply(&mut records, "state=working:app=deploy", Some(10));
        apply(&mut records, "state=blocked:id=eu-west:kind=question", Some(10));
        apply(&mut records, "state=done:id=us-east", Some(10));
        apply(&mut records, "state=working:id=us-east/test", Some(11));
        assert_eq!(records.list().len(), 4);
        assert_eq!(records.list()[1].app.as_deref(), Some("deploy"), "inherited from the root");
        assert_eq!(records.most_demanding().unwrap().id, "eu-west");
        apply(&mut records, "state=working", Some(10));
        assert_eq!(records.root().unwrap().app, None);
        assert!(records.exited(10));
        assert_eq!(records.list().iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["us-east", "us-east/test"]);
        apply(&mut records, "state=working:id=other", None);
        assert!(records.prompt());
        assert_eq!(records.list().len(), 1);
        assert!(records.seen(None));
        assert!(records.is_empty());
        apply(&mut records, "state=done:id=a", None);
        apply(&mut records, "state=done:id=a/b", None);
        apply(&mut records, "state=done:id=ab", None);
        apply(&mut records, "state=clear:id=a", None);
        assert_eq!(records.list().len(), 1);
        assert!(records.reset());
    }
    #[test]
    fn conemu_progress_is_the_root_record_until_a_real_report() {
        assert_eq!(parse_progress(";1;40"), Some(Progress::Set(40)));
        assert_eq!(parse_progress(";1;250"), Some(Progress::Set(100)));
        assert_eq!(parse_progress(";2"), Some(Progress::Error(None)));
        assert_eq!(parse_progress(";3"), Some(Progress::Indeterminate));
        assert_eq!(parse_progress(";4;70"), Some(Progress::Paused(Some(70))));
        assert_eq!(parse_progress(""), Some(Progress::Remove));
        assert_eq!(parse_progress(";9"), None);
        let mut records = Records::default();
        records.progress(Progress::Set(40), Some(7));
        let root = records.root().unwrap();
        assert_eq!((root.state, root.progress), (State::Working, Some(40)));
        records.progress(Progress::Paused(None), Some(7));
        assert_eq!(records.root().unwrap().progress, Some(40), "paused keeps the last percent");
        records.progress(Progress::Error(Some(40)), Some(7));
        assert_eq!((records.root().unwrap().state, records.root().unwrap().progress), (State::Error, None));
        records.progress(Progress::Indeterminate, Some(7));
        assert!(records.exited(7), "a running mapped record dies with its process");
        records.progress(Progress::Set(10), None);
        assert!(records.progress(Progress::Remove, None) && records.is_empty());
        records.apply(parse("state=blocked:kind=question:msg=T2s/").unwrap(), None);
        assert!(!records.progress(Progress::Set(90), None));
        assert_eq!(records.root().unwrap().state, State::Blocked);
        records.reset();
        assert!(records.progress(Progress::Set(90), None));
    }
    #[test]
    fn a_spinning_title_is_working_until_it_stops() {
        assert!(spins("\u{280b} Fixing the build") && spins("  \u{2819}"));
        assert!(!spins("\u{2733} Claude Code") && !spins("\u{2800} blank") && !spins("π - pumi") && !spins(""));
        let mut records = Records::default();
        assert!(records.title("\u{280b} Working", Some(7)));
        assert_eq!(records.root().unwrap().state, State::Working);
        assert!(!records.title("\u{2819} Working", Some(7)), "the next frame changes nothing");
        assert!(records.title("\u{2733} Done thinking", Some(7)));
        assert_eq!(records.root().unwrap().state, State::Idle, "it stopped: the program is there, idle");
        assert!(!records.title("other title", Some(7)));
        // Progress says more than a spinner: a spinner does not replace it.
        records.progress(Progress::Set(30), Some(7));
        assert!(!records.title("\u{280b} x", Some(7)));
        assert_eq!(records.root().unwrap().progress, Some(30));
    }
    #[test]
    fn a_notification_leaves_a_result_until_it_is_seen() {
        let mut records = Records::default();
        records.title("\u{280b} Working", Some(7));
        assert!(records.notified(Some("Claude needs your permission\u{7}"), Some(7)));
        let root = records.root().unwrap();
        assert_eq!((root.state, root.msg.as_deref()), (State::Done, Some("Claude needs your permission")));
        records.title("\u{2733} idle", Some(7));
        assert_eq!(records.root().unwrap().state, State::Done, "the spinner stopping keeps the result");
        assert!(!records.progress(Progress::Remove, Some(7)), "nor does progress going");
        assert!(records.seen(Some(7)));
        assert_eq!(records.root().unwrap().state, State::Idle);
        assert!(records.title("\u{280b} again", Some(7)), "a new turn works again");
        assert_eq!(records.root().unwrap().state, State::Working);
    }
    #[test]
    fn a_real_report_stops_the_fallbacks_while_its_program_is_there() {
        let mut records = Records::default();
        records.progress(Progress::Indeterminate, Some(7));
        records.apply(parse("state=idle:app=pi").unwrap(), Some(7));
        assert!(!records.progress(Progress::Set(50), Some(7)));
        assert!(!records.title("\u{280b} x", Some(7)));
        assert!(!records.notified(Some("hi"), Some(7)));
        assert_eq!(records.root().unwrap().app.as_deref(), Some("pi"));
        records.foreground(Some(7));
        assert!(!records.progress(Progress::Set(50), Some(7)), "still the same program");
        records.foreground(Some(9));
        assert!(records.progress(Progress::Set(50), Some(9)), "the next program may not speak OSC 7501");
        records.apply(parse("state=working").unwrap(), Some(9));
        assert!(records.prompt());
        assert!(records.notified(None, None), "the shell's prompt: that program is gone too");
    }
    #[test]
    fn a_program_that_is_gone_leaves_only_its_results() {
        let mut records = Records::default();
        let apply = |records: &mut Records, body: &str, owner| records.apply(parse(body).unwrap(), owner);
        apply(&mut records, "state=idle:app=claude", Some(5));
        apply(&mut records, "state=working:id=w", Some(5));
        apply(&mut records, "state=done:id=d", Some(5));
        apply(&mut records, "state=working:id=other", Some(6));
        assert_eq!(records.owners(), [5, 6], "idle counts: it says the program is there");
        assert!(records.exited(5));
        let ids: Vec<String> = records.list().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["d", "other"]);
        assert!(records.ended());
        assert_eq!(records.list().len(), 1, "the result stays for the user");
    }
    #[test]
    fn a_seen_result_of_a_program_still_there_reads_idle() {
        let mut records = Records::default();
        records.apply(parse("state=done:app=pi:msg=aGk=").unwrap(), Some(9));
        records.apply(parse("state=error:id=job").unwrap(), Some(4));
        assert!(records.seen(Some(9)));
        let list = records.list();
        assert_eq!(list.len(), 1, "the gone program's result went");
        assert_eq!((list[0].state, list[0].app.as_deref(), list[0].msg.as_deref()), (State::Idle, Some("pi"), None));
        assert!(!records.seen(Some(9)), "nothing left to see");
    }
    #[test]
    fn one_order_for_every_rollup() {
        assert_eq!(most_urgent(["idle", "working", "done"]), "done");
        assert_eq!(most_urgent(["working", "error", "done"]), "error");
        assert_eq!(most_urgent(["error", "blocked"]), "blocked");
        assert_eq!(most_urgent(["", "unknown"]), "");
        assert!(State::ALL.windows(2).all(|w| w[0].urgency() < w[1].urgency()));
        assert!(State::ALL.iter().all(|s| State::parse(s.word()) == Some(*s)));
        let restored =
            Records::restore(vec![report("state=blocked:kind=question"), report("state=done:id=a")], Some(3));
        assert_eq!(restored.most_demanding().unwrap().state, State::Blocked);
        assert_eq!(restored.owners(), [3]);
    }
    #[test]
    fn the_least_recently_updated_record_makes_room() {
        let mut records = Records::default();
        for i in 0..MAX_RECORDS {
            records.apply(parse(&format!("state=working:id=r{i}")).unwrap(), None);
        }
        records.apply(parse("state=done:id=r0").unwrap(), None);
        records.apply(parse("state=working:id=new").unwrap(), None);
        let ids: Vec<String> = records.list().into_iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), MAX_RECORDS);
        assert!(ids.contains(&"r0".into()) && ids.contains(&"new".into()) && !ids.contains(&"r1".into()));
    }
}
