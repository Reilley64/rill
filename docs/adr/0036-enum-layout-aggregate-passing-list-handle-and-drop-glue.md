# Enums are a tag and a payload, aggregates return through an out-pointer, a List is a runtime handle

**Status:** proposed, 2026-11-26. Drafted from the chief of staff's ruling 1 of 2026-11-26 on ri-qlh.22 (backend lowering of the full program) and charting Q10, which makes each ABI decision an ADR the owner approves. Acceptance is the owner's; the lowering ticket (ri-qlh.22.25) may start on this draft, as it did on ADRs [0030](0030-inkwell-on-a-source-built-static-llvm-23.md) to [0035](0035-panic-ends-the-program-with-status-101.md). [ADR 0031](0031-owned-strings-are-pointer-length-capacity.md) fixed strings, [ADR 0032](0032-generated-code-allocates-through-three-runtime-functions.md) allocation, [ADR 0034](0034-llvm-default-c-calling-convention-for-slice-1.md) the convention and [ADR 0033](0033-the-runtime-owns-main-and-the-entry-stack.md) startup cleanup. They cover `log("hello world")`. The acceptance program (`prototypes/00-hello.rill`) also calls `inputs()` and `args()`, which return `Result` values, reads a `List<string>` with `get`, matches an `Option<&string>` and ends with owned values to clean up. Nothing decided how those look in generated code.

Everything here is internal and unoptimised. RFC 0005 says no fixed tag encoding or object ABI follows from the language, and nothing below is promised stable. Only the boundary between generated code and `runtime/native` is a contract, and this ADR keeps it to scalars and pointers.

## Recommendation

### 1. Enum layout

- A value of an enum is a tag followed by a payload area.
- The tag is at offset 0. Its value is the variant's declaration index, so `Option` is `Some` = 0, `None` = 1 and `Result` is `Ok` = 0, `Err` = 1. Its width is the smallest of `i8`, `i16` and `i32` that holds the variant count; every enum in slice 1 uses `i8`.
- All variants share one payload offset: the tag's size rounded up to the largest payload alignment among the variants. The payload area is as large as the largest variant payload. The total size is rounded up to the larger of the tag's and the payload's alignment.
- A variant's payload is its field laid out as a record: fields in declaration order, each at its natural alignment, the size rounded up to the largest field alignment. Slice 1 reorders no fields.
- The LLVM type is `{ iTag, [n x iA] }`, where `iA` is an integer of the payload's alignment and `n` covers the largest payload. Code reads and writes a payload through a byte offset into the enum's storage, never through the aggregate type.
- Padding is never initialised and never read.
- No niche and no pointer tagging, including for `Option<&T>`.
- An enum with no variants is uninhabited and has size 0. A payload whose type is uninhabited has size 0 and its variant is never constructed.

| Type | Size | Alignment | Layout |
|---|---|---|---|
| `string`, `Bytes` | 24 | 8 | pointer, length, capacity ([ADR 0031](0031-owned-strings-are-pointer-length-capacity.md)); `Bytes` is open below |
| `&string` | 16 | 8 | pointer, length |
| `List<T>` | 8 | 8 | the runtime handle (section 3) |
| `Inputs = { arguments: List<Bytes> }` | 8 | 8 | the handle |
| `Option<&string>` | 24 | 8 | tag at 0, view at 8 |
| `Option<string>` | 32 | 8 | tag at 0, string at 8 |
| `EnvError = { kind: EnvErrorKind }` | 1 | 1 | the tag of the three-variant `EnvErrorKind` |
| `SourceLocation = { file: string, byteOffset: u64 }` | 32 | 8 | string at 0, offset at 24 |
| `Option<Cause>` | 1 | 1 | tag only: `Cause = { storage: never }` has size 0, so `Some` is never built |
| `Err<EnvError>` | 48 | 8 | `EnvError` at 0, `SourceLocation` at 8, `Option<Cause>` at 40 |
| `Result<Inputs, EnvError>` | 56 | 8 | tag at 0, payload at 8 |
| `Result<List<string>, EnvError>` | 56 | 8 | tag at 0, payload at 8 |

