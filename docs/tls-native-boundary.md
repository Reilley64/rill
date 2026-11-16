# TLS native boundary. Semantic contract v1

This is the closed toolchain-owned operation/data/ownership contract selected by #38 Q5/Q9/Q27/Q56/Q76/Q79 and explicit consolidated approval. The [public TLS contract](tls-contracts.md) governs behavior; [#44](https://github.com/Reilley64/rill/issues/44) must implement and validate bindings, layouts, C shim and native integration. This document delivers no working ABI, native extension format, C header implementation or security proof. A declaration, schema, reference calculation or another provider's fixture is not implementation evidence.

## 1. Authority and status

The standard module is trusted compiler/runtime support, not an external-provider plugin. Only toolchain-owned, allowlisted TLS operations can enter this boundary. No project-provided symbols/build hooks/metadata grant authority; no arbitrary library loading, C/Rust extern source syntax, Rill callbacks, public raw pointer/FD/pinning or forgeable native handle is introduced.

Native targets are Linux/macOS × x86-64/AArch64, with all acceptance NOT RUN. Standard wasm32-wasip1 has portable values/signatures, but provider-dependent operations return Unsupported. No imports/capabilities are inferred. #30's approved portable activation-record/nonrecursive dispatcher direction is current, while its complete typed/owned ABI, executor, leases, wake/reclamation and target acceptance remain OPEN. #43's SQL data/binding/wire design is not a TLS ABI or general FFI permission; reuse only justified common runtime invariants, never its wire format by assumption.

Semantic v1 fixes operations, effects and ownership below. Actual ABI version negotiation, concrete byte layouts, alignments, generated marshalling and private C declarations must be pinned to the delivered compiler/runtime under #44. Never reinterpret Rill enum/record/List/string/Bytes/Err layouts as C structures or presume LLVM23 alone establishes a C ABI. Unsupported or unsafe expressibility returns to map #1/provider selection before support, not relaxed semantics.

## 2. Controlled dependency and initialization profile

- The selected line is OpenSSL 3.5 LTS; the inspected version is 3.5.9 at commit `45e844fa2a14ec92d146bd8f5778ac130b6625fb`. Pin complete source/toolchain/build/patch/dependency closure and effective features. Security updates are deliberate maintained-line updates; do not permanently freeze a vulnerable version, make an unreviewed major upgrade or substitute a system library.
- Use a controlled private static libssl/libcrypto instance, with actual symbol isolation/coexistence tests. Static linking alone does not hide symbols or prevent earlier initialization by another consumer. Inventory constructors, global state, thread-local queues, callbacks, shutdown ordering and foreign dependencies.
- Suppress automatic configuration loading; disable engines, dynamic providers/DSOs, optional key logging and provider-owned HTTP/socket paths. Review applicable no-autoload-config/no-dso/no-engine/no-sslkeylog/no-http/no-sock build controls and explicit context APIs; their actual effects are source/target-specific, not a universal environment sandbox.
- Use only approved built-in default/base facilities needed by the fixed algorithm/decoder profile. There is no legacy/FIPS/custom-provider mode or validated-FIPS claim. Use an independent library context and explicit configuration, no default trust paths/URI/directory lookups, RAND files or native network callbacks.
- Retain reviewed platform acceleration. Residual x86/ARM CPU-capability and host-loader environment inputs are documented deployment assumptions, not controlled by Rill startup Inputs. Late initialization cannot undo constructor reads or another consumer's earlier state. Do not claim that per-call getenv/setenv changes or a Rill-only lock control those inputs.
- Use a built-in DRBG with a reviewed Linux/macOS getrandom/getentropy secure seeding dependency closure. Do not use device/file/jitter/unreviewed-provider fallbacks or manual weak seeds. This is not std::random-hook injection or a claim to use #37's exact macOS SecRandomCopyBytes path. Validate readiness/blocking/reseed/error/fork/initialization behavior reachable in the delivered profile.
- Rill maintainers own advisory monitoring, provenance, source-specific notices/license review, review of any integration patch, rebuilding and redistribution. No root bundle is shipped by this decision. No legal/security certification follows.

## 3. Closed operation catalogue

