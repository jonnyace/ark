//! The native engine: implements every `lbn_core::api::Command` for desktop and CLI on top of the store, personal
//! database and AI runner. Long work (installs, answers) runs on threads and reports through `Event`s.
use crate::ai::Ai;
use crate::fetch::{base_url, file_url, manifest_url, Fetcher};
use crate::personal_db::PersonalDb;
use crate::store::Store;
use crate::util::{free_space, join_rel, now_rfc3339, random_id};
use crate::{legacy_import, unpack, NativeError, Result};
use lbn_core::api::*;
use lbn_core::citation::Citation;
use lbn_core::index::{search_many, Index};
use lbn_core::install::{self, ActivationOptions, JobError, JobProgress, JobState};
use lbn_core::manifest::{self, Manifest, PackKind, ResourceType, MANIFEST_NAME, SIGNATURE_NAME};
use lbn_core::personal::SavedAnswer;
use lbn_core::rag::{self, RetrievalConfig, Template};
use lbn_core::signature::TrustedKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub struct EngineOptions {
    pub platform: String,
    pub app_version: String,
    pub builtin_keys: Vec<TrustedKey>,
    pub runner_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobRecord {
    id: String,
    source: PackSource,
    allow_unsigned: bool,
    allow_downgrade: bool,
    /// For repairs: the pack id being repaired.
    #[serde(default)]
    repair_of: Option<String>,
    progress: JobProgress,
}

struct Running {
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
}

pub struct NativeEngine {
    pub store: Arc<Store>,
    pub personal: PersonalDb,
    pub ai: Arc<Ai>,
    events: EventSink,
    opts: EngineOptions,
    running: Mutex<HashMap<String, Running>>,
    asks: Mutex<HashMap<String, Arc<AtomicBool>>>,
    fits: Mutex<HashMap<String, Fit>>,
}

fn ok<T: Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v).unwrap())
}

impl NativeEngine {
    pub fn open(data: &Path, opts: EngineOptions, events: EventSink) -> Result<Arc<NativeEngine>> {
        std::fs::create_dir_all(data)?;
        let store = Arc::new(Store::open(data, opts.builtin_keys.clone())?);
        let personal = PersonalDb::open(&data.join("personal.sqlite"), &data.join("backups"))?;
        let ai = Arc::new(Ai::new(crate::ai::find_runner(&opts.runner_dirs)));
        let e = Arc::new(NativeEngine {
            store,
            personal,
            ai,
            events,
            opts,
            running: Mutex::new(HashMap::new()),
            asks: Mutex::new(HashMap::new()),
            fits: Mutex::new(HashMap::new()),
        });
        // Jobs interrupted by a restart become paused: the person resumes them (or they can be cancelled).
        for (id, v) in e.store.jobs()? {
            if let Ok(mut j) = serde_json::from_value::<JobRecord>(v) {
                if matches!(
                    j.progress.state,
                    JobState::Downloading | JobState::Verifying | JobState::Activating | JobState::Queued
                ) {
                    j.progress.state = JobState::Paused;
                    e.save_job(&j)?;
                }
            } else {
                e.store.delete_job(&id)?;
            }
        }
        Ok(e)
    }

    pub fn emit(&self, ev: Event) {
        (self.events)(ev)
    }

    /// Handle one request. Never panics on bad input; errors become `Reply::err`.
    pub fn handle(self: &Arc<Self>, req: Request) -> Reply {
        if req.v != lbn_core::ENGINE_API_VERSION {
            return Reply::err(
                req.id,
                "incompatible",
                format!("engine API {} is not supported (this engine speaks {})", req.v, lbn_core::ENGINE_API_VERSION),
            );
        }
        match self.dispatch(req.command) {
            Ok(v) => Reply { v: lbn_core::ENGINE_API_VERSION, id: req.id, ok: Some(v), err: None },
            Err(e) => Reply::err(req.id, e.code(), e.to_string()),
        }
    }

    pub fn handle_json(self: &Arc<Self>, json: &str) -> String {
        let reply = match serde_json::from_str::<Request>(json) {
            Ok(r) => self.handle(r),
            Err(e) => Reply::err(0, "bad_request", format!("bad request: {e}")),
        };
        serde_json::to_string(&reply).unwrap()
    }

