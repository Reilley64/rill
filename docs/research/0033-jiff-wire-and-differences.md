# Jiff wire text and pure differences. Fact audit for Rill #33

This is primary-source research, not a Rill specification or a selection of remaining policy.

Pin: Jiff 0.2.44, commit `95f343d3f137e5596288d862f4a04dc3be52b07b`. Background/provenance: [0033-jiff-api-model.md](0033-jiff-api-model.md). Source links below are commit-pinned; API links are version-pinned.

Scope supplied by the user: approved Rill-native Jiff core comprises Timestamp, Zoned, civil types, Span, SignedDuration, offsets/named zones, arithmetic/differences/rounding and canonical text. It does not include every Rust convenience. Named rule snapshots are explicit. Rill will reject leap `:60`, not copy Jiff's clamping. Rill parsers must consume the whole input and be type-specific, not silently discard extra date/time/offset/zone fields. These directions supersede the earlier note's open leap-parser choice. No other Rill policy is settled here.

The audit inspected the pinned official repository archive and RFC texts. It ran no upstream executable probes or Rill tests and performed no implementation work. Examples are upstream documented/test fixtures or explicitly identified deductions from source, not executed observations. No runtime/backend/safety proof claims follow from this audit.

## 1. What "canonical" means in this audit

Here, Jiff default text means ordinary, non-alternate `Display`, with no precision/custom-printer options. Jiff accepts more spellings than it emits. It calls this a Temporal-style hybrid of RFC 3339, RFC 9557 and ISO 8601, not a strict RFC 3339 codec. "Canonical" below describes that deterministic default output, not a uniquely mandated RFC normalization and not yet Rill's selected wire grammar. Span/SignedDuration alternate display and Debug use friendly text; their `FromStr` also accepts friendly syntax. [T, SP, SD]

### Default date/time output

| Value | Default form and important detail |
| --- | --- |
| `civil::Date` | `YYYY-MM-DD`, except negative years use the six-digit signed spelling below. |
| `civil::Time` | `HH:MM:SS` plus optional `.fraction`; seconds are always printed, including `:00`. |
| `civil::DateTime` | It joins the date and time with uppercase `T`, without an offset or zone. |
| `Timestamp` | It appends uppercase `Z` to the UTC civil datetime and does not retain the original parsed offset/zone. |
| `Zoned` with named zone | It appends numeric `±HH:MM` and `[name]` to the civil datetime. The name comes from the stored zone, not the original input spelling. It rounds offset seconds rather than printing them here. |
| `Zoned` with no name | It appends numeric `±HH:MM` and `[±HH:MM]` to the civil datetime. See the rules-loss caveats in §4. |
| `Zoned` in `TimeZone::UTC` | Numeric `+00:00[UTC]`, not `Z[UTC]`. |
| `Zoned` in Jiff's unknown zone | Special case `Z[Etc/Unknown]`. This zone behaves like UTC but is not an IANA identifier. |

Source: printer implementations and pinned fixtures. [DP, DP-tests, TZ]

### Years, fractions and normalization

- Values use the proleptic Gregorian calendar, including year zero (`1 BCE`). Year range is `-9999..=9999`. Default output year zero is `0000`; positive years through 9999 use four digits, no `+`. A negative year uses `-` plus six digits: year −1 prints as `-000001`, and the minimum year prints as `-009999`. There is no canonical `-0000` year. [Y, DP]
- Parser years are either exactly four unsigned ASCII digits, or ASCII `+`/`-` followed by exactly six digits, still constrained to Jiff's year range. Thus `+002024` is an accepted noncanonical spelling of 2024, `+000000` of zero; `-000000` is rejected, and `-0001` is not the supported negative-year syntax. Six-digit syntax does not expand the value range; `+010000` fails. Unicode minus is not recognized. These are source grammar facts; the example outcomes here are deductions from it. [Y]
- Default clock fractions are omitted when zero; otherwise `.` plus enough digits to express the nanoseconds exactly, stripping trailing zeros, at most nine digits. For example, 120,000,000 ns prints as `.12`, and 1 ns prints as `.000000001`. Leading fractional zeros are significant. Input fractional width/trailing zeros are not retained. [DP, F]
- Datetime parsing accepts `.` or `,`, with 1-9 ASCII fractional digits, after seconds only. A bare decimal separator fails. More than nine digits are not silently truncated by the whole-input API: the fraction scanner leaves excess input and `into_full` rejects it. Fractional hours/minutes in a datetime are not accepted. [P, F, FULL]
- Custom precision printing can pad or truncate fractions; it is not the default canonical path and truncation is not a call to datetime rounding. Do not use custom low precision and claim lossless output. [DAPI-precision, F]

## 2. Accepted lexical forms are broader than emitted text

The following are Jiff acceptance facts, not approval to adopt every form in Rill. [P, Y, O, A, FULL]

