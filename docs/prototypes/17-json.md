# JSON design evidence (ri-1zg)

Read the source examples in [`prototypes/17-json.rill`](../../prototypes/17-json.rill).

Source: [ADR 0018](../adr/0018-owned-precision-preserving-json.md) and the [JSON contract](../json-contracts.md). `17-json.rill` contains 53 intended-valid expectation declarations, including invalid-input checks. Reviewers checked all outcomes by hand. There is no JSON parser or compiler implementation here, and no Rill body or expectation was executed. ri-48g retains the checking, inference and cleanup work; ri-dej retains pinned backend support; ri-1wl retains conversion machinery.

## Active source inventory

| Expectations | Intended result |
| --- | --- |
| 1–13 | Top-level scalars/JSON whitespace, exact numeric spelling beyond f64, minimal escaping, surrogate-pair decoding, noncharacters, FEFF string data and distinct non-normalized names. |
| 14–23 | Reject comments/trailing commas/duplicate names, lone surrogates, invalid numbers, missing value and a second document. |
| 24–26 | Initial BOM at byte 0, duplicate opening quotes at bytes 7/1, trailing comma after é at UTF-8 byte 6 rather than scalar index 5. |
| 27–30 | Replacement retains order; removal/reinsertion appends; mutable value access and ordered borrowed member traversal. |
| 31–32 | Independent deep copy preserves unchanged number tokens/order; a parsed owned tree outlives its local input string. |
| 33–37 | Exact number-token text, no surrounding whitespace/leading zeros, owned construction and token-relative validation position. |
| 38–43 | Inclusive independent input/depth/node/decoded-string/surrogate/token budgets and expected source offsets. |
| 44–46 | Emitted UTF-8 byte budget, exact four-byte output boundary and depth rejection of a programmatically built tree. |
| 47–50 | Default/zero indentation, compact-versus-pretty size pressure and huge indentation rejected by a budget rather than unchecked arithmetic/allocation. |
| 51 | Grammar-only Number construction accepts 1,025 digits while a document token cap of 1,024 rejects it. |
| 52–53 | Typed owned ParseError can be retained under a custom wrapper; all documented defaults match. |

The sameLimit helper compares only test budget enums through explicit patterns. It does not implement semantic equality or hashing for Value/Number. No numeric conversion implementation is embedded in active source; the error display body uses ri-1wl's confirmed language-wide implement spelling.

## Number conversion matrix: semantic obligations for ri-1wl

These are independent hand-checked cases, not calls to a settled converter implementation. The error labels describe meaning. ri-1wl's confirmed decision (comment 6886119679) defines selected-E evidence, requirements, registration, aliases and coherence. ri-48g integrates the JSON implementations' concrete error metadata without caller-injected errors.

| Number token / primitive source | Target or operation | Expected semantics |
| --- | --- | --- |
| `9007199254740993` | i64 | Exact 9007199254740993; no f64 intermediate. |
| `9007199254740993` | f64 | Explicit nearest/ties-even rounding may yield 9007199254740992; original token remains unchanged. |
| `1.0`, `1e2`, `1E+01` | sufficiently wide integer | Exact 1, 100, 10. |
| `1.5`, `-0.5`, `1e-4000` | integer | Non-integral error, not truncation/float underflow first. |
| `-1` | unsigned integer | Range error. |
| `-0`, `-0.00e999999999999999999999` | unsigned integer | Mathematical zero succeeds; integer has no negative-zero distinction. |
| `-0`, `-0.0` | f32/f64 | Preserve negative zero. |
| `1e400` | f64 | Finite token preserved by parsing; explicit extraction rejects overflow to infinity. |
| `1e-4000` | f64 | Explicit extraction may underflow to zero. |
| `0e999999999999999999999999999999` | numeric primitive | Zero without overflowing a native exponent parser. |
| `18446744073709551615` / `18446744073709551616` | u64 | Maximum succeeds / next integer fails range. |
| `340282366920938463463374607431768211455` / next integer | u128 | Maximum succeeds / next integer fails range. |
| `-170141183460469231731687303715884105728` / one less | i128 | Minimum succeeds / next lower integer fails range. |
| Typed integer primitive | Number | Exact decimal, no source width suffix; uniform Result shape even on guaranteed-success cases. |
| Finite `0.1f32` / `0.1f64` | Number | Canonical shortest same-source-width roundtrip, not exact binary-rational expansion or identical cross-width interpretation. |
| Primitive NaN/positive or negative infinity | Number | Non-finite source error, not a valid JSON numeric token. |

