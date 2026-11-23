# ri-vgz.7: record-valued yield, partial item and suffix (2026-11-23)

Gap-analysis row C5, audit witness `partial-item::onBreak` (`r76-e4-audit.5tyuUI`, step 2). Commit `eee25c91` on branch `m5-ownership-cleanup/c5-ri-vgz.7`.

## What changed

A consuming `for` over an ordinary List literal of application records now lowers when every field of the record is an owned `string` and the record selects no custom disposer. The written records are staged and committed to the linked List (`CommitListRecordText`), the List moves into the owned cursor (`OpenOwnedListRecordText`), and each `Some` initializes every field of the binder (`AdvanceOwnedListRecordText`). The cursor's cleanup is `DisposeOwnedListIterRecordText`, whose element is `RecordTextDisposal`: the record, released field by field in reverse declaration order through the protected string winner. The graph closes these edges as `ListRecordTextCollection` (iterate, next, dispose iterator) and `RecordTextElementDisposal`.

Two supporting changes:

- The M4 body classifier counts a field read as an ordinary body shape when the yield is an owned application record. This changes no M4 cost: the fixture's receipt is 384,408 / 55,353 / 12 before and after.
- Inside such a loop, `dispose(record.field)` of an owned string field is `DisposeText` through the winner proved for the yield, with a `ListTextElementDisposal` edge.

The literal must be a bracket of record constructions with no exit (no `?`, return, break or continue in the source). Computed sources, `List<Entry>` parameters (no M4 receipt: `ResultUseBlocker::Storage`), grouped sources `(([...]))` (same M4 blocker), propagation in the body, records with a non-string field and records with a custom disposer stay `Incomplete`.

## Tests

`tests/m5/iteration_record.rs`, all passing in both installed load modes:

- `record_yield_cleans_the_partial_item_then_the_suffix`. For each positive, targeted and ordinary graph-root MIR are each walked as one ordered list of operations, on the `Some` path and on the `None` path. `onBreak`: stage both records, commit, open, advance, `DisposeText item.take`, drop `item.keep`, end `item`, retire the suffix, return. `whole` drops `item.keep` then `item.take` before the suffix. `exhausted` disposes `item.keep`, drops `item.take` and ends `item` before returning to the same advance. On `None` only the suffix is retired. The test also checks the selected providers, the backings (`Elements` for the committed List, `UnyieldedElements` for the cursor), the record element, and the graph dependencies.
- `string_on_break_keeps_its_cap`: the `List<string>` `onBreak` of `iteration_text.rs`.
- `linked_record_suffix_releases_whole_remaining_records_in_reverse`: the runtime `OwnedListIter::dispose_with` retires only the unyielded records, last first; the test's callback releases `keep` then `take`.

## Caps

Each cap below is pinned exact and one short on each axis. One short gives `ResourceLimit` with no MIR and no finding. The numbers are the same in both modes.

| Query | Outcome | Work / entries / depth |
|---|---|---|
| typed receipt | issued | 384,408 / 55,353 / 12 |
| graph | `SourceRejected` | 234,907 / 13,094 / 9 |
| `onBreak` | `Checked` | 5,605 / 1,356 / 7 |
| `grouped` (`(item)`, `(Entry{...})`, `dispose((item.take))`) | `Checked` | 5,611 / 1,356 / 8 |
| `scoped` | `Checked` | 5,607 / 1,356 / 8 |
| `wrapped` (inside `while`) | `Checked` | 6,065 / 1,604 / 9 |
| `whole` | `Checked` | 5,318 / 1,209 / 5 |
| `exhausted` | `Checked` | 5,624 / 1,362 / 7 |
| `reused`: `R0200` at the second `dispose(item.take)` | `SourceRejected` | 5,655 / 1,409 / 7 |
| `movedSource`: `R0200` at `dispose(a)` after the loop | `SourceRejected` | 5,351 / 1,224 / 5 |
| `dead` | `Checked`, parent cap | 2,690 / 426 / 2 |
| `pruned` | `Checked`, parent cap | 2,738 / 456 / 4 |
| `onPostfix` | `Incomplete` at the `for`, parent cap | 3,649 / 562 / 4 |
| `textLiteral` | `Incomplete` at the `for`, parent cap | 2,512 / 350 / 2 |
| `mixed` (`i32` field) | `Incomplete` at the `for` | 2,515 / 350 / 2 (parent 2,512 / 350 / 2) |
| `custom` (custom disposer) | `Incomplete` at the `for` | 2,894 / 419 / 2 (parent 2,522 / 358 / 2) |
| `iteration_text.rs` `onBreak` (strings) | `Checked` | 2,837 / 457 / 6 |

Parent caps were measured at `057c5005`, before the record lane, on the same fixture (log: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-c9hs/parent-caps-057c5005.log`). `mixed` and `custom` are not parent-cap siblings. They are new rows, and rejecting them needs the record's fields and its disposer selection, which no earlier fact supplies.

Row C5 cites 2,547 / 312 / 6 for the string `onBreak`. That is the ledger's figure at `7c145d3b`. At `057c5005`, before this ticket, the same function already measured 2,837 / 457 / 6, and this ticket leaves that unchanged.

No fixed cap moved: `m5::legacy_caps`, `m5::rfc_cases`, `m5::graph::inventory`, `m4_8_scoped_join_caps` and `m4_8_match_boolean_snapshots` pass unedited.

## Not established

- Rill execution. The MIR names the element callback; the runtime test shows only the linked cursor's reverse order.
- `partial-item::onPostfix`, or propagation of any kind with a record cursor.
- Records with non-string fields, nested records, or a custom disposer.
- Computed or named `List<Entry>` sources, and grouped List sources, which have no M4 receipt.
