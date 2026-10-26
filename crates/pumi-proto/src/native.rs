use serde::{Deserialize, Serialize};
pub const PROTOCOL: u32 = 2;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub protocol: u32,
    pub client: String,
    pub client_version: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Welcome {
    Ok { protocol: u32, server_version: String, boot_id: String, pid: u32 },
    Refused { protocol: u32, server_version: String, reason: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientMsg {
    Request { id: u64, method: String, params: String },
    Subscribe { id: u64, subscriptions: String },
    Attach(Attach),
    Ping { id: u64 },
    Watch,
}
pub const APP_EVENTS: &[&str] = &["pumi.notification"];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMsg {
    Response { id: u64, result: Result<String, ApiError> },
    Event { json: String },
    Pong { id: u64 },
    State { revision: u64, json: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}
impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for ApiError {}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Control,
    Observe,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attach {
    pub tab: String,
    pub mode: Mode,
    pub size: Size,
    pub budget: Option<u64>,
    #[serde(default)]
    pub resume: Option<u64>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachClient {
    Input(#[serde(with = "bytes")] Vec<u8>),
    Paste(String),
    Resize(Size),
    Claim,
    Release,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachServer {
    Snapshot {
        cols: u16,
        rows: u16,
        #[serde(with = "bytes")]
        bytes: Vec<u8>,
        seq: u64,
        mode: Mode,
    },
    Output {
        seq: u64,
        #[serde(with = "bytes")]
        bytes: Vec<u8>,
    },
    Resized {
        cols: u16,
        rows: u16,
    },
    ControlLost,
    ControlGained,
    Exited {
        status: Option<i32>,
    },
    Error(String),
}
mod bytes {
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::{Deserializer, Serializer};
    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        struct Bytes;
        impl<'de> Visitor<'de> for Bytes {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E: Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
                Ok(v.to_vec())
            }
            fn visit_byte_buf<E: Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
                Ok(v)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(b) = seq.next_element()? {
                    out.push(b);
                }
                Ok(out)
            }
        }
        deserializer.deserialize_byte_buf(Bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame;
    #[test]
    fn attach_messages_round_trip() {
        let msgs = vec![
            AttachServer::Snapshot { cols: 80, rows: 24, bytes: b"\x1b[?1h".to_vec(), seq: 9, mode: Mode::Control },
            AttachServer::Output { seq: 10, bytes: b"x".to_vec() },
            AttachServer::Exited { status: Some(1) },
        ];
        let mut buf = Vec::new();
        for msg in &msgs {
            frame::write(&mut buf, msg).unwrap();
        }
        let mut cursor = std::io::Cursor::new(buf);
        for msg in msgs {
            assert_eq!(frame::read::<_, AttachServer>(&mut cursor).unwrap(), Some(msg));
        }
    }
}
