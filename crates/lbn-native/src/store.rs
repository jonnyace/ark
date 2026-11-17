//! The native pack store: registry (SQLite), immutable installed builds, staging, verification, atomic activation,
//! rollback, removal and recovery after a crash.
use crate::util::{hash_file, join_rel, now_rfc3339, sync_dir};
use crate::{NativeError, Result};
use lbn_core::api::{PackHealth, VerifyReport};
use lbn_core::index::Index;
use lbn_core::install::{can_activate, ActivationOptions, InstalledPack};
use lbn_core::manifest::{self, Manifest, ParsedManifest, MANIFEST_NAME, SIGNATURE_NAME};
use lbn_core::signature::{self, TrustedKey};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const REGISTRY_SCHEMA: i64 = 1;

pub struct Store {
    root: PathBuf,
    db: Mutex<Connection>,
    trusted: Mutex<Vec<TrustedKey>>,
    indexes: Mutex<HashMap<String, Arc<Index>>>,
    manifests: Mutex<HashMap<String, Arc<Manifest>>>,
}

/// What a staged build turned out to be after full verification.
pub struct Verified {
    pub parsed: ParsedManifest,
    pub signed_by: Option<String>,
}

impl Store {
    pub fn open(root: &Path, builtin_keys: Vec<TrustedKey>) -> Result<Store> {
        for d in ["packs", "staging", "trash", "backups"] {
            std::fs::create_dir_all(root.join(d))?;
        }
        let db = Connection::open(root.join("registry.sqlite"))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
             CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS builds(
               id TEXT NOT NULL, sha TEXT NOT NULL, version TEXT NOT NULL, dir TEXT NOT NULL, kind TEXT NOT NULL,
               title TEXT NOT NULL, bytes INTEGER NOT NULL, installed_at TEXT NOT NULL, signed_by TEXT,
               source TEXT, PRIMARY KEY(id, sha));
             CREATE TABLE IF NOT EXISTS active(
               id TEXT PRIMARY KEY, sha TEXT NOT NULL, previous TEXT, activated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY, json TEXT NOT NULL);",
        )?;
        let schema: Option<String> = db.query_row("SELECT v FROM meta WHERE k='schema'", [], |r| r.get(0)).optional()?;
        match schema.as_deref().map(|s| s.parse::<i64>().unwrap_or(i64::MAX)) {
            None => {
                db.execute("INSERT INTO meta(k,v) VALUES('schema',?1)", [REGISTRY_SCHEMA.to_string()])?;
            }
            Some(v) if v > REGISTRY_SCHEMA => {
                return Err(NativeError::coded(
                    "incompatible",
                    format!("the library registry was written by a newer app (schema {v}); use that app or a newer one"),
                ))
            }
            Some(_) => {}
        }
        let mut keys = builtin_keys;
        if let Ok(b) = std::fs::read(root.join("trusted-keys.json")) {
            if let Ok(extra) = serde_json::from_slice::<Vec<TrustedKey>>(&b) {
                keys.extend(extra);
            }
        }
        let s = Store {
            root: root.to_path_buf(),
            db: Mutex::new(db),
            trusted: Mutex::new(keys),
            indexes: Mutex::new(HashMap::new()),
            manifests: Mutex::new(HashMap::new()),
        };
        s.recover()?;
        Ok(s)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn staging_dir(&self, job: &str) -> PathBuf {
        self.root.join("staging").join(job)
    }
    pub fn trusted_keys(&self) -> Vec<TrustedKey> {
        self.trusted.lock().unwrap().clone()
    }
    pub fn add_trusted_key(&self, k: TrustedKey) -> Result<()> {
        let mut keys = self.trusted.lock().unwrap();
        let path = self.root.join("trusted-keys.json");
        let mut extra: Vec<TrustedKey> =
            std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        extra.push(k.clone());
        crate::util::write_atomic(&path, &serde_json::to_vec_pretty(&extra).unwrap())?;
        keys.push(k);
        Ok(())
    }