    fn dispatch(self: &Arc<Self>, cmd: Command) -> Result<Value> {
        match cmd {
            Command::Hello { .. } => ok(HelloReply {
                api_version: lbn_core::ENGINE_API_VERSION,
                engine_version: self.opts.app_version.clone(),
                platform: self.opts.platform.clone(),
                capabilities: Capabilities {
                    ai_runtime: self.ai.runner().is_some(),
                    ai_unavailable_reason: self.ai.unavailable_reason(),
                    evictable_storage: false,
                    can_download: true,
                    can_import_files: true,
                    storage_unavailable_reason: None,
                },
            }),
            Command::ListPacks => ok(self.list_packs()?),
            Command::ListResources { pack } => ok(self.list_resources(pack.as_deref())?),
            Command::GetResource { pack, resource } => ok(self.resource_info(&pack, &resource)?),
            Command::ReadCatalog { url } => ok(self.read_catalog(&url)?),
            Command::InspectSource { source } => ok(self.inspect(&source)?),
            Command::Install { source, allow_unsigned, allow_downgrade } => {
                ok(self.start_install(source, allow_unsigned, allow_downgrade, None)?)
            }
            Command::PauseJob { job } => {
                if let Some(r) = self.running.lock().unwrap().get(&job) {
                    r.pause.store(true, Ordering::Relaxed);
                    r.cancel.store(true, Ordering::Relaxed);
                }
                ok(true)
            }
            Command::ResumeJob { job } => ok(self.resume(&job)?),
            Command::CancelJob { job } => ok(self.cancel_job(&job)?),
            Command::ListJobs => ok(self.jobs()?),
            Command::VerifyPack { pack } => {
                let b = self.active_or_err(&pack)?;
                ok(self.store.verify_build(&b, |_| {})?)
            }
            Command::RepairPack { pack } => ok(self.repair(&pack)?),
            Command::RemovePack { pack, version } => {
                if self.ai.loaded().as_deref() == Some(pack.as_str()) {
                    self.ai.unload();
                }
                let freed = self.store.remove(&pack, version.as_deref())?;
                self.emit(Event::PacksChanged);
                ok(serde_json::json!({ "freed_bytes": freed }))
            }
            Command::RollbackPack { pack } => {
                let b = self.store.rollback(&pack)?;
                self.emit(Event::PacksChanged);
                ok(b)
            }
            Command::Search { query, limit, packs } => ok(self.search(&query, limit.clamp(1, 100) as usize, packs)?),
            Command::GetChunk { pack, chunk } => {
                let b = self.active_or_err(&pack)?;
                let ix = self.store.index_of(&b)?.ok_or_else(|| NativeError::coded("not_found", "pack has no index"))?;
                ok(ix.chunk(chunk).cloned().ok_or_else(|| NativeError::coded("not_found", "no such chunk"))?)
            }
            Command::ResolveCitation { uri } => ok(self.resolve(&uri)),
            Command::AiStatus => ok(self.ai_status()?),
            Command::LoadModel { pack } => {
                self.load_model(&pack)?;
                ok(true)
            }
            Command::UnloadModel => {
                self.ai.unload();
                ok(true)
            }
            Command::Ask { question, model } => ok(self.ask(question, model)?),
            Command::CancelAsk { ask } => {
                if let Some(f) = self.asks.lock().unwrap().get(&ask) {
                    f.store(true, Ordering::Relaxed);
                }
                ok(true)
            }
            Command::GetPersonal => ok(PersonalReply { data: self.personal.load()?, schema: lbn_core::PERSONAL_SCHEMA_VERSION }),
            Command::SaveBookmark { target, title } => {
                let b = self.personal.save_bookmark(&target, &title)?;
                self.emit(Event::PersonalChanged);
                ok(b)
            }
            Command::DeleteBookmark { id } => {
                self.personal.delete_bookmark(&id)?;
                self.emit(Event::PersonalChanged);
                ok(true)
            }
            Command::SaveNote { id, target, text } => {
                let n = self.personal.save_note(id.as_deref(), &target, &text)?;
                self.emit(Event::PersonalChanged);
                ok(n)
            }
            Command::DeleteNote { id } => {
                self.personal.delete_note(&id)?;
                self.emit(Event::PersonalChanged);
                ok(true)
            }
            Command::AddHistory { target, title } => {
                self.personal.add_history(&target, &title)?;
                ok(true)
            }
            Command::SaveSettings { settings } => {
                self.personal.save_settings(&settings)?;
                if !settings.ai_enabled {
                    self.ai.unload();
                }
                self.emit(Event::PersonalChanged);
                ok(true)
            }
            Command::ExportPersonal => ok(self.personal.export_json(&self.opts.app_version)?),
            Command::ImportPersonal { data } => {
                let d = self.personal.import_json(data.as_bytes())?;
                self.emit(Event::PersonalChanged);
                ok(PersonalReply { data: d, schema: lbn_core::PERSONAL_SCHEMA_VERSION })
            }
            Command::StorageStatus => {
                let used: u64 = self.store.builds()?.iter().map(|b| b.bytes).sum();
                ok(StorageStatus {
                    used_bytes: used,
                    free_bytes: free_space(self.store.root()),
                    quota_bytes: None,
                    persisted: true,
                    evictable: false,
                    location: Some(self.store.root().display().to_string()),
                })
            }
            Command::RequestPersistence => ok(true),
        }
    }

    // ----------------------------------------------------------------------------------------------- library --

    fn active_or_err(&self, pack: &str) -> Result<lbn_core::install::InstalledPack> {
        self.store.active_build(pack)?.ok_or_else(|| NativeError::coded("not_found", format!("pack {pack} is not installed")))
    }

