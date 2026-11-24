# Review ci-0mdr fixes, part C6: test-only pins and measurements (2026-11-24, city bead ci-gjvp)

Part of the single fix pass of review `ci-0mdr` (second review of `81d317b3`), a child of `ri-px1.11` (ticket `ri-px1.11.5`). Base `origin/implementation/m5-ownership-cleanup` at `da5bec50` (contains C1, `ead19c6a`); branch `m5-ownership-cleanup/r2f-ri-px1.11.5`. No production source changed and no published cap moved. Every number below was measured by a throwaway probe test (not committed) and then pinned in a committed test; parents were measured in a detached worktree of the cited commit, never by stash. Logs: `~/.t3/evidence/rill/m5-build-2026-11-22/ci-gjvp/`. The records child (C7) consolidates this file into `ri-px1.11.md`.

## Findings

| Finding | Status | Where |
|---|---|---|
| ST-1 | fixed | `list_contents::list_destination_siblings_keep_their_outcomes_and_blockers` gains `GROUPED` (below); named in `compiler-metering.md` rule 1 (addition of 2026-11-24, review ci-0mdr ST-1) |
| ST-10 | fixed | `list_contents::r1_caller_provided_exclusive_source_stops_typed` asserts the typed report's Result-use pending list and body findings (below) |
| SP-1 | fixed | `list_contents::r1_old_contents_survive_a_later_retain` |
| SP-2 | fixed (temporary sources: stated limit) | `list_contents::r1_owned_parameter_and_temporary_sources_do_not_escape` |
| SP-5 | fixed | `list_contents::r1_pair_with_one_body_local_source_rejects`, `list_contents::r1_lone_destination_keeps_a_stored_local_rejected` |
| SP-6 | fixed | `list_contents::r1_local_then_parameter_reports_the_storing_call` |
| ST-6 | fixed | `o16_join_order::admitted_shapes_check_at_exact_caps` and `grouped_operands_add_depth_not_operations` |
| ST-7 | recorded, partly fixed (no revision holds the hybrid base) | `rfc_cases::o01_exclusive_write_siblings_typed_parent_cap_now_gives_resource_limit`; see ST-7 below |
| ST-11 | fixed | doc line with the parent commit on the four sibling constants in `rfc_cases.rs` |
| ST-12 | fixed | `rfc_cases::o11_pre_family_call_caps_now_give_resource_limit` |
| ST-13 | fixed | `pre_equality_graph` is `Option<Triple>` in `rfc_cases.rs` |
| ST-15 | fixed | `cancellation::installed_guard_release_on_budget_delivery` (`hold`) and `disposers::protected_guard_cleanup_follows_the_initialized_owner` (`replace`, `replaceMoved`) |
| ST-16 / R-4 | fixed | `o15_arithmetic::rfc_counter_caps_include_the_charged_local_reads` |
| ST-17 / S2-F4 | fixed | `scoped_join::reference_pair_reserves_before_it_builds_each_slot` |
| "Unverified" (`retained_contents` siblings) | measured; list forms listed under the standing ruling and pinned; two `&mut &R` forms over bound (c) reported | below; found bead `ri-zee8` |

Each new `list_contents` test goes through `check_cases`: the targeted query and the ordinary graph root, both load modes, exact caps, one short on each axis (`assert_body_limit`, `assert_graph_limit`), and the finding code, site and related site of every rejected root.

## ST-1: `grouped` is the fifth use of the standing rule-1 exception

Correction to `ri-ema.md` (its old/new table lists four siblings, `NAMED`, `NESTED`, `ENUM`, `FLAG`) and to the final record: `grouped` (`retain((dst), source)`) also pays before it stays `Incomplete`. It pays the parameter's one charged declaration lookup, as `FLAG` does, and keeps its outcome and blocker site.

