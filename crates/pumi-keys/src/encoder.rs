use crate::names::{Chord, MOD_ALT, MOD_CTRL};
use libghostty_vt::Terminal;
use libghostty_vt::key::{self, Key, KittyKeyFlags, Mods};
use libghostty_vt::terminal::{Mode, ModeKind};
pub mod kitty {
    pub const DISAMBIGUATE: u8 = 1;
    pub const EVENT_TYPES: u8 = 2;
    pub const ALTERNATE_KEYS: u8 = 4;
    pub const ALL_KEYS: u8 = 8;
    pub const ASSOCIATED_TEXT: u8 = 16;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Action {
    #[default]
    Press,
    Repeat,
    Release,
}
impl Action {
    fn raw(self) -> key::Action {
        match self {
            Action::Press => key::Action::Press,
            Action::Repeat => key::Action::Repeat,
            Action::Release => key::Action::Release,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptionAsAlt {
    #[default]
    Neither,
    Both,
    Left,
    Right,
}
impl OptionAsAlt {
    fn raw(self) -> key::OptionAsAlt {
        match self {
            OptionAsAlt::Neither => key::OptionAsAlt::False,
            OptionAsAlt::Both => key::OptionAsAlt::True,
            OptionAsAlt::Left => key::OptionAsAlt::Left,
            OptionAsAlt::Right => key::OptionAsAlt::Right,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    pub cursor_keys_application: bool,
    pub keypad_application: bool,
    pub ignore_keypad_with_numlock: bool,
    pub alt_esc_prefix: bool,
    pub modify_other_keys_2: bool,
    pub kitty_flags: u8,
    pub backarrow_sends_bs: bool,
    pub option_as_alt: OptionAsAlt,
}
type Setter = fn(&mut Options, bool);
const MODES: &[(u16, Setter)] = &[
    (1, |o, v| o.cursor_keys_application = v),
    (66, |o, v| o.keypad_application = v),
    (1035, |o, v| o.ignore_keypad_with_numlock = v),
    (1036, |o, v| o.alt_esc_prefix = v),
    (67, |o, v| o.backarrow_sends_bs = v),
];
impl Options {
    pub fn from_terminal(terminal: &Terminal<'_, '_>) -> Self {
        let mut options = Options::default();
        for (mode, set) in MODES {
            set(&mut options, terminal.mode(Mode::new(*mode, ModeKind::Dec)).unwrap_or(false));
        }
        options.modify_other_keys_2 = modify_other_keys_2(terminal);
        options.kitty_flags = terminal.kitty_keyboard_flags().map(|flags| flags.bits()).unwrap_or(0);
        options
    }
    pub fn extended(&self) -> bool {
        self.kitty_flags != 0 || self.modify_other_keys_2
    }
}
fn modify_other_keys_2(terminal: &Terminal<'_, '_>) -> bool {
    let probe = |force_off: bool| -> Option<Vec<u8>> {
        let mut encoder = key::Encoder::new().ok()?;
        encoder.set_options_from_terminal(terminal).set_kitty_flags(KittyKeyFlags::empty());
        if force_off {
            encoder.set_modify_other_keys_state_2(false);
        }
        let mut event = key::Event::new().ok()?;
        event
            .set_action(key::Action::Press)
            .set_key(Key::Comma)
            .set_mods(Mods::from_bits_retain(MOD_CTRL))
            .set_utf8(Some(","))
            .set_unshifted_codepoint(',');
        let mut out = Vec::new();
        encoder.encode_to_vec(&event, &mut out).ok()?;
        Some(out)
    };
    matches!((probe(false), probe(true)), (Some(set), Some(off)) if set != off)
}
pub struct Encoder {
    encoder: key::Encoder<'static>,
    event: key::Event<'static>,
}
unsafe impl Send for Encoder {}
#[derive(Debug)]
pub struct Error(libghostty_vt::Error);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ghostty key encoder error: {:?}", self.0)
    }
}
impl std::error::Error for Error {}
impl Encoder {
    pub fn new() -> Result<Self, Error> {
        Ok(Self { encoder: key::Encoder::new().map_err(Error)?, event: key::Event::new().map_err(Error)? })
    }
    fn configure(&mut self, options: &Options) {
        self.encoder
            .set_cursor_key_application(options.cursor_keys_application)
            .set_keypad_key_application(options.keypad_application)
            .set_ignore_keypad_with_numlock(options.ignore_keypad_with_numlock)
            .set_alt_esc_prefix(options.alt_esc_prefix)
            .set_modify_other_keys_state_2(options.modify_other_keys_2)
            .set_backarrow_key_mode(options.backarrow_sends_bs)
            .set_kitty_flags(KittyKeyFlags::from_bits_retain(options.kitty_flags))
            .set_macos_option_as_alt(options.option_as_alt.raw());
    }
    pub fn encode(&mut self, chord: &Chord, options: &Options) -> Vec<u8> {
        self.encode_action(chord, options, Action::Press)
    }
    pub fn encode_action(&mut self, chord: &Chord, options: &Options, action: Action) -> Vec<u8> {
        if chord.key == Key::Enter && chord.mods != 0 && !options.extended() {
            if action == Action::Release {
                return Vec::new();
            }
            let alt = chord.mods & MOD_ALT != 0 && options.option_as_alt != OptionAsAlt::Neither;
            return if alt { b"\x1b\r".to_vec() } else { b"\r".to_vec() };
        }
        if chord.key == Key::Unidentified {
            return if action == Action::Release {
                Vec::new()
            } else {
                chord.text.clone().unwrap_or_default().into_bytes()
            };
        }
        self.configure(options);
        let text = chord.text.as_deref().filter(|text| !text.is_empty());
        self.event
            .set_action(action.raw())
            .set_key(chord.key)
            .set_mods(Mods::from_bits_retain(chord.mods))
            .set_consumed_mods(Mods::empty())
            .set_composing(false)
            .set_utf8(text)
            .set_unshifted_codepoint(chord.unshifted.unwrap_or('\0'));
        let mut out = Vec::new();
        match self.encoder.encode_to_vec(&self.event, &mut out) {
            Ok(()) => out,
            Err(_) => Vec::new(),
        }
    }
}
#[cfg(test)]
mod tests {
    use crate::encode_names;
    use libghostty_vt::Terminal;
    fn bytes(names: &[&str], program: &[u8]) -> Vec<u8> {
        let mut terminal = Terminal::new(80, 24).unwrap();
        terminal.vt_write(program);
        encode_names(&names.iter().map(|n| n.to_string()).collect::<Vec<_>>(), &terminal).unwrap()
    }
    #[test]
    fn legacy_names_and_aliases() {
        assert_eq!(bytes(&["ctrl+c"], b""), [0x03]);
        assert_eq!(bytes(&["C-c"], b""), [0x03]);
        assert_eq!(bytes(&["Enter"], b""), b"\r");
        assert_eq!(bytes(&["esc"], b""), b"\x1b");
        assert_eq!(bytes(&["shift+tab"], b""), b"\x1b[Z");
        assert_eq!(bytes(&["backtab"], b""), b"\x1b[Z");
        assert_eq!(bytes(&["alt+b"], b""), b"\x1bb");
        assert_eq!(bytes(&["M-x"], b""), b"\x1bx");
        assert_eq!(bytes(&["a", "A", "plus", "+", "space"], b""), b"aA++ ");
        assert_eq!(bytes(&["up"], b""), b"\x1b[A");
        assert_eq!(bytes(&["F5"], b""), b"\x1b[15~");
        assert_eq!(bytes(&["backspace"], b""), [0x7f]);
        assert_eq!(bytes(&["shift+enter"], b""), b"\r", "a plain shell gets plain Enter");
    }
    #[test]
    fn the_programs_keyboard_mode_is_followed() {
        assert_eq!(bytes(&["up"], b"\x1b[?1h"), b"\x1bOA", "DECCKM");
        assert_eq!(bytes(&["shift+enter"], b"\x1b[>1u"), b"\x1b[13;2u", "kitty");
        assert_eq!(bytes(&["ctrl+c"], b"\x1b[>1u"), b"\x1b[99;5u");
        assert_eq!(bytes(&["alt+b"], b"\x1b[>1u"), b"\x1b[98;3u");
    }
    #[test]
    fn kitty_event_types_and_all_keys() {
        use super::{Action, Encoder, Options};
        use crate::names::{MOD_SHIFT, modifier_key, parse};
        let mut terminal = Terminal::new(80, 24).unwrap();
        let mut encoder = Encoder::new().unwrap();
        let mut encode = |terminal: &Terminal, chord: &crate::Chord, action| {
            encoder.encode_action(chord, &Options::from_terminal(terminal), action)
        };
        let a = parse("a").unwrap();
        let shift = modifier_key(MOD_SHIFT, MOD_SHIFT).unwrap();
        assert_eq!(encode(&terminal, &a, Action::Release), b"", "legacy: no releases");
        assert_eq!(encode(&terminal, &shift, Action::Press), b"", "legacy: no lone modifiers");
        terminal.vt_write(b"\x1b[>11u");
        assert_eq!(encode(&terminal, &a, Action::Press), b"\x1b[97u");
        assert_eq!(encode(&terminal, &a, Action::Repeat), b"\x1b[97;1:2u");
        assert_eq!(encode(&terminal, &a, Action::Release), b"\x1b[97;1:3u");
        assert_eq!(encode(&terminal, &shift, Action::Press), b"\x1b[57441;2u");
        let up = modifier_key(MOD_SHIFT, 0).unwrap();
        assert_eq!(encode(&terminal, &up, Action::Release), b"\x1b[57441;1:3u");
    }
    #[test]
    fn unknown_names_are_refused() {
        let terminal = Terminal::new(80, 24).unwrap();
        assert_eq!(encode_names(&["nosuchkey".into()], &terminal), Err("nosuchkey".into()));
    }
    #[test]
    fn modify_other_keys_is_read_from_the_terminal() {
        let mut terminal = Terminal::new(80, 24).unwrap();
        assert!(!super::Options::from_terminal(&terminal).modify_other_keys_2);
        terminal.vt_write(b"\x1b[>4;2m");
        assert!(super::Options::from_terminal(&terminal).modify_other_keys_2);
        terminal.vt_write(b"\x1b[>4;0m");
        assert!(!super::Options::from_terminal(&terminal).modify_other_keys_2);
    }
}
