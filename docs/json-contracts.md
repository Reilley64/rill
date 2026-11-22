# Rill v0 JSON contracts

This is confirmed design input for ri-1zg, summarized by [ADR 0018](adr/0018-owned-precision-preserving-json.md). The user explicitly confirmed the consolidated policy after the interview. This is **not** a parser implementation, executed compiler/library test suite, accepted feature RFC, or checker/backend proof. ri-48g integrates and validates the RFCs; ri-dej remains a backend gate.

The public module is `std::encoding::json`. Reuse the [core library contracts](standard-library-contracts.md), nominal enums/records, explicit borrowing/copying and origin/explicit-cause error model. JSON is transport-independent. Streaming I/O (ri-fnk), HTTP (ri-n5i), TLS/networking and database mapping are consumers/separate designs, not parsing prerequisites. No reflection, derive system, serialization traits, public dyn facility, schema mapping, JSONPath, streaming events or JSON Lines is introduced.

Canonical conversion machinery is separately confirmed in ri-1wl (comment 6886119679), including into/from, tryInto/tryFrom, selected implementation error evidence and explicit implement syntax. This document specifies JSON number semantics. It does not change registration, generic ordering, aliases, endpoint permissions, coherence or protected primitive pairs. ri-1wl's later decision controls over older closed-conversion/snake-case examples; its separate working-tree migration is not duplicated here. ri-48g must integrate both confirmed inputs and the JSON implementations' concrete error metadata; neither design is executed or proven by these fixtures.

## 1. Owned values and invariants

```rill
export enum Value {
  Null,
  Bool(bool),
  Number(Number),
  String(string),
  Array(List<Value>),
  Object(Object),
}
```

Number and Object are exported nominal private-field types, not source structural records or new language primitives. List is canonical std::collection::List; importing Value does not inject List or other helper bindings. Arrays are ordered owned Lists. Strings and names own valid UTF-8; all retained number/object storage is independently owned. Parsing borrows input only during the operation and returns no source views.

Number owns a validated JSON number token. Object owns unique names and values with insertion order. Public Value variants expose those types, **not** their private storage. No writable number-text or stored-name view is exported. A valid finite owned tree has no injected callbacks, arbitrary user variants, borrowed input or implicit counted sharing. Existing recursive property rules establish noBorrow/sendable/shareable from the contained values and justified library metadata; raw internal pointers are not permission to bypass those rules.

The copy family supplies `copy(&Number):Number`, `copy(&Object):Object`, and `copy(&Value):Value`, creating independent owned storage while preserving numeric spelling and object order. No general Value/Number/Object implicit-copy or persistent snapshot is added. Normal scalar/unit-variant eligibility remains. Read-only views retain their source dependencies; exclusive views retain normal restrictions through last use.

All standard parse/format/copy/dispose paths, partial-tree/error cleanup and displaced-value destruction use bounded call stacks per ri-636. Loops/worklists may grow on the heap; allocation failure remains fatal. Parse depth caps do not protect directly constructed trees by themselves. No recursive cleanup fallback, unsafe reference cast, total-memory quota or bounded completion latency is promised.

## 2. Public API

These are interface signatures, not complete source files or implementations. Opaque type layouts are deliberately unspecified. One body-less declaration owns an ordinary overload family; repeated shapes below list its supplied implementations, not duplicate declarations or return-only overloads. Family bodies use ri-1wl's explicit implement syntax.

| Area | Required public operations/types |
| --- | --- |
| Values | Value, Number, Object; canonical copy implementations |
| Number text | number, text, NumberError |
| Objects | Object type/factory, length, get, getMut, insert, remove |
| Traversal | Member, MemberRef, MemberMut, OwnedObjectIter, ObjectIter, ObjectIterMut; canonical iterate/next implementations |
| Parse | parse, parseWithLimits, ParseLimits, defaultParseLimits |
| Format | stringify, stringifyWithLimits, pretty, prettyWithOptions, FormatLimits, PrettyOptions, defaultFormatLimits, defaultPrettyOptions |
| Diagnostics | ParseError, SyntaxReason, ParseLimit, NumberError, FormatError; canonical display implementations |

