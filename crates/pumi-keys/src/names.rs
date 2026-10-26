use libghostty_vt::ffi;
use libghostty_vt::key::Key;
pub const MOD_SHIFT: u16 = ffi::MODS_SHIFT;
pub const MOD_CTRL: u16 = ffi::MODS_CTRL;
pub const MOD_ALT: u16 = ffi::MODS_ALT;
pub const MOD_SUPER: u16 = ffi::MODS_SUPER;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub mods: u16,
    pub text: Option<String>,
    pub unshifted: Option<char>,
}
const MODIFIERS: &[(&str, u16)] = &[
    ("ctrl", MOD_CTRL),
    ("control", MOD_CTRL),
    ("c", MOD_CTRL),
    ("alt", MOD_ALT),
    ("meta", MOD_ALT),
    ("option", MOD_ALT),
    ("opt", MOD_ALT),
    ("m", MOD_ALT),
    ("shift", MOD_SHIFT),
    ("s", MOD_SHIFT),
    ("super", MOD_SUPER),
    ("cmd", MOD_SUPER),
    ("command", MOD_SUPER),
    ("win", MOD_SUPER),
];
const NAMED: &[(&str, Key, u16)] = &[
    ("enter", Key::Enter, 0),
    ("return", Key::Enter, 0),
    ("cr", Key::Enter, 0),
    ("tab", Key::Tab, 0),
    ("backtab", Key::Tab, MOD_SHIFT),
    ("btab", Key::Tab, MOD_SHIFT),
    ("esc", Key::Escape, 0),
    ("escape", Key::Escape, 0),
    ("backspace", Key::Backspace, 0),
    ("bs", Key::Backspace, 0),
    ("bspace", Key::Backspace, 0),
    ("up", Key::ArrowUp, 0),
    ("down", Key::ArrowDown, 0),
    ("left", Key::ArrowLeft, 0),
    ("right", Key::ArrowRight, 0),
    ("home", Key::Home, 0),
    ("end", Key::End, 0),
    ("pageup", Key::PageUp, 0),
    ("page_up", Key::PageUp, 0),
    ("pgup", Key::PageUp, 0),
    ("ppage", Key::PageUp, 0),
    ("pagedown", Key::PageDown, 0),
    ("page_down", Key::PageDown, 0),
    ("pgdn", Key::PageDown, 0),
    ("npage", Key::PageDown, 0),
    ("insert", Key::Insert, 0),
    ("ins", Key::Insert, 0),
    ("ic", Key::Insert, 0),
    ("delete", Key::Delete, 0),
    ("del", Key::Delete, 0),
    ("dc", Key::Delete, 0),
    ("f1", Key::F1, 0),
    ("f2", Key::F2, 0),
    ("f3", Key::F3, 0),
    ("f4", Key::F4, 0),
    ("f5", Key::F5, 0),
    ("f6", Key::F6, 0),
    ("f7", Key::F7, 0),
    ("f8", Key::F8, 0),
    ("f9", Key::F9, 0),
    ("f10", Key::F10, 0),
    ("f11", Key::F11, 0),
    ("f12", Key::F12, 0),
];
const PUNCTUATION: &[(&str, char)] = &[
    ("space", ' '),
    ("minus", '-'),
    ("plus", '+'),
    ("equal", '='),
    ("equals", '='),
    ("comma", ','),
    ("period", '.'),
    ("dot", '.'),
    ("slash", '/'),
    ("backslash", '\\'),
    ("quote", '\''),
    ("apostrophe", '\''),
    ("doublequote", '"'),
    ("semicolon", ';'),
    ("colon", ':'),
    ("percent", '%'),
    ("ampersand", '&'),
    ("backtick", '`'),
    ("grave", '`'),
    ("tilde", '~'),
    ("lbracket", '['),
    ("rbracket", ']'),
    ("pipe", '|'),
    ("lt", '<'),
    ("gt", '>'),
];
const CHARS: &[(Key, char, char)] = &[
    (Key::Digit1, '1', '!'),
    (Key::Digit2, '2', '@'),
    (Key::Digit3, '3', '#'),
    (Key::Digit4, '4', '$'),
    (Key::Digit5, '5', '%'),
    (Key::Digit6, '6', '^'),
    (Key::Digit7, '7', '&'),
    (Key::Digit8, '8', '*'),
    (Key::Digit9, '9', '('),
    (Key::Digit0, '0', ')'),
    (Key::Minus, '-', '_'),
    (Key::Equal, '=', '+'),
    (Key::BracketLeft, '[', '{'),
    (Key::BracketRight, ']', '}'),
    (Key::Backslash, '\\', '|'),
    (Key::Semicolon, ';', ':'),
    (Key::Quote, '\'', '"'),
    (Key::Comma, ',', '<'),
    (Key::Period, '.', '>'),
    (Key::Slash, '/', '?'),
    (Key::Backquote, '`', '~'),
    (Key::Space, ' ', ' '),
];
const LETTERS: [Key; 26] = [
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
];
pub fn char_key(c: char) -> Option<(Key, char, bool)> {
    if c.is_ascii_alphabetic() {
        let lower = c.to_ascii_lowercase();
        return Some((LETTERS[(lower as u8 - b'a') as usize], lower, c.is_ascii_uppercase()));
    }
    CHARS.iter().find_map(|(key, plain, shifted)| {
        if c == *plain {
            Some((*key, *plain, false))
        } else if c == *shifted {
            Some((*key, *plain, true))
        } else {
            None
        }
    })
}
pub fn parse(token: &str) -> Option<Chord> {
    if token == " " {
        return parse("space");
    }
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    if token == "+" {
        return parse("plus");
    }
    let (mods, name) = split_modifiers(token)?;
    chord(mods, name)
}
fn modifier(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    MODIFIERS.iter().find(|(m, _)| *m == lower).map(|(_, bits)| *bits)
}
fn split_modifiers(token: &str) -> Option<(u16, &str)> {
    let mut mods = 0;
    let mut rest = token;
    loop {
        let cut = rest.char_indices().skip(1).find(|(_, c)| *c == '+' || *c == '-').map(|(i, _)| i);
        let Some(cut) = cut else { break };
        let Some(bits) = modifier(&rest[..cut]) else { break };
        mods |= bits;
        rest = &rest[cut + 1..];
        if rest.is_empty() {
            return None;
        }
    }
    Some((mods, rest))
}
pub fn chord(mods: u16, name: &str) -> Option<Chord> {
    let lower = name.to_ascii_lowercase();
    if let Some((_, key, implied)) = NAMED.iter().find(|(n, _, _)| *n == lower) {
        return Some(Chord { key: *key, mods: mods | implied, text: None, unshifted: None });
    }
    let c = match PUNCTUATION.iter().find(|(n, _)| *n == lower) {
        Some((_, c)) => *c,
        None => {
            let mut chars = name.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            c
        }
    };
    let Some((key, plain, shifted)) = char_key(c) else {
        return Some(Chord { key: Key::Unidentified, mods, text: Some(c.to_string()), unshifted: None });
    };
    let mut mods = mods;
    let shift = shifted || mods & MOD_SHIFT != 0;
    if shifted {
        mods |= MOD_SHIFT;
    }
    let text = if shift { shifted_char(plain) } else { plain };
    Some(Chord { key, mods, text: Some(text.to_string()), unshifted: Some(plain) })
}
const MODIFIER_KEYS: &[(u16, Key)] =
    &[(MOD_SHIFT, Key::ShiftLeft), (MOD_CTRL, Key::ControlLeft), (MOD_ALT, Key::AltLeft), (MOD_SUPER, Key::MetaLeft)];
