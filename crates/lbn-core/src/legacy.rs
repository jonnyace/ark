//! Reading libraries made by the original Lastbook (1.x): `LastbookData/library-data/catalog.json` + files.
//!
//! The importer never changes the original folder. It builds a new pack from it: PDFs, HTML pages and videos (with
//! their transcripts) become resources; search text comes from the library's own extracted text (`text_path`, PDF pages
//! separated by form feeds). Kiwix ZIM collections are reported as unsupported (no ZIM reader is included yet), as are
//! records whose files are missing or whose kind is unknown.
use crate::error::{Error, Result};
use crate::manifest::ResourceType;
use crate::path::check_rel_path;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct Record {
    pub id: String,
    pub kind: String,
    pub title: Option<String>,
    pub path: Option<String>,
    pub publisher: Option<String>,
    pub category: Option<String>,
    pub collection: Option<String>,
    pub bytes: Option<u64>,
    pub sha256: Option<String>,
    pub pages: Option<u32>,
    pub duration: Option<f64>,
    pub source_url: Option<String>,
    pub retrieved_at: Option<String>,
    pub edition: Option<String>,
    pub notes: Option<String>,
    pub text_path: Option<String>,
    pub video_path: Option<String>,
    pub vtt_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub video_id: Option<String>,
    #[serde(default)]
    pub captions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedResource {
    pub id: String,
    pub rtype: ResourceType,
    pub title: String,
    /// Path relative to `library-data/`.
    pub path: String,
    pub media_type: String,
    pub text_path: Option<String>,
    pub transcript: Option<String>,
    pub poster: Option<String>,
    pub pages: Option<u32>,
    pub duration_s: Option<f64>,
    pub publisher: Option<String>,
    pub source_url: Option<String>,
    pub retrieved: Option<String>,
    pub edition: Option<String>,
    pub notes: Option<String>,
    pub category: Option<String>,
    pub expected_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Unsupported {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Plan {
    pub resources: Vec<PlannedResource>,
    pub unsupported: Vec<Unsupported>,
}

/// Resource ids in the new pack: legacy ids are mostly safe already; anything else is mapped deterministically.
pub fn resource_id(legacy_id: &str) -> String {
    let s: String = legacy_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .take(120)
        .collect();
    if s.is_empty() {
        "item".into()
    } else {
        s
    }
}

fn media_type(rtype: ResourceType, path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match (rtype, ext.as_str()) {
        (ResourceType::Pdf, _) => "application/pdf".into(),
        (ResourceType::Html, _) => "text/html".into(),
        (ResourceType::Video, "webm") => "video/webm".into(),
        (ResourceType::Video, "mp4" | "m4v") => "video/mp4".into(),
        (ResourceType::Video, "mkv") => "video/x-matroska".into(),
        _ => "application/octet-stream".into(),
    }
}

pub fn plan(catalog_json: &[u8]) -> Result<Plan> {
    let records: Vec<Record> =
        serde_json::from_slice(catalog_json).map_err(|e| Error::Other(format!("catalog.json is not a Lastbook catalog: {e}")))?;
    let mut plan = Plan::default();
    let transcripts: Vec<&Record> = records.iter().filter(|r| r.kind == "transcript").collect();
    let thumbs: Vec<&Record> = records.iter().filter(|r| r.kind == "metadata").collect();
    for r in &records {
        let title = r.title.clone().unwrap_or_else(|| r.id.clone());
        let unsupported =
            |reason: &str| Unsupported { id: r.id.clone(), kind: r.kind.clone(), title: title.clone(), reason: reason.into() };
        let rtype = match r.kind.as_str() {
            "pdf" => ResourceType::Pdf,
            "html" => ResourceType::Html,
            "video" => ResourceType::Video,
            // Folded into their video below.
            "transcript" | "metadata" => continue,
            "zim" => {
                plan.unsupported
                    .push(unsupported("Kiwix ZIM collections are not supported yet; the original file is left in place"));
                continue;
            }
            _ => {
                plan.unsupported.push(unsupported("unknown record kind"));
                continue;
            }
        };
        let Some(path) = r.path.clone() else {
            plan.unsupported.push(unsupported("record has no file path"));
            continue;
        };
        if check_rel_path(&path).is_err() {
            plan.unsupported.push(unsupported("unsafe file path"));
            continue;
        }
        if rtype == ResourceType::Pdf && r.pages.unwrap_or(0) == 0 {
            plan.unsupported.push(unsupported("PDF without a page count"));
            continue;
        }
        if rtype == ResourceType::Video && !r.duration.is_some_and(|d| d > 0.0) {
            plan.unsupported.push(unsupported("video without a duration"));
            continue;
        }
        let (transcript, poster) = if rtype == ResourceType::Video {
            let t = transcripts
                .iter()
                .find(|t| t.video_path.as_deref() == Some(path.as_str()))
                .and_then(|t| t.vtt_path.clone())
                .or_else(|| r.captions.iter().find(|c| c.ends_with(".vtt")).cloned());
            let p = thumbs.iter().find(|m| m.video_path.as_deref() == Some(path.as_str())).and_then(|m| m.thumbnail_path.clone());
            (t.filter(|p| check_rel_path(p).is_ok()), p.filter(|p| check_rel_path(p).is_ok()))
        } else {
            (None, None)
        };
        plan.resources.push(PlannedResource {
            id: resource_id(&r.id),
            rtype,
            title,
            media_type: media_type(rtype, &path),
            path,
            text_path: r.text_path.clone().filter(|p| check_rel_path(p).is_ok()),
            transcript,
            poster,
            pages: r.pages,
            duration_s: r.duration,
            publisher: r.publisher.clone(),
            source_url: r.source_url.clone(),
            retrieved: r.retrieved_at.clone(),
            edition: r.edition.clone(),
            notes: r.notes.clone(),
            category: r.category.clone(),
            expected_sha256: r.sha256.clone(),
        });
    }
    let mut seen = std::collections::HashSet::new();
    plan.resources.retain(|r| seen.insert(r.id.clone()));
    Ok(plan)
}

/// Split legacy extracted PDF text into pages (form-feed separated).
pub fn pdf_pages(text: &str) -> Vec<String> {
    text.split('\u{c}').map(crate::text::squash_ws).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_catalog() {
        let cat = br#"[
          {"id":"v1","kind":"video","title":"V","path":"videos/a [x].mp4","duration":60,"captions":["videos/a [x].en.vtt"]},
          {"id":"t1","kind":"transcript","path":"transcripts/x.txt","video_path":"videos/a [x].mp4","vtt_path":"transcripts/x.en.vtt"},
          {"id":"m1","kind":"metadata","path":"m.webp","video_path":"videos/a [x].mp4","thumbnail_path":"metadata/v1.webp"},
          {"id":"eng-1","kind":"pdf","title":"P","path":"documents/p.pdf","pages":3,"text_path":"text/eng-1.txt"},
          {"id":"z","kind":"zim","title":"Z","path":"collections/z.zim"},
          {"id":"bad","kind":"html","title":"B","path":"../etc/passwd"},
          {"id":"np","kind":"pdf","title":"N","path":"documents/n.pdf"}
        ]"#;
        let p = plan(cat).unwrap();
        assert_eq!(p.resources.len(), 2);
        let v = &p.resources[0];
        assert_eq!(v.transcript.as_deref(), Some("transcripts/x.en.vtt"));
        assert_eq!(v.poster.as_deref(), Some("metadata/v1.webp"));
        assert_eq!(v.media_type, "video/mp4");
        assert_eq!(p.unsupported.len(), 3);
        assert!(p.unsupported.iter().any(|u| u.kind == "zim"));
        assert_eq!(pdf_pages("a\u{c}b  c\u{c}").len(), 3);
        assert_eq!(resource_id("a b/c"), "a_b_c");
    }
}
