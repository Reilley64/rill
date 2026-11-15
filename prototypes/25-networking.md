# Prototype 25: numeric addresses and controlled TCP networking

Human-confirmed [#34](https://github.com/Reilley64/rill/issues/34) Q1–Q93, the [networking contract](../docs/networking-contracts.md), and [ADR 0023](../docs/adr/0023-tcp-addresses-resolution-and-controlled-networking.md) govern this inventory. The contract and ADR are the authority, not the schematic workflows below. Prerequisites: [#23](https://github.com/Reilley64/rill/issues/23), [#32 canonical streaming I/O](https://github.com/Reilley64/rill/issues/32#issuecomment-6886715215) with its [scoped noBorrow refinement](https://github.com/Reilley64/rill/issues/32#issuecomment-6886862143), and [#33 clock/token contract](https://github.com/Reilley64/rill/issues/33#issuecomment-6891620139). Q81 explicitly adds `std::time::checkInstantDomain`; it is not inferred from private clock fields.

**Status: design fixtures only.** `25-networking.rill` contains **32 top-level intended-valid expectations**, with standalone typed helpers. These were manually compared with the confirmed contracts and published prototypes 19 (JSON), 23 and 24. No Rill compiler, parser, typechecker, runtime, native provider or security tests were run. Source-text counting/inspection is not such proof. No live connection, listener, lookup, interface query or system-clock read appears in an expectation. No opaque resource/FD is fabricated. All native workflows and matrix entries below are **NOT RUN**.

Q93 supersedes historical field-mutability shorthand: options and `NetControl` have ordinary public fields. A `let mut` binding/exclusive access permits edits; there is no field-level `mut`, `readonly` or `const` syntax. Operations/views capture stable by-value snapshots and retain ordinary borrow dependencies. A separately edited descriptor cannot retarget active work. A snapshot's token still observes its source's later request; snapshot does not freeze cancellation state or clock time.

## Intended-valid source coverage

| Expectations, in source order | Design assertion |
| --- | --- |
| 1–2 | Infallible fixed-tuple constructors, copied octet/segment observations, explicit `u8`/`u16` widths and canonical address copy/equality. Tuples are not fixed-array syntax. |
| 3–6 | Copied IP/port/scope observers; v4 scope `None` versus v6 `Some(0)`; port zero is data; maximum `u16` port and `u32` scope remain representable. No current interface existence check. |
| 7–15 | Strict IPv4 output; IPv6 lowercase/longest-leftmost zero compression/no single-zero compression; mapped dotted tail versus other embedded-v4 hex output; decimal port/scope normalization; zero scope omitted. |
| 16–18 | Equivalent spellings compare/hash equally, no asserted hash constant or unequal-key collision freedom; mapped IPv6 stays distinct from IPv4; port and scope affect endpoint equality. |
| 19–23 | Malformed parsing returns `InvalidInput`, the original parse operation and an empty native-code list; no resolver fallback, interface lookup or message-text matching. Each helper checks multiple independent spellings. |
| 24 | Owned numeric result outlives the local source string. |
| 25–29 | Options defaults, ordinary mutable fields, independently copied record snapshots, nominal family variants and exact custom-keepalive duration/count storage. Constructing an options record does not validate/apply it. |
| 30–32 | Identity-only domain check, same-domain copied clock versus `DomainMismatch`, ordinary mutable control descriptor versus saved snapshot, shared token observation and a manual absolute-deadline value. Only explicit manual-clock state is read/advanced. |

The source imports canonical `show`, `eq`, `hash`, `copy` (fixed prelude), error and observation identities rather than defining namesake dispatch. It defines no family bodies; standalone helpers use `function`. Any future canonical family body must use `implement`. No reflection-based record/enum equality, automatic conversions, arrays, traits, public `dyn`, unions or singleton-string types are implied.

The manual cases do **not** exercise controlled native preflight, actual options application, `ControlledTcp`, timer registration, late I/O or dial arbitration. Those require the independent cases and unpassed native evidence below. No public hostname-validation/options-validation/test-backend API is invented to make those cases executable without a runtime.

## Independent rejection and boundary inventory

These cases are separate from the intended-valid source, not deliberately invalid declarations mixed into that file. **Static rejection** means a future checker must reject; **typed input failure** means the source can typecheck but the operation must fail normally before native work. None has been compiler/runtime tested. Valid data is not rejected merely because it is unsuitable for an outgoing connection.

### Static ownership, capabilities and syntax

| Case | Required rejection/boundary |
| --- | --- |
| Manufacture an `Ipv4Address`, `SocketAddress`, `TcpStream`, listener, half, `Clock` or `NetError` from guessed fields/FD integers | Private nominal representation is not a public constructor/ABI. Numeric addresses use their approved factories; live resources must actually be acquired. |
| `copy(&stream)`, `copy(&listener)`, copy either owning half | No supplied resource-copy capability; no raw-FD/clone/reunite escape hatch. |
| Use whole stream after consuming `split`, split it twice, or close an already consumed owner | Move/remaining-field rules, not hidden duplicate ownership. |
| Two pending reads or two pending writes through one whole stream/half | Overlapping exclusive borrows, including suspension and actual native cleanup, reject. Whole-stream simultaneous read/write also rejects; distinct owning halves permit the independent directions. |
| Use/mutate/dispose underlying stream while `ControlledTcp` retains its exclusive borrow | Reject through last use/cleanup. Copying the view cannot erase this dependency. Dropping the view alone does not close the underlying stream. |
| `close(controlledView)` | No canonical consuming-close implementation for this borrowing view. Required teardown is on the actual owning resource. |
| Write/flush/shutdown-write a read half; read a write half; `remoteAddress(listener)` | No applicable canonical capability/observer implementation, not `None` or a dynamic unsupported operation. |
| Mutate read destination or write source, dispose a name, or reuse native state before completion/confirmed release | Borrow/use dependencies survive selection, cancellation requests, owner drops and blocking-pool migration. |
| Return a `NetControl` borrowing local clock/token owners; assert `noBorrow(NetControl)` | Ordinary borrowed fields retain dependencies; no secret ownership or lifetime erasure. Descriptor copy retains them too. |
| Mutate a non-mutable options/control binding, or introduce field-level `mut`/`readonly` syntax | Ordinary mutable-binding/exclusive rules apply; no such field modifier is authorized by Q93. |
| Use `int`, `byte`, `usize`, ambiguous numeric inputs without an established width, optional/named arguments, methods, traits, public `dyn`, unions or literal singleton types | No new surface syntax or fallback widths; ordinary immediate expected-type inference is unchanged. Nominal enums are qualified and records are named. |
| Substitute a local namesake for canonical I/O/show/error evidence or use `function` for a canonical implementation body | Ordinary identity/family rules and `implement` apply before monomorphization. |
| Buffered adapters with underlying errors lacking their scoped `noBorrow` requirements | #32 Q36 rejects those adapters; do not extend this into a blanket primitive-I/O or `sendable` requirement. `NetError` supplies the required properties. |
| Intercept protected disposal/copy/native release through a generic or nested-payload specialization | #23 protected actions remain non-forgeable; private fields alone are not the entire rule. |

### Pure parsing and operation-specific validation

All malformed pure parse cases below require `Result` failure with `NetErrorKind.InvalidInput`, the corresponding parse `NetOperation` and no native work. Human-readable message wording is not a portable test oracle.

| Boundary | Independent examples/expected result |
| --- | --- |
| IPv4 strictness | Reject `1`, `127.1`, `2130706433`, `0177.0.0.1`, `0x7f.0.0.1`, `+1.2.3.4`, `1.2.3.-1`, `1.2.3.256`, empty components, NUL, non-ASCII digits, surrounding whitespace, CIDR and trailing text. Accept `0.0.0.0` and `255.255.255.255` as data only. |
| IPv6 grammar | Reject duplicate `::`, too many/few uncompressed segments, overwide segments, malformed dotted tails, leading-zero dotted octets, brackets/zones in bare IP and trailing whitespace. Accept valid RFC 4291 numeric spellings, including all-zero and mapped data, without route probes. |
| Socket grammar | Require port and IPv6 brackets. Reject `[::1]`, `::1:80`, `[192.0.2.1]:80`, hostname/service names, `http://...`, empty/signed/fractional port, `65536`, trailing garbage and whitespace. Port `00080` means decimal 80; port `0` and `65535` are valid data. |
| Scope grammar | Reject named/empty/signed/overflow/repeated zones and URI-style names such as `%25eth0`. `%25` itself means numeric scope 25, **not** URI decoding. `%000` is scope zero and renders omitted; leading zeros otherwise decimal-normalize. Bare `parseIp` never accepts scope. |
| Scope native query | `scopeIndex` rejects NUL/overlong input rather than normalizing/truncating. Native zero is failure; success is positive `u32`. This is not a pure parser test or an interface-enumeration API. |
| Outgoing numeric destination | Connect rejects port zero, unspecified v4/v6, multicast and IPv4 limited broadcast before native setup; repeat v4 checks for mapped embedded-v4 addresses without folding identity. No route/subnet-broadcast oracle. The same numeric data can be parsed and wildcard/zero can be used for explicit binding. |
| Listener options | Zero backlog or value exceeding native signed-C-int range is invalid, not narrowing/wrapping. IPv4 plus `DualStack` is invalid; `V6Only` is neutral on IPv4. Valid backlog is still a native-capped hint. Unsupported requested mode is an error, not downgrade/multi-bind fallback. |
| Keepalive profile | Custom idle/interval must be exact whole seconds 1–32767 inclusive and count 1–127. Reject negative, zero, fractional, 32768-second, count-zero and count-128 profiles without clock reads/rounding/clamping. Records may store these values; operation admission validates them. |
| Dial policy | Reject negative delay and nonpositive `Some(attemptTimeout)`; `None` removes the library cap, zero delay is allowed. No implicit overall timeout or ignored clock failure. |

### Hostname-only boundary, not pure source expectations

`resolveAddresses`/dial validation has no separate public pure validator. The following are future operation-admission cases, **NOT RUN**, not live calls embedded in expectations:

- Accept single labels, digit-initial labels, underscores, ASCII letter/digit/hyphen/underscore labels of 1–63 bytes without leading/trailing hyphen; text at most 253 bytes without root dot or 254 with one. Preserve original case and optional terminal dot when forwarding. `localhost`, `3com.example`, `_service.example`, `123.example` and caller-prepared `xn--...` labels are grammar examples, not guaranteed DNS successes or IDNA-valid names.
- Reject empty/root-only names, empty interior labels, overlong labels/text, Unicode without explicit external ASCII conversion, escapes, NUL, whitespace, URI/path/port text and leading/trailing label hyphens. Test exact length boundaries without relying on DNS answers.
- Reject when **all** components are decimal digit+ or `0x`/`0X` plus hex digit* (including zero hex digits), ignoring only an optional final dot for this validation. Cases: `123`, `127.1`, `999.999.999.999`, `1.2.3.4.5`, `0x`, `0Xf.0x.010.`. This conservative policy excludes real numeric-looking names too; it is not validation as a literal.
- Strict literals use the numeric address path. No native names-only flag is claimed; no legacy numeric form may leak to resolver fallback. No automatic IDNA, search-policy override, own DNS client/cache, hidden `ADDRCONFIG` filter or mapped synthesis.
- A successful lookup owns a nonempty list, stable exact-deduplicated in system order. Empty compatible answers become `NoAddresses`; v4 and mapped-v6 are not duplicates. Caller hostname, resolved endpoints, canonical names and authenticated identity remain different concepts.

## Controlled-operation traces — schematic, NOT RUN

These are **design workflows, not compiler/runtime/security tests**, fake-provider source code, or timing guarantees. A future private implementation harness may coordinate native completion/registration barriers. That seam is not a public injectable networking/clock backend. Manual clocks control logical time only; they do not themselves manufacture native readiness, DNS completion or true quiescence.

### Preflight and control snapshot

| Sequence | Required observation |
| --- | --- |
| Invalid ordinary arguments and/or present deadline from another domain; token already requested | Argument/domain validation wins before token/native work. Domain identity check does not read time or test health. No ordering is promised among unrelated invalid basic inputs. |
| Valid basics/domain; already-requested token; deadline also reached | `TokenCancelled`, without a clock read, native admission or timer registration. |
| Valid basics/domain, token unrequested, required `now` fails | `ClockFailure` with explicit original clock cause; no wall-clock fallback/timing removal. |
| Valid basics/domain, token unrequested, deadline reached | `DeadlineExpired`, before native admission. |
| No deadline or internal timer policy | No needless clock read. Controlled zero read/empty write/no-op flush still preflight; uncontrolled zero/no-op rules are unchanged. |
| Construct a controlled view, then never perform I/O | Constructor stores borrows/control snapshot only; no native admission/clock read. Dropping view leaves the owner open. |
| Copy descriptor into operation/view; edit separate mutable caller descriptor | Captured deadline/clock/token target does not change. Source cancellation and clock advance remain observable through the captured borrowed targets. |

Fixed-arity controlled forms take explicit options/control, with `WithControl` rather than stacked suffixes. Pure parse, split and cached getters need no control; mandatory close/dispose is not externally abortable. `withDeadline` instead returns its nested `Result<R,DeadlineError>` and joins losing cleanup; it does not flatten network errors or inject an ambient control.

### Late I/O, one selected outcome, true release

1. Acquire a **real** loopback stream/half, prepare caller-owned input or append destination, and admit one operation under valid control. Record starting destination length/current write extent and actual native-use registration.
2. Arrange token request/expiry/clock failure after admission. Select exactly once; completion/control ready ties have no guaranteed winner. Preserve the selected original `Err<NetError>`, operation, origin/cause and available native details.
3. Request nondestructive withdrawal if supported. **Do not close an existing borrowed stream/listener/direction just to abort controlled work.** If withdrawal cannot end native use safely, await completion, even beyond the caller deadline.
4. Suppose the native completion reports 7 actual bytes after the selected control error. Final `IoFailure<NetError>.transferred` must be 7, with exactly that read append/current-input write acceptance accounted for. A selected error with late progress is not changed to success, guessed zero or rolled-back I/O. Previously committed prefixes are not replayed.
5. Only after actual completion/confirmed abort and unregister/required cleanup may buffer/handle borrows be released and the result published. `NativeTimeout` remains distinct from application expiry. Intrinsic routine cancellation is outside `Result` and does not promise a progress return to stopped code.
6. A canceled accept/connect/listen can produce an unpublished late resource; own and clean it safely before return. Do not treat cleanup as peer-visible rollback. Secondary ordinary teardown failure cannot replace the selected outcome; retain diagnostic phase/code context where available. Do not retry close against an unowned/recycled FD.

Repeat for read, write, zero operations, no-op flush, accept, resolver, scope lookup and explicit shutdown; shutdown itself may commit a late directional effect. Returning the underlying owner after controlled I/O proves neither protocol synchronization nor safe reuse.

### Dial scheduling, slots and winner retention

**Schematic trace, not a scheduler implementation or RFC 8305 conformance test.** One system lookup returns resolution order `[v6-A, v6-B, v4-C, v4-D]`; exact dedup precedes scheduling. The start order is `[v6-A, v4-C, v6-B, v4-D]`. Mapped answers remain v6. Diagnostic failures retain **original resolution order**, not start/completion order.

| Logical event | Eligibility, slots and outcome |
| --- | --- |
| Time 0: start A | First eligible candidate starts immediately; one live slot. Effective deadline is min(overall deadline, start plus optional attempt cap). |
| Time 250ms: A still active, start C | Default delay measured from actual start; two live slots. |
| Time 260ms: A reports a candidate failure, its native cleanup remains outstanding | Still two live slots. A failure/cancel request is not slot release; no B launch yet. A separately tested per-attempt expiry also permits fallback, unlike selected overall/token/clock control. |
| Time 500ms: delay eligible, but A cleanup still pending | No third live attempt, even if spacing allows one. Start eligibility is not a reserved slot or an accumulated timer credit. |
| Time 600ms: A actual cleanup finishes; C still active | Slot released; B may now start because spacing since C's actual start is satisfied. The next spacing anchor is B's actual start, not an old timer firing. |
| Time 610ms: C succeeds | Select C once and retain its owned winner; admit no D. Request/join B cleanup. If B later succeeds unused, own and release it too. |
| Cleanup extends beyond overall deadline or encounters ordinary cleanup failure | Do not reclassify the selected C success. Return only after required loser release; no hard return bound. Unstarted D and cleanup-stopped B do not acquire invented candidate failures. |

Separate traces must cover: no active attempt means immediate next start once a slot is truly free; single-family order; simultaneous successes unspecified; no launches after global control; all failures in resolution order; ordinary preparation/lookup errors retain `Preparation`; selected overall/token/clock errors retain `ControlFailure` plus already observed failures; total exhaustion uses `AllAttemptsFailed`. No repeated candidate/requery/history/protocol probe/payload replay occurs. Convenience timer policy acquires an explicit system clock/internal never-canceled token when needed; acquisition/read/timer failure is typed control failure, not endless fallback.

### Half release and peer EOF

**Schematic real-loopback workflow, NOT RUN:** acquire a whole stream, capture both numeric endpoint snapshots, then consume it into one read half and one write half. Move the halves to explicit separately managed routines if concurrent directions are needed.

1. Send a bounded request. On positive short writes advance only by confirmed acceptance. Raw TCP flush is a no-op, not a health/ACK test. Explicit `shutdownWrite` can fail; after success it is idempotent and subsequent nonempty writes fail with zero new transfer. Empty primitive writes retain the zero-operation rule.
2. Alternatively, owning raw write-half close attempts local send release and consumes the owner on success/failure. Disposal attempts the same directional release with no recoverable result. Neither joins/stops the owning read routine nor waits for its response handling to finish.
3. The peer can observe EOF on its read direction and send a response. Local read continues; data is delivered before graceful EOF. Peer FIN alone does not close local sending. Reset/error is not `None`/EOF; a zero read returns `Some(0)` without probing EOF.
4. Closing/discarding the read half does not drain incoming data or shut down local send. Final native state/FD release waits for true remaining owners **and** actual native uses, not a selected outcome or half count alone. Required cleanup may suspend.
5. Endpoint getters remain captured numeric metadata, not health/NAT/authentication checks. A listener has local metadata only; its close does not join any already accepted stream/handler.

## Bounded echo, buffering and stdio composition — schematic, NOT RUN

The following workflows are **not complete Rill programs or compiler/runtime/security tests**. They deliberately avoid fake handles, unknown adapter constructors and unapproved stream/stdio conversions.

### One explicit loopback listener and bounded echo handler

- Explicitly construct a loopback socket address with port zero, then acquire one real listener. Observe the assigned local port from the cached getter only after success. No wildcard/default-all-interfaces bind, implicit hostname binding, multi-address atomic bind or handler registry.
- Client connects to that concrete numeric endpoint. Server accepts one owned stream at a time with exclusive listener access. A real application chooses a finite handler count and explicitly joins/stops handlers; backlog 128 is a capped kernel hint, **not** that handler bound.
- Per handler, reuse a caller-owned `Bytes` buffer and request at most a chosen 4096u64 additional bytes per primitive read. Clear/reuse it only after all writes and native borrowers finish. Echo exactly the successful prefix with `writeAll`; its cumulative acceptance does not mean peer delivery. Short positive reads/writes are neither framing nor EOF.
- If a read fails after appending a prefix, retain its original error and exact progress; application policy explicitly decides whether to send that prefix or terminate. If writing fails, do not blindly replay the whole chunk. Treat EOF, reset and selected control failure separately.
- Bound total handler work/bytes/time explicitly where required. A 4096-byte chunk alone does not bound total transfer, all kernel memory, addressability, global tasks or return latency. A no-deadline echo loop can wait forever by design.

### Buffered ownership, handoff and abandonment

- Standard owning `BufferedReader<R>`/`BufferedWriter<W>` have explicit **nonzero logical capacities**, backpressure and no hidden background drain/unbounded queue. NetError meets the scoped noBorrow requirements; generic adapters still need their explicit canonical evidence.
- A buffered reader can read ahead; don't interleave raw reads on its owned underlying half. `intoParts` recovers the underlying owner **and unread/pending bytes** without I/O. Do not discard bytes at a protocol handoff. Reader close discards unread input rather than draining to EOF.
- A buffered writer may accept current-input bytes into its bounded queue while older data drains. Flush drains accepted prefixes and then flushes underlying; distinguish draining-write from underlying-flush failures. Preserve unsent suffixes and original envelopes; no duplicate replay.
- For deliberate request termination, successfully flush the outer buffer before raw `shutdownWrite` if queued output must be handed down. Recover the raw owner only with pending bytes handled explicitly. Consuming buffered close attempts flush and safe underlying close, preserving flush-only/close-only/both failures and consuming the handle regardless.
- **Disposing `BufferedWriter<TcpWriteHalf>` discards its queue, then the owned raw half attempts directional release.** Raw half close/disposal does not flush the enclosing queue, emit TLS shutdown or prove application receipt. Disposal/required cleanup is not a recoverable graceful-protocol operation.
- `readToEnd` needs an explicit maximum additional-byte budget. Its one-byte overflow probe can consume an extra byte and retain simultaneous failure, even with a zero budget; do not use it for exact no-lookahead message framing. Existing destination bytes are not charged as new input.

### Explicit stdio bridge, not owned global-stream handles

- Read bounded chunks with existing `readStdin(maxBytes:u64)`, distinguish `None` from present empty/short bytes, and explicitly write them to the network using canonical I/O. For reverse output, explicitly call `writeStdout`/`writeStderr` on received byte chunks; flush standard output/error explicitly where intended.
- Do not pass global stdio as fabricated `TcpStream`, close a borrowed global stream, or invent a competing adapter/family. Existing convenience stdio writes attempt the complete chunk and retain their older partial-effect/error contract; they do **not** gain `IoFailure` counts from TCP. Never infer a safe whole-chunk retry after their error.
- Text/UTF-8 handling is explicit; byte chunk boundaries can split characters. No automatic line/message framing, Unicode normalization or newline protocol is supplied by TCP/stdio composition.

### TLS, HTTP and database limits

A successful TCP dial authenticates nobody and proves no TLS handshake, hostname identity, HTTP response, database login, delivery, commit, rollback or connection-reuse safety. Preserve the intended hostname separately for TLS identity/SNI and HTTP authority; numeric/canonical resolver metadata is not a substitute. TLS close-notify/verification is [#38](https://github.com/Reilley64/rill/issues/38); HTTP framing/body abandonment/pooling is [#39](https://github.com/Reilley64/rill/issues/39); database protocol/cancel/transaction/reuse policy is [#40](https://github.com/Reilley64/rill/issues/40). None is implemented or security-tested here. Raw half release is not any of those shutdown handshakes. Late effects and buffered/read-ahead leftovers can make reuse unsafe after a typed control error. No automatic reconnect, replay, protocol probe or idempotence inference is authorized.

## Real native evidence matrix — all NOT RUN

Future runs must record OS release/kernel, architecture, libc/resolver details, compiler/runtime/backend revision, exact harness configuration, monotonic-clock source and reproduction logs. Each native scenario must use real acquired resources and actual release observations, not expected-value source or a mock labeled native evidence. Linux/macOS are the initial live-network scope, **not** a passed platform matrix. Standard `wasm32-wasip1` keeps the names typecheck-visible and valid live requests return typed `Unsupported`; no preview2/custom-host/inherited-socket capability is silently assumed. #30 independently gates actual backend/ABI/Wasm support.

| Evidence group | Linux native | macOS native | Real workflow/oracle still required |
| --- | --- | --- | --- |
| Numeric syntax/canonical value semantics | NOT RUN | NOT RUN | Execute pure cases plus independent boundary inventory; verify eq/hash laws without fixed hash constants. Portable/Wasm execution also NOT RUN. |
| Explicit bind/ephemeral port/metadata | NOT RUN | NOT RUN | Real v4 and available v6 loopback; one explicit bind; assigned port; accepted local/remote snapshots; metadata-setup failure owns/closes unpublished resource. |
| EOF/reset/short I/O/backpressure | NOT RUN | NOT RUN | Real paired sockets with bounded peer behavior; short read/write prefixes, data-before-FIN, independent send after peer EOF, reset distinct from EOF; no packet boundary assumptions. |
| Direction release/last owner | NOT RUN | NOT RUN | Real split halves; writer release lets peer respond while reader stays alive; no hidden reader join; final state retained until last actual native use. |
| Explicit close/disposal/FD reuse | NOT RUN | NOT RUN | Stress descriptor reuse/close failure/late completions under private native instrumentation; exactly-once release; no retry against recycled FD, double close, leak or premature borrower release. |
| SIGPIPE/send paths | NOT RUN | NOT RUN | Broken-peer sends through every actual raw/buffered/helper/controlled send path; typed errors/progress without process-global signal changes or fatal SIGPIPE. |
| Options/accepted normalization | NOT RUN | NOT RUN | Observe NODELAY/keepalive/mode application before publication, accepted option normalization, Disabled/SystemDefaults/Custom boundaries, requested-option failure, backlog cap hints/reuse semantics/linger disabled. No packet/latency/security proof. |
| Resolver policy/order/errors | NOT RUN | NOT RUN | Isolated configured native resolver/local sources, explicit inputs, one result batch, family filtering/exact stable dedup, no hidden ADDRCONFIG/mapped synthesis; capture gai plus system codes before migration. No external Internet dependency. |
| Hostname admission | NOT RUN | NOT RUN | Length/grammar/numeric-looking rejection before resolver admission; original accepted bytes forwarded unchanged. Private call tracing proves no fallback for rejected inputs. |
| Scope lookup | NOT RUN | NOT RUN | Real controlled interface-name query, positive result/NUL/overlong/missing name cases; distinguish numeric parser from native query; no interface-name portability promise. |
| Resolver worker retention | NOT RUN | NOT RUN | Delay actual blocking resolver completion; control request cannot free name/native state or occupy scheduler workers; late owned results cleaned. Environment/native lifecycle isolated, not a Rill-only foreign-environment lock. |
| Controlled preflight/zero operations | NOT RUN | NOT RUN | Actual operation-entry/clock/native-registration observations for the precedence table, domain mismatch, requested token, expiry, clock failure, no-time-read path and controlled zero I/O/no-op flush. |
| Manual-clock plus native I/O | NOT RUN | NOT RUN | Use public manual clock/control with real admitted socket work and private barriers; force late read/write/shutdown/accept completion, verify exact final prefixes and nondestructive abort-or-wait. Manual advance alone is not completion proof. |
| Dial timing/slots/winner cleanup | NOT RUN | NOT RUN | Real loopback candidates plus isolated resolver configuration/private coordination; actual-start spacing/no credits, at most two including cleanup, attempt cap vs overall, retained winner, late unused success release and ordered original failures. |
| Error/clock/cleanup metadata | NOT RUN | NOT RUN | Preserve primary Err origin/cause/native code namespaces across migration/cleanup; distinguish native timeout/global/attempt expiry; ordinary cleanup failures don't replace selected outcome. Native ResourceLimit vs fatal Rill allocation is not message-parsed. |
| Intrinsic stop/mandatory cleanup | NOT RUN | NOT RUN | Stop real suspended native users and join cleanup; retain buffers/names/handles until true release, no promised Result/progress return or hard bound. Fatal panic is not a cleanup guarantee. |
| `wasm32-wasip1` unsupported surface | NOT RUN | NOT RUN | Future target compilation/runtime verifies typecheck-visible names, pure values and typed Unsupported live behavior; no fake success/provider or #30 clearance. This is target evidence, not a Linux/macOS live-network run. |

For every applicable native row, both x86-64 and AArch64 evidence remain **NOT RUN**; unavailable host/IPv6/interface capabilities must be recorded as unavailable, not passed or silently skipped. Private fault injection/coordination is useful implementation evidence but does not itself prove every real native race or platform behavior. No tests, installs, network experiments, issues, commits or pushes were performed for this prototype publication.

## Publication/manual-check boundary

Only `prototypes/25-networking.rill` and this inventory are this fixture task's output. Manual review covered explicit imports/types/widths, named records, qualified nominal variants, `copy(&value)`, ordinary mutable bindings, pure-only expectations and the count of 32. Reference targets are supplied by the parent/contract writers and published-main snapshot union; this old checkout alone is not the complete documentation tree. #28 still owes formal checking/adversarial diagnostics; #30 still owes pinned backend/ABI/native/Wasm retention evidence. Design consistency and source-text checks are not parser/typechecker proof, runtime execution, protocol/security validation, hard-latency assurance or accepted RFCs.
