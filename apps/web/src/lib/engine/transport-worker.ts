// Browser transport: the engine runs in a dedicated Web Worker; pack files are read from this origin's private
// file system (OPFS) on the main thread when the UI needs a URL or bytes.
import type { Transport } from "./client";
import type { Event } from "./gen/Event";
import EngineWorker from "./worker/index.ts?worker";

interface Build { id: string; manifest_sha256: string; dir: string }

async function activeDir(pack: string): Promise<string> {
  const db: IDBDatabase = await new Promise((res, rej) => {
    const r = indexedDB.open("lastbook-next");
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  });
  const get = <T>(store: string, key: IDBValidKey) =>
    new Promise<T | undefined>((res, rej) => {
      const r = db.transaction(store).objectStore(store).get(key);
      r.onsuccess = () => res(r.result as T);
      r.onerror = () => rej(r.error);
    });
  const act = await get<{ sha: string }>("active", pack);
  if (!act) throw new Error(`pack ${pack} is not installed`);
  const b = await get<Build>("builds", [pack, act.sha]);
  if (!b) throw new Error(`pack ${pack} is damaged`);
  db.close();
  return b.dir;
}

async function opfsFile(pack: string, path: string): Promise<File> {
  if (path.split("/").some((p) => !p || p === "." || p === "..")) throw new Error("bad path");
  const dir = await activeDir(pack);
  let d = await navigator.storage.getDirectory();
  const parts = `${dir}/${path}`.split("/");
  for (const p of parts.slice(0, -1)) d = await d.getDirectoryHandle(p);
  return (await d.getFileHandle(parts[parts.length - 1])).getFile();
}

export async function createWorkerTransport(): Promise<Transport> {
  const w = new EngineWorker();
  // Replies can arrive in any order (the worker handles requests concurrently): match them by request id.
  const pending = new Map<number, (json: string) => void>();
  const fileAcks = new Map<string, () => void>();
  let cb: (e: Event) => void = () => {};
  const urls = new Map<string, string>();
  w.onmessage = (m) => {
    const d = m.data;
    if (d.t === "rep") {
      const id = JSON.parse(d.json).id;
      pending.get(id)?.(d.json);
      pending.delete(id);
    }
    else if (d.t === "ev") cb(JSON.parse(d.json));
    else if (d.t === "file-ok") fileAcks.get(d.id)?.();
    else if (d.t === "fatal") console.error("engine failed to start:", d.message);
  };
  const pick = (accept: string) =>
    new Promise<File | null>((res) => {
      const i = document.createElement("input");
      i.type = "file";
      i.accept = accept;
      i.onchange = () => res(i.files?.[0] ?? null);
      i.oncancel = () => res(null);
      i.click();
    });
  return {
    platform: "browser",
    send: (json) => new Promise((res) => { pending.set(JSON.parse(json).id, res); w.postMessage({ t: "req", json }); }),
    onEvent: (f) => { cb = f; },
    async fileUrl(pack, path) {
      const key = `${pack}/${path}`;
      if (!urls.has(key)) urls.set(key, URL.createObjectURL(await opfsFile(pack, path)));
      return urls.get(key)!;
    },
    readFile: async (pack, path) => (await opfsFile(pack, path)).arrayBuffer(),
    readRange: async (pack, path, offset, length) => (await opfsFile(pack, path)).slice(offset, offset + length).arrayBuffer(),
    async pickPackSource(kind) {
      if (kind === "folder") return null; // folders of old Lastbook libraries are imported in the desktop app
      const f = await pick(".lbpack,.zip,application/zip");
      if (!f) return null;
      const id = crypto.randomUUID();
      await new Promise<void>((res) => { fileAcks.set(id, res); w.postMessage({ t: "file", id, file: f }); });
      return { type: "local", path: `file:${id}` };
    },
    async pickTextFile() {
      const f = await pick(".json,application/json");
      return f ? f.text() : null;
    },
    async saveTextFile(name, text) {
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([text], { type: "application/json" }));
      a.download = name;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
      return true;
    },
    async requestPersistence() {
      try {
        return (await navigator.storage.persisted()) || (await navigator.storage.persist());
      } catch {
        return false;
      }
    },
  };
}
