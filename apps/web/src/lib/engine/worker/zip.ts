// Reads the central directory of a .lbpack (ZIP, including ZIP64) from a File without loading it into memory,
// and streams single entries out (STORED as-is, DEFLATE through the browser's DecompressionStream).
// Untrusted input: offsets and sizes are bounds-checked against the file; callers check names and sizes against the
// manifest before writing anything.
export interface ZipEntry {
  name: string;
  method: number;
  compressedSize: number;
  size: number;
  localOffset: number;
  encrypted: boolean;
  isDir: boolean;
  isLink: boolean;
}

const dv = async (f: Blob, start: number, len: number) => new DataView(await f.slice(start, start + len).arrayBuffer());

function u64(v: DataView, o: number): number {
  const lo = v.getUint32(o, true);
  const hi = v.getUint32(o + 4, true);
  if (hi > 0x1fffff) throw new Error("zip offset too large");
  return hi * 2 ** 32 + lo;
}

export async function readDirectory(f: Blob): Promise<ZipEntry[]> {
  const tailLen = Math.min(f.size, 65557);
  const tail = await dv(f, f.size - tailLen, tailLen);
  let eocd = -1;
  for (let i = tailLen - 22; i >= 0; i--) if (tail.getUint32(i, true) === 0x06054b50) { eocd = i; break; }
  if (eocd < 0) throw new Error("not a zip file");
  let count = tail.getUint16(eocd + 10, true);
  let cdSize = tail.getUint32(eocd + 12, true);
  let cdOff = tail.getUint32(eocd + 16, true);
  if (count === 0xffff || cdOff === 0xffffffff || cdSize === 0xffffffff) {
    const locAt = eocd - 20;
    if (locAt < 0 || tail.getUint32(locAt, true) !== 0x07064b50) throw new Error("bad zip64 locator");
    const z64Off = u64(tail, locAt + 8);
    const z = await dv(f, z64Off, 56);
    if (z.getUint32(0, true) !== 0x06064b50) throw new Error("bad zip64 directory end");
    count = u64(z, 32);
    cdSize = u64(z, 40);
    cdOff = u64(z, 48);
  }
  if (cdOff + cdSize > f.size || count > 1_000_000) throw new Error("zip directory out of bounds");
  const cd = await dv(f, cdOff, cdSize);
  const out: ZipEntry[] = [];
  let p = 0;
  for (let n = 0; n < count; n++) {
    if (p + 46 > cdSize || cd.getUint32(p, true) !== 0x02014b50) throw new Error("damaged zip directory");
    const flags = cd.getUint16(p + 8, true);
    const method = cd.getUint16(p + 10, true);
    let csize = cd.getUint32(p + 20, true);
    let usize = cd.getUint32(p + 24, true);
    const nlen = cd.getUint16(p + 28, true);
    const xlen = cd.getUint16(p + 30, true);
    const clen = cd.getUint16(p + 32, true);
    const extAttr = cd.getUint32(p + 38, true);
    let local = cd.getUint32(p + 42, true);
    const nameBytes = new Uint8Array(cd.buffer, cd.byteOffset + p + 46, nlen);
    const name = new TextDecoder("utf-8", { fatal: true }).decode(nameBytes);
    let x = p + 46 + nlen;
    const xend = x + xlen;
    while (x + 4 <= xend) {
      const id = cd.getUint16(x, true);
      const sz = cd.getUint16(x + 2, true);
      if (id === 0x0001) {
        let q = x + 4;
        if (usize === 0xffffffff) { usize = u64(cd, q); q += 8; }
        if (csize === 0xffffffff) { csize = u64(cd, q); q += 8; }
        if (local === 0xffffffff) { local = u64(cd, q); q += 8; }
      }
      x += 4 + sz;
    }
    const mode = extAttr >>> 16;
    out.push({
      name, method, compressedSize: csize, size: usize, localOffset: local,
      encrypted: (flags & 1) === 1, isDir: name.endsWith("/"), isLink: (mode & 0o170000) === 0o120000,
    });
    p = xend + clen;
  }
  return out;
}

/** A stream of the entry's uncompressed bytes. */
export async function entryStream(f: Blob, e: ZipEntry): Promise<ReadableStream<Uint8Array>> {
  const h = await dv(f, e.localOffset, 30);
  if (h.getUint32(0, true) !== 0x04034b50) throw new Error(`damaged entry ${e.name}`);
  const start = e.localOffset + 30 + h.getUint16(26, true) + h.getUint16(28, true);
  if (start + e.compressedSize > f.size) throw new Error(`entry ${e.name} is out of bounds`);
  const raw = f.slice(start, start + e.compressedSize).stream();
  if (e.method === 0) return raw;
  if (e.method === 8) return raw.pipeThrough(new DecompressionStream("deflate-raw")) as ReadableStream<Uint8Array>;
  throw new Error(`entry ${e.name} uses unsupported compression ${e.method}`);
}
