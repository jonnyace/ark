//! Path safety for untrusted pack contents (manifest paths, zip entries, legacy catalogs).
//!
//! A pack path is a relative, forward-slash path of plain components. Rules follow Lastbook's unpack guards
//! (zip-slip, absolute paths, drive letters, links are checked by the adapter, case clashes) and add limits on length
//! and characters so a pack cannot create names that are invalid or dangerous on any supported OS.
use crate::error::{Error, Result};
use std::collections::HashSet;

const MAX_PATH: usize = 512;
const MAX_COMPONENT: usize = 200;
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3",
    "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn bad(path: &str, why: &'static str) -> Error {
    Error::UnsafePath { path: path.chars().take(120).collect(), why }
}

/// Check that `p` is a safe relative pack path. Returns it unchanged when it is.
pub fn check_rel_path(p: &str) -> Result<&str> {
    if p.is_empty() {
        return Err(bad(p, "empty"));
    }
    if p.len() > MAX_PATH {
        return Err(bad(p, "too long"));
    }
    if p.starts_with('/') || p.starts_with('\\') {
        return Err(bad(p, "absolute"));
    }
    if p.contains('\\') {
        return Err(bad(p, "backslash"));
    }
    if p.len() >= 2 && p.as_bytes()[1] == b':' {
        return Err(bad(p, "drive letter"));
    }
    for c in p.chars() {
        if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            return Err(bad(p, "forbidden character"));
        }
    }
    for comp in p.split('/') {
        if comp.is_empty() {
            return Err(bad(p, "empty component"));
        }
        if comp == "." || comp == ".." {
            return Err(bad(p, "dot component"));
        }
        if comp.len() > MAX_COMPONENT {
            return Err(bad(p, "component too long"));
        }
        if comp.ends_with('.') || comp.ends_with(' ') || comp.starts_with(' ') {
            return Err(bad(p, "trailing dot or space"));
        }
        let stem = comp.split('.').next().unwrap_or("").to_ascii_lowercase();
        if WINDOWS_RESERVED.contains(&stem.as_str()) {
            return Err(bad(p, "reserved name"));
        }
    }
    Ok(p)
}

/// Check a set of paths: each safe, no duplicates, no case-insensitive clashes, and no path that is also a directory
/// of another (`a` and `a/b`).
pub fn check_path_set<'a, I: IntoIterator<Item = &'a str>>(paths: I) -> Result<()> {
    let mut seen = HashSet::new();
    let mut folded = HashSet::new();
    let mut all = Vec::new();
    for p in paths {
        check_rel_path(p)?;
        if !seen.insert(p) {
            return Err(bad(p, "duplicate"));
        }
        if !folded.insert(p.to_lowercase()) {
            return Err(bad(p, "case clash"));
        }
        all.push(p);
    }
    let dirs: HashSet<String> = all
        .iter()
        .flat_map(|p| {
            let parts: Vec<&str> = p.split('/').collect();
            (1..parts.len()).map(move |i| parts[..i].join("/").to_lowercase()).collect::<Vec<_>>()
        })
        .collect();
    for p in &all {
        if dirs.contains(&p.to_lowercase()) {
            return Err(bad(p, "file and directory with the same name"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_plain_paths() {
        for p in ["a.txt", "res/doc.html", "index/search.lbi", "v/Clip 1 [x].webm"] {
            assert!(check_rel_path(p).is_ok(), "{p}");
        }
    }
    #[test]
    fn rejects_unsafe_paths() {
        for p in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a//b",
            "C:/x",
            "c:x",
            "a\\b",
            "./a",
            "a/./b",
            "con.txt",
            "x/NUL",
            "a\u{0}b",
            "a\nb",
            "a:b",
            "dir/",
            "x.",
            " x",
            "a/b*",
        ] {
            assert!(check_rel_path(p).is_err(), "{p:?} should be rejected");
        }
        assert!(check_rel_path(&"a/".repeat(300)).is_err());
    }
    #[test]
    fn rejects_clashing_sets() {
        assert!(check_path_set(["a", "b/c"]).is_ok());
        assert!(check_path_set(["a", "a"]).is_err());
        assert!(check_path_set(["Read.me", "read.ME"]).is_err());
        assert!(check_path_set(["a", "a/b"]).is_err());
        assert!(check_path_set(["A/b", "a"]).is_err());
    }
}
