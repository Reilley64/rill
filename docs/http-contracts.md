# Standard HTTP values, HTTP/1 codec and connection drivers

This document records the explicitly confirmed ri-n5i design. It consumes the closed ri-w0c progress contract while keeping transport composition, protocol framing, application work and implementation evidence separate. It is a public design contract, not an implemented parser, client, server, conformance suite or security proof.

## 1. Scope and modules

- `std::http` owns shared values, errors, heads, plans, controls, limits, body/completion endpoints and terminal data.
- `std::http::h1` owns public transport-independent HTTP/1 request/response decoder and encoder state.
- `std::http::client` owns direct-origin client driver/connection/exchange/response types and operations.
- `std::http::server` owns sequential server driver/connection/exchange/response types and operations.

HTTP/1.1 is initial. Basic HTTP/1.0 is close-only. HTTP/2, HTTP/3/QUIC, forward proxies, successful CONNECT,101/WebSocket/upgrades, automatic redirects/retries/decompression/cookies/JSON, global pools and replay are unsupported. No h2/h3 ALPN is advertised by this foundation.

Shared values and the HTTP/1 codec are usable by external frameworks without the standard connection runners. The codec performs no dialing/listening/TLS, transport I/O, clock reads, channel operations or user callbacks. The runners consume one supplied concrete ri-w0c progress driver; no ordinary blocking-I/O adapter, TLS split, raw owner recovery or hidden detached routine exists.

## 2. Errors and diagnostics

`std::http` exports opaque owned `HttpError` with display/noBorrow/sendable/shareable evidence and:

```rill
export enum HttpErrorKind {
  InvalidInput,
  InvalidSyntax,
  InvalidFraming,
  UnsupportedVersion,
  UnsupportedTransferCoding,
  UnsupportedSwitch,
  LimitExceeded,
  IncompleteMessage,
  InvalidState,
  Other,
}

export enum HttpOperation {
  Method,
  Status,
  ReasonPhrase,
  Authority,
  RequestTarget,
  Header,
  TrailerPlan,
  EffectiveUri,
  DecodeRequest,
  DecodeResponse,
  EncodeRequest,
  EncodeResponse,
  ClientExchange,
  ServerExchange,
  RunClient,
  RunServer,
}

export enum HttpPhase {
  Input,
  StartLine,
  HeaderSection,
  Authority,
  Framing,
  Body,
  ChunkControl,
  TrailerSection,
  Expectation,
  Control,
  Cleanup,
}

export function kind(error: &HttpError): HttpErrorKind;
export function operation(error: &HttpError): HttpOperation;
export function phase(error: &HttpError): HttpPhase;
export function byteOffset(error: &HttpError): Option<u64>;
export function fieldIndex(error: &HttpError): Option<u64>;
export function message(error: &HttpError): &string;
export function messageTruncated(error: &HttpError): bool;
```

Offsets are zero-based in the current message/codec input domain and absent when no honest location exists. Field index is zero-based parsed order. Messages are bounded and curated. Default display/message never includes a raw target, field value, body, credentials, digest/signature value, native handle or recursively formatted arbitrary lower error. Callers inspect owned values and typed lower causes explicitly; messages are not classification inputs.

HttpError covers HTTP value/protocol/codec/state failures only. A valid final status, including4xx/5xx, is ordinary data. Lower transport, clock, cancellation/deadline and arbitrary application errors are separate below.

## 3. Methods, versions, statuses and reason phrases

Opaque owned `Method`, `HttpVersion`, `Status`, `ReasonPhrase` have explicit copy/eq/hash where their values support it and:

```rill
export function method(bytes: Bytes): Result<Method,HttpError>;
export function methodBytes(method: &Method): &Bytes;
export function getMethod(): Method;
export function headMethod(): Method;
export function postMethod(): Method;
export function optionsMethod(): Method;
export function connectMethod(): Method;

export function httpVersion(major: u8, minor: u8): Result<HttpVersion,HttpError>;
export function http10(): HttpVersion;
export function http11(): HttpVersion;
export function versionMajor(version: &HttpVersion): u8;
export function versionMinor(version: &HttpVersion): u8;

export function status(code: u16): Result<Status,HttpError>;
export function statusCode(status: &Status): u16;

export function reasonPhrase(bytes: Bytes): Result<ReasonPhrase,HttpError>;
export function emptyReasonPhrase(): ReasonPhrase;
export function reasonBytes(reason: &ReasonPhrase): &Bytes;
```

Method is a nonempty checked token, byte-exact and case-sensitive; unfamiliar methods remain valid. Named factories do not close the set or normalize another spelling. Status admits100–599, including unknown codes. ReasonPhrase is opaque valid field-text bytes with no status/error semantics or UTF8 requirement; reject CR/LF/NUL/forbidden controls. Outgoing default is empty, with required status-line spacing; no English table.

HTTP-version syntax is exactly case-sensitive `HTTP/DIGIT.DIGIT`. A parsed higher1.x minor is retained and processed with implemented1.1 semantics. An unsupported syntactically valid major is UnsupportedVersion; malformed spelling is InvalidSyntax. 505 is only an understood major refusal, not an HTTP1.0 or higher1.x-minor error. Senders emit HTTP1.1 and never advertise an unimplemented minor.

## 4. Encoded URI, authority and request target

