pub mod agents;
pub mod client;
pub mod completion;
pub mod help;
pub mod nouns;
pub mod plugin;
pub mod plugin_new;
pub mod run_do;
pub mod server;
pub mod skill;
use client::{Address, Client};
use pumi_api::{Error, ErrorKind};
use serde_json::{Value, json};

/// The commands `pumi` answers itself: not API functions, which are words (`pumi tab get %3`).
/// Each has its own verbs, or none when its arguments are its own words (a file, an expression).
pub const OWN: &[(&str, &[&str], &str)] = &[
    ("call", &[], "any function, by name"),
    ("completion", &["zsh", "bash", "fish"], "what a shell completes for pumi"),
    ("do", &[], "a TypeScript script, now, with its answer and status"),
    ("help", &[], "a function, an event, a block, or a noun's commands"),
    ("open", &[], "a file in a tab: `pumi open PATH [--line N]`"),
    ("plugin", &["new", "dev", "logs"], "make a plugin, run it as you edit it, read its log"),
    ("server", &["list", "stop"], "the server processes themselves"),
    ("skill", &["add", "remove", "list", "hook"], "agents' hooks, installed and run"),
    ("wait", &[], "until a tab's program ends, with its status: `tab wait --exit`"),
];

/// What `pumi` is: its own commands, and every function as words.
pub fn usage_text() -> String {
    let mut out = String::from(
        "usage: pumi COMMAND [ARGS…]\n\n  pumi NOUN VERB [ID] [ARGS…] [--flags]   every function as words (`pumi registry list fn`), its id first\n\n",
    );
    for (command, verbs, desc) in OWN {
        let mut form = format!("pumi {command}");
        if !verbs.is_empty() {
            form.push(' ');
            form.push_str(&verbs.join("|"));
        }
        out.push_str(&format!("  {form:<38}  {desc}\n"));
    }
    out.push_str(
        "
An argument is JSON when it reads as JSON (`3`, `true`, `{}`) and text when it does not. Flags are
the options of the function's last argument, in kebab case: `--title x`, `--no-focus`, `--state`.
A tab is an id (`%3`), a pane `:4`, a view `@2`, a session `$1`, a request `+4`; a bare number
takes the sigil its place asks for. After `--`, a program and its words: `pumi tab open terminal
--split right -- pi \"hi\"`.

For agents: `pumi tab list` and `pumi agent list` (what runs where), `pumi tab read %3` (its
text), `pumi agent status|wait %3`, `pumi agent ask %3 'text' --wait` and `pumi agent reply`,
`pumi help` (every command), `pumi registry list fn --names`.

Global flags: --json, --server NAME, --socket PATH, --headless, --clean, --version, --help.
",
    );
    out
}

#[derive(Default)]
struct Global {
    address: Address,
    json: bool,
    headless: bool,
    clean: bool,
    init: Option<String>,
}

pub fn main(argv: Vec<String>) -> i32 {
    let name = |s: &str| std::path::Path::new(s).file_name().and_then(|n| n.to_str()).unwrap_or(s).to_string();
    let invoked = argv.first().map(|a| name(a)).unwrap_or_default();
    // `pumi … | head` ends quietly when `head` has read enough, as other commands do.
    pumi_sys::signal::end_on_closed_pipe();
    let mut args: Vec<String> = argv.into_iter().skip(1).collect();
    if invoked == "pumi-cli" && args.first().is_some_and(|a| a == "pumi") {
        args.remove(0);
    }
    match run(args) {
        Ok(code) => code,
        Err(err) => report(&err),
    }
}

/// An error on stderr as `{kind, message}`, and its status: 2 when the command line named nothing
/// to do, 1 for every other failure.
fn report(err: &Error) -> i32 {
    let usage = err.data.as_ref().and_then(|data| data.get("usage")).and_then(Value::as_bool).unwrap_or(false);
    let mut said = json!({ "kind": err.kind.name(), "message": err.message });
    if let Some(data) = &err.data
        && !usage
    {
        said["data"] = data.clone();
    }
    eprintln!("{said}");
    if usage { 2 } else { 1 }
}

/// A usage error: the command line names nothing to do. It exits 2.
pub fn usage(message: impl std::fmt::Display) -> Error {
    Error::invalid_params(message.to_string()).with_data(json!({ "usage": true }))
}

