use crate::Result;
use lbn_core::hash::Hasher;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}

pub fn now_compact() -> String {
    let t = time::OffsetDateTime::now_utc();
    format!("{:04}{:02}{:02}T{:02}{:02}{:02}", t.year(), t.month() as u8, t.day(), t.hour(), t.minute(), t.second())
}

/// Hash a file, streaming. Calls `progress(bytes)` as it goes; returns (size, sha256).
pub fn hash_file(path: &Path, mut progress: impl FnMut(u64)) -> Result<(u64, String)> {
    let mut f = File::open(path)?;
    let mut h = Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        progress(n as u64);
    }
    Ok((h.bytes(), h.finish_hex()))
}

/// Join a *validated* pack-relative path onto a root (the path has passed `lbn_core::path::check_rel_path`).
pub fn join_rel(root: &Path, rel: &str) -> Result<PathBuf> {
    lbn_core::path::check_rel_path(rel)?;
    Ok(rel.split('/').fold(root.to_path_buf(), |p, c| p.join(c)))
}

/// Write a file atomically: temp file in the same directory, fsync, rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("f")));
    {
        let mut f = File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    sync_dir(dir);
    Ok(())
}

pub fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

pub fn free_space(path: &Path) -> Option<u64> {
    let mut p = path.to_path_buf();
    while !p.exists() {
        if !p.pop() {
            return None;
        }
    }
    fs4::available_space(&p).ok()
}

pub fn dir_size(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            match e.metadata() {
                Ok(m) if m.is_dir() => total += dir_size(&e.path()),
                Ok(m) => total += m.len(),
                Err(_) => {}
            }
        }
    }
    total
}

pub fn random_id() -> String {
    let mut b = [0u8; 8];
    if let Ok(mut f) = File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    } else {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        b = (t as u64 ^ std::process::id() as u64).to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}
