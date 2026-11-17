# Engine API, version 1

Every client talks to the engine with JSON **requests**, receives one **reply** per request, and listens for
**events**. The same protocol is served by the native engine (desktop over Tauri IPC, CLI in-process) and by the
browser engine (Web Worker). A future native mobile client links `lbn-native` (or `lbn-core` with its own adapters)
and speaks the same protocol. Source of truth: `crates/lbn-core/src/api.rs`; TypeScript types are generated into
`apps/web/src/lib/engine/gen/` by `build/gen-ts.sh` (CI fails if they drift).

```json
→ {"v": 1, "id": 7, "cmd": "search", "args": {"query": "boil water", "limit": 10, "packs": null}}
← {"v": 1, "id": 7, "ok": {"hits": [{"uri": "lbn:org.lastbook.sample.docs/water-guide#sec=boil", "title": "…", "snippet": "…", "score": 6.3, "chunk": 3, "citation": {…}}], "skipped": []}}
← {"v": 1, "id": 8, "err": {"code": "not_found", "message": "pack org.x is not installed on this device"}}
⇠ {"event": "job", "data": {"job": "job-…", "state": "downloading", "bytes_done": 1048576, "bytes_total": 872771, …}}
```

**Compatibility.** Within version 1, optional fields, commands and events may be added; clients ignore what they do
not know. Removing or changing the meaning of a field requires version 2. A request with another `v` is refused with
`incompatible`.

## Commands

| Command | Args | Reply |
|---|---|---|
| `hello` | `client` | `HelloReply`: api/engine version, platform, capabilities (AI runtime, storage evictable, storage unavailable reason) |
| `list_packs` | — | `PackInfo[]` (active build, health `ok`/`damaged`, other versions kept for rollback) |
| `list_resources` | `pack?` | `ResourceInfo[]` |
| `get_resource` | `pack, resource` | `ResourceInfo` |
| `read_catalog` | `url` | `CatalogReply` (signed by, expired, entries, installed versions) |
| `inspect_source` | `source` (`{type:"url",url}` / `{type:"local",path}`) | `SourceInfo` (incl. signer, size, licence, unsupported items of old libraries) |
| `install` | `source, allow_unsigned, allow_downgrade` | `JobProgress` (then `job` events) |
| `pause_job`, `resume_job`, `cancel_job` | `job` | — |
| `list_jobs` | — | `JobProgress[]` (jobs interrupted by a restart are `paused`) |
| `verify_pack` | `pack` | `VerifyReport` (re-hash every file) |
| `repair_pack` | `pack` | `{report, job}`: re-fetch from the pack's source |
| `remove_pack` | `pack, version?` | `{freed_bytes}` |
| `rollback_pack` | `pack` | the now-active `InstalledPack` |
| `search` | `query, limit, packs?` | `SearchReply` |
| `get_chunk` | `pack, chunk` | the indexed text chunk |
| `resolve_citation` | `uri` | `ResolvedCitation` (valid?, error, resource info) |
| `ai_status` | — | `AiStatus` (runtime, enabled, models with fit, loaded) |
| `load_model`, `unload_model` | `pack` / — | — |
| `ask` | `question, model?` | `{ask}` then `ask_status`, `ask_token`, `ask_done` or `ask_error` events |
| `cancel_ask` | `ask` | — |
| `get_personal` | — | `{data, schema}` |
| `save_bookmark`, `delete_bookmark`, `save_note`, `delete_note`, `add_history`, `save_settings` | see types | — |
| `export_personal` | — | export JSON text (format `lastbook-personal`, schema 2) |
| `import_personal` | `data` | merged data (a backup is taken first) |
| `storage_status` | — | used/free/quota bytes, persisted, evictable, location |
| `request_persistence` | — | browser: result of `navigator.storage.persist()`; native: `true` |

## Events

`job` (install progress and end state), `ask_status` (`searching`, `loading_model`, `generating`), `ask_token`,
`ask_done` (`AskResult`: outcome `answered` / `no_evidence` / `cancelled`, checked answer, sources, stats),
`ask_error`, `packs_changed`, `personal_changed`. Events can arrive before the reply to the request that caused them;
clients must buffer by id (see `apps/web/src/lib/views/Ask.svelte`).

## Error codes

`bad_request`, `incompatible`, `manifest`, `unsafe_path`, `checksum`, `size`, `signature`, `unsigned`
(needs consent), `downgrade` (needs consent), `not_found`, `nothing`, `busy`, `no_space`, `no_storage`, `network`,
`io`, `damaged`, `repair_source`, `ai_disabled`, `no_model`, `no_runtime`, `memory`, `too_long`, `runner`,
`cancelled`.

## Platform adapters

The UI's `Transport` (`apps/web/src/lib/engine/client.ts`) adds what the protocol cannot carry efficiently:
`fileUrl` / `mediaUrl` / `readFile` / `readRange` for pack files, file pickers, and saving exports.
Desktop: IPC + `lbn://` scheme + token-protected loopback media URLs. Browser: Web Worker + OPFS Blob URLs.
