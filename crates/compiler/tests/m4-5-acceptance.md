# M4.5 ordinary input analysis acceptance

ri-8p9 belongs to open M4 umbrella ri-b83 and [draft PR #58](https://github.com/Reilley64/rill/pull/58). The owner approved P1 through P4 (ri-8p9, comment 6963439018) at `3392baaac8f2ee50e86167700349273a76ce61c4` before model or Rust work. The [checkpoint](../../../docs/implementation-plan.md#m45-design-checkpoint) records the separate design reviews and decisions. Full RFC statuses remain Draft.

## Independent model

`models/m4_5_inputs.py` compares abstract patterns. `models/test_m4_5_inputs.py` tests symbolic inclusion and intersection against independently generated ground substitution sets. Inclusion uses fresh tuple arities absent from the patterns, including nested and correlated restrictions. Ground enumeration uses distinct pools for probes and witness search; truncating both sides to the same finite pool can hide a containing domain. Initial pool discrepancies were checked against concrete substitutions and corrected before Rust. They were not counterexamples to the procedure.

The initial reference run failed on the missing module. Seven tests passed before Rust began. An eighth test later added abstract required-winner and prohibited-input contrasts. The model grants no family role, protection admission or concrete provider authority. A finite check is neither a universal proof nor Rill execution.

```sh
mise exec -- python -B -m unittest discover -s crates/compiler/tests/models -p 'test_m4_5_inputs.py'
```

## Direct compiler assertions

`m4_5.rs` crosses the ordinary analysis, family query and established input selection interfaces on captured M3 graphs. The minimal prelude declarations supply only the earlier loader and schema fixtures. The tests do not provide invented protected operations, core catalogue entries, conversion roles or trusted providers.

| Procedure | Direct assertions |
| --- | --- |
| Shared header elaboration | Keep the old M4.1 entry incomplete for implementation headers. Solve the complete written signature with rigid implementation binders and leading explicit family arguments. Keep default/requirement metadata and do not repair conflicts or guess unused positions. |
| Declaration prerequisites | Reuse M4.2 validation and retain uncalled duplicate record fields and implementation parameters. Original M4.1–M4.4 tests are unchanged. |
| Collection and provenance | Use actual M3 implementation identities and installed routes. Treat type aliases and renamed binders alike. Ignore unused result binders and retain repeated-variable constraints. Find placement witnesses inside canonical parameter types, not returns or imported spellings. |
| Finite comparison | Check fixed-parent/read-only/mode alternatives and invariant nested positions. Compare whole vectors, detect cross-vector cycles and retain relations and intersection alternative indices. Equal admitted variants can be ambiguous. |
| Input-only selection | Resolve hypothetical winner, absence and ambiguity. Explicit arguments filter through retained substitutions without rewriting priority. Output-only arguments remain pending. The report preserves match/alternative indices and comparison provenance. |
| Boundary cases | Reject bad arity, foreign primitive identities and malformed nominal/variant applications. Bare never and unsupported scoped terms cannot establish an input contract. Check inclusive work, entry and depth limits and retain headers during product exhaustion. |

Rust tests began with a public tracer for the rigid header substitution. Missing seam imports were the initial red. Later tracers exercised prerequisites, canonical keys, finite comparison, defining ownership, selection, explicit argument filtering and metadata. A behavioral red showed an unsupported scoped type producing a hypothetical absence; the corrected check keeps the shape pending. The record does not claim every characterization had a separate behavioral red.

```sh
mise exec -- cargo test --locked -p compiler --no-default-features --test m4_5
```

## Completion boundary

M3 does not carry actual canonical core admission. Family role and both protection inventories remain pending for every family. The analysis can report structural duplicates, placement and match relations only under a hypothetical ordinary role. It does not emit authoritative ordinary duplicate/placement or protected-overlap rejection from those facts. It cannot issue a checked call or a completed ordinary collection receipt. Positive validator/provider and negative core catalogue integration remain unrun pending M4.8. The abstract protection test is not that integration.

Source-call context formation, default application, callback checking, symbolic satisfaction, selected evidence and output agreement remain M4.6 work. Conversion applicability, determinacy, coherence and canonical conversion exceptions remain M4.7 work. Core protocols, scoped visibility, other typed obligations and integrated completion remain M4.8 work. No ownership, loan, transfer, implicit-copy execution, initialization, cleanup, editor or runtime behavior is added.

## Verification and review

Before final reviews, `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold` passed locally. The compiler suite had 252 passing tests, including 13 M4.5 tests and 239 unchanged earlier tests. All 25 reference tests passed separately, including 8 M4.5 tests.

The final Spec review covered 18 staged files against `3392baaac8f2ee50e86167700349273a76ce61c4`. The staged diff SHA-256 was `4f934d30c01f994220e6fa845df2851c30036c9dc040030366528bbdc1b18156`. In a temporary index-only copy, the reviewer ran 8 model tests and 13 public Rust tests. Four separate Rust probes reproduced the first four findings below. No Rill ran.

Spec found that a predicate could enter an established type contract, intrinsic requirement targets were lost, failed annotations hid independent uniqueness errors, an entry limit could erase a known family header, and partial selection facts lacked an explicit role premise. Local corrections now reject non-type intrinsics at every type position and retain canonical predicate requirement targets without proving evidence. The shared declaration check also reports duplicates from M3 declarations when typed shapes are missing. Family facts reserve storage before moving a known header. The role premise applies to every selection fact, even before a winner or ambiguity is known.

The final Standards review covered the same staged diff. It found three documented-rule concerns and one duplication heuristic. It ran no tests. Predicate validation and metadata overlapped the Spec findings. The work-budget concern did not reproduce: `Budget::reserve(n)` already calls `step(n)` before storing or copying materialized nodes. A public test with 81 alternatives and a wide tuple confirms exhaustion under a tight work limit despite ample entries. This is a characterization, not a red-first fix. The metadata walk shares installed-binding lookup with the type elaborator. No broader loader refactor was made: its path walk reports different source diagnostics and has separate fallback rules and budgets.

The original reviews exclude these local corrections. The malformed-request, missing uniqueness and lost-header public regressions failed before the correction and passed afterward. The metadata and premise assertions pin the corrected public facts. No second delegated review is claimed.

After the corrections, all four required mise tasks passed again. `test:compiler` passed 256 tests: 17 for M4.5 and the prior 239 unchanged. `check` validated 1680 local documentation destinations with zero errors. All 25 reference tests passed separately, including 8 for M4.5. Formatting, LLVM-free Clippy and diff checks passed. The publication comment identifies the exact revision for these results.

No Rill program ran. `check:ci` is not required for this change because workflows did not change. Hosted and other-host acceptance, native runtime, LLVM backend, product execution, release/client and whole-M4 gates are not demonstrated by these local checks.
