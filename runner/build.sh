#!/usr/bin/env bash
# Build lastbook-llm, the pipe-only local AI runner, for Linux x86_64/aarch64.
# Adapted from Lastbook's packaging/llm/build_llm_linux.sh (MIT, Lastbook contributors).
#
# llama.cpp is pinned by tag AND commit. CPU is the supported path; set LBN_VULKAN=1 to add the
# optional Vulkan backend (needs libvulkan-dev, glslc, spirv-headers). Uses Ninja when present.
#
#   W=<work dir> runner/build.sh        default W=.dev-cache
# Output: $W/runner-bin/lastbook-llm plus the llama.cpp shared libraries beside it (RUNPATH $ORIGIN),
# LICENSE-llama.cpp.txt, NOTICE.txt and SHA256SUMS.txt.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(dirname "$HERE")"
TAG=b12032
COMMIT=dde879bc4f954b809892fd0d660515f42536394a
W="$(realpath -m "${W:-$ROOT/.dev-cache}")"
L="$W/llama.cpp"; B="$W/runner-build"; O="$W/runner-bin"
mkdir -p "$W" "$B" "$O"; rm -f "$O"/*
[ -d "$L/.git" ] || git -c advice.detachedHead=false clone -q --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp.git "$L"
[ "$(git -C "$L" rev-parse HEAD)" = "$COMMIT" ] || { echo "llama.cpp is not at $TAG ($COMMIT)" >&2; exit 1; }
GEN=(); command -v ninja >/dev/null && GEN=(-G Ninja)
VULKAN=OFF; [ "${LBN_VULKAN:-0}" = 1 ] && VULKAN=ON
MAP="-ffile-prefix-map=$L=llama.cpp -ffile-prefix-map=$HERE=lastbook-llm -ffile-prefix-map=$B=build"
cmake -S "$L" -B "$B" "${GEN[@]}" -DCMAKE_BUILD_TYPE=Release -DCMAKE_C_COMPILER=gcc -DCMAKE_CXX_COMPILER=g++ \
  -DBUILD_SHARED_LIBS=ON -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_NATIVE=OFF -DGGML_OPENMP=ON \
  -DGGML_VULKAN=$VULKAN -DGGML_RPC=OFF -DGGML_CCACHE=OFF \
  -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TOOLS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=OFF \
  -DLLAMA_CURL=OFF -DLLAMA_OPENSSL=OFF \
  -DCMAKE_SKIP_BUILD_RPATH=OFF -DCMAKE_BUILD_WITH_INSTALL_RPATH=ON -DCMAKE_INSTALL_RPATH='$ORIGIN' \
  -DCMAKE_SHARED_LINKER_FLAGS="-Wl,--enable-new-dtags" -DCMAKE_MODULE_LINKER_FLAGS="-Wl,--enable-new-dtags" \
  -DCMAKE_C_FLAGS="$MAP" -DCMAKE_CXX_FLAGS="$MAP" > "$W/runner-cmake.log"
CPUS="$(cmake --build "$B" --target help | grep -oE 'ggml-cpu-[a-z0-9_.]+' | sort -u | tr '\n' ' ')"
EXTRA=""; [ "$VULKAN" = ON ] && EXTRA="ggml-vulkan"
cmake --build "$B" -j "$(nproc)" --target llama $EXTRA $CPUS > "$W/runner-build.log"
g++ -std=c++17 -O3 -DNDEBUG $MAP \
  -I"$L/include" -I"$L/ggml/include" -I"$L/common" -I"$L/vendor" -I"$L/src" \
  "$HERE/lastbook_llm.cpp" "$L/common/fit.cpp" "$L/common/log.cpp" "$L/common/json.cpp" "$HERE/fit_unused_win.cpp" \
  -L"$B/bin" -lllama -lggml -lggml-base -pthread \
  -Wl,-rpath,'$ORIGIN' -Wl,--enable-new-dtags -Wl,--as-needed -s -o "$O/lastbook-llm"
for f in "$B"/bin/libllama.so "$B"/bin/libggml.so "$B"/bin/libggml-base.so; do
  cp -L "$f" "$O/$(objdump -p "$f" | sed -n 's/^ *SONAME *//p')"
done
cp "$B"/bin/libggml-cpu-*.so "$O/"; [ "$VULKAN" = ON ] && cp "$B"/bin/libggml-vulkan.so "$O/"
strip -s "$O"/*.so*
cp "$L/LICENSE" "$O/LICENSE-llama.cpp.txt"
cat > "$O/NOTICE.txt" <<NOTE
lastbook-llm: pipe-only local AI runner (runner/lastbook_llm.cpp, from Lastbook, MIT) linked to llama.cpp $TAG
($COMMIT), MIT licence (LICENSE-llama.cpp.txt). Vulkan backend: $VULKAN. Built with $(gcc --version | head -1).
Uses the system libstdc++, libgcc and libgomp (GCC runtime library exception); not shipped here.
NOTE
(cd "$O" && sha256sum lastbook-llm *.so* > SHA256SUMS.txt)
echo "built $O/lastbook-llm"
