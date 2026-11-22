# Rill v0 networking contracts

This is the human-confirmed design for ri-vno: consolidated Q1-Q92 plus Q93's ordinary-field/stable-snapshot clarification. This is **design approval, not an implemented standard library, executed Rill/native tests, safety proof, target/ABI validation or accepted feature RFC**. [ADR 0023](adr/0023-tcp-addresses-resolution-and-controlled-networking.md) records the decision; [prototype 22](../prototypes/22-networking.rill) and its [inventory](prototypes/22-networking.md) provide hand-checked design fixtures, not a network implementation.

Authoritative prerequisites are ri-fnk's streaming contract (comment 6886715215) and scoped Q36 evidence (ri-fnk, comment 6886862143), ri-j60's full temporal/clock/token contract (comment 6891620139), and ri-1wl's family/conversion refinement (comment 6886119679). See also the [library catalogue](standard-library-contracts.md), [clock/cancellation contract](clock-cancellation-contracts.md), and [date/time contract](time-contracts.md). Closing their design tickets does not satisfy ri-48g's formal/checker obligations or ri-dej's pinned backend/native/Wasm gates.

The [primary-source networking research](research/0034-tcp-dns-platform-and-cancellation.md), including hostname and native-option supplements, supplies facts and future validation cases, **not extra approved APIs, selected providers or proof**. Its historical open-question/prerequisite statements describe the time of research; the confirmed choices here supersede them.

## 1. Scope, modules and signature conventions

`std::net` exports the network-specific types and operations below, with no prelude expansion. TCP connect/listen/accept, numeric IP/socket values, native scope-name lookup, system hostname resolution and batch hostname dialing are v0 scope. UDP, Unix-domain/raw sockets, general DNS-record queries, service-name lookup, TLS, HTTP, database protocols, URL/routing/framing and general FFI are not added here.

Imports remain explicit: `Option` is `std::option::Option`; `Result` and the owned envelope `Err<E>` are `std::result` types; `List` is `std::collection::List`; `Bytes` is the existing `std::bytes::Bytes`. `Clock`, `Instant`, `SignedDuration`, `ClockError` and the domain-check integration export belong to `std::time`; `CancellationToken` belongs to `std::cancel`. Formatting, comparison, hashing, copying, disposal and I/O use their existing canonical families, not unrelated same-spelled local functions.

Blocks marked `rill` show public declaration schemas; signature listings marked `text` are authoritative fixed-arity interfaces, not complete library source or opaque-representation syntax. Repeated specialized signatures describe implementations of one canonical family. Family declarations use `export function`; bodies contributing to a declared family use `implement`, not another standalone `function` body. Ordinary canonical identity, visibility, duplicate, specialization and orphan rules apply. Error types are selected output evidence, not a dispatch input or caller-selectable alternative implementation. This contract introduces no methods, traits, public `dyn`, singleton-string types, unions, optional/named arguments, variadic generics or new field qualifiers.

Q93 controls every public record below. Fields are ordinary public fields. Editing requires an ordinary `let mut` binding/exclusive access under ri-lql (comment 6853088155). There are no per-field `mut`, `readonly` or `const` modifiers. Options and controls are captured value snapshots, not live links to the caller's descriptor. Public sizes/counts use explicit widths; no public `int`, `float`, `byte`, `usize`, `isize`, native descriptor or `sockaddr` layout is exposed.

## 2. Numeric values and pure operations

| Exported nominal type | Representation and properties |
| --- | --- |
| `Ipv4Address` | Private validated 32-bit numeric representation |
| `Ipv6Address` | Private validated 128-bit numeric representation |
| `IpAddress` | Public nominal enum below, not a structural union |
| `SocketAddress` | Opaque numeric family/address/port/applicable IPv6 scope value |

Private representation does not make these exported nominal types unnameable. Here "opaque" describes hidden fields, not ri-owe's genuinely private canonical type identity. All four are implicitly copyable scalar-data values, with justified `noBorrow`, `sendable`, `shareable` evidence. This is not evidence for copying resource owners. Construction, observation, parsing, canonical formatting, equality and hashing are portable and perform **no network, DNS, route or interface query**.

```rill
export enum IpAddress { V4(Ipv4Address), V6(Ipv6Address) }
export enum AddressFamily { Any, V4, V6 }
```

All operations in this listing are public `std::net` exports:

```text
ipv4([u8, u8, u8, u8]): Ipv4Address
ipv6([u16, u16, u16, u16, u16, u16, u16, u16]): Ipv6Address
ipv4Octets(&Ipv4Address): [u8, u8, u8, u8]
ipv6Segments(&Ipv6Address): [u16, u16, u16, u16, u16, u16, u16, u16]
socketAddress(IpAddress, u16): SocketAddress
scopedSocketAddress(Ipv6Address, u16, u32): SocketAddress
ip(&SocketAddress): IpAddress
port(&SocketAddress): u16
scopeId(&SocketAddress): Option<u32>
parseIpv4(&string): Result<Ipv4Address, NetError>
parseIpv6(&string): Result<Ipv6Address, NetError>
parseIp(&string): Result<IpAddress, NetError>
parseSocketAddress(&string): Result<SocketAddress, NetError>
```

Bracketed numeric inputs/outputs above are fixed tuples, not a new array or byte-header syntax. Octets/segments are in textual/network significance order, most significant first, and observers return copied tuples. Every numeric bit pattern is constructible without failure. `socketAddress` sets IPv6 scope to zero; IPv4 has no scope. `scopeId` returns `None` for IPv4 and `Some(id)` for IPv6, including `Some(0)`.

Canonical `std::compare::eq`, `std::hash::hash` and `std::format::show` implementations are supplied for these numeric types. Equality/hash use family and numeric bits, plus port and applicable scope for sockets. IPv4 and its mapped-IPv6 encoding remain distinct; no hidden mapped folding or host identity comparison occurs. Equivalent IPv6 spellings compare equal. A scope index is local, potentially reassigned metadata, not global/persistent interface identity; flow-info is not public endpoint identity. Hashes are not wire formats. There are no implicit string/address conversions or changes to `parseNumber`/`std::convert`.

### 2.1 Exact lexical profile

Parsing consumes the complete borrowed UTF-8 string; malformed text yields `NetErrorKind.InvalidInput`. No trimming, signs where not expressly allowed, prefix acceptance, trailing data, CIDR, hostname/service lookup or resolver fallback is permitted.

