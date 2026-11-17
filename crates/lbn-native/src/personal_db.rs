//! Personal data in SQLite (`personal.sqlite`), separate from packs and from the registry.
//!
//! Migrations: before any schema change, the whole database is copied to `backups/personal-<time>-v<old>.sqlite`
//! (`VACUUM INTO`), then old rows are converted with `lbn_core::personal::migrate` (the same rules the browser uses)
//! inside one transaction. A database from a newer app (higher schema) is opened read-only for nothing: it is refused
//! and left untouched, so going back to an older app never damages data.
use crate::util::{now_compact, now_rfc3339, random_id};
use crate::{NativeError, Result};
use lbn_core::personal::{self, Bookmark, HistoryItem, Note, PersonalData, SavedAnswer, Settings};
use lbn_core::PERSONAL_SCHEMA_VERSION;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const HISTORY_MAX: i64 = 500;

pub struct PersonalDb {
    db: Mutex<Connection>,
    backups: PathBuf,
}

const V2_TABLES: &str = "
CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS bookmarks(id TEXT PRIMARY KEY, target TEXT NOT NULL, title TEXT NOT NULL, created TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS notes(id TEXT PRIMARY KEY, target TEXT NOT NULL, text TEXT NOT NULL, created TEXT NOT NULL, updated TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS history(n INTEGER PRIMARY KEY AUTOINCREMENT, target TEXT NOT NULL, title TEXT NOT NULL, at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS answers(id TEXT PRIMARY KEY, question TEXT NOT NULL, answer TEXT NOT NULL, citations TEXT NOT NULL, model TEXT NOT NULL, at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY, v TEXT NOT NULL);
";

/// The schema-1 layout written by 0.8 development builds; kept so tests can create one and upgrades can read it.
pub const V1_TABLES: &str = "
CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS bookmarks(id TEXT PRIMARY KEY, pack TEXT NOT NULL, resource TEXT NOT NULL, page INTEGER, title TEXT NOT NULL, created TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS history(n INTEGER PRIMARY KEY AUTOINCREMENT, target TEXT NOT NULL, title TEXT NOT NULL, at TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY, v TEXT NOT NULL);
INSERT OR IGNORE INTO meta(k,v) VALUES('schema','1');
";

impl PersonalDb {
    pub fn open(path: &Path, backups: &Path) -> Result<PersonalDb> {
        Self::open_with(path, backups, PERSONAL_SCHEMA_VERSION)
    }

    /// `max_schema` is the newest schema this app understands (a parameter so tests can play an older app).
    pub fn open_with(path: &Path, backups: &Path, max_schema: u32) -> Result<PersonalDb> {
        std::fs::create_dir_all(backups)?;
        let fresh = !path.exists();
        let db = Connection::open(path)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT NOT NULL);")?;
        let schema: u32 = db
            .query_row("SELECT v FROM meta WHERE k='schema'", [], |r| r.get::<_, String>(0))
            .optional()?
            .map(|s| s.parse().unwrap_or(u32::MAX))
            .unwrap_or(if fresh { max_schema } else { 0 });
        if schema > max_schema {
            return Err(NativeError::Core(lbn_core::Error::Personal(format!(
                "your personal data was saved by a newer version of the app (schema {schema}; this version understands up to \
                 {max_schema}). Nothing was changed. Open it with the newer app, or restore a backup from {}.",
                backups.display()
            ))));
        }
        let me = PersonalDb { db: Mutex::new(db), backups: backups.to_path_buf() };
        if fresh || schema == 0 {
            let db = me.db.lock().unwrap();
            db.execute_batch(V2_TABLES)?;
            db.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('schema',?1)", [PERSONAL_SCHEMA_VERSION.to_string()])?;
        } else if schema < PERSONAL_SCHEMA_VERSION {
            me.migrate_from(schema)?;
        }
        Ok(me)
    }

    pub fn backup(&self, label: &str) -> Result<PathBuf> {
        let dest = self.backups.join(format!("personal-{}-{label}.sqlite", now_compact()));
        let dest = if dest.exists() {
            self.backups.join(format!("personal-{}-{label}-{}.sqlite", now_compact(), random_id()))
        } else {
            dest
        };
        self.db.lock().unwrap().execute("VACUUM INTO ?1", [dest.to_string_lossy()])?;
        Ok(dest)
    }

    fn migrate_from(&self, schema: u32) -> Result<()> {
        self.backup(&format!("v{schema}"))?;
        let mut db = self.db.lock().unwrap();
        // Read the old rows into the JSON shape `lbn_core::personal::migrate` understands.
        let old = match schema {
            1 => {
                let mut bms = Vec::new();
                {
                    let mut st = db.prepare("SELECT id,pack,resource,page,title,created FROM bookmarks")?;
                    let rows = st.query_map([], |r| {
                        Ok(serde_json::json!({"id": r.get::<_, String>(0)?, "pack": r.get::<_, String>(1)?, "resource": r.get::<_, String>(2)?,
                            "page": r.get::<_, Option<i64>>(3)?, "title": r.get::<_, String>(4)?, "created": r.get::<_, String>(5)?}))
                    })?;
                    for r in rows {
                        bms.push(r?);
                    }
                }
                let history = read_history(&db)?;
                let settings: Option<String> =
                    db.query_row("SELECT v FROM settings WHERE k='settings'", [], |r| r.get(0)).optional()?;
                serde_json::json!({"bookmarks": bms, "history": history,
                    "settings": settings.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()).unwrap_or(serde_json::json!({}))})
            }
            s => return Err(NativeError::coded("incompatible", format!("no migration from personal schema {s}"))),
        };
        let data = personal::migrate(schema, old)?;
        let tx = db.transaction()?;
        tx.execute_batch("DROP TABLE IF EXISTS bookmarks; DROP TABLE IF EXISTS history; DROP TABLE IF EXISTS settings;")?;
        tx.execute_batch(V2_TABLES)?;
        write_all(&tx, &data)?;
        tx.execute("INSERT OR REPLACE INTO meta(k,v) VALUES('schema',?1)", [PERSONAL_SCHEMA_VERSION.to_string()])?;
        tx.commit()?;
        Ok(())
    }

    pub fn load(&self) -> Result<PersonalData> {
        let db = self.db.lock().unwrap();
        let mut d = PersonalData::default();
        {
            let mut st = db.prepare("SELECT id,target,title,created FROM bookmarks ORDER BY created")?;
            d.bookmarks = st
                .query_map([], |r| Ok(Bookmark { id: r.get(0)?, target: r.get(1)?, title: r.get(2)?, created: r.get(3)? }))?
                .collect::<std::result::Result<_, _>>()?;
            let mut st = db.prepare("SELECT id,target,text,created,updated FROM notes ORDER BY updated DESC")?;
            d.notes = st
                .query_map([], |r| {
                    Ok(Note { id: r.get(0)?, target: r.get(1)?, text: r.get(2)?, created: r.get(3)?, updated: r.get(4)? })
                })?
                .collect::<std::result::Result<_, _>>()?;
            let mut st = db.prepare("SELECT id,question,answer,citations,model,at FROM answers ORDER BY at DESC")?;
            d.answers = st
                .query_map([], |r| {
                    Ok(SavedAnswer {
                        id: r.get(0)?,
                        question: r.get(1)?,
                        answer: r.get(2)?,
                        citations: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                        model: r.get(4)?,
                        at: r.get(5)?,
                    })
                })?
                .collect::<std::result::Result<_, _>>()?;
        }
        d.history = serde_json::from_value(serde_json::Value::Array(read_history(&db)?)).unwrap_or_default();
        d.settings = db
            .query_row("SELECT v FROM settings WHERE k='settings'", [], |r| r.get::<_, String>(0))
            .optional()?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Ok(d)
    }

    pub fn save_bookmark(&self, target: &str, title: &str) -> Result<Bookmark> {
        lbn_core::citation::Citation::parse(target)?;
        let db = self.db.lock().unwrap();
        if let Some(b) = db
            .query_row("SELECT id,target,title,created FROM bookmarks WHERE target=?1", [target], |r| {
                Ok(Bookmark { id: r.get(0)?, target: r.get(1)?, title: r.get(2)?, created: r.get(3)? })
            })
            .optional()?
        {
            return Ok(b);
        }
        let b = Bookmark { id: random_id(), target: target.into(), title: title.into(), created: now_rfc3339() };
        db.execute(
            "INSERT INTO bookmarks(id,target,title,created) VALUES(?1,?2,?3,?4)",
            params![b.id, b.target, b.title, b.created],
        )?;
        Ok(b)
    }

    pub fn delete_bookmark(&self, id: &str) -> Result<()> {
        self.db.lock().unwrap().execute("DELETE FROM bookmarks WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn save_note(&self, id: Option<&str>, target: &str, text: &str) -> Result<Note> {
        lbn_core::citation::Citation::parse(target)?;
        if text.len() > 1_000_000 {
            return Err(NativeError::coded("personal", "note too large"));
        }
        let now = now_rfc3339();
        let db = self.db.lock().unwrap();
        let id = id.map(String::from).unwrap_or_else(random_id);
        let created: String =
            db.query_row("SELECT created FROM notes WHERE id=?1", [&id], |r| r.get(0)).optional()?.unwrap_or_else(|| now.clone());
        db.execute(
            "INSERT INTO notes(id,target,text,created,updated) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(id) DO UPDATE SET target=excluded.target, text=excluded.text, updated=excluded.updated",
            params![id, target, text, created, now],
        )?;
        Ok(Note { id, target: target.into(), text: text.into(), created, updated: now })
    }

    pub fn delete_note(&self, id: &str) -> Result<()> {
        self.db.lock().unwrap().execute("DELETE FROM notes WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn add_history(&self, target: &str, title: &str) -> Result<()> {
        lbn_core::citation::Citation::parse(target)?;
        let db = self.db.lock().unwrap();
        db.execute("INSERT INTO history(target,title,at) VALUES(?1,?2,?3)", params![target, title, now_rfc3339()])?;
        db.execute("DELETE FROM history WHERE n <= (SELECT MAX(n) FROM history) - ?1", [HISTORY_MAX])?;
        Ok(())
    }

    pub fn save_answer(&self, a: &SavedAnswer) -> Result<()> {
        self.db.lock().unwrap().execute(
            "INSERT OR REPLACE INTO answers(id,question,answer,citations,model,at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![a.id, a.question, a.answer, serde_json::to_string(&a.citations).unwrap(), a.model, a.at],
        )?;
        Ok(())
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.db
            .lock()
            .unwrap()
            .execute("INSERT OR REPLACE INTO settings(k,v) VALUES('settings',?1)", [serde_json::to_string(s).unwrap()])?;
        Ok(())
    }

    pub fn export_json(&self, app_version: &str) -> Result<String> {
        let d = self.load()?;
        Ok(serde_json::to_string_pretty(&personal::export(&d, &now_rfc3339(), app_version)).unwrap())
    }

    /// Import an export file (any known schema) by merging; a backup is taken first.
    pub fn import_json(&self, bytes: &[u8]) -> Result<PersonalData> {
        let ex = personal::parse_export(bytes)?;
        self.backup("before-import")?;
        let mut d = self.load()?;
        personal::merge(&mut d, ex.data);
        personal::validate(&d)?;
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        tx.execute_batch("DELETE FROM bookmarks; DELETE FROM notes; DELETE FROM answers; DELETE FROM history;")?;
        write_all(&tx, &d)?;
        tx.commit()?;
        drop(db);
        self.load()
    }
}

fn read_history(db: &Connection) -> Result<Vec<serde_json::Value>> {
    let mut st = db.prepare("SELECT target,title,at FROM history ORDER BY n")?;
    let v = st
        .query_map([], |r| Ok(serde_json::json!({"target": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "at": r.get::<_, String>(2)?})))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(v)
}

fn write_all(tx: &rusqlite::Transaction<'_>, d: &PersonalData) -> Result<()> {
    for b in &d.bookmarks {
        tx.execute(
            "INSERT OR REPLACE INTO bookmarks(id,target,title,created) VALUES(?1,?2,?3,?4)",
            params![b.id, b.target, b.title, b.created],
        )?;
    }
    for n in &d.notes {
        tx.execute(
            "INSERT OR REPLACE INTO notes(id,target,text,created,updated) VALUES(?1,?2,?3,?4,?5)",
            params![n.id, n.target, n.text, n.created, n.updated],
        )?;
    }
    for h in &d.history {
        let HistoryItem { target, title, at } = h;
        tx.execute("INSERT INTO history(target,title,at) VALUES(?1,?2,?3)", params![target, title, at])?;
    }
    for a in &d.answers {
        tx.execute(
            "INSERT OR REPLACE INTO answers(id,question,answer,citations,model,at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![a.id, a.question, a.answer, serde_json::to_string(&a.citations).unwrap(), a.model, a.at],
        )?;
    }
    tx.execute("INSERT OR REPLACE INTO settings(k,v) VALUES('settings',?1)", [serde_json::to_string(&d.settings).unwrap()])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_from_v1_keeps_data_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personal.sqlite");
        {
            let db = Connection::open(&path).unwrap();
            db.execute_batch(V1_TABLES).unwrap();
            db.execute("INSERT INTO bookmarks VALUES('b1','org.lastbook.sample.docs','checklist',2,'Checklist p2','2026-01-01T00:00:00Z')", []).unwrap();
            db.execute("INSERT INTO history(target,title,at) VALUES('lbn:org.lastbook.sample.docs/water-guide','Water','2026-01-02T00:00:00Z')", []).unwrap();
            db.execute("INSERT INTO settings VALUES('settings','{\"ai_enabled\":true,\"text_size\":120}')", []).unwrap();
        }
        let p = PersonalDb::open(&path, &dir.path().join("backups")).unwrap();
        let d = p.load().unwrap();
        assert_eq!(d.bookmarks[0].target, "lbn:org.lastbook.sample.docs/checklist#page=2");
        assert_eq!(d.history.len(), 1);
        assert!(d.settings.ai_enabled);
        assert_eq!(d.settings.text_size, 120);
        let backups: Vec<_> = std::fs::read_dir(dir.path().join("backups")).unwrap().flatten().collect();
        assert_eq!(backups.len(), 1);
        assert!(backups[0].file_name().to_string_lossy().ends_with("-v1.sqlite"));
        // The backup is a usable v1 database.
        let b = Connection::open(backups[0].path()).unwrap();
        let s: String = b.query_row("SELECT v FROM meta WHERE k='schema'", [], |r| r.get(0)).unwrap();
        assert_eq!(s, "1");
    }

    #[test]
    fn older_app_refuses_newer_data_without_touching_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("personal.sqlite");
        let p = PersonalDb::open(&path, &dir.path().join("backups")).unwrap();
        p.save_bookmark("lbn:org.a.b/x", "X").unwrap();
        drop(p);
        let before = std::fs::read(&path).unwrap();
        let e = PersonalDb::open_with(&path, &dir.path().join("backups"), 1).err().unwrap();
        assert!(e.to_string().contains("newer version"));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(PersonalDb::open(&path, &dir.path().join("backups")).unwrap().load().unwrap().bookmarks.len(), 1);
    }

    #[test]
    fn export_import_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let a = PersonalDb::open(&dir.path().join("a.sqlite"), &dir.path().join("ba")).unwrap();
        a.save_bookmark("lbn:org.a.b/x#page=3", "X").unwrap();
        a.save_note(None, "lbn:org.a.b/x", "remember this").unwrap();
        a.add_history("lbn:org.a.b/x", "X").unwrap();
        a.save_settings(&Settings { ai_enabled: true, ..Default::default() }).unwrap();
        let json = a.export_json("0.9.0").unwrap();
        let b = PersonalDb::open(&dir.path().join("b.sqlite"), &dir.path().join("bb")).unwrap();
        let d = b.import_json(json.as_bytes()).unwrap();
        assert_eq!(d.bookmarks.len(), 1);
        assert_eq!(d.notes[0].text, "remember this");
        assert!(b.import_json(b"not json").is_err());
        assert!(a.save_bookmark("javascript:alert(1)", "x").is_err());
    }
}
