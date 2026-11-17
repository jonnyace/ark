//! Install safety tests for the native engine, against a real HTTP server on loopback that supports byte ranges and
//! can cut a transfer part-way. Covers: signed/unsigned packs, tampering, unsafe .lbpack entries, interrupted and
//! resumed downloads (including across an engine restart), failed updates, rollback, downgrades and crash recovery.
use lbn_core::api::{Command, Event, PackSource, Request};
use lbn_core::install::JobState;
use lbn_core::signature::TrustedKey;
use lbn_native::engine::{EngineOptions, NativeEngine};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ------------------------------------------------------------------------------------------------- test server --

#[derive(Default)]
struct ServerState {
    /// path -> cut the response body after this many bytes (once)
    cut: HashMap<String, u64>,
    /// (path, range start) of every GET
    log: Vec<(String, u64)>,
}

struct Server {
    base: String,
    state: Arc<Mutex<ServerState>>,
}

fn serve(root: PathBuf) -> Server {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let state = Arc::new(Mutex::new(ServerState::default()));
    let st = state.clone();
    std::thread::spawn(move || {
        for c in l.incoming().flatten() {
            let root = root.clone();
            let st = st.clone();
            std::thread::spawn(move || {
                let mut r = BufReader::new(c.try_clone().unwrap());
                let mut w = c;
                let mut line = String::new();
                if r.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let mut range = 0u64;
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        range = v.trim().trim_end_matches('-').parse().unwrap_or(0);
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let rel = percent_decode(&path[1..]);
                st.lock().unwrap().log.push((path.clone(), range));
                let f = root.join(&rel);
                let Ok(mut file) = std::fs::File::open(&f) else {
                    let _ = w.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    return;
                };
                let size = file.metadata().unwrap().len();
                let len = size - range.min(size);
                let head = if range > 0 {
                    format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {len}\r\nContent-Range: bytes {range}-{}/{size}\r\nConnection: close\r\n\r\n", size - 1)
                } else {
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n")
                };
                w.write_all(head.as_bytes()).unwrap();
                file.seek(SeekFrom::Start(range)).unwrap();
                let cut = st.lock().unwrap().cut.remove(&path);
                let mut body = Vec::new();
                file.read_to_end(&mut body).unwrap();
                match cut {
                    Some(n) => {
                        let _ = w.write_all(&body[..(n as usize).min(body.len())]);
                        let _ = w.flush();
                        // drop the connection without completing the declared length
                    }
                    None => {
                        let _ = w.write_all(&body);
                    }
                }
            });
        }
    });
    Server { base, state }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

// --------------------------------------------------------------------------------------------------- helpers --

struct Keys {
    sk: minisign::SecretKey,
    trusted: Vec<TrustedKey>,
}

fn keys() -> Keys {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    Keys { trusted: vec![TrustedKey { name: "test publisher".into(), public_key: kp.pk.to_base64() }], sk: kp.sk }
}

fn sign(dir: &Path, k: &Keys) {
    let data = std::fs::read(dir.join("pack.json")).unwrap();
    let sig = minisign::sign(None, &k.sk, std::io::Cursor::new(&data), Some("test"), None).unwrap();
    std::fs::write(dir.join("pack.json.minisig"), sig.to_string()).unwrap();
}

/// Build a small documents pack (one HTML page with a section, plus a 3 MB data file to make downloads cuttable).
fn build_pack(out: &Path, version: &str, text: &str) -> PathBuf {
    let src = out.with_extension("src");
    std::fs::create_dir_all(src.join("res")).unwrap();
    std::fs::write(src.join("res/a.html"), format!("<html><body><h2 id=\"s1\">Water</h2><p>{text}</p></body></html>")).unwrap();
    let big: Vec<u8> = (0..3_000_000u32).map(|i| (i % 251) as u8).collect();
    std::fs::write(src.join("res/big.bin"), big).unwrap();
    std::fs::write(
        src.join("pack.src.json"),
        json!({"id": "org.test.docs", "version": version, "kind": "documents", "title": "Test docs", "created": "2026-11-17",
               "publisher": {"name": "T"}, "license": {"spdx": "CC0-1.0", "redistributable": true}, "compat": {"min_engine_api": 1},
               "resources": [{"id": "a", "type": "html", "title": "A", "path": "res/a.html", "media_type": "text/html", "assets": ["res/big.bin"]}]})
        .to_string(),
    )
    .unwrap();
    lbn_native::build::build_from_dir(&src, out).unwrap();
    out.to_path_buf()
}

struct Eng {
    e: Arc<NativeEngine>,
    rx: Receiver<Event>,
}

fn open(data: &Path, k: &Keys) -> Eng {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let e = NativeEngine::open(
        data,
        EngineOptions {
            platform: "test".into(),
            app_version: "test".into(),
            builtin_keys: k.trusted.clone(),
            runner_dirs: vec![],
        },
        Arc::new(move |ev| {
            let _ = tx.lock().unwrap().send(ev);
        }),
    )
    .unwrap();
    Eng { e, rx }
}

impl Eng {
    fn call(&self, c: Command) -> Result<Value, (String, String)> {
        let r = self.e.handle(Request { v: 1, id: 1, command: c });
        match (r.ok, r.err) {
            (Some(v), _) => Ok(v),
            (_, Some(e)) => Err((e.code, e.message)),
            _ => unreachable!(),
        }
    }
    fn install(&self, source: PackSource, allow_unsigned: bool, allow_downgrade: bool) -> (JobState, Option<String>) {
        let j = self.call(Command::Install { source, allow_unsigned, allow_downgrade }).unwrap();
        self.wait(j["job"].as_str().unwrap())
    }
    fn wait(&self, job: &str) -> (JobState, Option<String>) {
        loop {
            match self.rx.recv_timeout(Duration::from_secs(60)).expect("job event") {
                Event::Job(p)
                    if p.job == job
                        && matches!(p.state, JobState::Done | JobState::Failed | JobState::Cancelled | JobState::Paused) =>
                {
                    return (p.state, p.error.map(|e| e.code));
                }
                _ => {}
            }
        }
    }
    fn active_version(&self) -> Option<String> {
        let v = self.call(Command::ListPacks).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .find(|p| p["installed"]["id"] == "org.test.docs")
            .map(|p| p["installed"]["version"].as_str().unwrap().to_string())
    }
    fn search_hits(&self, q: &str) -> usize {
        self.call(Command::Search { query: q.into(), limit: 10, packs: None }).unwrap()["hits"].as_array().unwrap().len()
    }
}

fn local(p: &Path) -> PackSource {
    PackSource::Local { path: p.display().to_string() }
}

// ------------------------------------------------------------------------------------------------------ tests --

#[test]
fn signed_pack_installs_and_unsigned_needs_consent() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let pack = build_pack(&t.path().join("p1"), "1.0.0", "boil water for one minute");
    let e = open(&t.path().join("data"), &k);
    // Unsigned: refused unless the person confirms.
    assert_eq!(e.install(local(&pack), false, false), (JobState::Failed, Some("unsigned".into())));
    assert_eq!(e.active_version(), None);
    sign(&pack, &k);
    assert_eq!(e.install(local(&pack), false, false).0, JobState::Done);
    assert_eq!(e.active_version().as_deref(), Some("1.0.0"));
    assert_eq!(e.search_hits("boil"), 1);
    let v = e.call(Command::VerifyPack { pack: "org.test.docs".into() }).unwrap();
    assert_eq!(v["ok"], true);
    let r = e.call(Command::ResolveCitation { uri: "lbn:org.test.docs/a#sec=s1".into() }).unwrap();
    assert_eq!(r["valid"], true);
}

