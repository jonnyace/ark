#!/bin/bash
# ADAPTED from Lastbook packaging/llm/build_llm_windows.sh (MIT, Lastbook contributors). UNTESTED in lastbook-ark:
# no Windows machine was available. Output goes to $W/bin-win (default W=.dev-cache) instead of derived/llm/.
# Builds lastbook-llm.exe for 64-bit Windows: llama.cpp at the pinned tag with Vulkan (any recent GPU) and every CPU variant
# as a fallback, linked to our runner with nothing that can open a socket (no RPC backend, no common library beyond the
# fitting and its log). One toolchain builds all of it, because fit.cpp passes C++ objects across llama.dll.
# Run in Git Bash on Windows:
#
#   TOOLS='C:\path\to\tools' bash runner/build-windows.sh
#
# TOOLS  A folder holding portable tools, none of them installed and none needing a licence to be accepted:
#          llvm-mingw-20261020-ucrt-x86_64\   github.com/mstorsjo/llvm-mingw (clang, lld, libc++, libomp; zip, sha256 fd5462b7…)
#          cmake-4.4.0-windows-x86_64\        github.com/Kitware/CMake (zip, sha256 156d70eb…)
#          ninja.exe                          github.com/ninja-build/ninja v1.13.2 (ninja-win.zip, sha256 07fc8261…)
#          msys\ucrt64\bin\glslc.exe          MSYS2 packages shaderc 2026.3-1, glslang 16.3.0-1, spirv-tools 1.4.350.1-1,
#                                             gcc-libs 16.2.0-3, unpacked. glslc only turns the shaders into SPIR-V during
#                                             the build; nothing of it ships.
#          vulkan-headers\                    github.com/KhronosGroup/Vulkan-Headers, tag vulkan-sdk-1.4.350.1 (8864cdc8)
#          spirv-headers\                     github.com/KhronosGroup/SPIRV-Headers, tag vulkan-sdk-1.4.350.1 (ad9184e7)
# W      Work folder (default derived/llm). llama.cpp is cloned into W\llama.cpp at the tag if it is not there.
# Output: W/bin-win/lastbook-llm.exe and the DLLs beside it.
#
# With Smart App Control on (the Alienware), Windows may refuse to run build-win\Release\vulkan-shaders-gen.exe, a helper
# the build makes and runs, and the build stops with "blocked by your organization's Device Guard policy". What worked there:
# build that one file again by hand with another -O level (vulkan-shaders-gen.cpp, the defines in its build.ninja), copy it over
# both copies in build-win, touch them, and run this script again. See derived/app-build/WINDOWS-LOCAL-AI.md.
set -euo pipefail
HERE="$(dirname "$(realpath "$0")")"
ROOT="$(dirname "$HERE")"
TAG=b12032       # the build the small-model evaluation ran on
COMMIT=dde879bc4f954b809892fd0d660515f42536394a
TOOLS="$(cygpath -m "${TOOLS:?Set TOOLS to the folder of portable build tools}")"
W="$(cygpath -m "${W:-$ROOT/.dev-cache}")"
L="$W/llama.cpp"
B="$W/build-win"
O="$W/bin-win"
HEREM="$(cygpath -m "$HERE")"
MINGW="$TOOLS/llvm-mingw-20261020-ucrt-x86_64"
export PATH="$(cygpath -u "$MINGW/bin"):$(cygpath -u "$TOOLS/cmake-4.4.0-windows-x86_64/bin"):$(cygpath -u "$TOOLS"):$(cygpath -u "$TOOLS/msys/ucrt64/bin"):$PATH"
mkdir -p "$W" "$B" "$O"
rm -f "$O"/*

[ -d "$L/.git" ] || git clone -q --depth 1 --branch "$TAG" https://github.com/ggml-org/llama.cpp.git "$L"
[ "$(git -C "$L" rev-parse HEAD)" = "$COMMIT" ] || { echo "llama.cpp is not at $TAG ($COMMIT)" >&2; exit 1; }

# The Vulkan loader comes with every GPU driver as C:\Windows\System32\vulkan-1.dll. Link against it by name; only the import
# library is made here, from the names that DLL exports.
VK="$B/vulkan-1"
mkdir -p "$VK"
( cd "$VK" && gendef - /c/Windows/System32/vulkan-1.dll > vulkan-1.def 2>/dev/null ) || true
if ! grep -q vkCreateInstance "$VK/vulkan-1.def" 2>/dev/null; then
  echo "could not read the exports of vulkan-1.dll (is a GPU driver with Vulkan installed?)" >&2; exit 1
fi
llvm-dlltool -m i386:x86-64 -d "$VK/vulkan-1.def" -D vulkan-1.dll -l "$VK/libvulkan-1.dll.a"

# SPIRV-Headers is headers only. ggml-vulkan looks for its CMake files but does not add its include folder (the Vulkan SDK
# would have it), so the folder goes into the C++ flags below.
cmake -S "$TOOLS/spirv-headers" -B "$B/spirv-headers" -G Ninja -DCMAKE_INSTALL_PREFIX="$B/spirv-headers/prefix" \
  -DSPIRV_HEADERS_ENABLE_TESTS=OFF > "$W/cmake-win.log"
cmake --install "$B/spirv-headers" >> "$W/cmake-win.log"

MAP="-ffile-prefix-map=$L=llama.cpp -ffile-prefix-map=$HEREM=lastbook-llm -ffile-prefix-map=$B=build"   # no folder of this machine inside
cmake -S "$L" -B "$B" -G Ninja -DCMAKE_BUILD_TYPE=Release -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++ \
  -DBUILD_SHARED_LIBS=ON -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_NATIVE=OFF -DGGML_OPENMP=ON \
  -DGGML_VULKAN=ON -DGGML_RPC=OFF -DGGML_CCACHE=OFF \
  -DLLAMA_BUILD_COMMON=OFF -DLLAMA_BUILD_TOOLS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_SERVER=OFF \
  -DLLAMA_CURL=OFF -DLLAMA_OPENSSL=OFF \
  -DVulkan_INCLUDE_DIR="$TOOLS/vulkan-headers/include" -DVulkan_LIBRARY="$VK/libvulkan-1.dll.a" \
  -DVulkan_GLSLC_EXECUTABLE="$TOOLS/msys/ucrt64/bin/glslc.exe" -DSPIRV-Headers_DIR="$B/spirv-headers/prefix/share/cmake/SPIRV-Headers" \
  -DCMAKE_C_FLAGS="$MAP" -DCMAKE_CXX_FLAGS="$MAP -isystem $B/spirv-headers/prefix/include" >> "$W/cmake-win.log"
# Only the libraries: the default target also builds llama.cpp's own app, which downloads. One library per CPU variant.
CPUS="$(cmake --build "$B" --target help | grep -oE '^ggml-cpu-[a-z0-9]+' | sort -u | tr '\n' ' ')"
cmake --build "$B" -j "$(nproc)" --target llama ggml-vulkan $CPUS > "$W/build-win.log"

# -O3 and no --gc-sections: on the test machine Smart App Control blocked some unsigned links of this same code and let others
# run, by file, with no pattern in the flags (see derived/app-build/WINDOWS-LOCAL-AI.md). Signing is the real answer.
clang++ -std=c++17 -O3 -DNDEBUG $MAP \
  -I"$L/include" -I"$L/ggml/include" -I"$L/common" -I"$L/vendor" -I"$L/src" \
  "$HERE/lastbook_llm.cpp" "$L/common/fit.cpp" "$L/common/log.cpp" "$L/common/json.cpp" "$HERE/fit_unused_win.cpp" \
  -L"$B/bin" -L"$B/src" -L"$B/ggml/src" -lllama -lggml -lggml-base \
  -s -o "$O/lastbook-llm.exe"

# The program, the llama.cpp libraries it loads, and the C++ runtime they share. The CPU variants are all kept: llama.cpp picks
# the best one this processor can run, from SSE4.2 machines up.
cp "$B"/bin/libllama.dll "$B"/bin/ggml.dll "$B"/bin/ggml-base.dll "$B"/bin/ggml-vulkan.dll "$B"/bin/ggml-cpu-*.dll "$O/"
cp "$MINGW"/x86_64-w64-mingw32/bin/{libc++.dll,libunwind.dll,libomp.dll} "$O/"
llvm-strip -s "$O"/*.dll
cp "$L/LICENSE" "$O/LICENSE-llama.cpp.txt"
cat "$MINGW/LICENSE.TXT" > "$O/LICENSE-llvm-mingw.txt" 2>/dev/null || true

# What each file imports. Every DLL it names must be beside it or part of Windows; vulkan-1.dll comes with the GPU driver.
bad=0
for f in "$O"/*.exe "$O"/*.dll; do
  dlls="$(llvm-objdump -p "$f" | sed -n 's/^ *DLL Name: //p' | tr 'A-Z' 'a-z' | sort -u)"
  echo "$(basename "$f"): $(echo $dlls)" >> "$W/imports-win.txt.new"
  # Nothing may reach a network: no Winsock, no HTTP, no name lookups.
  if echo "$dlls" | grep -Eq '^(ws2_32|wsock32|winhttp|wininet|urlmon|dnsapi|iphlpapi|mswsock|netapi32|rpcrt4|webio|httpapi)\.dll$'; then
    echo "$(basename "$f") imports a network library: $(echo $dlls)" >&2; bad=1
  fi
  for d in $dlls; do
    case "$d" in
      api-ms-win-*|kernel32.dll|user32.dll|advapi32.dll|shell32.dll|ole32.dll|ucrtbase.dll|msvcrt.dll|bcrypt.dll|ntdll.dll|vulkan-1.dll|psapi.dll|dbghelp.dll|shlwapi.dll) ;;
      *) [ -f "$O/$d" ] || { echo "$(basename "$f") needs $d, which is not beside it" >&2; bad=1; } ;;
    esac
  done
done
mv "$W/imports-win.txt.new" "$O/imports.txt"
if grep -aEiq 'C:[/\\]Users|app-test' "$O"/*.exe "$O"/*.dll; then echo "a file names a folder of this machine" >&2; bad=1; fi
[ "$bad" = 0 ] || exit 1
# Web addresses inside are text in llama.cpp's log and error messages; with no network calls they go nowhere.
cat "$O"/*.exe "$O"/*.dll | grep -aEio 'https?://[^ "<>]+' | sort -u > "$O/addresses-in-messages.txt" || true
( cd "$O" && sha256sum *.exe *.dll > SHA256SUMS.txt )
ls -l "$O"
