// The browser engine: implements the engine API (crates/lbn-core/src/api.rs) inside a dedicated Web Worker.
// Pack rules (manifest validation, paths, hashes, signatures, activation, search, citations, retrieval, personal-data
// migration) come from lbn-core compiled to WebAssembly; this file only does browser I/O:
//   files      -> origin private file system:  builds/<job-id>/<pack files>
//   registry   -> IndexedDB (builds, active pointer per pack, jobs)
//   personal   -> IndexedDB kv "personal" {schema, data}, with a backup copy before every migration or import.
// A build is downloaded or unpacked into its own folder, every file is hashed, and only then one IndexedDB
// transaction makes it the active build. Nothing is ever written into an active build.
import init, * as core from "../../wasm/lbn";
import wasmUrl from "../../wasm/lbn_bg.wasm?url";
import trustedKeys from "../../../../../../keys/trusted.json";
import * as opfs from "./opfs";
import * as db from "./db";
import { readDirectory, entryStream } from "./zip";
import * as ai from "./ai";
import type { Command } from "../gen/Command";
import type { Event } from "../gen/Event";
import type { Reply } from "../gen/Reply";
import type { InstalledPack } from "../gen/InstalledPack";
import type { JobProgress } from "../gen/JobProgress";
import type { PackInfo } from "../gen/PackInfo";
import type { ResourceInfo } from "../gen/ResourceInfo";
import type { PackSource } from "../gen/PackSource";
import type { PersonalData } from "../gen/PersonalData";
import type { Hit } from "../gen/Hit";
import type { AskResult } from "../gen/AskResult";
import type { AskSource } from "../gen/AskSource";
import type { Settings } from "../gen/Settings";

export const API_VERSION = 1;
export const PERSONAL_SCHEMA = 2;
const APP_VERSION = "0.9.0-rc.1";

type Manifest = any; // validated by lbn-core before use
interface Build extends Omit<InstalledPack, "active"> { manifest: Manifest; source: string | null }
interface Active { id: string; sha: string; previous: string | null; activated_at: string }
interface JobRecord { id: string; source: PackSource; allow_unsigned: boolean; allow_downgrade: boolean; repair_of: string | null; progress: JobProgress }

class EngineError extends Error {
  constructor(public code: string, message: string) { super(message); }
}
const fail = (code: string, msg: string): never => { throw new EngineError(code, msg); };

function coreCall<T>(f: () => T): T {
  try {
    return f();
  } catch (e) {
    if (typeof e === "string" && e.startsWith("{")) {
      const j = JSON.parse(e);
      throw new EngineError(j.code, j.message);
    }
    throw e;
  }
}

let emit: (e: Event) => void = () => {};
const library = new Map<string, string>(); // pack id -> loaded manifest sha
let lib: core.Library;
const pickedFiles = new Map<string, File>();
const running = new Map<string, { cancel: boolean; pause: boolean }>();
const asks = new Map<string, AbortController>();
const now = () => new Date().toISOString();
const rid = () => Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join("");

export async function start(sink: (e: Event) => void) {
  emit = sink;
  await init({ module_or_path: wasmUrl });
  lib = new core.Library();
  // Jobs interrupted by closing the tab or a crash become "paused"; staging folders with no job are removed.
  const jobs = await db.all<JobRecord>("jobs");
  for (const j of jobs) {
    if (["downloading", "verifying", "activating", "queued"].includes(j.progress.state)) {
      j.progress.state = "paused";
      await db.put("jobs", j);
    }
  }
  const builds = await db.all<Build>("builds");
  const keep = new Set([...builds.map((b) => b.dir.split("/")[1]), ...jobs.map((j) => j.id)]);
  for (const name of await opfs.list("builds")) if (!keep.has(name)) await opfs.remove(`builds/${name}`);
}

export function registerFile(id: string, f: File) {
  pickedFiles.set(id, f);
}

export async function handle(json: string): Promise<string> {
  let id = 0;
  try {
    const req = JSON.parse(json);
    id = req.id ?? 0;
    if (req.v !== API_VERSION) fail("incompatible", `engine API ${req.v} is not supported`);
    const ok = await dispatch(req as Command);
    return JSON.stringify({ v: API_VERSION, id, ok: ok ?? null } satisfies Reply);
  } catch (e: any) {
    const code = e instanceof EngineError ? e.code : e?.name === "QuotaExceededError" ? "no_space" : "io";
    return JSON.stringify({ v: API_VERSION, id, err: { code, message: String(e?.message ?? e) } } satisfies Reply);
  }
}

// ------------------------------------------------------------------------------------------------------ registry --

