const MODES: &[(u16, bool)] = &[
    (1, false),
    (7, true),
    (25, true),
    (1000, false),
    (1002, false),
    (1003, false),
    (1004, false),
    (1005, false),
    (1006, false),
    (1015, false),
    (2004, false),
];
const ALT: &[u16] = &[47, 1047, 1049];
pub const FORWARDED: &[u32] = &[9, 99, 133, 777, 7501];
const OSC_LIMIT: usize = 64 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Modes {
    dec: Vec<(u16, bool)>,
    pub alt: bool,
    pub keypad: bool,
    pub kitty: Vec<u32>,
    pub modify_other_keys: u32,
}
impl Default for Modes {
    fn default() -> Self {
        Modes { dec: MODES.to_vec(), alt: false, keypad: false, kitty: Vec::new(), modify_other_keys: 0 }
    }
}
impl Modes {
    pub fn get(&self, mode: u16) -> bool {
        self.dec.iter().find(|(m, _)| *m == mode).is_some_and(|(_, on)| *on)
    }
    fn set(&mut self, mode: u16, on: bool) {
        if ALT.contains(&mode) {
            self.alt = on;
        } else if let Some(slot) = self.dec.iter_mut().find(|(m, _)| *m == mode) {
            slot.1 = on;
        }
    }
    pub fn bracketed_paste(&self) -> bool {
        self.get(2004)
    }
    pub fn diff(&self, to: &Modes) -> Vec<u8> {
        let mut out = Vec::new();
        if self.alt != to.alt {
            out.extend_from_slice(if to.alt { b"\x1b[?1049h" } else { b"\x1b[?1049l" });
        }
        for ((mode, from), (_, on)) in self.dec.iter().zip(&to.dec) {
            if from != on {
                out.extend_from_slice(format!("\x1b[?{mode}{}", if *on { 'h' } else { 'l' }).as_bytes());
            }
        }
        if self.keypad != to.keypad {
            out.extend_from_slice(if to.keypad { b"\x1b=" } else { b"\x1b>" });
        }
        if self.kitty != to.kitty {
            if !self.kitty.is_empty() {
                out.extend_from_slice(format!("\x1b[<{}u", self.kitty.len()).as_bytes());
            }
            for flags in &to.kitty {
                out.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
            }
        }
        if self.modify_other_keys != to.modify_other_keys {
            out.extend_from_slice(format!("\x1b[>4;{}m", to.modify_other_keys).as_bytes());
        }
        out
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seen {
    Title(String),
    Cwd(String),
    Osc { code: u32, data: String },
    Query(Vec<u8>),
    Bell,
}
/// Where the tracker is in a sequence. Only 7-bit controls start and end one: the output is
/// UTF-8, whose continuation bytes are the 8-bit C1 controls (`✳` is `e2 9c b3`, and `9c` is ST).
#[derive(Clone, Debug, Default)]
enum State {
    #[default]
    Ground,
    Esc,
    EscArg,
    Csi,
    Osc,
    OscEsc,
    Str,
    StrEsc,
}
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    pub modes: Modes,
    state: State,
    buf: Vec<u8>,
}
impl Tracker {
    pub fn with(modes: Modes) -> Tracker {
        Tracker { modes, ..Tracker::default() }
    }
    pub fn feed(&mut self, bytes: &[u8], seen: &mut Vec<Seen>) {
        for &b in bytes {
            self.byte(b, seen);
        }
    }
    fn byte(&mut self, b: u8, seen: &mut Vec<Seen>) {
        match self.state {
            State::Ground => match b {
                0x1b => self.state = State::Esc,
                0x07 => seen.push(Seen::Bell),
                _ => {}
            },
            State::Esc => match b {
                b'[' => self.begin(State::Csi),
                b']' => self.begin(State::Osc),
                b'P' | b'X' | b'^' | b'_' => self.state = State::Str,
                b'=' => self.ground(|t| t.modes.keypad = true),
                b'>' => self.ground(|t| t.modes.keypad = false),
                b'c' => self.ground(|t| t.modes = Modes::default()),
                0x20..=0x2f => self.state = State::EscArg,
                0x1b => {}
                _ => self.state = State::Ground,
            },
            State::EscArg => self.state = State::Ground,
            State::Csi => match b {
                0x20..=0x3f if self.buf.len() < 256 => self.buf.push(b),
                0x40..=0x7e => {
                    let params = std::mem::take(&mut self.buf);
                    self.state = State::Ground;
                    self.csi(&params, b, seen);
                }
                0x1b => self.state = State::Esc,
                0x18 | 0x1a => self.state = State::Ground,
                _ => {}
            },
            State::Osc => match b {
                0x07 => self.osc_end(seen),
                0x1b => self.state = State::OscEsc,
                _ if self.buf.len() < OSC_LIMIT => self.buf.push(b),
                _ => {}
            },
            State::OscEsc => {
                if b == b'\\' {
                    self.osc_end(seen);
                } else {
                    self.buf.clear();
                    self.state = State::Esc;
                    self.byte(b, seen);
                }
            }
            State::Str => match b {
                0x1b => self.state = State::StrEsc,
                0x07 => self.state = State::Ground,
                _ => {}
            },
            State::StrEsc => self.state = if b == b'\\' { State::Ground } else { State::Str },
        }
    }
    fn begin(&mut self, state: State) {
        self.buf.clear();
        self.state = state;
    }
    fn ground(&mut self, f: impl FnOnce(&mut Tracker)) {
        f(self);
        self.state = State::Ground;
    }
    fn csi(&mut self, params: &[u8], fin: u8, seen: &mut Vec<Seen>) {
        let (marker, rest) = match params.first() {
            Some(m @ (b'?' | b'>' | b'<' | b'=')) => (Some(*m), &params[1..]),
            _ => (None, params),
        };
        let intermediate = rest.iter().rev().find(|b| (0x20..=0x2f).contains(*b)).copied();
        let text = std::str::from_utf8(rest).unwrap_or("");
        let nums: Vec<u32> =
            text.trim_end_matches(|c: char| !c.is_ascii_digit()).split(';').map(|n| n.parse().unwrap_or(0)).collect();
        let first = nums.first().copied().unwrap_or(0);
        match (marker, intermediate, fin) {
            (Some(b'?'), None, b'h' | b'l') => {
                for mode in &nums {
                    self.modes.set(*mode as u16, fin == b'h');
                }
            }
            (Some(b'>'), None, b'u') => {
                if self.modes.kitty.len() < 16 {
                    self.modes.kitty.push(first);
                }
            }
            (Some(b'<'), None, b'u') => {
                for _ in 0..first.max(1) {
                    self.modes.kitty.pop();
                }
            }
            (Some(b'='), None, b'u') => {
                let mode = nums.get(1).copied().unwrap_or(1);
                let top = self.modes.kitty.last().copied().unwrap_or(0);
                let flags = match mode {
                    2 => top | first,
                    3 => top & !first,
                    _ => first,
                };
                match self.modes.kitty.last_mut() {
                    Some(slot) => *slot = flags,
                    None => self.modes.kitty.push(flags),
                }
            }
            (Some(b'?'), None, b'u') => {
                let flags = self.modes.kitty.last().copied().unwrap_or(0);
                seen.push(Seen::Query(format!("\x1b[?{flags}u").into_bytes()));
            }
            (Some(b'>'), None, b'm') if first == 4 => self.modes.modify_other_keys = nums.get(1).copied().unwrap_or(0),
            (None, Some(b'!'), b'p') => {
                let (alt, kitty) = (self.modes.alt, self.modes.kitty.clone());
                self.modes = Modes { alt, kitty, ..Modes::default() };
            }
            (None, None, b'c') if first == 0 => seen.push(Seen::Query(b"\x1b[?62;22c".to_vec())),
            (Some(b'>'), None, b'c') if first == 0 => seen.push(Seen::Query(b"\x1b[>1;10;0c".to_vec())),
            (None, None, b'n') if first == 5 => seen.push(Seen::Query(b"\x1b[0n".to_vec())),
            (Some(b'>'), None, b'q') => seen.push(Seen::Query(b"\x1bP>|pumi\x1b\\".to_vec())),
            _ => {}
        }
    }
    fn osc_end(&mut self, seen: &mut Vec<Seen>) {
        self.state = State::Ground;
        let body = std::mem::take(&mut self.buf);
        let text = String::from_utf8_lossy(&body);
        let (code, data) = text.split_once(';').unwrap_or((&text, ""));
        let Ok(code) = code.parse::<u32>() else { return };
        match code {
            0 | 2 => seen.push(Seen::Title(data.to_string())),
            7 => seen.push(Seen::Cwd(data.to_string())),
            7501 if matches!(pumi_proto::status::parse(data), Some(pumi_proto::status::Sequence::Query)) => {
                seen.push(Seen::Query(pumi_proto::status::QUERY_REPLY.to_vec()))
            }
            _ if FORWARDED.contains(&code) => seen.push(Seen::Osc { code, data: data.to_string() }),
            _ => {}
        }
    }
}
pub fn pwd_path(raw: &str) -> Option<String> {
    let path = match raw.strip_prefix("file://") {
        Some(rest) => &rest[rest.find('/')?..],
        None if raw.starts_with('/') => raw,
        None => return None,
    };
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = path.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}
pub fn safe_start(tail: &[u8]) -> usize {
    tail.iter().take(64 * 1024).position(|b| *b == b'\n').map_or(0, |at| at + 1)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn feed(t: &mut Tracker, bytes: &[u8]) -> Vec<Seen> {
        let mut seen = Vec::new();
        for chunk in bytes.chunks(3) {
            t.feed(chunk, &mut seen);
        }
        seen
    }
    #[test]
    fn modes_across_chunks_and_their_diff() {
        let mut t = Tracker::default();
        feed(&mut t, b"hi\x1b[?1049h\x1b[?1;2004h\x1b=\x1b[>5u\x1b[?25l");
        assert!(t.modes.alt && t.modes.get(1) && t.modes.bracketed_paste() && t.modes.keypad);
        assert_eq!(t.modes.kitty, vec![5]);
        let diff = Modes::default().diff(&t.modes);
        let mut fresh = Tracker::default();
        feed(&mut fresh, &diff);
        assert_eq!(fresh.modes, t.modes, "the diff reproduces the modes");
        feed(&mut t, b"\x1bc");
        assert_eq!(t.modes, Modes::default());
    }
    #[test]
    fn a_title_s_utf8_is_not_read_as_c1_controls() {
        let mut t = Tracker::default();
        let seen = feed(&mut t, "\x1b]0;\u{2733} Claude Code\x07\u{271d}\x1b]2;after\x07".as_bytes());
        assert_eq!(seen, vec![Seen::Title("\u{2733} Claude Code".into()), Seen::Title("after".into())]);
    }
    #[test]
    fn title_cwd_forwarded_and_queries() {
        let mut t = Tracker::default();
        let seen =
            feed(&mut t, b"\x1b]2;build\x07\x1b]7;file://h/tmp/a%20b\x1b\\\x1b]9;done\x07\x1b[c\x1b]1337;File=x\x07");
        assert_eq!(seen[0], Seen::Title("build".into()));
        assert_eq!(seen[1], Seen::Cwd("file://h/tmp/a%20b".into()));
        assert_eq!(pwd_path("file://h/tmp/a%20b").as_deref(), Some("/tmp/a b"));
        assert_eq!(seen[2], Seen::Osc { code: 9, data: "done".into() });
        assert_eq!(seen[3], Seen::Query(b"\x1b[?62;22c".to_vec()));
        assert_eq!(seen.len(), 4, "OSC 1337 is not passed on");
    }
    #[test]
    fn the_program_status_probe_is_answered_before_device_attributes() {
        let mut t = Tracker::default();
        let seen = feed(&mut t, b"\x1b[?u\x1b]7501;?\x1b\\\x1b[c\x1b]7501;state=idle\x07");
        assert_eq!(seen[1], Seen::Query(pumi_proto::status::QUERY_REPLY.to_vec()));
        assert_eq!(seen[2], Seen::Query(b"\x1b[?62;22c".to_vec()));
        assert_eq!(seen[3], Seen::Osc { code: 7501, data: "state=idle".into() }, "reports go on to the host");
    }
}
