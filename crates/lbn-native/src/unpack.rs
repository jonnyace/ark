//! Staging a pack from a local directory or a `.lbpack` (ZIP) file. Untrusted input: only files named in the
//! manifest are taken, links are refused, sizes are enforced while copying, and nothing is written outside staging.
use crate::util::join_rel;
use crate::{NativeError, Result};
use lbn_core::manifest::{self, ParsedManifest, MANIFEST_NAME, SIGNATURE_NAME};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const MAX_SMALL: u64 = 1 << 20;

/// Copy a pack directory into staging. Returns the parsed manifest (signature and hashes are checked later).
pub fn stage_from_dir(src: &Path, staged: &Path, mut progress: impl FnMut(u64)) -> Result<ParsedManifest> {
    let mbytes = read_regular(&src.join(MANIFEST_NAME), 32 << 20)?;
    let parsed = manifest::parse(&mbytes)?;
    std::fs::create_dir_all(staged)?;
    std::fs::write(staged.join(MANIFEST_NAME), &mbytes)?;
    match read_regular(&src.join(SIGNATURE_NAME), MAX_SMALL) {
        Ok(sig) => std::fs::write(staged.join(SIGNATURE_NAME), sig)?,
        Err(NativeError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    for f in &parsed.manifest.files {
        let from = join_rel(src, &f.path)?;
        let to = join_rel(staged, &f.path)?;
        if std::fs::metadata(&to).map(|m| m.len() == f.size).unwrap_or(false) {
            progress(f.size);
            continue; // resumed import: hashes are checked before activation anyway
        }
        let md = std::fs::symlink_metadata(&from)
            .map_err(|_| NativeError::coded("missing", format!("{} is missing from the pack", f.path)))?;
        if !md.file_type().is_file() {
            return Err(NativeError::coded("unsafe_path", format!("{} is not a regular file", f.path)));
        }
        std::fs::create_dir_all(to.parent().unwrap())?;
        copy_limited(&mut File::open(&from)?, &to, f.size, &mut progress)?;
    }
    Ok(parsed)
}

/// Extract a `.lbpack` into staging, strictly.
pub fn stage_from_zip(zip_path: &Path, staged: &Path, mut progress: impl FnMut(u64)) -> Result<ParsedManifest> {
    let file = File::open(zip_path)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| NativeError::coded("unpack", format!("not a readable pack file: {e}")))?;
    let mbytes = {
        let mut e = zip
            .by_name(MANIFEST_NAME)
            .map_err(|_| NativeError::coded("manifest", "the pack file has no pack.json at its top"))?;
        check_entry(&e)?;
        read_limited(&mut e, 32 << 20)?
    };
    let parsed = manifest::parse(&mbytes)?;
    let listed: std::collections::HashMap<&str, u64> = parsed.manifest.files.iter().map(|f| (f.path.as_str(), f.size)).collect();
    // Check every entry before writing anything.
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i).map_err(|e| NativeError::coded("unpack", e.to_string()))?;
        let name = entry_name(&e)?;
        if e.is_dir() {
            lbn_core::path::check_rel_path(name.trim_end_matches('/'))?;
            continue;
        }
        check_entry(&e)?;
        if name == MANIFEST_NAME || name == SIGNATURE_NAME {
            continue;
        }
        match listed.get(name.as_str()) {
            Some(&size) if size == e.size() => {}
            Some(_) => {
                return Err(NativeError::coded("size", format!("{name}: size in the pack file differs from the manifest")))
            }
            None => return Err(NativeError::coded("manifest", format!("{name} is in the pack file but not in its manifest"))),
        }
    }
    std::fs::create_dir_all(staged)?;
    std::fs::write(staged.join(MANIFEST_NAME), &mbytes)?;
    if let Ok(mut e) = zip.by_name(SIGNATURE_NAME) {
        let sig = read_limited(&mut e, MAX_SMALL)?;
        std::fs::write(staged.join(SIGNATURE_NAME), sig)?;
    }
    for f in &parsed.manifest.files {
        let to = join_rel(staged, &f.path)?;
        if std::fs::metadata(&to).map(|m| m.len() == f.size).unwrap_or(false) {
            progress(f.size);
            continue;
        }
        let mut e = zip
            .by_name(&f.path)
            .map_err(|_| NativeError::coded("missing", format!("{} is missing from the pack file", f.path)))?;
        std::fs::create_dir_all(to.parent().unwrap())?;
        copy_limited(&mut e, &to, f.size, &mut progress)?;
    }
    Ok(parsed)
}

fn entry_name(e: &zip::read::ZipFile<'_, File>) -> Result<String> {
    e.name()
        .map(|n| n.into_owned())
        .map_err(|err| NativeError::coded("unsafe_path", format!("entry name is not valid UTF-8: {err}")))
}

fn check_entry(e: &zip::read::ZipFile<'_, File>) -> Result<()> {
    let name = entry_name(e)?;
    lbn_core::path::check_rel_path(&name)?;
    if e.encrypted() {
        return Err(NativeError::coded("unpack", format!("{name} is encrypted")));
    }
    if let Some(mode) = e.unix_mode() {
        if mode & 0o170000 == 0o120000 {
            return Err(NativeError::coded("unsafe_path", format!("{name} is a link")));
        }
    }
    Ok(())
}

fn read_regular(p: &Path, max: u64) -> Result<Vec<u8>> {
    let md = std::fs::symlink_metadata(p)?;
    if !md.file_type().is_file() {
        return Err(NativeError::coded("unsafe_path", format!("{} is not a regular file", p.display())));
    }
    read_limited(&mut File::open(p)?, max)
}

fn read_limited(r: &mut impl Read, max: u64) -> Result<Vec<u8>> {
    let mut v = Vec::new();
    r.take(max + 1).read_to_end(&mut v)?;
    if v.len() as u64 > max {
        return Err(NativeError::coded("unpack", "file is larger than allowed"));
    }
    Ok(v)
}

/// Copy at most `size` bytes; more (a lying size header, a zip bomb) is an error. Writes `<to>.part`, then renames.
fn copy_limited(r: &mut impl Read, to: &Path, size: u64, progress: &mut impl FnMut(u64)) -> Result<()> {
    let part =
        to.with_extension(format!("{}part", to.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()));
    let mut out = File::create(&part)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        done += n as u64;
        if done > size {
            drop(out);
            let _ = std::fs::remove_file(&part);
            return Err(NativeError::coded("size", format!("{} is larger than the manifest says", to.display())));
        }
        out.write_all(&buf[..n])?;
        progress(n as u64);
    }
    out.sync_all()?;
    if done != size {
        let _ = std::fs::remove_file(&part);
        return Err(NativeError::coded("size", format!("{} is shorter than the manifest says", to.display())));
    }
    std::fs::rename(&part, to)?;
    Ok(())
}
