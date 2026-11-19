//! Browser bindings of `lbn-core`. Values cross the boundary as JSON strings (or bytes), so the TypeScript side uses
//! the same generated types as the desktop (apps/web/src/lib/engine/gen). Errors are thrown as JSON
//! `{"code": "...", "message": "..."}` strings.
use lbn_core::citation::Citation;
use lbn_core::index::{search_many, Index};
use lbn_core::install::{self, ActivationOptions, InstalledPack};
use lbn_core::manifest::{self, FileEntry, Manifest};
use lbn_core::rag::{self, RetrievalConfig, Source, Template};
use lbn_core::{personal, signature};
use serde::Serialize;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

fn js_err(e: lbn_core::Error) -> JsValue {
    JsValue::from_str(&serde_json::json!({"code": e.code(), "message": e.to_string()}).to_string())
}
fn bad(msg: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&serde_json::json!({"code": "bad_request", "message": msg.to_string()}).to_string())
}
fn to_json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).expect("serialisable")
}
fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, JsValue> {
    serde_json::from_str(s).map_err(bad)
}

#[wasm_bindgen]
pub fn versions() -> String {
    serde_json::json!({
        "engine": env!("CARGO_PKG_VERSION"), "api": lbn_core::ENGINE_API_VERSION, "pack_format": lbn_core::PACK_FORMAT_VERSION,
        "index_format": lbn_core::INDEX_FORMAT_VERSION, "personal_schema": lbn_core::PERSONAL_SCHEMA_VERSION
    })
    .to_string()
}

/// Parse and validate `pack.json` bytes. Returns `{"manifest": ..., "sha256": "...", "dir": "<version>+<hash16>"}`.
#[wasm_bindgen]
pub fn parse_manifest(bytes: &[u8]) -> Result<String, JsValue> {
    let p = manifest::parse(bytes).map_err(js_err)?;
    Ok(serde_json::json!({"manifest": p.manifest, "sha256": p.sha256, "dir": p.install_dir_name()}).to_string())
}

/// Verify a minisign signature over the manifest bytes; returns the trusted key's name.
#[wasm_bindgen]
pub fn verify_signature(data: &[u8], sig: &str, keys_json: &str) -> Result<String, JsValue> {
    let keys: Vec<signature::TrustedKey> = from_json(keys_json)?;
    signature::verify(data, sig, &keys).map(|k| k.name.clone()).map_err(js_err)
}

#[wasm_bindgen]
pub fn sha256_hex(data: &[u8]) -> String {
    lbn_core::hash::sha256_hex(data)
}

/// Streaming SHA-256 for large files read in chunks.
#[wasm_bindgen]
pub struct Hasher(lbn_core::hash::Hasher);

#[wasm_bindgen]
impl Hasher {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Hasher {
        Hasher(lbn_core::hash::Hasher::new())
    }
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data)
    }
    pub fn bytes(&self) -> f64 {
        self.0.bytes() as f64
    }
    pub fn finish(self) -> String {
        self.0.finish_hex()
    }
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

/// `staged_json` = {"path": size, ...}; returns [[FileEntry, FileAction], ...].
#[wasm_bindgen]
pub fn install_plan(manifest_json: &str, staged_json: &str) -> Result<String, JsValue> {
    let m: Manifest = from_json(manifest_json)?;
    let staged: HashMap<String, u64> = from_json(staged_json)?;
    let plan = install::plan(&m, &staged);
    Ok(serde_json::json!({"plan": plan, "space": install::space_needed(&plan)}).to_string())
}

#[wasm_bindgen]
pub fn check_file(entry_json: &str, size: f64, sha256: &str) -> Result<(), JsValue> {
    let e: FileEntry = from_json(entry_json)?;
    install::check_file(&e, size as u64, sha256).map_err(js_err)
}

#[wasm_bindgen]
pub fn check_path(path: &str) -> Result<(), JsValue> {
    lbn_core::path::check_rel_path(path).map(|_| ()).map_err(js_err)
}