The last two are the two `Result` types of the acceptance program. The LLVM types `{ i8, [2 x i64] }` and `{ i8, [6 x i64] }` have the sizes and payload offsets above on all four targets (see Measured).

### 2. Passing and returning aggregates

In this section, a scalar is a boolean, a character, an integer, a float, a pointer, a list handle, or a `&T` for any `T` other than `string` and `Bytes` (see Open). An aggregate is a string, a `Bytes`, a view, a record, a tuple or an enum, whatever its size. A reference to an aggregate other than a string is a pointer to the aggregate's storage. A `&string` is the two-word view of ADR 0031, and the recommendation gives `&Bytes` the same two words.

- **Scalars** pass and return by value as LLVM scalars.
- **Strings, `Bytes` and their views as parameters** keep the split of ADRs 0031 and 0034: three scalar words for an owned string or `Bytes`, two for a view.
- **Other aggregates as parameters** pass as one `ptr` to storage the caller owns. An owned argument is a move: the callee owns the bytes and drops them, and the caller treats the source place as moved. A `&T` argument is the pointer to the place. The callee never receives the aggregate as an LLVM aggregate and no parameter has the `byval` attribute.
- **Every aggregate result**, a string and a view included, returns through a caller-provided out-pointer: the first parameter, `ptr sret(<type>) align <n>`, and the function returns `void`. The caller allocates the storage and the callee initialises all of it. A callee that panics leaves it uninitialised, and the caller never reads it ([ADR 0035](0035-panic-ends-the-program-with-status-101.md)).
- **Calls into the runtime** pass and return scalars only, as ADR 0032 and the runtime's table already do. A runtime function never reads or writes an enum or a record. It exchanges words, pointers and list handles, and generated code builds the `Result` or `Option` around them. The runtime therefore depends on no layout in section 1.

### 3. List from generated code

- `runtime/collection::List<T>` is a Rust generic over a `Vec<T>`. Its layout is unspecified and generated code cannot call a generic. Q24 links it unchanged.
- A `List<T>` is one pointer-sized handle to a `List<Slot>` the runtime allocated, where `Slot` is a plain 8-byte-aligned block of the element's size. The runtime exports a set of C functions per element size class. Slice 1 needs one class, 24 bytes, for `string` and `Bytes`.
- The functions below name the operations. D2 of the planner's decisions applies: the C names and parameters are the runtime's detail, and ri-qlh.22.25 and ri-qlh.22.13 settle them.

| Operation | Runtime function for the 24-byte class | Backing call |
|---|---|---|
| create | `void *rill_rt_list_w24_from(uint8_t *slots, size_t count)` moves `count` staged slots out of the caller's buffer and returns the handle | `List::from_elements` |
| get | `const uint8_t *rill_rt_list_w24_get(const void *list, uint64_t index)` returns the slot or null | `List::get` |
| len | `uint64_t rill_rt_list_w24_len(const void *list)` | `List::len` |
| copy | `void *rill_rt_list_w24_copy(const void *list, void (*copy)(uint8_t *to, const uint8_t *from))` | `List::copy_with` |
| free | `void rill_rt_list_w24_dispose(void *list, void (*drop)(uint8_t *slot))` calls `drop` once per element in reverse position order, then frees the backing and the handle | `List::dispose_with` |

- There is no push. The collection crate has none, and the ownership MIR builds a list once from staged elements (`CommitList*`, "no partial List is published"), which is `from_elements`. A later slice that needs `push` adds it to the collection crate, which slice 1 leaves unchanged.
- `get` gives an `Option<&T>`: generated code tests the pointer, writes tag `Some` or `None`, and for `Some` writes the element's view. For `List<string>` the view is the first two words of the slot (pointer, length), a copy of two words and never a pointer to the slot, as ADR 0031 requires. For another `T` the payload is the pointer to the slot.
- The handle's box, the `Vec` and its buffer allocate through the runtime's `#[global_allocator]`, so the checking allocator counts them ([ADR 0032](0032-generated-code-allocates-through-three-runtime-functions.md), planner decision D4). Strings inside a list allocate through the three runtime functions and are released by the element's drop glue.
- The runtime builds the `List<Bytes>` of `inputs()` itself with the same `from_elements`, allocating each argument's bytes with `rill_rt_allocate(len, 1)`, so capacity equals length. An empty argument holds the dangling address 1 and capacity 0, as an empty string does ([ADR 0031](0031-owned-strings-are-pointer-length-capacity.md)). One glue, `rill_rt_release(ptr, capacity, 1)` unless the capacity is 0, then releases every `Bytes` correctly.

