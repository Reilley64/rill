# How do languages run concurrent code without async/await?

Research for [issue #4](https://github.com/Reilley64/rill/issues/4). Every Rill function can suspend and `go f(x)` starts a goroutine, with no async/await in user code. This file records how Go, Java virtual threads (Project Loom), the BEAM (Erlang and Elixir) and Rust's `may` crate do this, what an LLVM-targeting compiler would need, and what changes on WebAssembly.

Links to source files point at the default branch of each project as read on 2026-10-21. Statements marked "inference" are my reading of the cited material, not a claim the source makes.

## Terms

- A **stackful** design gives each task its own call stack. Any function at any depth can suspend, because the whole stack is set aside and another one is switched in. Go, Loom, the BEAM and `may` all behave this way from the programmer's point of view.
- A **stackless** design turns each function that can suspend into a state machine whose live variables sit in a heap "coroutine frame". Suspending returns to the immediate caller, so every function on the path to the suspension point must itself be transformed. This is how LLVM coroutines and async/await work. The split between functions that can suspend and functions that cannot is what [Bob Nystrom called function color](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/).
- **M:N scheduling** runs M tasks on N OS threads. All four systems below use it. [Rust RFC 230](https://github.com/rust-lang/rfcs/blob/master/text/0230-remove-runtime.md) has a short overview of the 1:1 and M:N trade-offs, including that "blocking in the OS sense means blocking the worker thread" in M:N models.

## Go

### Stacks: stackful, contiguous, grown by copying

- Each goroutine starts with a small stack. The minimum is `stackMin = 2048` bytes ([runtime/stack.go](https://github.com/golang/go/blob/master/src/runtime/stack.go)).
- Every function prologue compares the stack pointer with `g->stackguard` and calls `morestack` if the frame does not fit ([runtime/stack.go](https://github.com/golang/go/blob/master/src/runtime/stack.go), header comment).
- `newstack` allocates a larger stack and relocates the goroutine onto it. "Stack growth is multiplicative, for constant amortized cost." `copystack` copies the old stack and `adjustpointers` rewrites every pointer that points into the old stack, using per-frame pointer bitmaps (stack maps) ([runtime/stack.go](https://github.com/golang/go/blob/master/src/runtime/stack.go)). `shrinkstack` shrinks stacks later.
- Go 1.3 replaced segmented stacks with this contiguous model because of the "hot split" problem, where "a calculation repeatedly steps across a segment boundary" ([Go 1.3 release notes](https://go.dev/doc/go1.3#stacks)).
- The [contiguous stacks design doc](https://go.dev/s/contigstacks) explains why copying is possible: escape analysis guarantees that "pointers to data on the stack are only ever passed down in the call tree", so "the only pointers to stack data are in the stack itself". Copying also needs to know "for each pointer-sized word on the stack, whether it is a pointer or not", which Go already had for its precise GC. The same doc lists the few exceptions (deferred call arguments and closures) that the runtime must patch by hand.

### Scheduler

- The runtime has three kinds of objects: G (goroutine), M (OS thread) and P (a "processor", a resource an M must hold to run Go code). An M "can be blocked or in a syscall w/o an associated P" ([runtime/proc.go](https://github.com/golang/go/blob/master/src/runtime/proc.go), "Goroutine scheduler" comment; design doc at https://golang.org/s/go11sched).
- Each P has a local run queue. `findRunnable` "tries to steal from other P's, get g from local or global queue, poll network", and checks the global queue every 61st tick so two goroutines cannot starve it by respawning each other ([runtime/proc.go](https://github.com/golang/go/blob/master/src/runtime/proc.go)).
- Idle threads "spin" looking for work before parking, and the runtime unparks a new thread only if there is an idle P and no spinning thread ([runtime/proc.go](https://github.com/golang/go/blob/master/src/runtime/proc.go)).

### Preemption

- Up to Go 1.10, goroutines could only be switched at "safe-points only at function calls", inserted by the compiler in function prologues. Tight loops without calls could stall the scheduler and the GC ([non-cooperative preemption proposal](https://github.com/golang/proposal/blob/master/design/24543-non-cooperative-preemption.md)).
- Go 1.14 made goroutines "asynchronously preemptible" by sending the thread an OS signal, on all platforms except `windows/arm`, `darwin/arm`, `js/wasm` and `plan9/*` ([Go 1.14 release notes](https://go.dev/doc/go1.14#runtime); [proposal](https://github.com/golang/proposal/blob/master/design/24543-non-cooperative-preemption.md)). The proposal says most of the complexity comes from the GC: "Go must be able to find the live pointers on a goroutine's stack wherever it stops it."
- The background `sysmon` thread preempts a goroutine that has run longer than `forcePreemptNS = 10ms` ([runtime/proc.go](https://github.com/golang/go/blob/master/src/runtime/proc.go)).

### How blocking parks a goroutine

- `gopark` is the common primitive. It records a wait reason and an unlock callback, takes the goroutine off its M, and the M picks another G. `goready` makes the G runnable again ([runtime/proc.go](https://github.com/golang/go/blob/master/src/runtime/proc.go)).
- Channels: a send that cannot proceed builds a `sudog` (the waiting goroutine plus a pointer to the value), enqueues it on the channel's `sendq`, and calls `gopark`. The receiver that later matches it copies the value and readies the sender ([runtime/chan.go](https://github.com/golang/go/blob/master/src/runtime/chan.go)).
- Network I/O: sockets are non-blocking and registered with an edge-triggered poller (epoll, kqueue, event ports, and a Windows implementation). When a read would block, `netpollblock` stores the G pointer in the fd's `pollDesc` and parks it. The scheduler calls `netpoll` and readies the Gs whose fds became ready ([runtime/netpoll.go](https://github.com/golang/go/blob/master/src/runtime/netpoll.go)).
- System calls that may block (including files and cgo): the goroutine calls `entersyscall` and keeps its M. `sysmon` runs `retake` to take the P away from Ms stuck in syscalls and hands it to another M (`handoffp`), so other goroutines keep running ([runtime/proc.go](https://github.com/golang/go/blob/master/src/runtime/proc.go)). The cost is one OS thread per blocked syscall.

## Java virtual threads (Project Loom)

### Stacks: stackful semantics, frames copied to the heap

- A virtual thread runs on a platform thread called its carrier. It "mounts" on the carrier to run and "unmounts" when it blocks ([JEP 444](https://openjdk.org/jeps/444)).
- "The stacks of virtual threads are stored in Java's garbage-collected heap as stack chunk objects. The stacks grow and shrink as the application runs ... to accommodate stacks of depth up to the JVM's configured platform thread stack size" ([JEP 444](https://openjdk.org/jeps/444)).
- HotSpot implements this as continuation "freeze" (yield) and "thaw" (run). Freeze copies the virtual thread's frames off the carrier stack into a stack chunk. Thaw copies frames back, and can do so lazily: a "return barrier" in the bottom frame thaws more frames when the code returns into them. The amortized budget for each operation is about 100 to 150 ns. On the fast path, compiled frames "are simply copied"; on the slow path, internal pointers in interpreted frames are converted to and from relative offsets ([continuationFreezeThaw.cpp](https://github.com/openjdk/jdk/blob/master/src/hotspot/share/runtime/continuationFreezeThaw.cpp), file comment).
- Inference: this works because the JVM knows the layout of every Java frame and Java code cannot hold a raw address of a stack slot, so frames can be moved. Native frames cannot be moved, which is why native code pins (below).

### Scheduler

- The scheduler is "a work-stealing ForkJoinPool that operates in FIFO mode". Its parallelism defaults to the number of available processors ([JEP 444](https://openjdk.org/jeps/444)).
- "The scheduler does not currently implement time sharing for virtual threads." A virtual thread runs until it blocks or ends ([JEP 444](https://openjdk.org/jeps/444)).

### How blocking parks a virtual thread

- `java.util.concurrent.LockSupport` parks a virtual thread by releasing the carrier and unparks it by scheduling it again, so locks, semaphores and blocking queues work without change ([JEP 444](https://openjdk.org/jeps/444), java.util.concurrent section). Channels in the Go sense are `BlockingQueue`s built on this.
- Socket operations in `java.net` and `java.nio.channels` release the carrier while they wait ([JEP 444](https://openjdk.org/jeps/444), Networking section).
- Some operations cannot unmount, "because of limitations at either the OS level (e.g., many filesystem operations) or the JDK level". These "compensate for the capture of the OS thread by temporarily expanding the parallelism of the scheduler" ([JEP 444](https://openjdk.org/jeps/444)).
- Pinning: in JDK 21 a virtual thread could not unmount inside `synchronized` or while running a native method or foreign function ([JEP 444](https://openjdk.org/jeps/444)). JDK 24 removed the `synchronized` case ([JEP 491](https://openjdk.org/jeps/491)). Native code that calls back into Java which then blocks still pins ([JEP 491](https://openjdk.org/jeps/491)).

## BEAM (Erlang and Elixir)

### Stacks: one small VM stack per process

- A new process uses 327 words of memory, including "233 words for the heap area (which includes the stack)" ([Erlang efficiency guide: processes](https://www.erlang.org/doc/system/eff_guide_processes.html)).
- "Each Erlang process has its own stack and heap which are allocated in the same memory block and grow towards each other." When they meet, a per-process copying GC runs, and the block grows if not enough memory was reclaimed ([GarbageCollection.md](https://github.com/erlang/otp/blob/master/erts/emulator/internal_doc/GarbageCollection.md)).
- This stack belongs to the VM, not the OS. With the BeamAsm JIT, native code runs on the Erlang stack, but "as Erlang stacks can be very small, we have to switch over to a different stack when we need to execute C code" ([BeamAsm.md](https://github.com/erlang/otp/blob/master/erts/emulator/internal_doc/BeamAsm.md)). Inference: stack growth needs no special machinery because the GC already relocates the process's memory block.

### Scheduler and preemption

- One scheduler thread per logical processor by default (`+S`) ([erl command flags](https://www.erlang.org/doc/apps/erts/erl_cmd.html)).
- Each scheduler has its own run queue. "When a scheduler thread runs out of work it tries to steal work from another scheduler threads run queue" ([ProcessManagementOptimizations.md](https://github.com/erlang/otp/blob/master/erts/emulator/internal_doc/ProcessManagementOptimizations.md)).
- Preemption counts reductions. A process is swapped out after `CONTEXT_REDS 4000` reductions ([erl_vm.h](https://github.com/erlang/otp/blob/master/erts/emulator/beam/erl_vm.h)). In BeamAsm a register holds the remaining reductions, and the return instruction decrements it and jumps to a shared yield fragment when it goes negative ([BeamAsm.md](https://github.com/erlang/otp/blob/master/erts/emulator/internal_doc/BeamAsm.md)). This is compiler-inserted preemption with no OS signals.

### How blocking parks a process

- Message passing is the channel mechanism. A `receive` with no matching message suspends the process "possibly indefinitely, until a message arrives that matches" ([Erlang reference manual: expressions](https://www.erlang.org/doc/system/expressions.html)).
- Network I/O goes through the "check I/O" subsystem, a platform poller (`erl_poll`) that delivers readiness to ports through port signals and to NIFs as Erlang messages ([CheckIO.md](https://github.com/erlang/otp/blob/master/erts/emulator/internal_doc/CheckIO.md)). So a process waiting on a socket is just a process waiting in `receive`.
- Native code that runs long is a problem the runtime cannot preempt. A NIF should return "within 1 millisecond", or be split up, moved to a thread, or marked dirty so it runs on separate dirty schedulers ([erl_nif](https://www.erlang.org/doc/apps/erts/erl_nif.html)). There are 10 dirty I/O scheduler threads by default, and "typical users of the dirty IO schedulers are reading and writing to files" ([erl command flags](https://www.erlang.org/doc/apps/erts/erl_cmd.html)).

## Rust `may` crate

### Stacks: stackful, fixed size, guard page

- `may` provides "stackful coroutines" and describes itself as "the Rust version of the popular Goroutine". The coroutine implementation is the `generator` crate ([may README](https://github.com/Xudong-Huang/may/blob/master/README.md)), which switches stacks with hand-written assembly per architecture and OS ABI (for example `asm_x86_64_sysv_elf.S`, `asm_aarch64_aapcs_macho.S`) ([generator-rs src/detail/asm](https://github.com/Xudong-Huang/generator-rs/tree/master/src/detail/asm)).
- "May doesn't support automatic stack increasing." Each coroutine gets a heap-allocated stack of 4k words (32 KiB on 64-bit) by default, configurable globally or per coroutine ([tune_stack_size.md](https://github.com/Xudong-Huang/may/blob/master/docs/tune_stack_size.md); `DEFAULT_STACK_SIZE` in [config.rs](https://github.com/Xudong-Huang/may/blob/master/src/config.rs)). "There is a guard page for each coroutine stack. When stack overflow occurs, it will trigger segment fault error" ([may README](https://github.com/Xudong-Huang/may/blob/master/README.md)). The caveats doc tells users to avoid recursion and functions with large frames ([may_caveat.md](https://github.com/Xudong-Huang/may/blob/master/docs/may_caveat.md)).

### Scheduler

- Worker threads with local queues, global queues and work stealing (`work_steal` is a default feature), plus event loop threads and a timer thread ([scheduler.rs](https://github.com/Xudong-Huang/may/blob/master/src/scheduler.rs); [Cargo.toml](https://github.com/Xudong-Huang/may/blob/master/Cargo.toml)).
- Scheduling is cooperative only: "if a running coroutine doesn't yield out, it will occupy the running thread". Long CPU-bound code should call `coroutine::yield_now()` ([may_caveat.md](https://github.com/Xudong-Huang/may/blob/master/docs/may_caveat.md)).

### How blocking parks a coroutine

- `may::net` sockets try the non-blocking syscall first. On `EAGAIN` the coroutine calls `yield_with_io`, which registers it on the fd's `IoData` and yields; the epoll or kqueue loop reschedules it when the fd is ready ([socket_read.rs](https://github.com/Xudong-Huang/may/blob/master/src/io/sys/unix/net/socket_read.rs)).
- `may` ships its own MPMC channel, semaphore and other primitives ([may README](https://github.com/Xudong-Huang/may/blob/master/README.md)).
- Because Rust's standard library is unaware of the scheduler, calling blocking std APIs (socket `read`, `Mutex`, `thread::sleep`) or third-party code that uses them "would halt the worker thread". Thread-local storage can be undefined behavior, because a coroutine can move to another thread between a TLS write and a TLS read ([may_caveat.md](https://github.com/Xudong-Huang/may/blob/master/docs/may_caveat.md)).

## Rust's own history

Rust once had this model. Its green threads were M:N with their own stacks. Segmented stacks were tried first and removed "in favor of pre-allocated stacks, which means Rust's green threads are not lightweight". RFC 230 then removed the runtime, citing forced co-evolution of the green and native I/O APIs, task-local storage cost, native code that blocks the green scheduler, and the setup needed to embed Rust in other programs ([RFC 230](https://github.com/rust-lang/rfcs/blob/master/text/0230-remove-runtime.md)).

## Summary table

| | Go | Loom | BEAM | `may` |
|---|---|---|---|---|
| Stack model | stackful, own contiguous stack | stackful semantics, frames copied between carrier stack and heap | stackful, VM stack inside the process heap | stackful, fixed native stack |
| Initial size | 2 KiB | grows with use | part of 233-word heap | 32 KiB default |
| Growth | copy to a stack twice as large, fix pointers with stack maps | grows as frames are frozen | GC grows the heap block | none; guard page faults |
| Preemption | prologue checks plus signals (10 ms) | none | reduction counting (4000) | none |
| Scheduler | per-P queues, global queue, stealing | ForkJoinPool, FIFO, stealing | per-scheduler queues, stealing | local and global queues, stealing |
| Network I/O | netpoller parks G | JDK sockets unmount | check I/O delivers messages | epoll/kqueue parks coroutine |
| Blocking syscalls and FFI | P handed to another M | pool expands; native frames pin | dirty schedulers | blocks worker thread |

## What an LLVM-targeting compiler needs

### Stack switching (stackful designs)

- A context switch saves callee-saved registers and the stack pointer and loads another set. LLVM has no intrinsic for this, so libraries write it in assembly per architecture and ABI, as `generator-rs` does ([generator-rs src/detail](https://github.com/Xudong-Huang/generator-rs/tree/master/src/detail)).
- The POSIX alternative, `makecontext`/`swapcontext`, was "removed in POSIX.1-2008, citing portability issues" ([makecontext(3)](https://man7.org/linux/man-pages/man3/makecontext.3.html)).
- Thread-local storage needs care: once a goroutine can move between OS threads, any cached TLS address is wrong after a switch ([may_caveat.md](https://github.com/Xudong-Huang/may/blob/master/docs/may_caveat.md)). Inference: a compiler that owns the runtime can keep goroutine-local state behind its own pointer instead of using OS TLS for it.

### Stack growth options

1. **Fixed stacks with a guard page** (`may`). Simple. Overflow is fatal and the user picks the size ([may_caveat.md](https://github.com/Xudong-Huang/may/blob/master/docs/may_caveat.md)).
2. **Segmented stacks.** LLVM supports them with the `"split-stack"` function attribute: the prologue compares the stack pointer with a limit in the thread control block and calls libgcc's `__morestack`, which links a new "stacklet" ([LLVM Segmented Stacks](https://llvm.org/docs/SegmentedStacks.html)). Go and Rust both dropped this because of the hot split cost ([Go 1.3](https://go.dev/doc/go1.3#stacks); [RFC 230](https://github.com/rust-lang/rfcs/blob/master/text/0230-remove-runtime.md)).
3. **Copying stacks** (Go). Needs every pointer into the stack to be findable and rewritable, which Go gets from escape analysis plus precise stack maps ([contiguous stacks design](https://go.dev/s/contigstacks)). LLVM's statepoint support records pointers held in frames so a relocating GC can update them ([LLVM Statepoints](https://llvm.org/docs/Statepoints.html)). Inference: LLVM does not track pointers to `alloca` slots in the same way, so a Rill compiler would have to forbid or box any stack address that could survive a growth point, and emit its own maps.
4. **Frame copying on suspend** (Loom). Run on a large per-thread stack and copy frames to the heap when a goroutine suspends ([continuationFreezeThaw.cpp](https://github.com/openjdk/jdk/blob/master/src/hotspot/share/runtime/continuationFreezeThaw.cpp)). Inference: this has the same "no stack addresses that outlive their frame position" requirement as option 3, and the cost grows with the number of frames copied, which lazy thawing reduces.

Each stackful option also needs a prologue check (options 2 and 3) or a guard page (options 1 and 4), plus a way for foreign calls to run on a large enough stack, as BeamAsm does when it switches to the C stack ([BeamAsm.md](https://github.com/erlang/otp/blob/master/erts/emulator/internal_doc/BeamAsm.md)).

### LLVM coroutine intrinsics (stackless)

- An LLVM coroutine is a function with `llvm.coro.suspend` points. The coroutine passes split it into a ramp function and resume functions, and "any state which must persist across suspensions is stored in the coroutine frame". Suspension returns "control ... back to its caller" ([LLVM Coroutines](https://llvm.org/docs/Coroutines.html)).
- Lowerings: switched-resume (`llvm.coro.id`; one resume function that switches on a stored suspend index), returned-continuation (`llvm.coro.id.retcon`), and async (`llvm.coro.id.async`, frontend-controlled context and one resume function per suspend point) ([LLVM Coroutines](https://llvm.org/docs/Coroutines.html)). The allocation protocol is designed so the frame allocation can be elided after inlining ([LLVM Coroutines](https://llvm.org/docs/Coroutines.html)). The doc warns that "compatibility across LLVM releases is not guaranteed".
- Inference: to hide this from users, the compiler would lower every function that can transitively reach a suspension point as a coroutine, and every call to such a function as "call, and if it suspended, suspend too". In Rill, where every function can suspend, that is all functions unless whole-program analysis proves otherwise. Calls through function values and calls into foreign code make that analysis conservative. Asyncify's measurements show the same effect on WebAssembly (below).
- There is no stack to grow in this design, and no per-goroutine stack memory beyond the frames that are alive. Recursion depth is bounded by heap rather than by a stack size.

### Other runtime pieces every design needs

- A per-thread run queue with work stealing (all four systems above).
- A poller thread or integrated poller for sockets, with fd readiness mapped to "make this goroutine runnable" (Go netpoll, BEAM check I/O, `may` event loop).
- A policy for calls that block an OS thread: hand the scheduler slot to another thread (Go), grow the pool (Loom), run on separate schedulers (BEAM dirty schedulers), or document it as a hazard (`may`).
- A preemption policy: none (Loom, `may`), compiler-inserted counters or checks (BEAM, Go before 1.14), or OS signals (Go 1.14+). Signals require the runtime to resume a goroutine at an arbitrary instruction, which Go found hard because of the GC ([proposal](https://github.com/golang/proposal/blob/master/design/24543-non-cooperative-preemption.md)).

## What changes on WebAssembly

- Wasm code cannot see or switch its own call stack. The call stack is "protected" and "invulnerable to buffer overflows in the module heap"; only values whose address is taken live in a separate stack in linear memory ([WebAssembly security docs](https://webassembly.org/docs/security/)). So a native-style assembly context switch is impossible, and stackful designs need one of the options below.
- **Go's approach**: the Go wasm backend keeps goroutine frames in linear memory, adds a resume point after every call, and switches goroutines by unwinding the Wasm stack (each call returns a flag that says "unwind") and re-entering functions through a jump table on the saved PC ([wasmobj.go](https://github.com/golang/go/blob/master/src/cmd/internal/obj/wasm/wasmobj.go)). `js/wasm` "has no support for threads yet. There is no preemption" ([lock_js.go](https://github.com/golang/go/blob/master/src/runtime/lock_js.go)). When no goroutine is runnable, `beforeIdle` pauses and returns to the JavaScript event loop ([lock_js.go](https://github.com/golang/go/blob/master/src/runtime/lock_js.go)).
- **Asyncify** (Binaryen pass): instruments code so the call stack can be unwound into linear memory and rewound later. Code size grew by about 50% on average in the worst-case mode, where every import and indirect call might unwind. Telling it which imports can unwind helps, but "this fails on larger programs" because of indirect calls ([Asyncify announcement by its author](https://kripken.github.io/blog/wasm/2019/07/16/asyncify.html)).
- **JS Promise Integration (JSPI)**: lets a Wasm module call a Promise-returning JavaScript import as if it were synchronous; the engine suspends the Wasm stack and resumes it when the Promise settles. It adds no Wasm instructions ([JSPI overview](https://github.com/WebAssembly/js-promise-integration/blob/main/proposals/js-promise-integration/Overview.md)). It is at phase 5, standardized ([WebAssembly proposals](https://github.com/WebAssembly/proposals/blob/main/README.md)). It suspends at the JS boundary only, so it does not by itself switch between goroutines inside the module. Inference: it fits a design where the whole Wasm instance waits on host I/O.
- **Stack switching proposal**: adds typed continuations (`cont.new`, `resume`, `suspend`, and a symmetric `switch`) whose stated use cases include "lightweight threads" ([stack switching explainer](https://github.com/WebAssembly/stack-switching/blob/main/proposals/stack-switching/Explainer.md)). It is at phase 3, implementation ([WebAssembly proposals](https://github.com/WebAssembly/proposals/blob/main/README.md)). When shipped, it is the direct equivalent of a native stack switch.
- **Threads**: the Threads proposal is at phase 4 and shared-everything threads at phase 1 ([WebAssembly proposals](https://github.com/WebAssembly/proposals/blob/main/README.md)). Inference: a Wasm runtime should assume a single-threaded scheduler for now, as Go does.
- **Stackless on Wasm**: LLVM coroutine lowering rewrites IR into ordinary functions before code generation ([LLVM Coroutines](https://llvm.org/docs/Coroutines.html)). Inference: the switched-resume and returned-continuation output needs no stack access, so it can target Wasm as is. The async lowering emits `musttail` calls at suspend points ([LLVM Coroutines](https://llvm.org/docs/Coroutines.html)), and Wasm gained tail calls in 3.0 ([finished proposals](https://github.com/WebAssembly/proposals/blob/main/finished-proposals.md)).

## Implications for Rill

These are options and trade-offs for the follow-up grilling ticket. This file does not pick one.

1. **Suspension mechanism.**
   - Stackful with fixed stacks and guard pages. Least compiler work, one assembly switch per target. Memory per goroutine is the chosen stack size, and overflow is fatal.
   - Stackful with segmented stacks (`split-stack`). LLVM support exists. Go and Rust both left it because of hot splits.
   - Stackful with copying stacks (Go) or frame copying (Loom). Small initial stacks and growth. Requires the compiler to know every pointer into the stack, which Rill has no GC metadata for today, or a language rule that stack addresses never escape.
   - Stackless via LLVM coroutines on every function that may suspend. No stacks and no growth problem, and it maps onto Wasm without stack switching. Costs: a frame per suspending call unless elided, code size, conservative analysis around function values and FFI, and reliance on intrinsics without cross-release stability.
   - A mix: stackless on Wasm and stackful on native, at the price of two code generation paths.
2. **Preemption.** None (Loom, `may`), compiler-inserted checks at calls and loop back edges with a budget (BEAM, Go before 1.14), or signal-based (Go 1.14+). Inserted checks also work on Wasm; signals do not exist there.
3. **Blocking syscalls and FFI.** Hand the worker slot to another thread (Go), grow the pool (Loom), route to separate blocking threads (BEAM dirty schedulers), or forbid or wrap them (`may`). Rill controls its standard library, so it can avoid the `may` problem of a scheduler-unaware std, but foreign code stays a risk.
4. **I/O integration.** An integrated poller that parks goroutines on fd readiness is common to all four. Decide whether the scheduler polls (Go) or a separate thread polls (`may`, BEAM).
5. **Channels.** All four park the waiter in a queue on the channel or mailbox and wake it on match. The choice is between Go-style typed channels with `select` and BEAM-style mailboxes. That is a language question as much as a runtime one.
6. **Scheduler shape.** Per-worker queues with work stealing are used by all four; there is little to decide beyond queue sizes and fairness rules such as Go's every-61-ticks global check.
7. **WebAssembly target.** Single-threaded scheduler for now. Pick between Go-style unwinding with resume points, Asyncify, stackless lowering, or waiting for the stack switching proposal (phase 3). JSPI covers host async calls at the boundary in browsers.
8. **Goroutine-local state and TLS.** Decide whether Rill exposes thread-local storage at all, since goroutines move between threads.
