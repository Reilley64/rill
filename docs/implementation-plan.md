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
| M5. Ownership and cleanup lowering | `crates/compiler/` | First complete dependency transfer, branch/loop, exit, capture and initialized-cleanup judgments. Validate MIR obligations and adversarial accepted/rejected cases. |
| M6. Execution and tools | Backend/runtime plus IDE/LSP/CLI owners | Implement only after each semantic/ABI prerequisite is complete. Native/Wasm/provider support and full ri-bv2 client acceptance retain separate gates. |

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

## Completion record

The following M1 statement belongs to the original planning baseline. M1/M2 later merged as recorded above; M3 evidence is separate.

M1 implementation: Not started. Acceptance matrix: NotChecked. No parser, lexer or source-mapper result is claimed by this planning document.

The documentation baseline records existing policy decisions without upgrading full RFC statuses, evidence labels or target/client support. The implementation ticket will record its exact base and later observed evidence separately.
