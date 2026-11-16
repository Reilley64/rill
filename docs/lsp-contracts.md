# Complete Rill LSP support for OpenIDEA v0.1.44

Confirmed [#24](https://github.com/Reilley64/rill/issues/24), [ADR 0016](adr/0016-structured-diagnostics-and-complete-openidea-lsp.md); [prioritized delivery gate #42](https://github.com/Reilley64/rill/issues/42). This is a **design contract and unpassed acceptance matrix**, not a working compiler/server/client certification. [Diagnostic/report contracts](diagnostic-contracts.md) define the common model. [Pinned research](research/openidea-language-server-capabilities.md#release-baseline-addendum-openidea-v0144) distinguishes release facts from older development evidence.

## 1. Scope, protocol and readiness

Target OpenIDEA **v0.1.44**, commit `bf36cc08019a1be78c151245c67dcd872a9c37b5`. Use LSP 3.17 core plus explicitly pinned, capability-negotiated newer used methods such as 3.18 inline completion; no blanket newer-protocol conformance claim. The relevant first-party protocol snapshot is `microsoft/language-server-protocol@ba1f1947af19dbf3c33fc763eadd7e7848f80766`, as linked by the research. Advertise actual implementation only; capabilities/empty replies/source wiring are not acceptance.

Cover **every applicable LSP family actually used by this client**, plus minimal `.rill` detection/language-id/server registration. Not a reduced diagnostics/hover/definition/completion milestone. Native server acceptance hosts: macOS/Linux × x86-64/AArch64; identify and test supported local/remote arrangements. No implicit Windows-native support or every conceivable client/remote pairing. Native/Wasm analysis contexts do not establish backend support; #30 remains separate. Analyzed program stack budgets are not Rust-server resources.

Non-LSP grammar/syntax/plugin folding/selection/colors, IDE Run/Test/DAP integration and new notebook/kernel/cross-cell semantics are outside this commitment. Ordinary Rill cell text can get loose/untitled assistance if delivered as text; do not advertise a notebook model that does not exist. A missing client LSP route and semantic non-applicability are different matrix classifications. Explicitly justify either; do not use no-methods/no-traits to omit enum/call hierarchies.

Full agreed support is a **v0 tooling-readiness gate**. #24 design closure is not delivery. A completed server is not a prerequisite for drafting #28 RFCs. #28 owns exact language/checker/lowering/observation rules; required inconsistencies return to the map, not weaker scope, a silently dropped target or an assumed client/backend architecture.

## 2. Launch, settings and contexts

The native toolchain provides `rill lsp`, stdio LSP by default: framed JSON-RPC on protocol stdout, operational logs on stderr/supported channels, not CLI JSON Lines. Minimal OpenIDEA registration maps `.rill`/language id `rill` to this executable/argument and required process approval. Exact registration must pass the pinned client's loader; declaring a language does not install a grammar. No automatic download/installer/workspace hook. Native executable approval is not sandbox proof.

Settings identify named contexts `(id, entry path, target, compiler options)`, an optional applicable default, optional document selections and standalone defaults. Entry paths may be explicitly workspace-relative or absolute settings, not Rill import syntax or a new project manifest. Launch working directory/root markers/workspace folders do not redefine the compiler's application origin. Toolchain std origin comes from selected trusted toolchain metadata, never a homonymous application directory.

A document selects an explicitly chosen containing context, else applicable configured default, else its sole containing program. Multiple remaining candidates require selection with configuration assistance, not arbitrary merging. Named unconfigured application files become identified standalone entries rooted at their directory with configured default target/options. Unlocated buffers get local syntax/assistance without guessed entry-dependent facts. Installed std source remains read-only and keeps its isolated origin even when opened directly. An unavailable necessary default/context is not a passed check.

Entry-directory application roots, bounded self/super, std isolation, canonical case-sensitive identity, item-site exports, separate namespaces and loaded-graph collection remain #17 rules, qualified by #19/#23. Each context has its own loaded graph/overload set and input snapshot. Indexing unrelated source for search does not load implementations or execute/startup-check it as part of that program. Test-only imports still participate in the real graph. A document's diagnostics, completion and navigation share its chosen context; searches disclose context/bounds.

## 3. Analysis and freshness

Reuse compiler front-end facts, no LLVM executable generation/startup/expectation/user-formatter execution. Equivalent inputs get equivalent front-end semantics; do not invent live backend checks. Parse/type errors allow trustworthy recovery in unaffected regions, without numeric defaults, assumed requirements or stale type guesses. Incomplete analysis is not successful checking.

Snapshot identity covers text versions and relevant graph/config/toolchain dependencies, not just the initiating file. Debounce/cancel superseded work, invalidate caches/resolve data on changes and reject known-stale results. Indexing/long operations use real progress/cancellation without hard latency promises. Required unknowns must remain visible; mutating refactors need sufficient complete searches, not merely a read-only best effort.

For this client, prefer document pull for open files; use push for closed loaded modules and clients lacking pull. Support previous-result identity/full/unchanged and refresh as negotiated; dependency changes trigger appropriate refresh. Clear old ownership/diagnostics on open/close/graph/context transitions and avoid duplicate producers. Do not assume workspace pull or consumed relatedDocuments. Server known-snapshot correctness does **not** promise this client instantly erases old underlines: its stale-push/retained-display behavior is a tested limitation, not a server success guarantee.

Common rich diagnostics retain labels/notes/docs/suggestions. Emit standard related metadata but include essential explanatory text/compact related locations in the message where this release drops them. One problem is not multiple fabricated errors. Use exact original UTF-8→UTF-16 mapping, not byte offsets as editor columns. Non-source failures use supported operational notification/logging, not fake source ranges.

## 4. Meaningful source-language behavior

### Identity, navigation and search

Later confirmed [#41](https://github.com/Reilley64/rill/issues/41#issuecomment-6886119679) uses `implement` (generic binders before the family name) for family bodies; standalone functions/body-less declarations still use `function`. Tokens/outlines/actions must preserve that distinction. Canonical conversions alone use (To, From) implementation keys and selected E evidence, not ordinary return-only dispatch. Respect their endpoint ownership/protected numeric/identity rules and camelCase tryInto/tryFrom aliases; ordinary copy/dispose/orphan rules are not replaced by conversion-specific ones.

Bindings/types/fields/variants navigate to actual source. Declaration finds canonical family origin; Definition finds the uniquely selected body where known, else declaration/clearly identified candidates. Implementation exposes relevant loaded bodies, not a fictitious runtime dispatcher or chosen body for unresolved evidence. Import/re-export aliases preserve canonical identities; source navigation does not grant private construction/access.

References/highlights use resolved identities and read/write facts, not text matching. Rename alias bindings/routes and their references without renaming the origin; canonical family rename handles declaration/bodies/uses coherently. Distinguish same-spelled fields/shadows/namespaces and coupled public type/value source tokens. Check affected configured contexts, read-only sources and conflicts; do not promise unconfigured external-program coverage or partially rename through unresolved required searches.

Document symbols provide source hierarchy and distinguish families/bodies. Include useful startup/expect/closure regions without source-level main fiction. Workspace search can index outside the active graph but labels scope/context and distinguishes declarations from checked facts. Return eager concrete ranges; this client advertises workspace-symbol resolve without a reliable implementation.

### Hierarchies and assistance

Type hierarchy uses nominal enum parent/variant edges at declaration level, keeping generic parameters generic. No inheritance edges for aliases, fields, numeric conversions or where clauses. Call hierarchy uses source calls/known implementation dependencies, identifies known implicit calls and unresolved generic/indirect targets. No invented possible targets, monomorphization explosion, complete runtime-call-graph claim or eliminated-tail reconstruction. Preserve source-position re-preparation identities used by this client.

Completion/signature/hover share compiler visibility, inference, callable mode and requirement evidence. Offer useful partial-typing candidates without claiming unknown requirements satisfied. Qualified variants/named constructors/public fields/import paths must be legal; no private-field construction, singleton types, numeric defaults or broad std prelude. Inferred/opaque/uninferred types are distinct; private type display is not permission to insert an illegal annotation. Auto-import edits use real aliases/imports, not assumed implicit bindings.

Inline completion is deterministic contextual insertion, not required AI/program-logic invention. Use real plain insertion behavior; this client flattens inline snippet defaults rather than providing normal interactive tab stops. Ordinary completion/resolve may use supported snippets/additional edits; do not depend on an unconditional resolve when data/documentation permit the client to skip it.

Semantic tokens use meaningful supported Rill kinds/modifiers, not invented classes/methods. Do not equate readonly/mutability with copying/noBorrow/sharing. Default inlays favor useful inferred types/non-obvious parameter names, not repeated explicit annotations; provide eager complete labels/tooltips/locations rather than unsupported resolve/edit acceptance. Full tokens are supported here, not assumed range/delta/multiline/overlap capabilities.

Lenses need working commands; baseline `rill.checkContext` rechecks the chosen context and refreshes diagnostics/progress, without backend/user execution. Namespaced commands validate context/snapshot/arguments and remain analysis/refactoring-only, not arbitrary shell/Run/Test/Debug dispatch. No no-op lens or assumption that returned locations open UI; avoid redundant count lenses over client code vision. The release renders command-bearing numeric count lenses, unlike the older development filter.

Document links use resolvable imports, available source and explicit documentation URLs with encoded safe URIs. No automatic network fetch, executed contents or guessed runtime path-string interpretation. Intrinsics without source are described honestly. Virtual-source content requires demonstrated client scheme/lifecycle/freshness; prefer available read-only file sources and never parse runtime formatLocation prose into tooling locations.

## 5. Imports, formatter and source-changing actions

Organize-imports preserves loaded graph, implementation sets, public bindings and expectations. Textually unused is not removable proof. Remove redundancy only when established safe; sorting cannot discard semantic module loads. Auto-import identifies provider and material resolution changes, uses valid explicit aliases and refuses known conflicts; it is not automatically behavior-preserving cleanup.

One deterministic document/range formatter: two-space fallback, soft 100 columns, honor supported client indentation. Preserve comments/literal spelling/content/widths/required syntax/meaning. No constant folding, ownership rewrite/import deletion or other semantic cleanup. Range edits expand only as necessary to a related syntactic unit. Format parseable code despite type errors; unsafe unrecoverable regions stay unchanged with explicit limitation. On-type formatting is not a claimed client LSP route.

Meaningful actions include evidence-backed fixes/imports/organize-imports/compatible safe fix-all; variable/free-function extract/inline; applicable signature/declaration moves; generation such as missing enum cases. Preserve evaluation order/suspension, owners/loans/cleanup/joins, dispatch/privacy/generic requirements; named extractions take explicit state. Mark unfinished scaffolds and behavior-changing choices (copy/Shared/error adaptation) honestly. Do not invent user captures, lifetime syntax or new type capabilities.

Require fresh sufficient facts, source versions and supported invocation/edit paths; unrelated errors need not disable every action. No partial refactor through an incomplete required search. Operations requiring unsafe entry-root/config/visibility changes need explicit supported handling or refusal, not silent re-rooting/permission bypass. Missing required workflows stay open readiness gaps, not stubs or invented generic interactive forms.

This client negotiates **abort**, not transactions. Test open/closed version guards, computation-to-preview windows, dirty files, resource options/URI conversion/annotation loss, live failures and failed rollback. An apply failure may leave changes; command failure after an applied edit does not undo it. Do not rely on dropped annotations for confirmation or claim unconditional atomicity/freshness. Respect actual capabilities; an unworkable required contract returns for a scope/mechanism decision, not silent feature deletion/client fork.

## 6. Lifecycle and trust

Versioned open/change/save/close, configuration/watch/refresh, progress/cancellation, graceful shutdown/exit and restart have explicit behavior. Source/config failures recover; internal failure is not a source error or success. Fail/restart if state untrusted, resync and invalidate old request/action data without overwriting unsaved work. Where the client cannot signal a relevant workspace/config change, require explicit reinitialization/refresh/restart. No hard responsiveness/time bounds.

Honor project trust and process approval/pinning. Discover configured origins/approved source/toolchain data, not arbitrary setup/build scripts. Documentation/links/commands remain validated/inert except explicit supported actions. Do not claim native-process sandboxing. Guest --stack-size is not a Rust-server resource setting.

## 7. Versioned acceptance matrix

**Every row is NOT RUN.** Owners indicate responsibility, not delivery. FE = shared Rill front end; LS = native Rill server/adapters; REG = minimal client registration. Each test dossier must record client/protocol/server/toolchain revisions, source/config snapshots, host/local-remote arrangement, actual provider/request trace and reproducible observable result. A successful menu backed by a host fallback is not proof the LS passed.

| ID / applicable family | Expected Rill result and representative acceptance cases | Owner / release constraint | Evidence |
| --- | --- | --- | --- |
| D1 push/pull diagnostics | Move/borrow/type/requirement errors have correct code/primary/reason; related-message fallback without fabricated extra errors | FE+LS; related labels/docs dropped in UI | NOT RUN |
| D2 freshness/refresh | Unsaved/dependency edits, late answers, graph/context/open-close handoff clear/update correct producer; canceled results cannot restore known-old facts | FE+LS; open pull only, stale push can display | NOT RUN |
| C1 completion/resolve | Visible names/variants/constructors/signatures, access/mode/requirements, valid explicit imports; stale/unknown data refused | FE+LS; resolve may be skipped unless needed | NOT RUN |
| C2 inline completion | Useful deterministic contextual suffix/template, no AI/evaluation, correct insertion/range | FE+LS; 3.18 method, flattened snippet defaults | NOT RUN |
| C3 signatures/hover | Relevant overloads/active parameter/docs/types, opaque/unknown explicit, no invalid source suggestions | FE+LS; invocation context/type-info extraction limits | NOT RUN |
| N1 definition/declaration/type/implementation | Canonical family/body/alias/type/variant targets, generic candidate uncertainty, read-only sources | FE+LS; UI declaration action may call definition | NOT RUN |
| N2 references/highlights | Identity/read/write/scope across imports, shadows and same-named fields; incomplete searches disclosed | FE+LS; no assumed generic partial streaming | NOT RUN |
| S1 document symbols | Hierarchical real source declarations, distinct overload bodies and useful execution/closure roots | FE+LS | NOT RUN |
| S2 workspace symbols | Scoped/index-labelled results, concrete eager ranges, no graph merging or fake checked facts | FE+LS; workspace resolve unwired | NOT RUN |
| A1 fixes/resolve/commands | Valid proof-backed fix or explicit behavior choice/scaffold; freshness preserved through resolve, no user code | FE+LS; actual generic command route | NOT RUN |
| A2 organize/add imports | Preserve implementations/public bindings/expectations; real alias syntax, graph changes disclosed, collisions refused | FE+LS | NOT RUN |
| A3 fix-all | Compatible mechanical edits only, preserve earlier fixes and semantics, no hidden ownership changes | FE+LS | NOT RUN |
| A4 extract/inline | Variables/free functions preserve order/effects/suspension/lifetimes; explicit state, no named captures | FE+LS; supported UI invocation required | NOT RUN |
| A5 signature/move | Applicable signature/declaration change with complete known caller/context edits, conflict/privacy/root safety | FE+LS; no assumed generic input dialog | NOT RUN |
| A6 generation | Qualified exhaustive enum scaffolding/named construction without guessed widths or fake completed bodies | FE+LS | NOT RUN |
| R1 prepare/rename | Aliases/families/fields/dual namespaces, conflicts and read-only/unresolved scopes; fresh multi-file preview | FE+LS; prepare placeholder loss | NOT RUN |
| R2 will/did file rename/resource edits | Import updates and known context/root handling; safe refusal where required, valid encoded URIs/options | FE+LS; only rename notifications established | NOT RUN |
| R3 adverse edit outcomes | Open/closed/dirty files, changed computation/preview, dropped annotations/options, preflight/live/rollback/command failures | LS+client acceptance; abort not atomic | NOT RUN |
| F1 document/range formatting | Deterministic client indentation/layout; preserved literals/comments/semantics; safe recovery/minimal related edits | FE+LS; no on-type route | NOT RUN |
| T1 full semantic tokens | Correct kind/modifier/legend/relative UTF-16 encoding, edit/refresh, no invented facts | FE+LS; no range/delta/multiline/overlap assumption | NOT RUN |
| H1 eager inlay hints | Useful exact inferred types/parameter labels/opaque handling, viewport/refresh/navigation | FE+LS; no resolve/text-edit acceptance | NOT RUN |
| L1 lenses/resolve | Actual context-check command revalidates/refreshes/progresses, no no-op/Run/Test/Debug or duplicate noise | FE+LS; command-bearing results only | NOT RUN |
| L2 links/resolve | Import/library/docs targets work with safe encoded URIs and no fetch/exec; no invented intrinsic body | FE+LS | NOT RUN |
| H2 call hierarchy | Source callers/callees/implicit edges, honest generic/indirect uncertainty, recursion/source re-prepare | FE+LS; not runtime trace | NOT RUN |
| H3 type hierarchy | Enum parent/variant edges and generics, aliases not inheritance, no class-only omission | FE+LS; source re-prepare | NOT RUN |
| V1 applicable virtual content | If needed, correct read-only source version/scheme/context, no stale content; physical sources preferred | FE+LS; verify scheme, no automatic arbitrary URI support | NOT RUN |
| P1 synchronization/coordinates | Full/incremental versions, Unicode/CRLF/tabs/escaped file URIs/unrepresentable range handling | FE+LS; UTF-16 advertised only | NOT RUN |
| P2 context/config/watch | Selected/default/sole/ambiguous contexts, standalone/untitled/std, dependency-only changes and cache invalidation | FE+LS; no assumed live workspace-folder notification | NOT RUN |
| P3 lifecycle/progress/cancel | Initial sync, long work/cancel, source vs internal error, restart old data, shutdown/exit, protocol-only stdout | LS+client acceptance | NOT RUN |
| P4 trust/registration | .rill/id/executable launch, approval/refusal/update, local/remote roots; no automatic install/hooks/user evaluation | REG+LS | NOT RUN |
| P5 host/config coverage | macOS/Linux x86-64/AArch64 and explicitly supported remote arrangements, native/Wasm analysis labels | LS+client acceptance; no backend certification | NOT RUN |

### Explicit classifications

| Surface | Classification at this baseline |
| --- | --- |
| Generic folding/selection/colors/on-type, linked editing/monikers/inline values, token range/delta, workspace pull, create/delete will/did notifications | No applicable generic client request/UI chain established; do not infer it from a filter/acknowledgement or add non-LSP plugin work |
| Workspace symbol resolve, rich diagnostic display fields, inlay resolve/edit acceptance | Client-specific absent/partial path; use documented eager/fallback behavior, not a silent claim of support |
| Java member hierarchy/gopls interactive commands and other language-specific adapters | Not generic Rill APIs; applicable Rill actions still need real generic workflows |
| Notebook/kernel/cross-cell execution, Run/Test/DAP, non-LSP syntax/grammar | Explicitly outside current language/file-model commitment, not new Rill semantics |
| Windows-native server/all remote combinations | Not implicit host acceptance targets; additional deliberate scope |

No row is passed by this document or [hand-authored prototype 17](prototypes/17-diagnostics.md). #42 must carry every outstanding obligation until complete or explicitly transferred to an open successor with human scope approval. Missing mechanisms return to the map; RFC drafting and future RFC acceptance remain distinct from delivered tooling.