All source-level names need imports. Module-local length/get/map-like names do not merge with same-spelled std families. JSON contributes implementations to canonical copy/display/iterate/next under ordinary identity/orphan rules; metadata access does not grant source privacy privileges.

### Number text

```rill
function number<S>(token: S): Result<Number, NumberError>;
// Supplied input patterns: string and &string.
// Ordinary single implementation:
text(value: &Number): &string
```

The owned form consumes text; the borrowed form copies validated text into independent ownership. Both validate exactly one numeric token, with no whitespace before or after it. A token is ASCII:

```text
[minus] (0 | nonzero-digit *digit) [ . 1*digit ] [ (e | E) [ + | - ] 1*digit ]
```

No leading plus/zeros, hex, separators, suffixes, NaN, Infinity or missing fraction/exponent digits. Preserve spelling, including `-0`, `1.0`, `1E+01` and exponent leading zeros. Grammar checking need not evaluate an arbitrarily large exponent. The constructor imposes no fixed numeric magnitude or document quota: parsing applies its configured token cap before construction, while formatting caps emitted bytes. Owned input can be reused as storage; no allocation-reuse guarantee is made. Invalid text returns NumberError.InvalidSyntax with the unexpected byte or EOF offset relative to that token input.

text exposes a read-only logical string view. A caller may explicitly compare it lexically, but the library does not call this numeric equality. The required v0 interface includes no arithmetic, semantic Value/Number/Object equality/hash contract, normalization or canonical JSON.

### Objects and iteration

```text
Object(): Object
length(&Object): u64
get(&Object, &string): Option<&Value>
getMut(&mut Object, &string): Option<&mut Value>
insert(&mut Object, string, Value): Option<Value>
remove(&mut Object, &string): Option<Value>
```

The type/factory share the exported Object name in the existing separate namespaces. There is no runtime exported singleton or new constructor syntax. Parsing rejects duplicate names; ordinary programmatic insertion deliberately replaces an equal name's value, retains the old name and position, disposes the incoming unused name and returns the displaced value. New names append. Builder names/Value.String payloads are already decoded data. The insert operation never reparses backslash escapes or embedded JSON. Removal disposes the stored name, returns the value and preserves remaining relative order. Reinsertion after removal appends. A constructed object has no attached parser quota.

Names compare by decoded UTF-8 content, with no normalization/case folding. `"a"` and `"\u0061"` are duplicates; `"é"` and `"e\u0301"` are distinct. Ordinary Map's unspecified iteration order does not become a JSON order promise. Private storage may use an index, list or other implementation but must preserve the public invariants.

Canonical iterate/next implementations yield these named records in insertion order:

```rill
export type Member = { name: string; value: Value };
export type MemberRef = { name: &string; value: &Value };
export type MemberMut = { name: &string; value: &mut Value };
```

Consuming Object produces OwnedObjectIter, which yields Member. Borrowing &Object produces ObjectIter, which yields MemberRef. Borrowing &mut Object produces ObjectIterMut, which yields MemberMut. Exclusive results reach last use before advancing; structure cannot change under live iterator/element views. Names stay read-only. Existing array List iteration/mutation is reused, not a second JSON array container. Wrapping/copying a view never erases dependencies or creates a writable invariant-bearing field.

## 3. Parsing contract and Unicode

```text
parse(&string): Result<Value, ParseError>
parseWithLimits(&string, ParseLimits): Result<Value, ParseError>
defaultParseLimits(): ParseLimits
```

Accept exactly one complete value of any kind with surrounding JSON whitespace. Only SP, TAB, LF and CR are JSON whitespace. Reject empty/whitespace-only input, comments, trailing commas, uppercase keywords, a second document and trailing non-whitespace. There is no prefix parser or error recovery. A leading U+FEFF at input offset zero is LeadingBom; U+FEFF elsewhere outside strings is not whitespace. Inside strings it is ordinary content. Output never adds a BOM.

Input is already valid UTF-8 because it is &string. Raw Bytes callers explicitly use std::string::viewUtf8 and retain that source through parsing. Invalid byte encoding is Utf8Error at that boundary, not JSON ParseError. There is no required parseBytes/reader adapter. JSON's input cap does not bound prior transport allocation, read or UTF-8 validation. Runtime input newlines are not normalized by Rill's source-file newline rules.

