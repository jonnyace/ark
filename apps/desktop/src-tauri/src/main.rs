// Lastbook Next desktop: the shared web UI (apps/web) in a Tauri window over the native engine (lbn-native).
//
// - `engine` IPC command: one JSON request in, one JSON reply out (lbn-core API). Engine events go to the window as
//   "engine-event". The window's capabilities allow only these commands and events (capabilities/main.json).
// - `lbn://localhost/<pack>/<path>`: read-only access to files of ACTIVE packs, limited to paths listed in their
//   manifests, with byte ranges for video, PDF and PMTiles. Responses are marked nosniff, and HTML is never served as
//   an active document (it is shown through the sandboxed viewer), so pack content cannot run scripts in the app.
// - File dialogs are native (rfd); the UI never gets general filesystem access.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use lbn_native::engine::{EngineOptions, NativeEngine};
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, OnceLock};
use tauri::http::{header, Response, StatusCode};
use tauri::{Emitter, Manager};

static ENGINE: OnceLock<Arc<NativeEngine>> = OnceLock::new();
static MEDIA_BASE: OnceLock<String> = OnceLock::new();

/// Media streaming for <video>/<audio>. WebKitGTK's GStreamer pipeline cannot read media from a custom URI scheme
/// (tested: MEDIA_ERR_SRC_NOT_SUPPORTED), and loading whole videos into Blob URLs fails for large files. So media is
/// served by a minimal HTTP/1.1 responder bound to 127.0.0.1 on a random port, answering only GET/HEAD for
/// `/<token>/<pack>/<path>`, where token is 32 random bytes made at launch, and only for files listed in the
/// manifest of an installed pack (same lookup as the lbn: scheme). Nothing outside this computer can reach it.
fn start_media_server() -> std::io::Result<String> {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let mut t = [0u8; 32];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut t)).or_else(|_| {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        t.iter_mut()
            .enumerate()
            .for_each(|(i, b)| *b = (n >> (i % 16 * 8)) as u8 ^ (std::process::id() as u8).wrapping_mul(i as u8));
        Ok::<(), std::io::Error>(())
    })?;
    let token: String = t.iter().map(|b| format!("{b:02x}")).collect();
    let tk = token.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let tk = tk.clone();
            std::thread::spawn(move || {
                let _ = conn.set_read_timeout(Some(std::time::Duration::from_secs(30)));
                let mut r = BufReader::new(match conn.try_clone() {
                    Ok(c) => c,
                    Err(_) => return,
                });
                let mut w = conn;
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    let mut range = None;
                    let mut total = line.len();
                    loop {
                        let mut h = String::new();
                        if r.read_line(&mut h).unwrap_or(0) == 0 {
                            return;
                        }
                        total += h.len();
                        if total > 16 * 1024 {
                            return;
                        }
                        if h == "\r\n" || h == "\n" {
                            break;
                        }
                        if let Some((k, v)) = h.split_once(':') {
                            if k.eq_ignore_ascii_case("range") {
                                range = Some(v.trim().to_string());
                            }
                        }
                    }
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    let (method, target) = match parts.as_slice() {
                        [m, t, ..] => (*m, *t),
                        _ => return,
                    };
                    let path = percent_encoding::percent_decode_str(target).decode_utf8_lossy().to_string();
                    let rest = path.strip_prefix(&format!("/{tk}/"));
                    let file = (method == "GET" || method == "HEAD")
                        .then_some(rest)
                        .flatten()
                        .and_then(|p| p.split_once('/'))
                        .and_then(|(pack, rel)| engine().ok()?.store.file_path(pack, rel).ok().map(|f| (f, rel.to_string())));
                    let Some((file, rel)) = file else {
                        let _ = w.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    };
                    let Ok(mut f) = std::fs::File::open(&file) else { return };
                    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                    let (mut start, mut end, mut status) = (0u64, size.saturating_sub(1), "200 OK");
                    if let Some(rg) = range.as_deref().and_then(|v| v.strip_prefix("bytes=")) {
                        let (a, b) = rg.split_once('-').unwrap_or((rg, ""));
                        match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(a), Ok(b)) => (start, end) = (a, b.min(end)),
                            (Ok(a), Err(_)) => start = a,
                            (Err(_), Ok(n)) => start = size.saturating_sub(n),
                            _ => {}
                        }
                        if start >= size && size > 0 {
                            let _ = w.write_all(format!("HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\n\r\n").as_bytes());
                            continue;
                        }
                        status = "206 Partial Content";
                    }
                    let len = if size == 0 { 0 } else { end - start + 1 };
                    let mut head = format!("HTTP/1.1 {status}\r\nContent-Type: {}\r\nContent-Length: {len}\r\nAccept-Ranges: bytes\r\nAccess-Control-Allow-Origin: *\r\nX-Content-Type-Options: nosniff\r\nCache-Control: no-store\r\n", content_type(&rel));
                    if status.starts_with("206") {
                        head.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
                    }
                    head.push_str("\r\n");
                    if w.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    if method == "GET" && len > 0 {
                        if f.seek(SeekFrom::Start(start)).is_err() {
                            return;
                        }
                        if std::io::copy(&mut (&mut f).take(len), &mut w).is_err() {
                            return;
                        }
                    }
                }
            });
        }
    });
    Ok(format!("http://127.0.0.1:{port}/{token}"))
}

#[tauri::command]
fn media_base() -> Option<String> {
    MEDIA_BASE.get().cloned()
}

fn engine() -> Result<&'static Arc<NativeEngine>, String> {
    ENGINE.get().ok_or_else(|| "the engine is not running".to_string())
}

