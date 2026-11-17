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

## D-011 (resolved) Browser inference: wllama works on Chromium, Firefox and WebKit
Experiment (TESTING.md): the same Qwen2.5-1.5B GGUF and prompt answer correctly with citations in Chromium 159
(~38 s per answer, 8 threads, cross-origin isolated) and Firefox (~27 s). WebKit's 1000 MB origin quota here is too
small for the 1.5B pack; the 0.5B model runs there (feasibility only). wllama's compat mode, which would download a
different build from a CDN on Safari, is switched off: no silent network use. Requirements are checked at start
(64-bit wasm memory, JSPI) and missing ones are reported in plain words.

## D-014 Desktop media over a token-protected loopback responder
WebKitGTK's GStreamer pipeline refuses media from custom URI schemes (measured: MEDIA_ERR_SRC_NOT_SUPPORTED with a
correct 206 response from `lbn://`), and Blob URLs would put whole videos in memory. `<video>` sources on desktop
come from a minimal HTTP/1.1 responder bound to 127.0.0.1 on a random port, with a 32-byte random token in every URL,
GET/HEAD only, and only files listed in active pack manifests. Every other pack file uses the `lbn://` scheme.
Trade-off: the app listens on loopback while running (upstream Lastbook had no listening port). Alternatives tried:
custom scheme (fails), Blob URL (memory). MSE streaming would avoid the port but needs a WebM cue parser.

## D-015 The desktop crate is its own Cargo workspace
It needs WebKitGTK headers that the libraries and CLI do not; keeping it separate lets `cargo test --workspace` run on
machines without them. It has its own Cargo.lock.

## D-016 Integrity errors use 'checksum' and 'size' everywhere
Defined once in `lbn_core::Error::code`, so native, browser and CLI report the same codes.

## D-017 Answer safety checks beyond citations
Small models loop and misread numbers (seen on the real imported library: "20 liters of bleach"). Answers are
checked after generation: repeated sentences are removed, generation stops once a sentence repeats three times, and
any number in a cited sentence that does not occur in the sources it cites is shown to the person as "check these
numbers yourself". A repetition penalty (1.1) is part of the model pack's generation settings.

## D-018 Nix: a flake for the dev shell and a reproducible CLI build (tested in a Nix container)
`flake.nix` (nixpkgs pinned to nixos-26.05 b349185, `flake.lock`) gives `nix develop` with every tool and the
WebKitGTK/GStreamer SDK (rustup inside honours rust-toolchain.toml) and `nix build .#lastbook-cli`, which builds the
CLI from the locked Cargo graph in the Nix sandbox and runs the core tests. Tested in `nixos/nix@sha256:e623d73a…`:
dev shell gave Rust 1.95.0, Node 22, WebKitGTK 2.54.2 and passed the core tests; the CLI built and ran. Nix is not a
runtime requirement. The desktop bundles are still built in the pinned Docker image, which the desktop tests exercise.

## D-019 Browser storage support declared from runs
Chromium, Firefox and Playwright WebKit pass the browser suite (OPFS + IndexedDB). WebKit gives no OPFS to
private/ephemeral windows; the app detects this at start and explains it. Real Safari (macOS/iOS) is untested.

## D-020 GPU inference stays optional; the AppImage bundles GStreamer
Vulkan on the test machine's integrated Radeon is faster (65 vs 53 tok/s) and uses less host RAM, but scored 6/7 on
the grounded check where CPU scores 7/7, so the packaged app keeps the CPU path and GPU is an opt-in build
(`LBN_VULKAN=1`) and switch (`LBN_GPU=1`). The AppImage crashed on video until `bundleMediaFramework` was enabled
(it ships its own WebKit without GStreamer otherwise); that needed `patchelf` in the build image.

## D-021 Publisher key held in 1Password; development key retired
Decided with the project owner on 2026-11-17: a new minisign key, "Lastbook Next publisher", signs packs, catalogs and
release checksums. Its secret exists only in 1Password (vault Eve); `build/with-signing-key.sh` fetches it into a
private temporary file for the duration of a signing command. `keys/trusted.json` trusts only this key; the
development key used during development was removed from the trust list and its secret deleted, so packs signed with
it now show as untrusted. A future maintainer without this secret publishes under their own key (docs/BUILDING.md).

## D-022 Windows cross-build from Linux, tested under Wine
With no Windows machine, the Windows release is cross-compiled with MinGW-w64 (`x86_64-pc-windows-gnu`) in
`build/docker/windows-cross.Dockerfile`: CLI, Tauri desktop + NSIS installer, and a static CPU runner (AVX2 baseline;
the upstream-style llvm-mingw script with all CPU variants and Vulkan remains `runner/build-windows.sh`). Wine runs the
Rust tests, the CLI and the runner; it cannot run the WebView2 window. Accepting Google's Android SDK licence to test
in an Android emulator was not done on the owner's behalf; mobile browsers stay untested.