async function activeBuilds(): Promise<Build[]> {
  const act = await db.all<Active>("active");
  const builds = await db.all<Build>("builds");
  return act.map((a) => builds.find((b) => b.id === a.id && b.manifest_sha256 === a.sha)).filter(Boolean) as Build[];
}
async function activeBuild(id: string): Promise<Build | undefined> {
  return (await activeBuilds()).find((b) => b.id === id);
}
async function mustActive(id: string): Promise<Build> {
  return (await activeBuild(id)) ?? fail("not_found", `pack ${id} is not installed on this device`);
}
function toInstalled(b: Build, active: boolean): InstalledPack {
  const { manifest: _m, source: _s, ...rest } = b;
  return { ...rest, active };
}

/** Files present at their manifest sizes? (Browsers may evict storage; this finds out.) */
async function health(b: Build): Promise<"ok" | "damaged"> {
  for (const f of b.manifest.files) {
    const s = await opfs.size(`${b.dir}/${f.path}`);
    if (s !== f.size) return "damaged";
  }
  return "ok";
}

async function ensureIndex(b: Build): Promise<boolean> {
  if (!b.manifest.index) return false;
  if (library.get(b.id) === b.manifest_sha256 && lib.has(b.id)) return true;
  const path = b.manifest.index.search;
  const f = await opfs.getFile(`${b.dir}/${path}`);
  if (!f) fail("damaged", `${b.id}: search index is missing (browser storage may have been cleared)`);
  const bytes = new Uint8Array(await f!.arrayBuffer());
  const entry = b.manifest.files.find((x: any) => x.path === path);
  coreCall(() => core.check_file(JSON.stringify(entry), bytes.length, core.sha256_hex(bytes)));
  coreCall(() => lib.add(b.id, bytes));
  library.set(b.id, b.manifest_sha256);
  return true;
}

function resourceInfo(m: Manifest, r: any): ResourceInfo {
  return {
    pack: m.id, id: r.id, type: r.type, title: r.title, path: r.path, media_type: r.media_type,
    pages: r.pages ?? null, duration_s: r.duration_s ?? null, transcript: r.transcript ?? null, poster: r.poster ?? null,
    map: r.map ?? null, sections: r.sections ?? [], source: r.source ?? null, license: r.license ?? m.license,
    category: r.category ?? null, summary: r.summary ?? null, pack_kind: m.kind,
  } as ResourceInfo;
}

// ------------------------------------------------------------------------------------------------------ personal --

async function personal(): Promise<PersonalData> {
  const rec = await db.get<{ schema: number; data: unknown }>("kv", "personal");
  if (!rec) return { bookmarks: [], notes: [], history: [], answers: [], settings: { ai_enabled: false, model: null, text_size: 100, theme: null } };
  if (rec.schema !== PERSONAL_SCHEMA) {
    // Migrate (or refuse a newer schema) with lbn-core; keep a backup copy of the old record first.
    const migrated = coreCall(() => core.personal_migrate(rec.schema, JSON.stringify(rec.data)));
    await db.put("kv", { at: now(), schema: rec.schema, data: rec.data }, `backup-${Date.now()}-v${rec.schema}`);
    const data = JSON.parse(migrated);
    await db.put("kv", { schema: PERSONAL_SCHEMA, data }, "personal");
    return data;
  }
  return rec.data as PersonalData;
}
async function savePersonal(d: PersonalData) {
  coreCall(() => core.personal_validate(JSON.stringify(d)));
  await db.put("kv", { schema: PERSONAL_SCHEMA, data: d }, "personal");
  emit({ event: "personal_changed" });
}

// ------------------------------------------------------------------------------------------------------ dispatch --

