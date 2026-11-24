#!/bin/bash
# usage: measure.sh <commit> <src-file> <names> <out> [raw]
set -e
W=/Users/reilley/.t3/projects/rill/worktrees/ci-paai
S=$W/target/scratch
export CARGO_TARGET_DIR=$W/target/scratch-target TMPDIR=$W/target/tmp CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
mkdir -p $TMPDIR
if [ ! -d $S ]; then git -C $W worktree add --detach $S $1 >/dev/null 2>&1; fi
cd $S
git checkout -q -f --detach $1
git clean -qfd crates/compiler/tests
cat /tmp/ci-paai/measure_snippet.rs >> crates/compiler/tests/m5.rs
export MEASURE_SRC=$2 MEASURE_NAMES=$3 MEASURE_OUT=$4
[ -n "$5" ] && export MEASURE_RAW=1
mise exec -- cargo test --locked --no-default-features -p compiler --test m5 -- zz_measure --exact --test-threads=1 > $4.log 2>&1 || { tail -30 $4.log; exit 1; }
git checkout -q -f -- crates/compiler/tests/m5.rs
cat $4
