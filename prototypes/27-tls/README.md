# Prototype 27: TLS values and owned composition

This fixture records the **explicitly consolidated, human-approved Rill [#38](https://github.com/Reilley64/rill/issues/38) Q1–Q80** contract. The parent-owned [TLS contract](../../docs/tls-contracts.md), [private native boundary](../../docs/tls-native-boundary.md) and [ADR 0025](../../docs/adr/0025-explicit-tls-trust-and-private-provider-boundary.md) are authoritative; these examples do not add APIs. The earlier round files and consistency review were interview checkpoints, not independent implementation approvals.

**Design fixtures only. No Rill compiler/runtime exists here. All Rill, native, TLS, cryptographic and security validation is NOT RUN.** `values.rill` declares **32 top-level intended-valid expectations**; counting them is not executing them. The two secure composition helpers are uncalled. There are no real certificates, native resources, handshakes or public test/provider backends in this directory.

[#44](https://github.com/Reilley64/rill/issues/44) gates actual TLS delivery and validation, not further RFC drafting. [#30](https://github.com/Reilley64/rill/issues/30) and [#43](https://github.com/Reilley64/rill/issues/43) remain **OPEN**. #30's current approved portable direction is compiler-owned activation records plus a nonrecursive dispatcher, **not the old Asyncify proposal**; full checker, typed-owned ABI, executor/leases, cleanup and target support remain unpassed. #43's separately recorded SQLite design and standalone evidence neither implement TLS nor authorize general FFI.

## Source coverage and deliberate limits

| Expectations in source order | Intended assertion, NOT RUN |
| --- | --- |
| 1–5 | DNS ASCII lowercase, removal of exactly one optional terminal root dot, single-label admission, digit-initial nonnumeric name and opaque `xn--` label admission. **No IDNA/Punycode/Unicode validation claim.** |
| 6–9 | Canonical DNS equality/hash; independent owned result and explicit copy survive local source/owner scope; `dnsName` borrowed observation versus absent `ipAddress`. No new `display(ServerName)` or `show(ServerName)` promise. |
| 10–12 | Copied IP getter, absent DNS getter, copy/eq/hash laws; IPv4 and mapped IPv6 stay distinct by family/bits, not host-equivalence heuristics. No hash collision-freedom assertion. |
| 13–21 | `TlsErrorKind.InvalidInput` for empty/root-only/NUL/underscore/wildcard/Unicode/interior or excess dots/edge hyphens/whitespace/decorations/numeric-looking input. No opaque error construction, message parsing or invented operation enum. |
| 22–23 | Label length 63 accepted/64 rejected; canonical total 253 accepted, optional root dot accepted, canonical 254 rejected. |
| 24–28 | `alpnId(Bytes(List<u8>))`: 1–255 opaque bytes, including NUL and nontext; reject 0/256; borrowed canonical `std::bytes::length/get`; explicit owned copy, byte equality and equal-key hashes. |
| 29–32 | Pure options factories default to Modern/Disabled, Auto client SNI, no credentials/client auth. Ordinary mutable binding edits before consumption; **no copy(Options) capability promised**. |

The DNS numeric-looking guard is exactly #34's conservative **all-components** rule: after removing the optional terminal dot, every component being ASCII decimal `digit+` or `0x`/`0X` plus `hex-digit*` (including no hex digits) rejects. Thus `123`, `127.1`, `999.999.999.999`, `1.2.3.4.5`, `0x` and `0Xf.0x.010.` reject, while `123.example` is admitted. This is not an IP parser. TLS DNS syntax is stricter than the resolver's: underscores reject. Reference names are not resolver canonical names, connected endpoints, HTTP authorities or authorization decisions.

Canonical value interfaces are `dnsServerName(&string) -> Result<ServerName,TlsError>`, `ipServerName(IpAddress) -> ServerName`, `dnsName(&ServerName) -> Option<&string>`, `ipAddress(&ServerName) -> Option<IpAddress>`, `alpnId(Bytes) -> Result<AlpnId,TlsError>`, and `alpnBytes(&AlpnId) -> &Bytes`. Owned names/ALPN have explicit copy/eq/hash; borrowed getters keep their ordinary source dependencies. No text conversion is needed for opaque ALPN.

### Independent rejection inventory — NOT RUN

These are future adversarial cases, not deliberately invalid declarations inside `values.rill`. Static rejection requires a future checker; typed input/policy rejection requires an implemented operation, not just Python agreement.

| Case | Required boundary |
| --- | --- |
| Construct `ServerName`, `AlpnId`, credentials/config/stream or `TlsError` from guessed fields, integers or native pointers | Opaque representation is not a public constructor/ABI. Use approved value factories or actually acquired resources. |
| Return `dnsName`/`alpnBytes`/cached metadata borrowed from a local destroyed owner; mutate/dispose that owner before last use | Reject dangling/transitive dependencies. An owned copy can escape; a borrowed getter cannot invent ownership. |
| `copy(&tlsStream)`, use TCP after handshake consumption, reuse a config without explicit copy, or use options after consumption | Reject missing copy/use-after-move. Snapshot copy is only the explicit resource capability, not implicit aliasing or an options-copy promise. |
| Overlap whole TLS read/write/flush, use underlying S while owned, close a controlled view, or return a view borrowing a local stream/control target | Reject exclusive/dependency/capability violation; the borrowing view has no consuming close. |
| Qualify transport by namesake I/O, expected common error, or just one retained error's properties | Require canonical independently selected RE/WE/FE/CE and each error's complete retained-cause evidence. Do not repair it by globally demanding noBorrow(S). |
| Mutate immutable binding or add per-field mutability, methods/traits/public dyn, literal types, fixed arrays or fallback-width primitives | Existing source rules only; no extra language surface. Address constructor brackets here are approved tuples, byte lists have explicit u8 context. |
| Wrong-purpose anchors, empty/partial material, leaf/intermediate anchors, conflicting same-subject/key anchors or competing CRLs | Typed config/load failure, no partial snapshot or implicit trust/coverage choice. Byte-identical material may coalesce; it does not evade pre-dedup work admission. |
| Empty/duplicate/257-ID configured ALPN list or wire sum 65,536; CRL age zero/negative/fractional | Typed validation failure at the relevant boundary, not silent trimming/dedup/rounding. Constructing an options record alone does not validate its future config. |
| Attempt insecure verifier, ambient roots, custom provider callbacks, alternate SNI identity, TLS split or raw transport recovery | No such public API. This fixture does not invent a public test backend to exercise failures. |

## Options, resources and uncalled secure helpers

The public ordinary records are exactly:

- `ClientOptions { profile: TlsProfile, sni: SniMode, alpn: AlpnPolicy, credentials: Option<Credentials> }`.
- `ServerOptions { profile: TlsProfile, alpn: AlpnPolicy, clientAuth: ClientAuth }`.
- Profiles `Modern` / `Compatible`; SNI `Auto` / `Omit`; ALPN `Disabled` / `Optional(List<AlpnId>)` / `Required(List<AlpnId>)`; auth `None` / `Required { anchors: TrustAnchors, crls: CrlSnapshot }`.

`clientOptions()` / `serverOptions()` are pure. Config factories consume owned inputs: `clientConfig(TrustAnchors,CrlSnapshot)`, `clientConfigWithOptions(TrustAnchors,CrlSnapshot,ClientOptions)`, `serverConfig(Credentials)` and `serverConfigWithOptions(Credentials,ServerOptions)`, all returning their config in `Result<...,TlsError>`. No builder, optional argument, field-level modifier, ambient trust factory or in-place refresh is added.

`secureClient` and `secureServer` take **already connected `TcpStream` owners**, owned PEM `Bytes`, a `SignedDuration` maximum CRL age and explicit `NetControl`. The client additionally takes the intended validated `ServerName`. They load purpose-specific anchors, CRLs and local credentials, construct configs, then consume the connected stream in `clientHandshakeWithControl` / `serverHandshakeWithControl`, with explicit empty prefix `Bytes([])`. Exact-`TlsError` postfix `?` forwards the original unique failure envelope without synthetic conversions. A loader/config failure disposes the still-owned TCP stream; successful preparation is not proof it will authenticate.

Both helpers require the same ALPN `http/1.1`, encoded as `[104u8,116u8,116u8,112u8,47u8,49u8,46u8,49u8]`. This is an ALPN policy example, **not an HTTP implementation**. The server requires verified client authentication; the client supplies credentials, sent only on request. Correct SANs, roles, complete root-inclusive CRLs and fresh material must actually be provisioned; none is asserted to exist. A certificate presented to a peer may already have been disclosed before later cancellation/failure.

Credentials/configs/trust anchors/CRL snapshots are owned, non-implicitly-copying resources. Their explicit `copy(&resource)` retains the **same logical immutable snapshot**, not a reparse, secret duplication promise, refresh or new trust generation. Actual provider work must retain the backing resource, not merely a public wrapper. New configs do not rewrite old configs or established channels; applications retire them explicitly. There is no background refresh or process-wide antirollback guarantee.

### Networking application composition — schematic, NOT RUN

1. Preserve the intended service name separately from resolution/routing. Explicitly dial with #34 networking or accept from an explicit listener. A successful TCP result authenticates nobody.
2. Keep `NetError`/`DialError` and `TlsError` distinct. Application code must choose an explicit application error envelope or handle these phases separately; this prototype invents **no NetError-to-TlsError converter**. Reuse one absolute control budget across dialing/acceptance and TLS where applicable, not a renewed per-phase timeout. Material loading/config construction has no invented controlled variant.
3. Pass the already connected owner into the relevant secure helper. For plaintext-to-TLS upgrade, finish application negotiation, flush plaintext output and recover underlying ownership **plus every unread suffix** via the existing buffering handoff. Use the handshake's explicit owned input-prefix form instead of these helpers' empty prefix. Feed that prefix in order before any underlying read; never discard/replay/reinterpret it as plaintext. Prefix maximum is 64 KiB.
4. Publish `TlsStream<S>` only after authentication, negotiated-policy checks and final handshake ciphertext write/flush handoff. Provider “done,” TCP connection and peer application receipt are different events. Any handshake failure consumes/releases the transport with no plaintext recovery or partial-authenticated stream; failed pending TLS output is discarded, not gracefully flushed.
5. Use canonical I/O with one exclusive TLS driver, explicit framing/budgets and deliberate close. No TLS split/reunite, independent application read/write drivers, raw TCP split workaround, mutable underlying getter or downgrade/`intoParts` escape hatch. Simultaneous duplex HTTP/DB designs require a later interface decision in their own scope; ALPN does not settle #39.

## Core security policy boundaries — all implementation evidence NOT RUN

- **Trust is explicit:** app-supplied cryptographically self-signed CA roots, no bundled or OS roots, default provider trust paths, intermediate/leaf anchors, partial-chain shortcuts or AUX trust overrides. Loading is an explicit grant for one purpose, not browser/OS policy equivalence. Strong root keys remain mandatory; the SHA1 exception is only an explicit root self-signature, never path/CRL signatures; MD5 is excluded.
- **Manual strict offline CRLs:** complete direct-issuer base CRLs signed by the actual issuing CA with explicit `cRLSign`, applicable scope and supported restrictions. Cover **every certificate in the actual built path, including the self-signed root**. Missing/stale/invalid/ambiguous/unsupported coverage fails closed; serial absence alone proves nothing without complete valid coverage. No hidden AIA/CRL/OCSP/native-trust network fetch, soft fail, OCSP stapling, delta/indirect/alternate-key/reason-partition fallback, or ignored must-staple/noRevAvail requirement. Root revocation evidence does not repair a compromised trust grant. Some ordinary public chains cannot satisfy this profile.
- **Authentication:** SAN mandatory, no CN fallback. DNS matching only a whole leftmost wildcard over exactly one label, with at least two valid suffix labels (`*.example.test`, not `*.test`); IP matches exact family-specific IP SAN bytes, not DNS text/scope/port. No PSL, issuance or account/role authorization guarantee. Server SNI must be well-formed and covered by the single configured credential; absent SNI is allowed. Auto SNI uses canonical DNS only, IP sends none, and explicit omission never disables identity verification. No alternate SNI hostname/domain-fronting API.
- **Local credential admission:** static key/leaf/role/material checks at construction and fresh validity at handshake admission, even for an optional client credential that is never requested. This does **not** certify its own chain/trust/revocation under anchors meant to verify the opposite role. The peer verifies its actual path independently. Required leaf digitalSignature KU and explicit role EKU, non-CA leaf, strict CA/path/name/critical-extension checks remain mandatory.
- **Checked time:** fresh checked wall time at the peer verification phase, floor nanoseconds toward negative infinity to whole seconds, check native/time_t/calendar representability. Certificate validity is `[notBefore,notAfter)`, deliberately excluding the expiry instant, with no skew or disabled checks. No frozen config “now,” unchecked libc fallback or monotonic-clock substitution. CRL maximum age must be positive, exact whole seconds; freshness is `[thisUpdate,min(nextUpdate,thisUpdate+maxAge))`, with required ordered endpoints and the same verification wall basis. Snapshot loading does not freeze freshness.
- **Protocol:** Modern defaults to TLS1.3 only; explicit Compatible allows the reviewed TLS1.2+1.3 profile, requiring TLS1.2 EMS and secure-renegotiation signaling in both roles before publication, while renegotiation itself is disabled. No SSL/old TLS, automatic weaker retry, resumption/tickets/0-RTT/session API, compression, post-handshake auth, arbitrary ciphers/provider flags or verification bypass. TLS1.2 has **no directional half-close API**; TLS1.3 alone permits explicit shutdown-write.
- **ALPN:** configured lists are nonempty and unique, at most 256 IDs and at most 65,535 wire bytes including each length byte. Peer count is capped before dedup; repetitions do not change server preference. Required absence fails; Optional absence can succeed but a negotiated no-overlap fails. Reject unoffered selections. No application fallback inferred from absence.
- **Encoding and input ownership:** complete DER consumption and strict supported PEM blocks (certificate, CRL, unencrypted PKCS8) with only space/tab/CR/LF outside, no bag attributes, junk, duplicate extensions or permissive-decoder ambiguity. Leaf-first local chain/key match; no password callbacks, encrypted/legacy key wrappers, hardware hooks or arbitrary provider loading. Caller input is consumed, not recovered on failure. Ordinary input Bytes are not universally secret-tracked/zeroized.

### Generic I/O evidence and typed progress

Generic owning transport eligibility independently requires canonical `read(S,RE)`, `write(S,WE)`, `flush(S,FE)`, `close(S,CE)`, with selected errors determined by input patterns, **not a desired common E**. Each of **RE, WE, FE and CE independently** needs `display`, `noBorrow`, `sendable`, `shareable`, because TLS may retain the original error as a unique heterogeneous cause. Do not require them equal or impose blanket `noBorrow(S)` or extend these scoped requirements to primitive I/O. A borrowed-error adapter must explicitly supply eligible owned errors.

`TlsStream<S>` is noncopyable; noBorrow/transfer/sharing depend on the actual S properties and safe retained implementation, not pointer opacity. Ownership does not erase embedded transport loans. TLS canonical read/write return `IoFailure<TlsError>`; flush/consuming close return `Result<void,TlsError>`, preserving 3/2/1/1 argument arities. `controlledTls(&mut stream,NetControl)` stores a borrowing view, no preflight until I/O, and supplies read/write/flush but **no consuming close**.

- Reads append authenticated plaintext only. Positive success is a positive prefix, `None` only after authenticated `close_notify` and drained plaintext. TCP EOF without it is `Truncated`, never clean EOF. A known terminal failure on the last drained authenticated prefix returns failure with its actual appended count; pending authenticated tail is never discarded merely to report early.
- Writes count **current-input plaintext accepted into bounded TLS-owned storage**, not ciphertext, peer receipt or application execution. Nonempty success is short-positive, never backpressure `Ok(0)`. Failure counts real accepted prefix, including late acceptance after selection. Do not replay the whole input or report older buffered ciphertext as new acceptance. Native retries retain stable owned backing/identity/length through actual use.
- Flush drives accepted output and required opposite-direction control traffic through underlying flush, retaining encountered authenticated input within bounds. It proves handoff only, not health/ACK/durability. No background unbounded drain or discarded authenticated data to unblock progress.
- Zero read/empty write are non-health observations; controlled calls still preflight. Control capture retains dependencies and one absolute budget. Validate ordinary inputs/domain first, then already requested token, then needed clock/expiry, then admission. No needless clock read with no deadline, no ambient control, no wall-time replacement for elapsed deadlines. Explicit token/expiry/clock failure is typed; intrinsic routine cancellation remains outside Result.
- Once admitted, select one primary outcome and preserve it and the exact final plaintext prefix through actual completion/confirmed abort/unregister/release. Cancel request is not quiescence; do not free active buffers or force-close a borrowed lower operation. Deadlines do not bound cleanup/return time, and successful selection is not replaced by a later timeout during mandatory release.

### Closed, poisoned and unique-error traces — schematic, NOT RUN

| State/event | Required behavior |
| --- | --- |
| Healthy TLS1.3 successful shutdown-write | Flush accepted output and local close-notify handoff, no peer-notify wait/raw TCP shutdown. Later nonempty write is `WriteClosed`; read continues. Repeated successful shutdown is idempotent subject to preflight. Empty-output flush succeeds without probing health. |
| Healthy authenticated inbound close | Drain queued authenticated plaintext before `None`. TLS1.3 sending can continue. TLS1.2 peer notify ends both application directions, discards pending writes and pumps its required response. TLS1.2 local shutdown-write rejects Unsupported before admission. |
| Pre-admission rejection | Borrowed healthy owner stays healthy. Controlled rejection does not steal pending error/tail. A consuming close still releases its owner even if preflight rejects. |
| Admitted protocol/provider/transport failure or cancellation abandonment | Poison; no protocol resume/new nonempty writes/flush pumping/health reentry. Already authenticated tail, zero observations and cached negotiated metadata remain usable under the contract. |
| Consuming close | Healthy local graceful flush/notify/handoff, then underlying close without peer-notify wait. Preserve primary graceful failure and discard remaining output before safe disposal. Poisoned close skips grace, reports failed state and releases. Owner is gone on either outcome. |
| Ordinary disposal | Discard output, no notify/flush/peer drain; peer may see truncation. Mandatory jobs/callback unregister/release still join. No immediate destructor, hard deadline or fatal-panic cleanup guarantee. |

**Trace A — original error still pending.** A private implementation has authenticated 7 bytes and learned truncation with one original owned error. Read at most 3 appends 3, leaving 4 and that pending error. A controlled read rejected before admission appends 0 and reports its control error; it changes neither tail nor pending original. Next admitted read of 4 appends the final 4 and returns `IoFailure<TlsError>` with `transferred=4` and the **original** truncation envelope/cause. A later positive read returns a new `AlreadyFailed`, not a copy of that cause. A zero read never consumes it.

**Trace B — original error already returned.** Write/flush fails and returns its unique original envelope once; 5 earlier authenticated bytes remain in the poisoned stream. Read of 2 may drain 2 without protocol work. A read draining the final 3 reports the known failed state with its real prefix and a **new `AlreadyFailed` envelope**, not cloned original cause; later positive operations likewise report failed state without native reentry. Cached version/cipher/ALPN/authenticated-chain/reference-name/SNI queries are observations, not revalidation. A controlled rejection during the drain leaves that tail intact. Logical poison never authorizes freeing still-active lower I/O.

TLS-owned diagnostics are bounded/owned/redacted: safe categories, available phase/operation/depth/input index, at most 4,096 UTF-8 summary bytes and 16 native numeric codes with explicit truncation. Do not invent unavailable precision, parse messages for portable branching, retain provider pointers, automatically dump material/secrets or duplicate original transport/clock causes. Foreign cause payloads/formatters are not magically capped by the TLS summary limit.

## Fixed limits and known source-to-native gaps

These are checked logical admission ceilings, **not proven global native memory/CPU/stack/work bounds**. All public capacity/count arithmetic is checked `u64` with target/native representability checks; reject rather than truncate or silently skip validation.

| Material/work | Fixed v0 ceiling |
| --- | --- |
| Certificate / decoded key / RSA modulus | 16 KiB / 64 KiB / 2,048–8,192 bits |
| Supplied local/peer chain and actual built path / decoded chain bytes | 10 certificates each (built path includes leaf/root) / 128 KiB |
| Anchors | 1,024 certificates / 16 MiB aggregate |
| CRLs | 1,024 / 16 MiB each / 64 MiB aggregate; 100,000 entries each / 1,000,000 aggregate |
| Raw per load | `2 * relevant decoded ceiling + 4096` bytes, charged before dedup |
| Pending plain-in/plain-out/cipher-in/cipher-out | 64 KiB **each**; initial prefix charged to ingress |
| Handshake-message reassembly | 256 KiB, plus stricter chain bound |
| Generated + processed records | `2^20` combined per session, all epochs; no KeyUpdate reset |
| Incoming records without app plaintext progress | 1,024 per operation |

Backpressure is not itself failure; if required progress cannot fit approved bounds, return typed limit failure/poison rather than discard authenticated data. CRL conflicts, anchor conflicts, complete parsing, actual path search and material quotas need native evidence beyond arithmetic. Required peer KeyUpdates remain within budgets; no public rekey control. Reconnect for long/high-volume sessions, not unproved automatic key rotation.

Provider direction is a pinned, privately linked Rill-controlled OpenSSL **3.5 LTS** build, inspected 3.5.9 contract baseline with reviewed security maintenance, not an installed system-library fallback or frozen forever version. Effective closed cipher/group/signature inventories, standardized hybrid X25519MLKEM768 behavior, signature_algorithms_cert/current RFC obligations, per-key/epoch limits, strict DER/KU/EKU/CRL/root enforcement, entropy seeding/reseed, residual host-loader/CPU-cap inputs and symbol isolation/distribution all remain acceptance gates. If the maintained provider cannot meet the contract, return to provider decision instead of quietly shrinking the profile. Builtin DRBG with reviewed secure OS getrandom/getentropy seeding is distinct from assuming #37's secure-byte API proves TLS.

The private bridge must use serialized engine steps/native-owned stable buffers, managed transport pumping outside C, joined potentially blocking work and same-OS-thread native operation/error classification/capture before migration. No Rill transport/verifier callbacks, suspended managed foreign frames, expired caller pointers, public native handles or general FFI. The versioned toolchain-owned operation/data/ownership contract must integrate executor/leases/wakes, atomic offer adoption or loser release, retries, native-only callbacks and true quiescence. A header/layout declaration or source inspection is not an implemented binding. No flag/libctx is claimed to sandbox every environment/loader input; private build closure, deployment assumptions, advisories/notices and rebuild/redeployment ownership require audit.

## Native target matrix — ALL NOT RUN

Every row requires actual toolchain/runtime/provider revisions, controlled build provenance, OS/architecture details, reproduction logs and private retention/release observations. No C-only fixture, Python model, mock provider, packet sketch or source catalogue counts as a Rill TLS run. Missing capability is recorded as unavailable, not passed.

| Acceptance evidence | macOS x86-64 | macOS AArch64 | Linux x86-64 | Linux AArch64 |
| --- | --- | --- | --- | --- |
| Rill parsing/checking/value execution and rejection diagnostics | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Private pinned build/inventory/entropy/env/symbol coexistence/distribution | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Real client/server TLS1.3 and TLS1.2 EMS/RI, mTLS, SAN/SNI/ALPN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Strict DER/PEM/path/whole-path CRL/time/local-credential admission | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Read/write/flush prefixes, authenticated tail/truncation/control saturation | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Preflight/late effects, migration/retry/wake/adoption/cancel/quiescent release | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Healthy shutdown/poisoned close/disposal/exactly-once error/resource transfer | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Limits/record and epoch accounting/KeyUpdate/current conformance/security | NOT RUN | NOT RUN | NOT RUN | NOT RUN |

On `wasm32-wasip1`, provider-free DNS/IP/ALPN values and pure options remain portable; names/types stay visible, while otherwise-valid provider-dependent/live TLS operations return typed `Unsupported`. This is **not** a working WASI TLS provider or native-target pass. Portable execution and Unsupported behavior are also **NOT RUN**; no custom imports/preview2/weaker fallback is authorized.

## Review-only Python checker

Run `python3 prototypes/27-tls/check.py` from any directory. It uses the Python standard library only, counts anchored declared expectation lines, and runs **independent author-written reference assertions** for ASCII DNS admission/canonicalization/numeric guard, opaque ALPN sizes/bytes, negative time flooring, half-open validity and CRL age, numerical quotas and a tiny unique-error/tail transfer model. Counts are printed separately by group. These are author data and toy bookkeeping, **not a Rill parser, evaluator, library implementation, TLS/provider/crypto, IDNA, DER validator, native test or security/soundness proof**. It does not read certificates, contact peers, invoke a compiler/provider or check the parent documentation links.

Only these three files belong to this task. #28 still owes formalization/adversarial checking; #30 and #43 remain OPEN; #44 must pass actual bridge/distribution/conformance/parser-limits/native-control-lifecycle/target/security acceptance before TLS support or v0 readiness is claimed. Review-only Python success cannot clear any of those gates.