async function dispatch(c: Command): Promise<unknown> {
  switch (c.cmd) {
    case "hello": {
      const cap = ai.capability();
      return {
        api_version: API_VERSION, engine_version: APP_VERSION, platform: "browser",
        capabilities: { ai_runtime: cap.ok, ai_unavailable_reason: cap.ok ? null : cap.reason, evictable_storage: true, can_download: true, can_import_files: true },
      };
    }
    case "list_packs": {
      const builds = await db.all<Build>("builds");
      const out: PackInfo[] = [];
      for (const b of await activeBuilds()) {
        out.push({
          installed: toInstalled(b, true), description: b.manifest.description ?? "", publisher: b.manifest.publisher?.name ?? "",
          license: b.manifest.license, resource_count: b.manifest.resources.length,
          other_versions: builds.filter((o) => o.id === b.id && o.manifest_sha256 !== b.manifest_sha256).map((o) => o.version),
          health: await health(b),
        });
      }
      return out;
    }
    case "list_resources": {
      const out: ResourceInfo[] = [];
      for (const b of await activeBuilds()) if (!c.args.pack || c.args.pack === b.id) for (const r of b.manifest.resources) out.push(resourceInfo(b.manifest, r));
      return out;
    }
    case "get_resource": {
      const b = await mustActive(c.args.pack);
      const r = b.manifest.resources.find((x: any) => x.id === c.args.resource) ?? fail("not_found", `${c.args.resource} is not in ${c.args.pack}`);
      return resourceInfo(b.manifest, r);
    }
    case "read_catalog": return readCatalog(c.args.url);
    case "inspect_source": return inspect(c.args.source);
    case "install": return startInstall(c.args.source, c.args.allow_unsigned, c.args.allow_downgrade, null);
    case "pause_job": { const r = running.get(c.args.job); if (r) { r.pause = true; r.cancel = true; } return true; }
    case "resume_job": return resume(c.args.job);
    case "cancel_job": {
      const r = running.get(c.args.job);
      if (r) { r.pause = false; r.cancel = true; return true; }
      await opfs.remove(`builds/${c.args.job}`);
      await db.del("jobs", c.args.job);
      return true;
    }
    case "list_jobs": return (await db.all<JobRecord>("jobs")).map((j) => j.progress);
    case "verify_pack": return verifyBuild(await mustActive(c.args.pack));
    case "repair_pack": return repair(c.args.pack);
    case "remove_pack": return removePack(c.args.pack, c.args.version);
    case "rollback_pack": return rollback(c.args.pack);
    case "search": {
      const skipped: string[] = [];
      for (const b of await activeBuilds()) {
        if (b.kind === "model" || (c.args.packs && !c.args.packs.includes(b.id))) continue;
        try { await ensureIndex(b); } catch { skipped.push(b.id); lib.remove(b.id); library.delete(b.id); }
      }
      const hits = JSON.parse(lib.search(c.args.query, Math.min(Math.max(c.args.limit, 1), 100), JSON.stringify(c.args.packs ?? null)));
      return { hits: hits.filter((h: Hit) => !skipped.includes(h.citation.pack)), skipped };
    }
    case "get_chunk": {
      await ensureIndex(await mustActive(c.args.pack));
      return JSON.parse(coreCall(() => lib.chunk(c.args.pack, c.args.chunk)));
    }
    case "resolve_citation": return resolve(c.args.uri);
    case "ai_status": return aiStatus();
    case "load_model": { await loadModel(c.args.pack); return true; }
    case "unload_model": { await ai.unload(); return true; }
    case "ask": return ask(c.args.question, c.args.model);
    case "cancel_ask": { asks.get(c.args.ask)?.abort(); return true; }
    case "get_personal": return { data: await personal(), schema: PERSONAL_SCHEMA };
    case "save_bookmark": {
      coreCall(() => core.parse_citation(c.args.target));
      const d = await personal();
      let b = d.bookmarks.find((x) => x.target === c.args.target);
      if (!b) { b = { id: rid(), target: c.args.target, title: c.args.title, created: now() }; d.bookmarks.push(b); await savePersonal(d); }
      return b;
    }
    case "delete_bookmark": { const d = await personal(); d.bookmarks = d.bookmarks.filter((x) => x.id !== c.args.id); await savePersonal(d); return true; }
    case "save_note": {
      coreCall(() => core.parse_citation(c.args.target));
      const d = await personal();
      const id = c.args.id ?? rid();
      const ex = d.notes.find((n) => n.id === id);
      const n = { id, target: c.args.target, text: c.args.text, created: ex?.created ?? now(), updated: now() };
      d.notes = [n, ...d.notes.filter((x) => x.id !== id)];
      await savePersonal(d);
      return n;
    }
    case "delete_note": { const d = await personal(); d.notes = d.notes.filter((x) => x.id !== c.args.id); await savePersonal(d); return true; }
    case "add_history": {
      coreCall(() => core.parse_citation(c.args.target));
      const d = await personal();
      d.history = [...d.history, { target: c.args.target, title: c.args.title, at: now() }].slice(-500);
      await db.put("kv", { schema: PERSONAL_SCHEMA, data: d }, "personal");
      return true;
    }
    case "save_settings": { const d = await personal(); d.settings = c.args.settings; await savePersonal(d); if (!c.args.settings.ai_enabled) await ai.unload(); return true; }
    case "export_personal": return core.personal_export(JSON.stringify(await personal()), now(), APP_VERSION);
    case "import_personal": {
      const cur = await personal();
      const merged = JSON.parse(coreCall(() => core.personal_import(JSON.stringify(cur), new TextEncoder().encode(c.args.data))));
      await db.put("kv", { at: now(), schema: PERSONAL_SCHEMA, data: cur }, `backup-${Date.now()}-before-import`);
      await savePersonal(merged);
      return { data: merged, schema: PERSONAL_SCHEMA };
    }
    case "storage_status": {
      const est = await navigator.storage.estimate();
      return {
        used_bytes: est.usage ?? 0, free_bytes: est.quota != null && est.usage != null ? Math.max(0, est.quota - est.usage) : null,
        quota_bytes: est.quota ?? null, persisted: await navigator.storage.persisted(), evictable: true, location: "this browser's site storage",
      };
    }
    case "request_persistence": return navigator.storage.persisted(); // persist() itself is called by the page (window-only API)
  }
}