### 4. Drop glue

- **Who.** `crates/codegen-llvm` generates it: one internal function `void(ptr)` per monomorphic type that needs cleanup, taking a pointer to the value's storage. The monomorphization pass ([ADR 0006](0006-whole-program-monomorphization-on-llvm-23.md), Q27) names each instance. Nothing in the runtime or in `ownership/` generates it.
- **What each does.**
  - `string` and `Bytes`: release the allocation through `rill_rt_release(ptr, capacity, 1)` unless the capacity is 0 ([ADR 0031](0031-owned-strings-are-pointer-length-capacity.md), [ADR 0032](0032-generated-code-allocates-through-three-runtime-functions.md)). This is the existing `rill.string.release`.
  - `&T`, scalars and a type with no owned part: no glue.
  - A record: its `dispose` implementation if one is selected, otherwise each field's glue in reverse declaration order (RFC 0007).
  - An enum: switch on the tag and run the glue of the active variant's payload. `None` and any variant without an owned payload do nothing.
  - `List<T>`: `rill_rt_list_*_dispose(handle, glue of T)`. The runtime calls the element glue in reverse position order. This is the `dispose` implementation `List` has in `stdlib/collection.rill`, bound to the runtime.
- **A partial move** (`intoValue` moving a field out of an `Err`) is a drop of each remaining field's place, which the ownership MIR lists. The lowering calls the field's glue and never the whole value's.
- **When.** At each `Operation::Drop` and `Cleanup::Drop` the MIR emits, and at nothing else. In slice 1 the startup scope's values are cleaned inside `rill_startup` before it returns, as the chief's ruling of 2026-11-26 on ri-qlh.22 says ADR 0033's dated note will record. `rill_startup_cleanup` stays defined and empty, and the call order of `main` is unchanged. A panic runs no glue ([ADR 0035](0035-panic-ends-the-program-with-status-101.md), rule R8).
- **Through what.** Every release goes through `rill_rt_release`. List buffers release inside the runtime through the global allocator.
- **A worked example.** `Inputs` holds `arguments`. Its glue calls the glue of `List<Bytes>`, which calls `rill_rt_list_w24_dispose` with the `Bytes` glue. The runtime visits the elements from last to first, the `Bytes` glue releases each one, and the runtime frees the buffer and the handle. `Result<Inputs, EnvError>`'s glue does this only when the tag is `Ok`. On `Err` it runs the glue of `Err<EnvError>`: `retained`, `site` (releasing `site.file`), then the payload, in reverse declaration order.

## Measured

