# Explicit, evidence-preserving v0 library contracts

[Issue #23](https://github.com/Reilley64/rill/issues/23) confirms the language-required standard-library APIs in [the public contract catalogue](../standard-library-contracts.md). Use owned collections, explicit access/copying, generic callable evidence and canonical protected ownership actions rather than persistent snapshots, erased capture assumptions, return-only dispatch or a broad implicit prelude. The complete interview (Q1-Q53) and consolidated summary were explicitly confirmed; this records design input, not an implemented library, accepted feature RFC or checker/backend proof.

ADRs 0013/0014 identify #29's error and #22's stack decisions prepared in separate sessions. This decision follows both, including when a separately prepared artifact is not yet present in a checkout.

## Later implementation and conversion refinement (#41)

[ADR 0017](0017-user-implementable-conversions.md) supersedes the closed-only numeric conversion boundary below and the old snake_case try spellings. Canonical into/from and tryInto/tryFrom allow user endpoint-owned implementations, destination/source specificity and implementation-defined typed errors. Numeric pairs retain protected standard behavior and ConversionError. Parsing stays separate. `implement` becomes language-wide family-body syntax; declarations and standalone functions retain `function`. The updated catalogue and prototypes distinguish these exceptions from unchanged ordinary overload, ownership and error/cause rules.

## Later streaming extension (#32)

[ADR 0018](0018-streaming-and-buffered-io.md) extends the baseline with separate ordinary read/write/flush/close families, exact typed progress errors, bounded append reads and owning buffered adapters. It adds Bytes clear/truncate/reserve, not a competing buffer or writable view API. Q36 explicitly requires scoped noBorrow evidence for readToEnd/local-buffer adapter errors and the flush error preserved across buffered consuming close, not a blanket prohibition on primary borrowed errors. Existing stdio/whole-file signatures and their partial-effect warnings stay unchanged; no reliable progress count is retrofitted into them. Explicit consuming close observes finalization failures, while disposal discards pending output and still completes required native cleanup. #33/#34/#35/#38/#39/#40 own deadlines/concrete consumers; #30 remains the backend gate. See the updated catalogue and [streaming examples](../../prototypes/12-streaming-io.rill) for hand-checked cases, not implemented streaming.

## Later filesystem extension (#35)

[ADR 0019](0019-paths-files-and-filesystem-operations.md) adds byte-preserving Path, access-mode-specific file handles, checked seek/explicit sync, focused metadata/directories/symlinks and nonrecursive temporary guards. Existing text whole-file APIs keep their guarantees and gain &Path implementations; IoErrorKind gains explicit filesystem outcomes. Native/WASI capability limits, path races, partial effects and cleanup are recorded rather than treated as a sandbox/durability proof. See the catalogue and [filesystem examples](../../prototypes/13-filesystem.rill); #28/#30 retain formal/provider validation.

## SQL extension (#40)

[ADR 0021](0021-partial-standard-sql-contract.md) and the [integrated SQL contract](../sql-contracts.md) now record the explicitly confirmed shared interface, including fixed control arities and #33 temporal/token integration. External drivers/pools/ORMs remain separate; native-provider work moves to #43 without being declared fulfilled. Named conversion helpers/user JSON examples preserve #41/#31; intrinsic cancellation stays outside Result while explicit SQL tokens have typed outcomes. The narrow std::time::validateInstantDomain amendment avoids private field access. Design closure is not runtime/provider support; #28/#30 retain formal/backend gates.

## Decisions and consequences

- `List` and hash `Map` own their data, support explicit mutation and produce independent copies. They use u64 counts/indices, Option queries, read-only keys and explicit eq/hash contracts. Map insertion retains an existing equal key and returns its displaced value. List transformations are eager, sequential and support both consumed and borrowed input.
- Add compiler-recognized callable(F,[Args],R), callableMut and callableOnce requirements, with weakening but no forged invocation/capture evidence. Keep callbacks generic and check before monomorphization; ordinary arrow erasure cannot invent noBorrow/sendable/copy properties.
- Canonical copying is `copy(&value)` with `copy<T>(&T):T`; disposal consumes T. This supersedes the older copy(value) convenience. Extend implicit-copy eligibility structurally to enums, never from a current runtime payload or a literal refinement.
- Protect genuine guard release, counted ownership/endpoint maintenance and native source retention from user interception, including otherwise orphan-permitted nested specializations. Ordinary unprotected defaults remain specializable. After a custom disposer, compiler cleanup disposes remaining initialized owned fields without duplicating moved/disposed fields or recursively reinvoking the outer disposer. This newly approves remaining-field semantics; #17 had not settled a mandatory nested cleanup mechanism.
- Use ordinary canonical iterate/next families for for-of, including user-defined containers. Borrowed traversal retains sources; exclusive yielded access ends before the next advance. No general disjoint-index proof or mutable stored-map-key access.
- Use explicit library module/factory paths. Primitive string/char already exist in the fixed prelude; helpers do not. Source panic is std::panic; compiler panic machinery is not redirected by lexical shadowing. Log is synchronized, text-only, void-returning and fatal on output failure; fallible I/O is separate.
- Spawn and fixed-arity spawnWithStack preserve owned once-callable/noBorrow/sendable evidence. Follow #22's addressable/saved-stack defaults/options, target validation, actual borrower completion and bounded standard traversal/default cleanup; #30 still gates backend support.
- Scoped join/all/allSettled/race/any take tuples of 2-4 jobs; no variadic generics or dynamic-sized combinators. Plain join and fallible all are separate to avoid callback-return-type dispatch. Preserve common-type requirements and all child cleanup before return. Any's total failure stores all original envelopes in AllFailed<E>.
- Channels take explicit u64 capacity; failed send returns its undelivered T in SendError<T>. Select takes receiver/handler tuples, commits exactly one receive and invokes only that handler. Closed/drained sources are ready with None; priority/fairness is not promised. Retire singleton string tags. Sleep/after use monotonic u64 milliseconds, not hard deadlines.
- Follow #29's typed Err<E> wrappers, origins and explicit causes, not propagation reports. Canonical Err construction preserves the agreed one-/two-argument forms as a closed, location-capturing exception; adapt it with an arrow for first-class use. Ordinary type/value namespaces share the name Err. Default text Error construction is explicit error(message); raw strings do not implicitly convert to Error.
- **Refine #29's erased-cause boundary.** Primary E may carry checked borrows, but retaining/erasing it as a cause needs display/noBorrow/sendable/shareable evidence. Unique Cause nodes preserve all original data/origins/formatters without public dyn/downcasts, hidden counts or a general Err/Result copy. Postfix propagation is unchanged; infix wrapping creates one new origin. mapErr preserves existing metadata while changing the payload; it does not add a causal layer.
- Bytes supplies genuine contiguous owned storage/logical borrowed subranges, separate from unspecified List layout. Distinguish source-retaining viewUtf8 from copying decodeUtf8/decodeLossy; no writable UTF-8 escape or mutable substring. Provide explicit Unicode queries/traversal and structured decoding errors.
- Finalize closed numeric conversions and parseNumber, six explicit wrapping/saturating integer helpers, typed errors and non-recursive canonical primitive formatter bootstrap. No as/assertions, implicit numeric/error conversions or default widths.
- Required I/O is direct stdio and whole-file byte operations with typed owned errors. Writes can be partial on failure/cancellation; no rollback, atomic replacement, durable-sync, general reader/writer or networking promise. Allocation failure remains fatal.

[ADR 0019](0019-owned-startup-environment-and-arguments.md) / #36 separately extend this baseline with std::env owned startup Inputs, strict byte/text access and isolated test inputs. The original minimum here is not a prohibition on that approved application foundation; #23 remains closed. The catalogue's section 12 records its complete contract without adding source globals, process mutation or implicit captures.

[ADR 0020](0020-secure-random-byte-generation.md) / #37 add std::random secure bytes with complete-fill-or-error, explicit platform providers/host trust, provider-free empty calls and native-retention/cancellation rules. Errors remain separate from fatal allocation/host failure. Real secure behavior also applies in rill test; #36's empty environment is not a general deterministic-provider override. This is another approved extension, not a reopening of the core baseline.

## Alternatives and supersession

The rejected alternatives are:

- Persistent collection defaults, implicit callback copying and once-only map callbacks.
- Erased arrows as spawn evidence and trait-based invocation/iteration.
- Broad ownership sealing/outermost-type orphan restrictions.
- Runtime overload dispatch, variable-arity ordinary helpers, hidden return-based all flattening and string-tagged select.
- Reflection-based error formatting and report/trace-by-propagation.
- Unsafe byte views and recoverable allocation errors everywhere.

This supersedes relevant interface sketches in #7/#8/#10/#15/#16/#17/#18/#19/#21 and refines #29. It does not undo nominal enum/record rules, explicit numeric widths, UTF-8 validity, ordinary family identity/collection, no named constraint groups, ownership/dependency/concurrency policy, proper-tail eligibility or fatal panic. Older fixtures are historical where spellings/contracts conflict, not silently declared fully migrated.

## Evidence and follow-through

[Prototype 9](../../prototypes/09-library-contracts.rill) and its [case inventory](../prototypes/09-library-contracts.md) are hand-checked design artifacts, not executed tests. #24 owns diagnostics; #25 exercises the combined program; #28 must specify/check generic inference, transitive dependencies, protected overlap/field cleanup, sealed-cause evidence, native states, bounded traversal and accepted/rejected cases. #30 remains the pinned backend gate. An inconsistent required interface/rule returns to the map rather than becoming a claimed safety proof.

The user also approved a separate v0 JSON-design ticket, [#31](https://github.com/Reilley64/rill/issues/31), for std::encoding::json with explicit value conversions and deliberate numeric precision. JSON feeds #28 but does not block completing #23; no parser/serializer is implemented here.