// ------------------------------------------------------------------------------------------------------ catalogs --

async function readCatalog(url: string) {
  const r = await fetch(url, { cache: "no-cache" }).catch(() => fail("network", `could not reach ${url}`));
  if (!r.ok) fail("network", `${url}: HTTP ${r.status}`);
  const bytes = new Uint8Array(await r.arrayBuffer());
  const { catalog, expired } = JSON.parse(coreCall(() => core.parse_catalog(bytes, now())));
  let signed_by: string | null = null;
  const sr = await fetch(url + ".minisig", { cache: "no-cache" }).catch(() => null);
  if (sr && sr.ok) {
    const sig = await sr.text();
    signed_by = coreCall(() => core.verify_signature(bytes, sig, JSON.stringify(trustedKeys)));
  }
  const base = new URL(".", new URL(url, location.href)).href;
  const packs = [];
  for (const e of catalog.packs) {
    const ab = await activeBuild(e.id);
    packs.push({ entry: e, manifest_url: new URL(e.manifest, base).href, installed_version: ab?.version ?? null });
  }
  return { url, title: catalog.title, published: catalog.published, expires: catalog.expires, expired, signed_by, packs };
}

// ---------------------------------------------------------------------------------------------------- installing --

const manifestUrl = (u: string) => (u.endsWith("/pack.json") ? u : u.replace(/\/$/, "") + "/pack.json");
const fileUrl = (base: string, rel: string) => base + rel.split("/").map(encodeURIComponent).join("/");

async function fetchSmall(url: string, max: number): Promise<Uint8Array | null> {
  const r = await fetch(url, { cache: "no-cache" }).catch(() => fail("network", `could not reach ${url}`));
  if (r.status === 404) return null;
  if (!r.ok) fail("network", `${url}: HTTP ${r.status}`);
  const b = new Uint8Array(await r.arrayBuffer());
  if (b.length > max) fail("network", `${url} is larger than expected`);
  return b;
}

function verifySig(mbytes: Uint8Array, sig: string | null, allowUnsigned: boolean): string | null {
  if (sig != null) return coreCall(() => core.verify_signature(mbytes, sig, JSON.stringify(trustedKeys)));
  if (!allowUnsigned) fail("unsigned", "this pack is not signed by a trusted publisher; install it only if you trust where it came from");
  return null;
}

async function readLocalManifest(f: File): Promise<{ mbytes: Uint8Array; sig: string | null; entries: Awaited<ReturnType<typeof readDirectory>> }> {
  const entries = await readDirectory(f).catch((e) => fail("unpack", `not a pack file: ${e.message}`));
  const read = async (name: string, max: number) => {
    const e = entries.find((x) => x.name === name);
    if (!e) return null;
    if (e.size > max) fail("unpack", `${name} is too large`);
    return new Uint8Array(await new Response(await entryStream(f, e)).arrayBuffer());
  };
  const mbytes = (await read("pack.json", 32 << 20)) ?? fail("manifest", "the pack file has no pack.json at its top");
  const s = await read("pack.json.minisig", 1 << 16);
  return { mbytes: mbytes!, sig: s ? new TextDecoder().decode(s) : null, entries };
}

function localFile(path: string): File {
  const f = pickedFiles.get(path.replace(/^file:/, ""));
  return f ?? fail("not_found", "the chosen file is no longer available; choose it again");
}

async function inspect(source: PackSource) {
  let mbytes: Uint8Array, sig: string | null;
  if (source.type === "url") {
    const mu = manifestUrl(source.url);
    mbytes = (await fetchSmall(mu, 32 << 20)) ?? fail("not_found", `no pack at ${source.url}`);
    const s = await fetchSmall(mu + ".minisig", 1 << 16);
    sig = s ? new TextDecoder().decode(s) : null;
  } else {
    ({ mbytes, sig } = await readLocalManifest(localFile(source.path)));
  }
  const p = JSON.parse(coreCall(() => core.parse_manifest(mbytes)));
  const signed_by = sig ? coreCall(() => core.verify_signature(mbytes, sig!, JSON.stringify(trustedKeys))) : null;
  const m = p.manifest;
  return {
    id: m.id, version: m.version, kind: m.kind, title: m.title, description: m.description, publisher: m.publisher.name, license: m.license,
    bytes: m.files.reduce((n: number, f: any) => n + f.size, 0), files: m.files.length, resources: m.resources.length,
    signed_by, installed_version: (await activeBuild(m.id))?.version ?? null, model: m.model ?? null, unsupported: [],
  };
}