`std::http` exports opaque owned `Authority`, `OriginTarget`, `HttpUri`, `RequestTarget` and:

```rill
export enum HttpScheme { Http, Https }

export enum RequestTarget {
  OriginForm(OriginTarget),
  AbsoluteForm(HttpUri),
  AuthorityForm(Authority),
  Asterisk,
}

export type Origin = {
  scheme: HttpScheme;
  authority: Authority;
};

export type ServerUriContext = {
  scheme: HttpScheme;
  defaultAuthority: Option<Authority>;
};

export function authority(bytes: Bytes): Result<Authority,HttpError>;
export function authorityBytes(authority: &Authority): &Bytes;
export function httpUri(bytes: Bytes): Result<HttpUri,HttpError>;
export function uriBytes(uri: &HttpUri): &Bytes;
export function requestTarget(bytes: Bytes): Result<RequestTarget,HttpError>;
export function targetBytes(target: &RequestTarget): &Bytes;
export function effectiveUri(
  request: &RequestHead,
  context: &ServerUriContext,
): Result<HttpUri,HttpError>;
```

Constructors consume and preserve validated encoded ASCII spelling. They validate percent triplets without decoding, reject fragments, raw controls/spaces, unencoded non-ASCII and HTTP(S) userinfo, and perform no dot/path normalization, query-map conversion, IDNA, browser URL interpretation or repeated percent-decoding. An origin path beginning `//` remains a path, not an authority.

Authority recognizes URI IP-literal, IPv4-like and reg-name syntax, preserving brackets/encoded spelling and syntactic optional port. Generic URI port grammar is not a checked TCP u16; URI reg-name is broader than DNS or TLS reference identity. HTTP(S) origins require nonempty host and scheme-specific port interpretation. There is no implicit Authority-to-SocketAddress/resolver/ServerName conversion or proof the injected transport is authenticated for an Origin. Application composition performs explicit decoding/name/port/dial/TLS policy.

Complete request validation enforces ordinary method origin-form/accepted absolute-form, OPTIONS asterisk allowance, and CONNECT authority-form. Direct clients emit origin-form except explicit asterisk OPTIONS. Authority-form recognition does not enable a tunnel.

Effective URI keeps original RequestTarget separately. Absolute-form uses target authority and ignores Host for routing reconstruction after Host validity checking. Origin/asterisk uses valid Host or explicit default authority where version/context allows. Missing context returns InvalidInput; never infer local socket, Forwarded or X-Forwarded values.

## 5. Headers and heads

Opaque owned `HeaderName`, `HeaderValue`, `Header`, `Headers` use:

```rill
export function headerName(bytes: Bytes): Result<HeaderName,HttpError>;
export function headerNameBytes(name: &HeaderName): &Bytes;
export function headerValue(bytes: Bytes): Result<HeaderValue,HttpError>;
export function headerValueBytes(value: &HeaderValue): &Bytes;
export function headerValueText(value: &HeaderValue): Result<&string,Utf8Error>;
export function header(name: HeaderName, value: HeaderValue): Header;
export function headerNameOf(field: &Header): &HeaderName;
export function headerValueOf(field: &Header): &HeaderValue;
export function headers(fields: List<Header>): Headers;
export function fields(headers: &Headers): &List<Header>;
export function append(headers: &mut Headers, field: Header): void;
export function values(headers: &Headers, name: &HeaderName): List<&HeaderValue>;
export function requireOne(
  headers: &Headers,
  name: &HeaderName,
): Result<Option<&HeaderValue>,HttpError>;
export function removeAll(
  headers: &mut Headers,
  name: &HeaderName,
): List<Header>;
export function replaceAll(
  headers: &mut Headers,
  name: HeaderName,
  values: List<HeaderValue>,
): void;
```

HeaderName is nonempty token bytes with ASCII case-insensitive eq/hash while preserving spelling. HeaderValue permits empty/opaque field-value bytes, including valid obs-text, and rejects CR/LF/NUL and forbidden controls. Parsing strips surrounding SP/HTAB OWS and preserves internal bytes. Explicit construction rejects leading/trailing OWS rather than silently trim. Strict text access preserves Utf8Error, never replacement text.

Headers preserves order, repetition and name spelling. `values` returns source-dependent borrows. `requireOne` errors on repetition. `replaceAll` inserts ordered new values at the first removed position or appends if absent; unrelated fields retain order; empty list removes all. Generic operations never comma-join/split/deduplicate or impose field semantics. Unknown fields remain repeatable opaque data.

Serialization uses preserved field-name spelling, canonical `: ` and CRLF. No original colon-whitespace reconstruction. Obs-fold and invalid bytes reject.

Opaque private-field owning `RequestHead` and `ResponseHead` prevent bypassing complete basic invariants:

```rill
export function requestHead(
  method: Method,
  target: RequestTarget,
  version: HttpVersion,
  headers: Headers,
): Result<RequestHead,HttpError>;

export function responseHead(
  version: HttpVersion,
  status: Status,
  reason: ReasonPhrase,
  headers: Headers,
): Result<ResponseHead,HttpError>;

export function requestMethod(head: &RequestHead): &Method;
export function requestTargetValue(head: &RequestHead): &RequestTarget;
export function requestVersion(head: &RequestHead): &HttpVersion;
export function requestHeaders(head: &RequestHead): &Headers;
export function responseVersion(head: &ResponseHead): &HttpVersion;
export function responseStatus(head: &ResponseHead): &Status;
export function responseReason(head: &ResponseHead): &ReasonPhrase;
export function responseHeaders(head: &ResponseHead): &Headers;
```

