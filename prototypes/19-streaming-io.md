# #32 streaming I/O: adapters and adversarial evidence

[ADR 0018](../docs/adr/0018-streaming-and-buffered-io.md) is the human-confirmed decision; the [companion source](19-streaming-io.rill) contains **15 intended-valid expectations**. All examples and traces are hand-checked, not compiler/runtime tests, protocol/security validation or a soundness proof. #28 formalizes checking; #30 still gates backends. Existing #41 changes are preserved.

## Positive source coverage

Application-owned MemorySource/MemorySink implement ordinary canonical families without traits/dyn. They expose short reads/writes, zero requests versus EOF, explicit Bytes accumulation/reuse, readExact/writeAll across short calls, early EOF, partial write errors preserving typed envelopes, exact-limit EOF, overflow lookahead and simultaneous overflow/error, buffered acceptance/decomposition, recoverable flush retry and consuming close. Generic obtain/send helpers state canonical evidence and display requirements before monomorphization without blanket noBorrow. Generic collect additionally states noBorrow(E) for readToEnd's local probe. MemoryError's unit enum supplies this evidence structurally, including for the buffered examples.

FaultSource/FaultSink deliberately fail once after an obtained/accepted prefix and then permit recovery. This is a property of these examples, not a universal transport-retry guarantee. The buffering checks use conservation/progress invariants rather than assuming mandatory eager read-ahead, mandatory lazy writing or an exact allocation capacity. Concrete adapter representations, allocator reuse and native integration are not implemented here.

## Public helper error declarations

The standard public declarations are in ADR 0018 and the catalogue. IoFailure<E> owns `{ transferred:u64; error:Err<E> }`. ReadExactError distinguishes UnexpectedEof from Read; ReadToEndError distinguishes Read from LimitExceeded. Read variants retain Err<IoFailure<E>> with cumulative progress. LimitExceeded retains exactly one extra byte, the destination-appended count and an optional original one-byte-probe failure envelope. Keeping the full Err<IoFailure<E>> preserves both the I/O wrapper and nested Err<E>; it is not heterogeneous cause erasure.

BufferedFlushError distinguishes draining Write from underlying Flush. BufferedCloseError distinguishes Flush, Close and Both, retaining typed envelopes independently. Standard display implementations require display of their parameter errors; primary borrowed failures remain possible. Normal noBorrow/sendable/shareable requirements apply only where independently needed, including cause erasure/transfer, not universally to all I/O errors.

## Scoped noBorrow evidence approved in Q36

| Operation | Required error property in addition to capability/display evidence |
| --- | --- |
| readToEnd | noBorrow(E) |
| BufferedReader read | noBorrow of underlying read E |
| BufferedWriter write / flush | noBorrow of underlying write WE |
| BufferedWriter consuming close | noBorrow(WE), noBorrow(FE) for underlying write/flush |

Local probe buffers cannot outlive returned errors, retained exclusive dependencies cannot block internal buffer mutation, and consuming close cannot return flush errors borrowing the destroyed inner resource. The user approved these conservative scoped requirements. They do not imply sendable/shareable, prohibit all primary borrowed errors, or add noBorrow to direct primitives, caller-buffer readExact/writeAll, buffered-reader close or the underlying consuming close error CE. Socket-like forwarding below does not introduce local buffers and therefore does not invent these properties. Selective dependency annotations that could permit more cases are not part of this design.

## Socket-like ordinary-family composition

This is a type-level composition example, **not an OS socket API or proof that real socket directions can split**. #34 owns actual directional handles. R and W below may simply be the memory adapters; any real endpoint must meet #34's ownership/native rules.

```rill
type SocketLike<R,W> = { receiver: R; sender: W };

implement<R,W,E> read(socket: &mut SocketLike<R,W>, output: &mut Bytes, maximum: u64)
  : Result<Option<u64>, IoFailure<E>> where read(R,E), display(E) {
  return read(&mut socket.receiver, output, maximum);
}
implement<R,W,E> write(socket: &mut SocketLike<R,W>, input: &Bytes)
  : Result<u64, IoFailure<E>> where write(W,E), display(E) {
  return write(&mut socket.sender, input);
}
implement<R,W,E> flush(socket: &mut SocketLike<R,W>): Result<void,E>
  where flush(W,E), display(E) {
  return flush(&mut socket.sender);
}
```

Distinct input patterns determine each family's output E. R/W type parameters do not grant unlisted capabilities. Each operation still takes one exclusive borrow of the whole SocketLike value; owning fields does not authorize callers to create overlapping whole-value borrows. Close with two underlying failures would need explicit composition, not sequential `?` that skips required closure of the second field. Automatic disposal of remaining owned fields cannot be suppressed by a wrapper.

## TLS-like composition seam, not a TLS implementation

The following signatures are **schematic boundary contracts**. `TlsLike<S>` owns S and private protocol state; its read/write implementation bodies require a real engine supplied by #38. Engine types/native calls, cryptography, negotiation and shutdown are intentionally not supplied here.

