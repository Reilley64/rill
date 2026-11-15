# Partial standard SQL contract — #40

**Status: human-confirmed independent decisions; incomplete integration contract.** The user approved Q1–Q57 and explicitly confirmed recording this checkpoint. [#40](https://github.com/Reilley64/rill/issues/40) stays open. [#33](https://github.com/Reilley64/rill/issues/33) still gates deadline/time/calendar integration and final operation parameter lists. An embedded native provider remains separately unspecified/unvalidated. Nothing here implements a driver, pool, ORM, protocol, compiler or security/soundness proof, or accepts a future RFC.

[ADR 0021](adr/0021-partial-standard-sql-contract.md) records placement/tradeoffs. [Primary-source research](research/0040-standard-sql-and-unique-errors.md) compares Go/pgx/sqlc/SQLx/Diesel/jOOQ and actual backend error fields. [Prototype 22](../prototypes/22-sql-codecs/main.rill) and its [inventory](../prototypes/22-sql-codecs/README.md) are hand-checked codec/error evidence. Numbers avoid #37's separately published ADR 0020/prototype 21 reservation. JSON's authoritative [#31 snapshot](https://github.com/Reilley64/rill/issues/31#issuecomment-6886030678) has separate drafts; its colliding historical numbers are not reused as identities here.

## 1. Placement and non-goals

Shared execution, value/codec, row and transaction interfaces belong in **std::sql**, not an external common database module. Driver-facing canonical families live in **std::sql::driver**; std::sql re-exports appropriate identities and supplies helpers. Re-exports do not create another implementation set. Drivers, concrete pool implementations, SQL dialect compilation/query ASTs, generated binders/decoders, entity mapping, change tracking and migrations remain external. No std driver implementation is promised.

This borrows Go's shared application/driver seam, not Go's dynamic interfaces, registry or std-owned sql.DB pool. Concrete connectors select drivers statically; no global registration, driver-name lookup, universal connection-string parser, public dyn, associated types, reflection, SQL closure capture, package manager or general FFI. Independent source modules can extend imported canonical families under the existing orphan/visibility/duplicate rules. An external generator can emit explicit SQL and ordinary calls without becoming a compiler reflection feature.

Common mechanics are not universal SQL semantics: placeholders, native types, constraints, row-count meanings and capabilities remain backend-specific. One statement/one result stream is the common contract. Reject unsupported batches/scripts/additional results rather than silently execute a prefix or ignore results. Do not split SQL on semicolons. Native extra capabilities remain accessible on concrete driver types.

## 2. Confirmed value surface

The following value shapes record the agreed common subset. They are not a mandatory universal wire/value ceiling; driver-specific values can use additional concrete encoding/conversion implementations.

```rill
enum ParameterType {
  Bool, I8, I16, I32, I64, I128, U8, U16, U32, U64, U128,
  F32, F64, Text, Binary, Decimal,
}
enum Parameter {
  Null(ParameterType), Bool(bool),
  I8(i8), I16(i16), I32(i32), I64(i64), I128(i128),
  U8(u8), U16(u16), U32(u32), U64(u64), U128(u128),
  F32(f32), F64(f64), Text(string), Binary(Bytes), Decimal(Decimal),
}
enum ColumnRef { Index(u64), Name(string) }
type ExecSummary = { rowsAffected: Option<u64> };
```

Parameter owns data. No untyped NULL, implicit absence, zero/false/empty substitute or lost source width. Drivers may use an exact wider native representation/check value ranges, but must not round/narrow silently, route exact integers/decimals through f64, replace invalid text, or turn unsupported float values into SQL NULL. A common decimal need not be supported by every backend: an incapable encoder returns an unsupported/type/range error. Explicit text-storage mapping is different, not a hidden fallback. Library encoding guarantees do not audit arbitrary coercions in SQL expressions, columns or triggers.

ColumnRef uses zero-based ordinals and exact returned names. Missing/out-of-range columns have structured errors; duplicate matching names are ambiguous, not first-wins. No SQL identifier folding/qualification guessing. Explicit aliases resolve collisions. Generated IDs require row-returning SQL or native capabilities, not a universal lastInsertId. Affected-row counts can be unavailable and retain backend meaning.

Parameter, Decimal, encoded arguments, owning prepared descriptors and default owning rows supply noBorrow evidence. This describes owned storage, not purity, implicit copying, session/native-resource sendability/shareability, or a universal restriction on error payloads. Borrowed cell/text views still depend on their owner. Argument encoders own bound data; a borrowed convenience encoder copies rather than secretly retaining the caller's borrow. A failed bind leaves the logical argument list unchanged, preserving earlier slots. One custom conversion occupies one slot, not automatic record/column expansion.

### Exact decimal tokens

Decimal is an opaque owned finite token, not a general arbitrary-precision arithmetic type. Its grammar is ASCII:

```text
-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?
```

`decimal(text)` returns Result<Decimal,DecimalError>, accepting owned string or borrowed &string (copying the latter). `decimalWithLimit(text,maxBytes:u64)` overrides the token budget; `decimalText(&Decimal):&string` borrows the original token. Preserve sign/negative zero, fractional trailing zeros and exponent spelling. Reject whitespace, leading plus/leading-zero forms, NaN/infinity. DecimalError distinguishes InvalidSyntax with byte offset from LimitExceeded with safe length/budget context; it does not dump input values. It is owned with standard safe display/property evidence.

Check token length before a borrowed copy; validation is bounded by token length and never expands exponents or converts through float. Backend range/scale conversion must also reject safely without enormous expansion. The resemblance to JSON number grammar is not a shared parser identity or authorization to import JSON numeric rounding into SQL decoding. No equality-as-arithmetic, SQL-literal interpolation, automatic text fallback or arithmetic operations are added.

## 3. User codecs and typed errors

Use existing std::convert::into/tryInto, identity-preserving aliases, explicit implementation syntax and conversion-only `(To,From)` keys. Source/target must be established; error is selected output evidence, never a candidate selector. No implicit chains or automatic framework registration follows. Into does not automatically supply TryInto.

Standard primitive/owned text/bytes/Decimal representations can explicitly convert into Parameter; user modules implement their own application conversions. Direct native values are not forced through Parameter. Two meanings for one source/target require nominal wrappers or explicit operations; different errors/constraints cannot distinguish duplicate pairs. Endpoint ownership and ordinary orphan rules still apply; unrelated glue modules do not acquire authority by importing two foreign types. Generic user conversions may declare underlying canonical evidence without importing every concrete driver; crossed patterns can still be ambiguous.

Public helper distinctions are confirmed:

- **bind:** directly encode a supported concrete value into the driver's arguments.
- **bindInto:** explicitly invoke into<Parameter>, then bind; only the binding error is fallible.
- **tryBind:** explicitly invoke tryInto<Parameter>, then bind; preserve conversion and binding errors separately.
- **get<T>:** one generic helper for cell access followed by canonical tryInto<T>; not ordinary get implementations selected by return type.

```rill
enum BindError<C,E> {
  Conversion(Err<C>),
  Binding(Err<DbError<E>>),
}
enum GetError<E,C> {
  Access(Err<DbError<E>>),
  Conversion(Err<C>),
}
```

get returns checked views/values according to the actual conversion. Plain T rejects SQL NULL; Option<T> maps NULL to None and still checks non-null T. No text-number parsing, precision-losing coercion or default substitutes masquerade as checked extraction. Explicit lossy work is separate (for example explicit numeric conversion after exact extraction or an explicit SQL cast); approved primitive numeric semantics remain unchanged.

User codec errors remain concrete and retain their Err envelope/origin/cause data. Helpers do not force them into a native error. Neither direct get over caller-owned rows nor binding over caller-owned arguments gets blanket noBorrow(C/E) requirements. Owning convenience helpers must require noBorrow for outputs/errors that could otherwise retain destroyed local row/cursor/storage or conflict with required cleanup. State exact requirements per implementation; do not let monomorphization repair missing evidence.

## 4. Database errors and uniqueness

DbError<E> has a first-class UniqueViolation and typed native failures, plus standard local cases for invalid arguments, decoding/null/range/precision/encoding, cardinality, invalid state, unsupported operations and limits. Local failures do not manufacture E. The partial structural vocabulary used by the examples is:

```rill
enum DbError<E> {
  UniqueViolation { info: UniqueViolation, source: Err<E> },
  Driver(Err<E>),
  InvalidArgument(ArgumentError),
  Decode(DecodeError),
  Cardinality(CardinalityError),
  InvalidState,
  Unsupported,
  LimitExceeded(LimitKind),
}
enum ArgumentError {
  ParameterCount { expected: u64, actual: u64 },
  ColumnNotFound(string), AmbiguousColumn(string),
  ColumnOutOfBounds { index: u64, columns: u64 },
}
enum DecodeError { UnexpectedNull, TypeMismatch, OutOfRange, PrecisionLoss, InvalidEncoding }
enum CardinalityError { NoRows, MoreThanOneRow }
```

These are confirmed categories with illustrative minimum payload shapes, not the closed final enum inventory/ABI. Preserve native diagnostics for everything not reliably classified; do not classify by finding an English word in a message. Native SQLSTATE/codes remain available through the concrete E. No retry implication follows from any category.

UniqueViolation covers actual primary-key/unique-key/rowid collisions, not only UUIDs. A composite `(tenant_id,email)` is one tuple constraint, not two independently unique columns. Ordinary native codes can identify uniqueness even when key details are unavailable. The research shows no universal complete column list: PostgreSQL generally reports constraint/index identity, SQLite no structured failing-key identity, and MySQL/SQL Server object names primarily in message text. Expression/partial indexes and triggers further defeat simplistic mapping.

Store names separately, with provenance, not ambiguous dotted strings:

```rill
enum DetailOrigin { ServerReported, DriverContext, MetadataObservation, DeclaredSchema }
type NameDetail = { name: string; origin: DetailOrigin };
type KeyIdentity = {
  database: Option<NameDetail>; schema: Option<NameDetail>;
  table: Option<NameDetail>; key: Option<NameDetail>;
};
enum KeyPart { Column(string), Expression(Option<string>) }
enum KeyCondition { Unknown, Unconditional, Partial(Option<string>) }
type KeyDescription = {
  parts: List<KeyPart>; condition: KeyCondition; origin: DetailOrigin;
};
enum KeyDetails { Unavailable, Described(KeyDescription) }
type UniqueViolation = { identity: KeyIdentity; details: KeyDetails };
```

Unavailable is not an empty complete key. Known direct columns are ordered; exclude INCLUDE/non-key attributes. Expressions are marked, not mislabeled as source-column uniqueness. A partial condition may be known without its SQL text. A complete description concerns the object/provenance actually established; a later metadata observation does not assert the original failure's schema snapshot. If association is uncertain, do not attach a guessed key. Preserve quoted/case-sensitive names; encoding failure does not justify replacement names. Copying plain diagnostic metadata can be explicit; no general error-envelope copy is introduced.

Metadata enrichment is a separate effectful optional resolver capability, not hidden SQL in an error accessor. Return its result/error separately, leaving the original typed failure intact. It may be unavailable/unsupported, denied, stale or unusable in an aborted transaction. Use the correct qualified identity and context, not search_path/name alone; a second connection does not recreate the failure snapshot. No mandatory heuristic message parser or standard schema cache is approved.

### Safe defaults, explicit native inspection

Default DbError/BindError/GetError/transaction formatting shows category and escaped safe identifiers/known column names only. It does not automatically print raw SQL, parameters, duplicate values, expression/predicate SQL, arbitrary codec messages or native error rendering. Retain typed Err<E>/Err<C> as fields, **not automatically as an erased Cause** whose recursive renderer could leak them. Redacted display can avoid requiring display(E/C) merely to render the standard wrapper; constructing a source Err or explicitly formatting it still obeys its own display evidence.

Raw diagnostics remain available for deliberate inspection of typed payloads, with documented sensitivity. If an application explicitly wraps a raw native error as Cause/logs it, that is not a guarantee of redaction. Existing noBorrow/sendable/shareable requirements for heterogeneous causes remain; no general downcast facility or implicit error adaptation.

## 5. Ownership, preparation and execution

A physical session is an owned noncopying resource. One active operation has exclusive access; rows/transactions retain dependencies. Concrete drivers remain accessible for extra capabilities. Preparation returns owning descriptors of SQL/parameter metadata, not dangling native pointers or permanent exclusive session borrows. Native/server prepared resources belong to bounded session-managed state; eviction/repreparation/session affinity/schema failure are explicit driver responsibilities. Several descriptors can be alive without preventing unrelated session use. No preparation guarantee survives arbitrary schema change.

Raw execution consumes owned SQL string and arguments. Prepared execution borrows its owning descriptor and consumes arguments. Returned cursors retain necessary session/descriptor/native storage dependencies; owning rows survive advance/close and only later cell views depend on those rows. No default zero-copy cursor view is added.

Canonical iterate/next yields Option<Result<Row,DbError<E>>>. A streaming error is yielded once then terminal None. EOF alone does not prove protocol synchronization/reuse. Explicit consuming result close can drain/cancel/resynchronize or report failure; disposal/early abandonment conservatively discards unless cleanup actually completed and positive safe-state evidence exists. No detached cleanup followed by premature reuse.

queryOne requires exactly one row; queryOptional permits zero or one; a second row is a structured cardinality failure. Finalize local cursors and preserve operation/cleanup errors, with scoped borrow-free evidence where retained failures would otherwise depend on destroyed storage. No unlimited queryAll; larger collections are explicit budgeted loops.

## 6. Transactions and external pools

Beginning a transaction returns a guard retaining exclusive session access. Queries/prepared execution use that guard; live rows prevent commit/rollback. Nested/savepoint operations are explicit optional capabilities, never simulated nested transactions. Options distinguish backend-default isolation, common explicit levels and a read-only request; unsupported requests fail, not silently substitute/ignore. Read-only is a backend transaction restriction, not proof of external-function purity. The exact options record and deadline fields remain pending integration.

Commit/rollback consume the guard even on failure. Success confirms the corresponding outcome; failure separately reports ConfirmedCommitted/ConfirmedRolledBack/Unknown and preserves finalization plus cleanup errors where both occur. A failed COMMIT reply is not proof of noncommit; late rollback/close cannot undo committed effects. Abandoned active transactions mark the session unusable and safely clean up rather than queue rollback and advertise immediate reuse. No promise of acknowledged remote rollback.

Managed guards own transaction control. Manual BEGIN/COMMIT/ROLLBACK/savepoint commands through ordinary execution are outside that managed contract. Std does not pretend to parse arbitrary SQL; drivers detect unexpected protocol state where possible and discard uncertainty, not manufacture a valid guard. Native escape operations have separate responsibilities.

Reuse disposition distinguishes CandidateForValidationReset from Discard. An operation can fail on a usable protocol session or have unknown effects on a discarded one. Candidate is not future liveness, a reset, or clean session variables/temp state. External pools must perform their documented validation/reset before reuse and must not treat discard as retry permission. No automatic replay of writes, reads-with-possible-effects, unique failures or unknown commits. Idempotency/reconciliation is an explicit application policy.

## 7. Limits

Proposed tuning values were explicitly accepted as safety defaults, not measured optimal performance. All public sizes/counts use checked u64; lower backend/ABI limits still apply, never wrap. First applicable limit wins. Settings are configurable; exceeding a client budget is a structured limit error, distinct from existing fatal allocator/addressability failure.

| Controlled quantity | Default |
| --- | ---: |
| Encoded parameter payload total | 16 MiB |
| One field payload | 16 MiB |
| One owning row payload total | 64 MiB |
| Protocol frame | 64 MiB |
| Result metadata | 1 MiB |
| Result columns | 4,096 |
| Parameters | 32,767 |
| Decimal token | 1 MiB |

These bound controlled payload/metadata copies and known-size protocol allocations, not exact allocator footprint, server work, every native-engine allocation or cancellation latency. Apply checks before controlled allocation where size is known. Native SQLite engine limits need provider-specific configuration/validation. An oversized frame/row may require discarding the session, not unbounded draining or a partial owning row presented as success. Session cache budgets must be bounded and documented by the driver; no unbounded cache is implied by prepared descriptors.

## 8. Deadline-independent family relationships — not final arities

The following are **schematic shapes for evidence review**. They deliberately omit unresolved #33 options/context and are not final callable declarations. Do not implement/freeze them and later silently change ordinary family arity.

```rill
// std::sql::driver identities; appropriate names re-exported by std::sql
connect<C,S,E>(C): Result<S,DbError<E>>
arguments<X,A>(&X): A
bind<A,V,E>(&mut A,V): Result<void,DbError<E>>
prepare<X,P,E>(&mut X,string): Result<P,DbError<E>>
execute<X,Q,A,E>(&mut X,Q,A): Result<ExecSummary,DbError<E>>
query<X,Q,A,R,E>(&mut X,Q,A): Result<R,DbError<E>>
begin<S,T,E>(&mut S, /* options pending */): Result<T,DbError<E>>
cellAt<R,C,E>(&R,ColumnRef): Result<&C,DbError<E>>
resolveUnique<M,E>(&mut M,&KeyIdentity): Result<KeyDetails,DbError<E>>
// commit/rollback: consume T, report independent outcome and phase errors.
// result/session close can use canonical std::io::close identity with SQL E.
```

Input patterns determine S/A/P/R/C/E; expected result/error types never choose another implementation. No associated projection or runtime registry. Public generic requirements name exported canonical families. In particular, conceptual evidence includes connect(C,S,E), arguments(X,A), bind(A,V,E), prepare(X,P,E), query(X,Q,A,R,E), cellAt(R,C,E), and canonical next(R,Result<Row,DbError<E>>). State noBorrow(A/P/Row) explicitly where required; do not add noBorrow(Session) or sendable/shareable by analogy. Standard close's error parameter, when reused, is the **whole DbError<E>**, not E alone.

For conversion-aware helpers, state into(Parameter,T) or tryInto(Parameter,T,C), plus bind(A,Parameter,E). For get<T>, state cellAt(R,C,E) and tryInto(T,&C,D); its error is GetError<E,D>. T chooses an already-approved canonical conversion, not ordinary return-only get dispatch. User errors can differ from native E and from other operations; retain separate phase parameters rather than assume equality. A helper owning local cursor/row storage must explicitly require borrow-free relevant errors/decoded outputs before destroying that storage. Concrete owned native diagnostics often satisfy this, not a general implicit rule for every E.

Final native-provider shapes, transaction error payload inventory, time/deadline options, temporal values and detailed generic cleanup/candidate rules remain open work; this checkpoint is not a completed checking mechanism.

## 9. JSON integration and native-provider gate

JSON's standard representation remains std::encoding::json::Value, with owned trees/precision-preserving Number and separate parse/stringify errors. Users already may implement explicit into/tryInto to/from Value. SQL adds a deliberately named conversion-aware binding operation; this is a narrow library integration beyond #41's baseline, not automatic conversion registration. Parsing/stringifying do not discover records, invoke arbitrary conversions or gain a mandatory show(Value). Owned/borrowed conversion sources remain distinct. JSON's exact integer/float construction/extraction policies do not become SQL's semantics or vice versa.

The example domain module implements SQL Parameter and JSON Value conversions and a generic checked cell decoder without a Serializable trait or per-driver imports. A whole record/table mapper is still external and explicit. #31/#41 need examples/cross-links, not re-opening their parser or numeric decisions. Do not edit other worktrees or assume same-number ADRs identify the same draft.

A PostgreSQL-like adapter owns TCP/TLS protocol state and errors; a SQLite-like adapter needs a separately specified narrow native-provider contract for native connections/statements/bindings/reset/finalization. Ordinary families express the seam but do not implement C ABI hooks. No raw pointers, public thread pinning or general FFI is authorized. Provider operations/disposal must be valid under worker migration and retain source/native state until completion or confirmed abortion. Blocking-pool dispatch/abort request is not release; Wasm pointer stability remains #30 work.

Cancellation stays outside Result, leaves possible effects and returns no progress/outcome to stopped code. Required cleanup may suspend indefinitely; no hard return bound from a timer race. #33 must settle actual deadline propagation/races and shared wall-clock/calendar choices; no competing epoch/duration/SQL temporal type is invented here.

## 10. Outstanding completion work

- Keep #40 open and its #33 native dependency intact. Read #33's confirmed result when available, then interview/finalize SQL deadline and temporal integration and operation arities.
- Specify/validate the narrow embedded native-provider prerequisite; adapter sketches are not supported driver delivery.
- Complete transaction options/outcome/error declarations, generic row-helper cleanup evidence and optional capability/resolver payload details in the integrated contract. Current categories/relationships are confirmed, exact remaining shapes are not frozen.
- #28 formalizes pattern specificity, selected outputs, generic evidence/error preservation, source/row/argument dependencies, limits, provenance/redaction and exactly-once state. #30 gates actual native/Wasm mechanisms. Provider/protocol/security tests remain entirely outstanding.
- Reconcile separately prepared JSON/time/random drafts and number reservations without overwriting them. Research/prototypes/ADRs are inputs, not accepted feature RFCs. Any unworkable mechanism returns to map #1 rather than being presumed proven by this checkpoint.