Method/target/basic head context is validated at construction. Framing/Host/Connection/Expect/TE/Trailer rules depending on role/body plan are validated on complete message admission/codec parsing before a head can route an application request. Parsed heads own data; no parser-buffer views or partial validated heads escape.

## 6. Field-specific authority, connection and version rules

For HTTP1.1/higher1.x requests require exactly one syntactically valid Host field line. HTTP1.0 Host is optional; if present it must be one valid line. Repeated Host always rejects, even equal/comma-combined. Absolute-form uses target authority and ignores Host for effective routing; no built-in lexical target-authority-equals-Host rejection. Direct clients generate/validate Host from bound Origin. Deployment accepted-origin policy is external.

Connection parsing combines every matching field value as its field-defined comma token list while generic Headers remains uncombined. `close` forces nonreuse. `TE: trailers` requires Connection token TE. Unknown connection-option tokens remain policy data and cannot enable proxy/switch behavior. Outgoing framing/close/expect/trailer settings generate or validate controlled fields; raw headers cannot contradict/override them.

Clients and servers emit HTTP1.1. Higher1.x requests are processed as1.1 and answered1.1. HTTP1.0 requests are accepted close-only: response is HTTP1.1 but uses interpretable1.0 features, no Transfer-Encoding and Connection close. Optional legacy keep-alive is not implemented. HTTP1.0 input containing Transfer-Encoding is faulty framing/close.

## 7. Body and trailer plans

```rill
export enum RequestBodyPlan {
  None,
  Fixed(u64),
  Chunked(TrailerPlan),
}

export enum ResponseBodyPlan {
  Bodyless,
  Fixed(u64),
  Chunked(TrailerPlan),
  CloseDelimited,
}

export function contentDigestTrailer(): TrailerName;
export function reprDigestTrailer(): TrailerName;
export function assertExtensionTrailer(
  name: HeaderName,
): Result<TrailerName,HttpError>;
export function trailerPlan(
  names: List<TrailerName>,
): Result<TrailerPlan,HttpError>;
export function acceptsTrailers(request: &RequestHead): bool;
```

Request plans cannot close-delimit. Response Bodyless is contextual: HEAD/304 terminate at header boundary and may carry legal hypothetical CL/TE metadata;1xx/204 terminate there and prohibit CL/TE. 205 uses Fixed(0), not Bodyless; incoming ordinary framing must contain zero content. A zero terminal chunk can frame zero 205 content; trailer section still validates.

Fixed sends admit only a whole chunk fitting remaining content length. Finish succeeds only at exactly zero remaining. No padding/truncation/repair. Chunked empty application sends are validated no-ops; only finish emits zero terminal chunk and trailers. CloseDelimited finish marks content complete, flushes and requires connection end; no trailers/reuse/request use.

Outgoing trailer permission is positive. Content-Digest/Repr-Digest are specification-backed built-ins. `assertExtensionTrailer` is the caller's explicit assertion that the field definition permits trailers; it rejects known core prohibitions including Host, Content-Length, Transfer-Encoding, Connection, TE and Trailer. It grants no native/compiler authority and does no digest/signature/auth work.

TrailerPlan is ordered and case-insensitively unique; encoder generates/validates initial Trailer declaration. Actual finish trailers can be a subset and field-defined repeated values; unplanned name rejects. Incoming structurally valid unknown/undeclared trailer fields remain ordered in separate Headers. Reject malformed and known prohibited placement, never merge into initial headers or redo routing/auth. Receipt does not prove sender permission/integrity/identity.

`acceptsTrailers` performs checked TE/Connection semantics. Client advertisement is explicit request option, default off, generating `TE: trailers` and `Connection: TE`; it advertises no other coding and does not promise downstream retention/understanding.

## 8. Strict HTTP/1 framing

Complete validation requires:

- exactly one Content-Length field line;
- one nonempty digits-only checked u64 value; leading zeros valid;
- no comma-list or repeated CL, even apparently equal;
- reject every Transfer-Encoding plus Content-Length combination, including bodyless metadata contexts;
- actual transfer coding exactly one final chunked;
- reject repeated chunked and every other actual transfer coding;
- distinguish malformed framing from syntactically valid unsupported coding;
- validate/bound chunk extension token/quoted syntax but ignore unknown semantics.

No first/last wins, equal-value repair, CL removal or hidden decoder fallback. Content-Encoding is application data; no automatic decompression.

HEAD/304 hypothetical framing is validated metadata but never consumed.1xx/204 CL/TE rejects. HEAD request method does not erase request body framing. 205 positive fixed content or positive chunks fail; zero framing is valid.

Message completion requires all framing and trailer terminator, not zero-size chunk alone. Close-delimited response completes only on real lower clean EOF after TCP/TLS distinction. Fixed/chunked/bodyless do not probe EOF after framing completion. Cancellation/timeout/generic fault is never clean EOF.

## 9. Limits