    pub fn list_packs(&self) -> Result<Vec<PackInfo>> {
        let builds = self.store.builds()?;
        let mut out = Vec::new();
        for b in builds.iter().filter(|b| b.active) {
            let health = self.store.quick_health(b);
            let m = self.store.manifest_of(b).ok();
            out.push(PackInfo {
                installed: b.clone(),
                description: m.as_ref().map(|m| m.description.clone()).unwrap_or_default(),
                publisher: m.as_ref().map(|m| m.publisher.name.clone()).unwrap_or_default(),
                license: m.as_ref().map(|m| m.license.clone()).unwrap_or(manifest::License {
                    spdx: "unknown".into(),
                    attribution: None,
                    file: None,
                    redistributable: false,
                }),
                resource_count: m.as_ref().map(|m| m.resources.len() as u32).unwrap_or(0),
                other_versions: builds.iter().filter(|o| o.id == b.id && !o.active).map(|o| o.version.clone()).collect(),
                health,
            });
        }
        Ok(out)
    }

    fn resource_info_from(&self, m: &Manifest, r: &manifest::Resource) -> ResourceInfo {
        ResourceInfo {
            pack: m.id.clone(),
            id: r.id.clone(),
            rtype: r.rtype,
            title: r.title.clone(),
            path: r.path.clone(),
            media_type: r.media_type.clone(),
            pages: r.pages,
            duration_s: r.duration_s,
            transcript: r.transcript.clone(),
            poster: r.poster.clone(),
            map: r.map.clone(),
            sections: r.sections.clone(),
            source: r.source.clone(),
            license: m.effective_license(r).clone(),
            category: r.category.clone(),
            summary: r.summary.clone(),
            pack_kind: m.kind,
        }
    }

    pub fn list_resources(&self, pack: Option<&str>) -> Result<Vec<ResourceInfo>> {
        let mut out = Vec::new();
        for b in self.store.active()? {
            if pack.is_some_and(|p| p != b.id) {
                continue;
            }
            let Ok(m) = self.store.manifest_of(&b) else { continue };
            out.extend(m.resources.iter().map(|r| self.resource_info_from(&m, r)));
        }
        Ok(out)
    }

    pub fn resource_info(&self, pack: &str, res: &str) -> Result<ResourceInfo> {
        let b = self.active_or_err(pack)?;
        let m = self.store.manifest_of(&b)?;
        let r = m.resource(res).ok_or_else(|| NativeError::coded("not_found", format!("{res} is not in {pack}")))?;
        Ok(self.resource_info_from(&m, r))
    }

    pub fn search(&self, query: &str, limit: usize, packs: Option<Vec<String>>) -> Result<SearchReply> {
        let mut indexes: Vec<Arc<Index>> = Vec::new();
        let mut skipped = Vec::new();
        for b in self.store.active()? {
            if b.kind == PackKind::Model || packs.as_ref().is_some_and(|p| !p.contains(&b.id)) {
                continue;
            }
            match self.store.index_of(&b) {
                Ok(Some(ix)) => indexes.push(ix),
                Ok(None) => {}
                Err(_) => skipped.push(b.id.clone()),
            }
        }
        let refs: Vec<&Index> = indexes.iter().map(|a| a.as_ref()).collect();
        Ok(SearchReply { hits: search_many(&refs, query, limit, 3), skipped })
    }

    pub fn resolve(&self, uri: &str) -> ResolvedCitation {
        let fail = |e: String, c: Option<Citation>| ResolvedCitation {
            uri: uri.into(),
            valid: false,
            error: Some(e),
            citation: c,
            resource: None,
        };
        let c = match Citation::parse(uri) {
            Ok(c) => c,
            Err(e) => return fail(e.to_string(), None),
        };
        let b = match self.store.active_build(&c.pack) {
            Ok(Some(b)) => b,
            _ => return fail(format!("the pack {} is not installed on this device", c.pack), Some(c)),
        };
        let m = match self.store.manifest_of(&b) {
            Ok(m) => m,
            Err(e) => return fail(e.to_string(), Some(c)),
        };
        if let Err(e) = c.validate_against(&m) {
            return fail(e.to_string(), Some(c));
        }
        let r = m.resource(&c.resource).unwrap();
        ResolvedCitation {
            uri: uri.into(),
            valid: true,
            error: None,
            resource: Some(self.resource_info_from(&m, r)),
            citation: Some(c),
        }
    }

    // --------------------------------------------------------------------------------------------- installing --

