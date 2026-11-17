// Origin private file system helpers for the engine worker. Paths are "/"-joined, already validated pack paths.

export async function root(): Promise<FileSystemDirectoryHandle> {
  return navigator.storage.getDirectory();
}

export async function dir(path: string, create = false): Promise<FileSystemDirectoryHandle> {
  let d = await root();
  for (const part of path.split("/").filter(Boolean)) d = await d.getDirectoryHandle(part, { create });
  return d;
}

function split(path: string): [string, string] {
  const i = path.lastIndexOf("/");
  return i < 0 ? ["", path] : [path.slice(0, i), path.slice(i + 1)];
}

export async function fileHandle(path: string, create = false): Promise<FileSystemFileHandle> {
  const [d, name] = split(path);
  return (await dir(d, create)).getFileHandle(name, { create });
}

export async function getFile(path: string): Promise<File | null> {
  try {
    return await (await fileHandle(path)).getFile();
  } catch {
    return null;
  }
}

export async function size(path: string): Promise<number | null> {
  const f = await getFile(path);
  return f ? f.size : null;
}

export async function remove(path: string): Promise<void> {
  const [d, name] = split(path);
  try {
    await (await dir(d)).removeEntry(name, { recursive: true });
  } catch {
    /* already gone */
  }
}

export async function list(path: string): Promise<string[]> {
  const out: string[] = [];
  try {
    const d = await dir(path);
    // @ts-ignore: async iteration of directory handles is supported in all OPFS browsers
    for await (const [name] of d.entries()) out.push(name);
  } catch {
    /* missing */
  }
  return out;
}

/** Open a file for writing with a synchronous access handle (dedicated workers only). */
export async function openWrite(path: string, truncate: boolean): Promise<FileSystemSyncAccessHandle> {
  const h = await (await fileHandle(path, true)).createSyncAccessHandle();
  if (truncate) h.truncate(0);
  return h;
}

export async function rename(from: string, to: string): Promise<void> {
  const h = await fileHandle(from);
  const [d, name] = split(to);
  // @ts-ignore: FileSystemHandle.move is supported in Chromium/Firefox/Safari OPFS
  await h.move(await dir(d, true), name);
}