- IPv4 has exactly four ASCII decimal octets `0`-`255`, separated by dots. Reject leading zeros except the single digit `0`. Reject abbreviated, integer, octal-looking and hexadecimal forms. Output is ordinary dotted decimal.
- For IPv6, accept RFC 4291 numeric forms, including uppercase hex, legal compression and a strict IPv4 embedded tail. Reject multiple `::`, invalid group counts/ranges, brackets, ports and zones in bare-IP parsers. Output follows RFC 5952 lowercase, no unnecessary leading zeros, longest eligible zero-run compression with the first run winning ties, and no compression of a single zero group. Mapped IPv6 renders with a dotted IPv4 tail; other IPv6 renders in hex. This does not normalize mapped values into IPv4.
- Socket text requires a decimal `u16` port, as `IPv4:port` or `[IPv6]:port`. Reject missing/default ports, unbracketed IPv6, schemes, service names and hostnames. Port leading zeros are decimal and normalize on output. Port zero is valid data and renders as `0`.
- IPv6 socket scope uses a single `%` followed by nonempty ASCII decimal `u32` inside brackets, for example `[fe80::1%3]:443`. Leading zeros are decimal/normalized; scope zero renders without a suffix. Reject signs, overflow, repeated/empty/named zones. No URI percent-decoding: `%25` denotes numeric scope 25, not an escape introducing a zone. Bare `parseIpv6`/`parseIp` reject scope text.

Canonical socket rendering includes the mandatory port and brackets for IPv6. All-zero/unspecified, multicast, limited broadcast, mapped and scoped values remain representable. Construction/parsing does not certify bind/connect usability, current interface existence or reachability.

### 2.2 Native interface scope lookup

```text
scopeIndex(&string): Result<u32, NetError>
scopeIndexWithControl(&string, NetControl): Result<u32, NetError>
```

This is a separate native operation, never a hidden parser action or DNS query. Forward the supplied UTF-8 bytes unchanged; reject NUL and names too long for the supported native interface without truncation, normalization or lossy conversion. It need not represent every arbitrary native byte name. Success is a positive index; native zero is failure, not successful unspecified scope. No reverse lookup, enumeration, raw interface handle or stability promise is supplied.

## 3. Hostname validation and owned resolution

```text
resolveAddresses(&string, u16): Result<List<SocketAddress>, NetError>
resolveAddressesWithFamily(&string, u16, AddressFamily): Result<List<SocketAddress>, NetError>
resolveAddressesWithControl(&string, u16, AddressFamily, NetControl): Result<List<SocketAddress>, NetError>
```

`resolveAddresses` uses `AddressFamily.Any`. The hostname boundary is mandatory, including for `dialHost`; there is no new public hostname wrapper/validator requirement. Ports are numeric `u16`, with zero permitted as resolution data, not outgoing-connect permission.

Accepted lookup-name grammar:

- Names use ASCII letters, digits, hyphen and underscore in nonempty dot-separated labels. Each label is 1-63 bytes and cannot begin or end with hyphen. Single-label and digit-initial names are allowed. Underscore is an intentional local/deployment compatibility allowance, **not** a claim of LDH, SRV, certificate-name or IDNA validity.
- Names may have one terminal dot. The whole text is at most 253 bytes without it or 254 with it. Reject empty/root-only names, interior empty labels, whitespace, NUL, escapes, URI syntax, ports and non-ASCII.
- Conservatively reject input whose components are all numeric-looking: ignore an optional terminal dot for this test only, split on dots, and reject if every component is either ASCII decimal `digit+` or `0x`/`0X` followed by `hex-digit*` (including zero hex digits). Thus `127.1`, integer/octal-looking/hex forms, `0x`, out-of-range numbers and more than four numeric components reject even when not valid IP literals. This deliberately excludes some real local names. Strict literal parsing is the separate numeric path; no nonexistent native hostname-only flag is assumed.

Explicit application IDNA-to-ASCII conversion may precede this API. There is no implicit Unicode/locale mapping or assertion that accepted `xn--` labels are valid IDNA. Preserve accepted case and terminal-dot spelling exactly when passing to the system. Do not lowercase, rewrite the root dot, URI-decode or invent a search-domain policy.

Use supported OS host-resolution policy, configuration, caches and local sources, not a Rill DNS client, DoH service or private cache. Use explicit family hints: no hidden `AI_ADDRCONFIG` filtering, null-hints defaults, mapped synthesis from IPv4, private reachability sorting or route probes. Family selection is not a connectability guarantee.

A successful lookup returns one nonempty owned batch, preserving native family, scope, numeric port and order. Stable first-occurrence deduplication uses exact `SocketAddress` equality. Do not merge IPv4/mapped addresses. A successful native lookup with no usable requested-family candidates reports `NoAddresses`. No partial-success batch on lookup failure, live native-result borrow, incremental subscription, hidden watcher or freshness/all-possible-answer guarantee exists. Blocking resolution must not occupy a scheduler worker. Inputs, job/callback/native result state and registrations remain valid through actual use and release, even for a lookup that cannot be aborted promptly. OS source selection/order can vary between calls.

The caller's intended hostname is distinct from a numeric answer or informational canonical name. Neither resolution nor TCP connection authenticates that identity.

## 4. Creation options and validation

These are public named value records/enums in `std::net`, not live-resource settings, native ABI structs, builders or environment configuration. Ordinary mutable bindings can edit their ordinary fields; each operation uses a passed snapshot. There are no live option setters.

```rill
export enum Ipv6ListenMode { V6Only, DualStack }
export enum KeepAlive {
  Disabled,
  SystemDefaults,
  Custom { idle: SignedDuration; interval: SignedDuration; probeCount: u32 },
}
export type TcpOptions = { noDelay: bool; keepAlive: KeepAlive };
export type ListenOptions = {
  backlog: u32;
  reuseAddress: bool;
  ipv6Mode: Ipv6ListenMode;
  acceptedTcp: TcpOptions;
};
export type DialOptions = {
  family: AddressFamily;
  tcp: TcpOptions;
  attemptDelay: SignedDuration;
  attemptTimeout: Option<SignedDuration>;
};
```

```text
tcpOptions(): TcpOptions
listenOptions(): ListenOptions
dialOptions(): DialOptions
```

| Factory | Exact defaults |
| --- | --- |
| `tcpOptions()` | `noDelay = true`; `keepAlive = KeepAlive.Disabled` |
| `listenOptions()` | `backlog = 128u32`; `reuseAddress = false`; `ipv6Mode = Ipv6ListenMode.V6Only`; `acceptedTcp = tcpOptions()` |
| `dialOptions()` | `family = AddressFamily.Any`; `tcp = tcpOptions()`; `attemptDelay = 250 ms`; `attemptTimeout = Some(10 s)` |

