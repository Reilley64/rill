# Jiff's date/time API model for Rill #33

**Status:** research input, not an approved specification or implementation plan.

**Checked:** 2026-11-15 (UTC).

**Ticket:** [#33. Clocks, durations, deadlines and cancellable waits](https://github.com/Reilley64/rill/issues/33).

The user's direction, **"We should take our time and date api from jiff,"** establishes Jiff as the date/time API and semantic-model source. It supersedes the assistant's unaccepted recommendation to exclude calendars from a minimal wall-clock surface. It does **not** approve every Jiff operation, Rust syntax/traits/operators, implicit environment access, a Rust dependency or FFI, backend feasibility, or new cancellation/deadline policies. The inclusion boundary still needs a human-confirmed design.

This note separates **advertised intent**, **precise upstream facts**, and **suggested Rill adaptations/open decisions**. Upstream facts below come only from Jiff's official documentation and source. Rill constraints come from the local contracts/ADR and the ticket, not from Jiff. No compiler, library, runtime, backend, scheduling-latency or soundness validation was performed.

## 1. Source pin and reading boundary

The version exposed by official docs.rs `latest` during this check was **Jiff 0.2.44**. All API citations below use that explicit version, not a moving `latest` URL. The annotated Git tag `0.2.44` resolves to commit **`95f343d3f137e5596288d862f4a04dc3be52b07b`**; the published crate's `.cargo_vcs_info.json` reports the same commit and `crates/jiff` path. Source citations use that commit. The checked workspace also contains `jiff-core 0.1.6` and `jiff-tzdb 0.1.10`; its bundled tzdb version is `2026f`. These are observed source versions, not a dependency selection for Rill. [S1-S4]

The crates.io API returned HTTP 403 in this environment, so "current" here means the [docs.rs discovery page](https://docs.rs/crate/jiff/latest) version corroborated by the official repository tag and published source metadata, not an independent successful crates.io registry query. [GitHub's "latest release" endpoint](https://api.github.com/repos/BurntSushi/jiff/releases/latest) returned the older `0.2.0` release announcement; that endpoint is not a reliable way to choose the newest crate patch version. Those discovery endpoints are moving; the evidence pin is S1. Do not mix the older announcement's API details into this pin.

**Advertised intent:** Jiff describes itself as high-level datetime primitives that are difficult to misuse, heavily inspired by Temporal. It advertises nanosecond precision, zone/DST-aware arithmetic and rounding, zone-preserving text interchange, and automatic tzdb integration. These are product/design goals; the type-specific restrictions below are the actual boundaries. Jiff itself excludes non-Unix timescales, leap-second support, non-Gregorian calendars and localization from its core model, recommending separate ICU integration for the latter two use cases. [S5]

## 2. Core values: what each type means

| Upstream type | Precise model and conversion boundary |
| --- | --- |
| `Timestamp` | A signed count of nanoseconds relative to `1970-01-01T00:00:00Z` in the **Unix timescale**. Represents a precise instant, not a civil date or a monotonic clock reading. Combine with `TimeZone` using `to_zoned`, or look up a named zone with `in_tz`, to obtain calendar/clock fields. [S6] |
| `Zoned` | A zone-aware instant: conceptually timestamp + civil datetime + zone rules. Retains a `TimeZone` for later arithmetic. Changing zones can preserve the instant while changing civil fields. Extracting `.datetime()`, `.date()` or `.time()` discards information; those smaller values alone cannot reconstruct the original instant/zone. [S7] |
| `civil::Date` | A calendar date without a time or zone. Jiff calls it Gregorian; the underlying source precisely identifies an ISO 8601 **proleptic** calendar with year zero: year `0` corresponds to `1 BCE`. No historical regional Gregorian cutover model. [S8] |
| `civil::Time` | Clock fields within a day, without a date, zone or instant. Nanosecond precision; no `24:00` alternative spelling of midnight. [S9, S14] |
| `civil::DateTime` | A date and clock time without a zone or unique instant. A perfectly valid civil value may fall in a zone's gap or fold when converted. Civil calculations have no DST transitions; civil days are treated as 24 hours. [S10, S11] |
| `Span` | A signed, unit-preserving mixture of years, months, weeks, days, hours, minutes, seconds, milliseconds, microseconds and nanoseconds. Not a scalar elapsed duration. [S11] |
| `SignedDuration` | A fixed signed elapsed quantity: whole seconds plus fractional nanoseconds with one effective sign. No months, years, weeks or days with context-dependent lengths. [S12] |
| `tz::Offset` / `tz::TimeZone` | Offset is a displacement from UTC in seconds. A zone supplies rules that can map different instants to different offsets; Jiff also supports fixed-offset zones, POSIX rule strings and TZif-backed zones. An offset observed today is not a substitute for a region's future/historical rules. [S15, S16] |

Jiff also supplies `Weekday` and `ISOWeekDate`; these are calendar helpers, not new clock kinds. Its `Zoned` equality/ordering compares **only the timestamp**, ignoring zone and civil representation. `TimeZone` equality is separate and conservative: equal zones guarantee the same rules, but equivalent rules need not compare equal; IANA-zone comparison uses both identifier and rules checksum. This matters to calendar differences and reproducibility. [S7, S8, S16]

**Suggested Rill adaptation:** retain these conceptual distinctions and explicit conversions rather than one overloaded "Date" or "Duration." Translate operations into Rill's ordinary imported families and nominal records/enums. Rust methods, `ToSpan` extension traits, chained builders, `Copy`/`Clone`, operator implementations and `Default` are upstream ergonomics, not automatically approved Rill language features. In particular, distinguish instant equality from same-zone/same-representation comparison in the design interview.

## 3. Calendar `Span` versus fixed elapsed `SignedDuration`

**Upstream facts:**

- A `Span` preserves unit fields: `PT1H60M` and `PT2H` are distinct fieldwise representations, even when they denote the same elapsed quantity in an appropriate context. It has one sign for the entire value, not independently signed components. A negative builder field makes the span negative; the upstream example `5.days().hours(-10).minutes(1)` has negative days, hours and minutes. This is not mixed-sign algebra. [S11]
- `Span` deliberately does **not** implement `PartialEq`/`Eq`. `.fieldwise()` provides explicit fieldwise equality. `.compare(...)`, `.total(...)`, `.round(...)` and `.to_duration(...)` use context when calendar units matter. A comparison such as "one month versus thirty days" cannot generally be answered without a reference date. [S11]
- For operations interpreting days or larger units, supply a relative civil/zoned datetime, or explicitly opt into invariant days via `SpanRelativeTo::days_are_24_hours()` (and corresponding configuration helpers). That opt-in also makes weeks seven 24-hour days; it does **not** fix the length of months or years. Relative civil values assume 24-hour days; relative zoned values incorporate transitions. [S11]
- Jiff recommends `Span` as its primary human-friendly datetime duration, but supplies `SignedDuration` when units need not be preserved and a fixed elapsed quantity is wanted. Adding a `SignedDuration` to `Zoned` performs timestamp arithmetic; a calendar `Span` can behave differently. Direct ISO-duration parsing into `SignedDuration` accepts hours and smaller units, not nonzero day-or-larger components. [S5, S12]
- `Span::total` returns `f64`; it is not an exact integer transport for arbitrary nanosecond totals. Conversion through `to_duration` keeps an exact fixed quantity where the reference calculation is representable. [S11]

**Suggested Rill adaptation:** use the fixed-elapsed concept for measuring intervals and as a possible input to monotonic deadlines, with a separately decided nonnegative-wait boundary. Do not interpret a calendar month/day as an elapsed timeout implicitly. Conversely, do not replace calendar `Span` with integer milliseconds merely because existing `sleep`/`after` take milliseconds. Whether mixed-sign construction should mirror Jiff's builder behavior or use an explicit sign plus nonnegative fields is a Rill surface decision; preserve the single-sign value semantics either way.

## 4. Range, precision and failure boundaries

These are **0.2.44 facts**, not promises that Rill must copy physical layouts or every limit:

| Value | Checked range/representation facts |
| --- | --- |
| Civil date/datetime | Years `-9999..=9999`; date endpoints `-9999-01-01` and `9999-12-31`. `DateTime::MAX` ends at `23:59:59.999999999`. Constructor validation includes the actual month/day combination. [S8, S10] |
| Civil time | `00:00:00` through `23:59:59.999999999`; seconds `0..=59`, fractional nanoseconds `0..=999999999`. [S9, S14] |
| Offset | Whole seconds `-93599..=93599`, i.e. `-25:59:59..=+25:59:59`. Broader than the familiar ±14 hours, and unlike Temporal's fractional-offset range. [S15, S23] |
| Timestamp | Logical 96-bit seconds/nanoseconds representation, but **not** the full signed-duration range. The source bounds give whole seconds `-377705023201..=253402207200`; the lower endpoint has fraction zero and the upper endpoint may have fraction `999999999`. Bounds are deliberately inset so conversion with any supported offset stays inside the civil date range. [S6, S13, S14] |
| SignedDuration | Normalized signed `i64` seconds plus signed fractional nanoseconds, whose signs agree or a component is zero. `MIN = (i64::MIN, -999999999)`; `MAX = (i64::MAX, 999999999)`. Negating the minimum cannot be assumed representable. “96-bit” describes the logical components, not a portable ABI/storage-size promise. [S12] |
| Span | Separate bounded fields, not arbitrary-precision components. Years have magnitude at most `19998`; most unit fields cover the whole supported datetime separation, while nanoseconds alone are restricted to `-i64::MAX..=i64::MAX`. Requesting the entire minimum-to-maximum datetime difference as a nanosecond-only `Span` can fail even though the datetime endpoints are valid. [S11, S17] |

**Source/prose discrepancy to retain:** `Timestamp::now`'s pinned panic documentation gives formatted range endpoints that do not match the pinned bounds/constants. The numeric endpoints above are computed directly from `UnixEpochSeconds` and `OffsetTotalSeconds` source bounds, corroborated by the error example in `timestamp.rs`, not copied from that `now` sentence. Reconcile this upstream discrepancy before transcribing endpoint prose into a Rill contract. No executable endpoint test was run here. [S6, S13, S14]

**API failure facts:** checked datetime arithmetic returns `Result<_, jiff::Error>`; explicit saturating variants exist. Many convenience constructors/builders and arithmetic operators can panic. `SignedDuration::checked_*` arithmetic generally uses `Option` on overflow; fallible constructors are not uniform either (`try_from_hours` returns `Option`, floating-second conversion returns `Result`), and rounding returns `Result`. `Time` additionally has deliberately wrapping operations: its `+`/`-` span operators wrap, while checked operations reject day-or-larger span fields and out-of-day results. Do not summarize Jiff as "all arithmetic errors" or "all operators panic." [S6, S7, S9, S11, S12]

Jiff's nanosecond **value precision is not clock accuracy, host clock resolution, timer resolution or a scheduling guarantee**. The checked `now.rs` uses Rust `SystemTime` normally and JavaScript date milliseconds in its opt-in web path. Neither establishes nanosecond physical observations. Rill still needs explicit overflow/conversion rules, including full `u64` millisecond inputs, negative subsecond normalization, lossy down-conversion and unrepresentable deadline addition. [S18; Rill baseline below]

## 5. Arithmetic, differences and rounding

### Calendar addition

Jiff's `Zoned::checked_add` implementation separates the calendar part from the time part: apply calendar units to civil datetime, resolve the result in the zone with compatible disambiguation, then add fixed time units to the timestamp. If only hours-or-smaller units are present it can use timestamp arithmetic directly. This order is semantic, not permission to reorder additions. [S19]

Adding a month constrains an otherwise invalid day to the target month's last day: upstream's `2019-01-31 + 1 month` is February 28. This is intentionally not reversible: March 31 + one month − one month can end on March 30. One composite span and multiple sequential additions must not be assumed interchangeable. [S7, S20]

The documented New York example starts at `2024-03-10T00:00-05`:

- add **one calendar day** → `2024-03-11T00:00-04`;
- add **24 elapsed hours** → `2024-03-11T01:00-04`.

Across the fall transition a day may instead span 25 hours. Jiff's calendar addition uses compatible gap/fold handling; "checked" does not mean "reject any ambiguous civil result." `Zoned::start_of_day` finds the first instant of the day, which need not be midnight. Civil `DateTime` has no such transitions. [S7, S10, S11]

### Differences

- `until` and `since` return `Span` and can configure largest/smallest units, rounding mode and increment through difference configuration types. Defaults differ: `Timestamp` differences use seconds as the largest unit; `Zoned` differences use hours and promise no calendar units by default. [S6, S7]
- `Zoned` differences requesting units larger than hours require equal zones; otherwise they error. Convert the endpoints to a common zone explicitly if that is intended. The default unrounded difference configuration is guaranteed not to fail; custom nanosecond-only or invalid rounding configurations can fail. [S7, S16]
- `duration_until` / `duration_since` on timestamp/zoned values instead return fixed `SignedDuration`. A wall-clock difference can be negative after a clock adjustment; changing the result type does not make the underlying observations monotonic. [S6, S7, S12, S29]

### Rounding and balancing

Rounding is not a single operation over every unit. `Timestamp::round` permits hours and smaller units; its increment must divide a 24-hour civil day. `Zoned::round` permits days and smaller units, with day increment exactly one; subday increments divide the next larger unit and cannot equal that unit. It does not round a datetime to a month/year/week. `Span::round` separately supports calendar-unit rounding/balancing with relative context. Invalid configurations and representational overflow can error. [S6, S7, S11]

`RoundMode` exposes `Ceil`, `Floor`, `Expand`, `Trunc`, `HalfCeil`, `HalfFloor`, `HalfExpand`, `HalfTrunc` and `HalfEven`. Direct span/datetime rounding defaults to `HalfExpand` (nearest, ties away from zero); rounding a span produced by datetime difference defaults to `Trunc`. Rounding a zoned day must respect that day's actual length, not blindly divide an epoch count by 86400. [S7, S11, S21]

**Suggested Rill adaptation:** copy these distinctions before deciding how much configuration to expose. A bounded v0 could expose fewer rounding operations/modes, but must name that subset rather than imply a generic `round` supports every unit. Prefer recoverable, typed error boundaries for user data; explicitly decide any panicking/saturating/wrapping conveniences instead of inheriting Rust operator behavior. Jiff's public `Error` is one opaque type with limited, non-exhaustive introspection; it is not an existing Rill error enum ready to reuse. [S30]

## 6. Zones, database selection and environment assumptions

**Precise facts:**

1. A `TimeZone` is **immutable**. Updating the tzdb or system configuration does not mutate already-loaded zone rules or existing zoned values. Re-request a zone to observe changes; caches may intervene. Thus a zone identifier alone does not pin rules across time. [S16]
2. High-level lookup/parsing APIs normally use a global `TimeZoneDatabase`. Explicit APIs include `TimeZoneDatabase::from_dir`, `from_concatenated_path`, `bundled`, `get`, and `DateTimeParser::parse_zoned_with`. Named lookup can fail; automatic integration is not a guarantee that every environment contains a usable database. [S22, S23]
3. The default features enable system zone discovery, zoneinfo, concatenated data and platform-conditional bundling. Unix normally reads system zoneinfo; Android supports its concatenated database; Windows and Wasm bundle data by default. `TZDIR` can select a zoneinfo directory. At this pin, selection chooses one backend, not per-name fallback across all installed databases: normally zoneinfo then concatenated then bundle, with concatenated tried first on Android. [S2, S22, S24]
4. System data can be updated without rebuilding the application. Bundled data needs updated crate data and application rebuilding/redeployment. `tzdb-bundle-always` enables a bundle but does not override a usable system database in global discovery; explicitly construct `TimeZoneDatabase::bundled()` and use explicit-database operations to force that source. [S2, S22, S24]
5. Database caching is documented without a stable public refresh-latency contract. `reset()` forces subsequent interactions to reload disk data. The checked zoneinfo/concatenated implementations use five-minute default TTLs, and system-zone discovery also has a five-minute cache. These are implementation facts, **not** immediate-update or five-minute end-to-end guarantees. Existing `TimeZone` values remain unchanged even after cache reset. [S16, S22, S25]
6. `TimeZone::system()` attempts platform-specific discovery, honors `TZ`, and falls back to `TimeZone::unknown()` on failure. The latter behaves like UTC but prints the distinct `Etc/Unknown` identifier. `try_system()` exposes failure instead. `Zoned::now()` uses the current system time and discovered system zone; it does not mean UTC or a request/user-selected zone. [S7, S16]
7. Unix discovery can use `/etc/localtime`; Windows maps native zone information to IANA identifiers with CLDR-derived data. Web Wasm current-time/default-zone access requires the opt-in `js` feature and assumes a browser-style environment. Unknown-unknown Wasm current-time access can panic without that feature; WASI host-filesystem/time-zone access is capability/environment dependent. These are **Jiff/Rust platform facts**, not evidence that Rill's LLVM/Wasm backend supports those integrations. [S18, S24]

**Open Rill policies:** explicit versus global database ownership; deterministic bundled data versus OS-managed freshness; version provenance in persisted application data; whether to expose system-zone fallback or require an error; default UTC versus no default; authority for reading files/environment/browser services; operational update responsibility. Adopting Jiff's semantics does not silently choose these environment policies. Recurring future civil appointments may need civil fields + zone + an explicit rules-update policy rather than only a precomputed timestamp; that is a suggested application/design question, not a Jiff scheduling service.

## 7. Gaps, folds and offset conflicts are separate questions

A civil datetime plus zone can have one mapping, no mapping (**gap**, clocks move forward), or two mappings (**fold**, clocks move backward). `to_ambiguous_zoned` / `to_ambiguous_timestamp` retain this intermediate state. The ambiguous wrappers allow `compatible()`, `earlier()`, `later()`, `unambiguous()` or a `Disambiguation` selection, with representability errors still possible. [S16, S26]

| Disambiguation | Fold | Gap |
| --- | --- | --- |
| `Compatible` (Jiff default) | Earlier occurrence | Shift to the later corresponding time |
| `Earlier` | Earlier occurrence | Shift backward by the gap |
| `Later` | Later occurrence | Shift forward by the gap |
| `Reject` | Error | Error |

For a New York spring gap, `02:30` can become `03:30` under compatible handling; it is not simply clamped to `03:00`. Earlier/later denote the resulting instant ordering, not a naive instruction to choose the numerically smaller/larger UTC offset. Jiff documents the gap's before/after-offset inversion explicitly. [S7, S26]

Separately, parsed text may contain **both offset and zone**, and those can conflict under the loaded rules. `OffsetConflict` has `Reject` (default), `AlwaysOffset` (preserve the stated instant, potentially change civil fields), `AlwaysTimeZone` (preserve civil fields, potentially change instant) and `PreferOffset` (use a valid offset, otherwise zone). The result may then still require gap/fold disambiguation. A stale future datetime serialized before a law change is an important reason for rejecting inconsistent offset/zone pairs by default. [S15, S22, S23]

The Temporal parser gives `Z` special meaning as an exact UTC instant even when a zone annotation is present; it is not interchangeable with the numeric-offset conflict path. Its pinned implementation resolves this case using the given instant before attaching the zone. [S27]

**Suggested Rill adaptation:** preserve explicit ambiguity and conflict policy concepts even if ergonomic defaults are offered. Confirm defaults per operation: Jiff's ordinary calendar addition uses compatible disambiguation, whereas parsing with an explicit inconsistent offset normally rejects. A proposed "reject everywhere" policy would be a deliberate adaptation, not a faithful description of Jiff. Do not collapse bad fields, unknown zone, overflow, ambiguous local time and offset conflict into "invalid timestamp" without deciding error classification.

## 8. Parsing, formatting and serialization boundary

**Jiff facts:**

- Default `FromStr`/`Display` use a Temporal-style hybrid drawing from RFC 3339, RFC 9557 and ISO 8601, not one strict RFC 3339 grammar. It accepts alternatives such as a space instead of `T`. Only fractional seconds are supported for datetime clock fields; arbitrary ISO 8601 forms are not. Smaller civil types can parse larger representations, so accepting a string does not guarantee all its information survived. Civil parsing rejects a `Z` offset as a likely misuse. [S23]
- Parsing a `Zoned` requires a bracketed named-zone or fixed-offset annotation. Offset-only input should be parsed as a `Timestamp` and explicitly converted if a zoned value is wanted. Default zoned output carries the offset and zone, e.g. `2024-07-04T08:39:00-04:00[America/New_York]`; timestamp output is UTC. A numeric offset alone loses named-zone arithmetic semantics. [S6, S7, S23]
- "Lossless zone-aware roundtrip" means preserving an instant and zone identity under suitable available rules, not serializing the entire tzdb snapshot or guaranteeing future deserialization after rule changes. Default offset-conflict rejection can intentionally break a roundtrip under a newer database instead of silently changing its interpretation. Unnamed/custom zones also need care: `TimeZone` has no direct `Display`/`FromStr`/Serde traits because not every value has an obvious succinct representation; helper APIs supply narrower choices. [S16, S22, S23]
- Other explicit facilities include RFC 2822, RFC 9557 and `strtime` (`strftime`/`strptime`-style) parsing/printing, plus the Jiff-specific friendly duration syntax. Core Jiff does not promise localized month names/calendars. [S5, S28]
- Serde is opt-in. The ordinary timestamp/zoned/span/signed-duration serializers emit strings through their display format; span/duration deserialization also accepts friendly syntax. `fmt::serde` separately supplies timestamp integer helpers with explicit second/millisecond/microsecond/nanosecond units and required/optional forms. This is Rust integration, not a Rill derive/trait/JSON policy. [S28, S31]

**Suggested Rill boundary:** separate human display, strict wire parsing and canonical formatting. Choose supported grammar subsets, full-input handling, resource limits and accepted annotations explicitly. Do not call a permissive Jiff parser "strict RFC 3339." Choose epoch units at numeric transport boundaries and avoid implicit floating-point seconds. Preserve the current [JSON contracts](../json-contracts.md): no reflected serialization/derive or date-specific JSON value is introduced by this note. Time-aware JSON/database adapters, if wanted, should be explicit consumers with precision/range/error rules; Jiff's Serde or SQL integration does not decide them.

## 9. Leap-second and timescale policy

Jiff deliberately behaves as if leap seconds do not exist, uses Unix time, and does not offer UTC-with-leap-seconds/TAI/GPS conversion as its datetime model. Its parser accepts a seconds component of `60` by constraining it to `59`; the documented timestamp example `2016-12-31T23:59:60Z` becomes `2016-12-31T23:59:59Z`. Ordinary civil constructors accept seconds only through 59. This is **lossy leap-second acceptance**, not leap-second storage or correct leap-inclusive elapsed measurement. [S5, S6, S9, S14]

**Rill decision still required:** mirror that parser accommodation or use a stricter boundary that rejects `:60`. Either way, keep the model's no-leap-timescale limitation explicit; do not infer leap-inclusive elapsed durations, clock-smearing policy or leap-safe hard deadlines from nanosecond representation.

## 10. What Jiff does not provide for #33

**Evidence boundary:** the checked public exports/modules and documentation provide datetime values, arithmetic, parsing and time-series iterators, not a public monotonic-instant/deadline/sleep/timer-registration/cancellation API. Jiff's design rationale explicitly distinguishes its `Timestamp` (like Rust `SystemTime`) from Rust's monotonic `Instant`. Its private `now.rs` does use monotonic readings **for cache expiration**, and can return no monotonic reading on web Wasm; that is not a public scheduling facility. [S18, S29, S32]

Jiff's `.series(...)` generates periodic datetime **values**. It does not wait, register wakeups or execute scheduled jobs. "Monotonically increasing" in those iterator docs means ordered generated values, not an elapsed-time clock or scheduler. [S7]

Therefore Jiff adoption does not answer:

- the identity/range/resolution of a Rill monotonic clock, suspend/resume semantics, process-local comparability, serialization prohibition or fake-clock seam;
- deadline versus timeout operations, overflow, zero/negative/already-expired inputs, or conversion/re-evaluation of wall-clock appointments after clock changes;
- timer registration/withdrawal/commit ownership, fairness, clock-domain selection, native/network abortability or cancellation propagation;
- when an operation stops touching borrowed buffers/resources, when cleanup finishes, or any upper bound on return latency.

These are **unanswered Rill design/runtime obligations**, not missing Jiff features that can be inferred from `SignedDuration` or `Zoned`.

### Existing Rill constraints that remain in force

The checked [standard-library contracts, §7](../standard-library-contracts.md#7-channels-selection-and-timers) specify `sleep(milliseconds: u64): void` and `after(milliseconds: u64): Receiver<void>`. Timers use **monotonic**, not calendar time; `after` sends once then closes; zero is immediately eligible, not a forced yield; duration/deadline representations must not silently wrap. Scheduling, native work or cleanup can delay completion indefinitely. This note does not rewrite those signatures or policies.

[ADR 0009](../adr/0009-concurrency-safety-and-cooperative-cancellation.md) requires cooperative cancellation and explicit ownership:

- A cancellation request is not delivered cancellation or confirmed native abort.
- Required disposal/installation/cleanup joins defer delivery; masked waits keep their obligations.
- Scoped helpers choose a winner/failure, cancel remaining children, **join execution and cleanup, then return**. Selected results and borrowed sources remain valid through that work.
- Ordinary timer/channel/poller registrations must be withdrawn safely, with exactly-once ownership at completion/cancellation races. Already-committed external effects are not rolled back.
- Native work must finish accessing all sources, or confirm abortion, before cleanup invalidates them. Moving it to a blocking pool or dropping a handle does not prove release.

Consequently, a timer winning a race is at most a result-selection/cancellation trigger: **it is not a hard return deadline**. Unabortable work or cleanup may prevent return forever. No proposed Jiff adaptation changes that invariant, introduces ambient request context, or approves public cancellation tokens.

## 11. Bounded v0 adoption choices. Not decisions

The useful scope boundary is **which coherent Jiff operations to take first**, not the superseded assertion that calendars must be excluded. Two candidate shapes for human discussion:

1. **Coherent calendar/zone slice:** timestamp, civil date/time/datetime, span, fixed signed duration, offsets/zones; validated construction, explicit conversions, checked arithmetic, fixed and calendar differences, explicit local-time resolution and one canonical text interchange path. Confirm an explicit database-source policy rather than accidentally inheriting the host. Defer some convenience queries, friendly/RFC 2822/strtime formats, broad rounding configuration and series iterators.
2. **Broader Jiff operation parity:** add the full rounding/balancing/comparison configuration and format helpers early. More familiar to Jiff users, but substantially increases edge-case, ownership/error, determinism and testing obligations. Still does not include localization, other calendars/timescales or a scheduling framework merely because separate Jiff ecosystem integrations exist.

Neither candidate settles module paths, exact signatures, representation limits or platform support. A staged implementation must say which calendar/zone APIs remain promised/deferred, rather than presenting timestamp-only implementation progress as the user's complete requested model.

Cross-cutting decisions to confirm for either shape:

- **Value and error surface:** naming; field widths; one-sign spans; instant versus representation equality; checked/saturating/wrapping availability; typed error categories and owned diagnostic context. No automatic borrowing/copyability/sendable/shareable inference from Rust traits/layouts.
- **Defaults with semantic consequences:** end-of-month constraint, compatible gaps/folds, offset conflict rejection, leap-second constraining/rejection, environment discovery/fallback, direct-versus-difference rounding defaults. Faithful semantics and safe wire strictness may require distinct explicit operations.
- **Rules ownership:** immutable zone snapshots versus re-resolution; explicit database handles versus an implicit global; source/version/update authority; deterministic fixtures and changed-rules persistence behavior.
- **Separate elapsed-time layer:** monotonic instant and clock injection, elapsed duration validation for waits, deadline overflow/expiry, wall-to-monotonic conversion policy, completion/cancellation/cleanup state machines. Reuse a fixed-duration concept without pretending Jiff supplies this layer.

### If implementation reuse is considered

Jiff is dual-licensed **Unlicense OR MIT**; `COPYING` permits either. The MIT option requires retaining its copyright/permission notice in copies or substantial portions. `jiff-core` and `jiff-tzdb` manifests declare the same license expression. This is source evidence, not a legal determination about a particular reuse plan. [S2, S3, S33]

At this pin Jiff's only unconditional direct dependency is `jiff-core`; other dependencies/features bring in bundled tzdb data, platform discovery, optional Serde/logging/static macros, Windows bindings, browser bindings, and conditional portable atomics. Defaults are not a dependency-free or environment-free profile. The workspace MSRV is Rust 1.70; manifest requirements are not a resolved lockfile or proof of compatibility with Rill's toolchain/ABI/Wasm target. [S2-S4]

**Questions, not selected policies:** semantic reimplementation, source port, Rust dependency or wrapper? What notice/provenance obligations apply to copied code/docs/tests and included IANA/CLDR-derived data? Which exact feature graph and target-specific transitive licenses are shipped? Who maintains tzdb/security/compatibility updates? Does a wrapper keep all internal allocations, counted zone handles and Rust errors behind owned, explicit boundaries? How are panics prevented or handled without contradicting Rill's fatal-panic/no-unwinding policy? No reviewed source establishes a Rill/C ABI or validates LLVM/native/Wasm linkage, host pointers, bounded stacks or cancellation-safe native operations. #30's backend gate remains separate. Do not select an FFI/backend from an API research note.

## 12. Recommended next design frontier and evidence gaps

**Recommendation (not approval):** first obtain human confirmation of a **Rill-shaped Jiff semantic slice**, including calendar/zone types and explicit defaults, rather than restarting the rejected calendar-exclusion proposal. Present named choice points for the ambiguities above. Then design the **separate monotonic-clock/deadline/wait layer** around the already-confirmed timer and joined-cleanup invariants. Keep the civil/wall-to-monotonic boundary explicit. Implementation reuse and backend validation follow separate decisions.

A design interview/prototype should at least cover these adversarial cases:

- End-of-month/leap-year constraining; composite versus sequential additions; single-sign span construction; fieldwise versus context-relative comparison.
- 23/25-hour days, a midnight transition, a skipped civil date, both fold occurrences, and each gap/fold strategy.
- Changed tzdb rules with the same identifier; stale explicit offset; `Z` versus numeric zero offset; missing database; `Etc/Unknown` versus real UTC; immutable retained zone versus a fresh lookup.
- Civil/timestamp endpoints, negative fractional epoch values, nanosecond-only span overflow, signed-duration minimum negation, numeric serialization precision and chosen `:60` handling.
- Backward/forward wall-clock adjustments without moving elapsed timers; zero/expired deadlines; unrepresentable `u64` millisecond/deadline conversions; cancellation-before/after timer/native commit; unabortable work and cleanup that never returns.

**Limits/unanswered facts:** examples are read from docs/source or proposed cases, not executed Rill/Jiff tests. Exact refresh latency, host clock accuracy, a resolved shipping dependency/license graph, target ABI feasibility, trusted property evidence, resource/stack bounds, deterministic fake-clock interfaces and cancellation state machines are not established. The timestamp range-prose discrepancy remains an explicit upstream clarification point. This note supplies primary-source API/semantic evidence, not a completed #33 contract, implementation selection, hard-deadline guarantee or RFC acceptance.

## Primary-source references

All Jiff API links below are version-pinned; implementation links are commit-pinned.

- **S1. Version provenance:** [published crate metadata](https://docs.rs/crate/jiff/0.2.44/source/.cargo_vcs_info.json), [tag object](https://api.github.com/repos/BurntSushi/jiff/git/tags/49a71de3f5e912b1359026a59402edb8b0306fa5), [commit](https://github.com/BurntSushi/jiff/commit/95f343d3f137e5596288d862f4a04dc3be52b07b).
- **S2. Package/features/dependencies:** [Jiff manifest](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/Cargo.toml).
- **S3. Supporting packages/data:** [jiff-core manifest](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/Cargo.toml), [jiff-tzdb manifest](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-tzdb/Cargo.toml), [bundled tzdb version](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-tzdb/tzname.rs#L1).
- **S4. Workspace/MSRV:** [workspace manifest](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/Cargo.toml).
- **S5. Advertised model/limits:** [crate overview and features](https://docs.rs/jiff/0.2.44/jiff/#overview).
- **S6. Timestamp:** [type documentation](https://docs.rs/jiff/0.2.44/jiff/struct.Timestamp.html), [source including constructors/range-error example](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/timestamp.rs#L319-L866).
- **S7. Zoned:** [type documentation, arithmetic, differences, rounding and series](https://docs.rs/jiff/0.2.44/jiff/struct.Zoned.html).
- **S8. Civil calendar:** [civil module](https://docs.rs/jiff/0.2.44/jiff/civil/index.html), [Date](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Date.html), [proleptic/year-zero source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/src/civil/date.rs#L7-L15).
- **S9. Time:** [type documentation](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Time.html).
- **S10. DateTime:** [type documentation](https://docs.rs/jiff/0.2.44/jiff/civil/struct.DateTime.html).
- **S11. Span:** [type documentation](https://docs.rs/jiff/0.2.44/jiff/struct.Span.html), [relative context](https://docs.rs/jiff/0.2.44/jiff/struct.SpanRelativeTo.html).
- **S12. SignedDuration:** [type documentation and API](https://docs.rs/jiff/0.2.44/jiff/struct.SignedDuration.html).
- **S13. Timestamp constants/normalization:** [core timestamp source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/src/timestamp.rs#L15-L104).
- **S14. Precise bounds:** [offset/seconds/fraction/epoch/year bounds](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/src/bounds.rs#L260-L403), [hour bounds](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/src/bounds.rs#L212-L217).
- **S15. Offset and conflict policy:** [Offset](https://docs.rs/jiff/0.2.44/jiff/tz/struct.Offset.html), [OffsetConflict](https://docs.rs/jiff/0.2.44/jiff/tz/enum.OffsetConflict.html).
- **S16. TimeZone:** [immutability, equality, system discovery, ambiguity and serialization](https://docs.rs/jiff/0.2.44/jiff/tz/struct.TimeZone.html).
- **S17. Span field limits:** [source bounds](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/util/b.rs#L192-L230).
- **S18. Actual clock interaction:** [private now.rs](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/now.rs).
- **S19. Calendar then elapsed arithmetic:** [Zoned implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/zoned.rs#L2294-L2338).
- **S20. Calendar addition properties:** [Date checked_add](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Date.html#method.checked_add), [Zoned checked_add](https://docs.rs/jiff/0.2.44/jiff/struct.Zoned.html#method.checked_add).
- **S21. Rounding modes:** [RoundMode](https://docs.rs/jiff/0.2.44/jiff/enum.RoundMode.html).
- **S22. Database semantics/update/reset:** [TimeZoneDatabase](https://docs.rs/jiff/0.2.44/jiff/tz/struct.TimeZoneDatabase.html), [backend selection source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/db/mod.rs#L293-L337).
- **S23. Temporal interchange grammar and parser configuration:** [module](https://docs.rs/jiff/0.2.44/jiff/fmt/temporal/index.html), [DateTimeParser](https://docs.rs/jiff/0.2.44/jiff/fmt/temporal/struct.DateTimeParser.html).
- **S24. Environment/platform policy:** [pinned PLATFORM.md](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/PLATFORM.md).
- **S25. Cache implementation details:** [zoneinfo TTL](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/db/zoneinfo/enabled.rs#L22), [concatenated TTL](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/db/concatenated/enabled.rs#L27), [system discovery cache](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/system/mod.rs#L69-L123).
- **S26. Ambiguity:** [Disambiguation](https://docs.rs/jiff/0.2.44/jiff/tz/enum.Disambiguation.html), [AmbiguousZoned](https://docs.rs/jiff/0.2.44/jiff/tz/struct.AmbiguousZoned.html), [AmbiguousOffset](https://docs.rs/jiff/0.2.44/jiff/tz/enum.AmbiguousOffset.html).
- **S27. Zulu offset interpretation:** [parser implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L65-L98).
- **S28. Formats and Serde helpers:** [fmt module](https://docs.rs/jiff/0.2.44/jiff/fmt/index.html), [Serde helpers](https://docs.rs/jiff/0.2.44/jiff/fmt/serde/index.html).
- **S29. Wall versus monotonic:** [official API design rationale](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/DESIGN.md#why-isnt-timestamp-called-instant).
- **S30. Public error model:** [Error documentation](https://docs.rs/jiff/0.2.44/jiff/struct.Error.html).
- **S31. String serialization implementations:** [Timestamp](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/timestamp.rs#L2628-L2679), [Zoned](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/zoned.rs#L4049-L4097), [Span](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L3339-L3385), [SignedDuration](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L2711-L2760).
- **S32. Public API versus private clock helper:** [exports and modules](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/lib.rs#L765-L800).
- **S33. License terms:** [COPYING](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/COPYING), [MIT](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/LICENSE-MIT), [Unlicense](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/UNLICENSE).