#[tauri::command]
async fn engine_call(request: String) -> Result<String, String> {
    let e = engine()?.clone();
    tauri::async_runtime::spawn_blocking(move || e.handle_json(&request)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn pick_path(kind: String) -> Option<String> {
    let d = rfd::AsyncFileDialog::new();
    let f = if kind == "folder" {
        d.set_title("Choose an old Lastbook library folder (LastbookData)").pick_folder().await
    } else {
        d.set_title("Choose a Lastbook pack").add_filter("Lastbook pack", &["lbpack", "zip"]).pick_file().await
    };
    f.map(|h| h.path().display().to_string())
}

#[tauri::command]
async fn pick_text_file() -> Option<String> {
    let f = rfd::AsyncFileDialog::new().add_filter("Lastbook personal data", &["json"]).pick_file().await?;
    let bytes = f.read().await;
    if bytes.len() > 64 << 20 {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[tauri::command]
async fn save_text_file(name: String, text: String) -> Result<bool, String> {
    let Some(f) = rfd::AsyncFileDialog::new().set_file_name(&name).save_file().await else { return Ok(false) };
    f.write(text.as_bytes()).await.map_err(|e| e.to_string())?;
    Ok(true)
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "webm" => "video/webm",
        "mp4" | "m4v" => "video/mp4",
        "vtt" => "text/vtt; charset=utf-8",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "css" => "text/css",
        // HTML and anything unknown are delivered as inert bytes; the viewer reads them as text.
        _ => "application/octet-stream",
    }
}

fn pack_file(req: &tauri::http::Request<Vec<u8>>) -> Response<Vec<u8>> {
    let err = |s: StatusCode, m: &str| {
        Response::builder()
            .status(s)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(m.as_bytes().to_vec())
            .unwrap()
    };
    let path = percent_encoding::percent_decode_str(req.uri().path()).decode_utf8_lossy().to_string();
    let path = path.trim_start_matches('/');
    let Some((pack, rel)) = path.split_once('/') else { return err(StatusCode::NOT_FOUND, "no file") };
    let Ok(e) = engine() else { return err(StatusCode::SERVICE_UNAVAILABLE, "engine not running") };
    let Ok(file) = e.store.file_path(pack, rel) else { return err(StatusCode::NOT_FOUND, "not part of an installed pack") };
    let Ok(mut f) = std::fs::File::open(&file) else { return err(StatusCode::NOT_FOUND, "missing") };
    let size = f.metadata().map(|m| m.len()).unwrap_or(0);
    let mut start = 0u64;
    let mut end = size.saturating_sub(1);
    let mut status = StatusCode::OK;
    if let Some(r) = req.headers().get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("bytes=")) {
        let (a, b) = r.split_once('-').unwrap_or((r, ""));
        match (a.parse::<u64>(), b.parse::<u64>()) {
            (Ok(a), Ok(b)) => (start, end) = (a, b.min(end)),
            (Ok(a), Err(_)) => start = a,
            (Err(_), Ok(n)) => start = size.saturating_sub(n),
            _ => {}
        }
        if start >= size && size > 0 {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .body(vec![])
                .unwrap();
        }
        status = StatusCode::PARTIAL_CONTENT;
        // Media players ask for open ranges; answer in pieces of at most 4 MiB so a large video is not read at once.
        end = end.min(start + (4 << 20) - 1);
    }
    let len = if size == 0 { 0 } else { end - start + 1 };
    let mut buf = vec![0u8; len as usize];
    if f.seek(SeekFrom::Start(start)).and_then(|_| f.read_exact(&mut buf)).is_err() {
        return err(StatusCode::INTERNAL_SERVER_ERROR, "read failed");
    }
    let mut b = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type(rel))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "Content-Range, Content-Length, Accept-Ranges")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox")
        .header(header::CONTENT_LENGTH, len.to_string());
    if status == StatusCode::PARTIAL_CONTENT {
        b = b.header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}"));
    }
    b.body(buf).unwrap()
}

fn builtin_keys() -> Vec<lbn_core::signature::TrustedKey> {
    serde_json::from_str(include_str!("../../../../keys/trusted.json")).expect("keys/trusted.json is valid")
}

fn main() {
    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("lbn", |_ctx, req, responder| {
            std::thread::spawn(move || responder.respond(pack_file(&req)));
        })
        .setup(|app| {
            let data = match std::env::var("LBN_DATA") {
                Ok(d) => std::path::PathBuf::from(d),
                Err(_) => app.path().app_data_dir()?,
            };
            let mut runner_dirs = vec![];
            if let Ok(r) = app.path().resource_dir() {
                runner_dirs.push(r.join("runner"));
            }
            let handle = app.handle().clone();
            let e = NativeEngine::open(
                &data,
                EngineOptions {
                    platform: format!("desktop-{}", std::env::consts::OS),
                    app_version: env!("CARGO_PKG_VERSION").into(),
                    builtin_keys: builtin_keys(),
                    runner_dirs,
                },
                Arc::new(move |ev| {
                    let _ = handle.emit("engine-event", serde_json::to_string(&ev).unwrap());
                }),
            )
            .map_err(|e| format!("could not open the library at {}: {e}", data.display()))?;
            let _ = ENGINE.set(e);
            match start_media_server() {
                Ok(b) => {
                    let _ = MEDIA_BASE.set(b);
                }
                Err(e) => eprintln!("media streaming unavailable: {e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![engine_call, pick_path, pick_text_file, save_text_file, media_base])
        .run(tauri::generate_context!())
        .expect("error while running Lastbook Next");
}