| Subject | Precise Jiff behavior |
| --- | --- |
| Date/time separator | It accepts exactly one ASCII `T`, `t`, or space between date and time, not arbitrary whitespace. Tabs, newlines and multiple spaces are not substitute separators. |
| Whitespace | Default public datetime/civil parsers do not trim leading/trailing whitespace. A separator space must be followed by a time; a trailing space is not ignored. ISO SpanParser likewise does not admit padding/internal spaces. Friendly-duration parsing is a separate grammar. |
| Date separators | Basic `YYYYMMDD` and extended `YYYY-MM-DD` are accepted; the hyphens must be consistent within the date. |
| Clock fields | It accepts a two-digit hour with optional two-digit minutes and seconds. Fields use basic or colon-delimited notation, consistently within the clock. Missing smaller fields default to zero. Dates and clocks can independently use basic/extended notation. |
| Standalone time | Can have optional `T`/`t` prefix. Jiff additionally checks ambiguous basic time spellings against month-day/year-month grammar; a prefix can disambiguate. |
| Clock bounds | Hour 00–23, minute 00–59. `24:00` is not an alternative midnight. Jiff accepts clock second `60` and clamps it to `59` (fraction retained); Rill's approved deviation is rejection. Offset seconds do not get this leap treatment. |
| UTC designator | `Z` or `z` in the outer offset position. Default Timestamp output uses `Z`. |
| Numeric offset | ASCII sign and two-digit hours, optionally minutes, optionally seconds (and, in the outer datetime position, a fraction of seconds). Basic and extended forms must be consistent within the offset: `-05`, `-0530`, `-05:30`, `-053015`, `-05:30:15`. |

### Whole input does not imply type-specific information preservation

The public datetime/civil parse functions call `into_full`: they already reject an unparsed suffix. Their loss of fields is a conversion after successful lexical consumption, not simply prefix parsing. [FULL, PARSE-API, P-convert]

| Target | Actual Jiff requirements/permissiveness |
| --- | --- |
| `Date` | It requires a date but can consume datetime/offset/zone text and return only the date. An outer `Z`/`z` errors; numeric offsets and annotations can be discarded. |
| `Time` | It requires a time but can consume a full date and time and return only the time, discarding the date/numeric offset/annotations. An outer `Z`/`z` errors. |
| `DateTime` | Date-only input is accepted as midnight by the implementation. Extra numeric offset/zone annotations can be consumed and discarded. Outer `Z`/`z` errors. |
| `Timestamp` | Requires date + time + outer offset (`Z`/`z` or numeric). A zone annotation alone cannot replace the offset. A consumed zone annotation is ignored; it need not be looked up, and its rules/conflicts do not determine the instant. |
| `Zoned` | Requires date + bracketed time-zone annotation. Time is optional (defaults to midnight); outer offset is optional. Without offset, zone rules/disambiguation determine the instant. Offset-only input without brackets fails. |

This confirms why a Rill implementation cannot simply reuse Jiff target parsing unchanged. Type-specific Rill parsing must reject the disallowed extra fields rather than merely ensure EOF. Whether Rill accepts shorthand/missing clock components, date-only Zoned, commas, basic forms or lowercase remains a grammar decision, not answered by the approved no-discard boundary.

### Annotations

- Jiff supports a time-zone annotation in the first annotation position: `[America/New_York]`, `[!America/New_York]`, or a numeric bracket offset. The optional `!` is accepted for recognized zone annotations. Named lookup for Zoned can fail; an unknown bare zone name is not an ignorable unknown key/value annotation. [A, P-convert]
- Subsequent annotations are syntactically validated key=value annotations. Noncritical ones, including `[u-ca=...]`, are ignored, not interpreted as calendar selection. Any critical key/value annotation (`[!key=value]`), including `[!u-ca=iso8601]`, fails: Jiff supports no such annotation semantically. Invalid annotation syntax fails even without `!`. The parser does not preserve ignored key/value data. [A]
- A second bare zone annotation or a bare zone following key/value annotations is not accepted by this grammar. Zone criticality is retained by the lower-level `Pieces` representation, not by ordinary Zoned default output; Zoned does not print `!`. Critically marked zone text does not make Timestamp/civil conversion preserve the zone: those conversions still discard it where otherwise accepted. [A, P-convert, DP]
- RFC 9557 permits elective unknown tags and requires action on critical unsupported tags. For offset/zone inconsistency, its critical flag requires action but does not require rejection specifically. Jiff default conflict rejection also applies without `!`; it is a Jiff policy/configuration default, not the sole RFC-mandated response. [RFC9557 §§3.1-3.4][R9557]

## 3. Offsets: value range, fractions, zero and conflict matching

### Three different text positions must not be conflated

