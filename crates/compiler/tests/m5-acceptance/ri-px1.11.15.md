# Second re-review ci-9qga fixes, part C18: records, and the OutputAgreement sweep (2026-11-24, city bead ci-d5gm)

Ticket `ri-px1.11.15` for spec `ri-2s0`, from the second focused re-review `ci-9qga` of `2e13bc6e` (Subagent D, with 2B-F6, 2A-F3, 2A-F1 and 2B-F1 as general claims that failed). Authority: chief rulings 21 to 25 (`notes/m5-ownership-cleanup/review/chief-rulings-21.md` to `-25.md`) and the planner's additions of 2026-11-24 18:10 and 19:11 AEDT. Base: `origin/implementation/m5-ownership-cleanup` at `f5f3419d` (C15b `b082e6ac`, C16 `0a8e2caf`, C17b `b0b8c13b` and C19b `f5f3419d` merged), branch `m5-ownership-cleanup/r4f-ri-px1.11.15`. Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-d5gm/`.

The brief names records only, with one test change allowed by ruling 24 (1). The brief overrides `common.md` here: this ticket edits `crates/compiler/tests/m5-acceptance.md` and other tickets' evidence files, by dated lines. No production change and no cap moved.

## Items

| Item | Finding | Outcome | Where |
|---|---|---|---|
| 1 | 2D-1 | C14's merge `2e13bc6e` and the green hosted checks (`45340209821`, `45340209901`) | dated line after the children table of "Final record after the re-review fix pass" in `m5-acceptance.md` |
| 2 | the pass | section "Fix pass after the second re-review" | `m5-acceptance.md` |
| 3 | 2D-2 | the rule-8 Check line (edited in place by C1, C8, C9, C7 and C11) and the O7 row (C2 and C13) say so by a dated note | `docs/agents/compiler-metering.md` rule 8 notes; `m5-status.md` O7 row |
| 4 | 2D-3 | dated "superseded" lines for "SP-6 fixed" and "not pinned" | `ri-px1.11.md`, `ri-px1.11.5.md`, `ri-px1.11.9.md` |
| 5 | general claims | dated corrections for 2B-F1, 2A-F1, 2A-F3 and 2B-F6; the 2C-F4 line was corrected by C17b already (`ri-px1.11.10.md` "Correction"), so none added | `ri-px1.11.8.md`, `ri-px1.11.9.md`, `ri-px1.11.11.md` |
| 6 | 2D-4 | `ri-j6fm` linked to `ri-qlh.4` (`relates-to`), parent `ri-qlh`; its description already carries the widened exception (C16) | beads |
| 7 | 2D-5, 2D-6 | 2D-5 answered by C15b item 5 (`m4_8_primitives`); 2D-6 (whether a limited `SelectionReport`'s matches are "facts") not answered by C15b and recorded open under `ri-qlh.9` | final record |
| 8 | ruling 24 (1) | the `OutputAgreement` sweep was missing and is added, red first | below |
| 9 | ruling 24 (2) | known-residue line in the rule-8 notes and in the final record | `compiler-metering.md`; `m5-acceptance.md` |

## Ruling 24 (1): the sweep of every public reader of `OutputAgreement`

`m4_8_limit_readers.rs` read no agreement. The only sweep was `m4_6_evidence::a_limited_generics_report_publishes_no_output_disagreement`, which runs `analyze_generics` at work caps only. The new test, `m4_8_limit_readers::a_limited_report_reads_an_output_disagreement_as_undecided_never_as_agreeing`, uses the same `inner` and `outer` source (the exact run publishes one `Disagrees`) and, at every work cap below the exact one and one short on entries and on depth, reads each public reader of the agreement: the typed report's `bodies()`, `analyze_generics(inventory, ..)` and `analyze_conversions(inventory, ..).bodies(..)`. For every ordinary call and value it asserts: no limited reader publishes `Disagrees`; a limited reader reads `Agrees` only where the exact run does (never more permissive than at the exact cap); the exact run publishes the disagreement through every reader; and at least one limited report reads that node as undecided (`NotDetermined`).

Red first: with the two `withhold_findings` of `OrdinaryCall` and `OrdinaryValue` (`scalars/generics/ordinary_calls.rs`) disabled (`if false && ..`), the test fails: `reader 0 publishes Disagrees at AnalysisLimits { max_work: 1883, max_entries: 727, max_depth: 3 }` (`red-r24-output-agreement.log`). The source was restored with `git checkout` (not a stash). Green with them: 7 tests of `m4_8_limit_readers` pass, and the 27 of `m4_6_evidence` (`green-r24.log`, `green-r24-final.log`). The test is not an `installed()` child test, so it ran in the parent process.

Not swept: sources other than this one for the agreement; a `Disagrees` that only a premise-carrying call reaches.

## Checks

Local targeted checks only; hosted CI runs the full gate after the merge.

| Check | Result | Log |
|---|---|---|
| `cargo test --locked --no-default-features -p compiler --test m4_8_limit_readers --test m4_6_evidence -- --test-threads=2` | 7 and 27 tests pass | `green-r24-final.log` |
| `cargo fmt --all -- --check` | clean | `fmt.log` |
| `cargo clippy --locked --no-default-features -p compiler --tests -- -D warnings` | clean | `clippy.log` |
| `mise run check:docs` | 2249 destinations, 0 errors | `check-docs.log` |
| `git diff --check` | clean | |
| `git status --short \| grep target/` | empty | |

## What it did not establish

- The full M5 binary and the full gate run in hosted CI, not here. Hosted CI is green at the base `f5f3419d` (`45353523226`, `45353523147`).
- Whether the ordinary `m5` filters still pass: this ticket changes no production code and one test file of a binary the m5 filters do not run.
