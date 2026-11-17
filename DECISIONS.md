# Decisions (architecture decision record)

Each entry: the decision, why, and what would change it. Newest entries at the bottom.
"Experiment" means something was run on the test machine; results are in TESTING.md.

## D-001 New repository; reuse upstream by copying with attribution
lastbook-next is a new repository (`~/src/lastbook-next`), not a fork. Upstream Lastbook
(Immortal-Protocols/lastbook, MIT) and lastbook-omarchy (MIT, same code base plus Arch packaging, upstream PR #60)
are read-only references. Reused, with attribution in NOTICE.md and file headers:
- `runner/lastbook_llm.cpp`, `runner/fit_unused_win.cpp` (pipe-only llama.cpp runner) and the build script logic.
- The library-safety rules: unpack beside, verify, switch with one rename; zip guards (slip, absolute paths, links,
  case clashes, size); download only when the person asks; SHA-256 on every file.
- The assistant rules (cite every claim with a source key, never invent keys, "Not found in the library", retrieved
  text is untrusted) — rewritten for a small model.
- The legacy library layout (`library-data/catalog.json`, `pack.json`, `pack-manifest.tsv`) for the importer.
Upstream is Python + per-OS shells and a custom map renderer; the brief asks for Rust/Svelte/Tauri/PWA with maintained
renderers, so code beyond the above is not reused.

## D-002 Layering
- `crates/lbn-core`: pure Rust, no I/O, compiles to native and `wasm32-unknown-unknown`. Pack manifest model and
  validation, path safety, hashing, verification plans, install-state machine, search index (build + query), citation
  targets and validation, retrieval + prompt assembly + answer citation parsing, personal-data schema, export format
  and migrations, the versioned engine command/event types.
- `crates/lbn-native`: native platform adapter: filesystem store (staging, atomic activation, GC), HTTP downloader
  with resume, zip import with guards, SQLite personal data + registry, llama.cpp runner adapter.
- `crates/lbn-cli`: `lastbook` command-line tool (inspect, build, import, verify, search, ask, export).
- `crates/lbn-wasm`: wasm-bindgen wrapper of `lbn-core` for the browser.
- `apps/web`: Svelte 5 + TypeScript UI built to static assets, shared by desktop and PWA. Talks to one `Engine`
  interface; two implementations: Tauri IPC (desktop) and a Web Worker running `lbn-wasm` with OPFS/IndexedDB (PWA).
- `apps/desktop`: Tauri 2 shell.
No server, no microservices. The CLI and desktop share `lbn-native`; desktop and browser share `lbn-core`.

## D-003 Pack format: directory + manifest; zip only as transport
A pack is a directory with `pack.json` (manifest) and files. It is distributed either as that directory over HTTP(S)
(file-by-file, resumable with Range, each file verified) or as one `.lbpack` file (a ZIP, entries STORED or DEFLATE)
for offline transfer. Installed packs live in content-addressed, never-modified directories
`packs/<pack-id>/<version>+<manifest-sha256-prefix>/`. Activation is an atomic pointer update in the registry
(SQLite transaction natively, IndexedDB transaction in the browser). Old versions stay until the new one has opened,
then are garbage-collected on request. This gives "never overwrite a working library" on both platforms, because the
browser (OPFS) has no atomic directory rename. Spec: `docs/PACK-FORMAT.md`.

## D-004 Content search: portable prebuilt BM25 index inside each pack
Each content pack carries `index/search.lbi`, built by `lastbook pack build` and covered by the manifest hashes.
The same Rust code reads it natively and in WASM, so results are identical on desktop and browser, the index is
verified like any other file, and nothing is rebuilt on import. SQLite FTS5 was considered for native search but
would give different ranking from the browser and would need sqlite-wasm/OPFS in the browser too.
SQLite is used natively for personal data and the pack registry (D-002); IndexedDB in the browser.
Limit: the index is loaded into memory per pack; very large packs (full Wikipedia) need a sharded/mmap index later.

## D-005 Integrity and trust: SHA-256 + minisign
Every file in a pack has size + SHA-256 in `pack.json`. The manifest may carry a detached minisign (Ed25519)
signature `pack.json.minisig`; verification uses the maintained `minisign-verify` crate (also in WASM). Trusted
publisher keys ship with the app and can be added by the person. Unsigned packs install only after an explicit
"install unsigned pack" confirmation and are labelled as such. The pack catalog (list of available packs) is signed the
same way and has an `expires` field that only gates *fetching* new packs: installed packs never expire.
No custom cryptography.

## D-006 Maps: PMTiles + MapLibre GL JS, assets bundled
Vector tiles in a single PMTiles file per map pack, rendered by MapLibre GL JS (BSD-3) through the `pmtiles` JS
library (BSD-3), which reads byte ranges from a local file/Blob. Glyphs and sprites are bundled locally; no tile,
font or style is fetched from the network. Sample map: a small extract of the Protomaps OpenStreetMap basemap
(ODbL, attribution in the pack).

## D-007 PDF: pdf.js in both shells; text extracted at pack-build time
pdf.js (pdfjs-dist, Apache-2.0) renders PDFs in WebKitGTK and in browsers. Per-page text for search and citations is
extracted when the pack is built (pdfjs-dist under Node) and stored in the pack, so the engine needs no PDF parser.

## D-008 Video: WebM (VP9 + Opus), WebVTT transcripts
Chromium builds without proprietary codecs and WebKitGTK with GStreamer good plugins both play VP9/Opus WebM.
Transcripts are WebVTT; citation targets are `{video, t_start, t_end}` in seconds.

## D-009 Native inference: upstream pipe-only runner on llama.cpp b12032, as a child process
The runner speaks JSON lines on stdin/stdout and opens no socket. Running it as a child process gives clean
cancellation (protocol cancel), unloading (quit/kill), and isolation from out-of-memory crashes. Prompt segments mark
which text may contain special tokens, so retrieved text cannot inject chat-template markup. CPU path supported;
Vulkan optional at build time (`LBN_VULKAN=1`).

## D-010 Initial model: Qwen2.5-1.5B-Instruct Q4_K_M (Apache-2.0)
Published by the Qwen team in `Qwen/Qwen2.5-1.5B-Instruct-GGUF`, licence file in the repository (Apache-2.0),
so it is redistributable inside an AI pack. Qwen2.5-0.5B-Instruct Q4_K_M (Apache-2.0) is the small/browser option.
Experiment: 1.5B answered a grounded question correctly; 0.5B did not (TESTING.md). Note: Qwen2.5-3B is NOT
Apache-2.0 and must not be packed without checking its licence.

## D-011 Browser inference: evaluate wllama (llama.cpp compiled to WASM)
Same GGUF models and chat template as desktop, CPU/WASM only, runs in a worker. Decided after the browser experiment.

## D-012 Pinning
Rust toolchain via `rust-toolchain.toml`; crates via `Cargo.lock`; JS via exact versions + `pnpm-lock.yaml`;
Docker base image by digest; llama.cpp by tag + commit; models by repository revision + SHA-256.

## D-013 Linux desktop build and tests run in a container
The test machine is headless, lacks WebKitGTK development packages and has no passwordless sudo. The desktop is
built and tested in `build/docker/Dockerfile` (Debian 13, pinned digest) under Xvfb with WebKitWebDriver; offline
tests run the container with `--network none`. The same packages on a Debian 13 host work without Docker.
