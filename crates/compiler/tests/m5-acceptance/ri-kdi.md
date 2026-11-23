# ri-kdi: a record's custom disposer reads its reference field (RFC 0007 O19), 2026-11-23

Ticket ri-kdi (Required row K1/O19, brought back into M5 by the owner's decision of 2026-11-23 19:18 AEDT), city bead ci-mw88. Branch `m5-ownership-cleanup/r-o19-ri-kdi`, parent `7002855e`.

## What changed

Commit `30e9fb30`:

- `aggregate` (`ownership/build.rs`) admits a shared primitive reference member (`&string`, `&bool`, `&i32`, ...) in a record with a custom disposer. Before, it stopped `Incomplete` at the binding. A record whose disposer can read such a member is recorded as an observing root, with no added work when there is none.
- `liveness` (`ownership/flow.rs`) treated every cleanup as unable to read a borrowed value. An observing root is now live up to its own scope cleanup, and its displacement reads the old value. The cleanup boundary already orders dependents before the sources they hold, so the holder's disposer runs before its source is dropped. A source that ends before the holder's cleanup, in a nested scope or by `dispose`, is `R0203` against a live holder. A source is never extended past its lexical scope.
- `m5-status.md`: row O19 is now `Checked`, and O7 is owned by ri-pfxi (below).

## Tests (all in `m5/rfc_cases.rs`, both installed load modes)

| Test | Result |
| --- | --- |
| `o19_cleanup_order_disposes_the_holder_before_its_source` (was `o19_cleanup_order_holder_is_incomplete_at_its_construction`) | `ordered` `Checked`, targeted and graph root; whole graph `Incomplete` (the standalone `implement release` root stays `Incomplete` at its declaration, as at the parent) |
| `o19_cleanup_sequence_disposes_the_holder_before_drop_source` | the whole targeted and graph-root action list of `ordered` (below), and the active disposer's own list, targeted (`Mir::disposer`) and in the graph (`active_disposers`, `Checked`) |
| `o19_pre_admission_caps_now_give_resource_limit` | the old targeted and graph caps give `ResourceLimit` |
| `o19_neighbours_reject_an_earlier_ending_source` | `scoped` and `early` `R0203`; `grouped` and `looped` `Checked`; `exclusive` and `deep` `Incomplete` at `holder`, at the parent's caps |

The `ordered` sequence, one `(kind, place)` per action: `copy seed -> holder.view`, `init holder`, `init source`, `borrow source -> Holder { view: &source }.view`, `init Holder { view: &source }`, `disposer holder` (the replaced value), `install Holder { view: &source } -> holder`, `disposer holder`, `drop source`, `end seed`. The active disposer: `copy value.view -> seen`, `end seen`, `defer value -> epilogue`, `end epilogue.view`, `end epilogue`.

Red at the parent: with only the test file at `7002855e` (in a `git worktree add` of the parent), all four O19 tests fail: `ordered` is `Incomplete`, the neighbours `Incomplete` at `holder` (`parent-7002855e-o19-red.log`).

## Caps

Targeted caps are work / entries / depth. Each new cap is exact; each one short on each axis gives `ResourceLimit` with no MIR and no finding (the harness asserts both).

| Root | Outcome | Cap |
| --- | --- | --- |
| O19 `ordered` | `Checked` | 6,121 / 711 / 5 |
| O19 whole graph | `Incomplete` | 189,320 / 4,794 / 5 |
| neighbours `scoped` | `R0203` at the inner block, related `&source` | 3,399 / 541 / 7 |
| neighbours `early` | `R0203` at `dispose(source)`, related `&source` | 3,384 / 522 / 5 |
| neighbours `grouped` | `Checked` | 6,555 / 897 / 7 |
| neighbours `looped` | `Checked` | 7,086 / 1,138 / 7 |
| neighbours `exclusive` (`{ view: &mut string }`) | `Incomplete` at `holder` | 2,444 / 185 / 1, the parent's cap |
| neighbours `deep` (`{ view: &&string }`) | `Incomplete` at `holder` | 2,440 / 182 / 1, the parent's cap |
| neighbours whole graph | `SourceRejected` | 201,497 / 6,580 / 7 |

Rule 1 and rule 2: `exclusive` and `deep`, the custom-disposer records the change still refuses, keep the caps measured at `7002855e`, in both modes (`parent-7002855e-neighbours.log`). At the parent the other four neighbours were `Incomplete` at `holder` (2,440 / 182 / 1, 2,440 / 182 / 1, 2,441 / 182 / 1, 2,446 / 186 / 1) and the neighbours graph `Incomplete` at 195,650 / 4,694 / 5. These were never published caps.

### Rule 11 note: the ticket's mandated flip, 2026-11-23

Named in ri-kdi before the diff (owner decision, chief's decision 3).

- O19 `ordered`, targeted: old 2,250 / 93 / 1 (`Incomplete` at `holder`), new 6,121 / 711 / 5 (`Checked`). Reason: the record's construction, its displacement through the custom disposer and the ordered scope cleanup are now lowered.
- O19 whole graph (D2 (b)): old 187,190 / 4,193 / 5, new 189,320 / 4,794 / 5, `Incomplete` both times. Per root: O19 without `ordered` measures 185,793 / 4,104 / 5 at the parent and on this branch, so no other root moved. `ordered`, with the `release` disposer frame it now reaches (one active disposer, none at the parent), adds 3,527 work and 690 entries, against 1,397 and 89 at the parent: +2,130 work, +601 entries. Depth stays 5, the depth of the i32 equality root and of `ordered` (`o19-graph-attribution.log`).
- `o19_pre_admission_caps_now_give_resource_limit` asserts both old caps give `ResourceLimit`. O19's `pre_equality_graph` (186,874 / 4,059 / 3) still does.
- The O19 caps live in `rfc_cases.rs`, not in `legacy-caps.tsv`, which has no rfc_cases rows. No `legacy-caps.tsv` row and no `m5::graph::inventory` row moved.

## Local checks (logs in `~/.t3/evidence/rill/m5-build-2026-11-22/ci-mw88/`)

| Check | Result | Log |
| --- | --- | --- |
| `--test m5 -- rfc_cases:: graph::inventory legacy_caps::` | 32 passed | `m5-rfc-inventory-legacy.log` |
| `--test m5 -- field_routes:: disposers:: records::` | 32 passed | `m5-field-disposers-records.log` |
| `--test m4_8_scoped_join_caps --test m4_8_match_boolean_snapshots` | 1 passed each | `m4_8-caps-snapshots.log` |
| `cargo fmt --all -- --check` | clean | `fmt.log` |
| `cargo clippy -p compiler --all-targets -- -D warnings` | clean | `clippy.log` |

The full M5 binary runs in hosted CI only.

## Not established

- O7 is not covered. It needs an exclusive reference member (`&mut i32`), a conflict between loans staged into one construction, and a store through a field dereference. A probe admitting `&mut` members passed `Holder { first: &mut value, second: &value }` with no `R0201` and stopped at `*holder.first += 1i32;`. Filed as ri-pfxi; the O7 row is unchanged.
- Only shared primitive references are admitted as fields of a custom-disposer record; a reference to a record, a nested reference and an exclusive reference stay `Incomplete`.
- The standalone `implement release` graph root stays `Incomplete` at its declaration; the disposer body is checked as the frame `ordered` reaches.
- These are compiler judgments over MIR. No Rill program ran.
