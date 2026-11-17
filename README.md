# Ark

Ark is a rewrite of [Lastbook](https://github.com/Immortal-Protocols/lastbook), the offline survival library: a new
Rust engine, Svelte interface, Tauri desktop app and offline browser app, using Lastbook's ideas, safety rules, pack
layout knowledge and AI runner (MIT, credited in [NOTICE.md](NOTICE.md)). It can import existing Lastbook libraries.

An open-source, offline knowledge library with optional local AI. Install packs of documents, PDFs, videos,
encyclopedias and maps once, then disconnect every network: browse, search, read, follow citations, watch and seek
videos, use the map, and (optionally) ask a local AI that answers only from your library with links to the exact page,
section or video moment.

**Status: release candidate 0.9.0-rc.1** — see [STATUS.md](STATUS.md) for what is tested and what is not.

Two ways to use it, one engine:
- **Desktop app** (Linux tested; Windows/macOS configured but untested): Tauri + the native Rust engine, local AI via
  llama.cpp on the CPU.
- **Browser app (PWA)**: install it once from a web address; it then starts and works offline from the browser's own
  storage. Local AI runs in the browser (WebAssembly) on supported browsers.

No account, server, subscription, activation, CDN or remote font is ever needed after installation.

## Install (Linux desktop)

```bash
sha256sum -c SHA256SUMS --ignore-missing          # after `ark-pack verify-file SHA256SUMS` or minisign
sudo apt install ./ark-library_0.9.0-rc.1_amd64.deb
ark-library                                      # or "Ark" in the application menu
```

Tested on Debian 13 (clean container, offline). It installs `/usr/bin/ark-library`, the `ark-pack` command-line
tool and the local AI runner. Your library and notes live in `~/.local/share/ark-library`. Then open **Packs**:
pick packs from a catalog address, or open `.lbpack` files you copied from a USB drive.

## Install (browser)

Open the hosted web app once while online (any static host; see docs/BUILDING.md), then choose packs. After that it
starts with no network. Browser storage can be cleared by the browser: press "Ask the browser to keep my library",
keep your `.lbpack` files, and export your notes (Saved → Export).

## Documentation

| | |
|---|---|
| [docs/PACK-FORMAT.md](docs/PACK-FORMAT.md) | pack format v1: manifest, hashes, signatures, citations, search index |
| [docs/ENGINE-API.md](docs/ENGINE-API.md) | the versioned engine commands and events (for new clients, e.g. mobile) |
| [docs/AI.md](docs/AI.md) | how answers are made and checked; model, runtime and measured performance; browser support |
| [docs/SECURITY.md](docs/SECURITY.md) | threats, controls, network use |
| [docs/BUILDING.md](docs/BUILDING.md) | build, test, sign, publish, reproducibility |
| [DECISIONS.md](DECISIONS.md) · [PLAN.md](PLAN.md) · [STATUS.md](STATUS.md) · [TESTING.md](TESTING.md) · [HANDOFF.md](HANDOFF.md) | project records |

## Licences

Code: MIT ([LICENSE](LICENSE)). Builds on [Lastbook](https://github.com/Immortal-Protocols/lastbook) (MIT) — see
[NOTICE.md](NOTICE.md). Third-party code licences: [docs/LICENSES.md](docs/LICENSES.md). Library content and model
weights keep their own licences, recorded in every pack.
