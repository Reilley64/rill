# An owned string is pointer, length and capacity; a view is pointer and length

**Status:** accepted, 2026-11-25 (owner decision). Drafted from Hello World grilling decision Q32, 2026-11-25; the owner decided Q10 that each ABI decision is recorded as an ADR. [ADR 0011](0011-explicit-numeric-widths-and-utf8-string-views.md) fixed the language's strings: one owned `string`, borrowed text views, UTF-8 throughout. No document fixed their run-time layout, and the first program that runs (`prototypes/00-hello.rill`) joins strings with `append`, so the backend needs one.

## Decision

- An owned `string` is three words: a pointer to its bytes, its byte length and its capacity (Q32).
- A `&string` view is two words: a pointer and a byte length (Q32).
- The bytes of a string literal live in read-only data. They are copied to the heap only when the literal becomes an owned value, for example `let mut greeting = "Hello ";` before `append` (Q32).
- A view of a literal, such as `&"world"`, points into read-only data and copies nothing.

The language-level meaning does not change: text is UTF-8, a view borrows from its owner, and an owned string frees its allocation when it is cleaned up.

## Consequences

- A view carries no capacity, so a view cannot grow or free anything, and a view and an owned string differ in size.
- A literal costs no allocation until it is owned. Cleanup of an owned string must free only what the allocator gave it, and a literal's read-only bytes are never freed because a literal is not owned until it is copied ([ADR 0032](0032-generated-code-allocates-through-three-runtime-functions.md) carries the allocation side).
- The runtime receives text from generated code as a pointer and a length. It documents how, with the rest of its symbols.
- Command-line arguments and the environment reach Rill as owned strings built from captured bytes, after the UTF-8 check that `std::env` performs in Rill ([ADR 0019](0019-owned-startup-environment-and-arguments.md)).

## Considered options

The ri-qlh record gives no rejected alternatives for Q32. These are the layouts its wording rules out; no source gives the owner's reasons for dropping them.

- **A NUL-terminated string.** Q32 stores the length.
- **A view that also carries capacity, or a single representation for both.** A view could then pass for an owner.
- **Copying every literal to the heap at its first use, or at program start.** Q32 limits the copy to the point where the literal becomes owned.
- **Small-string storage inside the three words.** It would change the layout of an owned value by length, and Q32 describes one layout.

## Settled by the backend

The first backend (ri-qlh.22.8, part A) fixed these. They are implementation details, not owner decisions.

- An owned string is the words pointer, length, capacity, in that order. A view is pointer, length. Each is a pointer-sized word on every target.
- An empty string holds the dangling address 1 and capacity 0. Generated code never allocates it, and releasing a string whose capacity is 0 does nothing. A view of the empty literal holds the same address.
- An owned string's capacity is the size of its allocation, so it is zero only for the empty string.
- A literal's bytes are one read-only, one-byte-aligned constant, shared by every literal with the same bytes.
- A string crosses a call as its separate words, three for an owned string and two for a view ([ADR 0034](0034-llvm-default-c-calling-convention-for-slice-1.md)).

The second backend increment (ri-qlh.22.12) fixed these. They are implementation details, not owner decisions.

- `append` grows capacity to the larger of twice the current capacity and the length the result needs, and does so only when the result does not fit. The new size comes from `rill_rt_grow` ([ADR 0032](0032-generated-code-allocates-through-three-runtime-functions.md)); an empty string has capacity 0, so its first growth allocates. A literal's read-only bytes are never passed to `rill_rt_grow` or `rill_rt_release`: a literal is copied to the heap when it becomes owned, so every owned string with a nonzero capacity is an allocation.
- `append` copies the bytes of the appended view after the existing bytes and checks nothing about UTF-8. The result is valid UTF-8 because both operands are valid strings and a concatenation of two valid UTF-8 byte sequences is valid UTF-8.
- An exclusive borrow of an owned string, `&mut string`, crosses a call as one word: the address of a cell holding the string's three words. Generated code gives an owned string that is borrowed exclusively a cell in memory and reads its words from the cell afterwards, so a call that grows the string changes the owner's pointer, length and capacity ([ADR 0034](0034-llvm-default-c-calling-convention-for-slice-1.md)).

## Open

Nothing is open.
