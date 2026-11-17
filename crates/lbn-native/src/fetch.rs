//! Downloading packs over HTTP(S), resumable. The only module that opens network connections.
//! Integrity comes from the manifest hashes and signatures, not from the transport; HTTPS keeps it private.
//! Plain HTTP is accepted only for loopback addresses (local testing).
use crate::{NativeError, Result};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub struct Fetcher {
    agent: ureq::Agent,
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}

pub fn check_url(url: &str) -> Result<()> {
    let ok = url.starts_with("https://")
        || url.starts_with("http://127.0.0.1")
        || url.starts_with("http://localhost")
        || url.starts_with("http://[::1]");
    if ok && url.len() < 2048 && !url.contains(char::is_whitespace) {
        Ok(())
    } else {
        Err(NativeError::coded("network", format!("only https:// addresses are allowed: {url}")))
    }
}

/// Resolve a pack location to the URL of its manifest: `.../pack.json` or a directory URL.
pub fn manifest_url(url: &str) -> String {
    if url.ends_with("/pack.json") {
        url.to_string()
    } else {
        format!("{}/pack.json", url.trim_end_matches('/'))
    }
}

pub fn base_url(manifest_url: &str) -> &str {
    manifest_url.strip_suffix("pack.json").unwrap_or(manifest_url)
}

/// URL of a pack file: path components are percent-encoded.
pub fn file_url(base: &str, rel: &str) -> String {
    let enc: Vec<String> = rel
        .split('/')
        .map(|c| {
            c.bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect()
        })
        .collect();
    format!("{base}{}", enc.join("/"))
}

impl Fetcher {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_body(Some(Duration::from_secs(60)))
            .user_agent("ark-library")
            .build()
            .into();
        Fetcher { agent }
    }

    /// GET a small resource. `Ok(None)` on 404.
    pub fn get_small(&self, url: &str, max: u64) -> Result<Option<Vec<u8>>> {
        check_url(url)?;
        let resp = self.agent.get(url).call().map_err(|e| NativeError::Network(e.to_string()))?;
        match resp.status().as_u16() {
            200 => {
                let mut v = Vec::new();
                resp.into_body()
                    .into_reader()
                    .take(max + 1)
                    .read_to_end(&mut v)
                    .map_err(|e| NativeError::Network(e.to_string()))?;
                if v.len() as u64 > max {
                    return Err(NativeError::coded("network", format!("{url} is larger than expected")));
                }
                Ok(Some(v))
            }
            404 => Ok(None),
            s => Err(NativeError::Network(format!("{url}: HTTP {s}"))),
        }
    }

    /// Download `url` into `part`, resuming from its current length. Stops early when `cancel` is set
    /// (returns `coded("cancelled")`). Never writes more than `size` bytes.
    pub fn download(&self, url: &str, part: &Path, size: u64, cancel: &AtomicBool, mut progress: impl FnMut(u64)) -> Result<()> {
        check_url(url)?;
        let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have > size {
            std::fs::remove_file(part)?;
            return self.download(url, part, size, cancel, progress);
        }
        if have == size {
            return Ok(());
        }
        let mut req = self.agent.get(url);
        if have > 0 {
            req = req.header("Range", &format!("bytes={have}-"));
        }
        let resp = req.call().map_err(|e| NativeError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let (mut file, mut done) = match status {
            206 if have > 0 => {
                // Content-Range must start where we asked.
                let cr = resp.headers().get("content-range").and_then(|v| v.to_str().ok()).unwrap_or("");
                if !cr.starts_with(&format!("bytes {have}-")) {
                    return Err(NativeError::Network(format!("server answered an unexpected range ({cr})")));
                }
                (OpenOptions::new().append(true).open(part)?, have)
            }
            200 => (OpenOptions::new().create(true).write(true).truncate(true).open(part)?, 0),
            416 => {
                std::fs::remove_file(part)?;
                return Err(NativeError::Network("the server's copy changed; starting this file again".into()));
            }
            s => return Err(NativeError::Network(format!("{url}: HTTP {s}"))),
        };
        if done > 0 {
            progress(done);
        }
        let mut body = resp.into_body();
        let mut reader = body.as_reader();
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                file.sync_all()?;
                return Err(NativeError::coded("cancelled", "stopped"));
            }
            let n = reader.read(&mut buf).map_err(|e| NativeError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            if done + n as u64 > size {
                drop(file);
                std::fs::remove_file(part)?;
                return Err(NativeError::coded("size", format!("{url} is larger than the manifest says")));
            }
            file.write_all(&buf[..n])?;
            done += n as u64;
            progress(n as u64);
        }
        file.sync_all()?;
        if done != size {
            return Err(NativeError::Network(format!("connection ended early ({done} of {size} bytes)")));
        }
        Ok(())
    }
}
