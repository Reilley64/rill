# Prototype 25: standard HTTP values and strict HTTP/1 boundaries

Read [`prototypes/25-http.rill`](../../prototypes/25-http.rill).

The human-confirmed ri-n5i design, [HTTP contract](../http-contracts.md) and [ADR0028](../adr/0028-strict-http1-codec-and-explicit-connection-drivers.md) are authoritative. This source contains only provider-free value/plan examples. It does not add APIs or fabricate parser, codec, channel, connection, transport or TLS resources.

The source contains **13 intended-valid expectations**. They were counted and hand-reviewed only. No Rill parser/typechecker/compiler/runtime or HTTP/native provider executed them. Every codec, framing, smuggling, driver and lifecycle case below is therefore NotChecked unless explicitly identified as supporting research.

## Intended-valid source coverage

| Expectations | HandChecked intent |
| --- | --- |
| 1 | Extension method `get` stays distinct from named uppercase GET; common factory bytes remain exact. |
| 2 | Unknown valid status599 is retained numerically. |
| 3 | Default reason phrase is valid empty opaque bytes, not an English table. |
| 4 | Encoded origin-target spelling including `%2F` is retained without percent decode or query-map conversion. |
| 5 | Header-name equality is ASCII case-insensitive; borrowed Header name/value access retains the field source. |
| 6 | Generic Headers retains repeated values in order. |
| 7 | `requireOne` rejects repetition rather than selecting first/last. |
| 8 | `replaceAll` matches case-insensitively and leaves one replacement value. |
| 9 | Default limits record exact start/head/chunk/body-queue values and no total-body cap. |
| 10 | `uncontrolled()` has no deadline; it does not obtain/read a clock. |
| 11 | Built-in Content-Digest/Repr-Digest trailer names form a valid ordered plan. |
| 12 | A validated GET origin-form HTTP1.1 head with one Host owns its data and exposes borrowed observations. |
| 13 | Empty method input reports HttpErrorKind.InvalidInput without native work/message parsing. |

These expectations validate no wire bytes. They avoid fake `HttpError`, decoder, progress driver, body endpoint, clock or transport construction.

## Independent rejection and adversarial inventory. NOT RUN

### Values, URI and heads

| Case | Required boundary |
| --- | --- |
| Empty/invalid-token method; lowercase names silently uppercased; unfamiliar method rejected | Empty/invalid returns InvalidInput; valid token spelling is case-sensitive/extensible. |
| Status99/600 or unknown599; reason with CR/LF/NUL/invalid control/nonUTF8 | Reject out-of-range status; retain unknown599; reason is opaque allowed bytes, no semantic/text repair. |
| Target fragment, userinfo, malformed percent triplet, raw space/control/nonASCII, path `//x`, `%2F`, dot segments, repeated query key | Reject forbidden syntax only; preserve valid encoded spelling, path/query order and `//` as path; no decode/normalize/map. |
| URI IPvFuture/reg-name/empty or overflowing port passed directly to dial/TLS | Parsing is not endpoint/reference conversion. Require explicit application mapping/default/range/name policy. |
| CONNECT with origin-form, ordinary method with authority-form, non-OPTIONS asterisk | Complete request-head context rejects. Recognition never enables tunnel. |
| Public record construction/fabricated parsed head or exchange; partial/copying endpoint extraction | Heads/endpoints/codecs are private-field values acquired through validated operations. Consuming exchange decomposition moves every endpoint exactly once. |
| Effective URI without scheme/authority context, or inferred from local socket/Forwarded | Return typed failure; use explicit ServerUriContext and target/Host precedence. |

### Header grammar and semantics

| Case | Required boundary |
| --- | --- |
| Empty/non-token field name; whitespace before colon; bare LF; obs-fold; CR/LF/NUL in value | Reject byte syntax, no repair. |
| Explicit HeaderValue with leading/trailing SP/HTAB | Reject rather than silently trim; parser alone removes surrounding OWS. |
| NonUTF8 value through text accessor | Preserve Utf8Error; byte accessor remains valid. |
| First/last field lookup, generic comma join/split/dedup, unknown-field rejection | No such generic semantics; values/requireOne and field-specific parsers are explicit. |
| Missing/repeated/invalid HTTP1.1 Host; repeated HTTP1.0 Host |400/invalid head. HTTP1.0 allows absence only. |
| Absolute-form with differing valid Host | Use target authority; do not route by Host or claim RFC-mandated lexical equality. |
| Connection close contradicted by raw header/plan; TE trailers without Connection TE | Controlled field validation rejects; generic storage does not decide. |

### Framing and body context

| Case | Required boundary |
| --- | --- |
| Multiple/equal/comma-list/empty/signed/overflow/whitespace Content-Length | Reject all except one digits-only checked value; leading zeros valid. |
| Any TE+CL, repeated chunked, chunked not final, gzip actual coding, malformed coding | Reject ambiguity/malformed; unsupported actual coding distinct. No CL removal. |
| HEAD/304 metadata treated as body length;1xx/204 CL0 or TE;205 positive content | HEAD/304 complete at head;1xx/204 fields reject;205 ordinary zero-content framing only. |
| Chunk extension overflow/malformed quote; zero application chunk interpreted terminal | Validate/bound extension; empty app chunk is no-op; only finish terminates. |
| Fixed excess accepted partially, short fixed finish padded, close-delimited request/trailers/reuse | Reject whole excess unchanged; short finish invalid; close delimitation response-only and MustClose. |
| Terminal chunk accepted before trailer terminator/validation | No Complete event until trailer section and final CRLF validate. |
| Declared/streamed total-body excess | Fail before dispatch/emission where possible or exact stream boundary; never truncated success. |