async function startInstall(source: PackSource, allow_unsigned: boolean, allow_downgrade: boolean, repair_of: string | null): Promise<JobProgress> {
  const id = `job-${rid()}`;
  const title = source.type === "url" ? source.url : pickedFiles.get(source.path.replace(/^file:/, ""))?.name ?? "pack file";
  const j: JobRecord = {
    id, source, allow_unsigned, allow_downgrade, repair_of,
    progress: { job: id, pack_id: "", version: "", title, state: "queued", bytes_done: 0, bytes_total: 0, files_done: 0, files_total: 0, current_file: null, error: null },
  };
  await db.put("jobs", j);
  runJob(j);
  return j.progress;
}

async function resume(job: string) {
  if (running.has(job)) fail("busy", "already running");
  const j = (await db.get<JobRecord>("jobs", job)) ?? fail("not_found", "no such job");
  j!.progress.error = null;
  j!.progress.state = "queued";
  await db.put("jobs", j);
  runJob(j!);
  return j!.progress;
}

function runJob(j: JobRecord) {
  const ctl = { cancel: false, pause: false };
  running.set(j.id, ctl);
  (async () => {
    try {
      await doJob(j, ctl);
      j.progress.state = "done";
      j.progress.current_file = null;
      await db.del("jobs", j.id);
      emit({ event: "job", data: j.progress });
      emit({ event: "packs_changed" });
    } catch (e: any) {
      const code = e instanceof EngineError ? e.code : e?.name === "QuotaExceededError" ? "no_space" : "io";
      if (code === "cancelled" && ctl.pause) {
        j.progress.state = "paused";
        await db.put("jobs", j);
      } else if (code === "cancelled") {
        j.progress.state = "cancelled";
        await opfs.remove(`builds/${j.id}`);
        await db.del("jobs", j.id);
      } else {
        if (!["network", "no_space", "io"].includes(code)) await opfs.remove(`builds/${j.id}`);
        j.progress.state = "failed";
        j.progress.error = { code, message: String(e?.message ?? e) };
        await db.put("jobs", j);
      }
      emit({ event: "job", data: j.progress });
    } finally {
      running.delete(j.id);
    }
  })();
}

