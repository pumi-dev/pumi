const NAMES: &[(&str, &str)] = &[
    ("cr", "Enter"),
    ("enter", "Enter"),
    ("return", "Enter"),
    ("esc", "Escape"),
    ("escape", "Escape"),
    ("tab", "Tab"),
    ("bs", "BSpace"),
    ("backspace", "BSpace"),
    ("bspace", "BSpace"),
    ("space", "Space"),
    ("lt", "<"),
    ("bar", "|"),
    ("bslash", "\\"),
    ("slash", "/"),
    ("minus", "-"),
    ("equal", "="),
    ("plus", "+"),
    ("del", "DC"),
    ("delete", "DC"),
    ("dc", "DC"),
    ("insert", "IC"),
    ("ic", "IC"),
    ("home", "Home"),
    ("end", "End"),
    ("pageup", "PPage"),
    ("ppage", "PPage"),
    ("pgup", "PPage"),
    ("pagedown", "NPage"),
    ("npage", "NPage"),
    ("pgdn", "NPage"),
    ("up", "Up"),
    ("down", "Down"),
    ("left", "Left"),
    ("right", "Right"),
    ("btab", "BTab"),
    ("leftmouse", "LeftMouse"),
    ("rightmouse", "RightMouse"),
    ("middlemouse", "MiddleMouse"),
    ("leftdrag", "LeftDrag"),
    ("leftrelease", "LeftRelease"),
    ("scrollwheelup", "ScrollWheelUp"),
    ("scrollwheeldown", "ScrollWheelDown"),
    ("scrollwheelleft", "ScrollWheelLeft"),
    ("scrollwheelright", "ScrollWheelRight"),
    ("x1mouse", "X1Mouse"),
    ("x2mouse", "X2Mouse"),
    ("pinchin", "PinchIn"),
    ("pinchout", "PinchOut"),
    ("forceclick", "ForceClick"),
    ("longpress", "LongPress"),
];
pub fn canonical(key: &str) -> Option<String> {
    let inner = key.strip_prefix('<').and_then(|k| k.strip_suffix('>')).unwrap_or(key);
    if inner.is_empty() {
        return None;
    }
    let (mut ctrl, mut alt, mut shift, mut cmd) = (false, false, false, false);
    let mut rest = inner;
    while rest.len() > 2 && rest.as_bytes()[1] == b'-' {
        match rest.as_bytes()[0].to_ascii_uppercase() {
            b'C' => ctrl = true,
            b'M' | b'A' => alt = true,
            b'S' => shift = true,
            b'D' => cmd = true,
            _ => break,
        }
        rest = &rest[2..];
    }
    let mut name = match NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(rest)) {
        Some((_, tmux)) => tmux.to_string(),
        None if rest.chars().count() == 1 => rest.to_string(),
        None if rest.len() >= 2 && rest.starts_with(['F', 'f']) && rest[1..].parse::<u8>().is_ok() => {
            format!("F{}", &rest[1..])
        }
        None => rest.to_string(),
    };
    if name.chars().count() == 1 {
        let c = name.chars().next().expect("one");
        if shift && c.is_ascii_alphabetic() {
            name = c.to_ascii_uppercase().to_string();
            shift = false;
        } else if ctrl && c.is_ascii_uppercase() {
            name = c.to_ascii_lowercase().to_string();
        }
    }
    let mut out = String::new();
    for (on, m) in [(ctrl, "C-"), (alt, "M-"), (shift, "S-"), (cmd, "D-")] {
        if on {
            out.push_str(m);
        }
    }
    out.push_str(&name);
    Some(out)
}
pub fn parse(lhs: &str) -> Vec<String> {
    let lhs = lhs.trim();
    if !lhs.contains('<') {
        if lhs.contains(' ') || lhs.contains('-') && lhs.len() > 1 || lhs.chars().count() == 1 {
            return lhs.split_whitespace().filter_map(canonical).collect();
        }
        if NAMES.iter().any(|(n, _)| n.eq_ignore_ascii_case(lhs)) || is_fkey(lhs) {
            return canonical(lhs).into_iter().collect();
        }
        // A key spelled as the app's keymap spells a chord (`cmd+shift+a`) is one key, not its
        // letters: read that way it would enter a table as `c m d + s h i f t + a`, and every
        // `c` typed would start a sequence instead of reaching the tab.
        return match chord_key(lhs) {
            Some(key) => vec![key],
            None => lhs.chars().filter_map(|c| canonical(&c.to_string())).collect(),
        };
    }
    let mut keys = Vec::new();
    let mut chars = lhs.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c == '<'
            && let Some(end) = lhs[at..].find('>')
            && end > 1
        {
            keys.extend(canonical(&lhs[at..=at + end]));
            while chars.peek().is_some_and(|(i, _)| *i <= at + end) {
                chars.next();
            }
            continue;
        }
        if !c.is_whitespace() {
            keys.extend(canonical(&c.to_string()));
        }
    }
    keys
}
fn is_fkey(s: &str) -> bool {
    s.len() >= 2 && s.starts_with(['F', 'f']) && s[1..].parse::<u8>().is_ok()
}
/// The modifiers as both spellings name them, as `from_chord` takes them.
fn modifier(word: &str) -> Option<&'static str> {
    match word.trim().to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some("ctrl"),
        "alt" | "option" | "opt" | "win" => Some("alt"),
        "shift" => Some("shift"),
        "cmd" | "command" | "super" | "meta" => Some("cmd"),
        _ => None,
    }
}
/// `cmd+shift+a` as one key (`D-A`): every part but the last a modifier, the last a key's name.
/// `None` for what is not a chord, so text stays text (`jk` is `j` then `k`).
fn chord_key(chord: &str) -> Option<String> {
    let parts: Vec<&str> = chord.split('+').collect();
    let (name, mods) = parts.split_last()?;
    if mods.is_empty() || name.trim().is_empty() {
        return None;
    }
    let known = NAMES.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) || is_fkey(name);
    if name.chars().count() != 1 && !known {
        return None;
    }
    let mut spelled: Vec<&str> = Vec::new();
    for m in mods {
        spelled.push(modifier(m)?);
    }
    spelled.push(name.trim());
    from_chord(&spelled.join("+"))
}
pub fn normalise(lhs: &str) -> String {
    parse(lhs).join(" ")
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replica {
    pub tables: std::collections::BTreeMap<String, Vec<(String, Option<u64>)>>,
    pub prefixes: Vec<String>,
    pub mode: String,
}
impl Replica {
    pub fn from_bindings(bindings: &[serde_json::Value], prefixes: Vec<String>, mode: String) -> Replica {
        let mut tables: std::collections::BTreeMap<String, Vec<(String, Option<u64>)>> = Default::default();
        for b in bindings {
            let (Some(table), Some(key)) = (b["table"].as_str(), b["key"].as_str()) else { continue };
            tables.entry(table.to_string()).or_default().push((key.to_string(), b["tab"].as_u64()));
        }
        Replica { tables, prefixes: prefixes.iter().map(|p| normalise(p)).filter(|p| !p.is_empty()).collect(), mode }
    }
    pub fn is_the_tabs(&self, tab_type: &str, tab: Option<u64>, pending: bool, key: &str) -> bool {
        if pending || !(self.mode.is_empty() || self.mode == "root") || self.prefixes.iter().any(|p| p == key) {
            return false;
        }
        let longer = format!("{key} ");
        ![tab_type, "global", "root"]
            .iter()
            .filter_map(|t| self.tables.get(*t))
            .flatten()
            .any(|(seq, local)| (local.is_none() || *local == tab) && (seq == key || seq.starts_with(&longer)))
    }
}
pub fn from_chord(chord: &str) -> Option<String> {
    let parts: Vec<&str> = chord.split('+').collect();
    let (name, mods) = match parts.split_last() {
        Some((&"", rest)) if !rest.is_empty() => ("+", &rest[..rest.len() - 1]),
        Some((name, mods)) => (*name, mods),
        None => return None,
    };
    let mut key = String::from("<");
    for (m, tag) in [("ctrl", "C-"), ("alt", "M-"), ("shift", "S-"), ("cmd", "D-")] {
        if mods.contains(&m) {
            key.push_str(tag);
        }
    }
    key.push_str(name);
    key.push('>');
    canonical(&key)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_notations_meet() {
        assert_eq!(normalise("<D-d>"), "D-d");
        assert_eq!(normalise("<C-b>c"), "C-b c");
        assert_eq!(normalise("C-b"), "C-b");
        assert_eq!(normalise("<CR>"), "Enter");
        assert_eq!(normalise("Enter"), "Enter");
        assert_eq!(normalise("<S-a>"), "A");
        assert_eq!(normalise("S-C-Left"), "C-S-Left");
        assert_eq!(normalise("M-Left"), "M-Left");
        assert_eq!(normalise("<A-x>"), "M-x");
        assert_eq!(normalise("|"), "|");
        assert_eq!(normalise("jk"), "j k");
        assert_eq!(normalise("F5"), "F5");
        assert_eq!(normalise("<D-PinchOut>"), "D-PinchOut");
        assert_eq!(normalise("-"), "-");
        assert_eq!(normalise("C--"), "C--");
    }
    #[test]
    fn a_chord_spelled_with_pluses_is_one_key() {
        assert_eq!(normalise("cmd+shift+a"), "D-A", "not c m d + s h i f t + a");
        assert_eq!(normalise("cmd+t"), "D-t");
        assert_eq!(normalise("command+ctrl+f"), "C-D-f");
        assert_eq!(normalise("ctrl+shift+left"), "C-S-Left");
        assert_eq!(normalise("option+left"), "M-Left");
        assert_eq!(normalise("shift+enter"), "S-Enter");
        assert_eq!(normalise("jk"), "j k", "text is still a sequence");
        assert_eq!(normalise("a+b"), "a + b", "`a` is no modifier");
    }
    #[test]
    fn chords_read_as_keys() {
        assert_eq!(from_chord("shift+cmd+d").as_deref(), Some("D-D"));
        assert_eq!(from_chord("ctrl+b").as_deref(), Some("C-b"));
        assert_eq!(from_chord("enter").as_deref(), Some("Enter"));
        assert_eq!(from_chord("alt+left").as_deref(), Some("M-Left"));
        assert_eq!(from_chord("cmd+d").as_deref(), Some("D-d"));
    }
    #[test]
    fn spelled_out_punctuation_is_its_character() {
        assert_eq!(from_chord("slash").as_deref(), Some("/"));
        assert_eq!(from_chord("minus").as_deref(), Some("-"));
        assert_eq!(from_chord("equal").as_deref(), Some("="));
        assert_eq!(from_chord("plus").as_deref(), Some("+"));
        assert_eq!(from_chord("shift+slash").as_deref(), Some("S-/"));
        assert_eq!(from_chord("/").as_deref(), Some("/"));
        assert_eq!(normalise("slash"), "/");
    }
}