| Position/API | Seconds/fractions and output |
| --- | --- |
| Outer datetime numeric offset | Parsing supports through `±25:59:59`, including basic syntax. It also accepts 1–9 fractional-second digits. Conversion to Offset, for example for Timestamp/Zoned, rounds to whole seconds, half away from zero. A converted result outside offset range errors. Default Zoned printing uses only rounded minutes. |
| Bracketed fixed-offset zone | Accepts hour-only or minute precision, no seconds or fractions, no `Z`/`z`. E.g. `[-04]`, `[-04:00]`, `[-0400]` work; `[-00:44:30]` does not. Range still permits hours 24 and 25, with minutes through 59. |
| Standalone `DateTimeParser::parse_time_zone` / zone printer | Fixed-offset parsing accepts second precision but rejects fractions and `Z`. Zone printing emits `±HH:MM` with `:SS` only when seconds are nonzero. This differs from bracket syntax and from `Offset` Display. |
| `Offset` ordinary Display | Shortens zero to `+00`, exact hours to e.g. `-05`, otherwise `±HH:MM` or `±HH:MM:SS`. There is no blanket rule that every offset string is always `±HH:MM`. |

The offset value range is whole seconds −93,599..=93,599, i.e. ±25:59:59, not ±14 hours or ±23:59. Parser components are hours 00-25, minutes/seconds 00-59. Source `Numeric::to_offset` rounds fractional magnitudes then applies the sign; e.g. an outer `+25:59:59.5` fails in Timestamp/Zoned conversion rather than becoming a valid larger offset. Civil conversions that discard the numeric offset do not invoke this conversion/range check. This accepts fractional syntax without storing fractional offsets. [O, B, A, ZONE-API, OD]

RFC 3339's §5.6 grammar uses four unsigned year digits, full clock seconds, integral-minute numeric offsets and offset hours 00-23. RFC 9557 imports that core grammar. Therefore Jiff's negative/signed-six-digit years, offset seconds and hours 24/25 are hybrid extensions, even where a printer comment says it is rounding "for RFC compatibility." That compatibility statement is limited to removing offset seconds, not proof that all Jiff output conforms to those RFC grammars. [R3339, R9557, T]

### Zero: `Z`, `+00`, and `-00` are not three stored offset values

- `Offset` has a single numeric zero. Conversion of parsed numeric `-00`, `-00:00`, `-00:00:00` yields that zero. Timestamp does not retain lexical sign or whether `Z` was used. [O]
- Zoned parsing special-cases `Z`/`z`: interpret the civil fields as UTC to obtain an exact instant, then attach the zone, regardless of configured offset-conflict strategy. Civil fields can change. The upstream fixture converts `2025-06-20T17:30Z[America/New_York]` to `2025-06-20T13:30:00-04:00[America/New_York]`. [P-convert, PARSE-API]
- Numeric positive or negative zero follows the normal numeric-offset conflict path. It is not the `Z` shortcut. Thus, under default Reject, `...+00[America/New_York]` or `...-00[America/New_York]` in that summer example conflicts. The negative-zero result is a deduction from `is_zulu`/`to_offset` and the normal resolver, not an executed probe. [P-convert, O]
- This is an important Jiff/RFC distinction: RFC 9557 §2.2 updates RFC 3339's `Z` semantics to unknown local offset, and §3.4 says `-00:00` can replace `Z` with the same meaning. Jiff preserves the `Z` shortcut but collapses numeric negative zero into ordinary numeric zero for zoned conflict resolution. Do not claim complete RFC semantic equivalence here. [R9557, P-convert]
- Lower-level `PiecesOffset`/`PiecesNumericOffset` can preserve Zulu vs numeric negative zero; `Pieces` printing emits negative zero as `-00:00`. That preservation is not a property of Offset/Timestamp/Zoned values. A zero fixed-offset zone is normalized by `TimeZone::fixed` to `UTC`, so bracket `[-00]` does not create a negative-zero rules identity. [O, DP, TZ-fixed]

### Historical second offsets: printing and parsing intentionally differ

In an authoritative upstream fixture, not a probe from this audit, `1968-02-01T23:15:00` in `Africa/Monrovia` has actual offset −00:44:30. Default Zoned output is `1968-02-01T23:15:00-00:45[Africa/Monrovia]`, retaining the local clock and zone name but rounding the displayed offset. [MATCH]

Printing rounds magnitude to nearest minute, half away from zero (`seconds >= 30`), retains the original offset sign, and clamps the extreme carry to `25:59` instead of printing `26:00`. Consequently tiny negative offsets can print `-00:00`. The local datetime is not shifted to compensate for printing a rounded offset. [DP-offset, OR]

The default temporal Zoned parser tests offset equality as follows: [P-convert, MATCH]

1. Exact parsed offset == rule candidate succeeds.
2. If the rule candidate is already an integral minute, compare exactly.
3. If input had an explicit seconds component, even `:00`, compare exactly after converting the parsed offset to whole seconds.
4. Otherwise round the rule candidate with `Offset::round(Unit::Minute)` and compare to the parsed minute offset. If rounding errors, fall back to exact comparison.

On a valid match under Reject, the rule-derived actual offset/instant is used, not the rounded text offset. Thus both outer `-00:45` and exact `-00:44:30` parse for the Monrovia fixture; `-00:45:00` fails. Reading that same canonical string as a Timestamp instead would use −00:45 and ignore the zone, yielding a different instant (30 seconds later; deduction from the conversions). A rounded textual offset is not always the actual displacement. [MATCH, P-convert]

