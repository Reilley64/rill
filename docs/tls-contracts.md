# Standard TLS transports

The human-confirmed ri-irw Q1-Q80 decisions and explicit consolidated confirmation govern this contract. [ADR 0025](adr/0025-explicit-tls-trust-and-private-provider-boundary.md) records the trade-off; [native boundary](tls-native-boundary.md) records the closed implementation interface; [prototype 23](prototypes/23-tls.md) supplies hand-checked examples and the NOT RUN inventory. ri-ei7 gates actual TLS delivery/support. This records the design, not implemented TLS, compiler/checker execution, protocol conformance or security certification.

## 1. Scope, precedence and targets

- `std::tls` provides certificate-authenticated client/server TLS over eligible owned bidirectional streams, including explicit plaintext-to-TLS upgrades. It does not expand the prelude. No hidden TCP dial/listen/accept loop, HTTP/database negotiation, reconnect, payload replay or plaintext fallback.
- Native Linux/macOS x86-64/AArch64 are intended live targets, all unvalidated. Provider-free name/ALPN values and option construction are portable. Public types/signatures remain visible on standard `wasm32-wasip1`; unavailable provider-dependent operations return typed `Unsupported`. Secure entropy, a supplied transport or a crypto source branch is not working Wasm TLS. No preview2/custom imports/inherited-socket profile is selected.
- Canonical streaming ri-fnk (comment 6886715215), its scoped error refinement (ri-fnk, comment 6886862143), [networking ri-vno](networking-contracts.md), [clocks/tokens ri-j60](clock-cancellation-contracts.md), [time](time-contracts.md), and [core-library/unique-cause ri-8n7](standard-library-contracts.md) remain binding.
- No methods, traits, public `dyn`, general unions/literal types, named constraint groups, optional/named arguments, public pointers/FDs, new field modifiers or source-level native extensions. Schemas below are declarations, not invented opaque-layout syntax or a library implementation. Family bodies use `implement`, ordinary functions `function`; item-site exports and exact canonical identity apply.
- Current ri-dej direction is compiler-owned Wasm activation records plus nonrecursive dispatcher, **not superseded tail-call/Asyncify composition**. Its full owned ABI/runtime/lease/reclamation gates remain OPEN. ri-3id's SQL-provider design and limited standalone evidence neither deliver nor authorize TLS/general FFI.

## 2. Public values and owners

| Public type | Meaning / supplied properties |
| --- | --- |
| `ServerName` | Opaque validated ASCII DNS reference name or numeric IP reference identity. Owned, non-implicitly-copyable; explicit copy, eq/hash, noBorrow/sendable/shareable. No native dependency. |
| `AlpnId` | Opaque owned 1–255-byte protocol identifier; byte-exact eq/hash, explicit copy, noBorrow/sendable/shareable. Not text. |
| `Credentials` | Owned certificate chain and matching private key; material/role checks do not certify own trust/status. |
| `TrustAnchors` | Owned immutable, purpose-bound explicit root grants, not a native/browser trust-policy snapshot. |
| `CrlSnapshot` | Owned immutable CRL inputs and maximum-age policy, not preauthenticated status for every peer. |
| `ClientConfig`, `ServerConfig` | Owned logical immutable configuration snapshots. |
| `TlsStream<S>` | Noncopy owner of one eligible transport value and TLS engine. No supplied copy. |
| `ControlledTls<S>` | Noncopy borrowing canonical-I/O view retaining stream/clock/token dependencies. No consuming close. |
| `TlsError` | Opaque owned display/noBorrow/sendable/shareable diagnostic payload, with original failures retained in the outer Err's unique cause. |

Credentials, trust/CRL snapshots and configurations are not implicitly copyable; standard explicit `copy(&value)` retains the same logical immutable snapshot/backing, not refreshed trust/time or reparsed secrets. Native mutable caches/reference counts must actually be synchronized and backing retained through every use; private pointers alone prove none of these properties. Required native ownership/copy/release actions remain protected under ri-8n7 even through generic specializations and remaining-field cleanup.

`TlsStream<S>` supplies noBorrow only with noBorrow(S), transfer only with sendable(S), and sharing only with shareable(S) and safe readonly implementation evidence. Owning an S containing caller borrows does not erase them. Sharing never allows overlapping exclusive I/O. Control views retain all borrowed dependencies and are not secretly owned clock/token handles.

### Portable factories

```text
dnsServerName(text: &string): Result<ServerName,TlsError>
ipServerName(address: IpAddress): ServerName
dnsName(name: &ServerName): Option<&string>
ipAddress(name: &ServerName): Option<IpAddress>
alpnId(bytes: Bytes): Result<AlpnId,TlsError>
alpnBytes(id: &AlpnId): &Bytes
```

