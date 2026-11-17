// Small promise wrapper around IndexedDB for the browser registry and personal data.
// Stores: builds [id, manifest_sha256] -> Build; active id -> Active; jobs id -> JobRecord; kv key -> value (personal data,
// backups, settings that must survive an app update).
export const DB_NAME = "lastbook-ark";
export const DB_VERSION = 1;

let dbp: Promise<IDBDatabase> | null = null;

export function open(): Promise<IDBDatabase> {
  if (!dbp) {
    dbp = new Promise((resolve, reject) => {
      const r = indexedDB.open(DB_NAME, DB_VERSION);
      r.onupgradeneeded = () => {
        const db = r.result;
        if (!db.objectStoreNames.contains("builds")) db.createObjectStore("builds", { keyPath: ["id", "manifest_sha256"] });
        if (!db.objectStoreNames.contains("active")) db.createObjectStore("active", { keyPath: "id" });
        if (!db.objectStoreNames.contains("jobs")) db.createObjectStore("jobs", { keyPath: "id" });
        if (!db.objectStoreNames.contains("kv")) db.createObjectStore("kv");
      };
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => reject(r.error);
      r.onblocked = () => reject(new Error("database blocked by another tab of an older version"));
    });
  }
  return dbp;
}

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((res, rej) => {
    r.onsuccess = () => res(r.result);
    r.onerror = () => rej(r.error);
  });
}

export async function all<T>(store: string): Promise<T[]> {
  const db = await open();
  return req(db.transaction(store).objectStore(store).getAll()) as Promise<T[]>;
}

export async function get<T>(store: string, key: IDBValidKey): Promise<T | undefined> {
  const db = await open();
  return req(db.transaction(store).objectStore(store).get(key)) as Promise<T | undefined>;
}

export async function put(store: string, value: unknown, key?: IDBValidKey): Promise<void> {
  const db = await open();
  const tx = db.transaction(store, "readwrite");
  tx.objectStore(store).put(value, key);
  await done(tx);
}

export async function del(store: string, key: IDBValidKey): Promise<void> {
  const db = await open();
  const tx = db.transaction(store, "readwrite");
  tx.objectStore(store).delete(key);
  await done(tx);
}

export function done(tx: IDBTransaction): Promise<void> {
  return new Promise((res, rej) => {
    tx.oncomplete = () => res();
    tx.onerror = () => rej(tx.error);
    tx.onabort = () => rej(tx.error ?? new Error("transaction aborted"));
  });
}

/** Run several writes in one atomic transaction. */
export async function atomically(stores: string[], fn: (tx: IDBTransaction) => void): Promise<void> {
  const db = await open();
  const tx = db.transaction(stores, "readwrite");
  fn(tx);
  await done(tx);
}
