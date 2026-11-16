# Temporal values. #33 design evidence

Read the source examples in [`prototypes/23-time-values.rill`](../../prototypes/23-time-values.rill).

[Contract](../time-contracts.md), [clock/token contract](../clock-cancellation-contracts.md), [ADR 0022](../adr/0022-jiff-time-and-explicit-cancellation.md). `23-time-values.rill` has **39 hand-checked intended-valid expectations**, including invalid-data checks. No Rill parser/compiler/library execution occurred. The imports name proposed library operations, not existing implementations. #28 supplies exact algorithms/checker evidence and #30 actual host/backend support.

## Active inventory

| Cases | Intended facts |
| --- | --- |
| 1–9 | Valid leap date, invalid fields, negative-year spelling, accepted space/comma versus canonical T/dot, no padding/shorthand/leap second/extra target fields. |
| 10–13 | Offset normalization to UTC; exact negative epoch quantity; negative-zero and discarded-zone rejection. |
| 14–17 | Unit-preserving larger Span fields, no days in fixed-duration grammar, subsecond text normalization without mutating the original value. |
| 18–22 | MIN−MIN succeeds, exact division rejects a remainder, named truncation/extraction is toward zero. |
| 23–28 | Whole-day Date arithmetic, rejected remainder, aggregate 23h+60m, one composite year/month clamp, direct HalfExpand and scalar copy. |
| 29–33 | New York spring calendar day is 23 elapsed hours, gap Compatible versus Reject, exact one-second fixed-zone text, explicit retained-zone copy. |
| 34–39 | Exact versus approximate totals, context before calendar cancellation, independent text budgets, ISO week-year differs from civil year. |

Zone examples assume a compatible pinned dataset with the documented 2024 New York rules. No process zone, live environment, source-owned deadline or clock read is used. The local database owner dies before returned namedZone values: real implementation must retain immutable data, not a private view. Owned text is explicitly formatted before comparisons; no temporal show/equality fallback is assumed.

## Independent lexical and precision vectors

- Date -000001-01-01, 0000-02-29 and 9999-12-31; reject -000000, short signed years, +010000 and impossible month/day combinations. +002024 is accepted and canonicalizes to 2024; it does not expand the year range.
- Full time through .999999999; reject 24:00:00, second 60, empty/ten-digit fraction, compact/omitted clock fields, tabs/multiple separator spaces, padding and trailing text. Fraction .000000001 is exact, not rounded through a double.
- Timestamp requires offset and rejects a bracket zone. Civil targets reject offsets/zones instead of parsing and throwing fields away. Zoned requires full date/time and one zone, with optional outer offset resolved under explicit defaults/options.
- Exact offset ±25:59:59; fractional offsets reject, including values that Jiff would round. Canonical second-offset fixed text retains the instant rather than printing rounded minute brackets. -00:00 and -00:00:00 reject; +00:00 has normal matching; Z[America/New_York] takes the distinct exact-instant path.
- Historical Monrovia -00:44:30 stays exact; rounded -00:45 fails default matching. Alternative conflict policy must be explicit. No universal RFC3339/9557 acceptance claim.
- Single !zone annotation recognized; key=value, extra zone and unsupported annotations reject, critical or noncritical. No silently discarded calendar tag.
- Span P1M versus PT1M, PT1H60M versus PT2H; +/- zero canonical PT0S. Last-unit fractions such as PT1.5H expand exactly. Reject fractions on calendar units, repeated/out-of-order fields and following components after a fractional unit, even zero. SignedDuration P0D rejects.
- Span printed second/subsecond aggregation may need checked spill into lower fields. Fieldwise preservation is not promised; 1000ms → PT1S. No implicit normalization mutates the original Span or its explicit copy.
- Exact Timestamp bounds -377705023201000000000..253402207200999999999 ns; SignedDuration bounds -9223372036854775808999999999..9223372036854775807999999999 ns. Endpoint±1 checks must not confuse full i128 representation with allowed value range.
- Integer unit constructors scale/check exactly; -1ns whole-second Trunc is zero, Floor is a separately selected operation. Approximate totals explicitly return f64; exact totals and parsing never pass through them.

## Arithmetic and contextual cases