DNS construction validates ASCII hostname syntax. Require nonempty LDH labels, each ≤63 bytes, with no leading/trailing hyphen. The total is ≤253 bytes after removing one optional terminal root dot. Reject empty/root-only/interior-empty, underscore, whitespace, NUL, Unicode, reference wildcards, leading-dot suffix searches, URI/port/zone syntax. Use ri-vno's conservative all-components numeric-looking guard: each component decimal digits or `0x`/`0X` plus zero or more hexadecimal digits; such names belong on the explicit IP path. Single-label and digit-initial nonnumeric names are admitted. Canonical storage is lowercase ASCII without terminal dot; no locale folding, lookup/search expansion or resolver canonicalization. This does not modify separate DNS routing input/case/root-dot intent.

LDH-admitted `xn--` strings are opaque ASCII reference strings, **not certified valid IDNA A-labels**. No Punycode/Unicode conversion, normalization or confusable defense is supplied. An IP reference retains family/bits; port/scope are not identity bytes, and mapped-v6 is not silently folded to v4. Equality/hash follow these canonical identities. The query interface implies no `display(ServerName)` or secret/resource reflection.

## 3. Policies and configuration construction

```rill
export enum TrustPurpose { ServerAuth, ClientAuth, }
export enum TlsProfile { Modern, Compatible, }
export enum TlsVersion { Tls12, Tls13, }
export enum SniMode { Auto, Omit, }
export enum AlpnPolicy {
  Disabled,
  Optional(List<AlpnId>),
  Required(List<AlpnId>),
}
export enum ClientAuth {
  None,
  Required { anchors: TrustAnchors, crls: CrlSnapshot },
}
export type ClientOptions = {
  profile: TlsProfile;
  sni: SniMode;
  alpn: AlpnPolicy;
  credentials: Option<Credentials>;
};
export type ServerOptions = {
  profile: TlsProfile;
  alpn: AlpnPolicy;
  clientAuth: ClientAuth;
};
```

All fields are ordinary public fields, edited through mutable bindings/exclusive access; there is no per-field `mut`/readonly/const. Logical immutability describes captured resources, not a new source qualifier. No implicit options/Err/Result copying follows from these declarations.

```text
clientOptions(): ClientOptions
serverOptions(): ServerOptions
clientConfig(anchors: TrustAnchors, crls: CrlSnapshot): Result<ClientConfig,TlsError>
clientConfigWithOptions(anchors: TrustAnchors, crls: CrlSnapshot,
  options: ClientOptions): Result<ClientConfig,TlsError>
serverConfig(credentials: Credentials): Result<ServerConfig,TlsError>
serverConfigWithOptions(credentials: Credentials,
  options: ServerOptions): Result<ServerConfig,TlsError>
```

The defaults are Modern, ALPN Disabled, no client credentials or client-auth request, and client SNI Auto. Client anchors must have ServerAuth purpose; server Required client-auth anchors must have ClientAuth purpose. Wrong purpose or invalid role/material/policy fails before configuration publication. There is no silent partial acceptance. Factories consume inputs and capture stable snapshots. Copy a resource explicitly to reuse it; later file/option edits do not alter a configuration.

Static local-credential key/leaf/role/material checks occur during construction. At handshake admission, check the **configured local chain's current validity**, including optional client credentials even if not subsequently requested. This can reject stale unused credentials. Peer path/status validation has its own fresh verification-phase wall sample. Configuration does not freeze the current time. Local checks do not certify local trust/revocation under the opposite role's peer anchors. The peer validates its actual path/status. There is no continuous certificate monitoring or perpetual-validity promise.

## 4. Input formats, trust and revocation

```text
credentialsFromPem(certificates: Bytes, privateKey: Bytes): Result<Credentials,TlsError>
credentialsFromDer(certificates: List<Bytes>, privateKey: Bytes): Result<Credentials,TlsError>
trustAnchorsFromPem(certificates: Bytes, purpose: TrustPurpose): Result<TrustAnchors,TlsError>
trustAnchorsFromDer(certificates: List<Bytes>, purpose: TrustPurpose): Result<TrustAnchors,TlsError>
crlsFromPem(crls: Bytes, maximumAge: SignedDuration): Result<CrlSnapshot,TlsError>
crlsFromDer(crls: List<Bytes>, maximumAge: SignedDuration): Result<CrlSnapshot,TlsError>
```

Factories consume inputs and retain them through actual native use. Failure returns no partial result or recovered input. DER list elements contain one complete object each; binary trailing bytes are invalid. Require DER-conformant objects, not merely a successful permissive BER/DER decode. PEM permits only supported `CERTIFICATE`, `X509 CRL`, unencrypted PKCS#8 `PRIVATE KEY` blocks as appropriate, with space/tab/CR/LF outside blocks. Reject unknown blocks, bag attributes, trailing junk, duplicate extensions and malformed/ambiguous encodings. Credentials use a leaf-first chain and matching key. Supplied local/peer chains and built paths are each limited to ten certificates. A peer leaf in the first position does not prove that the whole presented list is one valid path.

