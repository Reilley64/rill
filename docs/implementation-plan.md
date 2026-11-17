# Compiler implementation plan

## State and authority

The owner authorized this plan and a committed documentation baseline on 2026-11-17. Compiler milestone M1 has not started. This plan defines implementation scope and acceptance, not new language semantics or completed validation.

Use the committed baseline containing this plan, not a moving branch, as implementation input. Record its full commit SHA in the implementation ticket before coding. The baseline preserves Draft RFC statuses and their scoped human policy approvals.

[#28](https://github.com/Reilley64/rill/issues/28) retains the RFC collection, coverage/readiness and existing formal/production follow-through. Its writing deliverable does not become compiler implementation. Create and claim a separate implementation ticket when coding is authorized.

## Inputs

| Input | Applicable scope |
| --- | --- |
| [Development guide](development.md), [ADR 0026](adr/0026-toolchain-repository-and-mise.md) | Private crate ownership, pinned mise/Cargo/uv tools, LLVM-free analysis and unavailable product gates. |
| [Lexical/source approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825) | Exact source/lexer policies across RFCs 0002, 0003, 0006 and 0010. |
| [Source snapshots](rfcs/0002-modules-and-program-loading.md#source-snapshots-and-lexical-input) and [tokens/trivia](rfcs/0003-expressions-bindings-and-control-flow.md#lexical-tokens-and-trivia) | Immutable original bytes, source spans, identifiers, comments and longest token matching. |
| [Literal scanning](rfcs/0006-numbers-strings-and-primitive-operations.md#literal-scanning-and-token-boundaries) and [coordinate procedure](rfcs/0010-diagnostics-and-reporting.md#original-byte-coordinate-procedure) | Numeric/raw/template modes, controls, decoded provenance and byte/scalar/editor projection. |
| [Syntax approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6931939690), [annex](grammar/v0-syntax-v1.md), [EBNF](grammar/v0-syntax-v1.ebnf) | M2 parser policy. M1 preserves lexer tokens rather than applying parser commitment rules. |
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
| M5. Ownership and cleanup lowering | `crates/compiler/` | First complete dependency transfer, branch/loop, exit, capture and initialized-cleanup judgments. Validate MIR obligations and adversarial accepted/rejected cases. |
| M6. Execution and tools | Backend/runtime plus IDE/LSP/CLI owners | Implement only after each semantic/ABI prerequisite is complete. Native/Wasm/provider support and full #42 client acceptance retain separate gates. |

M2 through M6 are sequencing markers, not detailed implementation tickets or claims of ready specifications. Their missing rules do not block M1. Full standard-library or LSP delivery is not a lexer prerequisite.

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

This records Stage 1 of [M3 ticket #54](https://github.com/Reilley64/rill/issues/54). The owner subsequently [approved checkpoint a41e7cc](https://github.com/Reilley64/rill/issues/54#issuecomment-6939478619), authorizing Stage 2 within O1 through O7 and the three public seams. The earlier M1 planning statements remain historical. M1 and M2 have since merged through [#51](https://github.com/Reilley64/rill/pull/51) and [#53](https://github.com/Reilley64/rill/pull/53).

The implementation branch is `implementation/m3-loader-resolver`. Its exact starting commit is `04fd25ff9e8b4e02621802a111263947dbeb53b0`, fetched from origin/main after #53 merged. That commit contains M1 `7379a28` and M2 `6ea5fdd`. The original plan baseline is `e1559249c325c3898f506b271e91d76b884be290`. The ticket links the plan at the exact implementation start.

[RFC 0002's proposed procedure](rfcs/0002-modules-and-program-loading.md#proposed-m3-resolution-procedure) owns candidate discovery, route reduction, namespace installation, identities, cycles and graph capture. The checkpoint approval covers these scoped additions to a Draft RFC. Its existing identity/order, lexical/source and syntax approvals remain unchanged. This approval authorizes only the identified M3 contract, not the complete RFC set or #28's readiness obligations.

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

Each slice includes its own red-green filesystem cases. Preserve existing M1/M2 source and tests. Any necessary earlier-stage fix needs a separate regression test and explanation in #54 and the PR.

### M3 acceptance checklist, not observed evidence

The names below are planned test behaviors, not existing tests or passing results. Replace them with exact implemented test names and observed outcomes in #54 and the completion record. RFC case numbers here refer to module cases M1 through M15, not compiler milestones.

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

The [M3 acceptance record](../crates/compiler/tests/m3-acceptance.md) maps actual Rust tests to RFC 0002 M1 through M15, identity/order policies and the approved scope. Its later-stage column remains unpassed. It also records real filesystem limitations, the additive M2 parser-work statistic and unrun support gates. #54 and the M3-only draft PR retain command results and implementation revisions. A resolved graph is not a successfully checked program.

## M4 scope and design checkpoint

### Starting point and authority

[M4 umbrella #56](https://github.com/Reilley64/rill/issues/56) is a separate implementation ticket from #28. It is claimed by Reilley64 and remains open. No partial slice completes M4.

PR #55 merged on 2026-11-17 at 07:58:38 UTC. After fetching origin, `origin/main` was `ee12588f2cba912025900e1ac587586fb9cb6235`. The fresh branch `implementation/m4-typed-foundation` starts at that exact commit. It includes M3's reviewed source revision `862704bea6af46e1446b3b7942e690f877b116c1` and completion record `d05a0b559e6ee19f2e48d2e0f8b4719469d65f3e`.

The starting commit includes both mise tasks. `test:compiler` runs compiler unit, integration and documentation tests. `test` runs the implemented LLVM-free compiler, IDE, LSP and CLI Rust suites. Neither task executes complete Rill product acceptance.

M1 supplies immutable snapshots, spans, lexical facts and decoded provenance. M2 supplies strict source-preserving syntax without type-directed parsing retries. M3 supplies a captured resolved graph, installed bindings, canonical declarations, family attachments and source routes. Its [public interfaces](../crates/compiler/README.md#m3-loader-and-resolver) and [acceptance record](../crates/compiler/tests/m3-acceptance.md) retain their narrower guarantees.

M3 explicitly deferred expression/type/local-use resolution, signature compatibility, public-requirement checking, captures, layout, overloads, conversions, coherence and protected operations. Those deferrals are current M4/M5 prerequisites, not failed M3 acceptance. Older absent-implementation statements in RFC evidence tables remain historical records.

RFC 0001 remains Accepted at its recorded snapshot. RFCs 0002 through 0010 remain Draft with their existing scoped approvals. The owner subsequently [approved O1 through O4](https://github.com/Reilley64/rill/issues/56#issuecomment-6943516603) at checkpoint `71cbe3707814114b0f504f0458ada41d54c2593d`. This authorizes only M4.1 and its three public TDD seams. The original checkpoint claimed no implementation evidence.

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

Create and claim an M4.1 slice ticket under #56 after scope approval. Link the exact approved checkpoint and the starting commit. Only the first approved slice is authorized for implementation.

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

[Slice #57](https://github.com/Reilley64/rill/issues/57) records the approved M4.1 work. The [M4.1 acceptance record](../crates/compiler/tests/m4-1-acceptance.md) maps real tests to the checkpoint requirements. It preserves unimplemented checks and later-slice boundaries. It does not complete M4 or enable any product command.

## M4.2 design checkpoint

### Delivery and current baseline

The owner requested completion of all M4 slices in existing PR #58. This supersedes separate slice PRs and intermediate merge gates. It does not approve unwritten language policies or unreviewed procedures. PR #58 remains draft and unmerged; #56 remains open.

[M4.2 ticket #59](https://github.com/Reilley64/rill/issues/59) starts design work from branch commit `f405adaa01058849025105b11ed77fe32f6271b7`. M4.1 implementation revision `7602ed55f8b344646218e8cc0af8089bfee18efb` provides its input facts. The overall M4 starting main remains `ee12588f2cba912025900e1ac587586fb9cb6235`.

M4.1 has completed scoped implementation review and acceptance evidence, including four hosted scaffold profiles. Its integration remains pending as part of M4. No intermediate merge is needed before M4.2 design or subsequently approved implementation. The later documentation commit records hosted results without claiming it was itself already tested on those hosts.

No M4.2 compiler/model code or tests have been written. The new procedures and choices below await scoped owner approval. Full RFC statuses and earlier approvals remain unchanged.

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

| Decision | Recommendation and consequence |
| --- | --- |
| D1. Type positions | Permit void and never in every existing type position. Keep inhabitation and control-flow checks separate. This permits declarations such as UnitHolder with a void field and BottomHolder with a never field, without inventing an initializer or constructible BottomHolder value. |
| D2. Stored variants | Use the parent enum application's inline representation for stored variant types. Thus Choice<Node>.Empty forwards the parent enum's stored Node dependency and rejects the proposed Node example. A selected-payload representation would produce a different result and needs different rules. This is a new policy choice, not prior confirmed behavior. |
| D3. Missing representation contracts | Admit no nominal indirection exemptions in this slice. Actual source references stop traversal; names and private fields do not. Directly stored callables without a justified representation contract return Incomplete. Callable signatures do not become capture storage. Keep that obligation assigned to later callable/core integration before M4 completion. |
| D4. Scope, contracts and seams | Approve the two owning procedures, exact prelude schemas, three public seams and scoped completion boundary. Defaults, requirements and implementations retain M4.1's incomplete behavior. Approve the model-first validation and compiler TDD sequence below. |

These recommendations complete a proposed procedure, not an approved language rule. Do not implement a recommendation before its owner decision. A changed decision requires corresponding owning-text and acceptance revisions.

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

## Completion record

The following M1 statement belongs to the original planning baseline. M1/M2 later merged as recorded above; M3 evidence is separate.

M1 implementation: Not started. Acceptance matrix: NotChecked. No parser, lexer or source-mapper result is claimed by this planning document.

The documentation baseline records existing policy decisions without upgrading full RFC statuses, evidence labels or target/client support. The implementation ticket will record its exact base and later observed evidence separately.