JSON integer extraction intentionally differs from ordinary primitive `try_into<u8>(-0.5f64)` under the older ri-a2c contract, which truncates to zero. ri-1wl replaces that draft spelling with tryInto/tryFrom and selected implementation error evidence. JSON defines the conversion semantics. It does not add a compiler exception for conversions or user return-only overload syntax.

## Independent grammar and Unicode vectors

Accept, within configured budgets:

- Numbers `0`, `-0`, `1.0`, `1e0`, `1E+01`, `1e-0002`, huge exact integers/decimals/exponents whose token text fits the profile.
- Scalars `null`, `true`, `false`, strings and numbers at the top level; standard surrounding whitespace only.
- Escaped quote/backslash/slash, b/f/n/r/t controls and four-hex-digit Unicode escapes. Literal U+2028/U+2029, DEL/U+0085 and Unicode noncharacters are valid inside strings.
- Correct pairs such as `"\uD834\uDD1E"` and `"\uDBFF\uDFFF"`; the latter yields a valid noncharacter scalar allowed by this policy.
- Empty names/strings/containers, nested arrays/objects, and distinct `"é"` / `"e\u0301"` names without normalization.

Reject (do not quietly repair/accept a prefix):

- `+1`, `01`, `-01`, `.1`, `1.`, `1e`, `1e+`, hex, separators/suffixes, NaN or Infinity. Strict token construction rejects whitespace as well as invalid syntax.
- Whitespace-only input, `true false`, `{}[]`, comments, trailing commas and uppercase literal spellings. Exact syntax reason reflects the detected failure; no universal lexer-priority promise is added for every malformed numeric prefix.
- Lone/reversed/mismatched surrogates such as `"\uD800"`, `"\uDEAD"`, `"\uDD1E\uD834"`, `"\uD800x"` and `"\uD800\u0041"`.
- Invalid/short escape syntax, `\xNN`, `\u{...}`, `\U...`, `\v`, and raw U+0000-U+001F inside strings.
- U+00A0, U+0085, U+2028, U+FEFF or other Unicode whitespace-like scalars outside strings. Only U+FEFF at view offset zero has the dedicated LeadingBom case.
- Duplicate decoded names, including different escaped spellings. This is an explicit receiver restriction, not a claim that RFC 8259's grammar bans all duplicates/lone surrogate escapes.

Raw malformed UTF-8 (`C0 80`, `ED A0 80`, `F4 90 80 80`, truncated `E2 82`) fails the explicit Bytes-to-text validation before JSON parsing. UTF-16 input is not automatically detected/transcoded. BOM bytes EF BB BF form valid UTF-8 but the resulting leading FEFF is rejected by JSON. No parseBytes or lossy decode is silently inserted.

## Positions, budgets and boundary cases

- `{"a":1,"\u0061":2}` has the duplicate second opening quote at byte 7 and the original at byte 1. Compare decoded content.
- `["é",]` has an unexpected closing bracket at UTF-8 byte 6, not scalar index 5.
- For `"\uD800"`, the invalid scalar sequence identifies its escape starting at byte 1. Incomplete escape syntax may instead identify EOF; bad escape/hex spelling can identify the bad source byte.
- In `"\u12G4"`, the bad G is at byte 5. Runtime error wording is not fixed.
- For `1e`, an EOF-reported failure points to length 2, not a guessed following byte.
- In `{} true`, trailing input begins at byte 3.
- The complete output for `"é"` is four UTF-8 bytes. An output budget of 4 succeeds; 3 fails. An input cap of 2 identifies byte 2 even though it falls inside é's encoding. The supplied complete &string is still valid UTF-8.
- In `"\uD834\uDD1E"`, 12 escape-source bytes decode to four UTF-8 bytes. A decoded-string cap of 3 fails at the first escape, byte 1; cap 4 fits.
- In `[[]]`, root depth is 1 and inner depth is 2. maxDepth 1 fails at byte 1. Root scalar depth is zero.
- `[null]` has two nodes. maxNodes 1 fails at the child start, byte 1. `{"a":null}` also has two nodes, not three; its value starts at byte 5.
- For `123`, maxNumberBytes 2 fails at byte 2. The input cap includes surrounding whitespace. A zero-node cap allows no document; a zero-string cap permits empty decoded names/strings; a zero-depth cap permits scalars only.
- `[null]` compact output is 6 bytes; default pretty output is 10, zero-indent pretty is 8. No terminal newline.
- Root/child trees at cap−1/cap/cap+1 and directly constructed trees require the same depth accounting. Pretty indentation/escaping also counts against the output cap, never just stored source/token size.
- Raise individual budgets without creating a native-stack recursion allowance. Huge flat strings/arrays/tokens exercise resources independently of nesting. Allocation failure stays fatal; no process-memory or CPU/time guarantee follows from logical caps.

