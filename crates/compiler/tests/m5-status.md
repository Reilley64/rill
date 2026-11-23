# M5 status of the RFC 0007 cases

Written 2026-11-22 for ri-px1.2 (H2). Each row is one case from [RFC 0007](../../../docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md), O1 to O24, and the test in `m5/rfc_cases.rs` that pins its outcome today. When a ticket changes an outcome it edits the test expectation and the row in the same commit. The [acceptance ledger](m5-acceptance.md) stays the narrative record.

All outcomes are compiler judgments over MIR in both installed load modes (identical in each). No Rill program ran.

## How to read the outcome column

- **Checked**: the targeted query and the ordinary graph root both publish MIR.
- **R0xxx at `site`**: `SourceRejected` with that code at that site, for the targeted query and the graph root.
- **Incomplete at `site`**: `Incomplete` with that blocker site, no MIR, no finding.
- **Typed: Incomplete (reason)**: M4 issues no typed receipt, so M5 never runs. The reason is the blocker the typed report names in the module (`pending Storage` is a Result-use blocker, `obligation FamilySelection` an unselected family).
- Each pinned result is also replayed at its exact work, entries and depth, and one short on each axis. One short is always `ResourceLimit` with no MIR and no finding.

Module source: **RFC** means the complete module in the RFC, verbatim after the standard imports of `m5.rs`. **Approved source** means the O16 module the owner approved, already in `m5/scoped_join.rs`. **Authored** means the RFC states the case in prose only, so the module was written for that prose. An authored module cannot match the RFC's text, so a different outcome there does not contradict the RFC.

## Matrix

| Case | Test | Module | Outcome today | Owning bead |
| --- | --- | --- | --- | --- |
| O1 | `o01_shifted_point_waits_on_exclusive_deref_storage` | RFC | Typed: Incomplete (`pending Storage` at `*value += 1i32;`) | ri-1up |
| O2 | `o02_remembered_slot_waits_on_descriptor_slot_storage` | RFC | Typed: Incomplete (`pending Storage` at `*slot = source;` and `&mut view`) | ri-1up |
| O3 | `o03_identity_forwards_a_reference_but_its_roots_stay_incomplete` | RFC | Typed: Checked. `identity` Incomplete at `value: T` (graph root: at the whole function). `carried` Incomplete at `identity(&owner)`. Expectation: its comparison lowers (ri-785.9); it is Incomplete at `identity(&owner)` in the `carried` body it calls | ri-55r (rigid generic root) |
| O4 | `o04_invalid_move_is_r0200` | RFC | R0200 at `first` (related: the move into `second`) | none: matches RFC |
| O5 | `o05_disjoint_field_loans_check_and_field_arithmetic_blocks` | RFC | Typed: Checked. `separate` Incomplete at `pair.left + pair.right`. Expectation: its comparison lowers (ri-785.9); it is Incomplete at `pair.left + pair.right` in the `separate` body it calls | field arithmetic: none yet |
| O6 | `o06_reborrowing_calls_wait_on_exclusive_deref_storage` | RFC | Typed: Incomplete (`pending Storage` at `*value += 1i32;`) | ri-1up |
| O7 | `o07_overlapping_reference_fields_are_not_yet_rejected` | Authored | Typed: Checked. `overlap` Incomplete at `holder` | ri-pfxi (exclusive reference members; found by ri-kdi) |
| O8 | `o08_poisoned_slot_waits_on_descriptor_slot_storage` | RFC (O2 plus `poison`) | Typed: Incomplete (`pending Storage` at `*slot = source;`, `&mut view`, `*slot = &local;`) | ri-1up |
| O9 | `o09_retained_list_source_contract_is_incomplete_at_its_calls` | Authored | Typed: Checked. `forwarded` Incomplete at `retain(dst, source)`. `local` Incomplete at `retain(dst, &user)` | ri-ema |
| O10 | `o10_owned_list_reference_extraction_has_no_receipt` | Authored (two sources) | Typed: Incomplete. Consuming `for` over `List<&User>`: `pending Storage`. `get(&users, 0u64)`: `obligation FamilySelection` | ri-w0g |
| O11 | `o11_exclusive_return_view_contract_is_incomplete_at_its_call` | Authored | Typed: Checked. `conflict` and `ordered` both Incomplete at `editAndView(&mut owned)` | ri-ema |
| O12 | `o12_borrowed_temporary_is_incomplete_past_its_producer` | Authored | Typed: Checked. `shifted` Checked. `within` Incomplete at `(&shifted(Point { x: 1, y: 2 })).x`. `stored` Incomplete at `shifted(Point { x: 1, y: 2 })`. `literal` Incomplete at `"static"` | ri-w0g |
| O13 | `o13_joined_retained_sources_reject_early_release` | Authored (two sources) | Direct assignment joins: `joined` and `loops` R0203 at `dispose(left)` (related: `&left`). Through a void `remember` contract: `clear` Checked, `joined` Incomplete at `&mut slot` | ri-ema (void-call form) |
| O14 | `o14_replacement_reaches_the_old_disposer_and_a_stale_view_rejects` | Authored | Typed: Checked. `replace`: targeted query Checked, but the graph root is Incomplete at `dispose(value.text)` in the reached disposer. `stale` R0203 at `slot = Owner { text: next };` (related: `&slot`) | ri-4lq (graph root: written `dispose(value.field)`) |
| O15 | `o15_counter_body_and_its_expectation_check` | RFC | Typed: Checked. `counter` Checked. Expectation Checked with MIR (ri-785.9); the value 12 needs execution (M6) | none |
| O16 | `o16_two_shared_jobs_check_through_the_join` | Approved source (`SHARED` in `m5/scoped_join.rs`) | Typed: Checked. `joined` Checked. `joinedOrder` and the expectation Incomplete at `pair[0] == 13i32 && pair[1] == 29i32` (`&&` and an index operand; ri-785.9 admits neither) | ri-e9t8 (needs triage) |
| O17 | `o17_exclusive_copy_and_repeated_once_reject` | Authored (two sources) | `copy(&job)` of a closure holding `&mut flag`: typed SourceRejected, R0302 at `copy(&job)` (related: `&mut flag`). Repeated once: R0200 at the second `job()` (related: the first) | none: closed (ri-785.10 audit) |
| O18 | `o18_custom_disposer_rejects_field_extraction` | Authored | `plain` Checked. `custom` R0501 at `value.keep` (related: `value: Custom`) | none: closed (D2, ri-px1.7) |
| O19 | `o19_cleanup_order_disposes_the_holder_before_its_source` | Authored | Typed: Checked. `ordered` Checked: the replaced holder's disposer, then at scope end the holder's disposer before `drop source` (`o19_cleanup_sequence_disposes_the_holder_before_drop_source`). A source that ends first, in a nested scope or by `dispose`, is R0203 (`o19_neighbours_reject_an_earlier_ending_source`) | none: closed (ri-kdi) |
| O20 | `o20_transitive_guard_cleanup_is_incomplete_at_interpolation` | Authored | Typed: Checked. `render` Incomplete at the template expression | ri-w0g (template) |
| O21 | `o21_guard_release_replacement_reports_r0500` | Authored | Typed: Incomplete. R0500 is reported for `implement release(value: LockGuard<Payload>)`, but `obligation FamilySelection` at the empty body `{ }` keeps the completion Incomplete, not SourceRejected | none: see note 3 |
| O22 | `o22_descriptor_disposal_has_no_family_and_owner_disposal_rejects` | Authored (two sources) | `dispose(view)` of a `&string`: typed Incomplete (`obligation FamilySelection` at each `dispose(view)`). `dispose(owner)` while another descriptor copy is live: R0203 at `dispose(owner)` (related: `&owner`) | ri-w0g (descriptor disposal) |
| O23 | `o23_generic_no_borrow_evidence_rejects_r0302` | Authored | Typed: SourceRejected, R0302 at `onlyOwned(value)` | none: closed (ri-785.10 audit) |
| O24 | `o24_scoped_children_keep_caller_sources_and_native_has_no_source_form` | Authored, scoped children only | `joined` Checked. `released` R0203 at `dispose(first)` (related: `&first`). `afterUse` Checked. `childLocal` R0202 at `return &local;` (related: `local`). The native half has no source form | ri-qlh.2 (native and child completion, deferred to M6) |

