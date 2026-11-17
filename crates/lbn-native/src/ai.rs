//! Native local inference: drives the `lastbook-llm` runner (llama.cpp) as a child process over JSON lines.
//!
//! The runner opens no socket. One model is loaded at a time. Cancel = protocol `cancel`; unload = `quit`, then kill
//! after a grace period. If the process dies (for example killed for lack of memory) the error says so and the model
//! is marked unloaded; the app keeps working without AI.
use crate::{NativeError, Result};
use lbn_core::api::Fit;
use lbn_core::manifest::GenSettings;
use lbn_core::rag::Segment;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const RUNNER_NAME: &str = if cfg!(windows) { "lastbook-llm.exe" } else { "lastbook-llm" };

/// Find the runner: `LBN_RUNNER`, then beside the executable, then `<exe dir>/runner/`, then extra dirs.
pub fn find_runner(extra: &[PathBuf]) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LBN_RUNNER") {
        let p = PathBuf::from(p);
        return p.is_file().then_some(p);
    }
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = &exe_dir {
        dirs.push(d.clone());
        dirs.push(d.join("runner"));
        dirs.push(d.join("../lib/ark-library/runner"));
    }
    dirs.extend(extra.iter().cloned());
    dirs.into_iter().map(|d| d.join(RUNNER_NAME)).find(|p| p.is_file())
}

struct Loaded {
    pack: String,
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Value>,
}

pub struct GenResult {
    pub text: String,
    pub reason: String,
    pub n_prompt: u32,
    pub n_gen: u32,
    pub prompt_ms: u32,
    pub gen_ms: u32,
}

pub struct Ai {
    runner: Option<PathBuf>,
    loaded: Mutex<Option<Loaded>>,
    threads: Option<u32>,
    cpu_only: bool,
}

fn spawn_reader(child: &mut Child) -> Receiver<Value> {
    let out = child.stdout.take().expect("piped stdout");
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    break;
                }
            }
        }
    });
    rx
}

impl Ai {
    pub fn new(runner: Option<PathBuf>) -> Ai {
        let threads = std::env::var("LBN_THREADS").ok().and_then(|t| t.parse().ok());
        let cpu_only = std::env::var("LBN_GPU").map(|v| v != "1").unwrap_or(true);
        Ai { runner, loaded: Mutex::new(None), threads, cpu_only }
    }

    pub fn runner(&self) -> Option<&Path> {
        self.runner.as_deref()
    }

    pub fn unavailable_reason(&self) -> Option<String> {
        if self.runner.is_none() {
            Some("The local AI runner (lastbook-llm) is not installed with this copy of the app.".into())
        } else {
            None
        }
    }

    fn command(&self, model: &Path, ctx: u32) -> Result<Command> {
        let r = self.runner.as_ref().ok_or_else(|| NativeError::coded("no_runtime", self.unavailable_reason().unwrap()))?;
        let mut c = Command::new(r);
        c.arg("--model").arg(model).arg("--ctx").arg(ctx.to_string());
        if self.cpu_only {
            c.arg("--cpu-only");
        }
        if let Some(t) = self.threads {
            c.arg("--threads").arg(t.to_string());
        }
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        Ok(c)
    }