```rill
export type HttpLimits = {
  startLineBytes: u64;
  headBytes: u64;
  fieldsPerHead: u64;
  fieldLineBytes: u64;
  trailerBytes: u64;
  trailerFields: u64;
  chunkControlLineBytes: u64;
  chunkExtensionBytes: u64;
  informationalResponses: u64;
  bodyChunkBytes: u64;
  queuedBodyBytes: u64;
  queuedControlMessages: u64;
  queuedControlBytes: u64;
  codecIngressBytes: u64;
  codecEgressBytes: u64;
  totalBodyBytes: Option<u64>;
};

export function httpLimits(): HttpLimits;
```

Defaults respectively:8192,65536,128,16384,16384,64,4096,1048576,16,65536,1048576 per direction,32,1048576,65536,65536,None. Ordinary public fields can be edited through a mutable binding before consumption; explicit copy creates another snapshot. Every finite per-item/section/queue/staging field must be positive. `Some(0)` total body is valid zero-only. Validate checked arithmetic/target/native representability before allocation/narrowing; no clamping or zero-as-unlimited.

Total counts dechunked message-content bytes before any Content-Encoding interpretation, independently per message direction with checked u64. Reject excessive declared fixed length before emission/dispatch where possible; streamed excess fails at exact boundary. No successful truncation.

HTTP-owned queues/staging are not lower budgets. Application chunks up to65536 segment into ri-w0c writes up to16384. Increasing HTTP values cannot raise TLS/native caps or borrow unused layer budget. First applicable limit can fail; no global memory/CPU/security quota is claimed.

## 10. Public HTTP/1 decoder

`std::http::h1` exports opaque `RequestDecoder`, `ResponseDecoder` and:

```rill
export enum IncomingBodyPlan {
  None,
  Fixed(u64),
  Chunked,
  CloseDelimited,
}

export enum RequestDecodeEvent {
  Head { head: RequestHead; body: IncomingBodyPlan },
  Chunk(Bytes),
  Complete(Headers),
}

export enum ResponseDecodeEvent {
  Informational(ResponseHead),
  Final { head: ResponseHead; body: IncomingBodyPlan },
  Chunk(Bytes),
  Complete(Headers),
}

export enum DecodeSchedule { Runnable, NeedInput, Complete }

export type DecodeStep<E> = {
  consumed: u64;
  event: Option<E>;
  schedule: DecodeSchedule;
};

export type DecodeFailure = {
  consumed: u64;
  error: Err<HttpError>;
};

export function requestDecoder(limits: HttpLimits): Result<RequestDecoder,HttpError>;
export function responseDecoder(
  requestMethod: Method,
  limits: HttpLimits,
): Result<ResponseDecoder,HttpError>;
export function decodeRequest(
  decoder: &mut RequestDecoder,
  input: &Bytes,
): Result<DecodeStep<RequestDecodeEvent>,DecodeFailure>;
export function decodeResponse(
  decoder: &mut ResponseDecoder,
  input: &Bytes,
): Result<DecodeStep<ResponseDecodeEvent>,DecodeFailure>;
export function endRequestInput(
  decoder: &mut RequestDecoder,
): Result<DecodeStep<RequestDecodeEvent>,HttpError>;
export function endResponseInput(
  decoder: &mut ResponseDecoder,
): Result<DecodeStep<ResponseDecodeEvent>,HttpError>;
export function resetRequestDecoder(
  decoder: &mut RequestDecoder,
): Result<void,HttpError>;
export function resetResponseDecoder(
  decoder: &mut ResponseDecoder,
  nextRequestMethod: Method,
): Result<void,HttpError>;
```

Each decoder handles one message. Response method context handles HEAD/CONNECT. Input borrow ends on return; decoder copies only bounded partial syntax state. `consumed` is exact current input prefix, including failure; caller owns/resubmits suffix unchanged. At most one owned event/step. Zero+NeedInput valid; zero+Runnable must represent actual local event/work, not spin.

CloseDelimited is response-only. Bodyless still emits Complete(empty Headers). Informational opens no body. Chunks obey bodyChunk/total limits. Complete carries separate trailers once. Complete schedule means event transferred and no input until reset. Reset before completion/retained event rejects and moves no suffix.

Explicit endInput is clean lower EOF only. It completes valid close-delimited response; unfinished syntax/fixed/chunked/request is IncompleteMessage. TCP EOF/TLS close-notify distinction is settled below codec.

Strict octet parsing requires CRLF, rejects bare LF/obs-fold/whitespace-before-colon/forbidden values, receives entire bounded head before dispatch and applies full role/context framing/Host rules. No Unicode-first parser or repair path.

## 11. Public HTTP/1 encoder

