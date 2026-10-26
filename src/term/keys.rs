use gpui::Keystroke;
pub use pumi_gpuix::chord;
/// macOS editing shortcuts a shell understands, as Zed's terminal binds them
/// (assets/keymaps/default-macos.json, GPL-3.0-or-later).
pub const MAC_EDITING: &[(&str, &[u8])] = &[
    ("cmd+left", &[0x01]),
    ("cmd+up", &[0x01]),
    ("cmd+right", &[0x05]),
    ("cmd+down", &[0x05]),
    ("alt+left", b"\x1bb"),
    ("alt+right", b"\x1bf"),
    ("cmd+backspace", &[0x15]),
    ("cmd+delete", &[0x0b]),
];
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    Bytes(&'static [u8]),
    Key(pumi_keys::Chord),
    Text,
    None,
}
pub fn typed(keystroke: &Keystroke, option_as_alt: bool, kitty: u8) -> Typed {
    let chord = chord(keystroke);
    if let Some((_, bytes)) = MAC_EDITING.iter().find(|(name, _)| *name == chord) {
        return Typed::Bytes(bytes);
    }
    let m = keystroke.modifiers;
    if m.platform {
        return Typed::None;
    }
    let key = keystroke.key.as_str();
    let printable = key == "space" || key.chars().count() == 1;
    let composes = m.alt && !option_as_alt;
    let all_keys = kitty & pumi_keys::kitty::ALL_KEYS != 0;
    if printable && !m.control && (composes || !all_keys) && (!m.alt || composes) {
        return Typed::Text;
    }
    pumi_keys::chord(mods(m), key).map(Typed::Key).unwrap_or(Typed::None)
}
pub fn released(keystroke: &Keystroke, option_as_alt: bool) -> Option<pumi_keys::Chord> {
    match typed(keystroke, option_as_alt, pumi_keys::kitty::ALL_KEYS) {
        Typed::Key(chord) => Some(chord),
        _ => None,
    }
}
pub fn modifier_changes(was: gpui::Modifiers, now: gpui::Modifiers) -> Vec<(pumi_keys::Chord, bool)> {
    let (was, now) = (mods(was) | super_bit(was), mods(now) | super_bit(now));
    pumi_keys::MODIFIER_BITS
        .into_iter()
        .filter(|bit| (was ^ now) & bit != 0)
        .filter_map(|bit| pumi_keys::modifier_key(bit, now).map(|chord| (chord, now & bit != 0)))
        .collect()
}
fn super_bit(m: gpui::Modifiers) -> u16 {
    if m.platform { pumi_keys::MOD_SUPER } else { 0 }
}
fn mods(m: gpui::Modifiers) -> u16 {
    [(m.shift, pumi_keys::MOD_SHIFT), (m.control, pumi_keys::MOD_CTRL), (m.alt, pumi_keys::MOD_ALT)]
        .into_iter()
        .filter(|(on, _)| *on)
        .fold(0, |mods, (_, bit)| mods | bit)
}
pub fn option_as_alt(on: bool) -> pumi_keys::OptionAsAlt {
    if on { pumi_keys::OptionAsAlt::Both } else { pumi_keys::OptionAsAlt::Neither }
}
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    let clean: String = text.replace("\r\n", "\n").chars().filter(|&ch| ch != '\x1b').collect();
    if bracketed { format!("\x1b[200~{clean}\x1b[201~").into_bytes() } else { clean.replace('\n', "\r").into_bytes() }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(spec: &str) -> Keystroke {
        Keystroke::parse(spec).unwrap()
    }
    #[test]
    fn all_keys_makes_text_keys_keys_but_keeps_composing() {
        let all = pumi_keys::kitty::ALL_KEYS;
        assert!(matches!(typed(&key("a"), true, all), Typed::Key(_)));
        assert_eq!(typed(&key("alt-e"), false, all), Typed::Text, "option still composes");
        assert_eq!(typed(&key("a"), true, 1), Typed::Text, "disambiguate alone: text is text");
        assert!(released(&key("a"), true).is_some());
        assert!(released(&key("cmd-a"), true).is_none());
    }
    #[test]
    fn modifier_changes_name_each_key() {
        let shift = gpui::Modifiers { shift: true, ..Default::default() };
        let both = gpui::Modifiers { shift: true, control: true, ..Default::default() };
        let pressed = modifier_changes(shift, both);
        assert_eq!(pressed.len(), 1);
        assert!(pressed[0].1, "ctrl went down");
        assert_eq!(pressed[0].0.mods, pumi_keys::MOD_SHIFT | pumi_keys::MOD_CTRL);
        let released = modifier_changes(both, gpui::Modifiers::default());
        assert_eq!(released.iter().filter(|(_, down)| !down).count(), 2);
    }
    #[test]
    fn keystrokes_for_ghostty() {
        let t = |spec: &str, alt: bool| typed(&key(spec), alt, 0);
        assert_eq!(t("cmd-left", true), Typed::Bytes(&[0x01]));
        assert_eq!(t("alt-left", false), Typed::Bytes(b"\x1bb"));
        assert_eq!(t("cmd-t", true), Typed::None);
        assert_eq!(t("a", true), Typed::Text);
        assert_eq!(t("shift-a", true), Typed::Text);
        assert_eq!(t("alt-e", false), Typed::Text, "option composes");
        let Typed::Key(chord) = t("alt-b", true) else { panic!("alt+b is a key") };
        assert_eq!(chord.mods, pumi_keys::MOD_ALT);
        let Typed::Key(chord) = t("ctrl-shift-enter", true) else { panic!("a key") };
        assert_eq!(chord.mods, pumi_keys::MOD_CTRL | pumi_keys::MOD_SHIFT);
        assert!(matches!(t("up", false), Typed::Key(_)));
        assert!(matches!(t("ctrl-c", false), Typed::Key(_)));
    }
    #[test]
    fn paste_is_bracketed_and_sanitized() {
        assert_eq!(paste("a\r\nb", true), b"\x1b[200~a\nb\x1b[201~");
        assert_eq!(paste("x\x1b[201~rm -rf", true), b"\x1b[200~x[201~rm -rf\x1b[201~");
        assert_eq!(paste("a\r\nb\n", false), b"a\rb\r", "unbracketed lines end as Enter does");
    }
}
