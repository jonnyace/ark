//! lbn-native: the native (desktop/CLI) platform adapter around `lbn-core`.
//!
//! Data directory layout (all writable data; the application itself is read-only):
//! ```text
//! <data>/registry.sqlite      installed builds, active pointers, jobs
//! <data>/personal.sqlite      bookmarks, notes, history, saved answers, settings
//! <data>/backups/             personal-data backups made before every schema migration
//! <data>/packs/<id>/<ver>+<hash16>/   installed builds, never modified after activation
//! <data>/staging/<job>/       downloads and imports in progress
//! <data>/trash/               removed builds waiting for deletion
//! ```
pub mod ai;
pub mod build;
pub mod engine;
pub mod fetch;
pub mod legacy_import;
pub mod personal_db;
pub mod store;
pub mod unpack;
pub mod util;

pub use engine::NativeEngine;
pub use store::Store;

#[derive(Debug, thiserror::Error)]
pub enum NativeError {
    #[error(transparent)]
    Core(#[from] lbn_core::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("network: {0}")]
    Network(String),
    #[error("{1}")]
    Coded(&'static str, String),
}

impl NativeError {
    pub fn code(&self) -> &'static str {
        match self {
            NativeError::Core(e) => e.code(),
            NativeError::Io(e) if e.raw_os_error() == Some(28) => "no_space",
            NativeError::Io(_) => "io",
            NativeError::Db(_) => "db",
            NativeError::Network(_) => "network",
            NativeError::Coded(c, _) => c,
        }
    }
    pub fn coded(code: &'static str, msg: impl Into<String>) -> Self {
        NativeError::Coded(code, msg.into())
    }
}

pub type Result<T> = std::result::Result<T, NativeError>;
