# Bounded streaming and buffered I/O with explicit progress

[Issue #32](https://github.com/Reilley64/rill/issues/32) extends #23's stdio/whole-file baseline with ordinary byte-stream capabilities, caller-owned reusable Bytes and bounded owning adapters. The user confirmed Q1-Q35 and the consolidated contract, then explicitly approved Q36's scoped borrow-free-error requirements after artifact validation exposed local-buffer escape hazards. This is API/behavior design input for #28, not implemented I/O, an accepted RFC, protocol/security validation or a checker/backend proof. Existing #41 working-tree decisions remain applicable.

## Public declarations and generic evidence

All names below are exported from `std::io`, except Bytes operations in `std::bytes`. Ordinary family declarations use `function`; bodies use language-wide `implement` per ADR 0017. Each family has one declaration. Implementations specialize input patterns under ordinary identity, duplicate, visibility, orphan and specificity rules; no traits, methods, public dyn, runtime registry or return-only selection are introduced.

```rill
function read<R, E>(source: &mut R, destination: &mut Bytes, maxBytes: u64)
  : Result<Option<u64>, IoFailure<E>>;
function write<W, E>(destination: &mut W, source: &Bytes)
  : Result<u64, IoFailure<E>>;
function flush<S, E>(stream: &mut S): Result<void, E>;
function close<S, E>(stream: S): Result<void, E>;
```

Read/write/flush/close are separate capabilities. Each independently determines its error type from the selected input implementation. Generic evidence is `read(R,E)`, `write(W,E)`, `flush(S,E)` or `close(S,E)` with explicit additional requirements such as `display(E)`. Bodies check against symbolic signature/access/dependency evidence before monomorphization, not source-inspected concrete bodies. Do not select an ordinary implementation by desired E or constraint priority. Adapters can expose differing read/write/flush/close errors.

Calls wait by suspending the current routine when progress is not available. Public primitive operations do not report WouldBlock or expose polling/async types. This is not permission to block a scheduler thread; #30 must validate backend machinery. There is no implicit buffering or broad import.

## Read, write and failure semantics

Read appends at most maxBytes to the existing destination and preserves its old contents. `Some(n)` with positive maxBytes has `1 <= n <= maxBytes`; `None` is EOF and appends nothing. A short positive read is neither EOF nor a message boundary. A zero request returns Some(0) without consuming input or testing EOF. The request is an upper bound, not a promise to allocate that amount; implementation-sized reads must not narrow/wrap public u64 counts.

Write accepts a prefix of its supplied logical Bytes extent. Nonempty input succeeds with a positive count no greater than its length; empty input succeeds immediately with zero. Unavailability suspends; inability to proceed fails rather than reporting successful zero progress. Accepted bytes belong to the receiving layer, not necessarily the disk, peer or application. Failed writes must not be retried from the beginning without respecting their progress.

```rill
type IoFailure<E> = { transferred: u64; error: Err<E> };
```

Failures preserve exact progress relative to this operation. For read, this is the number of bytes appended. For write, it is the number accepted from this call's input. Read's visible destination-length increase equals transferred, even on failure; no garbage/unreported initialized bytes or rollback is allowed. The original typed Err<E> retains origin/cause data and actual dependencies. Standard display requires display(E); storing this typed envelope does not erase it into Cause or require blanket noBorrow/sendable/shareable. Normal cause erasure, exact-E propagation, explicit error adaptation and fatal panic rules remain.

Helpers must validate counts before arithmetic/slicing: impossible bounds, successful no-progress reports, EOF with appended data, or read progress inconsistent with length change are fatal adapter-contract bugs, not invented values of arbitrary E. Do not promise expensive byte-for-byte validation of preserved prefixes. No memory-safety guarantee may depend on an unchecked adapter count. Implementations remain responsible for their semantic contract and safe native registration.

## Exact/all/bounded helpers

These are generic helpers, not additional capabilities. Their public shapes are:

```rill
readExact<R,E>(&mut R, &mut Bytes, u64): Result<void, ReadExactError<E>>
  where read(R,E), display(E)
writeAll<W,E>(&mut W, &Bytes): Result<void, IoFailure<E>>
  where write(W,E), display(E)
readToEnd<R,E>(&mut R, &mut Bytes, u64): Result<u64, ReadToEndError<E>>
  where read(R,E), display(E), noBorrow(E)
```

All counts are cumulative over the helper invocation, excluding destination contents that predate it. Empty readExact/writeAll complete without touching the stream. readExact appends exactly its request on success; early EOF and failure leave their obtained prefix. writeAll advances only over accepted input, never implicitly flushes/closes, and creates no message boundary. Implement bounded-stack loops, not recursion per chunk.

```rill
enum ReadExactError<E> {
  UnexpectedEof { transferred: u64 },
  Read(Err<IoFailure<E>>),
}
enum ReadToEndError<E> {
  Read(Err<IoFailure<E>>),
  LimitExceeded {
    transferred: u64,
    extra: Bytes,
    probeFailure: Option<Err<IoFailure<E>>>,
  },
}
```

The nested IoFailure in Read variants reports cumulative appended progress. When rebasing progress, preserve the existing outer envelope's metadata as well as its nested Err<E>, e.g. through normal mapErr rather than reconstructing E. New helper/context error envelopes capture their own normal construction origins. These error enums and the buffering enums below have standard display implementations requiring display for their type parameters; no universal reflected formatter or implicit error conversion.

readToEnd requires a maximum additional-byte budget. At the limit, probe with a distinct one-byte buffer: EOF succeeds; obtaining a byte yields LimitExceeded, with extra exactly one byte and never appended beyond the destination bound. If that byte and a failure arrive together, retain both extra and the original probe failure envelope. Its count is relative to the one-byte probe; the top-level transferred is the destination's cumulative appended count. A probe failure without a byte is Read with cumulative progress. Zero budget still probes; it is not the zero-request primitive operation. No unbounded variant or silently discarded overflow input. Callers enforcing a total-size budget subtract existing destination length. This bounds accumulation, not arbitrary allocations/effects in third-party sources.

## Bytes reuse without writable views

Add `clear(&mut Bytes):void`, `truncate(&mut Bytes,u64):void`, and `reserve(&mut Bytes,u64):void` in std::bytes. Clear empties while retaining reusable allocation. Truncate shortens to the requested length or does nothing if already shorter. Reserve ensures space for at least that many additional bytes without changing logical length; exact capacity/allocator footprint is not public. Checked length/addressability/reservation failure follows existing fatal allocation policy, never wraps. These operations cannot bypass conflicting views.

Keep Bytes as the sole contiguous owned byte container. &Bytes remains a read-only logical whole/subrange; &mut Bytes accesses a whole owner. No writable slice, uninitialized-capacity exposure, implicit List contiguity, mutable string-byte escape or overwrite-region read API is added.

## Owning bounded adapters

`bufferedReader<R>(R,u64):BufferedReader<R>` and `bufferedWriter<W>(W,u64):BufferedWriter<W>` own the wrapped value. Capacity is nonzero; zero is a programmer error/fatal panic, unsupported allocation/reservation follows fatal policy. The bound is on logical buffered byte length, not exact allocator footprint. No borrowed constructors, hidden background goroutines or unlimited queues. Capabilities become available when wrapped evidence suffices; construction does not fabricate them.

BufferedReader's read capability requires read(R,E), display(E), noBorrow(E), so a refill failure cannot retain the adapter buffer while it is accessed/mutated. BufferedReader reads from its unread buffer first and may refill/read ahead up to its capacity, returning no more than the caller's limit. A failed refill with a prefix appends as much as fits the current request, returns failure with caller-relative progress immediately, and retains any remaining obtained bytes. No deferred-error queue or silent byte loss; subsequent reads can retrieve retained bytes. Continued source usability is implementation-specific, not guaranteed by the generic API.

BufferedWriter's write capability requires write(W,WE), display(WE), noBorrow(WE), including when draining its internal pending storage. BufferedWriter reports acceptance of the current input, whether buffered or passed onward. Its pending bytes never exceed capacity. When full it drains with underlying write operations before accepting more; it does not implicitly invoke the underlying flush merely to create space. Failure draining old data can report zero current-input progress while older output has committed. Remove confirmed accepted prefixes only; retain the unsent suffix after recoverable failure. Retry flush never replays removed bytes. Underlying protocol errors can still make future operations unusable; retained bytes do not imply retry safety for every transport.

One ordinary `intoParts<B,S>(buffer:B):[S,Bytes]` family has distinct input-pattern implementations for the reader and writer adapters. Consuming decomposition does no I/O and returns the underlying value plus unread/pending bytes in order. It cannot silently discard them. The caller must deliver returned pending output before later output if preserving order. No peek/consume API is added.

## Flush, consuming close and disposal

Flush drains this layer's pending output, then invokes underlying flush. It does not mean filesystem crash durability, peer receipt or application acknowledgement. A memory sink can implement it as a no-op. BufferedWriter flush requires write(W,WE), flush(W,FE), display(WE), display(FE), noBorrow(WE), exposes BufferedFlushError<WE,FE>, and distinguishes draining from flushing. FE may still carry checked external/caller-resource dependencies while the writer remains alive:

```rill
enum BufferedFlushError<WE, FE> {
  Write(Err<IoFailure<WE>>),
  Flush(Err<FE>),
}
enum BufferedCloseError<WE, FE, CE> {
  Flush(Err<BufferedFlushError<WE, FE>>),
  Close(Err<CE>),
  Both { flushing: Err<BufferedFlushError<WE, FE>>, closing: Err<CE> },
}
```

A drain failure retains pending bytes and skips underlying flush. An underlying-flush failure occurs after the local buffer emptied, so retry must not replay the old buffer. Preserve every original typed envelope. Full flush/close capabilities add independent requirements to the writer's write/noBorrow(WE) evidence; they do not fabricate them.

Close consumes ownership. On success or recoverable failure, required local cleanup has completed and the caller has no retryable handle. BufferedWriter close requires write(W,WE), flush(W,FE), close(W,CE), display(WE), display(FE), display(CE), noBorrow(WE), noBorrow(FE). The FE restriction prevents returning an error borrowing the consumed inner resource; CE remains governed by normal consuming-call escape checks, not an extra blanket noBorrow(CE). BufferedWriter close attempts flush, then safe underlying close even if flush failed; report flush-only, close-only or both errors, not last-error-wins. Pending output may be discarded as the writer is consumed. Call flush first while retaining ownership if recovery is needed. BufferedReader close discards unread input and delegates underlying close; do not drain to EOF. Protocol-specific shutdown remains #34/#38/#39 work, not an invented generic half-close.

Disposal does not flush or perform graceful protocol shutdown: discard pending output and release local/native resources safely, with no recoverable Result channel. Explicit close and automatic field cleanup must move/dispose each owned resource exactly once, never close an already-consumed field twice. Dispose can suspend, with cancellation deferred during required cleanup; no hard cleanup deadline, global program-exit cleanup promise or fatal-panic cleanup guarantee.

## Suspension, cancellation and integration

Read/write exclusively borrow the stream for the operation. Read also exclusively borrows destination; write read-only borrows the supplied extent. Retain those permissions, buffers, callbacks and native resource state across suspension until completion or confirmed abortion. Cancellation/abort requests, handle drop or blocking-pool transfer are not release confirmations. Do not fabricate noBorrow/sendable/shareable or extend temporary lifetimes. Concurrent directions require explicitly separate handles designed by #34, not two simultaneous &mut operations on one value.

Cooperative cancellation remains outside Result. It may leave consumed input/committed output and does not return progress to stopped code. Cancellation during close's normally cancellable flushing phase still triggers required safe cleanup, not a delivery guarantee. Registered native work must cease access before cleanup frees buffers/handles; cleanup can wait indefinitely. #33 owns concrete deadline/context/race APIs: no timeout argument is added here and racing a timer is not a hard-return guarantee. #30 still gates native/Wasm suspension, pointer stability and backend validation.

#35/#34/#38 own actual file/socket/TLS handles, seeking, directional close, protocol handshakes and fatal connection states. #39/#40 must explicitly govern body/row abandonment, draining and connection reuse; disposal does not acknowledge a body or guarantee remote transaction rollback. Streaming byte boundaries do not align with UTF-8, lines, TLS records or messages. Text decoding is explicit above this layer.

## Scoped borrow-free errors confirmed after artifact validation (Q36)

Recording the contract exposed a conservative-borrow gap. readToEnd's local probe and adapters' internal buffers cannot be destroyed or mutated while an unknown error retains their exclusive dependency. Buffered consuming close also cannot return a flush error borrowing the resource it destroys. display(E) alone does not rule these out. The user explicitly approved these conservative requirements:

| Operation | Scoped requirement beyond capability/display evidence |
| --- | --- |
| readToEnd | Requires noBorrow(E). |
| BufferedReader read | Requires noBorrow for the underlying read error E. |
| BufferedWriter write / flush | Requires noBorrow for the underlying write error WE. |
| BufferedWriter consuming close | Requires noBorrow(WE) and noBorrow(FE). |

These requirements are deliberately stronger than the theoretically minimal dependency conditions. They use existing evidence rather than new selective lifetime/dependency annotations. No blanket restriction is added to primitive read/write/flush/close errors, readExact/writeAll over caller-owned buffers, or buffered-reader close delegated to consuming close(R,CE). No sendable/shareable requirement follows. A generic readToEnd caller must state noBorrow(E) even when some concrete instantiations would be borrow-free; MemoryError in prototype 19 satisfies it structurally. Applications needing borrowed errors can use direct operations/caller-buffer helpers or explicitly adapt errors to an owned form before these restricted adapters. #28 still owes formal checking/evidence transport and must not treat this approval or examples as a soundness proof.

## Later filesystem realization (#35)

[ADR 0019](0019-paths-files-and-filesystem-operations.md) now supplies concrete file access-mode types, explicit Path opens, seek/sync and filesystem/temp policies. It does not change these streaming/progress/cancellation or Q36 scoped error rules. Standard file IoError already satisfies the scoped noBorrow evidence. File flush is not sync, and buffered wrappers do not silently gain seek/sync forwarding.

## Baseline, artifacts and follow-through

Existing readStdin/writeStdout/writeStderr/flushStdout/flushStderr/readFile/writeFile signatures stay unchanged with #23's partial-effect warnings, not retroactive progress reporting. Shared standard-stream state remains trusted synchronized runtime state; no new global-handle ownership model is silently introduced. This extends #23, rather than reopening unrelated baseline decisions.

[Prototype 12](../../prototypes/12-streaming-io.rill) demonstrates application-owned memory adapters and generic evidence. Its [case inventory and socket/TLS composition sketches](../prototypes/12-streaming-io.md) cover positive/rejected/partial/EOF/limit/cancellation/cleanup cases. All are hand-checked. #28 must formalize ordinary result-parameter inference, wrapper capabilities, transitive dependencies, envelope/progress preservation, exactly-once cleanup and helper/adapter invariants. An unworkable mechanism returns to map #1; ticket closure is design approval only.

## Later single-owner progress capability (#45)

[ADR0027](0027-single-owner-progress-and-coalescing-wakes.md) and the [progress contract](../progress-io-contracts.md) add a separate owned admission/advance/wait family for general bidirectional coordinators. Ordinary canonical I/O above does not automatically implement it and keeps every arity/borrow/progress rule. Progress commands own bounded bytes rather than retaining caller scratch across steps; coalescing wakes carry no results. This is not a Future/Poll retrofit, cancellation yield or authorization for simultaneous `&mut` canonical calls.