Names identify semantic operations, not callable Rill externs, numeric opcodes or a stable public C ABI. Each operation has checked counts, bounded inputs/results and exactly one selected completion.

| Operation | Input / result ownership | Effect and gate |
| --- | --- | --- |
| Bootstrap | Takes a pinned trusted build description and returns runtime-private provider state. | Check required effective algorithms, suppressed features, initialization and isolation; no network/default trust loading. |
| LoadCredentials | Takes owned native copies of bounded certificate/key inputs and returns an unpublished credential result. | Complete strict framing/DER/key checks without a password prompt, and validate key match/material. This does not certify local trust/status. |
| LoadAnchors | Takes owned bounded input and ServerAuth/ClientAuth purpose, then returns an unpublished root snapshot. | Require explicit root-only grants, self-signature/CA/KU/role restrictions and the narrow SHA1-selfsig exception. Do not accept AUX/partial roots. |
| LoadCrls | Takes owned bounded input and an exact positive whole-second age, then returns an unpublished CRL snapshot. | Parse number/scope/form/conflict data without claiming authenticated peer status. Reject partial/unsupported input without fetching. |
| BuildClientConfig / BuildServerConfig | Takes retained credential/anchor/CRL snapshot references and closed policy data, then returns an unpublished config. | Freeze logical inputs, validate role/purpose/ALPN/profile and native effective inventory. No ambient overrides. |
| CreateClient / CreateServer | Takes retained config and client reference/SNI data, then returns an unpublished engine. | Create one engine with private input/output buffers and fresh local key state. No transport object/callback enters C. |
| ConfigureProgress | Takes a new unpublished engine and the fixed progress profile before any I/O. | Enable partial write; disable moving-buffer/AUTO_RETRY; set one pipeline, 16KiB fragment/attempt, no output padding and bounded16KiB BIO-pair halves. Reject effective-setting mismatch. No mid-retry mode change. |
| ObserveRenegotiation | Takes only fixed passive TLS message metadata from the engine's private observer and records a bounded policy fact. | Detect TLS1.2 post-handshake HelloRequest/renegotiating ClientHello; no Rill/user callback, OpenSSL re-entry or public message exposure. Consume only after immediate current-call classification, then terminally stop. |
| FeedCiphertext | Takes a stable owned input span and returns the exact consumed ciphertext count. | Process the prefix before managed transport reads, with bounded ingress and strict record/work accounting. This is not plaintext progress. |
| AcceptPlaintext | Takes stable owned pending plaintext and returns exact private-stage acceptance/processing facts. | Public acceptance was TLS-owned queue admission, not native return value; preserve retry identity and cursors. |
| Advance | Takes an engine, action and checked time/control admission context, then returns completion/WANT/failure facts. | Perform a handshake, record read/write, pending control, flush or send-notify as a serialized foreign step. Do not suspend/re-enter managed code. |
| TakeCiphertext | Takes engine output and returns a retained/copyable owned output span and exact count. | Perform managed S.write/S.flush outside C. Short writes retain the unaccepted tail. Do not replay or make native FD calls. |
| TakePlaintext | Takes authenticated engine output and returns bounded owned plaintext/count. | Append managed data into caller Bytes, never an unauthenticated fragment. Preserve the tail before a pending terminal fault. |
| CapturePublication | Takes a successful verified engine and returns owned negotiated/authenticated metadata. | Include actual built peer path, reference/SNI distinction, ALPN/version/cipher. No publication before policy and final handoff. |
| Adopt / Release | Takes an unpublished result or owned resource and runtime identity, then completes one transition. | Adopt ownership atomically or clean up a losing result. Release waits for every lease/callback/job use, not cancel/drop alone. |
| RetainSnapshot | Takes an owned immutable snapshot and returns protected retained ownership. | Implements explicit public copy semantics, not fresh time/trust or duplicated mutable engine. |
| CaptureFailure | Takes a native operation result and saved native facts, then returns an owned bounded diagnostic. | Classify and capture errors on the same thread before migration. Original managed transport errors stay #23 causes outside native marshalling. |

Managed read/write/flush/close are not native opcodes. Their independently selected RE/WE/FE/CE evidence is checked in Rill; the provider never sees S, its arbitrary object layout, error representation or callback. A borrowed S retains normal dependencies. Only owned copies/leases cross the boundary.

