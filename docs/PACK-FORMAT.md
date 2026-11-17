# Lastbook Ark pack format, version 1

A **pack** is an immutable, versioned bundle of library content (documents, encyclopedia articles, videos, maps) or of
one AI model. Packs are installed, verified, activated, rolled back and removed independently. This document is the
contract; the reference implementation is `crates/lbn-core/src/manifest.rs` (validation) and
`crates/lbn-native/src/build.rs` (building).

## Layout

A pack is a directory:

```
pack.json              manifest (this document); its SHA-256 identifies this build of the pack
pack.json.minisig      optional publisher signature over the exact bytes of pack.json (minisign, Ed25519)
<files...>             every file listed in pack.json "files" — nothing else
index/search.lbi       optional search index (content packs), itself listed in "files"
```

Transports:
- **HTTP(S) directory**: `https://host/path/<pack>/pack.json`, files fetched one by one at `…/<pack>/<path>`
  (path components percent-encoded), resumable with `Range`.
- **`.lbpack` file**: a ZIP (ZIP64 allowed) of the directory, `pack.json` at the top. Media and indexes are STORED,
  text may be DEFLATEd. Readers refuse entries not listed in `files`, links, encrypted entries, unsafe paths and
  sizes different from the manifest.

## pack.json

```json
{
  "format": "lastbook-pack",
  "format_version": 1,
  "id": "org.lastbook.sample.docs",
  "version": "2026.11.17",
  "kind": "documents",
  "title": "Sample documents: safe water and emergency supplies",
  "description": "…",
  "language": "en",
  "created": "2026-11-17",
  "publisher": {"name": "Lastbook Ark contributors", "url": "https://…"},
  "license": {"spdx": "CC0-1.0", "attribution": "…", "file": "LICENSES/CC0-1.0.txt", "redistributable": true},
  "compat": {"min_engine_api": 1, "note": "optional text for people"},
  "resources": [ … ],
  "files": [ {"path": "res/water-guide.html", "size": 2193, "sha256": "…64 lowercase hex…"} ],
  "index": {"search": "index/search.lbi"},
  "model": null
}
```

| Field | Rule |
|---|---|
| `format`, `format_version` | Must be `lastbook-pack` and ≤ the reader's version. A higher version is reported as *incompatible* (update the app), never guessed at. |
| `id` | Stable forever: 3–100 chars of `a-z 0-9 . -`, starting with a letter. Reverse-DNS style recommended. |
| `version` | Dotted numbers with an optional `-suffix` (`2026.11.17`, `2.5.0-q4km.2`). Compared numerically; a suffix sorts before the plain version. Installing an older version over a newer one needs explicit consent (rollback). |
| `kind` | `documents`, `encyclopedia`, `video`, `map`, `model` or `mixed`. |
| `license` | Pack-wide licence (SPDX id or `LicenseRef-…`). `redistributable: true` only when checked. A resource may override it. |
| `compat.min_engine_api` | Lowest engine API version that can use the pack. |
| `files` | Every file in the pack with exact `size` and `sha256`. Paths: relative, `/`-separated, no `.`/`..`/empty components, no backslash, drive letter, control or `<>:"|?*` characters, no trailing dot/space, no Windows reserved names, ≤ 512 bytes, no case-insensitive duplicates, no file that is also a directory. `pack.json` and its signature are not listed. |
| `index.search` | Path of the search index (see below), listed in `files`. |
| `model` | Present only (and required) for `kind: model`. |

### Resources

```json
{"id": "supplies-checklist", "type": "pdf", "title": "Emergency Supplies Checklist",
 "path": "res/supplies-checklist.pdf", "media_type": "application/pdf", "pages": 3,
 "source": {"url": "…", "publisher": "…", "retrieved": "…", "edition": "…", "notes": "…"},
 "license": {"spdx": "…"}, "category": "…", "summary": "…",
 "transcript": "res/x.en.vtt", "poster": "res/x.webp", "duration_s": 40.0,
 "map": {"style": "style.json", "bounds": [w, s, e, n], "minzoom": 0, "maxzoom": 15},
 "sections": [{"id": "boil", "title": "Boiling water"}], "assets": ["…"]}
```

