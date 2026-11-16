# #40: Standard SQL execution and column-naming unique errors

Research input for [Rill #40](https://github.com/Reilley64/rill/issues/40), **not a placement/API decision, implementation, protocol validation, or soundness/security proof**. This note follows the numbered issue-note convention in `docs/research/`.

## Interview follow-up

The user subsequently confirmed the [integrated SQL contract](../sql-contracts.md): std placement, external concrete pools, typed uniqueness with known/unavailable provenance, explicit codecs and redacted defaults. After #33 closed, Q58-Q77 confirmed owned controls, typed token outcomes distinct from intrinsic stopping, temporal mappings, fixed arities and the narrow validateInstantDomain consumer amendment. #43 separately tracks native-provider work; it is not fulfilled by design closure. The options/checklist and historical open-#33 statements below are research inputs at that time, not unresolved votes overriding later decisions or implementation evidence.

## Premises and short answer

The current interview direction supersedes the issue body's open placement question: **the database library belongs in std**, in the spirit of Go `database/sql`. An external shared database interface was rejected. Shared execution mechanics do not imply universally portable SQL. Concrete drivers are selected statically through ordinary canonical families; no new traits, associated projections, public `dyn`, runtime driver registry, broad FFI, or public OS-thread-affine resources are assumed. ORM/entity mapping, query ASTs/dialect compilation, migrations and drivers remain external. Concrete pools were previously external; the Go comparison raises an explicit decision about that boundary rather than silently changing it.

**Most important result:** a guaranteed, accurate, nonempty list of ordinary column names for *every* primary-key/unique failure across these backends is **not available from their current error contracts**. PostgreSQL normally transmits an object identity, not a composite column list. SQLite transmits a structured extended code but no structured failing index/column identity through its public error API. MySQL and SQL Server put relevant object names inside message text, not separate protocol fields. Expression keys may not have ordinary column-name equivalents at all. This remains true even if the application wants only UUID duplicates: the UUID's SQL type does not add error fields. [P1-P4, S1-S3, M1-M3, Q1-Q3]

A useful shared unique-violation classification **can coexist with the complete driver-defined typed error**. Column information needs an explicit known/unknown or capability-restricted contract; it must not be inferred by splitting arbitrary message strings. Decisions remain the user's.

## Local design inputs and unresolved gate

Read alongside these local decisions:

- [Catalogue](../standard-library-contracts.md), especially canonical family identity/requirements, result-related type arguments, pre-monomorphization evidence, typed `Err<E>`, explicit adaptation, and the stronger requirements for retained erased causes.
- [ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md): cooperative cancellation, joined cleanup, no public OS-thread pinning, and no hard return bound from racing a timer.
- [ADR 0013](../adr/0013-caused-by-errors-with-explicit-wrapping.md), as refined by the catalogue: preserve typed errors and origin/cause metadata; retained erasure is not an arbitrary public downcast channel.
- [ADR 0017](../adr/0017-user-implementable-conversions.md): `function` declarations, `implement` bodies, ordinary input-based family selection; conversions do not create automatic database mapping or expression capture.
- [ADR 0018](../adr/0018-streaming-and-buffered-io.md): explicit progress and owning adapters; cancellation/close/disposal do not prove remote rollback or connection reuse. Its scoped `noBorrow` restrictions are not universal driver-error restrictions.

At the **initial research checkpoint**, [#33](https://github.com/Reilley64/rill/issues/33) was still OPEN when inspected through `gh`, including its comments; the later integrated contract above supersedes this historical dependency state. Deadline representation, propagation, races, native-work abortion and wall-clock representation remain unresolved prerequisites. **This note chooses no timeout parameter, context/token type, clock epoch, duration unit or deadline API.** The #32 handoff comments on #40/#33 expressly leave that work open and require retaining native/buffer obligations through completion or confirmed abortion. #30's backend/native-pointer gates remain; a SQLite-like adapter is a contrasting design target, not permission to expose SQLite C handles today.

## Why these comparisons

Selection is based on distinct, documented architectural mechanisms, **not an unsupported ranking of "best" or "most loved" libraries**:

| Library and inspected baseline | Why it is relevant to this question | What not to import automatically |
| --- | --- | --- |
| Go `database/sql` + `database/sql/driver`, Go **1.24.0** | Actual language-standard SQL boundary, external drivers, std-owned pool, separation of public facade from driver contracts. [G1–G2] | Interfaces, `any`/reflection, global name registration, dynamic error assertions. |
| pgx **v5.7.2** | Contrasts a database-specific native API with an adapter to `database/sql`; exposes capabilities hidden by a common minimum. Separate `pgxpool`. [X1–X4] | Runtime codec registration, Go interfaces, native API's context semantics. |
| sqlc **v1.28.0** | Generates typed application calls from explicit SQL/schema, targeting existing execution APIs; demonstrates leverage without std ORM/query capture. [C1–C2] | Its generated `DBTX` interface or assumptions that generation removes runtime failures. |
| Rust SQLx **v0.8.3** | Async prepared/bound execution, optional SQL-checking macros, concrete backends, external decimal/calendar integrations, normalized errors plus backend access. [R1–R4] | Rust traits/associated types, public boxed dynamic errors, or a requirement for build-time database access. |
| Diesel **v2.2.8** / official **2.2.x** API docs | Typed SQL DSL/schema mapping and transaction management; demonstrates both useful checking and the breadth of an ORM/backend extension surface. [D1–D3] | DSL/ORM in std, Rust's trait machinery, optional-field diagnostics as guaranteed fields. |
| jOOQ official **3.20** manual | Generated schema-aware DSL with dialect machinery above execution; JDBC binding and exception preservation; illustrates a deliberately much larger external layer. [J1–J4] | Java exceptions/reflection/JDBC interfaces, all dialects/type mappings in std. |

Dapper is omitted: handwritten-SQL mapping would add less contrast here than sqlc's generated boundary. These selections are comparison cases, not evidence of relative popularity or suitability for all workloads.

## Portable mechanics versus backend-specific evidence

| Concern | Reusable leverage / possible standard responsibility | Backend-specific boundary or failure case |
| --- | --- | --- |
| Session/connection ownership | Explicit acquisition, exclusive use, cleanup, and a stable target for transactions and statements. | A pool handle is not a physical session. Go `DB` is concurrent and pooled; driver `Conn` is stateful and not concurrently used. pgx `Conn` is not concurrency-safe. [G1–G2, X1] |
| Streaming rows | Advance, inspect/decode, distinguish exhaustion from failure, explicitly close/abandon. | Borrowed row views may be invalidated on advance/close. Go documents `RawBytes` lifetime and a separate owning byte copy. pgx must close rows before reusing the connection and supports owning collectors. Closing may drain or discard a connection, not undo execution. [G1, X1, X3] |
| Parameters / prepared execution | Bind data independently of SQL syntax; explicit statement lifecycle and checked encoders. | Placeholder spelling, type inference, prepare/cache strategy, batch/multiple results and proxy compatibility differ. Go driver statements are connection-bound; facade statements can prepare on multiple underlying connections. pgx caches prepared statements by default but supports other execution modes. [G1–G2, X1] |
| Typed SQL null | Distinguish absent SQL value from empty text/zero; retain the intended SQL type where encoding needs it. | Go uses `Null[T]`/nullable wrappers, pgx uses nullable pgtype values, SQLx supports `Option<T>`, Diesel has `Nullable<ST>`, JDBC supports typed `setNull`. A bare untyped null does not always determine a PostgreSQL parameter type. [G1, X4, R2, D3, J4] |
| Exact data | Checked extraction/encoding; no mandatory conversion through binary float or a narrowing integer. | Go's basic driver values do not constitute a universal decimal/calendar model. pgx has extensible PostgreSQL codecs. SQLx documents `BigDecimal` and `rust_decimal` with **different ranges**, and separate timestamp/date/time mappings. [G2, X4, R3] |
| Transactions | Scope/session affinity, terminal commit/rollback, failure preservation, explicit savepoint capability. | Go BeginTx cancellation arranges rollback; pgx's Begin context affects only BEGIN, not automatic rollback. Diesel describes rollback/commit failures and broken transaction managers. SQLx drop starts/queues rollback, not a universal synchronous rollback acknowledgement. [G1, X2, D2, R4] |
| Generic checking | Generic helpers declare every execution/row/codec/error requirement before monomorphization. | sqlc generation, SQLx SQL checking, Diesel's typed DSL and jOOQ schema generation check different things. Neither schema drift nor all backend/runtime errors disappear. [C1–C2, R1–R2, D1–D3, J1] |
| Escape hatches | Keep concrete driver types available so external code can invoke extra ordinary families directly. | pgx COPY, LISTEN/NOTIFY, low-level PgConn and specialized types are examples; no common-driver API should pretend SQLite has these. Go's `Conn.Raw` shows the need, but its `any` callback is not a Rill design. [G1, X1] |
| Errors | Shared categories can aid control flow while retaining native typed payload, SQLSTATE/native code and diagnostic metadata. | SQLx/Diesel explicitly make table/constraint/column information backend-dependent or optional. Their error representation/erasure is not Rill's. [R1, D1, X3] |
| Pooling | Lease lifetime, unusable-connection disposition and reset/validation obligations need stable semantics whether pools are std or external. | Go std owns the concrete pool and retry machinery; pgxpool and SQLx pools are library-owned. Pool-size/idle policy, session reset and statement caches are operational decisions, not SQL portability. [G1–G2, X1, R1] |

### What is especially worth borrowing

**Go:** separate a small driver-facing mechanics contract from ergonomic consumers; explicit rows/statement/transaction cleanup; distinguish per-session resources from a concurrent handle. `OpenDB(driver.Connector)` already shows that even Go's facade need not be entered through the string registry, although the connector is still a Go interface. No database driver ships inside Go std. `ErrBadConn` has an unusually important contract: drivers **must not** return it if the operation might have executed, because std can retry. Its separate `Validator` hook permits discarding a connection while returning the real query error. Rill should similarly avoid making "discard connection" mean "safe to repeat operation." [G1-G2]

**pgx:** a standard minimum should not suppress typed native capabilities. The native API and `database/sql` adapter coexist. Preserve driver codecs, error diagnostics, COPY and protocol hooks outside std's minimum; do not use a single closed value enum as a ceiling on every SQL type. Its documented default cancellation behavior often closes the connection; customizable cancellation demonstrates that this is a concrete driver policy, not proof that cancellation is rollback. [X1-X4]

**sqlc / SQLx / Diesel / jOOQ:** treat SQL/schema checking as an external layer with several legitimate strategies. sqlc generates from SQL and binds generated `Queries` to transactions; SQLx's `query!` needs a schema database at build time or cached offline query data, and nullability inference has documented backend caveats/overrides. Diesel encodes SQL/schema relationships in types. jOOQ code generation gives a typed schema/dialect DSL, but also permits other usage. These are not ordinary closure inspection, and none is an argument for adding expression-tree reflection to Rill. [C1-C2, R1-R2, D1-D3, J1]

jOOQ's `DataAccessException` wraps underlying `SQLException`; preserving a native cause is valuable even where its unchecked-exception control flow is not. Diesel's explicit optional backend fields and SQLx's normalized `UniqueViolation` are useful precedents for **classification without fabricated detail**, not precedents for guaranteeing a column list. [J2, D1, R1]

### Decimal/calendar/null decisions still required

Options for std's minimum are (a) own small, precisely specified exact wire/value representations, or (b) standardize encoding/decoding families while external exact-decimal/calendar types supply implementations. An open codec seam is needed either way. Do not silently choose a fixed decimal width just because one backend mapping supports it: SQLx explicitly documents that `rust_decimal::Decimal` cannot represent all PostgreSQL NUMERIC values, while `BigDecimal` can exceed PostgreSQL's range. Range errors must remain errors. Infinity/NaN support also varies by representation. [R3]

Distinguish date, local date-time, time-of-day, offset-bearing value and instant; PostgreSQL TIMESTAMP and TIMESTAMPTZ are not interchangeable mappings. A wall-clock timestamp is not a monotonic deadline. UUID can remain a driver/external codec type unless separately chosen for std. Typed null should retain codec/type evidence; "None" is a value state, not necessarily enough SQL type information by itself. Calendar/decimal placement and actual shared representations require explicit user decisions coordinated with #33. [X4, R2-R3, J4]

## Unique violations: backend evidence

"Duplicate unique identifier" has two possible scopes:

1. **All primary-key / UNIQUE violations**, including non-UUID identifiers and composite keys. A composite `(tenant_id, external_id)` collision identifies a key tuple: it does **not** mean either column independently violates uniqueness.
2. **Only an application's UUID identifier column.** This can permit a narrow schema-aware promise if the operation is constrained to one identified key, but the backend does not label an error "UUID duplicate." Another unique key or a trigger's insertion can fail first. A chosen UUID convention alone cannot establish which object failed.

The following evidence supports both interpretations, without deciding which the user intends.

| Backend baseline | Structured native classification | Column names explicitly transmitted in that failure? | Metadata resolution | Message parsing / unavailable information |
| --- | --- | --- | --- | --- |
| PostgreSQL **17**, source **REL_17_0** | SQLSTATE **23505** `unique_violation`; ErrorResponse has optional schema `s`, table `t`, singular column `c`, constraint `n`. [P1–P3] | **Not generally.** The ordinary btree duplicate path reports `errtableconstraint(...)` and optional textual key detail, not a repeated column-name field. Constraint/index name is structured, a composite list is not. [P2] | For normal engine errors with supplied identity, resolve schema + relation + index name. `pg_index.indkey` and `pg_attribute` yield ordered simple key columns; respect `indnkeyatts`, excluding INCLUDE columns. `pg_constraint.conkey` works for actual key constraints, but standalone unique indexes require `pg_index`. [P4–P5] | DETAIL can contain key names/expressions **and duplicate values**; not a structured list. Expressions have zero `indkey` entries; partial predicates have separate metadata. User-raised 23505 need not identify an object. [P2, P4, P6] |
| SQLite public C API, source **3.49.1** | Base **19** `SQLITE_CONSTRAINT`; extended **1555 PRIMARYKEY**, **2067 UNIQUE**, **2579 ROWID**, **1811 TRIGGER**. [S1] | **No structured failing table/index/column list** in the error-code/message APIs. `sqlite3_error_offset` is an SQL-text offset facility, not a unique-key identity. [S2] | `PRAGMA index_list`, `index_xinfo`, `table_xinfo` can describe a **known** index/table and key order. They do not by themselves identify which of several unique indexes caused an error. `table_column_metadata` describes an already-named column, not the failed key. [S3–S4] | Engine source constructs ordinary key names into **message text**; expression-index errors use the index name in text instead. No public structured identity ties the error to those metadata rows. [S5] |
| MySQL **8.4.0 / 8.4 docs** | **1062**, `ER_DUP_ENTRY`, SQLSTATE **23000**. Protocol ERR packet: numeric error code, optional five-character SQLSTATE, message. [M1–M3] | **No.** Neither column list nor structured index name is a separate field in this ERR packet. | With independently trustworthy table/index identity, `INFORMATION_SCHEMA.STATISTICS` supplies `SEQ_IN_INDEX`, `COLUMN_NAME`, `EXPRESSION`, uniqueness and index name (`PRIMARY` for PK). [M4] | Key name is in localized message text; pinned server messages bound the printed duplicate value/key lengths. SQLSTATE 23000 alone is broader than uniqueness. Functional/prefix indexes add semantics a plain name list omits. [M2–M5] |
| SQL Server official **ver17** docs; TDS ERROR token | **2627** for key-constraint violation, **2601** for duplicate unique-index key; structured number/state/class/message/server/procedure/line. [Q1–Q3] | **No dedicated failing constraint/index/column fields** in ERROR. | Given trusted object/index identity, `sys.index_columns` yields column IDs and key ordinals; `sys.columns` supplies names. Exclude included/non-key entries; filtered indexes/computed columns need additional semantics. [Q4] | Constraint/index/object identities in these diagnostic descriptions are text substitutions. A numerical category is more reliable than a parser for localized messages. [Q1–Q3] |

"Not transmitted" is not the same as "impossible to learn with additional schema knowledge." Conversely, the ability to enumerate metadata is not evidence that the error identifies the right metadata row.

### PostgreSQL: strongest identity, still no unconditional column-list guarantee

The protocol expressly says not to assume object diagnostic fields are all present together. A singular `c` field is **not** an array of every column participating in an arbitrary unique key. PostgreSQL's normal btree failure source is decisive evidence: SQLSTATE 23505, constraint/index identity via `errtableconstraint`, and a separately constructed human-readable DETAIL. PostgreSQL treats indexes as constraints for the diagnostic `n` field even when created without constraint syntax. [P1-P3]

A practical resolver for an ordinary, simple key can return all ordered key columns using catalogue identity, without parsing DETAIL. It must:

- Use database/session context plus **schema, table and index identity**, not constraint name alone or the caller's `search_path`. `pg_constraint.conname` is explicitly not necessarily unique. Preserve server identifier strings and quote SQL identifiers correctly; bind names as values in catalogue queries instead of interpolating them. [P1, P4-P5]
- Include primary keys and standalone unique indexes. Handle composite key order; exclude INCLUDE attributes. For an expression such as `lower(email)`, do not falsely report `email` as if that column's raw value were uniquely constrained. There may be several source columns or no source column at all. A partial index's predicate changes the domain over which uniqueness applies. [P4]
- Respect transaction state: after an error the PostgreSQL transaction can be aborted, so a normal catalogue query on that connection fails until rollback/rollback-to-savepoint. A deferred constraint can fail at COMMIT rather than at the originating statement. Do not erase the original error to make enrichment succeed. [P7, P8]
- Treat caches as enrichment, not eternal truth. Names can be renamed, dropped and reused; a second connection can see a different schema/snapshot or lack privileges. Even without a race, absent or user-supplied diagnostic fields may not resolve. For a guarantee, an explicit schema/version/DDL coordination contract would be necessary. [P1, P4-P6; inference from those contracts]

A second connection avoids the failed-transaction restriction but does not recreate the failure's snapshot or object identity. Querying before execution avoids the failed-transaction query but adds latency and invalidation obligations. Error inspection should not quietly issue new blocking SQL merely because it looks like a cheap accessor.

### SQLite: structured code, not structured failing columns

`sqlite3_extended_errcode` gives real native information beyond `errmsg`: classification of unique vs primary-key vs rowid vs trigger constraints. **It does not give the failing index's identity.** The public error APIs supply codes/messages/SQL-text error offset, while schema APIs answer questions about objects already identified. SQLx's pinned SQLite implementation captures precisely native extended code and copied message; it classifies PRIMARYKEY/UNIQUE as `UniqueViolation` but supplies no invented constraint list. Its treatment of ROWID is a separate policy gap to consider if Rill means all identity collisions. [S1-S4, R1]

In SQLite 3.49.1 `sqlite3UniqueConstraint`, ordinary key column names are concatenated as `table.column, ...` with no general machine-readable escaping scheme; expression-index errors instead format `index 'name'`. A quoted identifier can legally contain punctuation that resembles that delimiter. Splitting the string on commas or periods therefore cannot guarantee correct identifiers even for simple columns. Rowid errors follow a separate path. [S5]

Custom instrumentation, a patched engine or application schema/statement restrictions could supply additional identity; they are **additional capabilities**, not the existing C error API. Inferring the failed key by searching for duplicate values can be wrong under triggers, several violated keys, collations, expression predicates and concurrent effects. It also adds reads of potentially sensitive data.

### MySQL and SQL Server: category is structured, object identity usually is not

MySQL's 1062 is narrower than its SQLSTATE 23000. The official ERR payload documents only code/state/message, confirmed by the pinned Go MySQL driver's packet decoder and native `MySQLError` fields. Its typed error is valuable, but not a hidden structured column array. Server message source has localized templates and bounded key-name formatting: parsing is a best-effort, version/locale-sensitive adapter feature, not a portable promise. Schema metadata can resolve an index only once the correct database/table/index is known. [M1-M4, M6]

SQL Server's TDS token similarly gives number/state/class and textual diagnostics, not a structured constraint field. The documented 2627/2601 descriptions distinguish constraints and unique indexes, but both need care. `sys.index_columns` is useful metadata, not an error-to-object mapping service. Index key columns can include computed columns; a computed column name is still not equivalent to a list of underlying source columns. [Q1-Q4]

### Nulls, triggers, identifiers and privacy

- **Nullable uniqueness is not portable SQL semantics.** PostgreSQL defaults to nulls-distinct and supports `NULLS NOT DISTINCT`; a primary key implies not-null. SQLite normally treats nulls as distinct for UNIQUE and documents legacy nullable PRIMARY KEY exceptions except specified table/key forms. MySQL unique indexes permit multiple null values. A unique classifier reports an actual backend rejection; it must not promise that every null-containing duplicate is rejected. [P8, S6, M5]
- **Expression/partial uniqueness is not a raw column constraint.** Choose whether "columns" means direct key columns, source dependencies, user-facing fields, or a richer key-parts description. These are different data models. Partial predicates and collations must not be silently discarded when claiming semantic equivalence. [P4, S3, S7, M4-M5]
- **Triggers can reject another table's key.** Do not attach the INSERT target's columns to every duplicate error. PostgreSQL `RAISE` can choose an SQLSTATE and optional diagnostic fields; SQLite `RAISE` produces trigger constraints and arbitrary diagnostic messages. A custom trigger message containing "unique" is not sufficient classification evidence. [P6, S8]
- **Preserve identifier identity, not a dotted display string.** Store database/schema/table/constraint/column components separately where known. Quoted identifiers, identical constraint names in different relations/schemas and attached SQLite databases defeat unqualified-name shortcuts. [P1, P5, S3, S5]
- **Do not log duplicate VALUES by default.** PostgreSQL DETAIL and MySQL's duplicate-entry message can contain actual data. Preserve native diagnostics for deliberate inspection, but offer a category/object-only safe display or documented redaction policy. Error wrapping that logs the entire retained native cause can reintroduce the leak. A unique error need not retain parameter values to name a key. This is a design recommendation, not a claim that all such logs can be automatically sanitized. [P2, M3]

## Honest error design options

These are semantic alternatives, **not proposed final Rill declarations**:

1. **Structured known/unknown enrichment (broadest backend coverage).** Keep the driver's `E` and `Err<E>` intact; separately classify unique failures. Represent key identity and columns with explicit availability. Distinguish a complete ordered list of direct key columns, expression/mixed key parts, and unavailable details. Unknown must not masquerade as an empty complete key. Optionally expose provenance: server field, metadata resolution, application-declared schema, or heuristic text parse. Prefer no heuristic parsing in std's guaranteed path.
2. **Mandatory precise-column capability (strongest narrower promise).** Only admit driver/operation/schema combinations that can prove the required information. State restrictions upfront: perhaps direct-column keys, known schema versions, controlled DDL and instrumentation. Stock generic SQLite/MySQL error APIs alone cannot meet the universal version. Reject unsupported capability at the boundary rather than fabricate names. A requirement on "driver supports SQL" is insufficient evidence for this stronger capability.
3. **Standard-owned metadata/enrichment contract (larger std responsibility).** Keep drivers/pools external but standardize qualified object IDs, ordered key parts, schema snapshots/version identity, resolver failure, cache invalidation and observation/provenance rules. This enables consistent enrichment, but does not magically reconstruct an error's missing key identity; restrictions/unknowns remain necessary. Decide whether resolution is an explicit effectful operation rather than hidden error-accessor I/O.
4. **Application identifier contract (UUID-only interpretation).** Let application/generated schema metadata identify a designated key. Return its column names only when operation constraints or trustworthy backend identity establish that *that key* failed. A duplicate on a different unique key must remain differently identified/unknown. "Every table has id UUID" is not enough.

**Research recommendation:** option 1 is the most honest default for a broad Go-like execution layer; offer stronger guarantees only under an explicit capability/schema contract (option 2 or 4). If the user requires an unconditional nonempty column list for every supported violation, present the incompatibility and negotiate narrower supported operations/backends or new provider instrumentation. Do not silently weaken "guaranteed" to best-effort. Option 3 is a significant scope addition requiring separate approval.

Classification must not replace native E with a universal erased database error. In Rill, a driver-defined concrete error can have ordinary classification/access families; a generic consumer declares that evidence explicitly. A standard wrapper, if chosen, can carry the **typed** original `Err<E>` plus classification/enrichment, or inspection can leave the original envelope untouched. Neither needs runtime downcasting. Exact signatures, borrowing/ownership and whether wrapper construction creates a new origin remain design work under the catalogue. Do not reconstruct errors merely to discard origin/cause data. Erasing a native error as Cause additionally requires the catalogue's display/noBorrow/sendable/shareable evidence; primary typed errors do not automatically require all those properties.

## Connection, transaction and cancellation implications for Rill

### Two contrasting adapter sketches (no invented API)

**PostgreSQL-like network driver:** a concrete session owns a transport and protocol state; query execution binds values, creates a concrete streaming row state and prevents conflicting session use while results are active. Row decoding exposes either owned values or checked row-buffer borrows. Prepared statements are scoped/cached against the actual session. A unique failure retains a concrete network-driver error containing SQLSTATE and native optional fields. Catalogue enrichment is explicit and can be unavailable in an aborted transaction. COPY/notifications remain extra driver capabilities. [G1-G2, X1-X4, P1-P8]

**SQLite-like embedded driver:** an opaque concrete adapter owns native connection/statement obligations behind a separately approved native-provider boundary; it cannot expose raw C pointers or demand public OS-thread pinning. Bindings and row views retain native storage until completion/reset/finalization under checked ownership. Copy native error diagnostics before another operation overwrites connection error state. Extended codes classify failures, but there is no native structured column-list guarantee. A worker/native implementation may be necessary, but broad FFI and safe cancellation of blocking work are not established by this note. [S1-S5; local ADRs 0009/0018]

An external generator/builder can produce backend SQL, typed binders and decoders for either target and use ordinary standard execution families. The generic helper must name the session, parameter, row/statement and error type relationships through available ordinary requirements, checked **before monomorphization**. Result-related type parameters may bind the selected implementation's outputs; they must not select an alternative driver by desired return/error type. These are requirements to validate in #28, not a claim that associated projections or reflection are already available.

### Required behavior questions, independent of final spelling

- Can rows/transactions/statement handles escape a local session? If borrowed, dependencies must prevent destroying or reusing the owner. If owning a lease, the lease stays unavailable until all dependent work is finished. Decide own-row vs borrow-row defaults and explicit copying.
- After partial row consumption or decode failure, who drains/cancels/finalizes? What positive evidence permits returning the connection to a pool? A local close or disposal is not that evidence by itself.
- Does commit consume the transaction even on failure? Separate **confirmed rollback / confirmed commit / outcome unknown** from **session reusable / discard**. Loss of a COMMIT reply can leave the client unable to distinguish a committed transaction from a failed one. A later rollback attempt or socket close cannot undo an already committed transaction. This is a distributed-observation inference, not a portable server-provided "unknown" error code.
- Preserve the original statement/commit error if cleanup also fails; consider reporting both. Diesel's documented transaction failure handling illustrates why rollback failure and broken-state information matter. [D2]
- No automatic replay on generic transport errors or commit-unknown errors. Go's `ErrBadConn` prohibition is a useful precedent; retrying a transaction needs a separately authorized idempotency/reconciliation policy. [G2]
- PostgreSQL cancellation requests are race-prone: sending one does not prove the target query was canceled. Consume protocol results/reach a known state or discard; classify effects separately. pgx's native context behavior is not a Rill deadline design. [P7, X2-X3]
- Cooperative cancellation remains outside Rill's recoverable Result. Required cleanup may suspend indefinitely; retain all borrowed/native state until completion or confirmed abortion. Unknown remote effects do not turn cancellation into a recoverable timeout value. #33 decides any public deadline/race surface.

### Pooling: explicit user decision reopened by the Go analogy

**Keep concrete pools external (existing direction):** std owns execution/lease/reset/disposition semantics sufficient for reusable external pools; pool policy/configuration remains independent. This minimizes std concurrency/time-policy scope, but ecosystem pools need consistent connection health and cleanup contracts. A generic execution helper should accept the acquired execution target rather than require one particular pool.

**Put a Go-like concrete pool in std (possible revised direction):** closer match to `sql.DB`, one operational/default pooling contract and fewer repeated lease/reset implementations; larger std surface for queueing, limits, validation, session reset, prepared caches, fairness and shutdown. It depends directly on unresolved #33 and #30 machinery. "Go-like" cannot silently imply this choice, because Go `DB` really **does** own a pool. [G1-G2]

A third placement is standard pool/lease contracts with external implementations, but that still needs a concrete ownership/evidence story, not a new interface/trait system. No choice here reopens the already rejected external placement of the **shared database execution library**.

## Decision checklist for the parent interview

1. Does "duplicate identifier" mean all PK/unique violations (including composite, expression and partial keys), or one designated UUID key?
2. Is known/unknown acceptable, or must supported operations be restricted to guarantee a complete direct-column list? What should expression keys report?
3. Should metadata resolution be explicit external driver work, a standard enrichment operation, or omitted? Who owns schema/cache consistency and failed-resolution errors?
4. Should category inspection borrow E, or should execution return a typed standard wrapper preserving E? What redacted default display is expected?
5. Does the new Go-like request revise external concrete pooling, or retain it? #33 remains open either way.
6. Which exact/null/calendar value contracts are shared, which codecs remain external, and what native-provider prerequisite permits an embedded adapter?

## Primary sources and verification boundaries

Pinned source links below refer to **inspected baselines**, not claims that these are the latest releases. PostgreSQL pages are version 17; SQLx docs are 0.8.3; source tags identify Go/pgx/sqlc/SQLx/Diesel/SQLite/MySQL versions. Diesel 2.2.x, jOOQ 3.20, SQLite live C API pages, Microsoft ver17 and MySQL's `latest` protocol page are moving documentation; pinned implementations corroborate where available. MySQL's current protocol page identified itself as source documentation **26.10.0** when fetched; it is not misrepresented here as a pinned 8.4 page. The 8.4 code/messages and pinned driver corroborate the ERR fields used in this note.

### Libraries

- **G1:** [Go 1.24.0 `database/sql/sql.go`](https://github.com/golang/go/blob/go1.24.0/src/database/sql/sql.go): `DB`, `OpenDB`, `Conn`, `Stmt`, `Rows`, `RawBytes`, `Null[T]`, `BeginTx`, `Tx.Commit`, reset/retry/pool implementation.
- **G2:** [Go 1.24.0 driver contracts](https://github.com/golang/go/blob/go1.24.0/src/database/sql/driver/driver.go): `Value`, `Conn`, `Stmt`, context-aware optional contracts, `NamedValueChecker`, `ErrBadConn`, `SessionResetter`, `Validator`, optional column metadata.
- **X1:** [pgx v5.7.2 package documentation](https://github.com/jackc/pgx/blob/v5.7.2/doc.go): native/std adapters, pgxpool, binding/preparation, COPY, notifications and lower-level API.
- **X2:** [pgx v5.7.2 transactions](https://github.com/jackc/pgx/blob/v5.7.2/tx.go): begin-context limitation, savepoints, commit/rollback terminal behavior.
- **X3:** [pgx v5.7.2 rows](https://github.com/jackc/pgx/blob/v5.7.2/rows.go), [native errors](https://github.com/jackc/pgx/blob/v5.7.2/pgconn/errors.go) and [pgconn context/cancellation docs](https://github.com/jackc/pgx/blob/v5.7.2/pgconn/doc.go).
- **X4:** [pgtype v5.7.2 documentation](https://github.com/jackc/pgx/blob/v5.7.2/pgtype/doc.go): nullable values and codec extension/registration mechanisms.
- **C1:** [sqlc v1.28.0 README](https://github.com/sqlc-dev/sqlc/blob/v1.28.0/README.md) and [transaction examples/generated DBTX](https://github.com/sqlc-dev/sqlc/blob/v1.28.0/docs/howto/transactions.md).
- **C2:** [sqlc v1.28.0 datatype mappings](https://github.com/sqlc-dev/sqlc/blob/v1.28.0/docs/reference/datatypes.md): nullability and driver-dependent Go mappings.
- **R1:** [SQLx v0.8.3 README](https://github.com/launchbadge/sqlx/blob/v0.8.3/README.md); [error contracts](https://github.com/launchbadge/sqlx/blob/v0.8.3/sqlx-core/src/error.rs); [PostgreSQL error fields](https://github.com/launchbadge/sqlx/blob/v0.8.3/sqlx-postgres/src/error.rs); [SQLite native error capture/classification](https://github.com/launchbadge/sqlx/blob/v0.8.3/sqlx-sqlite/src/error.rs).
- **R2:** [SQLx 0.8.3 `query!`](https://docs.rs/sqlx/0.8.3/sqlx/macro.query.html): build-time requirements, bind types, nullability caveats/overrides and offline mode.
- **R3:** [SQLx 0.8.3 PostgreSQL datatype mappings](https://docs.rs/sqlx/0.8.3/sqlx/postgres/types/index.html): external decimal/calendar types, range mismatch and timestamp distinctions.
- **R4:** [SQLx v0.8.3 transaction implementation](https://github.com/launchbadge/sqlx/blob/v0.8.3/sqlx-core/src/transaction.rs): consuming commit/rollback and start-rollback on drop.
- **D1:** [Diesel v2.2.8 error definitions](https://github.com/diesel-rs/diesel/blob/v2.2.8/diesel/src/result.rs): `UniqueViolation`, `DatabaseErrorInformation`, optional table/column/constraint fields and error erasure.
- **D2:** [Diesel 2.2.x Connection contract](https://docs.diesel.rs/2.2.x/diesel/connection/trait.Connection.html): extension surface, prepared caching and transaction/rollback failure behavior.
- **D3:** [Diesel 2.2.x overview](https://docs.diesel.rs/2.2.x/diesel/index.html), [SQL types](https://docs.diesel.rs/2.2.x/diesel/sql_types/index.html), [Selectable](https://docs.diesel.rs/2.2.x/diesel/expression/trait.Selectable.html): typed DSL/schema/value mapping.
- **J1:** [jOOQ 3.20 code generation](https://www.jooq.org/doc/3.20/manual/code-generation/).
- **J2:** [jOOQ 3.20 exception handling](https://www.jooq.org/doc/3.20/manual/sql-execution/exception-handling/).
- **J3:** [jOOQ 3.20 bind values](https://www.jooq.org/doc/3.20/manual/sql-building/bind-values/), [transactions](https://www.jooq.org/doc/3.20/manual/sql-execution/transaction-management/), [lazy fetching / cursor cleanup](https://www.jooq.org/doc/3.20/manual/sql-execution/fetching/lazy-fetching/).
- **J4:** [jOOQ 3.20 data types](https://www.jooq.org/doc/3.20/manual/sql-building/data-types/) and [Java 21 JDBC PreparedStatement](https://docs.oracle.com/en/java/javase/21/docs/api/java.sql/java/sql/PreparedStatement.html), especially typed `setNull`, `setBigDecimal` and calendar-related bindings.

### PostgreSQL

- **P1:** [PostgreSQL 17 ErrorResponse fields](https://www.postgresql.org/docs/17/protocol-error-fields.html): field meanings, optionality and indexes treated as constraints.
- **P2:** [PostgreSQL REL_17_0 `nbtinsert.c`](https://github.com/postgres/postgres/blob/REL_17_0/src/backend/access/nbtree/nbtinsert.c), `_bt_check_unique`, particularly the `ERRCODE_UNIQUE_VIOLATION`, `BuildIndexValueDescription`, `errdetail` and `errtableconstraint` block.
- **P3:** [PostgreSQL 17 error codes](https://www.postgresql.org/docs/17/errcodes-appendix.html): 23505 and integrity-class structured names.
- **P4:** [PostgreSQL 17 `pg_index`](https://www.postgresql.org/docs/17/catalog-pg-index.html): `indnkeyatts`, `indkey`, expressions/predicates and null-distinctness.
- **P5:** [PostgreSQL 17 `pg_constraint`](https://www.postgresql.org/docs/17/catalog-pg-constraint.html): nonunique constraint names, namespaces, relation IDs and `conkey`.
- **P6:** [PostgreSQL 17 PL/pgSQL errors/messages](https://www.postgresql.org/docs/17/plpgsql-errors-and-messages.html): user-selected ERRCODE and optional diagnostic fields.
- **P7:** [PostgreSQL 17 ROLLBACK TO](https://www.postgresql.org/docs/17/sql-rollback-to.html) and [protocol flow / cancellation](https://www.postgresql.org/docs/17/protocol-flow.html): aborted-state recovery, transaction status, cancellation delivery/races.
- **P8:** [PostgreSQL 17 constraints](https://www.postgresql.org/docs/17/ddl-constraints.html) and [CREATE TABLE constraint deferral](https://www.postgresql.org/docs/17/sql-createtable.html): primary/unique/composite/null behavior and deferrable checking.

### SQLite

- **S1:** [SQLite result/extended result codes](https://www.sqlite.org/rescode.html): actual native distinctions, not English-message matching.
- **S2:** [SQLite error APIs](https://www.sqlite.org/c3ref/errcode.html): code, extended code, messages, offset and connection-state limitations.
- **S3:** [SQLite PRAGMAs](https://www.sqlite.org/pragma.html), `index_list`, `index_xinfo`, `table_xinfo`: index origin, partial flag, key/auxiliary entries, expression/rowid markers and schema inspection.
- **S4:** [SQLite `sqlite3_table_column_metadata`](https://www.sqlite.org/c3ref/table_column_metadata.html): input object names and returned column properties; not a failing-key API.
- **S5:** [SQLite version-3.49.1 `build.c`](https://github.com/sqlite/sqlite/blob/version-3.49.1/src/build.c): `sqlite3UniqueConstraint` and `sqlite3RowidConstraint` message/code construction.
- **S6:** [SQLite CREATE TABLE](https://www.sqlite.org/lang_createtable.html): UNIQUE null behavior and PRIMARY KEY legacy exceptions.
- **S7:** [SQLite expression indexes](https://www.sqlite.org/expridx.html).
- **S8:** [SQLite CREATE TRIGGER / RAISE](https://www.sqlite.org/lang_createtrigger.html).

### MySQL and SQL Server

- **M1:** [MySQL official ERR packet documentation](https://dev.mysql.com/doc/dev/mysql-server/latest/page_protocol_basic_err_packet.html): code/state/message payload.
- **M2:** [MySQL 8.4 server error reference](https://dev.mysql.com/doc/mysql-errors/8.4/en/server-error-reference.html), 1062 / `ER_DUP_ENTRY` and `ER_DUP_ENTRY_WITH_KEY_NAME` formatting relationship.
- **M3:** [MySQL mysql-8.4.0 message source](https://github.com/mysql/mysql-server/blob/mysql-8.4.0/share/messages_to_clients.txt): `ER_DUP_ENTRY_WITH_KEY_NAME`, localized templates and printed value/key size limits.
- **M4:** [MySQL 8.4 INFORMATION_SCHEMA.STATISTICS](https://dev.mysql.com/doc/refman/8.4/en/information-schema-statistics-table.html): ordered key column/expression metadata.
- **M5:** [MySQL 8.4 CREATE INDEX](https://dev.mysql.com/doc/refman/8.4/en/create-index.html): UNIQUE/null, functional and prefix key behavior.
- **M6:** [go-sql-driver/mysql v1.9.0 packet decoder](https://github.com/go-sql-driver/mysql/blob/v1.9.0/packets.go), `handleErrorPacket`, and [MySQLError fields](https://github.com/go-sql-driver/mysql/blob/v1.9.0/errors.go). This is primary evidence for that driver's representation, not a substitute for the server protocol specification.
- **Q1:** [Microsoft SQL Server 2627](https://learn.microsoft.com/en-us/sql/relational-databases/replication/mssql-eng002627?view=sql-server-ver17).
- **Q2:** [Microsoft SQL Server 2601](https://learn.microsoft.com/en-us/sql/relational-databases/replication/mssql-eng002601?view=sql-server-ver17).
- **Q3:** [Microsoft MS-TDS ERROR token](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tds/9805e9fa-1f8b-4cf8-8f78-8d2602228635): structured token members; no failing key-column array.
- **Q4:** [Microsoft `sys.index_columns`](https://learn.microsoft.com/en-us/sql/relational-databases/system-catalog-views/sys-index-columns-transact-sql?view=sql-server-ver17): object/index/column identity, key ordinal and included-column flag.

Sources were fetched from official docs or owning repositories and inspected as text/source, including native duplicate-reporting implementations. Issue #40/#33 and comments were read via `gh`. No database server/protocol integration suite, performance comparison, Rill compiler, or security/soundness test was run. No claim of exhaustive behavior across server releases, extensions, proxies, triggers or alternate SQLite providers is made. In particular, classification and metadata possibilities are evidence-backed design inputs, not a fabricated universal column guarantee.
