# Standard SQL interface contract — #40

**Status: human-confirmed shared-interface design.** The user confirmed the Q1–Q57 checkpoint, then Q58–Q77 and the consolidated integration summary after [#33](https://github.com/Reilley64/rill/issues/33) closed. This supersedes the partial checkpoint: control, temporal mappings and fixed operation arities are now specified. [#40](https://github.com/Reilley64/rill/issues/40) completes shared-interface design only. The separately approved [embedded native-provider follow-up #43](https://github.com/Reilley64/rill/issues/43) remains unfulfilled and gates claims of embedded support, not this design closure or RFC drafting. Nothing here implements a driver, pool, ORM, protocol, compiler or security/soundness proof, or accepts a future RFC.

[ADR 0021](adr/0021-partial-standard-sql-contract.md) records placement/tradeoffs; its historical filename is retained for existing links. [#33's confirmed snapshot](https://github.com/Reilley64/rill/issues/33#issuecomment-6891620139) supplies temporal/clock/token types; its separately prepared files are not assumed present in this checkout. [Primary-source research](research/0040-standard-sql-and-unique-errors.md) compares Go/pgx/sqlc/SQLx/Diesel/jOOQ and actual backend error fields. [Prototype 22](../prototypes/22-sql-codecs/main.rill), its [control/temporal entry](../prototypes/22-sql-codecs/control.rill) and [inventory](../prototypes/22-sql-codecs/README.md) are hand-checked codec/error/control evidence. Numbers avoid #37's separately published ADR 0020/prototype 21 reservation. JSON's authoritative [#31 snapshot](https://github.com/Reilley64/rill/issues/31#issuecomment-6886030678) has separate drafts; its colliding historical numbers are not reused as identities here.

## 1. Placement and non-goals

Shared execution, value/codec, row and transaction interfaces belong in **std::sql**, not an external common database module. Driver-facing canonical families live in **std::sql::driver**; std::sql re-exports appropriate identities and supplies helpers. Re-exports do not create another implementation set. Drivers, concrete pool implementations, SQL dialect compilation/query ASTs, generated binders/decoders, entity mapping, change tracking and migrations remain external. No std driver implementation is promised.

This borrows Go's shared application/driver seam, not Go's dynamic interfaces, registry or std-owned sql.DB pool. Concrete connectors select drivers statically; no global registration, driver-name lookup, universal connection-string parser, public dyn, associated types, reflection, SQL closure capture, package manager or general FFI. Independent source modules can extend imported canonical families under the existing orphan/visibility/duplicate rules. An external generator can emit explicit SQL and ordinary calls without becoming a compiler reflection feature.

Common mechanics are not universal SQL semantics: placeholders, native types, constraints, row-count meanings and capabilities remain backend-specific. One statement/one result stream is the common contract. Reject unsupported batches/scripts/additional results rather than silently execute a prefix or ignore results. Do not split SQL on semicolons. Native extra capabilities remain accessible on concrete driver types.

## 2. Confirmed value surface

The following value shapes record the agreed common subset. They are not a mandatory universal wire/value ceiling; driver-specific values can use additional concrete encoding/conversion implementations.

```rill
enum ParameterType {
  Bool, I8, I16, I32, I64, I128, U8, U16, U32, U64, U128,
  F32, F64, Text, Binary, Decimal, Date, Time, DateTime, Timestamp,
}
enum Parameter {
  Null(ParameterType), Bool(bool),
  I8(i8), I16(i16), I32(i32), I64(i64), I128(i128),
  U8(u8), U16(u16), U32(u32), U64(u64), U128(u128),
  F32(f32), F64(f64), Text(string), Binary(Bytes), Decimal(Decimal),
  Date(Date), Time(Time), DateTime(DateTime), Timestamp(Timestamp),
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

```rill
function decimal<S>(text: S): Result<Decimal,DecimalError>;
function decimalWithLimit<S>(text: S, maxBytes: u64): Result<Decimal,DecimalError>;
decimalText(&Decimal): &string
enum DecimalError {
  InvalidSyntax { offset: u64 },
  LimitExceeded { maxBytes: u64, actualBytes: u64 },
}
```

The two factory families accept owned string or borrowed &string (copying the latter). `decimalWithLimit(text,maxBytes:u64)` overrides the token budget; `decimalText(&Decimal):&string` borrows the original token. Preserve sign/negative zero, fractional trailing zeros and exponent spelling. Reject whitespace, leading plus/leading-zero forms, NaN/infinity. DecimalError distinguishes InvalidSyntax with byte offset from LimitExceeded with safe length/budget context; it does not dump input values. It is owned with standard safe display/property evidence.

Check token length before a borrowed copy; validation is bounded by token length and never expands exponents or converts through float. Backend range/scale conversion must also reject safely without enormous expansion. The resemblance to JSON number grammar is not a shared parser identity or authorization to import JSON numeric rounding into SQL decoding. No equality-as-arithmetic, SQL-literal interpolation, automatic text fallback or arithmetic operations are added.

### Temporal values from #33

Date/Time/DateTime are std::time::civil types; Timestamp is std::time::Timestamp. They are existing nominal implicit-copy/noBorrow/sendable/shareable scalars, not new SQL wrappers. Standard into<Parameter> pairs pack these validated values; they do not automatically supply tryInto pairs or database support. Drivers document faithful native mappings; absent such a mapping, reject unless the caller explicitly chooses a text/integer storage codec. Do not select a mapping merely from a backend's type-name spelling.

DateTime is civil, not an instant; Timestamp is the defined Unix-timeline instant, not a monotonic Instant. No hidden session/system zone, inferred epoch unit, numeric float seconds or automatic wall-clock-to-deadline conversion. Explicit timestamp extraction yields the instant, not a promise to preserve native zone/offset metadata. No implicit Zoned/TimeZone/Span/SignedDuration common parameter: retained rules, contextual calendars, mixed-sign database intervals and elapsed durations are not interchangeable. Concrete native codecs and explicit schema mappings remain available.

Reject unsupported ranges, invalid/zero dates, 24:00/leap seconds, native infinity sentinels or unrepresentable precision instead of repairing/truncating them. Nanoseconds bound to a microsecond representation must be divisible by 1,000. Explicit lossy conversion is separate. #33's civil/instant ranges and fixed-unit exact extractors apply; neither the library nor a type name audits coercions explicitly performed by SQL/columns/triggers. Retain an original Err<TimeError> in DbError.Temporal when a checked temporal operation fails; do not manufacture a native E or discard the original metadata. Local checks with no underlying TimeError can use DecodeError.

### Explicit owned operation control

```rill
// Clock/Instant/ClockError from std::time; token from std::cancel.
type SqlControl = {
  deadline: Option<[Clock,Instant]>;
  cancellation: CancellationToken;
};
uncontrolled(): SqlControl
// std::time consumer amendment approved in #40 Q68:
validateInstantDomain(&Clock, Instant): Result<void,ClockError>
```

SqlControl owns its handles, is noBorrow/sendable/shareable, and is not implicitly copied. Explicit copy retains the same clock service/token state and absolute target; it neither restarts a timeout nor aliases the caller's mutable record slot. uncontrolled supplies None and neverCanceledToken without obtaining/reading a clock. Each controlled operation consumes its control, allowing cursors to retain it without borrowing a temporary caller slot. Dropping ordinary handles does not request cancellation.

validateInstantDomain belongs to std::time, not std::sql: compare immutable private domain identity only, without reading the clock, registering a timer, exposing IDs or testing timeline health. Return DomainMismatch for a different domain; even a terminally failed matching clock still has matching identity, with read failure checked later if needed. This narrow public amendment avoids granting SQL privileged field access or misusing a wait just to validate a pair. It does not reopen #33's clock/deadline model.

Controls are per operation. Successful connect/begin does not impose its control on the session/transaction lifetime. For a whole transaction budget, explicitly copy the same absolute control to each operation. Idle transactions are not asynchronously disposed by a flag. Query cursors retain query control for subsequent advances; there is no ambient token or automatic inheritance. Local value/codec helpers gain no clock/token parameter. withDeadline remains #33's scoped helper with its own callable/sendability evidence, joined cleanup and exact nested Result; it neither flattens SQL errors nor sets token flags or automatically controls a returned cursor's future advances.

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

DbError<E> has a first-class UniqueViolation and typed native failures, plus standard local cases for invalid arguments, decoding/null/range/precision/encoding, cardinality, invalid state, unsupported operations and limits. Local failures do not manufacture E. The standard error vocabulary is:

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
  Cancelled { observation: ExecutionObservation, source: Option<Err<E>> },
  DeadlineExceeded { observation: ExecutionObservation, source: Option<Err<E>> },
  ClockFailure {
    observation: ExecutionObservation, clock: Err<ClockError>, source: Option<Err<E>>,
  },
  Temporal { error: Err<TimeError> },
}
enum ExecutionObservation { NotSubmitted, MayHaveExecuted }
enum LimitKind { ParameterBytes, FieldBytes, RowBytes, FrameBytes, MetadataBytes,
  Columns, Parameters, DecimalTokenBytes }
enum ArgumentError {
  ParameterCount { expected: u64, actual: u64 },
  ColumnNotFound(string), AmbiguousColumn(string),
  ColumnOutOfBounds { index: u64, columns: u64 },
}
enum DecodeError { UnexpectedNull, TypeMismatch, OutOfRange, PrecisionLoss, InvalidEncoding }
enum CardinalityError { NoRows, MoreThanOneRow }
```

These are public semantic shapes, not a native memory layout/ABI. Preserve native diagnostics for everything not reliably classified; do not classify by finding an English word in a message. Native SQLSTATE/codes remain available through the concrete E. No retry implication follows from any category. Control failures preserve independent typed clock/native diagnostics when present, with no fabricated native value for preflight errors. NotSubmitted concerns this requested operation, not prior transaction work or mandatory cleanup; on an admitted query it cannot merely describe one next call that fetched nothing. Transaction outcome and reuse disposition remain independent.

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

Canonical iterate/next yields Option<Result<Row,DbError<E>>>. Each active advance checks retained query control before selecting even a buffered row; already-requested/expired control can produce the one terminal failure, with MayHaveExecuted observation. After that preflight, completion/control races have no guaranteed winner. A streaming error is yielded once then terminal None. An already-terminal stream stays None without new token/clock failures or replacement of previously selected outcomes. EOF alone does not prove protocol synchronization/reuse. Explicit consuming result close can drain/cancel/resynchronize or report failure; disposal/early abandonment conservatively discards unless cleanup actually completed and positive safe-state evidence exists. No detached cleanup followed by premature reuse.

queryOne requires exactly one row; queryOptional permits zero or one; a second row is a structured cardinality failure. Finalize local cursors and preserve operation/cleanup errors, with scoped borrow-free evidence where retained failures would otherwise depend on destroyed storage. No unlimited queryAll; larger collections are explicit budgeted loops.

## 6. Transactions and external pools

Beginning a transaction returns a guard retaining exclusive session access. Queries/prepared execution use that guard; live rows prevent commit/rollback. Nested/savepoint operations are explicit optional capabilities, never simulated nested transactions. Options distinguish backend-default isolation, common explicit levels and a read-only request; unsupported requests fail, not silently substitute/ignore. Read-only is a backend transaction restriction, not proof of external-function purity.

```rill
enum IsolationLevel { BackendDefault, ReadUncommitted, ReadCommitted, RepeatableRead, Serializable }
type TransactionOptions = { isolation: IsolationLevel; readOnly: bool };
enum TransactionOutcome { ConfirmedCommitted, ConfirmedRolledBack, Unknown }
enum TransactionFailure<E,C> {
  Operation {
    outcome: TransactionOutcome, error: Err<DbError<E>>,
    cleanup: Option<Err<DbError<C>>>,
  },
  Cleanup { outcome: TransactionOutcome, error: Err<DbError<C>> },
}
```

readOnly=false requests no read-only restriction, not write authority. BackendDefault uses the documented backend default; explicit unsupported options are not silently substituted. Commit success confirms commit; rollback success confirms rollback. A successful finalization followed by failed cleanup uses Cleanup with the confirmed outcome. A native response that actually establishes rollback is not successful commit. Root control error selection is not replaced by a late acknowledgement, but a transaction failure can still report a subsequently confirmed outcome. Independent E/C types preserve both phases without implicit unification. An implementation retaining a phase error across mutating cleanup/destruction must establish scoped noBorrow or valid disjoint/externally rooted dependencies; independent error parameters do not authorize dangling or conflicting borrows.

Commit/rollback consume the guard even on failure. Success confirms the corresponding outcome; failure separately reports ConfirmedCommitted/ConfirmedRolledBack/Unknown and preserves finalization plus cleanup errors where both occur. A failed COMMIT reply is not proof of noncommit; late rollback/close cannot undo committed effects. Abandoned active transactions mark the session unusable and safely clean up rather than queue rollback and advertise immediate reuse. No promise of acknowledged remote rollback.

Managed guards own transaction control. Manual BEGIN/COMMIT/ROLLBACK/savepoint commands through ordinary execution are outside that managed contract. Std does not pretend to parse arbitrary SQL; drivers detect unexpected protocol state where possible and discard uncertainty, not manufacture a valid guard. Native escape operations have separate responsibilities.

Reuse disposition distinguishes CandidateForValidationReset from Discard. An operation can fail on a usable protocol session or have unknown effects on a discarded one. Candidate is not future liveness, a reset, or clean session variables/temp state. External pools must perform their documented validation/reset before reuse and must not treat discard as retry permission. No automatic replay of writes, reads-with-possible-effects, unique failures or unknown commits. Idempotency/reconciliation is an explicit application policy.

```rill
enum ReuseDisposition { CandidateForValidationReset, Discard }
```

reuseDisposition is a local observation with no implicit ping/SQL/time operation. reset is an optional controlled driver capability restoring its documented baseline, not a universal SQL recipe or future-liveness proof. Pools require suitable validation/reset support or discard. Failed/interrupted reset requires discard; a Discard session is not silently reconnected/replayed by reset. It stays available for safe close/disposal, not ordinary new work. Reset/metadata resolution receive explicit operation control as potentially blocking work.

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

## 8. Fixed operation shapes and generic evidence

These are public fixed-arity family declarations, not repeated per-driver declarations or native ABI layouts. They replace the Q1–Q57 deadline-independent sketches. All SQL types/helpers are public std::sql exports; the following canonical identities originate in std::sql::driver and are re-exported unchanged. Q is an owned raw string or a borrowed concrete prepared descriptor; input implementations determine outputs, never expected return/error types.

```rill
function connect<C,S,E>(connector: C, control: SqlControl): Result<S,DbError<E>>;
function arguments<X,A>(target: &X): A;
function bind<A,V,E>(arguments: &mut A, value: V): Result<void,DbError<E>>;
function prepare<X,P,E>(target: &mut X, sql: string, control: SqlControl): Result<P,DbError<E>>;
function execute<X,Q,A,E>(target: &mut X, query: Q, arguments: A, control: SqlControl)
  : Result<ExecSummary,DbError<E>>;
function query<X,Q,A,R,E>(target: &mut X, query: Q, arguments: A, control: SqlControl)
  : Result<R,DbError<E>>;
function begin<S,T,E>(session: &mut S, options: TransactionOptions, control: SqlControl)
  : Result<T,DbError<E>>;
function commit<T,E,C>(transaction: T, control: SqlControl): Result<void,TransactionFailure<E,C>>;
function rollback<T,E,C>(transaction: T, control: SqlControl): Result<void,TransactionFailure<E,C>>;
function cellAt<R,C,E>(row: &R, column: ColumnRef): Result<&C,DbError<E>>;
function resolveUnique<M,E>(resolver: &mut M, identity: &KeyIdentity, control: SqlControl)
  : Result<KeyDetails,DbError<E>>;
function reuseDisposition<S>(session: &S): ReuseDisposition;
function reset<S,E>(session: &mut S, control: SqlControl): Result<void,DbError<E>>;
```

Canonical iterate/next retain std::iter identity; canonical consuming close retains std::io identity, re-exported as appropriate, not a new SQL family. Session/cursor implementations return Result<void,DbError<E>> through close's existing single error parameter. No close control argument or generic withCancellation is added. Transaction finalization uses commit/rollback, not an implied generic close-as-rollback.

Input patterns determine S/A/P/R/C/E; expected result/error types never choose another implementation. No associated projection or runtime registry. Public generic requirements name exported canonical families. Evidence includes connect(C,S,E), arguments(X,A), bind(A,V,E), prepare(X,P,E), execute(X,Q,A,E), query(X,Q,A,R,E), begin(S,T,E), commit(T,E,C), rollback(T,E,C), cellAt(R,C,E), resolveUnique(M,E), reuseDisposition(S), reset(S,E), and canonical next(R,Result<Row,DbError<E>>). SqlControl is a concrete parameter, not another generic argument in these requirements. State noBorrow(A/P/Row) explicitly where required; do not add noBorrow(Session) or sendable/shareable by analogy. Standard close's error parameter, when reused, is the **whole DbError<E>**, not E alone.

For conversion-aware helpers, state into(Parameter,T) or tryInto(Parameter,T,C), plus bind(A,Parameter,E). For get<T>, state cellAt(R,C,E) and tryInto(T,&C,D); its error is GetError<E,D>. T chooses an already-approved canonical conversion, not ordinary return-only get dispatch. User errors can differ from native E and from other operations; retain separate phase parameters rather than assume equality. A helper owning local cursor/row storage must explicitly require borrow-free relevant errors/decoded outputs before destroying that storage. Concrete owned native diagnostics often satisfy this, not a general implicit rule for every E.

### Helper shapes and phase-preserving failures

bindInto<A,T,E>(&mut A,T) returns Result<void,DbError<E>> with into(Parameter,T), bind(A,Parameter,E), noBorrow(A). tryBind<A,T,C,E>(&mut A,T) returns Result<void,BindError<C,E>> with tryInto(Parameter,T,C), bind(A,Parameter,E), noBorrow(A). get<T,R,C,E,D>(&R,ColumnRef) returns Result<T,GetError<E,D>> with cellAt(R,C,E), tryInto(T,&C,D). They are generic helper bodies, not more return-only implementation sets; no extra display(E/D) is needed merely for redacted wrapper rendering.

```rill
enum QueryFailure<Q,N,C> {
  Start(Err<DbError<Q>>),
  Iteration { error: Err<DbError<N>>, closing: Option<Err<DbError<C>>> },
  Cardinality { error: CardinalityError, closing: Option<Err<DbError<C>>> },
  Close(Err<DbError<C>>),
}
```

queryOne<X,Q,A,R,V,QE,NE,CE>(&mut X,Q,A,SqlControl) returns Result<V,QueryFailure<QE,NE,CE>>; queryOptional has the same inputs and returns Result<Option<V>,QueryFailure<QE,NE,CE>>. Both require query(X,Q,A,R,QE), next(R,Result<V,DbError<NE>>), close(R,DbError<CE>), noBorrow(A), noBorrow(V), noBorrow(NE). The last requirement permits retaining an iteration error while closing/destroying the local cursor. A start error hands no cursor to the helper (query itself finishes required release); a consuming close cannot return references into its destroyed owned cursor fields. QE/CE may still retain valid caller-target/descriptor dependencies, so no universal noBorrow(QE/CE) is added. Any implementation introducing additional local borrowed temporaries must establish the corresponding extra evidence, not assume it at monomorphization.

After every started cursor, finalize it even on iteration/cardinality failure and retain the close error alongside the primary failure. A successful row/no-row result is not returned if close fails. Second-row detection is MoreThanOneRow, not silently taking the first. Intrinsic cancellation runs required cleanup instead of returning these errors. Get decodes from a caller-owned row, so T/D can retain valid row dependencies; an additional helper decoding from a local row must prove noBorrow of escaping output/errors before destroying it. These are design/checking obligations for #28, not an implemented checker proof.

### Admission, races and cleanup

Validate basic client inputs and domain pairing first using validateInstantDomain without reading the clock. Then observe an already-requested token; otherwise read/check an optional deadline before submitting new work. With no deadline, do not consult a clock. Invalid-domain failure wins over an already-requested token; within other simultaneously invalid basic arguments no additional broad priority is promised. After admission there is no guaranteed bias or physical completion-timestamp ordering: preserve the selected outcome, do not reclassify success from a later request/return-time check. On each active row advance apply this policy to the retained query control; execution observation remains relative to the whole query. Terminal streams do not restart preflight.

Explicit token cancellation may return a concrete SQL Cancelled outcome after required native release. Intrinsic goroutine cancellation remains outside ordinary Result and does not request a token flag. Deadline expiry/native cancel requests prove neither abortion nor rollback/reuse. Mandatory unregister/discard/release and native joins ignore external control (use neverCanceledToken/non-expiring internal cleanup where needed), with intrinsic delivery deferred as required. Canonical close is uncontrolled finalization; it may safely discard rather than promise graceful reuse. A cancelled commit/rollback still consumes the guard, finishes cleanup and reports only positively known transaction outcome; NotSubmitted does not mean prior work rolled back. Return can wait indefinitely.

withDeadline around an operation preserves R exactly: for SQL Result it returns nested Ok(Result), not flattening. Its intrinsic child cancellation and typed SQL token requests are different mechanisms. A returned live cursor needs its own explicitly supplied query control to budget later advancement; merely wrapping query creation does not attach the outer deadline to that cursor.

## 9. JSON integration and native-provider follow-up

JSON's standard representation remains std::encoding::json::Value, with owned trees/precision-preserving Number and separate parse/stringify errors. Users already may implement explicit into/tryInto to/from Value. SQL adds a deliberately named conversion-aware binding operation; this is a narrow library integration beyond #41's baseline, not automatic conversion registration. Parsing/stringifying do not discover records, invoke arbitrary conversions or gain a mandatory show(Value). Owned/borrowed conversion sources remain distinct. JSON's exact integer/float construction/extraction policies do not become SQL's semantics or vice versa.

The example domain module implements SQL Parameter and JSON Value conversions and a generic checked cell decoder without a Serializable trait or per-driver imports. A whole record/table mapper is still external and explicit. #31/#41 need examples/cross-links, not re-opening their parser or numeric decisions. Do not edit other worktrees or assume same-number ADRs identify the same draft.

A PostgreSQL-like adapter owns TCP/TLS protocol state and errors; a SQLite-like adapter needs a separately specified narrow native-provider contract for native connections/statements/bindings/reset/finalization. Ordinary families express the seam but do not implement C ABI hooks. No raw pointers, public thread pinning or general FFI is authorized. Provider operations/disposal must be valid under worker migration and retain source/native state until completion or confirmed abortion. Blocking-pool dispatch/abort request is not release; Wasm pointer stability remains #30 work.

[Native-provider follow-up #43](https://github.com/Reilley64/rill/issues/43) now tracks that contract and the validation needed before claiming embedded support. The user explicitly approved separating it from shared-interface design closure/RFC drafting; it has not been fulfilled. A PostgreSQL/SQLite sketch remains a design target, not delivered support. No native ABI, general FFI, public pinning or provider/target guarantee is inferred from closing #40.

#33 is closed and its confirmed types/semantics are integrated above, with the narrow domain-validation amendment recorded back to #33. Original blanket 'cancellation outside Result' SQL wording is superseded: it applies to intrinsic stopping, not explicitly token-aware SQL outcomes. No competing clock/epoch/calendar model or generic cancellation wrapper is introduced.

## 10. Design completion versus implementation gates

#40 can close as the explicitly confirmed shared-interface design. Its #23/#32/#33 inputs are closed; no native dependency is silently removed. #43 tracks the separately approved embedded-provider prerequisite and does not block shared-interface closure or RFC drafting. Actual driver/provider delivery, protocol/security tests and supported target claims remain outstanding.

#28 formalizes pattern specificity, input-determined outputs, exact helper/error/evidence transport, transitive dependencies, limits/redaction, outcome races and exactly-once cleanup. #30 retains actual native/Wasm pointer/scheduling/reclamation gates. The public family/control/error contracts are not a checker proof. Any unworkable mechanism returns to map #1, not a hidden restriction or fabricated native guarantee.

Reconcile separately prepared JSON/time/random artifacts and historical number collisions without overwriting them. #33's validateInstantDomain follow-up must be merged into its clock contract; the authoritative issue handoff records it even when that separate file is absent here. Research/prototypes/ADRs are inputs, not accepted feature RFCs or implementation evidence.
