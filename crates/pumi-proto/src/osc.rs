use std::collections::HashMap;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Osc {
    Title(String),
    Notify { title: Option<String>, body: String },
    Pointer(String),
    Status(crate::status::Sequence),
    Progress(crate::status::Progress),
    Prompt,
    Reset,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Title,
    Notify9,
    Notify777,
    Notify99,
    Pointer,
    Status,
    Semantic,
}
const HANDLERS: &[(&str, Kind)] = &[
    ("0", Kind::Title),
    ("2", Kind::Title),
    ("9", Kind::Notify9),
    ("777", Kind::Notify777),
    ("99", Kind::Notify99),
    ("22", Kind::Pointer),
    (crate::status::OSC, Kind::Status),
    ("133", Kind::Semantic),
];
#[derive(Debug, Default)]
struct Pending {
    title: String,
    body: String,
}
const MAX_PAYLOAD: usize = crate::status::MAX_SEQUENCE;
/// Where the scanner is in a sequence. Only 7-bit controls start and end one: the output is
/// UTF-8, whose continuation bytes are the 8-bit C1 controls (`✳` is `e2 9c b3`, and `9c` is ST).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Ground,
    Escape,
    Payload,
    PayloadEscape,
}
#[derive(Debug, Default)]
pub struct Scanner {
    mode: Mode,
    payload: Vec<u8>,
    overflow: bool,
    pending: HashMap<String, Pending>,
}
impl Scanner {
    pub fn feed(&mut self, bytes: &[u8], out: &mut Vec<Osc>) {
        for &byte in bytes {
            match (self.mode, byte) {
                (Mode::Ground, 0x1b) => self.mode = Mode::Escape,
                (Mode::Ground, _) => {}
                (Mode::Escape, b']') => self.begin(),
                (Mode::Escape, b'c') => {
                    self.mode = Mode::Ground;
                    out.push(Osc::Reset);
                }
                (Mode::Escape, 0x1b) => {}
                (Mode::Escape, _) => self.mode = Mode::Ground,
                (Mode::Payload, 0x07) => self.end(out),
                (Mode::Payload, 0x1b) => self.mode = Mode::PayloadEscape,
                (Mode::Payload, byte) => self.push(byte),
                (Mode::PayloadEscape, b'\\') => self.end(out),
                (Mode::PayloadEscape, b']') => self.begin(),
                (Mode::PayloadEscape, _) => {
                    self.mode = Mode::Ground;
                    self.payload.clear();
                }
            }
        }
    }
    fn begin(&mut self) {
        self.mode = Mode::Payload;
        self.payload.clear();
        self.overflow = false;
    }
    fn push(&mut self, byte: u8) {
        if self.payload.len() < MAX_PAYLOAD {
            self.payload.push(byte);
        } else {
            self.overflow = true;
        }
    }
    fn end(&mut self, out: &mut Vec<Osc>) {
        self.mode = Mode::Ground;
        if self.overflow {
            return;
        }
        let payload = String::from_utf8_lossy(&self.payload).into_owned();
        let (number, rest) = payload.split_once(';').unwrap_or((payload.as_str(), ""));
        let Some((_, kind)) = HANDLERS.iter().find(|(n, _)| *n == number) else { return };
        let event = match kind {
            Kind::Title => Some(Osc::Title(clean(rest))),
            Kind::Notify9 if rest == "4" || rest.starts_with("4;") => {
                crate::status::parse_progress(&rest[1..]).map(Osc::Progress)
            }
            Kind::Notify9 if rest.trim().is_empty() => None,
            Kind::Notify9 => Some(Osc::Notify { title: None, body: clean(rest) }),
            Kind::Notify99 => self.kitty_notification(rest),
            Kind::Pointer => Some(Osc::Pointer(clean(rest).trim().to_string())),
            Kind::Status => crate::status::parse(rest).map(Osc::Status),
            Kind::Semantic => (rest == "A" || rest.starts_with("A;")).then_some(Osc::Prompt),
            Kind::Notify777 => {
                let mut parts = rest.splitn(3, ';');
                match (parts.next(), parts.next(), parts.next()) {
                    (Some("notify"), title, body) => Some(Osc::Notify {
                        title: title.map(clean).filter(|t| !t.is_empty()),
                        body: body.map(clean).unwrap_or_default(),
                    }),
                    _ => None,
                }
            }
        };
        out.extend(event);
    }
}
impl Scanner {
    fn kitty_notification(&mut self, rest: &str) -> Option<Osc> {
        let (metadata, payload) = rest.split_once(';').unwrap_or((rest, ""));
        let meta: HashMap<&str, &str> = metadata.split(':').filter_map(|kv| kv.split_once('=')).collect();
        let id = meta.get("i").copied().unwrap_or("").to_string();
        let part = meta.get("p").copied().unwrap_or("title");
        let text = if meta.get("e") == Some(&"1") {
            String::from_utf8_lossy(&base64(payload)?).into_owned()
        } else {
            payload.to_string()
        };
        let pending = self.pending.entry(id.clone()).or_default();
        match part {
            "title" => pending.title.push_str(&clean(&text)),
            "body" => pending.body.push_str(&clean(&text)),
            _ => {}
        }
        if meta.get("d") == Some(&"0") {
            return None;
        }
        let done = self.pending.remove(&id)?;
        if !matches!(part, "title" | "body") && done.title.is_empty() && done.body.is_empty() {
            return None;
        }
        Some(match (done.title.is_empty(), done.body.is_empty()) {
            (false, false) => Osc::Notify { title: Some(done.title), body: done.body },
            (false, true) => Osc::Notify { title: None, body: done.title },
            _ => Osc::Notify { title: None, body: done.body },
        })
    }
}
fn base64(text: &str) -> Option<Vec<u8>> {
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
    let digits: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(digits.len() * 3 / 4);
    for chunk in digits.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= value(*c)? << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}