Durations are exact `std::time::SignedDuration` quantities, not floating seconds, calendar `Span`, public primitive aliases or magic infinite values. All known syntactic/range validation occurs before native setup. Apply and normalize applicable requested settings before resource publication, or fail with safe owned cleanup. Do not silently ignore settings, remove timing policy or downgrade dual-stack behavior.

- `backlog` must be positive and fit the target's native signed C `int` argument. Reject zero/out-of-range as `InvalidInput`, never wrap/narrow or use a native-zero convention. The OS may cap this hint; it is not exact effective capacity, reserved future successes or a handler-concurrency limit.
- `reuseAddress` defaults off and opts into `SO_REUSEADDR`-style native behavior when true. It is not `SO_REUSEPORT`, load distribution, atomic reservation or portable exclusivity/security. Wildcard/specific binding and credential/restart/TIME_WAIT rules differ by OS; successful reuse does not guarantee every rebind.
- Explicitly set IPv6 listener mode before binding rather than accepting host defaults. `V6Only` is neutral on IPv4; IPv4 plus `DualStack` is `InvalidInput` before native setup. IPv6 dual-stack failure does not create a second socket or silently become v6-only. Preserve observed mapped peer representation.
- Normalize `noDelay` and keepalive on every outgoing/accepted stream, not by assuming accepted-socket inheritance. NODELAY is latency-oriented, not a packet-boundary, forced-transmission, ACK, flush or bounded-latency promise. Batching is explicit buffering.
- `KeepAlive.SystemDefaults` enables native default probing without inventing timings. `Custom` requires idle and interval each exactly a whole number of seconds 1-32767 inclusive, and `probeCount` 1-127 inclusive. Negative, zero, fractional or out-of-profile values are `InvalidInput`, without rounding/clamping or a clock read. Apply the supported native profile or report failure. Probes are neither an application deadline nor application-health/portable failure-time proof.
- Linger is disabled; no public linger/abortive-close option. Native send paths must suppress SIGPIPE locally and report typed error/progress on every actual sending path. No hidden process-global signal disposition change is authorized. Suppression does not mean send success.

No outgoing local-address/interface binding, unconnected-socket builder, descriptor constructor, handle escape or public provider hook is added.

## 5. TCP owners, public operations and cached metadata

`TcpStream`, `TcpListener`, `TcpReadHalf` and `TcpWriteHalf` are public opaque owned, noncopyable resources. Their justified trusted `noBorrow`, `sendable`, `shareable` properties allow ownership transfer and permitted readonly observations, not overlapping exclusive operations. Private integer fields alone do not justify those properties. No public clone/dup/raw-FD reconstruction or thread-pinning mechanism exists. Ordinary explicit copying is spelled `copy(&value)`; these TCP owners have no supplied copy implementation.

```text
connect(SocketAddress): Result<TcpStream, NetError>
connectWithOptions(SocketAddress, TcpOptions): Result<TcpStream, NetError>
connectWithControl(SocketAddress, TcpOptions, NetControl): Result<TcpStream, NetError>
listen(SocketAddress): Result<TcpListener, NetError>
listenWithOptions(SocketAddress, ListenOptions): Result<TcpListener, NetError>
listenWithControl(SocketAddress, ListenOptions, NetControl): Result<TcpListener, NetError>
accept(&mut TcpListener): Result<TcpStream, NetError>
acceptWithControl(&mut TcpListener, NetControl): Result<TcpStream, NetError>
split(TcpStream): [TcpReadHalf, TcpWriteHalf]
shutdownWrite<S>(&mut S): Result<void, NetError>
shutdownWriteWithControl<S>(&mut S, NetControl): Result<void, NetError>
localAddress<S>(&S): SocketAddress
remoteAddress<S>(&S): SocketAddress
```

The generic family shapes have only these supplied TCP implementations: shutdown on whole stream/write half; local address on all four owners; remote address on whole stream/both halves, not listener. An inapplicable body is a missing capability, not `None`, a runtime lookup or an overloaded return-type choice. Pure split/getters have no control variant. Convenience connect/listen use their default option factories; WithControl always takes explicit options, not stacked `WithOptionsWithControl` suffixes.

Connect attempts exactly one numeric destination with a fresh socket. Reject destination port zero, unspecified IPv4/IPv6, multicast and IPv4 limited broadcast before attempting connection. Apply IPv4 destination exclusions to embedded mapped-IPv4 too, without changing value identity. No route probe, all-subnet-broadcast detection or reachability promise is made. No DNS, candidate fallback, reconnect or payload replay occurs. Native retransmission belongs to the one attempt. Failed/unpublished setup owns and safely reclaims its socket; a failure after native connect may already be peer-visible.

Listen binds exactly one explicit numeric endpoint. Port zero requests an ephemeral bind; observation reports the actual assigned port. Loopback/specific/wildcard exposure is caller choice; there is no default all-interfaces exposure, hostname/multi-bind, silent second-family socket or atomic multi-listener transaction. Partial external effects cannot be rolled back by cleanup.

Accept exclusively borrows one listener, returns one independent stream and normalizes required flags/options/metadata before publishing it. Only one pending accept is allowed per listener. Handlers, loops and routine ownership are explicit. Listener close is not a registry drain, stop or join of already returned streams. Internal would-block waits park cooperatively; interrupted/would-block mechanics are not public `WouldBlock` failures. Substantive connection-local accept errors may be returned without consuming or automatically poisoning the listener. The application chooses retry/backoff/stop; no hidden unbounded recovery loop for persistent/resource/permission failures is promised.

Capture actual numeric local/remote endpoints before publishing streams/halves and the local bound endpoint before publishing a listener. Metadata-setup failure cleans unpublished ownership and reports failure, without undoing observed native connection/binding. Getters are then infallible copied snapshots, no native query/health probe, still usable after peer failure or directional shutdown while the owner exists. Wildcards remain wildcards; these are not NAT/public addresses, authenticated identity or live routing state.

## 6. Canonical I/O, ownership and direction release

There is no network-specific parallel read/write API. TCP supplies `implement` bodies for the canonical `std::io` families:

