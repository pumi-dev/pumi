//! `cargo xtask platform`: the rule that code for one OS lives in a file named for it.
//!
//! A file named `macos.rs`, `linux.rs`, `windows.rs` or `unix.rs` — a test file under `tests/`
//! included — may hold anything, and so may `crates/pumi-sys`, which is where the OS layer is.
//! Everything else builds on every OS, so it may not hold a `#[cfg(..)]`/`#[cfg_attr(..)]` that
//! mentions one, nor reach for an OS's own API by path. A `cfg!(..)` *expression* is allowed:
//! both of its branches compile, so the file does build everywhere.
//!
//! The one attribute allowed outside is the `#[path = "…"] mod os;` declaration that picks an
//! OS file — that line is the rule itself.
//!
//! Findings print as `path:line: the line`, and any finding fails the task.

use std::path::{Path, PathBuf};

/// The names of the files that may be one OS's: a module beside its `mod.rs`.
const OS_FILES: [&str; 4] = ["macos.rs", "linux.rs", "windows.rs", "unix.rs"];

/// An OS's own API, as code reaches it: allowed only in those files and in `crates/pumi-sys`.
const FOREIGN: [&str; 8] =
    ["libc::", "objc2", "block2", "windows::", "gtk::", "std::os::unix", "std::os::windows", "std::os::fd"];

/// What a `#[cfg(..)]` may not mention outside those files.
const OS_WORDS: [&str; 3] = ["target_os", "unix", "windows"];

pub fn run() -> Result<(), String> {
    let root = crate::root();
    let mut files = Vec::new();
    walk(&root, &root, &mut files).map_err(|err| format!("{}: {err}", root.display()))?;
    files.sort();
    let mut found = Vec::new();
    for file in &files {
        if is_os_file(file) {
            continue;
        }
        let text = std::fs::read_to_string(file).map_err(|err| format!("{}: {err}", file.display()))?;
        let lines: Vec<&str> = text.lines().collect();
        let path = file.strip_prefix(&root).unwrap_or(file);
        for number in offending_lines(&text) {
            let what = lines.get(number - 1).map_or("", |line| line.trim());
            found.push(format!("{}:{number}: {what}", path.display()));
        }
    }
    for finding in &found {
        println!("{finding}");
    }
    if found.is_empty() {
        println!("platform: nothing is built for one OS outside crates/pumi-sys and the OS-named files");
        return Ok(());
    }
    Err(format!(
        "{} finding(s): code that builds on one OS belongs in crates/pumi-sys, or in a file named for it (macos.rs, linux.rs, windows.rs, unix.rs)",
        found.len()
    ))
}

/// The workspace's `.rs` files: everything but `vendor/` (a checkout of someone else's), cargo's
/// `target/`, and `crates/pumi-sys`, which is where an OS's code is meant to be.
fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if path.is_dir() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if name.starts_with('.')
                || matches!(name, "vendor" | "target" | "node_modules")
                || relative == Path::new("crates/pumi-sys")
            {
                continue;
            }
            walk(&path, root, out)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn is_os_file(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()).is_some_and(|name| OS_FILES.contains(&name))
}

