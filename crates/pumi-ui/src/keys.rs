use gpui::{Keystroke, Modifiers};
pub fn chord(keystroke: &Keystroke) -> String {
    let Modifiers { control, alt, mut shift, platform, .. } = keystroke.modifiers;
    let mut key = normalize_key(&keystroke.key);
    // Off macOS terminals copy and paste with ctrl+shift+c/v (ctrl+c is the shell's); there they
    // are the app's cmd+c/v. Everything else bound to cmd is the Super (Windows) key, as gpui has it.
    if !cfg!(target_os = "macos") && control && shift && !alt && !platform && matches!(key.as_str(), "c" | "v") {
        return format!("cmd+{key}");
    }
    if key == "~" && !shift {
        (shift, key) = (true, "`".into());
    }
    let mut parts: Vec<&str> = Vec::new();
    for (pressed, name) in [(control, "ctrl"), (alt, "alt"), (shift, "shift"), (platform, "cmd")] {
        if pressed {
            parts.push(name);
        }
    }
    if parts.is_empty() { key } else { format!("{}+{key}", parts.join("+")) }
}
pub fn normalize_key(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cmd" | "command" | "super" | "meta" => "cmd".into(),
        "ctrl" | "control" => "ctrl".into(),
        "option" | "opt" => "alt".into(),
        "-" => "minus".into(),
        "=" => "equal".into(),
        "+" => "plus".into(),
        "/" => "slash".into(),
        other => other.to_string(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_chord_has_one_name() {
        let named = |s: &str| chord(&Keystroke::parse(s).unwrap());
        assert_eq!(named("cmd-shift-p"), "shift+cmd+p");
        assert_eq!(named("alt-left"), "alt+left");
        assert_eq!(named("enter"), "enter");
        assert_eq!(normalize_key("Command"), "cmd");
        assert_eq!(normalize_key("-"), "minus");
    }
}