Default conflict strategy is Reject, disambiguation Compatible. A numeric offset in a fold can select either matching occurrence; a gap has no valid offset for Reject. Other strategies are AlwaysOffset (honor stated instant), AlwaysTimeZone (honor local fields/rules), PreferOffset (match if possible, otherwise rules). With no outer offset, only disambiguation applies. [PARSE-API, CONFLICT]

Edge limitation: the printer's extreme-offset clamp and parser's `Offset::round` error fallback are different code paths. Do not extrapolate the normal named historical-offset roundtrip to arbitrary near-±26-hour custom named rules. Such cases need an explicit acceptance/roundtrip contract or further targeted upstream investigation. [OR, P-convert]

## 4. Zone text is not rule-snapshot serialization

- Named Zoned printing emits a name (including a user-supplied name on a TZif zone), not TZif bytes, checksums or tzdb version. Parsing uses the supplied/current database for that name. The explicit named rule-snapshot requirement in Rill is not satisfied by this text alone. Even unchanged offset at the serialized instant would not prove every past/future rule is unchanged. [TZ, DP, PARSE-API]
- POSIX zones and unnamed/custom TZif zones have no IANA name for Zoned printing. The printer substitutes the current numeric offset in brackets. Parsing reconstructs a fixed-offset zone, not the original transition rules. With integral-minute offsets this can preserve the instant but loses rule behavior; with nonzero offset seconds both copies are rounded, so it can lose even the original instant/offset. A name supplied for custom rules avoids that particular fallback but does not ship those rules or ensure the name resolves to them later. [DP, TZ]
- Exact deduction illustrating seconds loss: a Zoned at local `2000-01-01T00:00:00` with unnamed fixed offset `+00:00:01` prints `2000-01-01T00:00:00+00:00[+00:00]`; parsing makes UTC, moving the instant forward by one second. Pinned printer fixtures explicitly show Offset::MAX/MIN printing as `+25:59[+25:59]` / `-25:59[-25:59]`, not retaining the final `:59`. [DP-tests, DP-offset]
- The standalone zone printer can emit an IANA name, full-precision fixed offset, or a POSIX rule string, e.g. `EST5EDT,M3.2.0,M11.1.0`. It returns `Result` and errors for a zone lacking a supported succinct representation (e.g. unnamed TZif). This is a different API from Zoned printing, which uses the offset fallback. TimeZone itself deliberately lacks ordinary Display/FromStr/Serde. [ZONE-API, TZ]
- Fixed-offset bracket annotations represent a zone with constant displacement, not a region/rules snapshot. Outer offset and bracket offset can conflict just like named rules; neither permits a hidden seconds component in the bracket grammar. `TimeZone::UTC` prints `[UTC]`, while arbitrary nonzero fixed offsets use brackets containing an offset. [A, P-convert, TZ-fixed, DP]

None of the rule-loss behavior above selects Rill's persistence policy. A rules-preserving canonical format, a restricted printer domain, and an explicit rule-snapshot reference remain distinct design possibilities, not decisions made by this note.

## 5. Span and SignedDuration default text

### Span: unit preservation is qualified

Default output is an optional leading `-`, uppercase `P`, nonzero calendar fields in Y M W D order, and (if time fields exist) uppercase `T` with H M S in order. Positive output has no `+`; there are no individually signed components. Zero is `PT0S`, regardless of whether input spelled `P0D`, `PT0H`, `+PT0S` or `-PT0S`. Zero fields and negative zero are not retained. Weeks may mix with other units. [SP-print, SP-parse]

Years/months/weeks/days/hours/minutes are not automatically balanced upward or downward by printing: `PT1H60M` stays distinct from `PT2H`; `PT60S` is not automatically `PT1M`. But seconds, milliseconds, microseconds and nanoseconds are combined into one decimal-seconds field, possibly carrying to seconds. Fractional trailing zeros are stripped. [SP-print]

Default text therefore does not roundtrip every arbitrary Span fieldwise. For example, a Span containing `1000 milliseconds` prints `PT1S` and reparses as `1 second`; `1 second + 1000 milliseconds` prints `PT2S`. Input `PT0.001S` naturally becomes 1 ms, but that does not establish preservation of an arbitrary original subsecond decomposition. This is a deduction from printer aggregation and parser decomposition. Do not turn "unit-preserving Span" into an unqualified fieldwise-wire guarantee. [SP-print, DUTIL]

### Accepted ISO-style duration syntax and bounds

