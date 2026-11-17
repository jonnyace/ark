//! Install rules shared by the native and browser adapters: what to fetch or check, whether there is room,
//! and whether a verified pack may become the active version.
//!
//! Life of an install: files are written to a staging area -> every file is checked against size and SHA-256 ->
//! only then the registry's active pointer for the pack id moves to the new build (one atomic step) -> the previous
//! build is kept until the person removes it. A failure at any step leaves the active build untouched.
use crate::error::{Error, Result};
use crate::manifest::{FileEntry, Manifest};
use crate::version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FileAction {
    /// The file is present at full size: hash it.
    Verify,
    /// Fetch from byte `from` (0 = whole file; > 0 = resume a partial file).
    Download { from: u64 },
}

/// Decide, per manifest file, what to do given the sizes already in staging (`None` / absent = no file).
pub fn plan(m: &Manifest, staged: &HashMap<String, u64>) -> Vec<(FileEntry, FileAction)> {
    m.files
        .iter()
        .map(|f| {
            let a = match staged.get(&f.path) {
                Some(&s) if s == f.size => FileAction::Verify,
                Some(&s) if s < f.size => FileAction::Download { from: s },
                _ => FileAction::Download { from: 0 },
            };
            (f.clone(), a)
        })
        .collect()
}

/// Check one file's observed size and hash.
pub fn check_file(entry: &FileEntry, size: u64, sha256: &str) -> Result<()> {
    if size != entry.size {
        return Err(Error::SizeMismatch { path: entry.path.clone(), expected: entry.size, actual: size });
    }
    if sha256 != entry.sha256 {
        return Err(Error::HashMismatch { path: entry.path.clone() });
    }
    Ok(())
}

/// Bytes still to be written for this plan, plus a safety margin (5 % + 64 MiB).
pub fn space_needed(plan: &[(FileEntry, FileAction)]) -> u64 {
    let remaining: u64 = plan
        .iter()
        .map(|(f, a)| match a {
            FileAction::Verify => 0,
            FileAction::Download { from } => f.size - from,
        })
        .sum();
    remaining + remaining / 20 + 64 * 1024 * 1024
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceCheck {
    pub need_bytes: u64,
    pub free_bytes: u64,
    pub ok: bool,
}

pub fn check_space(plan: &[(FileEntry, FileAction)], free_bytes: u64) -> SpaceCheck {
    let need = space_needed(plan);
    SpaceCheck { need_bytes: need, free_bytes, ok: free_bytes >= need }
}

/// A pack build known to the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct InstalledPack {
    pub id: String,
    pub version: String,
    pub manifest_sha256: String,
    /// Directory / storage key holding this build (`packs/<id>/<version>+<hash16>`).
    pub dir: String,
    pub kind: crate::manifest::PackKind,
    pub title: String,
    pub bytes: u64,
    pub installed_at: String,
    /// Fingerprint (key id) of the trusted key that signed it, or `None` for an unsigned pack.
    pub signed_by: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ActivationOptions {
    /// Allow replacing a newer active version with an older one (rollback).
    pub allow_downgrade: bool,
}

/// May a fully verified build become the active version of its pack?
pub fn can_activate(candidate: &Manifest, current: Option<&InstalledPack>, opts: ActivationOptions) -> Result<()> {
    if let Some(cur) = current {
        if cur.id != candidate.id {
            return Err(Error::Other("pack id mismatch".into()));
        }
        if !opts.allow_downgrade && version::compare(&candidate.version, &cur.version)? == std::cmp::Ordering::Less {
            return Err(Error::Other(format!(
                "version {} is older than the installed {}; choose 'roll back' to use it",
                candidate.version, cur.version
            )));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum JobState {
    Queued,
    Downloading,
    Verifying,
    Activating,
    Paused,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct JobError {
    /// `network`, `no_space`, `checksum`, `size`, `manifest`, `signature`, `unsafe_path`, `incompatible`, `io`,
    /// `cancelled`, `unsigned` (needs confirmation), `downgrade`.
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct JobProgress {
    pub job: String,
    pub pack_id: String,
    pub version: String,
    pub title: String,
    pub state: JobState,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u32,
    pub files_total: u32,
    pub current_file: Option<String>,
    pub error: Option<JobError>,
}

pub fn job_error(e: &Error) -> JobError {
    let code = match e {
        Error::SizeMismatch { .. } => "size",
        Error::HashMismatch { .. } => "checksum",
        other => other.code(),
    };
    JobError { code: code.into(), message: e.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::sha256_hex;
    use crate::manifest::tests::sample;

    #[test]
    fn plans_resume_and_verify() {
        let mut m = sample();
        m.files.push(FileEntry { path: "b".into(), size: 100, sha256: sha256_hex(b"b") });
        m.files.push(FileEntry { path: "c".into(), size: 10, sha256: sha256_hex(b"c") });
        let staged: HashMap<String, u64> = [("res/doc1.html".to_string(), 3), ("b".into(), 40), ("c".into(), 99)].into();
        let p = plan(&m, &staged);
        assert_eq!(p[0].1, FileAction::Verify);
        assert_eq!(p[1].1, FileAction::Download { from: 40 });
        assert_eq!(p[2].1, FileAction::Download { from: 0 });
        assert_eq!(space_needed(&p), 70 + 3 + 64 * 1024 * 1024);
        assert!(!check_space(&p, 1000).ok);
    }

    #[test]
    fn checks_files() {
        let f = FileEntry { path: "x".into(), size: 3, sha256: sha256_hex(b"abc") };
        assert!(check_file(&f, 3, &sha256_hex(b"abc")).is_ok());
        assert_eq!(check_file(&f, 3, &sha256_hex(b"abd")).unwrap_err().code(), "hash_mismatch");
        assert_eq!(job_error(&check_file(&f, 4, "").unwrap_err()).code, "size");
    }

    #[test]
    fn refuses_silent_downgrade() {
        let m = sample();
        let cur = InstalledPack {
            id: m.id.clone(),
            version: "2.0.0".into(),
            manifest_sha256: "x".into(),
            dir: "d".into(),
            kind: m.kind,
            title: "t".into(),
            bytes: 0,
            installed_at: "".into(),
            signed_by: None,
            active: true,
        };
        assert!(can_activate(&m, Some(&cur), ActivationOptions::default()).is_err());
        assert!(can_activate(&m, Some(&cur), ActivationOptions { allow_downgrade: true }).is_ok());
        assert!(can_activate(&m, None, ActivationOptions::default()).is_ok());
    }
}
