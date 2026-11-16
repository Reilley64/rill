# #30: stronger native fixed-layout tail-ABI mechanism matrix

**Executed candidate mechanisms, not a production ABI, checker, compiler,
architecture adoption or production support closure. #30's standalone mechanism
stage is complete; #28's production/formal gate remains OPEN.** See the
[combined disposition](../../../docs/research/0030-runtime-validation.md).
Source rules, public callable APIs and intended targets are unchanged.
This extends the single-packet evidence in [`../native-capacity/`](../native-capacity/README.md),
without replacing its capacity experiment or AArch64 incoming-`byval` negative
control. Parent-owned lifecycle and portable probes are independent.

## Reproduce and evidence

```sh
python3 experiments/backend-30/native-abi/run.py \
  --out /tmp/rill-30-native-abi-final
```

Requires Python with assertions enabled, Homebrew LLVM **23.1.5** at
`/opt/homebrew/opt/llvm/bin`, the macOS SDK, installed Rosetta, running Docker,
and the previously prepared, read-only C++/UAPI sysroots:

- `/tmp/rill-30-lifecycle-tools/headers-arm`
- `/tmp/rill-30-lifecycle-tools/headers-x86`

These are `/usr/include` and `/usr/lib/linux` from the corresponding immutable
**official Rust image digests** pinned in `run.py`, including libstdc++14 headers
and architecture-specific headers. The runner reads them; it does not edit the
parent's files or download new dependencies. A fresh machine must first stage
those headers from those exact images (the existing lifecycle runner documents
the `docker create` / `docker cp` preparation). Containers run with networking
disabled. No Rust compiler builds this mechanism.

All fixture bodies, including `main`, SP sampling, layout/ownership validation,
copy/zero helpers, final result handling and disposal calls, are compiled by
**Clang23.1.5**, including Linux bodies cross-compiled on macOS. `opt` and `llc`
are also **23.1.5**. Linux uses `--sysroot`, `-nostdinc++`, and explicit
libstdc++14 / GNU architecture include paths. Image `c++` **14.2.0-19** is invoked
**only with LLVM-generated object files to link**, not to compile any mechanism
or wrapper source. GNU ld is **2.44**, glibc **2.41-12+deb13u4**, libstdc++
**14.2.0-19**. Image Rust **1.99.1** / its LLVM **23.1.1** are inventoried, not used.
Mac links use Apple ld **27037.1**; its inventoried Apple LLVM21 LTO component is
not used to generate these non-LTO object files.

Recorded host: macOS **27.0.1 (26A434)**, Darwin27 / Apple ARM64 (LLVM identifies
host CPU `apple-m3`); SDK **27.0 (26A425)**. Docker **29.8.2** runs Linux kernel
**7.2.12-orbstack-00394-gbecdd6130068**. ARM Linux executes in that ARM64 VM.
Mac x86 executes through Rosetta (package version **1.0.0.0.1790234402**);
Linux x86 executes through VM translation/emulation. **No physical x86 hardware
was tested.** Exact triples, image digests, version output and commands are in
checked-in JSON.

- [`evidence/commands.json`](evidence/commands.json): **123 successful commands**,
  exact argv, linker/container scripts, cwd and exit statuses from the final run.
- [`evidence/results.json`](evidence/results.json): **24 executed chain outcomes**,
  exact versions, source/tool/artifact SHA256s, `.su` frames, per-transfer acyclic
  call graphs and generated tail branch summaries.
- Raw IR (source and optimized), assembly, object/executable disassemblies,
  dependency/header hash manifests, binaries and stdout/stderr are **only under
  `/tmp/rill-30-native-abi-final`** (resolved as `/private/tmp/...` on this host).
  The JSON records their hashes; it does not check in raw IR/logs/binaries. The
  full used-header manifest is hashed in results. The runner writes evidence to
  `--out`, never to the checkout; checked-in JSON is copied from that final run.

## Layout, value and ownership matrix

Every source-like body has the **same** `Result` record. Its private lowering uses
uniform result storage, not mismatched C aggregate return conventions. Three
**different fixed record layouts** are actually constructed and transitioned:

| Record | Size | Alignment | `Value` offset | Distinct shape |
| --- | ---: | ---: | ---: | --- |
| A | 176 | 16 | 0 | trailing 7 bytes |
| B | 224 | 32 | 48 | leading 39 bytes |
| C | 320 | 64 | 48 | leading five u64s; trailing 67 bytes |

Sizes match on all four tested targets. `Slot` is a 320-byte, 64-aligned union.
A `Packet` adds its active-member tag and padding, making **384 bytes**. The
ordinary root owns **two packets = 768 bytes**: current arguments and destination
staging. No third transition packet or depth-dependent history exists. `Result`
is **176 bytes**. The root's packets + result + borrowed bytes + context total
**1159 object bytes**; this is NOT a full frame/capacity bound. Actual Clang `.su`
root frame sizes are **1272-1664 bytes**, including more compiler storage; return
addresses, realignment/red zones and callees are separate concerns.