- Date +23h/-23h errors, +24h gives one day, +23h+60m gives one day. DateTime retains the remainder; Time cannot wrap midnight; Timestamp rejects a nonzero calendar Day even in UTC.
- 2020-02-29 + composite 1y1mo →2021-03-29, versus sequential +1y then +1mo →2021-03-28. One clamp on combined year/month, then weeks/days/time carry.
- 2021-01-30T23:00 + composite 1mo2h →2021-03-01T01:00. Do not mutate the original date by the clock carry first.
- Relative Span addition applies left then right, then takes a difference. Order can matter. 1h plus -90m balances to one-sign -30m; raw mixed-sign fields are not stored. Calendar operands need context even if they cancel; explicit fixed-day mode does not define months/years.
- Pure civil relative arithmetic near endpoint retains civil range, not a hidden UTC Timestamp inset. Actual intermediate/final overflow and requested Span-field overflow still fail.
- Fixed-duration subtraction succeeds whenever exact final result fits; minimum negation/abs can genuinely fail. Exact division rejects 1ns/2; explicit truncation returns zero. Check zero divisor and MIN/-1.
- Default Span differences: Date Day/Day, Time Hour/ns, DateTime Day/ns, Timestamp Second/ns, Zoned Hour/ns. Configured calendar Zoned difference checks actual equal zones before an early zero return. Fixed-duration differences are direct, including civil 24h assumptions, never inferred midnight wrap.
- Direct HalfExpand differs from difference Trunc. Test negative/directed modes before negating the final answer; unsupported units/increments/order reject. Zoned day rounding uses actual day boundaries/length, not fixed epoch 86400 division.
- Civil fields, one-based ordinal and ISO-week-year queries are pure; a Weekday enum avoids silently choosing 0/1 or Sunday/Monday numeric conventions.

## Ownership, source and resource rejection inventory

1. Reject private Date/Time/Timestamp/Zone raw record construction, writable invariant fields, name mutation, unsafe casts and source aliases treated as privacy privileges.
2. Reject treating an owned zone as implicitly copied. copy(&zone)/copy(&zoned) explicitly retain immutable rules; a borrowed identifier/zone view remains dependent on its owner. Reset cannot invalidate live zones.
3. Reject canonical Span eq/hash without explicit fieldwise/context semantics. Zoned eq/hash is instant-only; testing different representations requires an explicit operation.
4. Reject implicit numeric/float/string conversions, bare return-selected parse overloads, calendar-to-timeout conversion or automatic JSON/SQL mapping. #41 does not authorize them by name alone.
5. Query names select indexed ASCII-case-insensitive names, preserve aliases, and reject path escapes. Bundled/system factories do not authorize arbitrary root/TZif/POSIX input. OS source/symlinks are not sandbox proof.
6. Exercise each DB cap at below/equal/above values, zero, aliases sharing bytes, concurrent same-name loads, full-cache hits/new-entry failures, no negative-cache growth, partial parsing, invalid counts and recurrence expansion. Budget source bytes are not exact heap usage.
7. Reset racing old loads cannot publish old results into the new generation. Old snapshots/copies and provenance survive. A named string plus matching current offset does not prove future rules stayed the same.
8. Q92: explicit Inputs controls TZ; missing permits host discovery, malformed/non-UTF8/unsupported present values error safely. Never call live getenv or assume rill test inherited an environment. Inputs/test rules remain #36's; actual host-zone calls are not automatically fake/deterministic.
9. Text cap includes full supplied UTF-8 and emitted bytes; no implicit trim/partial string. Input size check first, UTF-8 errors before JSON/time boundary if callers start from Bytes. Error offsets are bytes, not graphemes/file bases/Err origins.
10. Resource/native failures retain data until actual completion/confirmed abort. Partial input/data/copies/cache worklists and canceled operations dispose initialized state once with bounded standard call stacks, including raised limits and live old generations. Fatal allocation is not a typed recovery promise.

## Validation boundary

Performed validation: local links/Markdown fences/whitespace, source delimiter/explicit-width/count checks and independent integer/calendar/text/2024-zone reference facts passed. Read-only consistency review found no blocker; its missing ZonedParseOptions module-home note was fixed explicitly in std::time::fmt. These are artifact checks, not execution of 39 expectations. External Python date/zone/decimal helpers, if used, cannot prove Rill's deliberately different range/grammar/error/cleanup contracts. #28 still owes exact public-family inference/evidence, private metadata, transitive views/callee exits and algorithms; #30 real provider/ABI/Wasm behavior. Do not claim a native date/time library or a successful timezone/security audit.