#[test]
fn tampered_packs_are_rejected_before_activation() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let e = open(&t.path().join("data"), &k);
    // 1. A file changed after signing: hash mismatch.
    let p = build_pack(&t.path().join("p1"), "1.0.0", "boil");
    sign(&p, &k);
    let mut html = std::fs::read(p.join("res/a.html")).unwrap();
    html[10] ^= 1;
    std::fs::write(p.join("res/a.html"), html).unwrap();
    assert_eq!(e.install(local(&p), false, false), (JobState::Failed, Some("checksum".into())));
    // 2. The manifest changed after signing: signature mismatch, even with "allow unsigned".
    let p2 = build_pack(&t.path().join("p2"), "1.0.0", "boil");
    sign(&p2, &k);
    let m = std::fs::read_to_string(p2.join("pack.json")).unwrap().replace("Test docs", "Evil docs");
    std::fs::write(p2.join("pack.json"), m).unwrap();
    assert_eq!(e.install(local(&p2), true, false), (JobState::Failed, Some("signature".into())));
    // 3. Signed by an untrusted key.
    let p3 = build_pack(&t.path().join("p3"), "1.0.0", "boil");
    sign(&p3, &keys());
    assert_eq!(e.install(local(&p3), true, false), (JobState::Failed, Some("signature".into())));
    assert_eq!(e.active_version(), None);
    // Nothing was left in staging.
    assert_eq!(std::fs::read_dir(t.path().join("data/staging")).unwrap().count(), 0);
}

