# Jiff checked arithmetic — final fact audit for Rill #33

**Status:** primary-source research, not a Rill specification or implementation/backend selection.

**Pin:** Jiff **0.2.44**, commit **`95f343d3f137e5596288d862f4a04dc3be52b07b`**. Context: [API/model note](0033-jiff-api-model.md) and [wire/differences note](0033-jiff-wire-and-differences.md). All upstream source links below use this commit; public API links use this version.

**Approved direction supplied for this audit:** a coherent Jiff core adapted to checked Rill functions; no implicit saturation/wrapping; explicit standalone-calendar context; one-sign Span with explicit sign and magnitudes; whole-input, type-specific no-discard parsing; exact offsets. These directions do **not** by themselves decide whether Date arithmetic may discard a duration remainder. No remaining Rill policy is selected here.

**Method:** static reading of the pinned official repository archive. The seven arithmetic source files (`date.rs`, `time.rs`, `datetime.rs`, `timestamp.rs`, `zoned.rs`, `span.rs`, `signed_duration.rs`) were also SHA-256 matched against official raw files at the commit. Examples described as deductions are calculations from source, not executed Jiff/Rill observations. No Rill implementation, build, test, backend or clock experiment was run.

## 1. Checked add/sub boundaries by receiver

These datetime `checked_add`/`checked_sub` functions return `Result<value, Error>`. They accept Span and SignedDuration (also Rust's unsigned Duration, which is not a required Rill type). Subtraction negates the input through an internal duration adapter, then follows addition; the adapter can temporarily represent an unnegatable signed duration as unsigned, but conversion/application can still fail. It is not saturation. [D, T, DT, TS, Z, ADAPTER]

| Receiver | Span boundary | SignedDuration boundary |
| --- | --- | --- |
| **Date** | All ten units accepted. Years/months constrain the target day; weeks/days advance the date; aggregated time fields contribute only **whole 24-hour days**. The remaining time is discarded. | Only whole 24-hour days contribute; remaining seconds/nanoseconds are discarded, toward zero for negatives. Date range is checked. |
| **Time** | Only nonzero Hour and smaller units. Any nonzero Year/Month/Week/Day errors. The resulting clock must remain in `00:00:00..=23:59:59.999999999`; crossing midnight errors. | Add/subtract the **entire** elapsed quantity; no remainder discard or midnight wrap. Out-of-day result errors. |
| **DateTime** | All ten units. Civil calendar arithmetic plus full subday arithmetic; clock overflow/underflow carries into the date using 24-hour days. | Full fixed elapsed addition on a civil 24-hour-day scale, retaining nanoseconds. DateTime overflow errors. |
| **Timestamp** | Hour and smaller only. Nonzero Day/Week/Month/Year errors, **even though it prints in UTC**. Use Zoned for calendar arithmetic; 24 hours is permitted where one day is not. | Full exact elapsed addition, subject to Timestamp's narrower range, not the whole SignedDuration range. |
| **Zoned** | All ten units. Calendar portion first on local fields, compatible zone resolution, then fixed Hour-and-smaller portion on the timestamp. | Entire operation is timestamp arithmetic; retain the same zone rules for the resulting representation. |

Sources: implementations and public method prose. [D, T, DT, TS, Z] Restrictions concern **nonzero stored fields**, not whether construction once mentioned a zero field. A zero Span is an identity for these receivers. A checked result does not imply that every input component was preserved: Date is the important exception.

### Date: exact loss and no-op cases

For an already-valid Date `d`, the source computes:

- With a fixed duration `q`: `whole_days = q.as_secs() / 86400`, integer division **toward zero**. Fractional nanoseconds cannot contribute another whole day because SignedDuration's components have consistent signs. Add `whole_days`; discard the remainder. [D, DAYS]
- With Span: apply calendar fields as in §2, then convert the **sum of all Hour-and-smaller fields** to a duration and use the same whole-day extraction. It does not discard each field separately: `23h + 60m` contributes one day. The clock portion's leftover is discarded, not carried into any hidden clock. [D, DAYS, SP-FIXED]

**Deductions for either equivalent time-only Span or SignedDuration:** `d + 23h = d`; `d + 24h = d + 1 day`; `d + 47h = d + 1 day`; `d + (−23h) = d`; `d + (−47h) = d − 1 day`. Likewise ±1ns is a no-op. Exactly 24h counts: prose saying time that does not “exceed” a day is ignored must not be read as excluding the equality case. `Date::MAX + 23h` and `Date::MIN − 1ns` succeed unchanged, whereas adding/subtracting a whole day past the corresponding endpoint errors. [D, DAYS]

This is **not** equivalent to inserting midnight, doing full DateTime arithmetic, then extracting the date: at midnight, −1ns would move into the preceding date. Nor does an explicit one-sign Span preserve any discarded remainder in the result. Pure time remainders may be lost even when calendar fields also change the date. [D, DT]

Jiff's documented reversibility for day-or-smaller Date arithmetic is not a losslessness guarantee: applying a truncated increment and its opposite can restore the input while both operations discard the remainder. The approved no-discard **parse** policy does not choose whether Rill Date arithmetic retains Jiff truncation, rejects a nonzero remainder, or requires an explicit conversion/operation for it. That is a human semantic decision, not an algorithm/backend question.

### Time: checked is not the operator behavior

`Time::checked_add(Span)` first rejects nonzero calendar fields, computes the full time sum, and errors if there is a day carry/borrow. Fixed-duration checked addition uses the exact nanosecond quantity and rejects out-of-day results. Consequently neither `Time + 1 day` via the **checked** API nor `Time + 24 elapsed hours` succeeds. [T]

Jiff's ordinary `Time +/− Span` **and** `Time +/− SignedDuration` operators instead call wrapping functions. The Span wrapping path simply ignores Year/Month/Week/Day; pure calendar spans are no-ops. Time overflow wraps around the clock. This is upstream operator behavior, not the approved Rill checked boundary. Other datetime addition operators generally unwrap their checked operation and panic on failure; there is no universal Jiff operator rule. [T-WRAP, T-OPS, D, DT, TS, Z]

**Additional source qualification:** large-Span wrapping multiplies/adds in wrapping `i64` **before** reducing modulo nanoseconds/day. SignedDuration wrapping computes in `i128`. Thus Span wrapping is not uniformly equivalent to exact elapsed-duration modulo-day arithmetic. For example, time 1ns plus a Span of `i64::MAX` nanoseconds makes the intermediate `i64` sum `−2^63`, not `+2^63`; these have different remainders modulo a day because `2^64` is not a multiple of a day's nanoseconds. This is a static source deduction and a caveat to the broad wrapping prose, not a checked-arithmetic loss or a Rill wrapping proposal. [T-WRAP]

## 2. Composite calendar order and end-of-month constraint count

### Date

The general `Date::checked_add_span` path is precise, not ten sequential public additions: [D, MONTH]

1. Add the signed Month field to the original month, producing a normalized month and a year carry using Euclidean division.
2. Add that carry and the signed Year field to the original year, with bounds checks.
3. Construct the date using the original day, **constraining it once** to the last day of the **combined target year/month**. It does **not** constrain after years and then again after months, nor once per individual month.
4. Add `7 × weeks`, then days to the epoch-day position, with range checks.
5. Add whole 24-hour days extracted from aggregated time fields; discard their remainder as in §1.

There is one day-of-month constraint operation per nontrivial composite general-path addition, not necessarily one actual change of day. Zero/day-only fast paths need no such constraint. Weeks/days/time-day advances do not each reapply an end-of-month clamp. Subtraction uses the same order with the Span sign flipped. [D]

**Deduced distinguishing cases:** `2020-02-29 + {1 year, 1 month}` → `2021-03-29`; two calls, first +1 year and then +1 month, give `2021-03-28`. `2024-03-31 + 2 months` → May 31, while two +1-month calls give May 30 (the former is also an upstream documented example). These are intentional calendar semantics/nonassociativity, not overflow or rounding. [D]

### DateTime

For a composite calendar+time Span the implementation splits the Span. It **computes** the clock sum/carry first, then adds the calendar portion to the original date, then adds the time carry to that calendar result. Therefore the semantic date order is combined years/months with one constraint → weeks/days → clock day carry. It does not first shift the original date by the clock carry and then perform month arithmetic. The full clock remainder is retained. Negative clock sums use Euclidean division so the clock stays nonnegative and the date receives the borrow. [DT, T-CARRY]

A useful deduced discriminator is `2021-01-30T23:00 + {1 month, 2 hours}` → `2021-03-01T01:00`; clock-carry-first **date mutation** would instead give February 28 at 01:00. This is not the Date arithmetic remainder rule: DateTime retains all elapsed nanoseconds. [DT]

### Zoned

The calendar portion is Year through Day. Add that entire portion to the local DateTime (hence the combined-year/month single constraint above), then resolve the resulting civil datetime in the **retained zone** with **Compatible** handling; only then add Hour through Nanosecond on its Timestamp. If there are no calendar fields, directly use timestamp arithmetic, avoiding re-resolution of an already-selected fold occurrence. Fixed SignedDuration always takes that direct path. [Z]

Calendar days can span 23/25 hours or other transition lengths; checked does not mean “reject gaps/folds.” Compatible means earlier occurrence in a fold and forward shift in a gap, not an error. The source performs one calendar-result zone resolution here, not separate resolution after every year/month/week/day component. Range or resolution errors can still occur. These facts do not select a different Rill ambiguity policy, nor license reordering calendar and fixed portions. [Z, DISAMBIG]

## 3. Standalone Span checked addition/subtraction

This is **not fieldwise vector addition**, and opposite signs between operands are legal. Each operand and the result individually still has one sign. [SP-ADD, SP-SIGN]

### Context and actual algorithm

- Let `largest` be the largest nonzero unit in either Span. With no context, Hour and smaller use invariant arithmetic; **any nonzero Day-or-larger field in either operand requires context before cancellation**. Thus `1 day + (−1 day)` and `1 month + zero` still error without context. No “same units” or “zero final answer” exemption is implemented. [SP-ADD]
- Explicit `days_are_24_hours` permits days and seven-day weeks, **not months/years**. A Date reference means a civil midnight DateTime, not Date's lossy arithmetic; a DateTime reference means civil 24-hour days; a Zoned reference incorporates transitions. [SP-REL, SP-ADD]
- Invariant case: convert both operands to exact fixed durations, checked-add them, then decompose into a single-sign Span capped at `largest`. No automatic promotion above that cap: `90 minutes + 30 minutes` yields 120 minutes, not 2 hours. A provided reference is ignored when `largest` is invariant. [SP-ADD, SP-FIXED, SP-REL]
- Real relative context: compute **`mid = start + left; end = mid + right; result = start.until(end, largest)`**. For Span ± SignedDuration, apply the left Span, then the full fixed duration, then take that difference; its output cap is the left Span's largest unit (a duration is treated as Nanosecond for this purpose). Subtraction follows the same algorithm after negating the right operand through the duration adapter. [SP-ADD, ADAPTER]

The relative algorithm is ordered and may be noncommutative; it does not merge component fields before applying them. For example, from `2021-01-30`, +1 month then +1 day ends on March 1, while +1 day then +1 month ends on February 28. This is a deduced distinction between **Span arithmetic with a reference** and applying **one combined Span**. [SP-ADD, D]

### Opposite signs, balancing and errors

Invariant arithmetic determines the result's net sign and redistributes magnitudes: `1 hour + (−90 minutes)` → −30 minutes; cancellation → zero. Relative arithmetic determines the endpoint first and obtains a one-sign difference from the start. A hypothetical intermediate fieldwise result such as `+1 hour, −90 minutes` is **not** a stored result and is not itself a reason to reject the operation. Input construction is separate: signed Jiff builder fields change the whole Span sign; Rill's approved explicit sign+magnitudes avoids treating those builders as mixed-sign algebra. [SP-ADD, SP-FIXED, SP-SIGN]

Errors include missing/inadequate context, unrepresentable fixed sum or resulting Span field, intermediate/final datetime overflow, zone resolution/representation failure, and failure to represent the requested largest-unit difference. Rebalancing means this is **not** simply “reject if the raw sum of corresponding fields exceeds a field bound.” `i64::MAX nanoseconds + 1 nanosecond` errors because the cap stays Nanosecond; choosing operands whose largest unit is Second can instead represent such a total distributed across fields. Conversely, calendar cancellation does not rescue an out-of-range **intermediate** application. [SP-ADD, SP-FIXED]

**Implementation-specific civil context limit:** Jiff's private `RelativeCivil` converts the reference and intermediate/final civil datetimes to UTC Timestamps as well. A valid civil DateTime near an endpoint can therefore fail in standalone calendar Span arithmetic even where direct civil arithmetic would fit. This is a real pinned source boundary, not a need for tzdb or wall-clock access and not proof that Rill must narrow its civil context range. [SP-CIVIL]

### Span negation/absolute value

All Span fields have symmetric permitted signed ranges, including Nanosecond **`−i64::MAX..=i64::MAX`**, not `i64::MIN`. Stored magnitudes plus a separate sign allow `Span::negate` and `Span::abs` to be infallible for **every valid Span**; zero remains zero. There is no valid minimum Span whose negation/absolute value overflows. Passing `i64::MIN` as a nanosecond field is instead a construction-range failure (fallible builder errors; convenience builder panics). Do not import SignedDuration's asymmetric-min rule into Span. [SP-SIGN, SP-BOUNDS]

## 4. SignedDuration: exact fixed arithmetic, with a subtraction caveat

SignedDuration is a normalized, sign-consistent seconds/fraction pair with nanosecond resolution. Range is `(i64::MIN, −999999999ns)` through `(i64::MAX, +999999999ns)`. Integer `checked_add` normalizes carries and cross-component signs without float conversion, rounding, calendar interpretation, wrapping or saturation; returns `Option`, `None` on overflow. Success preserves the entire exact sum. [SD-ADD, SD-API]

**Important pinned subtraction discrepancy:** public `checked_sub` prose says subtract, returning None if overflow occurs. The implementation is **`rhs.checked_neg()?; self.checked_add(negated_rhs)`**. It rejects **every** right operand whose seconds field is `i64::MIN`, not only situations where the mathematical difference is out of range. In particular, `MIN.checked_sub(MIN)` returns **None**, although the exact result zero fits. `SignedDuration::new(i64::MIN, 0)` subtracted from itself has the same problem. These are static deductions, not executed probes. This is an intermediate-negation restriction beyond result-range overflow; do not describe pinned checked subtraction as exact-and-complete over all representable differences. [SD-SUB, SD-NEG]

- `checked_neg` returns None whenever **seconds == `i64::MIN`**, including fractional values above the single absolute MIN. Every such value's positive magnitude really is outside SignedDuration's range. [SD-NEG]
- `abs` is **not checked**; its documented panic boundary is the same entire `i64::MIN`-seconds band. `unsigned_abs` returns Rust's unsigned Duration and can represent every magnitude, including MIN. The checked Rill surface cannot simply expose upstream `abs` as universally safe. [SD-NEG]
- Integer `checked_mul(i32)` preserves an exact nanosecond product if representable. `checked_div(i32)` rejects zero and the `i64::MIN`-seconds / −1 overflow band, but **truncates any sub-nanosecond remainder toward zero**: 1ns / 2 → zero. “Checked” division is not an exact-divisibility check. Floats, rounding and explicit saturating methods are separate operations, not implicit parts of integer addition/subtraction. [SD-MULDIV]

The asymmetric minimum is a genuine representation limit; rejecting `MIN − MIN` is an implementation consequence of how subtraction is written. Whether a Rill contract promises exact subtraction whenever the final result fits, or explicitly inherits this extra failure boundary, must be made explicit rather than hidden behind “copy Jiff.” This note selects neither. #28 can formalize the chosen arithmetic contract; #30 cannot decide it by choosing a backend.

## 5. Basic civil queries really present upstream

This is an availability audit, **not** a proposal requiring every helper in Rill or selecting names/signatures. These are pure queries on existing values, not environment/clock reads. [Q-DATE, Q-TIME, Q-DT, Q-ISO, Q-WEEKDAY]

| Query | Actual pinned Jiff helpers and qualification |
| --- | --- |
| Civil fields | Date: `year`, `month`, `day` (also `era_year`). Time: `hour`, `minute`, `second`, `subsec_nanosecond`. DateTime exposes the corresponding date/time fields and `date()`/`time()`. |
| Fraction components | Time/DateTime `millisecond`, `microsecond`, `nanosecond` are **individual base-1000 components**, each 0–999; `subsec_nanosecond` is the **whole fraction**, 0–999999999. Do not mistake `.nanosecond()` for the latter. |
| Weekday | `Date::weekday` / `DateTime::weekday` → Weekday; explicit Monday/Sunday-based zero/one-offset numeric conversions exist. |
| Leap and lengths | `in_leap_year`, `days_in_month`, `days_in_year` on Date/DateTime. |
| Ordinal | `day_of_year` is **1-based**, 1–365/366. `day_of_year_no_leap` is a distinct Option query: February 29 → None, subsequent dates use the 365-day numbering. |
| ISO week | `iso_week_date()` → ISOWeekDate; that value has `year`, `week` (1–53), `weekday`, `weeks_in_year` (52/53), `in_long_year`, and conversion back with `date()`. ISO week-year need not equal civil year: upstream example `2019-12-30` → week 1 of **2020**. |

## 6. What remains human policy, versus follow-through work

**The narrow arithmetic choices exposed here:**

1. **Date remainder treatment:** faithfully retain Jiff whole-day truncation/no-ops, reject nonzero subday remainders, or require an explicit lossy step/operation. Specify equivalent Span and fixed-duration behavior. Neither parse strictness nor checked overflow already answers this.
2. **Source/prose edge fidelity:** decide the contract for SignedDuration subtraction's representable-result/failed-negation cases and, if integer division is included, fractional-nanosecond truncation versus exact-divisibility failure. Do not accidentally inherit those from an implementation shortcut or the word “checked.” Also record whether Jiff's private UTC-Timestamp civil-reference limitation is part of the intended contract or merely an upstream implementation artifact.
3. **Any still-unconfirmed defaults:** the precise once-constrained composite calendar order and Compatible zoned arithmetic above must be recorded as upstream facts. If the coherent-core approval did not already include those defaults, confirm them; do not silently replace them with per-unit clamping or reject-all ambiguity. Earlier wire/snapshot/grammar choices remain in the prior note, not reopened or resolved here.

**Not new human API requirements:** naming/shape for a minimal subset of the available pure query helpers. Availability does not force broad convenience parity.

**#28 — algorithm formalization:** once policy is explicit, specify normalized representations, exact checked-success domains, composite application order, invariants and adversarial/property cases (including negative truncation, endpoints, one-sign balancing and intermediate failure). That is algorithm/spec work, not a reason to ask a human to pick algorithms already determined by a semantic contract.

**#30 — backend:** feasibility, representation, linkage, host/zone-data integration and validation remain separate. No dependency, FFI, port, backend or implementation was chosen, and no upstream source fact proves Rill runtime support. This arithmetic audit does not settle the separate monotonic-clock/deadline/wait/cancellation policies in #33.

## Primary-source references

- **D:** [Date checked add/sub prose and implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L1234-L1429).
- **DAYS:** [whole civil day extraction and remainders](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L2372-L2409).
- **MONTH:** [month normalization](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L3416-L3434), [constrained construction](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L252-L262).
- **T:** [Time checked add/sub](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/time.rs#L805-L991).
- **T-CARRY:** [Time overflowing addition used internally by checked arithmetic](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/time.rs#L1071-L1153).
- **T-WRAP:** [Time wrapping docs and distinct Span/fixed implementations](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/time.rs#L605-L802).
- **T-OPS:** [Time operators](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/time.rs#L1877-L2023).
- **DT:** [DateTime checked add/sub](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/datetime.rs#L1573-L1784).
- **TS:** [Timestamp checked add/sub](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/timestamp.rs#L1324-L1545).
- **Z:** [Zoned arithmetic public contract](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/zoned.rs#L2050-L2279), [calendar/fixed sequencing and subtraction](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/zoned.rs#L2294-L2401).
- **DISAMBIG:** [Disambiguation public API](https://docs.rs/jiff/0.2.44/jiff/tz/enum.Disambiguation.html).
- **ADAPTER:** [internal duration conversion/negation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/duration.rs#L17-L103).
- **SP-ADD:** [Span checked addition/subtraction prose and algorithms](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L1566-L1880).
- **SP-FIXED:** [fixed-duration decomposition into capped Span](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L2666-L2731), [invariant Span duration accumulation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L2408-L2489).
- **SP-REL:** [SpanArithmetic context](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L4097-L4213), [context selection/marker restrictions](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L5684-L5710), [Date to midnight-relative conversion](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L5718-L5735).
- **SP-CIVIL:** [RelativeCivil construction, application and difference](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L6155-L6234).
- **SP-SIGN:** [one-sign model and builder example](https://docs.rs/jiff/0.2.44/jiff/struct.Span.html), [magnitude/sign representation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L708-L726), [abs/negate](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/span.rs#L1308-L1363).
- **SP-BOUNDS:** [symmetric field bounds and nanosecond minimum](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/util/b.rs#L193-L230).
- **SD-API:** [SignedDuration API/range](https://docs.rs/jiff/0.2.44/jiff/struct.SignedDuration.html).
- **SD-ADD:** [checked add/normalization](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L1017-L1093).
- **SD-SUB:** [checked subtraction prose and actual negation-first implementation](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L1122-L1149).
- **SD-NEG:** [abs, unsigned_abs and checked_neg](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L1903-L1988).
- **SD-MULDIV:** [checked integer multiplication/division](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/signed_duration.rs#L1177-L1283).
- **Q-DATE:** [Date fields, weekday, ordinal, lengths and leap query](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L404-L703), [ISO week conversion](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/date.rs#L995-L1037).
- **Q-TIME:** [Time fields and fractional components](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/time.rs#L386-L540).
- **Q-DT:** [DateTime fields and date queries](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/datetime.rs#L450-L943), [date/time/ISO week extraction](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/datetime.rs#L1228-L1317).
- **Q-ISO:** [ISO week fields](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/iso_week_date.rs#L263-L331), [week count and long-year query](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/iso_week_date.rs#L525-L570), [conversion to Date](https://docs.rs/jiff/0.2.44/jiff/civil/struct.ISOWeekDate.html#method.date).
- **Q-WEEKDAY:** [explicit weekday numbering schemes](https://github.com/BurntSushi/jiff/blob/95f343d3f137e5596288d862f4a04dc3be52b07b/crates/jiff/src/civil/weekday.rs#L221-L295).
