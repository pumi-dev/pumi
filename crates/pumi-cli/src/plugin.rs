//! `pumi plugin`: authoring one — a plugin to start from (`new`), one run as it is edited (`dev`),
//! and what it said (`logs`). Installing, updating and removing a plugin are the API's own:
//! `pumi plugin add`, `pumi plugin update`, `pumi plugin remove`, and `pumi registry list plugin`
//! lists what is installed.
use crate::client::{Address, Client};
use pumi_api::Error;
use std::path::PathBuf;

const USAGE: &str = "usage: pumi plugin new [DIR] [--theme default]   a plugin to start from
       pumi plugin dev DIR [--inspect]         link it, and reload it as you edit it
       pumi plugin logs NAME [-n LINES]        what it said, its last LINES (50)";

fn logs_dir() -> PathBuf {
    pumi_proto::paths::stdpath("data").expect("a stdpath").join("plugins")
}

/// Where the app keeps what a plugin said (its console, its stderr, why a run ended), by its id
/// (`owner/name`); `host` is the Bun host's own.
pub fn log_path(id: &str) -> PathBuf {
    logs_dir().join(id).join("log")
}

/// A plugin's log by its id, or by its bare name when only one plugin that has a log has it.
fn find_log(name: &str) -> Result<PathBuf, Error> {
    let exact = log_path(name);
    if exact.is_file() || name.contains('/') {
        return Ok(exact);
    }
    let found: Vec<PathBuf> = std::fs::read_dir(logs_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|owner| owner.path().join(name).join("log"))
        .filter(|path| path.is_file())
        .collect();
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => Ok(exact),
        many => {
            let ids: Vec<String> = many
                .iter()
                .filter_map(|path| path.parent()?.strip_prefix(logs_dir()).ok().map(|id| id.display().to_string()))
                .collect();
            Err(crate::usage(format!("`{name}` is {}: say which", ids.join(" or "))))
        }
    }
}

/// `pumi plugin new|dev|logs`; `pumi plugin add …` and the rest of the noun's verbs are the API's.
pub fn main(address: &Address, args: &[String], json: bool) -> Result<i32, Error> {
    match args.first().map(String::as_str) {
        Some("new") => new(&args[1..]),
        Some("dev") => dev(address, &args[1..]),
        Some("logs") => logs(&args[1..], json),
        Some(verb) => {
            let mut args = args.to_vec();
            // A directory here is the caller's: the server resolves a relative path against its
            // own directory, which is not this shell's (`./clock` would be `/clock`).
            if verb == "add"
                && let Some(src) = args.get_mut(1)
                && (src.starts_with('.') || src.starts_with('~'))
            {
                *src = crate::whole_path(src)?;
            }
            let words: Vec<String> = std::iter::once("plugin".to_string()).chain(args).collect();
            crate::nouns::run(address, &words, json)?.ok_or_else(|| crate::usage(USAGE))
        }
        None => Err(crate::usage(USAGE)),
    }
}

/// `pumi plugin new [DIR] [--theme default]`: a plugin to start from, in DIR (here by default).
fn new(args: &[String]) -> Result<i32, Error> {
    let mut dir = None;
    let mut theme = None;
    let mut more = args.iter();
    while let Some(word) = more.next() {
        match word.as_str() {
            "--theme" => {
                let name = more.next().ok_or_else(|| crate::usage("--theme needs a theme"))?;
                if name != "default" {
                    return Err(crate::usage(format!("the themes to start from are `default`, not `{name}`")));
                }
                theme = Some(name.clone());
            }
            other if other.starts_with('-') => return Err(crate::usage(format!("unknown option {other}"))),
            other if dir.is_none() => dir = Some(other.to_string()),
            extra => return Err(crate::usage(format!("one directory at a time, not {extra} too"))),
        }
    }
    let from_theme = theme.is_some();
    crate::plugin_new::new(dir.as_deref(), from_theme)
}

/// `pumi plugin dev DIR [--inspect]`: links DIR and asks the app to reload it as it changes.
fn dev(address: &Address, args: &[String]) -> Result<i32, Error> {
    let inspect = args.iter().any(|word| word == "--inspect");
    let dir = args
        .iter()
        .find(|word| !word.starts_with("--"))
        .ok_or_else(|| crate::usage("which plugin? (`pumi plugin dev DIR`)"))?;
    let mut client = Client::open(address, false)?;
    client.introduce("cli")?;
    crate::plugin_new::dev(&mut client, dir, inspect)
}

/// `pumi plugin logs NAME [-n LINES]`: what a plugin said, its last lines.
fn logs(args: &[String], json: bool) -> Result<i32, Error> {
    let mut name = None;
    let mut lines = 50;
    let mut more = args.iter();
    while let Some(word) = more.next() {
        match word.as_str() {
            "-n" => {
                let text = more.next().ok_or_else(|| crate::usage("-n needs a count"))?;
                lines = text.parse().map_err(|_| crate::usage(format!("-n takes a count, not {text}")))?;
            }
            other if other.starts_with('-') => return Err(crate::usage(format!("unknown option {other}"))),
            other if name.is_none() => name = Some(other.to_string()),
            extra => return Err(crate::usage(format!("one plugin at a time, not {extra} too"))),
        }
    }
    let name = name.ok_or_else(|| crate::usage("which plugin? (`pumi plugin logs NAME`)"))?;
    let path = find_log(&name)?;
    if json {
        let text =
            std::fs::read_to_string(&path).map_err(|err| Error::not_found(format!("{}: {err}", path.display())))?;
        println!("{}", serde_json::json!({ "plugin": name, "path": path, "lines": text.lines().collect::<Vec<_>>() }));
        return Ok(0);
    }
    let text = std::fs::read_to_string(&path).map_err(|err| Error::not_found(format!("{}: {err}", path.display())))?;
    let all: Vec<&str> = text.lines().collect();
    for line in &all[all.len().saturating_sub(lines)..] {
        println!("{line}");
    }
    Ok(0)
}
