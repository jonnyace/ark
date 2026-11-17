use thiserror::Error;

/// Errors carry a stable machine `code` (for UIs and tests) and a plain-language message.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum Error {
    #[error("{0}")]
    Manifest(String),
    #[error("unsafe path {path:?}: {why}")]
    UnsafePath { path: String, why: &'static str },
    #[error("{path}: size {actual} does not match the manifest ({expected})")]
    SizeMismatch { path: String, expected: u64, actual: u64 },
    #[error("{path}: SHA-256 does not match the manifest")]
    HashMismatch { path: String },
    #[error("{0}")]
    Signature(String),
    #[error("not compatible: {0}")]
    Incompatible(String),
    #[error("search index: {0}")]
    Index(String),
    #[error("citation: {0}")]
    Citation(String),
    #[error("personal data: {0}")]
    Personal(String),
    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::Manifest(_) => "manifest",
            Error::UnsafePath { .. } => "unsafe_path",
            Error::SizeMismatch { .. } => "size",
            Error::HashMismatch { .. } => "checksum",
            Error::Signature(_) => "signature",
            Error::Incompatible(_) => "incompatible",
            Error::Index(_) => "index",
            Error::Citation(_) => "citation",
            Error::Personal(_) => "personal",
            Error::Other(_) => "other",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