```rill
export type RequestEncodeContext = {
  peerVersion: PeerVersionKnowledge;
};

export type ResponseEncodeContext = {
  requestMethod: Method;
  requestVersion: HttpVersion;
};

export type EncodedBatch = {
  pieces: List<Bytes>;
  contentAccepted: u64;
  messageComplete: bool;
};

export type EncodeBodyRejection = {
  error: Err<HttpError>;
  input: Bytes;
};
export enum EncodeBodyOutcome {
  Encoded(EncodedBatch),
  Rejected(EncodeBodyRejection),
}

export type EncodeFinishRejection = {
  error: Err<HttpError>;
  trailers: Headers;
};
export enum EncodeFinishOutcome {
  Encoded(EncodedBatch),
  Rejected(EncodeFinishRejection),
}

export function requestEncoder(
  head: RequestHead,
  body: RequestBodyPlan,
  context: RequestEncodeContext,
  limits: HttpLimits,
): Result<[RequestEncoder,EncodedBatch],HttpError>;
export function responseEncoder(
  head: ResponseHead,
  body: ResponseBodyPlan,
  context: ResponseEncodeContext,
  limits: HttpLimits,
): Result<[ResponseEncoder,EncodedBatch],HttpError>;
export function encodeRequestBody(
  encoder: &mut RequestEncoder,
  input: Bytes,
): EncodeBodyOutcome;
export function encodeResponseBody(
  encoder: &mut ResponseEncoder,
  input: Bytes,
): EncodeBodyOutcome;
export function finishRequestEncoder(
  encoder: RequestEncoder,
  trailers: Headers,
): EncodeFinishOutcome;
export function finishResponseEncoder(
  encoder: ResponseEncoder,
  trailers: Headers,
): EncodeFinishOutcome;
```

Construction validates complete head/plan/context, controlled Host/framing/Connection/TE/Trailer/Expect fields and returns complete initial head batch with no content. Failure exposes no encoded prefix. Request context gates chunked knowledge; response context enforces method/no-body/recipient-version rules. Head bytes use HTTP1.1 output.

Body excess/state/limit rejects before consuming and returns input. Successful batch owns/moves content. Pieces preserve exact wire order; chunked can return prefix, moved content, CRLF separately rather than oversized copy. `contentAccepted` is application content represented, not transport handoff. Empty no-op can return no pieces.

Finish consumes encoder. Validate fixed remainder, trailer plan/subset/permissions and state before terminal bytes. Rejection returns unchanged trailers; prior batches remain irreversible/incomplete. Chunked emits terminal framing/trailers; fixed/bodyless normally no bytes; close-delimited marks message complete and driver closes connection. Caller hands every piece in order and retains lower unaccepted tails.

## 12. Controls and request options

```rill
export type HttpControl = {
  deadline: Option<[Clock,Instant]>;
  cancellation: CancellationToken;
};
export function uncontrolled(): HttpControl;

export type ExpectContinue = {
  clock: Clock;
  wait: SignedDuration;
};

export type ClientRequestOptions = {
  receiveTrailers: bool;
  expectContinue: Option<ExpectContinue>;
};
export function clientRequestOptions(): ClientRequestOptions;

export enum PeerVersionKnowledge {
  Unknown,
  Http11OrLater,
}
```

HttpControl is non-implicitly-copyable, noBorrow/sendable/shareable. Explicit copy retains same handles/state/absolute deadline; uncontrolled uses None+never token without a clock read. It is consumed per client exchange/server next request and remains through head/body/trailer/application backpressure/terminal protocol work. No ambient/permanent connection control or timeout renewal.

Preflight is basic/domain validation, already-requested token, needed clock/expiry then admission. No unneeded clock read. Post-admission completion/cancel/expiry/clock races select once with unspecified ready tie; cleanup never rewrites. Cleanup can outlast deadline indefinitely; neither outcome proves remote nonexecution.

Expect clock/wait is separate. Negative duration rejects; zero makes body immediately eligible after expectation, no forced yield. Timer failure terminally reports original clock cause. Server valid expectation requires explicit accept body or final response; it never waits for body first. Unsupported expectation gets417+close before dispatch. Client has no automatic retry without Expect.

PeerVersionKnowledge is connection-level explicit assertion/cached valid response knowledge. Unknown rejects chunked request before head. HTTP1.0 response makes connection close-only. Knowledge says no transport authentication.

## 13. Body, response and completion endpoint values

Opaque noncopyable/nonshareable `IncomingBody`, `OutgoingBody`, `ClientResponses`, `ResponseSender`, and generic `ExchangeCompletion<RE,WE,FE,CE,DE>` own channel/protocol state only. They never own/expose the transport/codec driver. Distinct endpoints can move to scoped/owned routines under actual sendability evidence; calls on one endpoint require exclusive access.

```rill
export enum BodyInterruption {
  ExchangeFailed,
  EarlyFinal,
  ApplicationAbandoned,
  TerminalStop,
}

export enum BodyEvent {
  Chunk(Bytes),
  Complete(Headers),
  Interrupted(BodyInterruption),
}

export enum BodySendRejectionReason {
  LimitExceeded,
  FixedLengthExceeded,
  Finished,
  Interrupted(BodyInterruption),
}
export type BodySendRejection = {
  reason: BodySendRejectionReason;
  input: Bytes;
};
export enum BodySendOutcome {
  Accepted,
  Rejected(BodySendRejection),
}

export enum BodyFinishRejectionReason {
  Invalid(Err<HttpError>),
  Interrupted(BodyInterruption),
}
export type BodyFinishRejection = {
  reason: BodyFinishRejectionReason;
  trailers: Headers;
};
export enum BodyFinishOutcome {
  Accepted,
  Rejected(BodyFinishRejection),
}

export function nextBody(body: &mut IncomingBody): Option<BodyEvent>;
export function sendBody(body: &mut OutgoingBody, input: Bytes): BodySendOutcome;
export function finishBody(body: OutgoingBody, trailers: Headers): BodyFinishOutcome;
```

