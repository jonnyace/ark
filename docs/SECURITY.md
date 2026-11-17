# Security and privacy model

## Assets and threats
- **The person's library and notes** must not be lost or silently altered.
- **Downloaded content is untrusted**: a pack, catalog or old library may be damaged or malicious (path traversal,
  oversized or extra files, lying sizes, scripts in HTML, prompt injection against the AI).
- **The app has privileges** (desktop: files, the AI runner). Content must never reach them.

## Controls
| Threat | Control | Tested by |
|---|---|---|
| Tampered or damaged pack | Size + SHA-256 for every file, checked before activation; signature over `pack.json` | `crates/lbn-native/tests/install.rs`, `tests/e2e/browser/safety.spec.ts` |
| Foreign/forged publisher | minisign signatures, trusted key list; invalid signature = reject; unsigned = explicit consent | same |
| Path traversal, links, extra or oversized entries | `lbn_core::path` rules, strict `.lbpack` reader (native + browser), copy limited to manifest size | install.rs (`unsafe_lbpack_entries_are_rejected`) |
| Broken library after failed update or crash | Build into new folder, verify, one atomic registry switch; old build kept; orphan cleanup | install.rs, safety.spec.ts |
| Scripts in library HTML | Sandboxed iframe without `allow-scripts`, CSP `default-src 'none'`, scripts/handlers/frames/forms stripped; `lbn://` serves HTML as inert bytes with `nosniff` and a sandbox CSP | desktop journey ("HTML viewer has no scripts") |
| Content calling desktop commands | Only the app's own page has IPC; capabilities allow only engine/events; no shell or fs plugin; library pages cannot run scripts | configuration (`capabilities/main.json`), journey |
| Prompt injection via retrieved text | Retrieved text marked plain (runner never parses its special tokens), template markers neutralised, rules say to ignore instructions in sources; AI has no tools or privileges | `rag::tests::retrieved_text_cannot_inject_markup` |
| Wrong AI answers | Citations validated; numbers checked against cited sources; loops cut; refusal when evidence is weak; "can be wrong" shown | `rag` tests, `tools/ai_eval.py` |
| Newer personal data opened by an older app | Refused unchanged; migrations make backups first | `personal_db` tests, clean-install test |

## Network use
The engine opens connections only when the person installs, updates or repairs a pack, or loads a catalog
(`crates/lbn-native/src/fetch.rs`; browser: the engine worker). HTTPS only (plain HTTP to loopback for testing).
No telemetry, no update checks, no fonts/tiles/CDNs at run time, no cloud AI. The desktop app listens on 127.0.0.1
(random port, 32-byte token per launch) only to stream videos to WebKitGTK's media pipeline (DECISIONS.md D-014).

## Known gaps
- The trusted key list ships a **development** key whose secret is on one machine; replace it before publishing.
- No sandboxing of the runner process beyond being a separate process with no network use; it reads one model file.
- Browser storage can be cleared by the browser or the person; the app detects missing files but cannot prevent it.
- Not externally audited.