No encrypted PKCS#8, legacy PKCS#1/SEC1 wrappers, PKCS#12, password prompts/default callbacks, HSM/hardware stores, key export or automatic format/decryption fallback. Applications provision protected inputs; ordinary Bytes and native copies have no universal secure-memory/zeroization guarantee. Unsupported critical extensions reject; recognized unsupported revocation/TLS features are not ignored merely because noncritical. Ordinary standards-permitted noncritical extension handling is not replaced with reject-every-unknown behavior.

### Explicit anchor grants

- No automatic OS-store import, compiled/provider-default trust paths, environment trust overrides or Rill-maintained bundled root set. Public HTTPS applications must maintain their chosen CA source.
- Accept only explicit cryptographically self-signed CA roots with strong approved keys, proper CA/basic constraints and KU/purpose restrictions. A self-signature does not confer trust. The application grants it.
- No leaf/intermediate terminal grants, partial-chain or OpenSSL AUX trust overrides. Untrusted intermediates remain path-building material; peer-supplied roots never become grants.
- Permit SHA1 **only for an explicitly granted root's self-signature**; no MD5, weak root key or SHA1 path/CRL signature. This is not a public algorithm downgrade switch. Selected root time/constraints and whole-path revocation still apply.
- Coalesce identical certificate data. Reject conflicting anchor certificates with the same subject/public key. Do not union restrictions or pick the first. Loading DER/PEM does not claim Mozilla/p11-kit/macOS policy equivalence; external trust restrictions are not translated or silently claimed preserved.

### Strict offline CRLs

- No AIA issuer, CRLDP, OCSP or native-trust fetching, URI/lazy-directory lookup, background refresh or app retrieval callback. Application retrieval is explicit and separate.
- Require usable evidence for **every certificate in the actual built path, including its final root**. Root CRLs do not repair a compromised anchor; retire its grant. Bounded path construction need not discover every valid alternative, but an accepted path must satisfy all rules.
- Accept only complete direct-issuer base CRLs. Verify with the actual issuing CA; require explicit issuer KU/cRLSign and approved signature/key strength. Honor distribution-point/IDP scope and complete reason coverage; an absent serial alone does not establish good status.
- Reject delta, indirect, alternate-key and reason-partitioned forms, unsupported required restrictions and unhandled critical extensions; never ignore a needed delta. Reject unsupported stapling/TLSFeature requirements and `noRevAvail` exceptions. OCSP/must-staple cannot be claimed implemented from CRL flags or an installed callback.
- Require well-formed nonnegative CRL numbers, thisUpdate and nextUpdate with valid ordering, and a positive **whole-second** maximumAge. Usable interval is `[thisUpdate, min(nextUpdate, thisUpdate + maximumAge))` on the checked verification-time basis, no tolerance or silent fractional rounding. Missing/future/expired/invalid/incomplete evidence fails, distinct from revoked where evidence permits.
- Coalesce identical CRLs, otherwise one CRL per issuer-key/scope in a snapshot. Ambiguous overlapping coverage fails rather than guessing. Do not preload versions for implicit newest/first selection. A parsed snapshot is not authenticated status until used against the built path.
- Refresh constructs a new configuration/snapshot. Old owners retain old grants/CRLs and still age out on new handshakes. No global/persistent anti-rollback service or forced closure of established channels; provenance, rollback protection and retirement are application obligations.

### Checked wall time

Use a fresh checked Timestamp for local admission and for peer certificate/status verification, not elapsed Clock/Instant. Explicitly **floor toward negative infinity** to integral seconds (negative fractional epochs are not truncation toward zero). Check time_t signedness/width and native UTC/calendar bounds before supplying OpenSSL; its void set-time call is not validation. No unchecked libc-time or stale-config-time fallback.

Certificate validity is intentionally the provider's stricter `[notBefore,notAfter)`: equality at expiry fails, unlike RFC5280's inclusive wording. No success-overriding callback, disabled time checks, skew grace or fake-wall-clock provider hook. Elapsed/manual deadline clocks do not control certificate wall time. Raw nanosecond time semantics elsewhere remain unchanged; no lossless-conversion claim follows.

## 5. Identity, SNI and ALPN

Require chain validation, intended service identity and handshake proof. Encryption/SNI/verify-result alone is insufficient. Never substitute resolver canonical name, CNAME/PTR or dialed address for the caller's reference.

