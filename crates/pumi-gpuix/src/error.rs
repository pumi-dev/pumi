//! The renderer's error: a message for whoever drives it (`crate::addon` hands it to JS as an
//! `Error`).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub reason: String,
}

impl Error {
    pub fn from_reason(reason: impl Into<String>) -> Self {
        Self { reason: reason.into() }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(reason: String) -> Self {
        Self { reason }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