pub fn modifier_key(bit: u16, mods: u16) -> Option<Chord> {
    let (_, key) = MODIFIER_KEYS.iter().find(|(b, _)| *b == bit)?;
    Some(Chord { key: *key, mods, text: None, unshifted: None })
}
pub fn is_modifier(key: Key) -> bool {
    MODIFIER_KEYS.iter().any(|(_, k)| *k == key)
        || [Key::ShiftRight, Key::ControlRight, Key::AltRight, Key::MetaRight].contains(&key)
}
pub const MODIFIER_BITS: [u16; 4] = [MOD_SHIFT, MOD_CTRL, MOD_ALT, MOD_SUPER];
fn shifted_char(plain: char) -> char {
    if plain.is_ascii_alphabetic() {
        return plain.to_ascii_uppercase();
    }
    CHARS.iter().find(|(_, p, _)| *p == plain).map(|(_, _, s)| *s).unwrap_or(plain)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_parse_with_modifiers_and_aliases() {
        assert_eq!(parse(" ").map(|c| c.key), parse("space").map(|c| c.key), "a space is the space key");
        let c = parse("ctrl+shift+tab").unwrap();
        assert_eq!((c.key, c.mods), (Key::Tab, MOD_CTRL | MOD_SHIFT));
        assert_eq!(parse("C-c").unwrap().mods, MOD_CTRL);
        assert_eq!(parse("A").unwrap().mods, MOD_SHIFT);
        assert_eq!(parse("A").unwrap().unshifted, Some('a'));
        assert_eq!(parse("+").unwrap().text.as_deref(), Some("+"));
        assert_eq!(parse("-").map(|c| c.text), Some(Some("-".into())));
        assert_eq!(parse("é").map(|c| c.text), Some(Some("é".into())));
        assert!(parse("ctrl+").is_none());
        assert!(parse("nosuchkey").is_none());
    }
}
