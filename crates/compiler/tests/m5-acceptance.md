# M5 ownership and cleanup evidence

[M5 #66](https://github.com/Reilley64/rill/issues/66) is open. This is a scoped compiler increment. It does not satisfy the whole [M5 exit gate](../../../docs/implementation-plan.md#ordered-milestones).

## Verified input and authority

The input is the M4 merge `b6af60a0a6cc175e386593769fbfd97087d8f04e`. Its tree `3211ffdb3a2ae42ddfb2129f9beb978b3fe249a4` matches [M4 PR #58](https://github.com/Reilley64/rill/pull/58) head `0a924afe92b7077ba2b0e89c9598a667b77d1bc5`. The [installed command](../../../docs/development.md#future-installed-toolchain) returned `Checked` with no core blockers and issued receipts in both modes on macOS AArch64 at the input commit. The [M4 record](m4-8-acceptance.md#verified-closeout-revision) names the original checked revisions and negative cases. This evidence does not extend to other installed profiles, execution, or native ABI acceptance.

The design inputs below use the M4 input commit unless a decision identifies a different snapshot. Only [RFC 0001](../../../docs/rfcs/0001-top-level-expectations.md) is Accepted. No RFC status changes here.

| Input | Use in this increment |
| --- | --- |
| [RFC 0007](../../../docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md) and [five-policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379), snapshot SHA-256 `59d1bca35706045c2f0fda08a1d12847c79f99ceaad8316a17daafe0f7cf4c70` | Track initialized storage, access restrictions and carried sources separately. Preserve stable free-place borrows. Retire dependents before sources, with reverse declaration order for ties. Canonical explicit `dispose` consumes its named operand. |
| [#7](https://github.com/Reilley64/rill/issues/7#issuecomment-6308080484), [#15](https://github.com/Reilley64/rill/issues/15#issuecomment-6853088155), [ADR 0002](../../../docs/adr/0002-visible-ownership-without-lifetimes.md), [ADR 0007](../../../docs/adr/0007-explicit-mutation-with-conservative-borrow-dependencies.md), [ADR 0011](../../../docs/adr/0011-explicit-numeric-widths-and-utf8-string-views.md) | The later #15 decision replaces the immutable-only and return-only dependency model. Keep the distinction between a descriptor slot, its backing source and the suspended parent access of a reborrow. A named `string` moves, even if initialized from a literal. |
| [RFC 0003](../../../docs/rfcs/0003-expressions-bindings-and-control-flow.md), [RFC 0004](../../../docs/rfcs/0004-functions-overloads-and-generic-requirements.md), [RFC 0005](../../../docs/rfcs/0005-types-records-enums-and-inference.md), [#23](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850), [ADR 0015](../../../docs/adr/0015-language-required-library-contracts.md) | Use typed identities and evaluate arguments left to right. Keep invocation mode separate from capture acquisition. Do not treat a partial aggregate as eligible for its outer disposer. Canonical operations require canonical identity and evidence. |
| [RFC 0008](../../../docs/rfcs/0008-errors-propagation-and-caused-by-wrapping.md), [RFC 0009](../../../docs/rfcs/0009-user-implementable-conversions.md), [#29](https://github.com/Reilley64/rill/issues/29), [#23 cause refinement](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850) | Later decisions replace recoverable propagation traces and narrow retained-cause evidence. A conversion or error shape cannot substitute for an ownership transfer contract. Those edges remain pending here. |
| [#16](https://github.com/Reilley64/rill/issues/16#issuecomment-6863910595), [ADR 0009](../../../docs/adr/0009-concurrency-safety-and-cooperative-cancellation.md), [ADR 0005](../../../docs/adr/0005-panics-end-the-program-without-unwinding.md), [ADR 0014](../../../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md) | The replacement tests establish normal-edge order only. Cancellation delivery must be deferred through old-value disposal and installation. Fatal exits do not unwind. No tail-eligibility or protected-release claim follows. |
| [RFC 0010](../../../docs/rfcs/0010-diagnostics-and-reporting.md) and [diagnostic codes](../../../docs/diagnostic-codes.md) | Attach source sites to findings. `R0200` names a transfer and a later use. Access, escape and release findings use `R0201` through `R0203`. No CLI, JSONL or LSP acceptance follows. |

## Compiler interface and procedure

`analysis::ownership::lower_body` takes a `TypedReceipt` and a named source function from the same capture. It returns a finding, an incomplete obligation, a budget limit or ownership MIR for that body. It checks every source callee reached by the supported direct call graph before it publishes MIR. Recursive calls use declared contracts provisionally until the checker verifies every reached body. This does not issue a whole-program ownership receipt.

The builder uses M4 place paths, declaration identities and Boolean facts to form a control graph. It puts scopes on normal, return, break and continue edges. The backward pass computes possible future reads. The forward pass remembers distinct states at each instruction. A read must succeed on every reached state. A repeated state reuses its MIR block, so a loop does not have to be unrolled to a fixed number of iterations. Budget exhaustion never exposes a partial MIR.

A copied descriptor keeps its backing loans. Borrowing a descriptor slot also retains access to its stored references. A reborrow can restrict access through its parent without using the parent slot as backing. Calls in this slice support standalone application bodies and primitive or primitive-reference arguments and results. The caller retains possible input sources for a reference result. A primitive result retains none. Nested mutable destinations are not classified by this rule.

Normal cleanup omits moved storage. A scope batch orders initialized holders before backing sources, then breaks ties in reverse declaration order. A replacement computes the incoming value before displacement. It drops the displaced value only if the old slot is still initialized, then installs the incoming value. Retiring a storage version removes its obsolete loans before the slot can receive a new value. The MIR names obligations; it does not encode target layout or execute code.

## Public tests

On macOS AArch64, [`m5.rs`](m5.rs) stages first-party sources and linked artifacts. It loads the graph in normal and test modes and requires a typed receipt before querying the compiler interface. The installed test does not run on other profiles. Each test family first failed against the prior increment.

| Issue | Witnesses |
| --- | --- |
| [#67](https://github.com/Reilley64/rill/issues/67) | `split` cleans `taken` on the move edge and `text` on the other edge. `bad` rejects the post-join use. `moved` returns without dropping the transferred owner. `discardOwned` drops the moved value in the wildcard's temporary. |
| [#68](https://github.com/Reilley64/rill/issues/68) | `lastUse` accepts a move after the last descriptor copy. `liveAlias` and `exclusive` reject conflict with a live holder. `forward` returns incoming backing; `ownSlot` cannot return body-local backing. `leavesScope` rejects a still-live borrow on inner scope exit. `nestedView` preserves a descriptor's stored backing. `descriptorSlot` rejects return of a reference to the descriptor slot itself. |
| [#69](https://github.com/Reilley64/rill/issues/69) | `callRetainsBoth` rejects a move of the second possible input source without inspecting the callee body. `callNoBorrow` permits a later replacement. `resumeParent` and `suspendParent` distinguish ended and retained reborrows. `releaseScalar` proves explicit canonical consumption. `groupedBorrow` accepts a grouped fresh loan. `transitivelyBad` rejects the callee's local escape without publishing caller MIR. `recursive` checks a recursive edge. |
| [#70](https://github.com/Reilley64/rill/issues/70) | `repeatMove` fails on a backedge. `restore` reinstates initialization and keeps normal drop before install. `leave` cleans only the initialized iteration locals, in reverse declaration order. |
| [#71](https://github.com/Reilley64/rill/issues/71) | `ownedCapture` leaves one captured string obligation. `consumedCapture` rejects use of the moved source. `copiedCapture` keeps a scalar source initialized. `escapingCapture` rejects a stable borrow of a body-local owner. |
| [#72](https://github.com/Reilley64/rill/issues/72) | `ordered` retires an earlier-declared dependent before its later-declared source. `reinitialized` does not let the old view order the replacement owner's cleanup. A typed `List<bool>` body reports `Incomplete`. Cross-capture queries are invalid. Exact work and entry limits succeed; one-short limits issue no MIR. |

The reborrow test exposed an M4 handoff issue. Mutable primitive calls removed saved Boolean facts and treated their loss as unresolved Result storage. The change discards those Boolean facts without a blocker and keeps the blocker for every other affected fact. The installed tests still require the M4 receipt before invoking M5.

## Independent review and verification

Standards and Spec independently reviewed `068bd37bf7ca10e2e462556a06c3dc1dbeb26eff` against the M4 input. The checked code revision is `9838f3729b27e2b199e553f0ed1ba94ef0df608f`. Its binary diff from the reviewed revision has SHA-256 `2fb6336176f6bb805ba4f73aa47dfbc2fa13ce813090b7d90cb47da58bb08b52`. The corrections passed the tests below and were not independently re-reviewed.

| Axis | Findings and corrections |
| --- | --- |
| Standards | Charge branch-state copies before allocation, and share cleanup classification across boundary and replacement paths. The tests now require a replacement drop, include drops inside scope batches, and require exactly one captured-string drop. This record separates code checks and unrun product gates. |
| Spec | Carry stored backing through nested descriptor borrows. Verify reached direct callee exits before returning caller MIR. Remove retired storage dependencies before reinitialization. Classify a grouped fresh borrow as fresh rather than as an existing mutable descriptor. Public regressions cover all four. |

| Check at `9838f37` | Outcome |
| --- | --- |
| `mise run test:compiler`, `mise run test` | 424 compiler tests and 440 workspace tests passed on macOS AArch64. These totals include the installed M5 probe, which checks both load modes. |
| `mise run check`, `mise run build:scaffold` | Passed. Formatting, LLVM-free Clippy, tests, tools and fixtures passed. The documentation checker found zero errors across 1,856 destinations. The compiler dependency tree has no LLVM dependency. |
| Installed M4 staging command | Both modes returned `Checked` with zero core blockers. Both issued typed receipts, with ownership and execution unperformed by the M4 runner. |
| Existing Python compiler models | 55 passed. They are not M5 compiler or runtime evidence. |
| [Hosted scaffold checks](https://github.com/Reilley64/rill/actions/runs/44319992775) | All four profiles passed. The first M5 revision failed on profiles without the M4 linked archive. The corrected installed M5 test runs only on macOS AArch64. A scaffold pass elsewhere does not run that probe. |
| [Hosted native acceptance](https://github.com/Reilley64/rill/actions/runs/44319990794) | Failed at the intentionally unavailable `test:runtime` gate. This is not runtime evidence. |

No workflow changed, so `check:ci` was not run. Product build, language, standard-library, runtime and client acceptance remain unavailable. LLVM, provider ABI and Rill execution were not tested.

## Owned record places

[#74](https://github.com/Reilley64/rill/issues/74) extends the same installed source-to-MIR seam. The input remains the verified M4 merge above. The increment starts at M5 publication `9d072a1a8a8657ff6b5af25d9b2b592f3a22af7b`.

The checker admits monomorphic application records with primitive fields. It tracks named roots and an owned temporary projected directly in a return expression. It uses the canonical outer `dispose` family and exact typed input to distinguish default field cleanup from a selected unconditional disposer. Unknown or conditional selection stays incomplete. A whole read requires every field. A field read requires that field. Whole-place access overlaps each field, and distinct named fields of the same owned record do not overlap. The field identity in MIR is the original M4 declaration, not a spelling or field index.

The [approved partial-move and cleanup policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379) require keeping initialized fields after a default partial move. Scope cleanup retires holders before their sources, then cleans the live fields in reverse declaration order. Construction evaluates the written initializers in source order. Replacement drops the old initialized storage before installing the new value. It does not drop fields already moved from that storage.

The [record tests](m5/records.rs) run in both installed load modes:

| Source cases | Observed compiler or MIR obligation |
| --- | --- |
| `splitField`, `movedField`, `movedWhole` | One path consumes `first`, the other keeps it. Both can return `second`. Reading the moved field or whole record rejects with `R0200`, with `pack.first` as the transfer witness. |
| `construct`, `restoreField`, `replaceField`, `replacePartial`, `temporaryField`, `copyRecord` | Written move order differs from reverse declaration cleanup order. Restoring a moved field does not dispose its old value again. Replacement of an initialized field drops it before installation. Whole replacement cleans only the old live fields. Returning a temporary's field transfers it and cleans the other fields. An eligible record copies and leaves both owners initialized. |
| `disjointFields`, `sameField`, `overlappingWhole`, `escapingField` | An exclusive loan of `first` does not block moving `second`. It does block replacing `first`, with the loan as a witness. A live whole-record loan rejects the overlapping move. A field borrow cannot escape the body-local record. |
| `customPartial`, `customConditional`, `customWhole`, `customCleanup`, `borrowedFieldMove` | The compiler rejects a move from a record awaiting its custom disposer, and a move of non-copying data through a borrow. It permits whole transfer without local disposal. At this record-place revision, a required outer disposer returns `Incomplete`, never default cleanup MIR. That incomplete exit does not hide a definite forbidden move on a different reached path. The later normal-edge disposer increment below replaces this limitation for the supported shape. |

A monomorphic application disposer for its own record previously blocked the installed M4 receipt. The typed handoff now checks that body under the closed installed role inventory. Canonical identity, ordinary placement and compatibility, duplicate detection and the separate protection gate still apply. The alias in the fixture exercises the canonical family. A separate capture with a second implementation under the canonical name produces `R0304` without a call and has no receipt. The M4 receipt still grants no ownership permission.

### Record-place review and verification

Standards and Spec independently reviewed `737088ad264ee8977161f28d6b8c4d1f24b8960f` against `9d072a1a8a8657ff6b5af25d9b2b592f3a22af7b`. The checked corrections are at `b35d2503b696d84072c8e121bd8d329b01a9a76f`. The binary diff from the reviewed revision has SHA-256 `160ad3022e59b9effd83053cf0063d46adc875e0d97020a0421cd83357a21202`. The corrections passed local checks and were not independently re-reviewed.

- Standards: share primitive ownership classification and cleanup dependency lookup. Store only the proved custom-disposer flag after selection. Record verification separately. The original `check` run also caught needless borrows in the probe; the corrected run passed.
- Spec: add direct return of an owned temporary's field, a same-field access conflict, and the duplicate-disposer receipt denial. A separate regression proves that incomplete custom cleanup on one path does not hide a forbidden partial move on another.

At `b35d250` on macOS AArch64, `mise run test:compiler` passed 425 tests, `mise run test` passed 441, and `mise run check` and `mise run build:scaffold` passed. The documentation checker found no errors across 1,857 destinations. The installed M4 runner again issued typed receipts in both modes. The M5 probe checked both modes in both test groups. No Rill code ran.

No workflow changed, so `check:ci` was not run. The hosted scaffold runs for this revision were pending at recording time. Product build, language, standard-library, runtime and client acceptance remain unavailable. LLVM and provider ABI acceptance were not run.

## Carried references in owned fields

[#68](https://github.com/Reilley64/rill/issues/68) starts at M5 publication `e62a85da1107bd9ddb0c05c7bdbbd2091ae45523` on the same M4 input. The [approved #15 decision](https://github.com/Reilley64/rill/issues/15#issuecomment-6853088155) permits returning contained references while forbidding escape from body-local storage. The [layered dependency model](../../../docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#layered-dependency-model) explains why `x.value` and `&x.value` have different dependencies. This increment does not accept the full RFC or establish a nested destination contract.

Flat application records with default field cleanup can contain read-only references to primitive values. A descriptor field stores its carried sources separately from the slot holding it. Reading or reborrowing through a reference field preserves the record's possible carried sources, not a borrow of the record's slot. Taking the field's address adds that slot as actual backing. A whole-value use keeps its initialized reference fields live. Replacement changes the destination field's stored reference without erasing copies held elsewhere. A call with an owned record argument transfers or copies the argument as usual. A reference result conservatively retains the input's carried sources, not the argument's local record storage. The checker still verifies reached callee bodies before publishing MIR.

The [field-route tests](m5/field_routes.rs) require a typed receipt and exercise the public ownership seam in both installed load modes:

| Case | Checked result |
| --- | --- |
| `forward`, `forwardAndClean`, `escapes` | A stored descriptor copies out and returns. MIR retires the local record and drops its owned text, if any, without disposing the caller's backing. Returning the address of a descriptor slot rejects with `R0202` and points at `&x.value`. |
| `escapeLocal`, `escapeInRecord` | The compiler rejects both projecting a reference to a body-local source and returning the containing record. |
| `callerForward`, `callerEscapes`, `callerRetains` | The caller hands off the owned text, keeps any reference result's carried sources, and rejects a local-source escape or release while the result still needs it. |
| `fieldReplacement`, `otherHolder`, `possibleSource`, `futureProjection`, `borrowHolder`, `retainsExclusiveRestriction` | A proved replacement retires the old field's reference, not another holder's copy. Reference extraction retains possible sibling sources. Borrowing the whole owner retains its contents as well as the record slot. Copying a record with a shared view does not erase the exclusive restriction retained by that view. |
| `groupedReborrow`, `reborrowEscape`, `reborrowAfterRelease` | Grouping preserves successful reborrow of incoming backing. A field reborrow cannot discard possible sibling sources at escape or release. |
| `storageBeforeSource`, `branchRetains`, `loopEscapes`, `loopClears` | Cleanup retires the retaining record before its source despite declaration order. A branch or loop exit does not erase a live source. A backedge after replacement does not revive the old source. |

The checked subset does not include borrowed record projections, exclusive or nested reference fields, reference-bearing custom disposers, record-valued call results, or retained mutable destinations. Record grouping does not establish field orders for internal or mutually dependent record cleanup. `groupedOnly` confirms that an unresolved grouped cycle returns `Incomplete`, not a successful cleanup or a new language rejection. The [#69 preflight](https://github.com/Reilley64/rill/issues/69#issuecomment-6982582460) separately identifies a typed-receipt blocker for writes through `&mut &bool`. Its finite reference checks do not grant an ownership result.

### Carried-field review and verification

Standards and Spec independently reviewed `4f388d7c3dc2f1dccb4552b2e0dbc0e007b01acf` against `e62a85da1107bd9ddb0c05c7bdbbd2091ae45523`. The checked corrections are at `fa3b47744cb0899d60fccf51f27630c89a1b5115`. The binary diff has SHA-256 `b5cb318f1f81373c50558c73190f14ba7a8f809b6319acfb080fbf0ed304d1fe`. The corrections passed local checks and were not independently re-reviewed.

Standards asked for a shared rule selecting the carried-loan root, a test helper that names failing cases, and this verification record. Spec found that direct reborrow missed sibling sources and grouping prevented an otherwise supported reborrow. Public regressions cover the corrected escape, release and grouping outcomes.

At `fa3b477` on macOS AArch64 with `TMPDIR` set to a short, fresh directory under `/tmp`, `mise run test:compiler` passed 426 tests and `mise run test` passed 442. `mise run check` and `mise run build:scaffold` passed. The documentation checker found no errors across 1,859 destinations. Each installed M5 test group checked both load modes. The staged baseline runner still issued typed receipts in both modes. No Rill program ran.

With the default host `TMPDIR`, an earlier `check` failed in the unchanged M3 `long_alias_graphs_terminate_deterministically_and_respect_limits` test at `m3.rs:1065` with `ResourceLimit`. An exact-test comparison reproduced the failure on both `e62a85d` and `fa3b477` with the default path. Both passed with a shorter path. This increment does not change the loader, test limits or fixture. The passing full checks above use the explicit short path.

No workflow changed, so `check:ci` was not run. Product build, language, standard-library, runtime and client acceptance remain unavailable. LLVM and provider ABI acceptance were not run.

## Normal-edge custom disposer cleanup

[#72](https://github.com/Reilley64/rill/issues/72) starts at M5 publication `d5bfb60849f3b801377556d005ac167303fa02ff` on the same verified M4 input. The [approved RFC 0007 policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379) and [#23 remaining-field decision](https://github.com/Reilley64/rill/issues/23#issuecomment-6872129850) govern this slice. The scope is monomorphic application records with primitive fields and a proved unconditional canonical disposer. An aliased registration identifies the same canonical family. The implementation neither substitutes a source name for the selected implementation nor trusts an opaque payload.

A normal cleanup boundary transfers a complete value to its selected outer disposer. The caller's MIR names that implementation and does not also drop its fields. The checker starts the disposer with an active outer value. A field move or explicit primitive disposal consumes only that field. Moving the complete active value within its body preserves the active state. Attempting to pass it as a fresh owned argument rejects. When a local holding the active value ends, MIR transfers its remaining fields to a compiler-only epilogue. This transfer does not extend the source binding's lifetime. The epilogue runs after normal disposal-body work, cleans only the remaining initialized fields in reverse declaration order, and does not invoke the active outer disposer again.

The root MIR exposes reached disposer bodies by implementation identity. The bounded worklist validates direct calls and selected disposers before publishing any MIR. An incomplete, rejected or resource-limited body prevents publication. This is a normal-edge check, not a cancellation, unwinding, ABI or execution claim.

The [disposer tests](m5/disposers.rs) require installed typed receipts in both modes:

| Case | Checked source or MIR obligation |
| --- | --- |
| `normal`, `customCleanup` | An inner local cleans before parameters. Reverse parameter order puts the later independent owner before the selected outer disposer. Its body drops the moved local once and skips the explicitly disposed field. An empty disposer still cleans its remaining fields. |
| `replace`, `replaceField`, `split`, `ordered`, `cleanResult` | Replace a whole owner or field by cleaning the old value before installation. A direct call may return a supported borrow-free record for cleanup. On the active body's early return, skip the field moved on that edge. The other edge cleans both initialized fields. Ending an inner active alias does not drop its fields before a later explicit disposal in the body. |
| `triesEscape`, `triesInvalid`, `liveFieldLoan` | Reject consuming the active outer value after a local move, rereading a moved field inside a reached disposer, and leaving a still-live field borrow at cleanup. No MIR is published. |
| `protectedCleanup`, one-short work | A `LockGuard` wrapper does not get successful cleanup from an empty custom body. The protected payload remains incomplete. An exhausted query does not expose a partial disposer graph. |

Explicit custom-disposer dispatch, replacement of the whole active outer value, non-primitive and protected payloads, partial construction interrupts, and cancellation remain outside this slice. The [explicit-call preflight](https://github.com/Reilley64/rill/issues/72#issuecomment-6983412457) had no M4 typed receipt for the aliased call. The [retained-destination typed blocker](https://github.com/Reilley64/rill/issues/69#issuecomment-6982582460) does not block these normal-edge checks.

### Disposer review and verification

Standards and Spec independently reviewed `14b5f574acd20f6032632ca948fca0c9a77215b8` against `d5bfb60849f3b801377556d005ac167303fa02ff`. The checked corrections are at `760434d54572615fb3f8b9a2bc975c468500f889`. The binary diff has SHA-256 `deab923bbf3d00adde08e7bb9c74e7692d05469d43673ba9e6195a5678c95b9c`. The corrections passed local checks and were not independently re-reviewed.

- Standards: centralize boundary cleanup and outstanding-disposer classification. Carry the reached call kind into lowering. Name the active value's diagnostic origin. Record the actual results below. The first `check` also found an unnecessary closure in the worklist lookup; the corrected run passed.
- Spec: prevent an inner alias from dropping fields before the disposer finishes. Stage remaining fields for the normal epilogue. Cover ordinary custom-record field replacement and cleanup after a borrow-free record-valued call. The source-to-MIR regressions verify all three.

At `760434d` on macOS AArch64 with a short, fresh `TMPDIR` under `/tmp`, `mise run test:compiler` passed 427 tests and `mise run test` passed 443. `mise run check` and `mise run build:scaffold` passed. The documentation checker found no errors across 1,860 destinations. The staged M4 runner still issued typed receipts in both modes. The installed M5 tests ran in both load modes. The short path avoids the separately reproduced M3 budget failure above.

Hosted checks for this revision were queued at recording time. No workflow changed, so `check:ci` was not run. Product build, language, standard-library, runtime and client acceptance remain unavailable. LLVM and provider ABI acceptance were not run. No Rill program ran.

## Fixed owned tuple places

[#74](https://github.com/Reilley64/rill/issues/74#issuecomment-6983811432) extends the receipt-to-MIR check to flat tuples of primitive values and shared primitive references, after ruling out a custom outer disposer. The M4 index fact supplies the constant position. The M5 checker does not reinterpret the selector as a runtime index. The [approved initialization and cleanup policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379) keep per-path storage state separate from carried sources and specify reverse-position cleanup. The [mutation decision](https://github.com/Reilley64/rill/issues/15#issuecomment-6853088155) governs staging and displacement. This check adds no loan-disjointness rule for indexed places.

The installed probe `tuples::fixed_tuple_places_preserve_initialization_and_order` first failed with `Incomplete` for `split` despite having a typed receipt. It then passed in normal and test modes at `ef2493630f5302a7e41a48570e1fdc0fab8c5e2c`.

| Cases | Observed compiler and MIR obligations |
| --- | --- |
| `split`, `usedAfterJoin`, `wholeAfterJoin` | Move position 0 on one edge and return position 1 on either edge. The true edge drops the moved binding and position 2. The false edge drops positions 2 and 0. Reading position 0 or the whole tuple after the join rejects with `R0200` and no MIR. |
| `construct`, `restore`, `replace` | Evaluate elements in written order. Refill a moved slot without dropping it. Replacing an initialized slot drops its old value before installation; final cleanup drops positions 1 and 0. |
| `forward`, `forwardCall`, `temporary` | Copy a carried reference out of the tuple and forward it through a checked direct call. A borrow-free tuple result can feed a return projection. The remaining positions 2 and 0 clean after staging position 1. |
| `borrowElement` | Taking a fresh loan of an indexed slot is incomplete and exposes no MIR. Copying the reference stored in a slot is a separate operation. |

`mise run test:compiler` passed 428 tests and `mise run test` passed 444 at the exact code revision above. `mise run check`, `mise run build:scaffold`, and `git diff --check` also passed on macOS AArch64 with a fresh short `TMPDIR` under `/tmp`. The staged M4 replay still issued both receipts. The separate `let [left, _, right] = value;` preflight also issued both receipts. That preflight has no M5 pattern or cleanup result.

Independent Standards and Spec review requests for `9dda758778e874262c93e9c4fc6fcc25d237afcb...ef2493630f5302a7e41a48570e1fdc0fab8c5e2c` failed with provider HTTP 429. Both reviews remain required. No workflow changed; `check:ci` was not run. No Rill code ran. Product, runtime and client acceptance remain unavailable; LLVM and provider ABI acceptance were not run.

## Irrefutable owned tuple bindings

[#75](https://github.com/Reilley64/rill/issues/75) uses the [approved M4 pattern handoff](https://github.com/Reilley64/rill/issues/61#issuecomment-6957273163). It lowers a flat owned tuple with names and wildcards only after M4 proves the pattern irrefutable and supplies the binder routes. The initializer evaluates once into an owned statement temporary. Each name gets its matched element under the existing copy and move rules. Wildcard elements stay in the temporary for statement-exit cleanup under the [approved ordering policy](https://github.com/Reilley64/rill/issues/28#issuecomment-6915849379). Named bindings remain in lexical scope and carry their dependencies with them.

The installed probe `patterns::owned_tuple_binding_transfers_and_cleans_initialized_elements` first returned `Incomplete` for the typed `unpack` body. At `163338c2dcb84650e3efec09363ea0e6073707a7`, it checks these cases in both load modes:

| Cases | Observed compiler and MIR obligations |
| --- | --- |
| `unpack` | Call the initializer once. Move positions 0 and 2 to `first` and `third`. At statement exit, drop positions 3 and 1. At scope exit, drop `third` then `first`. A one-short work budget returns no MIR. |
| `reuse`, `alreadyPartial` | Reject a subsequent read from the moved source and reject whole initialization from an already partial tuple. Both return `R0200` and no MIR. |
| `copyPair` | Copy the eligible element into `first`. The original tuple stays readable. |
| `forward`, `expires` | Returning a stored caller reference is valid. The wildcard string still drops at the binding statement. Returning a reference to a by-value parameter through the same route rejects with `R0202` and publishes no MIR. |

At that revision, `mise run test:compiler` passed 429 tests and `mise run test` passed 445. `mise run check`, `mise run build:scaffold`, and `git diff --check` passed on macOS AArch64 with a fresh short `TMPDIR` under `/tmp`. The staged M4 replay issued both typed receipts. Independent #75 reviews were requested at `163338c`. The earlier #74 Standards and Spec retries failed again with HTTP 429.

Separate installed preflights for an owned record pattern, a borrowed tuple pattern and an `Option` propagation exit issued both M4 receipts. This increment does not claim an ownership result for those cases. Guard commitment, nested patterns, iterators and cancellation remain open. No workflow changed; `check:ci` was not run. No Rill program ran.

## Open acceptance obligations

- Nested and borrowed place projections, nested mutable storage effects and callee-exit checks for retained destinations. Preserve the covered distinction between a carried reference and a borrow of its slot when extending these routes.
- General aggregate initialization, pattern and guard commitment, partial moves, iteration and rebuilding. Extend the demonstrated remaining-field cleanup to custom disposers over non-primitive fields and to protected obligations.
- Callable invocation, borrowed passing and scoped jobs, multiple or computed captures, and erased/generic evidence. A known single captured value does not prove these modes.
- Typed propagation/wrapping exits, statement-temporary dependencies, protected actions, cancellation edge obligations, and full graph coverage of bodies, startup and expectations. Pending call and provider contracts cannot produce a program receipt.

No Rill program ran. Runtime, native/Wasm support, backend, CLI and client acceptance remain separate. Do not merge this increment or begin M6.
