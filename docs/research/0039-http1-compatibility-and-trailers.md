# ri-n5i research: HTTP/1 compatibility, authority, trailers and early finals

Research snapshot: **2026-11-16**. Primary-source and metadata inspection only. **No HTTP conformance, security, Rill, runtime, native or TLS tests were run; no implementation or liveness proof is supplied.** This is factual input, not a policy/API selection or approval. Its remaining-choice list records the research stage; the later human-confirmed [HTTP contract](../http-contracts.md) is authoritative.

## 1. Scope and source currency

Working choices supplied for this research: initial general-streaming HTTP/1.1, direct-origin client/server; no client pipelining or parallel per-connection handlers; no H2/H3, successful CONNECT, 101/upgrades or proxy support initially. A transport-independent public `std::http::h1` codec, owning heads and ordered byte headers, strict CRLF, no obs-fold/invalid-field repair, rejection of all TE+CL and repeated/comma CL, chunked-only **actual** transfer coding, and an explicit outgoing framing plan are working inputs, not decisions made by this note.

Read the current ri-n5i and ri-w0c bodies. **ri-w0c is an open upstream design prerequisite for resumable single-owner progress/wait.** Neither that ticket nor the existing [duplex/TLS research](0039-duplex-progress-and-tls-retry.md) demonstrates an actual TLS/runtime implementation. This note supplements [the earlier HTTP research](0039-http-standards-and-streaming.md), rather than changing its files or the issues.

Primary sources retrieved:

- [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110.html) (semantics), [RFC 9112](https://www.rfc-editor.org/rfc/rfc9112.html) (HTTP/1.1), [RFC 3986](https://www.rfc-editor.org/rfc/rfc3986.html) (URI grammar).
- RFC Editor information pages for [9110](https://www.rfc-editor.org/info/rfc9110) and [9112](https://www.rfc-editor.org/info/rfc9112), plus the [errata database](https://www.rfc-editor.org/errata.json). At retrieval, 9110 showed no direct updating RFC; 9112 showed **[RFC 9931](https://www.rfc-editor.org/rfc/rfc9931.html)**. Its §8 updates HTTP/1.1 CONNECT handling: proxy clients forwarding untrusted optimistic data must wait for success or send `Connection: close` (or both); rejecting proxy servers must close without processing more requests, with the specified known-waiting-client exception. CONNECT remains excluded here; exclusion is not implementation of this update.
- Relevant **verified** errata: [9112/7744](https://www.rfc-editor.org/errata/eid7744) fixes the `obs-text` reference to 9110 §5.5. [9110/8268](https://www.rfc-editor.org/errata/eid8268) and [9112/8284](https://www.rfc-editor.org/errata/eid8284) move the absolute-form-over-Host change note to the right appendix; the operative rule remains 9112 §3.2.2. None changes framing precedence. Other verified 9110 entries concern 308 method preservation (7109), CN-ID change notes (7105), Accept examples (7138), Range whitespace (7306), and Accept-Charset naming (7419); they do not create trailer or version exceptions.
- [IANA HTTP Field Name Registry](https://www.iana.org/assignments/http-fields/http-fields.xhtml) and its XML/CSV, [RFC 9530](https://www.rfc-editor.org/rfc/rfc9530.html) (Digest Fields), [RFC 9421](https://www.rfc-editor.org/rfc/rfc9421.html) (HTTP Message Signatures). Registry presence is **not** evidence that a field is trailer-permitted. Its field specification is the authority.

Erratum status matters: reported, held and rejected proposals are not silently normative corrections. In particular, [9112/9284](https://www.rfc-editor.org/errata/eid9284) was **Rejected**: §3.3 does not override §3.2's mandatory 400 for invalid/missing HTTP/1.1 Host. [9112/7633](https://www.rfc-editor.org/errata/eid7633), also rejected, does not authorize bare LF in chunk delimiters. These metadata checks are dated evidence, not a permanent claim about future updates.

BCP 14 terms below retain their source strength: **SHOULD is not MUST**; a departure needs understood consequences, not an invented mandatory rule ([9110 §2.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-2.2)).

## 2. Versions: HTTP/1.0, higher 1.x minors, and 505

### What version numbers promise

[9110 §2.5](https://www.rfc-editor.org/rfc/rfc9110.html#section-2.5) says the major identifies messaging syntax and the minor the highest minor to which the sender is conformant, including understanding for future communication. Advertising `HTTP/1.1` is not merely labeling an arbitrary application protocol. [9112 §2.3](https://www.rfc-editor.org/rfc/rfc9112.html#section-2.3) defines `HTTP-version = %s"HTTP" "/" DIGIT "." DIGIT`; casing and single-digit grammar matter. `HTTP/1.10` is not simply another valid 1.x minor under that grammar.

[9110 §6.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-6.2):

- Client **SHOULD** send its highest conformant version whose major is no greater than the server's highest known major, and **MUST NOT** send a version to which it is not conformant. A lower-version workaround for a broken server has the stated prior-attempt conditions.
- Server **SHOULD** send its highest conformant version with major no greater than the request's major; it **MUST NOT** send a version to which it is not conformant. This is not a universal requirement to echo the request's exact minor. Thus a 1.1 response to a 1.0 request is not forbidden merely by the version digits; features still have recipient-version restrictions.
- A recipient implementing the major but receiving a **higher minor SHOULD process it as its highest implemented minor**. For a 1.1 recipient, an otherwise valid `HTTP/1.2` message is not automatically a version error. Senders cannot rely on unannounced higher-minor-only features being understood.

[9112 §2.3](https://www.rfc-editor.org/rfc/rfc9112.html#section-2.3) explicitly requires constructing a 1.1 message for a 1.0 or version-unknown recipient so it remains interpretable as a valid 1.0 message when newer features are ignored. Specific feature restrictions, especially TE, still apply. It also permits a server's 1.0 response to a 1.1 request as a broken-client workaround, discouraging indiscriminate downgrade.

### 505 is a major-version refusal

[9110 §15.6.6](https://www.rfc-editor.org/rfc/rfc9110.html#section-15.6.6) defines 505 as unsupported/refused **major** version; §6.2 allows refusal of the client's major for any reason. It is **not** the specified generic response to an unknown higher 1.x minor or a 1.0 request. A malformed version token is a syntax problem, not proof of a legitimate unsupported major. The 505 response **SHOULD** explain the refusal and supported protocols.

**Limited-profile distinction:** a project can honestly document “only peers using 1.1 are serviced; 1.0 unsupported” as a deliberate interoperability limit. The RFCs do not oblige an application to offer successful service to every request. But this is not evidence that a blanket 1.0 rejection/505 is the RFC-prescribed behavior of a general 1.1 implementation. It gives up the backwards-compatible behavior described above; higher-minor blanket rejection also departs from a SHOULD. Do not label such a restricted product fully interoperable by relabeling a minor mismatch as a major-version error. The error mapping and exact support claim remain human choices; no rejection status is selected here.

### If 1.0 is accepted or received

[9112 §§6.1–6.3](https://www.rfc-editor.org/rfc/rfc9112.html#section-6.1), [§9.3](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.3):

- **No TE:** a server **MUST NOT** send TE unless the corresponding request indicates 1.1 or later minor. A client **MUST NOT** send TE without knowing the server handles 1.1 or later minor (configuration or a previous response can provide knowledge). “Initial streaming” alone supplies no such knowledge.
- A received **HTTP/1.0 message containing TE MUST be treated as faulty framing and the connection closed after processing**, even with CL. This includes misleading hypothetical TE in a 1.0 response; no decoding/persistence shortcut makes it valid.
- After bodyless-response exceptions, a response without TE uses valid CL for a fixed length, or connection closure for delimitation if CL is absent. A request without TE/CL has zero body, not a close-delimited body. A streaming response of unknown length to a 1.0 request can be close-delimited; it cannot be chunked. A fixed-length response must actually honor that length.
- 1.0 defaults to closure. Optional `Connection: keep-alive` is honored only if the recipient wishes, with the role conditions in §9.3; supporting it is not required. 1.1 defaults to persistence, but `close` takes priority. Persistence requires self-defined message lengths, complete request consumption by the server (or close), and complete response consumption by a reusing client. A close-delimited response cannot persist.
- A non-persistent client must send `Connection: close` on every request; a non-persistent server on every non-1xx response. A 1.1 client receiving 1.0 without keep-alive cannot send another request there. No pool or mandatory keep-alive policy follows from these rules.

**Open:** accept 1.0 with close-only service, implement its optional keep-alive, or explicitly restrict interoperability? Separately, how will a first unknown-server upload avoid forbidden initial TE? Configuration, prior capability knowledge, known-length framing or a different bounded workflow are different choices; silently buffering an unbounded stream is not a standards requirement.

## 3. No-body responses: metadata validity is not framing

The first rule of [9112 §6.3](https://www.rfc-editor.org/rfc/rfc9112.html#section-6.3) is decisive: responses to **HEAD**, and **1xx, 204, 304**, end at the header terminator and **cannot contain a message body or trailer section**, regardless of header fields. HEAD here concerns the **response**: a request method does not erase request framing.

| Response context | Content-Length generation | Transfer-Encoding generation |
| --- | --- | --- |
| HEAD (subject also to status prohibitions) | MAY, only if equal to octets that would have been sent for the corresponding GET | MAY describe coding that would have applied to an unconditional GET |
| 304 | MAY, only if equal to octets that would have been sent in a 200 response to the same request | MAY describe the hypothetical unconditional GET coding (9112 specifies 304 to GET) |
| 1xx or 204 | MUST NOT, including `Content-Length: 0` | MUST NOT |
| 2xx CONNECT (excluded here) | MUST NOT | MUST NOT; recipient switches to tunnel semantics |

Sources: [9110 §8.6](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.6), [9112 §6.1](https://www.rfc-editor.org/rfc/rfc9112.html#section-6.1). These permissions do not relax TE's recipient-version restrictions or the sender prohibition on combining TE and CL. HEAD with status 204 does not bypass the status prohibition.

Consequences for the chunked-only working scope:

- Valid hypothetical `Transfer-Encoding: gzip, chunked` on a HEAD/304 response does **not** require actually decoding gzip: there is no body. Unsupported **actual** transfer-coding rejection and hypothetical metadata acceptance are separate capabilities. Invalid coding syntax is a different question from lack of a decoder.
- The server SHOULD send 501 for an unsupported **request** transfer coding ([9112 §6.1](https://www.rfc-editor.org/rfc/rfc9112.html#section-6.1)); that is not a general mandate for clients to reject legal hypothetical response metadata. A request to HEAD with TE still has request framing. The mandatory 400/close for a request whose final transfer coding is not chunked remains a separate framing rule (§6.3).
- A prohibited field on 204 does not cause a body to exist. A TE+CL HEAD response does not acquire a body by moving rule 3 ahead of rule 1. Under the supplied strict profile it can still be **rejected as invalid**, rather than accepted or followed as body framing.
- Do not consume an advertised hypothetical CL or wait for a zero chunk after HEAD/304. Those bytes could be another message or extraneous data, not this response's body/trailers.

### 205 is not another header-terminated response

Additional primary-source check: [9110 §15.3.6](https://www.rfc-editor.org/rfc/rfc9110.html#section-15.3.6) says a server **MUST NOT generate content** in a 205 response. Unlike 204, 205 is not in 9112 §6.3's header-terminated response list. Therefore zero content still needs ordinary wire framing: do not classify 205 as implicitly ending at the header terminator or misinterpret its zero chunk as a new message. A proposed `Content-Length: 0` emitter policy and rejecting actual received content would be design choices; this factual note does not adopt them. HEAD's independent response rule still applies. Source text inspected only; no test or implementation proof.

### Complete validation versus precedence

[9112 §§2.2, 5](https://www.rfc-editor.org/rfc/rfc9112.html#section-5), [9110 §§5.1–5.5](https://www.rfc-editor.org/rfc/rfc9110.html#section-5): parse octets, token field names, colon/whitespace rules, allowed field-value octets, CRLF boundaries, repetitions, and applicable field semantics/limits. Obs-text is permitted opaque bytes, not generally invalid UTF-8. CR/LF/NUL in values require rejection or replacement under 9110; rejection is the supplied choice. Strict CRLF and rejecting obs-fold avoid optional repair paths.

The body-length precedence algorithm is **not** permission to skip validating the rest of the head once HEAD/204 is known. Nor does it require decoding every unknown extension field's application grammar. Servers **MUST NOT apply the request to its resource before the entire header section is received** (§5.3). Unrecognized fields normally remain extensible; non-proxy recipients **SHOULD ignore** their semantics (§5.1).

Identical repeated/comma CL values have an RFC-permitted normalization alternative; rejecting all is the supplied stricter choice ([9110 §8.6](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.6)). General TE+CL recipient behavior is not a universal MUST-reject, though senders must not generate it; §6.1 mandates server closure after responding. Applying the chosen rejection even to no-body contexts is a validation policy, **not a rewritten §6.3 body-length rule**. Exact diagnostic precedence when multiple invalidities coexist is not prescribed as one universal error hierarchy.

An outgoing framing plan therefore needs to distinguish actual fixed/chunked/close-delimited content, no-body responses, and optional hypothetical metadata; preserving bytes in an owning head alone does not validate an emitter.

## 4. Absolute-form authority and Host

### Required parsing and precedence

[9112 §3.2.2](https://www.rfc-editor.org/rfc/rfc9112.html#section-3.2.2): an origin server **MUST accept absolute-form** even though direct clients normally send origin-form. Excluding proxy operation does not remove this origin-server requirement. For absolute-form, the origin server **MUST ignore received Host and use request-target host information**. Routing must not choose the conflicting Host instead.

[9112 §3.2](https://www.rfc-editor.org/rfc/rfc9112.html#section-3.2): a 1.1 client **MUST send Host**, even with absolute-form. Its value must be identical to the target authority, excluding userinfo and `@`, or empty when authority is absent/undefined. The server **MUST send 400** for missing Host in a 1.1 request, or for any request with multiple Host field lines or invalid Host value. Two identical Host lines are still multiple lines; comma-combining them does not repair the violation. Mandatory Host validity and absolute-form precedence are distinct requirements.

**Mismatch is not a mandatory matching check for the recipient.** The client's matching obligation does not cancel the server's absolute-form precedence rule. A deployment can choose a stricter rejection/filter policy for a mismatch rather than route using Host, but it must identify that as policy, not claim “RFC requires mismatch rejection”. Moreover, because the text says **MUST ignore Host**, this research does not certify a blanket equality-gating rule as unqualified compliance with that requirement. Distinguish an explicitly narrower acceptance/security policy from the standards parsing/routing algorithm; do not hide that tension. The exact rejection mapping remains unselected.

Authority checks on the **target** are still meaningful: [9110 §7.4](https://www.rfc-editor.org/rfc/rfc9110.html#section-7.4) requires checking scheme-specific requirements (including HTTPS origin security), and discusses 421 for misdirection. This is not the same as a mandatory lexical Host-equals-target policy.

### URI authority is not a TCP/TLS name constructor

[9110 §§4.2.1–4.2.4, 7.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-4.2.1), [3986 §§3.2–3.2.3](https://www.rfc-editor.org/rfc/rfc3986.html#section-3.2):

- `Host = uri-host [ ":" port ]`; userinfo is not Host syntax. HTTP(S) senders **MUST NOT generate userinfo** in message URI references; recipients of untrusted references **SHOULD** treat its presence as error. Rejecting it is compatible with that recommendation; extracting credentials/authentication is not required.
- Generic `host = IP-literal / IPv4address / reg-name`; IP literals are bracketed IPv6 or IPvFuture. URI IPv6 brackets are not part of a bare numeric socket address. IPvFuture syntax does not establish a supported network address implementation.
- `reg-name = *( unreserved / pct-encoded / sub-delims )`; it is broader than a DNS hostname or TLS DNS reference identifier. `%HH` is syntax, not permission for repeated decoding or decoding a delimiter before authority segmentation. RFC 3986 §3.2.2 restricts producer use of host percent-encoding to representing UTF-8 sequences; an explicit mapping to supported lookup/TLS identities still needs definition.
- URI `port = *DIGIT` allows an omitted/empty port syntactically; HTTP scheme defaults/normalization are separate. Generic URI syntax does not impose the numeric range of a TCP endpoint. CONNECT's mandatory nonempty valid port rule is special, not a rule for all Host fields.
- HTTP(S) requires a nonempty host. Generic URI allowances do not make `http:///x` a valid ordinary HTTP origin. Default-authority reconstruction cannot repair mandatory-invalid 1.1 Host (§3.2).

The narrower ri-vno dial-name and ri-irw TLS reference-name contracts must not be silently promoted into the HTTP grammar. A byte-preserving codec can recognize a syntactically valid authority that an eventual resolver/verification policy cannot use. Whether to reject unsupported address/name forms, expose them without dialing, or define a separate conversion layer is a human choice. DNS, IDNA, percent-decoding, URI normalization, authority comparison and TLS verification are not one operation.

## 5. Trailers: permissions, declarations and recipients

### Sender obligations are definition-based, not merely a denylist

[9110 §6.5.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-6.5.1):

> A sender MUST NOT generate a trailer field unless the sender knows the corresponding header field name's definition permits the field to be sent in trailers.

Thus an arbitrary syntactically valid `X-Whatever` is **not automatically sendable** as a trailer. Neither registry registration nor `Trailer: X-Whatever` creates permission. Definitions requiring decisions before content—framing, routing, authentication, request modifiers, response controls, content format—are unsuitable for deferral. Common forbidden examples include Content-Length, Transfer-Encoding and Host; a catalog of familiar bad names is **not sufficient to satisfy the positive knowledge MUST** for every other name. “Everything not on the denylist is allowed” would misstate the current standard.

A plausible **unselected** design could enforce a small specification-backed send allowlist and permit explicit extension assertions supplying that field's trailer permission/definition. Such an assertion must mean the caller is taking responsibility for actual definition-based knowledge; it cannot truthfully declare that arbitrary unknown fields have become RFC-permitted. Name/value syntax, repetition rules, bounds and any field-specific constraints still apply. No assertion/API shape, built-in list, or trust model is selected here.

### What Trailer and TE do, and do not, promise

[9110 §6.6.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-6.6.2): `Trailer = #field-name` lists fields **anticipated** in the trailer section. A sender intending trailers **SHOULD** declare them initially. This is not a MUST-declare requirement, an exhaustive authenticated commitment, a guarantee they will arrive, or permission to send prohibited fields. The specification expressly allows a declaration not followed by those trailers. Mere absence from Trailer does not by itself make a received permitted extension field invalid under core HTTP.

[9110 §§6.5.1–6.5.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-6.5.1), [9112 §7.4](https://www.rfc-editor.org/rfc/rfc9112.html#section-7.4): request `TE: trailers` advertises willingness not to discard response trailers, including downstream clients where relevant; it does **not** promise to understand every field. `TE` is hop-by-hop and its sender **MUST** also send `Connection: TE`. Do not confuse `TE` with `Transfer-Encoding`. Absence of `TE: trailers` is not a blanket MUST-NOT-send-trailers rule; servers **SHOULD NOT** generate trailers believed necessary for the user agent to receive, given possible loss in transit.

### Recipient obligations differ from sender permission

[9112 §7.1.2](https://www.rfc-editor.org/rfc/rfc9112.html#section-7.1.2) permits a dechunking recipient to selectively retain or discard received trailers. Retained fields must be separate or merged under the definition-based rule: a recipient **MUST NOT merge** a trailer into headers unless it understands the definition and that definition explicitly permits and specifies safe merging. Keeping a distinct late section is possible without making any merge decision.

[9110 §§5.1, 6.5.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-6.5.2): other recipients SHOULD ignore unrecognized header/trailer semantics. Same-name trailer order matters; fields potentially generated more than once must be defined list-based. Parsing/bounding the complete trailer section remains necessary for chunked completion even when its values are discarded. A received unknown field does not impose a requirement to authenticate it, merge it, or retroactively redo routing. Conversely, retaining unknown bytes does not prove that their sender met the trailer permission MUST.

**Unselected policies:** reject forbidden/undeclared/unknown trailers, discard unrecognized semantics, or retain structurally valid unknown extensions in a separate bounded section. Blanket unknown-field rejection narrows extensibility and needs a justified departure from the SHOULD-ignore baseline. “Unknown”, “known forbidden”, “known permitted” and “declared initially” are different facts. No automatic late authentication decision or automatic merge is inferred.

### Useful authoritative field examples (not an adopted allowlist)

| Field | Authoritative trailer fact |
| --- | --- |
| `Content-Digest` | [RFC 9530 §2](https://www.rfc-editor.org/rfc/rfc9530.html#section-2) explicitly permits trailers and permits merging into headers. Digest covers message content, not chunk framing. |
| `Repr-Digest` | [RFC 9530 §3](https://www.rfc-editor.org/rfc/rfc9530.html#section-3) explicitly permits trailers and merging. Covers the entire selected representation, which can differ from content for HEAD/ranges. |
| `Signature-Input`, `Signature` | [RFC 9421 §§4.1–4.2](https://www.rfc-editor.org/rfc/rfc9421.html#section-4.1) permits trailer placement but recommends header placement to avoid stripping. Signature semantics/canonicalization need separate implementation; this is not automatic authentication. |
| `Authentication-Info`, `Proxy-Authentication-Info` | [9110 §§11.6.3, 11.7.3](https://www.rfc-editor.org/rfc/rfc9110.html#section-11.6.3) explicitly allow trailers when the authentication scheme permits. This is why “all auth-related names always forbidden” is also too crude; proxy support remains excluded. |

RFC 9530 obsoletes RFC 3230's `Digest`/`Want-Digest`; IANA marks those names obsoleted. `Want-Content-Digest`/`Want-Repr-Digest` are preferences (§4), not digest values and not evidence that a digest was sent or verified. Recipients MAY ignore digests; automatic validation is not selected. Checksums alone do not establish sender identity ([9530 §6](https://www.rfc-editor.org/rfc/rfc9530.html#section-6)).

Currency caveat: the retrieved IANA registry also has permanent `Unencoded-Digest`/`Want-Unencoded-Digest` entries pointing to **`RFC-ietf-httpbis-unencoded-digest-05`**, a draft-style pending RFC reference, not an assigned numeric RFC in that entry. This note does not invent an RFC number or infer its trailer rules from registration. Recheck the authoritative published specification before including it in a built-in list. Verified 9530 errata 8158/8273 correct example captions/wording, not these trailer permissions; verified 9421 errata 8102/8103 concern signature examples/parsing, not blanket trailer authorization.

## 6. Early final responses, output admission, closure and reuse

### HTTP obligations versus recommendations

- **Permitted:** responses, including final responses, can start and finish before the request completes ([9110 §7.5](https://www.rfc-editor.org/rfc/rfc9110.html#section-7.5)). A server need not first drain a whole upload to send a refusal.
- **SHOULD:** a client receiving a response while still sending should continue the request **unless explicitly indicated otherwise** (§7.5). “Any final always cancels upload” is therefore a policy, not this standard's universal rule.
- **SHOULD:** an uploading client should monitor for error responses; if the response indicates the server will not receive the body and is closing, it should immediately cease body transmission and close its side ([9112 §9.5](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.5)). This is not a new universal requirement for two concurrent TLS application drivers.
- **MUST:** a server must consume the entire request body or close after responding; a client intending reuse must consume the entire response body ([9112 §9.3](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.3)). An early final does not supply a new request boundary. Stopping a fixed/chunked upload before its framing completes cannot be followed by an ordinary new request on that same byte stream as though completion occurred.
- **MUST:** `Connection: close` forbids subsequent requests and requires the role-specific closure behavior after the final response; servers must not process later requests ([9112 §9.6](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.6)). Reading/draining is not the same as applying a later request.

`Expect: 100-continue` can avoid admitting some body bytes, but the client need not wait indefinitely, and a server receiving the complete expectation-bearing head must promptly send the applicable final response or 100; it must not wait for body first to send 100 ([9110 §10.1.1](https://www.rfc-editor.org/rfc/rfc9110.html#section-10.1.1)). A soft expectation timer is not request cancellation or a reusable canceled TLS read. General streaming still has to handle later final responses after an upload begins.

### Accepted prefixes cannot be recalled

An application can stop **new body admission** after learning a final response, but this cannot retract bytes already accepted into the coordinator, codec, TLS engine, ciphertext queue, socket, or peer. These are distinct acceptance boundaries; none implies remote execution, receipt or safe replay. This is a consequence of stream transport and the existing ri-vno/#38 prefix contracts, not an RFC-defined rollback operation. Exact discarded/pending/accepted prefix reporting remains a design obligation.

Under ri-irw, abandoning a genuinely admitted TLS operation can poison the stream. Detecting an HTTP early final is not permission to cancel that operation, discard its stable retry storage and reuse TLS. Stopping future HTTP admission and arranging retained-operation completion/terminal cleanup are separate actions. ri-w0c must define the safe progress/wait boundary; this research selects neither a TLS split nor a retry scheduler.

### Close does not mean instant destructive transport abort

[9112 §9.6](https://www.rfc-editor.org/rfc/rfc9112.html#section-9.6) describes staged server TCP closure to avoid a reset erasing the final response: write-half close, continue reading until peer closure or reasonable assurance of acknowledgment, then close fully. This is explanatory transport guidance, not an unconditional generic TLS half-close API requirement. A client TCP half-close does not delimit an HTTP request. TLS closure/truncation and ri-irw's lack of TLS 1.2 directional shutdown cannot be inferred away by copying this TCP sequence.

For a reuse claim, establish **both** HTTP boundaries and healthy transport/operation state, respect Connection/version rules, and exclude unread/ambiguous suffixes. Fully consuming a response is necessary but does not repair an incomplete sent request, TLS poison or mandated close. Completing a framed HTTP body also does not by itself certify TLS clean closure. Close-delimited responses need the lower-layer error distinction; [9112 §§8, 9.8](https://www.rfc-editor.org/rfc/rfc9112.html#section-8) distinguish normal completion from incomplete transport closure.

## 7. Remaining human choices and explicit unknowns

No policy or public API is selected by this note. Before consolidating ri-n5i, settle:

1. **Version acceptance/support claim:** 1.0 service and optional persistence; higher 1.x treatment; honest limited-profile wording and errors without misusing 505. Separately establish when a client knows initial TE is legal.
2. **No-body metadata:** accept legal hypothetical unsupported coding descriptions while refusing actual unsupported coding? Define invalid-field rejection separately from framing precedence and retain the already chosen strict TE+CL/CL policies.
3. **Authority policy:** standards absolute-form routing and mandatory Host checks; any explicit mismatch rejection and its narrower acceptance claim; conversions to supported ri-vno/#38 names without equating URI grammar with dial/verification grammar.
4. **Trailer contract:** specification-backed outgoing permissions and any explicit extension assertion; declaration policy; separate receipt/discard/unknown-field behavior, bounds and terminal visibility. No automatic merge, digest verification or auth result follows.
5. **Early-final disposition:** which indications stop admission, treatment of queued/accepted prefixes, response consumption, request drain/completion versus closure, and preservation of live TLS retry state through the ri-w0c boundary.

**Liveness boundary:** RFC permissions and SHOULD-monitor guidance explain why a general-streaming coordinator cannot simply block forever writing an upload before reading a final response. They do not prescribe an invented TLS split, a particular task/channel API or progress despite an indefinitely stalled application sink. Bounded queues, resumable lower I/O, application scheduling, fair progress, cancellation cleanup and provider retry constraints still need ri-w0c's design and actual runtime/TLS evidence. Excluding pipelining/parallel handlers simplifies message correlation; it does not remove opposite-direction progress within one exchange. No completed security audit, target support, compiler acceptance or protocol conformance is asserted.