Input size is checked first. Other simultaneous failures at one position have no global priority guarantee; tests asserting a particular budget keep other caps nonbinding. Reject rather than truncate. Errors carry no copied source/key excerpt. Error-wrapper construction origins remain source-code locations, not these runtime data positions or an implicit file/view base offset.

## Ownership, privacy and cleanup rejection cases

1. Reject returning parsed string/name/number views as if the whole tree owned them after input release. A real owned Value must detach retained storage. `text(&Number)` and member views remain borrowed even when their owner is borrow-free.
2. Reject mutable access to number text or stored names through private fields, imported aliases, descriptor-slot references or a caller-provided member record. Value.String/insert treat input strings as data, not text to reparse or a way to inject JSON syntax.
3. Reject direct Number construction with malformed text, direct Object record construction or a public raw duplicate-member backing list. The exposed variant payload is an invariant-bearing nominal type, not permission to bypass its constructors.
4. Reject structural mutation while object/member/iterator loans are live and advancing a mutable iterator with outstanding exclusive data. No implicit shared downgrade, disjoint-name/index reasoning or view-to-owned cast is assumed.
5. Explicit deep copy must preserve numbers and member order without sharing mutable backing or dropping obligations. Copying/formatting/disposal of programmatic deep trees and partially constructed parse output still need bounded standard call stacks.
6. On syntax, duplicate, depth or output failure and on cancellation, cleanup must dispose each initialized owned node or worklist slot once. It must respect moved and displaced values. Normal cancellation is not a returned ParseError/FormatError; use existing masking/cleanup rules. A worklist failure is fatal, not a recursive cleanup fallback or returned partial string/tree.
7. Case distinctions are typed error metadata, not reflected field dumps or parsed human messages. Json errors can become retained causes because their owned scalar/enum diagnostics establish the required properties; generic outer error types do not erase unknown borrowed causes.
8. No standard Value/Number equality, hashing, arithmetic, normalization, implicit stringify/Value show, or string-to-number coercion is inferred. Handle a fallible formatting Result before logging; do not swallow a limit or emit a placeholder.
9. Do not supply conversion registration/declaration/where/orphan/alias/error-injection rules here. ri-1wl's confirmed decision owns them; active JSON examples intentionally do not implement numeric converters. ri-48g must integrate both decisions before implementation/RFC readiness.
10. Whole-document JSON is not a stream framing, transport/body limit, HTML embedding, schema/record-mapping or database expression feature. ri-fnk/#39/#40 and application adapters compose this boundary explicitly.

## Validation status

The intended outcomes come from inspection and reference facts, not measured parser results. Validation passed for whitespace, local links, Markdown fences, source delimiters, explicit widths and the 53-expectation inventory. Reviewers inspected thirteen positive literal/encoding cases using Python's JSON reference parser with number-token preservation. They also checked independent UTF-8, offset, size and integer-boundary arithmetic. A read-only consistency review found no actionable blockers or obvious intended-valid source mistakes. The external parser is not an oracle for Rill's duplicate/surrogate/BOM/limit policy; none of these checks executes the 53 Rill expectations or validates a Rill implementation.

ri-48g still owes complete grammar/inference/metadata/privacy/callee-exit/transitive-reference and cancellation/cleanup algorithms plus adversarial accepted/rejected cases; ri-dej owns backend mechanisms, and ri-1wl owns conversions. If an accepted public operation cannot be specified or checked consistently, return the design blocker to the map rather than declaring these fixtures a safety proof.