#[test]
fn unsafe_lbpack_entries_are_rejected() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let e = open(&t.path().join("data"), &k);
    let p = build_pack(&t.path().join("p1"), "1.0.0", "boil");
    let mk = |name: &str, extra: Option<(&str, &[u8])>| {
        let f = t.path().join(name);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&f).unwrap());
        let o = zip::write::SimpleFileOptions::default();
        for n in ["pack.json", "res/a.html", "res/big.bin", "index/search.lbi"] {
            z.start_file(n, o).unwrap();
            z.write_all(&std::fs::read(p.join(n)).unwrap()).unwrap();
        }
        if let Some((n, d)) = extra {
            z.start_file(n, o).unwrap();
            z.write_all(d).unwrap();
        }
        z.finish().unwrap();
        f
    };
    let ok = mk("ok.lbpack", None);
    let traversal = mk("evil1.lbpack", Some(("../../outside.txt", b"x")));
    let extra = mk("evil2.lbpack", Some(("res/extra.js", b"alert(1)")));
    for (f, code) in [(&traversal, "unsafe_path"), (&extra, "manifest")] {
        assert_eq!(e.install(local(f), true, false), (JobState::Failed, Some(code.into())), "{f:?}");
    }
    assert!(!t.path().join("outside.txt").exists() && !t.path().join("data/outside.txt").exists());
    assert_eq!(e.install(local(&ok), true, false).0, JobState::Done);
}

#[test]
fn interrupted_download_resumes_with_range() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let srv_root = t.path().join("www");
    let p = build_pack(&srv_root.join("docs"), "1.0.0", "boil");
    sign(&p, &k);
    let srv = serve(srv_root);
    srv.state.lock().unwrap().cut.insert("/docs/res/big.bin".into(), 1_000_000);
    let e = open(&t.path().join("data"), &k);
    let url = format!("{}/docs/pack.json", srv.base);
    assert_eq!(e.install(PackSource::Url { url }, false, false).0, JobState::Done);
    let log = srv.state.lock().unwrap().log.clone();
    let big: Vec<_> = log.iter().filter(|(p, _)| p == "/docs/res/big.bin").collect();
    assert!(big.len() >= 2, "{log:?}");
    assert_eq!(big[0].1, 0);
    assert!(big[1].1 >= 1_000_000 - 256 * 1024 && big[1].1 <= 1_000_000, "resumed from {:?}", big[1]);
    assert_eq!(e.call(Command::VerifyPack { pack: "org.test.docs".into() }).unwrap()["ok"], true);
}

#[test]
fn download_paused_survives_restart_and_resumes() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let srv_root = t.path().join("www");
    let p = build_pack(&srv_root.join("docs"), "1.0.0", "boil");
    sign(&p, &k);
    let srv = serve(srv_root);
    // The first transfer of the big file breaks; with the publisher "down" for retries the job fails, keeping its part.
    srv.state.lock().unwrap().cut.insert("/docs/res/big.bin".into(), 1_500_000);
    let data = t.path().join("data");
    let url = format!("{}/docs/pack.json", srv.base);
    let job;
    {
        let e = open(&data, &k);
        let j = e
            .call(Command::Install {
                source: PackSource::Url { url: url.clone() },
                allow_unsigned: false,
                allow_downgrade: false,
            })
            .unwrap();
        job = j["job"].as_str().unwrap().to_string();
        // Pause as soon as the download is under way, then "quit the app".
        loop {
            if let Event::Job(p) = e.rx.recv_timeout(Duration::from_secs(30)).unwrap() {
                if p.bytes_done > 0 {
                    e.call(Command::PauseJob { job: job.clone() }).unwrap();
                    break;
                }
            }
        }
        let st = e.wait(&job).0;
        assert!(matches!(st, JobState::Paused | JobState::Done), "{st:?}");
        if st == JobState::Done {
            panic!("download finished before it could be paused; make the test file larger");
        }
    }
    let e = open(&data, &k);
    let jobs = e.call(Command::ListJobs).unwrap();
    assert_eq!(jobs[0]["state"], "paused");
    e.call(Command::ResumeJob { job: job.clone() }).unwrap();
    assert_eq!(e.wait(&job).0, JobState::Done);
    assert_eq!(e.active_version().as_deref(), Some("1.0.0"));
}

