# #47 native readiness reactor selection and first TCP mechanism evidence

Status: implementation research and current-host component evidence, **not** a Rill ABI, Linux/macOS support matrix, TCP conformance result or #47 completion.

## Selected private mechanism

The implementation-private `runtime` package uses locked **mio1.2.5** with default features disabled and only `os-poll,net`. Tests pin socket2 0.6.5 for portable send-buffer/linger fault setup. Cargo metadata reports mio MIT, socket2/libc MIT OR Apache-2.0; Cargo.lock owns exact registry checksums. These are not public Rill dependencies or readiness APIs.

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
- 2 reactor tests cover checked token-space exhaustion before registration and explicit-owner drop retiring an active callback generation.
- 13 TCP loopback tests cover deferred nonblocking setup, read wait/wake/withdraw, registration after already-readable state, full write plus ordered flush, simultaneous logical read/write demand, clean FIN/cached End, buffered data before reset failure, stale withdrawn token, duplicate registration, stop, close/limits, sustained send-buffer backpressure with wait/rearm, and32 concurrent connections whose wake hints coalesce without losing per-driver results.
- The complete30-test runtime suite passed20 repeated invocations after the multi-connection addition and rearm correction. The normal repository check executes it once with clippy warnings denied.

This is Rust component execution using loopback TCP, not Rill/generated code or the future Rill scheduler. GitHub Actions [run43841069174](https://github.com/Reilley64/rill/actions/runs/43841069174) passed `mise run check` at commit9952484 on `ubuntu-24.04`, `ubuntu-24.04-arm`, `macos-15` and `macos-15-intel`; all four jobs executed the30 runtime tests through the shared check task. Hosted labels/configuration are evidence for those runner cells, not a kernel/SDK minimum or physical-hardware attestation beyond GitHub's service. Resource-allocation/reactor-poll failure, guaranteed native partial-write (distinct from WouldBlock) and true token wrap remain untested; the current multi-connection case is a finite32-loopback fixture, not load/stress acceptance. The use of std::net::shutdown during release is a current implementation boundary, not peer acknowledgement or graceful application protocol completion.

`mise run test:runtime` deliberately remains unavailable/exit2. #47 stays open for compiler intrinsics/metadata, generated ABI, scheduler/cancellation integration, complete progress outcomes, real native target matrix and cross-component Rill acceptance. #44/#48 remain separate TLS/HTTP gates.
