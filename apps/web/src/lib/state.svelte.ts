// Application state shared by the views, refreshed from engine events.
import { getEngine, type Engine } from "./engine/client";
import type { HelloReply } from "./engine/gen/HelloReply";
import type { PackInfo } from "./engine/gen/PackInfo";
import type { JobProgress } from "./engine/gen/JobProgress";
import type { AiStatus } from "./engine/gen/AiStatus";
import type { PersonalData } from "./engine/gen/PersonalData";
import type { StorageStatus } from "./engine/gen/StorageStatus";

export const app = $state({
  ready: false,
  error: "" as string,
  hello: null as HelloReply | null,
  packs: [] as PackInfo[],
  jobs: [] as JobProgress[],
  ai: null as AiStatus | null,
  personal: null as PersonalData | null,
  storage: null as StorageStatus | null,
  online: navigator.onLine,
  announce: "" as string,
});

let engine: Engine;
export function eng(): Engine {
  return engine;
}

export function say(msg: string) {
  app.announce = "";
  queueMicrotask(() => (app.announce = msg));
}

export async function refreshPacks() {
  app.packs = await engine.call({ cmd: "list_packs" });
  app.jobs = await engine.call({ cmd: "list_jobs" });
}
export async function refreshAi() {
  app.ai = await engine.call({ cmd: "ai_status" });
}
export async function refreshPersonal() {
  app.personal = (await engine.call<{ data: PersonalData }>({ cmd: "get_personal" })).data;
}
export async function refreshStorage() {
  app.storage = await engine.call({ cmd: "storage_status" });
}

export async function boot() {
  try {
    engine = await getEngine();
    app.hello = await engine.call({ cmd: "hello", args: { client: "ark-library-ui" } });
    engine.on((e) => {
      if (e.event === "job") {
        const i = app.jobs.findIndex((j) => j.job === e.data.job);
        if (i >= 0) app.jobs[i] = e.data;
        else app.jobs = [...app.jobs, e.data];
        if (["done", "failed", "cancelled", "paused"].includes(e.data.state)) {
          say(`${e.data.title}: ${e.data.state === "done" ? "installed" : e.data.state}${e.data.error ? ". " + e.data.error.message : ""}`);
          refreshPacks();
          refreshStorage();
        }
      } else if (e.event === "packs_changed") {
        refreshPacks();
        refreshAi();
      } else if (e.event === "personal_changed") {
        refreshPersonal();
      }
    });
    await Promise.all([refreshPacks(), refreshPersonal(), refreshStorage()]);
    app.ready = true;
    refreshAi().catch(() => {});
  } catch (e: any) {
    app.error = String(e?.message ?? e);
  }
  addEventListener("online", () => (app.online = true));
  addEventListener("offline", () => (app.online = false));
}

export function fmtBytes(n: number | null | undefined): string {
  if (n == null) return "unknown";
  if (n < 1024) return `${n} B`;
  const u = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v < 10 ? 1 : 0)} ${u[i]}`;
}