| Sibling | Parent (`b6c93f30`) | Now | Move |
|---|---|---|---|
| `grouped`, own module (`GROUPED`) | 1,905 / 42 / 3 | 1,979 / 45 / 3 | +74 work, +3 entries |
| `grouped`, graph of that module | 188,126 / 4,143 / 5 | 188,200 / 4,146 / 5 | +74, +3 |
| `grouped`, in `FAMILY_FORMS` (the review's figure) | 1,995 / 87 / 3 | 2,069 / 90 / 3 | +74, +3 |

Conditions (a) to (d) of the standing exception: (a) the lookup comes after the free type screen; (b) outcome `Incomplete` and blocker `retain((dst), source)` are asserted; (c) one lookup in one module, +74 work, under 150, plus the +3 entries of that lookup (bound as amended by ruling 11, 2026-11-24 11:35); (d) the old/new table is the one above and the parent cap is asserted to give `ResourceLimit` (`old_root_caps`, no MIR, no finding). The module-local figures differ from the `FAMILY_FORMS` ones because root statistics depend on the module's other roots. The rule-1 note in `docs/agents/compiler-metering.md` names the test.

## ST-10: the typed stop of a caller-provided exclusive source names its sites

The typed report has no body obligation and no core blocker for these two programs, but its Result-use pending list names the stop (`typed.result_use().unwrap().pending()`), and the test now asserts it, in both modes, beside the exact typed caps and one-short `ResourceLimit`:

| Program | Typed caps | Pending (blocker, site) | Body findings |
|---|---|---|---|
| `retain(dst, &*source); source.id = 2;` | 326,581 / 48,099 / 12 | `(Storage, "source.id = 2;")` | none |
| `retain(dst, source); source.id = 2;` | 326,114 / 47,888 / 12 | `(TypedInput, "retain(dst, source)")`, `(Storage, "source.id = 2;")` | `R0300` at `retain(dst, source)` |

Finding for the record: the second program is an ordinary type error (`&mut User` where the destination's source parameter is `&User`), so "passing `source` itself" is rejected by the typed pass, not only stopped. The doc comment of the test says so. The `assert_typed_limit_keeping_body_findings` calls stay (pending `ri-68mu`).

## SP-1, SP-2, SP-5, SP-6: new R1 pins (no behaviour change)

All measured at this branch's base; every shape came out as the review's probes predicted, except where noted.

| Test | Program shape | Outcome and site |
|---|---|---|
| SP-1 | local stored first, then `retain(dst, a)` (parameter), directly and through `forward(dst, a)` | `R0202` at `retain(dst, a)` / `forward(dst, a)`, related `&user`; `forward` alone checks. A strong update would check both |
| SP-6 | `retain(dst, a); retain(dst, &user)` and `retain(dst, &user); retain(dst, a)` | `R0202` related `&user`, at `retain(dst, &user)` and at `retain(dst, a)`. The site the rule gives is the **last retaining call before the exit**, not the call that stored the local; the second is the valid call |
| SP-2 | `&owned` of a by-value parameter: direct, through a local view, on one branch, `while (flag)`, loop that breaks | `R0202` at the retaining call, related `&owned` |
| SP-2 | temporary source: `retain(dst, &make())`, `retain(dst, &User { id: 1 })` | targeted and graph `Incomplete`, blocker `make()` / `User { id: 1 }`; stated limit, no list destination for a temporary |
| SP-5 | pair with one body-local source, either position | `R0202` at `retain(dst, a, &user)` / `retain(dst, &user, a)` |
| SP-5 | lone destination (`touch(dst)`) after, and before, a stored local | `R0202` at `touch(dst)` (local first) and at `retain(dst, &user)` (local last): again the last retaining call |

Caps pinned (typed / graph / roots, work, entries, depth; each exact and one short on every axis by `check_cases`):

| Module | Typed | Graph | Roots |
|---|---|---|---|
| `OLD_CONTENTS` | 333,755 / 49,416 / 12 | 197,880 / 5,390 / 6 | `forward` 3,384 / 270 / 5; `reversed` 5,130 / 572 / 6; `reversedViaCall` 4,360 / 572 / 6 |
| `ORDER` | 332,006 / 49,227 / 12 | 196,271 / 5,172 / 6 | `paramThenLocal` 5,108 / 561 / 6; `localThenParam` 5,104 / 559 / 6 |
| `OWNED_SOURCES` | 339,046 / 50,101 / 12 | 201,987 / 5,871 / 8 | `byValue` 3,689 / 314 / 6; `hop` 3,811 / 372 / 5; `oneBranch` 3,974 / 459 / 8; `loopOwned` 4,234 / 584 / 8; `loopBreak` 3,809 / 376 / 8 |
| `TEMPORARY_SOURCES` | 331,121 / 48,738 / 12 | 191,890 / 4,301 / 6 (`Incomplete`) | `make` 2,363 / 154 / 4; `call`, `literal` 2,941 / 63 / 6 |
| `PAIR_LOCAL` | 331,050 / 49,095 / 12 | 193,688 / 4,884 / 6 | `localSecond` 3,901 / 415 / 6; `localFirst` 3,899 / 414 / 6 |
| `LONE_LOCAL` | 333,673 / 49,207 / 12 | 197,402 / 5,006 / 6 | `loneAfterLocal` 4,951 / 475 / 6; `loneBeforeLocal` 4,960 / 478 / 6 |

Noted while measuring (not pinned): `while (true) { retain(dst, &owned); }` with no exit checks (no exit reaches the check), as `looped` does with a parameter source; the review's loop probe needs a loop that can leave.

## ST-6: O16's admitted shapes assert the whole MIR

`o16_join_order::ADMITTED` gains `sequence`, the whole MIR of `f` for each of the 17 `Checked` rows (the one `SourceRejected` row has none): every block in index order as (operation and where control goes next, site text), so branches, loops and the cleanup forms (`lit-leak`, `lit-scoped`, `lit-loop`, `lit-arg`, `mut-pair`) are pinned, not only the straight path. The test compares the targeted query and the graph root's ordinary MIR to it, both modes. `grouped_operands_add_depth_not_operations` also compares the whole list with places (it compared operation names only) against the `param-pair` and `grouped` rows. Lists were measured on this branch by a throwaway dump, then committed; no cap moved.

## ST-15: whole lists for `hold`'s suffix and the `replace` routes

`hold`'s ordinary suffix is `init 7`, `export 7`, `cleanup [release lock: LockGuard<i32>; end again: bool]`, `none`, now one list (the old test found the export and the release with `position`). The `replace` route is `move first -> held`, `move second -> second`, `cleanup [release held]`, `install held <- second`, `cleanup [release held]`, `cleanup []`, `none`; `replaceMoved` is `move first -> held`, `cleanup [release held]`, `init dispose(held)`, `none`, `move second -> second`, `cleanup []`, `install held <- second`, `cleanup [release held]`, `cleanup []`, `none`. The shared renderer is `render_path` in `tests/m5.rs`; the earlier filtered slice patterns stay beside the whole-list assertions. (The review cites `disposers.rs:396-405`, now `:393-440`.)

## ST-11, ST-12, ST-13

- ST-11: `O10_SIBLINGS` cites `4f7a8032`, `O10_LOOKUP_SIBLINGS` `63b9b3e3`, `O10_GET_SIBLINGS` `b7e58baf`, `O11_SIBLINGS` `7002855e` (each from the evidence file named beside it, `ri-2s0.7.md`, `ri-2s0.7.2.md`, `ri-2s0.7.1.md`, `ri-2s0.6.md`).
- ST-12: O11's graph cap before the i32 equality root was 189,259 / 4,085 / 4. `ri-2s0.6.md` never held it (the commit `2d5a1c03` overwrote the row's `pre_equality_graph`), so `o11_pre_family_call_caps_now_give_resource_limit` now asserts it gives `ResourceLimit` beside 189,575 / 4,219 / 5, and its doc says where the number came from. It is not re-measured (the revision before ri-785.9 does not hold this module).
- ST-13: `Outcome::Receipt.pre_equality_graph` is `Option<Triple>`. Three rows carry `None` because their number was not a measurement of the module: the O11 negatives (`indexed`, `reborrowed`, `disposed`, `moved`, `escaped`, which held the neighbouring module's 189,575 / 4,219 / 5), O13 (203,548 / 7,569 / 8, the graph minus one work) and O22 (202,578 / 6,570 / 8, derived). Not changed: rows whose comment says the value is the graph less the expectation root's attributed work (O1, O6 and a few more); those are derived from a measurement of the same module, and the review did not list them.

## ST-7: the O1 sibling caps are the typed admission on the parent

`o01_exclusive_i32_write_siblings_keep_their_screens` pins its `Incomplete` roots at caps measured "with only the typed admission applied". No revision holds that (the ticket landed as one commit, `9547ef70`), and the parent `7002855e` alone stops this module in the typed pass (typed `Incomplete`, 334,460 / 48,395 / 12, no receipt), so no parent ownership cap exists for these roots. Measured here: a detached worktree of `7002855e` with only the typed-admission part of `9547ef70` applied (`o01-typed-admission-only.patch`, `result_use.rs`, in the log directory) gives typed 335,273 / 48,561 / 12 (`Checked`, equal to the pinned value) and exactly the pinned `Incomplete` roots: `groupedTarget`, `groupedOperand`, `groupedDescriptor` 1,998 / 92 / 2, `computed` 2,004 / 96 / 2, `viaLocal` 2,008 / 101 / 2; `dead` 2,055 / 118 / 2. So the ownership lowering adds nothing to a sibling that it does not admit: the property rule 2 asks for holds, measured on the hybrid base, not on a commit. Committed: the sibling constant's doc now says so, and `o01_exclusive_write_siblings_typed_parent_cap_now_gives_resource_limit` asserts that the parent's typed cap (334,460 / 48,395 / 12) now gives `ResourceLimit` with no receipt. Open for the chief: accept the hybrid base as the rule-2 witness for this row, or ask for a patch-built revision to be kept.

## ST-16 / R-4: the counter's +58

`rfc_counter_caps_include_the_charged_local_reads` computes it: the parent's 2,504 charged each read of `first` and `second` as one step; now each charges the key's path plus one, three probes. The test asserts `2,562 - 2,504 == 2 x (3 x (len("main.rill") + 1) - 1)` with the length read from the module, and that the parent cap gives `ResourceLimit`.

## ST-17 / S2-F4: a cap that separates the two reserve orders

The reorder moved six units of reserve earlier and left the total (427,606 / 49,151 / 12) alone, so the exact and one-short caps hold in either order. An entries cap does separate them, inside a narrow window: a scan of every entries cap from the total down 5,600 (both trees, same module) found the stop statistics identical outside back 4,187 to 4,194 and different inside it. Pinned in the test, three caps in the window, at `max_work` and `max_depth` exact:

| `max_entries` | Stops at, with the fix (pinned) | Stops at, fix reverted |
|---|---|---|
| 44,964 | 405,468 / 44,963 / 12 | 405,462 / 44,957 / 12 |
| 44,962 | 405,465 / 44,960 / 12 | 405,462 / 44,957 / 12 |
| 44,959 | 405,434 / 44,957 / 12 | 405,462 / 44,957 / 12 |

Reverting only `typed/result_use/pairs.rs` (`git apply -R` of the source part of `e0c20e4c`, in a detached worktree) fails the test at the first cap (log `red-s2f4-reverted.log`); the fix passes it.

## The "Unverified" item: who pays the `retained_contents` lookup

Measured by a throwaway probe over 160 one-function modules (5 record shapes, 17 contexts each for `&mut &R` and for `&mut List<&R>`: a bare `&mut` borrow, a direct call, a piped call with the destination first or later, a family call and a forwarded parameter, among others), run on this branch and on the parents `bc9bfdd5` (before `ri-1up`) and `b6c93f30` (before `ri-ema`). Shapes and outputs: `probe-shapes/` and `compare-*.txt` in the log directory. No outcome or blocker moved for an unsupported record; the only flips are the admitted `i32` records (`Incomplete` to `Checked`, the ruled admissions).

- Piped calls pay. `dst |> touchTwo(s)` costs what `touchTwo(dst, s)` does (+144 work for a `string` field behind a list), and `dst |> touchF(s)` (family) pays the parameter's lookup only (+72).
- A bare `&mut view` borrow of `&R` pays one lookup (`f` +72 for a `string` record).
- Lists (against `b6c93f30`): every body that declares a `&mut List<&R>` parameter for an unsupported `R` pays one lookup (+72 `string` or nested, +70 enum), `Checked` or not; a root that also passes it on pays two (+144, up to +148 work and +3 entries). All stay inside bound (c). Under the standing ruling, listed in `docs/agents/compiler-metering.md` and pinned by `rfc_cases::list_contents::list_parameter_siblings_pay_their_lookup_and_keep_their_outcomes` (`string` field; seven module forms; old caps give `ResourceLimit`). A nested record and an enum were measured and not pinned: nested +72/+144/+148 as `string`, enum +70/+140/+144.
- `&mut &R` (against `bc9bfdd5`): the same, except a root that declares a `&mut &R` parameter and passes it to a function that also takes a `&R` pays **three** lookups: +216 work for `string` and nested records, +210 for an enum, over bound (c). Reported, not listed: found bead `ri-zee8` (needs-triage).
- Typed work moves +72 once in each module that holds such a parameter (one typed lookup).

Old (parent) caps for the list modules were measured in a detached worktree of `b6c93f30` with the same module text; only roots whose cap moved assert `ResourceLimit` for the old value.

## Caps

No published cap moved: every existing expectation passes unedited. Caps added are pins of current behaviour, each exact and one short by `check_cases`.

## Standing-ruling uses

`grouped` (ST-1, fifth sibling of the ri-ema exception, in `compiler-metering.md`); the list-parameter siblings above (listed under the standing ruling, in `compiler-metering.md`). Over bound and reported: the `&mut &R` forwarding forms (`ri-zee8`).

## Not established

- `retained_contents` sites were not traced one by one; the lookup counts are inferred from whole multiples of one lookup's work.
- ST-7's hybrid base is a measurement, not a revision.
- Unlisted rows of `pre_equality_graph` that are derived by subtraction keep their values.
- The review's `o07`, ST-2 to ST-5, ST-8, ST-9 and SP-3, SP-4, SP-7 belong to other children of `ri-px1.11` and are not touched here.