fn run(args: Vec<String>) -> Result<i32, Error> {
    let mut global = Global::default();
    let mut rest = args.into_iter().peekable();
    while let Some(arg) = rest.peek().cloned() {
        if arg == "--" {
            rest.next();
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            rest.next();
            let (name, inline) = long.split_once('=').map_or((long, None), |(n, v)| (n, Some(v.to_string())));
            let mut value =
                || inline.clone().or_else(|| rest.next()).ok_or_else(|| usage(format!("--{name} needs a value")));
            match name {
                "json" => global.json = true,
                "version" => {
                    println!("pumi {}", pumi_proto::PUMI_VERSION);
                    return Ok(0);
                }
                "help" => {
                    println!("{}", usage_text());
                    return Ok(0);
                }
                "server" => {
                    // A name (`work`: `data/servers/work`), or a socket's path.
                    let address = value()?;
                    if address.contains('/') {
                        global.address.socket = Some(address.into());
                    } else {
                        global.address.name = Some(address);
                    }
                }
                "socket" => global.address.socket = Some(value()?.into()),
                "clean" => global.clean = true,
                "headless" => global.headless = true,
                other => return Err(usage(format!("unknown option --{other}\n{}", usage_text()))),
            }
            continue;
        }
        let Some(cluster) = arg.strip_prefix('-').filter(|c| !c.is_empty()) else { break };
        rest.next();
        let mut chars = cluster.chars().peekable();
        while let Some(flag) = chars.next() {
            let attached: String = chars.clone().collect();
            let mut value = |what: &str| -> Result<String, Error> {
                if !attached.is_empty() {
                    return Ok(attached.clone());
                }
                rest.next().ok_or_else(|| usage(format!("-{flag} needs {what}")))
            };
            match flag {
                'h' => {
                    println!("{}", usage_text());
                    return Ok(0);
                }
                'u' => {
                    global.init = Some(value("a file or NONE")?);
                    break;
                }
                other => return Err(usage(format!("unknown option -{other}\n{}", usage_text()))),
            }
        }
    }
    let rest: Vec<String> = rest.collect();
    let _headless = if global.headless {
        let server = Headless::start(global.clean, global.init.as_deref())?;
        global.address = Address { socket: Some(server.socket.clone()), name: None };
        Some(server)
    } else {
        None
    };
    if let Some(ran) = agents::run(&global.address, &rest, global.json) {
        return ran;
    }
    let first = rest.first().map(String::as_str);
    match first {
        None => {
            println!("{}", usage_text());
            Ok(0)
        }
        Some("call") => call(&global.address, &rest[1..], global.json),
        Some("completion") => completion::main(&global.address, &rest[1..]),
        Some("do") => run_do::main(&global.address, &rest[1..], global.json),
        Some("help") => help::main(&global.address, &rest[1..]),
        Some("open") => {
            let (words, line) = opened(&rest[1..])?;
            nouns::run_with(&global.address, &words, global.json, |given| filed(given, line))?
                .ok_or_else(|| unknown("open"))
        }
        Some("wait") => nouns::run(&global.address, &waited(&rest[1..])?, global.json)?.ok_or_else(|| unknown("wait")),
        Some("plugin") => plugin::main(&global.address, &rest[1..], global.json),
        Some("server") => server::main(&global.address, &rest[1..], global.json),
        Some("skill") => skill::main(&global.address, &rest[1..]),
        Some("registry") if rest.iter().any(|word| word == "--names") => names(&global.address, &rest),
        Some(word) if nouns::could_be(word) => match nouns::run(&global.address, &rest, global.json)? {
            Some(code) => Ok(code),
            None => Err(unknown(word)),
        },
        Some(word) => Err(unknown(word)),
    }
}

/// `pumi registry list KIND --names`: each entry's name and the first line of what it is, one
/// a line, instead of all of it.
fn names(address: &Address, words: &[String]) -> Result<i32, Error> {
    let words: Vec<String> = words.iter().filter(|word| *word != "--names").cloned().collect();
    let [_, list, kind, rest @ ..] = words.as_slice() else {
        return Err(usage("pumi registry list KIND --names   (`fn`, `block`, `option`, `tabMethod`, …)"));
    };
    if list != "list" || !rest.is_empty() {
        return Err(usage("--names is `pumi registry list KIND`'s, with no other flags"));
    }
    let mut client = Client::open(address, true)?;
    client.introduce("cli")?;
    let entries = client.call("registry.list", json!([kind]))?;
    let rows: Vec<(String, String)> = entries
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            let name = ["name", "id", "path"].iter().find_map(|key| entry[*key].as_str()).unwrap_or_default();
            let desc = ["desc", "description", "title"].iter().find_map(|key| entry[*key].as_str()).unwrap_or_default();
            (name.to_string(), desc.trim().lines().next().unwrap_or("").to_string())
        })
        .collect();
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    for (name, desc) in rows {
        println!("{}", format!("{name:width$}  {desc}").trim_end());
    }
    Ok(0)
}