Incoming ordered chunks precede terminal Complete/Interrupted. Complete only after verified framing/trailer terminator and transfers once; subsequent next returns None immediately. Failure never becomes EOF/success. Outgoing send may suspend on bounded queue credits; rejection returns unchanged. Accepted means endpoint/driver owns data, not lower/peer acceptance. Finish Accepted means terminal work owned; inspect completion. Drop unfinished endpoint requests abandonment, not empty body/terminal framing.

```rill
export enum ClientResponseEvent {
  Informational(ResponseHead),
  Final { head: ResponseHead; body: IncomingBody },
}
export function nextClientResponse(
  responses: &mut ClientResponses,
): Option<ClientResponseEvent>;
```

Zero or more bounded informational then one final. Valid error status is Final. Final transfers once and responses then None. Failure before final can return None; actual cause is completion. Unexpected101/successful CONNECT is failure, no final body/raw stream.

Server response operations use:

```rill
export enum ResponseCommandRejectionReason {
  Invalid(Err<HttpError>),
  Interrupted(BodyInterruption),
}
export enum ResponseCommandOutcome {
  Accepted,
  Rejected(ResponseCommandRejectionReason),
}

export type InformationalRejection = {
  reason: ResponseCommandRejectionReason;
  head: ResponseHead;
};
export enum InformationalOutcome {
  Accepted,
  Rejected(InformationalRejection),
}

export type FinalResponseAdmission = {
  body: Option<OutgoingBody>;
};
export type FinalResponseRejection = {
  reason: ResponseCommandRejectionReason;
  head: ResponseHead;
  body: ResponseBodyPlan;
};
export enum FinalResponseOutcome {
  Admitted(FinalResponseAdmission),
  Rejected(FinalResponseRejection),
}

export function acceptRequestBody(
  response: &mut ResponseSender,
): ResponseCommandOutcome;
export function sendInformational(
  response: &mut ResponseSender,
  head: ResponseHead,
): InformationalOutcome;
export function startFinalResponse(
  response: &mut ResponseSender,
  head: ResponseHead,
  body: ResponseBodyPlan,
): FinalResponseOutcome;
```

Local rejection returns head/plan and leaves capability usable. Accepting request body emits100 if still appropriate; arrived body bytes can make100 unnecessary but explicit acceptance remains. Final admission closes informational commands and returns optional body. Successful CONNECT/101 rejects before bytes.

## 14. Exchange failure, observations and reuse

```rill
export enum HttpFailure<RE,WE,FE,CE,DE> {
  Protocol(Err<HttpError>),
  TransportRead(ReadFailure<RE>),
  TransportWrite(IoFailure<WE>),
  TransportFlush(Err<FE>),
  TransportClose(Err<CE>),
  TransportDriver(DriverFailureSet<DE>),
  Clock(Err<ClockError>),
}

export enum ReuseDisposition { Eligible, MustClose, Unusable }

export type OutboundObservation = {
  headAccepted: bool;
  bodyQueued: u64;
  bodyTransportAccepted: u64;
  messageComplete: bool;
  handoffComplete: bool;
};
export type InboundObservation = {
  finalHeadReceived: bool;
  bodyParsed: u64;
  bodyDelivered: u64;
  messageComplete: bool;
};
export type ExchangeProgress = {
  outbound: OutboundObservation;
  inbound: InboundObservation;
};

export enum ExchangeTerminal<RE,WE,FE,CE,DE> {
  Completed,
  Cancelled,
  DeadlineExpired,
  Failed(HttpFailure<RE,WE,FE,CE,DE>),
}
export type ExchangeOutcome<RE,WE,FE,CE,DE> = {
  terminal: ExchangeTerminal<RE,WE,FE,CE,DE>;
  cleanup: Option<HttpFailure<RE,WE,FE,CE,DE>>;
  progress: ExchangeProgress;
  reuse: ReuseDisposition;
};

export function completeExchange<RE,WE,FE,CE,DE>(
  completion: ExchangeCompletion<RE,WE,FE,CE,DE>,
): ExchangeOutcome<RE,WE,FE,CE,DE>;
```

Lower envelopes/progress/errors stay exact; no common conversion/message parsing. TLS instantiates all lower E as TlsError; TCP as NetError; user drivers can differ. Cancellation/deadline are outcomes, not HttpError. Selected terminal and cleanup failure are independent; completely framed can remain Completed with cleanup failure and Unusable. Return may outlast deadline.

Observations count content bytes, not HTTP framing: application queue admission, lower ri-w0c acceptance, decoder parsing and app delivery remain distinct. headAccepted/handoff/messageComplete are layer-specific. Counts checked u64; no ciphertext/peer conversion. None implies peer receipt/execution/nonexecution/rollback/replay safety.

Eligible requires verified both boundaries, output handoff, no mandated close/abandon/switch/pending operation/known fault. MustClose is valid message with version/framing/Connection close. Unusable is ambiguity/incompletion/early final/failure/poison. Eligible is no future-health/pool promise.

Completion owns actual terminal error once. Disposal relinquishes receiving ownership; runner adopts undelivered outcome without cancelling. Body/response endpoints carry classifications only, avoiding cause cloning. Terminal state/wake is independent of full body queues.

## 15. Client connection driver

`std::http::client` exports opaque generic `ClientDriver<D,G,RE,WE,FE,CE,DE>`, `ClientConnection<RE,WE,FE,CE,DE>`, `ClientExchange<RE,WE,FE,CE,DE>`.

