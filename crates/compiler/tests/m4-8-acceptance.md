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

## Remaining obligations at `853af9a`

The [installed Option checks](#installed-option-checks) verify a limited set of first-party sources. The complete installed bundle, provider and protection inventories, opaque representations and real numeric conversion providers are unverified. Captured paths, selectors and fixtures cannot supply that evidence.

The compiler has no source-level Result-use solver. Canonical Result, Err, List, formatting and iterator checks still need admitted contracts. Propagation through a pipe and aggregate checks that depend on custom disposal or protection remain pending. A partial report cannot issue a typed-stage receipt.

The tests did not execute a Rill program. No full M4 or RFC acceptance, ownership permission, runtime, hosted runner, LLVM or product result follows. RFC 0001 remains Accepted at its recorded snapshot; RFCs 0002 through 0010 remain Draft. Workflows did not change, so `check:ci` was not run.

## Pending match and guard checks

Standards and Spec independently reviewed `5b10326e9ffc6dd638287f2fe3714c22a22e3aff` against `3c7e9bb0664ecb866630fbc1f349e333aef57ac5`. The code diff has SHA-256 `d257fcb306ceb6a7659f230ed43206bce094c954b7292280ae8703fe512c063e`. This review excluded the separate authority proposal. Standards found no documented violations and suggested sharing guard decoding. Spec identified a false numeric-context diagnostic, lost binder and variant facts, and missed guard fallthrough when a blocked read occurs only in a dead branch.

Three public regressions reproduced the problems. The corrections check written arms when coverage is pending. They defer missing receiving context, keep known sibling binder types and variant views, and derive guard fallthrough from reachable flow. The partial report does not publish unsettled match coverage or arm reachability.

The corrected code at `d6e76d91e649d3d834f246a14812bbb87eadd90b` passed the four mise gates locally. `test:compiler` passed 368 tests including 18 M4.8 tests, and `test` passed 376. `check` verified 1,817 documentation destinations with no errors. The correction diff has SHA-256 `8b4e034a763e75008e0faff15801ca68f7fb5fd3847a22b2eb15eb76e66ffd9f`. The corrections were not independently re-reviewed. No installed core test or Rill execution ran at that revision.

## Installed Option checks

The owner [approved A1 through A3](https://github.com/Reilley64/rill/issues/65#issuecomment-6971033745) at `3c7e9bb0664ecb866630fbc1f349e333aef57ac5` before compiler and core changes. This increment checks the first installed source group. It does not complete the approved authority or provider inventories.

Run `mise exec -- cargo test --locked -p compiler --no-default-features --test m4_8_installed`. The Rust test runs a copy of itself under a temporary `bin/` directory beside the staged `stdlib/core.rill` and `stdlib/option.rill` sources in `lib/rill/std/`. It calls `loader::load_installed` and `analyze_typed` from another working directory. It does not stage a full distribution or exercise a product command.

| Case | Observed outcome |
| --- | --- |
| Pinned sources in both load modes | The report admits the `Option` schema and the checked `isSome` provider. It checks the Rill bodies of `asRef`, `asMut`, `map` and `andThen`. The prelude declarations still lack providers. |
| Postfix propagation through an import alias | The shared checker keeps the canonical variants and the callable target. Unknown, exact `None` and exact `Some` operands retain different flow facts. A borrowed operand or incompatible return type is rejected. |
| Source-only load with identical bytes, unsupported target, or unsupported options | The source graph loads but carries no first-party admission. |
| Question-mark trigger without a source import | The loader captures the pinned metadata file without adding an application binding. The local same-shaped enum is not `Option`. If the optional file is missing, the source graph still loads and the role remains pending. |
| Changed pin and retained graph | Stable mismatched bytes are `InvalidRequest`. Revalidating an earlier capture after editing the file is `SourceChanged`. The typed checker still uses its immutable captured bytes. |
| One-short limits | The metadata dependency is charged against the module limit. The shared analysis budget also reports exhaustion when shortened by one unit of work. |

### Review and verification

Standards and Spec independently reviewed `d3cdbfa9df2de0ec36ce1728169c353311741c2c` against `a6b763216e41f2b55f9b6d47c0973e6f5fa895ae`. The diff has SHA-256 `2ccb252e0ec589bbc41ce52477850c5d6918c6279eef31532c1cd99872f9fd3a`.

| Axis | Findings and corrections |
| --- | --- |
| Standards | A known invalid propagation target became pending. The checker now validates that target before it depends on the operand's core role. Admission lookups lacked work charges. A test checks the exact role budget and the one-short limit. Public and metered body queries share the identity comparison. |
| Spec | A source error in the operand created a false core blocker. Pending arrow flow caused a false return-coverage error. Option schema and inspection checks allowed extra defaults or requirements. Regressions distinguish all of these cases, including an admitted `Some` path that can fall through. |

The correction at `7490ce4ae063ae1bbe77e7347f1799f0e9bd6f02` has diff SHA-256 `cd6b1b534b6c6c00124db35db157f07798d380b309b111058cd8979a39bda640` from the reviewed revision. Local tests verified the corrections. They were not independently re-reviewed.

| Check | Result on macOS 27.0.1 AArch64 |
| --- | --- |
| `mise run test:compiler` | 370 passed, including the installed test and header-schema unit test. |
| `mise run test` | 378 passed. |
| `mise run check` and `mise run build:scaffold` | Passed. The documentation check verified 1,820 destinations with no errors. |
| Python models | 55 passed. |
| Rust formatting, LLVM-free Clippy, dependency tree, and diff | Passed. The compiler dependency tree contains only `compiler`. |

The installed test results are static checks. No Rill program ran. At that revision, native artifacts, complete inventories, and positive typed-stage receipts in both modes remained NotChecked.

## Integrated installed core slices

The combined macOS AArch64 probe in [`m4_8_ownership.rs`](m4_8_ownership.rs) stages pinned standard-library sources, the build-produced ownership archive, and the error-storage source. It starts from a different working directory and uses `load_installed` and `analyze_typed` in both load modes. In a single capture, it checks a body that builds a List, converts and formats an element, and returns a Result. It also checks entry startup and an expectation, and observes independent role and provider facts. **The report is `Incomplete` in both modes.** The probe does not stage the production executable or implement a distribution recipe.

| Slice | Verified evidence | Still pending |
| --- | --- | --- |
| Option, Result and Err | Canonical roles and aliases, Result's default Error, owned and borrowed guard bindings, exact-E propagation and propagating pipes. The Rust storage suite checks owned heterogeneous causes and iterative release of a deep chain. | Checked Err construction and infix wrapping with selected display and retention evidence; lowered representation and provider closure. |
| Result use | The source pass emits R0103 with origin and residence for tested discards, including renames, tuple siblings and early propagation exits. Simple dead-branch expressions are checked separately. A simple immutable borrow passed to `isOk` leaves an unrelated sibling's obligation intact. | General aliases, guard and match commitment, lexical entry to unreachable regions, opaque containment and unyielded iterator elements. |
| List and iteration | Linked Rust storage tests exercise non-copying values and borrowed access. The checker records List construction and exact-`u64` index projections. `get`, `iterate` and `next` retain conditional selections. | Protection clearance, completed iteration binding and flow, and representation and copy/dispose contracts. |
| Formatting and conversions | Pinned primitive formatting and numeric infallible `into`/`from` check selected operations and distinguish protected numeric gaps from missing providers. | Exact-E fallible conversions, remaining parsing and encoding pairs, general dispatch and formatting, and missing comparison providers. |
| Native ownership actions | Installed tests verify a build-produced archive for selected shared-owner, mutex guard and channel endpoint actions. They distinguish a provider from protection clearance, reject overlapping replacements, and retain captured facts after an archive change. | Complete inventories, native retention for the remaining contracts, and source-level calls that require those inventories. |

### Independent review and verification

Standards and Spec reviewed `5edc2d4da82a9032ea1116674fb4060e4224ce5e` against `522b86b2046bf19751fa4fa2b8bb3706382eb8c8`. The binary diff SHA-256 is `48e097a8574f3697e0edd131a25f4e28cd168beccfb8f0a4759f560107892686`.

- **Standards:** Moved host error-storage tests to their runtime crate and corrected the documented test scope. The review also suggested sharing role classification, artifact capture and probe setup; those consolidations were not needed for the tested corrections.
- **Spec:** Regressions cover pending opaque List containment, an independent sibling beside an immutable alias, a dead branch with a discarded Result call, a noncanonical operand when both roles are known, and a refutable guard with an error name. The corrections preserve missing evidence instead of using the empty List source shape, and reject the proved invalid forms. General alias and dead-source flow remain partial.

The locally verified correction is `c5703e7ee78c74ca584a6d79fe5c915c7e318de5`. Its binary diff from the reviewed revision has SHA-256 `ec812c70a7d3fc3800a0077fc6563949d9e008f53735b762aa49c1d23a7d7832`. The combined probe and corrections were not independently re-reviewed.

| Check | Result on macOS 27.0.1 AArch64 |
| --- | --- |
| `mise run test:compiler` | 388 passed. |
| `mise run test` | 402 passed, including four collection and two error-storage host tests. |
| `mise run check` | Passed; 1,826 documentation destinations and zero errors at the code revision. Includes formatting and LLVM-free Clippy. |
| `mise run build:scaffold` | Passed. |
| Python models | 55 passed. |
| LLVM-free compiler dependency tree and diff | Passed. The compiler depends only on the linked collection crate. |

A2 still needs closed role, provider, protection and representation inventories. A3 still needs a complete staged distribution and positive typed-stage receipts in both modes. No Rill execution, ownership permission, native ABI acceptance, hosted result, full M4 acceptance or RFC status change follows from these checks.

## Production-layout probe

The [staging command](../../../docs/development.md#future-installed-toolchain) builds `bin/rill`, its sibling Rust runner and the ownership archive in one locked build. It installs the pinned sources, the built archive and the error-storage source. From a separate working directory, `rill --version` prints `0.1.0-dev`. The runner loads [`main.rill`](../../../tests/installed/typed/main.rill) in both modes on macOS AArch64 with empty options. The probe exercises List construction and indexing, iteration, conversion and formatting, Shared retain, Err wrapping and rendering, startup, and imported expectations.

Additional installed tests check closed Err construction and propagating pipes, borrowed error observation, primitive providers, the numeric and text/byte conversion pairs, and user-defined iterator selection. The Result-use tests check original occurrences through guard commitment, projections, calls, List storage and early exits. For a known owned List traversal, breaking or returning leaves an unyielded Result at the exit; a fully drained traversal does not. An unknown generic value is not evidence for a discard or for absence.

| Check at `6382b4483beb29604b96e8855a103e60206cb1f5` | Local result |
| --- | --- |
| `mise run test:compiler` | 416 passed. |
| `mise run test` | 432 passed. |
| `mise run check` and `mise run build:scaffold` | Passed. |
| Python models | 55 passed. |
| Staged normal mode and test mode | Both reported `Incomplete`. No typed-stage receipt; the runner exited 1. |

The staged report has no scalar body findings or obligations. Result-use checking still has unresolved containment and call boundaries in loaded standard-library roots, including generic callbacks, retained causes in `intoValue`, and the native release bridges. List and iterator copy/dispose contracts and closed core inventories are still incomplete. These checks do not establish ownership, execution, ABI compatibility or hosted acceptance.

This revision is locally verified but has not had an independent Standards or Spec review of the combined changes. The binary diff from `2f3c63d39865b868e92fc89f561f2aa95ef49eb5` has SHA-256 `7368b0a8b95440569f9e4cc2d1969c4be82f1c2164509c999e3e2e066b8ef69e`.

At `2ddbb926204589edcb7b4b4a43555ce9af9b9d55`, `Err.intoValue` takes the payload for return and explicitly acknowledges the retained cause. The installed regression failed at the cause containment check before the source change. After the change it passes in both modes, and dropping a returned Result still reports R0103. All four local mise gates and 55 model tests pass. The staged runner still exits 1 with `Incomplete` in both modes. Each report has 14 Result-use blockers: five callback invocations, five skipped callback environments, and four native release boundaries. All four core inventory blockers remain. This revision has not had independent review.