### Versions, expectation and switches

| Case | Required boundary |
| --- | --- |
| `http/1.1`, HTTP/1.10, malformed token, HTTP/2.0 text | Syntax failure except valid single digits; unsupported major distinct and server may505. |
| Higher1.x minor rejected as505 or emitted back | Process as1.1, retain received value, emit1.1. |
| HTTP1.0 TE/legacy keepalive/chunked response | Faulty TE; close-only noTE profile. |
| Unknown peer chunked request, hidden probe/buffer/retry | Reject before head; only explicit/cached knowledge admits. |
| Expect with no content, negative soft wait, server waits for body before100/final, automatic retry after417 | Reject invalid setup; zero wait allowed; explicit decision; no auto retry. |
| Final response during upload treated as rollback or connection reuse | Stop new admission, preserve queued/lower progress, interrupt body and mark Unusable. |
| Successful CONNECT/101/raw optimistic bytes | Reject server emission; client UnsupportedSwitch/unusable/close; no raw escape/reinterpretation. |

### Codec state

| Case | Required boundary |
| --- | --- |
| Decoder retains caller `&Bytes`, consumes unknown suffix, emits two events, spins zero+Runnable | Borrow ends each step; exact consumed count, caller suffix, max one event; Runnable must have work. |
| Clean EOF used for cancellation/error/TCP reset/TLS truncation | Only actual lower clean EOF calls endInput; unfinished fixed/chunked/request is IncompleteMessage. |
| Reset before Complete/event transfer or response reset without next request method | InvalidState; no hidden suffix transfer. |
| Encoder raw header changes framing after construction | Plan immutable; reject contradictions before initial batch. |
| Body excess consumes part/input lost on local reject | EncodeBodyRejection returns unchanged input. |
| Finish invalid length/trailer emits terminal prefix or loses trailers | Validate first; return unchanged trailers; prior batches remain incomplete. |
| 64KiB chunk copied with framing into >64KiB staging | Return ordered separate pieces; caller hands off each with lower tail retention. |

### Connection ownership, controls and outcomes

| Case | Required boundary |
| --- | --- |
| Constructor creates hidden routine/pool, recovers lower on failure/return, or endpoint accesses transport | Explicit driver+endpoint; caller runs/joins; consumed lower always released; no raw recovery. |
| Two client exchanges or implicit request queue; parallel server dispatch | Busy/no queue; one active; server next waits prior terminal. |
| Arbitrary application handler called by runner | No callbacks. Move distinct endpoints into explicitly scoped/owned work. |
| Concurrent sends on one OutgoingBody/ResponseSender or copied endpoint | No copy/share; exclusive command order. Distinct endpoints can move separately. |
| Full body queue prevents terminal failure recording | Terminal slot/wake independent; outcome delivered/adopted without queue insertion. |
| Completion and runner both own same lower error | Protected one-adopter transition; runner gets classification if endpoint received, exact recovered outcome only if endpoint gone. |
| Completion drop cancels exchange or loses error | It relinquishes adoption; runner adopts. Endpoint drop/unfinished body has separate abandonment policy. |
| Deadline renewed per chunk/head, soft100 timer cancels native operation, drop establishes release | One absolute HttpControl; soft timer scheduling only; stop/join until quiescent. |
| Cleanup failure replaces Completed/Cancelled/DeadlineExpired/primary | Preserve selected terminal and independent cleanup; reuse Unusable where applicable. |
| Eligible inferred from final head/body alone | Require both boundaries, output handoff, no pending op/fault/mandated close/abandon. No health promise/pool insertion. |
| Progress counts inferred from ciphertext/peer effects | Track queue/lower/parsed/delivered content independently; no ACK/execution/replay inference. |
| Unexpected response bytes with no client pipeline; server next-request read-ahead dispatched without control | Client protocol failure; server bounded suffix waits next HttpControl. |
| Application endpoint drop called graceful shutdown | Drop terminally stops/abandons; explicit consuming shutdown permits active terminal then orderly close. |

### Server invalid-message handling

Best-effort automatic mappings are400 malformed/framing/Host,501 unsupported request transfer coding,505 unsupported major,417 unsupported expectation,431 head limit and413 body limit, followed by close. Invalid request never dispatches. If syntax/transport is too unsafe, send no response. Original HttpError, generated-response effects and cleanup failure remain separate; delivery is unknown.

## Evidence and handoffs

Primary standards/provider notes in `docs/research/0039-*` are research only. The [ri-w0c OpenSSL matrix](../research/0045-progress-reference/) is ReferenceExecuted lower-layer evidence, not HTTP execution. This source and inventory are **HandChecked**. HTTP codec, Rill runner, TCP/TLS integration, conformance, smuggling/security and targets are **NotChecked**.

ri-19d owns implementation and adversarial validation. ri-hw5 and ri-ei7 own lower progress/TLS delivery. ri-48g owns review-ready RFC drafting; none of these artifacts is human RFC acceptance or software support.
