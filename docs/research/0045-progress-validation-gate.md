# #45: unresolved TLS progress-mechanism validation gate

Status: **PARTIALLY EXECUTED — selected-source reference complete; asynchronous Rill/runtime/provider validation remains open in #47/#44.** Explicitly recorded at the user's request. This is a work/evidence gate, not a new public API or accepted RFC.

## Engineering gate

The proposed single-owner TLS progress scheduler and four existing64KiB staging-budget partition have **not been fully execution-validated**. A finite selected-source reference now executes important transitions, including an unsafe TLS1.2 rejected-renegotiation continuation and narrower supported TLS1.3 read/write interleaves, but does not establish the complete asynchronous scheduler or budget ledger. See [transition facts](0045-openssl-bio-pair-transition-facts.md), [pending-read facts](0045-pending-read-and-write-quantum-facts.md) and [unapproved ledger candidate](0045-progress-buffer-ledger-candidate.md).

Working Q33–Q35 directions (terminal handling of post-handshake renegotiation attempts, a selected-source reference experiment, and fixed candidate record settings) do not validate that mechanism. The storage partition remains an UNAPPROVED experiment hypothesis. No Rill compiler/runtime, real native jobs/leases, target support, full TLS profile, conformance, security or universal-liveness proof exists from these notes.

## Historical tool-access blocker, resolved for the reference run

Two delegated attempts returned the same provider/tool content flag and **no completed reference results**:

- Original model `openai/gpt-6.1-sol`, task `node:delegated-task:command%3Amcp%3A0bca7d5e-4f38-4525-b4b9-133ff29505ab%3Adelegate-task%3Ac0c85198-d1fe-4e16-bb1a-fa2e81150431`.
- User-requested retry `openai/gpt-daybreak-blue-latest`, task `node:delegated-task:command%3Amcp%3A0bca7d5e-4f38-4525-b4b9-133ff29505ab%3Adelegate-task%3A8c79fc1c-74e4-4645-9dd7-2d32c734fccb`.

Message: “This content was flagged for possible cybersecurity risk. If this seems wrong, try rephrasing your request. Otherwise, confirm that the access_programs.cyber parameter is set to the appropriate tier, and note that some cybersecurity requests are still limited, even when Daybreak is on.”

The returned message is evidence of the block, not proof of its cause, the account's entitlement, accepted parameter values, or a Pi configuration limitation. The user has authorized **read-only research into documented Pi/provider configuration**, not changing access settings, retrying the blocked experiment, switching providers to evade it or claiming a client parameter grants server authorization. The access investigation and the TLS mechanism gate are different questions.

### Read-only configuration finding

[Pi configuration research](0045-pi-access-programs-configuration.md) found that installed Pi1.5.2 can serialize this nested field through per-model `samplingParams` on `openai-responses`; `openai-codex-responses` does not merge that configuration into its request body. Official OpenAI Responses documentation/SDK7.57.0 document `standard`, `daybreak_blue`, `daybreak_red`, subject to model and organization/project authorization. No dedicated Pi cyber flag was found. The earlier inability to set it through the delegation tool is not proof that Pi itself cannot forward it.

At that research stage T3's effective adapter, embedded version/configuration lookup and parent/child reload/inheritance remained unproven. No credentials/account entitlement was inspected. Later user-authorized trials below established the working Blue route and completed the reference run; this historical finding still does not close the engineering gate.

### Subsequent user-authorized Sol configuration trial

The user explicitly requested trying `gpt-6.1-sol` with `access_programs.cyber=daybreak_blue`. A read-only query of the **live parent's** codemode model registry returned `provider=openai`, `id=gpt-6.1-sol`, `api=openai-responses`, with no sampling defaults. The existing `/Users/reilley/.pi/agent/models.json` Sol override contained only `contextWindow`. A permission-preserving backup was made, and only the Sol sampling block was added; JSON and unrelated-field preservation were checked.

A fresh Sol child was asked only to inspect its nonsecret model metadata, **not run the TLS experiment**. It failed before returning that metadata:

> OpenAI API error (403): 403 {"detail":"Daybreak isn't available for this model. Turn off Daybreak or choose another model."}

This is an observed program-specific provider denial on the current request route, not a successful registry/payload inspection and not proof of which model/project/account/auth-surface restriction caused it. The official model guide's conditional Sol eligibility and actual route authorization remain different facts. No entitlement was granted or bypassed, and no TLS/reference execution occurred.

The added sampling block was then surgically removed. Parsed configuration and permissions were verified equal to the pre-trial backup; unrelated settings were not overwritten. No further retry or model/auth/project switch followed. The reference experiment and engineering gate remain OPEN. The earlier “no configuration changed” statements describe the preceding research phase; this later, explicitly authorized trial is recorded separately.

