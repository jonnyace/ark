//! `lastbook` — command-line tool for Lastbook Next packs and libraries.
//!
//! Pack authoring:   pack build | pack zip | pack sign | pack inspect | pack verify | key gen
//! Library (uses the same data folder as the desktop app unless --data is given):
//!   list | install | jobs | resume | cancel | verify | repair | remove | rollback | search | resolve | ask |
//!   ai | export | import | legacy-inspect
use clap::{Parser, Subcommand};
use lbn_core::api::{Command, Event, PackSource, Request};
use lbn_core::manifest::{self, MANIFEST_NAME, SIGNATURE_NAME};
use lbn_native::engine::{EngineOptions, NativeEngine};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "lastbook", version, about = "Build, inspect, verify, install and search Lastbook Next packs")]
struct Cli {
    /// Library data folder (default: the desktop app's, or $LBN_DATA).
    #[arg(long, global = true)]
    data: Option<PathBuf>,
    /// Print machine-readable JSON.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pack authoring and checking (no library needed).
    #[command(subcommand)]
    Pack(PackCmd),
    /// Publisher keys.
    #[command(subcommand)]
    Key(KeyCmd),
    /// List installed packs.
    List,
    /// Install a pack: a URL, a pack folder, a .lbpack file, or an existing Lastbook library folder.
    Install {
        source: String,
        #[arg(long)]
        allow_unsigned: bool,
        #[arg(long)]
        allow_downgrade: bool,
    },
    /// Show install jobs (including paused or failed ones after a restart).
    Jobs,
    /// Resume a paused or failed job and wait for it.
    Resume { job: String },
    /// Cancel a job and delete its partial files.
    Cancel { job: String },
    /// Re-hash every file of an installed pack.
    Verify { pack: String },
    /// Re-fetch damaged files of a pack from its original source.
    Repair { pack: String },
    /// Remove a pack (or one inactive version of it).
    Remove {
        pack: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Switch a pack back to its previous installed version.
    Rollback { pack: String },
    /// Search the installed library.
    Search {
        query: Vec<String>,
        #[arg(long, default_value_t = 10)]
        limit: u32,
    },
    /// Check a citation (lbn: URI) against the installed library.
    Resolve { uri: String },
    /// Ask the local AI a question grounded in the installed library.
    Ask {
        question: Vec<String>,
        #[arg(long)]
        model: Option<String>,
    },
    /// AI status, or turn AI on/off.
    Ai {
        #[arg(long)]
        enable: bool,
        #[arg(long)]
        disable: bool,
    },
    /// Export personal data (bookmarks, notes, history, saved answers, settings) to a JSON file.
    Export { file: PathBuf },
    /// Import personal data from an export file (merged; a backup is taken first).
    Import { file: PathBuf },
    /// Show what importing an existing Lastbook library would do (nothing is changed).
    LegacyInspect { path: PathBuf },
}

#[derive(Subcommand)]
enum PackCmd {
    /// Build a pack folder from a source folder with pack.src.json.
    Build {
        src: PathBuf,
        out: PathBuf,
        /// minisign secret key file to sign pack.json with.
        #[arg(long)]
        sign: Option<PathBuf>,
    },
    /// Sign a built pack's pack.json (writes pack.json.minisig).
    Sign { dir: PathBuf, key: PathBuf },
    /// Sign a catalog.json (writes catalog.json.minisig).
    SignCatalog { file: PathBuf, key: PathBuf },
    /// Write a pack folder as a single .lbpack file (media stored, text compressed).
    Zip { dir: PathBuf, out: PathBuf },
    /// Show a pack's manifest summary (folder, .lbpack or URL).
    Inspect { source: String },
    /// Check every file of a pack folder or .lbpack against its manifest (and signature, if trusted keys given).
    Verify {
        source: PathBuf,
        /// Trusted public key (minisign .pub file); may be repeated.
        #[arg(long)]
        key: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
enum KeyCmd {
    /// Generate an unencrypted minisign key pair: <prefix>.key (secret, keep private) and <prefix>.pub.
    Gen { prefix: PathBuf, #[arg(long, default_value = "Lastbook Next publisher")] name: String },
}

fn default_data() -> PathBuf {
    if let Ok(d) = std::env::var("LBN_DATA") {
        return PathBuf::from(d);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("org.lastbook.next")
}

fn builtin_keys() -> Vec<lbn_core::signature::TrustedKey> {
    serde_json::from_str(include_str!("../../../keys/trusted.json")).expect("keys/trusted.json is valid")
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

type R<T> = std::result::Result<T, String>;

fn engine(data: &Path) -> R<(Arc<NativeEngine>, mpsc::Receiver<Event>)> {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let e = NativeEngine::open(
        data,
        EngineOptions { platform: "cli".into(), app_version: env!("CARGO_PKG_VERSION").into(), builtin_keys: builtin_keys(), runner_dirs: vec![] },
        Arc::new(move |ev| {
            let _ = tx.lock().unwrap().send(ev);
        }),
    )
    .map_err(|e| e.to_string())?;
    Ok((e, rx))
}

fn call(e: &Arc<NativeEngine>, cmd: Command) -> R<Value> {
    let r = e.handle(Request { v: lbn_core::ENGINE_API_VERSION, id: 1, command: cmd });
    match (r.ok, r.err) {
        (Some(v), _) => Ok(v),
        (_, Some(err)) => Err(format!("{} ({})", err.message, err.code)),
        _ => Err("empty reply".into()),
    }
}

fn print(json_out: bool, v: &Value, human: impl FnOnce(&Value)) {
    if json_out {
        println!("{}", serde_json::to_string_pretty(v).unwrap());
    } else {
        human(v);
    }
}

/// Wait for a job to end, printing progress. Returns an error if it failed.
fn wait_job(rx: &mpsc::Receiver<Event>, job: &str, quiet: bool) -> R<()> {
    let mut last = String::new();
    loop {
        match rx.recv_timeout(std::time::Duration::from_secs(600)) {
            Ok(Event::Job(p)) if p.job == job => {
                let line = format!("{:?} {} {}/{} MB", p.state, p.title, p.bytes_done >> 20, p.bytes_total >> 20);
                if !quiet && line != last {
                    eprintln!("{line}");
                    last = line;
                }
                match p.state {
                    lbn_core::install::JobState::Done => return Ok(()),
                    lbn_core::install::JobState::Failed => {
                        let e = p.error.unwrap();
                        return Err(format!("{} ({})", e.message, e.code));
                    }
                    lbn_core::install::JobState::Cancelled => return Err("cancelled".into()),
                    lbn_core::install::JobState::Paused => return Err(format!("paused; resume with: lastbook resume {job}")),
                    _ => {}
                }
            }
            Ok(_) => {}
            Err(_) => return Err("timed out waiting for the job".into()),
        }
    }
}

fn read_source_manifest(source: &str) -> R<(Vec<u8>, Option<String>)> {
    let p = Path::new(source);
    if p.is_dir() {
        let m = std::fs::read(p.join(MANIFEST_NAME)).map_err(|e| e.to_string())?;
        Ok((m, std::fs::read_to_string(p.join(SIGNATURE_NAME)).ok()))
    } else if p.is_file() {
        let mut z = zip::ZipArchive::new(std::fs::File::open(p).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let mut rd = |n: &str| -> Option<Vec<u8>> {
            let mut e = z.by_name(n).ok()?;
            let mut v = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut v).ok()?;
            Some(v)
        };
        let m = rd(MANIFEST_NAME).ok_or("no pack.json in that file")?;
        Ok((m, rd(SIGNATURE_NAME).map(|s| String::from_utf8_lossy(&s).into_owned())))
    } else {
        let f = lbn_native::fetch::Fetcher::new();
        let murl = lbn_native::fetch::manifest_url(source);
        let m = f.get_small(&murl, 32 << 20).map_err(|e| e.to_string())?.ok_or("no pack at that address")?;
        let s = f.get_small(&format!("{murl}.minisig"), 1 << 16).map_err(|e| e.to_string())?;
        Ok((m, s.map(|s| String::from_utf8_lossy(&s).into_owned())))
    }
}

fn sign_dir(dir: &Path, key: &Path) -> R<()> {
    let sk_box = minisign::SecretKeyBox::from_string(&std::fs::read_to_string(key).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let sk = sk_box.into_unencrypted_secret_key().map_err(|e| e.to_string())?;
    let data = std::fs::read(dir.join(MANIFEST_NAME)).map_err(|e| e.to_string())?;
    let p = manifest::parse(&data).map_err(|e| e.to_string())?;
    // Fixed trusted comment (no timestamp) so the same pack and key give the same signature file.
    let tc = format!("lastbook-next pack {} {} sha256:{}", p.manifest.id, p.manifest.version, p.sha256);
    let sig = minisign::sign(None, &sk, std::io::Cursor::new(&data), Some(&tc), Some("signature from lastbook-next")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(SIGNATURE_NAME), sig.to_string()).map_err(|e| e.to_string())?;
    Ok(())
}

fn run(cli: Cli) -> R<()> {
    let data = cli.data.clone().unwrap_or_else(default_data);
    let j = cli.json;
    match cli.cmd {
        Cmd::Key(KeyCmd::Gen { prefix, name }) => {
            let kp = minisign::KeyPair::generate_unencrypted_keypair().map_err(|e| e.to_string())?;
            let sk = kp.sk.to_box(Some(&name)).map_err(|e| e.to_string())?.to_string();
            let pk = kp.pk.to_base64();
            let skp = prefix.with_extension("key");
            std::fs::write(&skp, sk).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&skp, std::fs::Permissions::from_mode(0o600));
            }
            std::fs::write(prefix.with_extension("pub"), format!("untrusted comment: {name}\n{pk}\n")).map_err(|e| e.to_string())?;
            print(j, &json!({"name": name, "public_key": pk}), |_| println!("secret key: {}\npublic key: {pk}", skp.display()));
        }
        Cmd::Pack(PackCmd::Build { src, out, sign }) => {
            let p = lbn_native::build::build_from_dir(&src, &out).map_err(|e| e.to_string())?;
            if let Some(k) = sign {
                sign_dir(&out, &k)?;
            }
            let m = &p.manifest;
            print(j, &json!({"id": m.id, "version": m.version, "sha256": p.sha256, "files": m.files.len(), "bytes": m.total_bytes()}), |_| {
                println!("built {} {} ({} files, {} bytes)\nmanifest sha256 {}", m.id, m.version, m.files.len(), m.total_bytes(), p.sha256)
            });
        }
        Cmd::Pack(PackCmd::Sign { dir, key }) => sign_dir(&dir, &key)?,
        Cmd::Pack(PackCmd::SignCatalog { file, key }) => {
            let data = std::fs::read(&file).map_err(|e| e.to_string())?;
            let c = lbn_core::catalog::parse(&data).map_err(|e| e.to_string())?;
            let sk = minisign::SecretKeyBox::from_string(&std::fs::read_to_string(&key).map_err(|e| e.to_string())?)
                .and_then(|b| b.into_unencrypted_secret_key())
                .map_err(|e| e.to_string())?;
            let tc = format!("lastbook-next catalog {} published {} sha256:{}", c.title, c.published, lbn_core::hash::sha256_hex(&data));
            let sig = minisign::sign(None, &sk, std::io::Cursor::new(&data), Some(&tc), Some("signature from lastbook-next")).map_err(|e| e.to_string())?;
            let mut out = file.clone().into_os_string();
            out.push(".minisig");
            std::fs::write(out, sig.to_string()).map_err(|e| e.to_string())?;
        }
        Cmd::Pack(PackCmd::Zip { dir, out }) => {
            let mbytes = std::fs::read(dir.join(MANIFEST_NAME)).map_err(|e| e.to_string())?;
            let p = manifest::parse(&mbytes).map_err(|e| e.to_string())?;
            let f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            let mut z = zip::ZipWriter::new(f);
            // Fixed timestamps: the same pack folder always gives the same .lbpack bytes.
            let fixed = zip::DateTime::from_date_and_time(2026, 1, 1, 0, 0, 0).unwrap();
            let opt = |stored: bool| {
                zip::write::SimpleFileOptions::default()
                    .compression_method(if stored { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated })
                    .last_modified_time(fixed)
                    .unix_permissions(0o644)
                    .large_file(true)
            };
            let mut names: Vec<String> = vec![MANIFEST_NAME.into()];
            if dir.join(SIGNATURE_NAME).is_file() {
                names.push(SIGNATURE_NAME.into());
            }
            names.extend(p.manifest.files.iter().map(|f| f.path.clone()));
            for n in names {
                let ext = n.rsplit('.').next().unwrap_or("");
                let stored = matches!(ext, "webm" | "mp4" | "pdf" | "pmtiles" | "gguf" | "webp" | "png" | "jpg" | "pbf" | "lbi");
                z.start_file(n.as_str(), opt(stored)).map_err(|e| e.to_string())?;
                let mut src = std::fs::File::open(lbn_native::util::join_rel(&dir, &n).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                std::io::copy(&mut src, &mut z).map_err(|e| e.to_string())?;
            }
            z.finish().map_err(|e| e.to_string())?;
            println!("wrote {}", out.display());
        }
        Cmd::Pack(PackCmd::Inspect { source }) => {
            let (m, sig) = read_source_manifest(&source)?;
            let p = manifest::parse(&m).map_err(|e| e.to_string())?;
            let signer = sig.as_deref().map(|s| lbn_core::signature::verify(&m, s, &builtin_keys()).map(|k| k.name.clone()).map_err(|e| e.to_string()));
            let mm = &p.manifest;
            let v = json!({"id": mm.id, "version": mm.version, "kind": mm.kind, "title": mm.title, "license": mm.license, "files": mm.files.len(),
                           "bytes": mm.total_bytes(), "resources": mm.resources.iter().map(|r| json!({"id": r.id, "type": r.rtype, "title": r.title})).collect::<Vec<_>>(),
                           "manifest_sha256": p.sha256, "signature": match &signer { None => json!("unsigned"), Some(Ok(k)) => json!({"signed_by": k}), Some(Err(e)) => json!({"invalid": e}) }});
            print(j, &v, |v| println!("{}", serde_json::to_string_pretty(v).unwrap()));
        }
        Cmd::Pack(PackCmd::Verify { source, key }) => {
            let mut keys = builtin_keys();
            for k in key {
                let t = std::fs::read_to_string(&k).map_err(|e| e.to_string())?;
                let b64 = t.lines().find(|l| !l.starts_with("untrusted comment")).unwrap_or("").to_string();
                keys.push(lbn_core::signature::TrustedKey { name: k.display().to_string(), public_key: b64 });
            }
            let tmp;
            let dir = if source.is_dir() {
                source.clone()
            } else {
                tmp = std::env::temp_dir().join(format!("lbn-verify-{}", lbn_native::util::random_id()));
                lbn_native::unpack::stage_from_zip(&source, &tmp, |_| {}).map_err(|e| e.to_string())?;
                tmp.clone()
            };
            let store_dir = std::env::temp_dir().join(format!("lbn-verify-store-{}", lbn_native::util::random_id()));
            let store = lbn_native::Store::open(&store_dir, keys).map_err(|e| e.to_string())?;
            let r = store.verify_staged(&dir, true, |_| {});
            let _ = std::fs::remove_dir_all(&store_dir);
            if !source.is_dir() {
                let _ = std::fs::remove_dir_all(&dir);
            }
            let v = r.map_err(|e| format!("REJECTED: {e}"))?;
            print(j, &json!({"ok": true, "id": v.parsed.manifest.id, "signed_by": v.signed_by}), |_| {
                println!("OK: {} {} — every file matches; signature: {}", v.parsed.manifest.id, v.parsed.manifest.version, v.signed_by.clone().unwrap_or("none (unsigned)".into()))
            });
        }
        Cmd::LegacyInspect { path } => {
            let lib = lbn_native::legacy_import::find_library(&path).ok_or("no Lastbook library (library-data/catalog.json) found there")?;
            let info = lbn_native::legacy_import::inspect(&lib).map_err(|e| e.to_string())?;
            let v = json!({"id": info.id, "version": info.version, "importable": info.plan.resources.len(), "bytes": info.bytes, "unsupported": info.plan.unsupported});
            print(j, &v, |v| println!("{}", serde_json::to_string_pretty(v).unwrap()));
        }
        cmd => {
            let (e, rx) = engine(&data)?;
            match cmd {
                Cmd::List => {
                    let v = call(&e, Command::ListPacks)?;
                    print(j, &v, |v| {
                        for p in v.as_array().unwrap() {
                            let i = &p["installed"];
                            println!("{:<40} {:<14} {:<12} {:>8} MB  {}  {}", i["id"].as_str().unwrap(), i["version"].as_str().unwrap(), i["kind"].as_str().unwrap(),
                                i["bytes"].as_u64().unwrap() >> 20, p["health"].as_str().unwrap(), i["signed_by"].as_str().unwrap_or("unsigned"));
                        }
                    });
                }
                Cmd::Install { source, allow_unsigned, allow_downgrade } => {
                    let src = if source.starts_with("http://") || source.starts_with("https://") {
                        PackSource::Url { url: source }
                    } else {
                        PackSource::Local { path: std::fs::canonicalize(&source).map_err(|e| format!("{source}: {e}"))?.display().to_string() }
                    };
                    let p = call(&e, Command::Install { source: src, allow_unsigned, allow_downgrade })?;
                    let job = p["job"].as_str().unwrap().to_string();
                    wait_job(&rx, &job, j)?;
                    println!("installed");
                }
                Cmd::Jobs => {
                    let v = call(&e, Command::ListJobs)?;
                    print(j, &v, |v| {
                        for p in v.as_array().unwrap() {
                            println!("{}  {}  {}  {}/{} bytes  {}", p["job"].as_str().unwrap(), p["state"].as_str().unwrap(), p["title"].as_str().unwrap(),
                                p["bytes_done"], p["bytes_total"], p["error"]["message"].as_str().unwrap_or(""));
                        }
                    });
                }
                Cmd::Resume { job } => {
                    call(&e, Command::ResumeJob { job: job.clone() })?;
                    wait_job(&rx, &job, j)?;
                    println!("installed");
                }
                Cmd::Cancel { job } => {
                    call(&e, Command::CancelJob { job })?;
                }
                Cmd::Verify { pack } => {
                    let v = call(&e, Command::VerifyPack { pack })?;
                    print(j, &v, |v| println!("{}", serde_json::to_string_pretty(v).unwrap()));
                    if !v["ok"].as_bool().unwrap_or(false) {
                        return Err("pack is damaged; run: lastbook repair <pack>".into());
                    }
                }
                Cmd::Repair { pack } => {
                    let v = call(&e, Command::RepairPack { pack })?;
                    if let Some(job) = v["job"]["job"].as_str() {
                        wait_job(&rx, job, j)?;
                        println!("repaired");
                    } else {
                        println!("nothing to repair");
                    }
                }
                Cmd::Remove { pack, version } => {
                    let v = call(&e, Command::RemovePack { pack, version })?;
                    print(j, &v, |v| println!("removed; freed {} MB", v["freed_bytes"].as_u64().unwrap_or(0) >> 20));
                }
                Cmd::Rollback { pack } => {
                    let v = call(&e, Command::RollbackPack { pack })?;
                    print(j, &v, |v| println!("now using {} {}", v["id"].as_str().unwrap(), v["version"].as_str().unwrap()));
                }
                Cmd::Search { query, limit } => {
                    let v = call(&e, Command::Search { query: query.join(" "), limit, packs: None })?;
                    print(j, &v, |v| {
                        for h in v["hits"].as_array().unwrap() {
                            let c: lbn_core::citation::Citation = serde_json::from_value(h["citation"].clone()).unwrap();
                            println!("{:6.2}  {}\n        {}\n        {}", h["score"].as_f64().unwrap(), h["title"].as_str().unwrap(), c.to_uri(), h["snippet"].as_str().unwrap());
                        }
                    });
                }
                Cmd::Resolve { uri } => {
                    let v = call(&e, Command::ResolveCitation { uri })?;
                    print(j, &v, |v| println!("{}", serde_json::to_string_pretty(v).unwrap()));
                }
                Cmd::Ai { enable, disable } => {
                    if enable || disable {
                        let mut s: lbn_core::personal::Settings = serde_json::from_value(call(&e, Command::GetPersonal)?["data"]["settings"].clone()).unwrap();
                        s.ai_enabled = enable;
                        call(&e, Command::SaveSettings { settings: s })?;
                    }
                    let v = call(&e, Command::AiStatus)?;
                    print(j, &v, |v| println!("{}", serde_json::to_string_pretty(v).unwrap()));
                }
                Cmd::Ask { question, model } => {
                    let v = call(&e, Command::Ask { question: question.join(" "), model })?;
                    let ask = v["ask"].as_str().unwrap().to_string();
                    loop {
                        match rx.recv_timeout(std::time::Duration::from_secs(900)).map_err(|_| "timed out")? {
                            Event::AskStatus { ask: a, status } if a == ask && !j => eprintln!("[{status}]"),
                            Event::AskToken { ask: a, text } if a == ask && !j => {
                                eprint!("{text}");
                            }
                            Event::AskDone(r) if r.ask == ask => {
                                let v = serde_json::to_value(&r).unwrap();
                                print(j, &v, |_| {
                                    eprintln!();
                                    match &r.answer {
                                        None => println!("The installed library does not seem to cover this. Closest matches:"),
                                        Some(a) => {
                                            println!("\n{}\n", a.text);
                                            if a.uncited {
                                                println!("(warning: the model did not cite its sources; check the sources below)");
                                            }
                                            if !a.unverified_numbers.is_empty() {
                                                println!("(warning: these numbers are not in the cited sources; check them: {})", a.unverified_numbers.join(", "));
                                            }
                                            if a.repeats_removed > 0 {
                                                println!("(removed {} repeated sentences)", a.repeats_removed);
                                            }
                                            if !a.unknown_keys.is_empty() {
                                                println!("(removed citations to sources it was not given: {})", a.unknown_keys.join(", "));
                                            }
                                        }
                                    }
                                    for s in &r.sources {
                                        println!("[{}] {}  {}", s.key, s.title, s.citation);
                                    }
                                    if let Some(st) = &r.stats {
                                        println!("({} prompt tokens in {} ms, {} generated in {} ms, model {})", st.prompt_tokens, st.prompt_ms, st.generated_tokens, st.gen_ms, r.model.clone().unwrap_or_default());
                                    }
                                });
                                break;
                            }
                            Event::AskError { ask: a, code, message } if a == ask => return Err(format!("{message} ({code})")),
                            _ => {}
                        }
                    }
                }
                Cmd::Export { file } => {
                    let v = call(&e, Command::ExportPersonal)?;
                    std::fs::write(&file, v.as_str().unwrap()).map_err(|e| e.to_string())?;
                    println!("exported to {}", file.display());
                }
                Cmd::Import { file } => {
                    let data = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
                    let v = call(&e, Command::ImportPersonal { data })?;
                    print(j, &v, |v| println!("imported; {} bookmarks, {} notes", v["data"]["bookmarks"].as_array().unwrap().len(), v["data"]["notes"].as_array().unwrap().len()));
                }
                _ => unreachable!(),
            }
        }
    }
    Ok(())
}
