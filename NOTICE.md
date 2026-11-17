# Notices and attribution

lastbook-ark is MIT-licensed (LICENSE). It reuses and builds on the work below. Library content and model weights
are not part of the source tree; their licences travel inside each pack (`pack.json` → `license`, `LICENSES/`).
The full dependency licence inventory is generated at release time (docs/LICENSES.md).

## Reused from Lastbook (MIT, Copyright (c) 2026 Lastbook contributors)

Source: https://github.com/Immortal-Protocols/lastbook (commit 83f2ca9) and the Arch/Omarchy packaging fork
https://github.com/jonnyace/lastbook-omarchy (upstream PR #60).

| What | Where here | How |
|---|---|---|
| `packaging/llm/lastbook_llm.cpp`, `fit_unused_win.cpp` — pipe-only llama.cpp runner | `runner/` | Copied; lastbook-ark adds an optional repetition penalty to `gen` |
| `packaging/llm/build_llm_linux.sh` | `runner/build.sh` | Adapted (CPU default, optional Vulkan, Ninja optional) |
| Library safety rules: stage beside, verify every SHA-256, switch atomically, unpack guards | `crates/lbn-core/src/{path,install}.rs`, `crates/lbn-native` | Re-implemented in Rust |
| Assistant rules: cite every claim, never invent keys, "Not found in the library", retrieved text is untrusted | `crates/lbn-core/src/rag.rs` | Rewritten for a small model |
| Library layout (`catalog.json`, `pack.json`, text with form-feed page breaks) | `crates/lbn-core/src/legacy.rs` | Read-only importer |

## Third-party components used by the build or the apps

| Component | Licence | Use |
|---|---|---|
| llama.cpp / ggml (tag b12032, dde879bc) | MIT | Native inference runner |
| nlohmann/json (vendored in llama.cpp) | MIT | Runner protocol |
| Protomaps basemaps-assets (commit 028c18f) fonts | SIL OFL 1.1 (Noto Sans) | Map labels in the sample map pack |
| Protomaps basemaps-assets sprites | MIT (derived from tangrams/icons) | Map icons in the sample map pack |
| Protomaps basemap tiles (build 20261115) | ODbL 1.0, © OpenStreetMap contributors | Sample map pack data |
| Qwen2.5-1.5B/0.5B-Instruct GGUF (Qwen team) | Apache-2.0 | Model packs |

Rust crates and npm packages keep their own licences; see docs/LICENSES.md.