Every chain executes A → B → C → A repeatedly, with exact observed edge counts
**333334 / 333333 / 333333**. Each step checks source and destination shape,
alignment, transported fields and real resource contents, including a destination
check **before** advancing the scalar/tag state:

- signed i8/i16/i32/i64/i128 and unsigned i128, including nonzero high i128 bits;
- bool, float and double; alternating exact **negative-zero and quiet-NaN payload
  bits**, copied and checked via `__builtin_bit_cast`, without FP arithmetic;
- an explicitly tagged union alternating u128, double-bit-pattern and 23-byte
  array variants;
- an immutable borrowed pointer into **31 bytes owned by the earlier ordinary
  root**, checked for identity and all bytes at every step;
- a real **73-byte malloc-owned buffer**, including pointer/length, ownership
  flag, exact allocation identity and every byte;
- a separately malloc-owned **56-byte callable-environment-like capture**, with
  its own ownership flag, salt, 41 payload bytes and a typed invocation pointer.
  Identity/content/callable fields are checked every step; the final callee
  actually invokes it once and checks its result.

Root makes **one explicit independent 73-byte deep copy before the chain**.
It is a distinct malloc allocation, remains live throughout all transfers and
is checked unchanged afterward. Root does not dispose the moved original during
the chain. The three user payload allocations total **202 requested bytes**,
not allocator metadata; there is no per-tail allocation.

`move_value` does the trivial value copy, then nulls the old buffer/capture/
callable pointers, clears length and ownership flags, and calls a separate
noinline poison checker before retiring the source record. The destination
becomes the sole logical owner. Pointer/scalar copies transiently used while
moving are not a second live owner or retained activation. `retire` explicitly
ends the active record lifetime and marks its packet dead. Next staging must
already be dead. Final output is moved into the earlier-root-owned result,
and both packets are dead on return.

The final callee invokes and **actually frees the capture exactly once** after
the final sample/check. Root then verifies and consumes the result buffer
(observed byte sum **6439**) and **actually frees it exactly once**, followed by
one real free of the independent deep copy. Counter checks supplement these
real `malloc`/`free` calls; they do not substitute for them. All members are
trivially destructible: disposal is explicit manual fixture lowering, not an
implemented Rill drop/ownership checker. Callable fields are a **private ABI
mechanism**, not a new public `dyn`, reflection, capture-mode or callable API.

### C++ lifetime validity

This uses normal C++20 placement construction, **not** a Clang aliasing extension
or `-fno-strict-aliasing`. Packet unions initially have no active record member.
Placement new starts the selected A/B/C member, and only that member is read.
Old ownership is cleared/checked while the old object is still alive, then its
trivial destructor ends its lifetime. All pointers to values are reacquired
from the currently active member; no pointer to an eliminated activation or
stale union member is dereferenced. Tagged scalar/array variants are likewise
explicitly placement-constructed. Trivial union copy assignment begins the
corresponding destination variant's lifetime. Nonallocating array placement new
uses the C++20 no-array-overhead rule. No constructor/destructor allocates.
The heap capture is placement-constructed in malloc storage and explicitly
destroyed before free. This is a manual fixture, not a general soundness proof.

## Native transfer mechanism and execution

The root ordinary-calls the dispatcher; root's objects remain alive throughout.
All tail bodies share this legal private signature:

```llvm
define tailcc void @f(i64 %n, ptr %cur, ptr %staging, ptr %out, ptr %ctx)
; ordinary helpers inspect/move/construct, and fully return first
musttail call tailcc void @g(i64 %nn, ptr %staging, ptr %cur, ptr %out, ptr %ctx)
ret void
```

No tail IR body has `alloca`, `byval`, changing prototype or caller-local pointer
passed onward. Helper locals may exist **only until the helper returns**, before
the LLVM transfer. The ordinary root is intentional earlier ownership, not a
reclassification of an eligible tail or survival of eliminated callers.

- **Self**: one native tail function cycles through all layouts; O0 tail-branches
  to itself, O2 transforms it to a local loop.
- **Mutual**: three different native tail functions A/B/C; generated direct tail
  branches link them without ordinary recursive calls.
- **Indirect**: three functions, with the next code pointer obtained through a
  volatile load from a private three-entry target table indexed by the actual
  destination layout. Generated AArch64 **`br x5`** and x86 **`jmpq *%rax`** remain
  indirect at **both** O0/O2. This is not merely a source pointer optimized to a
  known direct call, or a dispatcher interpreting all tail functions in a loop.

On **each** of mac ARM64, mac x86-64/Rosetta, Linux ARM64 VM and Linux x86-64 VM,
**both O0 and O2**, every self/mutual/indirect chain executes **1,000,000 actual
transfers**, **1,000,001 actual SP samples**, and **1,000,001 ownership moves**
(the last move is into the result). Total: **24,000,000 transfers / 24,000,024
samples**. Every outcome passes. Every chain's sampled SP span is **0 bytes**;
sampled depth below the root's pre-dispatch SP is **96-192 bytes**. Samples are
at the observer, not every instruction: they do NOT measure peak helper stack,
full root storage, prologue writes, free, allocator or libc depth.

