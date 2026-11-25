#!/bin/sh
# PROTOTYPE (ci-ajns): build the prototype against a trimmed LLVM prefix, emit
# an object, link it with the host cc, run it and assert stdout and exit status.
# Usage: [FEATURES=no-libffi] [RUN_OUT=dir] [CARGO_TARGET_DIR=dir] run.sh <llvm-prefix>
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
cargo build --locked --release ${FEATURES:+--features "$FEATURES"}
built=$(date +%s)
echo "prototype_build_seconds=$((built - start))" | tee "$out/timing.txt"

proto=$CARGO_TARGET_DIR/release/llvm23-inkwell
"$proto" "$out/hello.o" | tee "$out/hello.ll"
cc "$out/hello.o" -o "$out/hello"

# With one argument: Hello Ada\n. With none: Hello\n. Both exit 0.
status=0
"$out/hello" Ada > "$out/stdout-ada.bin" || status=$?
bare_status=0
"$out/hello" > "$out/stdout-bare.bin" || bare_status=$?
printf 'Hello Ada\n' > "$out/expected-ada.bin"
printf 'Hello\n' > "$out/expected-bare.bin"

{
  echo "--- needed by the prototype binary"
  case $(uname -s) in
    Darwin) otool -L "$proto"; echo "--- needed by the emitted program"; otool -L "$out/hello" ;;
    *) ldd "$proto"; echo "--- needed by the emitted program"; ldd "$out/hello" ;;
  esac
} | tee "$out/deps.txt"

test "$status" -eq 0 || { echo "FAIL: exit status $status with an argument" >&2; exit 1; }
test "$bare_status" -eq 0 || { echo "FAIL: exit status $bare_status with no argument" >&2; exit 1; }
cmp "$out/stdout-ada.bin" "$out/expected-ada.bin" || { echo "FAIL: stdout differs with Ada" >&2; exit 1; }
cmp "$out/stdout-bare.bin" "$out/expected-bare.bin" || { echo "FAIL: stdout differs with no argument" >&2; exit 1; }
! grep -qi "libLLVM" "$out/deps.txt" || { echo "FAIL: shared LLVM in the loader dependencies" >&2; exit 1; }
echo "PASS: Ada gives exactly Hello Ada plus a newline, no argument gives Hello plus a newline, both exit 0"
