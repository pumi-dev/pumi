//! Unix: `ps` for the processes, `lsof` for their directories.
use super::ProcessInfo;
use crate::tool;
use pumi_api::{Nullable, Result};
use std::collections::BTreeMap;

pub fn processes(want_cwd: bool) -> Result<Vec<ProcessInfo>> {
    let out = tool("ps", &["-axo", "pid=,ppid=,pcpu=,rss=,command="])?;
    let mut processes: Vec<(u64, u64, f64, u64, Vec<String>)> = Vec::new();
    for line in out.lines() {
        let mut parts = line.splitn(5, char::is_whitespace).filter(|part| !part.is_empty());
        let (Some(pid), Some(ppid)) =
            (parts.next().and_then(|p| p.parse().ok()), parts.next().and_then(|p| p.parse().ok()))
        else {
            continue;
        };
        let cpu = parts.next().and_then(|p| p.parse().ok()).unwrap_or_default();
        // `ps` counts kilobytes.
        let rss = parts.next().and_then(|p| p.parse::<u64>().ok()).unwrap_or_default() * 1024;
        processes.push((pid, ppid, cpu, rss, argv(parts.next().unwrap_or_default())));
    }
    let dirs =
        if want_cwd { cwds(&processes.iter().map(|(pid, ..)| *pid).collect::<Vec<_>>()) } else { BTreeMap::new() };
    Ok(processes
        .into_iter()
        .map(|(pid, ppid, cpu, rss, command)| ProcessInfo {
            pid,
            parent_pid: Nullable(Some(ppid)),
            cpu,
            rss,
            command,
            cwd: Nullable(dirs.get(&pid).cloned()),
        })
        .collect())
}
/// Each process's directory, in as few `lsof` calls as the system takes.
fn cwds(pids: &[u64]) -> BTreeMap<u64, String> {
    let mut out = BTreeMap::new();
    for chunk in pids.chunks(200) {
        let list = chunk.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        let Ok(text) = tool("lsof", &["-a", "-d", "cwd", "-Fn", "-p", &list]) else { continue };
        let mut pid = None;
        for line in text.lines() {
            if let Some(id) = line.strip_prefix('p') {
                pid = id.parse().ok();
            } else if let Some(path) = line.strip_prefix('n')
                && let Some(pid) = pid
            {
                out.insert(pid, path.to_string());
            }
        }
    }
    out
}
/// A command line as its arguments: spaces split them, and quotes and backslashes group them.
fn argv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted: Option<char> = None;
    let mut started = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (Some(q), c) if c == q => quoted = None,
            (Some(_), '\\') => word.extend(chars.next()),
            (Some(_), c) => word.push(c),
            (None, '\'' | '"') => {
                quoted = Some(c);
                started = true;
            }
            (None, '\\') => {
                word.extend(chars.next());
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if started || !word.is_empty() {
        out.push(word);
    }
    out
}
#[cfg(test)]
mod tests {
    use super::argv;
    #[test]
    fn a_command_line_splits_into_its_arguments() {
        assert_eq!(argv("vim src/main.rs"), ["vim", "src/main.rs"]);
        assert_eq!(argv("sh -c 'echo hi there'"), ["sh", "-c", "echo hi there"]);
        assert_eq!(argv(r#"open "My File.md""#), ["open", "My File.md"]);
        assert_eq!(argv(""), Vec::<String>::new());
    }
}
