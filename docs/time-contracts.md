# Rill v0 date/time contracts

This is human-confirmed [#33](https://github.com/Reilley64/rill/issues/33) input, with [ADR 0022](adr/0022-jiff-time-and-explicit-cancellation.md), the [clock/cancellation contract](clock-cancellation-contracts.md), and prototypes [18](../prototypes/18-time-values.rill) / [19](../prototypes/19-clocks-cancellation.rill). Q1-Q91 were consolidated and explicitly confirmed; Q92 additionally confirms explicit std::env::Inputs for system-zone discovery. This is **design approval, not an implemented library, executed Rill tests, checker/safety proof, backend support or accepted RFC**. #28/#30 retain formalization/backend gates.

Use Jiff 0.2.44, commit `95f343d3f137e5596288d862f4a04dc3be52b07b`, as the semantic reference, not an automatically selected Rust dependency/FFI/backend. Rill adaptations below control over upstream convenience/implementation behavior. The [model](research/0033-jiff-api-model.md), [wire/differences](research/0033-jiff-wire-and-differences.md), [clock/zone](research/0033-clock-and-zone-boundaries.md) and [arithmetic](research/0033-jiff-checked-arithmetic.md) audits provide pinned primary citations and explicit discrepancies; they are research, not additional approved APIs.

## 1. Modules, values and evidence

| Home | Required public types |
| --- | --- |
| std::time | Timestamp, Zoned, Span, SignedDuration, SpanSign, SpanFields, Unit, RoundMode, Order, RelativeTo, RoundOptions, DifferenceOptions, SpanRoundOptions, SpanTotalOptions, TextLimits, TimeError, TimeField, TimeSyntaxReason, TimeLimit; clock types in the companion contract |
| std::time::civil | Date, Time, DateTime, DateFields, TimeFields, Weekday, ISOWeekFields |
| std::time::tz | Offset, TimeZone, TimeZoneDatabase, DatabaseLimits, Disambiguation, OffsetConflict |
| std::time::fmt | ZonedParseOptions, defaultZonedParseOptions and explicit parsing/formatting operations listed below; no reflection/derive/Serde |

No prelude expansion, directory-as-module rule, implicit child import, runtime exported option globals, optional arguments, methods, traits, user operators or general return-only overloads. These are ordinary file-module exports. Shared operations have one canonical family declaration. Child modules may contribute implementations using implement under #41. The signatures below are interface schemas, not conflicting body-less declarations or complete source files. Source bindings still need imports; internal evidence access grants no source privacy bypass.

Date/Time/DateTime/Timestamp/Span/SignedDuration/Offset are public nominal private-field types with justified scalar-only implicit-copy, noBorrow/sendable/shareable metadata. A named input record is not itself a validated temporal value. No mutable representation escapes. Existing explicit copy, callee-exit, transitive view and property rules apply.

TimeZone and Zoned own retained immutable rule data. They are noBorrow/sendable/shareable, not implicitly copyable; canonical copy(&value) explicitly retains immutable backing rather than copying every transition. A Zoned owns its timestamp and zone snapshot; civil fields always agree with those. Borrowed accessors retain source dependencies. Cache reset/disposal cannot invalidate a returned zone. This is documented shared ownership, not permission for writable rules or implicit ordinary-value copying. Native metadata still needs #28/#30 justification.

Canonical eq/hash compare civil fields for civil values, the instant for Timestamp, the elapsed quantity for SignedDuration, and seconds for Offset. For Zoned, they compare only the timestamp and ignore zone representation. Span has no canonical semantic eq/hash. Use fieldwiseEqual or context-relative compareSpan. TimeZone equality compares fixed offsets or resolved named identity and actual rule contents, independent of cache generation/static-versus-loaded representation. Matching identifiers/version labels/CRCs alone are insufficient; aliases can remain unequal. Fingerprints accelerate equality checks but do not prove equality. TimeZoneDatabase has no eq/hash. Hashes are not persistence formats.

## 2. Precision, bounds and checked construction

Unix time ignores leap seconds; there is no TAI/GPS/non-Gregorian/localization model. Civil years are -9999 through 9999, proleptic Gregorian with year zero. Time is 00:00:00 through 23:59:59.999999999, not 24:00. Nanosecond representation is not clock accuracy/resolution.

Timestamp bounds, from pinned source rather than the discrepant now documentation, are **-377705023201000000000 through 253402207200999999999 Unix nanoseconds**. They are inset to keep conversion through all supported offsets within civil bounds. Explicit conversion *to Timestamp* checks this narrower range; pure civil arithmetic does not acquire it through a hidden UTC helper. Offset is an integral -93599 through 93599 seconds (±25:59:59).

SignedDuration bounds are -9223372036854775808999999999 through 9223372036854775807999999999 nanoseconds (i64 seconds plus same-sign subsecond fraction). They do not cover the full i128 range. Span has one sign and bounded nonnegative magnitudes: years 19998; months 239976; weeks 1043497; days 7304484; hours 175307616; minutes 10518456960; seconds 631107417600; milliseconds 631107417600000; microseconds 631107417600000000; nanoseconds 9223372036854775807. Input zero sign normalizes to positive zero; mixed-sign stored fields do not exist.

```rill
// std::time::civil: public input/output records, not private representations.
export type DateFields = { year: i32; month: u8; day: u8 };
export type TimeFields = { hour: u8; minute: u8; second: u8; nanosecond: u32 };
export enum Weekday { Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday }
export type ISOWeekFields = { year: i32; week: u8; weekday: Weekday };
// std::time
export enum SpanSign { Positive, Negative }
export type SpanFields = {
  years: u64; months: u64; weeks: u64; days: u64; hours: u64;
  minutes: u64; seconds: u64; milliseconds: u64; microseconds: u64; nanoseconds: u64;
};
```

Required operation signatures (constructors sharing a type's spelling are ordinary factory functions in the separate value namespace, not public raw-record construction):

```text
civil::Date(DateFields): Result<Date, TimeError>
civil::Time(TimeFields): Result<Time, TimeError>
civil::DateTime(Date, Time): DateTime
civil::dateFields(&Date): DateFields
civil::timeFields(&Time): TimeFields
civil::date(&DateTime): Date
civil::time(&DateTime): Time
civil::weekday(&Date): Weekday
civil::inLeapYear(&Date): bool
civil::daysInMonth(&Date): u8
civil::daysInYear(&Date): u16
civil::dayOfYear(&Date): u16
civil::isoWeekFields(&Date): ISOWeekFields
civil::dateFromISOWeek(ISOWeekFields): Result<Date, TimeError>
Span(SpanSign, SpanFields): Result<Span, TimeError>
defaultSpanFields(): SpanFields
spanFields(&Span): SpanFields
spanSign(&Span): SpanSign
timestampFromNanoseconds(i128): Result<Timestamp, TimeError>
durationFromNanoseconds(i128): Result<SignedDuration, TimeError>
unixNanoseconds(&Timestamp): i128
durationNanoseconds(&SignedDuration): i128
tz::offsetFromSeconds(i32): Result<Offset, TimeError>
tz::offsetSeconds(&Offset): i32
tz::fixedZone(Offset): TimeZone
tz::utcZone(): TimeZone
zonedAt(Timestamp, &TimeZone): Zoned
resolveLocal(DateTime, &TimeZone, Disambiguation): Result<Zoned, TimeError>
timestamp(&Zoned): Timestamp
datetime(&Zoned): DateTime
zone(&Zoned): &TimeZone
```

The nanosecond field is the whole fractional second, not Jiff's separately named base-1000 component. Ordinal day is 1-based. ISO week-year may differ from civil year; conversion validates actual week count and resulting date. Specialized leap-excluding ordinals and other convenience-query parity are not required.

Named timestampFromSeconds/Milliseconds/Microseconds and durationFromSeconds/Milliseconds/Microseconds/Hours/Minutes also take i128 counts and check exact scaling/range. unixSecondsExact/MillisecondsExact/MicrosecondsExact and durationSecondsExact/MillisecondsExact/MicrosecondsExact/HoursExact/MinutesExact return Result<i128,TimeError>, failing on a remainder; corresponding **Trunc**-suffixed operations return i128 toward zero. No implicit float seconds or standard string/numeric conversion registration is added. Explicit #41 application conversions can call these functions; units are never guessed from primitive type alone.

## 3. Arithmetic, comparison and relative context

All arithmetic uses named functions, not Date + Span operator syntax. add/subtract take borrowed left operands and scalar right operands; supply implementations for Date/Time/DateTime/Timestamp/Zoned with Span or SignedDuration, and SignedDuration with SignedDuration. Return Result<same nominal left type,TimeError>. Scalar value comparison uses compare(&T,&T):Order for civil values, Timestamp, SignedDuration, Offset and Zoned (timestamp order); Order has Less/Equal/Greater. Eq/hash remain canonical standard families, not locally unrelated functions.

- For Date, compute the combined year/month target and constrain the original day once. Then apply weeks/days, followed by whole 24-hour days from the aggregated time portion. Any nonzero remainder produces an error. 23h+60m succeeds as one day; ±23h fails. Use DateTime when a clock result is wanted.
- For Time, reject nonzero calendar fields, preserve the entire fixed quantity and return an error on crossing midnight. No wrapping API/operator is required.
- DateTime uses civil 24-hour days. Apply the combined calendar target first, then the full time carry/remainder. Do not mutate the date by time carry before month arithmetic.
- Timestamp accepts only hours-and-smaller Span fields. Nonzero day-or-larger fields produce an error, even in UTC. SignedDuration adds exact elapsed time.
- For Zoned, the calendar portion changes local fields and resolves once with Compatible. Then fixed hours-and-smaller change the timestamp. Pure fixed input retains the selected fold occurrence by avoiding needless civil re-resolution.
- Checked SignedDuration add/subtract/multiply are exact whenever the final mathematical result fits. MIN−MIN succeeds as zero; no spurious negation-first rejection. multiplyDuration(&SignedDuration,i128) checks range before unsafe intermediate overflow. divideDurationExact(&SignedDuration,i128) requires exact nanosecond divisibility; divideDurationTrunc uses toward-zero division. Both reject zero and genuine overflow. negateDuration/absDuration return Result; the whole asymmetric-minimum band genuinely cannot be made positive. negateSpan/absSpan return Span directly because its bounds are symmetric.

```rill
export enum Unit { Year, Month, Week, Day, Hour, Minute, Second, Millisecond, Microsecond, Nanosecond }
export enum RoundMode { Ceil, Floor, Expand, Trunc, HalfCeil, HalfFloor, HalfExpand, HalfTrunc, HalfEven }
export enum Order { Less, Equal, Greater }
export enum RelativeTo {
  CivilDate(Date),
  CivilDateTime(DateTime),
  Zoned(&Zoned),
  DaysAre24Hours,
}
export type RoundOptions = { unit: Unit; increment: u64; mode: RoundMode };
export type DifferenceOptions = { largest: Option<Unit>; smallest: Unit; increment: u64; mode: RoundMode };
export type SpanRoundOptions = {
  largest: Option<Unit>; smallest: Unit; increment: u64; mode: RoundMode;
  relative: Option<RelativeTo>;
};
export type SpanTotalOptions = { unit: Unit; relative: Option<RelativeTo> };
```

Source bindings for Option/Date/etc. need explicit imports in a real source file. RelativeTo/options containing a Zoned view preserve ordinary dependencies, including after copying descriptors. They do not become noBorrow just because options are passed by value.

addSpan/subtractSpan(&Span,Span,Option<RelativeTo>) and addSpanDuration/subtractSpanDuration(&Span,SignedDuration,Option<RelativeTo>) return Result<Span,TimeError>. Any nonzero calendar operand needs context **before cancellation**, including 1 month plus zero or 1 day minus itself. Hours-and-smaller can use None. Real relative arithmetic applies the left operand to the start, applies the right operand to that result, then computes the difference from the start. It is not componentwise addition. Preserve Jiff's largest-input-unit cap. Opposite operand signs are allowed and produce a balanced one-sign result. Genuine intermediate/result-field overflow still produces an error. Date context means civil midnight, not lossy Date arithmetic. DaysAre24Hours supports days/seven-day weeks, never months/years.

fieldwiseEqual(&Span,&Span):bool compares stored sign/magnitudes. compareSpan(&Span,&Span,Option<RelativeTo>):Result<Order,TimeError>, spanTotalExact(&Span,SpanTotalOptions):Result<i128,TimeError> and spanTotalApprox(...):Result<f64,TimeError> apply the same context rules, including requested calendar units. Exact totals reject fractional units and never route through f64. Approximate totals intentionally permit precision loss under f64 rules, not a new float constructor/rational type. Missing/invalid context or unrepresentable calculations remain errors.

## 4. Differences and rounding

until/since(&T,&T):Result<Span,TimeError> supply Date, Time, DateTime, Timestamp, Zoned implementations. untilWithOptions/sinceWithOptions add DifferenceOptions. Unconfigured defaults are guaranteed successful for valid endpoints under normal execution (not immunity to cancellation/fatal allocation).

| Endpoint | Default largest / smallest |
| --- | --- |
| Date | Day / Day |
| Time | Hour / Nanosecond |
| DateTime | Day / Nanosecond |
| Timestamp | Second / Nanosecond |
| Zoned | Hour / Nanosecond |

The defaults are increment 1 and mode Trunc. Raising smallest without explicit largest selects max(default largest,smallest), subject to the endpoint domain; Time remains capped at Hour. Date admits day/week/month/year; Time and Timestamp admit at most Hour. Configured Zoned calendar differences require **equal zones before any zero shortcut**, unlike the pinned early-zero implementation. Do not equate names/current offsets alone. Apply since direction **before rounding**.

durationUntil/durationSince(&T,&T):SignedDuration supply all five types directly/infallibly for valid values. Civil Date/DateTime use 24-hour days; Time has no inferred midnight wrap. Zoned compares timestamps regardless of zones. A returned standalone Span does not retain its reference context.

roundTo(&T,Unit) and roundWithOptions(&T,RoundOptions) return Result<T,TimeError> for Time/DateTime/Timestamp/Zoned/SignedDuration/Offset. Direct default mode is HalfExpand, increment 1; defaultRoundOptions():RoundOptions uses Nanosecond; defaultOffsetRoundOptions():RoundOptions uses Second. Span uses roundSpan(&Span,SpanRoundOptions), with defaultSpanRoundOptions(): None largest, Nanosecond smallest, increment 1, HalfExpand, None relative. defaultDifferenceOptions() is the common field seed (None/Nanosecond/1/Trunc); defaultDifferenceOptionsForDate() uses Day smallest. defaultSpanTotalOptions():SpanTotalOptions uses Nanosecond/None. No direct Date round is promised.

Validate unit order/domain and nonzero increments; never ignore unsupported settings. Time/Timestamp allow at most Hour; DateTime/Zoned allow at most Day. Timestamp increments divide 24 hours; zoned day increment is 1, subday increments divide the next unit and are smaller than it. Zoned day rounding uses actual day length. Span calendar balancing needs real context or the fixed-day marker where permitted. No reinterpretation of numeric conversions' ties-to-even rule.

## 5. Zone resolution, sources and limits

Disambiguation has Compatible, Earlier, Later, Reject. Compatible chooses the earlier fold occurrence and shifts forward by the gap. For a one-hour gap, 02:30 becomes 03:30. Explicit Reject distinguishes nonexistent from ambiguous local times. A valid civil input need not identify a unique instant.

OffsetConflict has Reject (default), AlwaysOffset, AlwaysTimeZone, PreferOffset. Reject uses exact whole-second rule matching, with no minute-rounded tolerance. Explicit outer Z identifies an exact UTC instant before attaching the zone, distinct from ordinary numeric +00:00 conflict resolution. Negative-zero offset text is rejected rather than claiming Jiff/RFC9557 equivalence.

```rill
export type DatabaseLimits = {
  maxIdentifierBytes: u64; maxIndexEntries: u64; maxRuleBytes: u64;
  maxTransitions: u64; maxCachedZones: u64; maxCachedRuleBytes: u64;
};
```

```text
defaultDatabaseLimits(): DatabaseLimits
bundledDatabase(): Result<TimeZoneDatabase, TimeError>
bundledDatabaseWithLimits(DatabaseLimits): Result<TimeZoneDatabase, TimeError>
defaultDatabase(): Result<TimeZoneDatabase, TimeError> // canonical alias of bundledDatabase
systemDatabase(): Result<TimeZoneDatabase, TimeError>
systemDatabaseWithLimits(DatabaseLimits): Result<TimeZoneDatabase, TimeError>
get(&TimeZoneDatabase, &string): Result<TimeZone, TimeError>
reset(&TimeZoneDatabase): void
identifier(&TimeZone): Option<&string>
zoneVersion(&TimeZone): Option<&string>
databaseVersion(&TimeZoneDatabase): Option<string>
systemTimeZone(&TimeZoneDatabase, &std::env::Inputs): Result<TimeZone, TimeError>
```

Database handles are noBorrow/sendable/shareable, not implicitly copyable. Explicit copy retains the same service/cache; independent construction creates an independent service, possibly sharing immutable bundle storage. No ambient current DB/zone, automatic host-first fallback, TZDIR selection or raw/custom-directory/TZif/POSIX public loader. Parsing internally still interprets IANA recurrence rules. Bundle data is pinned per distributed build; updates require an explicit maintained rebuild/redeployment path. This design does not freeze a shipping tzdb edition forever or select dependency/licenses.

Q92 specifies that systemTimeZone reads TZ only from the supplied owned Inputs, never live getenv/guest-libc state. Missing TZ permits supported OS discovery; a present invalid/non-UTF8/unsupported TZ produces an error rather than being ignored. Resolve names through the selected DB; never substitute UTC, Etc/Unknown or today's fixed offset for unknown regional rules. Preserve #36's safe EnvError diagnostics/cause boundaries. Native host discovery is an explicit capability; WASI/preopens and #35 filesystem authority still apply. No sandbox/atomic external filesystem snapshot is promised.

Lookup is ASCII-case-insensitive and returns the entry's spelling; `us/eastern` stays `US/Eastern` if present, not primary-alias normalization. Reject empty/absolute/escape names, NUL/backslash and empty/dot/dot-dot components; accepted identifiers are ASCII letters/digits plus `_`, `-`, `+`, `.`, `/`, selecting validated indexed entries. Case-colliding source entries are invalid data rather than order-dependent selection. OS-managed source contents/symlinks are a trust boundary, not a race-free confinement proof.

The defaults are 255 identifier bytes, 65536 index entries, 1048576 source rule bytes, 100000 retained transitions/zone, 4096 cache entries and 16777216 summed source-rule bytes/cache. These are inclusive independent u64 caps; zero is restrictive. Index inspection counts encountered entries, including aliases/directories/metadata in a disk walk; bundled indexing charges its source entries, not only queried names. Rule bytes bound the selected encoded payload before excess reading; transition count bounds retained imported/generated records, not recursion. Avoid unbounded derived-table expansion. Pure fixed/UTC construction does not require a rule-file read.

Charge each cached resolved-name entry its full source-rule byte weight even if backing bytes are shared; cache hits/copies do not add entries. A new entry exceeding either cap produces an error, not silent eviction. Failed unknown-name queries do not build an unbounded negative cache. Reset starts a new lookup generation; in-flight older calls may return older snapshots but cannot populate the new generation with stale data. Successful cached values remain until reset. Reload/lookup is not a whole-edition snapshot; mixed system data/provenance can be unknown. Do not infer a version from a name, toolchain version or current offset. External zones, old generations, scratch/concurrent loads remain outside these logical cache caps, not an exact resident-memory quota. Allocation failure is fatal.

## 6. Parsing and formatting

Public fmt operations: parseDate/Time/DateTime/Timestamp/Span/SignedDuration/Offset(&string):Result<target,TimeError>, corresponding **WithLimits** forms taking TextLimits; parseZoned(&string,&TimeZoneDatabase):Result<Zoned,TimeError> and parseZonedWithOptions(text,db,ZonedParseOptions). formatDate/Time/DateTime/Timestamp/Zoned/Span/SignedDuration/Offset borrow the value and return Result<string,TimeError>; corresponding **WithLimits** forms add TextLimits. No generic parse selected solely by return type, implicit into<string>, canonical show adapter, reflected JSON/SQL mapping or string coercion. Handle the Result before ordinary logging/interpolation. Error payloads still supply display.

TextLimits has maxInputBytes:u64 and maxOutputBytes:u64, defaults 4096/4096 via defaultTextLimits(). Limits are inclusive and independently configurable; zero is valid. Parse counts the complete supplied UTF-8, including any invalid padding. Check size first. Format counts actual emitted UTF-8, returns complete owned text or error, never partial output/repair. Overflow is checked before excess allocation. Limits do not bound earlier reads/UTF-8 validation, total heap or time. &string already validates UTF-8; Bytes callers use explicit viewUtf8, preserving its Utf8Error boundary.

std::time::fmt exports ZonedParseOptions (`{ limits: TextLimits; disambiguation: Disambiguation; offsetConflict: OffsetConflict }`) and defaultZonedParseOptions():ZonedParseOptions, which supplies defaultTextLimits/Compatible/Reject. Format is deterministic canonical text, not an HTML/JavaScript embedding encoder or original-spelling preservation.

Required lexical profile:

- Date accepts extended year-month-day only. The year is unsigned with four digits, or an ASCII sign plus six digits. The logical range remains -9999..9999. Reject -000000. Canonical nonnegative year is four digits (0000 included); negative year is minus/six digits.
- Time requires full HH:MM:SS, with optional 1-9 fractional digits after `.` or `,`. No omitted fields, basic compact clock/date, standalone T prefix, 24:00, fractional hour/minute or second 60. Canonical fractions use dot, omit zero and strip trailing zeros.
- DateTime requires a full date, T/t/one ASCII space, then a full time without offset/zone. Canonical output uses uppercase T. No padding whitespace, prefix acceptance or ignored suffix.
- Timestamp requires full date/time plus Z/z or full numeric ±HH:MM[:SS], with no zone annotation. Canonical output uses UTC/Z, not the retained input offset.
- Zoned requires full date/time plus one bracketed named/fixed zone, with an optional outer offset. Without an outer offset, apply disambiguation. Canonical output emits the exact numeric offset then bracketed resolved name/fixed offset, including seconds when nonzero; UTC prints +00:00[UTC]. Do not minute-round either position. Reject numeric fractional offsets and negative zero offsets, including -00:00:00; +00:00 follows normal matching. Z has the distinct exact-instant path above.
- Optional ! on the single recognized zone annotation is allowed, redundant on canonical output. All key=value/extra annotations reject, including noncritical and u-ca=iso8601. Non-Zoned targets reject zones; do not discard data after successful lexical consumption.
- Offset standalone uses full ±HH:MM[:SS], range ±25:59:59, no fractions/Z/negative zero; canonical zero +00:00. Fixed bracket offsets use the same exact profile (a deliberate Jiff extension).

Span/duration permits one optional ASCII sign before P. Components are ordered, unique nonnegative integers, with case-insensitive P/T/unit letters and T before time components. At least one component is required, and one must follow T if T occurs. Last H/M/S component alone may have 1-9 decimal digits (dot/comma); expand exactly. Do not accept exponents, component signs or fractional calendar units. Span permits mixed weeks/calendar units and checked Jiff HMS spill needed for normalized output; SignedDuration requires PT with H/M/S only. Even P0D is rejected. Friendly syntax is not supported. Canonical output is uppercase, has no positive sign and renders zero as PT0S. Span preserves Y/M/W/D/H/M fields but combines seconds/subseconds; SignedDuration balances to total hours/residual minutes/seconds, never calendar days. No arbitrary fieldwise wire guarantee; original values remain unchanged.

Named-zone text serializes a name, not complete rules. Default exact-offset checking catches inconsistency at that instant, not every changed future rule. Snapshot persistence requires retaining known dataset/rules outside the string. Unsupported succinct zones produce an error, never a fallback to a fixed offset. Under suitable same rules and compatible limits, canonical parse/format preserves the instant; it does not promise recovery of source spelling, every Span subfield, or an old rules dataset. This hybrid with deliberate restrictions is not unrestricted RFC3339/RFC9557 acceptance.

## 7. Small typed errors

TimeError/TimeField/TimeSyntaxReason/TimeLimit are exported nominal enums in std::time. Concrete variants specify the agreed categories, not human-message parsing:

```rill
export enum TimeField { Year, Month, Day, Hour, Minute, Second, Nanosecond, OffsetSeconds, Week, SpanMagnitude }
export enum TimeSyntaxReason { UnexpectedEnd, UnexpectedToken, InvalidNumber, InvalidField, InvalidOffset, NegativeZero, TrailingInput }
export enum TimeLimit { InputBytes, OutputBytes, IdentifierBytes, IndexEntries, RuleBytes, Transitions, CachedZones, CachedRuleBytes }
export enum TimeError {
  InvalidField(TimeField), Range, PrecisionLoss, DivisionByZero, ContextRequired,
  InvalidUnit, InvalidIncrement, UnitOrder, NonexistentLocalTime, AmbiguousLocalTime,
  OffsetConflict, ZoneMismatch, InvalidIdentifier, UnknownZone, SourceUnavailable,
  SourceFailure, InvalidRuleData, UnsupportedConfiguration, UnsupportedZoneFormat,
  Syntax { offset: u64; reason: TimeSyntaxReason },
  UnsupportedAnnotation { offset: u64 },
  LimitExceeded { kind: TimeLimit; maximum: u64; offset: Option<u64> },
}
```

Payloads contain no borrowed input, keys, excerpts or secret environment values; scalar/enum metadata provides display/noBorrow/sendable/shareable. Safe underlying source errors may be explicitly retained causes under #23/#29, never implicit error conversion. Diagnostics identify input-relative zero-based UTF-8 bytes; EOF is at length. These positions are not columns, file bases or Err code origins. Input-byte limits can point inside a scalar; output/data limits need not have input positions. Semantic post-parse range/zone errors need not invent lexical offsets. Fail fast; after the initial size check no priority across unrelated simultaneous failures is promised. All partial copies/trees/data/strings release exactly once with bounded native call stacks, including raised limits, cancellation and native-source release.

## 8. Integration and honest gates

The companion contract defines clock/token distinctions, typed waits and unchanged withDeadline. #32/#34/#35/#38/#39/#40 consume clocks, explicit tokens and retained snapshots; this document does not finalize network/HTTP/TLS/SQL/provider APIs. #36 supplies explicit Inputs, never live environment reads. #41 controls implement/conversion evidence, not automatic temporal codecs. #24 owns diagnostic presentation.

#28 must formalize exact arithmetic/context/rounding/parse algorithms, evidence, native private-type metadata, transitive callee exits, races/generation state and bounded cleanup. #30 must validate actual target/provider/ABI/Wasm support. Unworkable required rules return to the map; neither privacy, pinned research nor hand-checked fixtures constitutes a proof. No commit/push or backend selection is implied by closing #33.
