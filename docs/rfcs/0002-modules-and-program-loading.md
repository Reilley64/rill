# RFC 0002: Modules and program loading

| Field | Value |
| --- | --- |
| ID | 0002 |
| Status | Draft |
| Owner | Reilley64. ri-48g tracks this draft. |
| Source inputs | Confirmed module decision ri-owe (comment 6866705217). [Source reconciliation](#source-reconciliation) lists later refinements. Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The confirmed policy decisions (ri-48g, comment 6908371073) supplement those inputs. |
| RFC dependencies | [RFC 0001](0001-top-level-expectations.md) owns expectation behavior. Its accepted snapshot (ri-48g, comment 6907867211) has SHA-256 `ee4980216dd2ed051cb6153600bf8b5f6c039145c083abfeeb37119d027bd954`. Other required companions appear [below](#required-companion-rules). |
| Replaces / superseded by | None. This draft does not replace or amend RFC 0001. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes one file per module, bounded application and standard-library origins, explicit imports and item-site exports. It defines canonical module identity, graph loading, namespaces and source visibility. It also defines the declaration set available to later checking.

[RFC 0001](0001-top-level-expectations.md) owns expectation placement, execution and failure reporting. This RFC supplies its module identities and ordering. It does not duplicate those test rules.

This draft does not define overload ranking, general type inference, enum operations or the ownership checker. It records their required boundaries and missing companions. M3 now implements the approved module-resolution subset. A typed checker and module runtime remain absent.

Module identities are stable within a selected analysis context. Contexts do not merge their graphs.

## Motivation

A compiler needs one answer to three questions. Which files belong to the program? Which declaration does a name identify? Which representation can the caller access?

Aliases must not create new nominal types or overload families. Repeated import paths must not duplicate implementations or tests. A workspace search index must not change program meaning.

Resource interfaces also need precise privacy. Exporting a factory must not expose private fields or grant ownership of its return type's declaration. Compiler metadata must support checking and cleanup without granting those source permissions.

## Guide-level explanation

The following complete three-file example uses a facade. It illustrates proposed source behavior, not an executed program.

```rill
// geometry/point.rill
export type Point = { x: i32; y: i32 };

export function origin(): Point {
  return Point { x: 0, y: 0 };
}
```

```rill
// api.rill
export import geometry::point::{Point as Position, origin};
```

```rill
// main.rill
import api::{Position, origin};

function originX(): i32 {
  let point: Position = origin();
  return point.x;
}

expect originX() == 0;
```

Selecting `main.rill` makes its directory the application root. `geometry/point.rill` defines a module. The `geometry` directory supplies a path segment, not a module facade.

`Position` identifies the original `Point` type. The re-export does not create a new type or change its defining module. Application code can also import the public declaration directly from its origin.

Each declaration is private unless its item has `export`. Record fields are public by default. A private field prevents construction or rebuilding outside its defining module. A private type can instead cross an exported operation as an opaque inferred value.

## Syntax and reference semantics

The owner confirmed the three syntax policy groups (ri-48g, comment 6931939690) in [annex version 1](../grammar/v0-syntax-v1.md). Its [EBNF](../grammar/v0-syntax-v1.ebnf) and commitment/context rules form one policy review unit. Full annex/RFC acceptance and static-semantics completion remain separate.

### Files and origins

Each `.rill` file defines one module. Directories supply `::` path segments. A directory alone is not importable. There are no inline modules or automatic `index.rill` or `mod.rill` facades.

An analysis context selects an entry file, target, compiler options and compatible standard-library inputs. The application root is the entry file's directory. The standard library has a separate toolchain-selected origin. Application imports cannot replace that origin.

Unprefixed application imports start at the application root. Unprefixed imports within standard-library source start at the standard root. `std` selects the standard origin explicitly. `self::` starts beside the importing file. Each leading `super::` moves to its parent directory. Relative paths cannot escape their current origin.

Import paths are case-sensitive. This RFC adds no user manifest, package resolver or configurable search path. The process working directory does not select installed standard-library source.

The toolchain explicitly maps the standard root to its facade. Following [ri-x5n](../adr/0026-toolchain-repository-and-mise.md), this draft maps `std` to the selected `std.rill` facade. Other standard module paths use that selected source tree. This is one explicit mapping, not a filename fallback search. The current scaffold does not contain the facade or implement this lookup.

<a id="proposed-filesystem-identity-policy"></a>

### Filesystem identity policy

The project owner confirmed this policy (ri-48g, comment 6908371073) on 2026-11-16. It supplements ri-owe without accepting the complete RFC.

Establish the application root from the requested entry path's directory. Canonicalize that directory before loading. Resolve the entry file and require it to remain within that root. An entry-file link cannot silently select another root.

Canonicalize the selected standard root separately. For each imported file, resolve symbolic links and require the result to remain within its selected origin. Reject an escape with R0404. Symbolic links within an origin can identify the same module through several paths.

Require exact case and Unicode spelling for path components. Do not apply case folding or Unicode normalization during file matching. Check the directory entry spelling instead of trusting a case-insensitive lookup. If the host cannot represent the required distinction, report a resolution failure. Do not merge different requested names.

The canonical module key contains its origin tag and root-relative canonical file path. Use `app` and `std` as distinct origin tags. The path includes the `.rill` filename. Use `/` between components, independent of host path separators.

Two imports with the same key identify one module. Import aliases and re-exports do not change that key. Separate canonical paths remain separate modules even when their bytes or hard-link storage match. Do not use inode numbers or content hashes as module identity.

Resolve relative imports from the canonical file location. An alias path must not change which siblings `self` or `super` selects.

Never merge application and standard identities because their files match. A similarly named application file has no trusted standard identity. Selection of a standard origin does not authorize arbitrary native implementations. Protected identities and provider admission remain separate compiler responsibilities.

Keys belong to their analysis context. Do not compare them across independently selected program graphs to merge declarations. Diagnostic source URIs and snapshots still identify actual source versions. A module key is not a substitute for that information.

<a id="proposed-canonical-ordering"></a>

### Canonical ordering

This section supplies the exact ordering required by RFC 0001. The project owner confirmed this policy (ri-48g, comment 6908371073) on 2026-11-16. This supplements ri-owe. It does not accept the complete RFC.

Within one analysis context, order application modules before standard modules. Within each origin, compare the root-relative file paths by unsigned UTF-8 byte order. Compare full paths, including `/` and the `.rill` suffix. A shorter byte prefix sorts first.

Do not use locale order, filesystem enumeration, dependency discovery, absolute installation paths or import aliases. Within each module, order source declarations by their original UTF-8 starting byte offset. RFC 0001 uses this module order and then expectation source position.

For example, this policy orders these module keys as shown:

```text
app:api.rill
app:geometry/point.rill
app:main.rill
std:collection.rill
std:std.rill
```

This ordering does not execute module startup or initialize dependencies.

### Import and export forms

The following forms illustrate import syntax. They are excerpts, not one combined program. The shared syntax annex records the confirmed placement and separator policies.

```rill
import geometry::point;
import geometry::point as points;
import geometry::point::Point as Position;
import geometry::point::{Point as Position, origin};
export import geometry::point::{Point as Position, origin};
export import geometry::point as points;
```

A module import binds its final name unless `as` supplies another name. An item import binds the selected exported name. A grouped import selects named exports from its resolved module. It does not select child files.

Use item-site `export function`, `export type`, `export enum` and `export import`. Imports and declarations are otherwise private source bindings. There are no bottom export lists, export-from forms, wildcards, default imports or string-path imports. Runtime `let` bindings cannot be exported.

An exported import preserves the original declaration or module identity. It does not transfer defining-module permissions to the facade. A facade does not hide other public paths to the original declaration.

### Path resolution and namespaces

Resolve import paths from an origin or explicit `std`, `self` or `super` prefix. Do not consult aliases in the importing module's local scope. An import alias can qualify later source expressions and types. It cannot silently redefine another import's root.

During import resolution, a file path can identify a source module. An exported module binding can also provide a public route through a facade. Traverse such bindings by canonical identity. This does not permit local aliases to change root lookup.

For an ungrouped import, collect applicable file-module and exported-item candidates for the complete path. Resolve any exported module routes along that path. Directories contribute path segments without becoming module bindings.

Reduce candidates by canonical identity within each namespace. The namespaces are type/module and value/function. Repeated routes to one identity count once. Distinct identities in the same namespace make the path ambiguous. Report R0402 instead of choosing filesystem or import-order precedence.

A grouped import first requires one resolved module. Each selected name then uses that module's exported bindings. If `api.rill` exports type `Widget`, `import api::{Widget};` selects that export. It does not select `api/Widget.rill`.

If the name has exported bindings in both namespaces, import both. Apply an `as` alias to both public bindings. A private same-spelled counterpart does not become public. A module candidate and value candidate can coexist when each namespace has one identity. Distinct type and module candidates cannot coexist in the type/module namespace.

Check same-scope collisions before installing the imported bindings. Distinct bindings in the same namespace cause R0403. Do not install only the non-conflicting half of an import. Inner lexical shadowing remains permitted. Valid family declarations and implementations follow their separate collection rules, not ordinary duplicate-binding rejection.

Ordinary qualification starts from an imported module binding in the type/module namespace. It selects that module's exported bindings in the required namespace. It does not load a child file merely because that file exists. Viewing private source in an editor does not grant source access.

### Source snapshots and lexical input

The owner confirmed this file-level lexical input policy (ri-48g, comment 6929923825) with RFCs 0003, 0006 and 0010. The glossary distinguishes [source snapshots/spans](../../GLOSSARY.md#language) from analysis contexts and temporal Span values. This does not complete graph snapshot construction, filesystem transactions or editor freshness. The full RFC remains Draft.

Capture one immutable raw byte sequence before lexing a source file. Associate it with its canonical source identity and an opaque sourceSnapshotId. Changes to a file/editor buffer cannot mutate that saved sequence.

A snapshot ID must never designate different bytes or another canonical source identity. Reusing an identical immutable snapshot is permitted. Equality of a digest alone cannot establish equality if the implementation cannot rule out a collision.

This byte snapshot does not promise an atomic filesystem read or multi-file transaction. Host input failures remain driver I/O failures. The [proposed M3 procedure](#proposed-m3-resolution-procedure) adds graph capture and validation without promising simultaneous filesystem coherence.

Assign spans against unchanged source bytes. An admitted initial BOM, CRLF pairs and comments remain present. Any normalized literal content has separate provenance under RFC 0006.

Require valid UTF-8, including inside comments and literals. Reject overlong encodings, surrogates, values above U+10FFFF, isolated continuation bytes and incomplete sequences. Never use lossy decoding to make malformed source acceptable.

For malformed source, at least report the first invalid sequence in byte order. Use RFC 0006's [invalid UTF-8 span rule](0006-numbers-strings-and-primitive-operations.md#invalid-utf-8-span-and-replacement-policy) for that byte extent. This reuse defines a range, not permission to insert replacement text into source.

An invalid sequence rejects the file before an accepted ordinary token stream exists. Optional recovery can supply partial facts, but must retain the encoding failure. It cannot claim a valid decoded literal or successfully checked file.

An input/resource size that the host cannot represent is not a lexical source error. Report unavailable/incomplete processing without wrapped offsets or invented successful validation. The language adds no fixed file/token/hash/nesting quota through a host integer choice.

Original source spans use natural-number byte offsets with exclusive ends. Require `0 <= start <= end <= source length`. EOF is the empty span at source length. Tokens do not use decoded string offsets or runtime SourceLocation values.

File-level sourceSnapshotId identifies these bytes, not the entire dependency graph. The analysis context separately identifies its source/dependency snapshot and options. A matching file snapshot alone cannot validate an edit or stale analysis result.

Case X1 supplies a file beginning with BOM bytes and an ASCII declaration. The first keyword starts after the three BOM bytes. Case X2 supplies invalid UTF-8 inside a comment; it rejects rather than disappearing as trivia.

Case X3 changes a buffer after snapshot capture. Earlier spans still refer to the saved bytes, not the changed buffer. Case X4 supplies input that exceeds host representation; processing stays incomplete, not an accepted empty file.

All four cases are NotChecked by a Rill loader/lexer. Source identity remains governed by the confirmed canonical-path policy. Physical report-sink alias checks still do not merge canonical modules.

### Loading and cycles

Load the entry, fixed prelude dependencies and transitive imports/re-exports. Include imports used only by expectations. Canonicalize each module key before adding it to the graph. Load each key once. Do not parse unrelated files found on disk.

Existence checks for an explicit import can identify competing file candidates. They do not add unrelated directory contents to the program. The complete successful graph consists of resolved import and required prelude dependencies.

Parse loaded modules and collect declaration identities before checking bodies. Resolve import and re-export bindings against those identities. Compute canonical export identities without depending on source traversal order.

Declaration cycles are not automatically errors. Named declarations can anchor cyclic references between modules. Repeated aliases of an anchored identity resolve to that identity. An alias-only cycle with no declaration identity fails with R0405.

Type checking must separately reject impossible recursive value layouts with R0406. A module cycle alone does not establish an impossible layout. [Draft RFC 0005](0005-types-records-enums-and-inference.md#proposed-finite-inline-layout-checking) proposes the missing layout analysis. It still needs formal checking.

Missing imports or source bindings have meaning R0400. Inaccessible bindings have meaning R0401. Errors in an unused loaded body still reject the program. Dead-code elimination cannot remove a module to change the collected implementation set or dispatch.

### Proposed M3 resolution procedure

The owner approved the M3 checkpoint (ri-1fk, comment 6939478619) at `a41e7cc955e67329d6dc3a8a69de667e0af204ac`. Approval covers O1 through O7, the test seams and implementation slices. This section supplies that scoped M3 contract. Earlier approvals and full RFC status remain unchanged. The [implementation checkpoint](../implementation-plan.md#m3-design-checkpoint) records the review boundary. The [acceptance record](../../crates/compiler/tests/m3-acceptance.md) distinguishes implemented resolution assertions from later checks.

#### Identities and declaration records

Keep origin, module, declaration, binding and source-snapshot identities separate. A context owns every identity in its graph. A module identity uses the confirmed origin-relative canonical key. A declaration identity adds its original declaration byte offset and declaration kind. Nested declarations also retain their containing declaration or lexical scope. Enum variants retain their parent declaration and variant offset.

A source binding records its scope, namespace, exact spelling, visibility, source span and canonical target. Import bindings also record the import site and route provenance. Several bindings can target one declaration. Re-export visibility describes the new route, not new ownership or permission over the target. A public route cannot promote a private target that its source module cannot export.

Collect standalone functions, body-less families, records, enums and their member records before resolving imports. Retain startup, expectation and nested declaration syntax for later checking. Do not install nested declarations in module export tables. An implementation record has its own source identity, defining module, target path, binders, signature, requirements and body span. Resolve its family target through the value/function namespace after imports stabilize. Missing, private, ambiguous or non-family targets cannot become successful attachments.

Collect each implementation record once per canonical module and declaration position. Retain written type syntax and aliases for later canonical type checking. These records do not establish duplicate-pattern rejection, orphan permission, conversion coherence, protected-operation safety or valid bodies. Those obligations remain with the companion RFCs and later compiler stages.

#### Exact filesystem probes

Establish canonical application and selected standard roots before discovery. Validate exact directory-entry spelling for requested components, including the entry file. Follow internal symbolic links. Repeat confinement and exact-spelling checks for their targets. Resolve self and super from the importing module's canonical parent. Reject a parent step beyond its origin with R0404 before probing outside that origin.

A probe checks one explicitly derived path, not a recursive directory search. Directory enumeration may verify spelling but cannot enroll other files. A regular file with the exact .rill suffix can supply a module. A directory supplies only traversal. A missing component or a non-directory intermediate component supplies no candidate through that route. Do not try index.rill, mod.rill, extension variants or normalization fallbacks.

Record positive and negative probe observations. Distinguish absence from permission errors, unreadable directories, broken links, link loops and unrepresentable paths. Never turn those host failures into proof that an alternative candidate is unique. Refuse non-regular source inputs before reading, including devices and pipes. This avoids promising bounded reads from arbitrary special files.

Host case folding or Unicode normalization cannot establish an exact match. If directory entries cannot establish the requested spelling, return an explicit host limitation. Separate hard-link paths retain separate canonical keys. Files from overlapping application and standard roots still retain separate origin tags.

#### Candidate discovery and exported routes

Normalize each import into its selected origin, bounded starting directory and remaining segments. Reserve std, self and leading super for these path roles. Do not consult importing-module aliases. An unprefixed standard-source import has no application fallback. Bare std uses only the explicitly selected std.rill facade mapping.

For an ungrouped path with n remaining segments, probe every nonempty filesystem prefix ending in .rill. For example, a::b::C probes a.rill, a/b.rill and a/b/C.rill where traversal permits. A full-length file supplies a terminal type/module candidate. A shorter prefix supplies a module whose exports can consume the remaining segments.

For each shorter prefix, traverse only exported bindings. Each nonterminal segment must resolve to a module identity in the type/module namespace. A nominal type cannot supply an import-path directory. At the terminal segment, retain exported candidates in both namespaces. Do not search filesystem children after entering an exported module route. Several prefix splits can supply competing routes; none has precedence.

Inspect the binding table for diagnostic-only privacy witnesses at each requested segment. A witness records an existing private binding, its defining module and the blocked route position. Record both private terminal names and private intermediate module bindings. Stop that route at the private barrier. Never traverse the private target or add its identity to public candidate sets.

A witness cannot create ambiguity, install a binding or anchor a public alias cycle. A public result suppresses private counterparts of that requested name in either namespace. If no public result exists, preserve private barriers separately from absent names and unresolved alias dependencies. This distinction supplies R0401 without granting private access.

A grouped path uses the same discovery procedure but requires exactly one module identity in the type/module namespace. A value candidate does not make another module. A nominal type is not an acceptable group base. Resolve each selected group name only through that module's exports. Never probe a child file for a group member.

Cache route states by import obligation, canonical module identity, namespace and remaining segment position. Keep predecessor edges as provenance instead of enumerating complete route histories. Each route step consumes one segment. Re-export aliases use the closure procedure below rather than recursive path expansion. Repeated aliases cannot produce infinitely many route histories.

Every parsed prefix module is a required resolution dependency, even if it supplies no final candidate. Its imports, declarations and expectations therefore join the graph. This proposed rule makes candidate inspection explicit and reproducible. It can enlarge the declaration set compared with retaining only the winning terminal module. The M3 checkpoint approval covers this consequence. An existence-only probe that does not parse a source contributes an observation, not declarations.

#### Closure, canonical reduction and installation

Maintain a queue of canonical modules, import obligations and route states. Capture a newly required module once. Parse its saved snapshot once. Collect its declaration seeds before adding its imports and re-exports to the queue. A failed parse retains its failure and cannot supply accepted exports. Revisit dependent obligations when a new module or candidate fact appears.

Represent each export or import obligation as separate type/module and value/function candidate sets. Seed declarations with canonical identities. Propagate identities along permitted export edges. Union repeated routes to the same target within each namespace. Preserve all distinct targets and their provenance. Never choose one candidate because the other has not yet been discovered.

Continue until the queue is empty and a full pass adds no modules, route states or candidate identities. During closure, facts are provisional. Collision and uniqueness checks cannot publish bindings early. Discovery and candidate propagation must not depend on whether an earlier source import installed successfully.

After closure, a terminal set with several identities in one namespace fails with R0402. One identity in each namespace can coexist. Private counterparts do not enter exported candidate sets. If both public sets are empty, distinguish private-only, missing and cyclic outcomes as described below.

Stage all bindings from one import declaration, including every grouped member and both namespaces. Compare staged targets against local declarations, other imports and each other. Repeated bindings to the same canonical target in one namespace are idempotent. Distinct targets under one same-scope spelling fail with R0403. Stage no implementation as an ordinary value binding. Family attachments follow their separate rules.

Use one publication barrier for the entire attempted graph. Validate every import's selections and collisions before publishing any imported or re-exported binding. A failed grouped member rejects the whole declaration. Any failed import prevents publication of all staged imports in that attempted graph. Publish only after family-target validation and input replay also succeed. A rejected result exposes provisional candidates and failures, not installed imports or a successful export lookup interface.

This barrier prevents downstream publication through a failed exported import. Provisional targets can affect other provisional candidates and cycle anchors, but they cannot establish a resolved result when any transaction fails. No invalidation algorithm or traversal-order retry is needed. Dependent diagnostics may be incomplete after another import fails. The result must disclose that limit rather than claim every secondary error. The M3 checkpoint approval covers these transaction and repeated-binding rules.

The M3 implementation resolves module bindings, exported qualification and implementation-family targets. It inventories nested lexical scopes without claiming expression or type checking. Full local binding, capture, pattern and control-target resolution remains later work. Module startup binding syntax must still participate in same-scope collision inventory. Typed pattern validity remains unchecked. The M3 checkpoint approval covers this scope boundary.

#### Anchored and unanchored cycles

Use an iterative least fixed point over canonical candidate identities. Declaration seeds and direct file-module identities anchor propagation. A cyclic chain reaching one seed retains that seed once. Different seeds remain different candidates; a cycle does not erase ambiguity. A module import cycle alone does not fail.

After closure, compute strongly connected components of unresolved alias obligations without host recursion. A component with no path to a canonical seed and an internal dependency cycle is unanchored. Mark those obligations with a cyclic cause. A private declaration cannot anchor an external public route.

Build the acyclic graph of those components. Propagate cyclic, missing and private causes from dependencies to their dependents in reverse dependency order. An acyclic alias feeding an unanchored cycle therefore retains R0405, not a fabricated missing-name R0400. An actually absent export retains R0400. A private barrier retains R0401. A branch can retain several independent causes.

Report missing or private causes only when the requested name has no public result in either namespace. Retain failures of explicitly exported unresolved obligations, including cyclic causes. Do not invent an error for an absent or private opposite namespace. A module import cycle alone is not an alias failure. Successful anchoring does not establish finite type layouts. R0406 remains a later type-layout assertion.

#### Captured graphs and source changes

Capture one immutable M1 snapshot for each canonical module before lexing or M2 module parsing. Retain actual canonical source paths separately from origin-relative keys. Diagnostic spans always refer to these captured snapshots. Reuse the M1 and M2 interfaces without rescanning source syntax in the resolver.

A graph snapshot records the selected entry, target/options, roots, explicit prelude inputs, source snapshots and resolution observations. Its opaque identity belongs to this capture, not a content digest. Declaration identities are meaningful only with this graph and its source versions. Fresh captures cannot merge declarations through matching paths or snapshot digests.

Before returning a complete resolution result, replay every relevant path observation. Compare every captured source's bytes. Include negative probes so a newly created competing file invalidates uniqueness. Compare bytes directly, not only timestamps, sizes or hashes. Recheck root mappings, canonical targets, spelling and regular-file status. A changed or vanished observation returns SourceChanged or an input failure, not a source diagnostic or automatic retry.

The caller can explicitly validate a retained graph again. Later validation never mutates old snapshots or their spans. A later load creates a new graph identity and new observations. No source edit, automatic restart or persistent source cache belongs to M3.

This procedure detects observed changes. It does not establish a simultaneous multi-file filesystem transaction or prevent changes after validation. A hostile writer can change and restore bytes between observations. Metadata equality does not prove freshness. A complete result means closure over retained inputs with successful observation replay, not a checked program or a live-filesystem guarantee. If simultaneous filesystem coherence is required, the owner must choose a stronger capture mechanism before implementation.

#### Outcomes, limits and termination

Keep graph closure, source validity and later checking separate. A source-rejected result can retain complete discovery facts or partial facts. It cannot expose an accepted resolved graph. Input failures and resource exhaustion retain only explicitly incomplete facts.

| Outcome | Proposed meaning |
| --- | --- |
| Resolved | Every required module parses, every import installs, family targets resolve, and input replay succeeds. No body/type/ownership validation is claimed. |
| SourceRejected | Captured source fails lexical, syntax or M3 binding rules. Findings retain original spans and candidate provenance. |
| InvalidRequest | Required configuration or prelude selectors are absent or structurally incompatible. No trusted runtime permission follows from accepted selectors. |
| InputFailure | A required host operation fails, or exact identity/spelling cannot be established. Missing selected roots or entry files are input failures. |
| SourceChanged | Replay observes changed bytes, roots, routes, file kinds or previously absent candidates. The attempted graph remains incomplete. |
| ResourceLimit | A configured work, storage, probe, byte, module or parser budget prevents completion. This is not a language rejection. |

A stable missing import has meaning R0400. A private-only requested export has meaning R0401. An inaccessible counterpart does not invalidate a uniquely public counterpart. R0402, R0403, R0404 and R0405 retain their meanings above. A boundary escape is R0404, not a missing file.

Filesystem link loops are host failures, not alias-cycle R0405. Non-family implementation targets are source errors with no new stable diagnostic code assigned here.

Do not report absence, privacy, ambiguity or a unique binding until all applicable routes finish. An unreadable alternative prevents that conclusion. Keep observed source findings when processing becomes incomplete, but do not assert complete diagnostics. Order findings by canonical module, source position, finding kind, namespace and canonical target. This order does not depend on discovery order or localized host error text.

Charge new modules, source bytes, parser operations, filesystem probes, inspected directory entries, route states, candidate insertions and dependency visits. Charge privacy witnesses and cause propagation too. Check limits before allocating or enqueueing the charged item.

Bound individual reads and retained bytes, including validation reads. Include failed probes and tentative work in accounting. Checked counters must not wrap. Parser ResourceLimit remains incomplete, not a syntax error. Existing M1/M2 limits remain in force.

A fixed captured input has finitely many declaration seeds and segment positions. Memoized states and monotone finite candidate sets reach a fixed point. Iterative graph algorithms avoid recursive resolver stack growth. A changing or excessively large host graph instead reaches an explicit budget or replay failure. Budgets bound compiler work, not host syscall latency, allocation success or wall-clock completion. Abrupt host exhaustion need not produce a final outcome.

### Prelude and startup checking

The fixed prelude supplies primitive types, intrinsic predicates and canonical `copy` and `dispose` declarations. Other library names need explicit imports. In particular, primitive `string` is not an implicit import of its helper module.

Compiler-required operations use canonical identities despite local shadowing. Compiler metadata access does not create a source binding. It does not grant access to private representations or make private type names importable.

Named functions cannot capture enclosing runtime `let` bindings. Expectations cannot capture ordinary startup bindings either. Constant folding does not grant capture permission. These failures have meaning R0502.

Check loaded startup statements and expectation bodies in ordinary and test modes. Ordinary execution runs only the entry file's startup. Imported modules have no implicit runtime initializer. RFC 0001 owns test execution and production exclusion of test-only code.

### Privacy and opaque values

A declaration belongs to its canonical defining module. Importing or re-exporting it does not change that owner. Parent directories and neighboring modules receive no extra private access. Ordinary source use requires an accessible binding or permitted qualification.

Record fields are public by default. Only the defining module can read, change or bind a `private` field. If any field is private, outside code cannot construct or rebuild the whole record. An existing value does not grant rebuilding permission. Exported factories and updaters can provide controlled operations.

A private nominal type can occur in an exported operation's signature. Clients can infer its identity, hold it, pass it and use visible operations. Compiler metadata supports checking and disposal. Clients cannot name, construct, destructure or inspect the private representation.

A public type missing from local imports is not opaque. Its canonical public declaration controls representation permission. Lack of a local name does not hide its public fields. Source construction still needs the required type binding and ordinary ownership evidence.

An exported enum exposes all variants and payload fields. There are no individually private enum variants or payload fields in v0. Publicly exposed payload types must meet public-interface visibility rules. A public origin path is sufficient; a facade need not re-export every payload type.

A private enum remains opaque when returned through exported operations. Clients cannot inspect variants or narrow its representation. A public record can instead hide an enum behind a private field. General union syntax does not provide another visibility mechanism.

### Family collection and metadata boundaries

An imported family declaration retains its canonical identity. An alias does not create a family. A same-spelled unrelated function does not join it. An implementation attaches to the declaration, not to a facade's spelling.

Every implementation in a loaded module contributes to its family. Implementations inherit declaration-wide visibility. They are not individually exported or hidden. An extension does not re-export its module's private import binding.

Collect canonical bodies once. Reject distinct bodies with identical ordinary family and parameter patterns, even without a call. Normalize aliases and type-parameter renaming. Returns and requirements do not distinguish such duplicates. The diagnostic meaning is R0304.

Generic and concrete patterns can remain distinct. [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md#ordinary-call-selection) proposes overload ranking and call ambiguity. This RFC does not rank by return type, requirements, import order or runtime value.

An ordinary implementation belongs in the family-defining module or a module that defines a type occurring in its parameters. Re-exports grant no defining-module ownership. [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md#collection-and-placement-permission) now proposes the exact ordinary patterns and placement rules. Canonical conversions have the separate endpoint/key rules confirmed in ri-1wl. Do not apply those exceptions to every family.

Ordinary families named in an exported generic requirement must have publicly nameable exported declarations. Their origin public paths are sufficient. A facade need not re-export them. Importing an operation does not bind its requirement names in the caller. Private body dependencies remain permitted.

Required guard release, owner-count maintenance and native-borrow retention remain protected independently of ordinary placement permission. Collection must reject non-trusted implementations that replace or tie a required action. The diagnostic meaning is R0500. Do not impose a blanket outermost-type rule or seal all standard operations.

Private fields alone do not prevent interception. Later ri-8n7 cleanup rules also preserve remaining initialized owned fields after custom disposal. Their full checking and runtime rules need companion RFCs. This document does not claim to prove them.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [RFC 0001](0001-top-level-expectations.md) | Expectation declarations, Boolean outcomes, test execution and trusted observations. | Accepted at the snapshot identified in metadata. This RFC does not amend it. |
| Expressions and declarations | Identifier grammar, lexical scopes, expression checking and non-capturing declarations. | [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) proposes bindings and control flow. Its lexical identifier/token policy is confirmed. The shared syntax annex records confirmed declaration/parser policies. Full resolver and checking procedures remain incomplete. |
| [Draft RFC 0005](0005-types-records-enums-and-inference.md) | Nominal declaration identity, public-interface exposure and finite layout checking. | Proposed, not accepted. Concrete type-syntax policies are confirmed. Formal layout checking remains incomplete. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) | Canonical patterns, family evidence, duplicates, placement and call ranking. | Proposed. Its full text is not accepted. This RFC owns loading and visibility, not ranking. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md), with runtime integration still missing. | Protected operations, remaining-field cleanup and opaque-value metadata. | Proposed, not accepted. Complete registry/runtime checking remains incomplete. Private fields do not validate required actions. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md), with full LSP still missing. | Shared contexts, snapshots, path failures, safe edits and client behavior. | Reporting is proposed, not accepted. Complete source/snapshot and driver rules remain incomplete. Actual LSP delivery remains ri-bv2. |

## Valid/invalid examples and diagnostics

The guide's three files form case M1. Each additional case below is independent. No Rill compiler has checked or executed these cases.

Case M2 is an import excerpt. Assume `api.rill` exports type `Widget` and `api/Widget.rill` defines a distinct module.

```rill
import api::Widget;     // Rejected: competing type/module identities.
import api::{Widget};   // Independent valid alternative: select the export.
```

Case M3 is an excerpt in an importing module. Assume `api.rill` has no public binding that exposes private `Hidden`.

```rill
import api as A;
import A::Hidden; // A is not a canonical root module in this example.
```

The second import does not consult the local alias. If a real `A.rill` exists, it instead resolves from that source path. Its exports and privacy rules still apply.

Case M4 is an excerpt outside `resources.rill`. That module exports `Ticket` and `issue`, but declares `Ticket.code` private.

```rill
let ticket = issue("first");
let forged = Ticket { code: 7, label: "forged" }; // Rejected.
let rebuilt = Ticket { ...ticket, label: "changed" }; // Rejected.
let code = ticket.code; // Rejected.
```

These lines are separate rejection modifications. A factory-produced value does not grant access to private fields or whole-record construction.

Case M5 is a complete intended-rejection module. The exported operation names a private required family.

```rill
function ready<T>(value: &T): bool;
implement ready(value: &i32): bool { return true; }

export function check<T>(value: &T): bool where ready(T) {
  return ready(value);
}
```

Exporting `ready` would repair this visibility error. It would not add a local `ready` binding to callers that import only `check`.

| Case / artifact | Context | Expected outcome | Owning rule | Evidence |
| --- | --- | --- | --- | --- |
| M1, guide graph | A facade re-exports a public nominal type and helper. | Load three application modules once. The alias preserves type identity. The local expectation passes. | [Imports](#import-and-export-forms), [loading](#loading-and-cycles), [privacy](#privacy-and-opaque-values). | HandChecked source reasoning. Resolution and execution are NotChecked. |
| M2 | A type export competes with a child file module. | Reject the ungrouped form with R0402. The grouped alternative selects the type export. | [Resolution](#path-resolution-and-namespaces). | HandChecked candidate identities. No compiler diagnostic exists. |
| M3 | A local module alias appears as a later import root. | Do not use the alias. Report R0400 if the canonical source path is missing. | [Resolution](#path-resolution-and-namespaces). | HandChecked. Source resolution is NotChecked. |
| M4 | A public record has a private field. | Reject each external construction, rebuild or field access with R0401. | [Privacy](#privacy-and-opaque-values). | HandChecked against ri-owe. No ownership checker ran. |
| M5 | A public generic interface requires a private family. | Reject the inaccessible public requirement with R0401. | [Family metadata](#family-collection-and-metadata-boundaries). | HandChecked. Generic checking is NotChecked. |
| M6 | Two import routes reach one canonical body. | Collect it once. Do not report a duplicate body. | [Loading](#loading-and-cycles), [collection](#family-collection-and-metadata-boundaries). | HandChecked identity distinction. Actual collection is NotChecked. |
| M7 | Two distinct bodies have alias-equivalent parameter patterns. | Reject with R0304 even without calls. Return differences do not repair the duplicate. | [Collection](#family-collection-and-metadata-boundaries). | Supporting source rule only. Full pattern normalization remains companion work. |
| M8 | An alias-only cycle has no declaration anchor. | Reject with R0405. Do not reject every declaration cycle. | [Cycles](#loading-and-cycles). | HandChecked distinction. Resolver checking is NotChecked. |
| M9 | An imported file resolves outside its origin through a symbolic link. | Under the confirmed filesystem policy, reject with R0404. | [Identity policy](#filesystem-identity-policy). | Confirmed policy. Implementation is NotChecked. |
| M10 | App and std roots move without changing their relative files. | Preserve the confirmed within-context ordering. Do not order by absolute installation paths. | [Ordering](#canonical-ordering). | HandChecked comparator definition. Host resolution is NotChecked. |
| M11 | An expectation-only import supplies an implementation. | Keep it in ordinary and test collection. Later elimination cannot change dispatch. | [Loading](#loading-and-cycles), [startup](#prelude-and-startup-checking). | HandChecked source rule. Compiler behavior is NotChecked. |
| M12 | An ungrouped path supplies one module and one exported function under the same spelling. | Bind both namespaces. Calls use the function. Qualification uses the module. | [Resolution](#path-resolution-and-namespaces). | HandChecked namespace separation. Actual lookup is NotChecked. |
| M13 | An alias would collide in one imported namespace but not the other. | Reject the import with R0403. Do not install only its non-conflicting half. | [Resolution](#path-resolution-and-namespaces). | HandChecked collision rule. Actual lookup is NotChecked. |
| M14 | An unprefixed import within std has no standard match but has an application match. | Report R0400. Do not fall back into application source. | [Origins](#files-and-origins). | HandChecked origin isolation. Host lookup is NotChecked. |
| M15 | A name has a public type binding and a private function binding. | Import only the public type. Do not expose the private function. | [Resolution](#path-resolution-and-namespaces). | HandChecked public binding rule. Actual lookup is NotChecked. |
| Existing seven-file `prototypes/07-modules/` group | Alias, privacy, family and startup cases. | Discover fifteen application expectations once. Standard-module expectations can add to that count. | [Discovery](#loading-and-cycles), [metadata](#family-collection-and-metadata-boundaries), RFC 0001. | HandChecked source inventory. No Rill execution occurred. |

The drafting agent compared these cases with the named rules on 2026-11-16. Diagnostic codes identify meanings, not exact wording or a complete cascade. These checks do not prove resolution, ownership safety or readiness.

## Feature interactions

### Relationship with RFC 0001

This RFC supplies module identity, graph membership and canonical ordering. RFC 0001 owns expectation behavior. Repeated routes to one module therefore contribute one set of expectations. Imports used only by expectations still affect static family collection.

The accepted RFC 0001 snapshot names missing module rules. This draft proposes that companion. It does not become normative through that reference alone. RFC 0002 needs its own human acceptance.

A reading order can place this document before RFC 0001. Document identity does not change. Do not renumber accepted or published RFCs to sort dependencies.

### Standard-library and compiler identities

`copy`, `dispose` and intrinsic predicates retain canonical identities through shadowing and aliases. Metadata can expose facts needed for checking without exposing source names. Neither a matching filename nor a copied declaration manufactures trusted runtime actions.

The standard facade follows the compatible selected toolchain. This does not create an application package manifest or arbitrary native-provider registration. Native-provider admission remains separate from ordinary source exports.

### Editor contexts

An editor uses the selected entry, target and options to identify an analysis context. A workspace folder is not the application root by itself. Search results cannot add implementations to that context's loaded graph. Contexts remain separate even when they share files.

Organize-imports must preserve loaded modules, public bindings, implementations and expectations. Removing a textually unused import can change that graph. Navigation and rename must preserve canonical aliases, namespaces and private access. Actual client behavior remains unvalidated under ri-bv2.

### Source reconciliation

Repository inputs use [`f33dcf4cf3f815824eeb897135775d7884f65c1a`](https://github.com/Reilley64/rill/tree/f33dcf4cf3f815824eeb897135775d7884f65c1a). Source decisions do not accept this RFC.

| Source | Disposition |
| --- | --- |
| Confirmed RFC 0002 policies (ri-48g, comment 6908371073) | Specify filesystem identity and canonical ordering. The approval covers these policies, not the complete RFC. |
| ri-owe confirmation (comment 6866705217), ADR 0010 | Include roots, explicit imports/exports, namespaces, privacy, opaque values and graph collection. The later policy decision confirms the filesystem and ordering details. |
| ri-kty confirmation (comment 6870013901), ADR 0012 | Replace historical public-union rules with enum visibility. Do not restore private enum variants or general unions. |
| ri-8n7 confirmation (comment 6872129850), catalogue sections 1 and 2 | Preserve fixed prelude identities, protected actions and remaining-field cleanup. Their complete checking rules remain companion work. |
| ri-1wl confirmation (comment 6886119679), ADR 0017 | Use `implement` for family bodies. Keep conversion-specific keys and endpoint permission separate from ordinary rules. |
| ri-3t8 confirmation (comment 6885970775), diagnostic registry and LSP contracts | Use assigned path/privacy code meanings. Preserve context separation and graph-aware edits. No resolver or client behavior is claimed. |
| Accepted RFC 0001 snapshot (ri-48g, comment 6907867211) | Reuse expectation behavior. Propose its missing module rules without changing its accepted semantics or ID. |
| ADR 0026 and standard-library ownership note | Use a toolchain-selected source origin and explicit future `std.rill` facade. Do not invent package lookup or claim the scaffold supplies it. |
| Module prototype and [consolidation record](../prototypes/consolidation.md) | Use current `07-modules/` paths. Preserve all source and fixture bytes. Historical `10-modules/` references identify earlier paths. |

## Compatibility and migration

There is no released module implementation to migrate. Current examples already use file modules, explicit exports and canonical aliases. Keep `implement` rather than older family-body spellings. Keep enums rather than general unions.

Reject layouts that depend on automatic index/mod files, implicit module initializers or configurable package searches. A directory can still contain child modules. Add an explicit facade when a source module is needed.

The confirmed policies make previously unspecified details explicit. The resolver must reject symbolic-link escapes. Distinct canonical hard-link paths remain distinct modules. Exact spelling can expose differences on case-insensitive or Unicode-normalizing hosts. The owner approved both policies. Host implementation and validation remain unverified.

This draft does not change RFC 0001's accepted semantics.

## Security/resource implications

A bounded import origin is not a filesystem sandbox. Compiler access, source permissions and runtime filesystem authority are separate. A hostile process can still change files during loading. A resolver needs coherent source snapshots and must report unresolvable changes instead of combining incompatible identities.

Import resolution must not execute startup, tests, user formatters or native providers. A finite reachable graph can still be large or cyclic. Bound resolver work and memory. Resource failure or incomplete analysis cannot report successful checking.

Private fields restrict source operations. They do not prove required cleanup or defend against protected-operation interception alone. Opaque compiler metadata must retain ownership and property facts without granting representation access.

The selected standard origin must not fall back into application files. Protected runtime identity requires compiler authority beyond a pathname. This RFC does not introduce general FFI, package installation or remote import execution.

## Rationale, alternatives and prior art

File modules make the defining location explicit. Inline modules could organize declarations without extra files, but ri-owe rejected them for v0. This draft retains that boundary.

Explicit item-site exports show public bindings at their declaration or import. Bottom export lists and wildcard exports can reduce repetition. They also introduce alternate visibility paths that the confirmed design does not adopt.

Separate namespaces let a type and its constructor helper share a spelling. Syntax determines which namespace is needed. Importing both public halves preserves that relationship. Ambiguous same-namespace identities still need aliases or explicit grouping.

The confirmed key uses origin and relative path rather than content or inode identity. This preserves nominal ownership across copied files and machine relocation. Resolving symbolic links avoids duplicate modules through filesystem aliases. Rejecting origin escapes preserves bounded lookup, but limits linked external source trees.

The confirmed byte ordering avoids locale and discovery differences. The project owner selected application-first ordering in the policy decision (ri-48g, comment 6908371073). This is a design choice, not a conclusion from a native experiment.

## Implementation implications

The workspace now implements scoped M3 resolution and declaration collection in crates/compiler. The [acceptance record](../../crates/compiler/tests/m3-acceptance.md) identifies real loader tests and host limits. No typed graph, body checker or runtime follows from those results. [Development status](../development.md) records the separate infrastructure and product gates.

A resolver needs separate origin, module, declaration, binding and source-snapshot identities. Alias lookup must preserve the declaration owner. Export lookup must not turn metadata access into source permission.

Collect names before checking bodies. Track visited canonical modules and alias dependencies. Distinguish unresolved cyclic aliases from valid declaration cycles. Record enough provenance to explain both candidates in an ambiguous import.

Family collection must use the loaded graph before dead-code elimination. It needs canonical body identities and later pattern normalization. Protected-operation and conversion-specific checks need their companion rules. A generic symbol table does not establish those guarantees.

Source loading must enforce exact spelling and stable snapshots on each supported host. It must not derive context graphs from editor indexes or process working directories. Failure to establish a required identity must remain explicit.

## Evidence and checking plan

The 2026-11-17 file-level snapshot proposal adds cases X1 through X4. All remain NotChecked by a Rill loader/lexer. Document checks do not establish atomic filesystem capture or complete graph freshness.

### Actual evidence

The table below preserves pre-M3 evidence at the stated drafting revisions. The [M3 acceptance record](../../crates/compiler/tests/m3-acceptance.md) reports newer scoped resolution observations. Its metadata-only assertions do not establish later typed or runtime outcomes.

| Artifact / revision | Evidence label | Rule exercised | Checker / environment | Expected / observed result | Limits |
| --- | --- | --- | --- | --- | --- |
| M1 through M15 in this draft | HandChecked | Alias identity, namespaces, visibility, cycles, graph membership, filesystem identity and ordering. | Drafting agent on 2026-11-16. Manual comparison against the listed source decisions. | Expected outcomes follow the stated rules. Policy approval is separate from full RFC acceptance. | No parser, resolver, generic checker or Rill execution. |
| Module sources at `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Seven reachable files and fifteen application expectations. | Source inspection and declaration counts. | Existing case inventory agrees with the application source count. | This does not discover loaded standard tests or execute startup/tests. |
| Host identity, ordering and cycle implementation | NotChecked | Exact spelling, aliases, roots and graph resolution. | No implementation is available. | No host result is claimed. | Requires real resolver tests on supported hosts. |
| Protected metadata and ownership interactions | NotChecked | Preserved cleanup, properties and generic visibility. | No checker or runtime implementation is available. | No safety or implementation result is claimed. | Requires companion rules and executable validation. |

### Planned checks. Not executed evidence

- Test application, standard, self and repeated-super roots. Reject lexical and symbolic-link escapes. Confirm no fallback into application source.
- Test aliases, re-exports and both namespaces. Include public/private same-spelled counterparts, collisions and grouped disambiguation.
- Test direct file and exported-module routes to one identity. Test distinct competing identities. Ensure expressions do not autoload filesystem children.
- Test exact case and Unicode spelling on supported host filesystems. Include internal links, escaping links, hard links and relocated roots.
- Test anchored cycles, unanchored aliases and impossible value layouts independently. Preserve distinct diagnostic meanings.
- Test loaded implementation sets with test-only imports and later dead-code elimination. Check canonical duplicates even without calls.
- Test private fields, opaque return values, public unimported types and enum payload exposure. Include generic and aliased routes.
- Test shadowed canonical operations and protected nested-payload specializations. Confirm ordinary unprotected defaults remain replaceable.
- Test entry startup and test discovery through RFC 0001. Count repeated module paths once. Include loaded standard expectations.
- Compare canonical ordering across hosts and root relocation. Exclude locale, import order and absolute path effects.
- Test separate editor contexts and graph-preserving import edits. Do not treat source navigation as private access or client acceptance.

## Open/deferred questions

### Remaining authoring work

The project owner approved both policy groups (ri-48g, comment 6908371073). Filesystem identity and canonical ordering are no longer open policy questions. The approval does not accept the complete RFC.

The [proposed M3 procedure](#proposed-m3-resolution-procedure) supplies candidate, cycle, graph-capture and host-failure rules for owner review. The owner approved its graph-membership, transaction and freshness choices for M3 at the identified checkpoint. The linked acceptance record reports actual resolution tests and their limits. Full RFC readiness and companion checking remain incomplete. The confirmed file-level snapshot policy does not itself approve these additions. Preserve the stated namespace and origin rules.

[Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) proposes expression, binding and control rules. Its precedence, evaluation, binding and parsing policies are confirmed. Its full text is not accepted.

Draft RFCs [0004](0004-functions-overloads-and-generic-requirements.md) and [0005](0005-types-records-enums-and-inference.md) propose function and type companions. Their full texts remain Draft. [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) proposes ownership and cleanup.

[Draft RFC 0010](0010-diagnostics-and-reporting.md) proposes diagnostics/reporting. Concurrency/runtime and full LSP companions remain missing. Their exact sections and review revisions are needed. This draft does not supply complete overload ranking or protected-action checking through a supporting link.

### Implementation/validation gates

M3 supplies scoped resolution and real filesystem tests on the recorded local macOS AArch64 host. The [hosted record](../../crates/compiler/tests/m3-acceptance.md#hosted-publication-evidence) adds passing resolver tests on four Linux/macOS runner profiles. Native/runtime acceptance still fails as unavailable. The typed checker, persistent source cache, runtime and complete client workflows remain unimplemented. Structural text checks cannot replace those gates. ri-48g retains formalization; ri-bv2 retains actual LSP delivery.

### Future/out-of-scope work

Package management, configurable search paths, inline modules and automatic index/mod facades remain outside v0. This RFC adds no remote import scheme or public native-registration API. It does not define a new standard-library release identity or installer format.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. Status is Draft. Source comparisons and structural checks do not accept this proposal. Resolver details and companion rules still need review against the [canonical checklist](README.md#canonical-reviewready-checklist). The policy approvals do not complete that review.

### Human decisions and amendments

The owner approved the M3 procedure through checkpoint `a41e7cc955e67329d6dc3a8a69de667e0af204ac` under ri-1fk. The approval record (ri-1fk, comment 6939478619) identifies O1 through O7 and the test seams. This decision authorizes scoped implementation, not full RFC acceptance or readiness. Existing case evidence and earlier policy approvals retain their original scopes.

On 2026-11-17, the owner requested the complete syntax grammar. The owner then confirmed all three syntax groups (ri-48g, comment 6931939690) at the two identified annex snapshots. Existing lexical/source and semantic approvals retain their exact scopes. Full annex/RFC acceptance, readiness and implementation remain separate.

On 2026-11-17, the owner requested completion of the lexical/source contract. The owner then confirmed all three lexical/source groups (ri-48g, comment 6929923825), including this file-level snapshot/UTF-8 policy. Earlier canonical identity/order approvals remain unchanged. No full RFC acceptance is inferred.

| Date / actor | Change | Reviewed revision or snapshot | Decision / rationale |
| --- | --- | --- | --- |
| 2026-11-16, project owner | Requested the module RFC after RFC 0001. | No RFC 0002 revision was accepted. | The request authorizes drafting, not acceptance or renumbering an existing RFC. |
| 2026-11-16, drafting agent under ri-48g | Created and registered this substantive draft as RFC 0002. | Source inputs use `f33dcf4cf3f815824eeb897135775d7884f65c1a`. RFC 0001 uses the accepted snapshot identified in metadata. | New filesystem and ordering details are proposals. No implementation or formal ASD-STE100 compliance is claimed. |
| 2026-11-16, project owner | Approved the filesystem identity and canonical ordering policies. The full RFC remains Draft. | The two policy recommendations presented in chat. No complete RFC revision was accepted. | Recorded approval (ri-48g, comment 6908371073). Remaining authoring and validation work stays open. |
| 2026-11-16, drafting agent under ri-48g | Linked Draft RFC 0003 as a proposed companion. | No policy approval or complete RFC acceptance changed. | Expression and control rules still need their own review. |
| 2026-11-17, project owner | Approved the coordinated lexical/source groups, including file snapshots and UTF-8 rejection. | Pre-recording SHA-256: `f9283861444fb3004d31b7dd7c563f27228916258013f606eb4059102413de08`. The shared record (ri-48g, comment 6929923825) identifies the other three snapshots. | Policy approval only. Graph freshness, full RFC acceptance, readiness and implementation remain open. |
