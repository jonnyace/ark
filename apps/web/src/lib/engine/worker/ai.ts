// In-browser local inference with wllama (llama.cpp compiled to WebAssembly), running inside the engine worker.
//
// Never uses the network: the wasm file is part of the app build, the model is a File from this device's storage,
// and wllama's "compat" mode (which downloads a different build from a CDN on Safari) is switched off.
// Requirements (checked here, reported in plain words when missing): WebAssembly 64-bit memory and JavaScript Promise
// Integration (JSPI). Multi-threading additionally needs cross-origin isolation (the service worker sets COOP/COEP).
import { Wllama } from "@wllama/wllama";
import wllamaWasm from "@wllama/wllama/esm/wasm/wllama.wasm?url";

let w: Wllama | null = null;
let loaded: string | null = null;

export interface Capability { ok: boolean; reason: string; threads: number }

export function capability(): Capability {
  const WA: any = (globalThis as any).WebAssembly;
  let mem64 = false;
  try {
    new WA.Memory({ initial: 1n, maximum: 2n, address: "i64" });
    mem64 = true;
  } catch {
    try {
      new WA.Memory({ initial: 1, maximum: 2, index: "i64" });
      mem64 = true;
    } catch { /* unsupported */ }
  }
  const jspi = typeof WA?.Suspending === "function" || typeof WA?.promising === "function";
  const isolated = (globalThis as any).crossOriginIsolated === true;
  const cores = Math.max(1, Math.min(8, (navigator.hardwareConcurrency || 2) - 1));
  if (!mem64 || !jspi) {
    const missing = [!mem64 && "64-bit WebAssembly memory", !jspi && "WebAssembly promise integration (JSPI)"].filter(Boolean).join(" and ");
    return {
      ok: false,
      threads: 0,
      reason: `This browser cannot run the local AI: it lacks ${missing}. Use a current Chromium-based browser (Chrome, Edge, Brave, Chromium 137 or newer) or the desktop app. Reading, search, maps and video still work.`,
    };
  }
  return { ok: true, reason: "", threads: isolated ? cores : 1 };
}

export function estimateFit(info: { memory_bytes: number; min_ram_bytes: number }) {
  const dm = (navigator as any).deviceMemory as number | undefined; // GiB, rounded and capped at 8 by browsers
  const ram = dm ? dm * 2 ** 30 : null;
  const ok = ram == null ? true : ram >= Math.min(info.min_ram_bytes, 8 * 2 ** 30);
  return {
    ok,
    reason: ram == null
      ? `Estimate only: this browser does not report memory. The model needs about ${Math.round(info.memory_bytes / 2 ** 20)} MB while answering.`
      : ok
        ? `Estimate: the browser reports at least ${dm} GB of memory; the model needs about ${Math.round(info.memory_bytes / 2 ** 20)} MB.`
        : `Estimate: the browser reports ${dm} GB of memory, less than the ${Math.round(info.min_ram_bytes / 2 ** 30)} GB this model needs.`,
    ram_total_bytes: ram,
    ram_available_bytes: null,
  };
}

export function loadedPack() {
  return loaded;
}

export async function unload() {
  if (w) {
    try { await w.exit(); } catch { /* already gone */ }
  }
  w = null;
  loaded = null;
}

export async function load(pack: string, file: File, ctx: number) {
  if (loaded === pack && w) return;
  await unload();
  const cap = capability();
  const inst = new Wllama({ default: wllamaWasm, "single-thread/wllama.wasm": wllamaWasm, "multi-thread/wllama.wasm": wllamaWasm } as any, {
    suppressNativeLog: true,
  });
  try {
    inst.setCompat(null);
  } catch { /* older API */ }
  try {
    await inst.loadModel([file], { n_ctx: ctx, n_threads: cap.threads || 1, n_batch: 512 } as any);
  } catch (e: any) {
    try { await inst.exit(); } catch { /* ignore */ }
    const msg = String(e?.message ?? e);
    const err: any = new Error(/memory|alloc|OOM/i.test(msg)
      ? `The browser could not give the model enough memory (${msg}). Close other tabs, choose a smaller model, or use the desktop app.`
      : `The model could not be loaded in this browser: ${msg}`);
    err.code = /memory|alloc|OOM/i.test(msg) ? "memory" : "runner";
    throw err;
  }
  w = inst;
  loaded = pack;
}

export async function generate(
  prompt: string,
  s: { temperature: number; top_k: number; top_p: number; min_p: number; max_tokens: number; seed: number; repeat_penalty?: number; repeat_last_n?: number },
  stop: string[],
  onToken: (t: string) => void,
  signal: AbortSignal,
): Promise<{ text: string; cancelled: boolean; stats: { prompt_tokens: number; generated_tokens: number; prompt_ms: number; gen_ms: number } | null }> {
  if (!w) throw Object.assign(new Error("no model is loaded"), { code: "no_model" });
  let text = "";
  let stats: any = null;
  const t0 = performance.now();
  let first = 0;
  try {
    await w.createCompletion({
      prompt,
      stream: true,
      max_tokens: s.max_tokens,
      temperature: s.temperature,
      temp: s.temperature,
      top_k: s.top_k,
      top_p: s.top_p,
      min_p: s.min_p,
      seed: s.seed,
      penalty_repeat: s.repeat_penalty ?? 1.0,
      penalty_last_n: s.repeat_last_n ?? 64,
      stop,
      abortSignal: signal,
      onData: (chunk: any) => {
        const t = chunk?.choices?.[0]?.text ?? "";
        if (t) {
          if (!first) first = performance.now();
          text += t;
          onToken(t);
        }
        if (chunk?.usage || chunk?.timings) {
          const u = chunk.usage ?? {};
          const tm = chunk.timings ?? {};
          stats = {
            prompt_tokens: u.prompt_tokens ?? tm.prompt_n ?? 0,
            generated_tokens: u.completion_tokens ?? tm.predicted_n ?? 0,
            prompt_ms: Math.round(tm.prompt_ms ?? (first ? first - t0 : 0)),
            gen_ms: Math.round(tm.predicted_ms ?? (first ? performance.now() - first : 0)),
          };
        }
      },
    } as any);
  } catch (e: any) {
    if (signal.aborted || e?.name === "AbortError") return { text, cancelled: true, stats };
    throw e;
  }
  return { text, cancelled: signal.aborted, stats: stats ?? { prompt_tokens: 0, generated_tokens: 0, prompt_ms: Math.round(first ? first - t0 : 0), gen_ms: Math.round(first ? performance.now() - first : 0) } };
}