```rill
// Canonical declarations in std::io, not new std::net families.
export function read<R,E>(source: &mut R, destination: &mut Bytes, maxBytes: u64)
  : Result<Option<u64>, IoFailure<E>>;
export function write<W,E>(destination: &mut W, source: &Bytes)
  : Result<u64, IoFailure<E>>;
export function flush<S,E>(stream: &mut S): Result<void,E>;
export function close<S,E>(stream: S): Result<void,E>;
export type IoFailure<E> = { transferred: u64; error: Err<E> };
```

| TCP owner | Supplied canonical evidence (independent capabilities) |
| --- | --- |
| `TcpStream` | `read(TcpStream,NetError)`, `write(TcpStream,NetError)`, `flush(TcpStream,NetError)`, `close(TcpStream,NetError)` |
| `TcpListener` | `close(TcpListener,NetError)` only |
| `TcpReadHalf` | `read(TcpReadHalf,NetError)`, `close(TcpReadHalf,NetError)` |
| `TcpWriteHalf` | `write(TcpWriteHalf,NetError)`, `flush(TcpWriteHalf,NetError)`, `close(TcpWriteHalf,NetError)` |

The generic boundary needs the named canonical evidence and appropriate explicit properties, such as `display(E)`; it cannot infer capabilities from a later monomorphized body or select an implementation to fit an expected error. Source imports/ordinary orphan rules still apply. Body spelling is, for example, `implement read(source: &mut TcpStream, destination: &mut Bytes, maxBytes: u64): Result<Option<u64>, IoFailure<NetError>> { ... }` as a body schema, not an executable native implementation supplied here.

### 6.1 Progress, EOF and buffering

TCP is an ordered byte stream, not a message protocol. Raw streams have no Rill read-ahead/output queue. `read` appends at most the requested additional bytes to caller-owned `Bytes`, preserving old contents. Positive successful reads report `Some(positive count)`; only drained graceful EOF reports `None`. Short reads/hangup with buffered data are not EOF or message boundaries. Reset/errors are not normalized to EOF. A zero request yields `Some(0)` without consuming/probing EOF; an empty write yields zero without probing health. Nonempty successful write makes positive prefix progress.

`IoFailure<NetError>` retains the **original `Err<NetError>`** and exact prefix: appended bytes for read, accepted current-input bytes for write. Progress is not remote application receipt, durability or permission to replay the whole buffer. Impossible counts or success-with-no-progress for a positive request are fatal adapter bugs, not an infinite helper retry. Raw flush is a successful no-op/layer handoff, not a peer-health probe, packet push or ACK. It can succeed after peer failure. Controlled no-op preflight is qualified below.

`read` exclusively borrows resource and destination; `write` exclusively borrows resource and retains readonly source access. Preserve all borrow dependencies through suspension and real native release. No concurrent reads/writes on one whole stream through overlapping exclusive borrows. Shared inspection cannot manufacture mutable I/O. To run opposite directions concurrently, explicitly consume the whole owner with `split`, yielding owning halves of that same connection. Each half retains exclusive same-direction operation rules. There is no public reunite, clone or implication that arbitrary buffered/TLS wrappers independently parallelize.

Reuse ri-fnk's `readExact`, `writeAll`, bounded `readToEnd`, `BufferedReader`/`BufferedWriter`, typed helper errors, `intoParts`, `BufferedFlushError` and `BufferedCloseError`. No hidden flush in `writeAll`, silent prefix replay or competing byte buffer. `readToEnd`'s mandatory additional-byte budget and retained extra-byte/error probe still apply, including zero budget. Buffer capacities are explicit nonzero `u64`; queues/read-ahead stay bounded, and `intoParts` preserves unread/pending bytes without I/O.

Keep ri-fnk Q36's scoped, not blanket, restrictions: `readToEnd` needs `noBorrow(E)`; buffered-reader read needs `noBorrow` of underlying read error; buffered-writer write/flush need `noBorrow` of underlying write error; buffered consuming close additionally needs `noBorrow` of underlying flush error. Primitive errors, caller-buffer `readExact`/`writeAll`, buffered-reader close and underlying consuming-close error gain no blanket `noBorrow` or extra sendability/sharing requirement. NetError's supplied properties satisfy applicable restrictions without changing generic APIs.

### 6.2 Shutdown, close and disposal

Successful `shutdownWrite` is idempotent. It signals no further application sends but retains ownership/read ability where state permits; nonempty later writes fail with zero new transfer. Empty primitive writes retain their no-probe rule. It does not release the owner, flush an outer buffer, perform TLS/application shutdown or prove peer receipt. Peer FIN does not itself disable local send. There is no public receive/both-direction shutdown API.

Canonical `close` consumes its owner on success or failure. Automatic disposal has no recoverable result. Both must complete mandatory safe release exactly once; neither gives a hard return bound. For a split connection:

- Read-half release does not drain input or close the send direction.
- Raw write-half close/disposal attempts local send-direction shutdown/release. Explicit close can report ordinary teardown failure; disposal cannot return one. This is the specifically approved ri-vno refinement of ri-fnk, not a general graceful-flushing disposal rule.
- Half release finishes its own obligations without joining/waiting for termination of the other owning half/routine. The reader can wait for a reply after the writer closes. Shared native state/last descriptor release waits for the actual last owner and native use; an abort/drop request alone is not release.
- Disposing `BufferedWriter<TcpWriteHalf>` discards pending buffered output, then disposes/releases the raw direction. It **does not flush that queue, TLS close-notify or application framing**. Explicit buffered close follows ri-fnk's flush-then-safe-close policy, preserving both failures when needed.

Secondary cleanup errors do not replace an already selected operation/dial outcome or ordinary primary `Err` metadata. Available native secondary codes/phase context are diagnostic; no new panic/recoverable cleanup result is invented for automatic disposal or losers. Explicit consuming close reports its ordinary teardown error when that is its result. Do not blindly retry native close on a recycled/unowned descriptor after an error. Resource identity, stale events, native borrowers and exactly-once release must be synchronized according to the actual supported OS; integer descriptor identity is insufficient.

## 7. Owned errors and original envelopes

`NetError` is a public opaque owned payload with canonical `display`, `noBorrow`, `sendable`, `shareable` evidence, not a borrowed thread-local message or handle. Public readonly queries are:

```text
kind(&NetError): NetErrorKind
operation(&NetError): NetOperation
message(&NetError): &string
nativeCodes(&NetError): &List<NativeNetCode>
```

Kind/operation are copied values; borrowed message/list queries retain their source dependencies. The public native-code record and nominal enums are:

