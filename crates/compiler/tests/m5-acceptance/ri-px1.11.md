# Final record and PR statement (ri-px1.11, D6, 2026-11-23)

Ticket D6 (gap-analysis row D6), umbrella ri-px1. This ticket changes no compiler source, test or cap. Under the planner's decision of 2026-11-23 on `ri-2s0` it writes the record; the full gates run in hosted CI only, and the paired Standards and Spec reviews follow as `implement-spec` step 7.

## What changed

- [`m5-acceptance.md`](../m5-acceptance.md): "Open acceptance obligations" is replaced by "Final record: deferred obligations (2026-11-23, ri-px1.11)". It lists every §1.6 row with its bead, the ticket-index deferrals, the found beads still open (state checked 2026-11-23) and points to the three closeout audits.
- [`m5-status.md`](../m5-status.md): owners that named closed tickets now name the bead that owns the gap. O14: `ri-4lq` (the graph root of a written `dispose(value.field)`), O18: none (D2 closed), O20: `ri-w0g` only (D3 closed), O17 and O23: none (B5 audit closed).
- Body of draft PR #73: a section states that the M4 Result-use work stays in this PR (owner decision D6 (a)). The PR stays draft.

## Superseded record line

D4's file [`ri-px1.9.md`](ri-px1.9.md) says `ordinary_only_list_loops_keep_the_pre_delivery_caps` "still holds at 191,781 / 4,867 / 6". B6 (`ri-785.9`) re-pinned that whole-graph cap to 192,097 / 5,001 / 6 ([`ri-785.9.md`](ri-785.9.md), rows `fixed-list` and `cancellation.rs` ordinary list loops), each old cap asserting `ResourceLimit`. The D4 line is superseded by the B6 re-pin; D4's file is not edited. D5's audit ([`ri-px1.10.md`](ri-px1.10.md)) already names B6's re-pin.

## Not established

- The hosted checks on the final head and the paired reviews are not part of this record; they follow in `implement-spec` step 7. `ri-px1.11` stays open until then.
- No Rill program ran.