```rill
export enum ClientAdmissionReason {
  Busy,
  ConnectionClosed,
  Invalid(Err<HttpError>),
}
export type ClientAdmissionRejection = {
  reason: ClientAdmissionReason;
  request: RequestHead;
  body: RequestBodyPlan;
  options: ClientRequestOptions;
  control: HttpControl;
};
export enum ClientAdmissionOutcome<RE,WE,FE,CE,DE> {
  Admitted(ClientExchange<RE,WE,FE,CE,DE>),
  Rejected(ClientAdmissionRejection),
}
```

Logical ClientExchange owns `requestBody:Option<OutgoingBody>`, `responses:ClientResponses`, `completion:ExchangeCompletion<...>` through private fields. No-body/fixed-zero auto-completes outbound and has None. It decomposes only by consuming the owner:

```rill
export type ClientExchangeParts<RE,WE,FE,CE,DE> = {
  requestBody: Option<OutgoingBody>;
  responses: ClientResponses;
  completion: ExchangeCompletion<RE,WE,FE,CE,DE>;
};
export function intoClientExchange<RE,WE,FE,CE,DE>(
  exchange: ClientExchange<RE,WE,FE,CE,DE>,
): ClientExchangeParts<RE,WE,FE,CE,DE>;

export function startClientExchange<RE,WE,FE,CE,DE>(
  connection: &mut ClientConnection<RE,WE,FE,CE,DE>,
  request: RequestHead,
  body: RequestBodyPlan,
  options: ClientRequestOptions,
  control: HttpControl,
): ClientAdmissionOutcome<RE,WE,FE,CE,DE>;

export function shutdownClient<RE,WE,FE,CE,DE>(
  connection: ClientConnection<RE,WE,FE,CE,DE>,
): void;
```

One active exchange. Admission may suspend coordinating with explicit runner; second is Busy, never queued. Pre-command local reject returns all unchanged. After admission outcome owns terminal; no request/control recovery. Client bound Origin enforces same direct origin and generated Host. Initial/cached peer version gates chunking.

Early final stops new upload admission, request body becomes Interrupted(EarlyFinal), rejects new input unchanged and makes connection Unusable. Accepted channel chunks are not recovered/replayed; distinguish queued/lower acceptance. Preserve native retry state. Continue receiving final response under original control when possible, without full-response/bounded promise.

No client pipelining. After completed response any already-read unexpected response bytes are protocol failure/unusable. No raw suffix.

## 16. Server connection driver

`std::http::server` exports opaque generic `ServerDriver<D,G,RE,WE,FE,CE,DE>`, `ServerConnection<RE,WE,FE,CE,DE>`, `ServerExchange<RE,WE,FE,CE,DE>`.

Logical ServerExchange owns RequestHead, IncomingBody, ResponseSender and ExchangeCompletion through private fields. The response can progress before request body completion. Next exchange is not delivered until prior terminal release. Consuming decomposition moves every endpoint once:

```rill
export type ServerExchangeParts<RE,WE,FE,CE,DE> = {
  request: RequestHead;
  requestBody: IncomingBody;
  response: ResponseSender;
  completion: ExchangeCompletion<RE,WE,FE,CE,DE>;
};
export function intoServerExchange<RE,WE,FE,CE,DE>(
  exchange: ServerExchange<RE,WE,FE,CE,DE>,
): ServerExchangeParts<RE,WE,FE,CE,DE>;

export enum ServerExchangeRejectionReason {
  Busy,
  ConnectionClosed,
  Invalid(Err<HttpError>),
}
export type ServerExchangeRejection = {
  reason: ServerExchangeRejectionReason;
  control: HttpControl;
};
export enum ServerExchangeOutcome<RE,WE,FE,CE,DE> {
  Exchange(ServerExchange<RE,WE,FE,CE,DE>),
  Rejected(ServerExchangeRejection),
  ConnectionEnded,
}

export function nextServerExchange<RE,WE,FE,CE,DE>(
  connection: &mut ServerConnection<RE,WE,FE,CE,DE>,
  control: HttpControl,
): ServerExchangeOutcome<RE,WE,FE,CE,DE>;

export function shutdownServer<RE,WE,FE,CE,DE>(
  connection: ServerConnection<RE,WE,FE,CE,DE>,
): void;
```

Control is supplied/admitted before waiting for head; buffered read-ahead cannot bypass. Pre-admission rejected returns unchanged control. Once admitted it belongs to pending wait and is not recovered on terminal failure. Server parses sequential requests; bounded bytes for next request wait for next control. No parallel handlers/pipelining dispatch.

Invalid request never dispatches. Best-effort automatic response then close:400 syntax/framing/Host;501 unsupported transfer coding;505 unsupported major;417 unsupported expectation;431 excessive head;413 excessive body. Preserve original HttpError terminal and secondary output/cleanup uncertainty. Do not respond when insufficient safe syntax or unusable transport.

## 17. Construction and runner lifecycle

```rill
export type ConnectionConstructionFailure<RE,WE,FE,CE,DE> = {
  setup: Err<HttpError>;
  cleanup: Option<HttpFailure<RE,WE,FE,CE,DE>>;
};

export enum ConnectionTerminal<RE,WE,FE,CE,DE> {
  Orderly,
  ApplicationClosed,
  ExchangeEnded,
  Failed(HttpFailure<RE,WE,FE,CE,DE>),
}
export type ConnectionRunOutcome<RE,WE,FE,CE,DE> = {
  terminal: ConnectionTerminal<RE,WE,FE,CE,DE>;
  cleanup: Option<HttpFailure<RE,WE,FE,CE,DE>>;
  recoveredExchange: Option<ExchangeOutcome<RE,WE,FE,CE,DE>>;
  exchanges: u64;
};
```