/// What a word that names nothing means: the command it might have been.
fn unknown(word: &str) -> Error {
    usage(format!("no command `{word}`\n{}", usage_text()))
}

/// `pumi open PATH [--line N]`: `tab.open("file", {path})`, tiled beside the caller's tab
/// unless a place's flags say otherwise, as the words the generated commands take; the path is
/// the one positional left after the block, and `--line` is the file's, not the place's.
fn opened(args: &[String]) -> Result<(Vec<String>, Option<u64>), Error> {
    let mut words: Vec<String> = ["tab", "open", "file"].iter().map(|w| w.to_string()).collect();
    let mut line = None;
    let mut more = args.iter();
    while let Some(word) = more.next() {
        match word.as_str() {
            "--line" => {
                let text = more.next().ok_or_else(|| usage("--line needs a line"))?;
                let n = text
                    .parse::<u64>()
                    .ok()
                    .filter(|n| *n >= 1)
                    .ok_or_else(|| usage(format!("--line takes a line from 1, not {text}")))?;
                line = Some(n);
            }
            other => words.push(other.to_string()),
        }
    }
    if !args.iter().any(|a| a == "--kind" || a.starts_with("--kind=")) {
        words.extend(["--kind".to_string(), "tiled".to_string()]);
    }
    Ok((words, line))
}

/// `pumi open`'s positional path as `tab.open`'s `args`: `{path, line}`.
fn filed(given: &mut nouns::Given, line: Option<u64>) -> Result<(), Error> {
    let mut paths = given.positionals.split_off(1.min(given.positionals.len()));
    if paths.len() > 1 {
        return Err(usage(format!("one file at a time, not {} too", paths[1])));
    }
    let path = match paths.pop() {
        Some(Value::String(path)) => path,
        Some(other) => other.to_string(),
        None => return Err(usage("which file? (`pumi open PATH`)")),
    };
    let mut args = serde_json::Map::new();
    args.insert("path".into(), json!(whole_path(&path)?));
    if let Some(line) = line {
        args.insert("line".into(), json!(line));
    }
    given.positionals.push(Value::Object(args));
    Ok(())
}

/// `pumi wait [TAB] [--timeout SECONDS]`: `tab wait --exit`, whose answer is the program's
/// status. `--for 10m` is the same as `--timeout 600`.
fn waited(args: &[String]) -> Result<Vec<String>, Error> {
    let mut words: Vec<String> = ["tab", "wait"].iter().map(|w| w.to_string()).collect();
    let mut rest = Vec::new();
    let mut asking = false;
    let mut more = args.iter();
    while let Some(word) = more.next() {
        match word.as_str() {
            "--for" => {
                let text = more.next().ok_or_else(|| usage("--for needs a duration"))?;
                let millis = duration(text)?;
                rest.push("--timeout".to_string());
                rest.push(format!("{}", millis as f64 / 1000.0));
            }
            "--exit" | "--match" | "--regex" => {
                asking = true;
                rest.push(word.clone());
            }
            other => rest.push(other.to_string()),
        }
    }
    if !asking {
        words.push("--exit".into());
    }
    words.extend(rest);
    Ok(words)
}

/// `PATH` as a whole one: `~` is the home directory, and a relative path is from the caller's.
pub(crate) fn whole_path(path: &str) -> Result<String, Error> {
    let home = pumi_proto::paths::home_dir();
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path == "~" => home,
        None => std::path::PathBuf::from(path),
    };
    if expanded.is_absolute() {
        return Ok(expanded.display().to_string());
    }
    let cwd = std::env::current_dir().map_err(|err| Error::exception(format!("cwd: {err}")))?;
    Ok(cwd.join(expanded).display().to_string())
}

/// `pumi call NAME [JSON …]`: a function by name, each argument a JSON value when it reads as one
/// and text otherwise.
fn call(address: &Address, args: &[String], json: bool) -> Result<i32, Error> {
    let json = json || args.iter().any(|a| a == "--json");
    let words: Vec<&String> = args.iter().filter(|a| *a != "--json").collect();
    let Some((name, args)) = words.split_first() else { return Err(usage("call needs a function's name")) };
    let args: Vec<Value> = args.iter().map(|word| text(word.as_str())).collect();
    let mut client = Client::open(address, true)?;
    client.introduce("cli")?;
    match client.call("call", json!([name, args]))? {
        Value::Null => {}
        Value::String(text) if !json => println!("{text}"),
        answer => println!("{answer}"),
    }
    Ok(0)
}

/// A word of a call as an argument: JSON when it reads as one (`3`, `true`, `[1]`), else the text.
fn text(word: &str) -> Value {
    serde_json::from_str(word).unwrap_or_else(|_| Value::String(word.to_string()))
}

