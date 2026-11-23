# ri-fmsk (R-O5, review SP-F5): RFC 0007 O5, 2026-11-23

City beads `ci-5lzj` (blocked, first pass, WIP commit `524df9b2`) and `ci-donb` (this pass), branch `m5-ownership-cleanup/r-o5b-ri-fmsk`, resumed from the WIP branch after `ri-2s0.5` (`dde1cafb`) landed. The compiler judges O5 over MIR. It does not execute it: the expectation value (6) is not observed (M6).

## What changed

- **`scalars.rs` (from the first pass).** `I32Operand::Field`: an `i32` field read of a local record (`Access::Owned`, one `Field` projection, root a parameter or binding) is an operand of `+`. It is not an operand of `*` or of equality (`i32_comparison` excludes it). Screened from facts already paid for, before any charge (rule 1).
- **`ownership/build/i32_arithmetic.rs` (from the first pass).** The addition reads a field operand into a scratch slot through the ordinary expression path and ends the scratch slots, left before right. No new keyed lookup.
- **`ownership/build/i32_arithmetic.rs`, `add_through_exclusive_i32` (this pass).** `ri-2s0.5` admitted `*parameter += literal` only. It now also admits `*name += literal` through a local `&mut i32` that is written `&mut record.field` (`exclusive_field_reborrow`: an exclusive, owned, writable `i32` field of a local binding, one `Field` projection). The flow needed no change: `Action::InstallScalarThrough` already checks the install with `Access::Exclusive`, and the existing `Borrow` conflict rules already keep the loans of distinct fields apart.
- **How the local form is screened without moving other caps.** `declare_binding` marks the slot it has just paid for (`Local::exclusive_field_reborrow`) and counts the marks (`Build::exclusive_field_reborrows`). Neither adds an entry or a lookup. `add_through_exclusive_i32` rules a local descriptor out for free when the count is zero, so a body with no such binding (`viaLocal`, `localScalar`) charges exactly what it charged before. Only a body that has a field reborrow pays one `place` lookup to read the mark. Rule 3's path-keyed probes are untouched.

## Tests, all green

`cargo test --locked --no-default-features -p compiler --test m5 -- rfc_cases o05_field_arithmetic o15_arithmetic legacy_caps graph::inventory` (66 passed). Each `installed()` child ran: the failing assertions on the parent are inside the child (log `red-on-parent.log`).

