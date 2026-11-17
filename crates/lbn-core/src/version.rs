//! Content versions and compatibility ranges.
//!
//! Pack versions are dotted numeric sequences with an optional `-suffix` (`2026.11.16`, `1.2.0`, `1.2.0-rc.1`).
//! They are compared component by component; a version with a suffix sorts before the same version without one.
use crate::error::{Error, Result};
use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub nums: Vec<u64>,
    pub pre: Option<String>,
}

impl Version {
    pub fn parse(s: &str) -> Result<Self> {
        if s.is_empty() || s.len() > 64 {
            return Err(Error::Manifest(format!("bad version {s:?}")));
        }
        let (main, pre) = match s.split_once('-') {
            Some((m, p)) => (m, Some(p.to_string())),
            None => (s, None),
        };
        if let Some(p) = &pre {
            if p.is_empty() || !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
                return Err(Error::Manifest(format!("bad version suffix in {s:?}")));
            }
        }
        let nums = main
            .split('.')
            .map(|n| {
                if n.is_empty() || n.len() > 18 || !n.bytes().all(|b| b.is_ascii_digit()) {
                    Err(Error::Manifest(format!("bad version {s:?}")))
                } else {
                    Ok(n.parse::<u64>().unwrap())
                }
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Version { nums, pre })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let n = self.nums.len().max(other.nums.len());
        for i in 0..n {
            let a = self.nums.get(i).copied().unwrap_or(0);
            let b = other.nums.get(i).copied().unwrap_or(0);
            match a.cmp(&b) {
                Ordering::Equal => {}
                o => return o,
            }
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

pub fn compare(a: &str, b: &str) -> Result<Ordering> {
    Ok(Version::parse(a)?.cmp(&Version::parse(b)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordering() {
        assert_eq!(compare("1.2", "1.2.0").unwrap(), Ordering::Equal);
        assert_eq!(compare("2026.11.16", "2026.11.2").unwrap(), Ordering::Greater);
        assert_eq!(compare("1.0.0-rc.1", "1.0.0").unwrap(), Ordering::Less);
        assert!(Version::parse("1..2").is_err());
        assert!(Version::parse("v1").is_err());
        assert!(Version::parse("1.0-").is_err());
    }
}