async function doJob(j: JobRecord, ctl: { cancel: boolean }) {
  const dir = `builds/${j.id}`;
  let last = 0;
  const report = async (force = false) => {
    if (force || Date.now() - last > 200) {
      last = Date.now();
      await db.put("jobs", j);
      emit({ event: "job", data: { ...j.progress } });
    }
  };
  const checkCancel = () => { if (ctl.cancel) fail("cancelled", "stopped"); };
  j.progress.state = "downloading";
  let mbytes: Uint8Array;
  let sig: string | null;
  let sourceLabel: string;
  let local: { file: File; entries: Awaited<ReturnType<typeof readDirectory>> } | null = null;
  if (j.source.type === "url") {
    const mu = manifestUrl(j.source.url);
    sourceLabel = mu;
    mbytes = (await fetchSmall(mu, 32 << 20)) ?? fail("not_found", `no pack at ${j.source.url}`);
    const s = await fetchSmall(mu + ".minisig", 1 << 16);
    sig = s ? new TextDecoder().decode(s) : null;
  } else {
    const f = localFile(j.source.path);
    sourceLabel = `file:${f.name}`;
    const r = await readLocalManifest(f);
    ({ mbytes, sig } = r);
    local = { file: f, entries: r.entries };
  }
  const parsed = JSON.parse(coreCall(() => core.parse_manifest(mbytes)));
  const m = parsed.manifest;
  // Untrusted or tampered manifests are refused before any large transfer.
  const signedBy = verifySig(mbytes, sig, j.allow_unsigned);
  const cur = await activeBuild(m.id);
  if (cur && cur.manifest_sha256 === parsed.sha256 && !j.repair_of) fail("nothing", `${m.title} ${m.version} is already installed`);
  const old = await opfs.getFile(`${dir}/.pack.json`);
  if (old && new Uint8Array(await old.arrayBuffer()).join() !== mbytes.join()) await opfs.remove(dir); // publisher changed the pack
  await writeAll(`${dir}/.pack.json`, mbytes);
  Object.assign(j.progress, { pack_id: m.id, version: m.version, title: m.title, files_total: m.files.length, bytes_total: m.files.reduce((n: number, f: any) => n + f.size, 0) });
  const staged: Record<string, number> = {};
  for (const f of m.files) {
    const s = (await opfs.size(`${dir}/${f.path}`)) ?? (await opfs.size(`${dir}/${f.path}.part`));
    if (s != null) staged[f.path] = s;
  }
  const { plan, space } = JSON.parse(coreCall(() => core.install_plan(JSON.stringify(m), JSON.stringify(staged))));
  const est = await navigator.storage.estimate();
  if (est.quota != null && est.usage != null && est.quota - est.usage < space) {
    fail("no_space", `not enough browser storage: ${Math.ceil(space / 2 ** 20)} MB needed, about ${Math.floor((est.quota - est.usage) / 2 ** 20)} MB available. Free space or remove packs.`);
  }
  j.progress.bytes_done = 0;
  j.progress.files_done = 0;
  if (local) {
    const listed = new Map<string, number>(m.files.map((f: any) => [f.path, f.size]));
    for (const e of local.entries) {
      if (e.isDir) continue;
      coreCall(() => core.check_path(e.name));
      if (e.encrypted) fail("unpack", `${e.name} is encrypted`);
      if (e.isLink) fail("unsafe_path", `${e.name} is a link`);
      if (e.name === "pack.json" || e.name === "pack.json.minisig") continue;
      if (!listed.has(e.name)) fail("manifest", `${e.name} is in the pack file but not in its manifest`);
      if (listed.get(e.name) !== e.size) fail("size", `${e.name}: size in the pack file differs from the manifest`);
    }
  }
  const base = j.source.type === "url" ? manifestUrl(j.source.url).replace(/pack\.json$/, "") : "";
  for (const [fe, act] of plan as [any, any][]) {
    checkCancel();
    if (act.action === "verify") { j.progress.bytes_done += fe.size; j.progress.files_done++; continue; }
    j.progress.current_file = fe.path;
    await report(true);
    const part = `${dir}/${fe.path}.part`;
    if (local) {
      const e = local.entries.find((x) => x.name === fe.path) ?? fail("missing", `${fe.path} is missing from the pack file`);
      await streamTo(part, await entryStream(local.file, e!), 0, fe.size, ctl, (n) => { j.progress.bytes_done += n; report(); });
    } else {
      let attempts = 0;
      for (;;) {
        const before = j.progress.bytes_done;
        try {
          await download(fileUrl(base, fe.path), part, fe.size, ctl, (n) => { j.progress.bytes_done += n; report(); });
          break;
        } catch (e: any) {
          j.progress.bytes_done = before;
          if (e instanceof EngineError && e.code === "network" && attempts < 3 && !ctl.cancel) {
            attempts++;
            await new Promise((r) => setTimeout(r, 500 * attempts));
          } else throw e;
        }
      }
    }
    await opfs.rename(part, `${dir}/${fe.path}`);
    j.progress.files_done++;
  }
  checkCancel();
  // Verify every file (including resumed ones), then activate in one transaction.
  j.progress.state = "verifying";
  j.progress.current_file = null;
  j.progress.bytes_done = 0;
  await report(true);
  for (const fe of m.files) {
    checkCancel();
    const f = (await opfs.getFile(`${dir}/${fe.path}`)) ?? fail("missing", `${fe.path} is missing`);
    const h = new core.Hasher();
    const reader = f!.stream().getReader();
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      h.update(value);
      j.progress.bytes_done += value.length;
      report();
    }
    const n = h.bytes();
    coreCall(() => core.check_file(JSON.stringify(fe), n, h.finish()));
  }
  j.progress.state = "activating";
  await report(true);
  coreCall(() => core.can_activate(JSON.stringify(m), JSON.stringify(cur ? toInstalled(cur, true) : null), j.allow_downgrade));
  if (ai.loadedPack() === m.id) await ai.unload();
  const build: Build = {
    id: m.id, version: m.version, manifest_sha256: parsed.sha256, dir, kind: m.kind, title: m.title,
    bytes: m.files.reduce((n: number, f: any) => n + f.size, 0), installed_at: now(), signed_by: signedBy, manifest: m, source: sourceLabel,
  };
  await db.atomically(["builds", "active", "jobs"], (tx) => {
    tx.objectStore("builds").put(build);
    const prev = cur && cur.manifest_sha256 !== parsed.sha256 ? cur.manifest_sha256 : null;
    tx.objectStore("active").put({ id: m.id, sha: parsed.sha256, previous: prev, activated_at: now() } satisfies Active);
    tx.objectStore("jobs").delete(j.id);
  });
  library.delete(m.id);
  lib.remove(m.id);
}

async function writeAll(path: string, data: Uint8Array) {
  const h = await opfs.openWrite(path, true);
  try { h.write(data, { at: 0 }); h.flush(); } finally { h.close(); }
}

async function streamTo(path: string, stream: ReadableStream<Uint8Array>, from: number, size: number, ctl: { cancel: boolean }, progress: (n: number) => void) {
  const h = await opfs.openWrite(path, from === 0);
  let at = from;
  const reader = stream.getReader();
  try {
    for (;;) {
      if (ctl.cancel) { await reader.cancel(); fail("cancelled", "stopped"); }
      const { value, done } = await reader.read();
      if (done) break;
      if (at + value.length > size) fail("size", `${path} is larger than the manifest says`);
      h.write(value, { at });
      at += value.length;
      progress(value.length);
    }
    h.flush();
  } finally {
    h.close();
  }
  // A short body is a dropped connection (a longer one was refused above): keep the part and resume later.
  if (at !== size) fail("network", `${path}: connection ended after ${at} of ${size} bytes`);
}