WANT is continuation of the same engine operation, not success, transport health, EOF, cancellation completion or a new handshake/reconnect. Preserve the original plaintext buffer identity/length/content and internal native cursor until that retry obligation actually ends. Do not infer cancellation-abortability from memory BIO support.

For progress engines, connected BIO-pair ring capacity is charged inside the existing ciphertext-in/out ceilings. The external managed owner drains/feeds only while no SSL call is active and preserves lower unaccepted tails/jobs/copy overlap. Full/empty retry is pressure, not EOF/resource failure. Write-WANT_WRITE permits only external drain plus the same native write retry. Other interleavings require reviewed state-specific evidence; last WANT alone is not a public scheduler oracle. Positive partial success is the only fresh-suffix boundary.

## 4. Data and memory contract

- Inputs distinguish policy scalars, immutable snapshot references, engine identity, stable native buffer spans, checked wall time and operation/wake generations. Counts use checked u64 semantics; selected ABI sizes/signed-int/time_t/calendar range checks precede narrowing. Outputs distinguish actual consumed/produced native bytes from managed public plaintext transfer.
- Runtime identities are non-forgeable protected state, not user integers or public pointers. Concrete slot/generation/wrap rules belong to the delivered ABI and must prevent stale-result/ABA adoption and wake reuse.
- Native-owned buffers have demonstrated address/content validity across call, wait, retry and worker migration. A retained owner alone is not proof of pointer stability. No pointer into resizable Bytes/List, expired managed temporaries, caller stack storage or movable Wasm memory is retained by OpenSSL.
- Managed input is copied into bounded private storage before foreign use. Provider output and metadata are copied/adopted into owned validated representations before pointer invalidation/release. No borrowed native char*, error queue string, peer stack or X509 object escapes as a safe Rill view.
- Logical snapshots can be shared only with real cache/reference synchronization and protected lifetime. Mutable engines are serialized; at most one native step owns an engine at a time. Cached public observations do not enter it concurrently. Native locks are private obligations, not user thread-affine guards.
- Accepted-material/staging limits are not a transient heap/stack proof. Audit reassembly, native caches, key generation, signature/path/policy work, CRL expansion, callbacks and outstanding leases. Native reported shortage may become NativeResourceLimit; Rill/runtime allocation/bookkeeping/error-construction failure remains fatal, with no reporting/unwind guarantee.
- Progress ring reservations are 16384 bytes per ciphertext direction within, not in addition to, the existing65536-byte direction limits. Account copied drains, lower-job payload/tails and simultaneous copy overlap until actual release. Managed command bytes, stable native retry copies and output result storage similarly share plaintext ceilings. The standalone reference does not prove this asynchronous ledger.

## 5. Execution, error capture and callbacks

Potentially blocking initialization/entropy/parser/crypto/free work runs on admitted joined native work, not by blocking a managed M:N worker indefinitely or detaching cleanup. Managed code must not suspend while foreign frames retain managed storage. Native work may block; no hard CPU/return bound is asserted. Worker stacks are distinct from #22 managed 1MiB stacks and need their own evidence.

For each foreign step, establish error-queue preconditions and execute the relevant SSL operation. Save relevant native system-error state. Perform required SSL_get_error/classification on the same native thread without intervening unrelated OpenSSL calls. Drain/capture safe numeric facts before migration/re-entry. Keep native error, outer TLS operation/phase and original managed transport cause distinct. Native strings/file paths/data dumps are not public messages.

Private SNI/ALPN/verification callbacks inspect only retained native state and bounded admitted data. No arbitrary Rill closure re-entry, suspending callback, app retrieval/signing/password/keylog callback or success override of failed verification. The progress-only renegotiation observer is likewise fixed/passive: record message direction/type after handshake and perform no OpenSSL call. After the enclosing SSL call is classified immediately, a recorded event selects ProtocolFailure/Control and terminal release with no further application SSL I/O or SSL_shutdown. Where an extra restriction cannot be enforced safely in the chosen maintained provider, use reviewed upstream support/patches or return to the decision. Do not assume a callback flag alone proves complete enforcement.

