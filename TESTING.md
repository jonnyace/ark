# Testing

Test machine: Beelink SER8 (AMD Ryzen, 16 threads, 27 GiB RAM), Debian 13, headless, no GPU used. Rust 1.95.0,
Node 22.23.4, Docker 29.9. Desktop tests run in the pinned build container (Debian 13 + WebKitGTK 2.x) under Xvfb,
driven by WebDriver (tauri-driver + WebKitWebDriver). Browser tests use Playwright 1.65.1 builds of Chromium 159,
Firefox and WebKit 27.2. Every result below was produced by a run on this machine (dates 2026-11-16/17); nothing is
estimated unless it says so.

## Commands

| What | Command | Time |
|---|---|---|
| Fast checks (fmt, clippy -D warnings, Rust tests, wasm32, TS drift, svelte-check, web build) | `build/ci.sh` | ~20 s |
| Browser suite, 3 engines | `build/ci.sh --e2e` | ~4 min |
| Desktop build + journey (online install, then `--network none`) | `build/ci.sh --desktop` | ~6 min |
| Clean-machine install of the .deb, offline | `build/ci.sh --clean-install` | ~3 min |
| Real-model grounded answers | `build/ci.sh --ai` | ~1 min |
| Reproducibility | `build/repro-check.sh --desktop` | ~5 min |
| Runner speed/memory | `tools/runner_probe.py <runner> <model.gguf>` | |

## Results

### Unit and integration tests (Rust)
- `lbn-core`: 32 tests (manifest validation and future-format handling, unsafe paths, versions, index build/search/
  determinism/damage, citations parse/validate, retrieval refusal, prompt injection of template markup, citation
  checking, number verification and loop detection, personal schema v1→v2 / newer-schema refusal / export-import-merge,
  install plans, downgrade rule, catalogs, wire format).
- `lbn-native` unit: 4 (HTML extraction, personal DB upgrade with backup, newer-schema refusal leaves file byte-identical,
  export/import round trip).
- `lbn-native/tests/install.rs` (real HTTP server on loopback): 8 — signed/unsigned consent; tampered file, tampered
  signed manifest and untrusted signer rejected with nothing left in staging; traversal and unlisted .lbpack entries
  rejected; cut transfer resumes with `Range`; paused download survives engine restart and resumes; damaged update
  keeps v1 active/searchable/verified, good update + rollback + downgrade consent; damaged pack detected and repaired;
  crash-orphaned build folder removed. **All pass.** Two bugs were found and fixed by these tests (error codes,
  repair of an identical damaged build).

### Desktop (Linux, container, WebDriver) — `tests/e2e/desktop/journey.mjs`
- Install phase 7/7: signed catalog shown; docs, video, map, encyclopedia installed over HTTP; AI model installed from a
  `.lbpack` path; AI enabled after a measured fit check ("Fits: needs about 891 MB").
- Offline phase (`--network none`) 14/14: network confirmed absent; packs present after restart; search; HTML section
  highlighted with no scripts in the sandboxed frame; PDF page 2 with text; video opens at 27 s, plays (27.0 → 29.0 s);
  transcript (6 cues); map drawn (WebGL, software); **real AI answer** "…At elevations above 6,500 feet…, boil for three
  minutes [S2]"; its citation opens "Making water safe to drink"; out-of-library question refused without generation.
- Found and fixed on the way: WebKitGTK cannot play media or load caption tracks from a custom URI scheme (D-014);
  WebKit ignores `default` on late-added tracks; an ask event race in the UI.

### Clean-machine install — `tests/clean-install/run.sh`
Plain `debian:trixie-slim` (pinned digest), `apt-get install ./lastbook-next_0.9.0-rc.1_amd64.deb`: installs app,
CLI and runner with dependencies resolved by apt. Then install phase 7/7 and offline phase 14/14 on
`/usr/bin/lastbook-next`; a schema-1 personal database seeded beforehand ("older app") is migrated, its bookmark shown
with the new citation target, a `personal-…-v1.sqlite` backup kept, and `lastbook export` writes the data.

