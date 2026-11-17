//! Import a library made by the original Lastbook (a `LastbookData` folder) as a new, unsigned pack.
//! The original folder is only read; nothing in it is changed, moved or deleted. Items that cannot be imported are
//! listed with a reason (and stay where they are).
use crate::build::{build, BuildInput, SrcResource, SrcSpec, TextOverride};
use crate::util::join_rel;
use crate::{NativeError, Result};
use lbn_core::legacy::{self, Plan, Unsupported};
use lbn_core::manifest::{self, Compat, License, PackKind, ParsedManifest, Publisher, Resource, ResourceType, Source};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The `LastbookData` folder for a path that is that folder, or holds it.
pub fn find_library(p: &Path) -> Option<PathBuf> {
    [p.to_path_buf(), p.join("LastbookData")].into_iter().find(|c| c.join("library-data/catalog.json").is_file())
}

pub struct LegacyInfo {
    pub id: String,
    pub version: String,
    pub title: String,
    pub plan: Plan,
    pub bytes: u64,
}

fn read_info(lib: &Path) -> Result<LegacyInfo> {
    let data = lib.join("library-data");
    let mut plan = legacy::plan(&std::fs::read(data.join("catalog.json"))?)?;
    // Drop items whose files are missing (a partial download, `known_gaps`) and say so.
    let mut keep = Vec::new();
    for r in plan.resources.drain(..) {
        let ok = join_rel(&data, &r.path).map(|p| p.is_file()).unwrap_or(false);
        if ok {
            keep.push(r);
        } else {
            plan.unsupported.push(Unsupported {
                id: r.id.clone(),
                kind: format!("{:?}", r.rtype).to_lowercase(),
                title: r.title.clone(),
                reason: "the file is missing from this library".into(),
            });
        }
    }
    plan.resources = keep;
    // The 1.x offline map is a custom SQLite format that this version does not read: say so, keep it in place.
    if lib.join("derived/geo").is_dir() {
        plan.unsupported.push(Unsupported {
            id: "derived/geo".into(),
            kind: "map".into(),
            title: "Offline map (Lastbook 1.x format)".into(),
            reason: "this map format is not supported; install a map pack instead. The original files are left in place".into(),
        });
    }
    let pj: serde_json::Value =
        std::fs::read(lib.join("pack.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    let name = pj["name"].as_str().unwrap_or("library");
    let name: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    let built = pj["built"].as_str().unwrap_or("2026-01-01").replace('-', ".");
    let version = if lbn_core::version::Version::parse(&built).is_ok() { built } else { "1.0.0".into() };
    let bytes = plan
        .resources
        .iter()
        .filter_map(|r| join_rel(&data, &r.path).ok())
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    Ok(LegacyInfo {
        id: format!("org.lastbook.legacy.{}", if name.is_empty() { "library".into() } else { name.clone() }),
        version,
        title: format!("Lastbook library ({})", if name.is_empty() { "imported" } else { &name }),
        plan,
        bytes,
    })
}

pub fn inspect(lib: &Path) -> Result<LegacyInfo> {
    read_info(lib)
}

/// Build the pack into `staged` (hard links when `link` and on the same disk, else copies).
pub fn stage(lib: &Path, staged: &Path, link: bool) -> Result<(ParsedManifest, LegacyInfo)> {
    let info = read_info(lib)?;
    let data = lib.join("library-data");
    let mut texts = HashMap::new();
    let mut resources = Vec::new();
    for r in &info.plan.resources {
        if let Some(tp) = &r.text_path {
            if let Ok(t) = std::fs::read(join_rel(&data, tp)?) {
                let t = String::from_utf8_lossy(&t).into_owned();
                let ov = match r.rtype {
                    ResourceType::Pdf => TextOverride::Pages(legacy::pdf_pages(&t)),
                    _ => TextOverride::Plain(t),
                };
                texts.insert(r.id.clone(), ov);
            }
        }
        let transcript = r.transcript.clone().filter(|t| join_rel(&data, t).map(|p| p.is_file()).unwrap_or(false));
        let poster = r.poster.clone().filter(|t| join_rel(&data, t).map(|p| p.is_file()).unwrap_or(false));
        resources.push(SrcResource {
            resource: Resource {
                id: r.id.clone(),
                rtype: r.rtype,
                title: r.title.clone(),
                path: r.path.clone(),
                media_type: r.media_type.clone(),
                source: Some(Source {
                    url: r.source_url.clone(),
                    publisher: r.publisher.clone(),
                    retrieved: r.retrieved.clone(),
                    edition: r.edition.clone(),
                    notes: r.notes.clone(),
                }),
                license: None,
                pages: r.pages,
                duration_s: r.duration_s,
                transcript,
                poster,
                map: None,
                assets: vec![],
                sections: vec![],
                category: r.category.clone(),
                summary: None,
            },
            text_pages: None,
            text: None,
        });
    }
    let spec = SrcSpec {
        id: info.id.clone(),
        version: info.version.clone(),
        kind: PackKind::Mixed,
        title: info.title.clone(),
        description: "Imported from an existing Lastbook library. The original folder was not changed.".into(),
        language: Some("en".into()),
        created: info.version.clone(),
        publisher: Publisher { name: "Imported from Lastbook".into(), url: None },
        license: License {
            spdx: "LicenseRef-Lastbook-Mixed".into(),
            attribution: Some(
                "Each item keeps its publisher's terms; see the item's source. Not checked for redistribution.".into(),
            ),
            file: None,
            redistributable: false,
        },
        compat: Compat { min_engine_api: 1, note: None },
        resources,
        include: vec![],
        model: None,
    };
    let parsed = build(BuildInput { spec, root: &data, texts, link }, staged, |_| {})?;
    // Compare with the checksums the old library recorded; a mismatch means the old copy is damaged.
    let mut damaged = Vec::new();
    for r in &info.plan.resources {
        if let (Some(exp), Some(f)) = (&r.expected_sha256, parsed.manifest.file(&r.path)) {
            if exp != &f.sha256 {
                damaged.push(r.path.clone());
            }
        }
    }
    if !damaged.is_empty() {
        return Err(NativeError::coded(
            "checksum",
            format!("these files in the old library do not match its own checksums: {}", damaged.join(", ")),
        ));
    }
    let _ = manifest::MANIFEST_NAME;
    Ok((parsed, info))
}