- Optional one ASCII sign before `P`; `P/p`, `T/t` and unit designators are case-insensitive. Values are nonnegative decimal integers (no exponent or component signs). Units must occur once each, largest to smallest. At least one unit is required; `P` and `PT` fail, and `P1DT` fails for lack of time units after `T`. [SP-parse, DUTIL]
- Only the last supplied H, M or S may have a fraction; decimal `.` or `,`, 1-9 digits. Fractional days/weeks/months/years fail. `PT1.5H` and `PT1.5M` work; `PT1.5H0M` does not, even though the following field is zero. More than nine digits fail the whole parse, not rounded/truncated acceptance. [SP-parse, F]
- Fractions are expanded exactly into smaller units down to nanoseconds. Upstream source fixtures expand `PT0.5H` to 30 minutes and `PT0.123456789H` to 7 minutes, 24 seconds, 444 milliseconds, 440 microseconds and 400 nanoseconds. They expand `PT0.123456789M` to 7 seconds, 407 milliseconds, 407 microseconds and 340 nanoseconds, and `PT0.123456789S` to 123 milliseconds, 456 microseconds and 789 nanoseconds. The nine-digit limit applies even to hours/minutes, although finer fractional hours/minutes could mathematically still represent whole nanoseconds. [SP-fraction-tests, DUTIL]
- Span is bounded, not arbitrary-precision ISO duration storage. Fields' maximum magnitudes include 19,998 years, 239,976 months, 175,307,616 hours, 10,518,456,960 minutes, 631,107,417,600 seconds, and `i64::MAX` nanoseconds; negatives are symmetric. [SB]
- An overflowing H/M/S field can spill into smaller fields rather than fail immediately at that individual field's limit. This is needed because printing combines subsecond fields into a seconds total larger than the single seconds field maximum. Source fixture `PT1902545624836.854775807S` parses into seconds/milliseconds/microseconds/nanoseconds fields each at their maximum; `PT175307617H1M` spills to 175,307,616h + 61m. Larger values eventually error; integer scanning itself is bounded by `u64`, followed by signed/range/accumulation checks. Do not define acceptance merely as "each literal H/M/S integer ≤ corresponding Span field bound." [DUTIL, SP-fraction-tests]

### SignedDuration: a normalized elapsed quantity, not retained units

Default text starts with an optional `-` and `PT`, followed by total hours, residual minutes and residual seconds/fraction. Total hours are not limited to 23. Zero components are omitted unless seconds are needed to represent zero/fraction. No days are printed. Direct deductions from the printer give 60s as `PT1M`, 3600s as `PT1H`, 86400s as `PT24H`, −1ns as `-PT0.000000001S`, and zero as `PT0S`. The printer emits no positive sign and retains no negative zero, input fractional width or input units. [SP-print]

The ISO-style parser requires `P` immediately followed by `T` after an optional sign, and at least one H/M/S field. It accepts the same last-unit fractions/case/comma variants. Even zero calendar components are rejected: `P0D` and `P0DT1S` fail, not just nonzero days. This sharpens the earlier background note's "nonzero day-or-larger components" wording without editing that note. [SP-parse, SD-tests]

Total range is `(i64::MIN seconds, −999999999 ns)` through `(i64::MAX seconds, +999999999 ns)`. Parsed components are accumulated with overflow checking; no saturating parse. Pinned fixtures accept `PT2562047788015215H30M7.999999999S` at maximum, `-PT2562047788015215H30M8.999999999S` at minimum, and reject overflow in individual signed values, accumulation or a final fraction. [SD, SD-tests, DUTIL]

Friendly parsing, Rust alternate formatting, and serde convenience acceptance are upstream conveniences. They do not approve widening Rill's canonical-text parser to accept them.

## 6. Pure differences: exact defaults and failure shapes

These are calculations on already-existing values; no clock read, wait or timer behavior is implied.

### `until` / `since` → `Span`

`a.until(b)` is forward from a to b; `a.since(b)` gives the reverse-signed difference. The ordinary same-type subtraction convenience uses `since`, not SignedDuration. Every listed `until`/`since` method returns `Result<Span, jiff::Error>`, even though its unconfigured default is guaranteed not to fail for valid endpoints. [DATE, TIME, DATETIME, TS, ZONED]

| Endpoint type | Default largest | Default smallest | Arithmetic interpretation / configurable boundary |
| --- | --- | --- | --- |
| `Date` | Day | Day | Calendar date separation; only day/week/month/year units, no subday fields. |
| `Time` | Hour | Nanosecond | Signed within-day clock separation, not a wrap-to-next-day inference; largest/smallest cannot exceed Hour. |
| `DateTime` | Day | Nanosecond | Civil calculation; 24-hour days; can explicitly request up through Year. |
| `Timestamp` | Second | Nanosecond | Fixed elapsed nanoseconds; can request Hour at most, not Day. |
| `Zoned` | Hour | Nanosecond | Default is elapsed hours/smaller with no calendar units; explicit Day–Year uses zone-aware calendar rules. |

All difference configurations default rounding mode to Trunc, increment 1: no rounding in those initial smallest units. If largest is not supplied and smallest is raised, Date/DateTime/Timestamp/Zoned choose the larger of their default largest and the configured smallest. Time keeps Hour (and rejects coarser settings). Source constructors and selection code substantiate these defaults. [DATE-config, TIME-config, DATETIME-config, TS-config, ZONED-config]

Configured differences can fail for invalid unit ordering, units outside the endpoint type's domain, invalid rounding increments, or representational overflow during calculation/rounding. Zoned differences requesting calendar units can also fail for unequal zones. A nanosecond-only Span can fail for widely separated DateTime/Timestamp/Zoned endpoints; valid endpoint range does not ensure that one Span field can hold the difference. Time's entire within-day difference fits its nanoseconds field. Date does not admit a nanosecond-only request at all. [DATE, TIME, DATETIME, TS, ZONED]