A throwaway prototype built the same values in C and in LLVM IR and called each across the boundary in both directions: C calling an IR function, and an IR function calling a C function. It checked every field of the value that arrived, and printed the assembly of small probe functions. The sources are `experiments/adr-0036-abi/` (`gen.py` writes the C and the IR, `run.sh` builds and runs them) in commit `7c59d55a`, run by a temporary workflow on all four hosted platforms. The commit that reports this ADR removes both, so no prototype code remains in the tree. Run [45722861314](https://github.com/Reilley64/rill/actions/runs/45722861314) is green on each platform:

| Platform | Job | Compiler |
|---|---|---|
| Linux x86-64 (`ubuntu-24.04`) | [139585235535](https://github.com/Reilley64/rill/actions/runs/45722861314/job/139585235535) | Ubuntu clang 18.1.3 |
| Linux AArch64 (`ubuntu-24.04-arm`) | [139585235713](https://github.com/Reilley64/rill/actions/runs/45722861314/job/139585235713) | Ubuntu clang 18.1.3 |
| Apple arm64 (`macos-15`) | [139585235741](https://github.com/Reilley64/rill/actions/runs/45722861314/job/139585235741) | Apple clang 17.0.0 |
| Apple x86-64 (`macos-15-intel`) | [139585235663](https://github.com/Reilley64/rill/actions/runs/45722861314/job/139585235663) | Apple clang 17.0.0 |

The IR is textual with opaque pointers and uses no attribute newer than LLVM 18. It was compiled at `-O0` by each runner's clang, not by the LLVM 23.1.5 of [ADR 0030](0030-inkwell-on-a-source-built-static-llvm-23.md): the prototype shows how LLVM's backends treat these shapes, and the pinned version is not itself measured. Where the table says "agrees", the value arrived intact in both directions. A result that matched in only one direction is marked "differs", because a direction that matched can do so by accident: on Apple arm64 an IR caller's `byval` 56-byte copy reached a C callee only because the `memcpy` that made it left its address in `x0`, where the C callee expects a pointer.

| IR shape against the C ABI for the same struct | Linux x86-64 | Linux AArch64 | Apple arm64 | Apple x86-64 |
|---|---|---|---|---|
| 16-byte aggregate (`{ ptr, i64 }`) by value, argument and result | agrees | agrees | agrees | agrees |
| 24-byte aggregate (`string`, `Option<&string>`) by value, argument and result | differs | differs | differs | differs |
| 56-byte aggregate (`Result<Inputs, EnvError>`) by value as a result | agrees | differs | differs | agrees |
| 56-byte aggregate by value as an argument | differs | differs | differs | differs |
| `sret` result of 24 or 56 bytes | agrees | agrees | agrees | agrees |
| `sret` result of 16 bytes | differs | differs | differs | differs |
| plain `ptr` out-parameter against a C out-parameter, 16 to 56 bytes | agrees | agrees | agrees | agrees |
| plain `ptr` out-parameter against a C struct return of 24 or 56 bytes | agrees | crashes | crashes | agrees |
| two or three scalar words as separate arguments | agrees | agrees | agrees | agrees |
| `byval` parameter of 24 or 56 bytes | agrees | differs | differs | agrees |
| `ptr` to the aggregate | agrees | agrees | agrees | agrees |
| `{ i8, [n x iA] }` size and payload offset against the layout rule, for enums of 24, 56, 16, 2 and 8 bytes | agrees | agrees | agrees | agrees |

What the assembly shows:

- A first-class aggregate result is returned element by element in registers: `rax`, `rdx`, `rcx` for a three-word value on x86-64, `x0` to `x2` on AArch64. On x86-64 LLVM demotes a seven-element result to a hidden pointer in `rdi`, which is why the 56-byte result agrees there. On AArch64 it returns the seven elements in `x0` to `x6`, which C does not (it uses `x8`).
- A first-class aggregate argument is passed element by element in argument registers, `rdi` onward on x86-64 (a seventh element goes on the stack) and `x0` to `x6` on AArch64. C passes a 24-byte or larger struct in memory on x86-64 and as a pointer to a copy on AArch64.
- `sret` on x86-64 is the hidden pointer in `rdi`, also returned in `rax`. On AArch64 it is `x8`. A plain first parameter is `rdi` or `x0`. C also returns a struct of 16 bytes or less in registers, which `sret` does not match.
- Both Apple targets behave as their Linux counterparts for every shape above. Apple's arm64 ABI packs small stack arguments tighter than AAPCS64. No shape here has one, so that difference is not measured.

The table decides section 2. Generated functions call only each other and the runtime, and the runtime takes scalars and pointers, so C-struct agreement matters only for what crosses into `rill_rt_*`: scalars and pointers agree on all four. The shapes that differ from C can still be used between generated functions, because one compiler produces both sides. The recommendation avoids them anyway, for the reasons in the options: a 24-byte or 56-byte first-class aggregate is a shape no C compiler produces, and `byval` fails on AArch64.

## Consequences

- Lowering needs one rule for any aggregate result and one for any aggregate parameter, and the same rules hold for every type, including generic instances.
- No runtime function depends on the layout of an enum or a record. The runtime sees strings and `Bytes` as three words and a list as a handle.
- A list costs one extra allocation for its handle. Slice 1 accepts that for a layout that does not depend on how rustc lays out a `Vec`.
- Moves cost no copy: the caller's storage is the callee's.
- The slice 3 `tailcc` can forward a caller's out-pointer to a tail callee, which [research 0030](../research/0030-backend-experiments.md) measured with an `sret` buffer. No parameter is `byval`, which that research found fails `musttail` on AArch64.
- Drop glue is a function per type and a call per drop, with no drop flag. The lowering reports a place that is possibly initialised at a join as unsupported.

## Considered options

The ri-qlh record fixes none of these. They are the alternatives the recommendation weighs.

**Enum layout.**

- **Tag in the first byte, payload at its natural alignment (recommended).** It is the plainest layout and the measured LLVM layout agrees with it.
- **A niche in a pointer, or pointer tagging.** It saves a word for `Option<&T>`, but it needs a per-type analysis and the chief's ruling excludes it for slice 1.
- **A word-sized tag.** The layout is then uniform, and `Option<u8>` grows from 2 bytes to 16. It gains nothing the byte tag lacks.
- **A tag after the payload, or per-variant payload offsets.** Neither helps and both make a read depend on the variant before it can find the tag.

**Aggregate passing.**

- **First-class LLVM aggregates by value.** LLVM splits an aggregate argument into its elements and returns it in registers up to a limit. This is not the C struct ABI. The measurements show where it agrees with C and where it does not. It spreads a 56-byte enum over seven registers and leaves the rest to the stack.
- **All aggregates split into scalar words.** It matches C for the words themselves. It cannot return more than one word, and a 56-byte enum would take seven parameters, stack-spilled past six on x86-64 System V and eight on AArch64.
- **Out-pointer for results and a pointer for other parameters (recommended).** It works the same on four targets, matches C where C returns indirectly, never crosses into the runtime, and suits `tailcc`.
- **`byval` parameters.** It matches C on x86-64 System V and fails on AArch64, where C passes a pointer to a copy. Research 0030 found incoming `byval` rejected under `musttail` on AArch64.
- **A plain `ptr` out-parameter without `sret`.** Between generated functions it works and loses an optimiser hint. On AArch64 `sret` travels in `x8` and a plain pointer in `x0`, so the two differ when a C caller or callee is on the other side. Slice 1 has none, and `sret` is what research 0030 checked with `musttail`.

**List.**

- **A pointer to a boxed runtime list (recommended).** It depends on nothing about `Vec`.
- **The list inline, as an opaque block of `size_of::<List<Slot>>()` bytes.** It saves the handle's allocation. It needs the runtime to export that size, and it ties generated code to a rustc-chosen layout.
- **Generated code owns the list as pointer, length and capacity, and `runtime/collection` is bypassed or wrapped.** This contradicts Q24, which links the existing backing unchanged, and duplicates it.
- **Box every element.** It makes the element size irrelevant and costs an allocation per element, against ADR 0006's "no boxing of every value".
- **One runtime function set for any element size.** Rust cannot instantiate a generic at a size known only at run time, so the runtime keeps one set per size class.

**Drop glue.**

- **Generated functions per monomorphic type (recommended).** The lowering owns the types, the runtime stays free of layouts, and the element callback is the one function the runtime needs.
- **A runtime that walks a type descriptor.** It puts every layout in the runtime and a descriptor format in the ABI.
- **Inline expansion at each drop.** It repeats the code and cannot serve as the list's element callback.

## Open

The owner did not decide these. Nothing above decides them.

- Whether `Bytes` is the same three words as `string`, with `&Bytes` the same two words as `&string`. No ADR covers `Bytes`. The recommendation assumes yes: `decodeView` then needs no copy and the runtime builds the argument list from bytes it allocates.
- The tag width above 256 variants. The rule says `i16`; no slice 1 type has it.
- How a `Cause` is laid out once the runtime builds a real one. Slice 1 never constructs `Some(Cause)`.
- Element size classes beyond 24 bytes, classes with smaller alignment, and a `push` in the collection crate.
- Whether the checked MIR can leave a place possibly initialised at a join, which would need a drop flag.
- Whether a Rust panic inside the collection crate, such as an index past the end in `Index`, is turned into a Rill panic. The shims above use `get`, which does not panic.
- Whether the C convention carries over to the runtime's calls, as ADR 0034 also leaves open. The measurements show it does for scalars and pointers.
