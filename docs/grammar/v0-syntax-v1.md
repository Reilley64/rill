# Rill v0 syntax annex, version 1

| Field | Value |
| --- | --- |
| State | Draft annex with three policy groups confirmed. Full annex acceptance remains undecided; no parser is implemented. |
| Artifacts | This document and [v0-syntax-v1.ebnf](v0-syntax-v1.ebnf) form one review unit. Both are required. |
| EBNF snapshot | Current recording SHA-256 `c8510c7d8f4537303ec502a71013d129f77726dad34fdcfdeea578dc5951d3af`. The [policy review](#confirmed-policy-groups) identifies both original artifact digests. |
| Owner | [RFC 0003](../rfcs/0003-expressions-bindings-and-control-flow.md) coordinates syntax. RFCs 0002, 0004, 0005 and 0006 retain their rule ownership. |
| Source baseline | Repository `f33dcf4cf3f815824eeb897135775d7884f65c1a`, current owning RFCs and confirmed lexical/source snapshots (ri-48g, comment 6929923825). |
| Human decision | Three syntax policy groups confirmed (ri-48g, comment 6931939690), 2026-11-17. Full annex acceptance is not yet decided. |
| Reviewed revision | Policy review covers the [two exact pre-recording snapshots](#confirmed-policy-groups). Full annex acceptance review has not occurred. |

## Purpose and authority

This annex records the confirmed concrete syntax policy groups for the current v0 language forms. Its EBNF productions and commitment/context rules operate together. The EBNF alone is not a parser-generator specification or unambiguity proof.

The RFCs explain semantics, rationale and source reconciliation. This annex consolidates syntax rather than adding a competing language reference. The policy decision (ri-48g, comment 6931939690) does not accept complete owning RFCs. Full annex acceptance remains separate under the [RFC conventions](../rfcs/README.md#normative-ownership-and-dependencies).

The accepted RFC 0001 snapshot (ri-48g, comment 6907867211) still owns expectations. No capture, Boolean, execution-order or return-context amendment is proposed. The later syntax approval has its own exact scope, separate from earlier lexical decisions.

The grammar covers module items, declarations, types, expressions, patterns, statements and template holes. It does not complete resolution, typing, ownership, coverage, runtime transitions or parser recovery. A syntactically admitted program can still be rejected by those rules.

## Notation and lexical interface

The grammar uses named productions, quoted terminals, comma concatenation, `|` alternatives, optional brackets and repetition braces. Parentheses group grammar alternatives. Every production ends with a metasyntactic semicolon; quoted semicolons are Rill tokens.

`(* ... *)` contains grammar comments, not Rill comments. `? ... ?` names an external lexical category or required syntactic predicate. Every such predicate/category is defined here or by the exact lexical baseline.

Terminals match complete tokens, never characters joined across trivia. Significant-token lookahead ignores whitespace/comments. Identifiers and complete literal spellings follow the confirmed lexical contract, including pinned Unicode membership and original byte spans.

Template categories describe an abstract mode/event interface, not a Rust token ABI. A parser can adapt a nested lexical representation to these events. Template text segments are nonempty; an empty template needs only its start/end events.

The lexical scanner already distinguishes text, holes, nested quotes/comments and raw delimiters. Code-mode braces inside a hole are ordinary parser tokens until the hole's lexical closing event. Parsing never interprets decoded escape characters as delimiters.

The complete entry point is `module`. Tests/editors can also request expression, type or pattern fragments with explicit end-of-fragment checking. A successful fragment is not a checked module or complete analysis context.

## Declaration, type and pattern choices

### Positions, delimiters and lists

Admit imports, exports, implementations and body-less family declarations only at module level. Blocks admit standalone named functions with bodies, nominal declarations and statements. A local named function remains noncapturing under RFC 0004.

This placement is a confirmed syntax policy. It does not allow runtime-dependent implementation enrollment or local imports to change the loaded family graph. Scope/hoisting/visibility still follow their semantic owners.

Expectations are module items only, as accepted. A parser can recognize a misplaced expectation for R0102 without accepting it as a block item. Top-level return/break/continue still fail their control-target rules even when their statement shape parses.

A named function/family/implementation writes all value parameter types and its result type. Parameters are simple names or `_`, not destructuring patterns. An underscore parameter introduces no usable placeholder expression.

Call/value/type argument lists, generic binders, import groups, enum variants, match arms and requirement lists permit one trailing comma. Empty explicit type-argument/binder lists are not admitted. Empty ordinary call/value parameter lists are admitted.

Record declaration fields and enum record-payload fields use semicolons, with an optional final semicolon. Constructions/patterns use commas. Record/enum declarations permit a final declaration semicolon; function bodies and statement blocks do not require one.

A standalone semicolon is not an empty statement under this policy. An extra semicolon after a function/block is therefore invalid. An expression statement, including a match used as a statement, requires its own semicolon.

Implementations use their own binders before the family name and optional family arguments after it. Implementation binders have no defaults. This resolves their previously unwritten syntax without changing inference-before-default behavior.

For declaration defaults, only a trailing suffix of parameters may have defaults. A default can reference earlier parameters and available named types, not itself/later binders. Checking that dependency is a well-formedness task, not token matching.

A generic use supplies a nonempty leading type-argument sequence. Omitted trailing arguments retain the approved inference/default procedure. `_` type holes, higher-kinded binders, named type arguments and explicit value-parameter defaults are not introduced.

### Nominal and callable shapes

`type Name = { ... }` declares a nominal record, not a structural alias. There is no general `type Name = OtherType` form. Enum payloads are unit, nonempty tuple-payload or record-payload declarations.

Admit empty enums as uninhabited nominal types. Empty named records remain admitted. Empty record-payload variants differ syntactically from unit variants; their constructor shape remains explicit.

Admit tuple types of every finite arity, including zero and one. `[T]` is a one-position tuple type, not List<T>; `[]` is a zero-position tuple type, not void. Callable parameter parentheses also distinguish one tuple parameter from multiple parameters.

Value brackets keep RFC 0005's confirmed contextual tuple/List category rules. A zero-position tuple value needs the established tuple context. An uncontextualized empty bracket value still cannot infer a List element type.

Callable types are `(A, B) => R`, optionally prefixed with mut or once. Their result type associates to the right. Parenthesized types can group nested callable/reference forms.

When a type-start parenthesis could be a group or callable head, a following `=>` selects the callable production. This is syntactic commitment, not expected-type selection. Reference types can wrap complete callable types.

Arrows have parenthesized parameter lists under the confirmed syntax policy. Each arrow parameter can omit its type; an optional result annotation follows the closing parenthesis. Block arrow bodies use explicit returns; expression bodies yield their expression under existing rules.

No bare-name arrow shorthand, explicit value-level mut/once arrow prefix or generic arrow binder is added. Invocation mode still follows capture/evidence rules. These are confirmed spelling choices, not purity or ownership guarantees.

### Construction and patterns

A named constructor path has a module-qualified root, optional root type arguments and at most one variant selection. Thus `Packet<i32>.Data` keeps the approved parent-applied spelling. A dot-selected variant cannot acquire another generic argument list.

In value position, a constructor path immediately followed by `{` commits to named construction. It is not a value followed by an implicit statement block. Type/name resolution determines whether that path actually denotes a constructible record/variant.

Admit construction field shorthand `field` as exactly `field: field`. It changes no move/copy/borrow or evaluation rules. A rebuild has exactly one leading spread; later fields are explicit replacements.

Record patterns may have a nominal prefix or use bare braces against an established scrutinee. They can omit fields, as stated in RFC 0005. There is no source rest-pattern marker or anonymous record value.

A standalone single identifier pattern binds, even when a same-spelled value exists. A unit-variant pattern needs qualification. Constructor-shaped patterns resolve nominal constructors, not ordinary function calls or user callbacks.

The parser records pattern syntax without searching types to retry another parse. Name resolution, constructor arity, privacy, irrefutability and exhaustiveness remain semantic checks. Public Some/None bindings still require their actual library definitions.

Literal patterns admit numeric signs, cooked/raw text, characters, booleans and hole-free templates. Template holes and arbitrary value expressions are not patterns. Numeric type/range checks remain separate, including the confirmed signed-minimum case.

`guard let` can annotate its successful pattern as an explicit local type context. The annotation supplies no missing error type or implicit conversion. Refutable payload behavior remains with the type/guard owner.

For-of keeps the written `for (let pattern of expression)` form. It adds no for-let-mut, range loop, label or loop value. A body can introduce an ordinary mutable binding when needed.

## Required parsing commitments

### Generic arguments versus comparisons

In a type, pattern-constructor, declaration-binder or implementation-signature context, `<...>` has its grammatical type meaning. Angle closers use the token views below. Value references need a separate syntactic commitment rule.

At a value-reference root followed by `<`, attempt only a type-argument syntax scan. Name resolution and desired types cannot participate. An incomplete/invalid candidate does not commit that root to generic syntax.

After a complete candidate, inspect the next significant token, including any residual closer piece. GENERIC_COMMIT succeeds only for this finite follow set:

```text
( { . ; , ) ] } : = => ? |> else EOF TEMPLATE_HOLE_END
```

Then commit the generic reference before its ordinary call/field/construction suffix. Whitespace does not affect this decision. A type/name failure after commitment cannot retry a comparison parse.

Otherwise leave the original tokens for comparison/shift parsing. In particular, residual `>`, `>=` or `>>` is not in the follow set. This preserves `a < b >> c` as a comparison against a shifted expression.

Also reject value-level commitment when its final closer leaves `=` from an original `>=` or `>>=` token. This preserves comparison/assignment punctuation instead of manufacturing a value-instantiation assignment. Established type contexts still split those closers normally.

Consequently, `a < b > (c)` commits as a generic call candidate. Write `(a < b) > (c)` for that comparison grouping. A generic function value before an ambiguous arithmetic/comparison operator needs parentheses.

This is a confirmed syntax policy, not a claim that a name actually denotes a generic family/type. It preserves explicit generic function values at delimiters and parent-applied variants. It adds no first-class polymorphism or result-based dispatch.

### Closing-token views

The confirmed lexer keeps its longest punctuation tokens. In an established angle-closing context, the parser may consume one leading `>` from `>>`, `>=` or `>>=`. Preserve the exact source subspan of each consumed/residual piece.

Use the confirmed analogous type-only view for nested references. A type parser can consume one leading `&` from `&&`, leaving another `&`. Expression parsing never splits `&&` into prefix borrows.

Thus `&&T` can denote a nested reference type. Write `& &value` or `&(&value)` for nested expression borrowing. Boolean `left && right` keeps its normal token and meaning.

These views never recombine separated characters across trivia. A tentative generic scan uses a checkpoint/view, not destructive lexer mutation. Choosing a type-argument context and committing it remain syntactic operations.

### Arrow and statement contexts

At expression start, a complete parenthesized arrow head followed by `=>` selects arrow syntax before grouping/call parsing. Without that marker, use ordinary parentheses. `()` alone is not a written void value.

Arrow expression bodies extend through a complete expression. Calling the resulting arrow immediately requires grouping the whole arrow first. An initial `{` after `=>` always starts its block body.

At statement start, `if` selects the statement form with blocks. A conditional expression used as a statement needs outer parentheses. A bare `{` in statement/match-arm/arrow-body position starts a block, not a record expression.

Within expressions, an if-expression has both branches and the written condition parentheses. Each branch parses a complete expression. An else pairs with the nearest unfinished conditional at that nesting level.

Match guards reserve their top-level `=>` for the arm delimiter. In this mode, that token cannot commit an arrow head. Thus `Variant if (ready) => answer` has a grouped Boolean guard, not an arrow guard.

An arrow inside a nested delimiter pair remains permitted. Its `=>` is deeper than the reserved arm delimiter. A match arm's initial `{` selects a block; a named record value starts with its name.

### Question marks and expression starts

The whitespace/comment-insensitive question-mark policy uses this confirmed EXPR_START set:

```text
IDENT NUMBER CHARACTER STRING TEMPLATE_START true false
( [ if match ! - ~ & *
```

Here NUMBER retains the complete approved spelling, not a default width. STRING includes cooked/raw string tokens. IDENT is not a reserved keyword; parentheses include possible arrow heads.

For a `?`, use infix wrapping when the next significant token is in EXPR_START. Otherwise use postfix propagation. Commit from that token class, not success of parsing the complete following expression.

This preserves the approved minus, call and index examples. It also means `value?()` selects an invalid infix RHS, not a fallback postfix call. Use `(value?)()` for that call.

Each wrapping production admits at most one infix question mark. `a ? b ? c` therefore rejects without grouping. Nested arrow/conditional/parenthesized subexpressions have their own expression boundaries; no type-driven retry is permitted.

Infix wrapping remains below pipes, then Boolean/bitwise/comparison/arithmetic operators. Prefix operators group right; ordinary binary repetitions group left. Repeated comparisons are ordinary binary operations, not mathematical comparison chains.

### Pipe steps and placeholders

A pipe step is a callable name/path, optional argument list, or a fully parenthesized arrow expression. No arbitrary expression/index computation is a step. Dotted callable fields add no method or implicit receiver argument.

One direct `_` argument can place the pipe input. More placeholders for that step, or free placeholders inside its argument expressions, reject. Outside a step argument, `_` is not an ordinary value expression.

A nested pipeline establishes its own step/placeholder context. Its placeholders do not count against the outer step. No placeholder escapes its own argument position.

The question-mark test above also decides a step's postfix marker. If EXPR_START follows, leave the question mark for wrapping the complete pipe result. Otherwise propagation applies to that step's result, not its callable value.

The restricted step grammar does not make following arithmetic part of an already completed pipe. For example, `(x |> f) + y` groups the pipeline before addition. Evaluation order remains the approved input-first/callee/argument order.

## Ownership of rules and migration

| Owner | Contribution retained here | Not supplied by syntax |
| --- | --- | --- |
| [Accepted RFC 0001](../rfcs/0001-top-level-expectations.md) | Module-level expect declaration with an expression and semicolon. | Bool-only checking, return contexts, execution/capture and bounded observation. No amendment is proposed. |
| [Draft RFC 0002](../rfcs/0002-modules-and-program-loading.md) | Imports/exports, canonical paths and nominal source bindings. | Complete candidate/cycle resolution, scope/metadata and whole-context freshness. |
| [Draft RFC 0003](../rfcs/0003-expressions-bindings-and-control-flow.md) | Approved lexical policy, precedence, sequencing and control forms. | Complete flow/guard/unused-Result judgments and the new syntax-policy approval. |
| [Draft RFC 0004](../rfcs/0004-functions-overloads-and-generic-requirements.md) | Named declarations, implement, requirements and callable modes. | Overload applicability/comparison, inference and evidence procedures. |
| [Draft RFC 0005](../rfcs/0005-types-records-enums-and-inference.md) | Nominal records/enums, parent-applied variants and bracket/pattern shapes. | Layout, coverage, contextual category checking and new shape-policy approval. |
| [Draft RFC 0006](../rfcs/0006-numbers-strings-and-primitive-operations.md) | Literal tokens, template modes and exact spelling. | Numeric context/ranges, operations, formatting and ownership/lowering. |
| Draft RFCs [0007](../rfcs/0007-ownership-borrowing-mutation-and-cleanup.md), [0008](../rfcs/0008-errors-propagation-and-caused-by-wrapping.md), [0009](../rfcs/0009-user-implementable-conversions.md) and [0010](../rfcs/0010-diagnostics-and-reporting.md) | Existing source, ownership, error, conversion and diagnostic distinctions. | Their full checking/runtime/driver/client procedures or implementation acceptance. |

The syntax policy groups are confirmed, not a silent rewrite of historical sketches or full owning-RFC acceptance. Existing fenced programs/grammar, prototypes and reports retain their bytes. Where a historical fixture conflicts, record migration rather than rewriting it as a passed current test.

Ordinary calls parse independently of canonical constructor/family identity. Thus `Ok()`, borrowed Err construction or a wrong explicit E can parse and still reject. Grammar acceptance cannot override their actual arity, ownership or selection rules.

The grammar introduces no general union, singleton type, anonymous record value, cast, optional/variadic value parameters or inline module. It adds no runtime conversion chain, reflection, new prelude name or automatic serializer enrollment.

## Boundary examples

These are independent syntax inputs, not promised complete well-typed modules. The intended outcomes follow both artifacts and the commitments above. Every Rill parser/source-checker outcome is NotChecked.

| Case | Input or scenario | Intended result |
| --- | --- | --- |
| G1 | `import geometry::{Point as P, origin,};` | Grouped import with one trailing comma. Resolution still needs the module graph. |
| G2 | An import or implementation written inside a function body | Reject under the confirmed module-only placement rule. |
| G3 | A nested named function with typed parameters/result and a body | Syntax admitted. Capturing enclosing runtime lets still rejects. |
| G4 | `a < b >> c` | Comparison of a with a shifted b/c expression, not a generic reference. |
| G5 | `a < b > (c)` | Commit a generic call candidate. Type failure does not retry comparisons. |
| G6 | `(a < b) > (c)` | Explicit comparison grouping. Operand legality remains separate. |
| G7 | `let converter = into<i64, i32>;` | Explicit generic function-value syntax. Canonical selected-signature checking remains required. |
| G8 | `let item: Box<Box<i32>>=value;` | Nested type closers then assignment. Subspans split the original `>>=` token. |
| G9 | `Packet<i32>.Data(3i32)` | Parent-applied generic variant with an ordinary call-shaped payload. |
| G10 | `Packet.Data<i32>` | No variant-owned type-argument suffix; cannot produce a generic-variant node. |
| G11 | `(number: i32): i32 => number` | Typed arrow with an explicit result annotation. |
| G12 | `number => number` | Bare-name arrow shorthand is not admitted by this policy. |
| G13 | `&(() => i32)` and `&&string` as types | Reference to a callable; nested reference using the type-only ampersand view. |
| G14 | `&&value` at expression start | Reject. The expression parser cannot split Boolean-and into borrows. |
| G15 | `[i32]`, `[]` as types | Confirmed one-position/zero-position tuple shapes, not List or void. |
| G16 | `let value = [];` | Shape parses; absent List element context still rejects semantically. |
| G17 | `enum Empty {}` and `type Marker = {};` | Confirmed empty enum and empty nominal record shapes. |
| G18 | `type Alias = Other;` | Reject. No general type-alias feature is introduced. |
| G19 | `implement<T = i32> f(value: T): T { return value; }` | Reject implementation-binder defaults under the confirmed rule. |
| G20 | `function f<T = i32, U>(value: U): U;` | Grammar shape parses; nontrailing declaration default fails well-formedness. |
| G21 | `Point { x, y }` | Confirmed field shorthand, with ordinary moves/copies and written evaluation order. |
| G22 | `Point { ...base, x: 2 }` | One leading rebuild base. Declared x type supplies numeric context as before. |
| G23 | `Point { x: 2, ...base }` | Reject a nonleading spread. No multiple-spread construction. |
| G24 | `let { child: { value } } = record;` | Contextual nested record pattern, not an anonymous record value. |
| G25 | `None` versus `Option.None` in a pattern | A binder versus a qualified unit-constructor candidate. Actual definitions remain required. |
| G26 | A template hole inside a literal pattern | Reject. Hole-free templates remain literal patterns. |
| G27 | `if (ready) { work(); } else { stop(); }` | Statement conditional. No semicolon after the complete block statement. |
| G28 | `(if (ready) left else right);` | Conditional expression explicitly grouped as an expression statement. |
| G29 | `if (ready) left else right;` at statement start | Reject. Statement form requires blocks. |
| G30 | `return;` at module level | Statement shape parses; invalid control target still rejects. |
| G31 | `read()? - 1i32` versus `(read()?) - 1i32` | Approved wrapping versus propagation followed by subtraction. |
| G32 | `value?()` versus `(value?)()` | Invalid infix RHS versus explicit call after propagation. No fallback parse. |
| G33 | `a ? b ? c` | Reject the unparenthesized same-level wrapping chain. |
| G34 | `x |> f ? Failure {}` | Wrap the complete pipe result; do not propagate the callable. |
| G35 | `x |> pad(10i32, _)` | One direct placeholder inserts input in the second argument position. |
| G36 | `x |> f([_])` or `x |> f(_, _)` | Reject nested/multiple placeholders. No partial application. |
| G37 | `x |> ((value) => value)` | Fully parenthesized arrow step. Capture/type/evaluation checking remains separate. |
| G38 | `x |> f + y` versus `(x |> f) + y` | Reject the restricted-step form; explicit outer grouping admits addition. |
| G39 | `guard let item: Item = operation() else (error) { return; }` | Annotated guard payload syntax. Container/error/exit rules still decide legality. |
| G40 | `expect (() => { return true; })();` | Shape retains the nested callable context. It adds no expectation-level return. |
| G41 | A nested expect declaration | Reject as misplaced expectation, R0102 meaning. No implicit local test function. |
| G42 | `Ok()` or `Err<Success, ErrorType>(error)` | Ordinary call shapes. Closed-helper/arity/evidence rules still apply. |
| G43 | `Variant if (ready) => answer` inside match arms | Reserve the arm delimiter; the guard is a parenthesized value, not an arrow. |
| G44 | `Variant if ((item) => test(item))(value) => answer` | The nested arrow/call can form the guard. Bool/type/evidence checks still apply. |
| G45 | `x |> f(y |> g(_), _)` | Each step has one direct placeholder. The inner step does not consume the outer allowance. |
| G46 | `a < b >= c` | Preserve comparison tokens/grouping, not a generic value followed by manufactured assignment. |
| G47 | `-(128i8)` as a numeric pattern | Parenthesized magnitude remains admitted; apply the approved directly-negated minimum rule, not positive i8 rejection. |

## Evidence and review gates

The drafting agent hand-compared the productions with the current RFC syntax owners on 2026-11-17. This is provenance work, not a successful parse of all source examples. Lexical/data check results retain their separate scopes.

On 2026-11-17, temporary `/tmp/rill-grammar-check.py` passed structural checks for all 128 EBNF productions through `mise exec -- python`. Definitions were unique, reachable, productive and complete. No repetition had a nullable body, and the grammar's left-corner graph had no left recursion.

The computed expression FIRST set matched the specified 16 token classes/spellings. All 47 boundary-case rows were present. These ToolChecked grammar/data results do not establish unambiguity, semantic preservation or an accepted parser. No Rill parser or parser generator executed these examples.

Before claiming implementation, test every commitment with valid/rejected neighbors and exact source spans. Include comments/newlines, nested generic closers, arrows, conditional branches, templates and pipe suffixes. Resource limits must not turn incomplete parsing into source success.

No new parser-recovery, incremental AST or public compiler API contract is supplied. A later parser can retain partial/error nodes without accepting malformed source. Editor protocol recovery is outside the grammar contract.

The drafting agent applied unslop after the prose review. Sentence/paragraph and document-structure checks passed. Formal ASD-STE100 dictionary compliance remains unverified.

<a id="review-choices"></a>

### Confirmed policy groups

The owner approved all three groups (ri-48g, comment 6931939690) on 2026-11-17. The policy review covers these exact pre-recording artifacts:

| Artifact | Reviewed SHA-256 |
| --- | --- |
| v0-syntax-v1.ebnf | `158c9b74bb2f36dbf992e454ebc10d57bfac9bb3859b04a394d61d29e0bd94e7` |
| v0-syntax-v1.md | `0d7eeef8bf1abddebf6c04501eaf2572ea0f94f528ac69b933109afb9a3fdbf6` |

1. Declaration/type/pattern shapes include positions, separators/defaults, parenthesized arrows, finite tuples, empty enums, field shorthand and pattern forms.
2. Parsing commitments use the finite generic follow set, callable/group priority and source-preserving token views without type-directed retries.
3. Expression/control boundaries use the exact question-mark FIRST set and restricted pipe/placeholder rules while preserving approved precedence.

The decision covers the three groups in both artifacts, not future unwritten rules. Full annex/RFC acceptance and canonical readiness remain separate. Typing, ownership, coverage, parser recovery and implementation validation remain incomplete.

## Change and version discipline

Version 1 is a Draft review identity, not a released source-compatibility guarantee. A decision must identify the grammar and this document together by exact revision/digest. Later updates cannot silently amend accepted rules through a moving link.

Keep earlier reviewed snapshots and their decisions discoverable. Semantic changes require explicit amendment/review; unsupported implementation behavior cannot redefine the grammar. No RFC ID is allocated for this annex.

The 2026-11-17 drafting request created these artifacts and linked their owners. The later decision confirms the three syntax policy groups at the snapshots above. Full annex and owning-RFC acceptance remain undecided. No readiness, implementation, native/Wasm or client acceptance is inferred.
