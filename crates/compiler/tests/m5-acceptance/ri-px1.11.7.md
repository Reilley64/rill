# Review ci-0mdr fixes, part C10: rule 8 "no facts", the ConversionReport pin and `Ok(None)` (2026-11-24, city bead ci-1lo8)

`ri-px1.11.7` under `ri-px1.11`, for spec `ri-2s0`. Base `origin/implementation/m5-ownership-cleanup` at `4ab0a82c` (holds C7, `b2524932`); branch `m5-ownership-cleanup/r2f-ri-px1.11.7`. The chief of staff accepted C7's `ConversionReport` change on 2026-11-24 at 16:02 AEDT (`review/chief-rulings-17.md` in the city notes) with three conditions; this ticket does them. Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-1lo8/`. No Rill program ran. No production change other than one doc comment; no cap moved.

## What changed

1. `docs/agents/compiler-metering.md`: a dated note under rule 8 holds "A ResourceLimit report publishes no finding, no MIR, no receipt and no facts." and the `Ok(None)` sentence below. The earlier rule-8 text and notes are untouched.
2. `crates/compiler/tests/support/limit.rs`: `conversion_limit_flaws` (so `assert_conversion_limit`) now also flags any declaration, type use or obligation through `type_facts()` and any header through `header_metadata(..)`. The rule-8 text already said the helper asserts this; it asserted findings only. New helper `conversion_headers` counts declarations whose header is `Some`.
3. `crates/compiler/tests/m4_7.rs`: `conversion_report_publishes_no_finding_list_at_resource_limit` (C7's test, extended, not duplicated).
4. `src/analysis/conversions.rs`: doc comment on `ConversionReport::header_metadata`. Nothing else.

## The test

Two sources, both with an `implement into<Target>` header and type facts; `Completion` has only `Incomplete` and `ResourceLimit`, so "complete" is `Incomplete`.

| Outcome | `completion()` | `type_facts()` | `header_metadata(..)` |
|---|---|---|---|
| exact cap (work, entries and depth of the unlimited run, statistics equal) | `Incomplete` | declarations and type uses non-empty | `Ok(Some(..))` for the implementation; some, not all, declarations have a header |
| one short, on work, entries and depth each | `ResourceLimit` | no declaration, use, obligation or finding | `Ok(None)` for every declaration |

Green: `green-focused.log` (and `m4_7.log`, 17 tests).

## Red

With C7's change reverted in `conversions.rs` (the `withheld` early return in `type_facts` and the `filter` in `header_metadata` removed; `c7-conversions.diff` is C7's own change), the test fails at the first one-short cap:

```
["publishes a type finding", "publishes type facts", "publishes a header metadata finding", "publishes a header"] at Statistics { work: 0, entries: 0, max_depth: 0 }
```

Log: `red-c7-reverted.log`. The file was restored afterwards (`conversions.rs.orig`).

## `Ok(None)` versus "no header"

`OrdinaryAnalysis::header_metadata` returns `Ok(self.headers.get(id))` after validating the id, so it already returns `Ok(None)` for a valid declaration with no header (headers exist for signature-bearing declarations). The test pins it: at the exact cap fewer declarations have a header than the graph holds. So `Ok(None)` is ambiguous at `ResourceLimit`. Added: the rule-8 sentence "`ConversionReport::header_metadata` returns `Ok(None)` both for a header that does not exist and at ResourceLimit; check `completion()` first." and the same in the doc comment.

## Callers

Found by renaming both `ConversionReport` accessors and building the whole workspace with `cargo check --workspace --all-targets --keep-going` (the name `type_facts` alone also matches `OrdinaryAnalysis`, `Inventory` and `TypedReport`, which this ticket does not touch). The only receivers of type `ConversionReport` are:

| Caller | Accessor | Checks completion first? |
|---|---|---|
| `crates/compiler/tests/m4_7.rs:1263` (the replay block) | `type_facts` | yes, `completion() == Incomplete` asserted just before |
| `crates/compiler/tests/support/limit.rs:159` `conversion_limit_flaws` | `type_facts` | yes, flags a non-`ResourceLimit` completion in the same function; reads either way on purpose |
| `crates/compiler/tests/support/limit.rs:176` `conversion_header_finding` | `header_metadata` | no itself; its callers `conversion_limit_flaws` and the `m4_7` replay check completion |

No production caller exists: `rg` over `crates/` finds no `ConversionReport::type_facts` or `header_metadata` outside tests. Production code reads the input's facts directly (`input.type_facts()` on `OrdinaryAnalysis`, in `conversions.rs`, `scalars/generics.rs` and `scalars/generics/evidence/conversions.rs`; `ConversionReport::bodies` calls `self.input()`), so no production caller can take a limited report's `Ok(None)` for "no header" or its empty facts for "none". Nothing to report to the chief.

## Checks

| Check | Result | Log |
|---|---|---|
| `--test m4_7` | 17 passed | `m4_7.log` |
| `--test m5 -- legacy_caps graph::inventory` | 4 passed | `m5-filtered.log` |
| `cargo fmt --all -- --check` | clean | `fmt.log` |
| `cargo clippy -p compiler --tests -- -D warnings` | clean | `clippy.log` |
| `mise run check:docs` | 0 errors | `docs.log` |
| `git diff --check` | clean | |

Caps: none pinned, none moved. Not established: whether other `ConversionReport` accessors (`family`, `bodies`' siblings) should withhold at `ResourceLimit`; this ticket did not look.
