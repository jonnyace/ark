# Build and test environment for lastbook-ark on Linux.
# Holds everything the desktop (Tauri/WebKitGTK), the llama.cpp runner, the web app and
# the end-to-end tests need, so a maintainer only needs Docker (or the same packages on a
# Debian 13 machine) and not this project's original machines or accounts.
# Base image pinned by digest; toolchains pinned by version (see DECISIONS.md D-012).
FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2

ARG RUST_VERSION=1.95.0
ARG NODE_VERSION=22.23.4
ENV DEBIAN_FRONTEND=noninteractive LANG=C.UTF-8

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl xz-utils git file pkg-config build-essential cmake ninja-build clang \
      libwebkit2gtk-4.1-dev libgtk-3-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev \
      webkit2gtk-driver xvfb xauth dbus-x11 at-spi2-core \
      gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav gstreamer1.0-gl \
      ffmpeg fonts-dejavu-core sqlite3 zip unzip jq python3 procps imagemagick \
      libvulkan-dev glslc spirv-headers mesa-vulkan-drivers vulkan-tools patchelf \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:/opt/node/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} \
    && rustup target add wasm32-unknown-unknown && chmod -R a+rwX /opt/rustup /opt/cargo
RUN curl -sSfL https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz -o /tmp/node.tar.xz \
    && mkdir -p /opt/node && tar -xJf /tmp/node.tar.xz -C /opt/node --strip-components=1 && rm /tmp/node.tar.xz \
    && npm install -g pnpm@11.20.0 && chmod -R a+rwX /opt/node
WORKDIR /src
