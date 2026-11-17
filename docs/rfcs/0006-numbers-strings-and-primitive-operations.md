# RFC 0006: Numbers, strings and primitive operations

| Field | Value |
| --- | --- |
| ID | 0006 |
| Status | Draft |
| Owner | Reilley64. [#28](https://github.com/Reilley64/rill/issues/28) tracks this draft. |
| Source inputs | [#18 confirmed decision](https://github.com/Reilley64/rill/issues/18#issuecomment-6869848603), [ADR 0011](../adr/0011-explicit-numeric-widths-and-utf8-string-views.md) and later refinements under [source reconciliation](#source-reconciliation). Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The [policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6913372816) confirms literal spelling, decimal parser grammar and UTF-8 spans. The [interface decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6913810445) confirms strict conversion entry points and directs the core predictability audit. |
| RFC dependencies | Draft RFCs [0002](0002-modules-and-program-loading.md), [0003](0003-expressions-bindings-and-control-flow.md), [0004](0004-functions-overloads-and-generic-requirements.md) and [0005](0005-types-records-enums-and-inference.md) propose shared rules. Their full texts are not accepted. Ownership, errors, conversions, complete grammar and runtime [companions](#required-companion-rules) remain incomplete or missing. |
| Replaces / superseded by | None. This draft does not accept or amend another RFC. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes numeric primitive behavior, text literals, Unicode scalars, logical string access and canonical interpolation. It also owns protected numeric conversion behavior, strict numeric parsing and the described UTF-8 operations.

The glossary defines [numeric conversion, string, string view, character and Bytes](../../GLOSSARY.md#language). String is owned UTF-8 text. A string view provides read-only access to a specific extent without owning it.

The source decisions already fix explicit widths, checked integers, IEEE floats and no implicit typed numeric conversion. This draft preserves those decisions. Later enum, library and conversion decisions replace obsolete literal-type, copy-call and closed-family claims.

The owner confirmed source literal spelling, strict decimal parser grammar and invalid UTF-8 span grouping. These three policies are no longer open choices. The owner confirmed canonical strict conversion entry points and requested a predictable core conversion interface. The broader pair matrix and ScalarError below are draft details under that direction. This document remains Draft. Syntax policies are confirmed; formatting details and ownership/runtime procedures still prevent readiness.

General conversion coherence belongs to a separate companion. This RFC does not add implicit casts, general operator overloading, arbitrary-precision integers, mutable substring views or a public FFI. No Rill parser or runtime has executed its examples.

## Motivation

A programmer must know a number's width without guessing from target architecture or optimization mode. Arithmetic and conversion must distinguish a malformed literal, a runtime panic and a recoverable range failure.

Text access must preserve UTF-8 validity and the exact observed extent. A substring view cannot behave as an owning string header. Copying a descriptor must not copy its backing owner or erase its source dependency.

Parsing and decoding also need exact failure boundaries. Different permissive parsers or replacement groupings could produce different accepted inputs and text. The confirmed policies fix those choices before implementation.

## Guide-level explanation

Case N1 is a complete module:

```rill
function shifted(value: i32): i32 {
  return value >> 1;
}
function skippedTrap(): i8 {
  if (false) { return 127i8 + 1i8; }
  return -128i8;
}
expect shifted(-4i32) == -2i32;
expect skippedTrap() == -128i8;
```

The shift supplies u32 context for its count. Integer overflow would panic if evaluated. The unexecuted addition remains well-typed. By contrast, the individual literal `128i8` is invalid outside the defined negated-literal case.

Case N2 is a complete module:

```rill
import std::string::slice;

function middle(): string {
  let owner = "aéz";
  guard let view = slice(&owner, 1u64, 3u64) else { return "bad slice"; }
  return copy(view);
}
expect middle() == "é";
```

Offsets count UTF-8 bytes. The slice selects exactly the two-byte scalar é. Copy already receives `&string`, so it returns independently owned text of that extent. It does not copy the entire original owner.

Case N3 is a complete module:

```rill
import std::convert::{into, tryInto};
import std::bytes::Bytes;

function parsed(): i32 {
  guard let value = tryInto<i32>(&"42") else { return 0i32; }
  return value;
}
function truncated(): u8 {
  guard let value = tryInto<u8>(-0.5f64) else { return 255u8; }
  return value;
}
function decoded(): string {
  let bytes = into<Bytes>([0xc3u8, 0xa9u8]);
  guard let text = tryInto<string>(&bytes) else { return "bad UTF-8"; }
  return text;
}
expect parsed() == 42i32;
expect truncated() == 0u8;
expect decoded() == "é";
```

Parsing consumes decimal text. Conversion instead receives an already typed number. Float-to-integer conversion truncates before checking range. Neither operation adds a default source width or an implicit error conversion.

All results above are expected outcomes, not compiler observations. Standard helper declarations and error metadata remain required dependencies.

## Syntax and reference semantics

The owner [confirmed the three syntax policy groups](https://github.com/Reilley64/rill/issues/28#issuecomment-6931939690) in [annex version 1](../grammar/v0-syntax-v1.md). Its [EBNF](../grammar/v0-syntax-v1.ebnf) and commitment/context rules form one policy review unit. Full annex/RFC acceptance and static-semantics completion remain separate.

### Primitive types and width inference

Signed integers are i8, i16, i32, i64 and i128. Unsigned integers are u8, u16, u32, u64 and u128. They have the named bit widths. Signed representation uses two's complement.

Floating primitives are f32 and f64, using IEEE binary32 and binary64. Bool, char and string are distinct nonnumeric primitives. This draft introduces no int, float, byte, isize or usize aliases.

A numeric literal needs a type from its suffix or immediate known context. An annotation, established parameter, return or operand type can provide that context. Do not select a smallest-fitting type or target-sized default.

Already typed operands do not change width through context. `1i32 + 1i64` is invalid. An unsuffixed literal may use an established operand's compatible context. Two otherwise unconstrained literals do not create a numeric default.

Primitive literal expressions have ordinary base types, not singleton types. Named immutable bindings do not retain a literal subtype. Constant knowledge does not create infallible narrowing conversions or alter overload ranking.

<a id="proposed-numeric-literal-spelling"></a>

### Numeric literal spelling

The following grammar uses ASCII digits. It records the confirmed literal spelling, not a complete token grammar.

```text
digits(base) = digit(base) { ["_"] digit(base) }
integer-body = digits(10)
             | "0b" digits(2)
             | "0o" digits(8)
             | "0x" digits(16)
integer-suffix = i8 | i16 | i32 | i64 | i128
               | u8 | u16 | u32 | u64 | u128
exponent = ("e" | "E") ["+" | "-"] digits(10)
float-body = digits(10) "." digits(10) [exponent]
           | digits(10) exponent
float-suffix = f32 | f64
integer = integer-body [integer-suffix]
float = float-body [float-suffix] | digits(10) float-suffix
```

An underscore separates two valid digits in the same digit sequence. Reject leading, trailing or repeated underscores. Do not place one beside a prefix, decimal point, exponent marker, sign or suffix.

Base prefixes are lowercase. Hexadecimal digits may use either case. Leading decimal zeroes do not select octal. A suffix directly follows the body.

Base-prefixed literals accept integer suffixes only. Thus `0xff32` contains hexadecimal digits, not an f32 suffix. It still needs independent integer width context. `0xff32u32` supplies that width explicitly.

An unsuffixed decimal digit form can use integer or float context. A base-prefixed form requires integer context. Decimal float syntax requires digits on both sides of a written decimal point. A fraction or exponent requires floating context or a floating suffix. An integer-looking form such as `1f32` explicitly selects a float.

A leading sign is an operator, not part of the token. Prefix plus is not introduced. The exponent's plus or minus belongs to the exponent. Operator grouping follows RFC 0003.

Check an integer literal's exact mathematical value against its established range. Reject invalid digits, incompatible literal categories and out-of-range individual values. A wide intermediate float must not decide integer literal validity.

For signed minima, range-check one directly negated integer literal as a negative mathematical value. Whitespace, comments and parentheses around that literal do not change this case. Thus `-128i8` and `-(128i8)` are confirmed valid spellings.

An ordinary positive `128i8` remains invalid. `0i8 - 128i8` cannot use the exception. In `-(-128i8)`, the outer negation is ordinary checked arithmetic and panics if evaluated. Unsigned unary negation is invalid, including `-0u8`.

Decimal floating literals round to the selected width using nearest, ties-to-even. Reject a finite literal whose rounded result is infinity. Permit underflow to a subnormal or signed zero. Source NaN/infinity literals are not introduced; ordinary expressions can produce those values.

The [confirmed literal-scanning policy](#proposed-literal-scanning-and-token-boundaries) specifies identifier/dot boundaries. The shared syntax annex records confirmed generic commitment and closing-token policies. This policy does not authorize hexadecimal floats or an arbitrary constant-expression literal category.

### Integer operations and traps

Ordinary `+`, `-`, `*`, `/` and `%` require the same concrete numeric type. Integer arithmetic is checked in every build. Overflow panics rather than wrapping or depending on debug settings.

Integer division truncates toward zero. Remainder has the dividend's sign when nonzero. Division or remainder by zero panics. Signed MIN divided by -1 and signed MIN remainder -1 both panic.

Bit operations `&`, `|`, `^` and `~` operate on integer bit patterns. Binary bit operands use the same concrete integer type. They do not add boolean bit operations or user overloads.

Shifts require an integer left operand and a u32 count. A count at least the left operand's width panics. Do not mask counts to match host instructions.

Left shift discards outgoing bits rather than using arithmetic overflow checks. Signed right shift extends the sign. Unsigned right shift inserts zero bits. For example, `255u8 << 1u32` yields 254u8.

Bool supports logical negation and the short-circuit operators from RFC 0003. Integer, float, bool and char equality retain their primitive meanings. String equality and ordering are specified below. Complete char relational-operator admission remains in-scope authoring work.

### Floating operations

Floating `+`, `-`, `*` and `/` use the established width and IEEE behavior. Preserve NaNs, infinities, signed zero and gradual underflow. Floating zero division does not use integer panic rules.

Use nearest, ties-to-even rounding. Do not change observable results through reassociation, fused operations or flushing subnormals. A target needs correct lowering rather than a weaker release-build mode.

Floating `%` uses truncating fmod-style remainder, not nearest-quotient IEEE remainder. A finite dividend with an infinite divisor returns the dividend. An infinite dividend or zero divisor produces NaN. A zero remainder retains the dividend's sign.

NaN compares unequal to every value, including itself. Ordered comparisons involving NaN are false. The two zero signs compare equal. No portable NaN payload or diagnostic-payload bit pattern is promised.

No numeric operator creates an implicit conversion between widths. Explicit library operations can provide separately specified behavior. This draft introduces no global fast-math switch or fused-operation API.

### Static invalidity versus evaluated failure

An invalid individual literal rejects the program even in an unexecuted branch. Missing width has meaning R0003. Established literal range/encoding invalidity has meaning R0002. Malformed syntax remains R0001.

A well-typed overflowing operation instead panics only if evaluated. This includes provable constant arithmetic, zero divisors and invalid shift counts. No mandatory arithmetic constant-evaluation rejection regime is added.

Folding must preserve acceptance, evaluation order, effects and panic timing. Do not hoist a trap before earlier effects or remove an evaluated trap because its value is unused. Advisory warnings cannot turn valid trap-if-evaluated expressions into language errors.

Finite floating arithmetic may produce infinity. That differs from rejecting an overflowing individual finite literal. Strict parsing and numeric conversion also retain their own outcomes below.

### Proposed M4.3 literal and primitive checking

[M4.3 ticket #60](https://github.com/Reilley64/rill/issues/60) proposes this procedure and admission matrix. Owner approval is pending. Confirmed spelling, explicit widths and trap timing remain unchanged.

Use M1's retained literal/token facts and original source maps. Do not rescan source grammar or reinterpret parser commitments. A suffix names the actual compiler primitive identity despite source shadowing. A user nominal type with the same spelling is not that primitive.

First collect fixed operand types, explicit annotations, immediate expectations and declared parameter/result contexts. Connect the primitive operation's operand/result relationships. Propagate established context to a fixed point before finalizing unsuffixed literals. Do not emit a missing-width error merely because the other operand has not yet supplied its established type.

A suffixed literal fixes its type before expected-type agreement. An unsuffixed decimal digit literal can use established integer or floating context. Base-prefixed literals require integer context. Fraction/exponent forms require floating context. Do not choose a smallest fitting width, default width or type from value ranges.

A comparison's Boolean result does not establish numeric operand widths. Same-type arithmetic can obtain width from its immediate numeric result context or an independently typed operand. Shift counts use canonical u32 independently of the left operand's integer width.

After context closure, range-check the exact integer magnitude at its established width. Apply the confirmed signed-minimum exception only to a directly negated integer literal, ignoring grouping and trivia. Do not apply it to a computed positive value or subtraction operand. Reject unsigned negation, including negative zero.

Round decimal floating literals directly at the selected width with nearest, ties-to-even. Reject a finite literal that rounds to infinity. Permit subnormal and zero underflow, retaining negative zero under ordinary floating negation. Do not round through another floating width first.

Retain exact original spelling, selected primitive identity and range/rounded-value evidence. Reuse decoded M1 character/text facts and their original spans. Static uninterpolated templates are strings; interpolation remains incomplete until canonical formatting evidence is implemented.

Use the following closed primitive matrix. These are type contracts, not user-overload searches:

| Operation | Inputs and result |
| --- | --- |
| Arithmetic +, -, *, /, % | Two equal concrete numeric primitives N, producing N. |
| Unary minus | Signed integer or floating N, producing N. |
| Bitwise &, \|, ^, ~ | Equal integer inputs, or one integer for ~, producing that integer type. |
| Shifts <<, >> | Integer N and u32, producing N. |
| Logical !, &&, \|\| | Boolean inputs, producing bool. |
| Equality ==, != | Equal integer/float/bool/char primitives, or the text observation forms below, producing bool. |
| Relational <, <=, >, >= | Equal numeric primitives, char or the text observation forms below, producing bool. |

The proposed char relation uses Unicode scalar code-point order. It adds no locale or grapheme ordering. This resolves the explicitly unfinished char-operator admission question only if the owner approves it.

Text observation admits string, &string and &mut string operands, including mixed pairs. Each operand denotes its observed extent. Preserve its actual type and source access fact; do not convert a view into an owner. Equality and ordering do not consume operands.

Deeper descriptor references require explicit dereference before comparison. Other primitive operators require their listed primitive types, not implicit reference dereferencing. This is an intrinsic text-observation rule, not an ordinary-call adapter. Complete access and retained-source safety remain unperformed.

Other known operand constructors do not gain a primitive operation through spelling or field shape. The matrix introduces no string arithmetic, Boolean ordering, pointer identity or user-defined operator dispatch. Never admission follows its proposed RFC 0005 rule without supplying a missing numeric width.

Unknown numeric context after the implemented finite constraint procedure requires an annotation. A valid numeric literal without width has R0003 meaning. Individual established literal range/category failure has R0002 meaning. An already typed incompatible value has R0100 meaning. Unsupported later procedures and exhausted budgets remain separate incomplete/resource outcomes.

Do not evaluate arithmetic, comparisons or user functions to check types or control flow. Provable overflow, zero division and excessive shifts remain well-typed traps if operands satisfy the matrix. They are not literal-range errors or statically proved divergence. Check invalid literals and type mismatches even in skipped or unreachable source.

### Protected numeric conversions and explicit arithmetic helpers

The canonical families are `into<To, From>(From): To` and `tryInto<To, From, E>(From): Result<To, E>`. `from` aliases `into`; `tryFrom` aliases `tryInto` with the same destination-first argument order. These names require explicit imports from std::convert.

Only canonical conversion families use destination/source keys. General registration, endpoint ownership, inference and selected-E evidence need the conversion companion. RFC 0004 preserves their distinction from ordinary overloads.

This RFC owns standard numeric pair behavior. Protected numeric implementations use ConversionError for fallible failure. ConversionError has OutOfRange and NotFinite unit variants, with canonical display and equality.

| Established source and destination | Infallible availability | Value behavior |
| --- | --- | --- |
| Integer to integer | The destination contains the source base type's entire range. | Preserve the mathematical integer. A fallible pair checks range. |
| Integer to float | Available for every numeric integer/float pair. | Round to the destination width. Precision loss or infinity is permitted. |
| Float to float | Available for either width. | Round at the destination width. Narrowing may produce infinity. |
| Float to integer | Not available through infallible into. | Truncate toward zero, then check the resulting integer range. NaN/infinity fail. |

Signed integer width n has range `-2^(n-1)` through `2^(n-1)-1`. Unsigned width n has range zero through `2^n-1`. Use full base ranges, not a known fitting argument value.

Every numeric pair has a fallible form, including safe and identity pairs. Safe numeric tryInto returns Ok with the established numeric error type. This does not generate an automatic fallible adapter for user conversions.

Float-to-integer non-finite input returns NotFinite. An out-of-range truncated finite integer returns OutOfRange. `tryInto<u8>(-0.5f64)` therefore succeeds with zero.

Check integer boundaries without a lossy intermediate comparison. In particular, rounding u128's maximum to f64 must not make `2^128` appear in range. Exact-E propagation and error origins belong to the error companion.

Numeric registration does not create bool, char, string or reference conversions. Code-point construction and parsing use their explicit APIs. A destination never establishes a missing numeric source width.

`std::number` supplies `wrappingAdd`, `wrappingSub`, `wrappingMul`, `saturatingAdd`, `saturatingSub` and `saturatingMul`. Each is an ordinary `(T,T): T` family with integer implementations only. Wrapping uses the fixed-width residue; saturation clamps to the destination integer range. Neither changes ordinary arithmetic or shift behavior.

### Canonical core conversion interface

Use into for a canonical conversion without recoverable failure. Use tryInto for a canonical fallible conversion. Allocation failure can still be fatal. Infallible does not mean pure, allocation-free or lossless.

Keep named operations for explicit policy, extra context, queries, traversal and mutation. Changing a type alone does not make every function a conversion. Record/enum constructors and zero-input factories keep their established construction role.

The owner [confirmed strict number parsing and owned UTF-8 decoding through tryInto](https://github.com/Reilley64/rill/issues/28#issuecomment-6913810445). The owner also directed an audit of related core conversions. The following matrix is the concrete draft under that direction.

| Source | Destination | Family | Selected error | Required meaning |
| --- | --- | --- | --- | --- |
| `&string` | Each numeric primitive N | tryInto | `std::number::ParseError` | Strict decimal parsing of the whole observed view. |
| `&Bytes` | `string` | tryInto | `std::string::Utf8Error` | Strict UTF-8 validation and independent owned text output. |
| `&Bytes` | `&string` | tryInto | `std::string::Utf8Error` | Strict borrowed UTF-8 validation, retaining source dependencies. |
| `&string` | `&Bytes` | into | None. | Read-only UTF-8 byte view of the observed text extent. |
| `char` | `u32` | into | None. | The scalar's Unicode code point. |
| `u32` | `char` | tryInto | `std::char::ScalarError` | Validate the Unicode scalar range and exclude surrogates. |
| `List<u8>` | `Bytes` | into | None. | Consume the List and produce owned contiguous bytes. |
| `&List<u8>` | `Bytes` | into | None. | Copy elements into independently owned contiguous bytes. |
| `Bytes` | `List<u8>` | into | None. | Consume the byte owner and produce an owned List. |
| `&Bytes` | `List<u8>` | into | None. | Copy the observed bytes into an independently owned List. |

The first row describes twelve concrete standard pairs, not a new generic parser fallback. Other user endpoints retain ordinary conversion registration permission. They do not acquire parsing from their shape or from show support.

ScalarError is proposed as an enum with one unit variant, InvalidScalar. It has standard display and equality implementations. It is not numeric ConversionError. Valid noncharacters and unassigned scalars still succeed.

Register each pair explicitly under existing canonical identity, placement and duplicate rules. Errors are selected output evidence, not caller-chosen dispatch keys. No automatic infallible-to-fallible adapter or extra owned/borrowed source pair follows.

Importing the canonical std::convert operation must load the promised core registrations through declared metadata dependencies. Callers do not import error names merely to activate pairs. Naming those types still needs ordinary source imports. This adds no wildcard source binding, installed-module scan or runtime registry.

Borrowed outputs retain actual backing dependencies. Independent copied outputs do not retain the source merely because their input was borrowed. Consuming forms use normal ownership rules; allocation reuse is optional and never assumes List contiguity.

Keep source types independently established. For example, byte List inputs use established u8 elements. The desired Bytes destination does not justify guessing a missing numeric source width. Explicit From arguments can supply source context, as in `into<Bytes, List<u8>>([])`.

Public parseNumber and its dedicated parseNumber(T) requirement are removed. Public decodeUtf8, viewUtf8, bytes, codePoint, fromCodePoint, intoList, copyFromList and copyToList also give way to this matrix. The Bytes(List<u8>) value factory is replaced by into<Bytes>; the Bytes type remains.

These removals concern the listed standard module bindings only. Other operations named bytes, such as JSON or path observations, do not change through spelling. Private implementation helpers are not new public contracts.

Keep decodeLossy named because replacement is an explicit policy choice. Do not register into<string>(&Bytes) as a default lossy decode. Show remains the canonical interpolation contract; it does not automatically register string conversions.

`slice`, `append`, `length`, byte lookup, `chars` and `charIndices` remain explicit operations. They select subranges, mutate, inspect or establish traversal state. Context-dependent parsing and protocol-specific interfaces require their own review.

<a id="proposed-strict-decimal-parser-grammar"></a>

### Strict decimal parser grammar

Standard `tryInto<N>(&text)` parses text for each numeric primitive N. Its selected error is ParseError. Generic callers use `tryInto(T, &string, ParseError)` evidence for that relationship, not a dedicated parseNumber(T) predicate.

A generic helper using selected E instead states `tryInto(T, &string, E)`. It must preserve E symbolically and add other requirements only when needed. Concrete standard numeric callers establish E=ParseError; unrelated user conversions need not use that error.

ParseError has InvalidSyntax and OutOfRange unit variants with standard display and equality. Parsing consumes the whole view. It does not trim whitespace or accept source separators, suffixes, automatic base prefixes or locale-specific digits.

Use these confirmed ASCII grammars for default parsing:

```text
signed-integer-text = ["-"] decimal-digits
unsigned-integer-text = decimal-digits
finite-float-text = ["-"] decimal-digits ["." decimal-digits]
                    [("e" | "E") ["+" | "-"] decimal-digits]
special-float-text = "NaN" | "inf" | "-inf"
decimal-digits = digit(10) {digit(10)}
```

Leading zeroes remain decimal. The grammar rejects an initial plus sign. Exponents may contain plus. Reject `.5`, `1.`, unsigned `-0`, alternate special-value spellings and negative NaN.

Malformed text returns InvalidSyntax. A valid integer spelling outside its destination range returns OutOfRange. A fractional or exponent form is not integer text and must not be truncated by the parser.

Floating parsing accepts integer-looking decimals and preserves negative zero. Round finite decimal input using nearest, ties-to-even at the requested width. If that rounded result is infinity, return OutOfRange. Underflow may produce signed zero.

Explicit NaN/inf/-inf text is accepted only for float destinations. This is distinct from finite overflow. No radix-parsing API is added to the required interface here.

The owner confirmed this grammar independently of the public parsing entry point. It does not adopt a host parser's whitespace, plus-sign, prefix or special-value extensions.

### Characters and text literal syntax

Char represents one four-byte Unicode scalar value. Valid values include U+0000 through U+10FFFF except U+D800 through U+DFFF. Unassigned, noncharacter and non-printable scalars remain valid. A scalar is not necessarily a grapheme.

Single-quoted character literals contain exactly one decoded scalar. Double-quoted string literals produce UTF-8 string values. Backtick templates produce strings with optional interpolation. Raw strings place optional hash marks between r and the opening quote. The closing quote has the same hash count.

Common escapes are `\n`, `\r`, `\t`, `\0`, escaped quotes and backslash. Text `\xHH` contains exactly two hexadecimal digits and must denote ASCII. Use one through six hexadecimal digits inside `\u{...}`, without separators, yielding a valid scalar.

Templates can escape their interpolation opener with `\${`. Raw strings perform no escapes or interpolation. They do not introduce raw identifiers or byte-string literals.

Ordinary quoted strings and chars reject unescaped physical line breaks. Raw strings and templates can contain multiple lines without dedent. Normalize physical CRLF pairs to LF in literal content. Preserve explicit escaped CR and runtime-decoded CR/LF bytes.

Retain original source byte offsets despite literal-content normalization. Do not rewrite source files or confuse decoded content offsets with source spans. Unicode normalization and locale transformations are not implicit.

The [confirmed literal-scanning policy](#proposed-literal-scanning-and-token-boundaries) supplies delimiter, control and recovery rules. It preserves UTF-8/scalar validity and the existing CRLF rule. The [lexical decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825) also confirms lone-CR admission/normalization.

<a id="proposed-literal-scanning-and-token-boundaries"></a>

### Literal scanning and token boundaries

The approved spelling grammar above remains unchanged. The owner [confirmed scanner boundaries, literal modes, controls and recovery](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825) in the coordinated lexical review. This approves those policies, not the full RFC or a scanner implementation.

#### Numeric candidate scanning

A numeric candidate starts with an ASCII decimal digit. A leading sign remains a separate operator. A dot alone never starts a numeric candidate.

For lowercase base prefixes, consume the prefix and the following maximal identifier-continue run under RFC 0003's pinned Unicode profile. Validate the complete candidate against the integer spelling grammar. Invalid digits, separators or suffixes reject the candidate; they do not create an accepted numeric prefix followed by an identifier.

For a decimal candidate, first consume the maximal digit/underscore run. Consume a decimal point only when its next source byte is an ASCII decimal digit. Then consume that fractional digit/underscore run.

If the next source byte is e or E, consume it and at most one following sign. Consume its digit/underscore run, even when empty. Finally consume any immediately adjoining identifier-continue run as the candidate's suffix/remainder. Numeric spelling itself still uses only the approved ASCII digits and suffixes.

Validate the complete candidate against the approved spelling grammar. Empty exponent digits, unknown suffixes and trailing identifier text are invalid literal spelling, R0002. This validation does not infer a width or round a magnitude.

For prefixed integers, valid hexadecimal digits take priority over possible float-suffix text. Thus `0xff32` remains one unsuffixed hexadecimal integer. An i/u suffix starts only after the maximal valid base-digit sequence.

The grammar decides digit separators and suffix legality after boundary scanning. For example, an underscore before a suffix cannot be repaired by splitting tokens. Uppercase base prefixes are invalid candidates, not identifiers attached to zero.

Whitespace/comments always end a numeric candidate. Signs are included only immediately after a decimal exponent marker during that scan. No hexadecimal-float or numeric-member shorthand is added.

A token can contain a magnitude outside its written width. The type/literal checker applies the approved directly-negated signed-minimum rule before rejecting that magnitude. Lexing `128i8` alone cannot decide whether the surrounding expression is `-(128i8)`.

#### Quoted, raw and template scanning modes

Single and double quotes open cooked character/string scanning. An unescaped matching quote closes the literal. The other quote character is content unless escaped.

Empty string, raw-string and template contents are permitted. A character literal still requires one scalar. Adjacent literals do not concatenate implicitly.

For delimiter detection, a backslash protects the next scalar from closing a cooked literal. It cannot protect a physical line ending. Escape validation then checks the complete spelling rather than accepting an unknown escape.

An ordinary quoted literal stops before an unescaped physical line ending or at EOF if no close appears. Report an unmatched delimiter with R0001. Do not search later lines for a convenient quote and conceal intervening declarations.

A complete raw opener is `r`, N consecutive hashes and a double quote, with N at least zero. Its close is a double quote followed immediately by N hashes. The first such close ends the literal.

A quote followed by fewer than N hashes remains raw content. Consume exactly N hashes at a close; additional hashes belong to subsequent source. Backslashes, `${` and comment openers have no special meaning inside raw content.

The hash count is bounded by input length, not a new fixed language quota. Resource/counter limits cannot silently select another delimiter. A raw string without a close rejects through EOF.

A backtick opens template-text mode. An unescaped backtick ends it; an unescaped `${` opens an expression hole. An ordinary dollar sign is text when it does not start that pair.

Each hole starts with brace depth zero. Code-mode `{` increments that depth; `}` decrements a positive depth. A code-mode `}` at depth zero closes the hole and resumes template text.

Strings, raw strings, comments and nested templates inside a hole use their own scanning modes. Their braces do not alter the enclosing hole's depth. Parenthesis/bracket mismatches remain parser errors; they do not change the hole's closing delimiter.

The hole must contain exactly one permitted expression. Empty holes and trailing nonexpression text reject in the parser. A colon does not introduce a format mini-language. An unterminated template or hole rejects through EOF.

Nesting state must not wrap or rely on unlimited host recursion. A controlled implementation resource limit reports incomplete analysis, not accepted source. Abrupt host exhaustion need not produce a report. This contract does not choose a Rust token-tree representation.

#### Exact escapes and physical content

The complete confirmed common escape set is:

```text
\n      LF
\r      CR
\t      horizontal tab
\0      NUL
\\      backslash
\'      single quote
\"      double quote
\xHH    exactly two hexadecimal digits, value 00 through 7F
\u{H}   one through six hexadecimal digits, valid Unicode scalar
```

These common escapes work in all cooked literal forms. Hexadecimal escape digits accept either case and no separators. Unicode escapes require their closing brace and reject surrogate/out-of-range values.

Templates additionally admit escaped backtick and `\${`. The latter contributes literal dollar and opening brace without opening a hole. There is no general dollar escape, octal escape or escaped-newline continuation.

A `\xHH` consumes exactly two digits; a following digit is ordinary content. Unknown or incomplete escapes in a closed literal reject with R0002. Escapes never reinterpret invalid source UTF-8 as valid text. Decoded characters never become lexical delimiters.

Character literals must decode to exactly one scalar. Combining sequences can therefore be too long even when they display as one grapheme. All scalar values remain representable through appropriate escapes.

Reject unescaped U+0000 through U+001F except admitted tabs and physical line endings. Also reject unescaped U+007F through U+009F. Apply this restriction to literal/comment content; escaped scalar values remain valid.

Ordinary quotes still reject physical line endings. Raw strings/templates admit LF and CRLF without dedent, as approved. The confirmed lone-CR line ending also works there and normalizes to LF.

Normalize each physical CRLF pair to one LF in literal content. Under the confirmed policy, normalize a physical lone CR to LF too. Explicit escaped CR and runtime-decoded CR/LF remain unchanged.

Other scalars retain their exact content. In particular, U+2028/U+2029 are literal/comment content, not source whitespace or physical line endings. This adds no Unicode normalization. Identifier membership follows RFC 0003, not literal-content admission.

#### Decoded content and source provenance

Retain the original literal span, including delimiters, alongside decoded content. Each decoded scalar maps to the original scalar, escape or physical line-ending span that produced it. Delimiters produce no content.

The two scalars from `\${` can share the complete escape span. CRLF contributes one decoded LF with a two-byte source span. Do not derive source positions by adding a decoded offset to the token start.

A diagnostic over decoded content covers the corresponding original source pieces. An interior decoded UTF-8 byte is not a scalar boundary or fabricated original-source offset. Exact maps can use compact runs without changing this relation.

Runtime formatted hole output has no invented character-by-character source map. The hole expression retains its actual original span. Canonical formatting and evaluation still follow the existing interpolation rules.

#### Literal boundary cases

These are independent lexical inputs/scenarios, not complete programs. All scanner/type/parser outcomes remain NotChecked by a Rill implementation.

| Case | Input or condition | Intended result |
| --- | --- | --- |
| S1 | `0xff32` and `0xff32u32` | Complete hexadecimal integer candidates. The first still needs width context. |
| S2 | `123name`, `1i32tail`, `1é`, `0b102`, `0Xff` | Invalid complete numeric candidates, R0002. No valid-prefix salvage. |
| S3 | `1e+`, `1__2`, `1_f32`, `1.2u8` | Invalid numeric spelling, R0002, before missing-width inference. |
| S4 | `.5`, `1.`, `1..2` | Dot/integer token sequences, not float/range tokens. Parser rejection is separate. |
| S5 | `1...rest` and `1.0f32.field` | Numeric token followed by spread/identifier or dot/identifier. No greedy decimal point. |
| S6 | `1e-2f32 - 1i32` | One exponent-bearing numeric token, separate minus, then integer token. Operand typing remains separate. |
| S7 | `128i8` inside `-(128i8)` | Retain the magnitude/token. The approved signed-minimum checker decides validity. |
| S8 | `r##"a"#b"##` | Raw content includes the partial close `"#`; the later two-hash close ends it. |
| S9 | `r#"x"##` | The raw literal closes after one hash. The extra hash is invalid subsequent source. |
| S10 | `r "x"` | Identifier plus ordinary string token. No raw opener or implicit concatenation. |
| S11 | `"${name}"` and `r"${name}"` | Literal dollar/braces/name text. Only backtick templates interpolate. |
| S12 | A template hole containing a named record and a quoted `}` | Only code-mode braces affect hole nesting. Quoted braces cannot close it. |
| S13 | A template containing `\${name}` | Literal `${name}` text, not a hole. |
| S14 | A template containing `\\${name}` | Decoded backslash followed by a real hole. Escapes act in source order. |
| S15 | `"\x414"` and `'\x414'` | String content is A then 4. The character literal rejects because it decodes two scalars. |
| S16 | `'\u{1F642}'`, `'e\u{301}'`, `'\u{D800}'` | One valid scalar; two-scalar rejection; surrogate rejection. |
| S17 | `"\q"`, `"\x80"`, `"\u{}"` | Closed literals with invalid escape contents, R0002. |
| S18 | Unescaped control bytes in comments/raw text | Rejection under the proposed control restriction, not silent removal. |
| S19 | Physical CRLF or lone CR in raw/template text | One decoded LF, with original source spans preserved. Escaped `\r` stays CR. |
| S20 | EOF in a raw string, template or interpolation hole | Unmatched lexical delimiter, R0001. No invented closing token or successful remainder. |

These rules specify confirmed scanner policies, not expression/type grammar or a verified scanner. The three confirmed lexical groups appear in RFC 0003. Full RFC 0006 acceptance remains separate.

### Logical text access, copies and static literals

String owns valid UTF-8 text. `&string` observes a whole string or substring with its own extent. It is not uniformly a pointer to an owner header. `&mut string` exclusively accesses a whole owner.

There are no mutable substring views or writable raw-byte escapes from text. Generic dereference and reborrow must preserve the observed extent. They must not fabricate ownership from a view.

`copy(&owner)` returns an independent owned copy of the text. `copy(view)` already receives `&string` and copies exactly the viewed extent. `copy(&*view)` observes that same extent through an explicit reborrow.

Copying the view descriptor instead preserves its source dependencies. `&&string` can borrow a stored descriptor. `&mut &string` changes a descriptor slot, not the backing text's permissions.

A view descriptor may pass by value through a local binding while retaining its backing source. A reference to a local descriptor slot cannot escape. Named owners must remain valid through all retained uses, including disposal and native completion.

Direct borrows of uninterpolated quoted, raw or backtick literals may use program-lifetime static backing. A template containing an interpolation remains computed even when constant folding could remove its work.

This exception does not extend a named local owner's scope. Static views still fail noBorrow(&string). Named functions still cannot capture startup lets. A static address does not grant ownership or copyability.

Uninterpolated literal expressions may be rematerialized as independent logical owners. A named string binding moves unless explicitly copied. Constant knowledge cannot duplicate ownership of freeable storage. No hidden reference counting follows.

The full judgments for backing sources, descriptor storage, retained mutation and exclusive restrictions need the ownership companion. This RFC records their required distinctions, not a complete borrow checker.

### Text operations and public error shapes

The following signatures describe required operations, not complete source declarations. Home names refer to standard modules. Library identities and actual representations remain required integration work.

| Home and operation | Signature and behavior |
| --- | --- |
| std::string length | `length(&string): u64`, counting UTF-8 bytes. |
| std::string slice | `slice(&string,u64,u64): Option<&string>`, with half-open byte bounds. |
| std::string append | `append(&mut string,&string): void`, preserving valid UTF-8. |
| std::convert text byte view | `into<&Bytes>(&string): &Bytes`, a read-only logical view. |
| std::convert borrowed validation | `tryInto<&string>(&Bytes): Result<&string,Utf8Error>`, retaining original byte storage. |
| std::convert owned text | `tryInto<string>(&Bytes): Result<string,Utf8Error>`, returning owned copied text. |
| std::string decodeLossy | `decodeLossy(&Bytes): string`, returning owned text with explicit replacement. |
| std::string traversal | `chars(&string): Chars`; `charIndices(&string): CharIndices`. Iterators retain their source. |
| std::convert scalar access | `into<u32>(char): u32`; `tryInto<char>(u32): Result<char,ScalarError>`. |
| std::bytes observation | `length(&Bytes): u64`; `get(&Bytes,u64): Option<u8>`. |

Text slices require ordered, in-bounds offsets at scalar boundaries. Empty valid ranges succeed. An empty range inside a multibyte scalar is not a valid endpoint pair. Failure returns None, not a truncated or repaired slice.

Byte lookup can return continuation bytes. There is no integer character indexing or string range-index syntax. Scalar traversal yields char values or CharAt records containing `offset: u64` and `value: char`.

Utf8Error is a named record with `validUpTo: u64` and `errorLength: Option<u64>`. It has canonical display. Errors own their diagnostic data rather than borrowing input storage. Confirmed decoder span details appear below.

Borrowed validation success retains the original byte owner and access restrictions. Copying decoding success does not retain an input borrow. Neither operation assumes List contiguity or creates a borrowed temporary List header.

Text equality compares exact UTF-8 content without consuming the operands. Ordering is lexicographic byte order. No normalization, case folding or locale collation occurs. Canonically equivalent different scalar sequences can compare unequal.

Append and other mutation cannot bypass conflicting views or self-aliasing restrictions. Bytes capacity/reuse and collection algorithms belong to their library contracts. This RFC supplies no new mutable UTF-8 byte interface.

<a id="proposed-invalid-utf-8-span-and-replacement-policy"></a>

### Invalid UTF-8 span and replacement policy

Strict validation reports the first invalid position in the observed byte view. validUpTo counts the maximal complete valid prefix from that view's start. It is not an absolute backing-buffer offset.

Accept only well-formed UTF-8 scalar encodings. Two-byte leads are C2 through DF. Three-byte leads are E0 through EF; four-byte leads are F0 through F4. Continuation bytes are 80 through BF, subject to the second-byte restrictions below.

| Lead byte | Permitted second byte | Remaining continuation count |
| --- | --- | --- |
| C2 through DF | 80 through BF | Zero. |
| E0 | A0 through BF | One. |
| E1 through EC, EE through EF | 80 through BF | One. |
| ED | 80 through 9F | One. |
| F0 | 90 through BF | Two. |
| F1 through F3 | 80 through BF | Two. |
| F4 | 80 through 8F | Two. |

The table's byte numbers are hexadecimal. ASCII bytes 00 through 7F are complete scalar encodings. These restrictions reject overlong forms, surrogate encodings and values above U+10FFFF.

For an invalid lead byte, report errorLength Some(1). For a valid lead with an invalid following byte, consume only its valid incomplete prefix. Report that prefix length as Some(length). Leave the offending next byte for subsequent decoding. Complete error spans have one, two or three bytes.

If the input ends inside an otherwise valid prefix, report errorLength None. The trailing prefix begins at validUpTo. Do not invent missing bytes or treat an incomplete sequence as valid text.

Lossy decoding copies each valid prefix, then emits one U+FFFD per reported error span. For an incomplete trailing prefix, emit one replacement and stop. Continue after a complete invalid span using the same rules.

For example, `E1 80 41` becomes replacement followed by A. `E0 80 80` produces three replacements because each span is independently invalid. `F0 90 80` at end produces one replacement for the incomplete trailing prefix.

The owner confirmed this maximal-prefix policy. Strict and lossy functions must share its span interpretation. Preserve valid NUL, BOM and CR/LF content rather than silently stripping or normalizing decoded text.

### Canonical interpolation and default formatting

Templates use canonical `std::format::show<T>(&T): string`. A same-spelled local family is unrelated. Generic interpolation requires canonical show(T) evidence before monomorphization. Missing support is not a reflected-record fallback.

Evaluate an interpolation expression once, then format it immediately. Finish that formatting before evaluating the next expression. Output concatenation preserves this left-to-right order. Show may suspend or have effects.

Observe existing values through read access rather than moving them merely for display. Standard explicit reference-forwarding implementations cover shared references and read-only observation of exclusive references. This is not general automatic dereference dispatch.

Normal temporary cleanup still applies. Do not unlock a temporary guard early to permit the next interpolation. Keep owners, guards and native inputs valid through formatter execution and required cleanup.

Show returns owned text that retains no input borrow after completion. That does not authorize premature input release during a suspended formatter. Primitive formatters must bootstrap without recursively interpolating their own primitive input.

| Value category | Required default output |
| --- | --- |
| String and char | Text without added quotes or escaping. |
| Bool | `true` or `false`. |
| Integer | Decimal digits, with minus for negative values and no width suffix. |
| Finite float | A shortest decimal accepted by the default parser that round-trips at the same established width. |
| Float special values | `NaN`, `inf`, `-inf`, and `-0` for negative zero. |

NaN payloads need not round-trip. The default parser accepts the formatter's numeric output at its known destination width. Shortest output uses that parser's grammar, not rejected shorthand such as `.5`. Canonical notation/tie selection still needs precise specification before readiness.

Canonical error display is a separate family. Standard wrapper show can bridge to error display and render explicit origins/causes. This does not rename show or add propagation traces. No template format mini-language is introduced.

### Required companion rules

| Rule owner | Required contribution | Current dependency state |
| --- | --- | --- |
| [Draft RFC 0002](0002-modules-and-program-loading.md) | Module/prelude identity, imports, privacy and original source snapshots. | Its policies are confirmed; full resolver rules remain incomplete. |
| [Draft RFC 0003](0003-expressions-bindings-and-control-flow.md) | Grouping, evaluation, short-circuiting, control transfers and skipped branches. | Its four policy groups are confirmed; complete grammar/flow checking remains incomplete. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) | Generic evidence, function values and ordinary family rules. | Its three precision policies are confirmed; the complete evidence engine remains missing. |
| [Draft RFC 0005](0005-types-records-enums-and-inference.md) | Type identity, generic construction, tuples/Lists and finite representation. | Its three precision policies are confirmed; formal checking remains incomplete. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) | Logical places, backing/descriptor dependencies, copies, cleanup and exclusive restrictions. | Proposed, not accepted. Complete route/projection and cleanup judgments remain incomplete. Text signatures are not a borrow proof. |
| Draft RFCs [0008](0008-errors-propagation-and-caused-by-wrapping.md) and [0009](0009-user-implementable-conversions.md). | Canonical pair coherence, exact E, Result/Err metadata and construction origins. | Both remain Draft. Complete origins/evidence judgments and conversion coherence remain incomplete. This RFC owns numeric pair values, not general conversion selection. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md), with lexical/runtime integration incomplete. | Token boundaries, source maps, statuses, LLVM/native/Wasm lowering and precise limits. | Reporting is proposed, not accepted. [#28](https://github.com/Reilley64/rill/issues/28) retains formal/production acceptance; [#42](https://github.com/Reilley64/rill/issues/42) retains full editor delivery. |

## Valid/invalid examples and diagnostics

N1 through N3 appear in the guide. Each following case is independent unless it names another context. Outcomes are expected, not observed Rill results. Fatal cases require separate processes once execution exists.

Case N4 compares literal forms under the confirmed spelling policy. `1_000i32` and `0xCA_FEu32` are valid. `1__0i32`, `0x_FFu32` and `1_i32` are rejected. `0xff32` needs integer context; it is not an f32 literal.

Case N5 compares negation. `-(128i8)` is a confirmed valid minimum spelling. `128i8`, `-129i8` and `-0u8` reject statically. `-(-128i8)` is well-typed but panics when evaluated.

Case N6 separates typed traps from literals. `if (false) { let value = 127i8 + 1i8; }` need not panic or reject. Replacing its initializer with `256u8` rejects the program, R0002. Executed overflow remains fatal even if value is unused.

Case N7 compares numeric operators. `255u8 << 1u32` yields 254u8. `1u8 << 8u32` panics. `1u8 << 1i32` has a type mismatch, R0100. Integer `-128i8 % -1i8` panics despite the mathematical remainder being zero.

Case N8 describes floating outcomes. `1.0f32 / 0.0f32` yields infinity. `0.0f32 / 0.0f32` yields NaN. `-5.5f64 % 2.0f64` yields -1.5f64. Floating `%` with a zero divisor yields NaN, not an integer panic.

Case N9 compares numeric conversion and parsing. `into<u8>(1i32)` is unavailable because the source base range is not contained. `tryInto<u8>(300u16)` returns OutOfRange failure. `tryInto<u8>(-0.5f64)` succeeds with zero. Expected numeric E is ConversionError, not an automatically chosen application error.

Case N10 is a strict-parser scenario. Parsing `+42`, ` 42`, `1_000`, `0xff` or unsigned `-0` returns InvalidSyntax under the confirmed grammar. Parsing `128` as i8 returns OutOfRange. Parsing `1e2` as i32 returns InvalidSyntax rather than 100.

Case N11 describes float parsing. At f32, `-0` retains negative zero and `inf` is accepted. `+inf`, `nan`, `.5` and `1.` are rejected under the confirmed grammar. Finite `1e100` returns OutOfRange at f32. Numeric narrowing can instead produce infinity under its separate approved contract.

Case N12 is a complete module:

```rill
import std::string::{length, slice};
function invalidBoundary(): bool {
  guard slice(&"é", 1u64, 1u64) else { return true; }
  return false;
}
expect length(&"aé") == 3u64;
expect invalidBoundary();
```

The empty byte range is inside a scalar, so its endpoints are invalid. A zero-length slice at a genuine boundary instead succeeds.

Case N13 is a generic copy excerpt with visible canonical copy:

```rill
function cloneText<T>(value: &T): T where copy(T) {
  return copy(value);
}
```

With T=string and a substring input, the result owns exactly the view's text. With T=&string and a borrowed descriptor, copying retains the descriptor's backing dependency. Neither operation fabricates a new owner header.

Case N14 is a rejected complete function excerpt:

```rill
function badOwner(): &string {
  let local = "temporary";
  return &local;
}
```

The named owner ends at function exit, R0202. Literal initialization does not give that owner's borrow static duration. Returning `&r#"temporary"#` directly can use the explicit literal exception.

Case N15 describes descriptor storage. Returning an input-derived view by value through a local descriptor can preserve its source. Returning a reference to that local descriptor slot cannot, R0202. Retaining a local owner's view in caller-owned mutable storage is also invalid.

Case N16 describes literal text. Raw `r#"${text}"#` does not interpolate. Ordinary `"\x80"` is not a valid text escape because x escapes are ASCII-only. A surrogate Unicode escape and a multi-scalar char literal reject, R0002. Composed é and decomposed e-plus-combining-accent remain different text.

Case N17 describes the confirmed UTF-8 spans. Input bytes `E1 80 41` report validUpTo zero and errorLength Some(2). Lossy output contains one replacement followed by A. Input `E0 80 80` has first errorLength Some(1) and produces three replacements.

Case N18 describes truncated UTF-8. Input `61 F0 90 80` reports validUpTo one and errorLength None. Strict decoding fails. Lossy decoding returns a followed by one replacement. Decoding valid CR/LF bytes does not normalize them.

Case N19 is a complete interpolation module:

```rill
function replace(owner: &mut string): i32 {
  *owner = "after";
  return 2i32;
}
function snapshots(): string {
  let mut owner = "before";
  return `${owner}:${replace(&mut owner)}:${owner}`;
}
expect snapshots() == "before:2:after";
```

Formatting completes before the next interpolation expression runs. The first formatter's input access does not extend merely because the final output owns earlier text. Resource-valued temporaries still keep their separate cleanup obligations.

Case N20 describes unrelated names and generic evidence. A local unrelated show cannot format a private record for a template. A generic template using T requires canonical show(T). Missing evidence rejects, R0302. Primitive formatters cannot bootstrap through the same template operation they implement.

Case N21 describes scalar conversion. `tryInto<char>(0xD800u32)` returns InvalidScalar failure. `tryInto<char>(0x10FFFFu32)` succeeds with a valid scalar. `into<u32>('é')` yields 0xE9u32. No infallible u32-to-char pair or grapheme index follows.

Case N22 describes byte/List conversion. An owned List<u8> moves into Bytes; owned Bytes moves into List<u8>. Borrowed forms produce independent owned copies. They do not assume storage reuse, List contiguity or an automatic mutable-source adapter.

Case N23 describes conversion views. `into<&Bytes>(&text)` retains the text's observed extent and backing source. `tryInto<&string>(&bytes)` validates without copying and retains the byte owner. By contrast, `tryInto<string>(&bytes)` returns independently owned text. Returning either borrowed result from a destroyed local owner remains invalid, R0202.

| Case / artifact and revision | Context | Expected outcome | Owning rule | Evidence label / observed result |
| --- | --- | --- | --- | --- |
| N1/N4/N5/N6/N7, this draft | Complete scalar module and separate rejected/fatal expressions. | Preserve explicit widths and trap timing. Apply confirmed literal spelling without unsigned negation. | [Literals](#proposed-numeric-literal-spelling), [integers](#integer-operations-and-traps), [timing](#static-invalidity-versus-evaluated-failure). | NotChecked. No lexer, checker or runtime result. |
| N3/N8/N9, this draft | Parsing helper, floating expressions and conversion scenarios. | Preserve IEEE, full-base-range availability and truncation-before-check. | [Floats](#floating-operations), [numeric conversions](#protected-numeric-conversions-and-explicit-arithmetic-helpers). | NotChecked. No native/Wasm numeric acceptance. |
| N10/N11, this draft | Explicit input text and numeric destinations. | Return the specified syntax/range outcomes under the confirmed grammar. | [Parser](#proposed-strict-decimal-parser-grammar). | NotChecked. No parser has executed this grammar. |
| N2/N12/N13/N14/N15, this draft | Substring module, scalar boundaries and reference/descriptor contexts. | Preserve exact extents and source dependencies. Reject local-source escape. | [Logical access](#logical-text-access-copies-and-static-literals), [text APIs](#text-operations-and-public-error-shapes). | NotChecked. Full logical-place/ownership checking remains missing. |
| N16/N21, this draft | Literal and runtime scalar cases. | Reject invalid literal scalars. Use selected ScalarError for invalid u32 conversion and preserve valid noncharacters. | [Text syntax](#characters-and-text-literal-syntax), [core pairs](#canonical-core-conversion-interface). | NotChecked. No scalar/literal implementation. |
| N17/N18, this draft | Explicit malformed byte sequences. | Use the confirmed strict spans and matching lossy replacements. | [UTF-8 policy](#proposed-invalid-utf-8-span-and-replacement-policy). | NotChecked. No decoder/reference model was run. |
| N19/N20, this draft | Complete template module and generic/namesake scenarios. | Format sequentially with canonical evidence and preserved cleanup. | [Interpolation](#canonical-interpolation-and-default-formatting). | NotChecked. No template lowering or capture checker. |
| N22/N23, this draft | Byte/List and borrowed-view conversion scenarios. | Distinguish consuming, independent-copy and source-retaining output contracts. | [Core pairs](#canonical-core-conversion-interface). | NotChecked. Pair metadata and ownership checking remain unimplemented. |

The [diagnostic registry](../diagnostic-codes.md) fixes the cited meanings, not exact output bytes. Primitive operand admission and literal syntax still need complete diagnostic integration. Expected runtime failures are not compile-time failures or passing expectations.

## Feature interactions

### Types, functions and control flow

[RFC 0005](0005-types-records-enums-and-inference.md) owns base type identity and literal/container contexts. This RFC owns primitive values and literal/operation behavior. Neither context nor generic defaults can invent a numeric width.

[RFC 0004](0004-functions-overloads-and-generic-requirements.md) owns ordinary family ranking and symbolic evidence. Numeric operators are not an implicit user operator-family feature. Canonical conversions retain their narrow destination/source exception.

[RFC 0003](0003-expressions-bindings-and-control-flow.md) owns grouping, short-circuiting and control targets. A skipped well-typed trap remains skipped. Infix error wrapping remains lazy wrap-and-propagate, not numeric fallback or parsing recovery.

[RFC 0001's accepted snapshot](https://github.com/Reilley64/rill/issues/28#issuecomment-6907867211) owns expectation execution and trusted observations. Expectations add no default width, implicit show requirement or user formatting callback to diagnostic capture.

### Ownership, errors and runtime boundaries

Owned output is not permission to shorten a formatter's input dependency. String views, byte views, descriptor slots and native source retention need the same actual backing owners through all uses.

Typed primitive errors retain the standard Err envelope and construction-origin contract. Different ParseError, Utf8Error and ConversionError values do not implicitly convert to each other. Explicit adaptation or causal wrapping keeps its own evidence obligations.

Source CRLF normalization changes literal values, not original source identities or coordinate units. Runtime decoding preserves input line endings. Source maps must keep those domains separate.

LLVM/native/Wasm lowering must preserve checked integer timing, exact conversion boundaries and IEEE behavior. Host instruction defaults, saturating casts or masked shifts cannot substitute for this contract. Separate target acceptance remains required.

### Source reconciliation

| Input | Disposition |
| --- | --- |
| [Core conversion interface decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6913810445) | Consolidate strict parsing/owned decoding under tryInto and audit related core helpers for predictability. The matrix and ScalarError are concrete draft details under that direction. |
| [RFC 0006 policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6913372816) | The owner confirmed literal spelling, strict decimal grammar and UTF-8 spans. The later interface decision resolves strict parsing through tryInto. Broader core pair details follow the owner's predictability direction. |
| [#18 confirmation](https://github.com/Reilley64/rill/issues/18#issuecomment-6869848603), [ADR 0011](../adr/0011-explicit-numeric-widths-and-utf8-string-views.md) | Preserve explicit widths, arithmetic, conversions, logical views, text/scalar behavior and sequential interpolation. The later decision confirms literal/parser/decoder precision. Do not import unrelated host extensions. |
| #19 and [ADR 0012](../adr/0012-enums-replace-unions-and-literal-overloads.md) | Remove singleton literal refinements and copy exceptions based on named constant knowledge. Preserve base types and direct-literal materialization without duplicate owners. |
| #23 and [library catalogue](../standard-library-contracts.md), sections 2/8/9 | Use current borrowed copy, Bytes views, error shapes, parsing, formatting and six explicit integer helpers. Concrete integration and property evidence remain incomplete. |
| [#41 confirmation](https://github.com/Reilley64/rill/issues/41#issuecomment-6886119679), [ADR 0017](../adr/0017-user-implementable-conversions.md) | Use into/from and tryInto/tryFrom with selected E. Numeric pairs remain protected. General pair coherence and user conversions still need their own RFC. |
| #11/#15/#16 and ADRs 0005/0007/0009 | Preserve fatal integer traps, runtime evaluation, retained dependencies and cleanup. Floating division does not use historical integer-only zero-divisor panic wording. |
| #24 and [diagnostic contracts](../diagnostic-contracts.md) | Separate invalid literals, missing widths, mismatches, dependency errors and trap warnings. Preserve original byte coordinates despite normalized literal content. |
| [Numeric/text source](../../prototypes/08-numbers-strings.rill) and [case inventory](../prototypes/08-numbers-strings.md), source revision | Supporting design inputs cover 41 intended-valid expectations and independent negative/runtime cases. They are not compiler-executed tests. No artifact bytes change here. |

## Compatibility and migration

Historical int/float/byte placeholders require the approved concrete types. Earlier primitive singleton inference and named-string copy exceptions do not return. No as conversion/assertion syntax is added.

Use current camelCase tryInto/tryFrom rather than historical snake_case aliases. The later user-extensible conversion design does not change numeric availability, rounding or ConversionError behavior.

Historical `copy(*view)` relied on the old copy-call convenience. Current copy borrows its input. Use `copy(view)` or `copy(&*view)` for owned text. Descriptor copying instead preserves its source dependency.

The confirmed separator, signed-minimum, parse-sign and UTF-8 grouping rules fix source precision gaps. They still need fixture reconciliation. A source example's existence does not prove prior acceptance of these details.

The [interface decision](https://github.com/Reilley64/rill/issues/28#issuecomment-6913810445) supersedes separate public parseNumber/decodeUtf8 entry points. The audited matrix also replaces listed byte/scalar conversion helpers. It does not retain duplicate compatibility aliases.

| Former public call | Current draft spelling |
| --- | --- |
| `std::number::parseNumber<N>(&text)` | `tryInto<N>(&text)`. |
| `std::string::decodeUtf8(&bytes)` | `tryInto<string>(&bytes)`. |
| `std::string::viewUtf8(&bytes)` | `tryInto<&string>(&bytes)`. |
| `std::string::bytes(&text)` | `into<&Bytes>(&text)`. |
| `std::char::codePoint(value)` | `into<u32>(value)`. |
| `std::char::fromCodePoint(value)` | `tryInto<char>(value)`, with Result/ScalarError rather than Option. |
| `Bytes(values)` | `into<Bytes>(values)`. |
| `std::bytes::intoList(bytes)` | `into<List<u8>>(bytes)`. |
| `std::bytes::copyFromList(&values)` | `into<Bytes>(&values)`. |
| `std::bytes::copyToList(&bytes)` | `into<List<u8>>(&bytes)`. |

Import into/tryInto explicitly from std::convert. Migrate Option-based scalar failure handling to Result handling. When the old Bytes factory supplied u8 context, add explicit element suffixes or a List<u8> annotation before conversion.

Prototype sources and authored reports remain byte-preserved evidence of their source revision. Their old helper names require migration under the new interface. The prototype index records this distinction without rewriting old results.

No released numeric/text ABI exists. This draft does not select string header layout, allocator, native codec or LLVM lowering library. Native and Wasm implementations must demonstrate the same promised semantics.

## Security/resource implications

Malformed UTF-8, numeric overflow and u64 offsets must not become native-size wraps or out-of-bounds access. Validate before pointer/index conversion. Do not infer a safe contiguous List representation from its spelling.

Lossy decoding can expand output because each replacement uses three UTF-8 bytes. Parsing can receive arbitrarily long text; interpolation can allocate growing output. No constant memory, hard latency or allocation-success guarantee is promised.

Strict errors must own required diagnostic data without retaining secret input unnecessarily. This is not a promise to print raw input bytes. Diagnostics and runtime error formatting keep their separate bounded-data obligations.

Static text views remain borrows. A raw string or constant-folded template cannot certify noBorrow, duplicate exclusive access or bypass named-function capture restrictions.

No parser/decoder model or source inspection proves memory safety, Unicode correctness or target support. Ownership, allocation, native release and supported-profile tests remain required.

## Rationale, alternatives and prior art

The confirmed scanning policy rejects a complete malformed numeric candidate rather than salvaging a valid prefix. Explicit modes keep braces/quotes inside comments and literals from changing outer interpolation boundaries. Original-byte provenance prevents CRLF/escape decoding from shifting diagnostic locations.

Implicit default widths would reduce annotations but make arithmetic depend on guessed types. The approved #18 decision instead requires immediate concrete context or suffixes. Checked integers and explicit wrapping helpers also avoid optimization-mode changes.

Permissive host parsers accept different whitespace, signs and special values. The confirmed strict grammar fixes the accepted default input language. More permissive or radix APIs can be separate proposals, not silent extensions.

The confirmed UTF-8 span policy makes strict errors and lossy replacement agree. Dropping bad bytes would lose information without marking it. Consuming the offending next byte could also discard a valid following scalar.

A canonical default needs a stable meaning, not merely a total implementation. Approved numeric precision loss does not make lossy UTF-8 replacement the default text conversion. Keeping decodeLossy explicit preserves that policy choice.

Logical string views preserve subtext without a public String/str split. Assuming a uniform owner header would invalidate exact-extent copies and descriptor reasoning. This still needs explicit ownership and lowering rules.

[ADR 0011](../adr/0011-explicit-numeric-widths-and-utf8-string-views.md) records the source tradeoffs. The catalogue and cited confirmed decisions are provenance, not proof that another language's parser or numeric conversion table matches Rill. No external implementation is adopted here.

## Implementation implications

The lexer/type checker needs exact literal magnitudes, category context and the signed-minimum rule. It must not type literals by first rounding them through f64. Constant folding needs explicit effect/trap preservation.

Integer lowering needs overflow and divisor guards where target instructions differ. Shift lowering must check the full u32 count. Float lowering must preserve subnormals, sign and non-finite behavior without result-changing fast-math transformations.

Conversion lowering needs exact integer-range relations and safe float/integer boundary checks. Protected numeric registration must remain canonical through aliases and generic evidence. The conversion companion still owns full candidate selection.

Text lowering needs logical places that distinguish owner, extent and descriptor storage. Strict and lossy decoding need one shared span policy. Input ownership, borrowed success and independent copying output must stay distinct.

Interpolation needs canonical show evidence, immediate sequential formatting and normal temporary cleanup. Primitive formatter bootstrap and exact shortest-float selection remain required work. The scaffold implements none of these product phases.

## Evidence and checking plan

Cases S1 through S20 are new independent lexical inputs/scenarios from 2026-11-17. Their scanner/type/parser behavior remains NotChecked by Rill. The proposal adds no lexer implementation, host-parser comparison or language execution.

### Actual evidence

| Artifact / revision | Evidence label | Rule/question exercised | Checker/tool and command/environment | Expected / observed outcome | Limits, negative/unavailable results and gate links |
| --- | --- | --- | --- | --- | --- |
| Source decisions and repository base `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Current numeric/text rules and later copy/conversion supersession. | Drafting agent, 2026-11-16. Read current #18 decision/body, ADR 0011, catalogue sections 8/9 and the source inventory. | The drafting agent identified later singleton, borrowed-copy and user-conversion refinements as inputs. | This is source reconciliation, not a checker/runtime proof. |
| N1 through N23, this draft | NotChecked | Literals, arithmetic, conversions, parsing, views and interpolation. | No Rill lexer, checker or runtime is available. | Expected outcomes appear above. No Rill result is observed. | No case is RillExecuted. Fatal cases have not run in isolated processes. |
| Confirmed grammar and UTF-8 policies | NotChecked | Separators, signs, decimal parsing and malformed byte spans. | No parser/decoder/reference model has been run for this draft. | No formal grammar completeness or codec result is claimed. | Policies are confirmed. Positive/negative tests remain missing. |
| Existing source and research artifacts | NotChecked | Executable acceptance of the relevant Rill toolchain. | Artifacts retain their own historical evidence scopes. | No new reference or Rill execution was performed here. | Host-library behavior or old native experiments do not validate this RFC. |
| RFC documents and index, local working draft | ToolChecked | Structure, writing limits, local links and preservation of earlier rules/artifacts. | Drafting agent, 2026-11-16. Temporary Python checks; `mise run check:docs`. | Six file/register identities, required metadata/headings, sentence/paragraph limits and local destinations passed. Earlier RFC examples and accepted rules stayed unchanged. | Document checks do not parse Rill, test primitive behavior or audit the formal ASD-STE100 dictionary. |

### Planned checks. Not executed evidence

- Parse all literal bases, suffixes, separators, exponents and signed-minimum boundaries.
- Reject missing widths without guessing from expected conversion destinations.
- Distinguish invalid literals from skipped or evaluated constant traps.
- Test every integer width's overflow, zero-divisor, MIN/-1 and shift-count edges.
- Test floating signed zero, NaN, infinity, subnormals, fmod and forbidden result-changing transformations.
- Check all numeric pair availability from complete base ranges.
- Test i128/u128 boundaries without lossy float comparisons and preserve truncate-before-check.
- Test strict parser syntax, whole-input consumption, error classes and formatter roundtrips.
- Test quoted/raw/template contents, scalar escapes and source/runtime CRLF differences.
- Validate logical copy/reborrow and descriptor dependencies through generic functions and mutable storage.
- Compare strict and lossy UTF-8 spans for every lead/continuation boundary and truncated prefix.
- Preserve byte-view offsets, valid BOM/NUL data and scalar-boundary slice checks.
- Test interpolation effects, canonical names, suspension and resource-temporary cleanup.
- Validate diagnostics, source coordinates and native/Wasm behavior on supported profiles.

Runtime/compiler suites must execute these cases before implementation/support claims. Document checks cannot substitute for their missing results.

## Open/deferred questions

<a id="review-choices"></a>

### Confirmed policies

The owner [approved all three precision policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6913372816). Literal spelling, decimal parser grammar and UTF-8 span grouping are no longer open choices. This does not accept the complete RFC. The later interface decision resolves the public parsing entry point.

<a id="parsing-and-conversion-api-review"></a>

### Confirmed interface direction and audit details

The owner [approved strict parsing/owned decoding through tryInto and requested predictable core conversions](https://github.com/Reilley64/rill/issues/28#issuecomment-6913810445). That entry-point question is resolved. The [core matrix](#canonical-core-conversion-interface) applies the direction to related text, byte/List and scalar helpers.

The broader matrix and ScalarError are draft details under that direction, not a separate full RFC acceptance. Check their ownership/evidence and migration rules before readiness. The approved literal, numeric, parsing and UTF-8 semantics remain unchanged.

### Remaining in-scope authoring work

The [three lexical/source groups](0003-expressions-bindings-and-control-flow.md#confirmed-lexical-policies) are confirmed. Numeric/text spelling approvals remain unchanged. Complete parser grammar and actual scanner validation remain separate.

The shared syntax annex confirms parser/literal boundary policies. Complete remaining primitive operand admission separately. Complete mixed text access judgments and canonical shortest-float notation/tie selection. Do not leave observable formatting freedom hidden inside a host implementation.

Formalize exact literal/parse rounding and conversion lowering across all widths. Complete UTF-8 span proof/tests, logical backing/descriptor dependency rules and formatter cleanup with the required companions.

Identify exact dependency revisions and complete the [canonical readiness review](README.md#canonical-reviewready-checklist). Supporting ADRs and catalogue tables do not replace missing normative rules.

### Implementation/validation gates and future scope

Implement and validate the parser, numeric checking/lowering, text storage, decoder, primitive formatter and diagnostics. Keep compiler/runtime, native/Wasm and editor acceptance separate from policy approval.

This RFC adds no arbitrary-precision numeric types, general casts, mutable substring views, locale collation, grapheme indexing or template formatting mini-language. Explicit radix parsing and extra formatting/math APIs remain outside this required interface.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. This document remains Draft. The drafting agent checked sentence/paragraph limits and applied unslop as the final prose pass. Structure and link checks passed. Formal ASD-STE100 dictionary compliance remains unverified. These checks cannot complete missing semantics or implementation evidence.

### Human decisions and amendments

On 2026-11-17, the owner requested the complete syntax grammar. The owner then [confirmed all three syntax groups](https://github.com/Reilley64/rill/issues/28#issuecomment-6931939690) at the two identified annex snapshots. Existing lexical/source and semantic approvals retain their exact scopes. Full annex/RFC acceptance, readiness and implementation remain separate.

On 2026-11-17, the owner requested completion of the lexical/source contract. The owner then [confirmed scanner modes, candidate boundaries, controls and lone-CR handling](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825). This is policy approval, not full RFC acceptance or scanner validation.

The project owner requested the next foundation RFC on 2026-11-16. This authorizes drafting Numbers, strings and primitive operations. The drafting request did not approve new policies. The owner later [confirmed all three precision policies](https://github.com/Reilley64/rill/issues/28#issuecomment-6913372816). No complete RFC revision has been accepted.

| Date / actor | Kind and scope of change | Reviewed revision or snapshot | Decision link / rationale |
| --- | --- | --- | --- |
| 2026-11-16, drafting agent | Created substantive Draft RFC 0006, register and partial coverage entries. | Source base `f33dcf4cf3f815824eeb897135775d7884f65c1a` and current source decisions cited above. This is not a reviewed RFC snapshot. | User request to continue to the next foundation RFC. No new policy or full RFC was accepted at creation. |
| 2026-11-16, project owner | Approved literal spelling, decimal parser grammar and UTF-8 span policy. Opened API review for parsing and other conversions through tryInto. | Three recommendations presented in chat. Pre-recording draft SHA-256: `47d3a1de7b6b48503e790c007916caf7b9db0310e1aef4297a0b8f5a7b338909`. No complete RFC revision was accepted. | [Policy approval and API question](https://github.com/Reilley64/rill/issues/28#issuecomment-6913372816). Status remains Draft. At that point, conversion registration changes remained undecided. |
| 2026-11-16, project owner | Approved strict number parsing and owned UTF-8 decoding through tryInto. Requested predictable use of into/tryInto for related core conversions. | The preceding recommendation and the owner's follow-up direction. Pre-interface draft SHA-256: `be423fc01f6cfd4d3bd626746e1303b6df7aae99a8caf50778e2d606ecb60936`. | [Interface decision and audit scope](https://github.com/Reilley64/rill/issues/28#issuecomment-6913810445). Full RFC remains Draft. Concrete matrix and ScalarError still need specification review and implementation validation. |
| 2026-11-17, project owner | Approved the coordinated lexical/source groups, including literal scanning and content provenance. | Pre-recording SHA-256: `3c2142924cc6fd7065529ec42f489b4781c8eea77be4c07289ea635af22a3a15`. The shared decision identifies the other owner snapshots. | [Lexical policy approval](https://github.com/Reilley64/rill/issues/28#issuecomment-6929923825). Full RFC acceptance, parser integration, readiness and actual scanner/target validation remain open. |