| `type` | Required extra fields | Rendering |
|---|---|---|
| `html`, `article` | — (`sections` from `h2`/`h3` ids) | Sandboxed frame, no scripts, CSP blocks all loads; images from the pack |
| `pdf` | `pages` | pdf.js |
| `video` | `duration_s`; `transcript` (WebVTT) recommended | `<video>`; WebM VP9/Opus recommended (plays everywhere tested) |
| `map` | `map` (style, bounds, zooms) | MapLibre GL + PMTiles; style URLs are pack-relative |
| `model` | — | AI runtime only |

`id` is stable across versions of the pack (citations, bookmarks and notes refer to it): 1–128 chars of
`A-Za-z0-9 . _ - :`. Every path a resource names must be in `files`.

### Model packs

```json
"model": {"name": "Qwen2.5 1.5B Instruct", "upstream": "Qwen/Qwen2.5-1.5B-Instruct-GGUF@<revision>",
  "runtime": "llama.cpp", "min_runtime": "b12032", "format": "gguf", "quantization": "Q4_K_M",
  "tokenizer": "embedded in the GGUF file (Qwen2 BPE)", "template": "chatml", "context_length": 4096,
  "parameters": "1.54B", "memory_bytes": 1900000000, "min_ram_bytes": 4000000000, "languages": ["en", …],
  "defaults": {"temperature": 0.1, "top_k": 40, "top_p": 0.9, "min_p": 0.05, "max_tokens": 400, "seed": 1,
               "repeat_penalty": 1.1, "repeat_last_n": 64}}
```

Exactly one resource of type `model` (the GGUF file). Supported: `runtime: llama.cpp`, `format: gguf`,
`template: chatml`. Anything else is reported as incompatible.

## Citation targets

Stable text form, used by the AI, links, bookmarks, notes and exports:

| Form | Meaning |
|---|---|
| `lbn:<pack-id>/<resource-id>` | the whole resource |
| `…#page=12` | PDF page, 1-based as numbered in the file |
| `…#t=63.5` or `…#t=63.5,90` | video moment or span, seconds |
| `…#sec=boiling` | HTML section anchor (must be listed in `sections`) |
| `…#at=42.44,-76.50,15` | map position (lat, lon, zoom), inside the map's bounds |

A target is valid only if the pack is installed and the page/time/section/position exists in its manifest.

## Search index (`index/search.lbi`, version 1)

4 magic bytes `LBI1`, then a [postcard](https://postcard.jamesmunns.com/wire-format)-encoded structure: format
version, pack id, chunks (resource id, location, title, text), token counts, sorted term → postings (chunk, term
frequency), title terms. Tokens: lower-cased alphanumeric words, English stop words removed, Snowball English stems.
Ranking: BM25 (k1 1.2, b 0.75) × coverage of the query's distinct terms × a small title bonus. The index is built
when the pack is built, covered by the manifest hash, and read identically on desktop and in the browser
(`crates/lbn-core/src/index.rs`). Builds are deterministic.

## Signatures and trust

`pack.json.minisig` is a [minisign](https://jedisct1.github.io/minisign/) signature over the exact bytes of
`pack.json`. Because `pack.json` holds every file's SHA-256, a valid signature covers the whole pack. Apps ship a
list of trusted publisher keys (`keys/trusted.json`); people may add keys. Rules:

- valid signature by a trusted key → installed as *signed by <name>*;
- signature present but invalid or by an unknown key → **rejected** (a tampered or foreign pack);
- no signature → installed only after the person confirms they trust the source; shown as *unsigned*.

Catalogs (`catalog.json`, `format: lastbook-catalog`) list available packs with their manifest hashes and are signed
the same way. A catalog's `expires` date only limits finding new packs; installed packs never expire.

## Installing (all platforms)

1. Fetch/read `pack.json` (+ signature); validate; apply the signature rules **before** downloading large files.
2. Check free space (remaining bytes + 5 % + 64 MiB) or browser quota.
3. Write files into a new staging build (`<path>.part`, resumable), never into an active build.
4. Hash every file; any mismatch discards the build.
5. Activate in one atomic step (SQLite transaction natively, IndexedDB transaction in the browser), keeping the
   previous build for rollback.

## Building packs

`lastbook pack build <src> <out> [--sign key]` from a folder with `pack.src.json` (the manifest without
`format`, `files` and `index`; PDFs may give `text_pages`, a JSON array of page texts from
`apps/web/tools/pdf-pages.mjs`). `lastbook pack zip` writes the `.lbpack`. See `packs/sample/*/pack.src.json`.
