// The UI's single way to reach the engine, whichever platform runs it.
import type { Command } from "./gen/Command";
import type { Event } from "./gen/Event";
import type { Reply } from "./gen/Reply";
import type { PackSource } from "./gen/PackSource";

export const API_VERSION = 1;

export interface Transport {
  platform: "desktop" | "browser";
  send(json: string): Promise<string>;
  onEvent(cb: (e: Event) => void): void;
  /** URL a <video>, <img> or pdf.js can load for a pack file. */
  fileUrl(pack: string, path: string): Promise<string>;
  /** Whole file contents (small files: HTML, VTT, styles, glyphs). */
  readFile(pack: string, path: string): Promise<ArrayBuffer>;
  /** Byte range of a file (PMTiles). */
  readRange(pack: string, path: string, offset: number, length: number): Promise<ArrayBuffer>;
  /** Let the person choose a pack file (or, on desktop, an old Lastbook library folder). */
  pickPackSource(kind: "file" | "folder"): Promise<PackSource | null>;
  pickTextFile(): Promise<string | null>;
  saveTextFile(suggestedName: string, text: string): Promise<boolean>;
  requestPersistence(): Promise<boolean>;
}

export class EngineError extends Error {
  constructor(public code: string, message: string) {
    super(message);
  }
}

export class Engine {
  private next = 1;
  private listeners = new Set<(e: Event) => void>();
  constructor(public t: Transport) {
    t.onEvent((e) => this.listeners.forEach((l) => l(e)));
  }
  get platform() {
    return this.t.platform;
  }
  async call<T = any>(cmd: Command): Promise<T> {
    if (cmd.cmd === "request_persistence") return (await this.t.requestPersistence()) as T;
    const id = this.next++;
    const reply: Reply = JSON.parse(await this.t.send(JSON.stringify({ v: API_VERSION, id, ...cmd })));
    if (reply.err) throw new EngineError(reply.err.code, reply.err.message);
    return reply.ok as T;
  }
  on(cb: (e: Event) => void): () => void {
    this.listeners.add(cb);
    return () => this.listeners.delete(cb);
  }
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let engineP: Promise<Engine> | null = null;
export function getEngine(): Promise<Engine> {
  if (!engineP) {
    engineP = (async () => {
      const t = isTauri() ? await import("./transport-tauri").then((m) => m.createTauriTransport()) : await import("./transport-worker").then((m) => m.createWorkerTransport());
      return new Engine(t);
    })();
  }
  return engineP;
}
