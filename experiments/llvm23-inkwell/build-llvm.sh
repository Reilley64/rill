#!/bin/sh
# PROTOTYPE (ci-ajns): build a trimmed static LLVM 23.1.5 from the pinned source
# archive. Usage: build-llvm.sh <workdir> <prefix>
# Native target backend only, static libraries and headers, llvm-config, no clang.
# LLVM_BUILD_TOOLS=ON only creates the install-llvm-config target; ninja builds
# just the three install targets named below, never the other tools.
set -eu

work=$1
prefix=$2
version=23.1.5
archive=llvm-project-$version.src.tar.xz
url=https://github.com/llvm/llvm-project/releases/download/llvmorg-$version/$archive
# Same pin as tools/llvm/common.py (ARTIFACTS["macos-x64"]).
sha256=fb50b1eac93ddeb3aa8dab4cfe324dff74f4a9c2bdee6160c9e36188f8d06a6e

case $(uname -m) in
  x86_64) target=X86 ;;
  arm64 | aarch64) target=AArch64 ;;
  *) echo "unsupported machine $(uname -m)" >&2; exit 1 ;;
esac

mkdir -p "$work" "$prefix"
cd "$work"

start=$(date +%s)
curl -fsSL --retry 3 -o "$archive" "$url"
if command -v sha256sum >/dev/null 2>&1; then
  echo "$sha256  $archive" | sha256sum -c -
else
  echo "$sha256  $archive" | shasum -a 256 -c -
fi
mkdir -p src
tar xf "$archive" -C src --strip-components=1 \
  "llvm-project-$version.src/llvm" "llvm-project-$version.src/cmake" "llvm-project-$version.src/third-party" "llvm-project-$version.src/libc"
rm "$archive"
fetched=$(date +%s)

flags="-G Ninja
-DCMAKE_BUILD_TYPE=Release
-DCMAKE_INSTALL_PREFIX=$prefix
-DLLVM_TARGETS_TO_BUILD=$target
-DLLVM_ENABLE_PROJECTS=
-DLLVM_ENABLE_RUNTIMES=
-DLLVM_BUILD_TOOLS=ON
-DLLVM_INCLUDE_TOOLS=ON
-DLLVM_INCLUDE_TESTS=OFF
-DLLVM_INCLUDE_EXAMPLES=OFF
-DLLVM_INCLUDE_BENCHMARKS=OFF
-DLLVM_INCLUDE_DOCS=OFF
-DLLVM_INCLUDE_UTILS=OFF
-DLLVM_ENABLE_BINDINGS=OFF
-DLLVM_ENABLE_ASSERTIONS=OFF
-DLLVM_ENABLE_ZLIB=OFF
-DLLVM_ENABLE_ZSTD=OFF
-DLLVM_ENABLE_LIBXML2=OFF
-DLLVM_ENABLE_LIBEDIT=OFF
-DLLVM_ENABLE_FFI=OFF
-DLLVM_BUILD_LLVM_DYLIB=OFF
-DLLVM_LINK_LLVM_DYLIB=OFF"

# shellcheck disable=SC2086
cmake -S src/llvm -B build $flags
configured=$(date +%s)

ninja -C build install-llvm-headers install-llvm-libraries install-llvm-config
built=$(date +%s)

{
  echo "llvm_version=$version"
  echo "target_backend=$target"
  echo "host=$(uname -sm)"
  echo "cpus=$(getconf _NPROCESSORS_ONLN)"
  echo "fetch_seconds=$((fetched - start))"
  echo "configure_seconds=$((configured - fetched))"
  echo "build_install_seconds=$((built - configured))"
  echo "total_seconds=$((built - start))"
  echo "installed_kib=$(du -sk "$prefix" | cut -f1)"
  echo "llvm_config_version=$("$prefix/bin/llvm-config" --version)"
  echo "llvm_config_system_libs=$("$prefix/bin/llvm-config" --link-static --system-libs)"
  echo "llvm_config_targets_built=$("$prefix/bin/llvm-config" --targets-built)"
  echo "configure_flags:"
  echo "$flags" | sed 's/^/  /'
  echo "bin:"
  ls "$prefix/bin" | sed 's/^/  /'
  echo "lib_files=$(ls "$prefix/lib" | wc -l)"
} > "$prefix/BUILDINFO.txt"
cat "$prefix/BUILDINFO.txt"