async function download(url: string, part: string, size: number, ctl: { cancel: boolean }, progress: (n: number) => void) {
  let have = (await opfs.size(part)) ?? 0;
  if (have > size) { await opfs.remove(part); have = 0; }
  if (have === size && size > 0) return;
  const headers: Record<string, string> = have > 0 ? { Range: `bytes=${have}-` } : {};
  const r = await fetch(url, { headers, cache: "no-store" }).catch(() => fail("network", "the connection failed"));
  let from = 0;
  if (r.status === 206 && have > 0) {
    const cr = r.headers.get("content-range") ?? "";
    if (!cr.startsWith(`bytes ${have}-`)) fail("network", `server answered an unexpected range (${cr})`);
    from = have;
    progress(have);
  } else if (r.status !== 200) {
    fail("network", `${url}: HTTP ${r.status}`);
  }
  try {
    await streamTo(part, r.body!, from, size, ctl, progress);
  } catch (e: any) {
    if (e instanceof EngineError) throw e;
    fail("network", `connection ended early: ${e?.message ?? e}`);
  }
}

async function verifyBuild(b: Build) {
  const rep = { pack: b.id, version: b.version, ok: true, checked: 0, damaged: [] as string[], missing: [] as string[] };
  for (const fe of b.manifest.files) {
    const f = await opfs.getFile(`${b.dir}/${fe.path}`);
    if (!f) { rep.missing.push(fe.path); continue; }
    const h = new core.Hasher();
    const reader = f.stream().getReader();
    for (;;) { const { value, done } = await reader.read(); if (done) break; h.update(value); }
    rep.checked++;
    const n = h.bytes();
    const sha = h.finish();
    if (n !== fe.size || sha !== fe.sha256) rep.damaged.push(fe.path);
  }
  rep.ok = !rep.damaged.length && !rep.missing.length;
  return rep;
}

async function repair(pack: string) {
  const b = await mustActive(pack);
  const report = await verifyBuild(b);
  if (report.ok) return { report, job: null };
  if (b.source?.startsWith("http")) {
    const job = await startInstall({ type: "url", url: b.source }, b.signed_by == null, false, pack);
    return { report, job };
  }
  return fail("repair_source", "this pack came from a file; import the same .lbpack file again to repair it");
}

async function removePack(id: string, version: string | null) {
  const builds = (await db.all<Build>("builds")).filter((b) => b.id === id);
  if (!builds.length) fail("not_found", `${id} is not installed`);
  const act = await db.get<Active>("active", id);
  const targets = version ? builds.filter((b) => b.version === version) : builds;
  if (version && targets.some((b) => b.manifest_sha256 === act?.sha)) fail("busy", "that version is in use");
  if (ai.loadedPack() === id) await ai.unload();
  await db.atomically(["builds", "active"], (tx) => {
    for (const b of targets) tx.objectStore("builds").delete([b.id, b.manifest_sha256]);
    if (!version) tx.objectStore("active").delete(id);
    else if (act && targets.some((b) => b.manifest_sha256 === act.previous)) tx.objectStore("active").put({ ...act, previous: null });
  });
  let freed = 0;
  for (const b of targets) { await opfs.remove(b.dir); freed += b.bytes; }
  lib.remove(id);
  library.delete(id);
  emit({ event: "packs_changed" });
  return { freed_bytes: freed };
}

async function rollback(id: string) {
  const act = (await db.get<Active>("active", id)) ?? fail("not_found", `${id} is not installed`);
  if (!act!.previous) fail("nothing", "there is no earlier version to go back to");
  const prev = (await db.get<Build>("builds", [id, act!.previous!])) ?? fail("nothing", "the earlier version has been removed");
  if ((await health(prev!)) !== "ok") fail("damaged", "the earlier version is damaged");
  await db.put("active", { id, sha: prev!.manifest_sha256, previous: act!.sha, activated_at: now() } satisfies Active);
  lib.remove(id);
  library.delete(id);
  emit({ event: "packs_changed" });
  return toInstalled(prev!, true);
}

async function resolve(uri: string) {
  let citation: any = null;
  try {
    citation = JSON.parse(coreCall(() => core.parse_citation(uri)));
    const b = (await activeBuild(citation.pack)) ?? fail("not_found", `the pack ${citation.pack} is not installed on this device`);
    coreCall(() => core.validate_citation(uri, JSON.stringify(b!.manifest)));
    const r = b!.manifest.resources.find((x: any) => x.id === citation.resource);
    return { uri, valid: true, error: null, citation, resource: resourceInfo(b!.manifest, r) };
  } catch (e: any) {
    return { uri, valid: false, error: String(e?.message ?? e), citation, resource: null };
  }
}