#[wasm_bindgen]
pub fn can_activate(manifest_json: &str, current_json: &str, allow_downgrade: bool) -> Result<(), JsValue> {
    let m: Manifest = from_json(manifest_json)?;
    let cur: Option<InstalledPack> = from_json(current_json)?;
    install::can_activate(&m, cur.as_ref(), ActivationOptions { allow_downgrade }).map_err(js_err)
}

/// Check an `lbn:` citation against a manifest; returns the parsed citation.
#[wasm_bindgen]
pub fn validate_citation(uri: &str, manifest_json: &str) -> Result<String, JsValue> {
    let c = Citation::parse(uri).map_err(js_err)?;
    let m: Manifest = from_json(manifest_json)?;
    c.validate_against(&m).map_err(js_err)?;
    Ok(to_json(&c))
}

#[wasm_bindgen]
pub fn parse_citation(uri: &str) -> Result<String, JsValue> {
    Citation::parse(uri).map(|c| to_json(&c)).map_err(js_err)
}

#[wasm_bindgen]
pub fn citation_uri(citation_json: &str) -> Result<String, JsValue> {
    let c: Citation = from_json(citation_json)?;
    Ok(c.to_uri())
}

/// The installed search indexes of this device (one per active pack).
#[wasm_bindgen]
pub struct Library {
    indexes: HashMap<String, Index>,
}

#[wasm_bindgen]
impl Library {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Library {
        Library { indexes: HashMap::new() }
    }
    /// Load a pack's index (bytes already checked against the manifest hash by the caller).
    pub fn add(&mut self, pack: &str, bytes: &[u8]) -> Result<u32, JsValue> {
        let ix = Index::from_bytes(bytes).map_err(js_err)?;
        if ix.pack() != pack {
            return Err(bad("index belongs to another pack"));
        }
        let n = ix.chunk_count() as u32;
        self.indexes.insert(pack.to_string(), ix);
        Ok(n)
    }
    pub fn remove(&mut self, pack: &str) {
        self.indexes.remove(pack);
    }
    pub fn has(&self, pack: &str) -> bool {
        self.indexes.contains_key(pack)
    }
    /// `packs_json`: null or ["pack-id", ...]. Returns Hit[].
    pub fn search(&self, query: &str, limit: u32, packs_json: &str) -> Result<String, JsValue> {
        let only: Option<Vec<String>> = from_json(packs_json)?;
        let refs: Vec<&Index> =
            self.indexes.iter().filter(|(id, _)| only.as_ref().is_none_or(|o| o.contains(id))).map(|(_, ix)| ix).collect();
        Ok(to_json(&search_many(&refs, query, limit.clamp(1, 100) as usize, 3)))
    }
    pub fn chunk(&self, pack: &str, chunk: u32) -> Result<String, JsValue> {
        let ix = self.indexes.get(pack).ok_or_else(|| bad("pack not loaded"))?;
        ix.chunk(chunk).map(to_json).ok_or_else(|| bad("no such chunk"))
    }
}

impl Default for Library {
    fn default() -> Self {
        Self::new()
    }
}