DNS uses SAN dNSName only, with ASCII whole-label comparison and no CN fallback, even under Compatible. The supported wildcard is exactly one `*` occupying the entire leftmost label, followed by at least two valid hostname labels. It matches exactly one reference label, never parent/deeper/partial labels. Invalid presented identifiers/patterns are not repaired; another usable SAN can match subject to complete certificate validation. This is not public-suffix checking or CA-misissuance protection. IP matches IP SAN octets exactly, not DNS text/wildcards or scope/port.

Client SniMode.Auto derives canonical DNS SNI; IP sends no hostname SNI. Omit does not disable identity verification. No alternate-SNI/domain-fronting API or ECH profile. Ordinary SNI is not confidential. The server has one credential/config. Present wire SNI must be well formed (no literal IP, terminal-dot repair or leading-dot search) and covered by its DNS SANs. Unknown/malformed SNI fails. Missing SNI may use the single credential. No dynamic credential callback/vhost table. SNI is not client authentication or HTTP/SQL authorization.

ALPN identifiers are opaque nonempty bytes with length 1-255. Optional/Required own a nonempty configured list of unique identifiers of at most 256 identifiers, with wire size including length prefixes ≤65535. Peer entry count must be ≤256 before deduplication. Repeated IDs do not change server-preference intersection; malformed encoding fails. Select the first server-preference ID offered by the client. Required absence fails. Optional allows absence but fails on attempted negotiation with no overlap. Disabled negotiates/implies nothing. Reject unoffered selections before publication. HTTP/database profiles, not TLS, choose what absent ALPN permits.

## 6. Fixed cryptographic profile

Modern offers TLS 1.3 only. Compatible explicitly offers TLS 1.2 and 1.3 on the same connection, not failure-driven downgrade. SSL/TLS 1.0/1.1 are not allowed. Require actual EMS and secure-renegotiation signaling in both TLS1.2 roles before publication; support alone is not enforcement. Renegotiation itself, record/certificate compression and PHA are disabled. Initial required mTLS remains. No resumption/session injection/cache, issued/stored/reused tickets, PSKs, 0RTT/early-data or session export. Safely process valid received control traffic without acquiring resumable state; don't reject every unknown extensibility offer.

| Preference family | TLS1.3 IANA suite | TLS1.2 IANA suites (ECDSA before RSA where applicable) |
| --- | --- | --- |
| 1 | TLS_AES_128_GCM_SHA256 | TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256; TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256 |
| 2 | TLS_CHACHA20_POLY1305_SHA256 | TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256; TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256 |
| 3 | TLS_AES_256_GCM_SHA384 | TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384; TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384 |

`CipherSuite` is a nominal scalar enum with these nine full IANA names as unit variants. CBC/static-RSA/FFDHE/anonymous suites are not allowed. Suite selection does not choose certificate or key-exchange algorithms.

TLS 1.3 group preference is X25519MLKEM768, then X25519/P-256/P-384. Supply initial hybrid and X25519 shares; other groups use normal negotiation. TLS 1.2 uses classical groups only. Classical groups within the configured list are allowed. There is no require-PQ guarantee. Other hybrids/pureMLKEM/PQ authentication are deferred. Hybrid key agreement is not PQ certificate authentication. No cross-connection local key-share reuse; no rejection cache for backward-compatible peer share reuse.

Path/CRL signatures use RSA-PSS and RSA-PKCS1v1.5 SHA256/384/512, ECDSA P-256/P-384/P-521 with corresponding SHA2, or Ed25519/Ed448. RSA keys must be 2048-8192 bits. DSA/SHA1/MD5 path/CRL signatures are not allowed. TLS1.3 CertificateVerify excludes ordinary PKCS1 while PKCS1 certificate signatures remain supported; TLS1.2 approved PKCS1 handshake signatures are not static RSA key exchange. Enforce scheme parameters, leaf digitalSignature KU, explicit serverAuth/clientAuth EKU and non-CA leaf status. Both EKUs may be explicit; missing KU/EKU, SGC-only and anyEKU-only substitutes fail. CA EKU may be absent; present restrictions apply, no AUX bypass.

Validate effective inventory against the fixed lists, not successful setters/security levels/optional missing-token syntax. No insecure verification switch, raw provider cipher/property strings, reduced security level or replacement verifier. Tests use explicit CAs/correct names; faults stay private test machinery.

Current RFC9846/9852/10015/10024 and PKIX obligations remain binding. In particular, OpenSSL3.5.9 does not generate signature_algorithms_cert; stock automatic key-use/epoch/KeyUpdate enforcement and strict DER/revocation completeness are **not established**. Seek maintained fixes/private enforcement, explicitly review any provider patch and ownership; if the line cannot meet the profile return to the provider/map decision. No handwritten crypto or silent conformance exception.

## 7. Generic evidence and handshake entrypoints

