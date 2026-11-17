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

Status values: `todo`, `pass` (evidence in TESTING.md), `blocked` (reason in STATUS.md). Updated 2026-11-17.

| # | Criterion | How it is checked | Status |
|---|---|---|---|
| 1 | Clean-machine install documented and exercised | `tests/clean-install/run.sh`: apt install of the .deb on plain Debian 13, journey online + offline | pass |
| 2 | Full sample-library journey on Linux desktop | WebDriver E2E against the Tauri build under Xvfb | pass (7/7 + 14/14) |
| 3 | PWA cold-starts offline after first install | Playwright: install, close, server stopped, new browser process offline | pass (Chromium, Firefox, WebKit) |
| 4 | Search, documents, maps, video with network denied | Desktop container `--network none`; PWA offline cold start | pass |
| 5 | Desktop AI answers offline; citations open | E2E asks a question in `--network none`, opens each citation | pass |
| 6 | Browser AI capability and limits documented | Real wllama answers in Chromium/Firefox; WebKit quota limit; docs/AI.md | pass |
| 7 | Interrupted downloads resume or recover | install.rs (Range resume, restart) + safety.spec (exact byte, outage + reload) | pass |
| 8 | Tampered packs rejected before activation | install.rs + safety.spec (file, manifest, signer, transfer corruption, unsafe zip) | pass |
| 9 | Failed update leaves previous library usable | install.rs + safety.spec (packs); app-update.spec (a broken PWA update keeps the old version) | pass |
| 10 | Personal data survives upgrade; exportable | personal_db tests, personal.spec, clean-install (installed app + CLI export) | pass |
| 11 | Automated checks pass | `build/ci.sh --all` | pass (see TESTING.md) |
| 12 | Release artifacts, checksums, build instructions | `build/release.sh`, signed SHA256SUMS, docs/BUILDING.md, repro check | pass |
| 13 | Known limitations and untested platforms stated | STATUS.md, TESTING.md coverage limits, docs/AI.md | pass |
| 14 | Windows/macOS build configurations where feasible | Tauri platform configs, runner scripts, CI matrix | configured, **untested** (no machines) |

## Highest risks (tackled first)

1. WebKitGTK media through a custom URI scheme (video seek in Tauri on Linux).
2. WebGL in WebKitGTK under Xvfb for MapLibre (fallback: software rendering / document as limit).
3. Browser inference speed/memory (wllama) and OPFS/storage quota behaviour.
4. Headless desktop E2E (tauri-driver + WebKitWebDriver in container).
5. Model quality at 1.5B for grounded, cited answers.

Outcomes: (1) custom scheme fails for media → loopback media responder (D-014), video seek tested; (2) WebGL works
under Xvfb with software rendering; (3) wllama works in Chromium/Firefox, measured; OPFS works except WebKit private
windows (detected); (4) WebDriver E2E works in the container; (5) 7/7 grounded answers after prompt work, plus guards.
