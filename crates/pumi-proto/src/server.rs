use crate::paths::{File, Paths};
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Write};
use pumi_sys::ipc::Stream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
pub const WITHIN: Duration = Duration::from_secs(20);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ensured {
    Current,
    Started,
    Updated,
}
pub fn answers(socket: &Path) -> bool {
    pumi_sys::ipc::answers(socket)
}
pub fn call(socket: &Path, method: &str, params: Value) -> io::Result<Result<Value, String>> {
    let mut stream = Stream::connect(socket)?;
    stream.set_read_timeout(Some(WITHIN))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    round_trip(&mut stream, &mut reader, method, params)
}
fn round_trip(
    stream: &mut Stream,
    reader: &mut BufReader<Stream>,
    method: &str,
    params: Value,
) -> io::Result<Result<Value, String>> {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    writeln!(stream, "{request}")?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "the server closed the connection"));
        }
        let reply: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
        if reply.get("id") != Some(&json!(1)) {
            continue;
        }
        return Ok(match reply.get("error") {
            Some(error) => Err(error["message"].as_str().unwrap_or("failed").to_string()),
            None => Ok(reply.get("result").cloned().unwrap_or_default()),
        });
    }
}
pub fn start(pumid: &Path, paths: &Paths) -> io::Result<()> {
    let out = Command::new(pumid)
        .arg("--daemon")
        .arg("--data-dir")
        .arg(&paths.data)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| io::Error::new(err.kind(), format!("cannot run {}: {err}", pumid.display())))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(io::Error::other(format!("{} exited with {}: {}", pumid.display(), out.status, why.trim())));
    }
    until("the server did not start; see its log", || answers(&paths.file(File::PumiSocket)))
}
pub fn ensure(pumid: &Path, paths: &Paths) -> io::Result<Ensured> {
    if !answers(&paths.file(File::PumiSocket)) {
        return start(pumid, paths).map(|()| Ensured::Started);
    }
    Ok(if update(pumid, paths)? { Ensured::Updated } else { Ensured::Current })
}
pub fn update(pumid: &Path, paths: &Paths) -> io::Result<bool> {
    match call(&paths.file(File::PumiSocket), "server.getImage", json!([])) {
        Ok(Ok(image)) if !outdated(&image) => Ok(false),
        Ok(Ok(_)) => restart(pumid, paths).map(|()| true),
        Ok(Err(why)) if too_old(&why) => replace(pumid, paths).map(|()| true),
        Ok(Err(_)) | Err(_) => Ok(false),
    }
}
pub fn restart(pumid: &Path, paths: &Paths) -> io::Result<()> {
    let socket = paths.file(File::PumiSocket);
    if let Ok(Err(why)) = call(&socket, "server.getImage", json!([]))
        && too_old(&why)
    {
        return replace(pumid, paths);
    }
    let mut stream = Stream::connect(&socket)?;
    stream.set_read_timeout(Some(WITHIN))?;
    writeln!(stream, "{}", json!({ "jsonrpc": "2.0", "id": 1, "method": "server.restart", "params": [] }))?;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let reply: Value = serde_json::from_str(&line).unwrap_or_default();
        if reply.get("id") == Some(&json!(1))
            && let Some(error) = reply.get("error")
        {
            return Err(io::Error::other(error["message"].as_str().unwrap_or("refused").to_string()));
        }
    }
    until("the server did not come back from its restart", || {
        call(&socket, "server.getImage", json!([])).is_ok_and(|image| image.is_ok_and(|i| !outdated(&i)))
    })
}
pub fn stop(paths: &Paths) -> io::Result<()> {
    let socket = paths.file(File::PumiSocket);
    if let Ok(Err(why)) = call(&socket, "server.kill", json!([])) {
        return Err(io::Error::other(why));
    }
    until("the server did not stop", || !answers(&socket))
}
fn replace(pumid: &Path, paths: &Paths) -> io::Result<()> {
    let socket = paths.file(File::PumiSocket);
    let chans = call(&socket, "channel.list", json!([])).ok().and_then(Result::ok).unwrap_or_default();
    let hosts: Vec<i64> = chans
        .as_array()
        .into_iter()
        .flatten()
        .filter(|chan| chan["client"]["name"] == "pumi-host")
        .filter_map(|chan| chan["client"]["attributes"]["pid"].as_i64())
        .collect();
    stop(paths)?;
    for pid in hosts {
        let _ = pumi_sys::process::signal(pid as u32, pumi_sys::pty::Signal::Term, false);
    }
    start(pumid, paths)
}
fn outdated(image: &Value) -> bool {
    image["outdated"].as_bool().unwrap_or(false)
}
fn too_old(why: &str) -> bool {
    why.contains("no function")
}
fn until(what: &str, mut done: impl FnMut() -> bool) -> io::Result<()> {
    let deadline = Instant::now() + WITHIN;
    while !done() {
        if Instant::now() > deadline {
            return Err(io::Error::new(io::ErrorKind::TimedOut, what.to_string()));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