- `rfc_cases::o05_disjoint_field_loans_check_through_local_field_descriptors`: the RFC's O5 module. Typed `Checked`; `separate` and the expectation `Checked` with MIR as targeted queries and as ordinary graph roots, both load modes, exact and one-short caps on each axis.
- `rfc_cases::o05_old_caps_give_resource_limit`: the caps at which O5 was `Incomplete` now give `ResourceLimit`: graph 188,006 / 4,283 / 5, `separate` 2,281 / 116 / 3, expectation 4,305 / 216 / 3.
- `rfc_cases::o05_field_loan_conflicts_reject_and_disjoint_forms_check` (`O5_LOANS`): `onlyLeft`, `otherRead` (the other field read while one loan is live) and `wholeReadAfter` (the whole record moved after both loans' last use) are `Checked`. R0201: `readLive` (a field read under its own loan), `sameField` (two live loans of one field), `wholeBeforeFirst` (a whole-record loan live at the first acquisition), `wholeBeforeSecond` (live at the second), `wholeBetween` (a whole-record loan acquired under a live field loan), `sharedWholeBeforeFirst` (a shared whole-record loan live at an acquisition) and `wholeReadWhileLive` (a whole-record move under a field loan). Sites and related sites are pinned.
- `o05_field_arithmetic::separate_borrows_both_fields_then_writes_through_each_descriptor`: the MIR order, borrow left, borrow right, read, add, install, read, add, install, copy left, copy right, add, with one descriptor per field.
- `o05_field_arithmetic::field_addition_checks_with_exact_and_one_short_caps` (first pass): `pair.left + pair.right` over a local record, 2,711 / 328 / 4.
- Siblings at parent caps: `o05_field_arithmetic::unsupported_field_arithmetic_keeps_its_parent_caps_and_sites` (nested, `i64`, product, literal, lent, and their grouped forms) and `o05_field_arithmetic::unsupported_field_reborrow_writes_keep_their_parent_caps_and_sites` (a scalar local, a nested field, grouped target, descriptor and operand, a computed operand, a write through a whole-record loan, a whole-record loan used by a call and by a field read). Both pin the module's graph at the parent caps and each function's caps and blocker, exact and one short, as targeted query and as graph root.
- Unchanged and still green: `o01_exclusive_i32_write_siblings_keep_their_screens`, including `viaLocal` at 2,008 / 101 / 2.

Red on the parent `f24d825b` (the base merged into the WIP branch), measured in a `git worktree add` with its own target directory: the three O5 flip tests fail, and the two sibling tests and `o05_old_caps_give_resource_limit` pass (`red-on-parent.log`).

## Caps

Mandated flip (rule 11, third case; the old caps assert `ResourceLimit`):

| Row | Old (parent, `Incomplete` at `pair.left + pair.right`) | New | Reason |
| --- | --- | --- | --- |
| O5 graph | 188,006 / 4,283 / 5 | 190,039 / 5,288 / 5 (`Incomplete`: other roots of the graph) | `separate` and the expectation lower |
| O5 `separate` | 2,281 / 116 / 3 | 4,314 / 1,121 / 4 `Checked` | field arithmetic and two writes through local field descriptors |
| O5 expectation | 4,305 / 216 / 3 | 6,338 / 1,221 / 4 `Checked` | the comparison now reaches the `separate` body's MIR |

The O5 typed cap is unchanged, 325,745 / 48,049 / 12. The whole-graph move is a D2 (b) re-pin with per-root attribution: the graph gains +2,033 work and +1,005 entries, exactly the gain of the `separate` root (2,281 / 116 to 4,314 / 1,121). The expectation root's own gain is the same +2,033 work, because its body call replays `separate`; the graph counts that work once. `m5::graph::inventory` did not move.

`O5_LOANS` is a new source, so it has no old row. At the parent (`f24d825b`) every function of it stopped `Incomplete` at a `*name += literal;` or `*name += literal` site; now:

| Source `O5_LOANS` | Parent | New |
| --- | --- | --- |
| typed | 347,333 / 51,302 / 12 | unchanged |
| graph | `Incomplete`, 205,449 / 5,349 / 5 | `SourceRejected`, 214,245 / 9,734 / 5 |
| `onlyLeft`, `otherRead`, `wholeReadAfter` | `Incomplete` | `Checked`: 3,802 / 869 / 4; 3,562 / 749 / 4; 5,493 / 1,596 / 4 |
| `readLive`, `sameField`, `wholeBeforeFirst`, `wholeBeforeSecond`, `wholeBetween`, `sharedWholeBeforeFirst`, `wholeReadWhileLive` | `Incomplete` | R0201: 3,248 / 598 / 4; 3,333 / 636 / 4; 3,176 / 561 / 4; 3,849 / 900 / 4; 3,182 / 563 / 4; 3,177 / 561 / 4; 3,505 / 615 / 4 |

Siblings kept at their parent caps, measured at `f24d825b` and pinned exact and one short: `WRITE_SIBLINGS` graph 201,135 / 5,074 / 5; `localScalar` 2,203 / 196 / 3; `nestedField` 2,931 / 314 / 3; `groupedTarget`, `groupedDescriptor` and `groupedOperand` 2,565 / 255 / 3; `computedOperand` 2,571 / 261 / 3; `throughWholeLoan` 2,568 / 260 / 3; `wholeLoanCalled` 2,587 / 264 / 3; `wholeLoanRead` 2,584 / 265 / 3. `SIBLINGS` (field arithmetic) keeps the caps of the first pass, graph 195,115 / 4,770 / 5. No other cap moved. Fixed caps left unchanged and passing: `m5::legacy_caps`, `m5::graph::inventory` and the rest of `graph::`, `o15_arithmetic`, `records`, `retained`, `field_routes`, `--test m4_8_scoped_join_caps`, `--test m4_8_match_boolean_snapshots`. The M4 17-source baseline (372,246 / 54,059 / 12) is not edited.

Rule 1: the local form is screened from written shape and paid-for facts; a body with no `&mut record.field` binding charges nothing new, which is why `viaLocal` and `localScalar` hold their caps.

## Not established

Execution of O5 (M6). A use of a whole-record reference other than moving it (`touch(whole)`, `whole.right`, `peek(&pair)`): M5 still stops `Incomplete` at them, so a live whole-record loan is shown through `let kept = whole;`. `-=`, `*=` and `=` through `&mut i32`, a field of a field (`&mut outer.inner.left`), a descriptor reached through a `&mut Record` parameter, and `i64` fields: unchanged, `Incomplete`. `m5-status.md` line 48 still records the first pass's expectation of H2; it is history. The full M5 binary and `mise run check` run in CI on the integration branch, not here.