    fn inspect(&self, source: &PackSource) -> Result<SourceInfo> {
        let (mbytes, sig, unsupported) = match source {
            PackSource::Url { url } => {
                let f = Fetcher::new();
                let murl = manifest_url(url);
                let m =
                    f.get_small(&murl, 32 << 20)?.ok_or_else(|| NativeError::coded("not_found", format!("no pack at {url}")))?;
                let sig = f.get_small(&format!("{murl}.minisig"), 1 << 16)?;
                (m, sig, vec![])
            }
            PackSource::Local { path } => {
                let p = PathBuf::from(path);
                if let Some(lib) = legacy_import::find_library(&p) {
                    let info = legacy_import::inspect(&lib)?;
                    return Ok(SourceInfo {
                        id: info.id.clone(),
                        version: info.version.clone(),
                        kind: PackKind::Mixed,
                        title: info.title.clone(),
                        description:
                            "An existing Lastbook library. It will be copied into a new pack; the original is not changed.".into(),
                        publisher: "Imported from Lastbook".into(),
                        license: manifest::License {
                            spdx: "LicenseRef-Lastbook-Mixed".into(),
                            attribution: None,
                            file: None,
                            redistributable: false,
                        },
                        bytes: info.bytes,
                        files: info.plan.resources.len() as u32,
                        resources: info.plan.resources.len() as u32,
                        signed_by: None,
                        installed_version: self.store.active_build(&info.id)?.map(|b| b.version),
                        model: None,
                        unsupported: info.plan.unsupported,
                    });
                }
                if p.is_dir() {
                    let sig = std::fs::read(p.join(SIGNATURE_NAME)).ok();
                    (std::fs::read(p.join(MANIFEST_NAME))?, sig, vec![])
                } else {
                    let mut z = zip::ZipArchive::new(std::fs::File::open(&p)?)
                        .map_err(|e| NativeError::coded("unpack", format!("not a pack file: {e}")))?;
                    let mut read = |n: &str| -> Option<Vec<u8>> {
                        let mut e = z.by_name(n).ok()?;
                        let mut v = Vec::new();
                        std::io::Read::read_to_end(&mut std::io::Read::take(&mut e, 32 << 20), &mut v).ok()?;
                        Some(v)
                    };
                    let m = read(MANIFEST_NAME).ok_or_else(|| NativeError::coded("manifest", "no pack.json in this file"))?;
                    (m, read(SIGNATURE_NAME), vec![])
                }
            }
        };
        let parsed = manifest::parse(&mbytes)?;
        let signed_by = match sig {
            Some(s) => {
                Some(lbn_core::signature::verify(&mbytes, &String::from_utf8_lossy(&s), &self.store.trusted_keys())?.name.clone())
            }
            None => None,
        };
        let m = parsed.manifest;
        Ok(SourceInfo {
            installed_version: self.store.active_build(&m.id)?.map(|b| b.version),
            id: m.id.clone(),
            version: m.version.clone(),
            kind: m.kind,
            title: m.title.clone(),
            description: m.description.clone(),
            publisher: m.publisher.name.clone(),
            license: m.license.clone(),
            bytes: m.total_bytes(),
            files: m.files.len() as u32,
            resources: m.resources.len() as u32,
            signed_by,
            model: m.model.clone(),
            unsupported,
        })
    }

    fn read_catalog(&self, url: &str) -> Result<CatalogReply> {
        let f = Fetcher::new();
        let bytes = f.get_small(url, 8 << 20)?.ok_or_else(|| NativeError::coded("not_found", format!("no catalog at {url}")))?;
        let c = lbn_core::catalog::parse(&bytes)?;
        let signed_by = match f.get_small(&format!("{url}.minisig"), 1 << 16)? {
            Some(s) => {
                Some(lbn_core::signature::verify(&bytes, &String::from_utf8_lossy(&s), &self.store.trusted_keys())?.name.clone())
            }
            None => None,
        };
        let base = url.rsplit_once('/').map(|(b, _)| format!("{b}/")).unwrap_or_default();
        let mut packs = Vec::new();
        for e in c.packs.iter() {
            packs.push(CatalogPack {
                manifest_url: file_url(&base, &e.manifest),
                installed_version: self.store.active_build(&e.id)?.map(|b| b.version),
                entry: e.clone(),
            });
        }
        Ok(CatalogReply {
            url: url.into(),
            expired: lbn_core::catalog::is_expired(&c, &now_rfc3339()),
            title: c.title,
            published: c.published,
            expires: c.expires,
            signed_by,
            packs,
        })
    }

    fn save_job(&self, j: &JobRecord) -> Result<()> {
        self.store.save_job(&j.id, &serde_json::to_value(j).unwrap())
    }

    fn jobs(&self) -> Result<Vec<JobProgress>> {
        Ok(self
            .store
            .jobs()?
            .into_iter()
            .filter_map(|(_, v)| serde_json::from_value::<JobRecord>(v).ok())
            .map(|j| j.progress)
            .collect())
    }

    fn start_install(
        self: &Arc<Self>,
        source: PackSource,
        allow_unsigned: bool,
        allow_downgrade: bool,
        repair_of: Option<String>,
    ) -> Result<JobProgress> {
        if let PackSource::Url { url } = &source {
            crate::fetch::check_url(&manifest_url(url))?;
        }
        let id = format!("job-{}", random_id());
        let title = match &source {
            PackSource::Url { url } => url.clone(),
            PackSource::Local { path } => {
                Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
            }
        };
        let j = JobRecord {
            id: id.clone(),
            source,
            allow_unsigned,
            allow_downgrade,
            repair_of,
            progress: JobProgress {
                job: id.clone(),
                pack_id: String::new(),
                version: String::new(),
                title,
                state: JobState::Queued,
                bytes_done: 0,
                bytes_total: 0,
                files_done: 0,
                files_total: 0,
                current_file: None,
                error: None,
            },
        };
        self.save_job(&j)?;
        self.spawn_job(j.clone());
        Ok(j.progress)
    }

