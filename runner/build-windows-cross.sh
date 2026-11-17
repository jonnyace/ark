#!/usr/bin/env bash
# Cross-compile lastbook-llm for Windows x86-64 from Linux with MinGW-w64 GCC (CPU only, AVX2 baseline, static).
# Runs in build/docker/windows-cross.Dockerfile. Tested by running real inference under Wine; not yet on real Windows.
# The upstream-style native Windows build (llvm-mingw, all CPU variants, Vulkan) is runner/build-windows.sh.
#   W=<work dir> runner/build-windows-cross.sh     -> $W/bin-win/lastbook-llm.exe
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"; ROOT="$(dirname "$HERE")"
TAG=b12032; COMMIT=dde879bc4f954b809892fd0d660515f42536394a
W="$(realpath -m "${W:-$ROOT/.dev-cache}")"; L="$W/llama.cpp"; B="$W/runner-build-win"; O="$W/bin-win"
mkdir -p "$B" "$O"; rm -f "$O"/*.exe
[ -d "$L/.git" ] || git -c advice.detachedHead=false clone -q --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp.git "$L"
[ "$(git -C "$L" rev-parse HEAD)" = "$COMMIT" ] || { echo "llama.cpp is not at $TAG" >&2; exit 1; }
CC=x86_64-w64-mingw32-gcc-posix; CXX=x86_64-w64-mingw32-g++-posix
cmake -S "$L" -B "$B" -G Ninja -DCMAKE_SYSTEM_NAME=Windows -DCMAKE_C_COMPILER=$CC -DCMAKE_CXX_COMPILER=$CXX \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DGGML_BACKEND_DL=OFF -DGGML_NATIVE=OFF -DGGML_AVX2=ON -DGGML_FMA=ON \
  -DGGML_F16C=ON -DGGML_OPENMP=OFF -DGGML_RPC=OFF -DGGML_CCACHE=OFF -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TOOLS=OFF \
  -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=OFF -DLLAMA_CURL=OFF -DLLAMA_OPENSSL=OFF > "$W/runner-win-cmake.log"
cmake --build "$B" -j "$(nproc)" --target llama > "$W/runner-win-build.log"
$CXX -std=c++17 -O3 -DNDEBUG -I"$L/include" -I"$L/ggml/include" -I"$L/common" -I"$L/vendor" -I"$L/src" \
  "$HERE/lastbook_llm.cpp" "$L/common/fit.cpp" "$L/common/log.cpp" "$L/common/json.cpp" "$HERE/fit_unused_win.cpp" \
  -Wl,--start-group $(find "$B" -name 'libllama.a' -o -name '*ggml*.a') -Wl,--end-group \
  -static -static-libgcc -static-libstdc++ -lpthread -s -o "$O/lastbook-llm.exe"
cp "$L/LICENSE" "$O/LICENSE-llama.cpp.txt"
echo "built $O/lastbook-llm.exe"
