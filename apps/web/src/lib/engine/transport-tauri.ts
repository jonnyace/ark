// Desktop transport: the Rust engine runs in the Tauri process; the UI calls it over Tauri IPC and reads pack files
// through the app's own "lbn" URI scheme (served from the active build, read-only, with byte ranges).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Transport } from "./client";
import type { Event } from "./gen/Event";

const windows = navigator.userAgent.includes("Windows");
const base = windows ? "http://lbn.localhost" : "lbn://localhost";
const url = (pack: string, path: string) => `${base}/${encodeURIComponent(pack)}/${path.split("/").map(encodeURIComponent).join("/")}`;

export async function createTauriTransport(): Promise<Transport> {
  let cb: (e: Event) => void = () => {};
  const media = await invoke<string | null>("media_base");
  await listen<string>("engine-event", (e) => cb(JSON.parse(e.payload)));
  return {
    platform: "desktop",
    send: (json) => invoke<string>("engine_call", { request: json }),
    onEvent: (f) => { cb = f; },
    fileUrl: async (pack, path) => url(pack, path),
    // WebKitGTK cannot play media from a custom scheme; the app's loopback media responder serves it (see main.rs).
    mediaUrl: async (pack, path) =>
      media ? `${media}/${encodeURIComponent(pack)}/${path.split("/").map(encodeURIComponent).join("/")}` : url(pack, path),
    async readFile(pack, path) {
      const r = await fetch(url(pack, path));
      if (!r.ok) throw new Error(`${path}: ${r.status}`);
      return r.arrayBuffer();
    },
    async readRange(pack, path, offset, length) {
      const r = await fetch(url(pack, path), { headers: { Range: `bytes=${offset}-${offset + length - 1}` } });
      if (!r.ok) throw new Error(`${path}: ${r.status}`);
      return r.arrayBuffer();
    },
    async pickPackSource(kind) {
      const path = await invoke<string | null>("pick_path", { kind });
      return path ? { type: "local", path } : null;
    },
    pickTextFile: () => invoke<string | null>("pick_text_file"),
    saveTextFile: (name, text) => invoke<boolean>("save_text_file", { name, text }),
    requestPersistence: async () => true,
  };
}