## Notes on contradictions

1. H2's ticket expected O4, O5, O6, O8, O15 (body) and the O16 shared module to be `Checked`. Today O15's body and the O16 `joined` function are `Checked`. O4 is `SourceRejected` with R0200, which is what the RFC says (the ticket's guess was wrong). O5 has a typed receipt, but `separate` stops at the field arithmetic in its return. O6 and O8 have no typed receipt (O1 and O2 neither).
2. The ledger tests `*slot = input` through `&mut &bool` and cell updates in captures, but RFC 0007's own modules O1, O2 and O6 stop at M4 on `*value += 1i32;` and `*slot = source;` for `&mut i32` and `&mut &Point`. No closed slice claims those shapes, but nothing in M5 exercises the exclusive-reborrow and descriptor-slot rules the RFC's guide gives them. Filed as ri-1up.
3. O21: the protected-overlap boundary reports R0500 for the guard-release replacement, as the RFC asks. The typed report still completes `Incomplete` rather than `SourceRejected` because the empty body also leaves a `FamilySelection` obligation. The RFC wants a rejection during collection "even without a call". Whether the obligation should be credited as rejected is not decided here.
4. O14: the targeted query for `replace` is `Checked`, but the ordinary graph root is `Incomplete`, because the graph also checks the reached active disposer `implement release(value: Owner)` and stops at `dispose(value.text)`. This is the open ri-px1.7 gap, not a new one. It shows the targeted and graph results can differ for one root.
5. The RFC gives a complete module only for O1 to O6 and O15 (O8 adds `poison` to O2's module). Every other row is authored from prose or from the approved O16 source. Its outcome says what that module does, not what the RFC's case requires.
