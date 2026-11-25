#!/bin/sh
# Measures how LLVM IR's default C convention and the platform C ABI treat the aggregates of ADR 0036.
# Builds the same values in C and in LLVM IR, calls each across the boundary both ways, and prints the assembly of probe functions.
set -u
cd "$(dirname "$0")"
CLANG=${CLANG:-clang}
FLAGS=${ABI_FLAGS:-}

python3 gen.py || exit 1
echo "== toolchain"
uname -sm
"$CLANG" --version | head -n 1
echo "target: $("$CLANG" $FLAGS -dumpmachine)"

"$CLANG" $FLAGS -O0 -c cases.ll -o cases.o || exit 1
"$CLANG" $FLAGS -O0 -c harness.c -o harness.o || exit 1
"$CLANG" $FLAGS harness.o cases.o -o harness || exit 1

echo "== layout (LLVM struct { i8 tag, [n x payload] } against the rule: tag, then payload at its natural alignment)"
./harness layout A && echo "layout: MATCH" || echo "layout: MISMATCH"

echo "== calls across the boundary (A: C calls the IR function, B: IR calls the C function)"
./harness --list | while read -r name dirs; do
  for dir in $(echo "$dirs" | sed "s/./& /g"); do
    ./harness "$name" "$dir" > /dev/null 2>&1
    rc=$?
    if [ "$rc" -eq 0 ]; then result=match
    elif [ "$rc" -eq 1 ]; then result=MISMATCH
    elif [ "$rc" -ge 128 ]; then result="CRASH(signal $((rc - 128)))"
    else result="ERROR($rc)"; fi
    echo "case $name $dir: $result"
  done
done

"$CLANG" $FLAGS -O0 -S cases.ll -o cases.s
"$CLANG" $FLAGS -O0 -S harness.c -o harness.s
show() {
  echo "-- $1"
  awk -v n="$1" '$0 ~ "^_?" n ":" {p = 1} p {print} p && /cfi_endproc|\.size/ {exit}' "$2" | grep -v -E '^\s*(\.cfi|\.p2align|\.type|\.globl|\.size|\.section|\.build_version|;)' 
}
echo "== assembly: LLVM IR (first-class aggregate return, sret return, first-class argument) against C"
for t in S2 S3 E24 E56; do
  echo "=== $t"
  show "probe_ret_fc_$t" cases.s
  show "probe_ret_sret_$t" cases.s
  show "c_probe_ret_$t" harness.s
  show "probe_arg_fc_$t" cases.s
  show "c_probe_arg_$t" harness.s
done
