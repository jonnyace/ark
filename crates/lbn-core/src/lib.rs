//! lbn-core: the platform-independent engine of Ark.
//!
//! Everything here is pure computation over bytes and values: no files, no network, no clocks.
//! Platform adapters (`lbn-native`, `lbn-wasm` + the browser worker) do the I/O and call into this crate,
//! so the desktop app, the browser app and the CLI share one implementation of the rules.

pub mod api;
pub mod catalog;
pub mod citation;
pub mod error;
pub mod hash;
pub mod index;
pub mod install;
pub mod legacy;
pub mod manifest;
pub mod path;
pub mod personal;
pub mod rag;
pub mod signature;
pub mod text;
pub mod version;
pub mod vtt;

pub use error::{Error, Result};

/// Version of the engine command/event protocol (`api` module). Bumped on breaking changes.
pub const ENGINE_API_VERSION: u32 = 1;
/// Pack format version this engine writes and the highest it reads.
pub const PACK_FORMAT_VERSION: u32 = 1;
/// Search index format version (`index` module).
pub const INDEX_FORMAT_VERSION: u32 = 1;
/// Personal-data schema version (`personal` module).
pub const PERSONAL_SCHEMA_VERSION: u32 = 2;
