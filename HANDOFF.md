# Handoff

## Where things are
- Repository `~/src/lastbook-next`, branch `work/rc1` (local only — nothing pushed, no PR, no release published).
- Upstream references, read-only and unchanged: `~/src/lastbook`, `~/src/lastbook-omarchy`. The real old library
  used for import tests (`~/.hermes/cache/scratch/lastbook-advisor/real-library/LastbookData`) was only read.
- Outside git (rebuildable): `.dev-cache/` (llama.cpp checkout, runner build, models, container caches, test data),
  `dist/` (packs, desktop artefacts, release).
- Development publisher secret key: `~/.config/lastbook-next/keys/sample-publisher.key` (mode 600, not in git).
- Build image: `lastbook-next-build:dev` from `build/docker/Dockerfile`.
- Test server port 18787 (and 18790–18792 in some specs). Port 8787 belongs to another service on this machine.

## Install the release candidate (clean Linux machine, Debian 13 tested)
```bash
cd dist/release/0.9.0-rc.1
tar xzf lastbook-cli-0.9.0-rc.1-linux-x86_64.tar.gz
lastbook-cli-0.9.0-rc.1-linux-x86_64/lastbook verify-file SHA256SUMS   # or: minisign -Vm SHA256SUMS -p keys/sample-publisher.pub
sha256sum -c SHA256SUMS
sudo apt install ./lastbook-next_0.9.0-rc.1_amd64.deb
lastbook-next                              # Packs → open packs/*.lbpack, or a catalog address
```
Browser: serve `lastbook-next-web-0.9.0-rc.1.tar.gz` (unpacked) and `packs/` from any static HTTPS host; open it once.

## Rebuild and re-test from scratch
```bash
source ~/.nvm/nvm.sh && pnpm install --frozen-lockfile
runner/build.sh && cargo build --release --locked -p lbn-cli
LBN_SIGNING_KEY=~/.config/lastbook-next/keys/sample-publisher.key build/make-sample-packs.sh
build/ci.sh --all                          # everything: fast, browser (3 engines), desktop, clean install, AI
LBN_SIGNING_KEY=~/.config/lastbook-next/keys/sample-publisher.key build/release.sh
build/repro-check.sh --desktop
```
Desktop by hand (needs a display): `dist/desktop/lastbook-next` (runner is found in `dist/desktop/runner/`).
Headless: `tests/e2e/desktop/run.sh <script.mjs> …` runs WebDriver scripts in the container under Xvfb.

## Next steps (in priority order)
1. Replace the development publisher key with the real publisher's key; re-sign packs and catalog.
2. Run `.github/workflows/desktop-other.yml` (or the scripts) on macOS and Windows; fix and record results.
3. Manual screen-reader pass (Orca on Linux, NVDA, VoiceOver).
4. Real Safari and mobile browser testing of the PWA.
5. ZIM support (libzim is GPL — choose an approach that respects that) and conversion of 1.x maps.
6. Sharded/mmap search index for very large packs; consider MSE video streaming to drop the loopback media port.
