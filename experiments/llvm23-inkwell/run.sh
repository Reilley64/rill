#!/bin/sh
# PROTOTYPE (ci-ajns): build the prototype against a trimmed LLVM prefix, emit
# an object, link it with the host cc, run it and assert stdout and exit status.
# Usage: run.sh <llvm-prefix>
set -eu

prefix=$(cd "$1" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
out=${RUN_OUT:-$here/target/run}
mkdir -p "$out"
cd "$here"

export LLVM_SYS_231_PREFIX=$prefix
export LLVM_SYS_231_STRICT_VERSIONING=1
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$here/target}

start=$(date +%s)
cargo build --locked --release
built=$(date +%s)
echo "prototype_build_seconds=$((built - start))" | tee "$out/timing.txt"

proto=$CARGO_TARGET_DIR/release/llvm23-inkwell
"$proto" "$out/hello.o" | tee "$out/hello.ll"
cc "$out/hello.o" -o "$out/hello"

status=0
"$out/hello" > "$out/stdout.bin" || status=$?
printf 'hello world\n' > "$out/expected.bin"

{
  echo "--- needed by the prototype binary"
  case $(uname -s) in
    Darwin) otool -L "$proto"; echo "--- needed by the emitted program"; otool -L "$out/hello" ;;
    *) ldd "$proto"; echo "--- needed by the emitted program"; ldd "$out/hello" ;;
  esac
} | tee "$out/deps.txt"

test "$status" -eq 0 || { echo "FAIL: exit status $status" >&2; exit 1; }
cmp "$out/stdout.bin" "$out/expected.bin" || { echo "FAIL: stdout differs" >&2; exit 1; }
! grep -qi "libLLVM" "$out/deps.txt" || { echo "FAIL: shared LLVM in the loader dependencies" >&2; exit 1; }
echo "PASS: stdout is exactly hello world plus a newline, exit status 0"
