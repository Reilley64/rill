# Explicit numeric widths and UTF-8 string views

**Later refinements:** [ADR 0012](0012-enums-replace-unions-and-literal-overloads.md) (#19) removes the historical singleton types/literal refinements and their copy exceptions below, without restoring default widths or changing numeric availability/UTF-8 semantics. [ADR 0015](0015-language-required-library-contracts.md) (#23) finalizes byte/parser/conversion/format interfaces and `copy(&value)`; for an existing `&string` view, `copy(view)` copies its text. Historical examples remain design inputs where not superseded, not executed tests.

[Issue #18](https://github.com/Reilley64/rill/issues/18) adopts explicit-width numbers, checked integer operations, and one primary owned `string` spelling with borrowed text views. This takes Rust's explicitness without default numeric widths, a public String/str split, conversion traits, or programmer-written lifetimes. The consolidated interview and two consistency follow-ups are approved; the combined checker/runtime remains a proposal, not an implementation or proof.

## Numeric types and inference

The numeric primitives are `i8`, `i16`, `i32`, `i64`, `i128`, `u8`, `u16`, `u32`, `u64`, `u128`, `f32`, and `f64`. Signed integers use two's complement; floats use IEEE binary32/binary64. There are no built-in `int`, `float`, `byte`, `isize`, or `usize` aliases. Internal target-sized runtime values are not a public numeric default.

A literal needs a width from an annotation, known parameter/return context, or suffix. Never guess a fallback or smallest-fitting type. `let count = 10;` is rejected; `let count: i32 = 10;` and `let count = 10i32;` work. Unannotated immutable literal bindings retain precise refinements with established bases; mutable bindings widen to those bases. Explicit annotations choose the declared type. Initializer widening does not permit mutating an explicitly singleton-typed field as its unrestricted base.

Use decimal integers, `0b`/`0o`/`0x`, underscores, decimal floating fractions/exponents, and width suffixes. Out-of-range individual literals are compile errors. Check the negated value for a signed-minimum literal: `-128i8` is valid, `128i8` and `-129i8` are not. Unsigned unary negation, including `-0u8`, is invalid. Float literals round nearest, ties-to-even; finite literal overflow to infinity is rejected, while underflow can round to zero.

## Operations and observable traps

Already-typed numeric values do not implicitly convert. Ordinary arithmetic operands use the same concrete numeric type; literal refinement to its established base is not a cross-width conversion.

Integer arithmetic is checked in every build. Overflow, division/remainder by zero, and signed MIN divided/remainder by -1 panic. Division truncates toward zero and remainder has the dividend's sign. Wrapping/saturation require explicitly named operations, not optimization-mode changes.

Integer bit operations are `&`, `|`, `^`, `~`, `<<`, `>>`. Shift counts have type `u32`, providing context for literal counts without a numeric default. A count at least the left type's width panics. Left shifts discard outgoing bits; signed right shifts are arithmetic and unsigned right shifts logical.

Floats support NaNs, infinities, and signed zero with IEEE arithmetic/comparisons, nearest/ties-to-even rounding, and no implicit result-changing reassociation/fusion. Floating division by zero does not panic. Floating `%` is truncating fmod-style remainder, not nearest-quotient IEEE remainder. NaN payload bits are not a portable guarantee. This explicitly narrows ADR 0005/#11's old divide-by-zero panic wording to integer operations.

**Literal validity and expression evaluation differ.** `256u8` is a compile error, but `127i8 + 1i8` is well-typed and panics only if evaluated. The same applies to well-typed zero-divisor and invalid-count expressions, including in branches never executed. No mandatory arithmetic constant-evaluation/rejection regime or arbitrary user-function const execution is introduced. Warnings may identify a trap; folding must preserve program acceptance, effects, and panic timing, neither hoisting nor discarding observable traps. A valid constant floating expression can produce infinity although an overflowing individual finite float literal is rejected.

## Two conversion operations, four function names

No `as` conversion/assertion syntax. The free functions `from`/`into` alias one canonical infallible standard operation; `try_from`/`try_into` alias one canonical fallible operation returning `Result`. Both aliases share availability and behavior. The try operation also accepts infallible pairs, returning `Ok` with the same result shape.

- Integer infallibility means the destination contains the source **base type's entire range**. i32 to i64 and u32 to i64 work; i64 to i32 and signed to unsigned require try, even for known fitting values/refinements.
- Integer-to-float and float-to-float use IEEE rounding and may lose precision or narrow to infinity. Under this approved policy they do not error: infallible does not mean lossless, nor match Rust's exact From table.
- Float-to-integer truncates toward zero, then checks the integer result fits; NaN/infinity fail. Thus `try_into<u8>(-0.5f64)` succeeds with zero. Do not range-check the original fraction instead of the truncated result or use a lossy intermediate that accepts an overflowing integer boundary.

Use `into<To>(value)` or `try_into<To>(value)`, with source inferred from an already-typed input. An explicit expected type may uniquely establish the destination; no default is chosen. Trailing generic arguments can be inferred after supplied leading arguments. An independent source is not inferred from the destination: `into<u8>(10)` lacks a source width; a width-established source still needs an available pair. An unavailable infallible conversion is a compile error, not a runtime panic. A valid `try_into<u8>(300u16)` returns `Err`, even when constant; `300u8` is an invalid literal.

These are closed compiler-known numeric operations, not a collection of ordinary bodies distinguished by return types. Generic code needs canonical `where into(To, From)` or `where try_into(To, From)` evidence before monomorphization; aliases identify the same requirements. Other needed properties use their normal requirements, without implicit constraint grouping. No user-defined conversions/traits or return-only overload exception are added; domain conversions use explicit functions/factories. Public signatures/error types and metadata are #23/#28 obligations. Conversion functions are not added to the fixed prelude.

## Owned strings and logical references

`string` owns valid UTF-8 text. `&string` is read-only logical access to a whole text or substring with its own extent, not uniformly a pointer to an owned buffer header. `&mut string` exclusively accesses a **whole owned string**, supporting replacement/growth/UTF-8-preserving edits. No mutable substring views or writable raw-byte bypass in v0.

Generic dereference/reborrow/copy must respect that logical access. `*view` cannot move or fabricate an owner; `copy(*view)` produces independent owned text of exactly the viewed extent. Copying the view copies its descriptor and dependencies. `&*view` preserves extent/backing access; `&&string` borrows a stored descriptor, while `&mut &string` changes the descriptor slot, not its backing text. No general user-defined Deref mechanism or automatic view-to-owned conversion is added.

ADR 0007's dependency rules remain, including retained mutable storage, nested references, local-source exit checks, and conservative exclusive retention for read-only results from exclusive input. Input-derived views may pass through local descriptors; references to those local descriptor slots cannot escape. Conflicting owner moves/disposal/mutation remain excluded while views can be used. Scheduling/cancellation/native work must retain all backing owners/guards through completion or joined cleanup under ADR 0009. Owned string is borrow-free while views, including static views, are not; transfer/sharing follow the existing separate predicates, not copyability.

## Literals, characters, and copyability

`char` is a four-byte Unicode scalar: U+0000 through U+10FFFF excluding surrogates. Unassigned/non-printable values are valid; a scalar is not necessarily a grapheme and encodes to one through four UTF-8 bytes. Single-quoted literals contain one scalar after escape processing. Explicit Unicode queries yield u32 code points; checked construction from u32 returns `Option<char>`. These are distinct from UTF-8 encoding and numeric into conversions.

Use double-quoted strings, backtick templates, and Rust-style raw strings with matching hash delimiters. Raw text has no escapes/interpolation and is not a new string type; no raw identifiers or byte-string literal forms are introduced. Normal text escapes include newline/carriage return/tab/NUL, quotes/backslash, ASCII-only `\xNN`, and valid-scalar `\u{...}`. Templates can escape the interpolation opener. Ordinary quotes reject unescaped physical line breaks; raw strings/templates support multiline text without dedent. Source CRLF normalizes to LF, including literal breaks; escaped CR and runtime-decoded line endings are preserved. This is not Unicode/content normalization.

Direct borrows of uninterpolated literals may use program-lifetime static backing, an explicit exception to ordinary temporary restrictions. Local copies of that view retain its source. Borrowing through a named local owner still obeys its scope even when literal-initialized/refined. Constant folding changes neither lifetimes nor named-function non-capture rules; static views still fail `noBorrow`.

Precise string-literal constants remain cheaply copyable/re-materializable, preserving tag behavior. General `string` moves unless explicitly copied, including annotation-widened/mutable literal initializers. Neither optimizer knowledge nor a record's current contents makes a general string implicitly copyable. Literal reproduction/widening must never duplicate ownership of a freeable buffer; exact representation/allocation is implementation work. Read-only views copy descriptors/dependencies; exclusive references do not copy. No hidden reference counting is introduced.

## Text operations and parsing

Lengths and byte offsets are `u64`. Checked slicing functions return `Option<&string>` and validate order, bounds, and scalar endpoints. Explicitly named panicking convenience can exist; no string range-indexing syntax or integer character indexing. Byte/scalar traversal borrows; checked byte lookup returns `Option<u8>` and may read continuation bytes. Owned byte output copies unless a consuming transfer is explicitly specified. No fabricated `&List<u8>` temporary header or assumed List contiguity: #23 must give genuine byte/view/container interfaces.

Strict UTF-8 decoding returns `Result`; explicitly lossy decoding substitutes U+FFFD, never silently drops bad bytes. Borrowed zero-copy validation needs suitable contiguous storage and retains its source; allocation reuse is not assumed. String equality is exact UTF-8 equality and ordering lexicographic, without consuming operands or normalizing/case-folding/collating implicitly. Canonically equivalent different scalar sequences are distinct.

Numeric text parsing requires a known destination and returns `Result`, consuming the whole input. Default decimal grammar does not trim whitespace or accept separators, width suffixes, or automatic base prefixes. Integer destinations require integer text, not fractional truncation. Float parsing accepts decimal fractions/exponents and canonical formatter output, including integer-looking floats, `-0` with its sign preserved, NaN and infinities. Finite decimal text whose rounded result becomes infinity errors; underflow may round to zero. Explicit radix parsing is separate. This is not numeric from/into.

## Canonical interpolation

Templates use the canonical standard `show<T>(&T) -> string` family, not a same-spelled local function. Import its declaration to extend it under ordinary orphan rules; missing support is a compile error, not record/private-field reflection. Generic templates need canonical `show(T)` evidence. Standard reference-forwarding implementations cover read-only references and read-only observation of exclusive references when T is showable; no blanket automatic dereference dispatch, owner move, or exclusive-capability duplication. Exact declarations/path are #23's work. Required standard metadata/dependencies are loaded without injecting source bindings or expanding the prelude.

Evaluate and format once, left-to-right, before the next expression. Observe existing values through read access; show may suspend/effect. Owned rvalues follow normal temporary cleanup, not early guard unlocking. Owned output cannot retain input borrows. Primitive formatters must bootstrap without recursively interpolating the same argument through themselves; unrelated prototype-local show families remain unrelated.

[ADR 0013](0013-caused-by-errors-with-explicit-wrapping.md) adds a separate canonical `display(&E): string` contract for error payloads. It does not rename or replace template `show`: standard wrapper formatting bridges to each error's display and renders its caused-by chain and origins. #23 defines that interface; plain numeric/text formatting here is unchanged.

Default display: strings/chars as text without added quotes/escaping, lowercase booleans, decimal integers without width suffixes, finite floats in shortest round-tripping decimal for the same explicitly known width, and NaN/inf/-inf/-0. No NaN-payload roundtrip promise. Special formatting uses explicit functions, not a template mini-language.

## Reconciliation and remaining evidence

This supersedes #9's int/float placeholders and clarifies mutable literal widening, #11's float division and constant-trap interpretation, and #15's direct-literal exception/logical string access. #17's ordinary duplicate/overload/privacy rules and #20's no-grouping rule remain. #19 still owns literal-overload policy; refinements must not duplicate freeable ownership or invent widths/safe conversion pairs.

Existing examples use explicitly selected i32/f64 (not defaults); library parser/conversion/count/time spellings are illustrative pending #23. `prototypes/11-numbers-strings.rill` and `.md` add intended-valid expectations and independent rejection/runtime cases. All are hand-checked artifacts, not executed Rill tests. #23 owns complete evidence-preserving public signatures, byte interfaces, errors and primitive formatter bootstrapping; #24 diagnostics; #28 exact inference/availability/reference/cleanup/lowering rules and adversarial validation before implementation readiness. A missing consistent interface/checking rule is a map-level blocker, not a presumed proof from this ADR.