Conceptual exact constructor/runner shapes:

```rill
export function clientConnection<D,G,RE,WE,FE,CE,DE>(
  lower: D,
  origin: Origin,
  peerVersion: PeerVersionKnowledge,
  limits: HttpLimits,
): Result<
  [
    ClientDriver<D,G,RE,WE,FE,CE,DE>,
    ClientConnection<RE,WE,FE,CE,DE>,
  ],
  ConnectionConstructionFailure<RE,WE,FE,CE,DE>
>;

export function serverConnection<D,G,RE,WE,FE,CE,DE>(
  lower: D,
  limits: HttpLimits,
): Result<
  [
    ServerDriver<D,G,RE,WE,FE,CE,DE>,
    ServerConnection<RE,WE,FE,CE,DE>,
  ],
  ConnectionConstructionFailure<RE,WE,FE,CE,DE>
>;

export function runClient<D,G,RE,WE,FE,CE,DE>(
  driver: ClientDriver<D,G,RE,WE,FE,CE,DE>,
): ConnectionRunOutcome<RE,WE,FE,CE,DE>;

export function runServer<D,G,RE,WE,FE,CE,DE>(
  driver: ServerDriver<D,G,RE,WE,FE,CE,DE>,
): ConnectionRunOutcome<RE,WE,FE,CE,DE>;
```

Constructors require lower `admitRead`, admitWrite, admitFlush, admitClose, requestStop, advance(D,RE,WE,FE,CE,DE), registerProgressWait(D,G,DE), withdrawProgressWait(G,DE), plus actual noBorrow/sendable evidence required to retain/pass errors and owned values. Display only where diagnostic formatting requires it. There is no trait/associated output/dyn registry.

Construction consumes D; invalid limits/origin/setup has no HTTP wire effect but terminally stops/joins lower and returns typed setup+optional cleanup. No partial driver/raw recovery. It may wait indefinitely for mandatory release.

Constructors do not spawn. Runner consumes sole protocol coordinator and caller explicitly runs it in scoped/owned work and joins. It coordinates ri-w0c progress, codec, channel watches, controls/timers; invokes no app callbacks. Endpoint owns channels only. Runner returns after every exchange outcome transferred or adopted, all channels/watches/timers/registrations quiesced and lower released. `exchanges` is checked admitted count, not success count.

Exchange outcome has exactly one adopter. If completion alive, it owns outcome and runner terminal ExchangeEnded classification only; if gone, runner recoveredExchange owns it. Endpoint drop terminally stops, active exchange ApplicationAbandoned/Unusable. Consuming shutdown forbids new work, permits active already-selected terminal then orderly local close; drop is not orderly/graceful. Runner never returns raw lower/transport.

Connection failure after valid message completion cannot rewrite that exchange terminal; cleanup remains separate. Disposal/joins may return indefinitely late. No hidden global pool, task or liveness guarantee when app/peer refuses required progress.

## 18. CONNECT, upgrades and direct-origin policy

Valid CONNECT token/authority-form is recognized. Server cannot emit successful2xx CONNECT or101. Default non-success CONNECT is501+close; app may choose another non-success but still close. Ordinary Upgrade request can receive normal non101 response. Client unexpected101 or successful CONNECT is UnsupportedSwitch/Unusable; preserve read facts, terminally release and never expose/reinterpret optimistic bytes. RFC9931 anti-smuggling implications are met by exclusion/close, not proxy functionality.

Direct client usually origin-form and one bound Origin. Server accepts absolute-form as origin recipient, with target authority precedence. No proxy discovery/routing/tunnel, trusted Forwarded interpretation or raw transport escape.

## 19. Evidence and delivery gates

Evidence labels follow [RFC conventions](rfcs/README.md#examples-and-evidence):

- RFC9110/9112/3986/9931 and OpenSSL/duplex notes in `research/0039-*` are supporting **research**, not conformance/security execution.
- Prototype25 declarations/scenarios are **HandChecked**, not parser/compiler/runtime execution.
- ri-w0c's OpenSSL matrix is lower-layer **ReferenceExecuted**, not HTTP evidence.
- HTTP codec/runner/Rill/native/conformance/security and live target support are **NotChecked** here.

ri-19d owns HTTP implementation and validation. ri-hw5 owns shared progress/TCP delivery; ri-ei7 owns TLS delivery. ri-48g owns review-ready RFC drafting; neither design closure nor drafting is human RFC acceptance/software support.

Required delivery includes split/coalesced codec inputs; all strict line/field/Host/CL/TE/chunk/trailer/version/Expect/CONNECT/upgrade cases; limits and full-queue terminal independence; partial lower effects/exact causes; cancellation/deadline/drop/cleanup races; HTTP1.0 close/noTE; plain/TLS direct client and sequential server; malformed/smuggling corpora and all supported targets. Finite cases are not a universal conformance/security proof. Contradiction returns to the human/map rather than weakening strict parsing, general streaming, ownership, bounds or typed outcomes.