```rill
// Illustrative possible error composition, not #38's finalized error taxonomy.
enum TlsLikeError<RE,WE> {
  Receive(Err<IoFailure<RE>>),
  Send(Err<IoFailure<WE>>),
  Protocol { message: string },
}

// Signature-only implementation sketches, not executable bodies or new declarations:
// implement<S,RE,WE> read(stream:&mut TlsLike<S>, out:&mut Bytes, maximum:u64)
//   :Result<Option<u64>,IoFailure<TlsLikeError<RE,WE>>>
//   where read(S,RE), write(S,WE), display(RE), display(WE) { engine body }
// implement<S,RE,WE> write(stream:&mut TlsLike<S>, input:&Bytes)
//   :Result<u64,IoFailure<TlsLikeError<RE,WE>>>
//   where read(S,RE), write(S,WE), display(RE), display(WE) { engine body }
```

An engine may need both transport directions for a logical read or write. It must translate progress to **plaintext appended/accepted at this layer**, not forward ciphertext byte counts. Pending ciphertext/records remain engine-owned and bounded by its explicit #38 policy, with backpressure; no buffer or native borrower may be abandoned on suspension/cancellation. A TLS record boundary is not an application read boundary. EOF versus truncation, handshake/record errors, graceful close notifications, buffer bounds and reuse after interruption are #38 decisions, not invented by this sketch. No fake encryption or security validation is claimed.

## Independent partial-effect and EOF traces

| Case | Hand-checked expected behavior |
| --- | --- |
| Short positive read | Request 8, append 2, return Some(2); not EOF and not an exact read. |
| Positive request EOF | Append 0, return None; existing destination unchanged. No temporary unavailability masquerades as EOF. |
| Empty primitive read | Request 0 returns Some(0), does not advance source/test EOF, even if already ended. |
| Empty write/exact helpers | Empty write returns 0; empty writeAll/readExact do not call an underlying operation. |
| readExact early EOF | Request 5, successful prefix 2 then EOF: retain 2, UnexpectedEof.transferred=2. |
| readExact partial error | Request 5, success 2 then append 1/error: destination grows 3; nested IoFailure.transferred=3, original E envelope intact. |
| writeAll partial error | Input 5, accept 2 then accept 1/error: report 3, do not resend first 3; no flush implied. |
| Failure after full acceptance | An error can report the full requested count; helpers must not convert it into success. |
| readToEnd exact limit | Budget 2, append 2, one-byte probe EOF: success count 2, existing prefix excluded. |
| readToEnd excess | Budget 2, append 2, probe gets byte 3: destination growth 2; LimitExceeded contains byte 3; source advanced 3. |
| Probe gets byte and fails | Preserve limit count, extra byte and complete probe failure envelope with probe-relative progress 1. |
| Probe fails without byte | Read error with cumulative appended count; no extra byte fabricated. |
| Zero accumulation budget | Still probe one byte: empty source succeeds with 0; nonempty source yields retained overflow; do not confuse with zero primitive request. |
| Bounded-reader refill failure | Refill gets 3 and fails, caller limit 1: append/report 1/error immediately; retain other 2 in order; no deferred error replay. |
| Buffered write fails draining old output | Old pending=3, current input=2; underlying accepts 1 then errors. Current write reports 0 accepted, old pending=2; caller must not resend old 3. |
| Buffered flush partial failure | Pending [a,b,c], underlying accepts a then errors. Keep [b,c]; retry writes b,c only. |
| Underlying flush error | Drain all pending bytes, then underlying flush fails: local pending empty; retry never resends drained data. |
| Consuming close two failures | Flush fails A, underlying close fails B: BufferedCloseError.Both preserves both envelopes; cleanup finishes; no handle survives. |
| Reader decomposition | Source advancement equals delivered bytes plus unread buffer (relative to initial state); return unread bytes, never just source. |
| Writer decomposition | Return pending bytes without I/O; sending new data before that suffix changes order and is caller-visible misuse. |

For cumulative rebasing use metadata-preserving mapErr on IoFailure payloads, not a new underlying E or an implicit error conversion. ReadToEnd's separate overflow buffer is a documented one-byte exception to the destination accumulation bound, not a hidden unbounded queue.

## Independent rejection / contract-violation cases

Static rejection unless explicitly designated fatal runtime adapter/programmer bug:

1. A generic body invokes read/write/flush/close without the corresponding canonical evidence; a same-spelled unrelated family does not satisfy it.
2. Missing display(E) when constructing/formatting generic errors; no reflected fallback.
3. Two ordinary read bodies with identical input patterns but different E/constraints are duplicates during collection; #41's conversion-key exception does not apply.
4. A foreign module imports a standard handle type and adds an otherwise orphan-forbidden implementation; import aliases confer no ownership.
5. Two simultaneous &mut operations on the same stream, including concurrent read/write through one value, reject. Real separated direction handles belong to #34.
6. Clear, truncate, reserve, append or read into Bytes while a conflicting read-only view is still used reject; owner retention does not authorize mutation.
7. Overlapping source/destination backing access or assuming different arbitrary indices prove disjointness rejects. Read-only slice is not writable.
8. Reading into &Bytes or &mut &Bytes as if it were writable backing rejects. No mutable string-byte or uninitialized-capacity escape.
9. Return a borrowed success/error into destroyed local buffer storage; moving/wrapping does not erase dependency.
10. Treat a primary borrowed error as automatically noBorrow/sendable/shareable or retain it as a heterogeneous cause without the normal properties.
11. Use a stream after consuming close or intoParts, including the error path; copy an owning handle to evade consumption.
12. Expect bufferedReader/writer to fabricate missing wrapped capabilities or satisfy a public interface with hidden family requirements.
13. Treat IoFailure<E> as another expected E via implicit `?`; use explicit adaptation. Do not discard original envelopes while merely adjusting counts.
14. Capacity zero is a fatal programmer error. Impossible reservations/addressability or allocation failures follow fatal policy, never wrap u64 into native sizes.
15. Successful zero progress on a nonempty write/positive read request is a fatal adapter-contract violation detected by helpers, not an infinite retry loop.
16. Overlarge counts, EOF with appended data or count/length mismatch are fatal adapter-contract violations. Validate before arithmetic/slicing; do not promise checks of every byte.
17. Exceed readToEnd's destination budget, drop its consumed lookahead, or report success while suppressing a simultaneous probe failure violates the contract.
18. Interpret a short read as EOF/message/UTF-8 boundary, flush as durable storage/peer acknowledgement, or scope exit as successful delivery violates the contract.
19. Pretend all native I/O errors leave a reusable transport or automatically retry a whole failed buffer. Layer-relative acceptance is not remote rollback or replay safety.
20. Expect the older whole-file/stdio APIs to expose new reliable progress counts without a signature change; their baseline partial-effect warnings remain.
21. A generic readToEnd wrapper stating only read(R,E), display(E) rejects: it also needs noBorrow(E), even if all current calls happen to use a borrow-free error.
22. A BufferedReader read or BufferedWriter write/flush cannot satisfy its scoped capability when the underlying read/write error lacks noBorrow. Display alone does not permit returning or mutating locally retained buffer storage.
23. BufferedWriter close cannot preserve a flush error borrowing the consumed inner resource; noBorrow(FE) is required in addition to noBorrow(WE). A nonconsuming flush may still return checked borrowed FE while the writer remains alive; callers must release that failure before conflicting access. No extra noBorrow(CE) is silently added.

## Cancellation, backpressure and exactly-once cleanup traces

These traces specify obligations, not a tested native runtime. Track one owner per pending operation/resource state; use bounded-state/worklist cleanup rather than recursive chunk traversal.

| Event sequence | Required result |
| --- | --- |
| Read registered; stop requested before native completion | Request abort if supported; retain exclusive destination/stream and native state. Only completion or confirmed abortion permits release. No recoverable IoFailure is delivered to stopped code. |
| Native write commits prefix; cancellation wins delivery | External prefix remains committed; retain input/native references until access ends. No rollback, resend or synthetic success. |
| Stop requested during disposal | Defer delivery; cleanup can suspend/schedule and must finish release obligations once. No force-kill or hard return deadline. |
| Close flushing cancelled | Stop graceful delivery; finish safe cleanup/actual native completion; pending output may be discarded. Do not resume application code with a fabricated close error. |
| Flush fails; close returns recoverable error | Move wrapped resource into close once, then clear its ownership slot; wrapper field cleanup cannot close it again. Any still-owned buffer is disposed once. |
| Wrapper dropped without explicit close | Discard pending plaintext/output; dispose remaining owned resource once. Do not implicitly flush, acknowledge a protocol body, send a TLS close notification or promise remote rollback. |
| Native completion races abort confirmation | Synchronize one terminal registration transition; late completion cannot double-release buffers/handles/callback state. Abort request alone is not terminal. |
| Buffered writer full while downstream stalls | Suspend/drain with bounded pending bytes; no detached work or unlimited input queue. |
| Timer fires while losing work cannot abort | Deadline race cannot return until required cleanup/retention ends; #33 defines semantics, not a hard deadline here. |
| Wasm memory grows during native use | Keeping owner alive is insufficient evidence of pointer stability; #30 must validate actual pointer/runtime mechanism. |

Concrete state accounting for a wrapper owning handle H: before close, H belongs to the wrapper; while delegated close runs, H belongs to that consuming call; after either recoverable outcome H is no longer an initialized wrapper field. Remaining-field disposal may release buffers but cannot release H a second time. On cancellation, the currently owning frame/native registration retains H until its cleanup completes. Fatal panic remains outside the normal cleanup guarantee.

## Follow-through

#28 must validate the explicitly approved Q36 scoped noBorrow requirements and all other declared generic requirements, ordinary result inference, borrowed-error/field dependencies, bounded helpers, adapter state/progress rebasing, both-error preservation and exactly-once ownership transitions. #33 owns deadlines, #34 sockets/directional ownership, #35 file handles/seek/durability, #38 real TLS and interruption states, #39 HTTP body abandonment/reuse and #40 driver/transaction lifetimes. Unworkable checking or native contracts return to map #1, not an unrecorded change of the human-confirmed API.
