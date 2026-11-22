# RFC 0003: Expressions, bindings and control flow

| Field | Value |
| --- | --- |
| ID | 0003 |
| Status | Draft |
| Owner | Reilley64. ri-48g tracks this draft. |
| Source inputs | The [Unicode identifier snapshot](#identifiers-keywords-and-ordinary-names) supplies exact membership tables. ri-457 core decisions (comment 6308790321), with the later refinements listed under [source reconciliation](#source-reconciliation). Repository inputs use commit `f33dcf4cf3f815824eeb897135775d7884f65c1a`. The operator clarification (ri-48g, comment 6908782235) preserves wrap-and-propagate semantics. Later approvals (ri-48g, comment 6909312678) confirm precedence, evaluation order and binding initialization/scope. The option B decision (ri-48g, comment 6909516496) fixes whitespace-insensitive question-mark parsing. |
| RFC dependencies | [RFC 0001](0001-top-level-expectations.md) owns expectations. Use its accepted snapshot (ri-48g, comment 6907867211), SHA-256 `ee4980216dd2ed051cb6153600bf8b5f6c039145c083abfeeb37119d027bd954`. [RFC 0002](0002-modules-and-program-loading.md) proposes module scope and visibility. Other [companions](#required-companion-rules) remain missing. |
| Replaces / superseded by | None. This draft does not amend RFC 0001 or accept RFC 0002. |
| Decision | Not yet decided |
| Reviewed revision | Not yet reviewed |

## Summary and scope

This RFC proposes expression grouping, local bindings, statements and control transfers. It covers conditional expressions, matching, loops, guards, return, propagation and pipes. It separates normal cleanup from fatal termination.

The source decisions already define explicit returns, test-only expectations, immutable bindings by default and typed error propagation. This draft preserves those choices. It also reconciles later enum, mutation, numeric and error decisions.

The project owner confirmed precedence, evaluation order, binding initialization/scope and whitespace-insensitive `?` parsing. This RFC remains Draft. Concrete syntax policies are confirmed. Complete checking and companion rules remain authoring work.

This document does not define all literal tokens, nominal layouts, overload selection or the complete ownership checker. Required companion rules remain explicit. No Rill parser or runtime has executed these examples.

## Motivation

A programmer must know which expression executes first and which result a binding receives. A control transfer must identify its target without depending on optimization.

Rill uses both postfix propagation and infix error wrapping. Their grammar must distinguish those operations without guessing from runtime values. Pipe steps must also preserve the input and its evaluation order.

Explicit returns prevent a block's last statement from silently becoming its value. Guards must enforce their success condition before later statements execute. Mutation and early return must preserve ownership and required cleanup.

## Guide-level explanation

This complete module illustrates bindings, mutation, a loop, a guard and explicit return.

```rill
import std::collection::List;

function totalUntilZero(values: &List<i32>): i32 {
  let mut total: i32 = 0;
  for (let value of values) {
    guard *value != 0 else { break; }
    total += *value;
  }
  return total;
}

expect totalUntilZero(&[2i32, 3i32]) == 5i32;
expect totalUntilZero(&[2i32, 0i32, 9i32]) == 2i32;
```

The parameter supplies the List and element-type context. These totals are intended outcomes, not compiler results.

A simpler complete module needs no collection inference:

```rill
function magnitude(value: i32): i32 {
  return if (value >= 0) value else -value;
}

expect magnitude(-3i32) == 3i32;
```

The conditional expression requires `else`. A statement-form `if` instead uses blocks and can omit `else`. A block does not produce its final expression's value.

`magnitude` can panic on the minimum `i32` value because checked negation overflows. The example does not promise a total absolute-value operation.

## Syntax and reference semantics

The owner confirmed the three syntax policy groups (ri-48g, comment 6931939690) in [annex version 1](../grammar/v0-syntax-v1.md). Its [EBNF](../grammar/v0-syntax-v1.ebnf) and commitment/context rules form one policy review unit. Full annex/RFC acceptance and static-semantics completion remain separate.

<a id="proposed-lexical-tokens-and-trivia"></a>

### Lexical tokens and trivia

The owner confirmed these lexical policy groups (ri-48g, comment 6929923825), not the full RFC text. Existing precedence, evaluation order and question-mark policies remain confirmed.

RFC 0002 owns [source snapshots](0002-modules-and-program-loading.md#source-snapshots-and-lexical-input). RFC 0006 owns [literal scanning](0006-numbers-strings-and-primitive-operations.md#proposed-literal-scanning-and-token-boundaries). RFC 0010 owns [coordinate projection](0010-diagnostics-and-reporting.md#source-ranges-and-coordinate-projection). These Draft sections own the coordinated confirmed policy groups. Their full texts remain Draft.

#### Token identity and complete consumption

A token identifies its kind, exact source spelling and original half-open byte span within one source snapshot. This is a semantic description, not a required Rust structure or public token ABI. A decoded literal value does not replace its spelling or span.

Trivia consists of admitted whitespace and comments. Retain enough source/trivia information for diagnostics and formatting. Implementations may store that information as spans rather than separate allocated strings.

For valid source, tokens and trivia partition all bytes, including an optional initial BOM. EOF has the empty span at source length. Tokenization neither inserts semicolons nor interprets line placement as a statement boundary.

#### Identifiers, keywords and ordinary names

Use the confirmed Unicode 17.0.0 XID membership for v0 identifiers:

```text
identifier-start = XID_Start_17 | "_"
identifier-continue = XID_Continue_17
identifier = identifier-start { identifier-continue }
```

The identifier policy uses [Unicode 17.0.0 DerivedCoreProperties.txt](https://www.unicode.org/Public/17.0.0/ucd/DerivedCoreProperties.txt). Its SHA-256 is `24c7fed1195c482faaefd5c1e7eb821c5ee1fb6de07ecdbaa64b56a99da22c08`. Only its XID_Start/XID_Continue ranges are confirmed for identifier membership. Other Unicode algorithms/properties and full RFC/annex acceptance remain separate.

Scan the complete identifier before classifying a keyword. Compare spellings exactly, with case sensitivity and no normalization. Do not substitute the host library's unpinned Unicode version.

Non-ASCII identifiers can therefore spell Unicode import segments without introducing quoted imports. Canonically equivalent different spellings remain distinct identifiers. This policy does not promise confusable detection or adopt a default normalization profile.

A lone `_` is the placeholder/wildcard token, not an ordinary name. `_value` is an ordinary identifier. There are no escaped, raw, quoted or dollar-containing identifiers. An initial combining mark is not repaired by adding an invisible identifier start.

The complete confirmed reserved keyword set is:

```text
as break continue else enum expect export false for function guard
if implement import let match mut of once private return true type
where while
```

Reserved words cannot be identifiers after qualification or in field declarations. The parser assigns each admitted use its grammatical role. Reserving `as` preserves import aliases; it adds no cast operation.

Primitive names, numeric widths, intrinsic predicates, copy/dispose and library names are ordinary identifier tokens. Their binding/permission rules remain with their actual owners. `std`, `self` and `super` receive special import-root meaning only in the module grammar.

`NaN` and `inf` are identifiers, not numeric literal tokens. Removed or unsupported constructs do not gain syntax because their names tokenize. This policy adds no future-keyword reservation list.

The raw opener `r` followed immediately by zero or more hashes and a double quote takes priority over identifier scanning. Other uses of `r` remain identifiers. Whitespace/comments cannot join an identifier to a raw-string opener.

#### Whitespace, line endings and comments

Admit space, horizontal tab, LF, CRLF and lone CR as source whitespace. Treat CRLF as one physical line ending. The confirmed policy also treats a lone CR as a physical line ending.

Only a UTF-8 BOM at byte zero is BOM trivia. It remains part of the snapshot and offsets. A later U+FEFF is not whitespace or an identifier character; literal/comment rules can admit it as content.

Line comments start with `//` and end before the next physical line ending or EOF. Block comments start with `/*` and end with the matching `*/`. Use nested block comments with a checked nesting counter.

Inside a block comment, only `/*` and `*/` affect comment nesting. Quotes, backticks and `//` do not create other scanning modes there. Inside a line comment, all such text remains comment content.

Source must remain valid UTF-8 inside comments. The confirmed control-character exclusions in RFC 0006 also apply to comment content. Comments add no documentation attribute, formatter directive or conditional-compilation behavior.

A separated `/ *` is not a comment opener. Whitespace/comments cannot combine separated source characters into a multi-character token. There is no shebang or backslash line continuation.

#### Punctuation and operator matching

Outside literals/comments, match the longest member of this complete punctuation set at the current byte position:

```text
three bytes: <<= >>= ...
two bytes:   :: => == != <= >= && || << >> += -= *= /= %= &= |= ^= |>
one byte:    ( ) { } [ ] , : ; . ? = + - * / % ! ~ & | ^ < >
```

Literal openers and comment openers select their scanning modes before this table. A lone `_` has its separate role above. Characters outside admitted categories are invalid source, not ignored bytes.

`&mut` is `&` followed by keyword `mut`, not a combined token. `&mutvalue` is `&` followed by identifier `mutvalue`. Whitespace can separate `&` from `mut` without changing those tokens.

`++` and `--` are not single tokens or increment/decrement operators. Repeated single-character operators still need valid expression grammar. For example, consecutive minus tokens can represent nested/subtracted negation; they add no decrement statement.

The lexer never classifies `?` as postfix or infix. Both have the same token. The confirmed syntax-based parser policy decides its role after ignoring trivia.

Maximal matching produces shift/comparison tokens even beside generic syntax. A parser in an established type-argument context may consume a leading `>` from `>>`, `>=` or `>>=`. Each consumed/residual piece keeps its exact original-byte subspan.

For example, splitting `>>=` once leaves `>=`; splitting again leaves `=`. Do not combine separate tokens across trivia to invent a shift. Choosing the type-argument context remains full parser-grammar work, not a type-checker retry.

#### Lexical errors and recovery

A lexer must always consume source or finish. Malformed input cannot cause a zero-progress recovery loop or unchecked nesting/offset arithmetic. Resource exhaustion or unsupported host size is not successful lexing or fabricated source validity.

For an unknown admitted UTF-8 scalar outside a token, consume that entire scalar as one invalid unit. Preserve its byte span. Do not replace it with an identifier, whitespace or inferred delimiter.

A block comment without its final delimiter consumes through EOF and rejects. An ordinary unclosed quoted literal stops before the physical line ending or at EOF. The line ending remains available for subsequent recovery.

Unclosed raw strings/templates consume through EOF in their active modes. Numeric candidates and invalid literal contents follow RFC 0006. Recovery must not accept a valid prefix of one malformed candidate as the entire intended token.

Malformed source structure, invalid UTF-8 and unmatched lexical delimiters have R0001 meaning. Invalid numeric spelling or decoded literal contents have R0002 meaning. R0003 applies only when a valid numeric literal lacks type context.

Further diagnostics can depend on recovery, but the first lexical failure must remain visible. Partial tokens/error nodes are not an accepted program or invented type facts. This contract does not specify parser synchronization or AST recovery.

#### Lexical boundary cases

These are independent lexical inputs, not complete Rill modules. Token names below describe intended categories, not a Rust API. Every case is NotChecked by a Rill lexer.

| Case | Input or condition | Intended lexical result |
| --- | --- | --- |
| L1 | `letter`, `_value`, `value2`, `_` | Three identifiers and the separate placeholder token. |
| L2 | `functionality function Function` | Identifier, keyword, identifier. Keywords use the full case-sensitive spelling. |
| L3 | Non-ASCII `café` outside a literal/comment | One identifier under the confirmed pinned Unicode profile. |
| L4 | Initial bytes `EF BB BF`, then `let` | BOM trivia occupies bytes 0 through 3; the keyword starts at byte 3. |
| L5 | `/* outer /* inner */ end */name` | One nested block-comment trivia span, then identifier `name`. |
| L6 | Unterminated block comment | R0001 through EOF. No synthetic close or accepted remainder. |
| L7 | `/ * name` | Slash, star and identifier. No comment opener. |
| L8 | `&mut & mut &mutvalue` | Ampersand/keyword, ampersand/keyword, ampersand/identifier. |
| L9 | `Box<Box<i32>>=value` | Two `<` tokens and one `>>=` token before parser-owned closing splits. |
| L10 | `left > /* comment */ > right` | Two `>` tokens, never a shift token. |
| L11 | `value? /* gap */ -1i32` | Question mark, minus and numeric token remain independent. The confirmed parser policy selects wrapping. |
| L12 | `// quote " and /* text` followed by LF | One line comment. Its apparent literal/block delimiters have no effect. |
| L13 | Noninitial U+FEFF outside a literal/comment | Invalid scalar unit, not invisible whitespace. |
| L14 | Bare CR between declarations | Confirmed line-ending trivia, not semicolon insertion. |
| L15 | An invalid UTF-8 byte inside a comment | Source rejection before an accepted token stream; comments do not bypass encoding validation. |
| L16 | `@` or `#` outside a recognized literal | Invalid source units with original spans. |
| L17 | Composed é versus e followed by U+0301 in identifiers | Both spellings can be identifiers, but remain distinct. No normalization or keyword repair. |

Lexical acceptance does not imply syntactic or semantic acceptance. For example, a reserved word in an identifier position still rejects in the parser. Literal-specific cases continue in RFC 0006.

### Expressions, statements and blocks

An expression has a static type. If evaluation completes normally, it produces a value. Expression statements end with `;`. Bindings, assignments, return, break and continue statements also require semicolons. Statement `if`, loops, guards and standalone blocks need no trailing semicolon.

A block contains statements and declarations. It creates a lexical scope. It does not implicitly yield its final expression. A block-bodied function returns a value only through explicit `return`.

A `void` function can complete its body without `return;`. A non-`void` function cannot reach the end without supplying its declared result. A function returning `never` cannot return normally. A call returning `never` has no normal continuation.

An expression-bodied arrow returns its expression. A block-bodied arrow follows explicit-return rules. Callable declarations, capture modes and parameter inference belong to the function and ownership companions.

### Expression forms

This draft uses the following expression forms:

- Literals, names, qualified names and parenthesized expressions.
- Calls, field access and indexing.
- Named record construction, named rebuilding and qualified enum construction.
- Tuple and List literals under the companion typing rules.
- Borrowing, mutable borrowing, dereference and primitive unary and binary operations.
- Conditional expressions, exhaustive `match`, propagation, caused-by wrapping and pipes.
- Arrow expressions under the function companion's grammar.

These forms do not add methods, anonymous records, general unions or runtime overload dispatch. Source qualification and private access follow the module rules. Literal spelling and complete type grammar need their own owners.

### Boolean operations

Logical `!`, `&&` and `||` require Boolean operands. They produce `bool`, not one of their original operands. There is no general truthiness conversion.

Evaluate the left operand first. For `&&`, skip the right operand when the left is false. For `||`, skip it when the left is true. Otherwise evaluate the right operand once. Bitwise `&` and `|` are different operations and do not short-circuit.

<a id="proposed-evaluation-order"></a>

### Evaluation order

The project owner approved these sequencing rules (ri-48g, comment 6909312678). This does not accept the complete RFC.

Evaluate an ordinary call's callee expression before its arguments. Evaluate arguments from left to right. Evaluate ordinary binary operands from left to right. Evaluate aggregate elements and explicitly supplied fields in source order. Evaluate an indexing base before its index.

Each required subexpression executes once. Short-circuiting, conditional selection and lazy error wrapping can skip subexpressions under the rules below. Do not execute a skipped expression for inference, formatting or diagnostics.

Pipe evaluation is explicit. Evaluate its input first, then the step's callable and written arguments. A placeholder changes parameter position, not evaluation order. The input is not reevaluated when inserted.

An expression can suspend under ordinary function rules. Left-to-right evaluation does not serialize independent goroutines or promise atomic external effects.

<a id="proposed-precedence-and-question-mark-parsing"></a>

### Precedence and question-mark parsing

ri-n1j (comment 6308942432) fixes pipe precedence below ordinary arithmetic, comparison and logical operators. It also fixes left-to-right pipe grouping. [ri-vtw](../adr/0013-caused-by-errors-with-explicit-wrapping.md) adds a separate infix `?` operation.

The project owner approved this precedence table (ri-48g, comment 6909312678) and selected parsing option B (ri-48g, comment 6909516496). Rows run from weakest to strongest binding.

| Level | Forms | Grouping |
| --- | --- | --- |
| 1 | Infix caused-by `?` | Non-associative. Parenthesize nested uses. |
| 2 | `\|>` | Left to right. |
| 3 | `\|\|` | Left to right. |
| 4 | `&&` | Left to right. |
| 5 | Bitwise `\|` | Left to right. |
| 6 | Bitwise `^` | Left to right. |
| 7 | Bitwise `&` | Left to right. |
| 8 | `==`, `!=` | Left to right. No chained-comparison expansion. |
| 9 | `<`, `<=`, `>`, `>=` | Left to right. No chained-comparison expansion. |
| 10 | `<<`, `>>` | Left to right. |
| 11 | `+`, `-` | Left to right. |
| 12 | `*`, `/`, `%` | Left to right. |
| 13 | Prefix `!`, `-`, `~`, `&`, `&mut`, `*` | Right to left. |
| 14 | Calls, indexing, field access and postfix `?` | Postfix grouping. |

The table does not authorize unsupported operand types. Numeric widths, checked arithmetic, shifts and float behavior follow their numeric contract. Namespaced names and generic argument syntax are not new binary operators.

Question-mark parsing follows syntax, not spacing or types:

- If a right-hand expression can begin after `?`, parse infix wrapping.
- Otherwise parse postfix propagation.
- Ignore whitespace and comments when selecting that role. A line break does not insert a semicolon.

Both `read()? - 1i32` and `read() ? -1i32` therefore wrap with the error value `-1i32`. On success, each yields the original success value without evaluating that error operand. Use `(read()?) - 1i32` to propagate unchanged before subtraction.

A right-operand start takes priority over a postfix continuation after `?`. To call or index a propagated value, use `(value?)(args)` or `(value?)[index]`. Type checking must not retry another parse after an error.

The project owner selected option B (ri-48g, comment 6909516496) after comparing both designs. The original spacing-sensitive option A is not adopted. The shared syntax annex records the confirmed expression-start set and delimiter commitments. The lexical policy above does not choose question-mark roles.

An unparenthesized chain such as `a ? b ? c` is rejected. Parentheses must state the intended nesting. A nested right operand still follows ordinary lazy evaluation and early-return rules. It does not create an automatic multi-error chain.

### Bindings and inference

Confirmed mutation rules distinguish `let` from `let mut`. Ordinary owned parameters and `let` bindings are immutable. `let mut` permits reassignment and permitted mutation of owned fields. It does not change copyability or grant mutable access to shared payloads.

The project owner approved initializer and scope rules (ri-48g, comment 6909312678). The binding shape is:

```text
binding = "let" ["mut"] binding-pattern [":" type] "=" expression ";"
```

These bindings use the value/function namespace. An initializer is required. This draft adds no uninitialized local or implicit default value. A binding annotation checks the initializer's type. Without an annotation, infer from the initializer and its immediate context, not later statements.

A numeric literal needs an established width. `let count = 10;` is invalid. Use `let count: i32 = 10;` or `let count = 10i32;`. A typed `i32` value does not implicitly convert to `i64` because its destination has that annotation.

Later ri-kty removes singleton literal types. A string literal binding has type `string`, not a primitive literal subtype. Numeric literals retain their established numeric base type, not an optimizer-dependent singleton type.

Names, `_` and tuple destructuring appear in current sources. Tuple destructuring requires a tuple, not an implicit List-to-tuple conversion. Payload binding through borrows follows the ownership and pattern rules below. Complete binding-pattern checking remains companion work.

A new binding enters scope after its initializer. Its initializer can therefore refer to an outer same-spelled binding. Without such an outer binding, self-reference is unresolved. The compiler rejects duplicate value bindings in the same scope. Other namespaces follow the module rules. Inner lexical shadowing remains allowed.

Moving or copying a value into a binding preserves its dependencies. An immutable binding can still contain a mutable reference. Binding immutability does not weaken that reference's access restrictions or duplicate exclusive access.

### Assignment and replacement

Assignments and compound assignments are statements, not value-producing expressions. There is no `++`, `--` or `const` keyword in this design.

The proposed compound forms are `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=` and `>>=`. They use the corresponding primitive operation. Their complete operand legality follows the numeric and ownership companions. This does not add general assignment-operator overloads.

A destination place identifies writable storage. Its access must satisfy the ownership rules.

Evaluate the destination place once, before the right-hand expression. Keep the old value alive while computing its replacement. Then dispose the old value and install the replacement. A compound assignment must not evaluate the destination twice.

Reject a replacement that leaves references into freed old storage. Keep the replacement owned during old-value disposal. Track initialized and disposed states. Cancellation delivery stays deferred from old-value disposal through valid replacement installation. Destination and right-hand evaluation remain normally cancellable.

This RFC does not replace the full ownership or cancellation rules. An assignment statement does not authorize early source release or implicit copying of exclusive access.

### Conditional expressions and statements

A conditional expression has the following shape:

```text
if-expression = "if" "(" expression ")" expression "else" expression
```

The condition must have type `bool`. Evaluate it once, then evaluate only the selected branch. `else` is required. Both continuing branches must have a compatible result under the type-join rules.

No ternary `condition ? yes : no` exists. An assignment cannot stand in for a Boolean condition because it is a statement.

In statement position, `if` uses blocks. It permits `else if` and an optional final `else`. Syntactic context selects statement or expression form, not physical line placement. A bare block is not a value branch. A named record construction is an ordinary expression and does not need anonymous-record parentheses.

A statement `if` without `else` can continue when its condition is false. A nested `if` must not silently return its final statement's value.

### Matching and patterns

`match (value) { ... }` is an exhaustive expression. Evaluate its scrutinee once. Inspect arms in source order. For an arm that matches, evaluate its optional `if` guard. If that guard is false, continue to the next arm. Evaluate only the first selected arm body.

Qualified enum patterns, nested record and tuple patterns, literal patterns and `_` follow the type and pattern companion. A guard alone does not establish exhaustive coverage. Reject non-exhaustive matches. This RFC does not introduce general union matching or runtime overload dispatch.

Matching through a borrow does not require `&` in the pattern. Bound values are borrows unless their static type permits implicit copying. Mutable access, source retention and payload movement still need the ownership rules. Pattern success cannot bypass private fields or opaque representation.

After `=>`, a bare `{` starts a block. A named record expression uses its type name. Earlier anonymous-record parentheses are not valid record construction.

A block arm can complete normally in a `void` match. In a value-producing match, a block arm must leave the enclosing function or terminate through a `never` call. Its last expression cannot supply the arm value. Exact flow checking for nested transfers remains required authoring work.

Continuing expression arms must have compatible types. ri-kty permits the defined variant-to-parent enum joins. Do not synthesize a structural record, general union or numeric width to join unrelated arms. A `never` path supplies no ordinary value.

### Guards and successful payload bindings

A guard has one of these forms:

```text
guard expression else block
guard let binding-pattern = expression else block
guard let binding-pattern = expression else (error-name) block
```

A Boolean guard continues only when its condition is true. A direct `Result` guard tests `Ok`; a direct `Option` guard tests `Some`. These special guard forms do not add general truthiness to `if` or logical operators.

Evaluate the guarded expression once. A `guard let` exposes the successful payload to the following scope. On an owned container, bind its permitted moved or copied payload. Through a borrow, preserve borrowed access and only permitted implicit copies.

For `Result`, `else (error)` binds its `Err<E>` wrapper, not merely the payload `E`. Borrowed inspection yields borrowed error access, not a cloned owned envelope. An `Option` has no error binding; `else (error)` is invalid there.

Without a success binding, normal cleanup disposes an owned discarded payload. The failure block must not fall through to the guarded continuation. Use an appropriate `return`, `break`, `continue` or `never` call. A break from a loop nested entirely inside the failure block does not satisfy that requirement.

The complete legality of refutable payload patterns remains open. In particular, an `Ok` payload mismatch cannot invent an `Err` for `else (error)`. This draft's examples use irrefutable bindings.

### Loops and local transfers

The supported statement forms include:

```text
while (expression) block
for (let binding-pattern of expression) block
```

A `while` condition must be `bool`. Evaluate it before each iteration. A false result exits normally. Each iteration has its own body scope.

`for ... of` evaluates its source once. It constructs an iterator through canonical `iterate` and repeatedly calls canonical `next` until `None`. Syntax loads those identities without adding source bindings. User extensions still need normal imports and requirements.

`break;` exits the nearest enclosing loop. `continue;` starts its next iteration. These transfers do not cross a callable boundary. This draft adds no labels, loop values, C-style `for`, `do ... while` or loop `else`.

A while-loop continue returns to its condition. A for-of continue completes required cleanup for that iteration before the next advancement. Break and return dispose remaining iterator state as required. They do not invalidate retained sources or excuse outstanding exclusive yields.

Loops need state merging across iterations. A move or borrow can remain relevant on the next backedge. Do not assume each iteration resets the validity of outer values. Exact checking belongs to the ownership companion.

### Return, propagation and wrapping

`return expression;` supplies the enclosing callable's result. `return;` is valid for `void`. Return does not target an outer caller or another closure. A reachable missing non-void result is an error.

Postfix `?` on a `Result` yields its success value or returns its existing failure unchanged. The enclosing function must return a compatible `Result` with the exact error type. There is no automatic error conversion or added origin/cause node.

Postfix `?` on an `Option` yields its `Some` value or returns `None` from an enclosing Option-returning callable. These operations do not turn a `void` function into a fallible function.

Infix `inner ? outer` means wrap and propagate. It applies only to `Result`. It is not recovery or a default-value operation:

1. Evaluate `inner` once.
2. On success, yield its value without evaluating or moving `outer`.
3. On failure, evaluate the outer error value once.
4. Construct a new error wrapper at the infix operator's source location.
5. Retain the original failure and its complete cause chain.
6. Return the new failure from the enclosing Result-returning callable.

The following excerpt is part of case E13. Assume `readConfig()` returns `Result<Config, ReadFailed>`. The enclosing callable returns `Result<Config, LoadFailed>`. Both error types satisfy the required canonical error evidence.

```rill
let config = readConfig() ? LoadFailed {};
```

On failure, the callable returns a new failure caused by the original one. It does not bind an error or default value to `config`.

On failure, wrapping and early propagation are one operation. The enclosing callable needs no additional `return` or postfix `?` at that site. The right operand is an error value, not a mapper or implicitly called constructor. Its type must fit the enclosing error contract. The inner error type can differ. Retaining it requires canonical display, borrow-freedom, sendability and shareability evidence under the error contract.

A borrowed error wrapper cannot be copied into propagation or a retained cause merely because its payload appears copyable. Ordinary return preserves an existing wrapper's metadata. New explicit construction or wrapping adds a new origin. Normal cleanup applies to skipped moves and early exits.

The accepted [RFC 0001 control-flow rule](0001-top-level-expectations.md#control-flow) forbids return or propagation that exits an expectation. A helper or nested closure keeps its own return context. This RFC does not create an implicit fallible test function.

### Pipes and placeholders

`value |> functionName` means one call with the value as its first argument. `value |> functionName(extra)` inserts the value before the written arguments. The right-side call is not executed before insertion.

One `_` can place the input in another argument position. `value |> pad(10i32, _)` calls `pad(10i32, value)`. It still evaluates the input before the written argument. More than one placeholder is rejected. This does not create a partial-application or placeholder expression outside a pipe step.

A step is a callable name, its argument form, or a parenthesized arrow expression. Other right-side forms are rejected. [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md#contextual-callbacks-and-function-values) proposes callable-name resolution and overload contextualization.

A postfix `?` on a pipe step applies to that step's result. `input |> read?` means `read(input)?`. `input |> read(extra)?` means `read(input, extra)?`. It does not propagate the function value.

Under the confirmed precedence table, `input |> read ? Failure {}` wraps the complete step result. Parentheses can make this explicit as `(input |> read) ? Failure {}`. A record error still needs ordinary named construction and required display evidence.

Pipes do not branch automatically on `Result` or `Option`. Use ordinary `map`, `andThen` or `mapErr` when retaining those containers. There is no `|?` operator.

### Discarding results and preserving effects

An unused `Result` is a hard language error, not an advisory warning. In particular, an expression statement cannot silently discard a Result. `let _ = operation();` explicitly discards one with normal cleanup.

Other discarded values still execute their expressions and required disposal. An unused scalar result does not permit removing an observable trap. Exact unused-Result analysis through named storage and aggregate fields remains required companion work.

Check unexecuted source as well as executed paths. Invalid literals and type errors do not become valid through short-circuiting or dead-code elimination. A well-typed trapping arithmetic expression panics only when evaluated. Warnings cannot change acceptance or move that trap into an unexecuted path.

### Proposed M4.3 scalar and static-control procedure

M4.3 ticket ri-3gd records this bounded stage. The owner approved C1 through C4 (ri-3gd, comment 6947086957) at checkpoint e4750b8. Earlier confirmed scope, sequencing and expectation rules remain unchanged. This procedure does not accept the full RFC.

#### Inputs, scope and source identities

Require a completed M4.2 receipt over one retained M3 graph. Reuse the original M1/M2 facts and canonical declaration/type identities. Do not parse again. Do not read live sources. Do not combine captures.

Check supported standalone function bodies, every loaded module's startup statements and each expectation in both execution modes. Ordinary versus test mode changes no static checking rule. Imported startup checking does not imply imported startup execution.

Supported statements are simple named/wildcard bindings, assignment, blocks, statement if, while, Boolean guard, return, break, continue and expression statements. Supported expressions are literals, names/qualification, grouping, primitive operations, references/dereference, conditional expressions and explicit standalone calls.

Named construction/rebuilding, field/index operations, brackets, patterns, match, guard-let, for-of, pipes, arrows, interpolation and error operators retain their later procedures. Nested function/type declarations also remain incomplete until local declaration visibility and checking are specified. Function values and value-callable invocation remain with the callable owner.

An unsupported body/subtree must not receive a completed body fact. Preserve a source obligation and explicitly incomplete secondary diagnostics. Continue independent supported bodies where resources permit. Known unsupported constructs cannot become successfully checked through unreachable-source elimination.

#### Lexical environments and bindings

Keep the type/module and value/function namespaces separate. Named function type binders remain canonical symbolic types. Elaborate body annotations in that binder/module environment using M4.1's type rules. Store additional original-node type uses without mutating earlier receipts.

Resolve local value uses from innermost active scope outward, then through installed module bindings and the explicit prelude. Module qualification traverses installed exports only. It never probes filesystem children or grants private access.

Only already introduced runtime bindings participate in startup's active lexical environment. M3's complete binder inventory is not a runtime-let hoisting rule. A new binding enters scope after checking its initializer; its initializer can read an outer same-spelled binding. Reject duplicate names in one scope, including after an earlier failed initializer. `_` introduces no usable name.

Functions and expectations cannot capture ordinary startup bindings, even when those bindings hold literals. Retain R0502's meaning. Parameters and permitted local values retain their original M3 declaration identities. Static declarations remain available without capturing runtime values.

Keep missing type facts outside the canonical type enum. A failed initializer can retain its binder identity and the original error/obligation that blocked its type. Dependent uses must not invent a type or falsely report an absent binder. An earlier source error does not itself become an unsupported-analysis obligation.

A binding annotation supplies immediate initializer context and fixes its resulting static type. Without one, finish that initializer's inference before later statements. Do not use later assignments or uses to repair missing context. Lexical/type availability is not initialized ownership storage or move validity.

#### Scalar constraints and calls

Use [RFC 0006's proposed literal/primitive procedure](0006-numbers-strings-and-primitive-operations.md#proposed-m43-literal-and-primitive-checking). First collect explicit contexts, fixed operand types and the supported operation's finite relationships. Propagate them before finalizing unresolved literal widths.

Private inference slots are not valid source types. Keep equality/constructor constraints and their source sites separate from published terms. Check structural equations for conflicts and cyclic private inference terms. A completed expression fact contains only fully established canonical types.

When the finite implemented procedure has no permitted anchor, require an annotation. A numeric literal still has no default width. Unsupported generic inference, admissible-but-unimplemented variant/callable relationships and later family/evidence procedures remain incomplete instead of becoming guessed types or fabricated mismatches.

[Explicit standalone calls](0004-functions-overloads-and-generic-requirements.md#proposed-m43-explicit-standalone-calls) use declared signatures and one whole-signature substitution. Expected results do not rank or retry ordinary bodies. Do not inspect a callee's body to strengthen its declared result or control contract.

The proposed [never admission](0005-types-records-enums-and-inference.md#proposed-m43-never-admission) keeps static types separate from normal continuation. It supplies no literal width or missing generic argument. Differing non-never branch types still need their permitted join, even when a condition is known.

For scalar conditional joins, an established branch result can supply shared immediate context to an unresolved sibling literal. It cannot convert an already typed sibling. Two otherwise unanchored numeric branches still need an annotation. Do not use known condition truth to bypass a written branch's type agreement.

#### Places, assignment and access requirements

Retain a distinction between values and places, including read-only versus mutable referent access. Name/place facts identify original binding and expression nodes. They do not establish ownership, backing addresses, access loans or valid source lifetimes.

A read-only borrow wraps its established referent type. Immediate reference context can supply that type for a fresh borrow. Retain the operand's actual type and permitted admission separately. Existing references gain no covariance through this rule.

Borrowing an existing reference's descriptor is not borrowing its referent. Explicit dereference preserves that distinction. Dereference requires an independently established reference constructor and retains its access mode. Never alone cannot guess read-only versus mutable reference access. Do not infer an owner from a logical string view.

A mutable borrow requires a statically writable binding or mutable-reference referent in this slice. Preserve that place's exact referent type. Do not widen a mutable variant borrow to its parent. Reject a proved read-only place. Mutable acquisition from an unclassified temporary remains incomplete, not an invented permission or language prohibition. Later M4 integration must supply its remaining policy.

Assignment requires a statically writable destination and compatible right-hand type. Its destination type supplies immediate context. Compound assignment uses the corresponding primitive matrix and stores that result type. These statements produce no expression value. Record destination/operand source order without executing or duplicating either expression.

Static mutability does not prove loan compatibility, safe replacement, initialized storage or cleanup. Those remain unperformed. No move, implicit copy, disposal or cancellation transition belongs to this procedure.

#### Proposed finite Boolean and exit analysis

Track possible normal Boolean results as the finite sets empty, true, false or both. Boolean literals provide singleton sets without becoming singleton source types. Grouping preserves them and logical negation complements them.

For AND, each false left result contributes false; each true left result contributes the right set. For OR, each true left result contributes true; each false left result contributes the right set. Check both operands' types even when one is skipped.

Immutable Boolean bindings can retain their initializer's set. Parameters, mutable reads and other Boolean-producing operations use both unless they have no normal continuation. Do not evaluate arithmetic/comparisons, inspect callees or infer cross-statement path correlations to refine these sets.

Track normal continuation, returns, loop-targeted breaks/continues and divergence separately. Each target uses its actual source identity. Calls declared with canonical primitive never have no normal continuation. Arithmetic traps do not become statically proved divergence.

A return targets only its current callable. A bare return requires canonical void. Otherwise check the returned expression against the declared result using the permitted type rules. Reject normal returns from never functions. Returns/propagation cannot exit an expectation; unsupported error operators still do not create an implicit callable there.

A break or continue targets the nearest enclosing loop without crossing a callable boundary. A statement if combines selected branch exits under the Boolean sets; a missing else contributes normal continuation when false is possible. Type-check every written branch regardless of its flow contribution.

A while loop evaluates its condition before each iteration. False contributes normal loop exit. True contributes body returns/divergence and breaks targeting this loop. Body normal continuation and this loop's continues lead back to the condition. Consume those local transfers; do not confuse breaks from nested loops with exits from this loop.

A Boolean guard's else block must have no normal continuation, independent of the guard's known truth. Only transfers that leave that else block count. A break from a loop contained wholly inside it does not establish that property.

Check sequential statements even after flow loses normal continuation. Their type/name/target errors remain source errors. They cannot restore a normal path to an already noncontinuing prefix.

A void body can fall through. A non-void body must have no normal fallthrough under this finite analysis. A never body must also have no normal return. Failure of this static proof rejects as insufficient return coverage; it does not claim a witnessed runtime execution.

This proposed precision rule deliberately omits cross-branch correlations. Two separate tests of b and !b do not prove exhaustive return coverage merely because b is unchanged. Use explicit else structure or another supported proof. This owner decision is not an implementation resource limit or an optimizer-dependent rule.

#### Checking outcomes and evidence

Every completed body/node fact retains its canonical type where applicable, source snapshot/span, binding/callee identity, substitutions, expected contexts and static control targets. Statement facts have no expression value; that is not an error placeholder type.

A scalar-stage receipt requires every requested supported context to complete without errors or unresolved obligations. It records data/pattern coverage, family selection, generic/callable evidence, result-use analysis, ownership, cleanup, runtime and editor behavior as unperformed. It is not a checked program or complete typed-stage success.

Unsupported constructs produce Incomplete. Exhausted checked work/storage/depth limits produce ResourceLimit. Known source findings remain in explicitly partial results. Incomplete secondary diagnostics cannot be advertised as exhaustive. Never use unresolved facts or exhausted resources as missing-evidence proofs or successful fallbacks.

Charge source/scope visits, digits, constraints, substitutions, queue operations, Boolean/exit facts and provenance before growth. Use bounded term traversal and iterative processing for flat expression/statement chains. Do not recursively grow the host stack along arbitrarily long binary chains. No source validity, wall-clock or OOM guarantee follows from implementation budgets.

### Proposed M4.4 match and guard control

M4.4 ticket ri-qtl owns these data/control refinements. The owner approved K1 through K4 (ri-qtl, comment 6957273163) before model/compiler code. Earlier approvals and the full RFC status remain unchanged.

Match evaluates its original scrutinee position once. Pattern guards and bodies retain source order. Check every written pattern, guard and body without executing them. Coverage uses RFC 0005's static constructor procedure rather than known runtime values.

Create an arm scope for its canonical pattern binders and explicit variant view. Check the guard there with Boolean context. A failed guard restores the preceding scope before the next arm. The original scrutinee is not reevaluated.

Expression arms use the established immediate context and permitted joins. A block arm supplies void only when it can continue normally. Its final expression never supplies a value.

The proposed control precision also permits a block arm that has no normal continuation to supply no ordinary join value. Valid returns, outward loop transfers and recognized divergence can establish that property. This extends the earlier draft's return/never-call wording; it requires scoped approval.

Retain each return, break and continue target separately from divergence. A break consumed by a loop inside the arm does not leave the arm. A transfer to an actual enclosing loop does. A noncontinuing block is not an invented ordinary value.

Expression composition must carry these exit alternatives through calls, operators, conditional branches and short-circuiting. A skipped arm or operand contributes no executed transfer. Its static errors still reject. No normal-continuation path can reappear after a noncontinuing prefix.

Ordinary pattern bindings introduce names only after their initializer and require proved irrefutability. Guard success bindings introduce names only after their failure block. They cannot enter the failure scope. A named error binding belongs only to that failure scope.

Guard-let requires admitted canonical Option/Result contracts. A same-spelled enum or matching payload shape is insufficient. Missing core admission remains incomplete for later M4 integration, not a fabricated library definition or negative requirement proof.

The proposed refutable-payload policy permits a pattern mismatch to enter an unnamed failure block. For Result, that block can therefore handle Err or an Ok payload mismatch. It receives no invented Err wrapper on the latter path.

A Result failure block with an error-name requires a proved irrefutable success-payload pattern. Only a real Err branch can supply that error wrapper. Option still cannot bind an error-name. Refutable or unresolved patterns cannot manufacture an envelope to satisfy this syntax.

Check failure-block noncontinuation with the actual enclosing callable/loop context. Resource or unresolved coverage cannot prove fallthrough or irrefutability. No ownership transfer, payload destruction, cloning or loan commitment occurs during these checks.

No direct Option/Result guard can complete until actual canonical core admission exists. The bounded M4.4 implementation must retain this prerequisite explicitly. It can still check ordinary user-enum matches and irrefutable patterns without assuming core identities.

### Proposed M4.8 result-use checking

M4.8 ri-dm4 proposes a rule for discarded results in named storage and aggregates. It requires separate checkpoint approval. It uses admitted Result identity, not enum spelling. A proved `Ok` still belongs to Result. References do not own the referenced Result.

#### Which operations count as use

Track each owned Result occurrence, including an occurrence inside a record, tuple, variant payload, list or capture. Use exact substitutions and real stored members, not all written type arguments. Track private parts without making them visible to source. Unused type arguments and uninhabited alternatives introduce no occurrences. Use only admitted containment facts for opaque core storage. A containing Result also has its own obligation, distinct from any nested Result.

Follow the obligation through local bindings, aggregate construction, capture, rebuild and projection. A new local name does not satisfy it.

Observe a Result through its discriminant or an operation that checks its outcome. Such an observation handles only that occurrence. Borrow creation, printing, and reading an unrelated member do not count. Inspection that merely returns a descriptor also does not count. Use canonical contracts to distinguish observers from other calls.

An explicit `let _ = value;` acknowledges every contained occurrence in that value. A written wildcard in a destructuring pattern acknowledges the component it matches when the pattern commits. In a guarded arm, commit only on the guard-true edge. A guard-false edge carries the unacknowledged component to the next arm. An implicit omission, scope exit or explicit `dispose` is not acknowledgement.

A pattern that tests a variant observes its discriminant when the test executes. That test does not acknowledge any payload. Payload bindings carry their own obligations. A direct guard handles the guarded discriminant, but does not acknowledge an unbound nested Result in a discarded payload.

Forwarding an owned value across a checked return or by-value call boundary is a use at that boundary. Check known contained obligations at a source callee's parameter entry. Check returned values at their receiving expression. A call through an interface without checked result-use semantics cannot substitute for a checked boundary. These rules check typed source and contracts. They do not prove that ownership transfer or disposal is legal.

#### Where to check

Use the typed control edges and place identities from the shared checker. Seed each occurrence at its source or entry site. Keep its canonical component path and current storage location. A forwarding event inside a body changes the tracked location, not the source site. Project only the selected components.

On replacement, check the displaced value before installing the new one. On a normal scope exit, check each obligation whose location leaves scope. A return only forwards the returned components. A `break` or `continue` leaves the scopes of its actual target.

Check each alternative with its own variant and residence facts. At a join keep all feasible alternatives. One handled branch cannot clear a pending branch. Do not require result acknowledgement on fatal or diverging exits.

Check simple discarded expressions and typed local flow in unexecuted source. For an unreachable region start at its lexical entry with the bindings visible there. Do not merge those paths into reachable normal-flow facts. Do not require evaluating user code or folding general expressions to decide feasibility.

For loops, merge iteration and normal exit alternatives at their actual control targets. Use finite states for a fixed set of sites and component paths. Include a separate occurrence partition when values from different iterations can coexist. Do not conflate those values at their common source site.

If dynamic storage, aliasing or recursive containment needs a summary, preserve which obligations it can prove. Without a sound summary, keep the dependent judgment incomplete. Use the same rule for unknown closure environments. Keep open generic components as predicates on substituted storage shapes. Discharge such predicates only by a parametric proof or an applicable specialization. An unresolved predicate is not proof of absence.

At a proved implicit discard, report R0103 at the discard or exit site. Retain the source and component path as related evidence. Do not report a discard based solely on a missing type, admission or unresolved path. Bound the traversal, state comparisons, retained witnesses and worklist by the analysis limits. Resource exhaustion is not a completed result-use check.

#### Pipe and iteration handoff

For pipes, use the parsed insertion position, including the written placeholder when present. Type the input once, using the established first or placeholder argument context. Use the same call rules and record the input before the written arguments. A postfix question mark on the step applies to the call result. Do not introduce implicit Result handling.

For `for ... of`, use the admitted canonical `iterate` and `next` families without installing source bindings. Apply normal ordinary selection and evidence, including no fallback after selecting a body with failed evidence. Establish iterator type `I` from `iterate` on the actual source. Check `next` with the required mutable access to `I` and require exact `Option<T>` with admitted Option. Use `T` for the iteration pattern under the existing binding rules.

Keep the source node, selected operations, substitutions, pattern and loop targets. A hypothetical iterator or missing `next` output supplies no yield type. These facts grant no loan or permission to advance with live exclusive access.

Track unyielded owned occurrences in the iterator according to the checked iterator contract. On `Some`, follow the yielded components into the iteration pattern and retain the remaining components. On `None`, require evidence for any assertion that no owed components remain. Check `break` and `return` for pending occurrences that leave scope in the iterator. A borrowed iterator leaves the source's owned occurrences with the source.

If a contract does not distinguish yielded and retained components, keep dependent exit checks incomplete. Do not equate iteration with acknowledgement of every element.

### Required companion rules

| Rule owner | Required contribution | Dependency state |
| --- | --- | --- |
| [RFC 0001](0001-top-level-expectations.md) | Top-level expectations, test execution and trusted observations. | Accepted snapshot identified in metadata. No amendment is proposed. |
| [RFC 0002](0002-modules-and-program-loading.md) | Module names, both namespaces, privacy and checked startup. | Draft. Its filesystem and ordering policies are confirmed, not the complete RFC. |
| [Draft RFC 0004](0004-functions-overloads-and-generic-requirements.md) | Callable syntax, return signatures, argument inference, overload resolution and explicit evidence. | Proposed, not accepted. Complete grammar and checking rules remain incomplete. This document owns control targets and sequencing, not candidate ranking. |
| Draft RFCs [0005](0005-types-records-enums-and-inference.md) and [0006](0006-numbers-strings-and-primitive-operations.md). | Type relationships and patterns, numeric/text values, literals and primitive operations. | Both remain Draft. Complete grammar and pattern/ownership judgments remain incomplete. The numeric literal/parser/decoder precision is confirmed. Strict conversion entry points are confirmed. Broader core pair details follow the predictability audit in RFC 0006. |
| [Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md), with concurrency/runtime protocols still missing. | Moves, copies, reborrows, state merges, disposal order, retained sources and cancellation. | Proposed, not accepted. Full flow/route and cleanup-state judgments remain incomplete. Early transfer does not permit early source release. |
| [Draft RFC 0008](0008-errors-propagation-and-caused-by-wrapping.md), with iterator ownership still missing. | Canonical Result/Option operations, error construction, cause storage and iterator interfaces. | Errors are proposed, not accepted. Complete error judgments and iterator contracts remain incomplete. Catalogue signatures alone are not a normative annex. |
| [Draft RFC 0010](0010-diagnostics-and-reporting.md) | Stable problem meanings, warnings and precise source context. | Proposed, not accepted. Complete reporting/source judgments remain incomplete. This draft uses assigned codes only where the source registry fixes them. |

## Valid/invalid examples and diagnostics

The guide examples are cases E1 and E2. The remaining snippets are independent examples or excerpts with stated context. Fatal and rejection cases do not belong in a passing suite.

Case E3 is a complete intended-rejection module. Its block has no explicit return value.

```rill
function missingReturn(): i32 {
  7i32;
}
```

Case E4 is a complete illustrative module for assignment and short-circuiting.

```rill
function counted(): i32 {
  let mut count: i32 = 0;
  while (count < 3i32) {
    count += 1i32;
  }
  return count;
}

expect counted() == 3i32;
expect false && (1i32 / 0i32 == 0i32);
```

The second expectation is deliberately false. Its division is not evaluated. It is not a passing test or an observed execution trace.

Case E5 is an excerpt under the confirmed binding-scope policy.

```rill
let count = 1i32;
{
  let count = count + 1i32;
}
```

The inner initializer reads the outer binding. Without that outer binding, self-reference is unresolved. Same-scope redeclaration is a different rejection case.

Case E6 compares the confirmed whitespace-insensitive forms. Assume `read()` returns `Result<i32, i32>` inside a callable returning that same type.

```rill
let smaller = (read()?) - 1i32;
let contextual = read() ? -1i32;
let unspaced = read()?-1i32;
```

These are separate examples. The first propagates unchanged before subtraction. The second and third lazily use `-1i32` as an outer error value and propagate. If `read()` succeeds with `10i32`, the first yields `9i32`; the others yield `10i32`. These are expected results, not observed execution.

Case E7 is a complete module. Assignment cannot produce the expression required by return.

```rill
function assignmentValue(): i32 {
  let mut count: i32 = 0;
  return (count = 1i32);
}
```

Case E8 is an excerpt with a visible `Option<i32>` value named `candidate`.

```rill
guard let value = candidate else (error) {
  return;
}
```

The Option form has no error binding. This is independent of the enclosing callable's other return checks.

| Case / artifact | Context | Expected outcome | Owning proposed rule | Evidence |
| --- | --- | --- | --- | --- |
| E1, first guide module | A borrowed List supplies element and literal context. | Intended totals are 5 and 2. The guard exits before the later 9. | [Bindings](#bindings-and-inference), [loops](#loops-and-local-transfers). | HandChecked control intent. Compiler typing and execution are NotChecked. |
| E2, magnitude module | A known `i32` parameter supplies width context. | `-3i32` produces `3i32`. Minimum `i32` can panic. | [Conditionals](#conditional-expressions-and-statements), numeric companion. | HandChecked for the shown input. No Rill execution. |
| E3 | Non-void block ends without return. | Reject the missing result. No new diagnostic code is assigned here. | [Blocks](#expressions-statements-and-blocks). | HandChecked source rule. No flow checker ran. |
| E4 | A mutable counter and a skipped arithmetic trap. | The counter returns 3. The false expectation skips division and reports failure. | [Loops](#loops-and-local-transfers), [effects](#discarding-results-and-preserving-effects). | HandChecked. No runtime or report is claimed. |
| E5 | Inner shadowing under the confirmed scope rule. | Read the outer initializer source before introducing the inner binding. | [Bindings](#bindings-and-inference). | Confirmed policy. Compiler lookup is NotChecked. |
| E6 | Parenthesized postfix use and two equivalent wrapping spellings. | Propagate then subtract in the first example. Wrap and propagate in the other two. Spacing does not select the operation. | [Parsing](#precedence-and-question-mark-parsing), [errors](#return-propagation-and-wrapping). | Confirmed option B. Parser and error lowering remain NotChecked. |
| E7 | An assignment appears in value context. | Reject it as invalid expression syntax, with R0001 meaning. | [Assignment](#assignment-and-replacement). | HandChecked. No emitted diagnostic. |
| E8 | Option guard has an error binding. | Reject the unsupported binding. No specific additional code is assigned here. | [Guards](#guards-and-successful-payload-bindings). | HandChecked against ri-8hm. No compiler result. |
| E9 | Two independent non-copying sources appear in a pipe and its argument. | Evaluate input once before the written argument. Placeholder placement does not duplicate it. | [Evaluation](#evaluation-order), [pipes](#pipes-and-placeholders). | Confirmed sequencing policy. Effect and ownership checks are NotChecked. |
| E10 | A non-void match has a block ending in an ordinary value expression. | Reject missing value/return behavior. Do not infer an implicit arm value. | [Matching](#matching-and-patterns). | HandChecked against the explicit-return source rule. |
| E11 | A borrowed Result failure is used for owned propagation. | Do not clone its unique envelope or erase dependencies. | [Errors](#return-propagation-and-wrapping), ownership/error companions. | Supporting boundary only. Full checking is NotChecked. |
| E12 | A guard failure block breaks only an inner loop and then falls through. | Reject the guard continuation. | [Guards](#guards-and-successful-payload-bindings). | HandChecked control target. No CFG implementation. |
| E13, wrapping excerpt | An infix right operand constructs an error or can perform effects and move an owner. | Skip it on success. On failure, evaluate it once, wrap and return immediately. Do not bind an error as the success value. Preserve required cleanup. | [Errors](#return-propagation-and-wrapping). | HandChecked source decision. Runtime behavior is NotChecked. |
| E14 | Return, break or continue leaves initialized resource state. | Complete required normal cleanup without promising rollback or prompt cancellation. | [Loops](#loops-and-local-transfers), [assignment](#assignment-and-replacement), cleanup companion. | NotChecked. No disposal or cancellation proof. |
| E15 | A function call is final-looking but retains caller storage or cleanup. | Keep an ordinary call unless proper-tail eligibility actually holds. | [Cleanup and tail calls](#cleanup-and-tail-calls). | Supporting backend boundary. No generated-code evidence. |

The drafting agent compared these examples with the listed rules on 2026-11-16. Where the source does not fix a diagnostic code, this draft gives only its intended meaning. Hand checking is not parsing, typing, execution or a readiness review.

### Proposed M4.8 result-use cases

Assume the canonical Result role is admitted and all other type checks complete. These cases do not supply actual admission evidence.

| Case and context | Expected outcome | Rule | Evidence |
| --- | --- | --- | --- |
| U2a. A local record holds two owned Results. The body matches the first discriminant and reads no other field before returning `void`. | Report R0103 for the second field at its normal exit. Do not report the first field. | [Result-use checking](#proposed-m48-result-use-checking). | NotChecked. |
| U2b. A match arm has a nested wildcard over an owned Result and a guard that is false. The next arm binds that component and leaves it unused. | Do not acknowledge the wildcard on the guard-false edge. Report R0103 for the unused component on the later exit. | [Pattern commitment](#which-operations-count-as-use). | NotChecked. |
| U2c. An owned iterator is known to retain a second Result when the loop acknowledges the first yield and breaks. | Report R0103 for the retained component. If the retained shape cannot be proved, leave the dependent judgment incomplete. | [Iteration handoff](#pipe-and-iteration-handoff). | NotChecked. |

## Feature interactions

### Expectations and modules

RFC 0001 owns test-only expectation placement and execution. This RFC only references its accepted control boundary. A test expression does not receive a hidden Result-returning function.

RFC 0002 proposes lexical source bindings, private access and non-capturing declarations. Local expression checking cannot bypass those rules. A matching field shape does not create a nominal identity or source permission.

### Ownership and branch state

Branches and loops must merge initialization, move and dependency states. A value moved on one continuing branch cannot be treated as definitely available afterward. A returning branch has no later normal continuation, but must still satisfy its cleanup obligations.

Borrowed results and captures can retain sources beyond the immediate expression. Assignment, pattern binding and propagation cannot erase those dependencies. Definite initialization and exclusive-access checking need the complete ownership companion.

### Cleanup and tail calls

Normal return, loop transfer and recoverable propagation preserve required disposal and joins. Fatal panic does not unwind. Intrinsic cancellation has separate deferred-cleanup rules; it is not a catchable Result branch.

A final-looking return does not establish proper-tail eligibility. Retained caller storage or post-call obligations can require an ordinary call. Wrapping an error can add post-call work. Do not shorten lifetimes or omit cleanup to obtain a tail transfer.

The stack/backend companion must own full eligibility and support rules. This RFC does not introduce a tail keyword or claim that a source recursion example proves bounded activation growth.

### Source reconciliation

Repository inputs use [`f33dcf4cf3f815824eeb897135775d7884f65c1a`](https://github.com/Reilley64/rill/tree/f33dcf4cf3f815824eeb897135775d7884f65c1a). Later decisions take precedence over historical forms in the original tickets.

| Input | Disposition |
| --- | --- |
| Selected parsing option B (ri-48g, comment 6909516496) | Parse wrapping when a right operand can begin; otherwise parse postfix propagation. Whitespace, comments and error types do not select the role. The full RFC remains Draft. |
| Approved recommendations 2 through 4 (ri-48g, comment 6909312678) | Fix precedence, evaluation order and binding initialization/scope. The later option B decision resolves parsing. The full RFC is not accepted. |
| User clarification during drafting (ri-48g, comment 6908782235) | Infix `?` with an error wraps and propagates immediately. Bare postfix `?` propagates unchanged. No parsing proposal is approved by this clarification. |
| ri-457 core decisions (comment 6308790321) | Preserve local inference, explicit block returns, expression arrows, `void` and `never`. Later width, enum and construction decisions replace its older type forms. |
| ri-8hm control decisions (comment 6308855139) | Preserve guards, borrowed pattern inspection, loop transfers, explicit discard and fatal panic. Later cause and numeric decisions replace trace and float-trap wording. |
| ri-n1j pipe decisions (comment 6308942432) | Preserve left-grouped low-precedence pipes, one placeholder, step propagation and conditional forms. Later ri-vtw adds infix wrapping. It does not restore a ternary. |
| ri-lql and ADR 0007 | Preserve `let mut`, statement assignments, single-evaluation replacement and conservative dependencies. Full ownership checking remains companion scope. |
| ri-40y and ADR 0009 | Preserve cancellation deferral through disposal/replacement installation and required cleanup/joins. No force kill or global join is added. |
| ri-a2c and ADR 0011 | Preserve explicit widths and static-invalid versus panic-if-evaluated distinctions. Keep IEEE float behavior and no optimization-dependent acceptance. |
| ri-kty/#21 and ADRs 0012/0008 | Use nominal enum variants and named record construction. Do not restore singleton types, general unions or anonymous record values. |
| [ri-vtw error decision](../adr/0013-caused-by-errors-with-explicit-wrapping.md), ri-8n7 catalogue | Preserve exact propagation, lazy error-value wrapping, origins and unique retained causes. No mapper RHS, implicit conversion or propagation trace. |
| ri-8n7 catalogue sections 3/4 | Use canonical iterator operations, actual void-valued Ok input and the published Option/Result boundaries. Complete interfaces need companion owners. |
| ri-3t8 diagnostic contract | Keep unused Result as a hard error. Warnings and bounded observations do not change source effects or acceptance. |
| Accepted RFC 0001 snapshot (ri-48g, comment 6907867211) | Preserve the expectation control boundary. No accepted rule is amended. |
| RFC 0002 policy decisions (ri-48g, comment 6908371073) | Preserve confirmed module identity and ordering. Lexical binding proposals are separate. The complete module RFC remains Draft. |
| Current examples and [consolidation record](../prototypes/consolidation.md) | Use current paths and preserve original source/fixture bytes. Historical comments are not new executed tests. |

## Compatibility and migration

No released compiler behavior needs migration. Existing sources already use explicit returns, `let mut`, named constructors and ordinary loops. They use adjacent postfix questions and separated infix wrapping.

Option B keeps question-mark roles independent of whitespace and comments. Arithmetic, calls or indexing after postfix propagation can require parentheses. Formatting alone must not change the operation. Preserve token boundaries and required grouping.

Earlier implicit singleton inference, union matching and anonymous record construction do not return. Older `Ok()` conveniences are not a general overload. Pass an actual void-valued expression to the current helper when required.

Existing pipe syntax stays left-grouped and low-precedence. The confirmed wrapping precedence removes the need for some explanatory parentheses, but keeps them valid. Compound or nested wrapping should remain explicit where readability needs it.

## Security/resource implications

Unicode identifiers can contain confusable characters or visually hidden continuations. Exact spelling does not detect those similarities. Diagnostics must preserve identity while safely displaying the actual source, not merge similar-looking names.

Expressions can perform I/O, suspend or transfer ownership. A parser or optimizer cannot repeat them to decide their meaning. Conditional and error paths do not promise rollback of prior effects.

Early exits can retain resources until required cleanup or native completion. A pending cancellation request cannot free those sources early. This RFC promises no hard deadline, automatic deadlock prevention or global cleanup after panic.

Checked arithmetic still needs safe backend lowering without poison-dependent behavior. Static rejection of an invalid literal differs from a runtime trap. Diagnostic limits cannot turn incomplete checking into success.

Pattern inspection and error propagation must preserve private access, exclusive restrictions and retained causes. A source-level explanation or finite reference experiment does not prove those guarantees.

## Rationale, alternatives and prior art

The confirmed lexical policy pins Unicode membership to keep identifier admission repeatable. Unicode identifiers preserve expressible Unicode import names; an ASCII-only rule would narrow them. Exact spelling avoids silently adding normalization to existing source/module identities.

Nested comments permit commenting an existing block comment without exposing its suffix as code. Longest supported-token matching keeps lexing independent of type inference. Full parser grammar must still distinguish generic contexts and question-mark roles.

Explicit returns keep block syntax independent from value production. Expression-bodied arrows remain concise. An implicit final-expression return would change the confirmed ri-457/#11 model and complicate statement migration.

A pipe is call syntax, not a separate Result executor. One placeholder covers a non-first parameter without repeating the input. Ordinary library combinators handle container-preserving composition.

The confirmed option B selects infix `?` when a right operand can begin. It avoids changes in meaning from spacing or comments. Its cost is explicit parentheses around some uses of a propagated value.

The earlier adjacency option A would give `value? - 1i32` and `value ? -1i32` different meanings. The owner did not select it. A new wrapping token could avoid the ambiguity but would change the confirmed `inner ? outer` notation. Type-directed parsing would couple grammar to error types and overload evidence. It is not adopted.

[ri-vtw](../adr/0013-caused-by-errors-with-explicit-wrapping.md) cites Roc's infix question form as notation input. Rill evaluates a lazy error value rather than invoking a mapping handler. No other Roc semantics are imported.

## Implementation implications

The workspace has no Rill lexer, parser, type and flow checker or expression lowering. Current CLI tests validate routing, not this language. No generated CFG or cleanup transform exists.

A parser needs distinct statement and expression contexts. It must classify `?` from permitted operand starts and delimiters, ignoring whitespace and comments. Type checking follows that parse. A formatter must preserve tokens and grouping. These are requirements, not existing implementations.

Lowering must represent normal values, early returns, loop transfers and fatal paths separately. Preserve source attribution for explicit wrapping and construction. Do not turn recoverable Result flow into exception unwinding.

Evaluate pipe input through one temporary, not copied source substitution. Preserve source evaluation order while checking calls against canonical family evidence. Branch/backedge state and replacement installation need the ownership model before monomorphization.

If the grammar or checking rules conflict with a confirmed guarantee, return the conflict to ri-kl0 and ri-48g. Do not silently weaken cleanup, accept type-directed parsing or treat an unimplemented rule as successful checking.

## Evidence and checking plan

Cases L1 through L17 are new lexical inputs/scenarios from 2026-11-17. They remain NotChecked by a Rill lexer. Source comparison and document checks are not tokenization, parsing or implementation acceptance.

### Actual evidence

| Artifact / revision | Evidence label | Rule exercised | Checker / environment | Expected / observed result | Limits |
| --- | --- | --- | --- | --- | --- |
| E1 through E15 in this draft | HandChecked | Intended binding, control, error and pipe behavior. | Drafting agent, 2026-11-16. Manual comparison with the named rules. | Expected outcomes are stated. New proposals and missing companion rules remain explicit. | No Rill parser, checker, execution or readiness review. |
| Existing sources at `f33dcf4cf3f815824eeb897135775d7884f65c1a` | HandChecked | Current expression, return, guard, match, mutation and loop spellings. | Source inspection. | Examples agree with the recorded notation where later decisions apply. | This is not complete syntax/type checking or acceptance of every source line. |
| Unicode 17.0.0 proposed annex and coordinate data, 2026-11-17 | ToolChecked | Frozen property-table identity and representative membership/data boundaries. | HTTPS retrieval from Unicode.org; temporary `/tmp/rill-lexical-data-check.py` through `mise exec -- python`. | The declared table digest, eight representative membership assertions and Y1 byte/UTF-16 arithmetic passed. | This checks source data, not a Rill lexer, parser, source mapper or Unicode normalization algorithm. |
| Question-mark grammar and precedence | NotChecked | Operand-based role, grouping and pipe-step lowering. | No parser is available. | No parser outcome is claimed. | Confirmed option B and precedence still need complete grammar and parser tests. |
| Ownership and cleanup lowering | NotChecked | Single evaluation, retained storage, replacement and early exits. | No checker or runtime is available. | No safety or runtime result is claimed. | Requires missing companions and executable validation. |

### Planned checks. Not executed evidence

- Parse every expression and statement form. Test both conditional contexts and missing semicolons. Reject assignment expressions, increments and removed syntax.
- Test the full precedence table. Check equivalent question-mark parses across whitespace, comments and newlines. Include parenthesized postfix arithmetic/calls/indexing, nested wrapping and pipe suffixes.
- Count effects for callees, arguments, aggregates, assignment places and pipe placeholders. Each required source expression must execute once in the specified order.
- Test initialized bindings, inner shadowing, same-scope collisions and missing widths. Reject look-ahead inference from later statements.
- Test Boolean short-circuiting and conditional laziness with isolated fatal operations. Static-invalid literals must still reject on skipped paths.
- Test value and void match arms, exhaustiveness, guarded patterns and borrowed bindings. Do not permit implicit block values or private representation access.
- Test guard success, error bindings and required control targets. Include a break that exits only an inner loop.
- Test while/for break, continue and return with iteration cleanup. Validate borrowed yields, state merges and retained iterator sources.
- Test exact Result error types, Option propagation, lazy outer errors and preserved origins/causes. Do not clone borrowed error envelopes.
- Test explicit Result discard and complete unused-Result flow rules. Ordinary resource cleanup must remain observable.
- Test cancellation during assignment evaluation, disposal and installation. Test fatal paths separately without expecting unwind or a final report.
- Compare development and optimized builds after real implementations exist. Structural source checks cannot replace execution tests.

## Open/deferred questions

<a id="proposed-lexical-review-choices"></a>

### Confirmed lexical policies

The owner approved all three lexical/source groups (ri-48g, comment 6929923825) on 2026-11-17. The pre-recording RFC 0003 SHA-256 is `ad68791cccedb4b94f86d87b4d0b13e89ca25b0a0abf319ec7f86075061e5289`. The shared record identifies the exact RFC 0002, 0006 and 0010 snapshots.

1. Names/tokens use the pinned Unicode profile, complete keyword/punctuation set, nested comments and longest supported-token matching.
2. Literal scanning uses strict numeric candidates, literal/hole modes, exact escapes, control exclusions and specified physical line-ending content.
3. Source processing preserves immutable bytes, strict UTF-8, original-coordinate projection and finite recovery without invented source success.

This includes lone CR, initial BOM handling and unavailable CRLF-interior projection. Frozen XID ranges participate in the identifier policy only. The decision does not accept complete RFC texts or future unwritten procedures.

Complete type/flow/context/client rules and actual lexer/parser tests remain separate work.

### Review choices and remaining authoring work

The owner confirmed precedence, evaluation order and binding initialization/scope in the earlier decision (ri-48g, comment 6909312678). The owner then selected whitespace-insensitive option B (ri-48g, comment 6909516496). These policy questions are no longer open. The lexical/source and concrete syntax groups are confirmed. Complete type/flow checking rules remain authoring work.

The shared syntax annex records confirmed generic-argument, conditional-boundary and pipe-step policies. Its decision does not complete semantic checking. No parser success follows from the sketches or EBNF.

Complete binding-pattern coverage and refutable guard-payload behavior. Complete static flow rules for divergence, value-match blocks and required returns. Complete unused-Result analysis through bindings and aggregates.

[Draft RFC 0007](0007-ownership-borrowing-mutation-and-cleanup.md) proposes ownership and cleanup. [Draft RFC 0008](0008-errors-propagation-and-caused-by-wrapping.md) proposes errors and causes. [Draft RFC 0010](0010-diagnostics-and-reporting.md) proposes diagnostics/reporting. Iterator, concurrency and full LSP owners remain missing. Their exact sections/revisions are needed before ReviewReady. These gaps are not implied approvals of weaker behavior.

### Implementation/validation gates

The lexer, parser, checker, CFG, ownership model and runtime are absent. Execution, target lowering, cleanup/cancellation and snapshot tests have not passed. ri-48g retains formalization and backend acceptance; ri-bv2 retains actual LSP delivery.

### Future/out-of-scope work

This RFC adds no ternary, `|?`, assignment expression, increment/decrement, catchable panic or general block value. It adds no loop labels, loop values, public async syntax or runtime overload dispatcher.

## Decision and change history

### Readiness record

No canonical readiness review has occurred. Status is Draft. Source comparison and structural checks are authoring work only. Incomplete grammar and companion rules prevent completion of the [canonical checklist](README.md#canonical-reviewready-checklist). The policy approvals do not complete that review.

### Human decisions and amendments

On 2026-11-17, the owner requested the complete syntax grammar. The owner then confirmed all three syntax groups (ri-48g, comment 6931939690) at the two identified annex snapshots. Existing lexical/source and semantic approvals retain their exact scopes. Full annex/RFC acceptance, readiness and implementation remain separate.

On 2026-11-17, the owner requested the lexical/source contract. The owner then confirmed its three policy groups (ri-48g, comment 6929923825) across the four identified snapshots. This does not accept the full RFCs or validate a lexer.

| Date / actor | Change | Reviewed revision or snapshot | Decision / rationale |
| --- | --- | --- | --- |
| 2026-11-16, project owner | Requested this expression, binding and control-flow RFC. | No RFC 0003 text was accepted. | This authorizes drafting, not new syntax approval. |
| 2026-11-16, drafting agent under ri-48g | Created and registered substantive RFC 0003. | Inputs use `f33dcf4cf3f815824eeb897135775d7884f65c1a` and the accepted RFC 0001 snapshot identified above. | Parsing, precedence and completion details are proposals. No implementation or formal ASD-STE100 compliance is claimed. |
| 2026-11-16, project owner | Clarified that infix `?` with an error means wrap and propagate. It is not recovery. | Clarification of the operation, not acceptance of RFC 0003. | Recorded clarification (ri-48g, comment 6908782235) preserves ri-vtw semantics. It does not approve the proposed parsing or spacing rule. |
| 2026-11-16, project owner | Approved recommendations 2, 3 and 4. Recommendation 1 remains open pending examples. | The precedence table and evaluation/binding recommendations presented in chat. No complete RFC revision was accepted. | Recorded approvals (ri-48g, comment 6909312678). Status remains Draft. |
| 2026-11-16, project owner | Selected whitespace-insensitive question-mark parsing, option B. Option A is not adopted. | The two syntax designs and examples presented in chat. No complete RFC revision was accepted. | Recorded selection (ri-48g, comment 6909516496). Complete grammar, readiness and implementation remain open. |
| 2026-11-17, project owner | Approved the three coordinated lexical/source groups, including pinned identifier membership and lexical boundaries. | Pre-recording SHA-256: `ad68791cccedb4b94f86d87b4d0b13e89ca25b0a0abf319ec7f86075061e5289`. Other owner snapshots appear in the shared decision record. | Lexical policy approval (ri-48g, comment 6929923825). Full RFC acceptance, parser grammar, readiness and implementation remain open. |