For each eligible S bind the independently selected canonical evidence `std::io::read(S,RE)`, `write(S,WE)`, `flush(S,FE)`, `close(S,CE)`. Each of RE/WE/FE/CE requires display/noBorrow/sendable/shareable because TLS retains its original Err as a unique cause. They need not be equal. Requirements are checked before monomorphization, not selected by expected error type. There is no global noBorrow(S), structural name lookup or associated-type/trait invention. TLS-specific bounds do not alter primitive I/O or ri-fnk's scoped buffer-helper bounds.

```text
clientHandshake<S>(transport: S, name: ServerName, config: ClientConfig)
  : Result<TlsStream<S>,TlsError>
clientHandshakeWithInput<S>(transport: S, name: ServerName, config: ClientConfig,
  initialCiphertext: Bytes): Result<TlsStream<S>,TlsError>
clientHandshakeWithControl<S>(transport: S, name: ServerName, config: ClientConfig,
  initialCiphertext: Bytes, control: NetControl): Result<TlsStream<S>,TlsError>
serverHandshake<S>(transport: S, config: ServerConfig): Result<TlsStream<S>,TlsError>
serverHandshakeWithInput<S>(transport: S, config: ServerConfig,
  initialCiphertext: Bytes): Result<TlsStream<S>,TlsError>
serverHandshakeWithControl<S>(transport: S, config: ServerConfig,
  initialCiphertext: Bytes, control: NetControl): Result<TlsStream<S>,TlsError>
```

The schemas abbreviate the evidence above; actual generic declarations must retain independently selected outputs. All consume their transport/config/name/prefix inputs. Use copy(&config) explicitly to reuse the configuration. Basic forms imply an empty prefix. No borrowed-TLS constructor, TLS split/reunite, independent concurrent application drivers or raw transport escape. One operation may pump both directions; ri-n5i must not infer general duplex support from TCP halves. No hidden dial/retry, handler registry or global server shutdown framework.

Application upgrade negotiation and pending plaintext flush finish first. Existing buffer intoParts handoffs transfer owned transport and unread suffix explicitly. Feed all supplied suffix as TLS ciphertext in order before further reads; preserve remaining TLS input on success. No silent discard/re-read/replay or reinterpretation of pending plaintext output. Prefix is bounded like other input, not an unlimited exception.

Publish only after authentication, actual ALPN/version/EMS/identity/status policies and final handshake output through underlying write/flush. Provider handshakeDone/verifyResult alone is insufficient, and output handoff is not peer application receipt. Any handshake failure consumes/releases inputs; no reusable plaintext or partial-auth stream result. Discard pending TLS output on failure, don't flush or send graceful alerts during abandonment. Preserve selected primary outcome through true native quiescence/adoption-or-loser cleanup.

## 8. Canonical I/O, progress and control

Only implementations of the existing families, with exact arities:

```text
read<S>(&mut TlsStream<S>, &mut Bytes, u64): Result<Option<u64>,IoFailure<TlsError>>
write<S>(&mut TlsStream<S>, &Bytes): Result<u64,IoFailure<TlsError>>
flush<S>(&mut TlsStream<S>): Result<void,TlsError>
close<S>(TlsStream<S>): Result<void,TlsError>
```

The same read/write/flush implementations exist on ControlledTls<S>. There is no close on the view. Positive reads append authenticated plaintext, never unauthenticated fragments. Return positive progress, or None only after authenticated close-notify and drained prior plaintext. Raw EOF/reset/fatal alert is not clean EOF; EOF without notify is truncation. Preserve an authenticated tail before reporting a latched terminal fault. If final tail is appended in the failing call, IoFailure.transferred is its exact appended prefix; while tail remains, retain the pending fault. A fault already surfaced elsewhere is not cloned later. Zero read returns Some(0), no health probe, subject to controlled preflight.

Write progress counts current-input plaintext accepted into bounded TLS-owned storage, independent of caller buffer, not ciphertext/transport acceptance/peer execution. Nonempty success must be positive; short progress/backpressure are permitted, not Ok(0). Failure retains actual accepted prefix even if later delivery fails; no replay. An empty write returns 0 without a health probe, subject to control. Stable owned native retry buffers survive WANT/re-entry. Earlier-call acceptance is not counted again in a later call.

Flush drives accepted output and required control records through underlying flush, including opposite-direction WANT needs. Incoming authenticated data encountered stays bounded/retained. Read/write can also need control writes after handshake. Success is layer handoff, not ACK/health/durability. Ordinary backpressure is not failure; unsupported progress within bounds gives typed limit failure/poison, not dropped authenticated bytes or hidden unbounded queues. Underlying ciphertext counts remain layer-relative diagnostic context, never substituted for plaintext progress.