fn clean(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scan(chunks: &[&[u8]]) -> Vec<Osc> {
        let mut scanner = Scanner::default();
        let mut out = Vec::new();
        for chunk in chunks {
            scanner.feed(chunk, &mut out);
        }
        out
    }
    #[test]
    fn a_title_s_utf8_is_not_read_as_c1_controls() {
        let bytes = "\x1b]0;\u{2733} Claude Code\x07\u{271d}\x1b]2;after\x07";
        assert_eq!(
            scan(&[bytes.as_bytes()]),
            vec![Osc::Title("\u{2733} Claude Code".into()), Osc::Title("after".into())]
        );
    }
    #[test]
    fn titles_and_notifications_survive_split_reads() {
        assert_eq!(scan(&[b"x\x1b]0;hel", b"lo\x07y"]), vec![Osc::Title("hello".into())]);
        assert_eq!(scan(&[b"\x1b]2;a\x1b", b"\\"]), vec![Osc::Title("a".into())]);
        assert_eq!(
            scan(&[b"\x1b]777;notify;Done;All tests pass\x07"]),
            vec![Osc::Notify { title: Some("Done".into()), body: "All tests pass".into() }]
        );
        assert_eq!(scan(&[b"\x1b]9;ping\x07"]), vec![Osc::Notify { title: None, body: "ping".into() }]);
        assert_eq!(
            scan(&[b"\x1b]9;4;1;50\x07", b"\x1b]52;c;Zm9v\x07", b"\x1b]7;file:///tmp\x07", b"\x1b]9;4\x07"]),
            vec![Osc::Progress(crate::status::Progress::Set(50)), Osc::Progress(crate::status::Progress::Remove)]
        );
    }
    #[test]
    fn kitty_notifications_come_in_chunks_and_base64() {
        assert_eq!(
            scan(&[b"\x1b]99;i=1:d=0;Build\x1b\\", b"\x1b]99;i=1:p=body;passed\x1b\\"]),
            vec![Osc::Notify { title: Some("Build".into()), body: "passed".into() }]
        );
        assert_eq!(scan(&[b"\x1b]99;e=1;aGVsbG8=\x07"]), vec![Osc::Notify { title: None, body: "hello".into() }]);
        assert_eq!(scan(&[b"\x1b]99;i=2:p=?;\x07"]), vec![], "a query is not a notification");
    }
    #[test]
    fn program_status_prompts_and_resets() {
        let out = scan(&[b"\x1b]7501;state=work", b"ing:app=cargo\x1b\\\x1b]133;A\x07\x1bc\x1b]7501;?\x07"]);
        assert!(
            matches!(&out[0], Osc::Status(crate::status::Sequence::Report(r)) if r.app.as_deref() == Some("cargo"))
        );
        assert_eq!(out[1..], [Osc::Prompt, Osc::Reset, Osc::Status(crate::status::Sequence::Query)]);
        assert!(scan(&[b"\x1b]133;B\x07\x1b]7501;state=nope\x07"]).is_empty());
    }
    #[test]
    fn pointer_shapes() {
        assert_eq!(
            scan(&[b"\x1b]22;pointer\x07", b"\x1b]22;\x07"]),
            vec![Osc::Pointer("pointer".into()), Osc::Pointer("".into())]
        );
    }
    #[test]
    fn a_cut_off_sequence_does_not_swallow_the_next() {
        assert_eq!(scan(&[b"\x1b]0;partial\x1b[1m\x1b]0;whole\x07"]), vec![Osc::Title("whole".into())]);
    }
}
