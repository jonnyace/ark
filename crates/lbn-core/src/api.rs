//! The engine protocol, version `ENGINE_API_VERSION`. Specification: docs/ENGINE-API.md.
//!
//! Every client (desktop UI over Tauri IPC, browser UI over a Web Worker, future native mobile apps) sends
//! `Request`s and receives `Reply`s plus `Event`s, all JSON. Commands are typed here once; TypeScript types are
//! generated from these definitions (`cargo test -p lbn-core --features ts export_bindings`).
//!
//! Compatibility rule: within an API version, new optional fields and new commands/events may be added; clients must
//! ignore unknown fields and events. Removing or changing a field's meaning requires a new API version.
use crate::citation::Citation;
use crate::index::Hit;
use crate::install::{InstalledPack, JobProgress};
use crate::legacy::Unsupported;
use crate::manifest::{License, MapInfo, ModelInfo, PackKind, ResourceType, Section, Source};
use crate::personal::PersonalData;
use crate::rag::CheckedAnswer;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ts")]
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Request {
    /// API version the client speaks.
    pub v: u32,
    /// Client-chosen id echoed in the reply.
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
pub enum Command {
    Hello { client: String },
    ListPacks,
    ListResources { pack: Option<String> },
    GetResource { pack: String, resource: String },
    /// Fetch and check a publisher's pack catalog (signed list of available packs).
    ReadCatalog { url: String },
    /// Check a pack source (URL of a pack directory or catalog, or a local file the adapter can read) without installing.
    InspectSource { source: PackSource },
    Install { source: PackSource, allow_unsigned: bool, allow_downgrade: bool },
    PauseJob { job: String },
    ResumeJob { job: String },
    CancelJob { job: String },
    ListJobs,
    /// Re-hash every file of the active build; report what is damaged.
    VerifyPack { pack: String },
    /// Re-fetch damaged files into a new staging build and activate it once verified.
    RepairPack { pack: String },
    /// Remove a pack (all builds) or one inactive build.
    RemovePack { pack: String, version: Option<String> },
    /// Make the previous installed build active again.
    RollbackPack { pack: String },
    Search { query: String, limit: u32, packs: Option<Vec<String>> },
    GetChunk { pack: String, chunk: u32 },
    ResolveCitation { uri: String },
    AiStatus,
    LoadModel { pack: String },
    UnloadModel,
    Ask { question: String, model: Option<String> },
    CancelAsk { ask: String },
    GetPersonal,
    SaveBookmark { target: String, title: String },
    DeleteBookmark { id: String },
    SaveNote { id: Option<String>, target: String, text: String },
    DeleteNote { id: String },
    AddHistory { target: String, title: String },
    SaveSettings { settings: crate::personal::Settings },
    ExportPersonal,
    ImportPersonal { data: String },
    StorageStatus,
    RequestPersistence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PackSource {
    /// `https://.../pack.json` (or the pack directory URL). Browser: same-origin or CORS-enabled.
    Url { url: String },
    /// A `.lbpack` file or pack directory on this device (native path; browser: a picked File handled by the worker).
    Local { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Reply {
    pub v: u32,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub err: Option<ApiError>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl Reply {
    pub fn ok<T: Serialize>(id: u64, v: &T) -> Reply {
        Reply { v: crate::ENGINE_API_VERSION, id, ok: Some(serde_json::to_value(v).expect("serialisable")), err: None }
    }
    pub fn err(id: u64, code: &str, message: impl Into<String>) -> Reply {
        Reply { v: crate::ENGINE_API_VERSION, id, ok: None, err: Some(ApiError { code: code.into(), message: message.into() }) }
    }
    pub fn from_error(id: u64, e: &crate::Error) -> Reply {
        Reply::err(id, e.code(), e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum Event {
    Job(JobProgress),
    AskStatus { ask: String, status: String },
    AskToken { ask: String, text: String },
    AskDone(AskResult),
    AskError { ask: String, code: String, message: String },
    PacksChanged,
    PersonalChanged,
}

// ---------------------------------------------------------------------------------------------------- reply bodies --

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct HelloReply {
    pub api_version: u32,
    pub engine_version: String,
    /// `desktop-linux`, `desktop-windows`, `desktop-macos`, `browser`, `cli`.
    pub platform: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Capabilities {
    /// A local inference runtime is present on this device.
    pub ai_runtime: bool,
    /// Why not, in plain words, when `ai_runtime` is false.
    pub ai_unavailable_reason: Option<String>,
    /// Storage is managed by the browser and may be evicted.
    pub evictable_storage: bool,
    pub can_download: bool,
    pub can_import_files: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PackInfo {
    pub installed: InstalledPack,
    pub description: String,
    pub publisher: String,
    pub license: License,
    pub resource_count: u32,
    /// Other builds of this pack kept on the device (for rollback).
    pub other_versions: Vec<String>,
    /// Problems found when the library opened (missing files, evicted storage).
    pub health: PackHealth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PackHealth {
    Ok,
    /// Files are missing or the wrong size (e.g. browser storage evicted, disk damaged): repair or reinstall.
    Damaged,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ResourceInfo {
    pub pack: String,
    pub id: String,
    #[serde(rename = "type")]
    pub rtype: ResourceType,
    pub title: String,
    pub path: String,
    pub media_type: String,
    pub pages: Option<u32>,
    pub duration_s: Option<f64>,
    pub transcript: Option<String>,
    pub poster: Option<String>,
    pub map: Option<MapInfo>,
    pub sections: Vec<Section>,
    pub source: Option<Source>,
    pub license: License,
    pub category: Option<String>,
    pub summary: Option<String>,
    pub pack_kind: PackKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct SourceInfo {
    pub id: String,
    pub version: String,
    pub kind: PackKind,
    pub title: String,
    pub description: String,
    pub publisher: String,
    pub license: License,
    pub bytes: u64,
    pub files: u32,
    pub resources: u32,
    /// Name of the trusted key that signed the manifest; `None` = unsigned.
    pub signed_by: Option<String>,
    pub installed_version: Option<String>,
    pub model: Option<ModelInfo>,
    /// For legacy Lastbook libraries: items that will not be imported, and why.
    pub unsupported: Vec<Unsupported>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct CatalogReply {
    pub url: String,
    pub title: String,
    pub published: String,
    pub expires: String,
    /// Past its expiry date: shown, but new installs from it need confirmation. Installed packs are unaffected.
    pub expired: bool,
    /// Trusted key that signed the catalog; `None` = unsigned.
    pub signed_by: Option<String>,
    pub packs: Vec<CatalogPack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct CatalogPack {
    pub entry: crate::catalog::CatalogEntry,
    /// Absolute URL of the pack's manifest.
    pub manifest_url: String,
    pub installed_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct SearchReply {
    pub hits: Vec<Hit>,
    /// Packs that could not be searched (damaged index, missing files).
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ResolvedCitation {
    pub uri: String,
    pub valid: bool,
    pub error: Option<String>,
    pub citation: Option<Citation>,
    pub resource: Option<ResourceInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct ModelStatus {
    pub pack: String,
    pub title: String,
    pub info: ModelInfo,
    pub fits: Fit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct Fit {
    pub ok: bool,
    pub reason: String,
    pub ram_total_bytes: Option<u64>,
    pub ram_available_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AiStatus {
    pub runtime: bool,
    pub runtime_name: Option<String>,
    pub unavailable_reason: Option<String>,
    pub enabled: bool,
    pub models: Vec<ModelStatus>,
    /// Pack id of the loaded model.
    pub loaded: Option<String>,
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AskStarted {
    pub ask: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AskSource {
    pub key: String,
    pub citation: String,
    pub title: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AskResult {
    pub ask: String,
    pub question: String,
    /// `answered`, `no_evidence` (model not asked), `cancelled`.
    pub outcome: String,
    pub answer: Option<CheckedAnswer>,
    /// The sources the model was given (all citable), best first.
    pub sources: Vec<AskSource>,
    pub model: Option<String>,
    pub stats: Option<AskStats>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct AskStats {
    pub prompt_tokens: u32,
    pub generated_tokens: u32,
    pub prompt_ms: u32,
    pub gen_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct StorageStatus {
    pub used_bytes: u64,
    pub free_bytes: Option<u64>,
    pub quota_bytes: Option<u64>,
    /// Browser: `navigator.storage.persisted()`. Native: always true.
    pub persisted: bool,
    pub evictable: bool,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct VerifyReport {
    pub pack: String,
    pub version: String,
    pub ok: bool,
    pub checked: u32,
    pub damaged: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(TS), ts(export))]
pub struct PersonalReply {
    pub data: PersonalData,
    pub schema: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wire_format() {
        let r: Request = serde_json::from_str(r#"{"v":1,"id":7,"cmd":"search","args":{"query":"boil","limit":5,"packs":null}}"#).unwrap();
        assert_eq!(r.command, Command::Search { query: "boil".into(), limit: 5, packs: None });
        let r: Request = serde_json::from_str(r#"{"v":1,"id":1,"cmd":"list_packs"}"#).unwrap();
        assert_eq!(r.command, Command::ListPacks);
        let e = serde_json::to_string(&Event::AskToken { ask: "a".into(), text: "x".into() }).unwrap();
        assert_eq!(e, r#"{"event":"ask_token","data":{"ask":"a","text":"x"}}"#);
        let rep = serde_json::to_string(&Reply::err(3, "x", "y")).unwrap();
        assert_eq!(rep, r#"{"v":1,"id":3,"err":{"code":"x","message":"y"}}"#);
    }
}