```text
controlledTls<S>(stream: &mut TlsStream<S>, control: NetControl): ControlledTls<S>
shutdownWrite<S>(stream: &mut TlsStream<S>): Result<void,TlsError>
shutdownWriteWithControl<S>(stream: &mut TlsStream<S>, control: NetControl): Result<void,TlsError>
closeWithControl<S>(stream: TlsStream<S>, control: NetControl): Result<void,TlsError>
```

Use ri-vno NetControl directly: ordinary clock/token borrows and Option<Instant>, stable by-value descriptor snapshot. Constructor of the view stores dependencies without preflight; each operation validates basic inputs/domain, then already-requested token, then needed clock/expiry and admission, including zero/no-op/idempotent requests. No timer/elapsed clock read when not needed; certificate wall checks are separate. No ambient/permanent attachment, secretly owned descriptor handles, generic withCancellation or renamed canonical close. Forward the same absolute budget through explicit network/TLS calls, not renewed durations.

After admission, completion/token/expiry/clock failure select one outcome. Ready ties are unspecified. Preserve the outcome through actual completion or confirmed nondestructive lower-operation abort, unregister, joins and safe release. Final plaintext prefix includes real late effects; do not guess zero, roll back, reclassify after cleanup or force-close a borrowed lower operation to fake cancellation. Intrinsic routine stopping remains outside Result and masked through required cleanup; explicit token outcomes are typed. Return may outlast deadlines indefinitely.

### 8.1 Pre-handshake resumable progress (ri-w0c)

The [progress contract](progress-io-contracts.md) adds opaque `TlsProgress<D>` / `TlsProgressWait<D,G>` and consuming `clientProgress`, `clientProgressWithInput`, `serverProgress`, `serverProgressWithInput`. D must supply the complete admission/stop/advance/register/withdraw family with explicit G/RE/WE/FE/CE/DE; every lower error retains this contract's display/noBorrow/sendable/shareable cause evidence. TLS supplies TlsError for all five outer outputs. There is no ordinary blocking adapter, existing-TlsStream conversion, NetControl form, TLS split or raw-lower extraction.

Construction selects progress mode before handshake. Immediate input/provider setup failure consumes and stop/joins D. Handshake/authentication/policy/final-flight publication occurs through advance and emits started only after the existing section7 publication boundary. Standard writes and owned reads are capped at16384 bytes. Public write progress remains exact plaintext staging acceptance; OpenSSL zero/WANT/ciphertext/private counters cannot replace it.

The private OpenSSL3.5.9 path enables partial write and disables moving-write-buffer/AUTO_RETRY before I/O, with one pipeline,16384-byte fragment/native attempt, no configured output padding and16384-byte ingress/egress BIO-pair rings. Rings share, rather than extend, each existing65536-byte ciphertext direction ceiling; drains/lower tails/jobs/copy overlaps remain charged. WANT retries preserve original pointer/content/length; positive partial success starts a fresh suffix operation. Temporary capacity is Waiting; inability to retain/progress required state within approved bounds is terminal LimitExceeded.

A fixed private passive observer records TLS1.2 post-handshake HelloRequest/renegotiating ClientHello without reentrant/user callbacks. After immediate SSL result classification, select ProtocolFailure at Control phase, stop, and perform no further application SSL I/O or SSL_shutdown. Attribute it to an active read/write/flush when natural, otherwise Progress. Preserve real prefixes/handoffs. This supersedes provisional continue-after-provider-rejection: the pinned ReferenceExecuted matrix showed local fresh-write success can omit warning ciphertext, advance record state and make the peer fail record processing. ri-ei7 must validate both roles; TLS1.3 KeyUpdate is valid control traffic, not renegotiation.

Progress disposal/stop is terminal and joined, not graceful notify. Graceful progress close retains the existing local notify/handoff/lower-close rule. Names remain typecheck-visible on WASI; without a live provider construction deterministically returns Unsupported after validation and consumes/releases D. Full async partitions, callback/shim enforcement and target support remain ri-ei7/#47 gates.

## 9. State, closure and original errors