Recognize JSON escapes `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t` and `\u` plus exactly four hex digits. Do not accept Rill's `\u{...}`, `\xNN` or other language-literal spellings. Decode high/low surrogate pairs to a scalar. Reject lone low/high surrogates, reversed or mismatched pairs rather than replace them silently. Incomplete/invalid escape syntax and a completed but invalid scalar sequence are distinct reasons. Accept other Unicode scalars, including noncharacters; no normalization, case folding or I-JSON noncharacter restriction.

String quotes, backslashes and raw U+0000-U+001F need the proper JSON escape. Other valid scalars such as DEL/U+0085/U+2028/U+2029 are legal string data, not additional outside-string whitespace.

This is strict JSON grammar with explicit receiver restrictions for unique decoded names, representable scalar strings, BOM and configured limits. RFC 8259's grammar admits duplicate names and lone surrogate escapes; do not falsely claim it prohibits them or that this is unrestricted acceptance of all grammar-admitted texts. It is not the full I-JSON profile.

## 4. Precision and conversion semantics supplied to ri-1wl

Preserving tokens such as `9007199254740993`, `1e400` and long decimals needs neither f64 storage nor bigint/decimal arithmetic primitives. It does not guarantee an external consumer avoids binary64 rounding.

Conversions from borrowed Number to primitive types require the following semantics:

- Integer extraction requires exact mathematical integrality, then destination-range checking, with no f64 intermediate. `1.0` and `1e2` can become 1 and 100; `1.5` must not truncate. Negative zero becomes integer zero, including unsigned zero.
- Exponent syntax must not be parsed through an overflowing native exponent just to classify a conversion. Zero coefficients remain zero even with a very large exponent. Finite non-integral tiny values remain non-integral for integer extraction, not rounded-to-zero first.
- Float extraction uses nearest/ties-to-even rounding for the requested width. It allows precision loss and underflow to zero, rejects finite input that rounds to infinity, and preserves negative zero. Original Number storage is unchanged.

Conversions from primitive types to Number require the following semantics:

- Typed integers produce exact ordinary decimal text without width suffixes; result remains fallible/uniform even when that case always succeeds.
- Finite floats use canonical shortest text that round-trips to the same source width, preserving negative zero. Reject NaN/infinities. Do not promise exact binary-rational decimal expansion or identical cross-width interpretation (e.g. shortest f32 text re-read as f64).

Use the requested canonical tryInto/tryFrom operation, not per-width toI32/toF64 APIs, arbitrary user-return-only ordinary overloads, a locally unrelated same-spelled function or caller-injected error values. Conversion implementers return their E. The confirmed ri-1wl contract owns registration and exact selected-E evidence, with checking/proof still ri-48g work; distinguish non-integrality, range failure and non-finite source semantically without mandating a universal ConversionError or adding a new global variant here. In particular, ordinary primitive float-to-integer truncation from ri-a2c remains different from exact JSON integer extraction.

Application record conversion stays explicit, with user-written field/kind/missing-data handling through ri-1wl's confirmed interface. No automatic Value-to-record reflection, schema/derive, string-to-number coercion, injected fields, unchecked cast or lifetime extension is assumed. No active fixture supplies or claims to execute a numeric converter implementation.

## 5. Limits and count definitions

```rill
export type ParseLimits = {
  maxInputBytes: u64;
  maxDepth: u64;
  maxNodes: u64;
  maxStringBytes: u64;
  maxNumberBytes: u64;
};
export type FormatLimits = { maxDepth: u64; maxOutputBytes: u64 };
export type PrettyOptions = { indent: u64; limits: FormatLimits };
```

Default ParseLimits are respectively 16,777,216 bytes (16 MiB), 128, 1,000,000, 1,048,576 decoded bytes (1 MiB), and 1,024 token bytes. Default FormatLimits are depth 128 and output 16,777,216 bytes. defaultPrettyOptions uses indent 2 and defaultFormatLimits(). Defaults are functions, not exported runtime global records. Callers can rebuild named records from defaults; there are no optional/named arguments or manifest/environment configuration.

