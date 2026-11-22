# M5 compiler implementation compared with C, C++, Go, Rust, Zig and Hylo

## Scope and evidence

This report reads draft [PR #73][pr] at [`8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0`][pin].
It compares admitted M5 ownership and cleanup cases and the M4 facts they use. It does not compare language breadth or performance.
In the ri-2s0 GitHub snapshot read for this report, five of nine sub-issues are closed: ri-4yg, ri-cfy, ri-33m, ri-rmc and ri-9k9. ri-785, ri-px1, ri-9m8 and ri-vgz remain open, as does ri-2s0.
Those closures cover named acceptance witnesses, not the full rules suggested by the issue titles.

This is source inspection and a reading of recorded checks, not a rerun. At the pin, the PR records 48 focused M5 tests, scoped Clippy, formatting and whitespace checks.
The [scaffold run][scaffold] passed on four profiles. The installed source-to-MIR tests run on macOS AArch64 and load both normal and test modes.
All four [native jobs][native] failed at the unimplemented `test:runtime` task. No Rill program ran. The evidence does not verify executable cleanup, scheduling, target layout or ABI behavior.

## What the pinned compiler checks

- **M4 type checking.** `analyze_typed` issues a capture-bound `TypedReceipt` only for `Checked`. Canonical types, selected implementations, expression places, and generic or callable evidence feed later analysis. [`GoalFacts`][evidence] separates assumptions, proved evidence and unresolved goals, and records canonical selections and any known arrow origin. [Typed completion][typed-finish] also checks first-party core admission and Result use. Destroying storage does not acknowledge its `Result`. A receipt explicitly leaves ownership and execution unperformed. [Typed report][typed] · [M4 acceptance][m4]
- **M5 place and flow checks.** M4's [`PlacePath`][places] names a declaration and projections. A dynamic list selection gives no exact element identity. M5's [builder][build] resolves admitted paths to logical slots. `resolve_place` handles a root or one fixed member; helpers such as `nested_field` handle particular two-level routes. The builder uses checked identities and control facts to form actions. In [flow][flow], backward `liveness` finds possible future reads; forward `KnownStates` retains distinct states per instruction and reuses a block when a state repeats. `State` and `Held` distinguish initialized storage, carried backing, suspended access and exact descriptor targets. Each feasible state must permit a read. Cleanup retires dependent holders before sources, with reverse declaration order for ties in the supported scope cleanup.
- **MIR publication.** [`lower_body` and `lower_root`][api] check selected bodies and reached direct callees or disposers. Rejection, incomplete admission and resource exhaustion are different outcomes. None earns checked MIR for the affected query. [`analyze_ownership`][graph] inventories startup, functions, expectations and implementations under one budget. Its report distinguishes ordinary MIR, bounded cancellation proofs and unresolved delivery. It has no complete graph state. In the [173-root fixture][scalar-copy], admitting 14 standard scalar `copy` bodies and their selected call edges changed 25 ordinary checked roots to 39, leaving 134 incomplete. These are fixture counts, not coverage percentages for Rill.

[RFC 0007][rfc7] and [RFC 0008][rfc8] remain Draft. The owner confirmed policies such as layered dependencies, partial moves and cleanup order, but the [RFC register][register] separates policy approval from RFC acceptance. An M4 receipt or one admitted M5 route does not fill in general RFC judgments. The current baseline still includes implicit copying for eligible values and scoped infix `?` wrapping alongside postfix propagation. ri-bgy records explicit copying, `wrapErr` with postfix-only `?`, and scoped routine API changes **after M6**, not requirements on this pin.

## Three source-to-MIR traces

### 1. A rebuilt record is a transfer, not partial-base repair

The [record witness][rebuild] uses `Pack { ...base, last: replacement, mark: base.mark }`. `rebuild_record` stages the written operands before changing `base`. `CheckRebuild` checks the full initialized region and screens overlapping loans before commit. MIR drops the old `last` once, installs the stages and moves the completed root to `saved`. The untouched `keep` is neither copied nor dropped on transfer. The custom-disposer twin transfers the outer obligation too. It invokes the selected disposer on `saved` at scope exit, not on `base` during the rebuild. [Rebuild staging][rebuild-build] · [Commit check][rebuild-flow] · [Plain][records-test] and [custom][custom-test] tests

Ordinary field assignment can restore an admitted moved field on a still-live owner. Rebuild admits a fully initialized named non-copying flat record of the same nominal type with primitive fields and at most one observable displacement. A consumed base or an actual read of a moved field yields `R0200`. Trying to fill a pre-existing hole in a still-live base stays `Incomplete`. ri-2n9 owns that decision. The tests do not establish physical reuse of an address.

### 2. A retaining call must check its early exit and its caller's later use

The [propagation witness][retained-exit] takes an exact incoming `&mut &bool` destination, a `&bool` source and a canonical `Option<bool>` gate, and returns `Option<bool>`. The caller stages the three arguments once. `Action::Call` emits `CallRetainingWithOptionBool` and retains the old contents and compatible input backing. It excludes the temporary access reborrow and the independent gate and result. The caller does not narrow this dependency set by inspecting the callee. The reached callee must still check the outward slot at each feasible exit. Repair before `gate?` checks; repair after a feasible `None` exit leaves a body-local `&flag` in caller storage and gives `R0202` at the store, with no caller MIR. A statically `Some` gate removes that failure edge in the fixture. [Flow][flow] · [Installed retained-call tests][retained-test]

The [second-iteration contrast][loop] calls `remember(&mut out, carrier)` before setting `carrier` to `&flag`. Only a later iteration can carry that loan into `out`. The loop and its two-iteration control reject the later write to `flag` with `R0203`; the one-iteration control checks. Replacing `out` with `seed` before the write checks on the zero-, one- and two-iteration MIR exit states. This tests state convergence and last use together, not arbitrary retaining signatures or loop execution.

### 3. Replacement can carry pending cancellation through installation

In the [initialized `Bag` witness][replacement], `incoming` moves to a stage before the old owner's selected disposer runs. That disposer reaches a checked empty leaf while a moved string is still owned by a local. [`ReplacementCandidate::finish`][replacement-flow] verifies the exact leaf identity and the supported continuation through the disposer and remaining-field cleanup. On a yield with a pending request, the proof resumes under deferral, finishes the old disposer, then installs the stage. Without an enclosing mask, a pending request delivers at unmask without another yield. With an enclosing mask it remains pending. A fresh request at the later enabled check needs an actual yield. Terminal cleanup disposes the installed owner once, with no export or ordinary suffix. A vacant destination has no old-disposal checkpoint. [Flow][flow] · [Installed replacement tests][replacement-test]

This is an interval proof, not a proof for an entire enabled frame or for required child/native joins. The [ledger][ledger] also leaves general protected cleanup, opaque callable invocation, guarded pattern commitment, and wider propagation carriers and routes open. Some erased calls stop at M4 for lack of evidence; a checked non-invoking callable transfer can retain backing and cleanup without gaining invocation authority.

## What the other compilers do

### C and C++ with Clang

- **C, WG14 N1570.** Simple assignment stores a compatible struct value without consuming the source. Copying a pointer member does not extend its pointee's lifetime. `restrict` is a program obligation about accesses, not an ownership proof over field moves or callee exits. Clang's `CheckAssignmentConstraints` checks assignment types, `EmitAggregateCopy` can emit a memory copy, and `checkExprLifetimeImpl` diagnoses cases such as returning a stack address. Such diagnostics do not turn C into a loan-checked language. Leaving the scope of an automatic pointer does not free the allocation it points to. [WG14 draft text][c] · [Clang assignment][clang-c] · [Aggregate emission][clang-agg] · [Lifetime diagnostics][clang-life]
- **C++, WG21 N4950.** `std::move` casts. Overload resolution can select a move operation, but the cast itself transfers nothing. A moved-from object still has a lifetime and may owe destruction. This differs from Rill's moved slot. WG21 specifies destruction of automatic objects at scope exit and cleanup of completed subobjects on constructor failure. Clang's `EmitAutoVarCleanups` and `EnterDtorCleanups` lower obligations tied to object types and subobjects. RAII is the useful comparison for cleanup order, not a proof of transferred-member initialization or last-use exclusivity. Exception unwinding is not cooperative cancellation or a `Result` return. [Move helper][cpp-move] · [Classes][cpp-class] · [Scope exit][cpp-scope] · [Exceptional cleanup][cpp-eh] · [Clang local cleanup][clang-decl] · [Destructor lowering][clang-class]

### Go

The [Go spec][go] lets function literals share captured variables. [Pointers to locals can outlive the call][go-lifetime] without transferring an owner or suspending a borrow. The compiler's [`escape.Batch`][go-escape] analyzes a weighted location graph and records parameter-to-heap/result summaries for static calls. An escape can require heap allocation instead of rejection. These summaries answer a placement question, not whether an outward slot has forbidden backing. Garbage collection manages heap storage; `defer` registers an explicit call, not a per-value cleanup obligation. [`openDeferExit`][go-ssa] emits eligible deferred calls on return; the [runtime][go-panic] also services deferred calls on return and panic paths. [`context` cancellation][go-context] signals via `Done`; a `CancelFunc` does not wait for work to stop or insert an ownership exit. Rill's modeled deferral and cleanup should not be inferred from Go's cancellation API.

### Rust

The Reference distinguishes [moves and copies][rust-move] and [closure capture modes from call traits][rust-closure]. `rustc` determines captures in [`upvar.rs`][rust-upvar]. After type checking, [`do_mir_borrowck`][rust-borrow] gathers move paths and infers regions on MIR. The separate [`ElaborateDrops`][rust-drop] pass uses maybe-initialized and maybe-uninitialized places to remove, split or guard drops. This is a close comparison for testing field initialization and cleanup on different edges. It does not prescribe Rill's strategy of preserving distinct reached states or its conservative compatible-source call contracts. Rust's function signatures express lifetime relationships, with elision where applicable; Rill's admitted cases do not expose region parameters. [rustc borrow-check guide][rust-guide] · [Reference lifetime elision][rust-elision]

Rust's [functional update][rust-update] moves or copies the *unspecified* fields from the base. It need not access an overwritten field. That is different from Rill's checked commit on the original logical owner, including disposal of its displaced field before moving the whole owner. In a function body, Rust [`?`][rust-try] lowers to `Try::branch` and an early residual return through `FromResidual`, which can convert a `Result` error. It is not Rill's origin/cause construction or an ownership delivery edge. [rustc `lower_expr_try`][rust-try-lowering]

### Zig

Zig 0.15.2's [language reference][zig] assigns pointer and slice lifetime responsibility to the programmer. `defer` and `errdefer` state cleanup explicitly; `try` returns the error code from an error union on failure. In [`AstGen.tryExpr`][zig-ast], the ZIR error arm extracts the code, emits applicable defers through `genDefers`, and returns; [`Sema.zirTry`][zig-sema] checks and lowers the error-union operation to AIR. This offers a concrete check for edge-local cleanup. Zig does not infer per-owner drops from last-use loans, and its forwarded error code is not Rill's retained cause or a cooperative cancellation request.

### Hylo

Hylo's [language tour][hylo-tour] describes `let` observation, exclusive `inout`, consuming `sink`, and initialization through `set` as parameter conventions. They are not general first-class Rill reference descriptors with retained backing. In the pinned upstream compiler rewrite `hylo-new`, [`upholdExclusivity`][hylo-exclusivity] checks IR accesses and reborrows, [`closeOpenEndedRegions`][hylo-regions] inserts access-region ends from uses, and [`normalizeLifetimes`][hylo-lifetime] checks initialization, inserts deinitialization for live parts and checks `set` parameters on return. The [IR reference][hylo-ir] separates raw IR from legal IR after flow-sensitive checks. Hylo is a comparison for factoring access and initialization checks over projections, not evidence for Rill's implicit indirect retained writes or cancellation runtime.

### Case coverage and payload extraction

Rill M4 checks whether a pattern covers every admissible value. The [known-variant binding][variant] then lets M5 move the owned payload without a runtime tag test. The same binding against the parent enum fails at M4 because it may not match. Rust checks pattern usefulness and exhaustiveness [before building MIR][rust-patterns]. Zig requires coverage in a [`switch` over an ordinary enum][zig-switch] and supports capture of tagged-union payloads. Neither rule implies Rill's specific transfer and cleanup obligations. C's ordinary `switch` over an integer or enum does not require a case for every enumerator. Go's type switch narrows an interface value against listed dynamic types but does not require a closed set of cases. [Rill pattern facts][rill-patterns] · [C switch][c-switch] · [Go type switches][go-switch]

### Two reference compiler checks

These small checks ran on macOS AArch64, outside the Rill checkout. They verify the comparison examples, not the Rill implementation.

- Rust 1.98.0, `rustc --edition=2024`: construct an owned `Pack` with tracked `keep` and `last` fields, then use `Pack { last: new_last, mark: base.mark, ..base }`. Reading `base.last` still succeeds. Dropping the rebuilt value drops `keep` and `new_last`; the original `last` drops at its own scope exit. A separate `Pack` that implements `Drop` rejects the update that moves its owned `keep` with `E0509`. This matches the documented [functional update rule][rust-update] and [restriction on moving out of a type that implements `Drop`][rust-drop-move].
- Apple Clang 21.0.0, `clang++ -std=c++20`: construct an object with three tracked members and throw from the second member's constructor. The trace shows construction of the first and second members, destruction only of the first, then entry to the handler. It shows neither construction of the third member nor invocation of the outer destructor. The successful control invokes the outer destructor and drops the three members in reverse order. This is the [C++ initialized-subobject rule][cpp-eh], not a native Rill propagation test.

## Checks worth adding before widening admission

1. **Test place routes by operation, not by syntax shape.** Pair the same canonical nested field read, move and repair with grouping, a disjoint sibling and an overlapping whole-owner loan. Require identical place identities and cleanup after normalization, or an explicit blocker. Keep partial-base rebuild controls pending ri-2n9; do not generalize assignment repair into rebuild permission.
2. **Cross retaining routes with feasible exits.** Reuse the checked exact destination in a direct call and a supported relay. Cross last use or exact overwrite with repair before, after and on an infeasible `?` exit. Assert the callee's store site, the caller's old/new backing, and the exclusion of the temporary reborrow. Treat unsupported combinations as `Incomplete`, not evidence against their semantics.
3. **Prove transfer and invocation independently.** Compare a known closure and the same environment passed through the admitted transfer-only carrier. Assert backing and one cleanup in both. Require actual typed invocation evidence before publishing an invocation; keep an unrelated `Result` discard as an M4 control. This checks composition without accepting erased invocation by analogy.
4. **Check publication at both query levels.** For ordinary, propagation and delivered exits, check exact cleanup paths and absence of export on delivery. Keep the initialized/vacant replacement and masked/mismatched-leaf controls separate. Compare targeted and graph outcomes, share findings by source identity and verify exact and one-short work, entry and depth budgets. Leave execution checks to the separate runtime gate.

[pr]: https://github.com/Reilley64/rill/pull/73
[pin]: https://github.com/Reilley64/rill/commit/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0
[scaffold]: https://github.com/Reilley64/rill/actions/runs/44742425102
[native]: https://github.com/Reilley64/rill/actions/runs/44742425202
[register]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/docs/rfcs/README.md
[rfc7]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md
[rfc8]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/docs/rfcs/0008-errors-propagation-and-caused-by-wrapping.md
[typed]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/typed.rs
[typed-finish]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/typed/completion.rs
[evidence]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/scalars/generics/evidence.rs#L23-L126
[m4]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m4-8-acceptance.md#verified-closeout-revision
[places]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/scalars/data/places.rs#L4-L17
[build]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership/build.rs
[flow]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership/flow.rs
[api]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership.rs#L727-L836
[graph]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership/graph.rs
[ledger]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#open-acceptance-obligations
[scalar-copy]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#standard-scalar-copy-bodies-and-selected-calls
[rebuild]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#same-type-rebuilding-of-fully-initialized-owned-records
[rebuild-build]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership/build.rs#L4623-L4751
[rebuild-flow]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership/flow.rs#L2267-L2296
[custom-test]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5/disposers.rs#L4
[records-test]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5/records.rs#L3042-L3270
[retained-exit]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#retaining-calls-with-a-feasible-propagation-exit
[retained-test]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5/retained.rs#L3802-L4034
[loop]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#a-retained-loan-first-visible-on-the-second-ordinary-iteration
[replacement]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#deferring-a-pending-request-through-old-disposal-and-installation
[replacement-flow]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/ownership/flow.rs#L1723-L1828
[replacement-test]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5/cancellation/replacement.rs
[c]: https://www.sigbus.info/n1570 "WG14 N1570, HTML mirror; §§6.2.4, 6.5.16.1, 6.7.3.1 and 7.22.3.3"
[clang-c]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/Sema/SemaExpr.cpp#L9265-L9335
[clang-agg]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/CodeGen/CGExprAgg.cpp#L2286-L2382
[clang-life]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/Sema/CheckExprLifetime.cpp#L1486-L1524
[cpp-move]: https://github.com/cplusplus/draft/blob/n4950/source/utilities.tex#L419-L435
[cpp-class]: https://github.com/cplusplus/draft/blob/n4950/source/classes.tex#L1347 "N4950 [class.copy.ctor], [class.copy.assign] and [class.dtor]"
[cpp-scope]: https://github.com/cplusplus/draft/blob/n4950/source/statements.tex#L969 "N4950 [stmt.dcl]"
[cpp-eh]: https://github.com/cplusplus/draft/blob/n4950/source/exceptions.tex#L360 "N4950 [except.ctor]"
[clang-decl]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/CodeGen/CGDecl.cpp#L2204-L2217
[clang-class]: https://github.com/llvm/llvm-project/blob/llvmorg-21.1.8/clang/lib/CodeGen/CGClass.cpp#L1849-L1964
[go]: https://github.com/golang/go/blob/go1.25.0/doc/go_spec.html "Go 1.25 spec: Function literals, Address operators and Defer statements"
[go-lifetime]: https://go.dev/doc/faq#stack_or_heap
[go-escape]: https://github.com/golang/go/blob/go1.25.0/src/cmd/compile/internal/escape/escape.go#L19-L49
[go-ssa]: https://github.com/golang/go/blob/go1.25.0/src/cmd/compile/internal/ssagen/ssa.go#L4579
[go-panic]: https://github.com/golang/go/blob/go1.25.0/src/runtime/panic.go#L576-L591
[go-context]: https://github.com/golang/go/blob/go1.25.0/src/context/context.go#L224-L242
[rust-move]: https://doc.rust-lang.org/1.90.0/reference/expressions.html#moved-and-copied-types
[rust-closure]: https://doc.rust-lang.org/1.90.0/reference/types/closure.html#call-traits-and-coercions
[rust-upvar]: https://github.com/rust-lang/rust/blob/1.90.0/compiler/rustc_hir_typeck/src/upvar.rs#L1-L31
[rust-borrow]: https://github.com/rust-lang/rust/blob/1.90.0/compiler/rustc_borrowck/src/lib.rs#L294-L364
[rust-drop]: https://github.com/rust-lang/rust/blob/1.90.0/compiler/rustc_mir_transform/src/elaborate_drops.rs#L49-L95
[rust-guide]: https://rustc-dev-guide.rust-lang.org/borrow-check.html
[rust-elision]: https://doc.rust-lang.org/1.90.0/reference/lifetime-elision.html
[rust-update]: https://doc.rust-lang.org/1.90.0/reference/expressions/struct-expr.html#functional-update-syntax
[rust-try]: https://doc.rust-lang.org/1.90.0/reference/expressions/operator-expr.html#the-question-mark-operator
[rust-try-lowering]: https://github.com/rust-lang/rust/blob/1.90.0/compiler/rustc_ast_lowering/src/expr.rs#L1936-L1996
[zig]: https://ziglang.org/documentation/0.15.2/ "Zig 0.15.2: Lifetime and Ownership, defer, try and errdefer"
[zig-ast]: https://github.com/ziglang/zig/blob/0.15.2/lib/std/zig/AstGen.zig#L5957-L6020
[zig-sema]: https://github.com/ziglang/zig/blob/0.15.2/src/Sema.zig#L18415-L18476
[hylo-tour]: https://github.com/hylo-lang/hylo-lang.github.io/blob/37baf064a66d6b7a2ecb0634c7ba2f1c5e448835/src/content/docs/docs/user/language-tour/functions-and-methods.mdx#L105
[hylo-ir]: https://github.com/hylo-lang/hylo-lang.github.io/blob/37baf064a66d6b7a2ecb0634c7ba2f1c5e448835/src/content/docs/docs/reference/ir.mdx#L5-L15
[hylo-exclusivity]: https://github.com/hylo-lang/hylo-new/blob/fe703c7dae6c496ad21ae7024957c7e84020ef3c/Sources/FrontEnd/IR/Transforms/IRFunction+ExclusivityAnalysis.swift
[hylo-regions]: https://github.com/hylo-lang/hylo-new/blob/fe703c7dae6c496ad21ae7024957c7e84020ef3c/Sources/FrontEnd/IR/Transforms/IRFunction+RegionClosing.swift
[hylo-lifetime]: https://github.com/hylo-lang/hylo-new/blob/fe703c7dae6c496ad21ae7024957c7e84020ef3c/Sources/FrontEnd/IR/Transforms/IRFunction+LifetimeNormalization.swift
[variant]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/tests/m5-acceptance.md#irrefutable-binding-of-an-owned-variant-payload
[rill-patterns]: https://github.com/Reilley64/rill/blob/8ba3d48d5e20ce28a1ed848cd81572faaa28ddf0/crates/compiler/src/analysis/scalars/data/patterns.rs
[rust-patterns]: https://rustc-dev-guide.rust-lang.org/pat-exhaustive-checking.html
[rust-drop-move]: https://doc.rust-lang.org/error_codes/E0509.html
[zig-switch]: https://ziglang.org/documentation/0.15.2/#switch
[c-switch]: https://www.sigbus.info/n1570#6.8.4.2 "WG14 N1570 §6.8.4.2, HTML mirror"
[go-switch]: https://go.dev/ref/spec#Type_switches
