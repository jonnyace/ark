# Cross-compiles the Windows build from Linux and runs Windows test binaries under Wine.
# Wine is NOT Windows: results here show the Windows build compiles and its engine logic runs; they do not replace
# testing the desktop app (WebView2) on real Windows. Built on top of lastbook-ark-build (same pinned toolchains).
FROM lastbook-ark-build:dev
RUN dpkg --add-architecture i386 && apt-get update && apt-get install -y --no-install-recommends \
      mingw-w64 wine64 wine nsis \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-pc-windows-gnu && chmod -R a+rwX /opt/rustup /opt/cargo
