//! `pumi server list|stop`: the server processes themselves — `default`, and each named one
//! (`--server NAME`, whose data is `data/servers/NAME`) — and whether one answers on its socket.
//! Everything else about pumi is a function; a server is what answers them.
use crate::client::Address;
use pumi_api::{Error, ErrorKind};
use pumi_proto::paths::{File, Paths};
use serde_json::{Value, json};
use std::path::PathBuf;

const USAGE: &str = "usage: pumi server list [--json]   the servers, and whether one answers on its socket
       pumi server stop [NAME]       stop one: `default` (`pumi server stop`), or a named one";

/// A server: its name, and its socket.
struct Entry {
    name: String,
    socket: PathBuf,
}

pub fn main(address: &Address, args: &[String], json: bool) -> Result<i32, Error> {
    let json = json || args.iter().any(|word| word == "--json");
    let words: Vec<&str> = args.iter().map(String::as_str).filter(|word| !word.starts_with("--")).collect();
    match words.as_slice() {
        ["list"] => list(address, json),
        ["stop"] => stop(address.data_dir()),
        ["stop", name] => stop(directory(name)),
        _ => Err(crate::usage(USAGE)),
    }
}

/// A server's data directory: `default` is pumi's own, and any other name is `data/servers/NAME`.
fn directory(name: &str) -> PathBuf {
    let data = Paths::resolve().data;
    match name {
        "default" | "" => data,
        name => data.join("servers").join(name),
    }
}

/// Every server pumi can know of: the default one, the named ones, and the one this `pumi`
/// was pointed at when it is none of those.
fn entries(address: &Address) -> Vec<Entry> {
    let data = Paths::resolve().data;
    let mut out = vec![Entry { name: "default".into(), socket: Paths::at(&data).file(File::PumiSocket) }];
    for entry in std::fs::read_dir(data.join("servers")).into_iter().flatten().flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        out.push(Entry { name, socket: Paths::at(entry.path()).file(File::PumiSocket) });
    }
    let here = address.socket();
    if !out.iter().any(|entry| entry.socket == here) {
        out.push(Entry { name: "here".into(), socket: here });
    }
    out
}

fn list(address: &Address, json: bool) -> Result<i32, Error> {
    let servers: Vec<(String, PathBuf, bool)> = entries(address)
        .into_iter()
        .map(|entry| {
            let running = pumi_proto::server::answers(&entry.socket);
            (entry.name, entry.socket, running)
        })
        .collect();
    if json {
        let rows: Vec<Value> = servers
            .iter()
            .map(|(name, socket, running)| json!({ "name": name, "socket": socket, "running": running }))
            .collect();
        println!("{}", Value::Array(rows));
        return Ok(0);
    }
    for (name, socket, running) in servers {
        let state = if running { "running" } else { "stopped" };
        println!("{name:<16} {state:<8} {}", socket.display());
    }
    Ok(0)
}

/// Stops the server whose data directory is `dir`: it does not come back.
fn stop(dir: PathBuf) -> Result<i32, Error> {
    let paths = Paths::at(dir);
    let socket = paths.file(File::PumiSocket);
    if !pumi_proto::server::answers(&socket) {
        return Err(Error::new(ErrorKind::ProviderUnavailable, format!("no server on {}", socket.display())));
    }
    pumi_proto::server::stop(&paths)
        .map_err(|err| Error::new(ErrorKind::Exception, format!("{}: {err}", socket.display())))?;
    println!("stopped {}", paths.data.display());
    Ok(0)
}