All caps are inclusive; zero is a valid restrictive value. Root scalars have depth 0, root containers depth 1, and each nested array/object increases path depth by 1. Values count as nodes: root, array elements and object member values; names are not separate nodes. Empty containers count once. String cap applies per decoded UTF-8 value/name, not source escape bytes; number cap counts its original ASCII token bytes. Input cap includes surrounding whitespace. A depth-zero profile permits scalars, while a zero-node profile permits no complete document. Empty strings/names fit a zero string cap. There is no implicit relationship between the independently configurable caps.

Formatting measures emitted UTF-8 bytes, including escaping, newlines and spaces; parsed and application-built trees use the same depth definition. Pretty output may exceed the cap while compact output fits. No partial output string, accepted prefix, truncation, numeric approximation or silent loss is returned on a limit breach.

Check input length before parsing. Otherwise stop on the first detected failure, without promising a priority between checks failing at the same location. Check counter/indent/size calculations before overflow or excess allocation; representation overflow cannot silently wrap or become an avoidable arithmetic panic instead of the specified limit result. A huge indentation count is legal configuration whose required output can exceed the cap. Limits are logical resource budgets, not exact heap/stack residency, transport/CPU deadlines or recoverable allocation guarantees. Arbitrary raised limits still require bounded native stack use, including success/failure/cancellation/default cleanup of deep trees built outside the parser.

## 6. Formatting

```text
stringify(&Value): Result<string, FormatError>
stringifyWithLimits(&Value, FormatLimits): Result<string, FormatError>
pretty(&Value): Result<string, FormatError>
prettyWithOptions(&Value, PrettyOptions): Result<string, FormatError>
defaultFormatLimits(): FormatLimits
defaultPrettyOptions(): PrettyOptions
```

Return independent owned UTF-8 strings or an error, not input/tree views. Values are already grammar-valid: Number/Object privacy and scalar strings prevent malformed numeric tokens, duplicate stored names or invalid string scalars. Formatting checks resource budgets; it does not repair invalid nodes, insert null, serialize NaN/Infinity or catch allocation failure. No implicit Value-to-string conversion or required canonical show(Value) adapter is added. The infallible show signature must not silently discard a fallible formatting limit. Handle the explicit formatting Result before logging/interpolation.

Compact output has no insignificant whitespace or terminal newline. Pretty output defaults to two spaces per level, LF, one space after colons and one member/element per line for nonempty containers. It uses inline `[]`/`{}` and has no trailing comma or terminal newline. Scalars are inline. Configurable indent is a u64 count of ASCII spaces, including zero; no tabs/arbitrary indentation strings or template mini-language. Empty containers emit no indentation. Formats preserve array/member order and exact Number tokens, not source whitespace or original string escape spelling.

Escape quote/backslash, use `\b`, `\t`, `\n`, `\f`, `\r` for their controls and lowercase `\u00xx` for remaining U+0000-U+001F. Leave slash and other valid non-ASCII scalars unescaped, including U+2028/U+2029 and allowed noncharacters. This is JSON UTF-8, not HTML/JavaScript embedding escaping. Another-language/transport boundary needs its own handling. Parsed `"\u0061"` formats as `"a"`, while `1E+01` remains `1E+01`. Sorted/canonical output, numeric normalization and semantic equality are deferred.

Under sufficient compatible limits, formatting then parsing preserves the owned JSON data/order/number spelling; compact parse/format reaches the chosen escaping/whitespace representation, not original document bytes. Do not express this as unavailable generic Value equality or promise pretty output fits the defaults merely because parse succeeded.

## 7. Typed errors and positions

```rill
export enum SyntaxReason {
  UnexpectedEnd,
  UnexpectedToken,
  InvalidNumber,
  InvalidString,
  InvalidEscape,
  InvalidUnicode,
  TrailingInput,
}
export enum ParseLimit {
  InputBytes,
  Depth,
  Nodes,
  StringBytes,
  NumberBytes,
}
export enum ParseError {
  Syntax { offset: u64; reason: SyntaxReason },
  DuplicateName { offset: u64; firstOffset: u64 },
  LeadingBom { offset: u64 },
  LimitExceeded { offset: u64; kind: ParseLimit; maximum: u64 },
}
export enum NumberError {
  InvalidSyntax { offset: u64 },
}
export enum FormatError {
  DepthLimit { maximum: u64 },
  OutputLimit { maximum: u64 },
}
```