/// The 1-based lines of `text` that break the rule.
fn offending_lines(text: &str) -> Vec<usize> {
    let code = code_only(text);
    let line_of = line_index(&code);
    let mut lines = Vec::new();
    // Attributes, chain by chain: a run of `#[..]`s, then whatever they are attached to.
    let mut at = 0;
    while let Some(open) = code.get(at..).and_then(|rest| rest.find("#[")) {
        let start = at + open;
        let mut chain = Vec::new();
        let mut next = start;
        while let Some(rest) = code.get(next..).filter(|rest| rest.starts_with("#[")) {
            let end = attribute_end(&code, next).unwrap_or_else(|| rest.len() + next);
            chain.push((next, end));
            next = end + code[end..].find(|c: char| !c.is_whitespace()).unwrap_or(code.len() - end);
        }
        let picks_file = chain.iter().any(|(start, end)| code[*start..*end].starts_with("#[path"));
        let picks_module = code.get(next..).is_some_and(mod_declaration);
        if !(picks_file && picks_module) {
            for (start, end) in &chain {
                let attribute = &code[*start..*end];
                if attribute.starts_with("#[cfg") && OS_WORDS.iter().any(|word| attribute.contains(word)) {
                    lines.push(line_of(*start));
                }
            }
        }
        at = next.max(start + 1);
    }
    // An OS's own API: `use libc::…`, `windows::…`, `std::os::fd`, …
    for foreign in FOREIGN {
        lines.extend(code.match_indices(foreign).map(|(at, _)| line_of(at)));
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The byte after the `]` that closes the attribute at `start`, or None if it is unterminated.
fn attribute_end(code: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (at, byte) in code.bytes().enumerate().skip(start) {
        match byte {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `rest` starts with a module declaration (`mod os;`, `pub(crate) mod os;`) rather than
/// an inline module or something else.
fn mod_declaration(rest: &str) -> bool {
    let rest = rest.trim_start();
    let rest = match rest.strip_prefix("pub") {
        Some(after) => match after.strip_prefix('(') {
            Some(inner) => match inner.find(')') {
                Some(at) => &inner[at + 1..],
                None => return false,
            },
            None => after,
        },
        None => rest,
    };
    let Some(after) = rest.trim_start().strip_prefix("mod") else { return false };
    // `model` is not a module: the word after `mod` is whitespace.
    if !after.chars().next().is_some_and(char::is_whitespace) {
        return false;
    }
    let after = after.trim_start();
    let name: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    !name.is_empty() && after[name.len()..].trim_start().starts_with(';')
}

/// A function from a byte offset in `code` to its 1-based line.
fn line_index(code: &str) -> impl Fn(usize) -> usize + '_ {
    let starts: Vec<usize> = std::iter::once(0).chain(code.match_indices('\n').map(|(at, _)| at + 1)).collect();
    move |offset| starts.partition_point(|start| *start <= offset)
}

/// `text` with comments and string literals blanked out: same bytes, same line breaks, so an
/// offset still names the line it came from. The lint reads code only, which is also why this
/// file's list of forbidden paths does not flag itself.
fn code_only(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut code = bytes.to_vec();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    code[at] = b' ';
                    at += 1;
                }
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while at < bytes.len() {
                    if bytes[at] == b'/' && bytes.get(at + 1) == Some(&b'*') {
                        depth += 1;
                        blank(&mut code, at, 2);
                        at += 2;
                    } else if bytes[at] == b'*' && bytes.get(at + 1) == Some(&b'/') {
                        depth -= 1;
                        blank(&mut code, at, 2);
                        at += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        blank(&mut code, at, 1);
                        at += 1;
                    }
                }
            }
            b'"' => at = blank_literal(&mut code, bytes, at, b'"'),
            b'r' | b'b' | b'c' if raw_string_start(bytes, at) => at = blank_literal(&mut code, bytes, at, b'"'),
            b'\'' if bytes.get(at + 1) == Some(&b'\\') => at = blank_literal(&mut code, bytes, at, b'\''),
            b'\'' if bytes.get(at + 2) == Some(&b'\'') => at = blank_literal(&mut code, bytes, at, b'\''),
            _ => at += 1,
        }
    }
    String::from_utf8(code).expect("blanking keeps the file's UTF-8")
}

fn blank(code: &mut [u8], at: usize, len: usize) {
    let end = (at + len).min(code.len());
    for byte in &mut code[at..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

/// Whether the string starting at `at` is raw (`r"…"`, `r#"…"#`, `br#"…"#`): not an identifier
/// that happens to end in `r`.
fn raw_string_start(bytes: &[u8], at: usize) -> bool {
    let raw = if bytes[at] == b'r' {
        let identifier = at > 0 && (bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        if identifier {
            return false;
        }
        at
    } else {
        at + 1
    };
    if bytes.get(raw) != Some(&b'r') {
        return false;
    }
    let after = raw + 1 + bytes[raw + 1..].iter().take_while(|byte| **byte == b'#').count();
    bytes.get(after) == Some(&b'"')
}

/// Blanks the string or character literal at `at`, and returns the byte after it. A raw string
/// (`r"…"`, `r#"…"#`) ends at its quote plus its `#`s and knows no escapes; a plain one ends at
/// the first quote `close` that no `\` escapes.
fn blank_literal(code: &mut [u8], bytes: &[u8], at: usize, close: u8) -> usize {
    let raw = bytes[at] != b'"' && bytes[at] != b'\'';
    let quote = if raw { at + bytes[at..].iter().take_while(|byte| **byte != b'"').count() } else { at };
    let hashes = if raw { bytes[quote + 1..].iter().take_while(|byte| **byte == b'#').count() } else { 0 };
    let mut end = quote + 1 + hashes;
    while end < bytes.len() {
        let closes = bytes[end] == close
            && if raw {
                bytes[end + 1..].iter().take_while(|byte| **byte == b'#').count() >= hashes
            } else {
                !escaped(bytes, quote + 1, end)
            };
        if closes {
            end += 1 + hashes;
            break;
        }
        end += 1;
    }
    blank(code, at, end - at);
    end
}

/// Whether the byte at `at` is escaped: an odd number of `\` between `from` and it.
fn escaped(bytes: &[u8], from: usize, at: usize) -> bool {
    bytes[from..at].iter().rev().take_while(|byte| **byte == b'\\').count() % 2 == 1
}
