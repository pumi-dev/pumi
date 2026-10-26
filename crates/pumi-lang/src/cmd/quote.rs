/// The word as a command line reads it back: bare when it has nothing that says otherwise.
pub fn quote(word: &str) -> String {
    if !word.is_empty() && !word.chars().any(|c| " \t\n;{}\"'\\$#~%".contains(c)) {
        return word.to_string();
    }
    if word.contains(['\'', '\n']) {
        let mut out = String::from('"');
        for c in word.chars() {
            match c {
                '"' | '\\' | '$' => {
                    out.push('\\');
                    out.push(c);
                }
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
        out.push('"');
        return out;
    }
    format!("'{word}'")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_word_a_shell_would_read_apart_is_quoted() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("a;b"), "'a;b'");
        assert_eq!(quote("it's here"), "\"it's here\"");
    }
}