#[test]
fn failed_update_keeps_previous_library_and_rollback_works() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let e = open(&t.path().join("data"), &k);
    let v1 = build_pack(&t.path().join("v1"), "1.0.0", "boil water version one");
    sign(&v1, &k);
    assert_eq!(e.install(local(&v1), false, false).0, JobState::Done);
    // A damaged update: refused, v1 untouched and usable.
    let v2bad = build_pack(&t.path().join("v2bad"), "2.0.0", "boil water version two");
    sign(&v2bad, &k);
    let mut b = std::fs::read(v2bad.join("res/big.bin")).unwrap();
    b[123] ^= 0xff;
    std::fs::write(v2bad.join("res/big.bin"), b).unwrap();
    assert_eq!(e.install(local(&v2bad), false, false), (JobState::Failed, Some("checksum".into())));
    assert_eq!(e.active_version().as_deref(), Some("1.0.0"));
    assert_eq!(e.search_hits("one"), 1);
    assert_eq!(e.call(Command::VerifyPack { pack: "org.test.docs".into() }).unwrap()["ok"], true);
    // A good update activates, keeps v1 for rollback.
    let v2 = build_pack(&t.path().join("v2"), "2.0.0", "boil water version two");
    sign(&v2, &k);
    assert_eq!(e.install(local(&v2), false, false).0, JobState::Done);
    assert_eq!(e.active_version().as_deref(), Some("2.0.0"));
    assert_eq!(e.search_hits("two"), 1);
    e.call(Command::RollbackPack { pack: "org.test.docs".into() }).unwrap();
    assert_eq!(e.active_version().as_deref(), Some("1.0.0"));
    assert_eq!(e.search_hits("one"), 1);
    // Installing the older v1 again over v2 needs explicit consent.
    e.call(Command::RollbackPack { pack: "org.test.docs".into() }).unwrap();
    assert_eq!(e.active_version().as_deref(), Some("2.0.0"));
    e.call(Command::RemovePack { pack: "org.test.docs".into(), version: Some("1.0.0".into()) }).unwrap();
    assert_eq!(e.install(local(&v1), false, false), (JobState::Failed, Some("downgrade".into())));
    assert_eq!(e.install(local(&v1), false, true).0, JobState::Done);
    assert_eq!(e.active_version().as_deref(), Some("1.0.0"));
}

#[test]
fn damaged_installed_pack_is_detected_and_repaired_from_its_source() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let e = open(&t.path().join("data"), &k);
    let v1 = build_pack(&t.path().join("v1"), "1.0.0", "boil");
    sign(&v1, &k);
    assert_eq!(e.install(local(&v1), false, false).0, JobState::Done);
    let b = &e.call(Command::ListPacks).unwrap()[0]["installed"];
    let dir = t.path().join("data").join(b["dir"].as_str().unwrap());
    std::fs::write(dir.join("res/big.bin"), b"damaged").unwrap();
    assert_eq!(e.call(Command::ListPacks).unwrap()[0]["health"], "damaged");
    let r = e.call(Command::RepairPack { pack: "org.test.docs".into() }).unwrap();
    assert_eq!(r["report"]["damaged"][0], "res/big.bin");
    assert_eq!(e.wait(r["job"]["job"].as_str().unwrap()).0, JobState::Done);
    assert_eq!(e.call(Command::VerifyPack { pack: "org.test.docs".into() }).unwrap()["ok"], true);
}

#[test]
fn orphaned_build_folders_are_cleaned_after_a_crash() {
    let t = tempfile::tempdir().unwrap();
    let k = keys();
    let data = t.path().join("data");
    {
        let _e = open(&data, &k);
    }
    // A build moved into place whose registry commit never happened (crash between rename and commit).
    std::fs::create_dir_all(data.join("packs/org.test.docs/9.9.9+0000000000000000")).unwrap();
    std::fs::write(data.join("packs/org.test.docs/9.9.9+0000000000000000/pack.json"), "{}").unwrap();
    let e = open(&data, &k);
    assert!(!data.join("packs/org.test.docs/9.9.9+0000000000000000").exists());
    assert_eq!(e.active_version(), None);
}
