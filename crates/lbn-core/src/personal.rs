//! Personal data: bookmarks, notes, reading history, saved answers and settings. Writable, separate from packs.
//!
//! Schema history:
//! - v1 (0.8 development builds): bookmarks with separate `pack`, `resource`, `page` fields; settings; no notes.
//! - v2 (0.9): every item refers to a citation target string (`lbn:` URI); notes and saved answers added.
//!
//! Stores (SQLite natively, IndexedDB in the browser) keep the same logical records and use this module for the
//! export format, import validation and migrations. Opening data written by a NEWER schema is refused rather than
//! guessed at, so rolling the app back never damages data: the newer app's backup stays intact.
use crate::citation::Citation;
use crate::error::{Error, Result};
use crate::PERSONAL_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EXPORT_FORMAT: &str = "lastbook-personal";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Bookmark {
    pub id: String,
    pub target: String,
    pub title: String,
    pub created: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Note {
    pub id: String,
    pub target: String,
    pub text: String,
    pub created: String,
    pub updated: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct HistoryItem {
    pub target: String,
    pub title: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SavedAnswer {
    pub id: String,
    pub question: String,
    pub answer: String,
    pub citations: Vec<String>,
    pub model: String,
    pub at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Settings {
    #[serde(default)]
    pub ai_enabled: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default = "default_text_size")]
    pub text_size: u32,
    #[serde(default)]
    pub theme: Option<String>,
}

fn default_text_size() -> u32 {
    100
}

impl Default for Settings {
    fn default() -> Self {
        Settings { ai_enabled: false, model: None, text_size: 100, theme: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PersonalData {
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub history: Vec<HistoryItem>,
    #[serde(default)]
    pub answers: Vec<SavedAnswer>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub schema: u32,
    pub exported: String,
    pub app_version: String,
    pub data: PersonalData,
}

pub fn export(data: &PersonalData, now: &str, app_version: &str) -> Export {
    Export {
        format: EXPORT_FORMAT.into(),
        schema: PERSONAL_SCHEMA_VERSION,
        exported: now.into(),
        app_version: app_version.into(),
        data: data.clone(),
    }
}

/// Bring data of any known schema up to the current one. Refuses newer schemas.
pub fn migrate(schema: u32, mut data: Value) -> Result<PersonalData> {
    if schema > PERSONAL_SCHEMA_VERSION {
        return Err(Error::Personal(format!(
            "this personal data was written by a newer app (schema {schema}; this app understands up to \
             {PERSONAL_SCHEMA_VERSION}). It has not been changed. Use the newer app, or restore a backup."
        )));
    }
    if schema == 0 {
        return Err(Error::Personal("unknown schema 0".into()));
    }
    if schema == 1 {
        data = v1_to_v2(data)?;
    }
    let d: PersonalData = serde_json::from_value(data).map_err(|e| Error::Personal(format!("damaged personal data: {e}")))?;
    validate(&d)?;
    Ok(d)
}

fn v1_to_v2(mut v: Value) -> Result<Value> {
    let obj = v.as_object_mut().ok_or_else(|| Error::Personal("v1 data is not an object".into()))?;
    if let Some(Value::Array(bms)) = obj.get_mut("bookmarks") {
        for b in bms.iter_mut() {
            let o = b.as_object_mut().ok_or_else(|| Error::Personal("bad v1 bookmark".into()))?;
            let pack = o.remove("pack").and_then(|x| x.as_str().map(String::from)).unwrap_or_default();
            let res = o.remove("resource").and_then(|x| x.as_str().map(String::from)).unwrap_or_default();
            let page = o.remove("page").and_then(|x| x.as_u64());
            let target = match page {
                Some(p) => format!("lbn:{pack}/{res}#page={p}"),
                None => format!("lbn:{pack}/{res}"),
            };
            o.insert("target".into(), Value::String(target));
        }
    }
    obj.entry("notes").or_insert(Value::Array(vec![]));
    obj.entry("answers").or_insert(Value::Array(vec![]));
    Ok(v)
}

pub fn validate(d: &PersonalData) -> Result<()> {
    let targets = d
        .bookmarks
        .iter()
        .map(|b| &b.target)
        .chain(d.notes.iter().map(|n| &n.target))
        .chain(d.history.iter().map(|h| &h.target))
        .chain(d.answers.iter().flat_map(|a| a.citations.iter()));
    for t in targets {
        Citation::parse(t).map_err(|e| Error::Personal(format!("bad target {t:?}: {e}")))?;
    }
    if d.notes.iter().any(|n| n.text.len() > 1_000_000) {
        return Err(Error::Personal("note too large".into()));
    }
    Ok(())
}

/// Parse an export file (any known schema) for import.
pub fn parse_export(bytes: &[u8]) -> Result<Export> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| Error::Personal(format!("not a valid export: {e}")))?;
    if v.get("format").and_then(|f| f.as_str()) != Some(EXPORT_FORMAT) {
        return Err(Error::Personal("not a Lastbook personal-data export".into()));
    }
    let schema = v.get("schema").and_then(|s| s.as_u64()).unwrap_or(0) as u32;
    let data = migrate(schema, v.get("data").cloned().unwrap_or(Value::Null))?;
    Ok(Export {
        format: EXPORT_FORMAT.into(),
        schema: PERSONAL_SCHEMA_VERSION,
        exported: v.get("exported").and_then(|s| s.as_str()).unwrap_or("").into(),
        app_version: v.get("app_version").and_then(|s| s.as_str()).unwrap_or("").into(),
        data,
    })
}

/// Merge imported data into existing data: items are kept by id/target; nothing existing is deleted.
pub fn merge(into: &mut PersonalData, other: PersonalData) {
    for b in other.bookmarks {
        if !into.bookmarks.iter().any(|x| x.id == b.id || x.target == b.target) {
            into.bookmarks.push(b);
        }
    }
    for n in other.notes {
        match into.notes.iter_mut().find(|x| x.id == n.id) {
            Some(x) if n.updated > x.updated => *x = n,
            Some(_) => {}
            None => into.notes.push(n),
        }
    }
    for h in other.history {
        if !into.history.iter().any(|x| x.target == h.target && x.at == h.at) {
            into.history.push(h);
        }
    }
    for a in other.answers {
        if !into.answers.iter().any(|x| x.id == a.id) {
            into.answers.push(a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn migrates_v1() {
        let v1 = json!({"bookmarks": [{"id": "b1", "pack": "org.a.b", "resource": "doc", "page": 4, "title": "T", "created": "2026-01-01"}],
                        "history": [], "settings": {"ai_enabled": true}});
        let d = migrate(1, v1).unwrap();
        assert_eq!(d.bookmarks[0].target, "lbn:org.a.b/doc#page=4");
        assert!(d.settings.ai_enabled);
        assert_eq!(d.settings.text_size, 100);
    }

    #[test]
    fn refuses_newer_schema() {
        let e = migrate(PERSONAL_SCHEMA_VERSION + 1, json!({})).unwrap_err();
        assert!(e.to_string().contains("newer app"));
    }

    #[test]
    fn export_round_trip_and_merge() {
        let mut d = PersonalData::default();
        d.bookmarks.push(Bookmark {
            id: "1".into(),
            target: "lbn:org.a.b/x#page=2".into(),
            title: "X".into(),
            created: "t".into(),
        });
        d.notes.push(Note {
            id: "n".into(),
            target: "lbn:org.a.b/x".into(),
            text: "hi".into(),
            created: "1".into(),
            updated: "1".into(),
        });
        let bytes = serde_json::to_vec(&export(&d, "now", "0.9.0")).unwrap();
        let back = parse_export(&bytes).unwrap();
        assert_eq!(back.data, d);
        let mut other = back.data.clone();
        other.notes[0].text = "newer".into();
        other.notes[0].updated = "2".into();
        merge(&mut d, other);
        assert_eq!(d.bookmarks.len(), 1);
        assert_eq!(d.notes[0].text, "newer");
        assert!(parse_export(b"{\"format\":\"x\"}").is_err());
        let bad = json!({"format": EXPORT_FORMAT, "schema": 2, "data": {"bookmarks": [{"id":"1","target":"http://x","title":"","created":""}]}});
        assert!(parse_export(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
}