    fn resume(self: &Arc<Self>, job: &str) -> Result<JobProgress> {
        if self.running.lock().unwrap().contains_key(job) {
            return Err(NativeError::coded("busy", "already running"));
        }
        let v = self
            .store
            .jobs()?
            .into_iter()
            .find(|(id, _)| id == job)
            .map(|(_, v)| v)
            .ok_or_else(|| NativeError::coded("not_found", "no such job"))?;
        let mut j: JobRecord = serde_json::from_value(v).map_err(|e| NativeError::coded("io", e.to_string()))?;
        j.progress.error = None;
        j.progress.state = JobState::Queued;
        self.save_job(&j)?;
        self.spawn_job(j.clone());
        Ok(j.progress)
    }

    fn cancel_job(&self, job: &str) -> Result<bool> {
        if let Some(r) = self.running.lock().unwrap().get(job) {
            r.pause.store(false, Ordering::Relaxed);
            r.cancel.store(true, Ordering::Relaxed);
            return Ok(true); // the job thread cleans up
        }
        let _ = std::fs::remove_dir_all(self.store.staging_dir(job));
        self.store.delete_job(job)?;
        Ok(true)
    }

    fn spawn_job(self: &Arc<Self>, mut j: JobRecord) {
        let cancel = Arc::new(AtomicBool::new(false));
        let pause = Arc::new(AtomicBool::new(false));
        self.running.lock().unwrap().insert(j.id.clone(), Running { cancel: cancel.clone(), pause: pause.clone() });
        let me = self.clone();
        std::thread::spawn(move || {
            let res = me.run_job(&mut j, &cancel);
            me.running.lock().unwrap().remove(&j.id);
            match res {
                Ok(()) => {
                    j.progress.state = JobState::Done;
                    j.progress.current_file = None;
                    let _ = me.store.delete_job(&j.id);
                    me.emit(Event::Job(j.progress.clone()));
                    me.emit(Event::PacksChanged);
                }
                Err(e) if e.code() == "cancelled" && pause.load(Ordering::Relaxed) => {
                    j.progress.state = JobState::Paused;
                    let _ = me.save_job(&j);
                    me.emit(Event::Job(j.progress.clone()));
                }
                Err(e) if e.code() == "cancelled" => {
                    j.progress.state = JobState::Cancelled;
                    let _ = std::fs::remove_dir_all(me.store.staging_dir(&j.id));
                    let _ = me.store.delete_job(&j.id);
                    me.emit(Event::Job(j.progress.clone()));
                }
                Err(e) => {
                    // Damaged or untrusted content is thrown away; a network problem keeps the partial files to resume.
                    let keep = matches!(e.code(), "network" | "no_space" | "io");
                    if !keep {
                        let _ = std::fs::remove_dir_all(me.store.staging_dir(&j.id));
                    }
                    j.progress.state = JobState::Failed;
                    j.progress.error = Some(JobError { code: e.code().into(), message: e.to_string() });
                    let _ = me.save_job(&j);
                    me.emit(Event::Job(j.progress.clone()));
                }
            }
        });
    }