| State/event | Behavior |
| --- | --- |
| Pre-admission borrowed-operation rejection without effects | Preserve a healthy owner without poison. Consuming handshake/close still consumes/releases its input owner. |
| Admitted transport/protocol/provider failure or abandonment | Poison session; no new application writes/flush/protocol pumping or reuse claim. Preserve final real prefixes and pending authenticated tail. Logical poison is not immediate physical release. |
| Pending original fault, authenticated tail remains | Drain the tail under read semantics and preserve the original fault until first reported. A controlled rejection does not replace the still-pending original fault. |
| Original already returned by write/flush/read | Later tail can drain. The eventual failed-state error is a new AlreadyFailed, not a cloned cause or fictitious fresh native failure. Cached observations/zero semantics remain. |
| TLS1.3 peer close-notify | Drain prior authenticated input then clean EOF; local sends may continue until local write shutdown. |
| TLS1.2 peer close-notify | End both application directions, discard pending writes as required and drive protocol response. Response failure is still an operation failure, not invented successful handoff. |
| Successful TLS1.3 shutdownWrite | Flush accepted plaintext, send notify and finish local handoff, without peer-notify waiting/raw TCP shutdown. Later nonempty writes fail with WriteClosed; reads continue. Repeating the operation is idempotent subject to preflight. Healthy flush with nothing left succeeds without health probing. |
| TLS1.2 shutdownWrite request | Return pre-admission Unsupported based on the cached version. Do not promise half-close or poison healthy state merely for this unsupported operation. Use consuming close. |
| Poisoned flush/shutdown/nonempty write | Return AlreadyFailed once the original fault has been reported. Do not reopen native protocol work to probe health. |

Consuming close attempts local flush/notify/output handoff then underlying close, **not peer-notify waiting**. Success is locally graceful release, not authenticated inbound EOF or app delivery. If graceful phase fails preserve its primary error and dispose the transport without another implicit flush; poisoned close skips graceful protocol and reports failed state while releasing. Owner is gone on success/failure. Mandatory release ignores external cancellation; ordinary secondary cleanup errors do not replace selected primary outcome. Never retry native close on an unowned/recycled descriptor.

Ordinary disposal, including healthy disposal, discards TLS output and does not send notify/flush/drain. Peer may see truncation; call explicit close for local grace. Underlying disposal keeps its own contract; outstanding native uses must still complete/abort and quiesce. Fatal Rill panic/allocation promises no unwind/cleanup; no new universal destructor guarantee is introduced.

## 10. Limits and work accounting

These are fixed v0 admission ceilings, with no public native tuning. Raising them needs reviewed profile change; applications can impose stricter outer limits. Checked u64 counts/capacities and native/target representability apply before unsafe narrowing/growth; private allocations and expanded representations remain separate gates.

| Material / queue | Ceiling |
| --- | --- |
| One decoded certificate / private key | 16384 / 65536 bytes |
| RSA key | 2048–8192 bits |
| Chain material / supplied local+peer chains / built path | 131072 bytes / 10 certificates each |
| Anchors / aggregate decoded anchor data | 1024 / 16777216 bytes |
| CRLs / per-CRL bytes / aggregate decoded CRL data | 1024 / 16777216 / 67108864 bytes |
| Entries per CRL / aggregate entries | 100000 / 1000000 |
| Raw material bytes per load before dedup | 2 × relevant decoded-byte ceiling + 4096, checked arithmetic |
| Pending plaintext in/out and ciphertext in/out | 65536 bytes each |
| Initial upgrade prefix | 65536 bytes, charged to ingress staging |
| Handshake-message reassembly | 262144 bytes, plus smaller certificate-chain bound |
| ALPN count / wire list | 256 before peer dedup / 65535 bytes incl length prefixes |
| TLS-owned message / native code entries | 4096 UTF8 bytes / 16; truncation explicit |

Count combined generated/processed **cryptographic records**, not transport chunks, at most **1048576 (2^20) per connection across all epochs**, never reset on KeyUpdate. The operation budget is 1024 incoming records without application-plaintext progress. Retry accounting must reflect actual cryptographic work and not double-count a mere transport tail write. Close conservatively rather than rely on unproven automatic GCM-key rotation; mandatory peer KeyUpdate handling still applies within budget, no manual rekey API. Insufficient budget can force failure/release rather than a graceful alert that crosses the bound.

Long/high-volume connections reconnect when they reach the budget. Otherwise valid input with excessive fragmentation or only control records can fail. No reset-by-renamed-queue, automatic replay, truncation-to-success or disabling validation. Input/counter caps are not a global memory/CPU/stack bound. Native transient allocation, policy/path complexity, repeated callbacks, per-call enforcement, cipher key-use/epoch limits and scheduler fairness need actual evidence in ri-ei7/#30.

## 11. Errors and cached observations

```rill
export enum TlsErrorKind {
  InvalidInput, Unsupported, LimitExceeded, NativeResourceLimit, ProviderFailure,
  VerificationFailed, ProtocolFailure, Truncated, TransportFailure,
  TokenCancelled, DeadlineExpired, ClockFailure, WallTimeFailure, WriteClosed, AlreadyFailed,
}
export enum VerificationKind {
  UntrustedIssuer, InvalidChain, BadSignature, WrongPurpose, NameMismatch,
  NotYetValid, Expired, Revoked, RevocationUnavailable, RevocationStale, UnsupportedFeature, Other,
}
export type NativeTlsCode = { domain: string; code: i64; };
```

