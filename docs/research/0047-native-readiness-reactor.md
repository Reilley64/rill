# #47 native readiness reactor selection and first TCP mechanism evidence

Status: implementation research and current-host component evidence, **not** a Rill ABI, Linux/macOS support matrix, TCP conformance result or #47 completion.

## Selected private mechanism

The implementation-private `runtime` package uses locked **mio1.2.5** with default features disabled and only `os-poll,net`. Cargo metadata reports mio MIT and its transitive libc0.2.191 MIT OR Apache-2.0. Cargo.lock owns exact registry checksums. This is not a public Rill dependency or readiness API.

One explicitly owned `Reactor` owns one poll thread, mio Poll/Registry and Waker. Dropping Reactor sets terminal state, wakes Poll and joins the thread. `ReactorHandle` registers one-shot token generations; events remove the generation before coalescing a Rill-runtime WakeNotifier hint. Withdrawal deactivates callbacks, deregisters the source and waits for in-flight notification quiescence. Tokens start above the reserved reactor-waker token and are never reused before checked usize exhaustion, so stale/late events cannot target another registration.

`TcpProgress` consumes a standard TcpStream without native setup. Its first advance sets nonblocking mode and wraps it in mio. Each bounded advance attempts at most one nonblocking read/write/flush step; WouldBlock becomes Waiting and a scoped `TcpProgressWait` borrows the driver until consuming withdrawal. There is one logical read and one write lane, one flush barrier, quiescent close and joined terminal release. Read/write command units are capped at16KiB. No worker thread per socket or unbounded blocking executor is added.

## Primary source constraints

- [mio1.2.5 Poll/Registry source](https://github.com/tokio-rs/mio/blob/v1.2.5/src/poll.rs) documents registration, reregistration and deregistration. Deregistration guarantees the old token is not returned by a future poll when the call happens-before that poll; the source must have been registered with the same Poll.
- [mio1.2.5 Waker API/source](https://github.com/tokio-rs/mio/blob/v1.2.5/src/waker.rs) provides a cross-thread poll wake, used only for explicit reactor shutdown here—not as an application operation result.
- [Unix registered source](https://github.com/tokio-rs/mio/blob/v1.2.5/src/sys/unix/selector/registered_io_source.rs) re-registers stateful selectors after an I/O attempt returns WouldBlock. The driver therefore attempts real I/O and treats readiness as a hint, never success.
- [Linux epoll backend](https://github.com/tokio-rs/mio/blob/v1.2.5/src/sys/unix/selector/epoll.rs) uses edge-triggered EPOLLET; correct callers must continue bounded attempts/re-register around WouldBlock rather than assume one event per operation.
- `cargo info mio` reports version1.2.5, Rust minimum1.71, repository `tokio-rs/mio`, MIT license and features `os-poll`/`net`. The project compiler is Rust1.98.0.

Mio can report readiness that no longer permits progress, and another operation can consume readiness first. The runtime always retries the authoritative socket operation. Coalesced notifier hints carry no byte/error result. Source inspection does not prove target behavior.

## Current executed scope

On the current macOS27.0.1 AArch64 host:

- 15 channel/wake/watch tests cover buffered/rendezvous try operations, pending sender input recovery, closure/drain, coalescing, one-shot/rearm, endpoint/target loss, registration cleanup and synchronized arm-transition races.
- TCP loopback tests cover deferred nonblocking setup, read wait/wake/withdraw, registration after already-readable state, full write plus ordered flush, simultaneous logical read/write demand, clean FIN and cached End, stale withdrawn registration, duplicate registration rejection, stop interruption, limit rejection and terminal close.
- The complete runtime suite passed20 repeated invocations during development; the normal repository check executes it once with clippy warnings denied.

This is Rust component execution using loopback TCP on one host. It does not run Rill, generated code or the future Rill scheduler. No Linux, macOS x86-64, resource-fault, reset/prefix-error, sustained backpressure/partial-write, token-wrap, reactor-poll-failure or multi-connection stress cell has run. The use of std::net::shutdown during release is a current implementation boundary, not peer acknowledgement or graceful application protocol completion.

`mise run test:runtime` deliberately remains unavailable/exit2. #47 stays open for compiler intrinsics/metadata, generated ABI, scheduler/cancellation integration, complete progress outcomes, real native target matrix and cross-component Rill acceptance. #44/#48 remain separate TLS/HTTP gates.
