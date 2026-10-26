//! Windows: `Win32_Process`, through PowerShell (`Get-CimInstance`). A process's directory is not
//! known here: it is the process's own memory, which another process cannot read simply.
use super::ProcessInfo;
use pumi_api::{Error, Nullable, Result};
use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

/// `CREATE_NO_WINDOW`: PowerShell without a console window flashing up.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Each process as one JSON object: its processor time (100 ns units) and age (seconds) give
/// its use over its life, as `ps` reports it.
const SCRIPT: &str = r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8
$now = Get-Date
$all = @(Get-CimInstance Win32_Process | ForEach-Object {
  [pscustomobject]@{
    pid = $_.ProcessId; ppid = $_.ParentProcessId; name = $_.Name; commandLine = $_.CommandLine
    rss = $_.WorkingSetSize; time = $_.KernelModeTime + $_.UserModeTime
    age = $(if ($_.CreationDate) { ($now - $_.CreationDate).TotalSeconds } else { $null })
  }
})
ConvertTo-Json -InputObject $all -Compress"#;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Process {
    pid: u64,
    ppid: Option<u64>,
    name: Option<String>,
    command_line: Option<String>,
    rss: Option<u64>,
    time: Option<f64>,
    age: Option<f64>,
}

pub fn processes(_want_cwd: bool) -> Result<Vec<ProcessInfo>> {
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|err| Error::exception(format!("powershell: {err}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error::exception(if err.is_empty() { format!("powershell failed: {}", out.status) } else { err }));
    }
    let listed: Vec<Process> = serde_json::from_slice(&out.stdout)
        .map_err(|err| Error::exception(format!("Win32_Process as JSON: {err}")))?;
    Ok(listed.into_iter().map(info).collect())
}

fn info(p: Process) -> ProcessInfo {
    let cpu = match (p.time, p.age) {
        (Some(time), Some(age)) if age > 0.0 => time / 1e7 / age * 100.0,
        _ => 0.0,
    };
    let command = match p.command_line.as_deref().map(argv) {
        Some(argv) if !argv.is_empty() => argv,
        _ => p.name.into_iter().collect(),
    };
    ProcessInfo {
        pid: p.pid,
        parent_pid: Nullable(p.ppid),
        cpu,
        rss: p.rss.unwrap_or_default(),
        command,
        cwd: Nullable(None),
    }
}

/// A command line as its arguments, the way `CommandLineToArgvW` splits one: spaces outside
/// quotes split; backslashes are literal unless they come before a quote, where each pair is
/// one backslash and an odd one makes the quote literal.
fn argv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut started) = (false, false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut slashes = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    slashes += 1;
                }
                if chars.peek() == Some(&'"') {
                    word.extend(std::iter::repeat_n('\\', slashes / 2));
                    if slashes % 2 == 1 {
                        chars.next();
                        word.push('"');
                    }
                } else {
                    word.extend(std::iter::repeat_n('\\', slashes));
                }
                started = true;
            }
            '"' => {
                // `""` inside quotes is one literal quote.
                if quoted && chars.peek() == Some(&'"') {
                    chars.next();
                    word.push('"');
                } else {
                    quoted = !quoted;
                }
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    out.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(word);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::argv;
    #[test]
    fn a_command_line_splits_as_windows_splits_it() {
        assert_eq!(argv(r#""C:\Program Files\node.exe" app.js"#), [r"C:\Program Files\node.exe", "app.js"]);
        assert_eq!(argv(r#"cmd /c "echo hi""#), ["cmd", "/c", "echo hi"]);
        assert_eq!(argv(r#"a\\b \"q\" c\\\"d"#), [r"a\\b", r#""q""#, r#"c\"d"#]);
        assert_eq!(argv(r#"x """#), ["x", ""]);
        assert_eq!(argv(""), Vec::<String>::new());
    }
}