Certificate time comes from checked wall acquisition, not unchecked native time() or OCSP_check_validity convenience paths. Native parameter inheritance/application, floor-to-seconds, native range and [notBefore,notAfter) behavior need fixtures. Local admission checks and peer verification phases use fresh samples; immutable config never contains frozen now.

## 6. Completion, adoption and cancellation states

Semantic lifecycle:

1. In Prepared, managed arguments/control are validated and native buffers/snapshots are retained. There is no public result yet.
2. In Admitted, the runtime owns execution/lease identity and cleanup responsibility. Completion/control select once with unspecified ready ties.
3. In Running / WANT / awaiting transport, stable buffers and config/engine uses remain owned. Managed transport I/O happens outside foreign calls under normal exclusive/dependency rules.
4. In Result pending adoption, a newly created native object is still unpublished and held by exactly one protected result owner.
5. The object is Adopted only if the successful selected result is atomically installed in one managed owner. Otherwise loser release cleans it exactly once, including late success after cancellation.
6. In Quiescent release, execution has finished or is confirmed aborted, callbacks/registrations have unwound or unregistered with quiescence, and wake identities have retired. Only then is storage reclaimed.

Wake events carry the real operation/activation identity and generation; stale/duplicate/late events cannot resume reused storage or adopt a second owner. Keep actual surviving managed activation/loans valid through join, not a guessed reconstructed continuation. #30/#43 source models do not implement these obligations for TLS.

Before admission an effect-free borrowed-operation rejection preserves a healthy stream; a consuming handshake/close still releases inputs. After admission abandonment poisons TLS but does not authorize force-killing a worker/closing a borrowed lower operation/freeing its source to fake completion. Select token/deadline/clock outcome once and finalize real late public plaintext prefixes after required cleanup. Late native ciphertext is not itself a public plaintext append.

Intrinsic stopping stays outside Result and masked through required installation/replacement/disposal/joins. Mandatory release ignores external cancellation and can wait indefinitely. Do not reclassify selected success/error after cleanup. Cleanup provides no rollback. Do not replay, reuse a poisoned session as healthy or clone error envelopes. Failed/poisoned disposal does not perform graceful TLS I/O.

## 7. Conformance and acceptance inventory

[`research/0045-progress-reference/`](research/0045-progress-reference/) is bounded **ReferenceExecuted** evidence on one macOS-arm64 host against the selected OpenSSL3.5.9 archive. Its13/13 in-memory cases cover selected BIO/retry/KeyUpdate/closure paths and demonstrate unsafe TLS1.2 renegotiation continuation versus explicit stop. It is not this private ABI, TCP, Rill integration, complete async accounting or target acceptance. Every required #44 item below remains NOT RUN unless separately published:

Required #44 evidence includes:
- Actual LLVM23/Rill metadata consumer, trusted lowering/marshalling and C shim; immutable configuration copy and generic RE/WE/FE/CE evidence; no orphan interception of protected native obligations.
- Build/isolation/coexistence/initialization/entropy/environment/notice/update records for all four native cells.
- Effective closed cipher/group/signature lists, fresh shares, mandatory signature_algorithms_cert/certificate signatures, negotiated TLS12 EMS/RI, disabled optional machinery, required KeyUpdate and key-use/epoch accounting. No reliance on absent stock controls.
- Strict DER/PEM and type/length/critical/duplicate/role checks, complete actual-path/root CRLs, native KU/signature/status gaps, unsupported feature rejection and real checked-time failures/boundaries.
- Fragmentation/coalescing/read-needs-write, bounded buffers/reassembly/counters, initial prefix, zero/short/late prefixes, mutable buffer retry identity, authenticated tail/truncation, native error capture before migration.
- Pre-admission/control/adoption/cleanup races, stale wake epochs, late successes, exactly-once release and callback quiescence, secret-safe metadata/errors, healthy/poisoned/write-closed distinctions and both-version closure.
- Real target execution, supported backend/stack/address and allocation-fault evidence; native standalone references and Python counters are not Rill integration, a complete ABI validator or a security/soundness proof.

No live Wasm provider is selected. A non-workable boundary returns to #1/#38; #44 cannot close on this semantic document or a declaration-only header. #28 remains review-ready RFC drafting, separate from RFC acceptance and software delivery.
