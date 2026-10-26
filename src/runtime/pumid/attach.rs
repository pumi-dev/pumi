use crate::runtime::{AttachMode, Attached, TerminalInput, TerminalLink, TerminalOutput};
use crate::term::Size;
use pumi_proto::client::{AttachStream, AttachWriter};
use pumi_proto::native::{AttachClient, AttachServer, Mode, Size as WireSize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
fn wire(size: Size) -> WireSize {
    WireSize { cols: size.cols, rows: size.rows, cell_width: size.cell_width, cell_height: size.cell_height }
}
pub fn attach(
    socket: &std::path::Path,
    tab: &str,
    mode: AttachMode,
    size: Size,
    budget: Option<u64>,
    resume: Option<u64>,
) -> Result<Attached, String> {
    let mode = match mode {
        AttachMode::Control => Mode::Control,
        AttachMode::Observe => Mode::Observe,
    };
    let (stream, writer) =
        pumi_proto::client::attach(socket, "pumi", env!("CARGO_PKG_VERSION"), tab, mode, wire(size), budget, resume)
            .map_err(|err| err.to_string())?;
    let seq = Arc::new(AtomicU64::new(resume.unwrap_or(0)));
    Ok(Attached {
        link: Box::new(Link(writer)),
        output: Box::new(Output { stream, ended: false, seq: seq.clone() }),
        seq,
    })
}
struct Link(AttachWriter);
impl TerminalLink for Link {
    fn send(&mut self, input: TerminalInput) {
        let msg = match input {
            TerminalInput::Bytes(bytes) => AttachClient::Input(bytes),
            TerminalInput::Resize(size) => AttachClient::Resize(wire(size)),
        };
        let _ = self.0.send(&msg);
    }
}
impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.0.send(&AttachClient::Release);
        self.0.close();
    }
}
struct Output {
    stream: AttachStream,
    ended: bool,
    seq: Arc<AtomicU64>,
}
fn translate(msg: AttachServer, more: bool) -> Option<TerminalOutput> {
    Some(match msg {
        AttachServer::Snapshot { cols, rows, bytes, .. } => {
            TerminalOutput::Snapshot { width: cols, height: rows, bytes, more }
        }
        AttachServer::Output { bytes, .. } => TerminalOutput::Frame { width: 0, height: 0, bytes, more },
        AttachServer::Resized { cols, rows } => {
            TerminalOutput::Frame { width: cols, height: rows, bytes: Vec::new(), more }
        }
        AttachServer::ControlLost => TerminalOutput::Ended { taken_over: true },
        AttachServer::ControlGained => return None,
        AttachServer::Exited { .. } | AttachServer::Error(_) => TerminalOutput::Ended { taken_over: false },
    })
}
impl Iterator for Output {
    type Item = TerminalOutput;
    fn next(&mut self) -> Option<TerminalOutput> {
        if self.ended {
            return None;
        }
        loop {
            let Some(msg) = self.stream.recv() else {
                self.ended = true;
                return Some(TerminalOutput::Lost);
            };
            if let AttachServer::Snapshot { seq, .. } | AttachServer::Output { seq, .. } = &msg {
                self.seq.store(*seq, Ordering::Release);
            }
            if let Some(output) = translate(msg, self.stream.pending()) {
                if matches!(output, TerminalOutput::Ended { .. }) {
                    self.ended = true;
                }
                return Some(output);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stream_messages_become_session_output() {
        let snapshot = AttachServer::Snapshot { cols: 4, rows: 2, bytes: b"x".to_vec(), seq: 1, mode: Mode::Control };
        assert_eq!(
            translate(snapshot, false),
            Some(TerminalOutput::Snapshot { width: 4, height: 2, bytes: b"x".to_vec(), more: false })
        );
        assert_eq!(
            translate(AttachServer::Output { seq: 2, bytes: b"y".to_vec() }, true),
            Some(TerminalOutput::Frame { width: 0, height: 0, bytes: b"y".to_vec(), more: true })
        );
        assert_eq!(translate(AttachServer::ControlLost, false), Some(TerminalOutput::Ended { taken_over: true }));
        assert_eq!(translate(AttachServer::ControlGained, false), None);
        assert_eq!(
            translate(AttachServer::Exited { status: Some(0) }, false),
            Some(TerminalOutput::Ended { taken_over: false })
        );
    }
}
