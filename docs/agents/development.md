# Toolchain development

1. Read the [development guide](../development.md) for source ownership and the current scaffold limits. Locate task definitions and tool pins in `mise.toml`; use `mise tasks ls` to discover entry points.
2. Use mise for tool selection and task execution, Cargo for Rust dependencies and uv for Python dependencies. Treat lock updates as intentional changes. Keep the compiler and IDE analysis independent of native LLVM installation.
3. Full test suites and full validation gates run only in CI. On a development machine, run targeted checks: the repository's normal commands, scoped to the files you implement, test or review. Targeted is a scope, not a list of tasks. Run the same tasks and tools that CI runs, but narrow what they cover:

   - One test by name: `mise exec -- cargo test --locked --no-default-features -p compiler <test_name>`.
   - One test file: `mise exec -- cargo test --locked --no-default-features -p compiler --test m1`. Add `-- <test_name>` to select a test in it.
   - One M5 test or module: `mise run test:m5 -- <filter>`, for example `mise run test:m5 -- scoped_jobs`; it runs the M5 binary without LLVM at two test threads, and a filter after `--` selects tests by name. The same run by hand: `mise exec -- cargo test --locked --no-default-features -p compiler --test m5 -- <filter> --test-threads=2`. `mise run test:m5:caps` replays the published fixed caps (the `legacy_caps` filter).
   - One crate: `-p <crate>` on `cargo test`, `cargo clippy` and `cargo fmt`, for example `mise exec -- cargo clippy --locked --no-default-features -p compiler --all-targets -- -D warnings` and `mise exec -- cargo fmt -p compiler -- --check`.
   - One Rust file's formatting: `mise exec -- rustfmt --check --edition 2024 <file>`.
   - One Python test class or case: from `tools/`, `mise exec -- uv run --project python --locked --no-sync python -m unittest test_infrastructure -k <name>`.
   - Documentation: `mise run check:docs` takes about a second and has no narrower scope. Run it for any change to Markdown, and on each branch you rewrite. The tracked pre-push hook runs it before every push once `mise run setup` has run.

   Run `mise run check:ci` when you change workflows, and select other product checks by the changed component; for compiler analysis that spends work, entries or depth, follow the [compiler metering standard](compiler-metering.md). Run heavy provisioning or experiments only through their explicit profiles after reviewing the prerequisites. After you push, read the hosted result for the branch. A pull request that conflicts with its base gets no hosted run: start one with `gh workflow run check.yml --ref <branch>`. A missing run is not a passing run.

4. Report each check's actual scope and result. Write gate logs to `~/.t3/evidence/rill/<bead-id>/`, not `/tmp`. A source that reproduces a review finding is a committed test, not evidence. Keep unavailable product checks nonzero until real implementations and suites replace them. Scaffold compilation, Python/model checks and an LLVM C-API smoke do not establish Rill execution or complete client acceptance.
5. Give each worker that runs Cargo in parallel its own `CARGO_TARGET_DIR` inside its worktree, so removing the worktree reclaims the build. Workers sharing one target directory block each other on its lock, and a full disk (2026-11-20) stopped every M5 worker at once.

6. Know these traps:

   - `crates/compiler/build.rs` builds the ownership archive only for `aarch64-apple-darwin`. A test that uses `env!("RILL_OWNERSHIP_ARCHIVE")` needs `#[cfg(all(target_arch = "aarch64", target_os = "macos"))]`, or it passes locally and fails on the other hosted platforms. Read `gh pr checks <n>` after every push.
   - Workflows start on push to `main` and `integration/**`, and on pull requests only. A branch with no pull request needs `gh workflow run <file> --ref <branch>`; a dispatched `native.yml` also runs `verify:release`.
   - A cancelled hosted job is no result: rerun it.
   - `gc bd create --body-file` needs an absolute path.
   - `gc bd ready` hides the children of a blocked parent.
   - A worker's summary is not evidence: check the source and the logs.

Completion requires passing applicable implemented checks and an explicit list of unavailable or unrun acceptance gates. Release/support claims require every supported profile, not a successful skip or a configured CI matrix. Preserve prototype sources and recorded experiment evidence unless the task expressly changes them.
