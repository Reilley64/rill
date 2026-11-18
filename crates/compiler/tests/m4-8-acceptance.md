# M4.8 bounded typed analysis

The owner approved U1 through U4 at `f3d65f72062237f87372f1896db5f2a66d9f2a54`. The [approval record](https://github.com/Reilley64/rill/issues/65#issuecomment-6969612205) precedes model and compiler work. [Issue #65](https://github.com/Reilley64/rill/issues/65) stays open under [M4 #56](https://github.com/Reilley64/rill/issues/56) in [draft PR #58](https://github.com/Reilley64/rill/pull/58). The approval does not accept the full stage or authorize a merge.

## What the increment checks

`analysis::typed::analyze_typed` uses one M3 capture and one accumulated analysis budget for inventory, body and public-interface checks. `inventory` exposes captured declarations and conditional comparisons. `bodies` exposes local body, node and goal facts with their original identities. `core_blockers` separates the missing role, provider, protection and representation evidence. `not_performed` lists stages not reached or outside the increment. A locally checked body does not establish a typed-stage receipt.

| Approved choice and rule | Compiler assertion or model comparison |
| --- | --- |
| U1, [core authority](../../../docs/rfcs/0002-modules-and-program-loading.md#proposed-m48-core-admission-and-interface-checks) | The report distinguishes the four missing evidence categories. Same-spelled `Result` and `List` declarations do not grant roles. |
| U2, [Result use](../../../docs/rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m48-result-use-checking) | Six Python tests compare finite states and witnesses with a path-history oracle and fixed outcomes. They cover replacement, distinct sibling components at one origin, guard commitment, unreachable source, repeated loop occurrences, retained iterator contents and unknown storage. The model uses abstract occurrences. It does not inspect Rill source or emit R0103. |
| U3, [indexing](../../../docs/rfcs/0005-types-records-enums-and-inference.md#proposed-m48-list-index-and-aggregate-handoff) | The tuple selector tests check radix, zero-based bounds, invalid selector forms, actual component type and the mutable borrow path. List indexing stays blocked. |
| U3, [core handoff](../../../docs/rfcs/0008-errors-propagation-and-caused-by-wrapping.md#proposed-m48-option-result-and-err-checking) and [pipe and iteration](../../../docs/rfcs/0003-expressions-bindings-and-control-flow.md#pipe-and-iteration-handoff) | Tests check original input order for named pipes and invocation through a local callable. A pipe step with `?` still checks its arguments. Invalid return targets reject independently. Unavailable guard or iterator contracts do not hide errors in written code, and an unresolved guard failure body does not prove fallthrough. |
| U4, [public and nested declarations](../../../docs/rfcs/0002-modules-and-program-loading.md#public-interfaces-and-nested-declarations) | A failed payload does not hide resolved private types in sibling payloads or components. Other tests cover public defaults and requirements, opaque signatures, intrinsic predicates, lexical shadowing, and invalid runtime capture even in an unused nested function. |
| U4, [aggregate classification](../../../docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m48-aggregate-classification) | An established non-copying tuple component settles a borrowed binder's type and capture demand. It does not prove that other aggregates can copy or grant permission to copy, borrow or move. |
| U4, [completion gate](../../../docs/implementation-plan.md#m48-design-checkpoint) | In both modes, tests check an expectation-only dependency's body, startup and non-Boolean expectation, plus the entry's unused body. Duplicate members, malformed declarations and blocked destructuring do not stop unrelated checks. Tests cover exact and one-short limits, foreign nodes, stages never reached at zero work, and a missing name retained when analysis exhausts its budget. |

## Independent review and corrections

Standards and Spec independently reviewed `18d3813639e414d5e8b9f942d16f96e470767a3c` against the approved checkpoint. The 18-file diff added 1,745 lines, removed 67, and had SHA-256 `f421c64206a6f913350a803ad2df0d5d4ff22d95a1b4d5ba269f201ca67d3f57`.

| Axis | Findings and disposition |
| --- | --- |
| Standards | Local declaration errors skipped unrelated bodies. Early exits did not list the stages they skipped. The tests missed an expectation-only dependency and allowed same-origin siblings to lose their component witnesses. The corrections localize prerequisites, record stage reach, cover both load modes, and keep component paths. Both guard forms now use one fallthrough check. The existing per-binding reservation charges work as well as entries for scope cloning. |
| Spec | Blocked destructuring hid later findings, and one invalid payload hid unrelated visibility findings. Pipes bypassed callable values and skipped argument checks before `?`. Call dispatch ignored a deeper named declaration. A pending family could falsely prove guard fallthrough, and exhaustion could erase a missing name. Regressions cover these cases. The corrected checker uses captured payload type uses, the shared invocation and lexical checks, and retains independent lexical and target findings on exhaustion. It does not publish unclassified type diagnostics from interrupted expressions. |

Local regression runs reproduced the failures. The full compiler gate also caught a match reachability regression during the correction. The final revision preserves settled reachability and leaves unavailable coverage pending. The corrections were verified locally, not independently re-reviewed.

## Verified code

Code revision [`853af9a64e11da1555cbae8fc0096221885ee039`](https://github.com/Reilley64/rill/commit/853af9a64e11da1555cbae8fc0096221885ee039) passed on a clean macOS 27.0.1 AArch64 worktree. Its diff from the approved checkpoint has SHA-256 `2e64cea598ded752023ac119e2684d6dca3a01c42b25f099d359b80421a91dcf`. Its corrections from the reviewed revision have SHA-256 `6930ff16f0e68b6133d551a4a06659ab6f233391888925e0b9c6d4ddb5ec7d83`.

| Check | Result |
| --- | --- |
| `mise run test:compiler` | 365 passed, including 15 M4.8 public tests. |
| `mise run test` | 373 passed, including the compiler suite. |
| `mise run check` | Passed. 1,783 documentation destinations, zero errors at the code revision. |
| `mise run build:scaffold` | Passed. |
| Python models | 55 passed, including six M4.8 tests. |
| Formatting, LLVM-free Clippy, dependency tree and diff | Passed. The compiler dependency tree contained only `compiler`. |

## Remaining obligations

The worktree has no shipped core implementation or validated toolchain authority source. Required provider and protected-operation admission, complete protection inventories, opaque representations and real numeric conversion providers are unverified. Captured paths, selectors and fixtures cannot supply that evidence.

The compiler has no source-level Result-use solver. Canonical Option, Result, Err, List, formatting and iterator checks still need admitted contracts. Exact propagation, including through a pipe, is pending. Aggregate cases that depend on custom disposal or protection also remain pending. A partial report cannot issue a typed-stage receipt.

The tests did not execute a Rill program. No full M4 or RFC acceptance, ownership permission, runtime, hosted runner, LLVM or product result follows. RFC 0001 remains Accepted at its recorded snapshot; RFCs 0002 through 0010 remain Draft. Workflows did not change, so `check:ci` was not run.

## Pending match and guard checks

Standards and Spec independently reviewed `5b10326e9ffc6dd638287f2fe3714c22a22e3aff` against `3c7e9bb0664ecb866630fbc1f349e333aef57ac5`. The code diff has SHA-256 `d257fcb306ceb6a7659f230ed43206bce094c954b7292280ae8703fe512c063e`. This review excluded the separate authority proposal. Standards found no documented violations and suggested sharing guard decoding. Spec identified a false numeric-context diagnostic, lost binder and variant facts, and missed guard fallthrough when a blocked read occurs only in a dead branch.

Three public regressions reproduced the problems. The corrections check written arms when coverage is pending. They defer missing receiving context, keep known sibling binder types and variant views, and derive guard fallthrough from reachable flow. The partial report does not publish unsettled match coverage or arm reachability.

The corrected code at `d6e76d91e649d3d834f246a14812bbb87eadd90b` passed the four mise gates locally. `test:compiler` passed 368 tests including 18 M4.8 tests, and `test` passed 376. `check` verified 1,817 documentation destinations with no errors. The correction diff has SHA-256 `8b4e034a763e75008e0faff15801ca68f7fb5fd3847a22b2eb15eb76e66ffd9f`. The corrections were not independently re-reviewed. The [A1 through A3 authority checkpoint](../../../docs/implementation-plan.md#m48-completion-checkpoint-first-party-core-authority) is unchanged. No installed core test or Rill execution ran.
