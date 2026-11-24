# Compiler implementation plan

## State and authority

The owner authorized this plan and a committed documentation baseline on 2026-11-17. Compiler milestone M1 has not started. This plan defines implementation scope and acceptance, not new language semantics or completed validation.

Use the committed baseline containing this plan, not a moving branch, as implementation input. Record its full commit SHA in the implementation ticket before coding. The baseline preserves Draft RFC statuses and their scoped human policy approvals.

ri-48g retains the RFC collection, coverage/readiness and existing formal/production follow-through. Its writing deliverable does not become compiler implementation. Create and claim a separate implementation ticket when coding is authorized.

## Inputs

| Input | Applicable scope |
| --- | --- |
| [Development guide](development.md), [ADR 0026](adr/0026-toolchain-repository-and-mise.md) | Private crate ownership, pinned mise/Cargo/uv tools, LLVM-free analysis and unavailable product gates. |
| Lexical/source approval (ri-48g, comment 6929923825) | Exact source/lexer policies across RFCs 0002, 0003, 0006 and 0010. |
| [Source snapshots](rfcs/0002-modules-and-program-loading.md#source-snapshots-and-lexical-input) and [tokens/trivia](rfcs/0003-expressions-bindings-and-control-flow.md#lexical-tokens-and-trivia) | Immutable original bytes, source spans, identifiers, comments and longest token matching. |
| [Literal scanning](rfcs/0006-numbers-strings-and-primitive-operations.md#literal-scanning-and-token-boundaries) and [coordinate procedure](rfcs/0010-diagnostics-and-reporting.md#original-byte-coordinate-procedure) | Numeric/raw/template modes, controls, decoded provenance and byte/scalar/editor projection. |
| Syntax approval (ri-48g, comment 6931939690), [annex](grammar/v0-syntax-v1.md), [EBNF](grammar/v0-syntax-v1.ebnf) | M2 parser policy. M1 preserves lexer tokens rather than applying parser commitment rules. |
| [Test ownership](../tests/README.md) and [RFC evidence rules](rfcs/README.md#examples-and-evidence) | Component-local tests, later cross-stage suites and truthful acceptance records. |

The syntax policy review used EBNF SHA-256 `158c9b74bb2f36dbf992e454ebc10d57bfac9bb3859b04a394d61d29e0bd94e7`. Its companion SHA-256 was `0d7eeef8bf1abddebf6c04501eaf2572ea0f94f528ac69b933109afb9a3fdbf6`. Later status comments do not change those production rules or extend approval.

Full RFC/annex acceptance remains separate. Return conflicting required behavior to design review rather than adapting the specification silently.

## Ordered milestones

| Milestone | Owning implementation | Completion boundary and prerequisites |
| --- | --- | --- |
| M1. Source and lexer | `crates/compiler/` | Immutable snapshots, validated spans, coordinates and complete lexical behavior pass the acceptance matrix below. No parser or LLVM dependency. |
| M2. Parser | `crates/compiler/` | Apply both syntax-annex artifacts, build source-preserving syntax results and test all commitment/context cases. Recovery policy needs an explicit contract before recovery/client claims. |
| M3. Resolution and declaration collection | `crates/compiler/` | First complete candidate/cycle, namespace, scope and metadata procedures under RFC 0002. Validate canonical loaded-graph behavior independently of editor indexing. |
| M4. Typed analysis | `crates/compiler/` | First complete inference, pattern/coverage, overload/evidence and selected-output procedures under the owning RFCs. Produce checked facts rather than inventing successful types for incomplete analysis. |
| M5. Ownership and cleanup lowering | `crates/compiler/` | First complete dependency transfer, branch/loop, exit, capture and initialized-cleanup judgments. Validate MIR obligations and adversarial accepted/rejected cases. [Work and evidence](../crates/compiler/tests/m5-acceptance.md). |
| M6. Execution and tools (ri-qlh) | Backend/runtime plus IDE/LSP/CLI owners | Implement only after each semantic/ABI prerequisite is complete. Native, Wasm and provider support retain separate gates. Language server work ri-bv2 proceeds where the analysis can support it. It does not gate execution. |

M2 through M6 identify milestone boundaries. The table does not establish implementation readiness or replace the individual tickets. Full standard-library or LSP delivery is not a lexer prerequisite.

Post-M6 ri-bgy tracks explicit copying, `wrapErr` and the [scoped routine API changes](standard-library-contracts.md#post-m6-scoped-routine-names). It requires completed M6 acceptance. Use the verified M6 revision as the implementation input. Do not change the M5 or M6 baseline for this work.

### Acceptance evidence from M6

M1 through M5 keep their acceptance records as written. From M6 on the tests are the evidence: a ticket writes no evidence file and no list of tests or caps, and a milestone writes no acceptance record that repeats what the tests assert. A milestone's matrix maps each row to tests by name, and a script generates that map and fails on a row with no test. What a test cannot hold (what a ticket did not establish, an exception and its reason) goes in a `Not established` comment on the ticket's bead. The naming convention, the script and the note's place are in the [toolchain development guide](agents/development.md), item 4. An older section below that asks for "exact implemented test names and observed outcomes" in a record describes M1 to M5.

## M1 scope and interface

Implement source storage and lexing in `crates/compiler/`. Keep source facts reusable by later compiler and IDE callers. No LSP transport or LLVM types belong in this interface.

Accept explicit source identity and input bytes. Tests can supply synthetic identities without implementing a filesystem resolver or loaded graph. Freeze the original bytes before exposing spans or lexical results.

Expose a small interface for snapshot/span/mapping queries and lexical results. Return token/trivia spans, retained literal spelling/mode data, lexical diagnostics and explicit completion/failure state. Internal storage choices need not become public compiler APIs.

Tests and callers cross that same interface. Private scanner helpers can vary without exposing every state transition. No speculative plugin, alternate lexer or source-provider framework is required.

Use the approved Unicode 17.0.0 property membership, not the host's current character classification. Lock the selected implementation and record its table provenance. Tests must remain offline and reproducible.

Keep numeric magnitudes/spellings exact until later checking. Do not reject a signed-minimum magnitude before its enclosing syntax/type context exists. Literal decoding preserves original-byte provenance instead of changing source offsets.

Malformed input produces truthful diagnostics/invalid data, not an accepted prefix labeled as a valid file. Controlled host/resource limitations remain distinct from source errors. Fatal host exhaustion need not produce a final report.

M1 excludes AST construction, module resolution, type/evidence/ownership checking and user-code execution. It also excludes parser-owned generic/ampersand token views and postfix/infix question-mark selection. Interpolation scanning adds no runtime formatter calls.

## M1 acceptance matrix

Every row needs direct implemented Rust tests with positive, rejected and boundary inputs where applicable. Test names and fixture locations must be listed in the completion record. Placeholder tests and source-inventory counts do not satisfy this matrix.

| Area | Required observations | Existing design cases to translate |
| --- | --- | --- |
| Snapshots and spans | Bytes stay immutable; identity is not reused for different source; ranges validate bounds/order; EOF is exact. Changed caller input cannot alter saved bytes. | RFC 0002 X1 through X4. Controlled size-limit paths are not empty success. |
| Strict UTF-8 | Reject malformed sequences even inside comments/literals. Retain the first invalid byte extent without replacement decoding or fabricated coordinates. | Invalid-source cases plus RFC 0006's established span rules. |
| Identifiers and keywords | Match pinned XID membership, underscore and exact keyword spelling. Preserve distinct normalized-looking names and original bytes. | RFC 0003 L1 through L4/L13/L15/L17. |
| Trivia and punctuation | Preserve BOM, newline/comment spans, nested comments and EOF failures. Match longest punctuation without joining across trivia. | RFC 0003 L5 through L16, lexical portions only. |
| Numeric tokens | Retain full candidates, bases, suffixes and exact spelling. Reject malformed candidates without valid-prefix salvage or width inference. | RFC 0006 S1 through S7. Parser/type conclusions remain outside M1. |
| Text and escapes | Check raw delimiters, fixed escapes, scalar/control admission and physical-content normalization. Preserve decoded-to-original provenance. | RFC 0006 S8 through S19. No runtime codec/conversion validation is implied. |
| Templates | Distinguish text/hole/nested modes, quoted/comment braces, escaped openers and unfinished input. Emit no invented closer or rerun expression. | RFC 0006 S11 through S14/S20 and nested-mode neighbors. |
| Coordinates | Validate byte/scalar/UTF-8/UTF-16/UTF-32 projections, tabs, astral scalars, empty/final lines and unavailable endpoints. | RFC 0010 Y1 through Y5. No live client or filesystem freshness claim. |
| Progress and determinism | Every scan advances or finishes. Repeated inputs produce equivalent token/trivia/error facts; counters/spans do not wrap. | Truncations, malformed delimiters and bounded generated byte inputs. |

Use direct expected kinds, spellings, spans, decoded pieces and diagnostic meanings. Avoid depending on incidental Debug formatting. A focused snapshot can aid review, but must not replace assertions about the required facts.

Keep new component fixtures beside compiler tests, for example `crates/compiler/tests/fixtures/lex/`. That is a planned location, not an existing suite. Preserve historical `prototypes/` and `experiments/` bytes; copy/adapt named cases with provenance when needed.

Broader conformance inputs/runners belong under `tests/` when they cross compiler stages. Neither a fixture-schema check nor an ordinary Rust test count establishes Rill program execution.

## Verification and exit criteria

The planned direct test command is:

```sh
mise exec -- cargo test --locked -p compiler --no-default-features
```

Run applicable fast checks through the existing tasks:

```sh
mise run check
mise run build:scaffold
```

These commands alone cannot certify M1 while its implementation/tests are absent. Existing stub/manifest/CLI tests retain their infrastructure scope. No setup:llvm or native experiment reproduction is required for M1.

Before declaring M1 complete:

- Map every acceptance row to real tests and their observed results.
- Verify LLVM-free compilation and dependency direction.
- Record the implementation commit, host, tool versions and commands.
- Preserve approved policy behavior and original-source positions.
- List unsupported, incomplete and unrun checks explicitly.
- Review the diff for unrelated changes and historical-artifact preservation.

Keep product build, language/stdlib/runtime/client suites and release verification nonzero while their real scopes are unavailable. M1 does not enable a successful `rill check`, compile/run/test command or language server. Parser tests later do not prove complete source checking either.

## M3 design checkpoint

This records Stage 1 of M3 ticket ri-1fk. The owner subsequently approved checkpoint a41e7cc (ri-1fk, comment 6939478619), authorizing Stage 2 within O1 through O7 and the three public seams. The earlier M1 planning statements remain historical. M1 and M2 have since merged through [#51](https://github.com/Reilley64/rill/pull/51) and [#53](https://github.com/Reilley64/rill/pull/53).

The implementation branch is `implementation/m3-loader-resolver`. Its exact starting commit is `04fd25ff9e8b4e02621802a111263947dbeb53b0`, fetched from origin/main after #53 merged. That commit contains M1 `7379a28` and M2 `6ea5fdd`. The original plan baseline is `e1559249c325c3898f506b271e91d76b884be290`. The ticket links the plan at the exact implementation start.

[RFC 0002's proposed procedure](rfcs/0002-modules-and-program-loading.md#proposed-m3-resolution-procedure) owns candidate discovery, route reduction, namespace installation, identities, cycles and graph capture. The checkpoint approval covers these scoped additions to a Draft RFC. Its existing identity/order, lexical/source and syntax approvals remain unchanged. This approval authorizes only the identified M3 contract, not the complete RFC set or ri-48g's readiness obligations.

### Owner choices before coding

The owner approved all seven recommendations below at checkpoint a41e7cc. A departure requires a revised procedure and checkpoint, not a silent code substitution.

| Choice | Recommendation | Consequence or alternative |
| --- | --- | --- |
| O1. Candidate dependency membership | Parse every applicable filesystem-prefix facade and retain it as a required resolution dependency. Traverse exports without filesystem-child fallback. | Even a parsed facade without a winning export contributes declarations and expectations. Retaining only winning-route sources needs a different graph-membership rule and tests. |
| O2. Closure and cycles | Use monotone candidate sets, then classify unresolved alias components. Accept cycles anchored by canonical declarations or modules. | No traversal-order winner. Distinct anchors remain ambiguous. Empty alias cycles get R0405 rather than missing-name errors. |
| O3. Import transactions | Treat repeated same-target bindings as idempotent. Stage every grouped member and both namespaces together. Publish imports only after every M3 obligation and input replay succeeds. | This chooses whole-graph publication atomicity. Failed results expose provisional facts but no installed imports, so failed exported imports cannot supply downstream installed bindings. |
| O4. Graph capture | Retain immutable per-file snapshots and replay all positive/negative path and byte observations once before completion. Return SourceChanged on observed differences, without retries. | This is not simultaneous filesystem capture or a current-live-filesystem guarantee. Stronger coherence needs a different capture mechanism. |
| O5. Host uncertainty | Treat unreadable competing routes, broken links, filesystem link loops and unrepresentable exact spellings as incomplete input failures. | Do not silently ignore a route and declare another unique. Stable absence remains a source-level missing import. |
| O6. M3 checking boundary | Resolve module/import/export bindings and implementation-family targets. Inventory all loaded declaration syntax, member privacy and nested scopes. | Defer expression/type/local-use resolution, typed pattern checks, captures, layout, overloads, conversions, coherence and protected operations. Include syntactic module-startup binders in collision inventory. |
| O7. Explicit prelude and standard inputs | Require a selected standard root and explicit prelude description. Use compiler primitive/predicate identities and supplied canonical copy/dispose family declarations. | Tests provide minimal standard fixtures. Missing or structurally incompatible required prelude declarations reject the request. Typed signature compatibility remains unchecked. No empty-prelude success mode, shipped stdlib, implicit helper imports or installation discovery. |

O7 selects known declarations, not trusted runtime implementations. A path under the selected standard root cannot authorize protected actions or native providers. Public prelude selection cannot mint trusted operations.

Implementation choices do not need new language-policy approval. These include flat storage, private work queues, memoization containers and helper-file boundaries. Proposed budget defaults are 4,096 modules, 64 MiB per source, 256 MiB total retained source bytes and 10,000,000 resolver work units. Limit probes to 100,000 and inspected directory entries to 1,000,000. Keep existing M2 limits as explicit parser inputs. Charge validation reads too, with at most twice the total retained-source allowance for cumulative capture/replay bytes. Any exhausted budget returns ResourceLimit, not source rejection. These numbers are implementation limits, not valid-program limits or host-support guarantees.

### Proposed public interfaces and TDD seams

The owner approved the following behavioral interfaces. They are now implemented in `crates/compiler`, with actual tests mapped in the [M3 acceptance record](../crates/compiler/tests/m3-acceptance.md). Rust names can change without changing these behaviors. Keep filesystem mechanics behind a small compiler interface. Do not introduce a mock filesystem or editor-provider framework for this milestone.

| Proposed operation | Inputs and observations | What tests catch | What it does not establish |
| --- | --- | --- | --- |
| `load(request)` | Entry path, selected standard root, explicit prelude selectors, target/options and limits. Return a resolved graph or configuration/source/input/change/resource failure with explicitly partial facts. | Real path behavior, M1/M2 reuse, closure, public/private candidates, atomic import installation, declaration provenance, cycles and deterministic order. | Body legality, typed evidence, execution or live-client correctness. |
| `graph.lookup_export(module, namespace, segments)` | Exact export qualification from an established module identity. Return canonical target or missing/private/ambiguous outcome with provenance. No filesystem access. | Qualification, alias identity, defining-module ownership and no child autoload. | Expression typing, constructors, field access or overload selection. |
| `validate_inputs(graph, limits)` | Recheck the retained capture's roots, path observations and source bytes. Return unchanged, changed, input failure or resource limit. Keep the graph immutable. | Deterministic source-change tests between capture and validation, including new competing files and link retargeting. | A filesystem transaction or proof that no change happened and was restored between checks. |

Expose read-only canonical module/declaration/binding records, import outcomes and snapshots through these operations' results. Failed import outcomes must expose no installed half-binding. Findings retain source spans, canonical candidate identities and route edges. Do not expose private scanner or fixed-point states just for tests. A resolved result explicitly records later checking as not performed.

Tests must use these same operations as compiler callers. Real temporary filesystem fixtures establish path behavior; mocked path strings do not. Mutation between load and validation needs no timing-sensitive race or hidden test callback. Repeated load requests test deterministic semantic facts, not equality of fresh opaque capture IDs. Permission tests must detect privileged-host behavior and report it as unsupported, not silently pass.

Approval for all three seams and O1 through O7 preceded implementation. The implement and TDD skills govern vertical slices. Write one failing behavior test, observe its failure, implement only that behavior, and rerun the focused suite. Do not write the entire acceptance catalog before implementation.

### Proposed implementation slices

These slices remain one M3 ticket and one M3-only draft PR unless the owner requests separate delivery. No slice independently completes M3.

1. Load one entry and explicit prelude through M1/M2. Add canonical keys, immutable records, roots, exact spelling and explicit host/resource failures.
2. Add reachable imports, declarations and exported qualification. Test grouped exports, both namespaces, provenance and atomic collisions.
3. Add complete competing-prefix discovery and fixed-point re-export/cycle resolution. Test route deduplication, anchored cycles and unanchored aliases.
4. Add observation replay, change detection and canonical result ordering. Complete the acceptance mapping and review all M1/M2 regressions.

Each slice includes its own red-green filesystem cases. Preserve existing M1/M2 source and tests. Any necessary earlier-stage fix needs a separate regression test and explanation in ri-1fk and the PR.

### M3 acceptance checklist, not observed evidence

The names below are planned test behaviors, not existing tests or passing results. Replace them with exact implemented test names and observed outcomes in ri-1fk and the completion record. RFC case numbers here refer to module cases M1 through M15, not compiler milestones.

| RFC case or requirement | Planned direct test through the proposed interfaces | Explicit later-stage assertion |
| --- | --- | --- |
| M1 | `m1_facade_preserves_identity_and_owner` loads the three-file guide and checks aliases, declarations and expectation inventory. | The expectation's Boolean result and execution. |
| M2 | `m2_ungrouped_competition_grouped_export` checks file/type ambiguity and grouped export selection. | Type use after selection. |
| M3 | `m3_import_roots_ignore_local_aliases` tests absent and real A.rill independently. | Expression lexical lookup. |
| M4 | `m4_private_record_metadata_keeps_owner` checks defining module, exported type and private-field records through aliases. | Reject construction, rebuilding and field access with R0401. M3 must not pretend to run these checks. |
| M5 | `m5_generic_requirement_keeps_private_family_provenance` retains written exported generic requirements and the private family declaration's identity/visibility. It does not claim typed requirement resolution. | Typed public-interface requirement checking and its R0401 rejection. |
| M6 | `m6_repeated_routes_collect_one_body` observes one implementation record despite repeated canonical routes. | Typed body validity. |
| M7 | `m7_distinct_bodies_remain_distinct_records` retains both alias-equivalent written patterns and their provenance. | Canonical typed-pattern duplicate rejection with R0304, including renamed binders and differing returns. |
| M8 | `m8_anchored_and_unanchored_cycles` checks declaration anchors, module anchors, multiple-anchor ambiguity, empty alias cycles and acyclic missing exports. | Recursive layout rejection with R0406. |
| M9 | `m9_escaping_links_reject` checks entry, imported file and directory links escaping each origin. | No sandbox or concurrent-host security proof. |
| M10 | `m10_relocated_roots_keep_order` compares semantic records and unsigned UTF-8 module/declaration order across relocated app/std roots. | Execution order and cross-host support. |
| M11 | `m11_expectation_only_dependency_stays_loaded` checks the same complete declaration/implementation inventory for ordinary and test-mode requests. | Later body checking, DCE and dispatch. |
| M12 | `m12_module_and_function_both_install` checks one canonical target in each namespace under one spelling. | Calls and typed expression qualification. |
| M13 | `m13_collision_installs_neither_namespace` checks one-sided collisions, grouped-member failure and zero installation of the failed declaration. | Inner expression shadowing and local-use checking. |
| M14 | `m14_standard_unprefixed_has_no_app_fallback` tests separate roots, explicit std, self, repeated super, confinement and missing standard candidates. | Installed-toolchain discovery. |
| M15 | `m15_public_half_does_not_expose_private_half` tests both public/private namespace arrangements and aliases. | Private field and inferred opaque-value checking. |
| Confirmed identity | `exact_spelling_and_canonical_aliases` uses real case/Unicode entries, internal links, canonical-relative siblings, distinct hard-link paths and overlapping app/std roots. | Other-host results require actual execution there. |
| All applicable routes | `file_export_facade_routes_deduplicate` checks every prefix split, module re-export traversal, same-target deduplication, distinct-target ambiguity and O1 dependency membership. | No source ranking by route or order. |
| Qualification and graph limits | `no_child_autoload_or_unrelated_scan` keeps unrelated malformed files out, yet records explicit candidate-facade dependencies. | Editor indexes and graph-preserving IDE edits. |
| Collection | `all_declarations_and_implementation_targets` checks private/public/nested declarations, enum members, startup/expectation records and canonical family attachments. Reject missing/non-family targets without claiming typed checks. | Orphan ownership, conversions, overloads and protected-operation validation. |
| Input versions | `source_changes_keep_old_snapshots` changes bytes, creates a previously absent candidate, retargets a link and removes a dependency before validation. Earlier spans remain unchanged. | Simultaneous capture and adversarial change/restore detection. |
| Input failures | `host_failures_are_not_missing_candidates` checks missing entry/root, non-regular files, broken/looping links, unreadable alternatives and exact-spelling limitations where reproducible. | Permission and filesystem capability limitations must be reported per host. |
| Completion | `determinism_and_resource_limits` tests repeated and reordered discovery, long/cyclic graphs, exact budget boundaries, replay budgets and propagated M1/M2 failures. No incomplete result is Resolved. | Host OOM recovery, syscall latency and wall-clock bounds. |

Completion also requires full M1/M2 regression tests, LLVM-free dependency checks and unchanged nonzero unavailable product gates. Run the commands in the verification section, plus check:ci only if workflows change. Record actual host/tool versions, limitations, unrun profiles and named acceptance results. No fixture count or resolved graph establishes a successfully checked Rill program.

### Stage 1 observations

On macOS 27.0.1 AArch64, mise 2026.11.4 selected Rust/Cargo 1.98.0, Python 3.13.18 and uv 0.13.8. The unchanged compiler suite passed all 91 tests, comprising two unit, 23 M1, 19 M2 and 47 syntax-case tests. `mise run check` and `mise run build:scaffold` also passed. These observations validate existing M1/M2 behavior and documentation/scaffold checks, not M3. User-level mise configuration emitted unrelated CMake, Ninja and Go lock warnings. No repository locks or tool versions changed.

Workflows did not change, so check:ci was not run. M3 host filesystem cases, other host profiles, product build, language/stdlib/runtime/client gates and release verification were not run. The unavailable commands remain unchanged and nonzero under existing scaffold tests. No LLVM or user code ran. The drafting pass applied unslop and checked local links. Formal ASD-STE100 dictionary compliance remains unverified.

At this checkpoint, M3 implementation and every proposed acceptance test are NotChecked. Full RFC statuses, prior policy decisions and historical evidence remain unchanged. The checkpoint's Standards review found multi-action RFC instructions and misleading agreement/algorithm references. The Spec review found missing rules for failed-export publication, dependent cycle errors and diagnostic-only private routes. The revision splits those instructions, corrects the references and adds a whole-graph publication barrier, component-cause propagation and privacy witnesses. These reviews assess the proposed contract, not completed compiler behavior. Final implementation review, commit, push and an M3-only draft PR follow approval and implementation. Do not merge the PR or begin M4.

### Stage 2 acceptance

The [M3 acceptance record](../crates/compiler/tests/m3-acceptance.md) maps actual Rust tests to RFC 0002 M1 through M15, identity/order policies and the approved scope. Its later-stage column remains unpassed. It also records real filesystem limitations, the additive M2 parser-work statistic and unrun support gates. ri-1fk and the M3-only draft PR retain command results and implementation revisions. A resolved graph is not a successfully checked program.

## M4 scope and design checkpoint

### Starting point and authority

M4 umbrella ri-b83 is a separate implementation ticket from ri-48g. It is claimed by Reilley64 and remains open. No partial slice completes M4.

PR #55 merged on 2026-11-17 at 07:58:38 UTC. After fetching origin, `origin/main` was `ee12588f2cba912025900e1ac587586fb9cb6235`. The fresh branch `implementation/m4-typed-foundation` starts at that exact commit. It includes M3's reviewed source revision `862704bea6af46e1446b3b7942e690f877b116c1` and completion record `d05a0b559e6ee19f2e48d2e0f8b4719469d65f3e`.

The starting commit includes both mise tasks. `test:compiler` runs compiler unit, integration and documentation tests. `test` runs the implemented LLVM-free compiler, IDE, LSP and CLI Rust suites. Neither task executes complete Rill product acceptance.

M1 supplies immutable snapshots, spans, lexical facts and decoded provenance. M2 supplies strict source-preserving syntax without type-directed parsing retries. M3 supplies a captured resolved graph, installed bindings, canonical declarations, family attachments and source routes. Its [public interfaces](../crates/compiler/README.md#m3-loader-and-resolver) and [acceptance record](../crates/compiler/tests/m3-acceptance.md) retain their narrower guarantees.

M3 explicitly deferred expression/type/local-use resolution, signature compatibility, public-requirement checking, captures, layout, overloads, conversions, coherence and protected operations. Those deferrals are current M4/M5 prerequisites, not failed M3 acceptance. Older absent-implementation statements in RFC evidence tables remain historical records.

RFC 0001 remains Accepted at its recorded snapshot. RFCs 0002 through 0010 remain Draft with their existing scoped approvals. The owner subsequently approved O1 through O4 (ri-b83, comment 6943516603) at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`. This authorizes only M4.1 and its three public TDD seams. The original checkpoint claimed no implementation evidence.

### Proposed dependency-ordered slices

Each slice needs its own approved checking procedure, public TDD seam and acceptance record before implementation. The table allocates work; it does not claim that later procedures already exist. Dependencies name the facts required, not permission to begin later work.

| Slice | Dependencies | Scope and acceptance criteria | Rule owners |
| --- | --- | --- | --- |
| M4.1. Canonical explicit declaration types | M1 through M3 | Elaborate explicit declaration annotations into canonical terms with binder substitutions and original provenance. Reject invalid names, targets, variants and excess arguments. Return incomplete for unsupported obligations. Never emit a checked-program artifact. | RFC 0005 type identity and proposed M4.1 procedure; RFC 0004 explicit headers; RFC 0002 source lookup. |
| M4.2. Declaration validity and finite layout | M4.1 | Check field/variant/signature well-formedness and required prelude signature compatibility. Complete and model inline-parameter summaries before implementing them. Distinguish direct/mutual/type-growing cycles from finite nesting, phantom arguments and actual trusted indirection. Check loaded declarations even without uses. | RFC 0005 declarations/layout; RFC 0004 signature forms; RFC 0002 visibility; explicit core contracts. |
| M4.3. Local scalar inference and control targets | M4.1; M4.2 where declaration validity is required | Resolve lexical uses and named-function capture restrictions. Check bindings, explicit standalone calls, contextual scalar/text literals, primitive operations, Boolean flow, returns, loops and expectation boundaries. Check skipped source statically without evaluating it. Separate literal errors from runtime arithmetic traps. Unsupported calls remain incomplete. | RFCs 0003/0006; accepted RFC 0001; RFCs 0002/0004. |
| M4.4. Data construction, patterns and coverage | M4.2/M4.3 | Check record/enum construction and field visibility. Supply declared field literal context. Implement permitted variant relationships, joins and tuple/List category checks. Complete nested constructor coverage and guard fallthrough before checking matches or refutable bindings. Keep ownership and runtime assertions separate. | RFC 0005; RFC 0003 control/guards; RFC 0002 privacy; supplied collection contracts. |
| M4.5. Ordinary family collection and input selection | M4.1/M4.2; M4.4 input relations | Complete finite normalization/subsumption/intersection procedures. Check signature substitution, canonical duplicates and ordinary placement. Preserve correlations across entire parameter vectors. Test crossed ambiguity and static variant/parent selection. Return selection facts without claiming evidence-complete calls. | RFC 0004; RFCs 0002/0005; protected-action contracts where applicable. |
| M4.6. Generic inference and symbolic evidence | M4.3/M4.4/M4.5 | Infer before trailing defaults. Stage arrows and overloaded function values from established inputs. Check generic bodies symbolically and preserve specialization-dependent selected outputs. Complete exact equality and recursive-goal handling. Missing selected ordinary evidence never triggers fallback. | RFC 0004 inference/callbacks/evidence; RFC 0005 contextual categories; RFC 0008 exact error relationships. |
| M4.7. Conversion-specific analysis | M4.6 | Complete whole-pair matching and endpoint permission, duplicate/protected coherence, three-state requirement applicability, output determination and symbolic selected-E equality. Test unresolved winning/tied candidates, identity gates and expected-error nonselection. Do not reuse ordinary no-fallback as conversion applicability. | RFC 0009; RFCs 0004/0005/0006; supplied actual core/error contracts. |
| M4.8. Integrated typed-stage completion | Applicable earlier M4 facts | Finish static result-use checks through named storage and aggregates. Integrate closed Err/Option/Result/iterator checks, public requirement/payload visibility and trusted-core admission. Check all loaded bodies/startup/expectations in both modes. Publish typed-stage completion only after every required typed obligation completes. Ownership and execution remain unchecked. | RFCs 0001 through 0010 within their owning sections; explicit supplied core contracts. |

Source visibility applies in every slice that resolves source names or representation access. Compiler metadata never creates imports. Public generic requirements and payload visibility need their full checks before integrated completion.

Trusted-core checks apply before any slice claims a protected operation or indirect representation. Use actual canonical declarations and explicit supplied contracts. A standard-root path, private fields or matching spelling never grants trust. Do not invent standard-library source to satisfy these obligations.

M4.8 is a completion gate, not permission to hide unfinished work. Every remaining typed obligation must have an owning procedure and tests. M4 excludes ownership/loan transfer, disposal scheduling, MIR cleanup, runtime, codegen, LLVM and editor behavior. No slice enables successful product commands.

### Recommended first slice and exact prerequisites

Start with M4.1. Canonical declaration types provide a useful input for layout, inference and overload work without choosing their unresolved policies.

The proposed procedure lives in [RFC 0005](rfcs/0005-types-records-enums-and-inference.md#proposed-m41-canonical-declaration-type-procedure). [RFC 0004](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m41-explicit-header-elaboration) owns its function-header boundary. The owner approved these two additions at the exact checkpoint recorded above. They do not establish full RFC acceptance.

The other exact inputs are these sections at starting commit `ee12588f2cba912025900e1ac587586fb9cb6235`:

- RFC 0002, proposed M3 identities, installed bindings, exported qualification, prelude and privacy sections.
- RFC 0005, type forms and identity, and parent-applied enum variant spelling.
- RFC 0004, explicit function forms and callable modes. Mode relationships and evidence are not implemented by M4.1.
- RFC 0006, primitive types and width inference. M4.1 uses the primitive identities, not literal inference or numeric operations.
- Both version-1 syntax-annex artifacts, especially declaration/type shapes, finite tuple arities and generic-use syntax.
- RFC 0010, analysis/source identities and original-byte spans. No reporter or new diagnostic-code assignment is required.

RFCs 0001/0007/0008/0009 constrain exclusions and later integration. Their missing execution, ownership and evidence procedures do not block this annotation-only slice. The confirmed finite-layout policy remains required before any representation-validity claim, not before recording a canonical reference to a declaration.

### First-slice input/output contract

Proposed Rust names are descriptive, not committed interfaces:

```text
analyze_declaration_types(&Graph, AnalysisLimits)
    -> declaration-type receipt | rejected/incomplete/resource failure
receipt.declaration(DeclarationId)
    -> read-only binder, field/payload and parameter/result type facts
receipt.type_use(module, NodeId)
    -> canonical term, ordered substitution and source provenance
```

The operation takes only a successful M3 Graph, not raw text, a partial graph or hand-built type terms. It analyzes every module-level record/enum and named function/family header in that graph. It does not cherry-pick a valid declaration while hiding an unsupported in-scope header.

M4.1 supports primitive terms, nominal applications, variants, distinct symbolic binders, tuples, references and written callable modes. Generic applications need full written arguments. Generic defaults, where clauses and implementation headers return Incomplete. Missing arguments also return Incomplete rather than applying unimplemented inference. Excess arguments and unavailable source names reject.

Bodies, startup statements, expectations and declarations inside bodies are outside this requested stage. The receipt lists them as NotPerformed, even when their syntax is valid. Neither an invalid body nor an unimplemented body form receives a checked-body fact. Signature compatibility, position-validity of void/never, layout, coverage, evidence and ownership are also NotPerformed.

Canonical terms retain M3 capture/declaration identities. Binder terms use declaration identity, not spelling. Applications retain ordered binder substitutions. Each written use separately retains its M2 node, M1 Span, canonical target and source-binding route. Equal canonical terms can have different written-use provenance.

Later evidence facts must retain canonical goal/family identities, the shared substitution, selected-body identity and selected input/output relationships. They must link justified assumptions or derivations to their source sites. Proved, disproved and unresolved states remain distinct. Exact selected-output equalities retain both compared terms and their origins. M4.1 produces none of these proofs; unsupported requirements stay explicit obligations.

No semantic operation or trusted representation is admitted in this slice. M3's supplied canonical copy/dispose declarations can have their annotations elaborated. Typed library compatibility remains M4.2 work. M4.1 fabricates no Option, Result, List, conversion, error-wrapper or trusted-core definition.

| Outcome | Meaning |
| --- | --- |
| DeclarationTypesComplete | All requested annotation elaboration completed. The receipt is not a checked program, valid implementation, layout proof or ownership proof. |
| TypeRejected | An implemented annotation rule fails. Preserve original use/declaration spans and canonical candidates where available. No completed receipt. |
| Incomplete | An in-scope default, requirement, implementation header or needed inference is unsupported. Preserve the obligation and partial facts, without proving or disproving evidence. |
| ResourceLimit | A configured checked work/depth/storage budget prevents completion. No source-invalidity or missing-evidence conclusion follows. |
| InvalidRequest | A query or supplied identity does not belong to the retained graph/receipt. Do not confuse cross-capture misuse with a source type error. |

Failures retain known findings and explicitly partial typed facts. They disclose incomplete secondary diagnostics. A canonical-type enum must have no accepted Error or Unknown placeholder case. Missing terms stay missing facts, not fabricated valid types.

Analysis uses retained snapshots without filesystem reads. Later source changes cannot mutate those facts. Callers needing live-input validation use M3's existing validate_inputs operation separately. A receipt never claims continuous freshness.

Implementation choices include arena storage, private lookup indexes and memoization. Proposed defaults are 1,000,000 charged work units, 100,000 retained type/use/substitution entries and 64 traversal levels. Charge before growth, including failed work and provenance copies. These numbers limit implementation work, not valid Rill programs. Host allocation failure and wall-clock bounds remain outside the guarantee.

### Public TDD seams and planned acceptance

Approve the entry operation and both read-only queries together. Tests create real temporary source trees, call M3 load, then cross the proposed analysis interface. Do not synthesize graph identities or test private normalization helpers instead.

| Seam | Catches | Does not establish |
| --- | --- | --- |
| analyze_declaration_types | Whole requested-stage completion, source lookup, canonical annotation rules, unsupported obligations and resource outcomes. | Bodies, evidence, representation validity or product checking. |
| receipt.declaration | Canonical owners/binders and ordered explicit field/payload/signature types. | Implementation-family compatibility or callable permission. |
| receipt.type_use | Alias-preserving identities, substitutions, original snapshots/spans and retained source routes. | Editor navigation, live-file freshness or ownership provenance. |

Each requirement below needs direct positive, rejected and boundary assertions where applicable. Names describe planned behaviors, not existing tests or passing evidence.

| Requirement | Positive / rejected / boundary cases | Provenance and scope |
| --- | --- | --- |
| Nominal identity | Alias/re-export applications share identities; equal-shaped declarations differ; private/missing/non-type annotations reject. Separate captures remain distinct. | RFC 0005 T1/T4 identity portions; RFC 0002 M1/M15. No construction or assignment is asserted. |
| Generic binders and applications | Preserve ordered substitutions and repeated binder identity; reject duplicate binders and excess arguments; missing arguments are incomplete. Separate same-spelled binders do not merge. | RFC 0004 F1 header and correlated-variable preservation prerequisite. No F7 overload result is claimed. |
| Enum and variant terms | Elaborate parent-applied generic variant annotations and payload types; reject absent variants and selection on records. Reject E.A for `enum E { A, A }` with both canonical candidates and spans, never a traversal-order winner. Preserve empty enums and record/unit payload distinctions. | RFC 0005 T3 and the proposed multiple-match lookup rule; syntax G9/G10/G17. No matching or construction is asserted. |
| Structural terms | Preserve tuple order/arity, nested references and all written callable modes; reject wrong source targets; distinguish empty tuple from void and exact callable modes. | Syntax G13/G15 and RFC 0004 callable forms. No mode conversion or capture inference. |
| Primitive identities | Elaborate the actual prelude primitives; preserve source shadowing and compiler-owned identities separately; reject an unknown primitive-like name. | RFCs 0002/0006. No numeric default or same-spelled trusted operation. |
| Whole-stage refusal | Ordinary explicit generic headers complete; defaults, where clauses and implementations yield incomplete; resource limits cannot expose a completed receipt. | RFC 0004 unsupported-procedure boundary. Partial facts remain inspectable only as partial facts. |
| Ambiguity and insufficient context | Qualified aliases resolve after M3; an ambiguous import remains M3 rejection with no analyzable graph; missing generic arguments cannot create guessed terms. | RFC 0002 M2 and RFC 0004 R0301 context boundary. Typed overload ambiguity remains M4.5. |
| Recursive and growing obligations | Finite canonical references to recursive/growing declarations terminate without layout claims. Recursive/type-growing where goals return incomplete without proof search or fallback. | RFC 0005 T13/T15/T16 annotation portions; RFC 0004 evidence boundary. Actual layout and solver outcomes remain M4.2/M4.6/M4.7. |
| Source provenance | Retain Unicode/CRLF use and declaration spans, alias routes and source versions. Cross-capture IDs reject as request misuse; changed live files cannot alter retained facts. | M1/M2 provenance and RFC 0010 source identity. No new simultaneous capture claim. |
| Determinism and controlled limits | Repeat analysis and relocate source roots; compare canonical semantic facts independent of fresh opaque IDs. Exercise exact and one-short work/storage/depth budgets. | Explicit implementation limits. No OOM recovery or universal termination proof. |
| Honest completion | Every completed receipt lists unperformed stages. Unresolved or unsupported annotation obligations never produce one. Bodies with static/runtime problems receive no checked-body fact. | Milestone boundary and RFC 0010 truthful phase scope. No arithmetic evaluation or successful CLI product command. |

After approval, write one failing public-interface behavior test, observe red, implement that behavior and rerun focused tests. Repeat vertically. Do not write the entire catalog before implementation. Use mise-selected cargo check and focused compiler tests regularly. Map approved requirements to actual test names and results in a slice acceptance record.

### Owner decisions requested

The owner approved all four recommendations below at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`. Later slices remain unapproved.

| Decision | Recommendation and approval scope |
| --- | --- |
| O1. First delivery boundary | Approve M4.1 as canonical explicit declaration types, not full signature/body checking. Defaults, requirements and implementations return incomplete. Later procedure gaps do not block this independently useful slice. |
| O2. Source type lookup | Approve binder, module binding, then primitive-prelude precedence. Qualification uses M3 installed exports without child autoload. Multiple distinct same-spelled variants reject an annotation with all candidate identities and spans. Canonical compiler primitive identities remain unaffected by source shadowing. |
| O3. Typed facts and test seams | Approve the receipt contract and three public seams above. Complete means only annotation elaboration. Partial results and unsupported obligations never become checked artifacts. |
| O4. Recursive declarations and layout | Approve nonexpanding nominal terms in M4.1. Defer representation validity to M4.2's complete summary/containment procedure. Do not treat a canonical annotation as a constructible or finite value type. |

Later owner decisions remain local prerequisites, not M4.1 blockers. M4.2 needs trusted representation admission and complete transfer rules. M4.4 needs complete guarded/nested coverage and refutable guard-let behavior. M4.5 needs finite comparison over permitted admission relations and protected-overlap contracts. M4.6/M4.7 need recursive/nonmonotonic evidence, specialization-dependent equality and output-determination procedures. M4.8 needs exact aggregate/named-storage unused-Result checking and complete core visibility/admission contracts. RFC 0006 still lists char relational-operator admission as unfinished policy.

Return these choices to the owner before implementing the affected procedure. Preserve the already confirmed no-default-width, declared-field context, inference-before-defaults, whole-vector correlation, ordinary no-fallback, conversion applicability and expected-E nonselection policies. Resource limits and unresolved goals never prove missing evidence or authorize fallback.

### Delivery and verification after approval

Create and claim an M4.1 slice ticket under ri-b83 after scope approval. Link the exact approved checkpoint and the starting commit. Only the first approved slice is authorized for implementation.

Follow the implement and TDD skills. Run focused tests regularly, then test:compiler, test, check and build:scaffold through mise. Run check:ci if workflows change. Record the host, tools, commands, observed results, unsupported cases and unrun gates. Any M1 through M3 fix needs its own regression and explanation.

Use the code-review skill for independent Standards and Spec reviews against the exact starting commit. Address findings before the implementation commit. Record any local follow-up corrections without implying another independent review occurred.

The initial delivery proposal used separate slice PRs. The owner later requested all eight M4 slices in [existing PR #58](https://github.com/Reilley64/rill/pull/58). Keep that PR draft and M4-scoped. Add each slice only after its required procedure and seam approvals. Preserve separate slice reviews, commits and acceptance records. Link the umbrella and slice tickets. Leave the umbrella open until all typed obligations have acceptance evidence. Do not merge without explicit authorization. Do not add M5 work.

No successful rill check/build/run/test command is authorized. Keep unavailable product tasks nonzero and preserve historical evidence. Typed-stage success is not ownership safety, complete source checking or execution.

### Stage 1 observations and checkpoint review

Only this plan and the proposed sections in RFCs 0004/0005 changed. No production source, tests, workflows, locks or historical artifacts changed. At checkpoint creation, M4 implementation and every planned M4 test were NotChecked. Owner approval was pending.

On macOS 27.0.1 AArch64, mise 2026.11.4 selected Rust/Cargo 1.98.0, Python 3.13.18 and uv 0.13.8. The following commands passed:

- `mise run test:compiler`, all 138 existing compiler tests, including 47 M3 tests.
- `mise run test`, the existing implemented LLVM-free Rust suites.
- `mise run check`, existing Rust, documentation, tool and fixture checks.
- `mise run build:scaffold`, the unchanged CLI scaffold.
- `mise run check:docs`, including local links and whitespace checks.

These are baseline/document checks, not M4 acceptance or Rill execution. User-global CMake/Ninja/Go lock warnings did not change repository locks. Workflows did not change, so check:ci was not run. Other-host profiles, LLVM, product build, language/stdlib/runtime/client suites and release verification were not run. Unavailable gates remain unchanged and nonzero under existing infrastructure tests.

Two independent review axes examined the staged checkpoint against starting commit `ee12588f2cba912025900e1ac587586fb9cb6235`. Staged review preceded the documentation commit. This is a checkpoint review, not the required final implementation review.

#### Standards

One finding identified a multi-action RFC instruction. The local correction splits its three prohibitions into separate instructions. No judgement-call smell finding was reported.

#### Spec

One finding identified undefined lookup for multiple same-spelled enum variants. M3 collects them without establishing member uniqueness. The local correction rejects an ambiguous annotation, retains every candidate identity/span and adds a direct planned acceptance case. Full unused-declaration validity remains later work.

Both corrections were made locally without another delegated review. The prose pass applied unslop while preserving code, recorded evidence, full RFC statuses and prior approvals. Formal ASD-STE100 dictionary compliance remains unverified. Commit/push of an implemented slice and its scoped draft PR remain contingent on approval and Stage 2.

### M4.1 Stage 2 acceptance

Slice ri-11w records the approved M4.1 work. The [M4.1 acceptance record](../crates/compiler/tests/m4-1-acceptance.md) maps real tests to the checkpoint requirements. It preserves unimplemented checks and later-slice boundaries. It does not complete M4 or enable any product command.

## M4.2 design checkpoint

### Delivery and current baseline

The owner requested completion of all M4 slices in existing PR #58. This supersedes separate slice PRs and intermediate merge gates. It does not approve unwritten language policies or unreviewed procedures. PR #58 remains draft and unmerged; ri-b83 remains open.

M4.2 ticket ri-9y3 starts design work from branch commit `f405adaa01058849025105b11ed77fe32f6271b7`. M4.1 implementation revision `7602ed55f8b344646218e8cc0af8089bfee18efb` provides its input facts. The overall M4 starting main remains `ee12588f2cba912025900e1ac587586fb9cb6235`.

M4.1 has completed scoped implementation review and acceptance evidence, including four hosted scaffold profiles. Its integration remains pending as part of M4. No intermediate merge is needed before M4.2 design or subsequently approved implementation. The later documentation commit records hosted results without claiming it was itself already tested on those hosts.

At checkpoint creation, no M4.2 compiler/model code or tests had been written. The owner subsequently approved D1 through D4 (ri-9y3, comment 6945089123) at `c26d737439fd4c426d362655ad15984cc70108dc`. Approval covers both owning procedures, canonical prelude contracts, the three public TDD seams and model-first/compiler-TDD sequence. Full RFC statuses and earlier approvals remain unchanged.

### Owning procedures and exact prerequisites

[RFC 0005's proposed M4.2 procedure](rfcs/0005-types-records-enums-and-inference.md#proposed-m42-declaration-validity-and-inline-containment) owns duplicate members, type positions, inline summaries and containment. [RFC 0004's proposed M4.2 procedure](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m42-header-validity-and-prelude-signatures) owns parameter uniqueness and canonical prelude signature compatibility.

At baseline f405ada, RFC 0005 owns the confirmed summary/containment policy and T13 through T16. RFC 0004 owns explicit headers and fixed reference constructors. The library catalogue section 2 supplies the canonical borrowed-copy and consuming-dispose signatures. RFC 0002 supplies canonical ownership, source visibility and graph membership. RFC 0010 supplies original-source identity and truthful incomplete outcomes.

The callable capture distinction also follows RFC 0007. No ownership, capture-transfer or cleanup algorithm is imported into M4.2. Protected-operation and target ABI contracts remain separate.

### Proposed input/output contract and TDD seams

```text
check_declarations(completed M4.1 receipt, explicit prelude contracts, limits)
    -> scoped declaration/inline-containment receipt
       or rejected/incomplete/resource/request failure
result.declaration(canonical declaration identity)
    -> checked member/header facts with original declaration provenance
result.layout(canonical nominal identity)
    -> ordered inline-parameter summary, containment edges and source witnesses
```

These are behavioral names, not fixed Rust signatures. The request reuses the exact retained graph capture. It cannot consume a failed annotation attempt, invented types or contracts from another capture. Explicit prelude inputs must identify M3's selected copy/dispose declarations and the fixed approved schemas. They supply no runtime trust.

A completed result covers member/header uniqueness, required prelude signatures, the approved type-position rule and source-nominal inline containment. It must not claim full declaration validity or general signature compatibility. Public-interface visibility, implementations, bodies, capture storage, target layout/size, evidence, ownership and execution remain explicitly unperformed.

Each summary retains canonical declaration/binder identities and finite derivation evidence. Each containment edge retains its canonical endpoints, original storage member span and relevant forwarding substitutions. Aliases never create new graph vertices. Failed results retain known findings and explicitly partial facts without exposing a completed receipt.

| Public seam | Catches | Does not establish |
| --- | --- | --- |
| check_declarations | Whole requested-stage completion, canonical contract validation, unused duplicate/cycle rejection, incomplete storage and resource outcomes. | Complete source checking, body/evidence validity, target ABI or ownership. |
| declaration query | Distinct member scopes, source provenance and canonical prelude signature compatibility. | Implementation ranking, protected-operation admission or correct runtime copying/disposal. |
| layout query | Summary forwarding, finite nesting, phantom arguments, canonical edges and cycle witnesses. | Runtime layout sizes, constructibility, capture layout or safety. |

Tests must load real sources through M3, elaborate through M4.1 and call this analysis interface. A reference-model test supplements these seams; it cannot replace their direct compiler assertions.

### Owner decisions requested before code

The owner approved all four recommendations at checkpoint `c26d737439fd4c426d362655ad15984cc70108dc`. The table preserves the exact review choices and their consequences.

| Decision | Recommendation and consequence |
| --- | --- |
| D1. Type positions | Permit void and never in every existing type position. Keep inhabitation and control-flow checks separate. This permits declarations such as UnitHolder with a void field and BottomHolder with a never field, without inventing an initializer or constructible BottomHolder value. |
| D2. Stored variants | Use the parent enum application's inline representation for stored variant types. Thus Choice<Node>.Empty forwards the parent enum's stored Node dependency and rejects the proposed Node example. A selected-payload representation would produce a different result and needs different rules. This is a new policy choice, not prior confirmed behavior. |
| D3. Missing representation contracts | Admit no nominal indirection exemptions in this slice. Actual source references stop traversal; names and private fields do not. Directly stored callables without a justified representation contract return Incomplete. Callable signatures do not become capture storage. Keep that obligation assigned to later callable/core integration before M4 completion. |
| D4. Scope, contracts and seams | Approve the two owning procedures, exact prelude schemas, three public seams and scoped completion boundary. Defaults, requirements and implementations retain M4.1's incomplete behavior. Approve the model-first validation and compiler TDD sequence below. |

The scoped owner decision approves these recommendations for M4.2. It does not accept either full RFC or authorize later unwritten procedures. A changed decision requires corresponding owning-text and acceptance revisions.

### Model validation before compiler implementation

After approval, first write the finite reference model and its direct expected cases. Keep it beside compiler tests. Run it through mise-selected Python without new dependencies. Record ReferenceExecuted only after actual execution.

Use independently stated expected summary sets, graph edges and outcome categories. Include direct/mutual recursion, finite Box nesting, forwarded parameters, phantom arguments and Growing<Box<T>>. Include the approved variant rule, empty declarations, private fields and callable/reference storage boundaries. A model resource limit must not become a cycle proof or finite-layout success.

Then start Rust vertical TDD through the approved entry and queries. Observe one failing behavior, implement that behavior and rerun focused tests. Preserve M4.1 and M1 through M3. Model results are supporting evidence, not compiler acceptance or a soundness proof.

### Planned direct acceptance, not observed results

| Requirement | Positive, rejected and boundary assertions | Provenance |
| --- | --- | --- |
| Member scopes | Distinct records can reuse field names. Reject unused duplicate record fields, enum variants and record-payload fields. Preserve all conflicting identities/spans. | RFC 0005 declarations and the new uniqueness procedure. |
| Parameters | Distinct names and repeated underscores are valid. Reject repeated named value parameters. Type/value namespaces remain separate. | RFC 0004 proposed header procedure; confirmed parameter syntax. |
| Actual prelude contracts | Accept canonical copy<P0>(&P0):P0 and dispose<P0>(P0):void after binder renaming. Reject wrong arity, access constructor, binder relationship or result. Reject foreign/missing contract identities as requests. Source shadowing cannot change canonical void. | Library catalogue section 2 and RFC 0004 proposed schemas. |
| Type positions | Exercise approved void/never positions without asserting construction or runtime values. A never field cannot hide an inline self-cycle. | RFC 0005 type forms and decision D1. |
| Summary forwarding | Store T directly, through tuples and through wrappers. Preserve ordered parameter indices. Unused generic arguments contribute nothing. | Confirmed inline-summary policy; T15. |
| Finite and cyclic layouts | Accept finite nested wrappers and reference-broken cycles. Reject direct, mutual, enum-payload and type-growing by-value cycles even when unused. | RFC 0005 T13/T15/T16, R0406. |
| Genuine versus fake indirection | Inspect user/standard-origin source records alike. A same-spelled List containing a by-value cycle rejects. An explicit reference field stops traversal. No unsupported public marker grants trust. | Confirmed trusted-boundary policy and D3. |
| Variant storage | Keep variant type identity distinct. Exercise the approved Choice<Node>.Empty outcome and variants with different parent substitutions. | Decision D2; no earlier representation outcome is claimed. |
| Callable storage | Function signature callables do not imply nominal storage. Reference-wrapped or unused-argument callable terms do not create capture edges. Direct unknown storage prevents completion. | RFC 0005 callable/capture distinction and D3. |
| Determinism and witnesses | Repeated/relocated inputs preserve semantic summaries, ordered edges and original-source cycle/duplicate witnesses without equating fresh capture IDs. | M1 through M4.1 provenance and RFC 0010. |
| Limits and incompleteness | Exact and one-short work/storage/depth budgets have controlled outcomes. Test long graphs and retained known errors before exhaustion. Unsupported M4.1 inputs never acquire a completed M4.2 receipt. | Explicit stage and resource contracts. |
| Completion honesty | Every result lists the scoped checks actually performed. Unresolved layout, capture storage, later visibility and body checks do not become successful complete source checking. | M4 stage boundary and RFC authority rules. |

After implementation, map these rows to real test names, model commands, observed outcomes and limits in a separate M4.2 acceptance record. Run test:compiler, test, check and build:scaffold through mise. Run check:ci if workflows change. Complete independent final Standards/Spec review before committing the implemented slice into PR #58. Keep the umbrella open and the PR draft until all M4 obligations have evidence and the owner authorizes merge.

### Checkpoint verification

Only this plan and proposed sections in RFCs 0004/0005 changed. No M4.2 compiler/model code or tests exist. Every planned M4.2 assertion remains NotChecked.

On the existing macOS 27.0.1 AArch64 host, mise 2026.11.4 selected Rust/Cargo 1.98.0, Python 3.13.18 and uv 0.13.8. The unchanged 158-test compiler suite passed through test:compiler. The test, check and build:scaffold tasks also passed. Documentation validation found zero errors across 1629 local destinations. Diff whitespace checks passed.

These are baseline/document checks, not M4.2 or model acceptance. User-global CMake/Ninja/Go warnings did not change project locks. Workflows did not change, so local check:ci was unrun. Product build/suites, release, LLVM and native experiments were unrun. Earlier hosted results retain their exact M4.1 source revision and failed unavailable native/runtime gates.

### Independent checkpoint review

Both axes reviewed the staged three-document proposal against `f405adaa01058849025105b11ed77fe32f6271b7`. Review preceded the checkpoint commit. Neither reviewer edited files or reran the recorded checks.

#### Standards

No actionable documented-standard violation or heuristic finding was identified. The review confirmed the distinction between current delivery instructions, earlier approvals and recorded evidence.

#### Spec

No actionable finding was identified. The review checked summary forwarding, incomplete-storage propagation, finite/growing cases, canonical prelude contracts and the explicit D1 through D4 choices. It confirmed that those decisions and model/compiler acceptance remain pending.

These are design-checkpoint reviews, not owner approval or final implementation review. Every M4.2 model/compiler assertion remains NotChecked. Formal ASD-STE100 dictionary compliance remains unverified.

### M4.2 implementation acceptance

The [M4.2 acceptance record](../crates/compiler/tests/m4-2-acceptance.md) maps actual reference-model and compiler tests to the approved procedure. It preserves the distinction between model execution, compiler analysis assertions and Rill execution. M4.3 through M4.8 retain their separate procedure and approval gates. M4.2 does not complete the umbrella or authorize merge.

## M4.3 design checkpoint

### Baseline, scope and authorities

M4.3 ticket ri-3gd starts from `dcc4b1390fbbf163b54618a5b4c59151d71fdb9a`, the reviewed M4.2 implementation on the existing branch. All eight slices remain in draft PR #58. No intermediate merge or new PR is required. M4 as a whole remains incomplete, and ri-b83 remains open.

The owner approved C1 through C4 (ri-3gd, comment 6947086957) at checkpoint `e4750b8579ea11aa80f76f86f93014c78854fac0` before M4.3 code. The implementation and [acceptance record](../crates/compiler/tests/m4-3-acceptance.md) now cover this bounded stage. Full RFC statuses, earlier scoped approvals and historical evidence remain unchanged.

[RFC 0003](rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m43-scalar-and-static-control-procedure) owns supported scalar contexts, lexical bindings, static places and finite control analysis. [RFC 0006](rfcs/0006-numbers-strings-and-primitive-operations.md#proposed-m43-literal-and-primitive-checking) owns literal/context propagation and primitive operation admission. [RFC 0004](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m43-explicit-standalone-calls) owns explicit standalone calls. [RFC 0005](rfcs/0005-types-records-enums-and-inference.md#proposed-m43-never-admission) owns the proposed never precision rule.

Existing exact inputs at the baseline include RFC 0001's accepted expectation/control contract, RFC 0002's canonical graph/namespaces/startup/privacy rules, and confirmed syntax/source facts. RFC 0003's initializer/scope and sequencing approvals remain fixed. RFC 0006's explicit-width, signed-minimum, rounding and trap-timing policies remain fixed. This checkpoint must not silently reopen any of them.

### Proposed delivery boundary

Check supported standalone bodies, startup statements of every loaded module and expectations in both modes. Support simple named/wildcard bindings, assignments, blocks, statement/expression conditionals, while, Boolean guards, return and loop transfers. Support primitive literals/operators, names/qualification, references/dereference and direct standalone calls with complete explicit type arguments.

Data construction/rebuilding, field/index/bracket/pattern/match forms, guard-let, for-of, pipes, arrows/interpolation, function values, callable invocation, ordinary families and error operators remain incomplete until their later procedures exist. Nested declaration visibility/hoisting and mutable temporary acquisition remain explicitly assigned to later M4 integration. Unsupported source must not receive a completed body or scalar-stage receipt.

This is the scalar/control portion of M4.3, not a claim that data, family, callback or core protocols are checked. M4.4 supplies data/pattern/variant relations; M4.5/M4.6 supply families and generic/callable evidence; M4.8 supplies remaining core, nested-source and result-use integration. None of those obligations disappears from M4 completion.

### Input/output contract and public TDD seams

```text
check_scalars(completed M4.2 receipt, limits)
    -> scoped scalar receipt or rejected/incomplete/resource/request failure
receipt.body(module, source body owner)
    -> supported scalar/control completion, declaration context and exit facts
receipt.node(module, NodeId)
    -> expression, binding, statement or control facts with original provenance
```

These are behavioral names. Body owners identify actual function/expectation declarations or the module's startup context. Queries validate the retained capture. Foreign identities, modules and invalid node indices reject as request misuse; absent facts are not invented types.

Expression facts retain canonical static types, source snapshots/spans, expected contexts, binding/callee identities, explicit substitutions and original operand references. Literal facts retain original spelling/decoded provenance and exact range or rounded-bit evidence. Control facts retain actual callable/loop targets and finite exit summaries. Statement facts have no expression value, not an Error type.

Keep private inference slots and blocked-binding causes outside TypeTerm. A known failed initializer preserves its binder identity and error cause. Dependent uses cannot manufacture a valid type or a false missing-name proof. Later statements cannot repair a missing initializer context.

A completed receipt covers only supported scalar typing, lexical/static-place requirements and the approved control proof. Full body/source validity, data/pattern coverage, family selection, generic/callable evidence, result-use, ownership, cleanup, runtime and editor behavior remain unperformed. Static writable/read-only facts are not access loans, physical owner addresses or storage-initialization proofs.

| Public seam | Catches | Does not establish |
| --- | --- | --- |
| check_scalars | Whole requested-stage completion, literal/type/name/control findings, unsupported obligations and controlled resources. | Full typed checking, ownership safety or product execution. |
| body query | Declared result context, expectation/startup boundaries, finite returns/loop exits and explicit unsupported-body refusal. | Termination, arbitrary path-correlation proofs, cleanup or tail-call eligibility. |
| node query | Canonical expression types, local binding identities, literal evidence, substitutions, access requirements and source control targets. | Moves/copies, loan transfer, capture evidence, runtime effects or editor behavior. |

### Owner decisions requested before code

| Decision | Recommendation and consequence |
| --- | --- |
| C1. Primitive admission | Approve the closed RFC 0006 matrix. Char ordering uses scalar code points. Text comparisons observe string, &string and &mut string, including mixed pairs, without consuming or converting views to owners. Deeper descriptor references require explicit dereference; other primitive operations do not implicitly dereference references. No pointer identity, string arithmetic or operator-family dispatch is introduced. |
| C2. Never precision | Admit canonical never at immediate value expectations without inventing a normal value or changing its actual static type. Do not infer numeric widths or omitted source generic arguments from it alone. Keep declared intermediate result types even when an operand cannot return; dead flow does not repair mismatches. |
| C3. Finite return/guard proof | Approve the finite Boolean-set and exit procedure. It propagates literals/logical forms and immutable Boolean initializer facts, not arbitrary arithmetic/comparison results, callee bodies or cross-statement correlations. A supported non-void body without a proved absence of fallthrough rejects as insufficient static return coverage. This may reject logically exhaustive separate tests of b and !b; an explicit else structure supplies the supported proof. |
| C4. Procedures, scope and seams | Approve the four owning sections, contextual constraint procedure, explicit-call substitution, binding/access boundaries, three public TDD seams and direct acceptance below. Preserve all listed unsupported obligations for later M4. Use vertical public-interface TDD, final independent review and required mise checks before committing the slice into PR #58. |

The C3 precision consequence is intentional and requires owner approval. This independent excerpt would fail the proposed return proof despite its runtime logical coverage:

```rill
function choose(b: bool): i32 {
  if (b) { return 1i32; }
  if (!b) { return 2i32; }
}
```

The diagnostic means insufficient coverage under the approved finite checking rule, not a recorded runtime fallthrough. The acceptance record now maps direct compiler assertions for this case and its explicit if/else counterpart. Rill execution remains NotChecked. Arithmetic traps remain runtime events; they never become a substitute return proof.

### Planned direct acceptance, not observed results

| Requirement | Positive, rejected and boundary cases | Owning provenance |
| --- | --- | --- |
| Local names and canonical identity | Parameters and earlier bindings resolve; inner initialization sees outer names. Reject missing/self-before-introduction uses and same-scope duplicates. Separate namespaces, aliases, private barriers and captures retain correct identities. | Confirmed RFC 0003 scope; RFC 0002; R0502. |
| Binding context | Explicit annotations and initializer-local context establish types. Later uses do not fix missing widths. Failed initializers retain blocked binder causes, never valid placeholders or false absence. | RFC 0003 bindings and proposed finite constraints. |
| Numeric literal typing | Context from parameters, returns, operands, scalar joins and annotations; no numeric default. Test all widths, integer bases, suffix conflicts, signed minima through grouping, negative unsigned zero, float ties, overflow-to-infinity rejection and permitted underflow. | RFC 0006 N4/N5, M1 literal facts. |
| Static errors versus traps | Reject invalid literals/types in skipped and unreachable source. Accept typed arithmetic overflow, zero division and large shifts without evaluating or marking them diverging. | RFC 0006 N6/N7; confirmed trap timing. |
| Primitive matrix and text | Check every operator category and mismatched width/type. Test char order admission, owned/view/mutable text pairs and explicit descriptor dereference. A same-spelled nominal primitive gains no operation. | Proposed C1; RFC 0006 primitive/text contracts. |
| References and assignment | Preserve descriptor versus referent types and static access categories. Reject proved read-only writes/acquisition; return incomplete for unclassified mutable temporaries. Check statement-only assignment and compound type context without ownership claims. | RFC 0003 places; RFC 0007 exclusions. |
| Standalone calls | Complete explicit generic arguments retain one correlated substitution across the signature. Check arity, arguments and result. Runtime function recursion uses declarations, not proof search or caller-body specialization. | RFC 0004 explicit calls; confirmed correlation/inference boundaries. |
| Unsupported selection/evidence | Ordinary family calls, missing generic inference, defaults, callbacks and requirements remain incomplete. Expected results cannot retry or rank bodies. No requirement or resource uncertainty becomes fallback evidence. | RFCs 0004/0009 and existing M4 stage boundaries. |
| Joins and never | Established scalar siblings can supply immediate context without width conversion. Canonical never contributes no ordinary join value. Preserve the proposed g(f(stop())) mismatch and missing-context boundaries. Permitted unimplemented variant/callable relations remain incomplete. | RFC 0005 joins and proposed C2. |
| Control and return coverage | Explicit returns, void fallthrough, never contexts, logical/immutable Boolean facts, while exits and Boolean guards. Reject wrong targets, non-Boolean conditions and failed C3 coverage. Nested loop transfers cannot falsely discharge an outer guard. | RFC 0003 E3/E4/E5 and proposed C3. |
| Expectations/startup | Check loaded sources in both modes. Expectations require bool without numeric default or hidden fallible return context. Reject startup capture/return escapes; unsupported inner callables are not mistaken for outer targets. | Accepted RFC 0001; RFC 0002 startup rules. |
| Determinism, provenance and limits | Repeated/relocated contexts retain ordered canonical facts and original Unicode/CRLF/literal evidence. Foreign queries reject. Test long flat expressions/scopes, exact/one-short budgets, retained errors and unsupported-body refusal. | M1 through M4.2, RFC 0010 and explicit resource limits. |

These rows preserve the approved test plan, not its execution record. The [M4.3 acceptance mapping](../crates/compiler/tests/m4-3-acceptance.md) records actual compiler assertions and verification separately. No row authorizes ownership/runtime assertions as typing evidence. No separate M4.3 reference model or Rill execution is claimed.

### Checkpoint verification

Before independent review, `mise run check:docs` passed 1644 local destinations with zero errors. Added RFC prose passed a local sentence-length scan. Formal ASD-STE100 vocabulary compliance remains unverified. No M4.3 implementation/model execution is claimed.

Independent Standards and Spec reviews covered the five staged documents against `dcc4b1390fbbf163b54618a5b4c59151d71fdb9a`. Standards found one minor instruction-style violation and no heuristic findings. The local correction split the three source-capture prohibitions into separate instructions without changing meaning. Spec found no actionable findings.

Neither reviewer reran tests or documentation checks. Their staged reviews excluded the later local instruction split and this review record. No second delegated review of those editorial changes is claimed. Neither review grants owner approval.

After the correction, `mise run check:docs` again passed 1644 destinations with zero errors. Staged and unstaged diff checks passed. No compiler, model, product, hosted or other-host execution was run for this documentation-only checkpoint.

### M4.3 implementation acceptance

Implementation `abd1f65d716a8ea3eaac5c47f187458dedd2d2fd` is pushed in draft PR #58. [The acceptance record](../crates/compiler/tests/m4-3-acceptance.md) maps 28 public-interface tests, all 203 compiler tests and passing post-review required tasks. Standards had no findings. The one P2 Spec defect in unsigned-never negation was corrected locally with a red-first regression. No second independent review or Rill execution is claimed.

## M4.4 design checkpoint

### Baseline and owning procedures

M4.4 ticket ri-qtl starts at reviewed M4.3 revision `abd1f65d716a8ea3eaac5c47f187458dedd2d2fd`. All slices remain in existing draft PR #58. No intermediate merge is required or authorized. ri-b83 remains open.

The owner approved K1 through K4 (ri-qtl, comment 6957273163) at checkpoint `391a6aafadfcded9d33d498056b813a9e393db57`, before model/compiler code. The bounded implementation and acceptance evidence are recorded below. The owner later confirmed ordinary simple-binding value typing (ri-qtl, comment 6959565199), without implicit borrowing or permission for an illegal transfer. Earlier scoped approvals, source examples, evidence and full RFC statuses remain unchanged.

[RFC 0005](rfcs/0005-types-records-enums-and-inference.md#proposed-m44-data-and-pattern-procedure) owns constructors, field access, variant admission/joins, categories and finite coverage. [RFC 0003](rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m44-match-and-guard-control) owns match/guard scopes and complete expression exit composition. [RFC 0007](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m44-pattern-binding-type-classification) supplies the typing prerequisite for borrowed binders, not ownership execution.

Exact existing authorities include nominal named construction in ADRs 0004/0008, current enum/narrowing rules in ADR 0012, RFC 0002 privacy and confirmed bracket defaults. Superseded union, structural-record and primitive-singleton examples remain historical. The new procedures must not restore them.

### Bounded delivery and prerequisites

Extend the existing scalar/control engine rather than create a second checker. The data entry requires the M4.2 receipt, not a successful scalar-only receipt. Otherwise every data-bearing body would fail at its deliberately unsupported M4.3 boundary before M4.4 could check it.

Support named record/enum construction, exact rebuilding, public field projections, owned patterns, eligible borrowed patterns, scoped variant narrowing, permitted variant admission/joins, tuple-context brackets and nested constructor coverage. Preserve all existing scalar checks and original provenance.

Complete written generic arguments and immediately established full nominal applications supply one substitution. General field/payload generic inference and trailing defaults remain with M4.6. Do not repair conflicts from defaults or later uses.

List construction, dynamic/index protocols and direct Option/Result guards require canonical core admission. That prerequisite remains with M4.8. Preserve the confirmed uncontextualized List default; never retry a tuple merely because core admission is missing.

Borrowed binders requiring unresolved aggregate implicit-copy classification remain incomplete until its custom-disposer/protected-capability reconciliation exists. General family/callback/evidence and core obligations remain within M4.5 through M4.8. No obligation moves beyond M4 completion merely because this slice is bounded.

### Public TDD seams

```text
check_data(completed M4.2 receipt, limits)
    -> scoped scalar/data receipt or source/incomplete/resource failure
receipt.body(module, actual source owner)
    -> supported scalar/data completion and full exit alternatives
receipt.node(module, original NodeId)
    -> construction, projection, admission, pattern, refinement and coverage facts
```

These are behavioral names. Reuse actual M3 owners, binders, fields and variants. Reject foreign captures and invalid queries. Keep private incomplete state outside canonical value types.

| Seam | Catches | Does not establish |
| --- | --- | --- |
| Data entry | Every requested supported scalar/data context, unresolved prerequisites, known source errors and checked budgets. | Full typed M4, ownership safety or runtime execution. |
| Body query | Matches/blocks with correctly composed returns, loop transfers, normal paths and recognized divergence. | Termination, cleanup or arbitrary path correlations. |
| Node query | Named identities/substitutions, field context/privacy, pattern types/scopes, explicit narrowing and finite coverage witnesses. | Implicit-copy execution, loans, physical addresses or initialized storage. |

### Owner decisions requested before model or compiler code

| Choice | Recommendation and consequence |
| --- | --- |
| K1. Construction and qualification | Approve the RFC 0005 procedure. Named brace constructors use type/module bindings. For syntactic dot qualification, an enum binding takes priority; a grouped/computed base uses value fields. `(name).field` therefore forces the value meaning in a collision. Failed chosen lookup never retries another namespace. Field inspection admits one reference layer and preserves its access/provenance; deeper descriptors need explicit dereference. |
| K2. Binding views and guards | Approve the classification seam and scoped variant views. Original declarations retain parent types, but explicit matched-arm uses can have the variant view required by ADR 0012. Changing that scrutinee's tag while its view is active rejects; payload mutation can preserve it. Unresolved aggregate copy classification cannot choose owned versus borrowed output. Permit refutable success patterns only with unnamed failure blocks; a named Result error requires an irrefutable success pattern. |
| K3. Coverage and block exits | Approve finite constructor specialization using static types, not literal/flow singleton knowledge. Ignore guarded rows for coverage, even with a true guard. Require declared outer constructors despite never payloads. Only an initial Root empty domain is vacuously covered; nested zero-constructor Components are symbolic holes and cannot erase other refutable columns. Witnesses are symbolic uncovered shapes, not runtime values. Permit noncontinuing block arms to contribute no join value through actual outward returns/loop transfers or recognized divergence. This extends the earlier draft's return/never-call wording. |
| K4. Scope, interfaces and validation | Approve the three public seams, shared-engine layering, explicit deferred core/generic/classification prerequisites and acceptance plan. Validate coverage with a separate finite reference model before Rust. Then use vertical public-interface TDD, required mise checks and final independent review in PR #58. |

K2 deliberately uses lexical variant views instead of implementing mutation-sensitive refinement invalidation. A matched mutable place cannot change variants before its arm ends. Whole replacement of an ancestor also rejects while that view is active. Outside the arm its original declared parent type remains. This is a proposed precision limit, not an ownership proof.

K3 deliberately requires static constructor coverage despite constant Boolean flow information. A match on true still has bool as its source type. A literal or guarded arm cannot create singleton dispatch permission. Written unreachable arms still require typing.

### Planned acceptance, not observed results

| Requirement | Planned public-interface contrasts |
| --- | --- |
| Named identities and generic substitution | Record/variant construction retains actual defining identities and one complete substitution. Reject same-shape unrelated targets, missing/duplicate/unknown fields and conflicting established applications. Missing inference remains incomplete. |
| Declared field literal context | Unsuffixed fields receive declared numeric context after substitution. Already typed incompatible widths reject. Nested fields and payloads cannot default widths or repair conflicts. |
| Rebuilding and privacy | Exact same application, one leading base and written replacement order. Reject external rebuilding whenever any field is private, even from an existing value. Preserve public unimported versus genuinely opaque distinctions. |
| Qualification and field places | Enum/value collisions follow K1 without parse retries. Grouping forces value fields. Retain canonical field IDs, reference layers, original access requirements and private barriers. |
| Variant admission and joins | Owned/read-only admission and same-parent variant joins. Reject mutable widening, generic-argument conflicts, container covariance and unrelated structural joins. Parent boundaries do not recover variants except explicit scoped patterns. |
| Bracket categories | Tuple context fixes arity and element context. Uncontextualized brackets remain List-category even when heterogeneous. Missing List/core or family-input context stays incomplete, never tuple fallback or fabricated standard identity. |
| Patterns and bindings | Exact tuples, accessible partial records, all variant payload forms, nested refutability, literal tests and duplicate binders. Irrefutable lets introduce names after initialization. Borrowed scalar/string/reference projections preserve the classification and descriptor distinctions. |
| Scoped narrowing | Canonical variant views inside arms, declared parents outside, guard-failure scope restoration and tag-changing rejection. No optimizer or callee-body narrowing. Unresolved copy classification remains incomplete. |
| Complete expression exits | Match expression/block arms retain actual return/break/continue targets through short-circuiting and enclosing expressions. An inner-loop break cannot falsely discharge an outer arm/guard. Normal blocks yield only void. |
| Coverage | Closed enums, known variants, nested products, Boolean and literal domains, guarded rows, empty enums and never payloads. Both V(never, bool) and V(bool, never) require their remaining Boolean coverage. Test wildcard, single refutable and complementary rows in both orders; substitute empty enums too. Root-empty coverage remains vacuous. No outer-tag-only proof or inhabited-witness claim. |
| Guard-let prerequisites and policy | Missing canonical Option/Result admission stays incomplete, including same-spelled user enums. The reference policy distinguishes Err, Some/Ok mismatch and forbidden invented error wrappers. Actual core integration remains an M4.8 gate. |
| Provenance and limits | Original Unicode/CRLF snapshots, alias routes, member/binder IDs, substitutions and coverage sites. Repeated and relocated normalized facts agree; foreign queries reject. Exact/short budgets, recursive shapes and wide products cannot turn uncertainty into negative evidence. |

### Model-first and checkpoint verification

After approval, the finite model will compare constructor coverage with independently enumerated finite cases. Its public seam is `coverage(static_shapes, pattern_rows, limits)`. Outcomes distinguish Covered, UncoveredDeclaredShape, Incomplete and ResourceLimit. Separate expected cases will cover conservative never/empty-enum payload holes in both column orders and guard/refutability policy. The model must distinguish its abstract shape witnesses from real Rill values.

Before approval, all M4.4 model/compiler assertions remain NotChecked. Pre-review `mise run check:docs` passed 1658 local destinations with zero errors. Added RFC prose passed a local sentence-length scan. Formal ASD-STE100 vocabulary compliance remains unverified.

Independent Standards and Spec reviews covered the four staged documents against `abd1f65d716a8ea3eaac5c47f187458dedd2d2fd`. Standards found no documented violations or judgement-call findings. Spec found one P2 ambiguity in nested zero-constructor columns. The local correction distinguishes vacuous Root domains from symbolic Component holes and adds expected model cases for both payload orders.

Neither reviewer executed a model/compiler. The original staged reviews exclude the later local correction and review record. No second delegated review is claimed. Neither review granted owner approval. K1 through K4 were pending at this design checkpoint. The subsequent scoped owner approval is recorded above.

After the local correction, `mise run check:docs` passed 1658 destinations with zero errors. The added-RFC sentence-length scan and staged/unstaged diff checks passed. No M4.4 model/compiler code or execution was run. Formal vocabulary compliance remains unverified.

### M4.4 implementation and acceptance

The [M4.4 acceptance record](../crates/compiler/tests/m4-4-acceptance.md) maps direct data entry/body/node assertions and the separate model-first evidence. Both independent final axes reviewed the staged implementation. Standards found one nonblocking duplication heuristic, corrected with a shared budget-charging substitution builder. Spec found two P2 defects in implicit-dereference views and nested match context. Public regressions failed before both local corrections. The record separates the original review scope, later corrections and actual verification; no second delegated review is claimed.

The implemented scope retains explicit missing core and aggregate-classification prerequisites. Positive List/index/Option/Result admission remains an M4.8 integration gate. Neither that boundary nor the simple-binding clarification authorizes an illegal ownership transfer. M4.5 through M4.8 still require separate procedures, review and owner approval. PR #58 remains draft and unmerged; ri-b83 remains open.

## M4.5 design checkpoint

### Baseline and scope

M4.5 ri-8p9 starts from M4.4 implementation `8abca7a6b933b44c6abc29419b2f324dc51a97a8`. Its predecessor has 36 public-interface tests and 239 passing compiler tests. The [M4.4 acceptance record](../crates/compiler/tests/m4-4-acceptance.md) preserves its model, review, correction and verification scopes.

The owner approved exact wildcard capture (ri-8p9, comment 6962750287), including the stated parent-pattern incomparability. The owner then approved P1 through P4 (ri-8p9, comment 6963439018) at `3392baaac8f2ee50e86167700349273a76ce61c4`, before model or Rust work. The exact-wildcard decision alone did not approve the checkpoint.

All slices stay in existing draft PR #58. ri-b83 remains open. No intermediate merge, new PR, M5 or complete-M4 claim is authorized.

### Owning procedures

- [RFC 0004 ordinary input procedure](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m45-ordinary-input-procedure) owns header compatibility, canonical keys, finite comparison, collection and input-only selection.
- [RFC 0002 collection handoff](rfcs/0002-modules-and-program-loading.md#proposed-m45-collection-handoff) preserves the actual loaded graph, identities, routes and unresolved scoped declarations.
- [RFC 0005 static input boundary](rfcs/0005-types-records-enums-and-inference.md#proposed-m45-static-input-boundary) distinguishes fixed admissions, exact wildcard terms and never's no-value rule.
- [RFC 0007 protected-overlap boundary](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m45-protected-overlap-boundary) owns canonical protection prerequisites without granting runtime or ownership authority.

The proposed implementation extends shared header/type facilities rather than create another resolver. The existing M4.1 entry must retain its deliberately bounded behavior. Requiring its success on implementation-bearing source would block collection before the new procedure can inspect it.

The extended header view must reuse applicable M4.2 validation, not bypass it. Retain member uniqueness, required prelude schemas, type-position and containment checks, including uncalled declarations. Implementation header parameters receive the same uniqueness rule. Failed or unresolved prerequisites prevent completed collection.

Header compatibility solves family positions against the full written implementation signature. Implementation binders remain rigid. Normalized input variables instead range symbolically across type terms during comparison. Keep these distinct roles and namespaces.

Defaults and requirements retain source metadata, not guessed truth. Their unresolved use prevents affected completion. Input-only selection does not check generic bodies, derive callable captures, satisfy selected evidence or finish source-call contexts. Those remain M4.6 duties inside M4.

Missing canonical family-role/protected admission prevents completed global collection. [RFC 0004's completion boundary](rfcs/0004-functions-overloads-and-generic-requirements.md#input-selection-and-completion-boundaries) separates hypothetical ordinary facts from conclusive input relations. A pending role permits only a conditional outcome with that premise attached. It cannot establish source rejection, an ordinary winner or a checked call.

With an established ordinary role, input facts can still retain separate pending protected/evidence clearance. Overall analysis remains incomplete until required clearance finishes. Ground candidate matches and necessary comparisons must settle before winner, absence or ambiguity becomes conclusive.

### Proposed public seams

Names below define behavior, not final Rust spelling. The owner must approve the boundaries before tests or implementation.

```text
analyze_ordinary(captured M3 graph, actual admitted-catalogue state, limits)
    -> header/collection analysis with completion state, facts and obligations
analysis.family(canonical family identity)
    -> canonical header, implementation, key, placement and comparison provenance
select_inputs(analysis, canonical family, established input contracts,
              explicit family arguments, limits)
    -> selection report with hypothetical/conclusive input facts and completion state
```

| Seam | Catches | Does not establish |
| --- | --- | --- |
| Ordinary analysis | Header substitution, input keys, actual collection routes, duplicates, placement, known protected conflicts and pending admission. | Complete source validity, generic body/evidence checks, provider trust or execution. |
| Family query | Original family/body/binder/nominal identities, correlated keys, source witnesses and per-check completion. | Source bindings for metadata-only names or permission to inspect private representation. |
| Input selection | Established static vector matching and whole-vector winner/absence/ambiguity with retained comparisons. | Literal/callback inference, output agreement, selected requirements, access/ownership or an executable call. |

All interfaces reject foreign captures and malformed canonical types/indices. Public requests cannot supply an unchecked trusted-provider whitelist. Missing catalogue state differs from an actually validated empty inventory.

A hypothetical structural outcome must retain its ordinary-role premise. It is not a conclusive ordinary winner or source finding. A conclusive input relation can retain separate pending protected/evidence obligations without becoming a completed collection or call. Source diagnostics require the applicable role/admission premises, not a hypothetical interpretation of an unclassified conversion family.

Established type contracts differ from unresolved or bare-canonical-never positions. Bare never does not infer a width, wildcard binding or receiving shape. Other noncontinuing expressions retain their independently declared static types. Symbolic specialization-dependent vectors remain incomplete here. M4.6 must complete those source-call obligations without body-count guesses or result-directed ranking.

### Decisions requested before model/compiler code

| Choice | Proposal and consequence |
| --- | --- |
| P1. Finite static comparison | Approve canonical whole-vector alternatives, exact correlated wildcard matching, rigid-symbol inclusion, occurs-checked intersection and fresh-tuple separation. Fixed parent/read-only and outer callable-mode admissions expand finitely. Consuming expected mode accepts all three actual modes; the reverse does not follow. Rank original patterns after explicit input constraints filter matches. No container or mutable-reference covariance follows. Checker limits never shrink the universal type domain. Validate the finite-union rule before Rust; a counterexample returns to design. |
| P2. Header and input-context boundary | Approve shared ordinary-header elaboration without weakening the original M4.1 entry. One family substitution preserves the full written parameter/result signature, with implementation binders rigid. Keep never's no-value expectation distinct from type-vector matching. Literal/callback/default/source-call completion remains M4.6, not an arbitrary input shape chosen here. |
| P3. Canonical collection and protected clearance | Approve canonical body deduplication, input-key duplicates, nested nominal placement and RFC 0007's protected-record decisions. Validate per-family evidence and actual required providers. Retain no-provider prohibitions or explicit pending prohibition obligations. Missing family-role or protection admission keeps global completion incomplete. Conditional structural facts retain that premise; they never license checked calls or fabricated standard roles. Actual core admission remains within M4.8. |
| P4. Scope, seams and validation | Approve the three public seams and model-first plan. Validate normalization/comparison/collection/selection independently before vertical public-interface TDD. Keep defaults, evidence, callable formation, conversions, core and final integration inside their later M4 slices. Run required verification and separate final reviews in PR #58. |

P1 does not reopen the owner's exact-wildcard decision. It proposes the executable finite method and its remaining admission boundaries. Full RFC statuses and prior approvals remain unchanged.

### Planned model and compiler acceptance

The table records planned contrasts. They were NotChecked at the checkpoint. See the [M4.5 acceptance record](../crates/compiler/tests/m4-5-acceptance.md) for observed results and gates that remain unrun.

| Requirement | Planned contrasts |
| --- | --- |
| Alias and binder normalization | Different import routes to one family/body count once. Renamed binders and unused result/requirement-only binders do not distinguish input keys. Preserve all actual routes and source positions. |
| Signature substitution | One family argument must agree across complete parameters and result. Fixed references/modes cannot disappear. A rigid implementation binder cannot be solved to repair a mismatch. Missing family positions/default completion stay incomplete. |
| Correlations and ambiguity | `(T,T)` is narrower than `(A,B)`; crossed `(Point,T)`/`(U,Point)` are incomparable. Repeated wildcards reject mixed variant terms. Fixed `(E,E)` can overlap `(T,T)` without dominance, even at an equal-variant vector. |
| Static admissions | Owned/read-only variants fit fixed parent patterns. Parent values do not fit variant-only patterns. Mutable widening and nested container covariance reject. Pin expected/actual callable-mode direction without capture evidence. Bare never cannot infer an input shape; a noncontinuing i32-result intermediate still supplies i32. |
| Inclusion and intersection | Compare whole alternative vectors with distinct binder namespaces. Reject cyclic substitutions. Challenge finite-union inclusion with nested correlations and fresh tuple arities. Do not use runtime inhabitation or current caller counts. |
| Duplicates and placement | Reject uncalled distinct bodies with equal keys despite different returns/requirements. Permit nested local nominal ownership in parameters. Reject return-only, imported-owner and bare-binder permissions. |
| Role and protection | Never misclassify a possible canonical conversion as an ordinary duplicate. A missing/partial catalogue stays incomplete. Validate per-family evidence and exact required-provider links. Cover required-winner and no-provider prohibited domains, including generic adapters and pending negative metadata. Positive real admission is unrun until actual contracts exist. |
| Selection and evidence boundary | Ground static vectors produce settled input facts only with their premises proved. Pending role yields hypothetical facts, not source diagnostics. Explicit input arguments filter through retained header substitutions; original patterns still determine priority. Output-only arguments cannot filter. Unknown candidates/comparisons can block ambiguity as well as winner/absence. Requirements cannot tie-break or trigger fallback. |
| Provenance and resources | Foreign identities reject. Repeated/relocated normalized facts agree. Exact limits pass; one-short limits retain known facts as ResourceLimit. Wide products, long vectors and growing equations cannot exhaust host recursion or turn pending work into disjointness. |

The reference model's proposed public operations are normalization, inclusion/intersection comparison and input selection over abstract canonical identities. A separate bounded ground enumerator provides an independent finite oracle. Those bounds are test limits, not the language's type universe or a universal soundness proof.

Compiler assertions must use real M3 snapshots and canonical declaration inventories. No fabricated List, conversion or protected-capability definition can supply positive admission. Preserve incomplete outcomes when actual contracts are absent.

Required local verification remains `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold`. Run check:ci only if workflows change. Record model execution, compiler assertions, product execution and unrun gates separately.

### Checkpoint verification and review

No model, compiler assertion or Rill execution had run at the checkpoint. Pre-review documentation validation passed 1675 destinations with zero errors. Added RFC prose passed a local sentence-length scan. Formal ASD-STE100 vocabulary compliance remains unverified.

Independent Standards and Spec axes reviewed five staged documents against `8abca7a6b933b44c6abc29419b2f324dc51a97a8`. Their staged diff SHA-256 was `db7bdda5a51bdb491112e3eb7c538bc469894551cdb05951c59c18b74e35a701`.

Standards recorded three documented-rule findings and one editorial judgment. They concerned conditional collection prerequisites, duplicate ownership of protected decisions, the literal-context handoff and unclear disjointness prose. Spec recorded six clarification findings. They concerned filtering versus ranking, M4.2 prerequisites, conditional outcomes, executable protection records, module-only implementation syntax and mode/never precision.

The local correction ranks original patterns after candidate filtering through retained header substitutions. It reuses applicable M4.2 checks and distinguishes hypothetical ordinary outcomes from conclusive input relations. Unknown candidates/comparisons cannot settle ambiguity. RFC 0007 now owns per-family evidence, exact required-provider links and no-provider prohibitions, including pending negative metadata. The correction also preserves M2 rejection of nested implementations and reconciles M4.5 structural contexts with M4.6 source-call completion.

Spec hand-checked fresh-tuple separation and occurs-checked correlation. Standards ran the documentation checker and whitespace checks. Neither reviewer executed a model/compiler or granted owner approval. Their original staged reviews exclude the later corrections and this review record. No second delegated review is claimed.

After correction, documentation validation passed 1676 destinations with zero errors. Added RFC sentences remained within the local length limit; formal vocabulary compliance remains unverified. The checkpoint awaited separate P1 through P4 approval, which the owner later granted at the revision recorded above.

## M4.6 design checkpoint

### Baseline and scope

M4.6 ri-w05 follows M4.5 at `830a651d88bf1703bbfa877873ebd2289e52adcc`. The [M4.5 acceptance record](../crates/compiler/tests/m4-5-acceptance.md) records 17 scoped public tests and 256 passing compiler tests. Its ordinary facts remain hypothetical without canonical core admission. They do not authorize an ordinary call or stand in for evidence.

The owner chose the context agreement rule (ri-w05, comment 6964354024). For loose `pair<A,B>` with `(A,B)` and `(T,T)` implementations, `pair(1i32, 2)` needs an independent width. The viable fallback does not supply the same context as the specialized pattern. A declaration `pair<T>(left: T, right: T)` can supply it from the first argument. This precision does not approve the procedure or the public queries below.

### Owning procedures

- [RFC 0004 inference and evidence](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m46-inference-and-evidence-procedure) owns local substitution frames, context agreement, callable typing, symbolic calls and bounded proof work.
- [RFC 0005 applications and bracket category](rfcs/0005-types-records-enums-and-inference.md#proposed-m46-nominal-application-inference) owns nominal field/payload inference, defaulted applications and literal category commitment.
- [RFC 0007 typed capture and predicates](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m46-capture-and-invocation-facts) owns access demands, actual capture sources and typed predicate proofs.
- [RFC 0008 exact error evidence](rfcs/0008-errors-propagation-and-caused-by-wrapping.md#proposed-m46-exact-error-evidence) owns the ordinary exact-E witness boundary. M4.7 still owns conversion-selected E.
- [RFC 0002 requirement metadata](rfcs/0002-modules-and-program-loading.md#proposed-m46-requirement-metadata-boundary) preserves source routes and the separation of caller lookup from proof inputs.

Reuse M4.5's actual inventory and shared M4.1/M4.2 facilities. Extend the M4.3/M4.4 body engine rather than run a new resolver or body checker. The old bounded entries keep their scopes and tests. An earlier entry's failure on unsupported generics is not an M4.6 prerequisite. Applicable declaration checks still precede completed body facts. Preserve missing core, layout and representation prerequisites rather than assuming them complete.

This stage checks supported named and implementation bodies symbolically, including their nested arrows and supported calls in startup and expectations. Check written source independently of the set of concrete instantiations. Callee contracts come from declarations and proof records, never an inspection of an eventual callee body. Checking a fresh arrow's own source is not strengthening a callee contract.

M4.7 owns conversion applicability/determinacy/coherence. M4.8 owns actual core admission, canonical List/index/Option/Result/Err and iterator integration, visibility of public and nested declarations, remaining capture/aggregate typing prerequisites and integrated result-use completion. Keep pending typed obligations inside M4. This stage establishes no ownership, loan, storage, transfer, cleanup, runtime or editor facts. Keep ri-b83 and ri-w05 open and PR #58 draft and unmerged.

### Proposed public interfaces

Names describe behavior, not final Rust spelling. Obtain separate approval before tests or implementation.

```text
analyze_generics(captured ordinary analysis, analysis limits)
    -> scoped generic/body report with completion, findings and obligations
report.body(module, existing body owner)
report.node(module, original syntax node)
    -> checked or explicitly partial body and node facts
report.goal(issued goal identity)
    -> exact goal, assumption/selection dependencies, source sites and proof state
```

The analysis uses one actual M3 capture and the applicable declaration and ordinary-comparison facts. It may complete independent standalone bodies even if another body's required ordinary/core premises remain pending. Retain those pending premises in the report. Check every supported written body context. A scoped body completion does not prove its callees' source bodies, complete the loaded graph, or grant runtime permission.

| Interface | Catches | Does not establish |
| --- | --- | --- |
| Generic analysis | Header/default completion, independent type anchors, deferred source contexts, local scope and required evidence. | Successful analysis of unrelated unsupported constructs, protected role/provider admission or whole-source validity. |
| Body and node query | Canonical types, substitutions and their origins, literal/category commitments, arrow signatures, capture demands, admission and actual exit targets. Conditional ordinary facts retain their premise. | Erasure of actual dependencies, permission to move/borrow/copy, or a checked result for a failed call. |
| Goal query | Canonical ordered terms, explicit assumptions, selected body/equalities, proof dependencies or blocking cycle, and separate proof states. | A public way to inject a trusted assertion, choose a body by evidence, retag unknowns, or assume a pending goal. |

Reject cross-capture modules, owners and goal identities as InvalidRequest. Existing out-of-scope nodes return no checked fact. Give each body explicit completion and unperformed obligations. A report with partial facts cannot be substituted for a successful checked body. Goal queries only inspect goals issued by the checker; callers cannot construct preapproved evidence.

### Decisions requested before model or Rust

| Choice | Proposal and consequence |
| --- | --- |
| H1. Exact frames and defaults | Approve rigid body binders, flexible per-use slots, exact equations with bounded occurs checks, and fixed outer admissions as separate facts. Solve actual anchors before ready written defaults and resume inference after each one. Finish nominal construction and type applications without widening existing containers or guessing an unsuffixed width. Bare never does not create a substitution. |
| H2. Source context and callables | Confirm the recorded fallback agreement and approve its finite implementation. Require a written shape or independently substituted type for each needed projection, even with one survivor. Commit category/arity before checking fresh elements. Use canonical callable clauses for shape only, then type arrows once and run whole-vector selection. Arrow mode and capture origins are separate from static comparison. Output context cannot back-propagate into ordinary selection. |
| H3. Exact goals and symbolic outputs | Approve explicit checked assumptions, finite uniform-winner proofs and distinct determine/prove judgments. Infer any genuinely free ordinary result position only after selection. Keep pending outputs, explicit input filters, and the caller’s assumptions distinct. Preserve exact selected relationships, including E. Active cycles are unresolved, not success or proof of absence. Selected evidence failure never triggers fallback. Callable and structural property proofs preserve actual origins and never grant access. |
| H4. Interfaces, completion and validation | Approve the three query boundaries and partial facts. Validate the finite staging and proof procedure in a separate model before vertical public-interface TDD. Reuse real captures without a fake catalogue. Keep missing admission, representations and later typed obligations explicit. Run required local verification and independent final reviews in PR #58. |

### Planned model and compiler acceptance

These are expected contrasts, not observed results. All M4.6 model and compiler cases are NotChecked at this checkpoint.

| Counterexample to prevent | Planned check | Owning rule |
| --- | --- | --- |
| A default overrides independent inference or repairs a conflict. | `same<T=i32>(true)` binds bool. In `f<R=i32,A=bool>(cb: (A) => R)`, apply A's `bool` default, then infer R from `x => true` without using R's default. A written trailing default can supply width after inference. Earlier dependencies, same-slot conflicts, cycles and depth exhaustion have distinct results. | [Frames and defaults](rfcs/0004-functions-overloads-and-generic-requirements.md#worklist-and-defaults) |
| Construction invents a width or chooses another declaration. | Records and enum payloads use one frame and known field contexts. Rebuilds preserve exact applications. Standalone tuples use immediate independent contexts. Unknown List admission retains pending facts. An existing tuple cannot be retyped from a later use. | [Nominal inference](rfcs/0005-types-records-enums-and-inference.md#proposed-m46-nominal-application-inference) |
| A specialization alone supplies the missing width. | Loose repeated `pair(1i32, 2)` requires context with the viable fallback. The suffixed call selects by established inputs. A repeated declaration slot can supply that width. A previously typed i64 is not converted to i32. | [Context agreement](rfcs/0004-functions-overloads-and-generic-requirements.md#source-call-context-and-ordinary-selection) |
| Candidate context uses output, tries several types, or guesses from implementation count. | Conflicting List/tuple and callback projections do not materialize the source twice. A fixed 2/3/4-item tuple uses category then source arity. Each `join` callback gets its zero-parameter context while its result slot is open. Borrowed `map` supplies `&T` only from the established applicable pattern. An overloaded bare function value without context cannot use a sole body to complete it. | [Bracket staging](rfcs/0005-types-records-enums-and-inference.md#bracket-category-before-elements) and [callable formation](rfcs/0004-functions-overloads-and-generic-requirements.md#callable-formation-and-invocation) |
| Mode or erasure grants an environment capability. | Actual read-only/mutable/consuming modes retain parameter/result equality. Shared and mutable descriptors cannot consume the referent. Two same-signature closures do not share a property proof, even in `Holder<F>` fields. Resolve declared predicates and preserve origins through member projections. No fake native admission. | [Typed capture and predicates](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m46-capture-and-invocation-facts) |
| A generic body specializes itself from concrete callers. | Check with rigid parameters and actual assumptions. A uniform winner needs full coverage and dominance over all intersecting alternatives. A fallback that another instantiation can override gives no static output promise. The same input vector with distinct explicit filters can have distinct witnesses. Exact required outputs must agree after concrete normal selection, including E. | [Symbolic bodies](rfcs/0004-functions-overloads-and-generic-requirements.md#symbolic-bodies-and-evidence-goals) and [exact error evidence](rfcs/0008-errors-propagation-and-caused-by-wrapping.md#proposed-m46-exact-error-evidence) |
| A requirement supplies its own proof or changes body choice. | Declaration and selected-body requirements are conjunctive. A proved missing requirement does not retry. A selected `where` clause cannot seed a proof of itself. Active cycles, growing chains and unavailable premises stay distinct from settled negatives. An expectation cannot complete a pending output or contradict a determined one. | [Proof work](rfcs/0004-functions-overloads-and-generic-requirements.md#proof-worklist-and-completion) |
| A partial report becomes success or fabricates a type. | Unresolved context/core roles and exhausted budgets retain causes and known facts, no fresh checked result. Skip no written source, including inactive paths. Validate original syntax, scope, routes and graph identities. Exact limits and one-short cases verify bounded products, worklists and caches. | [Public interfaces](#proposed-public-interfaces) and [proof completion](rfcs/0004-functions-overloads-and-generic-requirements.md#proof-worklist-and-completion) |

The independent model should enumerate bounded input possibilities to challenge projected agreement and symbolic winner proofs. Test equation/default scheduling under worklist permutations. Compare proof outcomes to finite graphs with independent seeds, ungrounded cycles, conflicting outputs and expanding goals. A counterexample to the procedure returns to design before Rust. The model does not admit real core identities or run Rill.

Direct compiler tests must load real M3 source fixtures and query actual inferred declarations, bodies, nodes and goals. Use ordinary proof fixtures conditionally when the real core role is missing. Do not manufacture trusted contracts for unconditional ordinary proof tests. Preserve positive standalone and intrinsic tests where their prerequisites are already known.

Run `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold`. Run `check:ci` only if workflows change. Keep documentation checks, review scopes, model execution, compiler assertions and product execution separate. No M4.6 model, compiler or Rill execution has occurred at this checkpoint.

## M4.7 design checkpoint

M4.7 ri-lqn follows the [M4.6 completion record](../crates/compiler/tests/m4-6-acceptance.md#completion-increment) at `392d8428a81109b1dc4ed0ad8e60fd4738054522`. All eight slices remain in [draft PR #58](https://github.com/Reilley64/rill/pull/58). The three confirmed conversion policies (ri-48g, comment 6918885645) do not approve these new procedures or accept RFC 0009.

### Owning procedure and scope

[RFC 0009](rfcs/0009-user-implementable-conversions.md#proposed-m47-conversion-checking-procedure) owns pair collection, endpoint permission, exact destination matching, conversion applicability, selected-error determination and exact equality. It uses the finite pattern and bounded evidence procedures of [RFC 0004](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m45-ordinary-input-procedure). RFC 0005 still owns variant and reference relationships. RFC 0006 owns the numeric domain and core pair catalogue. RFC 0008 owns closed Err, propagation and causal wrapping.

The companion rules are pinned to [`392d8428a81109b1dc4ed0ad8e60fd4738054522`](https://github.com/Reilley64/rill/tree/392d8428a81109b1dc4ed0ad8e60fd4738054522). This review covers the new RFC 0009 procedure and this checkpoint, not amendments to or approval of the dependencies.

| Companion sections at the pinned revision | Role in this checkpoint |
| --- | --- |
| RFC 0002, [requirement metadata](rfcs/0002-modules-and-program-loading.md#proposed-m46-requirement-metadata-boundary) | Normative capture and binding boundary. |
| RFC 0004, [finite comparison](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m45-ordinary-input-procedure), [callable formation](rfcs/0004-functions-overloads-and-generic-requirements.md#callable-formation-and-invocation) and [bounded proof](rfcs/0004-functions-overloads-and-generic-requirements.md#proof-worklist-and-completion) | Normative pattern operations, type equations and proof limits. They do not determine conversion applicability. |
| RFC 0005, [data construction and patterns](rfcs/0005-types-records-enums-and-inference.md#admission-joins-and-bracket-categories) | Normative canonical types and fixed variant admission. |
| RFC 0006, [numeric protection](rfcs/0006-numbers-strings-and-primitive-operations.md#protected-numeric-conversions-and-explicit-arithmetic-helpers) | Normative numeric domain. Provider admission is a separate gate. |
| RFC 0008, [exact error evidence](rfcs/0008-errors-propagation-and-caused-by-wrapping.md#proposed-m46-exact-error-evidence) | Companion equality and propagation boundary. RFC 0009 owns conversion-selected E. |

Extend the shared typed checker. Use loaded M3 bodies, declaration IDs, syntax, spans and routes without resolving imports again. Use M4.1/M4.2 canonical terms and M4.5/M4.6 signature equations, pattern comparison and evidence. Do not treat conditional ordinary keys as conversion keys. Do not let a conversion role hypothesis strengthen the ordinary view. No change to ordinary placement, ranking, requirement checks or no-fallback behavior.

M4.8 still owns actual role, provider and protection admission and integrated core and result-use checks. M4.7 checks the structural protection rules and reports where real admission is needed. Do not invent standard source, providers or catalogues to establish a checked conversion. The procedure grants no access, move, borrow, storage or execution permission.

### Proposed public interfaces

Names describe the interface, not final Rust spelling. A role hypothesis is an explicit, report-wide premise. It names captured family identities and, for fallible analysis, a captured Result identity. Use actual M3 binding and signature evidence to check their proposed roles. The caller cannot designate a trusted provider or assert a requirement result. Keep the unfinished admission visible even when all structural comparisons settle.

| Interface | Catches | Does not establish |
| --- | --- | --- |
| Conversion analysis over one captured typed analysis context and limits; query family and registration facts | Canonical alias/body deduplication, pair keys, shape, placement, protection conflicts, and error-determination routes. Returns independent body findings and explicit blockers. | Source visibility not in the capture, trusted core admission, or body validity inferred from selection. |
| Query an established pair in that report, with optional exact source filter and separate E demand | Whole-pair matching, applicable and unresolved candidates, ordering witnesses, selected signature, required protected identity, and output agreement. Use canonical terms belonging to the same capture. | Literal or endpoint inference, injected proof assumptions, or a checked call from a type-vector comparison. |
| Query original call and function-value nodes, body status and checker-issued evidence goals | Independent endpoint contexts, actual source and admission, selection or symbolic relation, exact E, provenance and blocked dependencies. | A resolver, an alternate expression checker, a proof of ownership or a runtime adapter. |

An established-pair request can run the same proof worklist over retained inventory, without rechecking source or importing a caller's assumptions. The request has explicit analysis limits and returns a separate scoped report. Charge request validation, matching and proof work to that request. Its own exhaustion returns `ResourceLimit`. It cannot repair an incomplete inventory or borrow conclusions from another report.

Return `InvalidRequest` for foreign identities, structurally invalid queries and cross-report goals. Distinguish no fact from a partial fact. Give resource exhaustion precedence over incomplete and rejected outcomes. Known independent facts may survive exhaustion. Conditional results and findings carry their premise, including when work stops early.

### Decisions requested before model or Rust

| Choice | Proposal and consequence |
| --- | --- |
| V1. Identity, registration and protection | Use actual captured canonical identities. Approve distinct conditional interfaces for unadmitted roles. Require the written target and exact header equations. Normalize both endpoints with one binder numbering. Check unused registration bodies and reject duplicate, unauthorized and protected overlap under their applicable premises. Use actual endpoint equality for protected infallible identity. |
| V2. Pair matching and context | Approve exact destination and M4.5 fixed-outer source admissions only. A binder fixed by To remains an exact variable at the source. For example, with `Wrapped<E>` as To, `Wrapped<T> <- T` does not admit `E.Tag`; a written fixed `E` source can. Allow explicit From to supply context and filter the declared source term, not retag an already typed source. A `Parent` destination and an actual `Parent.Tag` source do not enter protected identity even with a written `Parent` source filter. At equal actual endpoints, an incompatible filter rejects the identity use without fallback. The alternative is to allow a destination-bound source variable to gain the admission of its substituted type. That changes the pair sets and needs a different finite comparison procedure. |
| V3. Applicability and selected E | Approve three-state applicability, registration-time reachability of error terms, frozen evidence inputs and bounded pair-keyed determination. A failed requirement can exclude a conversion candidate; unresolved potentially maximal candidates block success. An independently determined E can bind a flexible caller slot or check an existing term. Reuse exact symbolic assumptions only when filters preserve each candidate's domain. A uniform body may still have a pending dependent output. Keep a rigid relation separate from a selected body. No fallback after E mismatch. |
| V4. Seams and validation | Approve the public interfaces and their completion limits. Compare against an independent finite model before public-interface TDD. Use actual loaded captures and label hypothesized roles. Run required mise tasks, get independent final Standards and Spec reviews, and publish exact verification evidence in PR #58. |

### Planned checks

These are expected contrasts, not results.

| Counterexample to prevent | Planned model or public assertion |
| --- | --- |
| Aliases or different errors create new registrations; permission leaks through a nested local type. | Compare two true routes to the same declaration and distinct duplicate bodies. Check reference-peeled outer owners, a local wrapper, foreign box with a local argument, and a local error alone. Keep same-spelled application families ordinary. |
| Matching widens a target, breaks correlation or ranks an expected error. | Compare correlated and crossed pairs to bounded ground enumeration. Cover fixed source variant admission and exact target variant checks. Try the prebound-variable case and a source filter that removes a variant-specific winner without changing the accepted pairs in union. A wrong expected or explicit E retains the same winner. |
| A lone candidate chooses To or width, or implicit identity supplies an adapter. | Compare suffixed and unsuffixed sources, immediate and absent target context, a placed explicit source argument, and named function values. Check exact identity versus variant/parent pairs, generic identity intersections, specified numeric gaps versus missing required providers, and no universal fallible identity. |
| Unknown evidence counts as failure or prevents every winner. | Enumerate small proved/disproved/unresolved frontiers. Cover disproved specific candidates, a pending dominator or tie, a dominated pending candidate and settled ambiguity. Include incomplete inventories and charged exhaustion. |
| Display or expected E invents output; a cycle proves itself. | Check registration routes with an independent seed, nested selected errors, competing exact routes and unseeded cycles. Test caller inference from the selected E and prove that uniform body selection alone cannot settle dependent E. Query mismatch after normal selection. Reject a caller-chosen schema even when its body never returns. Compare growth limits to repeated-pair cycles. |
| A symbolic body normalizes an overridable fallback, or hypothetical facts escape. | Check exact symbolic assumptions and concrete specialization with distinct errors. Preserve missing roles and dependencies in source and issued goals. Reject foreign identities and check that exhaustion cannot publish a checked or unconditional negative outcome. |

Before Rust, model pair relations, frontier selection and output-route closure against independent bounded oracles. Before publication, run `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold`. Run `check:ci` only for workflow changes. Preserve earlier evidence and separate document checks, model results, compiler assertions and Rill execution. Full RFC status remains Draft.

### Independent design reviews

Standards and Spec reviewed `e0142f5c9da9bbc5258ed48168675f5b08f7b4e5` against `392d8428a81109b1dc4ed0ad8e60fd4738054522`. The two-file diff had SHA-256 `2fb98c6257ae4d2bdaf7f1628f58fe5b6abc6746b9190f0d2302cc2e4cbe8a79`.

| Axis | Disposition |
| --- | --- |
| Standards | Pin the companion revision and review scope. Correct the missing-target diagnostic in C5, split long RFC paragraphs, and give established-pair requests their own analysis limits. |
| Spec | Distinguish a missing provider from a specified numeric gap. Check filter equivalence per candidate, not over the union. Feed a determined E into the waiting caller and require separate symbolic output evidence. Leave admission at a destination-bound variable as a choice for the owner in V2. |

The corrected checkpoint was checked locally, not independently re-reviewed. At checkpoint publication, no model or compiler work had started. Formal ASD-STE100 vocabulary compliance is unverified.

### M4.7 implementation acceptance

The owner approved V1 through V4 (ri-lqn, comment 6967953617) at `f0b261a887dec3bf6252c40da928335e29bfc602` before model or compiler work. The approval includes V2's exact source capture for destination-bound variables. It does not approve core admission or authorize a merge.

[The acceptance record](../crates/compiler/tests/m4-7-acceptance.md) lists the bounded model and public tests, the independent Standards and Spec findings, and the locally verified corrections. M4.7 leaves actual core admission and integrated typed-stage completion to M4.8. PR #58 remains draft.

## M4.8 design checkpoint

M4.8 ri-dm4 starts from the [M4.7 publication](../crates/compiler/tests/m4-7-acceptance.md) at `965e27dee0bc76ee8bdae922b7fea933e7709391`. The slice stays in [draft PR #58](https://github.com/Reilley64/rill/pull/58). RFC 0001 remains Accepted at its recorded snapshot. RFCs 0002 through 0010 remain Draft. This checkpoint does not approve a new standard-library implementation or enable product commands.

### Owning procedures and completion

| New procedure | Owns |
| --- | --- |
| [RFC 0002](rfcs/0002-modules-and-program-loading.md#proposed-m48-core-admission-and-interface-checks) | Capture-bound admission, public interface visibility and nested declarations. |
| [RFC 0003 result use](rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m48-result-use-checking) and [guard control](rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m44-match-and-guard-control) | Result use through named storage and aggregates, pipe calls, iteration and guard checking. |
| [RFC 0005](rfcs/0005-types-records-enums-and-inference.md#proposed-m48-list-index-and-aggregate-handoff) | List, indexing and aggregate handoff. |
| [RFC 0007](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m48-aggregate-classification) | Implicit-copy classification for borrowed binders and capture demands. |
| [RFC 0008](rfcs/0008-errors-propagation-and-caused-by-wrapping.md#proposed-m48-option-result-and-err-checking) | Canonical Option and Result facts for guards, closed Err, propagation and causal wrapping. |

Pin all unchanged companion sections to [`965e27dee0bc76ee8bdae922b7fea933e7709391`](https://github.com/Reilley64/rill/tree/965e27dee0bc76ee8bdae922b7fea933e7709391). The dependencies are rules for this slice, not changes to those proposals or their status.

| Companion | Rule used here |
| --- | --- |
| [RFC 0001 checking context](rfcs/0001-top-level-expectations.md#declaration-and-checking-context) | Boolean expectation, separate captures and mode-independent static checking. |
| [RFC 0004 collection and bounded proof](rfcs/0004-functions-overloads-and-generic-requirements.md#proposed-m46-inference-and-evidence-procedure) | Shared ordinary selection, context, rigid bodies, exact selected outputs, defaults and no fallback. |
| [RFC 0006 interpolation](rfcs/0006-numbers-strings-and-primitive-operations.md#canonical-interpolation-and-default-formatting) and [numeric protection](rfcs/0006-numbers-strings-and-primitive-operations.md#protected-numeric-conversions-and-explicit-arithmetic-helpers) | Check each interpolation hole through canonical `show` in order. Preserve required numeric providers and specified gaps. |
| [RFC 0007 protected overlap](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m45-protected-overlap-boundary) and [capture facts](rfcs/0007-ownership-borrowing-mutation-and-cleanup.md#proposed-m46-capture-and-invocation-facts) | Required winners and prohibited inputs, typed capture and property prerequisites only. |
| [RFC 0009 conversions](rfcs/0009-user-implementable-conversions.md#proposed-m47-conversion-checking-procedure) | Pair keys, exact capture and error equality, conversion-only applicability and identity gate. |
| RFC 0010 [diagnostic model](rfcs/0010-diagnostics-and-reporting.md#common-diagnostic-model), [source ranges](rfcs/0010-diagnostics-and-reporting.md#source-ranges-and-coordinate-projection) and [driver statuses](rfcs/0010-diagnostics-and-reporting.md#driver-statuses-and-trust-boundaries); [diagnostic registry](diagnostic-codes.md) | Original locations, distinct blocking and advisory findings, and partial outcomes. No renderer or sink work. |
| Library catalogue [§1](standard-library-contracts.md#1-public-paths-and-the-fixed-prelude), [§2](standard-library-contracts.md#2-callable-and-ownership-evidence), [§3](standard-library-contracts.md#3-collections-equality-and-traversal), [§4](standard-library-contracts.md#4-option-result-origins-and-causes) and [§9](standard-library-contracts.md#9-conversion-numeric-compatibility-parsing-and-formatting) | Required core schemas and relationships, not implementation or authority evidence. |

Take one immutable M3 capture. Validate applicable declaration, public interface, layout and admission prerequisites before using their facts. Collect ordinary and conversion headers with their real roles before evidence can use them. Extend the shared checker and its worklist. Do not chain scoped receipts as if each proved its unperformed stages. A settled ordinary winner still does not fall back after a failed requirement. A conversion can exclude a disproved candidate before pair ranking. Neither family can use a conditional premise as an admitted proof.

Account for every loaded declaration and all checkable body roots. Check supported nested declarations even in unexecuted source. Check every loaded module's startup, every expectation and every body, including unused implementations. Use the same typed obligations in both modes. Keep original module and node identity. Missing prerequisites block only dependent checks. Continue independent checks within limits. Include typed capture, aggregate binding and temporary access consequences without granting ownership permissions. Record each remaining unsupported form as a blocked obligation, not a successful skipped body.

Only issue a typed-stage receipt when all required obligations for that capture complete without blocking findings, pending work or conditional premises. Advisory findings do not change acceptance. Use `ResourceLimit` before `Incomplete` before `SourceRejected`, after request validation. Preserve independent findings and known facts on failure. Report only stages actually performed. Typed completion is not ownership, loan, move, transfer, lifetime, disposal, MIR, runtime, codegen, LLVM, editor or product acceptance.

The source tree has no [standard-library implementation](../stdlib/README.md). There is no actual admitted core bundle at this revision. This checkpoint requests approval for the shared gate and testable independent checks. It does not define a trusted bundle format or authenticator. Positive core testing needs actual inputs and a separately reviewed toolchain authority source. The bounded increment can demonstrate failures and independent static facts. It cannot demonstrate admitted List, Result or protected providers with fabricated source or call-site assertions. Leave ri-dm4 and ri-b83 open while any required typed gate remains unverified.

### Proposed public interfaces

Names describe behavior, not Rust spelling. All queries use identities from the same capture. Checker-issued goal IDs also belong to their report. Invalid identity, query shape or provenance yields `InvalidRequest`. Public requests do not accept trust flags, assumed proofs or replacement types.

| Interface | Observable facts and boundaries |
| --- | --- |
| Integrated typed check over the captured graph and bounded limits | Returns a typed-stage receipt or a partial report. Expose per-declaration and per-body status, including startup and expectations, plus findings and prerequisite blockers. Report roles, providers, protection completeness and representation separately. Never read files, retry parsing or create source bindings. |
| Body, original-node and goal queries | Retain inferred type and access, substitutions, authorized evidence, data and control targets. Include List/index, pipe, interpolation, iteration, guards, Err/propagation and result-use facts where established. Preserve conditional premises and report-local identities. No manufactured wrapper, runtime value or transfer permission. |
| Result-use findings and witnesses | Identify the canonical Result occurrence, origin, component path and discard or exit site. Distinguish handled, explicitly acknowledged, forwarded, pending and rejected uses. An incomplete report cannot infer that a component has no Result. |

### Decisions requested before model or Rust

| Choice | Requested approval |
| --- | --- |
| U1. Core authority and unavailable inputs | Validate roles against the capture and keep provider, protection and representation authority separate. Reuse the protected-overlap and conversion procedures under actual admission. Implement the dependent blockers and independent checks first. Positive core acceptance waits for real inputs and their admission evidence; do not import fixtures as trusted core. |
| U2. Result use | Require discriminant/contract observation, explicit acknowledgement or forwarding across a checked boundary. Do not count any read, temporary borrow, new local name, unrelated field access or disposal as use. Check each contained occurrence and all feasible exit paths, preserving unknowns. Using any read as acknowledgement would be simpler but would miss discarded sibling fields and read-then-ignore paths. |
| U3. Closed operations and indexing | Approve the List and index judgment, including literal-only zero-based tuple selectors and exact `u64` List indices. Approve canonical guard, Err, propagation, pipe, interpolation and iteration handoffs. Preserve pending contracts rather than guessing from shape. Checked `get` still returns Option; List index syntax does not. |
| U4. Completion and validation | Approve the interfaces, visibility/capture/aggregate checks and both-mode inventory gate. Use an independent finite model for result-use flow before public-interface TDD. Run final independent Standards and Spec reviews and record exact revisions. A blocked increment is not M4 completion, RFC acceptance or merge authorization. |

### Planned checks

These assertions are NotChecked. See the owning [result-use cases](rfcs/0003-expressions-bindings-and-control-flow.md#proposed-m48-result-use-cases) and [indexing cases](rfcs/0005-types-records-enums-and-inference.md#proposed-m48-indexing-cases) for expected outcomes. The independent model should enumerate finite path histories and compare the result-use worklist to that oracle. Include sibling components, joins, replacements, unreachable source, normal and diverging exits, and loop occurrences. Model result roles are abstract labels, not admitted Rill declarations.

| Counterexample | Public test or model contrast |
| --- | --- |
| A standard-root namesake or an asserted descriptor creates trust. | Reject foreign evidence and check exact captured declarations. Test alias routes, incomplete inventories, protected overlaps and prohibited inputs. Distinguish a required but absent provider, a specified gap and a complete empty action set. Positive admission tests require real inputs. |
| A private type or enclosing variable becomes accessible through metadata. | Compare an opaque signature with a public enum payload, public defaults and requirements, intrinsic predicates, exported aliases and unimported public types. Check nested shadowing and unused nested bodies. Reject startup and named-function runtime capture without rejecting valid arrow context. |
| Field shape or a custom `copy` grants implicit copying. | Compare a plain aggregate, an applicable custom disposer, a protected prohibition and a missing inventory. Test both the borrowed binder's type and the invocation demand of a by-value owned capture. |
| A skipped source construct or incomplete predecessor disappears. | In both modes, check imported startup, an unused body, a test-only import and a non-Boolean expectation. Retain an independent finding after a missing core contract. An advisory alone cannot reject otherwise complete input. Check exact and one-short limits and foreign query identities. |
| An index or callback guesses a result or proof. | Contrast tuple and List contexts, empty List, wrong-width and out-of-range tuple indices, and same-spelled stand-ins. Check original pipe input order, template holes and canonical `iterate`/`next` contracts, including failed selected evidence and missing yield type. |
| Guard or wrapping manufactures a value. | Check exact E, differing success types, borrowed operands, missing outer `display(E)`, cause requirements, wrong Result/Option target and source site attribution. Check non-fallthrough and irrefutable success patterns. Type the RHS without marking it executed. |
| Renaming, copying a fact or reading a sibling hides a discarded Result. | Compare `let _`, bare expression discard, named storage, destructuring, observation and checked forwarding. Compare a committed wildcard with a guard-false arm followed by discard. Include replacement, partial projection, nested Results and unknown storage. Report R0103 only for an established discard. |
| Iteration handles all components when only one has been yielded. | Compare owned and borrowed iterators at `Some`, `None`, `break` and `return`. Keep unyielded owned occurrences or a dependent blocker when the contract does not account for them. |

Use real M3 captures for the public tests. Keep tests that need admitted core contracts blocked until actual inputs are available. Run `mise run test:compiler`, `mise run test`, `mise run check` and `mise run build:scaffold`. Run `check:ci` only for workflow changes. Record model, compiler, document and product evidence separately. No M4.8 model or compiler work has started at this checkpoint. Formal ASD-STE100 vocabulary compliance is unverified.

### Independent design reviews

Standards and Spec reviewed `a3adbdbf1f7fc42022ef9ebe3b8e2949cfc07665` against `965e27dee0bc76ee8bdae922b7fea933e7709391`. The five-file diff had SHA-256 `ea22790f6767d9a4f9e946ab2335886165b98c24d16acdfe009c5ca780de82ac`.

| Axis | Disposition |
| --- | --- |
| Standards | Keep advisory findings nonblocking. Give guard checking one owner. Pin the exact diagnostic and catalogue sections. Add traceable NotChecked cases. Split long RFC paragraphs and distinguish the tracked location from the source site. |
| Spec | Keep positive admission blocked until an authority source exists. Commit pattern acknowledgements only on the guard-true edge. Track unyielded iterator components. Check intrinsic predicate identity without a public path. Classify aggregate binder and capture demands with disposal and protection evidence. Require owned input and outer display evidence for infix wrapping. Keep advisories nonblocking. |

The checkpoint corrections were checked locally, not independently re-reviewed. At that point, model and compiler work still required owner approval.

### M4.8 partial implementation

The owner approved U1 through U4 (ri-dm4, comment 6969612205) at `f3d65f72062237f87372f1896db5f2a66d9f2a54` before model or compiler work. The [M4.8 verification record](../crates/compiler/tests/m4-8-acceptance.md) lists the partial report, finite model, local checks and exact reviewed and verified revisions.

Actual core admission still requires a shipped core implementation and approved authority source. Compiler Result-use checking, the remaining core-dependent checks and a typed-stage receipt are not established. Issues ri-dm4 and ri-b83 remain open, and PR #58 remains draft and unmerged.

### M4.8 completion checkpoint: first-party core authority

This proposed checkpoint follows the partial publication at `4f5de4bc153979949282953aad66a552fc700993`. The U1 through U4 approval still covers the agreed independent checks and Result-use procedure. It did not select a production authority source. [RFC 0002's first-party authority proposal](rfcs/0002-modules-and-program-loading.md#proposed-first-party-core-authority-for-m48-completion) owns the new procedure. All other M4.8 rule owners and their scope remain as pinned above.

| Decision for approval | Implementation and verification |
| --- | --- |
| A1. First-party pins are the authority. | Start with `aarch64-apple-darwin` and an empty option map. Pin exact first-party source bytes and inventories. The installed loader derives its root from the running executable and attaches private evidence to its graph only. The source-only loader cannot admit anything, even with copies of installed bytes. Use declared metadata triggers and preserve the source rules for discovery and dispatch. |
| A2. Deliver actual implementations. | Supply and package the [named core groups and providers](rfcs/0002-modules-and-program-loading.md#actual-providers-and-closed-inventories). Keep roles, providers, protection and representation separate. Native List/iterator storage, owned error state, and protected guard, counted-owner, endpoint and retention actions need matching implementations wherever their contracts rely on native code. That delivery is an implementation prerequisite; signatures and placeholder bodies do not substitute. Runtime execution, ABI acceptance and release remain separate. |
| A3. Check the installed capture through the shared checker. | Build and stage the real bundle for the [installed tests](rfcs/0002-modules-and-program-loading.md#planned-checks). Use the installed capture and `analyze_typed` interface. Implement source-level Result-use witnesses under U2. Exercise the planned U3/U4 cases and all loaded roots in both modes. Require positive typed receipts as well as invalid, rejected, incomplete and exhausted outcomes. Run those tests, the four mise gates and independent Standards and Spec reviews before publication. |

Standards and Spec independently reviewed `38942a0c5ed522d61e832de66b52e50f92ecd5d2` against `4f5de4bc153979949282953aad66a552fc700993`. The two-file diff has SHA-256 `65a344a7bed1debe302b629bba84139854af0c9c4b8bd08dded9b02e19c93a73`. Standards requested exact dependency links, individual expected outcomes and shorter prose. Spec requested the initial profile and root selection, actual provider ownership and coverage, reproducible installation, declared metadata triggers and separate provider/protection facts. The corrections address those findings; they are not independently re-reviewed.

The owner approved A1 through A3 (ri-dm4, comment 6971033745) at `3c7e9bb0664ecb866630fbc1f349e333aef57ac5` before implementation. The [installed Option checks](../crates/compiler/tests/m4-8-acceptance.md#installed-option-checks) record the first increment. The [integrated installed checks](../crates/compiler/tests/m4-8-acceptance.md#integrated-installed-core-slices) record the later partial publication. Both load modes reported `Incomplete` at that revision.

The [verified closeout revision](../crates/compiler/tests/m4-8-acceptance.md#verified-closeout-revision) records the A3 checks of the expanded entry. The staged `aarch64-apple-darwin` profile with empty options issues a receipt in both modes. This is static compiler evidence only. Keep ri-dm4 and ri-b83 open and PR #58 draft until the owner decides otherwise.

## Completion record

The following M1 statement belongs to the original planning baseline. M1/M2 later merged as recorded above; M3 evidence is separate.

M1 implementation: Not started. Acceptance matrix: NotChecked. No parser, lexer or source-mapper result is claimed by this planning document.

The documentation baseline records existing policy decisions without upgrading full RFC statuses, evidence labels or target/client support. The implementation ticket will record its exact base and later observed evidence separately.
