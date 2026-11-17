# Clean Arch Linux with what makepkg needs to build ark-library, plus test-only tools (Xvfb, WebKitWebDriver from
# webkitgtk-6.0, dbus). The package's runtime dependencies are NOT preinstalled: pacman -U must pull them in.
FROM archlinux:latest
RUN pacman -Syu --noconfirm --needed base-devel git rust rust-wasm nodejs pnpm cmake ninja pkgconf \
      webkit2gtk-4.1 xorg-server-xvfb xorg-xauth dbus webkitgtk-6.0 curl sudo && pacman -Scc --noconfirm \
    && useradd -m builder