    fn run_job(&self, j: &mut JobRecord, cancel: &AtomicBool) -> Result<()> {
        let staged = self.store.staging_dir(&j.id);
        std::fs::create_dir_all(&staged)?;
        let mut last_emit = std::time::Instant::now();
        let mut report = |me: &NativeEngine, j: &mut JobRecord, force: bool| {
            if force || last_emit.elapsed().as_millis() > 200 {
                last_emit = std::time::Instant::now();
                let _ = me.save_job(j);
                me.emit(Event::Job(j.progress.clone()));
            }
        };
        j.progress.state = JobState::Downloading;
        let source_label: Option<String>;
        match j.source.clone() {
            PackSource::Url { url } => {
                let f = Fetcher::new();
                let murl = manifest_url(&url);
                source_label = Some(murl.clone());
                let mbytes =
                    f.get_small(&murl, 32 << 20)?.ok_or_else(|| NativeError::coded("not_found", format!("no pack at {url}")))?;
                let parsed = manifest::parse(&mbytes)?;
                // A different manifest than last time (the publisher updated the pack): start this staging over.
                if std::fs::read(staged.join(MANIFEST_NAME)).ok().is_some_and(|old| old != mbytes) {
                    std::fs::remove_dir_all(&staged)?;
                    std::fs::create_dir_all(&staged)?;
                }
                std::fs::write(staged.join(MANIFEST_NAME), &mbytes)?;
                match f.get_small(&format!("{murl}.minisig"), 1 << 16)? {
                    Some(sig) => std::fs::write(staged.join(SIGNATURE_NAME), sig)?,
                    None => {
                        let _ = std::fs::remove_file(staged.join(SIGNATURE_NAME));
                    }
                }
                // Refuse untrusted or tampered manifests before downloading anything large.
                self.store.check_signature(&staged, &mbytes, j.allow_unsigned)?;
                let m = &parsed.manifest;
                if let Some(cur) = self.store.active_build(&m.id)? {
                    if cur.manifest_sha256 == parsed.sha256 && j.repair_of.is_none() {
                        return Err(NativeError::coded("nothing", format!("{} {} is already installed", m.title, m.version)));
                    }
                }
                j.progress.pack_id = m.id.clone();
                j.progress.version = m.version.clone();
                j.progress.title = m.title.clone();
                j.progress.files_total = m.files.len() as u32;
                j.progress.bytes_total = m.total_bytes();
                let staged_sizes: HashMap<String, u64> = m
                    .files
                    .iter()
                    .filter_map(|fe| {
                        let p = join_rel(&staged, &fe.path).ok()?;
                        let part = PathBuf::from(format!("{}.part", p.display()));
                        std::fs::metadata(&p).or_else(|_| std::fs::metadata(&part)).ok().map(|md| (fe.path.clone(), md.len()))
                    })
                    .collect();
                let plan = install::plan(m, &staged_sizes);
                // Repair: copy intact files from the installed build instead of downloading them.
                if let Some(pack) = &j.repair_of {
                    if let Some(b) = self.store.active_build(pack)? {
                        let dir = self.store.build_dir(&b);
                        if let Ok(old) = self.store.manifest_of(&b) {
                            for (fe, _) in &plan {
                                if old.file(&fe.path).is_some_and(|o| o.sha256 == fe.sha256) {
                                    let from = join_rel(&dir, &fe.path)?;
                                    let to = join_rel(&staged, &fe.path)?;
                                    if let Ok((sz, sha)) = crate::util::hash_file(&from, |_| {}) {
                                        if sz == fe.size && sha == fe.sha256 && !to.exists() {
                                            std::fs::create_dir_all(to.parent().unwrap())?;
                                            std::fs::copy(&from, &to)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let space = install::check_space(&plan, free_space(&staged).unwrap_or(u64::MAX));
                if !space.ok {
                    return Err(NativeError::coded(
                        "no_space",
                        format!(
                            "not enough free space: {} MB needed, {} MB free",
                            space.need_bytes >> 20,
                            space.free_bytes >> 20
                        ),
                    ));
                }
                let base = base_url(&murl).to_string();
                j.progress.bytes_done = 0;
                for (fe, _) in &plan {
                    let dest = join_rel(&staged, &fe.path)?;
                    if std::fs::metadata(&dest).map(|md| md.len() == fe.size).unwrap_or(false) {
                        j.progress.bytes_done += fe.size;
                        j.progress.files_done += 1;
                        continue;
                    }
                    std::fs::create_dir_all(dest.parent().unwrap())?;
                    let part = PathBuf::from(format!("{}.part", dest.display()));
                    j.progress.current_file = Some(fe.path.clone());
                    report(self, j, true);
                    let mut attempts = 0;
                    loop {
                        let before = j.progress.bytes_done;
                        let r = f.download(&file_url(&base, &fe.path), &part, fe.size, cancel, |n| {
                            j.progress.bytes_done += n;
                            report(self, j, false);
                        });
                        match r {
                            Ok(()) => break,
                            Err(e) if e.code() == "network" && attempts < 3 && !cancel.load(Ordering::Relaxed) => {
                                attempts += 1;
                                j.progress.bytes_done = before;
                                std::thread::sleep(std::time::Duration::from_millis(500 * attempts));
                            }
                            Err(e) => return Err(e),
                        }
                        j.progress.bytes_done = before;
                    }
                    std::fs::rename(&part, &dest)?;
                    j.progress.files_done += 1;
                }
            }
            PackSource::Local { path } => {
                let p = PathBuf::from(&path);
                source_label = Some(format!("file:{path}"));
                if let Some(lib) = legacy_import::find_library(&p) {
                    let _ = std::fs::remove_dir_all(&staged);
                    j.progress.title = "Importing an existing Lastbook library".into();
                    report(self, j, true);
                    legacy_import::stage(&lib, &staged, false)?;
                } else if p.is_dir() {
                    let mut done = 0u64;
                    let parsed = unpack::stage_from_dir(&p, &staged, |n| {
                        done += n;
                    })?;
                    j.progress.pack_id = parsed.manifest.id.clone();
                    j.progress.version = parsed.manifest.version.clone();
                    j.progress.title = parsed.manifest.title.clone();
                    j.progress.bytes_total = parsed.manifest.total_bytes();
                    j.progress.bytes_done = done;
                } else {
                    let mut done = 0u64;
                    let parsed = unpack::stage_from_zip(&p, &staged, |n| {
                        done += n;
                        if cancel.load(Ordering::Relaxed) {}
                    })?;
                    j.progress.pack_id = parsed.manifest.id.clone();
                    j.progress.version = parsed.manifest.version.clone();
                    j.progress.title = parsed.manifest.title.clone();
                    j.progress.bytes_total = parsed.manifest.total_bytes();
                    j.progress.bytes_done = done;
                }
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(NativeError::coded("cancelled", "stopped"));
        }
        // Verify everything (also what was resumed or copied), then activate.
        j.progress.state = JobState::Verifying;
        j.progress.current_file = None;
        let total = j.progress.bytes_total.max(1);
        j.progress.bytes_done = 0;
        report(self, j, true);
        let v = self.store.verify_staged(&staged, j.allow_unsigned, |n| {
            j.progress.bytes_done = (j.progress.bytes_done + n).min(total);
        })?;
        j.progress.pack_id = v.parsed.manifest.id.clone();
        j.progress.version = v.parsed.manifest.version.clone();
        j.progress.title = v.parsed.manifest.title.clone();
        j.progress.bytes_total = v.parsed.manifest.total_bytes();
        j.progress.bytes_done = j.progress.bytes_total;
        j.progress.files_total = v.parsed.manifest.files.len() as u32;
        j.progress.files_done = j.progress.files_total;
        j.progress.state = JobState::Activating;
        report(self, j, true);
        if self.ai.loaded().as_deref() == Some(v.parsed.manifest.id.as_str()) {
            self.ai.unload();
        }
        let opts = ActivationOptions { allow_downgrade: j.allow_downgrade };
        self.store.activate(&staged, &v, source_label.as_deref(), opts)?;
        Ok(())
    }

    fn repair(self: &Arc<Self>, pack: &str) -> Result<Value> {
        let b = self.active_or_err(pack)?;
        let rep = self.store.verify_build(&b, |_| {})?;
        if rep.ok {
            return ok(serde_json::json!({"report": rep, "job": null}));
        }
        let src = self.store.source_of(&b)?;
        match src.as_deref() {
            Some(s) if s.starts_with("http") => {
                let job = self.start_install(
                    PackSource::Url { url: s.to_string() },
                    b.signed_by.is_none(),
                    false,
                    Some(pack.to_string()),
                )?;
                ok(serde_json::json!({"report": rep, "job": job}))
            }
            Some(s) if s.starts_with("file:") && Path::new(&s[5..]).exists() => {
                let job = self.start_install(
                    PackSource::Local { path: s[5..].to_string() },
                    b.signed_by.is_none(),
                    false,
                    Some(pack.to_string()),
                )?;
                ok(serde_json::json!({"report": rep, "job": job}))
            }
            _ => Err(NativeError::coded(
                "repair_source",
                "this pack's source is not available; install it again from its file or address",
            )),
        }
    }

    // --------------------------------------------------------------------------------------------------- AI --

    fn model_packs(&self) -> Result<Vec<(lbn_core::install::InstalledPack, Arc<Manifest>)>> {
        Ok(self
            .store
            .active()?
            .into_iter()
            .filter(|b| b.kind == PackKind::Model)
            .filter_map(|b| self.store.manifest_of(&b).ok().map(|m| (b, m)))
            .collect())
    }

    fn model_path(&self, m: &Manifest) -> Result<PathBuf> {
        let r = m
            .resources
            .iter()
            .find(|r| r.rtype == ResourceType::Model)
            .ok_or_else(|| NativeError::coded("incompatible", "no model file"))?;
        self.store.file_path(&m.id, &r.path)
    }

    fn ai_status(&self) -> Result<AiStatus> {
        let settings = self.personal.load()?.settings;
        let mut models = Vec::new();
        for (b, m) in self.model_packs()? {
            let info = m.model.clone().unwrap();
            let fit = {
                let cached = self.fits.lock().unwrap().get(&b.manifest_sha256).cloned();
                match cached {
                    Some(f) => f,
                    None if self.ai.runner().is_some() => {
                        let f = self.model_path(&m).and_then(|p| self.ai.measure(&p, info.context_length)).unwrap_or_else(|e| {
                            Fit { ok: false, reason: e.to_string(), ram_total_bytes: None, ram_available_bytes: None }
                        });
                        self.fits.lock().unwrap().insert(b.manifest_sha256.clone(), f.clone());
                        f
                    }
                    None => Fit {
                        ok: false,
                        reason: self.ai.unavailable_reason().unwrap_or_default(),
                        ram_total_bytes: None,
                        ram_available_bytes: None,
                    },
                }
            };
            models.push(ModelStatus { pack: b.id.clone(), title: m.title.clone(), info, fits: fit });
        }
        Ok(AiStatus {
            runtime: self.ai.runner().is_some(),
            runtime_name: self.ai.runner().map(|_| "llama.cpp b12032 (lastbook-llm, CPU)".to_string()),
            unavailable_reason: self.ai.unavailable_reason(),
            enabled: settings.ai_enabled,
            models,
            loaded: self.ai.loaded(),
            busy: !self.asks.lock().unwrap().is_empty(),
        })
    }

    fn load_model(&self, pack: &str) -> Result<()> {
        let b = self.active_or_err(pack)?;
        let m = self.store.manifest_of(&b)?;
        let info = m.model.clone().ok_or_else(|| NativeError::coded("incompatible", format!("{pack} is not a model pack")))?;
        if self.ai.runner().is_none() {
            return Err(NativeError::coded("no_runtime", self.ai.unavailable_reason().unwrap()));
        }
        self.ai.load(pack, &self.model_path(&m)?, info.context_length)
    }

    fn ask(self: &Arc<Self>, question: String, model: Option<String>) -> Result<AskStarted> {
        let q = question.trim().to_string();
        if q.is_empty() || q.len() > 2000 {
            return Err(NativeError::coded("bad_request", "ask a question of 1 to 2000 characters"));
        }
        let settings = self.personal.load()?.settings;
        if !settings.ai_enabled {
            return Err(NativeError::coded("ai_disabled", "local AI is turned off; turn it on in AI settings"));
        }
        let model = model.or(settings.model.clone()).or_else(|| self.model_packs().ok()?.first().map(|(b, _)| b.id.clone()));
        let model = model.ok_or_else(|| NativeError::coded("no_model", "no AI model is installed"))?;
        let ask = format!("ask-{}", random_id());
        let flag = Arc::new(AtomicBool::new(false));
        self.asks.lock().unwrap().insert(ask.clone(), flag.clone());
        let me = self.clone();
        let id = ask.clone();
        std::thread::spawn(move || {
            let r = me.run_ask(&id, &q, &model, &flag);
            me.asks.lock().unwrap().remove(&id);
            match r {
                Ok(res) => me.emit(Event::AskDone(res)),
                Err(e) => me.emit(Event::AskError { ask: id, code: e.code().into(), message: e.to_string() }),
            }
        });
        Ok(AskStarted { ask })
    }

    pub fn run_ask(&self, ask: &str, q: &str, model: &str, cancel: &AtomicBool) -> Result<AskResult> {
        let status = |s: &str| self.emit(Event::AskStatus { ask: ask.into(), status: s.into() });
        status("searching");
        let hits = self.search(q, 12, None)?.hits;
        let mut with_text = Vec::new();
        for h in &hits {
            if let Some(b) = self.store.active_build(&h.citation.pack)? {
                if let Some(ix) = self.store.index_of(&b)? {
                    if let Some(c) = ix.chunk(h.chunk) {
                        with_text.push((h.clone(), c.text.clone()));
                    }
                }
            }
        }
        let to_sources = |v: &[rag::Source]| -> Vec<AskSource> {
            v.iter()
                .map(|s| AskSource {
                    key: s.key.clone(),
                    citation: s.citation.to_uri(),
                    title: s.title.clone(),
                    excerpt: s.text.chars().take(400).collect(),
                })
                .collect()
        };
        let Some(sources) = rag::select_sources(q, &with_text, &RetrievalConfig::default()) else {
            let near: Vec<AskSource> = hits
                .iter()
                .take(5)
                .enumerate()
                .map(|(i, h)| AskSource {
                    key: format!("N{}", i + 1),
                    citation: h.citation.to_uri(),
                    title: h.title.clone(),
                    excerpt: h.snippet.clone(),
                })
                .collect();
            return Ok(AskResult {
                ask: ask.into(),
                question: q.into(),
                outcome: "no_evidence".into(),
                answer: None,
                sources: near,
                model: None,
                stats: None,
            });
        };
        let b = self.active_or_err(model)?;
        let m = self.store.manifest_of(&b)?;
        let info = m.model.clone().ok_or_else(|| NativeError::coded("incompatible", "not a model pack"))?;
        let tpl =
            Template::from_id(&info.template).ok_or_else(|| NativeError::coded("incompatible", "unknown prompt template"))?;
        if self.ai.loaded().as_deref() != Some(model) {
            status("loading_model");
            self.load_model(model)?;
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(AskResult {
                ask: ask.into(),
                question: q.into(),
                outcome: "cancelled".into(),
                answer: None,
                sources: to_sources(&sources),
                model: Some(model.into()),
                stats: None,
            });
        }
        status("generating");
        let prompt = rag::build_prompt(tpl, q, &sources);
        // Stop early when the model starts repeating itself (small models can loop until the token limit).
        let written = std::cell::RefCell::new(String::new());
        let looped = std::cell::Cell::new(false);
        let g = self.ai.generate(
            &prompt,
            &info.defaults,
            &tpl.stop(),
            |t| {
                written.borrow_mut().push_str(t);
                if t.contains(['.', ']', '!', '?']) && rag::is_looping(&written.borrow()) {
                    looped.set(true);
                }
                self.emit(Event::AskToken { ask: ask.into(), text: t.into() })
            },
            || cancel.load(Ordering::Relaxed) || looped.get(),
        )?;
        let mut checked = rag::check_answer(&g.text, &sources);
        // Every cited target must exist in the installed library.
        checked.cited.retain(|c| self.resolve(&c.citation).valid);
        let outcome = if g.reason == "cancel" && !looped.get() { "cancelled" } else { "answered" };
        if outcome == "answered" {
            let _ = self.personal.save_answer(&SavedAnswer {
                id: ask.into(),
                question: q.into(),
                answer: checked.text.clone(),
                citations: checked.cited.iter().map(|c| c.citation.clone()).collect(),
                model: model.into(),
                at: now_rfc3339(),
            });
        }
        Ok(AskResult {
            ask: ask.into(),
            question: q.into(),
            outcome: outcome.into(),
            answer: Some(checked),
            sources: to_sources(&sources),
            model: Some(m.title.clone()),
            stats: Some(AskStats {
                prompt_tokens: g.n_prompt,
                generated_tokens: g.n_gen,
                prompt_ms: g.prompt_ms,
                gen_ms: g.gen_ms,
            }),
        })
    }
}