// ------------------------------------------------------------------------------------------------------------ AI --

async function modelBuilds(): Promise<Build[]> {
  return (await activeBuilds()).filter((b) => b.kind === "model");
}

async function aiStatus() {
  const cap = ai.capability();
  const settings = (await personal()).settings;
  const models = [];
  for (const b of await modelBuilds()) {
    const info = b.manifest.model;
    models.push({ pack: b.id, title: b.title, info, fits: ai.estimateFit(info) });
  }
  return {
    runtime: cap.ok, runtime_name: cap.ok ? `wllama (llama.cpp in WebAssembly, ${cap.threads > 1 ? cap.threads + " threads" : "1 thread"})` : null,
    unavailable_reason: cap.ok ? null : cap.reason, enabled: settings.ai_enabled, models, loaded: ai.loadedPack(), busy: asks.size > 0,
  };
}

async function loadModel(pack: string) {
  const b = await mustActive(pack);
  if (!b.manifest.model) fail("incompatible", `${pack} is not a model pack`);
  const cap = ai.capability();
  if (!cap.ok) fail("no_runtime", cap.reason);
  const r = b.manifest.resources.find((x: any) => x.type === "model");
  const f = (await opfs.getFile(`${b.dir}/${r.path}`)) ?? fail("damaged", "the model file is missing; repair or reinstall the model pack");
  await ai.load(pack, f!, b.manifest.model.context_length);
}

async function ask(question: string, model: string | null) {
  const q = question.trim();
  if (!q || q.length > 2000) fail("bad_request", "ask a question of 1 to 2000 characters");
  const settings: Settings = (await personal()).settings;
  if (!settings.ai_enabled) fail("ai_disabled", "local AI is turned off; turn it on in AI settings");
  const mid = model ?? settings.model ?? (await modelBuilds())[0]?.id ?? fail("no_model", "no AI model is installed");
  const askId = `ask-${rid()}`;
  const abort = new AbortController();
  asks.set(askId, abort);
  (async () => {
    try {
      emit({ event: "ask_done", data: await runAsk(askId, q, mid!, abort.signal) });
    } catch (e: any) {
      emit({ event: "ask_error", data: { ask: askId, code: e instanceof EngineError ? e.code : "runner", message: String(e?.message ?? e) } });
    } finally {
      asks.delete(askId);
    }
  })();
  return { ask: askId };
}

async function runAsk(askId: string, q: string, model: string, signal: AbortSignal): Promise<AskResult> {
  const status = (s: string) => emit({ event: "ask_status", data: { ask: askId, status: s } });
  status("searching");
  const sr: any = await dispatch({ cmd: "search", args: { query: q, limit: 12, packs: null } });
  const withText: [Hit, string][] = [];
  for (const h of sr.hits as Hit[]) {
    const c = JSON.parse(lib.chunk(h.citation.pack, h.chunk));
    withText.push([h, c.text]);
  }
  const b = await mustActive(model);
  const info = b.manifest.model ?? fail("incompatible", "not a model pack");
  const prep = JSON.parse(coreCall(() => core.prepare_answer(q, JSON.stringify(withText), info.template)));
  const toSources = (v: any[]): AskSource[] => v.map((s) => ({ key: s.key, citation: core.citation_uri(JSON.stringify(s.citation)), title: s.title, excerpt: String(s.text).slice(0, 400) }));
  if (!prep.sources) {
    const near = (sr.hits as Hit[]).slice(0, 5).map((h, i) => ({ key: `N${i + 1}`, citation: core.citation_uri(JSON.stringify(h.citation)), title: h.title, excerpt: h.snippet }));
    return { ask: askId, question: q, outcome: "no_evidence", answer: null, sources: near, model: null, stats: null };
  }
  if (ai.loadedPack() !== model) { status("loading_model"); await loadModel(model); }
  status("generating");
  const g = await ai.generate(prep.prompt, info.defaults, prep.stop, (t) => emit({ event: "ask_token", data: { ask: askId, text: t } }), signal);
  const checked = JSON.parse(coreCall(() => core.check_answer(g.text, JSON.stringify(prep.sources))));
  const valid = [];
  for (const c of checked.cited) if ((await resolve(c.citation)).valid) valid.push(c);
  checked.cited = valid;
  const outcome = g.cancelled ? "cancelled" : "answered";
  if (outcome === "answered") {
    const d = await personal();
    d.answers = [{ id: askId, question: q, answer: checked.text, citations: valid.map((c: any) => c.citation), model, at: now() }, ...d.answers].slice(0, 200);
    await db.put("kv", { schema: PERSONAL_SCHEMA, data: d }, "personal");
  }
  return { ask: askId, question: q, outcome, answer: checked, sources: toSources(prep.sources), model: b.title, stats: g.stats };
}