```rill
export type NativeNetCode = { domain: string; code: i64 };
export enum NetErrorKind {
  InvalidInput, Unsupported, PermissionDenied,
  AddressInUse, AddressNotAvailable, ResourceLimit,
  ConnectionRefused, ConnectionReset, ConnectionAborted, NotConnected,
  WriteClosed, BrokenPipe, NetworkUnreachable, HostUnreachable,
  NameNotFound, NoAddresses, TemporaryNameFailure, NameResolutionFailure,
  NativeTimeout, Other,
  TokenCancelled, DeadlineExpired, ClockFailure,
}
export enum NetOperation {
  ParseIpv4, ParseIpv6, ParseIp, ParseSocketAddress,
  ResolveAddresses, ScopeIndex, Connect, Listen, Accept,
  Read, Write, Flush, ShutdownWrite, Close, DialHost, Progress,
}
export type DialFailure = { address: SocketAddress; error: Err<NetError> };
export enum DialError {
  Preparation(Err<NetError>),
  AllAttemptsFailed(List<DialFailure>),
  ControlFailure { error: Err<NetError>; failures: List<DialFailure> },
}
```

`DialError` is owned with canonical `display`, `noBorrow`, `sendable`, `shareable` evidence. Aggregates preserve original envelopes, origins, causes and operations, not just reconstructed payloads or the last error. `ControlFailure.error` is the selected original control/clock envelope; `failures` contains already observed candidate failures as specified in section 9. These diagnostic aggregates own no live native attempt resources. Ordinary record/list ownership applies; no automatic `Err`/cause copying follows from scalar address copying.

Classify from documented native evidence, never diagnostic text or guessed causes. Capture all relevant native codes before migration/intervening overwrite, including resolver plus underlying system code where applicable. An empty native-code list means no detail was captured. Domain strings/native values are platform-specific, not portable enum meanings. Unknown substantive cases use `Other`. Name-not-found does not prove global absence; temporary failure does not guarantee retry success. `NativeTimeout` means native-reported timeout, **not** application deadline expiry.

WithOptions/WithControl preserve the base public operation identity; private setup phases are context, not new enum variants. Wrapping an original resolution/connect error does not rewrite its operation to DialHost. Pure cached getters and infallible split have no invented failure operations. Explicit clock failures use caused-by wrapping retaining the original `Err<ClockError>`; no automatic error conversion or message parsing is added.

Fatal programmer errors and **Rill/runtime** allocation, reservation, bookkeeping or error-construction failures remain fatal. A documented **native operation's reported** resource shortage, including native memory shortage, may be `ResourceLimit` with native details. This explicitly approved boundary does not add recoverable Rill allocation or guarantee enough memory to report a native failure. Retained erased causes retain ri-8n7's display/noBorrow/sendable/shareable requirements.

## 8. Explicit controls, borrowing adapter and preflight

```rill
export type NetControl = {
  clock: &Clock;
  deadline: Option<Instant>;
  token: &CancellationToken;
};
```

All fields are ordinary public fields under Q93. A descriptor can be edited through ordinary mutable access. Each operation and `ControlledTcp` view captures a stable by-value snapshot, retaining ordinary clock/token/resource borrow dependencies. Editing a separate caller descriptor cannot retarget admitted work; mutating storage conflicting with retained borrows is still rejected. Copying a descriptor does not erase dependencies or secretly own a new clock/token handle. A snapshot fixes the chosen clock/token/deadline, not clock readings or token request state. The captured token still observes later requests. `NetControl` is **not `noBorrow`**, not ambient context and not a permanent attachment to the TCP resource.

The explicit approved ri-j60 integration addition, exported from `std::time`, preserves both consumer-approved names as one canonical operation:

```text
validateInstantDomain(&Clock, Instant): Result<void, ClockError>
checkInstantDomain(&Clock, Instant): Result<void, ClockError> // canonical alias of validateInstantDomain
```

Q81 approved `checkInstantDomain`; the concurrently approved SQL amendment (ri-j60, comment 6892624943) approved `validateInstantDomain` with identical semantics. ri-owe's alias rules preserve both without a second implementation, dispatch route or policy.

It compares domain identity only. A mismatch is `ClockError.DomainMismatch`. It performs no clock reading, timer registration or health check, and matching is not proof that the clock can currently read/wait. There is no raw domain-ID accessor or privacy bypass. Clock failures are retained through the explicit network clock-failure wrapping, not flattened into diagnostic text.

### 8.1 Controlled public forms

Fixed-arity WithControl signatures are listed with their base operations above and in section 9: connect(address, options, control), listen(address, options, control), accept(listener, control), resolveAddresses(name, port, family, control), scopeIndex(name, control), dialHost(name, port, options, control), shutdownWrite(whole/write-half, control), all with the WithControl suffix. No stacked option/control suffixes or implicit optional fields/arguments. Numeric parse, constructors, split and cached observations stay pure/uncontrolled. Mandatory close/dispose is **not externally abortable**.

For canonical I/O, `std::net` additionally exports opaque `ControlledTcp<S>` and:

```text
controlledTcp<S>(&mut S, NetControl): ControlledTcp<S>
```

It is a noncopyable borrowing view, supplied for whole stream/read half/write half. Construction only stores the typechecked exclusive source borrow and captured clock/token/control snapshot. It performs no native I/O, clock read or preflight. Every later I/O operation preflights. Drop releases the view/dependencies, **not the underlying owner**. It supplies precisely:

| Borrowed source | Canonical evidence for the view |
| --- | --- |
| `TcpStream` | `read(ControlledTcp<TcpStream>,NetError)`, `write(ControlledTcp<TcpStream>,NetError)`, `flush(ControlledTcp<TcpStream>,NetError)` |
| `TcpReadHalf` | `read(ControlledTcp<TcpReadHalf>,NetError)` |
| `TcpWriteHalf` | `write(ControlledTcp<TcpWriteHalf>,NetError)`, `flush(ControlledTcp<TcpWriteHalf>,NetError)` |

There is **no canonical `close` capability on the view**, no listener I/O view and no independent copied handle. `read`/`write` retain `IoFailure<NetError>` progress; flush returns `Result<void,NetError>`. The underlying exclusive borrow prevents conflicting operations, split, close or disposal while the view still depends on it. Ordinary dependency/property checking applies; naming a wrapper cannot turn it into a `noBorrow` result.

### 8.2 Admission and selected outcome

For each explicitly controlled operation, including controlled zero-byte read, empty write and raw no-op flush:

1. Validate known ordinary syntax/ranges/options and any present deadline's clock domain first. No priority among unrelated invalid basic arguments is promised. Domain validation uses the identity-only helper, not `now`.
2. If the supplied token is already requested, select `TokenCancelled` **without clock read or native work**. It wins over an otherwise expired deadline/zero-op, not invalid basics/domain.
3. When required for a deadline or internal dial timing, read/check the clock with exact checked arithmetic and supported registration. A reached deadline is `DeadlineExpired`; clock acquisition/read/arithmetic/timer failure is `ClockFailure` with the original clock error retained. Then admit/register native work as needed. With no deadline/internal timers, do not read the clock needlessly.

Uncontrolled primitive zero/no-op behavior remains ri-fnk's unchanged no-probe behavior. Controlled preflight itself can fail even when the requested I/O would otherwise do nothing.

After admission, completion, explicit token request, deadline eligibility and clock failure select once. Simultaneously ready events have no guaranteed winner or universal deadline/token priority. Keep the selected success/failure through abort/completion/unregister/join cleanup; no return-time recheck or post-cleanup reclassification. Typed token cancellation, deadline expiry and clock failure are ordinary recoverable outcomes of explicitly controlled operations, **not intrinsic routine cancellation**.

For I/O, final failure progress is the real final prefix, including late native effects after control selection while required release completes. Preserve selected control kind/original error together with that prefix, not guessed zero or a count sampled before quiescence. No source/buffer/operation/registration storage is released while actually used.

Explicit token/expiry does **not** silently close a borrowed existing stream/listener or its directions to force abortion. Withdraw safely without destructive resource mutation if supported; otherwise wait for actual completion/release. Native I/O or explicit shutdown may commit late; no rollback, hard return bound, read-unconsume, peer-observed-nothing or subsequent protocol-reuse guarantee follows. An unpublished connect/listen/accept result that cannot be returned is owned and safely cleaned up. Intrinsic owner disposal during stopped-routine cleanup is a different obligation from using an external token to destroy a caller's borrowed resource.

### 8.3 Intrinsic cancellation and scoped deadlines

Uncontrolled APIs still obey intrinsic cooperative routine cancellation; they simply have no external application token/overall deadline. A delivered intrinsic cancellation remains **outside `Result`**, returns no progress to stopped code, can leave effects and must join required cleanup with further delivery deferred where required. Runtime masking defers that delivery, not truthful ordinary token observations, and no token can bypass a mandatory release join.

Do not emulate this contract with a blind timer race or a generic `withCancellation` (not in v0). `std::time::withDeadline<F,R>(&Clock,Instant,F):Result<R,DeadlineError>` retains ri-j60's once/sendable evidence and valid scoped borrows. If `R` is `Result<T,E>`, operation completion returns nested `Ok(Result<T,E>)`, including its original failure; it does not flatten into NetError. It selects once, requests cancellation and joins all losing scoped execution/native cleanup before return. Captured external tokens only affect explicitly supplied inner operations; they do not become ambient context or change intrinsic child cancellation into a typed network result.

## 9. Hostname dialing: batch order, timers, slots and errors

```text
dialHost(&string, u16): Result<TcpStream, DialError>
dialHostWithOptions(&string, u16, DialOptions): Result<TcpStream, DialError>
dialHostWithControl(&string, u16, DialOptions, NetControl): Result<TcpStream, DialError>
```

Convenience `dialHost` uses `dialOptions()`. This is a fixed batch-first fallback algorithm, not full RFC 8305 Happy Eyeballs v2 or authentication/protocol negotiation. It sends no application payload, retries no established session and performs no TLS/banner/health probes.

1. Validate hostname/port/options and controlled preflight; outgoing dial port zero is `InvalidInput`, not ephemeral binding or service lookup. Perform one completed system lookup with the requested family, and deduplicate stably as in section 3. Input/ordinary resolution failure yields `DialError.Preparation` preserving the original `Err<NetError>`. Caller control covers DNS as well as attempts. No incremental A/AAAA arrivals, private cache/history or hidden requery.
2. Schedule the first system candidate's family first; alternate IPv4/IPv6 while both remain, preserving within-family order. A mapped IPv6 candidate remains V6. Single-family input retains order. Consider each deduplicated candidate at most once in this call; no numeric sorting, cross-family folding or repeated candidate attempt. Native retry/readiness/retransmit within one attempt is not another candidate.
3. For timing validation, `attemptDelay` must be nonnegative and `Some(attemptTimeout)` must be strictly positive. `None` removes the library per-attempt cap. Defaults are 250 ms spacing and `Some(10 s)` cap, **no implicit overall timeout**. Values/range/arithmetic never wrap, clamp, round to a weaker policy or use wall-clock time.
4. The first attempt starts immediately once preparation permits. If no active attempt remains, the next candidate becomes immediately eligible, subject to a free slot. Otherwise enforce at least `attemptDelay` from the most recent actual start. Do not save timer credits or start catch-up bursts. Delayed starts reset spacing to their actual start. Zero delay permits immediate eligibility, not unbounded parallelism or a forced yield.
5. Allow at most two live attempts, including failed/canceled attempts whose required native cleanup is still in progress. Selected failure/cancel request does not free a slot. Eligibility is distinct from slot availability; cleanup can prevent an otherwise due start. This is per dial, not a global connection, queue, answer-memory or return-latency bound.
6. At an actual start, the effective absolute deadline is the earlier of `now + attemptTimeout` (if present) and the caller's overall deadline (if present), using the same clock domain and checked arithmetic. Attempt caps never extend/reset the caller budget. One supplied absolute overall deadline covers preparation, DNS and all candidates; do not restart it per stage. Native timeout remains a separate native observation.
7. The first observed successful fully prepared candidate wins; simultaneously ready successes have unspecified order. An earlier candidate failure does not defeat another possible success. Per-attempt cap expiry is a candidate failure (`DeadlineExpired`) allowing fallback. Selected overall expiry/token request/clock failure terminates the dial; a clock failure never disables timing or becomes a candidate-local excuse to continue without policy. Once a winner or global terminal outcome is selected, start no new candidate.
8. Request cancellation of all losing work and join all attempt execution, native release and registrations before return. Retain the winner while losers clean up. A late unused success needs exactly one owner and safe release; a cancel request is not cleanup completion. Secondary ordinary cleanup errors do not replace the selected winner/primary failure. Do not reclassify if the clock expires or the token is requested during this cleanup. Selection may be prompt while return remains unbounded.