### Browser (PWA) — `tests/e2e/browser/` (Playwright)
| Spec | Chromium 159 | Firefox | WebKit 27.2 |
|---|---|---|---|
| journey (catalog, 4 packs, search, HTML, PDF, video seek/play/transcript, map, article, missing pack) | pass | pass (map: headless Firefox has no WebGL; the app's explicit message is asserted) | pass |
| offline cold start: new browser process, server stopped, offline (search, PDF, video, map) | pass | pass (map drawn in the headed run) | pass |
| tampered pack and corrupted transfer rejected | pass | pass | pass |
| interrupted download resumes from the exact byte (`bytes=300000-`); outage → failed job survives reload → resumes | pass | pass | pass |
| failed update keeps old version; rollback | pass | pass | pass |
| evicted OPFS files detected and repaired | pass | pass | pass |
| personal data schema 1 migrated with backup; export | pass | pass | pass |
| AI capability reported, no requests to other origins | pass | pass | pass |
| ephemeral (private) window explains it cannot store a library | — | — | pass |
| real AI answer (1.5B) with citation that opens | pass (37.6 s) | pass (27.4 s) | 1.5B refused up front: quota 1000 MB (low-space state) ; 0.5B feasibility answer 23 s |
| axe-core WCAG 2 A/AA on 11 views + 360 px layout + skip link | pass | — | — |
| storage card shows quota; persistence message matches `navigator.storage.persisted()` after the request; no page claims permanence | pass (headless denied persistence) | pass (denied) | pass (denied) |
| broken app update (new service worker lists an undeliverable file): old version keeps running, online and offline | pass | — | — |

### Local AI quality and speed
- Runner probe (CPU, 4 threads, ctx 4096): 1.5B load 1.0 s, first token 0.33 s, 52 tok/s, peak RSS 1841 MB;
  0.5B 0.54 s / 0.13 s / 111 tok/s / 620 MB.
- `tools/ai_eval.py` on the sample library, 1.5B: 2/7 with rules only in the system prompt → **7/7** after repeating the
  citation rule at the end of the prompt; still 7/7 after the repetition/number guards. 0.5B: 3/7, including a wrong
  bleach dose — not recommended.
- Real imported Lastbook library (234 items, 2.8 GB): answers cite real PDF pages and video moments. One answer looped
  and contained an invented quantity ("20 liters of bleach"); after the guards (D-017) the same question stops at 204
  tokens with the correct "2 drops per liter" cited and no unverified numbers.

### Legacy Lastbook import
Real 7.2 GB library (read-only): 234 items imported (148 PDF, 45 HTML, 41 video) in 9 s; 2 ZIM collections and the
1.x map reported unsupported; search returns exact PDF page and video timestamp citations; the original folder's file
list, sizes and modification times are unchanged (checked by hash of the listing before and after).

### Reproducibility — `build/repro-check.sh --desktop`
Identical across two independent builds: web app, the four content `.lbpack` files, `lastbook` CLI and `lbn-wasm`
(container), desktop binary, and the `.deb` after deterministic re-packing (before that it differed by timestamps
only). Not checked: AppImage, model packs.

### AppImage, Firefox map, persistence granted, GPU (added after the first full run)
- AppImage (with bundled GStreamer), `--network none`: offline journey 14/14. Before bundling GStreamer it crashed at
  the video step.
- Firefox headed under Xvfb (`LBN_HEADED=1 xvfb-run …`): WebGL available, the map draws; journey passes.
- Firefox with persistent storage allowed by preference: the page shows "Persistent storage granted" and still warns
  that clearing site data deletes the library.
- Vulkan GPU (AMD Radeon Phoenix, RADV, GPU passed into the container): see docs/AI.md — 65 tok/s, grounded 6/7.

### Nix (in `nixos/nix` container, pinned digest)
`nix develop`: Rust 1.95.0 (via rust-toolchain.toml), Node 22, WebKitGTK 2.54.2; core tests pass inside the shell.
`nix build .#lastbook-cli`: builds in the sandbox from Cargo.lock, runs core tests, binary runs.

### Release
`build/release.sh` → `dist/release/0.9.0-rc.1/` with `SHA256SUMS` and `SHA256SUMS.minisig`; `sha256sum -c` passes;
`lastbook verify-file SHA256SUMS` accepts the real file and rejects a modified one.

### Final full run — `build/ci.sh --all --gpu`, 2026-11-17, commit 12926c0
All steps passed (exit 0): fmt, clippy `-D warnings`, 44 Rust tests, wasm32 build, TS drift, svelte-check, web build;
browser suite 34 passed / 15 skipped by design (engine-specific tests) on Chromium, Firefox and WebKit, plus headed
Firefox under Xvfb drawing the map; desktop build + journey 7/7 and 14/14 offline; AppImage 14/14 offline;
clean-machine install 7/7 + 14/14 + migrated bookmark + CLI export; real-model grounded answers 7/7; Vulkan probe on
the Radeon GPU (warm: 69.9 tok/s, first token 0.16 s). An earlier full run failed once because Playwright profiles
were in this machine's RAM-backed /tmp (99 % full): the Firefox model download slowed and timed out. Profiles now go to
`.dev-cache/tmp` and are deleted after each test.

## Coverage limits (not tested)
- Windows and macOS builds and runtime (configurations exist; no machines).
- Real Safari (macOS/iOS) and mobile browsers (Playwright's WebKit build on Linux is not Safari).
- GPU inference on other GPUs (NVIDIA, Intel, Apple Metal); machines with little RAM (fit check measured only here).
- Manual screen-reader use (only automated axe checks).
- Very large packs (index is held in memory per pack; tested up to the 2.8 GB imported library).
- Hosting on a real HTTPS server (tests use a loopback HTTP server; HTTPS is used only for public downloads during
  development).
