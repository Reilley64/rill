# M4.4 acceptance record

## Authority and scope

[M4.4 #61](https://github.com/Reilley64/rill/issues/61) belongs to open [M4 umbrella #56](https://github.com/Reilley64/rill/issues/56), in existing draft PR #58. The owner [approved K1 through K4](https://github.com/Reilley64/rill/issues/61#issuecomment-6957273163) at checkpoint `391a6aafadfcded9d33d498056b813a9e393db57`, before model/compiler implementation. The owner later [confirmed option A for simple bindings](https://github.com/Reilley64/rill/issues/61#issuecomment-6959565199). Full RFC statuses remain unchanged.

The owning procedures are RFC [0005](../../../docs/rfcs/0005-types-records-enums-and-inference.md#proposed-m44-data-and-pattern-procedure), [0003](../../../docs/rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m44-match-and-guard-control) and [0007](../../../docs/rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m44-pattern-binding-type-classification). This slice extends the existing scalar/control checker. It requires completed M4.2 input, not scalar-only success on data-bearing source.

`analysis::data::check_data`, `DataReceipt::body` and `DataReceipt::node` are the approved public seams. A successful receipt covers only the supported data/scalar analysis. It is not full M4 acceptance, transfer permission, initialized storage, ownership checking or Rill execution.

## Independent model before Rust

The separate [coverage model](models/m4_4_coverage.py) and its [nine tests](models/test_m4_4_coverage.py) passed before the first Rust implementation. The initial model test failed because the module did not exist. The model enumerates finite declared products; the compiler specializes constructor matrices. Neither implementation uses the other's result as its oracle.

Observed model cases include all 16 Boolean-pair row subsets, nested products, guarded rows, single-row irrefutability, root-empty domains and nested empty-component holes in both orders. The empty components represent canonical never or an inspectable empty enum. Complementary rows and wildcard rows cover; a single refutable Boolean row does not. Missing shapes, ill-typed patterns, recursion and exact/short budgets preserve Incomplete/ResourceLimit distinctions.

These are ReferenceExecuted results on abstract declared shapes. A hole or uncovered witness is not a Rill value. The model does not execute guards, payload transfers, Option/Result operations or user code. The earlier eight-test M4.2 layout model remains separate evidence.

## Direct public-interface tests

All 36 tests in [m4_4.rs](m4_4.rs) cross the data entry/body/node seams using actual M1/M2/M3 source captures and completed declaration receipts. They call no private inference, projection or matrix helpers.

| Requirement | Direct observed assertions |
| --- | --- |
| Construction and context | Named record fields retain source order and canonical field identities. Declared fields and tuple positions provide numeric context. Explicit or immediately established full applications share one substitution, including exact rebuild bases and sibling variant contexts. |
| Qualification and privacy | Enum qualification wins a syntactic collision without fallback. Grouping forces value fields. Public canonical types remain readable without a local type binding. Genuinely private nominals stay opaque; any private field prevents external whole-record construction. |
| Variants and relationships | Unit, tuple and record payload construction retains actual variant identity. Explicit admission records owned/read-only relationships. Different variants join at their shared application. Mutable widening, unit-as-call syntax, unrelated nominal shapes and existing-tuple covariance reject. |
| Patterns and provenance | Exact tuple and constructor patterns share irrefutability checking. Borrowed scalars, strings and reference descriptors retain their distinct static types. Binder facts preserve original source types and canonical field/dereference/payload/tuple projections. Failed binders retain identities and blocked causes. |
| Simple bindings | A simple name keeps the initializer's value type. A string field read does not silently become a borrow or an implicit copy. An explicit borrow has reference type. These scoped facts do not authorize moving a non-copyable value out of borrowed storage. |
| Classification | Symbolic T remains non-implicit-copyable in its body. Unresolved aggregate custom-disposer/protected-capability classification returns Incomplete without an owned/borrowed fallback or false missing-name findings. |
| Scoped views | Qualified nested patterns refine canonical named-root field/dereference paths. Views retain original declarations/types, exact pattern sites and tag-stability requirements. They do not escape arms or apply to repeated computed calls. Same-tag payload replacement can pass; ancestor/tag-changing replacement rejects. |
| Coverage | All 16 Boolean-pair subsets agree with the finite expectation. Both never/empty-enum payload orders retain the other Boolean column. Complementary rows cover; one refutable row does not. Root empty enums cover vacuously. Guarded rows, including true guards, never establish coverage. |
| Scalar literal domains | All u8 values establish finite coverage without enumerating large integer domains. A sparse u128 match remains uncovered. Static templates reuse decoded text. Coverage failures retain symbolic witnesses rather than fabricated runtime values. |
| Exit composition | Outward returns, breaks and continues survive calls and other enclosing expressions. Loop conditions use actual loop targets. Shadowed arms and skipped logical operands contribute no executed transfers, but written invalid bodies still reject. |
| Core prerequisites | Same-spelled Option does not grant a core role. Guard success names do not become false absence findings after missing admission. Uncontextualized brackets do not retry as tuples. Unresolved patterns do not become negative coverage evidence. |
| Capture and resources | Both expectation modes use original Unicode/CRLF captures after disk changes. Repeated statistics agree. Foreign queries and invalid node indices reject. Exact limits pass; one-short limits return ResourceLimit. A 2048-operation flat match arm completes at depth eight. |

Positive admitted List/index/Option/Result checking is not observed. It remains blocked on actual canonical core admission inside M4.8. General field/payload inference, defaults, family selection, callbacks and aggregate classification retain their explicit later-M4 prerequisites. No standard-library data definitions or shape-based capabilities were fabricated.

## TDD evidence and limits

The first Rust tracer failed for the absent data module and missing expression data facts. Subsequent failing tracers preceded field projection, established generic rebuilding, enum payloads, contextual tuples, irrefutable tuple/constructor bindings, match checking, nested views, borrowed projection classification, pattern provenance, explicit core prerequisites, symbolic witnesses, static templates, member-result anchors and sibling-application context. Logs are local evidence, not committed execution transcripts.

The transfer-composition fixture initially used a bodyless family header where the approved scope required a standalone function. Its first failure was FamilySelection, not a reproduced transfer defect. The fixture was corrected to an actual standalone body. No semantic red-first claim is made for that fixture. The simple-binding probe initially assumed option B; the owner clarified option A before that rule changed. Its final assertions record A, not an implementation of B.

Additional boundary tables characterize implemented behavior. They are not claimed as separate red-first implementations. No M1/M2/M3 tests, earlier M4 tests, dependencies, locks, workflows, product routes, prototypes or experiments changed.

Default budgets remain 1,000,000 work units, 100,000 charged entries and depth 64. Temporary queues, matrices, substitutions, cloned paths, source witnesses and exit alternatives charge before growth without refunds. Traversal is bounded, with iterative expression/coverage worklists. These are conservative accounting limits, not precise heap or wall-clock quotas or host OOM recovery guarantees.

Canonical types contain no valid Error/Unknown placeholders. Failures expose partial facts and incomplete secondary diagnostics, never a DataReceipt. ResourceLimit precedes Incomplete, then SourceRejected. Unknown copy/core/pattern prerequisites cannot prove absence or noncoverage.

## Verification before final independent review

The first full local verification passed `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold` with 231 compiler tests, including the first 28 M4.4 tests. The documentation checker passed 1658 destinations with zero errors. The combined explicit model command passed 17 tests, split into nine M4.4 and eight unchanged M4.2 tests.

Later capture, flat-chain and sibling-application tests brought M4.4 to 31 passing focused tests at the final implementation review snapshot. All four required mise tasks passed again with 234 compiler tests and 1666 documentation destinations, with zero documentation errors. The models passed separately, nine M4.4 and eight M4.2 tests. Neither checkpoint review is a final implementation review.

The local host remains macOS 27.0.1 AArch64 with mise-selected Rust/Cargo 1.98.0. User-global CMake/Ninja/Go lock warnings did not change repository locks. Native/LLVM, product/runtime/release, hosted/other-host and client acceptance remain unrun for this slice. Existing unavailable product gates remain nonzero. Historical M4.1 hosted evidence and failed native gates retain their exact revision and scope. Formal ASD-STE100 vocabulary compliance remains unverified.

## Independent final review and local corrections

Both independent axes reviewed the 15 staged files against checkpoint 391a6aa. The staged diff SHA-256 was `bea1dc9507038b49656e25bd2760004f6233ee6ad979de748cfc1b7aad013920`.

Standards found zero documented-rule violations and one nonblocking P3 duplication heuristic. Seven nominal-substitution builders now use one private budget-charging helper. That reviewer inspected the staged diff and whitespace checks but ran no tests, models or builds.

Spec found two P2 defects. Implicit field dereference failed to apply an established view at its canonical dereference path before member lookup. Nested matches failed to supply established sibling context before fresh-arm literal/generic finalization. The reviewer rebuilt an index-only snapshot outside the repository, excluding later unstaged consolidation. The reviewer built the LLVM-free library, ran the original 31 M4.4 tests and nine model tests, and reproduced both defects with temporary public-interface probes. These checks did not execute Rill or independently verify historical TDD.

Both public regressions failed before their local corrections. Implicit and explicit dereference now resolve the same viewed place. Nested context preparation checks and caches scrutinees and statement blocks whose checking does not depend on an arm-result context. It collects nested result anchors under actual pattern scopes before finalizing fresh siblings. It neither retries finalized literals nor inspects a selected callee's body.

Follow-up tests cover borrowed binder shadowing, declared return targets and reported depth for empty nested matches. A further red regression caught an unresolved classification being mistaken for successful pattern typing after diagnostic deduplication. Pattern typing now returns explicit failure independently of whether a finding/obligation is newly inserted. This prevents false missing-name or coverage evidence.

All 36 focused tests and all 28 M4.3 regressions pass after correction. The original delegated reviews exclude the consolidation, corrections, added tests and this review record. No second delegated review is claimed. After all corrections, `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold` passed again. The compiler total is 239, including 36 M4.4 tests and the unchanged 203 earlier tests. The documentation checker passed 1666 destinations with zero errors. The separate M4.4 coverage model passed nine tests; the unchanged M4.2 model passed eight. Formatting, LLVM-free Clippy and staged/unstaged diff checks passed. No workflow changed, so check:ci was not run. These local follow-up results do not expand the original delegated review scope or establish any unrun product/host gate.
