use crate::kernel::{ChanId, Kernel, TabId};
use crate::server::log;
use crate::tab::terminal::{Grids, Notes, Terminal};
use crate::term::feed::Take;
use crate::term::modes::Modes;
use crate::term::{Note, Term};
use pumi_api::{Error, Result};
use pumi_proto::frame;
use pumi_proto::native::{
    Attach, AttachClient, AttachServer, ClientMsg, Hello, Mode, PROTOCOL, ServerMsg, Size, Welcome,
};
use pumi_sys::ipc::{Listener, Stream as Socket};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::Write;
use std::sync::Arc;
use std::thread;
pub fn serve(listener: Listener, grids: Grids) -> std::io::Result<()> {
    thread::Builder::new().name("pumid-grid".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let grids = grids.clone();
            let _ = thread::Builder::new().name("pumid-grid-conn".into()).spawn(move || handle(&grids, stream));
        }
    })?;
    Ok(())
}
fn handle(grids: &Grids, mut stream: Socket) {
    let Ok(Some(hello)) = frame::read::<_, Hello>(&mut stream) else { return };
    if hello.protocol != PROTOCOL {
        let _ = frame::write(
            &mut stream,
            &Welcome::Refused {
                protocol: PROTOCOL,
                server_version: pumi_proto::PUMI_VERSION.into(),
                reason: format!("client speaks protocol {}, server {PROTOCOL}", hello.protocol),
            },
        );
        return;
    }
    let welcome = Welcome::Ok {
        protocol: PROTOCOL,
        server_version: pumi_proto::PUMI_VERSION.into(),
        boot_id: String::new(),
        pid: std::process::id(),
    };
    if frame::write(&mut stream, &welcome).is_err() {
        return;
    }
    loop {
        match frame::read::<_, ClientMsg>(&mut stream) {
            Ok(Some(ClientMsg::Ping { id })) => {
                if frame::write(&mut stream, &ServerMsg::Pong { id }).is_err() {
                    return;
                }
            }
            Ok(Some(ClientMsg::Attach(attach))) => return attach_stream(grids, stream, attach, &hello.client),
            Ok(Some(_)) => {
                let _ = frame::write(
                    &mut stream,
                    &AttachServer::Error("this socket only attaches to terminals; the API is on pumi.sock".into()),
                );
                return;
            }
            _ => return,
        }
    }
}
fn attach_stream(grids: &Grids, mut stream: Socket, attach: Attach, client: &str) {
    let term = attach.tab.trim_start_matches('%').parse().ok().and_then(|tab| grids.get(tab));
    let Some(term) = term else {
        let _ = frame::write(&mut stream, &AttachServer::Error(format!("tab not found: {}", attach.tab)));
        return;
    };
    let budget = attach.budget.map_or(crate::term::feed::LOCAL_BUDGET, |b| b as usize);
    let (id, feed) = term.attach(attach.mode, attach.size, budget, attach.resume);
    let Ok(mut out) = stream.try_clone() else {
        term.detach(id);
        return;
    };
    let resync = Arc::clone(&term);
    let writer = thread::Builder::new().name(format!("pumid-attach-{}", term.tab)).spawn(move || {
        let mut batch = Vec::new();
        loop {
            let queued = match feed.take(None) {
                Take::Messages(queued) => queued,
                Take::Behind => {
                    resync.resync(id);
                    continue;
                }
                Take::Closed | Take::Idle => {
                    let _ = out.shutdown(std::net::Shutdown::Both);
                    return;
                }
            };
            let exited = queued.iter().any(|m| matches!(m, AttachServer::Exited { .. }));
            batch.clear();
            for msg in queued {
                match frame::encode(&msg) {
                    Ok(bytes) => batch.extend_from_slice(&bytes),
                    Err(_) => return,
                }
            }
            if out.write_all(&batch).is_err() {
                return;
            }
            if exited {
                let _ = out.shutdown(std::net::Shutdown::Write);
            }
        }
    });
    if writer.is_err() {
        term.detach(id);
        return;
    }
    loop {
        let msg = match frame::read::<_, AttachClient>(&mut stream) {
            Ok(Some(msg)) => msg,
            Ok(None) => break,
            Err(err) => {
                log(&format!("{client} on %{}: {err}", term.tab));
                break;
            }
        };
        let result = match msg {
            AttachClient::Input(bytes) => {
                if !term.is_controller(id) {
                    term.claim(id);
                }
                term.write(&bytes)
            }
            AttachClient::Paste(text) => term.paste(&text),
            AttachClient::Resize(size) => {
                term.client_resize(id, size);
                Ok(())
            }
            AttachClient::Claim => {
                term.claim(id);
                Ok(())
            }
            AttachClient::Release => {
                term.release(id);
                Ok(())
            }
        };
        if result.is_err() {
            break;
        }
    }
    term.detach(id);
}
/// The streams attached over JSON-RPC (`tab.call(…, "attach")`), by id: each a client of its
/// tab's terminal whose frames go to the channel that attached it, as `tab.stream`
/// notifications. A stream ends with its channel, its `detach`, or its program.
#[derive(Default)]
pub struct Streams {
    next: u64,
    open: std::collections::BTreeMap<u64, Stream>,
}
struct Stream {
    chan: ChanId,
    client: u64,
    term: Arc<Term>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AttachMode {
    Control,
    Observe,
}
/// `tab.call(…, "attach")`'s arguments.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttachArgs {
    mode: AttachMode,
    /// Take control from the tab's controller; without it, a tab that has one refuses another.
    #[serde(default)]
    takeover: bool,
    #[serde(default)]
    cols: Option<u16>,
    #[serde(default)]
    rows: Option<u16>,
}
/// What a stream's owner sends it (`core.streamSend`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
enum Message {
    Input(String),
    Paste(String),
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Wheel steps at a cell: up when `lines` is negative.
    Scroll {
        lines: i32,
        col: u16,
        row: u16,
    },
    Mouse {
        action: MouseAction,
        #[serde(default)]
        button: Option<MouseButton>,
        col: u16,
        row: u16,
    },
    Detach,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MouseAction {
    Down,
    Up,
    Move,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum MouseButton {
    Left,
    Middle,
    Right,
}
fn streams_of(kernel: &mut Kernel) -> &mut Streams {
    if kernel.ext.get::<Streams>().is_none() {
        kernel.ext.insert(Streams::default());
    }
    kernel.ext.get_mut::<Streams>().expect("just inserted")
}
/// The prefix of a stream's id: `stream/1`.
const STREAM: &str = "stream/";
/// Attaches `chan` to the terminal in `tab` as a stream; answers `{stream}`. The first frame is
/// the screen as it is (`snapshot`), then its output as it comes.
pub fn open(kernel: &mut Kernel, chan: ChanId, tab: TabId, args: &Value) -> Result<Value> {
    let args: AttachArgs = serde_json::from_value(args.clone()).map_err(|err| {
        Error::invalid_params(format!("attach: {err} (`{{mode: control|observe, takeover, cols, rows}}`)"))
    })?;
    if chan == crate::kernel::SERVER {
        return Err(Error::invalid_params("the core cannot attach to its own terminals"));
    }
    let term = kernel
        .instance::<Terminal>(tab)
        .map(|t| t.term.clone())
        .ok_or_else(|| Error::invalid_params(format!("%{tab} is not a terminal: only a terminal attaches")))?;
    if args.mode == AttachMode::Control && !args.takeover && term.controller().is_some() {
        return Err(Error::invalid_params(format!("%{tab} has a controller: attach with `takeover` to replace it")));
    }
    let notes = kernel.ext.get::<Notes>().ok_or_else(|| Error::exception("this core runs no terminals"))?.0.clone();
    let now = term.size();
    let size = Size { cols: args.cols.unwrap_or(now.cols), rows: args.rows.unwrap_or(now.rows), ..now };
    let mode = match args.mode {
        AttachMode::Control => Mode::Control,
        AttachMode::Observe => Mode::Observe,
    };
    let (client, feed) = term.attach(mode, size, crate::term::feed::LOCAL_BUDGET, None);
    let streams = streams_of(kernel);
    streams.next += 1;
    let number = streams.next;
    let id = format!("{STREAM}{number}");
    streams.open.insert(number, Stream { chan, client, term: term.clone() });
    let name = id.clone();
    let pump = thread::Builder::new().name(format!("pumid-stream-{}", term.tab)).spawn(move || {
        loop {
            let queued = match feed.take(None) {
                Take::Messages(queued) => queued,
                Take::Behind => {
                    term.resync(client);
                    continue;
                }
                Take::Closed | Take::Idle => return,
            };
            let mut exited = false;
            for msg in queued {
                exited |= matches!(msg, AttachServer::Exited { .. });
                let line = pumi_api::rpc::notification("tab.stream", json!([name, frame(msg)]));
                if notes.send(Note::Stream { chan, line }).is_err() {
                    return term.detach(client);
                }
            }
            if exited {
                return term.detach(client);
            }
        }
    });
    if let Err(err) = pump {
        if let Some(stream) = streams_of(kernel).open.remove(&number) {
            stream.term.detach(stream.client);
        }
        return Err(Error::exception(format!("attaching %{tab}: {err}")));
    }
    Ok(json!({ "stream": id }))
}
/// A frame as a stream's owner reads it; bytes are base64.
fn frame(msg: AttachServer) -> Value {
    use base64::Engine as _;
    let data = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    match msg {
        AttachServer::Snapshot { cols, rows, bytes, seq, mode } => {
            let mode = if mode == Mode::Control { "control" } else { "observe" };
            json!({ "snapshot": { "cols": cols, "rows": rows, "data": data(&bytes), "seq": seq, "mode": mode } })
        }
        AttachServer::Output { seq, bytes } => json!({ "output": { "seq": seq, "data": data(&bytes) } }),
        AttachServer::Resized { cols, rows } => json!({ "resized": { "cols": cols, "rows": rows } }),
        AttachServer::ControlLost => json!({ "controlLost": {} }),
        AttachServer::ControlGained => json!({ "controlGained": {} }),
        AttachServer::Exited { status } => json!({ "exited": { "exitCode": status } }),
        AttachServer::Error(message) => json!({ "error": { "message": message } }),
    }
}
/// A message from `chan` on its stream `id`. An observer may only `detach`: what it would write
/// is refused before a byte reaches the program.
pub fn send(kernel: &mut Kernel, chan: ChanId, id: &str, message: Value) -> Result<()> {
    let number = id.strip_prefix(STREAM).and_then(|n| n.parse::<u64>().ok());
    let streams = streams_of(kernel);
    let stream = number
        .and_then(|n| streams.open.get(&n).map(|s| (n, s)))
        .filter(|(_, s)| s.chan == chan)
        .ok_or_else(|| Error::not_found(format!("no stream {id} of this connection")))?;
    let (number, client, term) = (stream.0, stream.1.client, stream.1.term.clone());
    let message: Message = serde_json::from_value(message).map_err(|err| {
        Error::invalid_params(format!("{id}: {err} (`input`, `paste`, `resize`, `scroll`, `mouse` or `detach`)"))
    })?;
    if matches!(message, Message::Detach) {
        streams.open.remove(&number);
        term.detach(client);
        return Ok(());
    }
    if !term.is_controller(client) {
        return Err(Error::invalid_params(format!("{id} observes %{}: an observer cannot write", term.tab)));
    }
    let io = |r: std::io::Result<()>| r.map_err(|err| Error::exception(format!("%{}: {err}", term.tab)));
    match message {
        Message::Input(text) => io(term.write(text.as_bytes())),
        Message::Paste(text) => io(term.paste(&text)),
        Message::Resize { cols, rows } => {
            term.client_resize(client, Size { cols, rows, ..term.size() });
            Ok(())
        }
        Message::Scroll { lines, col, row } => {
            let button = if lines < 0 { WHEEL_UP } else { WHEEL_DOWN };
            let one = mouse_bytes(&term.modes(), Some(button), MouseAction::Down, col, row);
            io(term.write(&one.repeat(lines.unsigned_abs() as usize)))
        }
        Message::Mouse { action, button, col, row } => {
            let code = button.map(|b| match b {
                MouseButton::Left => 0,
                MouseButton::Middle => 1,
                MouseButton::Right => 2,
            });
            io(term.write(&mouse_bytes(&term.modes(), code, action, col, row)))
        }
        Message::Detach => Ok(()),
    }
}
const WHEEL_UP: u8 = 64;
const WHEEL_DOWN: u8 = 65;
/// A mouse report as the program asked for them (DEC 1000, 1002, 1003; SGR with 1006), from a
/// 0-based cell; nothing when it tracks no mouse, or not this motion.
fn mouse_bytes(modes: &Modes, button: Option<u8>, action: MouseAction, col: u16, row: u16) -> Vec<u8> {
    let (all, drag, press) = (modes.get(1003), modes.get(1002), modes.get(1000));
    let reported = match action {
        MouseAction::Move => all || (drag && button.is_some()),
        _ => all || drag || press,
    };
    if !reported {
        return Vec::new();
    }
    let motion: u8 = if action == MouseAction::Move { 32 } else { 0 };
    let (x, y) = (u32::from(col) + 1, u32::from(row) + 1);
    if modes.get(1006) {
        let code = button.unwrap_or(3) + motion;
        let end = if action == MouseAction::Up { 'm' } else { 'M' };
        return format!("\x1b[<{code};{x};{y}{end}").into_bytes();
    }
    let code = match action {
        MouseAction::Up if button.is_none_or(|b| b < WHEEL_UP) => 3,
        _ => button.unwrap_or(3),
    } + motion;
    let cell = |n: u32| (n.min(223) + 32) as u8;
    vec![0x1b, b'[', b'M', code + 32, cell(x), cell(y)]
}
/// A channel closed: its streams go.
pub fn chan_closed(kernel: &mut Kernel, chan: ChanId) {
    let Some(streams) = kernel.ext.get_mut::<Streams>() else { return };
    streams.open.retain(|_, stream| {
        if stream.chan == chan {
            stream.term.detach(stream.client);
        }
        stream.chan != chan
    });
}