`AllAttemptsFailed` contains failures from the one-pass attempted candidates when exhausted with no success/global control selection. Entries retain addresses and original error envelopes in original resolution order, not start/interleaved/completion order. `ControlFailure` retains the selected original control/clock `Err<NetError>` plus failures already observed before terminal selection, also in original resolution order. Do not fabricate errors for unstarted candidates, candidates merely stopped for winner/global cleanup or outcomes first discovered only while cleaning those losers. Preserve original per-candidate operations/causes; no last-error flattening.

Ordinary convenience/WithOptions dialing has no external token/overall deadline, but still needs clocks for chosen internal timers. Explicitly obtain `systemClock()` and an internal `neverCanceledToken()` when this policy needs them; no ambient application context. Clock acquisition/use failure is typed `DialError.ControlFailure` with retained clock cause and observed failures, not wall-clock fallback, disabled staggering/caps or deliberate never-ready substitution. Timer-free policy needs no unnecessary clock reading. Supplied ManualClock controls both readings and registrations, not fake readings with real-time delay. Its advancement makes work eligible, not proof that attempts/cleanup have executed.

## 10. Native targets and unsupported boundary

Initial live TCP, system hostname lookup and interface-name lookup are **native Linux/macOS** design scope, subject to pinned implementation evidence. No poller/resolver/provider architecture, OS minimum, C ABI, callback library, Rust/Jiff dependency or public provider-injection interface is selected by this contract. Blocking work must cooperate with scheduling and all native calls/error extraction must honor migration/retention requirements.

ri-7ap's immutable startup Inputs (comment 6887766966) are not live process environment setters or a synchronization guarantee for foreign code. Applications may pass explicitly parsed configuration, but this does not force libc/NSS/resolver internals to consume that snapshot. Document and validate the selected native resolver/environment lifecycle separately; a Rill-only environment lock is not protection against arbitrary foreign mutation. Do not add a new environment-variable configuration layer or assume test Inputs isolate native DNS. Integration fixtures require explicitly isolated native configuration.

Portable numeric types/parsing/formatting remain available on `wasm32-wasip1`. Standard networking names/types stay typecheck-visible there; otherwise-valid live requests fail with typed `NetErrorKind.Unsupported` (retained in the appropriate DialError envelope where applicable), not disappearing names, fabricated results or a fake resolver. Input/control validation is not bypassed by unsupported status. There is no silent preview2/custom-import upgrade or inherited-socket capability handoff. Standard preview1 does not provide general creation/connect/bind/listen/DNS merely because it has operations on an already supplied socket. Existing stdio/filesystem contracts and Wasm backend gates remain unchanged. Any different host-capability/ABI/target support requires a separate explicit decision and evidence.

### 10.1 Whole-stream resumable progress (ri-w0c)

The [progress contract](progress-io-contracts.md) adds opaque `TcpProgress`, opaque `TcpProgressWait` and `tcpProgress(TcpStream):TcpProgress`. This consumes the unsplit owner; no conversion back, raw handle or split progress body exists. Its canonical progress implementations bind every lane/driver error to NetError and wait guard to TcpProgressWait. Failures naturally owned by read/write/flush/close retain those NetOperation values; background wait/job/stop/release uses NetOperation.Progress.

One public owner can privately retain at most one native read job and one native write job at once. This is an explicit private amendment to the earlier whole-stream exclusivity boundary, not public overlapping canonical calls. Same-direction work remains serialized; jobs use stable owned storage, generation-scoped wake/completion, immediate native error capture and mandatory join/release. A live progress wait guard borrows the whole driver and must quiesce before mutation. ri-hw5 must validate this mechanism.

Standard progress writes admit at most16384 bytes; larger input returns unchanged with LimitExceeded. Positive reads return at most min(requested,16384) owned bytes without fill/EOF probing. TCP FIN/reset/prefix/flush semantics remain section6: raw flush is ordered no-op, graceful close needs quiescent lanes, terminal stop can destructively wake jobs but must return truthful outcomes after join. No generic half-close is added.

The progress names remain typecheck-visible on WASI under the existing target rule. There is no live standard TcpStream creation/progress provider, hidden blocking fallback or preview2 upgrade there. The infallible ownership wrapper itself performs no native request; otherwise-valid live operations remain typed Unsupported where reachable. Design closure supplies no target support.

## 11. Narrow refinements and consumer boundaries

- ri-lql/Q93 permits ordinary public fields and binding/exclusive-access mutability only. Captured option/control snapshots are stable, not field-level immutability or live reconfiguration.
- ri-fnk's canonical I/O, actual prefix/original envelope retention and scoped Q36 evidence remain. Only raw TCP write-half release gains the explicitly approved local send-release disposal action; it does not authorize flushing buffered/TLS/application output during disposal. Historical "cancellation outside Result" means **intrinsic routine cancellation**, not these explicit token outcomes.
- Under ri-j60, explicitly preserve `checkInstantDomain`/`validateInstantDomain` as canonical aliases of one identity-only export; preserve suspend-inclusive system-clock profile, domain-tagged checked instants, ManualClock registrations, typed clock failures, ordinary tokens and nested/joining `withDeadline`. No generic token scope or hard deadline is inferred.
- Under ri-8n7/#29, explicitly distinguish fatal managed/runtime allocation from reported native-operation resource shortage. Preserve original Err origins/causes and erased-cause evidence; no recoverable managed allocation or public error downcast is added.
- ri-1wl's item-site exports, `implement` family bodies and exact selected error evidence apply. Parsing/formatting is separately named domain behavior, not implicit conversion/serialization.
- For ri-irw TLS, the caller's intended hostname, numeric endpoint, TCP success and authenticated peer identity remain distinct. TCP EOF is not TLS close-notify; raw shutdown/disposal is not TLS shutdown. No provider, public raw-handle bridge, thread pinning or independent TLS directional concurrency is promised.
- The [ri-n5i HTTP contract](http-contracts.md) consumes whole `TcpProgress` through one explicit connection runner. Preserve prefix/EOF/framing distinctions,16KiB lower units, bounded buffering and explicit control dependencies. Canceled transport or selected deadline does not establish request nonexecution, replay safety, complete body consumption or reusable connection state. HTTP close-delimited completion uses real drained FIN; reset stays failure.
- For ri-dka database consumers, transport cancellation/close cannot prove rollback, command nonexecution, protocol resynchronization or pool reuse. Protocol-specific cancellation/upgrade buffering/unknown outcomes remain that ticket/driver's design, not a standard DB provider or raw-FD entitlement.