`TlsOperation` is the nominal call-family enum: DnsServerName, AlpnId, LoadCredentials, LoadTrustAnchors, LoadCrls, MakeClientConfig, MakeServerConfig, ClientHandshake, ServerHandshake, Read, Write, Flush, ShutdownWrite, Close, Progress. `TlsPhase` has Input, ProviderSetup, Handshake, CertificateVerification, RevocationVerification, Negotiation, RecordProcessing, TransportRead, TransportWrite, TransportFlush, Control, Shutdown, Release. These normalize entrypoint formats/control variants, not a fake user stack trace.

```text
kind(&TlsError): TlsErrorKind
operation(&TlsError): TlsOperation
phase(&TlsError): TlsPhase
verificationKind(&TlsError): Option<VerificationKind>
certificateDepth(&TlsError): Option<u64>
inputIndex(&TlsError): Option<u64>
message(&TlsError): &string
nativeCodes(&TlsError): &List<NativeTlsCode>
messageTruncated(&TlsError): bool
nativeCodesTruncated(&TlsError): bool
version<S>(&TlsStream<S>): TlsVersion
cipherSuite<S>(&TlsStream<S>): CipherSuite
negotiatedAlpn<S>(&TlsStream<S>): Option<&AlpnId>
peerCertificates<S>(&TlsStream<S>): Option<&List<Bytes>>
referenceName<S>(&TlsStream<S>): Option<&ServerName>
receivedSni<S>(&TlsStream<S>): Option<&string>
```

Owned numeric native codes are diagnostic namespaces, not portable branching contracts. Preserve representable i64 values without wrapping; unavailable distinctions stay absent/Other/broader known category. Never parse messages. Depth/index are zero-based optional facts, not a guessed zero; native evidence can be incomplete. Elapsed ClockFailure differs from WallTimeFailure, stale/unavailable from Revoked, profile LimitExceeded from native **reported** resource shortage. Rill/runtime allocation/bookkeeping/error construction and invariant failures remain fatal.

Primary TLS wrapping retains the original underlying Err uniquely as ri-8n7 Cause, preserving its origin/operation/cause; no downcast/new public dyn or reconstructed payload. Summaries are curated/bounded, no automatic provider strings, file paths/config dumps, certificates/keys/plaintext/session secrets. Truncation is marked. Arbitrary application-defined cause payloads/formatters are not brought under TLS summary caps. Opaque/secret-bearing resource formatting is absent/redacted, but caller key Bytes remain ordinary diagnostic values: no universal secret tracking.

Metadata is captured at successful publication and available after poison without native re-entry, health checks or revalidation. The peer DER list is the authenticated actual built path including selected root, not raw peer-presented input. Client referenceName is separate from server receivedSni. Irrelevant/missing values are None. A NoClientAuth server has no authenticated client chain. Borrowed observations retain source dependencies. Numeric endpoint families forward only where S supplies the matching canonical capability. No private-key/session-secret export, raw provider object, mutable S accessor, intoParts or TLS-to-plaintext downgrade.

## 12. Evidence and handoff

Research snapshots are historical facts, not extra policy: [verification](research/0038-tls-verification-trust-and-protocol-limits.md), [provider/I/O](research/0038-tls-provider-and-native-io-boundaries.md), [trust/name/time](research/0038-trust-sources-identity-and-certificate-time.md), [crypto/revocation](research/0038-openssl-crypto-profiles-and-offline-revocation.md), and ri-w0c's [BIO/pending-read facts](research/0045-openssl-bio-pair-transition-facts.md). Approved choices above supersede their open-choice lists without converting source inspection into execution.

The [ri-w0c standalone reference](research/0045-progress-reference/) is ReferenceExecuted13/13 on one macOS-arm64 host against the pinned source archive. It validates selected mode/BIO/retry/control traces only. It is not the closed shim, Rill integration, TCP, async four-budget partition, server-role renegotiation fixture, target matrix or security/conformance proof. ri-hw5 owns shared progress runtime/TCP delivery; ri-ei7 consumes it for the TLS path.

ri-ei7 must deliver the controlled distribution, compiler/runtime bindings, complete accepted profile and target/security inventory; ri-dej's [standalone mechanism stage](research/0030-runtime-validation.md) is complete, while ri-48g retains production/formal backend acceptance and ri-3id stays separately OPEN for SQL-provider delivery. Review-ready ri-48g RFC drafting is not TLS implementation/support or RFC acceptance. The [ri-n5i HTTP contract](http-contracts.md) explicitly consumes one TlsProgress owner, advertises no h2/h3, distinguishes authenticated EOF/truncation and never infers safe reuse/replay from a head/body alone. ri-19d retains HTTP-over-TLS delivery; HTTP/SQL authorization/effects remain consumer decisions. No commit/push authorization follows from design confirmation.
