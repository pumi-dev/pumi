use std::path::{Path, PathBuf};
const LIMIT: usize = 35_000;
pub fn run() -> Result<(), String> {
    let root = crate::root().join("crates");
    let mut files = Vec::new();
    for src in ["pumid/src", "pumi-model/src"].map(|dir| root.join(dir)) {
        collect(&src, &mut files).map_err(|err| format!("{}: {err}", src.display()))?;
    }
    files.sort();
    let mut rows = Vec::new();
    for file in &files {
        let text = std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
        let bytes = without_tests(&text).len();
        rows.push((file.strip_prefix(&root).unwrap_or(file).display().to_string(), bytes));
    }
    rows.sort_by(|a, b| b.1.cmp(&a.1));
    let total: usize = rows.iter().map(|r| r.1).sum();
    for (name, bytes) in rows.iter().take(15) {
        println!("{:>8} B {:>7} tok  {name}", bytes, tokens(*bytes));
    }
    let tokens = tokens(total);
    println!("pumid: {} files, {total} B, {tokens} tokens (budget {LIMIT})", rows.len());
    if tokens > LIMIT { Err(format!("pumid is {tokens} tokens, over {LIMIT}")) } else { Ok(()) }
}
fn tokens(bytes: usize) -> usize {
    (bytes as f64 / 3.6).round() as usize
}
fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}
pub fn without_tests(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let item = &rest[at..];
        rest = &item[item_end(item)..];
    }
    out.push_str(rest);
    out
}
fn item_end(s: &str) -> usize {
    let b = s.as_bytes();
    let (mut i, mut depth) = ("#[cfg(test)]".len(), 0usize);
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => i += s[i..].find('\n').unwrap_or(s.len() - i),
            b'/' if b.get(i + 1) == Some(&b'*') => i += s[i..].find("*/").map_or(s.len() - i, |e| e + 1),
            b'r' if matches!(b.get(i + 1), Some(b'"' | b'#')) && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) => {
                let hashes = s[i + 1..].bytes().take_while(|&c| c == b'#').count();
                let close = format!("\"{}", "#".repeat(hashes));
                let open = i + 1 + hashes + 1;
                i = open + s[open..].find(&close).map_or(s.len() - open, |e| e + close.len()) - 1;
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'\'' if b.get(i + 1) == Some(&b'\\') => i += s[i + 3..].find('\'').map_or(s.len() - i, |e| e + 3),
            b'\'' if b.get(i + 2) == Some(&b'\'') => i += 2,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            b';' if depth == 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    s.len()
}
