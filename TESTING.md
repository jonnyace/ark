# Testing

Test machine: small-form-factor PC (AMD Ryzen with Radeon 780M-class iGPU, 16 threads, 27 GiB RAM), Debian 13, headless, no GPU used. Rust 1.95.0,
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
Plain `debian:trixie-slim` (pinned digest), `apt-get install ./ark-library_0.9.0-rc.1_amd64.deb`: installs app,
CLI and runner with dependencies resolved by apt. Then install phase 7/7 and offline phase 14/14 on
`/usr/bin/ark-library`; a schema-1 personal database seeded beforehand ("older app") is migrated, its bookmark shown
with the new citation target, a `personal-…-v1.sqlite` backup kept, and `ark-pack export` writes the data.

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
- Per CPU kernel (2026-11-18): `ai_eval.py` with the runner limited to one ggml CPU variant at a time, on this Ryzen
  (Zen 4). Before: x64, sandybridge, skylakex, icelake, cascadelake, cooperlake, zen4 7/7; **haswell and sse42 6/7**
  (haswell is what GitHub's AMD EPYC runners load), copying the prompt's format example "boil for one minute" into
  the high-elevation answer. Neutral example alone: 5–6/7 everywhere (the model summarised the encyclopedia's
  boiling-point line instead). Neutral example + one retry restricted to sources that state the asked-for amount:
  **7/7 on all nine variants**; the retry fired only for that question (about 4 s extra).
- Real imported Lastbook library (234 items, 2.8 GB): answers cite real PDF pages and video moments. One answer looped
  and contained an invented quantity ("20 liters of bleach"); after the guards (D-017) the same question stops at 204
  tokens with the correct "2 drops per liter" cited and no unverified numbers.

### Legacy Lastbook import
Real 7.2 GB library (read-only): 234 items imported (148 PDF, 45 HTML, 41 video) in 9 s; 2 ZIM collections and the
1.x map reported unsupported; search returns exact PDF page and video timestamp citations; the original folder's file
list, sizes and modification times are unchanged (checked by hash of the listing before and after).

### Reproducibility — `build/repro-check.sh --desktop`
Identical across two independent builds: web app, the four content `.lbpack` files, `ark-pack` CLI and `lbn-wasm`
(container), desktop binary, and the `.deb` after deterministic re-packing (before that it differed by timestamps
only). Not checked: AppImage, model packs.

### AppImage, Firefox map, persistence granted, GPU (added after the first full run)
- AppImage (with bundled GStreamer), `--network none`: offline journey 14/14. Before bundling GStreamer it crashed at
  the video step.
- Firefox headed under Xvfb (`LBN_HEADED=1 xvfb-run …`): WebGL available, the map draws; journey passes.
- Firefox with persistent storage allowed by preference: the page shows "Persistent storage granted" and still warns
  that clearing site data deletes the library.
- Vulkan GPU (AMD Radeon Phoenix, RADV, GPU passed into the container): see docs/AI.md — 65 tok/s, grounded 6/7.

### Arch Linux (Omarchy's base) — `packaging/arch/test.sh`, `tests/arch/run.sh`, `tests/arch/wayland.sh`
- Native package `ark-library` built from source by `makepkg` in a clean `archlinux:latest` container (Arch's Rust,
  WebKitGTK, GStreamer; no Python), `check()` runs the core and native tests, installed with `pacman -U` (which pulls in
  `gst-plugin-dav1d` and `gst-libav`).
- Installed package: install journey 7/7 (signed catalog, 4 packs over HTTP, model from a file, AI fit check), then
  offline journey 14/14 with `--network none` (including a real local AI answer with citations), then real imported
  library videos: H.264 1280×648 and AV1 1280×720 decode and play.
- AppImage on Arch: offline journey 14/14 and the same AV1/H.264 playback (it bundles its own decoders).
- Pure Wayland (no X server, `GDK_BACKEND=wayland`, headless sway): AppImage offline journey 14/14.
- Not tested: Hyprland itself. Hyprland 0.57 cannot run nested in a container (it needs `wl_compositor` and
  `xdg_wm_base` version 6, which headless sway and Weston do not offer), so Omarchy needs a hardware check.

### Windows build, cross-compiled from Linux and run under Wine (`build/windows-cross.sh`)
Wine is not Windows; these results show the Windows build compiles and its non-GUI parts work.
- `ark-pack.exe`, `ark-library.exe` (Tauri desktop) and an unsigned NSIS installer (14 MB) build with MinGW-w64.
- All 44 Rust tests compiled for `x86_64-pc-windows-gnu` pass under Wine, including the install-safety suite against a
  real HTTP server (renames, paths, resume, tamper, rollback).
- Windows CLI under Wine: installed the four signed sample packs, verify/search/resolve/export work.
- Windows llama.cpp runner (MinGW, CPU, AVX2 baseline, static): real inference under Wine, grounded check **7/7**, but
  slow under Wine (5.6 tok/s, 60–130 s per answer); native Windows speed unmeasured.
- Not tested: the desktop window (needs Microsoft WebView2, not available under Wine), the installer on Windows,
  anything on real Windows hardware.

### Nix (in `nixos/nix` container, pinned digest)
`nix develop`: Rust 1.95.0 (via rust-toolchain.toml), Node 22, WebKitGTK 2.54.2; core tests pass inside the shell.
`nix build .#ark-pack`: builds in the sandbox from Cargo.lock, runs core tests, binary runs.

### Release
`build/release.sh` → `dist/release/0.9.0-rc.1/` with `SHA256SUMS` and `SHA256SUMS.minisig`; `sha256sum -c` passes;
`ark-pack verify-file SHA256SUMS` accepts the real file and rejects a modified one.

### Final full run after the rename to Ark — 2026-11-17, commit a13640a + build-script fixes
`build/with-signing-key.sh … build/ci.sh --all` then `build/windows-cross.sh`, everything re-signed with the
"Ark publisher" key: fmt, clippy `-D warnings`, Rust tests, wasm32, TS drift, svelte-check, web build;
browser suite 36 passed on Chromium, Firefox and WebKit (offline cold start on all three) plus headed Firefox drawing
the map online and offline; desktop journey 7/7 and 14/14 offline; AppImage 14/14 offline; clean-machine install
7/7 + 14/14 + migrated bookmark + CLI export; real-model grounded answers 7/7; Windows cross-build with 44 Rust tests
passing under Wine. The first attempt stopped at copying build outputs because bundles from before the rename were
still in the build folder; the build scripts now clear old bundles and output folders first.

### AppImage, Firefox map, persistence granted, GPU (added after the first full run)
- AppImage (with bundled GStreamer), `--network none`: offline journey 14/14. Before bundling GStreamer it crashed at
  the video step.
- Firefox headed under Xvfb (`LBN_HEADED=1 xvfb-run …`): WebGL available, the map draws; journey passes.
- Firefox with persistent storage allowed by preference: the page shows "Persistent storage granted" and still warns
  that clearing site data deletes the library.
- Vulkan GPU (AMD Radeon Phoenix, RADV, GPU passed into the container): see docs/AI.md — 65 tok/s, grounded 6/7.

### Windows build, cross-compiled from Linux and run under Wine (`build/windows-cross.sh`)
Wine is not Windows; these results show the Windows build compiles and its non-GUI parts work.
- `ark-pack.exe`, `ark-library.exe` (Tauri desktop) and an unsigned NSIS installer (14 MB) build with MinGW-w64.
- All 44 Rust tests compiled for `x86_64-pc-windows-gnu` pass under Wine, including the install-safety suite against a
  real HTTP server (renames, paths, resume, tamper, rollback).
- Windows CLI under Wine: installed the four signed sample packs, verify/search/resolve/export work.
- Windows llama.cpp runner (MinGW, CPU, AVX2 baseline, static): real inference under Wine, grounded check **7/7**, but
  slow under Wine (5.6 tok/s, 60–130 s per answer); native Windows speed unmeasured.
- Not tested: the desktop window (needs Microsoft WebView2, not available under Wine), the installer on Windows,
  anything on real Windows hardware.

### Nix (in `nixos/nix` container, pinned digest)
`nix develop`: Rust 1.95.0 (via rust-toolchain.toml), Node 22, WebKitGTK 2.54.2; core tests pass inside the shell.
`nix build .#ark-pack`: builds in the sandbox from Cargo.lock, runs core tests, binary runs.

### Release
`build/release.sh` → `dist/release/0.9.0-rc.1/` with `SHA256SUMS` and `SHA256SUMS.minisig`; `sha256sum -c` passes;
`ark-pack verify-file SHA256SUMS` accepts the real file and rejects a modified one.

### Final full run — `build/with-signing-key.sh … build/ci.sh --all`, 2026-11-17, commit 27cadc8
After switching to the publisher key (packs, catalog, CLI, web app and desktop rebuilt and re-signed), all steps
passed (exit 0): fmt, clippy `-D warnings`, Rust tests, wasm32, TS drift, svelte-check, web build; browser suite
36 passed on Chromium, Firefox and WebKit (including offline cold start on all three), plus headed Firefox drawing the
map online and offline; desktop journey 7/7 and 14/14 offline ("signed by Ark publisher"); AppImage 14/14
offline; clean-machine install 7/7 + 14/14 + migrated bookmark + CLI export; real-model grounded answers 7/7.
The GPU probe ran in the previous full run (commit 12926c0). An earlier run failed once because Playwright profiles
were in a RAM-backed /tmp; they now go to `.dev-cache/tmp`.

## Coverage limits (not tested)
- Windows on real hardware (desktop window/WebView2, installer, native speed); everything on macOS (no Apple SDK or machine).
- Real Safari (macOS/iOS) and mobile browsers (Playwright's WebKit build on Linux is not Safari).
- GPU inference on other GPUs (NVIDIA, Intel, Apple Metal); machines with little RAM (fit check measured only here).
- Manual screen-reader use (only automated axe checks).
- Very large packs (index is held in memory per pack; tested up to the 2.8 GB imported library).
- Hosting on a real HTTPS server (tests use a loopback HTTP server; HTTPS is used only for public downloads during
  development).