The documented Zoned rule is that units larger than Hour require equal zones, not merely matching current offsets or identifier strings. `TimeZone` equality considers rule identity (and name/checksum for IANA zones), not "same offset now." Source qualification: `until_with_largest_unit` returns zero for equal instants before checking zone mismatch. Therefore the documentation's general mismatch rule has at least that zero-difference exception in the pinned implementation; do not invent an unconditional Rill validation order from its prose. [ZONED-config, TZ]

### `duration_until` / `duration_since` → `SignedDuration`

All five types have these methods, not only Timestamp/Zoned. They return SignedDuration directly, without Result/Option, and their docs guarantee no error/panic for valid endpoints. They can be negative. The helpers show why each full difference fits. [DUR]

| Endpoint type | Fixed-duration computation |
| --- | --- |
| `Date` | Signed date-day count × exactly 24h. |
| `Time` | Signed difference of within-day nanosecond positions; no midnight wrap. |
| `DateTime` | Date difference at 24h/day + signed clock difference. |
| `Timestamp` | Difference of stored elapsed quantities from Unix epoch. |
| `Zoned` | Difference of timestamps, regardless of whether zones match. |

There is no caller-supplied relative context for these operations. Civil durations are not a statement about elapsed time in an unspecified real-world zone. Rust conversion of a negative result to unsigned `std::time::Duration` is fallible; conversion from SignedDuration to Span is also fallible (a SignedDuration can exceed the Span seconds limit). Those are subsequent conversions, not failures of `duration_until`. [DUR, SP-convert]

<a id="relative-context-is-needed-at-a-different-seam"></a>

### When standalone spans need relative context

- Endpoint differences already have their reference endpoints. Date/DateTime/Zoned difference code supplies `self` as the relative point if it subsequently rounds the span. No separate relative argument is required for their calendar difference operation. Negation occurs before rounding in `since`; directed rounding means it is unsafe to negate an already-rounded `until` and assume equivalence. [DIFF-impl]
- A standalone Span does not retain the reference date/zone from which it was computed. Comparison, totals, rounding/balancing and span addition/subtraction interpreting calendar units generally require a reference whenever a nonzero day-or-larger field or requested calendar unit participates. Hours and smaller are invariant. Pure date differences returning day Spans do not grant those Spans an implicit permanent 24-hour-day interpretation. [REL]
- Explicit `SpanRelativeTo::days_are_24_hours()` permits invariant days and seven-day weeks, not months/years. A relative civil Date/DateTime also makes days invariant; a Zoned reference accounts for transitions. `Span::to_duration(relative)` syntactically takes a reference/marker even when only invariant units exist; supplying context can still fail if applying calendar fields runs out of datetime range. `Span::total` yields `f64`, not an exact integer transport. [REL, SP-duration]

## 7. Negative fractions and rounding defaults

### Numeric extraction truncates toward zero, not toward the earlier instant

Timestamp `as_second`, `as_millisecond`, `as_microsecond` truncate discarded fractions toward zero. Pinned docs explicitly give the extracted values for `Timestamp::new(-5, -123456789)` as −5 seconds, −5123 milliseconds and −5123456 microseconds. Timestamp nanoseconds are exact (`i128`). Thus −1ns extracts as 0 seconds/milliseconds/microseconds, not −1 in those units, by deduction from the implementation. This is not floor-to-an-epoch-bucket semantics. [NUM-TS]

SignedDuration follows the same sign-consistent seconds/fraction model: `as_secs` whole seconds, `as_millis`/`as_micros` whole totals (`i128`), `subsec_millis`/`subsec_micros` signed truncated fractional parts. `as_nanos` is exact (`i128`). A negative fraction between −1 unit and zero can extract as zero. None of these extraction operations is rounding to nearest. Span `get_*` accessors, by contrast, report stored unit fields, not total elapsed numeric units. [NUM-SD, SP]

### Direct rounding is not difference rounding

| Operation | Default mode | Initial smallest / increment |
| --- | --- | --- |
| Direct `Span::round` | HalfExpand | Nanosecond / 1; largest defaults to max(existing largest nonzero unit, configured smallest). Calendar context restrictions still apply. |
| Direct Time / DateTime / Timestamp / Zoned round | HalfExpand | Nanosecond / 1; initially no precision loss. |
| Direct SignedDuration / Offset round | HalfExpand | Nanosecond / 1 for SignedDuration; Second / 1 for Offset. |
| All five datetime difference configurations | Trunc | As in §6; increment 1. |

HalfExpand is nearest, ties away from zero; Trunc is toward zero. There is no separate `Date::round`/DateRound in this pin; Date differences can be rounded through DateDifference. Direct date/time rounding domains differ: Time/Timestamp allow Hour and smaller; DateTime/Zoned allow Day and smaller, not Month/Year; Zoned day rounding respects actual day length. SignedDuration rounds a fixed elapsed quantity. Merely saying "Jiff rounding defaults to nearest" would misdescribe differences. [ROUND, DATE-config]

