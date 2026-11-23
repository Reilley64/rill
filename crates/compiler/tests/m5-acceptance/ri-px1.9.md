# Guard release on a delivered cancellation exit (ri-px1.9, 2026-11-23)

This is ticket D4. Before it, an enabled-frame delivery proof admitted one owned resource with a release action: the `List<i32>` backing of a leaf frame whose only reached checkpoint is a simple top-level `while (param) { continue; }` loop ([Delivery cleanup at a local List loop](../m5-acceptance.md)). A frame that owned a `LockGuard<i32>` across the same loop published no proof. The flow pass now admits a whole `LockGuard<i32>` slot (cleanup `ReleaseGuard`) under exactly the List conditions: a leaf frame with no outward storage, no reached call, and no other backedge or branch. A written `dispose(lock)` is a leaf action in that frame, like a written List release. Delivery cleanup is built by the existing frame exit, so it uses the slot's own protected provider. The change is in `ownership/flow.rs` (the flag formerly called `list_backing` is now `loop_owner`).

[`installed_guard_release_on_budget_delivery`](../m5/cancellation.rs) checks the witness in both installed modes, through the targeted query and the ordinary graph root:

```rill
function hold(again: bool, lock: LockGuard<i32>): i32 {
    while (again) { continue; }
    return 7;
}
```

- Entry delivery and the one `LoopBackedge` delivery at `while (again) { continue; }` each clean `[ReleaseGuard(lock), EndStorage(again)]`. The guard is released once, through the clear required guard-release provider's implementation and entry, before the frame's remaining parameter storage ends. The delivery exit exports nothing and retains nothing.
- Without delivery the loop exits to the ordinary suffix, which exports the result and then releases the guard once.
- `released`, which writes `dispose(lock)` before the loop, releases the guard on entry delivery only. Its backedge delivery ends `again` and does not release the guard again. Its graph root is `Checked` with the same proof.
- `moved`, which moves the guard into `let held = lock;`, gets a targeted proof that releases `held` before `again` ends. Its graph root stays `Incomplete` at the body, as D3 left a released local binding, and publishes no delivery proof.

Caps (work / entries / depth). Each new cap is exact; each independently one-short cap gives `ResourceLimit` with no MIR, and no root keeps a proof.

| Query | Parent `3da1d5a3` (no proof) | Now |
| --- | --- | --- |
| `hold` targeted | 2,131 / 156 / 4 | 2,252 / 203 / 4 |
| `released` targeted | 2,174 / 175 / 4 | 2,294 / 223 / 4 |
| `moved` targeted | 2,177 / 171 / 4 | 2,311 / 223 / 4 |
| Whole graph of this fixture | 188,514 / 4,540 / 5, `Incomplete` | 188,887 / 4,635 / 5, `Incomplete` (`moved`) |

These are new fixtures; the parent figures are the red measurement, not re-pins. The whole-graph figures include B6's `compare.rill` i32 equality root (ri-785.9), which reached the base while this ticket was in flight; before it, at parent `0f595438`, the same fixture measured 188,198 / 4,406 / 4 and, with this change, 188,571 / 4,501 / 4. The targeted figures did not move with B6.

[`guard_loops_without_a_proof_keep_their_parent_caps`](../m5/cancellation.rs) pins, at the caps measured at the parent (the targeted caps at both `0f595438` and `3da1d5a3`), guard frames that must not gain a proof: an entry-only guard (1,992 / 83 / 1), a call before the loop (4,131 / 245 / 4), a loop on a local condition (2,209 / 192 / 4), a loop body with a `break` (2,162 / 166 / 6), and a shared view of the guard across the loop, which stays `Incomplete` at `&lock` (2,012 / 86 / 5). The whole graph stays `Incomplete` at 190,495 / 4,613 / 6, measured at the parent `3da1d5a3` (190,179 / 4,479 / 6 at `0f595438`, before B6). Each exact cap gives the parent outcome with no delivery proof, and each one-short cap gives `ResourceLimit`.

No existing expectation was edited and no fixed cap moved: `ordinary_only_list_loops_keep_the_pre_delivery_caps` still holds at 191,781 / 4,867 / 6, and `m5::legacy_caps`, `m5::rfc_cases` and `m5::graph::inventory` pass unchanged.

Commits: `349fc031` (flow change, tests and this record), then the post-B6 whole-graph pins, merged with the base tip.

Not established: a guard field of a record, a guard under a custom disposer, a guard released at a callee entry or a retaining call, and delivery with outward storage remain without a proof. The `moved` graph root waits for a verified scope-exit release of a local binding. This is compiler MIR evidence; no Rill program ran and no scheduler delivered a request.