/// Retrieval + prompt for a question. `hits_json` = [[Hit, fullText], ...].
/// Returns {"sources": Source[], "prompt": string, "stop": string[]} or {"sources": null} when evidence is too weak.
/// The browser runtime takes one prompt string, so markup and text are joined here; plain text has been neutralised
/// (no template markers can survive in it).
#[wasm_bindgen]
pub fn prepare_answer(question: &str, hits_json: &str, template: &str) -> Result<String, JsValue> {
    let hits: Vec<(lbn_core::index::Hit, String)> = from_json(hits_json)?;
    let tpl = Template::from_id(template).ok_or_else(|| bad("unknown prompt template"))?;
    match rag::select_sources(question, &hits, &RetrievalConfig::default()) {
        None => Ok(r#"{"sources":null}"#.into()),
        Some(sources) => {
            let segs = rag::build_prompt(tpl, question, &sources);
            let prompt: String = segs.iter().map(|s| s.0.as_str()).collect();
            Ok(serde_json::json!({"sources": sources, "prompt": prompt, "stop": tpl.stop()}).to_string())
        }
    }
}

#[wasm_bindgen]
pub fn check_answer(raw: &str, sources_json: &str) -> Result<String, JsValue> {
    let sources: Vec<Source> = from_json(sources_json)?;
    Ok(to_json(&rag::check_answer(raw, &sources)))
}

/// Why a checked answer (JSON from `check_answer`) deserves one more generation, or null. See `rag::retry_reason`.
#[wasm_bindgen]
pub fn retry_reason(question: &str, checked_json: &str) -> Result<Option<String>, JsValue> {
    let checked: rag::CheckedAnswer = from_json(checked_json)?;
    Ok(rag::retry_reason(question, &checked).map(String::from))
}

/// The retry's sources and prompt, in the same shape as `prepare_answer`, or {"sources": null} when a retry
/// could not do better. `reason` comes from `retry_reason`.
#[wasm_bindgen]
pub fn prepare_retry(question: &str, reason: &str, sources_json: &str, template: &str) -> Result<String, JsValue> {
    let sources: Vec<Source> = from_json(sources_json)?;
    let tpl = Template::from_id(template).ok_or_else(|| bad("unknown prompt template"))?;
    let Some(again) = rag::sources_for_retry(question, reason, &sources) else {
        return Ok(r#"{"sources":null}"#.into());
    };
    let prompt: String = rag::build_prompt(tpl, question, &again).iter().map(|s| s.0.as_str()).collect();
    Ok(serde_json::json!({"sources": again, "prompt": prompt, "stop": tpl.stop()}).to_string())
}

/// Migrate stored personal data of `schema` to the current schema (refuses newer schemas).
#[wasm_bindgen]
pub fn personal_migrate(schema: u32, data_json: &str) -> Result<String, JsValue> {
    let v: serde_json::Value = from_json(data_json)?;
    personal::migrate(schema, v).map(|d| to_json(&d)).map_err(js_err)
}

#[wasm_bindgen]
pub fn personal_export(data_json: &str, now: &str, app_version: &str) -> Result<String, JsValue> {
    let d: personal::PersonalData = from_json(data_json)?;
    Ok(serde_json::to_string_pretty(&personal::export(&d, now, app_version)).unwrap())
}

/// Parse an export (any known schema), merge into `current_json`, validate; returns the merged data.
#[wasm_bindgen]
pub fn personal_import(current_json: &str, export_bytes: &[u8]) -> Result<String, JsValue> {
    let mut cur: personal::PersonalData = from_json(current_json)?;
    let ex = personal::parse_export(export_bytes).map_err(js_err)?;
    personal::merge(&mut cur, ex.data);
    personal::validate(&cur).map_err(js_err)?;
    Ok(to_json(&cur))
}

#[wasm_bindgen]
pub fn personal_validate(data_json: &str) -> Result<(), JsValue> {
    let d: personal::PersonalData = from_json(data_json)?;
    personal::validate(&d).map_err(js_err)
}

/// Parse a catalog and report whether it has expired (for finding new packs only).
#[wasm_bindgen]
pub fn parse_catalog(bytes: &[u8], today: &str) -> Result<String, JsValue> {
    let c = lbn_core::catalog::parse(bytes).map_err(js_err)?;
    let expired = lbn_core::catalog::is_expired(&c, today);
    Ok(serde_json::json!({"catalog": c, "expired": expired}).to_string())
}

/// True when the streamed answer has started to loop (stop generating).
#[wasm_bindgen]
pub fn is_looping(text: &str) -> bool {
    rag::is_looping(text)
}