## 8. Findings to carry into design, and remaining factual/policy gaps

1. Do not promise one universal lossless Jiff string roundtrip. Named text does not serialize rules; unnamed/POSIX Zoned loses rules; second-offset fixed Zoned can lose the instant; arbitrary Span subsecond decomposition is normalized. Preserve these distinctions when specifying canonical text for the approved core. [§§3-5]
2. Whole-input and type-specific parsing are separate checks. Jiff already performs EOF checks in these datetime APIs but deliberately drops fields for smaller targets. Rill's no-discard rule and `:60` rejection are intentional adaptations, not descriptions of upstream behavior. [§2]
3. Avoid copying incorrect simplifications from background or upstream prose: SignedDuration rejects even zero calendar syntax; fixed duration differences exist for civil types too; offsets can parse fractions that get rounded; default Zoned differences use Hour. One private Zoned helper comment incorrectly says default `Unit::Day`, while its code/public documentation use Hour. [SD-tests, DUR, O, ZONED-config]
4. RFC/source mismatch needing explicit attention: numeric negative zero in Jiff does not take Zoned's `Z` conflict shortcut, despite RFC 9557's semantic equivalence of `-00:00` and `Z`. The temporal module's broad "RFCs ... supported in their entirety" summary is not a precise semantic guarantee. Its statement that calendar annotations are unsupported means "not interpreted; critical ones rejected," not "every calendar annotation spelling rejected." [T, R9557, A, P-convert]
5. Do not overgeneralize historical offset matching. Ordinary named minute-rounded text is intentionally accepted against second-precision rules; explicitly writing seconds changes equality to exact. Extreme-offset clamp/fallback behavior is not the same normal roundtrip case. [§3]
6. Separate fallibility layers. Default Span differences are Result-returning but guaranteed successful for valid endpoints; configured differences can fail; all five fixed-duration differences are direct/infallible; later Span/calendar conversion may fail and needs context. No need to obtain environment time merely to calculate these differences. [§6]

Rill still needs to choose which noncanonical spellings to accept and how to handle, retain or reject unknown/critical annotations beyond the approved no-discard field boundary. It must decide whether to reject, normalize or explicitly represent negative zero, and define error categories and validation precedence. Other choices remain: strict or Jiff-rounded historical offset matching; field-preserving Span wire text or Jiff normalization; printing behavior for unsupported/unnamed rules; and links from canonical named-zone text to an explicit immutable rule snapshot. A source audit cannot select those policies. Exact bound/roundtrip contracts for extreme custom second-offset rules remain a targeted factual follow-up if that domain is retained. No implementation/backend choice, runtime performance, clock accuracy, scheduling or safety validation was performed.

## Primary-source references

All repository URLs below share the required commit pin.

