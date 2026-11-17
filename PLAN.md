# Plan

Goal: a tested release candidate of an offline knowledge library (desktop + PWA) with optional local AI.
Architecture: DECISIONS.md. Progress: STATUS.md. Evidence: TESTING.md.

## Phases

| Phase | Content | Exit criterion |
|---|---|---|
| A. Inspect and record | Environment, upstream review, licences, ADR, this plan, risks | Docs committed |
| B. Foundation | Pack format + core crate + CLI; sample pack (text doc, PDF, video + transcript, map); native + browser import/verify/search/read/map/seek; early real AI (native + browser) and browser storage experiments | Each proven by an automated test or a recorded manual run |
| C. User journey | Open → choose/import packs → progress/verify → offline use → add model → grounded question → open citations; missing content, low space, unsupported device, interrupted download, recovery states | E2E tests on desktop (container) and PWA (Chromium) |
| D. Harden and package | Regression + offline tests, migration/rollback, legacy Lastbook import, artifacts + checksums, reproducibility check, docs | Release-candidate checklist below all green or explicitly blocked |

## Acceptance matrix (release candidate)

Status values: `todo`, `pass` (evidence in TESTING.md), `blocked` (reason in STATUS.md).

| # | Criterion | How it is checked | Status |
|---|---|---|---|
| 1 | Clean-machine install documented and exercised | HANDOFF.md steps run in a fresh container from artifacts only | todo |
| 2 | Full sample-library journey on Linux desktop | WebDriver E2E against the Tauri build under Xvfb | todo |
| 3 | PWA cold-starts offline after first install | Playwright: install, close, new browser process offline, app loads | todo |
| 4 | Search, documents, maps, video with network denied | Desktop container `--network none`; PWA offline context | todo |
| 5 | Desktop AI answers offline; citations open | E2E asks a question in `--network none`, opens each citation | todo |
| 6 | Browser AI capability and limits documented | Experiment results in TESTING.md and docs/AI.md | todo |
| 7 | Interrupted downloads resume or recover | Tests: cut connection, kill mid-download, restart, resume with Range | todo |
| 8 | Tampered packs rejected before activation | Tests: modified file, modified manifest, bad signature, unsafe paths | todo |
| 9 | Failed update leaves previous library usable | Test: failing update of an installed pack; old version still active and readable | todo |
| 10 | Personal data survives upgrade; exportable | Test: schema v1 data → v2 app migrates with backup; export/import round trip | todo |
| 11 | Automated checks pass | `build/ci.sh` | todo |
| 12 | Release artifacts, checksums, build instructions | `dist/` + SHA256SUMS + docs/BUILDING.md | todo |
| 13 | Known limitations and untested platforms stated | STATUS.md, README | todo |
| 14 | Windows/macOS build configurations where feasible | Config present; marked untested | todo |

## Highest risks (tackled first)

1. WebKitGTK media through a custom URI scheme (video seek in Tauri on Linux).
2. WebGL in WebKitGTK under Xvfb for MapLibre (fallback: software rendering / document as limit).
3. Browser inference speed/memory (wllama) and OPFS/storage quota behaviour.
4. Headless desktop E2E (tauri-driver + WebKitWebDriver in container).
5. Model quality at 1.5B for grounded, cited answers.