/// A duration — `30s`, `10m`, `2h`, `250ms`, or seconds — in milliseconds.
pub fn duration(text: &str) -> Result<u64, Error> {
    let bad = || usage(format!("{text:?} is not a duration (`30s`, `10m`, `2h`, `250ms`)"));
    let (number, unit) =
        text.find(|c: char| !c.is_ascii_digit() && c != '.').map_or((text, ""), |at| text.split_at(at));
    let n: f64 = number.parse().map_err(|_| bad())?;
    let ms = match unit {
        "" | "s" => n * 1000.0,
        "ms" => n,
        "m" => n * 60_000.0,
        "h" => n * 3_600_000.0,
        _ => return Err(bad()),
    };
    Ok(ms.round().max(1.0) as u64)
}

struct Headless {
    child: std::process::Child,
    socket: std::path::PathBuf,
    /// With `--clean`: the directories it uses instead of yours, removed when it stops.
    home: Option<std::path::PathBuf>,
}

/// Points pumi's config, data, state and cache directories (and the server's, which inherits
/// them) at `home`: a clean server reads none of yours and writes none of yours.
fn isolate(home: &std::path::Path) -> Result<(), Error> {
    for (var, sub) in [
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_STATE_HOME", "state"),
        ("XDG_CACHE_HOME", "cache"),
    ] {
        let dir = home.join(sub);
        std::fs::create_dir_all(&dir).map_err(|e| Error::exception(format!("{}: {e}", dir.display())))?;
        // Before any thread starts: the CLI is single-threaded here.
        unsafe { std::env::set_var(var, &dir) };
    }
    unsafe { std::env::remove_var(pumi_proto::paths::DATA_DIR_ENV) };
    Ok(())
}

impl Headless {
    fn start(clean: bool, init: Option<&str>) -> Result<Headless, Error> {
        use std::io::BufRead;
        let home = if clean {
            let home = std::env::temp_dir().join(format!("pumi-headless-{}", std::process::id()));
            isolate(&home)?;
            Some(home)
        } else {
            None
        };
        let program = client::pumid()
            .ok_or_else(|| Error::new(ErrorKind::ProviderUnavailable, "cannot find pumid to start a server"))?;
        let mut command = std::process::Command::new(&program);
        command.arg("--headless");
        if clean {
            command.arg("--clean");
        }
        if let Some(init) = init {
            command.args(["-u", init]);
        }
        let log = if std::env::var_os("PUMI_DEBUG").is_some() {
            std::process::Stdio::inherit()
        } else {
            std::process::Stdio::null()
        };
        let mut child = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(log)
            .spawn()
            .map_err(|err| Error::new(ErrorKind::ProviderUnavailable, format!("{}: {err}", program.display())))?;
        let mut line = String::new();
        let stdout = child.stdout.take().expect("piped");
        std::io::BufReader::new(stdout).read_line(&mut line).map_err(client::io_error)?;
        let socket = std::path::PathBuf::from(line.trim());
        if line.trim().is_empty() {
            let _ = child.kill();
            return Err(Error::new(ErrorKind::ProviderUnavailable, "the headless server did not start"));
        }
        HEADLESS.store(child.id() as i32, std::sync::atomic::Ordering::Relaxed);
        Ok(Headless { child, socket, home })
    }
}

pub static HEADLESS: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

impl Drop for Headless {
    fn drop(&mut self) {
        HEADLESS.store(0, std::sync::atomic::Ordering::Relaxed);
        let _ = pumi_sys::process::signal(self.child.id(), pumi_sys::pty::Signal::Term, false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut ended = false;
        while !ended && std::time::Instant::now() < deadline {
            ended = matches!(self.child.try_wait(), Ok(Some(_)));
            if !ended {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        if !ended {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(home) = &self.home {
            let _ = std::fs::remove_dir_all(home);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_duration_is_milliseconds() {
        assert_eq!(super::duration("30s").unwrap(), 30_000);
        assert_eq!(super::duration("10m").unwrap(), 600_000);
        assert_eq!(super::duration("2h").unwrap(), 7_200_000);
        assert_eq!(super::duration("250ms").unwrap(), 250);
        assert_eq!(super::duration("2").unwrap(), 2000);
        assert!(super::duration("soon").is_err());
    }
    #[test]
    fn a_path_is_whole() {
        let home = pumi_proto::paths::home_dir();
        assert_eq!(super::whole_path("~/a.md").unwrap(), home.join("a.md").display().to_string());
        assert_eq!(super::whole_path("/tmp/a.md").unwrap(), "/tmp/a.md");
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(super::whole_path("a.md").unwrap(), cwd.join("a.md").display().to_string());
    }
}