- **T. Hybrid grammar and summary claims:** [temporal module](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/mod.rs#L1-L170).
- **P. Datetime grammar:** [parser date/time entry points](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L275-L400), [field parser and leap constraint](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L524-L853).
- **Y. Years:** [year grammar/range/sign](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L681-L724), [ASCII sign](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L918-L940), [proleptic year zero](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/src/civil/date.rs#L7-L15).
- **DP. Default datetime printer:** [Zoned/Timestamp/date/time](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/printer.rs#L237-L461), [annotations/Pieces](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/printer.rs#L662-L893).
- **DP-tests. Pinned printer fixtures:** [Zoned](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/printer.rs#L1140-L1221).
- **F. Fractions:** [parser](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/util.rs#L713-L800), [printer](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/buffer.rs#L508-L555).
- **DAPI-precision:** [DateTimePrinter precision](https://docs.rs/jiff/0.2.44/jiff/fmt/temporal/struct.DateTimePrinter.html#method.precision).
- **FULL. EOF checks:** [Parsed::into_full](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/mod.rs#L184-L242).
- **PARSE-API. Target parsing:** [DateTimeParser](https://docs.rs/jiff/0.2.44/jiff/fmt/temporal/struct.DateTimeParser.html), [implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/mod.rs#L649-L891).
- **P-convert. Zoned matching/Zulu and target conversion:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L55-L168).
- **A. Annotation parsing:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/rfc9557.rs#L107-L398).
- **O. Offset grammar/conversion:** [parsed offsets/zero](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/offset.rs#L128-L249), [parser configuration/fields](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/offset.rs#L305-L654).
- **B. Offset bounds:** [core source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff-core/src/bounds.rs#L260-L282).
- **OD. Offset display:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/offset.rs#L1137-L1157).
- **DP-offset:** [rounded/full-precision offset printers](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/printer.rs#L774-L893).
- **OR. Minute rounding/clamping:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/offset.rs#L1069-L1120).
- **MATCH. Historical matching examples:** [OffsetConflict::resolve_with docs and implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/offset.rs#L1895-L2119).
- **CONFLICT:** [OffsetConflict](https://docs.rs/jiff/0.2.44/jiff/tz/enum.OffsetConflict.html).
- **TZ. Zone serialization/equality/name:** [TimeZone docs](https://docs.rs/jiff/0.2.44/jiff/tz/struct.TimeZone.html), [iana_name implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/timezone.rs#L652-L679).
- **TZ-fixed:** [TimeZone::fixed implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/tz/timezone.rs#L454-L465).
- **ZONE-API:** [standalone zone printing](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/mod.rs#L1872-L1960), [printer implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/printer.rs#L464-L503), [standalone parser](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L402-L478).
- **SP. Span:** [public API](https://docs.rs/jiff/0.2.44/jiff/struct.Span.html), [Display/FromStr](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L2994-L3029).
- **SP-print. Span/SignedDuration printer:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/printer.rs#L933-L1092).
- **SP-parse. ISO duration parser:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L1004-L1270).
- **SP-fraction-tests. Fractional/overflow-spill fixtures:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L1495-L1543).
- **DUTIL. Duration field/fraction accumulation:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/util.rs#L60-L490), [fraction distribution](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/util.rs#L801-L1024).
- **SB. Span field bounds:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/util/b.rs#L193-L230).
- **SD:** [SignedDuration](https://docs.rs/jiff/0.2.44/jiff/struct.SignedDuration.html), [Display](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L2465-L2480), [FromStr](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L2594-L2614).
- **SD-tests. Duration bounds/rejections:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/fmt/temporal/parser.rs#L1275-L1368).
- **DATE:** [Date::until](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Date.html#method.until). **DATE-config:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L2470-L2790).
- **TIME:** [Time::until](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Time.html#method.until). **TIME-config:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/time.rs#L2303-L2568).
- **DATETIME:** [DateTime::until](https://docs.rs/jiff/0.2.44/jiff/civil/struct.DateTime.html#method.until). **DATETIME-config:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/datetime.rs#L3020-L3325).
- **TS:** [Timestamp::until](https://docs.rs/jiff/0.2.44/jiff/struct.Timestamp.html#method.until). **TS-config:** [source](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/timestamp.rs#L2957-L3235).
- **ZONED:** [Zoned::until](https://docs.rs/jiff/0.2.44/jiff/struct.Zoned.html#method.until). **ZONED-config:** [source, including stale Day comment and actual Hour selection](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/zoned.rs#L4298-L4565).
- **DUR. Fixed duration differences:** [shared computation helpers](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L2065-L2117), [Date](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Date.html#method.duration_until), [Time](https://docs.rs/jiff/0.2.44/jiff/civil/struct.Time.html#method.duration_until), [DateTime](https://docs.rs/jiff/0.2.44/jiff/civil/struct.DateTime.html#method.duration_until), [Timestamp](https://docs.rs/jiff/0.2.44/jiff/struct.Timestamp.html#method.duration_until), [Zoned](https://docs.rs/jiff/0.2.44/jiff/struct.Zoned.html#method.duration_until).
- **DIFF-impl. Context and rounding order:** [Date](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L1622-L1671), [DateTime](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/datetime.rs#L1994-L2044), [Zoned](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/zoned.rs#L2683-L2743).
- **REL:** [SpanRelativeTo](https://docs.rs/jiff/0.2.44/jiff/struct.SpanRelativeTo.html), [SpanRound::relative](https://docs.rs/jiff/0.2.44/jiff/struct.SpanRound.html#method.relative), [round context checks](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L5354-L5409), [comparison checks](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L4512-L4535).
- **SP-duration:** [Span::to_duration](https://docs.rs/jiff/0.2.44/jiff/struct.Span.html#method.to_duration), [Span::total](https://docs.rs/jiff/0.2.44/jiff/struct.Span.html#method.total). **SP-convert:** [SignedDuration → Span](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L3252-L3325).
- **NUM-TS:** [Timestamp extraction docs/implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/timestamp.rs#L909-L1022). **NUM-SD:** [SignedDuration extraction](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L815-L994).
- **ROUND. Direct round configurations/modes:** [SpanRound](https://docs.rs/jiff/0.2.44/jiff/struct.SpanRound.html), [TimeRound](https://docs.rs/jiff/0.2.44/jiff/civil/struct.TimeRound.html), [DateTimeRound](https://docs.rs/jiff/0.2.44/jiff/civil/struct.DateTimeRound.html), [TimestampRound](https://docs.rs/jiff/0.2.44/jiff/struct.TimestampRound.html), [ZonedRound](https://docs.rs/jiff/0.2.44/jiff/struct.ZonedRound.html), [SignedDurationRound](https://docs.rs/jiff/0.2.44/jiff/struct.SignedDurationRound.html), [OffsetRound](https://docs.rs/jiff/0.2.44/jiff/tz/struct.OffsetRound.html), [RoundMode](https://docs.rs/jiff/0.2.44/jiff/enum.RoundMode.html).

[R3339]: https://www.rfc-editor.org/rfc/rfc3339.html#section-5.6
[R9557]: https://www.rfc-editor.org/rfc/rfc9557.html
