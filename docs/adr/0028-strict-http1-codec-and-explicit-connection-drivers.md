# Strict HTTP/1 codec and explicit connection drivers

Rill’s standard HTTP foundation uses owning shared values, a public transport-independent strict HTTP/1 codec, and explicit client/server driver-plus-endpoint pairs over #45 progress drivers. This avoids coupling protocol parsing to sockets/TLS or application callbacks, and avoids browser URL behavior, traits/futures, hidden tasks, pools and replay. Ordered opaque headers, explicit body/trailer plans, unambiguous framing, bounded channels/staging and uniquely adopted typed outcomes make message boundaries, backpressure, cleanup and reuse inspectable by external frameworks.

## Consequences

The initial profile is direct-origin HTTP/1.1 with close-only HTTP/1.0 compatibility. It deliberately rejects repeated/comma Content-Length and every TE+CL combination, repairs no obs-fold/bare-LF input, supports only actual chunked transfer coding and exposes no successful CONNECT/101/raw escape. Informational/final heads, body chunks, trailers and terminal outcome remain separate. Client/server runners own and release the injected progress driver; applications explicitly scope/join work through noncopy endpoints. Cancellation/deadline/drop does not imply wire completion or remote nonexecution, and valid error statuses stay data.

No HTTP2/3/QUIC/proxy/router/middleware/cookie/decompression/JSON/automatic retry is included. #48 retains actual codec/runner/adversarial conformance work; #47/#44 retain lower progress/TLS delivery. Research and HandChecked examples do not establish implementation or request-smuggling security. If evidence cannot satisfy strict framing/general streaming, return to the design rather than silently normalize ambiguity or narrow to write-all/read-later.
