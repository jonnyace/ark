# Status — release candidate 0.9.0-rc.1

Last updated 2026-11-17. Public repository github.com/jonnyace/lastbook-ark; branch `work/rc1`, PR #1 to `main`. Evidence for every "tested" line is in TESTING.md;
the acceptance matrix is in PLAN.md.

## Implemented and tested (on this Linux machine)
- **Pack format v1** with stable ids, versions, sizes, SHA-256, licences, compatibility and exact citation targets;
  document, encyclopedia, video, map and model packs install independently; minisign signatures; signed catalogs.
- **Install safety**: staged builds, full verification before atomic activation, previous build kept; tampered and
  unsafe packs rejected; downloads resume with Range, survive restarts and outages; repair; rollback; downgrade consent;
  crash cleanup; low-space refusal (native space check; browser quota, observed in WebKit).
- **Desktop app (Linux, Tauri)**: full journey online and with `--network none`: search, HTML sections, PDF pages,
  video seek/play/transcript, offline map, real local AI answer with citations that open, refusal when the library
  does not cover a question. `.deb` installed and exercised on a clean Debian 13 image.
- **Browser app (PWA)**: same journey in Chromium, Firefox and WebKit (Playwright builds); offline cold start in a new
  browser process with the server stopped (Chromium, Firefox, WebKit); OPFS + IndexedDB storage with eviction detection and repair;
  persistence request; private-window detection; personal data export.
- **Local AI**: Qwen2.5-1.5B-Instruct (Apache-2.0) on llama.cpp b12032, CPU; grounded answers 7/7 on the sample set;
  guards against loops and misread numbers; browser inference (wllama) in Chromium and Firefox, measured.
- **Personal data**: SQLite natively / IndexedDB in the browser, schema 2 with v1 migration and backups; newer
  schema refused untouched; export/import.
- **Legacy Lastbook import**: the real 7.2 GB library imported (234 items), unsupported ZIM/map reported, original
  unchanged.
- **CLI** `lastbook`: build, sign, zip, inspect, verify packs; install, search, resolve, ask, repair, rollback, export.
- **Accessibility**: automated WCAG 2 A/AA (axe) clean on 11 views; 360 px layout; skip link.
- **Release engineering**: `build/ci.sh`, signed SHA256SUMS, licence inventory, reproducible web app, packs, CLI,
  wasm, desktop binary and `.deb`; Nix flake (dev shell + CLI build) tested in a Nix container.
- **AppImage** (offline journey 14/14); **Firefox map drawing** with a display; **Vulkan GPU** inference on the AMD
  integrated GPU (optional; 6/7 grounded vs 7/7 on CPU, so CPU stays default).
- **Browser storage honesty**: quota shown, persistence requested only on request, message matches the browser's
  answer in all three engines; model licence verification recorded (docs/AI.md).

## Implemented but untested
- Windows desktop app and installer on real Windows: cross-built here; engine, CLI and AI runner tested under Wine
  (44 tests, 7/7 grounded answers), the WebView2 window not testable under Wine.
- macOS desktop build (Tauri config, runner script adapted from Lastbook, CI matrix): never built (needs Apple SDK).
- GPU acceleration on hardware other than the AMD integrated GPU tested here (see "tested" below).
- Manual screen-reader use (only automated checks).
- Real HTTPS hosting of packs and the PWA (tests use a loopback HTTP server).

## Deferred / externally blocked
- **Apple and Windows testing, code signing and notarization**: need Apple hardware + Developer ID, Windows machine +
  Authenticode certificate. Action: run `.github/workflows/desktop-other.yml` (or the scripts) on those platforms,
  then add signing with the publisher's certificates.
- Kiwix ZIM and Lastbook 1.x map import (reported as unsupported; originals kept).
- Native mobile apps (engine API and `lbn-core` are ready for them).
- Very large libraries: the search index is held in memory per pack (fine for the 2.8 GB imported library).
- **Publisher key**: decided 2026-11-17: a new "Lastbook Ark publisher" key; its secret is only in the maintainers' 1Password vault;
  the retired development key is no longer trusted. `build/public-readiness.sh` is the release gate.

- Mobile browsers: the owner decided (2026-11-17) not to accept the Android SDK licence for emulator tests;
  untested. Real Safari, macOS and the Windows desktop window need Apple/Windows hardware.

## Known limitations
- AI answers can be wrong; the app checks citations and numbers but cannot guarantee correctness.
- Browser AI is slow (20–40 s per answer on this machine) and needs a recent Chromium or Firefox; WebKit's quota here
  is too small for the recommended model; Safari untested.
- Firefox without a display (headless) has no WebGL; the app then shows a clear message instead of the map.
- The desktop app listens on 127.0.0.1 with a per-launch token while running, to stream video (D-014).
- Browser storage can be cleared by the browser or the person; keep pack files and export personal data.
