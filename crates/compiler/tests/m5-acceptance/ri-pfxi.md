# ri-pfxi: RFC 0007 O7, indirect overlap through record reference fields (R-O7), 2026-11-24

Ticket ri-pfxi (spec ri-2s0), city bead ci-jnns. Branch `m5-ownership-cleanup/r-o7-ri-pfxi`, parent `b6c93f30` (retained-store B, ri-1up, merged). The compiler judges O7 over MIR. It does not run it (M6).

## Plan (written before the code)

O7 needs three things (found by ri-kdi, city bead ci-mw88): an exclusive reference member in record storage, a conflict between loans staged into one construction (R0201), and a store through a field dereference. One session builds all three, in this order:

1. **Test first, before any `&mut` member is admitted (the chief's condition).** `rfc_cases::o07::overlapping_construction_rejects_r0201`: `Holder { first: &mut value, second: &value }` and the reversed order are `SourceRejected` with R0201, site the second borrow, related the first. It does not use the O7 text: that text uses the record afterwards (`*holder.first += 1i32;`), which build stops at before flow runs, so a probe that only admitted `&mut` members saw no R0201 there. The new source has no such use. The construction alone must reject. Red on the parent (`Incomplete` at `holder`), committed first.
2. **Admit the exclusive member and make the construction conflict.** `aggregate` (`ownership/build.rs`) admits an `&mut i32` field of a record without a custom disposer. The record is move-only. `Action::Init` of such a record uses every member slot (`flow.rs`, `liveness`), so the first member's loan is live at the second borrow and the existing `conflict` check rejects. The check is the sound part: without that use, the overlapping construction is `Checked`.
3. **Store through the field dereference.** `add_through_exclusive_i32` (`build/i32_arithmetic.rs`) admits `*record.field += literal` where the field is an `&mut i32` member of a local record, resolved to its member slot like a parameter descriptor. `*record.field` reads were already admitted.
4. **Flip the O7 row** (mandated flip, rule 11, third case) and update `m5-status.md`.

Split clause: not needed. All four steps fit one session, so there are no child tickets.

Rule 1: the screens for the new admission read facts already paid for (a type, a projection, an `Option`, a flag), before the first charge. No declaration lookup is added, so the standing rule-1 exception is not used. Rule 3: no new keyed probe.
