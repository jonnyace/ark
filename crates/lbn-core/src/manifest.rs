//! The pack manifest (`pack.json`), format version 1. Specification: docs/PACK-FORMAT.md.
//!
//! The manifest is untrusted input until `validate` has passed. Its SHA-256 (over the exact bytes) identifies the pack
//! build: it names the install directory and is what a signature covers.
use crate::error::{Error, Result};
use crate::hash::is_sha256_hex;
use crate::path::{check_path_set, check_rel_path};
use crate::version::Version;
use crate::{ENGINE_API_VERSION, PACK_FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MANIFEST_NAME: &str = "pack.json";
pub const SIGNATURE_NAME: &str = "pack.json.minisig";
pub const FORMAT_NAME: &str = "lastbook-pack";
const MAX_MANIFEST_BYTES: usize = 32 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 1 << 50;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PackKind {
    Documents,
    Encyclopedia,
    Video,
    Map,
    Model,
    Mixed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum ResourceType {
    /// An HTML document (sanitised, rendered in a sandboxed frame without scripts).
    Html,
    /// A PDF, rendered with pdf.js.
    Pdf,
    /// A video (WebM VP9/Opus recommended) with an optional WebVTT transcript.
    Video,
    /// A vector map (PMTiles) with a MapLibre style.
    Map,
    /// An encyclopedia article (HTML, same rendering rules as `Html`).
    Article,
    /// A language model file (GGUF).
    Model,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Publisher {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct License {
    /// SPDX identifier or expression (`CC0-1.0`, `CC-BY-4.0`, `ODbL-1.0`, `Apache-2.0`), or `LicenseRef-...`.
    pub spdx: String,
    /// Required attribution text, shown with the resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution: Option<String>,
    /// Path of the full licence text inside the pack.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Whether the publisher may redistribute this item (`true` only when checked).
    #[serde(default)]
    pub redistributable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edition: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Compat {
    /// Lowest engine API version that can use this pack.
    pub min_engine_api: u32,
    /// Free-form note for people (e.g. "needs 4 GB RAM").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MapInfo {
    /// MapLibre style JSON inside the pack. `pmtiles://`, glyph and sprite URLs in it are pack-relative.
    pub style: String,
    /// West, south, east, north in degrees.
    pub bounds: [f64; 4],
    pub minzoom: u8,
    pub maxzoom: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct GenSettings {
    pub temperature: f32,
    pub top_k: u32,
    pub top_p: f32,
    pub min_p: f32,
    pub max_tokens: u32,
    pub seed: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ModelInfo {
    /// Human name, e.g. "Qwen2.5 1.5B Instruct".
    pub name: String,
    /// Upstream repository and revision the weights came from.
    pub upstream: String,
    pub runtime: String,
    /// Lowest runtime build known to work (llama.cpp tag).
    pub min_runtime: String,
    pub format: String,
    pub quantization: String,
    /// Tokenizer: where it comes from (e.g. "embedded in GGUF").
    pub tokenizer: String,
    /// Prompt template id understood by `rag::Template`.
    pub template: String,
    pub context_length: u32,
    pub parameters: String,
    pub defaults: GenSettings,
    /// Measured or estimated memory for model + context at `context_length`, bytes.
    pub memory_bytes: u64,
    /// Smallest total RAM the publisher recommends.
    pub min_ram_bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Resource {
    /// Stable within the pack across versions: citations and personal data refer to it.
    pub id: String,
    #[serde(rename = "type")]
    pub rtype: ResourceType,
    pub title: String,
    /// Main file.
    pub path: String,
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Overrides the pack licence for this item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<License>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pages: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map: Option<MapInfo>,
    /// Other files that belong to this resource (map glyphs, images of an article...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<String>,
    /// HTML anchors (section ids) that citations may target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<Section>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Section {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexRef {
    pub search: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    pub id: String,
    pub version: String,
    pub kind: PackKind,
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub created: String,
    pub publisher: Publisher,
    pub license: License,
    pub compat: Compat,
    pub resources: Vec<Resource>,
    pub files: Vec<FileEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<IndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelInfo>,
}

pub fn is_valid_pack_id(id: &str) -> bool {
    (3..=100).contains(&id.len())
        && id.as_bytes()[0].is_ascii_lowercase()
        && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && !id.contains("..")
        && !id.ends_with('.')
        && !id.ends_with('-')
}

pub fn is_valid_resource_id(id: &str) -> bool {
    (1..=128).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'))
}

/// A parsed manifest plus its identity (SHA-256 of the exact bytes).
#[derive(Debug, Clone)]
pub struct ParsedManifest {
    pub manifest: Manifest,
    pub sha256: String,
}

impl ParsedManifest {
    /// Directory name for this build of the pack: `<version>+<first 16 hex of the manifest hash>`.
    pub fn install_dir_name(&self) -> String {
        format!("{}+{}", self.manifest.version, &self.sha256[..16])
    }
}

/// Parse and fully validate `pack.json` bytes.
pub fn parse(bytes: &[u8]) -> Result<ParsedManifest> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Manifest("pack.json is too large".into()));
    }
    let v: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| Error::Manifest(format!("pack.json is not valid JSON: {e}")))?;
    // Check format and version before the full shape, so a future format gets a clear "incompatible" answer.
    if v.get("format").and_then(|f| f.as_str()) != Some(FORMAT_NAME) {
        return Err(Error::Manifest("not a Lastbook pack (format is not \"lastbook-pack\")".into()));
    }
    let fv = v.get("format_version").and_then(|f| f.as_u64()).unwrap_or(0);
    if fv == 0 || fv > PACK_FORMAT_VERSION as u64 {
        return Err(Error::Incompatible(format!(
            "pack format version {fv}; this app reads up to {PACK_FORMAT_VERSION}. Update the app to use this pack."
        )));
    }
    let manifest: Manifest =
        serde_json::from_value(v).map_err(|e| Error::Manifest(format!("pack.json has a wrong shape: {e}")))?;
    validate(&manifest)?;
    Ok(ParsedManifest { manifest, sha256: crate::hash::sha256_hex(bytes) })
}

pub fn validate(m: &Manifest) -> Result<()> {
    let err = |s: String| Err(Error::Manifest(s));
    if !is_valid_pack_id(&m.id) {
        return err(format!("bad pack id {:?}", m.id));
    }
    Version::parse(&m.version)?;
    if m.title.trim().is_empty() {
        return err("pack title is empty".into());
    }
    if m.compat.min_engine_api > ENGINE_API_VERSION {
        return Err(Error::Incompatible(format!(
            "pack needs engine API {}; this app has {ENGINE_API_VERSION}",
            m.compat.min_engine_api
        )));
    }
    if m.license.spdx.trim().is_empty() {
        return err("pack licence is missing".into());
    }
    // Files
    check_path_set(m.files.iter().map(|f| f.path.as_str()))?;
    let mut files: HashMap<&str, &FileEntry> = HashMap::new();
    for f in &m.files {
        if f.path == MANIFEST_NAME || f.path == SIGNATURE_NAME {
            return err(format!("{} must not be listed in files", f.path));
        }
        if !is_sha256_hex(&f.sha256) {
            return err(format!("{}: sha256 must be 64 lowercase hex digits", f.path));
        }
        if f.size > MAX_FILE_BYTES {
            return err(format!("{}: size too large", f.path));
        }
        files.insert(f.path.as_str(), f);
    }
    let need = |p: &str, what: &str| -> Result<()> {
        check_rel_path(p)?;
        if files.contains_key(p) {
            Ok(())
        } else {
            Err(Error::Manifest(format!("{what} {p:?} is not listed in files")))
        }
    };
    if let Some(f) = &m.license.file {
        need(f, "licence file")?;
    }
    if let Some(ix) = &m.index {
        need(&ix.search, "search index")?;
    }
    // Resources
    let mut ids = HashSet::new();
    for r in &m.resources {
        if !is_valid_resource_id(&r.id) {
            return err(format!("bad resource id {:?}", r.id));
        }
        if !ids.insert(r.id.as_str()) {
            return err(format!("duplicate resource id {:?}", r.id));
        }
        if r.title.trim().is_empty() {
            return err(format!("resource {} has no title", r.id));
        }
        need(&r.path, "resource file")?;
        for p in r.transcript.iter().chain(r.poster.iter()).chain(r.assets.iter()) {
            need(p, "resource file")?;
        }
        if let Some(l) = &r.license {
            if l.spdx.trim().is_empty() {
                return err(format!("resource {} has an empty licence", r.id));
            }
            if let Some(f) = &l.file {
                need(f, "licence file")?;
            }
        }
        match r.rtype {
            ResourceType::Pdf if r.pages.unwrap_or(0) == 0 => return err(format!("PDF {} has no page count", r.id)),
            ResourceType::Video if r.duration_s.is_none_or(|d| !(d > 0.0 && d.is_finite())) => {
                return err(format!("video {} has no duration", r.id))
            }
            ResourceType::Map => {
                let Some(mi) = &r.map else { return err(format!("map {} has no map info", r.id)) };
                need(&mi.style, "map style")?;
                let [w, s, e, n] = mi.bounds;
                if !(-180.0..=180.0).contains(&w) || !(-180.0..=180.0).contains(&e) || !(-90.0..=90.0).contains(&s)
                    || !(-90.0..=90.0).contains(&n) || s >= n
                {
                    return err(format!("map {} has bad bounds", r.id));
                }
                if mi.minzoom > mi.maxzoom || mi.maxzoom > 24 {
                    return err(format!("map {} has bad zoom range", r.id));
                }
            }
            _ => {}
        }
        let mut sec = HashSet::new();
        for s in &r.sections {
            if !is_valid_resource_id(&s.id) || !sec.insert(s.id.as_str()) {
                return err(format!("resource {} has a bad or duplicate section id {:?}", r.id, s.id));
            }
        }
    }
    let models = m.resources.iter().filter(|r| r.rtype == ResourceType::Model).count();
    match (m.kind, &m.model) {
        (PackKind::Model, Some(mi)) => {
            if models != 1 {
                return err("a model pack holds exactly one model resource".into());
            }
            if mi.format != "gguf" || mi.runtime != "llama.cpp" {
                return Err(Error::Incompatible(format!("model format {} / runtime {} is not supported", mi.format, mi.runtime)));
            }
            if crate::rag::Template::from_id(&mi.template).is_none() {
                return Err(Error::Incompatible(format!("prompt template {:?} is not supported", mi.template)));
            }
        }
        (PackKind::Model, None) => return err("model pack without model info".into()),
        (_, Some(_)) => return err("model info in a pack that is not a model pack".into()),
        (_, None) if models > 0 => return err("model resource in a pack that is not a model pack".into()),
        _ => {}
    }
    Ok(())
}

impl Manifest {
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
    pub fn file(&self, path: &str) -> Option<&FileEntry> {
        self.files.iter().find(|f| f.path == path)
    }
    pub fn resource(&self, id: &str) -> Option<&Resource> {
        self.resources.iter().find(|r| r.id == id)
    }
    pub fn effective_license<'a>(&'a self, r: &'a Resource) -> &'a License {
        r.license.as_ref().unwrap_or(&self.license)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::hash::sha256_hex;

    pub fn sample() -> Manifest {
        Manifest {
            format: FORMAT_NAME.into(),
            format_version: 1,
            id: "org.example.test".into(),
            version: "1.0.0".into(),
            kind: PackKind::Documents,
            title: "Test".into(),
            description: String::new(),
            language: Some("en".into()),
            created: "2026-11-16".into(),
            publisher: Publisher { name: "Example".into(), url: None },
            license: License { spdx: "CC0-1.0".into(), attribution: None, file: None, redistributable: true },
            compat: Compat { min_engine_api: 1, note: None },
            resources: vec![Resource {
                id: "doc1".into(),
                rtype: ResourceType::Html,
                title: "Doc".into(),
                path: "res/doc1.html".into(),
                media_type: "text/html".into(),
                source: None,
                license: None,
                pages: None,
                duration_s: None,
                transcript: None,
                poster: None,
                map: None,
                assets: vec![],
                sections: vec![Section { id: "boil".into(), title: "Boil".into() }],
                category: None,
                summary: None,
            }],
            files: vec![FileEntry { path: "res/doc1.html".into(), size: 3, sha256: sha256_hex(b"abc") }],
            index: None,
            model: None,
        }
    }

    #[test]
    fn valid_manifest_parses() {
        let bytes = serde_json::to_vec(&sample()).unwrap();
        let p = parse(&bytes).unwrap();
        assert_eq!(p.sha256, sha256_hex(&bytes));
        assert!(p.install_dir_name().starts_with("1.0.0+"));
    }

    #[test]
    #[allow(clippy::type_complexity)]
    fn rejects_bad_manifests() {
        let cases: Vec<(&str, Box<dyn Fn(&mut Manifest)>)> = vec![
            ("id", Box::new(|m| m.id = "Bad ID".into())),
            ("version", Box::new(|m| m.version = "x".into())),
            ("path", Box::new(|m| m.files[0].path = "../evil".into())),
            ("missing file", Box::new(|m| m.resources[0].path = "res/none.html".into())),
            ("hash", Box::new(|m| m.files[0].sha256 = "ABC".into())),
            ("dup res", Box::new(|m| m.resources.push(m.resources[0].clone()))),
            ("manifest listed", Box::new(|m| m.files.push(FileEntry { path: "pack.json".into(), size: 1, sha256: sha256_hex(b"x") }))),
            ("engine", Box::new(|m| m.compat.min_engine_api = 99)),
            ("pdf pages", Box::new(|m| m.resources[0].rtype = ResourceType::Pdf)),
            ("model info", Box::new(|m| m.kind = PackKind::Model)),
        ];
        for (name, f) in cases {
            let mut m = sample();
            f(&mut m);
            assert!(validate(&m).is_err(), "{name} should fail");
        }
    }

    #[test]
    fn future_format_is_incompatible() {
        let mut v = serde_json::to_value(sample()).unwrap();
        v["format_version"] = 2.into();
        let e = parse(&serde_json::to_vec(&v).unwrap()).unwrap_err();
        assert_eq!(e.code(), "incompatible");
        let e = parse(b"{\"format\":\"zip\"}").unwrap_err();
        assert_eq!(e.code(), "manifest");
    }
}
