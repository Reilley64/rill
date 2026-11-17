# #45: candidate reference-harness buffer ledger

**Throwaway candidate, not an approved v0 partition or implementation/support proof.** The user approved #45 Q33–Q35: terminate/poison on post-handshake renegotiation attempts, execute a selected-source reference prototype, and use native application attempts <=16KiB, one pipeline, max fragment16KiB and no configured outbound padding. Earlier working mode choices are partial-write ON, moving-buffer OFF, AUTO_RETRY OFF. This ledger supplies an explicit hypothesis for that experiment, not another silently approved interface.

Question: can a source-backed, single-owner scheduler preserve both logical directions and native retry identity with bounded BIOs, safe policy observation and visible storage accounting? Counterexample baseline must be reproduced before calling its policy handled. Four existing 65536-byte budgets remain separate; caps do not bound all OpenSSL allocations, caller/harness state, or an opaque lower implementation's private memory. All overlaps still need inventory.

## Candidate capacities per endpoint

| Existing direction | Candidate named capacities | Sum |
| --- | --- | --- |
| Plaintext-out | admitted managed-equivalent input32KiB; private native retry span16KiB; admission/copy overlap reserve16KiB | 64KiB |
| Plaintext-in | private read destination16KiB; owned result awaiting handoff16KiB; authenticated fault-tail reserve16KiB; copy overlap16KiB | 64KiB |
| Ciphertext-out | BIO ring16KiB; copied drain span16KiB; lower-write tail/job payload16KiB; lower-copy overlap16KiB | 64KiB |
| Ciphertext-in | BIO ring16KiB; lower-read result/job payload16KiB; unconsumed feed/native copy16KiB; copy overlap16KiB | 64KiB |

An alias/cursor within one allocation does not count twice; an actual second copy does. Reserved ring capacity counts even while empty. Moving bytes out of a ring does not erase their charge while a drain/tail/job retains them. Unused reservations cannot secretly become general overflow. A synchronous all-C simulation does not prove Rill/Wasm foreign-copy/native-job leases or asynchronous overlap; report those as not exercised. The harness must list its real allocations and which proposed partitions it actually models, rather than presenting this table as measured enforcement.

Candidate public-equivalent write command max32KiB, read result max16KiB; HTTP's larger application chunks would need explicit bounded segmentation, not an increase in TLS's caps. Native write attempt max16KiB. These command maxima are experiment parameters, **not approved public driver limits**.

## Candidate egress reservation rule

Before a fresh established application SSL write, require32KiB of genuinely available ring/drain/tail capacity reserved to that attempt; do not count the lower-copy overlap reservation as ordinary output space. Previously queued ciphertext and lower jobs reduce availability. Only start from source-justified control state; space alone does not repair pending ALERT/control state. Isolated no-padding application record max16413 bytes for approved TLS1.2 AES-GCM, max16406 for TLS1.3, and one TLS1.3 KeyUpdate27 bytes are facts from the pinned-source note, not a total-flight guarantee.

A16KiB ring is too small for a maximum ciphertext record. Drain external BIO data into charged storage and retry the SAME native write on WANT_WRITE; never turn this into permission for another SSL read. Preserve positive-success suffix boundaries separately from WANT retries. Do not claim full coverage of control output, provider patches, peer behaviors, startup flights or fatal paths from this reservation candidate.

## Required honest output

Print source/build identity, effective modes/fragment/pipeline/padding settings, role/version, native last operation/WANT and saved obligations, command/staging/native-success/ciphertext/delivery counters, policy observations, queue/ring occupancy and capacity ledger. Report baseline BAD_WRITE_RETRY and policy-stop separately. Distinguish source inspection, executed reference fixtures and Rill/provider execution (absent).

Reference failures, unsupported control states, incomplete budget modeling and stalled bounded pumps are findings to return to the design—not expected successes to patch away by unsafe interleaving, extra buffers, silent input loss or loosened profile. No commit/push authorized.
