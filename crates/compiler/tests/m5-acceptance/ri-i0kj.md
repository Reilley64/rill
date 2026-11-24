# ri-i0kj: R0202 is reported at the call that stores the body-local source (2026-11-24, city bead ci-gs1f)

Ticket `ri-i0kj` (found in review `ci-0mdr` as SP-6, re-review `ci-b4b2` C-3), spec `ri-2s0`. Owner decision 2026-11-24: fix before M5 merges. Base `d85303bd` (`origin/implementation/m5-ownership-cleanup`), branch `m5-ownership-cleanup/r0202-site-ri-i0kj`. Logs: `~/.t3/evidence/rill/ci-gs1f/`.

This file supersedes the SP-6 line of "Final record after the re-review fix pass" in [`m5-acceptance.md`](../m5-acceptance.md) ("The compiler reports R0202 at the last call that retains into `dst` before the exit ... Pinned as current behaviour by `rfc_cases::list_contents::r1_blocker_site_is_the_last_retention_not_the_storing_call`"), and the stated limit recorded for it in [`ri-px1.11.md`](ri-px1.11.md) and [`ri-px1.11.5.md`](ri-px1.11.5.md). Those lines are not edited. The test was renamed back to `r1_local_then_parameter_reports_the_storing_call`.

## What changed

One source change, in `src/analysis/ownership/flow.rs`.

- Cause: `check_retained` reported the exit finding at `Held::origin`, which each retaining call overwrote with itself. A call that retained a valid parameter after the storing call, or a lone-destination call, became the site.
- Fix: a `Held` for outward contents carries `local_store`, the retaining call that first stored a body-local source into those contents and that source's borrow. A retaining call sets it only when the contents hold no body-local loan yet and its inputs add one; otherwise it keeps the earlier value. `check_retained` reports the loan that `local_store` names, at that call. `origin` and `RetainedAtDelivery::last_store` are unchanged (still the last store). A contents value with a body-local loan and no `local_store` (a strong update through `InstallThrough`) reports as before.
- `Held` grew from 208 to 232 bytes, which trips `clippy::large_enum_variant` on `State` (the lint fires above a 200-byte gap). `State` carries `#[allow(clippy::large_enum_variant)]`, as other files of the crate allow `too_many_arguments`. Boxing `Held` would touch every state site; sharing the bytes of `origin` would drop `last_store`, which `RetainedAtDelivery` publishes.
- Which store when there are two: the first. RFC 0007 line 218 has the callee's exit check reject "a body-local source that flows" to `dst`; the earliest store is where the contents first hold one, and the analysis reports one finding per root, so it names one. The related site is that call's own borrow, in either declaration order (`secondThenFirst`).
- Outcomes: every program stays `SourceRejected` with R0202; only the site moved. No cap moved: every existing pin passed with its caps unchanged (rule 11 does not apply; no old cap needed a `ResourceLimit` assertion).

## Tests

Red first (`red-site.log`, the source change reverted): three tests fail on the site alone and the new test fails.

| Test | Roots | Old site | New site |
|---|---|---|---|
| `list_contents::r1_local_then_parameter_reports_the_storing_call` (renamed) | `localThenParam` | `retain(dst, a)` | `retain(dst, &user)` |
| `list_contents::r1_old_contents_survive_a_later_retain` | `reversed`, `reversedViaCall` | `retain(dst, a)`, `forward(dst, a)` | `retain(dst, &user)` for both |
| `list_contents::r1_lone_destination_keeps_a_stored_local_rejected` | `loneAfterLocal` | `touch(dst)` | `retain(dst, &user)` |
| `retained::original_two_hop_relay_checks_reached_bodies_and_preserves_backing` | `relayTwice` | `keep(slot, input)` | `keep(slot, &flag)` |

The last three rows are the same defect, found by running the tests whose pins named a later call; the related site of each is unchanged (`&user`, `&flag`). `relayTwice` asserted that the related borrow precedes the site; it now asserts that the borrow is inside the site call.

New: `list_contents::r1_two_local_stores_report_the_first_storing_call` (`TWO_LOCALS`; both load modes; each root and the graph pinned exact and one short on every axis through `assert_body_limit` and `assert_graph_limit`). Typed 343,618 / 51,567 / 12; graph `SourceRejected` 210,460 / 7,464 / 8.

| Root | Site | Related | Caps |
|---|---|---|---|
| `firstThenSecond` | `retain(dst, &one)` | `&one` | 5,785 / 778 / 6 |
| `secondThenFirst` (locals declared `one`, `two`; stored `two` first) | `retain(dst, &two)` | `&two` | 5,786 / 778 / 6 |
| `twoThenParam` | `retain(dst, &two)` | `&two` | 7,309 / 1,087 / 6 |
| `loopLocalThenParam` | `retain(dst, &user)` | `&user` | 6,065 / 1,037 / 8 |

The loop forms already pinned (`loopOwned`, `loopBreak`, `oneBranch`, `r1_loop_body_local_source_retained_into_dst_rejects`, `r1_call_in_a_loop_checks_with_its_sequence`) pass with their caps unchanged.

## Checks run (targeted; no full suite)

- `cargo test -p compiler --test m5`, selected by name: `list_contents` (25 tests), then `retained::`, `graph::`, `field_routes`, `record_contents`, `legacy_caps`, `tuples`, `patterns`, `propagation` and `rfc_cases::` (333 tests, one failure: `relayTwice`, fixed above; `green-targeted.log`), then `cancellation`, `records`, `scoped_join`, `scoped_jobs` and `captures` (129 tests, all pass; `green-targeted-2.log`), and `retained::` again after the fix (25 pass).
- `cargo fmt -p compiler -- --check`, `cargo clippy --locked --no-default-features -p compiler --all-targets -- -D warnings` and `mise run check:docs`: clean on the final tree.

## Not established

- The remaining `m5` tests, the other test binaries and the hosted matrix did not run locally; CI runs them.
- Which of several body-local stores the RFC prefers is not stated in it; the first is a choice, recorded above.
