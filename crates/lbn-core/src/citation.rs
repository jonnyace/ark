//! Citation targets: an exact place inside an installed resource.
//!
//! Text form (stable, used in personal data, exports and links):
//!   `lbn:<pack-id>/<resource-id>`                 whole resource
//!   `lbn:<pack-id>/<resource-id>#page=12`         PDF page (1-based, as numbered in the file)
//!   `lbn:<pack-id>/<resource-id>#t=63.5`          video moment, seconds
//!   `lbn:<pack-id>/<resource-id>#t=63.5,90`       video span
//!   `lbn:<pack-id>/<resource-id>#sec=boiling`     HTML section anchor
//!   `lbn:<pack-id>/<resource-id>#at=42.44,-76.50,15` map position (lat, lon, zoom)
use crate::error::{Error, Result};
use crate::manifest::{is_valid_pack_id, is_valid_resource_id, Manifest, ResourceType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Loc {
    Whole,
    Page { page: u32 },
    Time { start: f64, end: Option<f64> },
    Section { id: String },
    At { lat: f64, lon: f64, zoom: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Citation {
    pub pack: String,
    pub resource: String,
    pub loc: Loc,
}

fn num(s: &str) -> Result<f64> {
    let v: f64 = s.parse().map_err(|_| Error::Citation(format!("bad number {s:?}")))?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(Error::Citation("number is not finite".into()))
    }
}

impl Citation {
    pub fn parse(s: &str) -> Result<Self> {
        let rest = s.strip_prefix("lbn:").ok_or_else(|| Error::Citation("must start with lbn:".into()))?;
        let (path, frag) = match rest.split_once('#') {
            Some((p, f)) => (p, Some(f)),
            None => (rest, None),
        };
        let (pack, resource) = path.split_once('/').ok_or_else(|| Error::Citation("missing resource".into()))?;
        if !is_valid_pack_id(pack) || !is_valid_resource_id(resource) {
            return Err(Error::Citation(format!("bad pack or resource id in {s:?}")));
        }
        let loc = match frag {
            None => Loc::Whole,
            Some(f) => {
                let (k, v) = f.split_once('=').ok_or_else(|| Error::Citation("bad fragment".into()))?;
                match k {
                    "page" => Loc::Page { page: v.parse().map_err(|_| Error::Citation("bad page".into()))? },
                    "t" => match v.split_once(',') {
                        Some((a, b)) => Loc::Time { start: num(a)?, end: Some(num(b)?) },
                        None => Loc::Time { start: num(v)?, end: None },
                    },
                    "sec" if is_valid_resource_id(v) => Loc::Section { id: v.to_string() },
                    "at" => {
                        let p: Vec<&str> = v.split(',').collect();
                        if p.len() != 3 {
                            return Err(Error::Citation("at= needs lat,lon,zoom".into()));
                        }
                        Loc::At { lat: num(p[0])?, lon: num(p[1])?, zoom: num(p[2])? }
                    }
                    _ => return Err(Error::Citation(format!("unknown fragment {k:?}"))),
                }
            }
        };
        Ok(Citation { pack: pack.into(), resource: resource.into(), loc })
    }

    pub fn to_uri(&self) -> String {
        let base = format!("lbn:{}/{}", self.pack, self.resource);
        match &self.loc {
            Loc::Whole => base,
            Loc::Page { page } => format!("{base}#page={page}"),
            Loc::Time { start, end: None } => format!("{base}#t={}", fmt_num(*start)),
            Loc::Time { start, end: Some(e) } => format!("{base}#t={},{}", fmt_num(*start), fmt_num(*e)),
            Loc::Section { id } => format!("{base}#sec={id}"),
            Loc::At { lat, lon, zoom } => format!("{base}#at={},{},{}", fmt_num(*lat), fmt_num(*lon), fmt_num(*zoom)),
        }
    }

    /// Check that the target exists in this pack: resource present, page within the PDF, time within the video,
    /// section known, position inside the map bounds.
    pub fn validate_against(&self, m: &Manifest) -> Result<()> {
        if self.pack != m.id {
            return Err(Error::Citation(format!("pack {} is not {}", self.pack, m.id)));
        }
        let r = m.resource(&self.resource).ok_or_else(|| Error::Citation(format!("no resource {}", self.resource)))?;
        match (&self.loc, r.rtype) {
            (Loc::Whole, _) => Ok(()),
            (Loc::Page { page }, ResourceType::Pdf) => {
                if *page >= 1 && *page <= r.pages.unwrap_or(0) {
                    Ok(())
                } else {
                    Err(Error::Citation(format!("page {page} is outside 1..={}", r.pages.unwrap_or(0))))
                }
            }
            (Loc::Time { start, end }, ResourceType::Video) => {
                let d = r.duration_s.unwrap_or(0.0);
                let e = end.unwrap_or(*start);
                if *start >= 0.0 && *start <= d && e >= *start && e <= d + 1.0 {
                    Ok(())
                } else {
                    Err(Error::Citation(format!("time {start} is outside the video ({d} s)")))
                }
            }
            (Loc::Section { id }, ResourceType::Html | ResourceType::Article) => {
                if r.sections.iter().any(|s| &s.id == id) {
                    Ok(())
                } else {
                    Err(Error::Citation(format!("no section {id} in {}", r.id)))
                }
            }
            (Loc::At { lat, lon, zoom }, ResourceType::Map) => {
                let mi = r.map.as_ref().ok_or_else(|| Error::Citation("map without info".into()))?;
                let [w, s, e, n] = mi.bounds;
                if *lon >= w && *lon <= e && *lat >= s && *lat <= n && *zoom >= 0.0 && *zoom <= 24.0 {
                    Ok(())
                } else {
                    Err(Error::Citation("position is outside the map".into()))
                }
            }
            (l, t) => Err(Error::Citation(format!("{l:?} does not apply to a {t:?} resource"))),
        }
    }
}

fn fmt_num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// "1:05:03" style label for a time in seconds.
pub fn time_label(t: f64) -> String {
    let s = t.max(0.0).floor() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::sample;

    #[test]
    fn round_trips() {
        for s in [
            "lbn:org.example.test/doc1",
            "lbn:org.example.test/doc1#page=3",
            "lbn:org.example.test/v1#t=63.5",
            "lbn:org.example.test/v1#t=0,12.25",
            "lbn:org.example.test/doc1#sec=boil",
            "lbn:org.example.test/m#at=42.44,-76.5,15",
        ] {
            assert_eq!(Citation::parse(s).unwrap().to_uri(), s);
        }
        for bad in ["http://x", "lbn:x", "lbn:Bad/doc", "lbn:org.ex/doc#page=x", "lbn:org.ex/doc#t=nan", "lbn:org.ex/d#zz=1"] {
            assert!(Citation::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn validates_targets() {
        let mut m = sample();
        let ok = Citation::parse("lbn:org.example.test/doc1#sec=boil").unwrap();
        assert!(ok.validate_against(&m).is_ok());
        assert!(Citation::parse("lbn:org.example.test/doc1#sec=nope").unwrap().validate_against(&m).is_err());
        assert!(Citation::parse("lbn:org.example.test/nope").unwrap().validate_against(&m).is_err());
        assert!(Citation::parse("lbn:org.example.test/doc1#page=1").unwrap().validate_against(&m).is_err());
        m.resources[0].rtype = crate::manifest::ResourceType::Pdf;
        m.resources[0].pages = Some(5);
        assert!(Citation::parse("lbn:org.example.test/doc1#page=5").unwrap().validate_against(&m).is_ok());
        assert!(Citation::parse("lbn:org.example.test/doc1#page=6").unwrap().validate_against(&m).is_err());
        assert!(Citation::parse("lbn:org.example.test/doc1#page=0").unwrap().validate_against(&m).is_err());
    }

    #[test]
    fn labels_times() {
        assert_eq!(time_label(65.9), "1:05");
        assert_eq!(time_label(3725.0), "1:02:05");
    }
}
