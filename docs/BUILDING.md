# Building, testing and distributing Lastbook Next

Everything here runs on an ordinary Linux machine with Docker. No account, service or machine of the original
authors is needed. CI (`.github/workflows/`) only calls these same scripts.

## What you need

| Tool | Version | Why |
|---|---|---|
| Rust | 1.95.0 (`rust-toolchain.toml`, installed by rustup automatically) + `wasm32-unknown-unknown` | engine, CLI, wasm |
| Node.js + pnpm | Node 22.23.4, pnpm 11.20.0 (`package.json` `packageManager`) | web app, tests, pdf text extraction |
| Docker | any recent | the pinned Linux build/test image for the desktop app (`build/docker/Dockerfile`) |
| gcc, cmake, git | Debian 13 versions | the llama.cpp runner (`runner/build.sh`), or build it in the container |
| Python 3 | ≥ 3.11 | small tools (`tools/`) |

Native SDKs: Linux desktop needs WebKitGTK 4.1, GTK 3, GStreamer (plugins base + good), all inside the container.
macOS needs Xcode command-line tools (Metal); Windows needs WebView2 and the llvm-mingw toolchain listed at the top of
`runner/build-windows.sh`. **The macOS and Windows builds have not been run** (no such machines were available).

## Build everything (Linux)

```bash
source ~/.nvm/nvm.sh                                  # or any Node 22 on PATH
pnpm install --frozen-lockfile
npx --prefix tests/e2e playwright install chromium firefox webkit   # only for browser tests
runner/build.sh                                       # llama.cpp runner (CPU) -> .dev-cache/runner-bin
cargo build --release --locked -p lbn-cli             # the `lastbook` tool
build/gen-ts.sh && build/build-wasm.sh                # engine types + browser engine
(cd apps/web && npx vite build)                       # PWA -> apps/web/dist
LBN_SIGNING_KEY=<your key> build/make-sample-packs.sh # sample + model packs -> dist/packs (downloads models once)
build/desktop.sh                                      # desktop app in the container -> dist/desktop (.deb, AppImage)
LBN_SIGNING_KEY=<your key> build/release.sh           # dist/release/<version> + SHA256SUMS(.minisig)
```

## Test

```bash
build/ci.sh            # fast: fmt, clippy -D warnings, Rust tests (incl. install safety against a real HTTP server),
                       #       wasm build, TS type drift, svelte-check, web build
build/ci.sh --e2e      # browser suite: Chromium, Firefox, WebKit (journey, offline cold start, safety, personal data, AI)
build/ci.sh --desktop  # desktop build + WebDriver journey: install online, then everything with --network none
build/ci.sh --clean-install   # the .deb on a clean Debian 13 image, offline journey, personal-data upgrade
build/ci.sh --ai       # real-model grounded answers (tools/ai_eval.py)
build/repro-check.sh [--desktop]   # build twice, compare bytes
```

## Becoming the publisher (new maintainer)

The repository trusts one development key (`keys/trusted.json`). Its secret is not in the repository. To publish:

1. `lastbook key gen ~/.config/lastbook-next/keys/publisher --name "Your project"` (keep the `.key` secret; offline
   backup recommended).
2. Put the public key in `keys/trusted.json` (and remove keys you do not control). The apps embed this list at build
   time; people can add more keys at run time (`trusted-keys.json` in the data folder).
3. Sign packs (`lastbook pack build … --sign key`), catalogs (`lastbook pack sign-catalog`) and release checksums
   (`build/release.sh` with `LBN_SIGNING_KEY`).
4. Host `dist/packs/` on any static HTTPS server (catalog + pack folders + `.lbpack` files). Range requests make
   downloads resumable. The PWA can be hosted on the same server (`lastbook-next-web-<version>.tar.gz`); for
   multi-threaded browser AI the host should send `Cross-Origin-Opener-Policy: same-origin` and
   `Cross-Origin-Embedder-Policy: require-corp` (the service worker adds them to cached responses after the first
   visit).

## Releases and updates

- `SHA256SUMS` lists every artefact; `SHA256SUMS.minisig` signs it. People verify with `lastbook verify-file
  SHA256SUMS` (or `minisign -Vm SHA256SUMS -p publisher.pub`) and then `sha256sum -c SHA256SUMS`.
- The apps never update themselves. The desktop app is updated through the package manager (`apt install ./new.deb`);
  personal data migrates with a backup; packs are untouched. The PWA updates when its service worker finds a new build
  while online; a failed or partial update leaves the cached version running.
- Pack updates are ordinary installs: verified before activation, previous version kept for rollback. Catalog
  `expires` dates gate only *new* installs; installed packs never expire.
- Code signing (Authenticode, Apple Developer ID + notarization) is separate from building and needs the publisher's
  own certificates. Unsigned payload reproducibility is checked by `build/repro-check.sh` before signing.

## Reproducibility (measured, see TESTING.md)

Byte-identical across two independent builds: web app, all content packs (.lbpack), `lastbook` CLI and wasm module
(container), desktop binary, `.deb` (re-packed with fixed times). Not checked/not expected: AppImage (bundler pulls
`linuxdeploy` "continuous" builds at build time — an unpinned input), model packs (identical inputs, but large; same
builder as content packs).

## Layout

```
crates/lbn-core     engine rules (no I/O; native + wasm)        crates/lbn-native  desktop/CLI adapter
crates/lbn-cli      `lastbook` tool                              crates/lbn-wasm    browser bindings
apps/web            shared Svelte UI + browser engine (worker)   apps/desktop       Tauri shell
runner/             llama.cpp runner (from Lastbook)             packs/             sample + model pack sources
tests/              browser (Playwright), desktop (WebDriver), clean install    build/  scripts, container
```
