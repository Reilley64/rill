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