### Subsequent user-authorized Blue configuration trial

After the Sol denial and rollback, the user explicitly requested the same documented setting on `gpt-daybreak-blue-latest`. The live parent registry returned `api=openai-responses` for that exact ID. A permission-preserving backup was made and ONLY the Blue model override was added to the existing Pi model configuration. Parsed JSON was checked against the prior file with that sole addition; Sol stayed restored.

A fresh Blue child completed a metadata-only task and reported `provider=openai`, `id=gpt-daybreak-blue-latest`, `api=openai-responses`, and `samplingParams.access_programs.cyber=daybreak_blue`. This verifies that a fresh child on this route loaded the model override. It is not a raw payload capture, independent account-entitlement audit or server-resolved alias proof. The later corrected child completed the authorized reference run; no broader entitlement/support inference follows.

### Corrected child precheck bug

The first reference-task child after Blue verification stopped claiming the override was absent. Inspection of its actual tool calls found it read `m.program`, not the documented/configured `m.samplingParams.access_programs.cyber`. It repeated that incorrect lookup and received null. The earlier verification child's actual tool call used the correct nested field and returned `daybreak_blue`; the on-disk Blue override was also rechecked and remained present, with Sol still restored. Thus the stopped child's conclusion does **not** establish inconsistent runtime/configuration loading. No TLS artifacts or execution were produced by that attempt. The next user-authorized reference retry must use an exact nested-field assertion before doing native work; no further configuration change/restart is justified by this false negative.

### Executed reference result: corrected mechanism and unsafe continuation

The corrected Blue-program child completed the standalone pinned OpenSSL3.5.9 reference build and execution under `docs/research/0045-progress-reference/`. The first fixture version executed13 scenarios with12 assertions: it falsified the predicted immediate `BAD_WRITE_RETRY` because the fresh local write succeeded. Three parent-side reruns confirmed that local result.

Source reinspection identified the omitted handshake write-buffer BIO, so full external-pair occupancy did not prove record-layer `wpend_type=ALERT`. The fixture was then strengthened rather than relabelled: it retained the prior ciphertext, transferred every later byte in observed order and verified the peer. Result: the warning-alert callback fired; the post-read write BIO was again the BIO pair;256 prior ciphertext bytes were preserved; the fresh local write succeeded; only a30-byte later application record was handed off; the peer received the prior227 plaintext bytes and then failed decryption/record processing before the fresh byte. The alert had advanced sender record state without corresponding ciphertext handoff. The explicit-stop branch avoided the unsafe write.

The rebuilt one-command matrix now passes13/13. The original `BAD_WRITE_RETRY` mechanism remains a falsified source-reading hypothesis, while the stronger executed peer consequence supports terminal stop for the detected TLS1.2 rejected-renegotiation path. It still does not prove the production callback/private-shim implementation or every control state.

Other positive finite evidence includes TLS1.2/1.3 both directions, fragmented feeds, same-argument WANT_WRITE retry for a record larger than the16KiB pair ring, partial-success suffix boundaries, bounded stall/resume, requested/nonrequested KeyUpdate with a pending read, and clean-close versus truncation distinction. The attempted tiny-ring KeyUpdate case did not actually block control output. Full four-budget physical partitioning, asynchronous jobs/copy overlaps, Rill leases/wakes/cancellation, TCP, received-ticket discard, key-use policy and full provider behavior remain unvalidated. Thus the broad gate stays OPEN despite useful ReferenceExecuted evidence.

## Continue / do not infer

Independent channel try/wake/watch design, TCP progress semantics and HTTP codec/value work may continue. Preserve ordinary canonical families, owned payloads and explicit scoped dependencies; #45 design completion does not discharge this engineering gate. #47 and #44 retain shared runtime/TCP and TLS delivery validation respectively; #39 may resume design without claiming either implementation. Existing backend/ABI/lease and TLS delivery gates remain OPEN.

Do not silently narrow #39 to write-all/read-later, abandon native retries, raise staging caps, relabel source inspection as execution, or change the selected provider as a workaround. No commits/pushes or production bridge changes are authorized by this gate note.

## Before closing the remaining engineering gate

The legitimate access route and reproducible selected-source mechanism evidence are now recorded. Remaining work is implementation evidence: complete asynchronous four-budget accounting/copy overlap, maintained provider/shim policy including both renegotiation roles, Rill jobs/leases/wakes/cancellation, real TCP and Linux/macOS target cells, ticket/key-use/profile enforcement and full #47/#44 acceptance. Any contradiction returns to the human/map. Keep the finite native matrix separate from Rill/runtime/provider/security support; its13/13 result does not close those delivery tickets.