The runner verifies every emitted source/optimized IR module with LLVM `opt`.
It checks generated direct/indirect/loop branches and absence of ordinary
recursive calls, and saves object and linked executable disassembly. It audits
the C++ per-transfer graph rooted at `abi_observe`/`abi_step`: no recursion,
unknown external or indirect helper, allocation, or dynamic `.su` frame is
accepted. Individual per-transfer helper frames are static and at most 512
bytes. This finite acyclic helper graph plus frame-replacing native transfers
establishes bounded fixture stack/storage independent of the transfer count;
the million-step execution corroborates, not proves arbitrary program behavior.

`-fno-optimize-sibling-calls` on C++ prevents hidden helper tail edges; the audit
checks the corresponding IR attributes. Only manually emitted native tail IR
uses required `musttail`. Copy/zero intrinsics are limited to this fixture's
Clang23-built `memcpy`, `memset`, `bzero`, `__bzero` helpers. Darwin codegen can
introduce either zero-helper spelling even with `-fno-builtin`, so the machine
call audit includes both rather than overlooking a libc call.

## Structural route and exact limits

A finite statically known tail component could reserve the maximum size/alignment
of its different fixed input layouts plus explicit staging/result storage in an
earlier ordinary owner, then use uniform typed-private pointer-based tail
lowering. Different source argument layouts need not be encoded with incompatible
LLVM `byval`, scalar register splits or C `sret` signatures. This matrix executes
three differing layouts/alignments, field transport, independent-copy lifetime,
borrowed earlier storage and two distinct owned resources; it removes the
**single tested packet layout** gap, not the need for a compiler/ABI design.

**No native-capacity validation is claimed here.** These are deliberately small
records/alignment, executed on the process's existing OS stack, not a managed
1 MiB NativeStack. There is no admission thunk in this fixture. The fixed object
count is not permission to write before a future production admission check.
Large/high-alignment layouts would require checked max-layout/staging/result
accounting **before the ordinary builder's prologue**, as well as helper/runtime
headroom; the separate capacity suite remains the relevant partial experiment.
Its AArch64 incoming-`byval` rejection evidence is preserved and not reclassified
as success by this alternative.

Real library `free`, `malloc`, `printf`, allocation-failure `abort`, and the final
capture invocation/disposal are **outside** this per-transfer call-graph audit
and outside the capacity suite's finite admitted subtree. We do **not** claim
all runtime/native/library calls are capacity-checked, or that this works on an
almost-exhausted stack. No fatal path, adjacent-canary protection or overflow
admission is tested here.

Also unimplemented/untested: arbitrary or dynamic records, alignments beyond
64, overlapping argument/result aliases, generic polymorphic indirect ABI,
escaping packets, nontrivial source destructors/partial-init failure, production
eligibility/borrow/move/drop elaboration, public/native interop, suspension,
M:N migration, cancellation, joins, emergency reporting, physical x86, and
sanitizer coverage. Pending caller cleanup/guards/joins or caller-local borrows
remain ordinary source cases; nothing here licenses early disposal or fallback
from an eligible tail to ordinary recursion. No hidden heap activation chain,
source weakening, target approval or #30/#28 support closure is claimed.

## Primary rules

- [Current #30 contract](https://github.com/Reilley64/rill/issues/30) and
  [ADR 0014](../../../docs/adr/0014-bounded-stacks-proper-tail-calls-and-backend-gates.md).
- LLVM **llvmorg-23.1.5**, commit
  `392dfff97d29b59d77e5cac6d4b6ef2585fe7117`:
  [call/musttail and caller-storage rules](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/LangRef.md#L13520-L13644),
  [tailcc](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/docs/LangRef.md#L498-L515),
  [AArch64 incoming-byval exclusion](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/llvm/lib/Target/AArch64/AArch64ISelLowering.cpp#L9800-L9873).
- C++20 draft **N4861**, source commit
  `6aea7f6be0895b9dd361c6562bdee2f3809e4fa0`:
  [unions and trivial copy assignment](https://github.com/cplusplus/draft/blob/6aea7f6be0895b9dd361c6562bdee2f3809e4fa0/source/classes.tex),
  [object lifetime](https://github.com/cplusplus/draft/blob/6aea7f6be0895b9dd361c6562bdee2f3809e4fa0/source/basic.tex),
  [placement new / array overhead](https://github.com/cplusplus/draft/blob/6aea7f6be0895b9dd361c6562bdee2f3809e4fa0/source/expressions.tex).
- [Clang23 bit-cast extension](https://github.com/llvm/llvm-project/blob/392dfff97d29b59d77e5cac6d4b6ef2585fe7117/clang/docs/LanguageExtensions.rst).
