#!/usr/bin/env bash
set -euo pipefail

[ "$#" -eq 2 ] || { echo "usage: tools/probe_at.sh <rev> <sources file>" >&2; exit 2; }
rev=$1
sources=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
[ -f "$sources" ] || { echo "no sources file: $2" >&2; exit 2; }
git rev-parse --verify --quiet "$rev^{commit}" >/dev/null || { echo "no such revision: $rev" >&2; exit 2; }
git cat-file -e "$rev:crates/compiler/tests/m5/probe.rs" 2>/dev/null \
  || { echo "$rev has no probe (mise run probe:m5 landed after it)" >&2; exit 2; }

main=$(git worktree list --porcelain | sed -n '1s/^worktree //p')
mkdir -p "$main/worktrees"
dir=$(mktemp -d "$main/worktrees/probe-at.XXXXXX")
cleanup() {
  cd "$main"
  git worktree remove --force "$dir" 2>/dev/null || rm -rf "$dir"
  git worktree prune
}
trap cleanup EXIT

git worktree add --detach --quiet "$dir" "$rev"
cd "$dir"
CARGO_TARGET_DIR="$dir/target" mise run probe:m5 -- "$sources"