    /// Ask the runner whether the model fits this computer's memory (loads nothing).
    pub fn measure(&self, model: &Path, ctx: u32) -> Result<Fit> {
        let mut c = self.command(model, ctx)?;
        c.arg("--measure");
        let mut child = c.spawn()?;
        let rx = spawn_reader(&mut child);
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut fit = None;
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(v) if v["ev"] == "fit" => {
                    fit = Some(v);
                    break;
                }
                Ok(v) if v["ev"] == "error" => {
                    let _ = child.kill();
                    return Ok(Fit {
                        ok: false,
                        reason: v["msg"].as_str().unwrap_or("the model cannot be read").into(),
                        ram_total_bytes: None,
                        ram_available_bytes: None,
                    });
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        let v = fit.ok_or_else(|| NativeError::coded("runner", "the AI runner did not answer"))?;
        let ok = v["status"] == "fits";
        let total = v["ram_total"].as_u64();
        let free = v["ram_free"].as_u64();
        let need: u64 = v["memory"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|m| {
                        m["model"].as_u64().unwrap_or(0) + m["context"].as_u64().unwrap_or(0) + m["compute"].as_u64().unwrap_or(0)
                    })
                    .sum()
            })
            .unwrap_or(0);
        let reason = if ok {
            format!("Fits: needs about {} MB; {} MB of memory is available now.", need >> 20, free.unwrap_or(0) >> 20)
        } else {
            format!("Does not fit: needs about {} MB; only {} MB of memory is available. Close other programs or choose a smaller model.", need >> 20, free.unwrap_or(0) >> 20)
        };
        Ok(Fit { ok, reason, ram_total_bytes: total, ram_available_bytes: free })
    }

    pub fn loaded(&self) -> Option<String> {
        let mut l = self.loaded.lock().unwrap();
        if let Some(x) = l.as_mut() {
            if let Ok(Some(_)) = x.child.try_wait() {
                *l = None; // the runner exited on its own (crash / killed)
            }
        }
        l.as_ref().map(|x| x.pack.clone())
    }

    pub fn load(&self, pack: &str, model: &Path, ctx: u32) -> Result<()> {
        if self.loaded().as_deref() == Some(pack) {
            return Ok(());
        }
        self.unload();
        let mut child = self.command(model, ctx)?.spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let events = spawn_reader(&mut child);
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            match events.recv_timeout(Duration::from_millis(250)) {
                Ok(v) if v["ev"] == "ready" => break,
                Ok(v) if v["ev"] == "error" => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let code = match v["code"].as_str() {
                        Some("context") | Some("load") => "memory",
                        Some("model_file") => "model_file",
                        _ => "runner",
                    };
                    return Err(NativeError::coded(
                        code,
                        v["msg"].as_str().unwrap_or("the model could not be loaded").to_string(),
                    ));
                }
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) if Instant::now() < deadline => {}
                Err(_) => {
                    let _ = child.kill();
                    let status = child.wait().ok();
                    return Err(NativeError::coded(
                        "memory",
                        format!("the AI runner stopped while loading the model ({status:?}); the computer may not have enough free memory"),
                    ));
                }
            }
        }
        let _ = stdin.flush();
        *self.loaded.lock().unwrap() = Some(Loaded { pack: pack.into(), child, stdin, events });
        Ok(())
    }

    pub fn unload(&self) {
        let mut l = self.loaded.lock().unwrap();
        if let Some(mut x) = l.take() {
            let _ = x.stdin.write_all(b"{\"op\":\"quit\"}\n");
            let _ = x.stdin.flush();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Ok(Some(_)) = x.child.try_wait() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = x.child.kill();
            let _ = x.child.wait();
        }
    }

    /// Generate an answer, streaming text to `on_token`. `cancelled()` is polled; when it turns true a protocol
    /// cancel is sent and the partial answer returns with reason `cancel`.
    pub fn generate(
        &self,
        segments: &[Segment],
        s: &GenSettings,
        stop: &[String],
        mut on_token: impl FnMut(&str),
        cancelled: impl Fn() -> bool,
    ) -> Result<GenResult> {
        let mut guard = self.loaded.lock().unwrap();
        let l = guard.as_mut().ok_or_else(|| NativeError::coded("no_model", "no model is loaded"))?;
        let segs: Vec<Value> = segments.iter().map(|Segment(t, sp)| json!([t, sp])).collect();
        let req = json!({"op": "gen", "id": 1, "segments": segs, "max_tokens": s.max_tokens, "temp": s.temperature,
                         "top_k": s.top_k, "top_p": s.top_p, "min_p": s.min_p, "seed": s.seed, "stop": stop,
                         "penalty_repeat": s.repeat_penalty, "penalty_last_n": s.repeat_last_n});
        writeln!(l.stdin, "{req}")?;
        l.stdin.flush()?;
        let mut text = String::new();
        let mut sent_cancel = false;
        loop {
            if !sent_cancel && cancelled() {
                let _ = l.stdin.write_all(b"{\"op\":\"cancel\"}\n");
                let _ = l.stdin.flush();
                sent_cancel = true;
            }
            match l.events.recv_timeout(Duration::from_millis(100)) {
                Ok(v) => match v["ev"].as_str() {
                    Some("tok") => {
                        let t = v["t"].as_str().unwrap_or("");
                        text.push_str(t);
                        on_token(t);
                    }
                    Some("done") => {
                        return Ok(GenResult {
                            text,
                            reason: v["reason"].as_str().unwrap_or("").into(),
                            n_prompt: v["n_prompt"].as_u64().unwrap_or(0) as u32,
                            n_gen: v["n_gen"].as_u64().unwrap_or(0) as u32,
                            prompt_ms: v["prompt_ms"].as_u64().unwrap_or(0) as u32,
                            gen_ms: v["gen_ms"].as_u64().unwrap_or(0) as u32,
                        })
                    }
                    Some("error") => {
                        let code = if v["code"] == "too_long" { "too_long" } else { "runner" };
                        return Err(NativeError::coded(code, v["msg"].as_str().unwrap_or("generation failed").to_string()));
                    }
                    _ => {}
                },
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    let status = l.child.wait().ok();
                    *guard = None;
                    return Err(NativeError::coded(
                        "memory",
                        format!("the AI runner stopped unexpectedly ({status:?}); it may have run out of memory. The model was unloaded."),
                    ));
                }
            }
        }
    }
}

impl Drop for Ai {
    fn drop(&mut self) {
        self.unload();
    }
}
