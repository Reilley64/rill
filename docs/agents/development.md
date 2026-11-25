# Toolchain development

1. Read the [development guide](../development.md) for source ownership and the current scaffold limits. Locate task definitions and tool pins in `mise.toml`; use `mise tasks ls` to discover entry points.
2. Use mise for tool selection and task execution, Cargo for Rust dependencies and uv for Python dependencies. Treat lock updates as intentional changes. Keep the compiler and IDE analysis independent of native LLVM installation.
3. Full test suites and full validation gates run only in CI. On a development machine, run targeted checks: the repository's normal commands, scoped to the files you implement, test or review. Targeted is a scope, not a list of tasks. Run the same tasks and tools that CI runs, but narrow what they cover:

   - One test by name: `mise exec -- cargo test --locked --no-default-features -p compiler <test_name>`.
   - One test file: `mise exec -- cargo test --locked --no-default-features -p compiler --test m1`. Add `-- <test_name>` to select a test in it.
   - One M5 test or module: `mise run test:m5 -- <filter>`, for example `mise run test:m5 -- scoped_jobs`; it runs the M5 binary without LLVM at two test threads, and a filter after `--` selects tests by name. The same run by hand: `mise exec -- cargo test --locked --no-default-features -p compiler --test m5 -- <filter> --test-threads=2`. `mise run test:m5:caps` replays the published fixed caps (the `legacy_caps` filter).
   - The MIR digest guard: `mise run test:m5 -- legacy_mir` compares the ownership output over the `legacy-caps` fixtures with `crates/compiler/tests/m5/legacy-mir.tsv` and names the fixture, mode and root that changed. A refactor ticket passes it unedited; a ticket that means to change the emitted MIR regenerates the file with `RILL_BLESS_MIR=1 mise run test:m5 -- legacy_mir` and commits it.
   - Measure source in both load modes: `mise run probe:m5 -- <sources file>` reads `=== name` sections (the `IMPORTS` block of `tests/m5.rs` is prepended) and prints typed and graph completion, each root's completion and blocker, findings, work/entries/depth and the Copy or Move of each whole-value use.
   - The same probe at another revision, in a detached worktree with its own `CARGO_TARGET_DIR`, removed on exit: `tools/probe_at.sh <rev> <sources file>` (the revision must contain the probe).
   - One crate: `-p <crate>` on `cargo test`, `cargo clippy` and `cargo fmt`, for example `mise exec -- cargo clippy --locked --no-default-features -p compiler --all-targets -- -D warnings` and `mise exec -- cargo fmt -p compiler -- --check`.
   - One Rust file's formatting: `mise exec -- rustfmt --check --edition 2024 <file>`.
   - One Python test class or case: from `tools/`, `mise exec -- uv run --project python --locked --no-sync python -m unittest test_infrastructure -k <name>`.
   - Documentation: `mise run check:docs` takes about a second and has no narrower scope. Run it for any change to Markdown, and on each branch you rewrite. The tracked pre-push hook runs it before every push once `mise run setup` has run.
   - Prototype status table: `mise run status:prototypes` regenerates `crates/compiler/tests/m5/prototype-status.tsv` through the public pipeline; commit it with the change that moved it, because `prototype_status::committed_table_matches_the_pipeline` fails when it differs.

   Run `mise run check:ci` when you change workflows, and select other product checks by the changed component; for compiler analysis that spends work, entries or depth, follow the [compiler metering standard](compiler-metering.md). Run heavy provisioning or experiments only through their explicit profiles after reviewing the prerequisites. After you push, read the hosted result for the branch. A pull request that conflicts with its base gets no hosted run: start one with `gh workflow run check.yml --ref <branch>`. A missing run is not a passing run.

4. Report each check's actual scope and result. Write gate logs to `~/.t3/evidence/rill/<bead-id>/`, not `/tmp`. A source that reproduces a review finding is a committed test, not evidence. Keep unavailable product checks nonzero until real implementations and suites replace them. Scaffold compilation, Python/model checks and an LLVM C-API smoke do not establish Rill execution or complete client acceptance.

   From M6 on the tests are the evidence. Write no per-ticket evidence file, no ledger note and no prose list of tests or caps. Keep only what a test cannot hold:

   - A cap that moves has its new pin and, beside it, an assertion that the old cap gives `ResourceLimit`; the old numbers, new numbers and reason go in the commit message ([metering rule 11](compiler-metering.md#11-cap-changes)).
   - What the ticket did not establish, an exception and its reason go in one comment on the ticket's bead headed `Not established`, and in the handoff. A later milestone finds them there. Nothing goes in a milestone record for this.
   - A milestone's row-to-test map is generated, not written by hand. A matrix is a Markdown table whose first cell is a row id such as `O1`, `K3` or `C12`. Name a test, or the file or directory that holds it, after its row: the id in lower case with its number padded to two digits, then an underscore (`o01_shifted_point_checks`, `k03_forms.rs`). `python tools/row_tests.py map` prints the map; `python tools/row_tests.py check` fails on a row with no test, and `mise run check:docs` runs it. A milestone adds its matrix to `MATRICES` in `tools/row_tests.py`. The unit tests are `tools/test_row_tests.py`.
5. Give each worker that runs Cargo in parallel its own `CARGO_TARGET_DIR` inside its worktree, so removing the worktree reclaims the build. Workers sharing one target directory block each other on its lock, and a full disk (2026-11-20) stopped every M5 worker at once.

6. Know these traps:

   - `crates/compiler/build.rs` builds the ownership archive for the target the compiler is built for (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin` or `x86_64-apple-darwin`) and fails on any other target. The installed analysis profile is that target only; there is no cross-compilation. A test names it with `env!("RILL_ANALYSIS_TARGET")`, never a literal triple, and a test that needs a target with no profile uses `x86_64-pc-windows-msvc`. No suite is gated by platform, and `mise run check:m5` fails a platform whose M5 binary lists no tests. Read `gh pr checks <n>` after every push.
   - Workflows start on push to `main` and on pull requests only. A branch with no pull request needs `gh workflow run <file> --ref <branch>`; a dispatched `native.yml` also runs `verify:release` and is green while `build` is a stub.
   - A cancelled hosted job is no result: rerun it.
   - `gc bd create --body-file` needs an absolute path.
   - `gc bd ready` hides the children of a blocked parent.

Completion requires passing applicable implemented checks and an explicit list of unavailable or unrun acceptance gates. Release/support claims require every supported profile, not a successful skip or a configured CI matrix. Preserve prototype sources and recorded experiment evidence unless the task expressly changes them.

## Comment check

[Comments in Rust code](comments.md) holds the policy. `mise run check:comments` enforces it, and `mise run check` and CI run it. It reads the `.rs` files under `crates/`, `runtime/` and `tools/` and refuses each comment added or changed since the merge base with `origin/main`, unless the comment fits one of the four forms in the policy. On a pull request the base is `origin/$GITHUB_BASE_REF`. A dispatched run on a branch with no pull request uses `origin/main`. The checkout needs the full history (`fetch-depth: 0`) to find it. `python tools/check_comments.py --base <commit>` picks another base.

`python tools/check_comments.py --tree` checks every comment in the tree, not only added ones. The check does not use that mode yet: the sweep (bead `ri-qlh.18.6`) turns it on in `mise.toml`.

The check reads a doc comment as a contract only when it sits on a `pub` item, on a variant of a `pub enum` or on a member of a `pub trait`, or when it is a `//!` on a crate root or a `pub mod`. It does not know whether a `pub` item is reachable from outside the crate.