These integration requirements refine only the stated interfaces; they do not reopen unrelated closed foundation decisions or declare consumer/provider implementation complete.

## 12. Evidence inventory and unpassed gates

**All Rill execution, native networking acceptance, compiler/checker proof and backend validation below are NOT RUN / NOT ESTABLISHED.** Hand-checked fixtures, static document checks and primary-source reading are not substitutes. Normal source fixtures must not make live networking/DNS success an intended-passing assumption or expose fake standard providers. Native/private harness instrumentation is separate from the public API.

| Required future evidence | Cases to retain; current status |
| --- | --- |
| Pure values/text | Test strict IPv4/IPv6 and embedded tails; malformed compression/ranges/trailing data; canonical eq/hash/round trips; mandatory ports; decimal scope/port normalization; mapped distinction; every hostname boundary, numeric-looking guard and unchanged forwarding. NOT RUN. |
| Config/preflight | Test defaults/ranges, the whole-second keepalive profile, C-int backlog, v4+DualStack rejection and requested-options-or-fail. Check invalid basics/domain before an already-requested token; token handling without a read; expired/zero/no-op control; and no needless clock reads. NOT RUN. |
| Ownership/generic evidence | Test moves/split/noncopy owners, same-direction exclusivity, permitted readonly sharing, invalid escaped view/control dependencies, no view close, scoped Q36 buffering evidence, exact selected E and preserved envelopes. NOT RUN; ri-48g retains the proof/checker gate. |
| I/O/EOF/backpressure | Test short/one-byte/zero I/O, destination append/prefix+error, buffered data before FIN/hangup, reset distinct from EOF, sends after peer FIN/local shutdown, idempotent shutdown, no raw-flush health promise, bounded queues and unsent suffixes. NOT RUN. |
| Native connect/accept/metadata | Test immediate/late success/error/readiness races, would-block after readiness, accepted flags/options, one-owner unpublished sockets, metadata setup failure, substantive accept failure without listener poisoning and active handlers surviving listener release. NOT RUN. |
| Dial/manual clock | Test family interleaving and resolution-order diagnostics; no requery/retry; actual-start spacing without credits; immediate eligibility when no attempt is active, with two live cleanup slots; per-attempt versus overall DNS-inclusive budgets; simultaneous winners/control; clock failure; retained winner/late unused success; and no post-cleanup reclassification. NOT RUN. |
| Resolver/scope/native errors | Test real configured DNS/local sources, delayed/queued/executing unabortable lookup, family filters without hidden ADDRCONFIG or synthesis, native resolver+system codes captured before migration, NUL/overlong scope names and positive-index/zero-failure behavior. NOT RUN. |
| Cancellation/retention | Test token versus intrinsic cancellation/masking, native completion after selected control, actual final late prefixes, nondestructive abort or wait on borrowed resources, unregister/callback quiescence and required joins without a hard return bound. NOT RUN; ri-48g/#30 retain their gates. |
| Teardown/descriptor reuse | Test half release without joining the peer owner, last-owner/native-use retention, buffered-write discard then raw direction release, close failure/interruption and recycled descriptor/stale event adversaries. Check that there is no blind retry/double release/leak or outcome replacement. NOT RUN. |
| Platform options/signals | Pin Linux/macOS kernel/libc/SDK/header modes. Test NODELAY/keepalive accepted normalization, actual option support, reuse wildcard/specific/credential/TIME_WAIT rules, v6-only/dual-stack mapped peers, backlog caps, disabled linger and all send paths' local SIGPIPE suppression. NOT RUN. |
| Backend/Wasm boundary | Test typed Unsupported with a stable public interface; actual host imports/capabilities; guest pointer stability across growth/reentrancy; native state retention and bounded cleanup traversal; and the pinned LLVM/native/Wasm pipeline. NOT RUN; ri-dej retains the support gate. |
| Protocol composition | Test actual TLS/provider bidirectional retries/errors/close-notify, the [HTTP contract](http-contracts.md) abandonment/framing/reuse over plain TCP and DB upgrade/cancel/unknown outcome. Do not infer security/rollback/replay from TCP. NOT RUN; ri-19d/consumer gates remain. |

ri-48g must formalize pre-monomorphization family/property/dependency evidence, borrowed controls/views and result escape, exact progress/envelope rebasing, snapshot admission, one-selection/late-completion accounting, exactly-once ownership and cancellation-masked joins, and bounded input-dependent standard traversal/default disposal. ri-dej must validate actual native/backend/ABI/Wasm mechanisms and supported configurations; research mechanism names do not clear it. Unworkable required rules return to map ri-kl0, not an invented safety claim or silently weakened policy. ri-3t8 owns compiler diagnostics; design closure does not accept future RFCs.

### Approval trail

Round approvals are retained for provenance, not 23 competing contracts. Later consolidated approval and Q93 control over historical pending-prerequisite/field-mutability wording:

- Q1-Q20: 1-4 (ri-vno, comment 6886621226), 5-8 (ri-vno, comment 6886739803), 9-12 (ri-vno, comment 6886829633), 13-16 (ri-vno, comment 6886982455), 17-20 (ri-vno, comment 6887163242).
- Q21-Q40: 21-24 (ri-vno, comment 6887368524), 25-28 (ri-vno, comment 6887522127), 29-32 (ri-vno, comment 6887657305), 33-36 (ri-vno, comment 6888381711), 37-40 (ri-vno, comment 6888612726).
- Q41-Q60: 41-44 (ri-vno, comment 6889126720), 45-48 (ri-vno, comment 6889456385), 49-52 (ri-vno, comment 6889626813), 53-56 (ri-vno, comment 6889755376), 57-60 (ri-vno, comment 6889994191).
- Q61-Q76: 61-64 (ri-vno, comment 6890081050), 65-68 (ri-vno, comment 6890187386), 69-72 (ri-vno, comment 6890335184), 73-76 (ri-vno, comment 6890422807).
- Q77-Q92: 77-80 (ri-vno, comment 6891967801), 81-84 (ri-vno, comment 6892095559), 85-88 (ri-vno, comment 6892179390), 89-92 (ri-vno, comment 6892205064). The human subsequently confirmed consolidation and then Q93's ordinary public fields/stable captured snapshots (ri-vno, comment 6892447736). Q93 supersedes literal per-field-mutability/immutable-field wording, not the retained-borrow or captured-control guarantees.
