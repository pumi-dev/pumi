//! `pumi do FILE [k=v …] [--args JSON|@FILE] [--session ID] [--tab ID] [--all] [--for DURATION]
//! [--fail-fast] [--json]`: one script, now, with its answer and an exit status. It runs on Bun
//! with the worker's SDK as `pumi`, against the server (`runtime/app/worker/do.ts`), its calls
//! defaulting to `--session`'s session (`PUMI_SESSION`) and `--tab`'s tab (`PUMI_TAB`).
use crate::client::Address;
use pumi_api::{Error, ErrorKind};
use serde_json::{Map, Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: pumi do FILE [KEY=VALUE …] [--args JSON|@FILE] [--session ID] [--tab ID] [--all] [--for DURATION] [--fail-fast] [--json]
       pumi do -e EXPRESSION [KEY=VALUE …]   an expression, with the SDK's names in scope
       pumi do - [KEY=VALUE …]               a script on stdin";

fn usage(message: impl std::fmt::Display) -> Error {
    Error::invalid_params(format!("{message}\n{USAGE}"))
}

/// The runtime directory: the app bundle's, an install's, or this checkout's.
fn runtime_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("PUMI_RUNTIME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let is_runtime = |dir: &Path| dir.join("app/worker/do.ts").is_file();
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.canonicalize().unwrap_or(exe);
        for up in exe.ancestors().skip(1).take(4) {
            for candidate in [up.join("Resources/runtime"), up.join("runtime"), up.join("share/pumi/runtime")] {
                if is_runtime(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    let checkout = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    is_runtime(&checkout).then_some(checkout)
}

/// Bun: `$PUMI_BUN`, `bun` on the PATH, or the app (a Bun executable, which runs a script as Bun
/// with `BUN_BE_BUN=1`).
fn bun() -> Option<(PathBuf, bool)> {
    if let Some(path) = std::env::var_os("PUMI_BUN") {
        return Some((PathBuf::from(path), false));
    }
    if let Some(found) = std::env::var_os("PATH")
        .and_then(|path| std::env::split_paths(&path).map(|d| d.join("bun")).find(|p| p.is_file()))
    {
        return Some((found, false));
    }
    let app = crate::client::pumid()?.parent()?.join("pumi");
    app.is_file().then_some((app, true))
}

/// An id as its place spells it: `%3` for a tab, and a bare `3` takes the sigil.
fn id_of(word: &str, sigil: char) -> String {
    let bare = word.strip_prefix(sigil).unwrap_or(word);
    if bare.chars().all(|c| c.is_ascii_digit()) && !bare.is_empty() {
        return format!("{sigil}{bare}");
    }
    word.to_string()
}

/// `value` as an argument: JSON when it is JSON (`3`, `true`, `[1]`), else the text.
fn value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
}

/// The nouns `-e` has as `noun.verb(…)` objects where the SDK has none of its own: `tab.cells("%3")`
/// is `call("tab.cells", "%3")`, and a tab method (`tab.read("%3", {lines: 5})`) is `tab.call`'s.
const NOUNS: &[&str] = &["tab", "pane", "view", "session", "app", "update"];
const TAB_METHODS: &[&str] = &["read", "input", "run", "wait", "selection", "report", "attach"];

/// `pumi do -e EXPRESSION` as a script: the SDK's names in scope (`workspace`, `call`, `tab`,
/// …), `pumi` itself, every noun as `noun.verb(…)`, and a name that is none of them says what is.
fn expression_script(expression: &str) -> String {
    let nouns = serde_json::to_string(NOUNS).unwrap_or_default();
    let methods = serde_json::to_string(TAB_METHODS).unwrap_or_default();
    format!(
        r#"import * as pumi from "pumi";
const {{ process: _process, ...names }} = pumi as any;
Object.assign(globalThis, names, {{ pumi }});
const methods = new Set({methods});
const noun = (name: string) => new Proxy({{}}, {{
  get: (_: unknown, verb: unknown) => typeof verb !== "string" || verb === "then" ? undefined
    : name === "tab" && methods.has(verb) ? (tab: unknown, opts: unknown = {{}}) => pumi.call("tab.call", tab, verb, opts)
    : (...args: unknown[]) => pumi.call(`${{name}}.${{verb}}`, ...args),
}});
for (const name of {nouns}) if (!(name in globalThis)) (globalThis as any)[name] = noun(name);
const scope = [...Object.keys(names), "pumi", ...{nouns}].sort().join(", ");
export default async (args: any) => {{
  try {{
    return await (async () => ({expression}))();
  }} catch (err) {{
    if (err instanceof ReferenceError) {{
      throw new ReferenceError(`${{err.message}}. In scope: ${{scope}}; any function by name: call("tab.cells", {{ tab: "%3" }})`);
    }}
    throw err;
  }}
}};
"#
    )
}

pub fn main(address: &Address, args: &[String], json: bool) -> Result<i32, Error> {
    let mut file: Option<String> = None;
    let mut expression: Option<String> = None;
    let mut stdin = false;
    let mut named = Map::new();
    let mut given: Option<Value> = None;
    let mut json = json;
    let mut lasting: Option<u64> = None;
    let mut session: Option<String> = None;
    let mut tab: Option<String> = None;
    let mut all = false;
    let mut fail_fast = false;
    let mut words = args.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "-e" => expression = Some(words.next().ok_or_else(|| usage("-e needs an expression"))?.clone()),
            "-" => stdin = true,
            "--json" => json = true,
            "--all" => all = true,
            "--fail-fast" => fail_fast = true,
            "--session" => session = Some(id_of(words.next().ok_or_else(|| usage("--session needs a session"))?, '$')),
            "--tab" => tab = Some(id_of(words.next().ok_or_else(|| usage("--tab needs a tab"))?, '%')),
            "--for" => lasting = Some(crate::duration(words.next().ok_or_else(|| usage("--for needs a duration"))?)?),
            "--args" => {
                let text = words.next().ok_or_else(|| usage("--args needs JSON or @FILE"))?;
                let text = match text.strip_prefix('@') {
                    Some(path) => {
                        std::fs::read_to_string(path).map_err(|e| Error::exception(format!("{path}: {e}")))?
                    }
                    None => text.clone(),
                };
                given = Some(serde_json::from_str(&text).map_err(|e| Error::invalid_params(format!("--args: {e}")))?);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(0);
            }
            pair if pair.contains('=') && !pair.starts_with('-') => {
                let (k, v) = pair.split_once('=').expect("has =");
                named.insert(k.to_string(), value(v));
            }
            other if other.starts_with('-') => return Err(usage(format!("unknown option {other}"))),
            path if file.is_none() => file = Some(path.to_string()),
            extra => return Err(usage(format!("one file at a time, not {extra} too"))),
        }
    }
    let arguments = match given {
        Some(Value::Object(mut map)) => {
            map.extend(named);
            Value::Object(map)
        }
        Some(other) if named.is_empty() => other,
        Some(_) => return Err(usage("--args is an object when key=value pairs are given too")),
        None => Value::Object(named),
    };
    let temp = std::env::temp_dir().join(format!("pumi-do-{}.ts", std::process::id()));
    let script = match (file, expression, stdin) {
        (Some(file), None, false) => PathBuf::from(file),
        (None, Some(expression), false) => {
            let text = expression_script(&expression);
            std::fs::write(&temp, text).map_err(|e| Error::exception(format!("{}: {e}", temp.display())))?;
            temp.clone()
        }
        (None, None, true) => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text).map_err(|e| Error::exception(format!("stdin: {e}")))?;
            std::fs::write(&temp, text).map_err(|e| Error::exception(format!("{}: {e}", temp.display())))?;
            temp.clone()
        }
        _ => return Err(usage("give one of FILE, -e EXPRESSION or -")),
    };
    let runtime = runtime_dir()
        .ok_or_else(|| Error::new(ErrorKind::ProviderUnavailable, "pumi do: cannot find pumi's runtime directory"))?;
    let (program, be_bun) = bun().ok_or_else(|| {
        Error::new(ErrorKind::ProviderUnavailable, "pumi do: needs Bun (`bun` on the PATH, or $PUMI_BUN)")
    })?;
    let socket = address.socket();
    let mut command = std::process::Command::new(&program);
    if be_bun {
        command.env("BUN_BE_BUN", "1");
    }
    command.arg(runtime.join("app/worker/do.ts")).arg(&script).env("PUMI", &socket);
    // The script's own: what its calls are relative to (`self`, the event's session), and how its
    // handlers treat a failure. `runtime/app/worker/do.ts` reads these.
    if let Some(session) = &session {
        command.env("PUMI_SESSION", session);
    }
    if let Some(tab) = &tab {
        command.env("PUMI_TAB", tab);
    }
    command
        .env("PUMI_DO", json!(arguments).to_string())
        .env("PUMI_DO_JSON", if json { "1" } else { "0" })
        .env("PUMI_DO_FOR", lasting.unwrap_or(0).to_string())
        .env("PUMI_DO_SESSION", session.clone().unwrap_or_default())
        .env("PUMI_DO_TAB", tab.clone().unwrap_or_default())
        .env("PUMI_DO_ALL", if all { "1" } else { "0" })
        .env("PUMI_DO_FAIL_FAST", if fail_fast { "1" } else { "0" });
    let status =
        command.status().map_err(|e| Error::new(ErrorKind::ProviderUnavailable, format!("{}: {e}", program.display())));
    let _ = std::fs::remove_file(&temp);
    Ok(status?.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_expression_has_every_noun_and_says_what_is_in_scope() {
        let script = super::expression_script("tab.cells(\"%3\")");
        assert!(script.contains("return await (async () => (tab.cells(\"%3\")))();"), "{script}");
        assert!(script.contains(r#"["tab","pane","view","session","app","update"]"#), "{script}");
        assert!(script.contains("In scope: "), "a name that is not there says what is");
    }
}
