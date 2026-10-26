use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
pub type Result<T, E = Error> = std::result::Result<T, E>;
/// What went wrong, in the API's own kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The arguments do not fit the function: wrong count, unknown name, wrong type.
    InvalidParams,
    /// The function, or an object an argument names, does not exist.
    NotFound,
    /// A target or name matched more than one object; `data.candidates` lists them.
    Ambiguous,
    /// A hook or the user cancelled the call.
    Cancelled,
    /// The call ran past its budget.
    Timeout,
    /// The function's provider (a plugin, a client) is not connected.
    ProviderUnavailable,
    /// Anything else: the function itself failed.
    Exception,
}
impl ErrorKind {
    pub const ALL: [ErrorKind; 7] = [
        ErrorKind::InvalidParams,
        ErrorKind::NotFound,
        ErrorKind::Ambiguous,
        ErrorKind::Cancelled,
        ErrorKind::Timeout,
        ErrorKind::ProviderUnavailable,
        ErrorKind::Exception,
    ];
    pub fn name(self) -> &'static str {
        match self {
            ErrorKind::InvalidParams => "invalid_params",
            ErrorKind::NotFound => "not_found",
            ErrorKind::Ambiguous => "ambiguous",
            ErrorKind::Cancelled => "cancelled",
            ErrorKind::Timeout => "timeout",
            ErrorKind::ProviderUnavailable => "provider_unavailable",
            ErrorKind::Exception => "exception",
        }
    }
    pub fn from_name(name: &str) -> Option<ErrorKind> {
        ErrorKind::ALL.into_iter().find(|kind| kind.name() == name)
    }
    /// The JSON-RPC 2.0 error code. `invalid_params` is the standard one; the rest are in the
    /// range the spec leaves to servers.
    pub fn rpc_code(self) -> i64 {
        match self {
            ErrorKind::InvalidParams => -32602,
            ErrorKind::Exception => -32000,
            ErrorKind::NotFound => -32001,
            ErrorKind::Ambiguous => -32002,
            ErrorKind::Cancelled => -32003,
            ErrorKind::Timeout => -32004,
            ErrorKind::ProviderUnavailable => -32005,
        }
    }
    pub fn from_rpc_code(code: i64) -> ErrorKind {
        match code {
            -32602 | -32600 | -32700 => ErrorKind::InvalidParams,
            -32601 | -32001 => ErrorKind::NotFound,
            -32002 => ErrorKind::Ambiguous,
            -32003 => ErrorKind::Cancelled,
            -32004 => ErrorKind::Timeout,
            -32005 => ErrorKind::ProviderUnavailable,
            _ => ErrorKind::Exception,
        }
    }
    /// The CLI's exit status (2 invalid params, 3 not found or ambiguous, 4 unavailable,
    /// 130 cancelled).
    pub fn exit_code(self) -> i32 {
        match self {
            ErrorKind::InvalidParams => 2,
            ErrorKind::NotFound | ErrorKind::Ambiguous => 3,
            ErrorKind::ProviderUnavailable => 4,
            ErrorKind::Cancelled => 130,
            ErrorKind::Timeout | ErrorKind::Exception => 1,
        }
    }
}
impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
    /// Extra detail for the caller: an ambiguous target's `candidates`, say.
    pub data: Option<Value>,
}
impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error { kind, message: message.into(), data: None }
    }
    pub fn with_data(mut self, data: Value) -> Error {
        self.data = Some(data);
        self
    }
    pub fn invalid_params(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::InvalidParams, message)
    }
    pub fn not_found(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::NotFound, message)
    }
    /// An ambiguous name: nothing was changed, and `candidates` says what it could have meant.
    pub fn ambiguous(message: impl Into<String>, candidates: Vec<String>) -> Error {
        Error::new(ErrorKind::Ambiguous, message).with_data(serde_json::json!({ "candidates": candidates }))
    }
    pub fn exception(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Exception, message)
    }
    pub fn exit_code(&self) -> i32 {
        self.kind.exit_code()
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for Error {}
