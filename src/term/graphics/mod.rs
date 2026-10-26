pub mod iterm;
pub mod sixel;
use pumi_proto::native::Size;
use std::borrow::Cow;
const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;
const MAX_PAYLOAD: usize = 96 << 20;
const MAX_HELD: usize = 64;
const DEFAULT_CELL: (u32, u32) = (8, 16);
const MAX_DEPTH: u8 = 2;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Esc,
    Csi,
    OscPrefix,
    Osc,
    OscEsc,
    DcsPrefix,
    Sixel,
    SixelEsc,
    Tmux,
    TmuxEsc,
}
struct Multipart {
    args: iterm::Args,
    base64: Vec<u8>,
}
pub struct Translated<'a> {
    pub bytes: Cow<'a, [u8]>,
    pub replies: Vec<u8>,
}
pub struct Translator {
    state: State,
    held: Vec<u8>,
    body: Vec<u8>,
    overflow: bool,
    sixel: Option<sixel::Decoder>,
    multipart: Option<Multipart>,
    sixel_cursor_right: bool,
}
impl Default for Translator {
    fn default() -> Self {
        Self {
            state: State::Ground,
            held: Vec::new(),
            body: Vec::new(),
            overflow: false,
            sixel: None,
            multipart: None,
            sixel_cursor_right: false,
        }
    }
}
impl Translator {
    pub fn feed<'a>(&mut self, bytes: &'a [u8], size: Size) -> Translated<'a> {
        if self.state == State::Ground && !bytes.contains(&ESC) {
            return Translated { bytes: Cow::Borrowed(bytes), replies: Vec::new() };
        }
        let mut out = Vec::with_capacity(bytes.len());
        let mut replies = Vec::new();
        let screen = Screen::of(size);
        self.scan(bytes, screen, &mut out, &mut replies, 0);
        Translated { bytes: Cow::Owned(out), replies }
    }
    fn scan(&mut self, mut bytes: &[u8], screen: Screen, out: &mut Vec<u8>, replies: &mut Vec<u8>, depth: u8) {
        while let Some(&byte) = bytes.first() {
            match self.state {
                State::Ground => match bytes.iter().position(|&b| b == ESC) {
                    None => {
                        out.extend_from_slice(bytes);
                        return;
                    }
                    Some(at) => {
                        out.extend_from_slice(&bytes[..at]);
                        self.held.push(ESC);
                        self.state = State::Esc;
                        bytes = &bytes[at + 1..];
                    }
                },
                State::Osc | State::Sixel | State::Tmux => {
                    let end = bytes.iter().position(|&b| matches!(b, ESC | BEL | CAN | SUB));
                    let (data, rest) = bytes.split_at(end.unwrap_or(bytes.len()));
                    match self.state {
                        State::Sixel => {
                            if let Some(decoder) = &mut self.sixel {
                                decoder.feed(data);
                            }
                        }
                        _ => self.collect(data),
                    }
                    let Some(&end) = rest.first() else { return };
                    bytes = &rest[1..];
                    match (self.state, end) {
                        (State::Osc, BEL) => self.finish_osc(false, screen, out, replies),
                        (State::Osc, ESC) => self.state = State::OscEsc,
                        (State::Sixel, ESC) => self.state = State::SixelEsc,
                        (State::Tmux, ESC) => self.state = State::TmuxEsc,
                        (State::Sixel, BEL) => {}
                        (State::Tmux, BEL) => self.collect(&[BEL]),
                        _ => self.abort(),
                    }
                }
                _ => {
                    if self.step(byte, screen, out, replies, depth) {
                        bytes = &bytes[1..];
                    }
                }
            }
        }
    }
    fn step(&mut self, byte: u8, screen: Screen, out: &mut Vec<u8>, replies: &mut Vec<u8>, depth: u8) -> bool {
        match self.state {
            State::Esc => match byte {
                b']' | b'P' | b'[' => {
                    self.held.push(byte);
                    self.state = match byte {
                        b']' => State::OscPrefix,
                        b'P' => State::DcsPrefix,
                        _ => State::Csi,
                    };
                    true
                }
                _ => self.pass(out),
            },
            State::Csi => {
                if self.held.len() == 2 {
                    return if byte == b'?' {
                        self.held.push(byte);
                        true
                    } else {
                        self.pass(out)
                    };
                }
                match byte {
                    0x20..=0x3f if self.held.len() < MAX_HELD => {
                        self.held.push(byte);
                        true
                    }
                    0x40..=0x7e => {
                        self.held.push(byte);
                        self.finish_csi(screen, out, replies);
                        true
                    }
                    _ => self.pass(out),
                }
            }
            State::OscPrefix => {
                const KEY: &[u8] = b"1337;";
                let at = self.held.len() - 2;
                if KEY[at] != byte {
                    return self.pass(out);
                }
                self.held.push(byte);
                if at + 1 == KEY.len() {
                    self.held.clear();
                    self.begin_body(State::Osc);
                }
                true
            }
            State::DcsPrefix => {
                const TMUX: &[u8] = b"tmux;";
                let so_far = &self.held[2..];
                let numeric = so_far.iter().all(|b| b.is_ascii_digit() || *b == b';');
                if TMUX.get(so_far.len()) == Some(&byte) && TMUX.starts_with(so_far) {
                    self.held.push(byte);
                    if self.held.len() - 2 == TMUX.len() {
                        self.held.clear();
                        self.begin_body(State::Tmux);
                    }
                    true
                } else if numeric && (byte.is_ascii_digit() || byte == b';') && self.held.len() < MAX_HELD {
                    self.held.push(byte);
                    true
                } else if numeric && byte == b'q' {
                    self.held.clear();
                    self.sixel = Some(sixel::Decoder::default());
                    self.state = State::Sixel;
                    true
                } else {
                    self.pass(out)
                }
            }
            State::OscEsc => {
                self.finish_osc(true, screen, out, replies);
                self.after_esc(byte)
            }
            State::SixelEsc => {
                self.finish_sixel(screen, out);
                self.after_esc(byte)
            }
            State::TmuxEsc => match byte {
                ESC => {
                    self.collect(&[ESC]);
                    self.state = State::Tmux;
                    true
                }
                _ => {
                    self.state = State::Ground;
                    let inner = std::mem::take(&mut self.body);
                    if self.overflow {
                        self.overflow = false;
                    } else if depth < MAX_DEPTH {
                        self.scan(&inner, screen, out, replies, depth + 1);
                        if self.state != State::Ground {
                            out.append(&mut self.held);
                            self.abort();
                        }
                    }
                    self.reuse(inner);
                    self.after_esc(byte)
                }
            },
            State::Ground | State::Osc | State::Sixel | State::Tmux => unreachable!("bulk states scan"),
        }
    }
    fn after_esc(&mut self, byte: u8) -> bool {
        if byte == b'\\' {
            return true;
        }
        self.held.push(ESC);
        self.state = State::Esc;
        false
    }
    fn pass(&mut self, out: &mut Vec<u8>) -> bool {
        out.append(&mut self.held);
        self.state = State::Ground;
        false
    }
    fn begin_body(&mut self, state: State) {
        self.body.clear();
        self.overflow = false;
        self.state = state;
    }
    fn collect(&mut self, data: &[u8]) {
        if self.body.len() + data.len() > MAX_PAYLOAD {
            self.overflow = true;
        } else if !self.overflow {
            self.body.extend_from_slice(data);
        }
    }
    fn abort(&mut self) {
        self.sixel = None;
        self.body.clear();
        self.overflow = false;
        self.state = State::Ground;
    }
    fn reuse(&mut self, mut body: Vec<u8>) {
        if body.capacity() <= 1 << 20 {
            body.clear();
            self.body = body;
        }
    }
    fn finish_csi(&mut self, screen: Screen, out: &mut Vec<u8>, replies: &mut Vec<u8>) {
        self.state = State::Ground;
        let held = std::mem::take(&mut self.held);
        let (params, end) = (&held[3..held.len() - 1], held[held.len() - 1]);
        if end == b'S' && params.iter().all(|b| b.is_ascii_digit() || *b == b';') {
            replies.extend(xtsmgraphics(params, screen).into_bytes());
        } else {
            if matches!(end, b'h' | b'l') && params.split(|b| *b == b';').any(|p| p == b"8452") {
                self.sixel_cursor_right = end == b'h';
            }
            out.extend_from_slice(&held);
        }
        self.held = held;
        self.held.clear();
    }
    fn finish_osc(&mut self, st: bool, screen: Screen, out: &mut Vec<u8>, replies: &mut Vec<u8>) {
        self.state = State::Ground;
        let body = std::mem::take(&mut self.body);
        if std::mem::take(&mut self.overflow) {
            self.multipart = None;
            return self.reuse(body);
        }
        let (key, value) = match body.iter().position(|b| *b == b'=') {
            Some(at) => (&body[..at], &body[at + 1..]),
            None => (&body[..], &[][..]),
        };
        match key {
            b"File" => {
                let at = value.iter().position(|b| *b == b':').unwrap_or(value.len());
                let args = iterm::Args::parse(&value[..at]);
                let file = base64_decode(value.get(at + 1..).unwrap_or_default());
                place_file(&args, &file, screen, out);
            }
            b"MultipartFile" => {
                self.multipart = Some(Multipart { args: iterm::Args::parse(value), base64: Vec::new() });
            }
            b"FilePart" => {
                if let Some(part) = &mut self.multipart {
                    if part.base64.len() + value.len() > MAX_PAYLOAD {
                        self.multipart = None;
                    } else {
                        part.base64.extend_from_slice(value);
                    }
                }
            }
            b"FileEnd" => {
                if let Some(part) = self.multipart.take() {
                    place_file(&part.args, &base64_decode(&part.base64), screen, out);
                }
            }
            b"ReportCellSize" => {
                let (w, h) = screen.cell;
                replies.extend(format!("\x1b]1337;ReportCellSize={h}.0;{w}.0;1.0\x1b\\").into_bytes());
            }
            _ => {
                out.extend_from_slice(b"\x1b]1337;");
                out.extend_from_slice(&body);
                out.extend_from_slice(if st { b"\x1b\\" } else { b"\x07" });
            }
        }
        self.reuse(body);
    }
    fn finish_sixel(&mut self, screen: Screen, out: &mut Vec<u8>) {
        self.state = State::Ground;
        let Some((width, height, rgba)) = self.sixel.take().and_then(sixel::Decoder::finish) else { return };
        let Some(png) = encode_png(width, height, &rgba) else { return };
        if self.sixel_cursor_right {
            kitty(out, "a=T,f=100,q=2,C=0", &png);
        } else {
            kitty(out, "a=T,f=100,q=2,C=1", &png);
            let rows = height.div_ceil(screen.cell.1) as usize;
            out.extend(std::iter::repeat_n(b'\n', rows));
            out.push(b'\r');
        }
    }
}
#[derive(Debug, Clone, Copy)]
struct Screen {
    cols: u16,
    rows: u16,
    cell: (u32, u32),
}
impl Screen {
    fn of(size: Size) -> Self {
        let cell = if size.cell_width > 0 && size.cell_height > 0 {
            (size.cell_width, size.cell_height)
        } else {
            DEFAULT_CELL
        };
        Self { cols: size.cols, rows: size.rows, cell }
    }
    fn pixels(self) -> (u32, u32) {
        (u32::from(self.cols) * self.cell.0, u32::from(self.rows) * self.cell.1)
    }
}
fn place_file(args: &iterm::Args, file: &[u8], screen: Screen, out: &mut Vec<u8>) {
    if !args.inline {
        return;
    }
    let Some((png, width, height)) = iterm::png(file) else { return };
    let fit = iterm::fit(args, (width, height), screen.cell, (screen.cols, screen.rows));
    let mut control = format!("a=T,f=100,q=2,C={}", u8::from(args.stay));
    if let Some(cols) = fit.cols {
        control.push_str(&format!(",c={cols}"));
    }
    if let Some(rows) = fit.rows {
        control.push_str(&format!(",r={rows}"));
    }
    kitty(out, &control, &png);
}
fn kitty(out: &mut Vec<u8>, control: &str, data: &[u8]) {
    transmit(out, control, data);
}
fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder as _;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut png = Vec::new();
    PngEncoder::new_with_quality(&mut png, CompressionType::Fast, FilterType::Adaptive)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .ok()?;
    Some(png)
}
fn xtsmgraphics(params: &[u8], screen: Screen) -> String {
    let mut numbers = params.split(|b| *b == b';').map(|p| std::str::from_utf8(p).ok()?.parse::<u32>().ok());
    let item = numbers.next().flatten().unwrap_or(0);
    let action = numbers.next().flatten().unwrap_or(0);
    let (w, h) = screen.pixels();
    let max = sixel::MAX_SIDE;
    match (item, action) {
        (1, 1..=4) => format!("\x1b[?1;0;{}S", sixel::REGISTERS),
        (2, 1..=3) => format!("\x1b[?2;0;{w};{h}S"),
        (2, 4) => format!("\x1b[?2;0;{max};{max}S"),
        (1, _) => "\x1b[?1;2;0S".into(),
        (2, _) => "\x1b[?2;2;0;0S".into(),
        _ => format!("\x1b[?{item};1;0S"),
    }
}
fn base64_decode(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for &c in text {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        acc = acc << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}
#[cfg(test)]
mod tests;
const CHUNK: usize = 4096;
fn transmit(out: &mut Vec<u8>, control: &str, data: &[u8]) {
    let encoded = base64(data);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        out.extend_from_slice(b"\x1b_G");
        if first {
            out.extend(format!("{control},m={more}").into_bytes());
            first = false;
        } else {
            out.extend(format!("m={more}").into_bytes());
        }
        out.push(b';');
        out.extend_from_slice(chunk);
        out.extend_from_slice(b"\x1b\\");
    }
}
fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