All are exported nominal types with canonical borrowed display implementations producing owned text. Their metadata is scalar/enum data: no input/tree views, copied keys, input excerpts or full document snapshots. No reflected formatter, mandatory message field, source-text-based machine protocol or fixed human wording. Callers retaining the source can add context themselves. Underlying Err<E> construction origins/explicit causes follow ri-vtw/#23 and are **not** JSON input positions. These error payloads satisfy display/noBorrow/sendable/shareable; retaining a cause still requires those normal checks.

Positions are zero-based byte offsets relative to the supplied view, including leading whitespace, with EOF at length. Do not silently add an underlying file/view base offset, normalize CRLF, count graphemes or use decoded-string offsets. Duplicate positions identify the second and original opening quotes after decoded comparison. A limit/error position need not always be a UTF-8 scalar boundary: an input byte cap can cut a multibyte encoding while the supplied whole string is itself valid.

Required limit positions:

- InputBytes identifies maxInputBytes, the first disallowed source byte, checked before grammar.
- Depth/Nodes identifies the offending container/value start.
- StringBytes identifies the source scalar/escape whose decoded output would exceed the per-string cap. A surrogate pair identifies its first escape.
- NumberBytes identifies the first source token byte beyond the cap.

Other syntax identifies an unexpected byte or EOF; a completed invalid scalar escape identifies the relevant escape start (e.g. the unpaired high/low surrogate). Incomplete escape syntax can report EOF; invalid hex/escape spelling can identify the bad source byte. Reasons describe the detected parsing failure, not an obligation to find all problems or classify every malformed numeric prefix under one universal lexer policy. TrailingInput means data remained after a complete value and surrounding whitespace. LeadingBom applies at view offset zero; U+FEFF after other outside-string input is ordinary invalid syntax. Errors are fail-fast; partially constructed owned state still cleans up with bounded stack use.

## 8. Evidence, scope and follow-through

`prototypes/17-json.rill` contains intended-valid calls and independent expectations; `docs/prototypes/17-json.md` contains rejection, numeric-conversion and adversarial/resource cases. They are hand-checked design artifacts, not executed tests or a parser implementation. They do not establish that ri-48g/#30 implementation/checker/backend gates are cleared.

Use ri-1wl's confirmed conversion registration/error-evidence/naming reconciliation; ri-3t8 owns compiler diagnostics; ri-48g integrates exact JSON algorithms/invariants, byte positions and scalar spans, float/integer boundary behavior, quota accounting, callee exits/transitive views, deep copy/partial cleanup/cancellation/native lifetime rules and eventual executable suites. ri-dej still validates backend/proper-tail/Wasm mechanisms. If an accepted operation cannot be specified/checked consistently, return that blocker through the map; do not assert that privacy or iterative intent proves memory safety. ri-fnk/#39 and other application foundations must compose these owned values/errors rather than assume a streaming/reflected serializer.

## Primary sources

- [RFC 8259 §§2-4](https://www.rfc-editor.org/rfc/rfc8259.html#section-2): complete value grammar, whitespace, arrays/objects and duplicate-name interoperability.
- [RFC 8259 §6](https://www.rfc-editor.org/rfc/rfc8259.html#section-6): number grammar, permitted range/precision limits and binary64 interoperability cautions; not mandatory f64 storage.
- [RFC 8259 §§7-8](https://www.rfc-editor.org/rfc/rfc8259.html#section-7): escapes, UTF-8/BOM, surrogate hazards and decoded-string comparison.
- [RFC 8259 §§9-10](https://www.rfc-editor.org/rfc/rfc8259.html#section-9): implementation limits and strict generator output.
- [RFC 7493 §2](https://www.rfc-editor.org/rfc/rfc7493.html#section-2): stricter I-JSON Unicode/number/duplicate-name profile; not automatically Rill's selected policy.
- [RFC 3629 §§3-4](https://www.rfc-editor.org/rfc/rfc3629.html#section-3): scalar-valid UTF-8, distinct from ASCII text spelling a surrogate escape.
