// Entry of the browser engine worker. Messages in: {t:"req", json} | {t:"file", id, file}. Out: {t:"rep", json} | {t:"ev", json}.
import { start, handle, registerFile } from "./engine";

const ready = start((ev) => postMessage({ t: "ev", json: JSON.stringify(ev) })).catch((e) => {
  postMessage({ t: "fatal", message: String(e?.message ?? e) });
  throw e;
});

onmessage = async (m: MessageEvent) => {
  const d = m.data;
  if (d.t === "file") {
    registerFile(d.id, d.file);
    postMessage({ t: "file-ok", id: d.id });
    return;
  }
  if (d.t === "req") {
    await ready;
    postMessage({ t: "rep", json: await handle(d.json) });
  }
};