    pub(crate) fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap()
    }

    /// After a crash: finish activations whose build directory is in place, move unknown build directories to trash,
    /// empty the trash. Staging directories are kept: jobs resume from them.
    fn recover(&self) -> Result<()> {
        let known: Vec<String> = {
            let db = self.db();
            let mut st = db.prepare("SELECT dir FROM builds")?;
            let v = st.query_map([], |r| r.get::<_, String>(0))?.collect::<std::result::Result<Vec<_>, _>>()?;
            v
        };
        let packs = self.root.join("packs");
        for id_dir in std::fs::read_dir(&packs)?.flatten() {
            for b in std::fs::read_dir(id_dir.path())?.flatten() {
                let rel = format!("packs/{}/{}", id_dir.file_name().to_string_lossy(), b.file_name().to_string_lossy());
                if !known.contains(&rel) {
                    let dest = self.root.join("trash").join(format!("orphan-{}", crate::util::random_id()));
                    let _ = std::fs::rename(b.path(), dest);
                }
            }
        }
        self.empty_trash();
        Ok(())
    }

    pub fn empty_trash(&self) {
        if let Ok(rd) = std::fs::read_dir(self.root.join("trash")) {
            for e in rd.flatten() {
                let _ = if e.path().is_dir() { std::fs::remove_dir_all(e.path()) } else { std::fs::remove_file(e.path()) };
            }
        }
    }

    // ------------------------------------------------------------------------------------------------ queries --

    pub fn builds(&self) -> Result<Vec<InstalledPack>> {
        let db = self.db();
        let mut st = db.prepare(
            "SELECT b.id,b.version,b.sha,b.dir,b.kind,b.title,b.bytes,b.installed_at,b.signed_by,(a.sha IS NOT NULL)
             FROM builds b LEFT JOIN active a ON a.id=b.id AND a.sha=b.sha ORDER BY b.id, b.installed_at",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(InstalledPack {
                    id: r.get(0)?,
                    version: r.get(1)?,
                    manifest_sha256: r.get(2)?,
                    dir: r.get(3)?,
                    kind: serde_json::from_value(serde_json::Value::String(r.get(4)?)).unwrap_or(manifest::PackKind::Mixed),
                    title: r.get(5)?,
                    bytes: r.get::<_, i64>(6)? as u64,
                    installed_at: r.get(7)?,
                    signed_by: r.get(8)?,
                    active: r.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn active(&self) -> Result<Vec<InstalledPack>> {
        Ok(self.builds()?.into_iter().filter(|b| b.active).collect())
    }

    pub fn active_build(&self, id: &str) -> Result<Option<InstalledPack>> {
        Ok(self.active()?.into_iter().find(|b| b.id == id))
    }

    pub fn build_dir(&self, b: &InstalledPack) -> PathBuf {
        self.root.join(&b.dir)
    }

    pub fn manifest_of(&self, b: &InstalledPack) -> Result<Arc<Manifest>> {
        if let Some(m) = self.manifests.lock().unwrap().get(&b.manifest_sha256) {
            return Ok(m.clone());
        }
        let bytes = std::fs::read(self.build_dir(b).join(MANIFEST_NAME))?;
        let p = manifest::parse(&bytes)?;
        if p.sha256 != b.manifest_sha256 {
            return Err(NativeError::coded("damaged", format!("{}: pack.json was changed after installation", b.id)));
        }
        let m = Arc::new(p.manifest);
        self.manifests.lock().unwrap().insert(b.manifest_sha256.clone(), m.clone());
        Ok(m)
    }

    pub fn index_of(&self, b: &InstalledPack) -> Result<Option<Arc<Index>>> {
        if let Some(ix) = self.indexes.lock().unwrap().get(&b.manifest_sha256) {
            return Ok(Some(ix.clone()));
        }
        let m = self.manifest_of(b)?;
        let Some(ixref) = &m.index else { return Ok(None) };
        let path = join_rel(&self.build_dir(b), &ixref.search)?;
        let bytes = std::fs::read(&path)?;
        let entry = m.file(&ixref.search).ok_or_else(|| NativeError::coded("damaged", "index not in manifest"))?;
        // The index is read on every search session; check it against the manifest before trusting it.
        lbn_core::install::check_file(entry, bytes.len() as u64, &lbn_core::hash::sha256_hex(&bytes))?;
        let ix = Arc::new(Index::from_bytes(&bytes)?);
        if ix.pack() != m.id {
            return Err(NativeError::coded("damaged", "index belongs to another pack"));
        }
        self.indexes.lock().unwrap().insert(b.manifest_sha256.clone(), ix.clone());
        Ok(Some(ix))
    }

    /// Absolute path of a file of the active build of `pack` (path validated, file must be in the manifest).
    pub fn file_path(&self, pack: &str, rel: &str) -> Result<PathBuf> {
        let b =
            self.active_build(pack)?.ok_or_else(|| NativeError::coded("not_found", format!("pack {pack} is not installed")))?;
        let m = self.manifest_of(&b)?;
        if m.file(rel).is_none() {
            return Err(NativeError::coded("not_found", format!("{rel} is not part of {pack}")));
        }
        join_rel(&self.build_dir(&b), rel)
    }

    /// Cheap health check used when listing: every file present at its manifest size.
    pub fn quick_health(&self, b: &InstalledPack) -> PackHealth {
        let Ok(m) = self.manifest_of(b) else { return PackHealth::Damaged };
        let dir = self.build_dir(b);
        for f in &m.files {
            match join_rel(&dir, &f.path).ok().and_then(|p| std::fs::metadata(p).ok()) {
                Some(md) if md.len() == f.size => {}
                _ => return PackHealth::Damaged,
            }
        }
        PackHealth::Ok
    }

    /// Re-hash every file of the active build.
    pub fn verify_build(&self, b: &InstalledPack, mut progress: impl FnMut(u64)) -> Result<VerifyReport> {
        let m = self.manifest_of(b)?;
        let dir = self.build_dir(b);
        let mut rep = VerifyReport {
            pack: b.id.clone(),
            version: b.version.clone(),
            ok: true,
            checked: 0,
            damaged: vec![],
            missing: vec![],
        };
        for f in &m.files {
            let p = join_rel(&dir, &f.path)?;
            if !p.is_file() {
                rep.missing.push(f.path.clone());
                continue;
            }
            let (size, sha) = hash_file(&p, &mut progress)?;
            rep.checked += 1;
            if lbn_core::install::check_file(f, size, &sha).is_err() {
                rep.damaged.push(f.path.clone());
            }
        }
        rep.ok = rep.damaged.is_empty() && rep.missing.is_empty();
        Ok(rep)
    }

    // ----------------------------------------------------------------------------------------------- staging --

    /// Fully verify a staged build: manifest valid, signature policy, every file's size and SHA-256, no extra files.
    pub fn verify_staged(&self, dir: &Path, allow_unsigned: bool, mut progress: impl FnMut(u64)) -> Result<Verified> {
        let mbytes = std::fs::read(dir.join(MANIFEST_NAME))?;
        let parsed = manifest::parse(&mbytes)?;
        let signed_by = self.check_signature(dir, &mbytes, allow_unsigned)?;
        for f in &parsed.manifest.files {
            let p = join_rel(dir, &f.path)?;
            let md =
                std::fs::symlink_metadata(&p).map_err(|_| NativeError::coded("missing", format!("{} is missing", f.path)))?;
            if !md.file_type().is_file() {
                return Err(NativeError::coded("unsafe_path", format!("{} is not a regular file", f.path)));
            }
            let (size, sha) = hash_file(&p, &mut progress)?;
            lbn_core::install::check_file(f, size, &sha)?;
        }
        let mut extra = Vec::new();
        list_files(dir, dir, &mut extra)?;
        let listed: std::collections::HashSet<&str> = parsed.manifest.files.iter().map(|f| f.path.as_str()).collect();
        for e in extra {
            if e != MANIFEST_NAME
                && e != SIGNATURE_NAME
                && !listed.contains(e.as_str())
                && !e.ends_with(".part")
                && e != "job.json"
            {
                return Err(NativeError::coded("manifest", format!("{e} is in the pack but not in its manifest")));
            }
        }
        Ok(Verified { parsed, signed_by })
    }

    pub fn check_signature(&self, dir: &Path, mbytes: &[u8], allow_unsigned: bool) -> Result<Option<String>> {
        match std::fs::read_to_string(dir.join(SIGNATURE_NAME)) {
            Ok(sig) => {
                let keys = self.trusted_keys();
                let k = signature::verify(mbytes, &sig, &keys)?;
                Ok(Some(k.name.clone()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if allow_unsigned {
                    Ok(None)
                } else {
                    Err(NativeError::coded(
                        "unsigned",
                        "this pack is not signed by a trusted publisher; install it only if you trust where it came from",
                    ))
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Move a verified staged build into place and make it the active build of its pack, in one registry transaction.
    /// The previously active build is kept for rollback.
    pub fn activate(&self, staged: &Path, v: &Verified, source: Option<&str>, opts: ActivationOptions) -> Result<InstalledPack> {
        let m = &v.parsed.manifest;
        let current = self.active_build(&m.id)?;
        can_activate(m, current.as_ref(), opts).map_err(|e| NativeError::coded("downgrade", e.to_string()))?;
        let rel = format!("packs/{}/{}", m.id, v.parsed.install_dir_name());
        let dest = self.root.join(&rel);
        let existing = self.builds()?.into_iter().find(|b| b.manifest_sha256 == v.parsed.sha256 && b.id == m.id);
        // The same build is already registered: keep it if intact; if it is damaged (a repair), replace its folder
        // with the freshly verified copy.
        let replace_damaged = existing.as_ref().is_some_and(|b| self.quick_health(b) != PackHealth::Ok);
        if replace_damaged {
            let b = existing.as_ref().unwrap();
            std::fs::rename(self.build_dir(b), self.root.join("trash").join(format!("damaged-{}", crate::util::random_id())))?;
            self.indexes.lock().unwrap().remove(&b.manifest_sha256);
        }
        let already = existing.is_some() && !replace_damaged;
        if !already {
            std::fs::create_dir_all(dest.parent().unwrap())?;
            if dest.exists() {
                // Left over from an interrupted activation: replace it with the freshly verified copy.
                std::fs::rename(&dest, self.root.join("trash").join(format!("stale-{}", crate::util::random_id())))?;
            }
            let _ = std::fs::remove_file(staged.join("job.json"));
            std::fs::rename(staged, &dest)?;
            sync_dir(dest.parent().unwrap());
        } else {
            let _ = std::fs::remove_dir_all(staged);
        }
        let now = now_rfc3339();
        let kind = serde_json::to_value(m.kind).unwrap().as_str().unwrap().to_string();
        {
            let mut db = self.db();
            let tx = db.transaction()?;
            tx.execute(
                "INSERT OR IGNORE INTO builds(id,sha,version,dir,kind,title,bytes,installed_at,signed_by,source)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![m.id, v.parsed.sha256, m.version, rel, kind, m.title, m.total_bytes() as i64, now, v.signed_by, source],
            )?;
            let prev: Option<String> = tx.query_row("SELECT sha FROM active WHERE id=?1", [&m.id], |r| r.get(0)).optional()?;
            let prev = prev.filter(|p| p != &v.parsed.sha256);
            tx.execute(
                "INSERT INTO active(id,sha,previous,activated_at) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(id) DO UPDATE SET sha=excluded.sha, previous=COALESCE(excluded.previous, active.previous), activated_at=excluded.activated_at",
                params![m.id, v.parsed.sha256, prev, now],
            )?;
            tx.commit()?;
        }
        self.active_build(&m.id)?.ok_or_else(|| NativeError::coded("activate", "activation did not take effect"))
    }

    /// Make the previous build active again (if it is still installed and intact).
    pub fn rollback(&self, id: &str) -> Result<InstalledPack> {
        let (cur, prev): (String, Option<String>) = self
            .db()
            .query_row("SELECT sha, previous FROM active WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| NativeError::coded("not_found", format!("{id} is not installed")))?;
        let prev = prev.ok_or_else(|| NativeError::coded("nothing", "there is no earlier version to go back to"))?;
        let b = self
            .builds()?
            .into_iter()
            .find(|b| b.id == id && b.manifest_sha256 == prev)
            .ok_or_else(|| NativeError::coded("nothing", "the earlier version has been removed"))?;
        if self.quick_health(&b) != PackHealth::Ok {
            return Err(NativeError::coded("damaged", "the earlier version is damaged"));
        }
        self.db().execute(
            "UPDATE active SET sha=?2, previous=?3, activated_at=?4 WHERE id=?1",
            params![id, prev, cur, now_rfc3339()],
        )?;
        self.active_build(id)?.ok_or_else(|| NativeError::coded("activate", "rollback failed"))
    }

    /// Remove all builds of a pack, or one inactive build (by version).
    pub fn remove(&self, id: &str, version: Option<&str>) -> Result<u64> {
        let builds: Vec<InstalledPack> = self.builds()?.into_iter().filter(|b| b.id == id).collect();
        if builds.is_empty() {
            return Err(NativeError::coded("not_found", format!("{id} is not installed")));
        }
        let targets: Vec<&InstalledPack> = match version {
            Some(v) => {
                let t: Vec<_> = builds.iter().filter(|b| b.version == v).collect();
                if t.iter().any(|b| b.active) {
                    return Err(NativeError::coded("busy", "that version is in use; remove the whole pack or switch first"));
                }
                t
            }
            None => builds.iter().collect(),
        };
        let mut freed = 0;
        {
            let mut db = self.db();
            let tx = db.transaction()?;
            for b in &targets {
                tx.execute("DELETE FROM builds WHERE id=?1 AND sha=?2", params![b.id, b.manifest_sha256])?;
                tx.execute("UPDATE active SET previous=NULL WHERE id=?1 AND previous=?2", params![b.id, b.manifest_sha256])?;
                freed += b.bytes;
            }
            if version.is_none() {
                tx.execute("DELETE FROM active WHERE id=?1", [id])?;
            }
            tx.commit()?;
        }
        for b in &targets {
            let _ =
                std::fs::rename(self.build_dir(b), self.root.join("trash").join(format!("removed-{}", crate::util::random_id())));
            self.indexes.lock().unwrap().remove(&b.manifest_sha256);
            self.manifests.lock().unwrap().remove(&b.manifest_sha256);
        }
        self.empty_trash();
        Ok(freed)
    }

    pub fn source_of(&self, b: &InstalledPack) -> Result<Option<String>> {
        Ok(self
            .db()
            .query_row("SELECT source FROM builds WHERE id=?1 AND sha=?2", params![b.id, b.manifest_sha256], |r| r.get(0))
            .optional()?
            .flatten())
    }

    // ---------------------------------------------------------------------------------------------------- jobs --

    pub fn save_job(&self, id: &str, json: &serde_json::Value) -> Result<()> {
        self.db().execute(
            "INSERT INTO jobs(id,json) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET json=excluded.json",
            params![id, json.to_string()],
        )?;
        Ok(())
    }
    pub fn delete_job(&self, id: &str) -> Result<()> {
        self.db().execute("DELETE FROM jobs WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn jobs(&self) -> Result<Vec<(String, serde_json::Value)>> {
        let db = self.db();
        let mut st = db.prepare("SELECT id, json FROM jobs")?;
        let v = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .filter_map(|(id, j)| serde_json::from_str(&j).ok().map(|v| (id, v)))
            .collect();
        Ok(v)
    }
}

fn list_files(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let ft = e.file_type()?;
        let p = e.path();
        if ft.is_dir() {
            list_files(root, &p, out)?;
        } else {
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if ft.is_symlink() {
                return Err(NativeError::coded("unsafe_path", format!("{rel} is a link")));
            }
            out.push(rel);
        }
    }
    Ok(())
}
