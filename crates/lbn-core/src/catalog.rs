//! Pack catalogs: a publisher's signed list of available packs (`catalog.json` + `catalog.json.minisig`).
//!
//! `expires` limits how long a catalog is trusted for *finding and installing new packs*. It never affects packs
//! that are already installed: they keep working offline indefinitely, whatever the date.
use crate::error::{Error, Result};
use crate::hash::is_sha256_hex;
use crate::manifest::{is_valid_pack_id, PackKind};
use serde::{Deserialize, Serialize};

pub const CATALOG_FORMAT: &str = "lastbook-catalog";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct CatalogEntry {
    pub id: String,
    pub version: String,
    pub kind: PackKind,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub license: String,
    pub bytes: u64,
    /// Path of the pack's `pack.json`, relative to the catalog.
    pub manifest: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub lbpack: Option<String>,
    #[serde(default)]
    pub lbpack_bytes: Option<u64>,
    #[serde(default)]
    pub lbpack_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogFile {
    pub format: String,
    pub format_version: u32,
    pub title: String,
    pub published: String,
    pub expires: String,
    pub packs: Vec<CatalogEntry>,
}

pub fn parse(bytes: &[u8]) -> Result<CatalogFile> {
    let c: CatalogFile = serde_json::from_slice(bytes).map_err(|e| Error::Manifest(format!("catalog is not valid: {e}")))?;
    if c.format != CATALOG_FORMAT || c.format_version != 1 {
        return Err(Error::Incompatible("unsupported catalog format".into()));
    }
    for p in &c.packs {
        if !is_valid_pack_id(&p.id) || !is_sha256_hex(&p.manifest_sha256) {
            return Err(Error::Manifest(format!("catalog entry {:?} is invalid", p.id)));
        }
        crate::path::check_rel_path(&p.manifest)?;
        if let Some(l) = &p.lbpack {
            crate::path::check_rel_path(l)?;
        }
    }
    Ok(c)
}

/// ISO dates (`YYYY-MM-DD...`) compare correctly as strings.
pub fn is_expired(c: &CatalogFile, today: &str) -> bool {
    today.get(..10).unwrap_or(today) > c.expires.get(..10).unwrap_or(&c.expires)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_expires() {
        let j = br#"{"format":"lastbook-catalog","format_version":1,"title":"t","published":"2026-11-17","expires":"2027-11-17",
          "packs":[{"id":"org.a.b","version":"1","kind":"documents","title":"x","license":"CC0-1.0","bytes":1,"manifest":"org.a.b/pack.json",
          "manifest_sha256":"0000000000000000000000000000000000000000000000000000000000000000"}]}"#;
        let c = parse(j).unwrap();
        assert!(!is_expired(&c, "2027-11-17T10:00:00Z"));
        assert!(is_expired(&c, "2027-11-18"));
        let bad = String::from_utf8_lossy(j).replace("org.a.b/pack.json", "../x");
        assert!(parse(bad.as_bytes()).is_err());
    }
}
